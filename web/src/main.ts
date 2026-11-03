// The page: connect, then once a frame read the controls, hand them to the
// netcode, and draw what it says. client-net decides nothing here either:
// the server does, and the page only draws.

import * as THREE from 'three';
import init, { Client, map_boxes, map_pads, map_layout, map_name, map_terrain, map_launchers, arm_name, skin_names } from './pkg/client_net.js';
import { Controls, FIRE, ZOOM } from './input';
import { buildWorld, dropWorld, defaultMaterials } from './world';
import { Players, STRIDE } from './players';
import { Vehicles, VEHICLE_H } from './vehicles';
import { Effects } from './effects';
import { Hud } from './hud';

const params = new URLSearchParams(location.search);
const AUTOPLAY = params.has('autoplay');

// Event kinds (sim::events::Kind).
const EV_SHOT = 1, EV_HIT = 2, EV_KILL = 3, EV_BLAST = 6;
const ROCKET = 6;
/** `part` on a shot heard but not seen (room::HEARD). */
const HEARD = 1;

interface Shared {
  connected: boolean;
  frames: number;
  kills: number;
  deaths: number;
  stats: number[];
  error: string;
  own: number[];
  others: number[];
  killcams: number;
  inKillcam: boolean;
  /** Our side with teams, else -1: whom the autopilot never chases. */
  mate: number;
}
const shared: Shared = {
  connected: false, frames: 0, kills: 0, deaths: 0, stats: [], error: '', own: [], others: [], killcams: 0, inKillcam: false,
  mate: -1,
};
(window as any).__oneshot = { state: () => shared };

async function main() {
  await init();
  const canvas = document.getElementById('game') as HTMLCanvasElement;
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, powerPreference: 'high-performance' });
  // ?lite: half resolution, no shadows. The browser test runs on a
  // software GPU, where the full picture would starve the netcode of frames.
  const LITE = params.has('lite');
  renderer.setPixelRatio(LITE ? 0.5 : Math.min(devicePixelRatio, 2));
  renderer.shadowMap.enabled = !LITE;
  renderer.shadowMap.type = THREE.PCFShadowMap;
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(78, 1, 0.05, 400);
  scene.add(camera);
  const resize = () => {
    renderer.setSize(innerWidth, innerHeight, false);
    camera.aspect = innerWidth / innerHeight;
    camera.updateProjectionMatrix();
  };
  addEventListener('resize', resize);
  resize();

  const players = new Players(scene);
  const vehicles = new Vehicles(scene);
  const effects = new Effects(scene);
  const hud = new Hud();
  const hooks = await import('./art').catch(() => null);
  const art = hooks ? await hooks.loadArt(scene, camera, renderer, players).catch((e: unknown) => {
    console.warn('art did not load', e);
    return null;
  }) : null;

  if (params.has('pose')) {
    buildWorld(scene, map_boxes(0), map_pads(0), defaultMaterials(), map_layout(0));
    return pose(scene, camera, renderer, players, art);
  }

  const menu = document.getElementById('menu')!;
  const status = document.getElementById('status')!;
  const nameInput = document.getElementById('name') as HTMLInputElement;
  const el = (id: string) => document.getElementById(id) as HTMLButtonElement;
  nameInput.value = params.get('name') || localStorage.getItem('oneshot.name') || '';
  // Skins: worn only if the signed-in wallet owns one; the server checks.
  const skinSelect = document.getElementById('skin') as HTMLSelectElement;
  skin_names().forEach((n: string, i: number) => skinSelect.add(new Option(n[0].toUpperCase() + n.slice(1), String(i))));
  try { skinSelect.value = localStorage.getItem('oneshot.skin') || '0'; } catch { /* private window */ }

  // Where the game's own endpoints are: beside the page in dev, the game box
  // when the page is published to elopros.com (`ci/publish_web.sh` writes
  // `origin.json`).
  let base = '';
  try {
    const o = await fetch('origin.json');
    if (o.ok) base = (await o.json()).server ?? '';
  } catch { /* none: beside the page */ }
  const info = await (await fetch(`${base}server.json`)).json();
  info.base = base;
  const signin = await setupSignIn(info, el('signin'), document.getElementById('who')!, [el('duel'), el('doubles'), el('cup')]);

  const MODES: Record<string, number> = { play: 0, duel: 1, doubles: 2, ffa: 3, gungame: 4, cup: 5, bigteam: 6 };
  let client: Client;
  let mode = 0;
  let name = 'player';
  // A pass goes with every mode once signed in: it is what proves a skin
  // is yours. Casual without one is a guest, in the standard look.
  // No hash: a CA's certificate, checked by the browser, never pinned.
  const connect = () => Client.connect(info.addr, info.hash ?? '', name, signin.pass(), mode, Number(skinSelect.value) || 0);
  try {
    mode = AUTOPLAY
      ? Number(params.get('mode') ?? 0)
      : await new Promise<number>((go) => {
          for (const id of Object.keys(MODES)) el(id).addEventListener('click', () => go(MODES[id]), { once: true });
        });
    name = nameInput.value.trim() || 'player';
    try {
      localStorage.setItem('oneshot.name', name);
      localStorage.setItem('oneshot.skin', skinSelect.value);
    } catch { /* private window */ }
    status.textContent = 'connecting…';
    client = await connect();
  } catch (e) {
    shared.error = String(e);
    status.textContent = String(e);
    return;
  }

  // What the server says on the stream, before and during play.
  const slotName = (n: { bot: boolean; name: string }) => (n.bot ? 'BOT' : n.name);
  let ended = false;
  // A cup: where we stand, as the server last said.
  let cupLine = '';
  const ordinal = (n: number) => `${n}${['th', 'st', 'nd', 'rd'][n % 100 > 10 && n % 100 < 14 ? 0 : Math.min(n % 10, 4) % 4] || 'th'}`;
  const handle = (m: any) => {
    if (m.type === 'seat') {
      hud.names[m.slot] = slotName(m);
      players.skins[m.slot] = m.skin;
    }
    else if (m.type === 'queued') {
      const t = m.waiting as number;
      status.textContent = `${m.mode === 5 ? 'Waiting for the cup to fill' : `Finding ${m.mode === 2 ? 'a 2v2' : 'an opponent'}`}… ${Math.floor(t / 60)}:${String(t % 60).padStart(2, '0')}`;
    } else if (m.type === 'cup') {
      const stage = m.round > m.rounds ? 'the final' : `round ${m.round} of ${m.rounds}`;
      cupLine = m.done
        ? `Cup over: ${ordinal(m.place)} of ${m.entrants}`
        : `Cup, ${stage}: ${ordinal(m.place)} of ${m.entrants} on ${m.points} points`;
      status.textContent = cupLine;
    } else if (m.type === 'refused') {
      shared.error = m.why;
      status.textContent = m.why;
    } else if (m.type === 'bye') ended = true;
    return m;
  };

  // Waiting for the welcome: at once against the bot, when the queue forms
  // a match for ranked.
  const cancel = el('cancel');
  cancel.hidden = false;
  cancel.onclick = () => {
    client.close();
    location.reload();
  };
  // "Full or restarting" from a server on its way down: if the live server
  // is now another (the deploy's other slot, ops/README.md), go there once,
  // with the same pass. True if we did.
  const REFUSE_FULL = 2;
  let moved = false;
  const follow = async (): Promise<boolean> => {
    if (moved) return false;
    moved = true;
    try {
      const now = await (await fetch(`${base}server.json`)).json();
      if (!now.addr || now.addr === info.addr) return false;
      client.close();
      info.addr = now.addr;
      info.hash = now.hash;
      client = await connect();
      return true;
    } catch {
      return false;
    }
  };
  await new Promise<void>((ready) => {
    const wait = () => {
      client.update(performance.now(), 0, 0, 0, 0, 0);
      for (const m of client.messages()) {
        if (m.type === 'refused' && m.reason === REFUSE_FULL && !moved) {
          status.textContent = 'moving to the new server…';
          follow().then((ok) => (ok ? wait() : handle(m)));
          return;
        }
        handle(m);
      }
      if (client.ready) return ready();
      if (client.closed || shared.error) return;
      requestAnimationFrame(wait);
    };
    wait();
  });
  cancel.hidden = true;
  menu.style.display = 'none';
  let gunGame = false;
  // The level the welcome named, from the sim's own tables; built again
  // when a new welcome comes (a cup's next heat, maybe on another map).
  let level: ReturnType<typeof buildWorld> | null = null;
  let welcomed = 0;
  const mats = defaultMaterials();
  const welcome = () => {
    welcomed = client.welcomes;
    gunGame = client.gun_game;
    if (level) dropWorld(level.level);
    vehicles.clear();
    vehicles.terrain = map_terrain(client.map);
    const layout = map_layout(client.map);
    level = buildWorld(scene, map_boxes(client.map), map_pads(client.map), mats, layout, map_terrain(client.map), map_launchers(client.map));
    route = layout.length > 4 ? Array.from({ length: (layout.length - 4) / 2 }, (_, k) => [layout[4 + 2 * k], layout[5 + 2 * k]]) : ROUTE;
    leg = 0;
    hud.kill(map_name(client.map));
  };
  welcome();
  shared.connected = true;
  const controls = new Controls(canvas);
  if (!AUTOPLAY) canvas.click();
  addEventListener('keydown', (e) => {
    if (e.code === 'KeyN') hud.netOn = !hud.netOn;
  });

  let last = performance.now();
  const v = new THREE.Vector3(), w = new THREE.Vector3();
  const loop = () => {
    const now = performance.now();
    const dt = Math.min(0.1, (now - last) / 1000);
    last = now;

    if (client.welcomes !== welcomed) welcome();
    let f = controls.frame();
    if (AUTOPLAY) f = autopilot(f, shared);
    client.update(now, f.buttons, f.yaw, f.pitch, f.mx, f.mz);

    const own = client.own();
    shared.own = Array.from(own);
    shared.mate = client.teams ? client.team : -1;
    const [px, py, pz, eye, , , alive, , shield, health, arm, mag, reserve, other, grenades, reloading, , kills, deaths, respawn] = own;
    // At the wheel: the camera rides behind the vehicle, looking where we
    // look; a passenger sees from its seat.
    const driving = own[22] === 0;
    // Aim is drawn from the controls (no round trip) on top of the
    // predicted body.
    const yaw = AUTOPLAY ? (f.yaw / 65536) * 2 * Math.PI : controls.yaw;
    const pitch = AUTOPLAY ? (f.pitch / 65536) * 2 * Math.PI : controls.pitch;
    // Dead, and we saw who did it: the run-up to the kill from their eye,
    // made of what we were sent (client-net's killcam).
    const cam = alive ? null : client.killcam(now);
    const killcam = cam !== null && cam.length >= 7;
    if (killcam) {
      const c = cam!;
      camera.position.set(c[0], c[1], c[2]);
      const kp = Math.cos(c[4]);
      camera.lookAt(c[0] + kp * Math.cos(c[3]), c[1] + Math.sin(c[4]), c[2] + kp * Math.sin(c[3]));
    } else if (driving) {
      const cp = Math.cos(pitch);
      const back = 7.5;
      camera.position.set(px - cp * Math.cos(yaw) * back, py + VEHICLE_H + 2.2 - Math.sin(pitch) * back, pz - cp * Math.sin(yaw) * back);
      camera.lookAt(px, py + VEHICLE_H, pz);
    } else {
      camera.position.set(px, py + eye, pz);
      const cp = Math.cos(pitch);
      camera.lookAt(px + cp * Math.cos(yaw), py + eye + Math.sin(pitch), pz + cp * Math.sin(yaw));
    }
    const zoom = (f.buttons & ZOOM) !== 0;
    const zoomed = zoom ? (arm === 4 ? 5 : 2) : 1;
    camera.fov = 78 / zoomed;
    camera.updateProjectionMatrix();

    const others = client.others(now);
    shared.others = Array.from(others);
    players.update(killcam ? cam!.subarray(7) : others, dt);
    vehicles.update(client.vehicles(now));
    shared.killcams += killcam && !shared.inKillcam ? 1 : 0;
    shared.inKillcam = killcam;
    effects.projectiles(client.projectiles());

    const slot = client.slot;
    const ev = client.events();
    for (let i = 0; i + 11 <= ev.length; i += 11) {
      const kind = ev[i], a = ev[i + 1], b = ev[i + 2], part = ev[i + 3], c = ev[i + 4];
      v.set(ev[i + 5], ev[i + 6], ev[i + 7]);
      w.set(ev[i + 8], ev[i + 9], ev[i + 10]);
      if (kind === EV_SHOT && part === HEARD) {
        // Fog: a shot from someone we can't see is only a sound, placed
        // roughly, with no tracer and no direction.
        art?.shot(false, c, v);
      } else if (kind === EV_SHOT && c !== ROCKET) {
        effects.tracer(v, w, a === slot);
        effects.spark(w);
        art?.shot(a === slot, c, v);
      } else if (kind === EV_HIT) {
        if (a === slot && b !== slot) hud.hit(false);
        if (b === slot) {
          // Where it came from, relative to where we look.
          const s = others.findIndex((_, j) => j % STRIDE === 0 && others[j] === a);
          if (s >= 0) hud.damage(Math.atan2(others[s + 8] - pz, others[s + 6] - px) - yaw + Math.PI / 2);
        }
        art?.hit(a === slot && b !== slot, part, b === slot);
      } else if (kind === EV_KILL) {
        if (a === slot && b !== slot) hud.hit(true);
      } else if (kind === EV_BLAST) {
        effects.blast(v, c === ROCKET ? 5 : 5);
        art?.blast(v);
      }
    }
    for (const m of client.messages()) {
      handle(m);
      if (m.type === 'kill') {
        hud.kill(m.killer === m.victim ? `${hud.name(m.killer)} died` : `${hud.name(m.killer)} ⟶ ${hud.name(m.victim)}`);
        if (m.killer === slot && m.victim !== slot) shared.kills++;
        if (m.victim === slot) shared.deaths++;
      }
    }

    const ms = client.match_state();
    const phase = ms[0], clock = ms[1], winner = ms[2], limit = ms[3];
    const scores = Array.from(ms.subarray(5, 5 + ms[4]));
    const mmss = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`;
    let center = '';
    if (phase === 0) center = `Starting in ${Math.ceil(clock)}`;
    else if (phase === 2) {
      center = winner === slot ? 'You win' : winner === 255 ? 'Draw' : `${hud.name(winner)} wins`;
      // The salt every spread was drawn from, checked against the
      // commitment the server made when we joined.
      const v = client.stats()[8];
      if (v === 1) center += ' · match verified ✓';
      if (v === 0) center += ' · verification FAILED';
      if (cupLine) center += `\n${cupLine}`;
    }
    else if (!alive && killcam) center = `Killed by ${hud.name(cam![5])} · respawning in ${Math.ceil(respawn)}`;
    else if (!alive) center = `Respawning in ${Math.ceil(respawn)}`;
    const st = client.stats();
    shared.stats = Array.from(st);
    hud.update({
      shield, health, ammo: String(mag), reserve: String(reserve), grenades,
      weapon: arm_name(arm), other: other ? arm_name(other) : '',
      score: [
        `${hud.name(slot)} ${scores[slot]} / ${limit}`,
        // The rest, best first: everyone in a duel or doubles, the top
        // four in a bigger room.
        ...scores.map((s, i) => [s, i] as const).filter(([, i]) => i !== slot && hud.names[i])
          .sort((a, b) => b[0] - a[0]).slice(0, 4).map(([s, i]) => `${hud.name(i)} ${s}`),
        gunGame ? `Rung ${Math.min(scores[slot], limit - 1) + 1} of ${limit}${scores[slot] < limit - 1 ? ` · next ${arm_name(client.rung(scores[slot] + 1))}` : ' · last gun'}` : '',
        phase === 1 ? mmss(clock) : '',
        `K ${kills} · D ${deaths}`,
      ],
      center, zoom: !killcam && zoom && (arm === 4 || arm === 2 || arm === 3), reloading, killcam,
      net: `snapshots ${st[0]}\nconfirmed ${st[1]}  corrected ${st[2]}\ndepth ${st[4]}  rate ${st[5].toFixed(2)}\ninterp ${st[6].toFixed(1)} ticks  dropped ${st[7]}`,
    });

    // Hands on the wheel: no gun in view.
    art?.frame(driving ? own.map((x, i) => (i === 10 ? 0 : x)) : own, f, dt, now, camera);
    effects.update();
    renderer.render(scene, camera);
    art?.afterRender(renderer);
    shared.frames++;
    if (client.closed || ended) {
      // The match is over (or the line dropped): back to the menu.
      status.textContent = ended ? 'Match over' : 'Disconnected';
      for (const id of Object.keys(MODES)) el(id).hidden = true;
      cancel.hidden = false;
      cancel.textContent = 'Back to menu';
      menu.style.display = 'flex';
      document.exitPointerLock();
      return;
    }
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);
}

/** ?pose: no server. One fighter on the floor in front of the camera, for
 * looking at the models (ci/look.mjs takes pictures of it). ?yaw= turns the
 * fighter (radians), ?arm= picks its gun, ?anim=speed, ?skin= its skin,
 * ?team= its side. */
function pose(scene: THREE.Scene, camera: THREE.PerspectiveCamera, renderer: THREE.WebGLRenderer,
  players: Players, art: Awaited<ReturnType<typeof import('./art').loadArt>> | null) {
  document.getElementById('menu')!.style.display = 'none';
  const yaw = Number(params.get('yaw') ?? Math.PI), arm = Number(params.get('arm') ?? 1);
  const speed = Number(params.get('speed') ?? 0);
  players.skins[1] = Number(params.get('skin') ?? 0);
  const o = new Float32Array([1, Number(params.get('team') ?? 0), 1, 0, 1, 0, 3, 0, 0, yaw, 0, arm, 1, speed]);
  const own = new Float32Array(22);
  own[3] = 1.6; own[6] = 1; own[10] = arm; own[11] = 10; own[20] = 1;
  camera.position.set(0, 1.6, 0);
  camera.lookAt(3, 1.0, 0);
  let last = performance.now();
  const loop = () => {
    const now = performance.now();
    const dt = Math.min(0.1, (now - last) / 1000);
    last = now;
    players.update(o, dt);
    art?.frame(own, { buttons: 0, yaw: 0, pitch: 0, mx: 0, mz: 0 }, dt, now, camera);
    renderer.render(scene, camera);
    art?.afterRender(renderer);
    shared.frames++;
    requestAnimationFrame(loop);
  };
  shared.connected = !!art;
  requestAnimationFrame(loop);
}

/** For the browser test (?autoplay): walk at the nearest enemy,
 * aim at them and keep pulling the trigger. Only inputs: the server still
 * decides every hit. */
// Where the autopilot walks while fog shows it nobody: the map's roam
// points, or round the open lanes of Proving Grounds (which has none).
const ROUTE = [[-15, 8], [15, 8], [21, 0], [15, -8], [-15, -8], [-21, 0]];
let route = ROUTE;
let leg = 0;

function autopilot(f: ReturnType<Controls['frame']>, s: Shared) {
  const o = s.others, me = s.own;
  if (me.length === 0) return f;
  const TURN = 65536;
  const toYaw = (a: number) => (((Math.round((a / (2 * Math.PI)) * TURN)) % TURN) + TURN) % TURN;
  let best = -1, bd = Infinity;
  for (let i = 0; i + STRIDE <= o.length; i += STRIDE) {
    if (!o[i + 2] || o[i + 1] === s.mate) continue;
    const d = Math.hypot(o[i + 6] - me[0], o[i + 8] - me[2]);
    if (d < bd) { bd = d; best = i; }
  }
  if (best < 0) {
    // Nobody in sight: walk the route until fog shows someone.
    let [tx, tz] = route[leg % route.length];
    if (Math.hypot(tx - me[0], tz - me[2]) < 2 || s.frames % 600 === 0) {
      leg = (leg + 1) % route.length;
      [tx, tz] = route[leg];
    }
    return { buttons: 0, yaw: toYaw(Math.atan2(tz - me[2], tx - me[0])), pitch: 0, mx: 0, mz: 127 };
  }
  const dx = o[best + 6] - me[0], dz = o[best + 8] - me[2];
  const dy = o[best + 7] + 1.3 - (me[1] + me[3]);
  const yaw = Math.atan2(dz, dx);
  const pitch = Math.atan2(dy, Math.hypot(dx, dz));
  return {
    buttons: (s.frames & 1) ? FIRE : 0,
    yaw: toYaw(yaw),
    pitch: Math.round((pitch / (2 * Math.PI)) * TURN),
    mx: (s.frames >> 6) & 1 ? 60 : -60,
    mz: bd > 8 ? 127 : 0,
  };
}

main();

/** Sign in with Elo Pros: a pass for this game, from Elo Pros' popup
 * (https://elopros.com/sdk/elo-signin-1.js). The server hands out the
 * nonce the pass must carry; we fetch one ahead so the popup opens on the
 * click itself (a popup opened later is blocked). Ranked needs a wallet
 * linked to the account: ratings are a wallet's. */
async function setupSignIn(info: any, button: HTMLButtonElement, who: HTMLElement, ranked: HTMLButtonElement[]) {
  let pass = '';
  let nonce = '';
  const fresh = async () => {
    try { nonce = (await (await fetch(`${info.base}nonce`)).json()).nonce; } catch { nonce = ''; }
  };
  const sdk = () => (window as any).EloSignIn;
  // The SDK script is async: give it a moment.
  for (let i = 0; i < 20 && !sdk(); i++) await new Promise((r) => setTimeout(r, 100));
  if (info.signin && sdk()) {
    button.hidden = false;
    await fresh();
    button.onclick = async () => {
      try {
        const res = await sdk().popup({ game: info.game, nonce });
        pass = res.pass;
        const text = atob(pass.split('.')[0].replace(/-/g, '+').replace(/_/g, '/'));
        const wallet = /wallet: (0x[0-9a-f]{40})/.exec(text)?.[1];
        who.textContent = wallet ? `Signed in · ${wallet.slice(0, 6)}…${wallet.slice(-4)}` : 'Signed in · link a wallet on Elo Pros to play ranked';
        for (const b of ranked) b.disabled = !wallet;
        button.hidden = true;
      } catch (e: any) {
        who.textContent = e?.code === 'cancelled' || e?.code === 'closed' ? '' : `Sign-in failed: ${e?.message ?? e}`;
      }
      await fresh();
    };
  } else {
    who.textContent = info.signin ? '' : 'Ranked is off on this server';
  }
  return { pass: () => pass };
}

// The page: connect, then once a frame read the controls, hand them to the
// netcode, and draw what it says. client-net decides nothing here either:
// the server does, and the page only draws.

import * as THREE from 'three';
import init, { Client, map_boxes, map_pads, arm_name } from './pkg/client_net.js';
import { Controls, FIRE, ZOOM } from './input';
import { buildWorld, defaultMaterials } from './world';
import { Players, STRIDE } from './players';
import { Effects } from './effects';
import { Hud } from './hud';

const params = new URLSearchParams(location.search);
const AUTOPLAY = params.has('autoplay');

// Event kinds (sim::events::Kind).
const EV_SHOT = 1, EV_HIT = 2, EV_KILL = 3, EV_BLAST = 6;
const ROCKET = 6;

interface Shared {
  connected: boolean;
  frames: number;
  kills: number;
  deaths: number;
  stats: number[];
  error: string;
  own: number[];
  others: number[];
}
const shared: Shared = { connected: false, frames: 0, kills: 0, deaths: 0, stats: [], error: '', own: [], others: [] };
(window as any).__oneshot = { state: () => shared };

async function main() {
  await init();
  const canvas = document.getElementById('game') as HTMLCanvasElement;
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, powerPreference: 'high-performance' });
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.PCFSoftShadowMap;
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

  buildWorld(scene, map_boxes(), map_pads(), defaultMaterials());
  const players = new Players(scene);
  const effects = new Effects(scene);
  const hud = new Hud();
  const hooks = await import('./art').catch(() => null);
  const art = hooks ? await hooks.loadArt(scene, camera, renderer, players).catch((e: unknown) => {
    console.warn('art did not load', e);
    return null;
  }) : null;

  const menu = document.getElementById('menu')!;
  const status = document.getElementById('status')!;
  const nameInput = document.getElementById('name') as HTMLInputElement;
  nameInput.value = params.get('name') || localStorage.getItem('oneshot.name') || '';

  async function join(): Promise<Client> {
    const name = nameInput.value.trim() || 'player';
    try { localStorage.setItem('oneshot.name', name); } catch { /* private window */ }
    status.textContent = 'connecting…';
    const info = await (await fetch('server.json')).json();
    return Client.connect(info.addr, info.hash, name);
  }

  let client: Client;
  try {
    if (!AUTOPLAY) {
      await new Promise<void>((go) => document.getElementById('play')!.addEventListener('click', () => go(), { once: true }));
    }
    client = await join();
  } catch (e) {
    shared.error = String(e);
    status.textContent = String(e);
    return;
  }
  menu.style.display = 'none';
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

    let f = controls.frame();
    if (AUTOPLAY) f = autopilot(f, shared);
    client.update(now, f.buttons, f.yaw, f.pitch, f.mx, f.mz);

    const own = client.own();
    shared.own = Array.from(own);
    const [px, py, pz, eye, , , alive, , shield, health, arm, mag, reserve, other, grenades, reloading, , kills, deaths, respawn] = own;
    // Aim is drawn from the controls (no round trip) on top of the
    // predicted body.
    const yaw = AUTOPLAY ? (f.yaw / 65536) * 2 * Math.PI : controls.yaw;
    const pitch = AUTOPLAY ? (f.pitch / 65536) * 2 * Math.PI : controls.pitch;
    camera.position.set(px, py + eye, pz);
    const cp = Math.cos(pitch);
    camera.lookAt(px + cp * Math.cos(yaw), py + eye + Math.sin(pitch), pz + cp * Math.sin(yaw));
    const zoom = (f.buttons & ZOOM) !== 0;
    const zoomed = zoom ? (arm === 4 ? 5 : 2) : 1;
    camera.fov = 78 / zoomed;
    camera.updateProjectionMatrix();

    const others = client.others(now);
    shared.others = Array.from(others);
    players.update(others, dt);
    effects.projectiles(client.projectiles());

    const slot = client.slot;
    const ev = client.events();
    for (let i = 0; i + 11 <= ev.length; i += 11) {
      const kind = ev[i], a = ev[i + 1], b = ev[i + 2], part = ev[i + 3], c = ev[i + 4];
      v.set(ev[i + 5], ev[i + 6], ev[i + 7]);
      w.set(ev[i + 8], ev[i + 9], ev[i + 10]);
      if (kind === EV_SHOT && c !== ROCKET) {
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
        art?.hit(a === slot, part);
      } else if (kind === EV_KILL) {
        if (a === slot && b !== slot) hud.hit(true);
      } else if (kind === EV_BLAST) {
        effects.blast(v, c === ROCKET ? 5 : 5);
        art?.blast(v);
      }
    }
    for (const m of client.messages()) {
      if (m.type === 'seat') hud.names[m.slot] = m.bot ? 'BOT' : m.name;
      if (m.type === 'kill') {
        hud.kill(m.killer === m.victim ? `${hud.name(m.killer)} died` : `${hud.name(m.killer)} ⟶ ${hud.name(m.victim)}`);
        if (m.killer === slot && m.victim !== slot) shared.kills++;
        if (m.victim === slot) shared.deaths++;
      }
    }

    const ms = client.match_state();
    const phase = ms[0], clock = ms[1], winner = ms[2], limit = ms[3];
    const scores = Array.from(ms.subarray(4, 12));
    const mmss = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`;
    let center = '';
    if (phase === 0) center = `Starting in ${Math.ceil(clock)}`;
    else if (phase === 2) center = winner === slot ? 'You win' : winner === 255 ? 'Draw' : `${hud.name(winner)} wins`;
    else if (!alive) center = `Respawning in ${Math.ceil(respawn)}`;
    const st = client.stats();
    shared.stats = Array.from(st);
    hud.update({
      shield, health, ammo: String(mag), reserve: String(reserve), grenades,
      weapon: arm_name(arm), other: other ? arm_name(other) : '',
      score: [
        `${hud.name(slot)} ${scores[slot]} / ${limit}`,
        ...scores.map((s, i) => [s, i] as const).filter(([, i]) => i !== slot && hud.names[i]).map(([s, i]) => `${hud.name(i)} ${s}`),
        phase === 1 ? mmss(clock) : '',
        `K ${kills} · D ${deaths}`,
      ],
      center, zoom: zoom && (arm === 4 || arm === 2 || arm === 3), reloading,
      net: `snapshots ${st[0]}\nconfirmed ${st[1]}  corrected ${st[2]}\ndepth ${st[4]}  rate ${st[5].toFixed(2)}\ninterp ${st[6].toFixed(1)} ticks  dropped ${st[7]}`,
    });

    art?.frame(own, f, dt, now);
    effects.update();
    renderer.render(scene, camera);
    shared.frames++;
    if (client.closed) {
      status.textContent = 'disconnected';
      menu.style.display = 'flex';
      return;
    }
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);
}

/** For the browser test (?autoplay): walk at the nearest other fighter,
 * aim at them and keep pulling the trigger. Only inputs: the server still
 * decides every hit. */
function autopilot(f: ReturnType<Controls['frame']>, s: Shared) {
  const o = s.others, me = s.own;
  if (o.length < STRIDE || me.length === 0) return f;
  let best = -1, bd = Infinity;
  for (let i = 0; i + STRIDE <= o.length; i += STRIDE) {
    if (!o[i + 2]) continue;
    const d = Math.hypot(o[i + 6] - me[0], o[i + 8] - me[2]);
    if (d < bd) { bd = d; best = i; }
  }
  if (best < 0) return { ...f, buttons: 0 };
  const dx = o[best + 6] - me[0], dz = o[best + 8] - me[2];
  const dy = o[best + 7] + 1.3 - (me[1] + me[3]);
  const yaw = Math.atan2(dz, dx);
  const pitch = Math.atan2(dy, Math.hypot(dx, dz));
  const TURN = 65536;
  return {
    buttons: (s.frames & 1) ? FIRE : 0,
    yaw: (((Math.round((yaw / (2 * Math.PI)) * TURN)) % TURN) + TURN) % TURN,
    pitch: Math.round((pitch / (2 * Math.PI)) * TURN),
    mx: (s.frames >> 6) & 1 ? 60 : -60,
    mz: bd > 8 ? 127 : 0,
  };
}

main();

// The page's menu: the dashboard. The game (Bevy, `front.rs`) runs behind
// it from the start, its live backdrop under the green glass; the menu picks
// what to play and signs in, the game joins it (`knock`) and comes back here
// when the match ends. The lobby session (the party, its friends, its voice)
// stays open the whole time. On the desktop the game's own title does this.
import { Voice, voiceWorks } from './voice.js';

// Raw mouse: the game's lock (winit asks with no options) asks the browser
// for unadjusted movement, so the OS's acceleration doesn't bend aim. Not on
// Linux, where Chrome refuses and a second request can lose the click; a
// refusal anywhere is remembered and asked plainly at once (Gates' app.js).
{
  const canvas = document.getElementById('game');
  const plain = canvas.requestPointerLock;
  const RAW_REFUSED = 'oneshot.rawMouseRefused';
  let refused = /Linux/.test(navigator.platform) && !/CrOS/.test(navigator.userAgent);
  try { refused = refused || localStorage.getItem(RAW_REFUSED) === '1'; } catch { /* private window */ }
  canvas.requestPointerLock = function () {
    if (refused) return plain.call(this);
    let p;
    try { p = plain.call(this, { unadjustedMovement: true }); } catch { return plain.call(this); }
    if (p && p.catch) p.catch((err) => {
      if (!err || err.name !== 'NotSupportedError') return;
      refused = true;
      try { localStorage.setItem(RAW_REFUSED, '1'); } catch { /* private window */ }
      plain.call(this);
    });
    return p;
  };
}

const params = new URLSearchParams(location.search);
const flag = (k) => params.get(k) === '1';
const AUTOPLAY = flag('autoplay');
// The effects demo (`?fxdemo=1`, `?fxstage=N`) skips the menu too.
const SKIP_MENU = AUTOPLAY || flag('fxdemo') || params.has('fxstage');
if (flag('lite')) document.getElementById('wrap').classList.add('lite');
const $ = (id) => document.getElementById(id);
const menu = $('menu'), status = $('status'), cancel = $('cancel'), nameInput = $('name'), skin = $('skin');
// The boot intro plays once from the page's start; anything pressed ends it
// (its animations would start again on whatever is shown).
const booted = () => menu.classList.remove('boot');
setTimeout(booted, 1700);
addEventListener('pointerdown', booted, { once: true, capture: true });
addEventListener('keydown', booted, { once: true, capture: true });
const MODES = { play: 0, duel: 1, doubles: 2, ffa: 3, gungame: 4, cup: 5, bigteam: 6 };
const RANKED = new Set([1, 2, 5]);
const ACT = { JOIN: 1, LEAVE: 2, START: 3, KICK: 4, PRIVACY: 5, LEAD: 6, PLAYING: 7, FRIENDS: 8 };
const START_PRIVATE = 0x100;
const PLAYING_NONE = 15;
const REFUSE_FULL = 2, REFUSE_BUSY = 5;
const MAX_PARTY = 8;
const store = (k, v) => { try { v === undefined ? (v = localStorage.getItem(k)) : localStorage.setItem(k, v); } catch { /* private window */ } return v; };
// This tab's: its ticket, its party, a game its party started while it reloaded.
const tab = (k, v) => {
  try {
    if (v === undefined) return sessionStorage.getItem(k);
    if (v === null) sessionStorage.removeItem(k); else sessionStorage.setItem(k, v);
  } catch { /* private window */ }
  return v;
};
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const short = (id) => (id && id.length > 12 ? `${id.slice(0, 6)}…${id.slice(-4)}` : id || '');
const say = (line, bad) => { status.textContent = line; status.classList.toggle('bad', !!bad); };
// The toast whose face buttons answer it (an invite).
let toastActions = null;

// ── the game's module (after Gates' web/app.js, pickModule) ─────────────
// Bevy picks its GPU API when it is compiled and has no fallback at run
// time, so the page picks before the download: the WebGPU module for a
// hardware adapter with filterable 32-bit floats, else WebGL2 (ci/client.sh
// builds both; `?gfx=webgl2|webgpu` forces one). A WebGPU module that failed
// here is remembered against that build (build.json's hash).
const gfxOpts = [
  ...(params.has('potato') && params.get('potato') !== '0' ? ['potato'] : []),
  ...(/^(potato|low|medium|high)$/.test(params.get('quality') ?? '') ? ['quality=' + params.get('quality')] : []),
];
const MODULES = {
  webgl2: { js: './oneshot_client.js', wasm: 'oneshot_client_bg.wasm', name: 'WebGL2' },
  webgpu: { js: './oneshot_client_gpu.js', wasm: 'oneshot_client_gpu_bg.wasm', name: 'WebGPU' },
};
const GPU_FAILED = 'oneshot.webgpu.failed';
const built = fetch('build.json', { cache: 'no-cache' }).then((r) => (r.ok ? r.json() : null)).catch(() => null);
const gpuProbe = (async () => {
  if (!navigator.gpu) return { ok: false, why: 'no WebGPU in this browser' };
  try {
    const adapter = await Promise.race([
      navigator.gpu.requestAdapter({ powerPreference: 'high-performance' }),
      new Promise((r) => setTimeout(() => r(null), 3000)),
    ]);
    if (!adapter) return { ok: false, why: 'no WebGPU adapter' };
    const info = adapter.info || {};
    if (info.isFallbackAdapter || adapter.isFallbackAdapter) return { ok: false, why: 'a software WebGPU adapter' };
    if (!adapter.features.has('float32-filterable')) return { ok: false, why: 'no float32-filterable' };
    return { ok: true, why: 'a WebGPU adapter' };
  } catch {
    return { ok: false, why: 'the WebGPU adapter was refused' };
  }
})();
let mods = null;
const gpuKey = () => String(mods?.webgpu?.sha256 ?? mods?.webgpu?.wasm_bytes ?? '');
async function pickModule() {
  const has = (k) => (mods ? !!mods[k] : k === 'webgl2');
  const forced = params.get('gfx');
  if (forced === 'webgpu') { try { localStorage.removeItem(GPU_FAILED); } catch { /* private window */ } }
  if ((forced === 'webgl2' || forced === 'webgpu') && has(forced)) return { gfx: forced, why: 'forced by ?gfx' };
  if (!has('webgpu')) return { gfx: 'webgl2', why: 'this build has no WebGPU module' };
  if (!has('webgl2')) return { gfx: 'webgpu', why: 'this build has no WebGL2 module' };
  try {
    if (localStorage.getItem(GPU_FAILED) === gpuKey()) return { gfx: 'webgl2', why: 'WebGPU failed here before' };
  } catch { /* private window */ }
  const probe = await gpuProbe;
  return { gfx: probe.ok ? 'webgpu' : 'webgl2', why: probe.why };
}
async function loadModule(gfx) {
  const m = MODULES[gfx];
  // Glue and module are a pair: both asked for by the pair's hash.
  const sha = mods?.[gfx]?.sha256;
  const v = sha ? `?v=${sha.slice(0, 12)}` : '';
  const res = fetch(new URL(m.wasm + v, location.href));
  res.catch(() => { /* awaited by the glue */ });
  const g = await import(m.js + v);
  await g.default({ module_or_path: res });
  window.__oneshotGfx = gfx;
  return g;
}
let started = 0;
/* Back into WebGL2, once: the failure is written against this build first,
   and a canvas that held a WebGPU context never takes a WebGL2 one, so the
   way there is a reload. */
function gpuFallBack(why) {
  let kept = false;
  try {
    localStorage.setItem(GPU_FAILED, gpuKey());
    kept = localStorage.getItem(GPU_FAILED) === gpuKey();
  } catch { /* storage blocked */ }
  const u = new URL(location.href);
  if (!kept || params.get('gfx') === 'webgpu') u.searchParams.set('gfx', 'webgl2');
  console.warn(`oneshot: ${why}: switching to WebGL2`);
  location.replace(u);
}
// The WebGPU module's device hooks (look/gpu.rs) and every panic.
window.__oneshotGpuError = (msg, kind) => {
  if (window.__oneshotGfx !== 'webgpu') return;
  if (kind === 'lost' || !st.gameUp) { gpuFallBack('WebGPU lost the device or refused the frame'); return; }
  try { localStorage.setItem(GPU_FAILED, gpuKey()); } catch { /* private window */ }
  const u = new URL(location.href);
  u.searchParams.set('gfx', 'webgl2');
  toast(`The graphics card refused part of the frame under WebGPU. <a href="${esc(u)}" style="color:var(--lime)">Reload with WebGL2</a>`, { ttl: 0, bad: true });
};
window.__oneshotPanic = (msg) => {
  const m = String(msg || '');
  const gpu = window.__oneshotGfx === 'webgpu';
  const renderer = /bevy_render|Unable to find a GPU|wgpu surface|request_device|adapter/.test(m);
  if (gpu && renderer && started && performance.now() - started < 30000) { gpuFallBack('WebGPU did not start here'); return; }
  if (gpu && /wgpu-[\d.]+\/src\/backend\/webgpu|GPUDevice|GPUQueue|GPUCanvasContext/.test(m)) { gpuFallBack('the graphics card refused WebGPU'); return; }
  say('The game stopped. Reload to try again.', true);
};
mods = (await built)?.modules ?? null;
let { gfx, why: gfxWhy } = await pickModule();
let glue;
try {
  glue = await loadModule(gfx);
} catch (err) {
  if (gfx !== 'webgpu' || !mods?.webgl2) throw err;
  console.warn('oneshot: the WebGPU module failed to load, loading WebGL2', err);
  gfx = 'webgl2';
  gfxWhy = 'the WebGPU module did not load';
  glue = await loadModule(gfx);
}
const { Lobby, party_code, mode_name, skin_names } = glue;
$('gfx').textContent = ` · ${MODULES[gfx].name.toUpperCase()}`;
$('gfx').title = gfxWhy;
/* FULLSCREEN (the settings' row) is the page's: while fullscreen, Keyboard
   Lock hands Esc to the game. */
document.addEventListener('fullscreenchange', () => {
  const kb = navigator.keyboard;
  if (!kb || !kb.lock) return;
  if (document.fullscreenElement) kb.lock(['Escape']).catch(() => {});
  else kb.unlock();
});
if (params.has('pose')) {
  // No server: one fighter for a picture (ci/look.mjs).
  menu.style.display = 'none';
  started = performance.now();
  glue.pose(location.search, ['lite'].filter(flag).concat(gfxOpts).join(','));
  await new Promise(() => {}); // nothing more for the page to do
}
nameInput.value = params.get('name') || store('oneshot.name') || '';
// Skins: worn only if the signed-in wallet owns one; the server checks.
skin_names().forEach((n, i) => skin.add(new Option(n[0].toUpperCase() + n.slice(1), String(i))));
skin.value = store('oneshot.skin') || '0';
const myName = () => nameInput.value.trim() || 'player';

// Where the game's own endpoints are: beside the page in dev, the game box
// when the page is published to elopros.com (ci/publish_web.sh writes
// origin.json).
let base = '';
try {
  const o = await fetch('origin.json');
  if (o.ok) base = (await o.json()).server ?? '';
} catch { /* none: beside the page */ }
let info = await (await fetch(`${base}server.json`)).json();

// A server on its way down: the live one, if it is now another.
const follow = async () => {
  try {
    const now = await (await fetch(`${base}server.json`)).json();
    if (now.addr && now.addr !== info.addr) {
      info = now;
      return [now.addr, now.hash ?? ''];
    }
  } catch { /* stay */ }
  return null;
};

// ── who you are, your party, your friends ───────────────────────────────
const st = {
  ticket: tab('oneshot.ticket') || '',
  who: null,
  party: null,
  friends: [], friendsNext: [], friendsN: -1, reachable: true,
  lobby: null, busy: false, playing: false,
  // The game has drawn behind the menu: a click on a mode counts now.
  gameUp: false,
};
/** Who a ticket names: `os1.` b64url("id\naccount\nwallet\nexp"). */
function whoOf(t) {
  try {
    const b = t.split('.')[1].replace(/-/g, '+').replace(/_/g, '/');
    const [id, account, wallet] = atob(b + '==='.slice((b.length + 3) % 4)).split('\n');
    return { id, account: account === 'none' ? null : account, wallet: wallet === 'none' ? null : wallet };
  } catch { return null; }
}
st.who = st.ticket ? whoOf(st.ticket) : null;
const signedIn = () => !!(st.who && st.who.account);
const members = () => (st.party ? st.party.members : []);
const me = () => members().find((m) => m.tag === st.party.me);
const leads = () => !st.party || st.party.leader === st.party.me;
const leader = () => members().find((m) => m.tag === st.party?.leader);
const voiceKey = (m) => m.player || `guest:${m.name}`;

// ── sound: the dashboard's blips and its hum (ours, synthesised) ─────────
const sfx = (() => {
  let ctx = null, hum = null;
  let on = store('oneshot.menuSound') !== 'off';
  const wake = () => {
    if (!ctx) { try { ctx = new AudioContext(); } catch { return null; } }
    if (ctx.state === 'suspended') ctx.resume().catch(() => {});
    return ctx;
  };
  const tone = (f0, f1, dur, type = 'sine', gain = 0.05, at = 0) => {
    if (!on || !wake()) return;
    const t = ctx.currentTime + at;
    const o = ctx.createOscillator(), g = ctx.createGain();
    o.type = type;
    o.frequency.setValueAtTime(f0, t);
    o.frequency.exponentialRampToValueAtTime(f1, t + dur);
    g.gain.setValueAtTime(0.0001, t);
    g.gain.exponentialRampToValueAtTime(gain, t + 0.008);
    g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
    o.connect(g).connect(ctx.destination);
    o.start(t);
    o.stop(t + dur + 0.02);
  };
  const humOn = () => {
    if (!on || !wake() || hum) return;
    const g = ctx.createGain();
    g.gain.value = 0;
    g.gain.linearRampToValueAtTime(0.018, ctx.currentTime + 2);
    const lp = ctx.createBiquadFilter();
    lp.type = 'lowpass';
    lp.frequency.value = 320;
    const lfo = ctx.createOscillator(), depth = ctx.createGain();
    lfo.frequency.value = 0.09;
    depth.gain.value = 120;
    lfo.connect(depth).connect(lp.frequency);
    const os = [55, 82.4, 110.3].map((f, i) => {
      const o = ctx.createOscillator();
      o.type = i === 1 ? 'triangle' : 'sine';
      o.frequency.value = f;
      o.detune.value = (i - 1) * 6;
      o.connect(lp);
      o.start();
      return o;
    });
    lp.connect(g).connect(ctx.destination);
    lfo.start();
    hum = { g, os, lfo };
  };
  const humOff = () => {
    if (!hum) return;
    const h = hum;
    hum = null;
    h.g.gain.linearRampToValueAtTime(0, ctx.currentTime + 0.6);
    setTimeout(() => { h.os.forEach((o) => o.stop()); h.lfo.stop(); }, 800);
  };
  return {
    move: () => tone(1480, 1100, 0.045, 'sine', 0.035),
    select: () => { tone(660, 880, 0.09, 'triangle', 0.05); tone(990, 1320, 0.12, 'sine', 0.03, 0.03); },
    back: () => tone(420, 190, 0.11, 'sine', 0.05),
    chime: () => [523, 659, 784, 1046].forEach((f, i) => tone(f, f * 1.01, 0.16, 'sine', 0.04, i * 0.07)),
    // The boot: a low swell rising into a bright chord.
    boot: () => {
      tone(55, 220, 1.2, 'sine', 0.06);
      tone(110, 440, 1.1, 'triangle', 0.03, 0.15);
      [659, 988, 1319].forEach((f, i) => tone(f, f, 0.9, 'sine', 0.025, 0.9 + i * 0.05));
    },
    bad: () => tone(200, 140, 0.18, 'square', 0.025),
    humOn, humOff,
    get on() { return on; },
    toggle() { on = !on; store('oneshot.menuSound', on ? 'on' : 'off'); if (on) humOn(); else humOff(); return on; },
  };
})();

// ── voice ─────────────────────────────────────────────────────────────────
const voice = new Voice((seq, end, bytes) => !!st.lobby && st.lobby.talk(seq, end, bytes), () => drawTalk());
const micBtn = $('mic');
if (!voiceWorks()) {
  micBtn.disabled = true;
  micBtn.textContent = 'No voice';
  micBtn.title = "This browser can't do voice";
}
$('talk-mode').value = voice.mode;
$('talk-mode').onchange = (e) => { voice.setMode(e.target.value); drawMic(); };
$('voice-vol').value = String(Math.round(voice.volume * 100));
$('voice-vol').oninput = (e) => voice.setVolume(Number(e.target.value) / 100);
micBtn.onclick = async () => {
  try {
    if (voice.mic) voice.micOff();
    else await voice.micOn();
    sfx.select();
  } catch (e) {
    toast(`The microphone didn't open: ${esc(e?.message ?? e)}`, { bad: true });
  }
  drawMic();
};
function drawMic() {
  if (micBtn.disabled) return;
  micBtn.textContent = !voice.mic ? 'Mic off' : voice.onAir ? 'Talking' : voice.mode === 'ptt' ? 'Mic on · hold V' : 'Mic on';
  micBtn.style.borderColor = voice.onAir ? 'var(--glow)' : '';
}
// The first gesture wakes sound (a browser won't before one), and the mic
// if it was on last time.
let swelled = false;
const wakeAll = async () => {
  if (!swelled && !st.playing) {
    swelled = true;
    if (!tab('oneshot.booted')) { tab('oneshot.booted', '1'); sfx.boot(); }
  }
  await voice.wake();
  if (st.playing) sfx.humOff(); else sfx.humOn();
  if (store('oneshot.mic') === 'on' && !voice.mic && voiceWorks()) {
    try { await voice.micOn(); } catch { /* asked again on the button */ }
  }
  drawMic();
};
// Without a gesture a browser may still let sound start (or keep it
// suspended until one): try now.
voice.wake();
addEventListener('pointerdown', wakeAll, { once: true, capture: true });
addEventListener('keydown', wakeAll, { once: true, capture: true });
// The browser test's fake microphone (?mic=open): on at once.
if (params.get('mic') === 'open') {
  voice.setMode('open');
  voice.wake().then(() => voice.micOn()).then(drawMic).catch((e) => console.warn('mic', e));
}

// ── sign-in ───────────────────────────────────────────────────────────────
let freshPass = '';
const signin = await setupSignIn(info, $('signin'));

// ── the lobby session ─────────────────────────────────────────────────────
let want = party_code(params.get('party') || '') || Number(tab('oneshot.party') || 0);
let backoff = 1000;
let connecting = false;
async function connectLobby() {
  if (connecting || st.lobby || st.busy || SKIP_MENU) return;
  connecting = true;
  try {
    const pass = freshPass || st.ticket || '';
    freshPass = '';
    st.lobby = await Lobby.connect(info.addr, info.hash ?? '', myName(), pass, want >>> 0, onLobby, onVoice);
    backoff = 1000;
  } catch (e) {
    st.lobby = null;
    setTimeout(connectLobby, backoff);
    backoff = Math.min(backoff * 2, 15000);
  } finally {
    connecting = false;
  }
  drawAll();
}

function onLobby(m) {
  switch (m.kind) {
    case 'ticket':
      st.ticket = m.ticket;
      tab('oneshot.ticket', m.ticket);
      st.who = whoOf(m.ticket);
      drawWho();
      if (st.who && st.who.account && recordOf !== st.who.id) {
        recordOf = st.who.id;
        showRecord(st.who);
      }
      break;
    case 'party': {
      const before = st.party;
      st.party = m;
      want = m.code;
      tab('oneshot.party', String(m.code));
      if (params.has('party')) {
        params.delete('party');
        history.replaceState(null, '', location.pathname + (params.toString() ? `?${params}` : ''));
      }
      voice.forget(m.members.map((x) => x.tag));
      if (before && m.members.length > before.members.length) sfx.chime();
      if (before && before.code !== m.code && m.members.length > 1) toast(`You're in <b>${esc(leader()?.name ?? 'a')}</b>'s party.`);
      drawAll();
      break;
    }
    case 'go':
      sfx.chime();
      startMatch(m.mode, m.private, true);
      break;
    case 'friends':
      st.friendsN = m.n;
      st.reachable = m.reachable;
      st.friendsNext = [];
      if (m.n === 0) { st.friends = []; drawFriends(); }
      break;
    case 'friend':
      st.friendsNext[m.i] = m;
      if (st.friendsNext.filter(Boolean).length >= st.friendsN) {
        st.friends = st.friendsNext.filter(Boolean);
        drawFriends();
      }
      break;
    case 'invited':
      sfx.chime();
      toast(`<b>${esc(m.from)}</b> asks you into their party.`, {
        ttl: 30000,
        actions: [
          { key: 'a', label: 'Join', fn: () => st.lobby?.act(ACT.JOIN, m.code) },
          { key: 'b', label: 'Not now' },
        ],
      });
      break;
    case 'refuse':
      if (m.reason === REFUSE_BUSY) {
        st.busy = true;
        toast('You are playing in another tab, so this one has let go.', { bad: true, ttl: 0 });
      } else if (m.reason === REFUSE_FULL) {
        // Going down for a deploy: the party's code goes with us to the live one.
        setTimeout(async () => { await follow(); st.lobby?.close(); }, 500);
      } else {
        sfx.bad();
        toast(esc(m.why), { bad: true });
      }
      break;
    case 'closed':
      st.lobby = null;
      drawAll();
      if (!st.busy) {
        setTimeout(connectLobby, backoff);
        backoff = Math.min(backoff * 2, 15000);
      }
      break;
    default:
  }
}

function onVoice(from, seq, flags, bytes) {
  const m = members().find((x) => x.tag === from);
  voice.hear(from, seq, flags, bytes, m ? voiceKey(m) : `tag:${from}`);
}

// ── drawing ─────────────────────────────────────────────────────────────
const SPEAKER = '<svg viewBox="0 0 16 16"><path d="M2 6h3l4-3v10l-4-3H2zM11 5.5a3.5 3.5 0 0 1 0 5M12.5 3.5a6 6 0 0 1 0 9" stroke="currentColor" fill="inherit" stroke-width="0"/><path d="M11.2 5.2q1.6 2.8 0 5.6l.9.5q1.9-3.3 0-6.6zM12.9 3.4q2.9 4.6 0 9.2l.9.5q3.3-5.1 0-10.2z"/></svg>';
function playingWords(p) {
  return p === PLAYING_NONE ? 'in the menu' : `playing ${mode_name(p)}`;
}

function drawWho() {
  const w = st.who;
  const who = $('who');
  if (w && w.wallet) who.textContent = `Signed in · ${short(w.wallet)}`;
  else if (w && w.account) who.textContent = 'Signed in · link a wallet for ranked';
  else who.textContent = info.signin ? 'Guest' : 'Guest · ranked is off on this server';
  $('signin').hidden = !signin.ready || signedIn();
  $('face').textContent = (myName()[0] || '?').toUpperCase();
  drawModes();
}

function drawModes() {
  const n = members().length || 1;
  const party = n > 1;
  const wallets = members().length ? members().every((m) => m.wallet) : !!st.who?.wallet;
  // Until the game has drawn, a click would be lost: wait (main's lesson).
  const mine = (!party || leads()) && st.gameUp;
  for (const id of ['play', 'ffa', 'gungame', 'bigteam']) {
    const b = $(id);
    b.disabled = !mine || (id === 'play' && party);
    b.title = !st.gameUp ? 'The game is loading' : !mine ? `${leader()?.name ?? 'The leader'} picks the game`
      : id === 'play' && party ? 'The bot is a game for one' : '';
  }
  const signedWallet = !!st.who?.wallet;
  $('duel').disabled = !mine || party || !signedWallet;
  $('doubles').disabled = !mine || n > 2 || !signedWallet || !wallets;
  $('cup').disabled = !mine || !signedWallet || !wallets;
}

function drawParty() {
  const ms = members();
  const n = ms.length || 1;
  $('party-size').textContent = `${n} of ${MAX_PARTY}`;
  $('party-count').textContent = `${n} of ${MAX_PARTY}`;
  $('party-code').textContent = st.party ? st.party.text : '';
  const lead = leads();
  const ul = $('roster');
  if (!st.party) {
    ul.innerHTML = `<li class="empty">${st.lobby ? 'Starting your party…' : 'Party off: reconnecting…'}</li>`;
  } else {
    ul.innerHTML = ms.map((m) => {
      const mine = m.tag === st.party.me;
      const key = voiceKey(m);
      const muted = !mine && voice.muted(key);
      const tools = lead && !mine
        ? `<button data-act="lead" data-tag="${m.tag}" title="Make them the leader">Lead</button><button data-act="kick" data-tag="${m.tag}" title="Remove them">✕</button>`
        : !mine && m.player && signedIn() && !isFriend(m.player)
          ? `<button data-act="ask" data-player="${esc(m.player)}" title="Ask them to be friends on Elo Pros">+ Friend</button>` : '';
      return `<li class="${mine ? 'me' : ''} ${m.away ? 'away' : ''}" data-tag="${m.tag}">
        <button class="spk ${muted ? 'mute' : ''}" data-act="${mine ? 'mic' : 'mute'}" data-key="${esc(key)}" title="${mine ? 'Your mic' : muted ? 'Unmute' : 'Mute'}">${SPEAKER}</button>
        <span class="nm">${esc(m.name)}${mine ? ' <span style="color:var(--faint)">(you)</span>' : ''}</span>
        ${m.tag === st.party.leader ? '<span class="lead" title="Leads the party">★</span>' : ''}
        <span class="st">${m.away ? 'on the way' : playingWords(m.playing)}</span>${tools}</li>`;
    }).join('') + (n < MAX_PARTY ? '<li class="empty">Invite: press X, or send the link</li>' : '');
  }
  const l = leader();
  $('lead-line').textContent = !st.party ? '' : n === 1 ? 'Just you. Send your link and play together.'
    : lead ? 'You lead: pick a game and the party plays it.' : `${l ? l.name : 'The leader'} picks the game.`;
  const priv = $('privacy');
  if (st.party) priv.value = String(st.party.privacy);
  priv.disabled = !lead || !st.party;
  $('leave').hidden = n <= 1;
  $('voice-line').textContent = voice.asleep && n > 1
    ? 'Click anywhere or press a key to hear your party.'
    : 'Your party hears you, in the menu and in a match. Nobody else does.';
  drawModes();
  drawTalk();
}

function isFriend(player) {
  return st.friends.some((f) => f.player === player);
}

function drawFriends() {
  const ul = $('friends');
  const line = $('friends-line');
  const here = st.friends.filter((f) => f.at >= 2).length;
  $('friend-count').textContent = st.friends.length ? `${here} here` : '';
  if (!signedIn()) {
    line.textContent = 'Sign in with Elo Pros to see your friends here.';
    ul.innerHTML = '';
    return;
  }
  if (!st.reachable) {
    line.textContent = "Elo Pros didn't answer just now. Trying again.";
  } else if (!st.friends.length) {
    line.innerHTML = 'No friends yet. Add them on your <a href="https://elopros.com/profile.html" target="_blank" rel="noopener" style="color:var(--lime)">Elo Pros profile</a>.';
  } else {
    line.textContent = `${st.friends.length} friends · ${here} here now`;
  }
  const inMine = new Set(members().map((m) => m.player).filter(Boolean));
  // Here first (in the menu, then in a match), then on Elo Pros, then away.
  const order = (f) => [0, 1, 3, 2][f.at] ?? 0;
  ul.innerHTML = [...st.friends].sort((a, b) => order(b) - order(a)).map((f) => {
    const dot = ['', 'elsewhere', 'here', 'match'][f.at];
    const where = inMine.has(f.player) ? 'In your party' : f.at === 3 ? `Playing ${mode_name(f.playing)}`
      : f.at === 2 ? (f.size > 1 ? `In a party of ${f.size}` : 'In the menu')
      : f.at === 1 ? 'Online on Elo Pros' : 'Offline';
    const btns = [
      f.party ? `<button data-act="join" data-code="${f.party}">Join</button>` : '',
      f.at >= 2 && !inMine.has(f.player) ? `<button data-act="invite" data-player="${esc(f.player)}">Invite</button>` : '',
    ].join('');
    return `<li><span class="dot ${dot}"></span><span class="nm">${esc(f.name)}</span><span class="st">${where}</span>${btns}</li>`;
  }).join('');
}

function drawTalk() {
  const talking = new Set(voice.talking());
  for (const li of $('roster').querySelectorAll('li[data-tag]')) {
    const tag = Number(li.dataset.tag);
    const mine = st.party && tag === st.party.me;
    li.querySelector('.spk')?.classList.toggle('talk', mine ? voice.onAir : talking.has(tag));
  }
  // Over the match: who is talking.
  const over = $('talking');
  if (menu.style.display === 'none') {
    const rows = members().filter((m) => (m.tag === st.party?.me ? voice.onAir : talking.has(m.tag)));
    over.innerHTML = rows.map((m) => `<div>${SPEAKER}${esc(m.tag === st.party.me ? 'You' : m.name)}</div>`).join('');
  } else {
    over.innerHTML = '';
  }
  drawMic();
}
setInterval(drawTalk, 150);

function drawAll() {
  drawWho();
  drawParty();
  drawFriends();
}

// ── toasts ────────────────────────────────────────────────────────────────
function toast(html, { actions = [], ttl = 6000, bad = false } = {}) {
  const t = document.createElement('div');
  t.className = 'toast';
  if (bad) t.style.borderColor = 'var(--bad)';
  t.innerHTML = html + (actions.length ? `<div class="row">${actions.map((a, i) =>
    `<button data-i="${i}"><span class="face-btn ${a.key}">${a.key.toUpperCase()}</span>${esc(a.label)}</button>`).join('')}</div>` : '');
  const close = () => { t.remove(); if (toastActions && toastActions.el === t) toastActions = null; };
  t.addEventListener('click', (e) => {
    const b = e.target.closest('button[data-i]');
    if (!b) return;
    actions[Number(b.dataset.i)].fn?.();
    sfx.select();
    close();
  });
  $('toasts').appendChild(t);
  if (actions.length) toastActions = { el: t, actions, close };
  if (ttl) setTimeout(close, ttl);
}

// ── panes, blades, and moving around ─────────────────────────────────────
const blades = [...document.querySelectorAll('.blade')];
function openPane(name, focus) {
  booted();
  for (const b of blades) b.classList.toggle('on', b.dataset.pane === name);
  for (const p of document.querySelectorAll('section.pane')) p.hidden = p.dataset.pane !== name;
  if (name === 'friends' && st.lobby && signedIn()) st.lobby.act(ACT.FRIENDS, 0);
  if (focus) first(document.querySelector(`section.pane[data-pane="${name}"]`))?.focus();
}
for (const b of blades) {
  b.onclick = () => { sfx.select(); openPane(b.dataset.pane, true); };
  b.onpointerenter = () => sfx.move();
}
const focusables = (zone) => [...zone.querySelectorAll('button:not(:disabled):not([hidden]), select:not(:disabled), input:not(:disabled)')]
  .filter((el) => el.offsetParent !== null);
const first = (zone) => zone && focusables(zone)[0];
const zones = () => [$('blades'), document.querySelector('section.pane:not([hidden])'), document.querySelector('aside.pane')].filter(Boolean);
function move(dy, dx) {
  const zs = zones();
  const at = document.activeElement;
  let zi = zs.findIndex((z) => z.contains(at));
  if (zi < 0) { zi = 0; first(zs[0])?.focus(); sfx.move(); return; }
  if (dx) {
    const next = zs[Math.max(0, Math.min(zs.length - 1, zi + dx))];
    const el = first(next);
    if (el && next !== zs[zi]) { el.focus(); sfx.move(); }
    return;
  }
  const list = focusables(zs[zi]);
  const i = list.indexOf(at);
  const el = list[Math.max(0, Math.min(list.length - 1, i + dy))];
  if (el && el !== at) { el.focus(); sfx.move(); }
}
const back = () => {
  const blade = blades.find((b) => b.classList.contains('on'));
  if (blade && !$('blades').contains(document.activeElement)) { blade.focus(); sfx.back(); }
};
const press = (key) => {
  if (toastActions) {
    const a = toastActions.actions.find((x) => x.key === key);
    if (a) { a.fn?.(); sfx.select(); toastActions.close(); return; }
  }
  if (key === 'a') document.activeElement?.click?.();
  if (key === 'b') back();
  if (key === 'x') inviteHint();
  if (key === 'y') { sfx.select(); openPane('friends', true); }
};
for (const h of document.querySelectorAll('footer .hint[data-key]')) h.onclick = () => press(h.dataset.key);
$('sound').onclick = () => { $('sound-word').textContent = sfx.toggle() ? 'Sound on' : 'Sound off'; };
$('sound-word').textContent = sfx.on ? 'Sound on' : 'Sound off';
addEventListener('keydown', (e) => {
  if (menu.style.display === 'none') return;
  const typing = e.target && (e.target.tagName === 'INPUT' && !['checkbox', 'range'].includes(e.target.type));
  const k = e.key;
  if (k === 'ArrowUp' || (!typing && k === 'w')) { move(-1, 0); e.preventDefault(); }
  else if (k === 'ArrowDown' || (!typing && k === 's')) { move(1, 0); e.preventDefault(); }
  else if (!typing && (k === 'ArrowLeft' || k === 'a')) { move(0, -1); e.preventDefault(); }
  else if (!typing && (k === 'ArrowRight' || k === 'd')) { move(0, 1); e.preventDefault(); }
  else if (k === 'Escape' || (!typing && k === 'Backspace')) { if (toastActions) press('b'); else back(); }
  else if (!typing && k === 'x') press('x');
  else if (!typing && k === 'y') press('y');
});
// A pad: the stick or the d-pad moves, A selects, B backs out, X invites,
// Y opens friends. Read only while the menu is up.
{
  const held = new Map();
  const step = (id, on, fn, now) => {
    const at = held.get(id);
    if (!on) { held.delete(id); return; }
    if (at === undefined) { held.set(id, now + 380); fn(); } else if (now >= at) { held.set(id, now + 110); fn(); }
  };
  const tick = (now) => {
    if (menu.style.display !== 'none') {
      for (const p of navigator.getGamepads ? navigator.getGamepads() : []) {
        if (!p) continue;
        const b = (i) => !!p.buttons[i]?.pressed;
        const ax = p.axes[0] ?? 0, ay = p.axes[1] ?? 0;
        step(`${p.index}u`, b(12) || ay < -0.6, () => move(-1, 0), now);
        step(`${p.index}d`, b(13) || ay > 0.6, () => move(1, 0), now);
        step(`${p.index}l`, b(14) || ax < -0.6, () => move(0, -1), now);
        step(`${p.index}r`, b(15) || ax > 0.6, () => move(0, 1), now);
        for (const [i, k] of [[0, 'a'], [1, 'b'], [2, 'x'], [3, 'y']]) {
          const was = held.get(`${p.index}b${i}`);
          if (b(i) && !was) press(k);
          if (b(i)) held.set(`${p.index}b${i}`, 1); else held.delete(`${p.index}b${i}`);
        }
      }
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

// ── the party's buttons ─────────────────────────────────────────────────
const link = () => {
  const u = new URL(location.href);
  u.search = '';
  if (st.party) u.searchParams.set('party', st.party.text);
  return u.toString();
};
async function copyLink() {
  if (!st.party) return;
  try {
    await navigator.clipboard.writeText(link());
    toast(`Copied your party link. Send it to whoever you want to play with. Code <b>${esc(st.party.text)}</b>.`);
  } catch {
    toast(`Your party's link: <b>${esc(link())}</b>`, { ttl: 15000 });
  }
}
function inviteHint() {
  sfx.select();
  copyLink();
}
$('copy-link').onclick = () => { sfx.select(); copyLink(); };
$('privacy').onchange = (e) => { st.lobby?.act(ACT.PRIVACY, Number(e.target.value)); sfx.select(); };
$('leave').onclick = () => { st.lobby?.act(ACT.LEAVE, 0); sfx.back(); };
$('join').onclick = () => {
  const code = party_code($('join-code').value);
  if (!code) { sfx.bad(); toast('A party code is six letters and numbers.', { bad: true }); return; }
  st.lobby?.act(ACT.JOIN, code);
  sfx.select();
};
$('roster').addEventListener('click', (e) => {
  const b = e.target.closest('button[data-act]');
  if (!b) return;
  const a = b.dataset.act;
  if (a === 'kick') st.lobby?.act(ACT.KICK, Number(b.dataset.tag));
  else if (a === 'lead') st.lobby?.act(ACT.LEAD, Number(b.dataset.tag));
  else if (a === 'mute') { voice.toggleMute(b.dataset.key); drawParty(); }
  else if (a === 'mic') { micBtn.click(); return; }
  else if (a === 'ask') askFriend(b.dataset.player);
  sfx.select();
});
$('friends').addEventListener('click', (e) => {
  const b = e.target.closest('button[data-act]');
  if (!b) return;
  if (b.dataset.act === 'join') st.lobby?.act(ACT.JOIN, Number(b.dataset.code));
  if (b.dataset.act === 'invite') {
    st.lobby?.invite(b.dataset.player);
    toast('Invited. They can join from their menu.');
  }
  sfx.select();
});
/** A friend request on Elo Pros, with the site's own sign-in (this page is
 * on elopros.com); elsewhere, their profile. */
async function askFriend(player) {
  let token = null;
  try { token = localStorage.getItem('elo.account.token'); } catch { /* none */ }
  if (token && location.hostname.endsWith('elopros.com')) {
    try {
      const r = await fetch('/api/friends/me', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: `Elo-Account ${token}` },
        body: JSON.stringify({ op: 'ask', args: { to: player } }),
      });
      toast(r.ok ? 'Friend request sent.' : 'Elo Pros said no to that request.', { bad: !r.ok });
      return;
    } catch { /* their profile below */ }
  }
  open(`https://elopros.com/profile.html?wallet=${encodeURIComponent(player)}`, '_blank', 'noopener');
}

// ── playing ───────────────────────────────────────────────────────────────
for (const id of Object.keys(MODES)) {
  $(id).addEventListener('click', () => {
    sfx.select();
    choose(MODES[id]);
  });
}
/** A mode picked: alone, it starts now; in a party, the leader starts it
 * for everyone (the server says go to each). */
function choose(mode) {
  const n = members().length;
  const priv = $('private').checked;
  if (st.lobby && st.party && (n > 1 || priv)) {
    if (!leads()) { toast(`${esc(leader()?.name ?? 'The leader')} picks the game.`); return; }
    st.lobby.act(ACT.START, mode | (priv ? START_PRIVATE : 0));
    say('Starting for the party…');
    return;
  }
  startMatch(mode, false, false);
}

/** Into a match: the game asks the server (`knock`), the menu says what
 * the stream says while it waits, and steps aside once the match is on. */
function startMatch(mode, priv, withParty) {
  if (st.playing) return;
  if (!st.gameUp) { toast('The game is still loading.'); return; }
  const code = withParty ? (st.party?.code || 0) : 0;
  st.playing = true;
  store('oneshot.name', myName());
  store('oneshot.skin', skin.value);
  for (const b of document.querySelectorAll('.modes')) b.hidden = true;
  $('news').innerHTML = '';
  cancel.hidden = false;
  cancel.textContent = 'Cancel';
  cancel.onclick = () => { glue.cancel(); sfx.back(); };
  say('Connecting…');
  openPane(RANKED.has(mode) ? 'ranked' : 'play');
  document.querySelector(`section.pane[data-pane="${RANKED.has(mode) ? 'ranked' : 'play'}"]`).appendChild(status.parentElement);
  st.lobby?.act(ACT.PLAYING, mode);
  // Our ticket says who we are (signed in, or a guest of the server's); a
  // fresh pass the first time, before the lobby has handed one.
  const pass = st.ticket || signin.pass();
  glue.knock(myName(), pass, mode, Number(skin.value) || 0, code >>> 0);
  const t0 = performance.now();
  let asked = false;
  let inMatch = false;
  const back = (line, bad) => {
    st.playing = false;
    st.lobby?.act(ACT.PLAYING, PLAYING_NONE);
    for (const b of document.querySelectorAll('.modes')) b.hidden = false;
    cancel.hidden = true;
    menu.style.display = 'flex';
    if (document.pointerLockElement) document.exitPointerLock();
    say(line, bad);
    sfx.humOn();
    drawAll();
  };
  const watch = () => {
    const s = window.__oneshot || {};
    if (!inMatch) {
      asked ||= s.knocking || s.in_match;
      if (s.in_match) {
        inMatch = true;
        menu.style.display = 'none';
        sfx.humOff();
        if (!SKIP_MENU) $('game').focus();
      } else if (asked && !s.knocking) {
        // Turned away, or cancelled.
        back(s.front_status || 'Cancelled', !!s.front_bad);
        return;
      } else if (!asked && performance.now() - t0 > 5000) {
        back('The game did not answer. Reload the page.', true);
        return;
      } else if (s.front_status) {
        say(s.front_status, !!s.front_bad);
      }
    } else if (!s.in_match) {
      // The match over, left, or the line dropped: back to the menu, the
      // party still here.
      back(s.front_status || 'Match over');
      if (st.who && st.who.account && before) showNews(st.who, before);
      return;
    }
    setTimeout(watch, 200);
  };
  watch();
}

// ── Elo Pros' record: ratings, achievements and boards, skins ────────────
// Read from elopros.com (the page's own origin once published). A read that
// fails shows nothing; it never blocks play.
const ELO = /(^|\.)elopros\.com$/.test(location.hostname) ? '' : 'https://elopros.com';
const GAME = 'oneshot';
const getJSON = async (path) => {
  try {
    const r = await fetch(`${ELO}${path}`, { cache: 'no-store' });
    return r.ok ? await r.json() : null;
  } catch { return null; }
};
// Their record when they signed in (or after the last match).
let before = null;
let recordOf = '';
/** One read of everything: {stats, ratings, items}. `fresh` asks past any
 * cache (after a match, the record just moved). */
async function record(who, fresh) {
  const id = who.wallet || who.account;
  const t = fresh ? `?t=${Date.now()}` : '';
  const [stats, ratings, rh, sol] = await Promise.all([
    getJSON(`/api/gamestats/${GAME}/of/${id}${t}`),
    who.wallet ? getJSON(`/api/ratings/${GAME}/of/${who.wallet}${t}`) : null,
    who.wallet ? getJSON(`/api/items/of/${who.wallet}?game=${GAME}&title=${GAME}`) : null,
    // the Solana wallet linked to it, where the free Pill goes
    who.wallet ? getJSON(`/api/items/sol/of/${who.wallet}`) : null,
  ]);
  const items = rh || sol ? { items: [...((rh && rh.items) || []), ...((sol && sol.items) || [])] } : null;
  return { stats, ratings, items };
}
/** The skins an items answer names (the server's `owned_skins`). */
function ownedSkins(items) {
  const names = skin_names();
  const out = new Set(['standard']);
  for (const i of (items && items.items) || []) {
    if (i.title && String(i.title).toLowerCase() !== GAME) continue;
    const n = String(i.skin || i.item || i.catalog_name || i.name || '').trim().toLowerCase();
    const first = n.split(/[^a-z0-9]/)[0];
    for (const c of [n, first]) if (names.includes(c)) out.add(c);
  }
  return out;
}
async function showRecord(who) {
  const rec = await record(who, false);
  before = rec;
  const bits = [];
  const lad = (rec.ratings && rec.ratings.ladders) || {};
  for (const [id, label] of [['duel', 'Duel'], ['doubles', 'Doubles']]) {
    const l = lad[id];
    if (l && l.rated && l.rating != null) bits.push(`${label} ${Math.round(l.rating)}${l.placed ? '' : ' (placing)'}`);
  }
  const tot = rec.stats && rec.stats.totals;
  if (tot) bits.push(`${tot.earned}/${tot.total} achievements`);
  const prof = who.wallet ? ` · <a href="https://elopros.com/profile.html?who=${esc(who.wallet)}" target="_blank" rel="noopener">profile</a>` : '';
  $('record').innerHTML = bits.length ? esc(bits.join(' · ')) + prof : 'No matches on record yet.' + prof;
  // Mark the skins this wallet holds; the server still checks.
  const owned = ownedSkins(rec.items);
  skin_names().forEach((n, i) => {
    const o = skin.options[i];
    if (o) o.textContent = n[0].toUpperCase() + n.slice(1) + (rec.items && !owned.has(n) ? ' · not yours yet' : '');
  });
}
/** After a match: what it unlocked and moved. The server reports as the
 * match ends, so ask a few times until the record moves. */
async function showNews(who, was) {
  let now = null;
  for (let i = 0; i < 5; i++) {
    await new Promise((r) => setTimeout(r, 2000));
    now = await record(who, true);
    if (now.stats && (!was.stats || now.stats.last !== was.stats.last)) break;
  }
  if (!now || !now.stats) return;
  const had = new Set(((was.stats && was.stats.achievements) || []).filter((a) => a.earned).map((a) => a.id));
  const got = (now.stats.achievements || []).filter((a) => a.earned && !had.has(a.id));
  const lines = got.map((a) => `<b>Unlocked</b> ${esc(a.name)}${a.rare ? ' · rare' : ''}`);
  const best = new Map(((was.stats && was.stats.boards) || []).map((b) => [b.id, b.score]));
  for (const b of now.stats.boards || []) {
    if (b.score != null && b.score !== best.get(b.id)) {
      const v = b.format === 'time_ms' ? `${(b.score / 1000).toFixed(1)} s` : b.score;
      lines.push(`<b>New best</b> ${esc(b.name)}: ${esc(v)}${b.rank ? ` · #${b.rank} of ${b.of}` : ''}`);
    }
  }
  const lad = (now.ratings && now.ratings.ladders) || {};
  const old = (was.ratings && was.ratings.ladders) || {};
  for (const [id, label] of [['duel', 'Duel'], ['doubles', 'Doubles'], ['cup', 'Cup']]) {
    const a = old[id], b = lad[id];
    if (b && b.rated && b.rating != null && (!a || a.rating !== b.rating)) {
      const d = a && a.rating != null ? Math.round(b.rating - a.rating) : null;
      lines.push(`<b>${label}</b> ${Math.round(b.rating)}${d ? ` (${d > 0 ? '+' : ''}${d})` : ''}`);
    }
  }
  if (lines.length) {
    $('news').innerHTML = lines.join('<br />');
    sfx.chime();
  }
  before = now;
}

/** Sign in with Elo Pros: a pass for this game, from Elo Pros' popup. The
 * server hands out the nonce the pass must carry; we fetch one ahead so the
 * popup opens on the click itself (a popup opened later is blocked). The
 * lobby takes the pass and hands back a ticket, which is who we are from
 * then on, on this tab, until it runs out. */
async function setupSignIn(info, button) {
  let pass = '';
  let nonce = '';
  let ready = false;
  const fresh = async () => {
    try { nonce = (await (await fetch(`${base}nonce`)).json()).nonce; } catch { nonce = ''; }
  };
  const sdk = () => window.EloSignIn;
  // The SDK script is async: give it a moment.
  for (let i = 0; i < 20 && !sdk(); i++) await new Promise((r) => setTimeout(r, 100));
  if (info.signin && sdk()) {
    ready = true;
    await fresh();
    button.onclick = async () => {
      try {
        const res = await sdk().popup({ game: info.game, nonce });
        pass = res.pass;
        if (st.lobby) st.lobby.sign_in(pass);
        else freshPass = pass;
        sfx.chime();
      } catch (e) {
        if (!(e?.code === 'cancelled' || e?.code === 'closed')) toast(`Sign-in failed: ${esc(e?.message ?? e)}`, { bad: true });
      }
      await fresh();
    };
  }
  return { pass: () => pass, get ready() { return ready; } };
}

nameInput.addEventListener('change', () => {
  store('oneshot.name', myName());
  drawWho();
  // The party sees the new name once the lobby says hello again.
  st.lobby?.close();
});

// For the browser gates (ci/party.mjs): the lobby as this page sees it.
Object.defineProperty(window, '__lobby', {
  get: () => ({
    connected: !!st.lobby, party: st.party, friends: st.friends, ticket: !!st.ticket,
    heard: voice.heard, sent: voice.sent, mic: !!voice.mic,
  }),
});

drawAll();
const launchOpts = ['autoplay', 'lite', 'capsules', 'fxdemo', 'menu', 'settings', 'graphics'].filter(flag)
  .concat(params.get('fxstage') ? ['fxstage' + Number(params.get('fxstage'))] : []).concat(gfxOpts);
if (SKIP_MENU) {
  // Straight in (the browser gate's autoplay, the effects demo): the game's
  // own way, no menu.
  menu.style.display = 'none';
  started = performance.now();
  glue.launch(info.addr, info.hash ?? '', launchOpts.join(','), myName(), Number(params.get('mode') || 0), null, 'page', follow);
} else {
  sfx.humOn();
  connectLobby();
  first($('blades'))?.focus({ preventScroll: true });
  // The game behind the menu: its live backdrop, and the door the menu
  // knocks at. A click on a mode counts once it has drawn.
  started = performance.now();
  const up = () => {
    if ((window.__oneshot?.frames ?? 0) > 3) { st.gameUp = true; drawModes(); return; }
    requestAnimationFrame(up);
  };
  requestAnimationFrame(up);
  try {
    glue.launch(info.addr, info.hash ?? '', launchOpts.concat('page').join(','), myName(), 0, null, 'page', follow);
  } catch (e) {
    // winit's web loop may unwind out of run(); the game goes on.
    if (!/control flow|isn't an error/i.test(String(e?.message ?? e))) throw e;
  }
}

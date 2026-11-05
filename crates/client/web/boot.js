// The Bevy client in the browser. The page finds the server, knows how to
// sign in, and does the party's voice (voice.js: the microphone and the
// speakers, which a browser gives a page, not a wasm module); the title, the
// party and friends, the queue's wait and everything after are the game's
// (crates/client/src/front.rs, party.rs).
import { Voice, voiceWorks } from './voice.js';

// The game is two modules (ci/client.sh), picked below before either
// downloads: `oneshot_client_gpu` draws through WebGPU, the desktop's
// frame, and `oneshot_client` through WebGL2.
// Raw mouse: the game's lock (winit asks with no options) asks the
// browser for unadjusted movement, so the OS's acceleration doesn't
// bend aim. Not on Linux, where Chrome refuses and a second request
// can lose the click; a refusal anywhere is remembered and asked
// plainly at once (Gates' app.js).
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
if (flag('lite')) document.getElementById('wrap').classList.add('lite');
// The graphics ladder's words for the game (look/quality.rs,
// look/potato.rs): `?potato` is POTATO from now on, `?quality=high`
// (potato, low, medium, high) that rung for this run, as a picture of
// it wants; `?lite=1` is POTATO for this run.
const gfxOpts = [
  ...(params.has('potato') && params.get('potato') !== '0' ? ['potato'] : []),
  ...(/^(potato|low|medium|high)$/.test(params.get('quality') ?? '') ? ['quality=' + params.get('quality')] : []),
];
/* FULLSCREEN on the settings screen is the page's (crates/client/src/
   display.rs). Esc is the game's menu key and a browser also ends a
   page's fullscreen on it: while fullscreen, Keyboard Lock (Chromium)
   hands Esc to the game, and holding it still leaves, as the browser
   says. Elsewhere the row follows the browser out. */
document.addEventListener('fullscreenchange', () => {
  const kb = navigator.keyboard;
  if (!kb || !kb.lock) return;
  if (document.fullscreenElement) kb.lock(['Escape']).catch(() => {});
  else kb.unlock();
});
const boot = document.getElementById('boot');
const say = (line, bad) => { document.getElementById('say').textContent = line; boot.classList.toggle('failed', !!bad); };
// The splash goes once the game has drawn a few frames.
const shown = () => {
  if ((window.__oneshot?.frames ?? 0) > 3) { boot.classList.add('gone'); return; }
  requestAnimationFrame(shown);
};

/* ── which module (after Gates' web/app.js, pickModule) ──────────────
   Bevy picks its GPU API when it is compiled and has no fallback at
   run time, so the page picks, before the download. A browser gets
   the WebGPU module when it hands back a hardware adapter with
   filterable 32-bit floats (Bevy binds the atmosphere's Rgba32Float
   tables as filterable and never checks, so without them the frame
   is lost, not degraded). A software adapter (isFallbackAdapter) gets
   WebGL2 too: the desktop's frame on a CPU is slower than the cheap
   one. What the device can bind past that, Rust reads off the device
   itself (look/gpu.rs). `?gfx=webgl2` or `?gfx=webgpu` forces one. A
   WebGPU module that failed in this browser is remembered against
   that build (build.json's hash), so the next visit goes straight to
   WebGL2 and a new build tries again; `?gfx=webgpu` forgets it. No
   build.json (tools/fx.sh's page) is the WebGL2 module alone. */
const MODULES = {
  webgl2: { js: './oneshot_client.js', wasm: 'oneshot_client_bg.wasm', name: 'WebGL2' },
  webgpu: { js: './oneshot_client_gpu.js', wasm: 'oneshot_client_gpu_bg.wasm', name: 'WebGPU' },
};
const GPU_FAILED = 'oneshot.webgpu.failed';
const SWITCHED = 'oneshot.webgpu.switched';
const built = fetch('build.json', { cache: 'no-cache' }).then((r) => (r.ok ? r.json() : null)).catch(() => null);
// Started at load, beside build.json, rather than after it.
const gpuProbe = (async () => {
  if (!navigator.gpu) return { ok: false, why: 'no WebGPU in this browser' };
  try {
    // A blocklisted GPU answers null; a wedged one may never answer.
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
const key = () => String(mods?.webgpu?.sha256 ?? mods?.webgpu?.wasm_bytes ?? '');
async function pickModule() {
  const has = (k) => (mods ? !!mods[k] : k === 'webgl2');
  const forced = params.get('gfx');
  if (forced === 'webgpu') { try { localStorage.removeItem(GPU_FAILED); } catch { /* private window */ } }
  if ((forced === 'webgl2' || forced === 'webgpu') && has(forced)) return { gfx: forced, why: 'forced by ?gfx' };
  if (!has('webgpu')) return { gfx: 'webgl2', why: 'this build has no WebGPU module' };
  if (!has('webgl2')) return { gfx: 'webgpu', why: 'this build has no WebGL2 module' };
  try {
    if (localStorage.getItem(GPU_FAILED) === key()) return { gfx: 'webgl2', why: 'WebGPU failed here before' };
  } catch { /* private window */ }
  const probe = await gpuProbe;
  return { gfx: probe.ok ? 'webgpu' : 'webgl2', why: probe.why };
}
// The splash line names the renderer (and why, on hover); after a
// fall back, what failed.
const showGfx = (gfx, why, note) => {
  const g = document.getElementById('gfx');
  g.textContent = `· ${MODULES[gfx].name.toUpperCase()}${note ? ` (${note.toUpperCase()})` : ''}`;
  g.title = why;
};
async function load(gfx) {
  const m = MODULES[gfx];
  // The glue and its module are a pair (wasm-bindgen hashes the
  // import names), so both are asked for by the pair's hash: a cached
  // glue from the last build can't meet this build's module.
  const sha = mods?.[gfx]?.sha256;
  const v = sha ? `?v=${sha.slice(0, 12)}` : '';
  // The module's fetch goes out before the glue's import: it is the
  // long one.
  const res = fetch(new URL(m.wasm + v, location.href));
  res.catch(() => { /* awaited by the glue */ });
  const glue = await import(m.js + v);
  await glue.default({ module_or_path: res });
  window.__oneshotGfx = gfx;
  return glue;
}
/* Back into the WebGL2 module, once: the failure is written against
   this build first, and the WebGL2 module has no hook that could send
   it back, so this can't loop. A canvas that has held a WebGPU
   context never takes a WebGL2 one, and Bevy's run() doesn't return,
   so the way there is a reload. */
let started = 0;
function gpuFallBack(why) {
  let kept = false;
  try {
    localStorage.setItem(GPU_FAILED, key());
    kept = localStorage.getItem(GPU_FAILED) === key();
    sessionStorage.setItem(SWITCHED, why);
  } catch { /* storage blocked */ }
  // Where the flag won't hold (storage refused, or ?gfx=webgpu), the
  // address carries the switch, or the reload lands on WebGPU again.
  const u = new URL(location.href);
  if (!kept || params.get('gfx') === 'webgpu') u.searchParams.set('gfx', 'webgl2');
  console.warn(`oneshot: ${why}: switching to WebGL2`);
  location.replace(u);
}
/* The WebGPU module's device hooks (look/gpu.rs, `watch`). WebGPU
   refuses a pipeline quietly and that frame isn't drawn. A lost
   device draws nothing again: back to WebGL2. A refusal before the
   game has drawn is the same; after, a line over the canvas offers
   WebGL2, and it is remembered. */
window.__oneshotGpuError = (msg, kind) => {
  if (window.__oneshotGfx !== 'webgpu') return;
  if (kind === 'lost') { gpuFallBack('the graphics device was lost under WebGPU'); return; }
  if (!boot.classList.contains('gone')) { gpuFallBack('WebGPU refused the frame'); return; }
  try { localStorage.setItem(GPU_FAILED, key()); } catch { /* private window */ }
  if (document.getElementById('gpubar')) return;
  const bar = document.createElement('div');
  bar.id = 'gpubar';
  bar.setAttribute('role', 'alert');
  const u = new URL(location.href);
  u.searchParams.set('gfx', 'webgl2');
  const a = document.createElement('a');
  a.href = u.toString();
  a.textContent = 'RELOAD WITH WEBGL2';
  bar.append('THE GRAPHICS CARD REFUSED PART OF THE FRAME UNDER WEBGPU (ESC TO CLICK)', a);
  bar.title = String(msg).slice(0, 400);
  document.body.append(bar);
};
/* Every panic (look/gpu.rs, `hook_panics`). A WebGPU module whose
   renderer never started (Bevy's "Unable to find a GPU!", a refused
   device or surface) panics before the hooks above exist; so does
   wgpu's WebGPU backend when the browser refuses a call it unwraps.
   Either way WebGL2 is the module that may live. Any other panic
   stops the game: said on the splash if it is still up. */
window.__oneshotPanic = (msg) => {
  const m = String(msg || '');
  const gpu = window.__oneshotGfx === 'webgpu';
  const renderer = /bevy_render|Unable to find a GPU|wgpu surface|request_device|adapter/.test(m);
  if (gpu && renderer && started && performance.now() - started < 30000) { gpuFallBack('WebGPU did not start here'); return; }
  if (gpu && /wgpu-[\d.]+\/src\/backend\/webgpu|GPUDevice|GPUQueue|GPUCanvasContext/.test(m)) {
    gpuFallBack('the graphics card refused WebGPU');
    return;
  }
  if (!boot.classList.contains('gone')) say('The game stopped. Reload to try again', true);
};

// Where the game's own endpoints are: beside the page in dev, the game
// box when the page is published to elopros.com (ci/publish_web.sh
// writes origin.json).
let base = '';
try {
  mods = (await built)?.modules ?? null;
  let { gfx, why } = await pickModule();
  let switched = null;
  try {
    switched = sessionStorage.getItem(SWITCHED);
    sessionStorage.removeItem(SWITCHED);
  } catch { /* private window */ }
  showGfx(gfx, why, gfx === 'webgl2' ? switched : null);
  let glue;
  try {
    glue = await load(gfx);
  } catch (err) {
    // A WebGPU module that won't compile or instantiate here still
    // leaves the WebGL2 one.
    if (gfx !== 'webgpu' || !(mods?.webgl2)) throw err;
    console.warn('oneshot: the WebGPU module failed to load, loading WebGL2', err);
    gfx = 'webgl2';
    why = 'the WebGPU module did not load';
    showGfx(gfx, why, why);
    glue = await load(gfx);
  }
  const { launch, pose } = glue;
  if (params.has('pose')) {
    // No server: one fighter for a picture (ci/look.mjs).
    boot.remove();
    started = performance.now();
    pose(location.search, ['lite'].filter(flag).concat(gfxOpts).join(','));
    await new Promise(() => {}); // nothing more for the page to do
  }
  try {
    const o = await fetch('origin.json');
    if (o.ok) base = (await o.json()).server ?? '';
  } catch { /* none: beside the page */ }
  say('Finding the server');
  const info = await (await fetch(`${base}server.json`)).json();
  // A server on its way down: the live one, if it is now another.
  const follow = async () => {
    try {
      const now = await (await fetch(`${base}server.json`)).json();
      return now.addr && now.addr !== info.addr ? [now.addr, now.hash ?? ''] : null;
    } catch { return null; }
  };
  const signin = await setupSignIn(info);
  // A party's link (`?party=CODE`): the game joins it.
  const party = /^[0-9a-zA-Z]{6}$/.test(params.get('party') ?? '') ? [`party=${params.get('party').toUpperCase()}`] : [];
  const opts = ['autoplay', 'lite', 'capsules', 'fxdemo', 'menu', 'settings', 'graphics'].filter(flag)
    .concat(params.get('fxstage') ? ['fxstage' + Number(params.get('fxstage'))] : []).concat(gfxOpts).concat(party).join(',');
  voiceBridge(glue);
  say('Starting');
  requestAnimationFrame(shown);
  started = performance.now();
  launch(info.addr, info.hash ?? '', opts, params.get('name') || '', Number(params.get('mode') || 0),
    signin.fn, signin.off, follow);
} catch (e) {
  say(String(e?.message ?? e), true);
  throw e;
}

/** Sign in with Elo Pros: a pass for this game, from Elo Pros' popup.
 * The server hands out the nonce the pass must carry; one is fetched
 * ahead so the popup opens on the click itself. Resolves to
 * `{pass, wallet}`; ranked needs the wallet. `fn` is null (and `off`
 * says why) when there is no sign-in here. */
async function setupSignIn(info) {
  const sdk = () => window.EloSignIn;
  // The SDK script is async: give it a moment.
  for (let i = 0; i < 20 && !sdk(); i++) await new Promise((r) => setTimeout(r, 100));
  if (!info.signin || !sdk()) {
    return { fn: null, off: info.signin ? 'Sign-in is unavailable right now' : 'Ranked is off on this server' };
  }
  let nonce = '';
  const fresh = async () => {
    try { nonce = (await (await fetch(`${base}nonce`)).json()).nonce; } catch { nonce = ''; }
  };
  await fresh();
  const fn = async () => {
    try {
      const res = await sdk().popup({ game: info.game, nonce });
      const text = atob(res.pass.split('.')[0].replace(/-/g, '+').replace(/_/g, '/'));
      const wallet = /wallet: (0x[0-9a-f]{40})/.exec(text)?.[1] ?? null;
      return { pass: res.pass, wallet };
    } finally {
      await fresh();
    }
  };
  return { fn, off: '' };
}

/** The party's voice, for the game (party.rs): it holds the party's session
 * and hands each member's frame here to be played; the microphone's frames
 * go back to it through `voice_up`. The game asks for the mic with
 * `toggle()`; V held talks (or open mic, past a level gate). */
function voiceBridge(glue) {
  const voice = new Voice((seq, end, bytes) => glue.voice_up(seq, end, bytes), () => {});
  window.__oneshotVoice = {
    works: voiceWorks(),
    hear: (tag, seq, flags, bytes, key) => voice.hear(tag, seq, flags, bytes, key),
    toggle: async () => {
      try {
        if (voice.mic) voice.micOff();
        else await voice.micOn();
      } catch (e) {
        console.warn('oneshot: the microphone did not open', e);
      }
    },
    state: () => (!voiceWorks() ? 'none' : !voice.mic ? 'off' : voice.onAir ? 'talking' : 'on'),
    mode: () => voice.mode,
    setMode: (m) => voice.setMode(m),
    muted: (k) => voice.muted(k),
    toggleMute: (k) => voice.toggleMute(k),
    forget: (tags) => voice.forget(Array.from(tags)),
    asleep: () => voice.asleep,
    heard: () => voice.heard,
    sent: () => voice.sent,
  };
  // The party's link, for COPY PARTY LINK.
  window.__oneshotCopy = (code) => {
    const u = new URL(location.href);
    u.search = '';
    u.searchParams.set('party', code);
    navigator.clipboard?.writeText(u.toString()).catch(() => {});
    return u.toString();
  };
  // A browser starts sound only after a gesture: try now, and on the first
  // one, with the mic back on if it was last time.
  voice.wake();
  const wake = async () => {
    await voice.wake();
    try {
      if (localStorage.getItem('oneshot.mic') === 'on' && !voice.mic && voiceWorks()) await voice.micOn();
    } catch { /* asked again on MIC */ }
  };
  addEventListener('pointerdown', wake, { once: true, capture: true });
  addEventListener('keydown', wake, { once: true, capture: true });
  // The browser test's fake microphone (?mic=open): on at once.
  if (params.get('mic') === 'open') {
    voice.setMode('open');
    voice.wake().then(() => voice.micOn()).catch((e) => console.warn('mic', e));
  }
}

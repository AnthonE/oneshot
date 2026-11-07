// The Bevy client in the browser. The page finds the server, knows how to
// sign in, and does the party's voice (voice.js: the microphone and the
// speakers, which a browser gives a page, not a wasm module); the title, the
// party and friends, the queue's wait and everything after are the game's
// (crates/client/src/front.rs, party.rs).
import { Voice, voiceWorks } from './voice.js';
import { startMix } from './mix.js';

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
const $id = (id) => document.getElementById(id);
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
/* F11 is the page's fullscreen, never the browser's (which the page can't
   see, and which takes no Keyboard Lock): asked for here, inside the key's
   own event, which is all Safari honours; the game's row follows the page
   (display.rs, `PageFull`). Captured first, so it works on the title's
   HTML as well as on the canvas. */
document.addEventListener('keydown', (e) => {
  if (e.code !== 'F11') return;
  e.preventDefault();
  if (e.repeat) return;
  const el = document.documentElement;
  if (document.fullscreenElement) document.exitFullscreen?.().catch(() => {});
  else el.requestFullscreen?.().catch(() => {});
}, true);
const boot = document.getElementById('boot');
const say = (line, bad) => { document.getElementById('say').textContent = line; boot.classList.toggle('failed', !!bad); };
/* The splash's bar: `share` 0..1 of the download, or null for the
   sliding bar (a wait with no end the page can see). */
const bar = (share) => {
  boot.classList.toggle('counting', share !== null);
  boot.querySelector('.bar i').style.width = share === null ? '' : `${(Math.min(1, share) * 100).toFixed(1)}%`;
};
/* Whether the tab was just in the background: a browser isn't told why
   the server closed the connection (client-net's close_why), and a tab
   that drew nothing for a while fell behind and was cut, so the game
   says that instead (crates/client/src/link.rs, `Web::ended_why`). */
{
  let hiddenAt = -1e9;
  let shownAt = -1e9;
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) hiddenAt = performance.now();
    else shownAt = performance.now();
  });
  window.__oneshotAsleep = () =>
    document.hidden || (performance.now() - shownAt < 5000 && shownAt - hiddenAt > 2000);
}
/* A JSON endpoint, or a line a player can read: a 404 or a 502 says so
   rather than surfacing as a JSON SyntaxError. */
async function getJson(url, what) {
  let res;
  try { res = await fetch(url); } catch (e) { throw new Error(`${what} didn't answer (${e?.message ?? e})`); }
  if (!res.ok) throw new Error(`${what} didn't answer (HTTP ${res.status})`);
  try { return await res.json(); } catch { throw new Error(`${what} answered with something that isn't JSON`); }
}
// Where the game's own endpoints are: beside the page in dev, the game
// box when the page is published to elopros.com (ci/publish_web.sh
// writes origin.json). oneshot-server answers origin.json itself (`{}`)
// when its page has none (server/src/http.rs), so one that doesn't
// answer is a published page missing it: an error, never a quiet guess
// at the page's own origin. Asked at once, so a report can go while the
// game downloads, or after it failed to.
const origin = getJson('origin.json', "the page's origin.json")
  .then((o) => o.server ?? '')
  .catch((e) => { throw new Error(`The page can't find the game: ${e.message}`); });
origin.catch(() => { /* said where it's awaited */ });
// The splash stays over the game until it says the title is all there
// (`loaded`: loading.rs, the title's level, models and shaders in), saying
// what it is on and how far; then it goes. After SPLASH_MAX_MS of the game
// drawing it goes anyway (a game that never says so is a bug, not a wall).
const SPLASH_MAX_MS = 150000;
let drawing = 0;
const shown = () => {
  const o = window.__oneshot;
  if ((o?.frames ?? 0) > 3) {
    drawing ||= performance.now();
    if (o.loaded !== false || performance.now() - drawing > SPLASH_MAX_MS) { boot.classList.add('gone'); return; }
    if (o.stage) say(o.stage);
    if (typeof o.progress === 'number') bar(o.progress);
  }
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
// A sign-in nonce older than this is fetched again (the server takes one
// for ten minutes).
const NONCE_FRESH_MS = 4 * 60 * 1000;
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
    const name = [info.vendor, info.architecture, info.description].filter(Boolean).join(' ');
    if (info.isFallbackAdapter || adapter.isFallbackAdapter) return { ok: false, soft: true, name, why: 'a software WebGPU adapter' };
    if (!adapter.features.has('float32-filterable')) return { ok: false, name, why: 'no float32-filterable' };
    return { ok: true, name, why: 'a WebGPU adapter' };
  } catch {
    return { ok: false, why: 'the WebGPU adapter was refused' };
  }
})();
let mods = null;
let buildInfo = null;
// The loaded module's `?v=` (`load`): what comes with the game is asked
// for by it (mix.js's theme, voice.js's worklet), so a cache can't keep an
// old one past a publish.
let pageV = '';
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
  const mine = ++downloads;
  downloading = mine;
  const res = fetch(new URL(m.wasm + v, location.href)).then((r) => counted(r, mods?.[gfx]?.wasm_bytes));
  res.catch(() => { /* awaited by the glue */ });
  let glue;
  try {
    glue = await import(m.js + v);
    await glue.default({ module_or_path: res });
  } finally {
    if (downloading === mine) downloading = 0;
    bar(null);
  }
  window.__oneshotGfx = gfx;
  pageV = v;
  // The game's sound (mix.js), once, before the game starts asking for it;
  // the mixer by its own hash (build.json's `mix`, ci/client.sh).
  const mixSha = buildInfo?.mix?.sha256;
  if (!window.__oneshotMix) startMix(v, mixSha ? `?v=${mixSha.slice(0, 12)}` : '');
  return glue;
}
/* The download, on the splash: "Downloading the game · 13 / 30 MB".
   The bytes are counted off a copy of the response while the compiler
   takes the original, so it still compiles as it streams and the
   browser's code cache still keys to its URL. The whole is the build's
   (build.json): a gzipped page's Content-Length isn't the module's size
   (Gates' app.js `counted`). */
// Which download the splash is on (0: none): each load's own number, so a
// fallen-back one's late chunks never pass for the next's.
let downloading = 0;
let downloads = 0;
function counted(res, whole) {
  const mine = downloading;
  if (!res.ok) throw new Error(`The game didn't download (HTTP ${res.status}). Reload to try again`);
  let want = Number(whole) || 0;
  if (!want && !res.headers.get('content-encoding')) want = Number(res.headers.get('content-length')) || 0;
  const reader = res.clone().body?.getReader();
  if (!reader) return res;
  const mb = (n) => (n / 1048576).toFixed(0);
  // Only while it's the download the splash is on (a late chunk's count
  // never paints over what came after).
  const paint = (got) => {
    if (downloading !== mine) return;
    say(`Downloading the game · ${mb(got)}${want ? ` / ${mb(want)}` : ''} MB`);
    bar(want ? got / want : null);
  };
  paint(0);
  let got = 0;
  let last = 0;
  (async () => {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      got += value.byteLength;
      const now = performance.now();
      if (now - last > 100) { last = now; paint(got); }
    }
    if (downloading !== mine) return;
    bar(null);
    say('Starting the game');
  })().catch(() => { /* the original's failure is the glue's to say */ });
  return res;
}
/* Back into the WebGL2 module, once: the failure is written against
   this build first, and the WebGL2 module has no hook that could send
   it back, so this can't loop. A canvas that has held a WebGPU
   context never takes a WebGL2 one, and Bevy's run() doesn't return,
   so the way there is a reload. */
let started = 0;
// Going back into WebGL2: what the dying module does next isn't a crash.
let switching = false;
function gpuFallBack(why) {
  switching = true;
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
  // Only a panic in the renderer's own set-up (Gates' app.js): a bug
  // in an asset upload panics in bevy_render too, and would panic
  // again under WebGL2.
  const renderer = /bevy_render[^\n]*renderer|Unable to find a GPU|wgpu surface|request_device/.test(m);
  if (gpu && renderer && started && performance.now() - started < 30000) { gpuFallBack('WebGPU did not start here'); return; }
  if (gpu && /wgpu-[\d.]+\/src\/backend\/webgpu|GPUDevice|GPUQueue|GPUCanvasContext/.test(m)) {
    gpuFallBack('the graphics card refused WebGPU');
    return;
  }
  stopped(m);
};

/* ── reports: crashes and a player's, to the server's /bugs ──────────
   (crates/server/src/bugs.rs: a JSON object of at most 16 KB, kept with
   the time it came and nothing about the sender.) Each carries the
   build, the browser and its GPU, and the match: its salt commitment
   (which the server's review files carry), map, tick and net counts
   (window.__oneshot, crates/client/src/lib.rs `expose`). */
let webglName = null;
function gpuName() {
  if (webglName === null) {
    webglName = '';
    try {
      const g = document.createElement('canvas').getContext('webgl2');
      const x = g && g.getExtension('WEBGL_debug_renderer_info');
      webglName = x ? String(g.getParameter(x.UNMASKED_RENDERER_WEBGL)) : g ? 'webgl2' : 'no webgl2';
      g?.getExtension('WEBGL_lose_context')?.loseContext();
    } catch { /* none */ }
  }
  return webglName;
}
async function reportDoc(kind, text) {
  const s = window.__oneshot || {};
  buildInfo ??= await built;
  const gfx = window.__oneshotGfx || '';
  const probe = await Promise.race([gpuProbe, new Promise((r) => setTimeout(() => r({}), 500))]);
  const doc = {
    kind,
    text: String(text).slice(0, 4000),
    build: {
      commit: buildInfo?.commit || '', gfx, sha: (mods?.[gfx]?.sha256 || '').slice(0, 12),
      version: s.version || '', proto: s.proto ?? null,
    },
    where: {
      ua: navigator.userAgent, gpu: gfx === 'webgpu' && probe.name ? probe.name : gpuName(),
      tier: s.tier || '', quality: s.quality || '',
      screen: `${innerWidth}x${innerHeight}@${devicePixelRatio}`, page: location.pathname,
    },
    game: {
      screen: s.screen || '', in_match: !!s.in_match, connected: !!s.connected, match: s.match || '',
      map: s.map ?? null, tick: s.tick ?? null, stats: s.stats ? Array.from(s.stats) : [],
      frames: s.frames ?? 0, ended: !!s.ended, reason: s.reason || '',
    },
  };
  // Under the server's 16 KB whatever the text's letters weigh.
  if (new Blob([JSON.stringify(doc)]).size > 15000) doc.text = doc.text.slice(0, 1000);
  return doc;
}
/* A crash goes as a beacon (it survives the page going); a player's
   report as a request, so the page can say it went. The status, or 0. */
async function sendReport(doc, beacon) {
  let at;
  try { at = await origin; } catch { return 0; }
  const url = `${at}bugs`;
  const body = JSON.stringify(doc);
  if (beacon && navigator.sendBeacon) return navigator.sendBeacon(url, body) ? 204 : 0;
  try {
    return (await fetch(url, { method: 'POST', body, headers: { 'Content-Type': 'text/plain' }, keepalive: true })).status;
  } catch { return 0; }
}
/* The game stopped (a panic, or a trap with no panic behind it: out of
   memory, an unreachable): reported once, and said, with a way back,
   rather than a frozen canvas. A panic is followed by its trap; the
   first, which has the panic's words, is the one sent. */
let crashed = false;
function stopped(line) {
  if (crashed || switching) return;
  crashed = true;
  reportDoc('crash', line).then((d) => sendReport(d, true)).catch(() => {});
  if (!boot.classList.contains('gone')) { say('The game stopped. Reload to try again', true); return; }
  try { document.exitPointerLock(); } catch { /* not locked */ }
  const note = document.createElement('div');
  note.id = 'stopbar';
  note.setAttribute('role', 'alert');
  const a = document.createElement('a');
  a.href = location.href;
  a.textContent = 'RELOAD';
  note.append('THE GAME STOPPED. WHAT IT SAID WAS SENT TO US', a);
  note.title = String(line).slice(0, 400);
  document.body.append(note);
}
const trap = (e) => e instanceof WebAssembly.RuntimeError;
addEventListener('error', (e) => { if (trap(e.error)) stopped(String(e.error)); });
addEventListener('unhandledrejection', (e) => { if (trap(e.reason)) stopped(String(e.reason)); });

/* A player's report: the game's Esc menu (REPORT A BUG, crates/client/
   src/pause.rs) opens this over it. The cursor is already free there. */
{
  const box = $id('report');
  const text = $id('report-text');
  const line = $id('report-status');
  const send = $id('report-send');
  const close = () => {
    box.hidden = true;
    document.getElementById('game').focus();
  };
  window.__oneshotReport = () => {
    try { document.exitPointerLock(); } catch { /* not locked */ }
    line.textContent = '';
    line.className = '';
    send.disabled = false;
    box.hidden = false;
    text.focus();
  };
  $id('report-cancel').addEventListener('click', close);
  box.addEventListener('keydown', (e) => {
    e.stopPropagation();
    if (e.key === 'Escape') close();
  });
  $id('report-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    if (!text.value.trim()) { line.textContent = 'Say what happened first.'; return; }
    send.disabled = true;
    line.className = '';
    line.textContent = 'Sending...';
    const code = await sendReport(await reportDoc('player', text.value), false);
    if (code === 204) {
      text.value = '';
      line.textContent = 'Sent. Thank you.';
      setTimeout(close, 1200);
      return;
    }
    send.disabled = false;
    line.className = 'bad';
    line.textContent = code === 429 ? 'Too many reports from here just now: try again later.'
      : code === 404 ? "This server doesn't take reports."
      : "It didn't go through. Try again in a moment.";
  });
}

/* ── the clip: F9 in a match (after Gates' web/app.js) ───────────────
   The game keeps its last minute (client-net's clip.rs) and F9 hands
   the last half of it here (crates/client/src/clip.rs). The video is
   drawn by the same game in a frame of its own (clip.html, clip.js): a
   page runs one Bevy app for its life, and this one is still playing
   under the card. The frame posts back an MP4 made on this machine;
   nothing is uploaded. One frame per video, so another style is
   another frame over the same tape. */
const clipUi = { tape: null, frame: null, url: null, send: null, style: 'wide', live: true, quality: '' };
const $c = (id) => document.getElementById(id);
const clipSay = (text, bad) => { $c('clip-status').textContent = text; $c('clip-status').className = bad ? 'bad' : ''; };
function clipReset() {
  if (clipUi.frame) { clipUi.frame.remove(); clipUi.frame = null; }
  if (clipUi.url) { URL.revokeObjectURL(clipUi.url); clipUi.url = null; }
  clipUi.send = null;
  $c('clip-view').replaceChildren();
  $c('clip-bar').style.width = '0';
  $c('clip-done').hidden = true;
  clipSay('');
}
/* While the match is still on only the player's own eye is offered
   (clip.rs LIVE_STYLES, from the module: `clipButtons`): a camera behind
   them would show where the enemies the fog sent went, not only the ones
   they saw. None until the module says. */
let CLIP_LIVE = [];
function clipMake(style) {
  if (!clipUi.tape) return;
  if (clipUi.live && !CLIP_LIVE.includes(style)) style = 'wide';
  clipReset();
  clipUi.style = style;
  for (const b of $c('clip-styles').querySelectorAll('button')) b.classList.toggle('on', b.dataset.style === style);
  const view = $c('clip-view');
  view.classList.toggle('tall', style !== 'wide');
  const frame = document.createElement('iframe');
  frame.title = 'Your clip, being drawn';
  /* Nothing in it takes the keys from the match behind the card. */
  frame.inert = true;
  frame.tabIndex = -1;
  frame.src = `clip.html?gfx=${encodeURIComponent(window.__oneshotGfx || 'webgl2')}`;
  view.append(frame);
  clipUi.frame = frame;
  clipSay('Loading the camera...');
  const send = { type: 'clip-tape', tape: clipUi.tape.slice(0), style, line: '', tag: true, quality: clipUi.quality };
  clipUi.send = () => frame.contentWindow.postMessage(send, location.origin, [send.tape]);
}
window.__oneshotClip = (bytes, held, api, live, quality) => {
  /* A copy the page owns: the game's view of its memory moves on. */
  clipUi.tape = new Uint8Array(bytes).buffer;
  clipUi.live = live !== false;
  /* The player's rung (clip.rs): the film draws no more than the match. */
  clipUi.quality = String(quality || '');
  for (const b of $c('clip-styles').querySelectorAll('button')) {
    const shut = clipUi.live && !CLIP_LIVE.includes(b.dataset.style);
    b.disabled = shut;
    b.title = shut ? 'After the match: a camera off your eye would show more than you saw' : '';
  }
  const s = Math.round(Number(held) || 0);
  $c('clip-held').textContent = s >= 15
    ? `The last ${s} seconds you played, made into a video on this computer. Esc frees the mouse to click.`
    : `The last ${s} seconds you played. The longer cameras want a little more play first.`;
  $c('clip').hidden = false;
  clipMake(clipUi.style);
};
window.addEventListener('message', (ev) => {
  if (ev.origin !== location.origin || !clipUi.frame || ev.source !== clipUi.frame.contentWindow) return;
  const m = ev.data || {};
  if (m.type === 'clip-ready' && clipUi.send) {
    clipUi.send();
    clipUi.send = null;
    clipSay("Drawing... you're still in the match behind this.");
  } else if (m.type === 'clip-progress') {
    $c('clip-bar').style.width = `${Math.round(Math.min(1, m.f) * 100)}%`;
    if (m.encoding) clipSay('Finishing the file...');
  } else if (m.type === 'clip-done') {
    clipUi.frame.remove();
    clipUi.frame = null;
    clipUi.url = URL.createObjectURL(m.blob);
    const video = document.createElement('video');
    video.src = clipUi.url;
    video.controls = true;
    video.playsInline = true;
    video.loop = true;
    video.muted = true;
    $c('clip-view').replaceChildren(video);
    const stamp = new Date().toISOString().slice(0, 19).replace(/[-:]/g, '').replace('T', '-');
    const name = `oneshot-clip-${stamp}.mp4`;
    $c('clip-save').href = clipUi.url;
    $c('clip-save').download = name;
    const file = new File([m.blob], name, { type: 'video/mp4' });
    const share = $c('clip-share');
    share.hidden = !(navigator.canShare && navigator.canShare({ files: [file] }));
    share.onclick = () => navigator.share({ files: [file], title: 'oneshot' }).catch(() => {});
    $c('clip-done').hidden = false;
    $c('clip-bar').style.width = '100%';
    clipSay(`${Math.round(m.seconds)} s, ${m.width}x${m.height}. Made on this computer.`);
  } else if (m.type === 'clip-failed') {
    if (clipUi.frame) { clipUi.frame.remove(); clipUi.frame = null; }
    clipSay(`The video could not be made: ${m.why}`, true);
  }
});
/* The style buttons, from the game's own list (clip.rs STYLES, the
   `clip_styles` export), so the card can't offer one the game hasn't or
   miss one it has. */
const CLIP_WORDS = { wide: 'WIDE', pov: 'VERTICAL', chase: 'CHASE', mix: 'MIX', long: 'LONG' };
function clipButtons(styles) {
  const row = $c('clip-styles');
  row.replaceChildren(...styles.map((s) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.dataset.style = s;
    b.textContent = CLIP_WORDS[s] ?? s.toUpperCase();
    b.classList.toggle('on', s === clipUi.style);
    b.addEventListener('click', () => clipMake(s));
    return b;
  }));
}
$c('clip-close').addEventListener('click', () => { clipReset(); clipUi.tape = null; $c('clip').hidden = true; });

let base = '';
try {
  buildInfo = await built;
  mods = buildInfo?.modules ?? null;
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
    // Remembered against this build, as gpuFallBack does: the next
    // visit goes straight to WebGL2.
    try { localStorage.setItem(GPU_FAILED, key()); } catch { /* private window */ }
    gfx = 'webgl2';
    why = 'the WebGPU module did not load';
    showGfx(gfx, why, why);
    glue = await load(gfx);
  }
  const { launch, pose } = glue;
  CLIP_LIVE = glue.clip_live_styles();
  clipButtons(glue.clip_styles());
  if (params.has('pose')) {
    // No server: one fighter for a picture (ci/look.mjs). Gone, as
    // the splash is once a game is drawing, so a refused frame later
    // shows the bar rather than reloading into WebGL2.
    boot.classList.add('gone');
    boot.remove();
    started = performance.now();
    pose(location.search, ['lite'].filter(flag).concat(gfxOpts).join(','));
    await new Promise(() => {}); // nothing more for the page to do
  }
  base = await origin;
  say('Finding the server');
  const info = await getJson(`${base}server.json`, 'The server');
  // A server on its way down: the live one, if it is now another than the
  // one in use (`cur`, `curHash`: a tab outlives deploys, the slots
  // alternate), or its certificate changed.
  const follow = async (cur = info.addr, curHash = info.hash ?? '') => {
    try {
      const now = await getJson(`${base}server.json`, 'The server');
      const hash = now.hash ?? '';
      return now.addr && (now.addr !== cur || hash !== curHash) ? [now.addr, hash] : null;
    } catch { return null; }
  };
  const signin = await setupSignIn(info);
  // A party's link (`?party=CODE`): the game joins it.
  const party = /^[0-9a-zA-Z]{6}$/.test(params.get('party') ?? '') ? [`party=${params.get('party').toUpperCase()}`] : [];
  // A software adapter the WebGPU module was forced onto: the game
  // can't tell from wgpu (look/potato.rs), so the page says.
  const soft = window.__oneshotGfx === 'webgpu' && (await gpuProbe).soft ? ['softgpu'] : [];
  const opts = ['autoplay', 'lite', 'capsules', 'fxdemo', 'menu', 'settings', 'graphics'].filter(flag)
    .concat(params.get('fxstage') ? ['fxstage' + Number(params.get('fxstage'))] : []).concat(gfxOpts, soft).concat(party).join(',');
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
  // The last /nonce's failure is kept, so a click says why (the game
  // shows "Sign-in failed: …", door.rs) rather than a popup with no nonce.
  let nonce = '';
  let nonceErr = null;
  const fresh = async () => {
    try {
      const n = (await getJson(`${base}nonce`, 'The server')).nonce;
      if (!n) throw new Error('The server sent no nonce');
      nonce = n;
      nonceErr = null;
    } catch (e) {
      nonce = '';
      nonceErr = e;
    }
  };
  await fresh();
  // The server takes a nonce for ten minutes (auth.rs NONCE_TTL): one is
  // kept fresher than that, so a click long after the page loaded still
  // opens a popup whose pass checks out. A hidden tab's timers slow; it
  // asks again as it comes back.
  let nonceAt = performance.now();
  const keep = () => {
    if (performance.now() - nonceAt < NONCE_FRESH_MS) return;
    nonceAt = performance.now();
    fresh();
  };
  setInterval(keep, NONCE_FRESH_MS / 4);
  addEventListener('visibilitychange', () => { if (!document.hidden) keep(); });
  const fn = async () => {
    try {
      if (nonceErr) throw nonceErr;
      const res = await sdk().popup({ game: info.game, nonce });
      const text = atob(res.pass.split('.')[0].replace(/-/g, '+').replace(/_/g, '/'));
      const wallet = /wallet: (0x[0-9a-f]{40}|[1-9A-HJ-NP-Za-km-z]{32,44})/.exec(text)?.[1] ?? null;
      return { pass: res.pass, wallet };
    } finally {
      nonceAt = performance.now();
      await fresh();
    }
  };
  return { fn, off: '' };
}

/** The party's voice, for the game (party.rs): it holds the party's session
 * and hands each member's frame here to be played; the microphone's frames
 * go back to it through `voice_up`. The game asks for the mic with
 * `toggle()` (`why()` says what stopped it), names its TALK bind with
 * `setTalk` (held, it talks; or open mic, past a level gate) and its volume
 * with `setVolume`. */
function voiceBridge(glue) {
  // The player pressed MIC; the page is putting the mic back as it was.
  let toggled = false;
  let restoring = false;
  const voice = new Voice((seq, end, bytes) => glue.voice_up(seq, end, bytes), () => {}, pageV);
  window.__oneshotVoice = {
    hear: (tag, seq, flags, bytes, key) => voice.hear(tag, seq, flags, bytes, key),
    toggle: async () => {
      toggled = true;
      // The mic on its way back as the page wakes: the UI showed it off,
      // so this click means on. It stays on.
      if (restoring) {
        restoring = false;
        return;
      }
      try {
        if (voice.mic || voice.opening) voice.micOff();
        else await voice.micOn();
      } catch (e) {
        console.warn('oneshot: the microphone did not open', e);
      }
    },
    state: () => (!voiceWorks() ? 'none' : !voice.mic ? 'off' : voice.onAir ? 'talking' : 'on'),
    why: () => voice.why ?? '',
    mode: () => voice.mode,
    setMode: (m) => voice.setMode(m),
    setTalk: (binds) => voice.setTalk(String(binds).split(' ').filter(Boolean)),
    setVolume: (v) => voice.setVolume(Number(v)),
    muted: (k) => voice.muted(k),
    toggleMute: (k) => voice.toggleMute(k),
    forget: (tags) => voice.forget(Array.from(tags)),
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
  // A browser starts sound only after a gesture: try now, and on every
  // one while it sleeps (suspended again later too: a device change, a
  // phone's call), with the mic back on the first time if it was last time.
  voice.wake();
  let restored = false;
  const wake = async () => {
    if (voice.ctx?.state === 'running' && restored) return;
    await voice.wake();
    if (restored) return;
    restored = true;
    // A MIC click already made the choice.
    if (toggled) return;
    try {
      if (localStorage.getItem('oneshot.mic') === 'on' && !voice.mic && voiceWorks()) {
        restoring = true;
        await voice.micOn();
      }
    } catch { /* asked again on MIC */ } finally { restoring = false; }
  };
  addEventListener('pointerdown', wake, { capture: true });
  addEventListener('keydown', wake, { capture: true });
  addEventListener('visibilitychange', () => { if (!document.hidden && voice.ctx) voice.wake(); });
  // The browser test's fake microphone (?mic=open): on at once.
  if (params.get('mic') === 'open') {
    voice.setMode('open');
    voice.wake().then(() => voice.micOn()).catch((e) => console.warn('mic', e));
  }
}

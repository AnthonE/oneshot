// node ci/e2e.mjs [seconds] [mode] [map]: the real server and the game's
// page (the Bevy client, crates/client/dist) in headless Chromium, WebGL2 on
// SwiftShader. First the menu: a casual mode's button must get a person
// into the match. Then the client drives itself (?autoplay=1: it walks the
// map's route, closes on whoever fog shows and fires; inputs only; ?lite=1:
// a quarter of the pixels, no shadows or grass, for the software GPU;
// ?capsules=1 past a duel) against bots
// that stand still: one (mode 0, the default), seven (3 free for all, 4 Gun
// Game) or fifteen (6 Big Team). Passes when it connected, drew, snapshots
// flowed, its predictions confirmed and it scored a kill. The map is
// Proving Grounds (id 0) unless named. Build first: ci/client.sh and
// `cargo build --release -p server`. A screenshot goes to
// traces/e2e-<mode>-<map>.png.
//
// The page is two modules (ci/client.sh): here it must pick WebGL2, since
// SwiftShader's WebGPU adapter is a software one and the page refuses
// those. `E2E_WEBGPU=1` then smokes the WebGPU module too, in a Chromium
// that draws WebGPU on SwiftShader's Vulkan (`--enable-unsafe-webgpu
// --enable-features=Vulkan --use-vulkan=swiftshader --use-angle=vulkan`;
// with ANGLE on GL the device is destroyed at once): the page must still
// refuse the software adapter by itself, and with `?gfx=webgpu` the module
// must start, keep its device (no fall back to WebGL2), join and draw.
// Off by default: another browser and another minute or two.

import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync, mkdirSync } from 'node:fs';

const seconds = Number(process.argv[2] || 90);
const mode = Number(process.argv[3] || 0);
const map = process.argv[4] || '0';
// Playwright is the repo's one Node dependency (package.json).
const require = createRequire(new URL('../package.json', import.meta.url));
const { chromium } = (() => {
  for (const n of ['playwright', 'playwright-core', '/opt/node-tools/node_modules/playwright']) {
    try { return require(n); } catch { /* next */ }
  }
  throw new Error('playwright not found: npm ci');
})();

const server = spawn('target/release/oneshot-server', [
  '--bind', '127.0.0.1:0', '--http', '127.0.0.1:0', '--web', 'crates/client/dist', '--passive-bots', '--map', map,
], { stdio: ['ignore', 'pipe', 'inherit'] });
const url = await new Promise((ok, fail) => {
  let out = '';
  server.stdout.on('data', (d) => {
    out += d;
    const m = out.match(/play: (http:\/\/\S+)/);
    if (m) ok(m[1]);
  });
  server.on('exit', (c) => fail(new Error(`server exited ${c}`)));
  setTimeout(() => fail(new Error('server never said where to play')), 20000);
});

const exe = process.env.CHROMIUM_PATH || (existsSync('/opt/pw-browsers/chromium') ? '/opt/pw-browsers/chromium' : undefined);
const browser = await chromium.launch({
  executablePath: exe,
  args: ['--no-sandbox', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist',
    '--autoplay-policy=no-user-gesture-required'],
});
const errors = [];
const watch = (p) => {
  p.on('pageerror', (e) => errors.push(e.message));
  p.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
};
const checks = [];
try {
  // The menu: a person's way in. In a browser it is the page's dashboard
  // (web/menu.js) over the game's backdrop; ranked's buttons wait for a
  // sign-in, so only casual modes go through it here.
  const BUTTONS = ['play', 'duel', 'doubles', 'ffa', 'gungame', 'cup', 'bigteam'];
  if ([0, 3, 4, 6].includes(mode)) {
    const p = await browser.newPage({ viewport: { width: 960, height: 540 } });
    watch(p);
    await p.goto(`${url}?lite=1&name=menu`);
    let m = null;
    try {
      // The buttons wait until the game behind the menu has drawn.
      await p.waitForSelector(`#${BUTTONS[mode]}:not([disabled])`, { timeout: 90000 });
      await p.click(`#${BUTTONS[mode]}`, { timeout: 60000 });
      const t = Date.now();
      while (Date.now() - t < 60000) {
        await p.waitForTimeout(500);
        m = await p.evaluate(() => ({ connected: !!window.__oneshot?.connected, menu: getComputedStyle(document.getElementById('menu')).display }));
        if (m.connected && m.menu === 'none') break;
      }
    } catch (e) {
      errors.push(`menu: ${e.message}`);
    }
    mkdirSync('traces', { recursive: true });
    await p.screenshot({ path: `traces/e2e-menu-${mode}.png` });
    checks.push([`the menu's ${BUTTONS[mode]} button got us into a match`, !!m?.connected && m.menu === 'none']);
    await p.close();
  }

  // Small, as the three.js page's gate ran: SwiftShader is fill-bound.
  const page = await browser.newPage({ viewport: { width: 480, height: 270 } });
  watch(page);
  // Every gap between animation frames, from when the page is told to
  // start counting (once it draws steadily): hitches show as gaps far over
  // the median.
  await page.addInitScript(() => {
    window.__gaps = [];
    let last = 0;
    const tick = (t) => {
      if (window.__countGaps && last) window.__gaps.push(t - last);
      last = t;
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });
  // Eight seats or more: capsules, not Triple T (SwiftShader can't skin a
  // room of them), as the three.js page's gate drew them.
  await page.goto(`${url}?mode=${mode}&name=e2e&autoplay=1&lite=1${mode ? '&capsules=1' : ''}`);
  const state = () => page.evaluate(() => window.__oneshot ?? null);
  // Predictions count from the end of the first sample the page drew
  // steadily (5 fps or more): before that it is loading models and
  // compiling shaders, frames take seconds, and inputs made in one miss
  // their ticks.
  const STEADY_FPS = 5;
  const COUNT = 600;
  let s = null, base = null, prev = null;
  const t0 = Date.now();
  while (Date.now() - t0 < seconds * 1000) {
    await page.waitForTimeout(2000);
    s = await state();
    if (!s?.stats) continue;
    const [snaps, conf, corr] = s.stats;
    const now = Date.now();
    const fps = prev ? (s.frames - prev.frames) / ((now - prev.at) / 1000) : 0;
    console.log(`${Math.round((now - t0) / 1000)}s frames ${s.frames} (${fps.toFixed(1)} fps) snapshots ${snaps} confirmed ${conf} corrected ${corr} kills ${s.kills}`);
    if (!base && fps >= STEADY_FPS) {
      base = s.stats.slice();
      await page.evaluate(() => { window.__countGaps = true; });
    }
    prev = { frames: s.frames, at: now, stats: s.stats.slice() };
    const counted = base ? conf - base[1] + corr - base[2] : 0;
    if (s.kills >= 1 && counted >= COUNT) break;
  }
  // Hitches: the worst gaps between frames once steady, against the median.
  const gaps = (await page.evaluate(() => window.__gaps)).sort((a, b) => a - b);
  if (gaps.length) {
    const at = (q) => gaps[Math.min(gaps.length - 1, Math.floor(q * gaps.length))];
    const med = at(0.5);
    const over = gaps.filter((g) => g > 3 * med).length;
    console.log(`frames ${gaps.length}: median ${med.toFixed(0)} ms, p99 ${at(0.99).toFixed(0)} ms, worst ${gaps[gaps.length - 1].toFixed(0)} ms, ${over} over 3x the median`);
  }
  mkdirSync('traces', { recursive: true });
  await page.screenshot({ path: `traces/e2e-${mode}-${map}.png` });
  console.log(JSON.stringify(s));
  const [snaps = 0, conf = 0, corr = 0] = s?.stats ?? [];
  const b = base ?? [conf, conf, corr];
  const counted = conf - b[1] + corr - b[2];
  const ratio = (conf - b[1]) / Math.max(1, counted);
  const fps = (s?.frames ?? 0) / ((Date.now() - t0) / 1000);
  // Noted, never a gate: at SwiftShader's 2 to 20 frames a second every
  // input made in a long frame reaches the server at once, and how many
  // miss their tick is the clock's luck. A real GPU draws 60+ and confirms
  // 99%+ (the native e2e).
  console.log(`note: ${!!base && counted >= COUNT ? 'drew' : 'never drew'} steadily (${STEADY_FPS}+ fps) for ${COUNT} predictions; ${(ratio * 100).toFixed(1)}% of ${counted} confirmed once steady, ${fps.toFixed(1)} fps overall`);
  checks.push(
    ['connected', !!s?.connected],
    ['drew frames', (s?.frames ?? 0) >= 30],
    ['snapshots flowed', snaps > 200],
    ['scored a kill', (s?.kills ?? 0) >= 1],
    [`drew with the WebGL2 module (${s?.gfx}: ${s?.tier})`, s?.gfx === 'webgl2'],
  );
} catch (e) {
  errors.push(String(e));
} finally {
  await browser.close();
}
if (process.env.E2E_WEBGPU === '1') {
  const gpu = await chromium.launch({
    executablePath: exe,
    args: ['--no-sandbox', '--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-vulkan=swiftshader',
      '--use-angle=vulkan', '--ignore-gpu-blocklist', '--autoplay-policy=no-user-gesture-required'],
  });
  try {
    // Left to itself, the page refuses SwiftShader's adapter.
    const p = await gpu.newPage({ viewport: { width: 480, height: 270 } });
    watch(p);
    await p.goto(`${url}?name=pick`);
    await p.waitForFunction(() => !!window.__oneshotGfx, null, { timeout: 120000 }).catch(() => {});
    const picked = await p.evaluate(() => [window.__oneshotGfx, document.getElementById('gfx')?.title]);
    checks.push([`webgpu: the page refused the software adapter (${picked.join(': ')})`, picked[0] === 'webgl2']);
    await p.close();
    // Forced: the WebGPU module joins and draws, and keeps its device.
    const g = await gpu.newPage({ viewport: { width: 480, height: 270 } });
    const gpuErrors = [];
    g.on('console', (m) => { if (/webgpu: |switching to WebGL2/.test(m.text())) gpuErrors.push(m.text().slice(0, 200)); });
    watch(g);
    await g.goto(`${url}?mode=${mode}&name=gpu&autoplay=1&lite=1&gfx=webgpu${mode ? '&capsules=1' : ''}`);
    let w = null;
    const t = Date.now();
    while (Date.now() - t < 150000) {
      await g.waitForTimeout(2000);
      w = await g.evaluate(() => ({ ...(window.__oneshot ?? {}), page: window.__oneshotGfx, at: location.search }));
      if (w.connected && w.frames >= 60 && (w.stats?.[0] ?? 0) > 200) break;
    }
    await g.screenshot({ path: `traces/e2e-webgpu-${mode}-${map}.png` });
    console.log(`webgpu: ${w?.gfx}: ${w?.tier}; frames ${w?.frames}, snapshots ${w?.stats?.[0]}, kills ${w?.kills}`);
    checks.push(
      [`webgpu: the WebGPU module drew (${w?.gfx}, ${w?.at})`, w?.gfx === 'webgpu' && w?.page === 'webgpu'],
      [`webgpu: kept its device${gpuErrors.length ? `: ${gpuErrors.slice(0, 2).join(' | ')}` : ''}`, gpuErrors.length === 0],
      ['webgpu: connected, drew 60 frames, snapshots flowed', !!w?.connected && w.frames >= 60 && (w.stats?.[0] ?? 0) > 200],
    );
  } catch (e) {
    errors.push(`webgpu: ${e}`);
  } finally {
    await gpu.close();
  }
}
server.kill('SIGINT'); // at once; SIGTERM would drain
// The 404 is the favicon or the sign-in SDK offline; anything else is ours.
const real = errors.filter((e) => !/404|ERR_|elopros\.com/.test(e));
checks.push([`no page errors${real.length ? `: ${real.slice(0, 3).join(' | ')}` : ''}`, real.length === 0]);
let failed = false;
for (const [what, ok] of checks) {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  failed ||= !ok;
}
process.exit(failed ? 1 : 0);

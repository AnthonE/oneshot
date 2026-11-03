// node ci/e2e.mjs [seconds] [mode] [map]: the real server and the real page,
// the Bevy client's browser build (ci/client.sh builds crates/client/dist),
// in headless Chromium on SwiftShader. The page drives itself (?autoplay=1:
// it walks the map's route, closes on whoever fog shows and fires; inputs
// only; ?lite=1: a quarter of the pixels and no shadows, for the software
// GPU; ?capsules=1 past one bot: everyone else a capsule) against bots
// that stand still: one (mode 0, the default), seven (3 free for all, 4
// Gun Game) or fifteen (6 Big Team). Passes when it connected, drew,
// snapshots flowed, its predictions confirmed and it scored a kill. The map
// is Proving Grounds (id 0) unless named. A screenshot goes to
// traces/e2e-<mode>-<map>.png.
//
// WEB=web/dist runs it against the three.js page instead, while it lasts.

import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync, mkdirSync } from 'node:fs';

const seconds = Number(process.argv[2] || 90);
const mode = Number(process.argv[3] || 0);
const map = process.argv[4] || '0';
const web = process.env.WEB || 'crates/client/dist';
const bevy = web !== 'web/dist';
// Resolved from web/, where playwright is a dev dependency.
const require = createRequire(new URL('../web/package.json', import.meta.url));
const { chromium } = (() => {
  for (const n of ['playwright', 'playwright-core', '/opt/node-tools/node_modules/playwright']) {
    try { return require(n); } catch { /* next */ }
  }
  throw new Error('playwright not found');
})();

const server = spawn('target/release/oneshot-server', [
  '--bind', '127.0.0.1:0', '--http', '127.0.0.1:0', '--web', web, '--passive-bots', '--map', map,
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
// The two pages report differently: the Bevy page sets window.__oneshot
// each frame, the three.js page exposes state().
const read = () => page.evaluate(() => {
  const o = window.__oneshot;
  if (!o) return null;
  return typeof o.state === 'function' ? o.state() : o;
});
let page;
let failed = false;
try {
  page = await browser.newPage({ viewport: { width: 480, height: 270 } });
  page.on('console', (m) => {
    // The favicon's 404 and Bevy's notes about a software GPU are noise.
    if (m.type() === 'error' && !/404/.test(m.text())) console.error(`[page] ${m.text().slice(0, 300)}`);
  });
  page.on('pageerror', (e) => console.error(`[page] ${e.message}`));
  await page.goto(`${url}?autoplay=1&lite=1&name=e2e&mode=${mode}${mode ? '&capsules=1' : ''}`);
  const t0 = Date.now();
  let s = null, base = [0, 0, 0], joined = 0;
  while (Date.now() - t0 < seconds * 1000) {
    await page.waitForTimeout(1000);
    s = await read();
    if (!s) continue;
    if (s.error) throw new Error(s.error);
    const [snaps = 0, conf = 0, corr = 0] = s.stats || [];
    console.log(`${Math.round((Date.now() - t0) / 1000)}s frames ${s.frames} snapshots ${snaps} confirmed ${conf} corrected ${corr} kills ${s.kills}`);
    // The first seconds after the welcome are the countdown and the clock
    // settling, and a software GPU's first frames compile every shader and
    // take seconds each: measured from the 60th frame on.
    if (!s.connected || s.frames < 60) {
      base = (s.stats || [0, 0, 0]).slice(0, 3);
      continue;
    }
    joined ||= Date.now();
    if (Date.now() - joined < 8000) base = (s.stats || [0, 0, 0]).slice(0, 3);
    if (s.kills >= 1 && Date.now() - joined > 30000) break;
  }
  mkdirSync('traces', { recursive: true });
  await page.screenshot({ path: `traces/e2e-${mode}-${map}.png` });
  if (!s) throw new Error('the page never reported');
  const [snaps = 0, conf = 0, corr = 0] = s.stats;
  const ratio = (conf - base[1]) / Math.max(1, conf - base[1] + corr - base[2]);
  const fps = s.frames / ((Date.now() - t0) / 1000);
  const checks = [
    ['connected', s.connected],
    // A software GPU draws a few frames a second of the Bevy client.
    ['drew frames', s.frames > (bevy ? 30 : 60)],
    ['snapshots flowed', snaps > 200],
    // Every input made in a long frame reaches the server at once, and a
    // few miss their tick. Real GPUs draw 60+ and confirm more (the native
    // e2e: 99%+).
    [`predictions confirmed after the start (${(ratio * 100).toFixed(1)}%, ${fps.toFixed(1)} fps)`, ratio >= 0.9],
    ['scored a kill', s.kills >= 1],
  ];
  for (const [what, ok] of checks) {
    console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
    failed ||= !ok;
  }
} catch (e) {
  console.error(e);
  failed = true;
} finally {
  await browser.close();
  server.kill('SIGINT'); // at once; SIGTERM would drain
}
process.exit(failed ? 1 : 0);

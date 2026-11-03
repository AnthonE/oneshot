// node ci/e2e.mjs [seconds] [mode] [map]: the real server and the real page in
// headless Chromium. The page autoplays (?autoplay: it walks at the bot,
// aims and fires; inputs only) against bots that stand still: one (mode 0,
// the default), seven (3 free for all, 4 Gun Game) or fifteen (6 Big
// Team). Passes when it
// connected, drew, its predictions confirmed and it scored a kill. The map
// is Proving Grounds (id 0) unless named.
// A screenshot goes to traces/e2e-<mode>.png.

import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync, mkdirSync } from 'node:fs';

const seconds = Number(process.argv[2] || 40);
const mode = Number(process.argv[3] || 0);
const map = process.argv[4] || '0';
// Resolved from web/, where playwright is a dev dependency.
const require = createRequire(new URL('../web/package.json', import.meta.url));
const { chromium } = (() => {
  for (const n of ['playwright', 'playwright-core', '/opt/node-tools/node_modules/playwright']) {
    try { return require(n); } catch { /* next */ }
  }
  throw new Error('playwright not found');
})();

const server = spawn('target/release/oneshot-server', [
  '--bind', '127.0.0.1:0', '--http', '127.0.0.1:0', '--web', 'web/dist', '--passive-bots', '--map', map,
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
let failed = false;
try {
  const page = await browser.newPage({ viewport: { width: 480, height: 270 } });
  page.on('console', (m) => { if (m.type() === 'error' || m.type() === 'warning') console.error(`[page] ${m.text()}`); });
  page.on('pageerror', (e) => console.error(`[page] ${e.message}`));
  await page.goto(`${url}?autoplay=1&lite=1&name=e2e&mode=${mode}${mode ? '&capsules=1' : ''}`);
  const t0 = Date.now();
  let s, base = [0, 0, 0];
  while (Date.now() - t0 < seconds * 1000) {
    await page.waitForTimeout(1000);
    s = await page.evaluate(() => window.__oneshot.state());
    if (s.error) throw new Error(s.error);
    const [snaps = 0, conf = 0, corr = 0] = s.stats;
    console.log(`${Math.round((Date.now() - t0) / 1000)}s frames ${s.frames} snapshots ${snaps} confirmed ${conf} corrected ${corr} kills ${s.kills}`);
    // The first seconds are the countdown and the clock settling.
    if (Date.now() - t0 < 5000) base = s.stats.slice(0, 3);
    if (s.kills >= 1 && Date.now() - t0 > 20000) break;
  }
  mkdirSync('traces', { recursive: true });
  await page.screenshot({ path: `traces/e2e-${mode}-${map}.png` });
  const [snaps = 0, conf = 0, corr = 0] = s.stats;
  const ratio = (conf - base[1]) / Math.max(1, conf - base[1] + corr - base[2]);
  const fps = s.frames / ((Date.now() - t0) / 1000);
  const checks = [
    ['connected', s.connected],
    ['drew frames', s.frames > 60],
    ['snapshots flowed', snaps > 200],
    // A software GPU draws 10 to 30 frames a second here; every input made
    // in a long frame reaches the server at once, and a few miss their tick.
    // Real GPUs draw 60+ and confirm more (the native e2e: 99%+).
    [`predictions confirmed after the start (${(ratio * 100).toFixed(1)}%, ${fps.toFixed(0)} fps)`, ratio >= 0.9],
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
  server.kill();
}
process.exit(failed ? 1 : 0);

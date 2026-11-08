// node ci/look.mjs [query...]: pictures of the art with no match running
// (the Bevy client's ?pose mode, crates/client/src/pose.rs): a fighter in
// front of the camera, facing it and side on, with a long gun and a pistol.
// Into traces/look-*.png (`--prefix=level` for traces/level-*.png; a map's
// view is `map=4&eye=x,y,z&look=x,y,z`). Build first: ci/client.sh, and the server.

import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync, mkdirSync } from 'node:fs';

const require = createRequire(new URL('../package.json', import.meta.url));
const { chromium } = (() => {
  for (const n of ['playwright', 'playwright-core', '/opt/node-tools/node_modules/playwright']) {
    try { return require(n); } catch { /* next */ }
  }
  throw new Error('playwright not found');
})();
const server = spawn('target/release/oneshot-server', ['--bind', '127.0.0.1:0', '--http', '127.0.0.1:0', '--web', 'crates/client/dist', '--all-modes'],
  { stdio: ['ignore', 'pipe', 'inherit'] });
const url = await new Promise((ok) => {
  let out = '';
  server.stdout.on('data', (d) => { out += d; const m = out.match(/play: (http:\/\/\S+)/); if (m) ok(m[1]); });
});
const exe = process.env.CHROMIUM_PATH || (existsSync('/opt/pw-browsers/chromium') ? '/opt/pw-browsers/chromium' : undefined);
const browser = await chromium.launch({ executablePath: exe, args: ['--no-sandbox', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
mkdirSync('traces', { recursive: true });
const argv = process.argv.slice(2);
const prefix = (argv.find((a) => a.startsWith('--prefix=')) || '--prefix=look').slice('--prefix='.length);
const queries = argv.filter((a) => !a.startsWith('--'));
const shots = queries.length ? queries : ['yaw=3.1416&arm=1', 'yaw=1.5708&arm=1', 'yaw=1.5708&arm=3', 'yaw=0&arm=4&speed=5'];
for (const [i, q] of shots.entries()) {
  const page = await browser.newPage({ viewport: { width: 800, height: 450 } });
  page.on('pageerror', (e) => console.error(`[page] ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error' || m.type() === 'warning') console.error(`[page] ${m.text()}`); });
  await page.goto(`${url}?pose=1&${q}`);
  // Frames drawn: the models and textures load over the first few.
  await page.waitForFunction(() => (window.__oneshot?.frames ?? 0) > 20, null, { timeout: 120000 }).catch(() => console.error('art never drew'));
  await page.waitForTimeout(1500);
  // A ship's hall on SwiftShader draws a frame every couple of seconds.
  await page.screenshot({ path: `traces/${prefix}-${i}.png`, timeout: 180000 });
  console.log(`traces/${prefix}-${i}.png  ${q}`);
  await page.close();
}
await browser.close();
server.kill('SIGINT'); // at once; SIGTERM would drain

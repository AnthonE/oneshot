// node ci/client_shot.mjs [out.png] [seconds] [mode] [map] [autoplay 0|1] [more query]: the Bevy client in
// headless Chromium (WebGL2 on SwiftShader) against the real server, a
// screenshot of what it draws: the browser tier's look (ci/client.sh builds
// crates/client/dist first). The seconds count from when the screen is
// there (`__oneshot.ready`: its level built and dressed, its shaders
// compiled, no curtain or loading overlay over it; crates/client/src/loading.rs),
// as ci/desktop_shot.sh's do. Defaults: Big Team on Dry Fork, 25 s.
// autoplay 0 is the title; `menu=1` or `settings=1` opens them (the match's
// with autoplay=1, else the settings over the title), `graphics=1` the
// settings on GRAPHICS; `quality=potato|low|medium|high` draws that rung
// (unset, the page's default for its module, or POTATO where the machine
// is a software one: crates/client/src/look/potato.rs). `fxdemo=1` as the last
// argument stages the effects in front of the camera, `fxstage=N` only
// stage N (crates/client/src/look/fx/demo.rs). `GFX=webgpu` shoots the
// WebGPU module (the desktop's frame; ci/e2e.mjs says how headless Chromium
// draws it), `GFX=webgl2` forces the other; unset, the page picks (WebGL2
// here: SwiftShader's WebGPU adapter is a software one, which it refuses).
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { createRequire } from 'node:module';
const require = createRequire(new URL('../package.json', import.meta.url));
const { chromium } = require('playwright');
const [out = 'traces/client.png', secs = '25', mode = '6', map = '3', auto = '0', more = ''] = process.argv.slice(2);
const server = spawn('target/release/oneshot-server', ['--bind', '127.0.0.1:0', '--http', '127.0.0.1:0', '--web', 'crates/client/dist', '--map', map], { stdio: ['ignore', 'pipe', 'inherit'] });
const url = await new Promise((ok) => { let o = ''; server.stdout.on('data', (d) => { o += d; const m = o.match(/play: (http:\/\/\S+)/); if (m) ok(m[1]); }); });
const exe = existsSync('/opt/pw-browsers/chromium') ? '/opt/pw-browsers/chromium' : undefined;
mkdirSync(dirname(out), { recursive: true });
const gfx = process.env.GFX || '';
const args = gfx === 'webgpu'
  ? ['--no-sandbox', '--enable-unsafe-webgpu', '--enable-features=Vulkan', '--use-vulkan=swiftshader', '--use-angle=vulkan', '--ignore-gpu-blocklist']
  : ['--no-sandbox', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'];
const browser = await chromium.launch({ executablePath: exe, args });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
page.on('console', (m) => { if (m.type() === 'error' || m.type() === 'warning') console.error('[page]', m.text().slice(0, 300)); });
page.on('pageerror', (e) => console.error('[pageerror]', e.message.slice(0, 300)));
await page.goto(`${url}?mode=${mode}&name=bevy&autoplay=${auto}${gfx ? '&gfx=' + gfx : ''}${more ? '&' + more : ''}`);
const t0 = Date.now();
const ready = await page.waitForFunction(() => window.__oneshot?.ready === true, null, { timeout: 300000 })
  .then(() => true, () => false);
const stage = await page.evaluate(() => window.__oneshot?.stage ?? 'nothing drawn');
console.log('drawing with', await page.evaluate(() => `${window.__oneshot?.gfx}: ${window.__oneshot?.tier}`));
console.log(ready ? `ready after ${((Date.now() - t0) / 1000).toFixed(1)} s` : `never ready (at ${stage}): shooting anyway`);
await page.waitForTimeout(Number(secs) * 1000);
await page.screenshot({ path: out });
console.log('shot', out);
await browser.close();
server.kill();

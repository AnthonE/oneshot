// node ci/client_shot.mjs [out.png] [seconds] [mode] [map] [autoplay 0|1] [more query]: the Bevy client in
// headless Chromium (WebGL2 on SwiftShader) against the real server, a
// screenshot of what it draws: the browser tier's look (ci/client.sh builds
// crates/client/dist first). Defaults: Big Team on Dry Fork, 25 s. `fxdemo=1`
// as the last argument stages the effects in front of the camera.
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { createRequire } from 'node:module';
const require = createRequire(new URL('../web/package.json', import.meta.url));
const { chromium } = require('playwright');
const [out = 'traces/client.png', secs = '25', mode = '6', map = '3', auto = '0', more = ''] = process.argv.slice(2);
const server = spawn('target/release/oneshot-server', ['--bind', '127.0.0.1:0', '--http', '127.0.0.1:0', '--web', 'crates/client/dist', '--map', map], { stdio: ['ignore', 'pipe', 'inherit'] });
const url = await new Promise((ok) => { let o = ''; server.stdout.on('data', (d) => { o += d; const m = o.match(/play: (http:\/\/\S+)/); if (m) ok(m[1]); }); });
const exe = existsSync('/opt/pw-browsers/chromium') ? '/opt/pw-browsers/chromium' : undefined;
mkdirSync(dirname(out), { recursive: true });
const browser = await chromium.launch({ executablePath: exe, args: ['--no-sandbox', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
page.on('console', (m) => { if (m.type() === 'error' || m.type() === 'warning') console.error('[page]', m.text().slice(0, 300)); });
page.on('pageerror', (e) => console.error('[pageerror]', e.message.slice(0, 300)));
await page.goto(`${url}?mode=${mode}&name=bevy&autoplay=${auto}${more ? '&' + more : ''}`);
await page.waitForTimeout(Number(secs) * 1000);
await page.screenshot({ path: out });
console.log('shot', out);
await browser.close();
server.kill();

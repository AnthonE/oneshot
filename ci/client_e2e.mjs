// node ci/client_e2e.mjs [seconds] [mode] [map]: the Bevy client in headless
// Chromium (WebGL2 on SwiftShader) against the real server, driving itself
// (?autoplay=1: it walks the map's route, closes on whoever fog shows and
// fires; inputs only; ?lite=1: a quarter of the pixels, no shadows, for the
// software GPU) against bots that stand still. Passes when it
// connected, drew frames and scored a kill. Defaults: a casual duel against
// the bot on Proving Grounds. ci/client.sh builds crates/client/dist first.
// A screenshot goes to traces/client-e2e-<mode>.png. First, the menu: its
// button for the mode must get a person into the match.

import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync, mkdirSync } from 'node:fs';

const seconds = Number(process.argv[2] || 90);
const mode = Number(process.argv[3] || 0);
const map = process.argv[4] || '0';
const require = createRequire(new URL('../web/package.json', import.meta.url));
const { chromium } = require('playwright');

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

const exe = existsSync('/opt/pw-browsers/chromium') ? '/opt/pw-browsers/chromium' : undefined;
const browser = await chromium.launch({
  executablePath: exe,
  args: ['--no-sandbox', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--autoplay-policy=no-user-gesture-required'],
});
const page = await browser.newPage({ viewport: { width: 960, height: 540 } });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
// The menu first: a person's way in. Pick the mode's button, and the menu
// must give way to the game.
const BUTTONS = ['play', 'duel', 'doubles', 'ffa', 'gungame', 'cup', 'bigteam'];
const menuFails = [];
// Ranked's buttons wait for a sign-in: only casual modes go through it here.
if ([0, 3, 4, 6].includes(mode)) {
  const p = await browser.newPage({ viewport: { width: 960, height: 540 } });
  p.on('pageerror', (e) => errors.push(e.message));
  await p.goto(`${url}?lite=1&name=menu`);
  await p.click(`#${BUTTONS[mode]}`, { timeout: 30000 }).catch((e) => menuFails.push(`no ${BUTTONS[mode]} button: ${e.message}`));
  const t = Date.now();
  let m = null;
  while (Date.now() - t < 30000) {
    await p.waitForTimeout(500);
    m = await p.evaluate(() => ({ connected: !!window.__oneshot?.connected, menu: getComputedStyle(document.getElementById('menu')).display }));
    if (m.connected && m.menu === 'none') break;
  }
  if (!m?.connected) menuFails.push('the menu never got us into a match');
  else if (m.menu !== 'none') menuFails.push('the menu stayed up in the match');
  await p.close();
}
await page.goto(`${url}?mode=${mode}&name=e2e&autoplay=1&lite=1`);

const state = () => page.evaluate(() => window.__oneshot ?? null);
let s = null;
const t0 = Date.now();
while (Date.now() - t0 < seconds * 1000) {
  await page.waitForTimeout(2000);
  s = await state();
  if (s && s.kills > 0) break;
}
mkdirSync('traces', { recursive: true });
await page.screenshot({ path: `traces/client-e2e-${mode}.png` });
await browser.close();
server.kill();

console.log(JSON.stringify(s));
const fails = [...menuFails];
if (!s || !s.connected) fails.push('never connected');
if (!s || s.frames < 30) fails.push(`drew ${s?.frames ?? 0} frames`);
if (!s || s.kills < 1) fails.push('scored no kill');
// The 404 is the favicon; anything else is ours.
const real = errors.filter((e) => !/404/.test(e));
if (real.length) fails.push(`page errors: ${real.slice(0, 3).join(' | ')}`);
if (fails.length) {
  console.error('client e2e FAILED:', fails.join('; '));
  process.exit(1);
}
console.log('client e2e ok');

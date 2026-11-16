// node ci/phone.mjs [out dir]: the page on a landscape phone (844 by 390 CSS
// px, touch, Chromium's emulation; WebGL2 on SwiftShader), played by touch
// alone (crates/client/src/touch.rs): the title's FREE FOR ALL row and PLAY
// tapped, then the stick held forward, a drag on the right half, both
// thumbs at once (the stick and FIRE), and MENU. Bots that don't shoot
// (`--passive-bots`), so a death doesn't spoil a check. Pictures of each
// into the out dir (traces/phone). Needs the release server and the page
// (ci/client.sh, or ci/client.sh dev). Exit 1 on a failed check.
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
const require = createRequire(new URL('../package.json', import.meta.url));
const { chromium } = require('playwright');
const out = process.argv[2] || 'traces/phone';
mkdirSync(out, { recursive: true });
// The phone the layout is drawn for (touch.rs, `PHONE`): its numbers hold
// as they are here.
const W = 844, H = 390, EDGE = 14;
const FIRE = [W - EDGE - 118, H - EDGE - 108];
const STICK = [Math.round(W * 0.14), Math.round(H * 0.72)];
const VIEW = [Math.round(W * 0.62), Math.round(H * 0.3)];
// MENU, under the shield: the HUD drawn at the UI's scale (ui.rs).
const MENU = [W / 2, Math.round(52 * Math.min(W / 1280, H / 720) + 27)];
const server = spawn('target/release/oneshot-server', ['--bind', '127.0.0.1:0', '--http', '127.0.0.1:0', '--web', 'crates/client/dist',
  '--all-modes', '--passive-bots', '--map', '3'], { stdio: ['ignore', 'pipe', 'ignore'] });
const url = await new Promise((ok) => { let o = ''; server.stdout.on('data', (d) => { o += d; const m = o.match(/play: (http:\/\/\S+)/); if (m) ok(m[1]); }); });
const exe = process.env.CHROMIUM_PATH || (existsSync('/opt/pw-browsers/chromium') ? '/opt/pw-browsers/chromium' : undefined);
const browser = await chromium.launch({ executablePath: exe,
  args: ['--no-sandbox', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
const ctx = await browser.newContext({ viewport: { width: W, height: H }, deviceScaleFactor: 1, isMobile: true, hasTouch: true });
const page = await ctx.newPage();
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text().slice(0, 300)); });
// Fingers by the DevTools protocol: every finger down is in each event, so
// two can be held at once (Playwright's touchscreen has one).
const cdp = await ctx.newCDPSession(page);
const pts = (list) => list.map(([x, y, id]) => ({ x, y, id, radiusX: 8, radiusY: 8, force: 1 }));
const down = (list) => cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: pts(list) });
const move = (list) => cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: pts(list) });
const up = () => cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
const tap = async ([x, y]) => { await down([[x, y, 1]]); await page.waitForTimeout(300); await up(); await page.waitForTimeout(500); };
const st = () => page.evaluate(() => {
  const o = window.__oneshot ?? {};
  return { screen: o.screen, connected: o.connected, touch: o.touch, alive: o.alive, pos: o.pos, yaw: o.yaw, ready: o.ready };
});
const until = async (f, ms) => {
  const t = Date.now();
  while (Date.now() - t < ms) { const v = await f(); if (v) return v; await page.waitForTimeout(500); }
  return null;
};
const checks = [];
const check = (what, ok) => { checks.push(ok); console.log(ok ? 'ok  ' : 'FAIL', what); };
try {
  await page.goto(`${url}?name=phone`);
  // The title is the game's (front.rs): its rows where it says they are.
  const rect = (tag) => page.evaluate((t) => window.__oneshot?.ui?.[t] ?? null, tag);
  const row = await until(() => rect('mode3'), 300000);
  if (!row) throw new Error('the title never showed its rows');
  await page.waitForTimeout(1500);
  await page.screenshot({ path: `${out}/title.png` });
  await tap(row);
  const go = await until(() => rect('go'), 20000);
  await tap(go);
  const s = await until(async () => { const v = await st(); return v.connected && v.screen === 'playing' && v.ready && v.alive ? v : null; }, 240000);
  check('tapped from the title into a match', !!s);
  check('the touch controls are on (a finger is the pointer)', !!s?.touch);
  await page.waitForTimeout(3000);
  await page.screenshot({ path: `${out}/match.png` });
  // The stick pushed a while: we walk. Forward, back, left and right, each
  // the way it was pushed (forward is where we face: 0 is +x, growing toward
  // +z; right a quarter turn on). A spawn boxed in walks nowhere far, but
  // still the right way.
  let walked = 0;
  let right = 0;
  for (const [dx, dy] of [[0, -1], [0, 1], [-1, 0], [1, 0]]) {
    const a = await st();
    await down([[STICK[0], STICK[1], 1]]);
    for (let i = 1; i <= 6; i++) { await move([[STICK[0] + 10 * i * dx, STICK[1] + 10 * i * dy, 1]]); await page.waitForTimeout(50); }
    await page.waitForTimeout(1000);
    if (dy === -1) await page.screenshot({ path: `${out}/stick.png` });
    await page.waitForTimeout(1000);
    const b = await st();
    await up();
    const [fwd, side] = [-dy, dx];
    const way = [fwd * Math.cos(a.yaw) - side * Math.sin(a.yaw), fwd * Math.sin(a.yaw) + side * Math.cos(a.yaw)];
    const [mx, mz] = [b.pos[0] - a.pos[0], b.pos[2] - a.pos[2]];
    if (a.alive && b.alive && Math.abs(b.yaw - a.yaw) < 0.2) {
      walked = Math.max(walked, Math.hypot(mx, mz));
      if (mx * way[0] + mz * way[1] > 0.005) right++;
    }
    if (process.env.PHONE_DEBUG) console.log('stick', dx, dy, JSON.stringify(a), JSON.stringify(b));
  }
  check(`the stick walked us ${walked.toFixed(2)} m at most, ${right} of 4 pushes the way pushed`,
    walked < 20 && (right === 4 || walked > 1.5));
  // 100 px across the right half: about 40 degrees at the default speed.
  await page.waitForTimeout(500);
  const c = await st();
  await down([[VIEW[0], VIEW[1], 2]]);
  for (let i = 1; i <= 10; i++) { await move([[VIEW[0] - 10 * i, VIEW[1], 2]]); await page.waitForTimeout(60); }
  await page.waitForTimeout(300);
  await up();
  await page.waitForTimeout(800);
  const d = await st();
  const turned = Math.atan2(Math.sin(d.yaw - c.yaw), Math.cos(d.yaw - c.yaw)) * 180 / Math.PI;
  // Exactly the finger's: a page's winit reports a finger as the mouse too
  // (lib.rs, `pump`), which turned it a sixth again.
  check(`a 100 px drag turned us ${turned.toFixed(1)} degrees`, Math.abs(turned) > 33 && Math.abs(turned) < 47);
  // Both thumbs: the stick, and FIRE dragged as it fires.
  await down([[STICK[0], STICK[1], 3], [FIRE[0], FIRE[1], 4]]);
  for (let i = 1; i <= 5; i++) { await move([[STICK[0] + 12 * i, STICK[1] - 6 * i, 3], [FIRE[0] + 4 * i, FIRE[1], 4]]); await page.waitForTimeout(80); }
  await page.waitForTimeout(500);
  await page.screenshot({ path: `${out}/fire.png` });
  await up();
  await page.waitForTimeout(600);
  await tap(MENU);
  check('MENU opened the Esc menu', !!(await until(async () => (await st()).screen === 'paused', 5000)));
  await page.waitForTimeout(800);
  await page.screenshot({ path: `${out}/menu.png` });
} catch (e) {
  errors.push(String(e?.stack ?? e));
}
check(`no page errors${errors.length ? ': ' + errors.slice(0, 5).join(' | ') : ''}`, errors.length === 0);
console.log('pictures in', out);
await browser.close();
server.kill();
process.exit(checks.every(Boolean) ? 0 : 1);

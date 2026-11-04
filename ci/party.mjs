// node ci/party.mjs: two pages make a party and play together, in headless
// Chromium against the real server (build first: ci/client.sh and
// `cargo build --release -p server`). The first page's party code brings the
// second in; the first talks (Chromium's fake microphone, a beep, on open
// mic) and the second plays its frames; the leader starts Free for all and
// both pages land in one match, each naming the other. Screenshots of the
// menu with the party go to traces/party-*.png.

import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { existsSync, mkdirSync } from 'node:fs';

const require = createRequire(new URL('../package.json', import.meta.url));
const { chromium } = (() => {
  for (const n of ['playwright', 'playwright-core', '/opt/node-tools/node_modules/playwright']) {
    try { return require(n); } catch { /* next */ }
  }
  throw new Error('playwright not found: npm ci');
})();

const server = spawn('target/release/oneshot-server', [
  '--bind', '127.0.0.1:0', '--http', '127.0.0.1:0', '--web', 'crates/client/dist', '--passive-bots',
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
    '--autoplay-policy=no-user-gesture-required', '--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream'],
});
const errors = [];
const checks = [];
const until = async (what, fn, ms = 60000) => {
  const t = Date.now();
  while (Date.now() - t < ms) {
    try {
      const v = await fn();
      if (v) return v;
    } catch { /* not yet */ }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(`timed out: ${what}`);
};
try {
  mkdirSync('traces', { recursive: true });
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 720 }, permissions: ['microphone'] });
  const open = async (q) => {
    const p = await ctx.newPage();
    p.on('pageerror', (e) => errors.push(e.message));
    p.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
    await p.goto(`${url}?lite=1&${q}`);
    return p;
  };
  const lobby = (p) => p.evaluate(() => window.__lobby);
  const a = await open('name=alpha&mic=open');
  const pa = await until('the first page has a party', async () => (await lobby(a))?.party);
  const b = await open(`name=bravo&party=${pa.text}`);
  const both = await until('the second page is in the first one\'s party', async () => {
    const [la, lb] = [await lobby(a), await lobby(b)];
    return la?.party?.members.length === 2 && lb?.party?.code === la.party.code ? [la, lb] : null;
  });
  checks.push([`the code ${pa.text} brought the second page in`, !!both]);
  await a.screenshot({ path: 'traces/party-menu.png' });
  await b.click('#blade-party');
  await b.screenshot({ path: 'traces/party-pane.png' });

  const heard = await until('the second page plays the first one\'s voice', async () => {
    const [la, lb] = [await lobby(a), await lobby(b)];
    return la.sent > 25 && lb.heard > 25 ? [la.sent, lb.heard] : null;
  }, 30000).catch((e) => { errors.push(e.message); return null; });
  checks.push([`voice: ${heard ? `${heard[0]} frames sent, ${heard[1]} played` : 'none'}`, !!heard]);

  await a.click('#ffa');
  const playing = async (p) => p.evaluate(() => !!window.__oneshot?.connected
    && getComputedStyle(document.getElementById('menu')).display === 'none' ? window.__oneshot.names : null);
  const [na, nb] = [await until('the leader is in the match', () => playing(a), 90000),
    await until('the member is in the match', () => playing(b), 90000)];
  checks.push(['the leader started Free for all for both', !!na && !!nb]);
  checks.push([`one match: each page names the other (${na.join(', ')})`, na.includes('bravo') && nb.includes('alpha')]);
  const still = await lobby(b);
  checks.push(['the party stays up through the match', still.connected && still.party?.members.length === 2]);
} catch (e) {
  errors.push(String(e));
} finally {
  await browser.close();
  server.kill('SIGINT');
}
// The 404 is the favicon or the sign-in SDK offline; anything else is ours.
const real = errors.filter((e) => !/404|ERR_|elopros\.com/.test(e));
checks.push([`no page errors${real.length ? `: ${real.slice(0, 3).join(' | ')}` : ''}`, real.length === 0]);
let failed = false;
for (const [what, ok] of checks) {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  failed ||= !ok;
}
process.exit(failed ? 1 : 0);

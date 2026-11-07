// render.mjs — draws make.py's SVGs (and banner.html) to PNG in Chromium.
//   node tools/logo/render.mjs tools/logo/drafts
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';

const require = createRequire(new URL('../../package.json', import.meta.url));
const { chromium } = require('playwright');

const dir = path.resolve(process.argv[2]);
const b = await chromium.launch();
const p = await b.newPage();
for (const f of fs.readdirSync(dir).filter((f) => f.endsWith('.svg'))) {
  const svg = fs.readFileSync(path.join(dir, f), 'utf8');
  const [, w, h] = svg.match(/width="(\d+)" height="(\d+)"/).map(Number);
  await p.setViewportSize({ width: w, height: h });
  await p.setContent(`<html><body style="margin:0;background:transparent">${svg}</body></html>`);
  await p.screenshot({ path: path.join(dir, f.replace(/\.svg$/, '.png')), omitBackground: true, clip: { x: 0, y: 0, width: w, height: h } });
}
const banner = path.join(dir, 'banner.html');
if (fs.existsSync(banner)) {
  await p.setViewportSize({ width: 1500, height: 500 });
  await p.goto('file://' + banner);
  await p.evaluate(() => document.fonts.ready);
  await p.screenshot({ path: path.join(dir, 'banner.png') });
}
await b.close();

// render.mjs — draws make.py's SVGs (and banner.html) to PNG in Chromium.
//   node tools/logo/render.mjs tools/logo/drafts
//   node tools/logo/render.mjs tools/logo/drafts post    # only post.html → post.png
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';

const require = createRequire(new URL('../../package.json', import.meta.url));
const { chromium } = require('playwright');

const dir = path.resolve(process.argv[2]);
const only = process.argv[3];
const b = await chromium.launch();
const p = await b.newPage();
const page = async (name, width, height) => {
  const f = path.join(dir, `${name}.html`);
  if (!fs.existsSync(f)) return;
  await p.setViewportSize({ width, height });
  await p.goto('file://' + f);
  await p.evaluate(() => document.fonts.ready);
  await p.screenshot({ path: path.join(dir, `${name}.png`) });
};
if (only === 'post') {
  await page('post', 1600, 900);
} else {
  for (const f of fs.readdirSync(dir).filter((f) => f.endsWith('.svg'))) {
    const svg = fs.readFileSync(path.join(dir, f), 'utf8');
    const [, w, h] = svg.match(/width="(\d+)" height="(\d+)"/).map(Number);
    await p.setViewportSize({ width: w, height: h });
    await p.setContent(`<html><body style="margin:0;background:transparent">${svg}</body></html>`);
    await p.screenshot({ path: path.join(dir, f.replace(/\.svg$/, '.png')), omitBackground: true, clip: { x: 0, y: 0, width: w, height: h } });
  }
  await page('banner', 1500, 500);
}
await b.close();

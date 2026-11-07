// node tools/flow/png.mjs plan.svg [plan.png]: a flow plan as a picture
// (headless Chromium draws the SVG; `npm ci` once for Playwright).
import { createRequire } from 'node:module';
import { existsSync, readFileSync } from 'node:fs';
const require = createRequire(new URL('../../package.json', import.meta.url));
const { chromium } = require('playwright');
const [inp, out = inp.replace(/\.svg$/, '.png')] = process.argv.slice(2);
const svg = readFileSync(inp, 'utf8');
const w = Number(svg.match(/width="([\d.]+)"/)[1]), h = Number(svg.match(/height="([\d.]+)"/)[1]);
const exe = process.env.CHROMIUM_PATH || (existsSync('/opt/pw-browsers/chromium') ? '/opt/pw-browsers/chromium' : undefined);
const b = await chromium.launch({ executablePath: exe, args: ['--no-sandbox'] });
const p = await b.newPage({ viewport: { width: Math.ceil(w), height: Math.ceil(h) } });
await p.setContent(`<body style="margin:0">${svg}</body>`);
await p.screenshot({ path: out });
await b.close();
console.log(out);

// Drive trailer.html in headless Chromium (make.sh copies this into build/).
// After Gates' ci/trailer/render.mjs (MIT); TRAILER_STEP is the preview's.
//   node render.mjs stills OUTDIR t1,t2,...      one JPEG per time, for checking frames
//   node render.mjs video OUT.mkv [from] [to]    every frame, piped to ffmpeg (no audio)
//   node render.mjs cues OUT.json                the scene times and the audio cue list
//   node render.mjs art OUTDIR name1,name2,...   the page's still compositions (key art, the poster, the capsule) as PNGs
// TRAILER_URL is the page (make.sh serves build/); TRAILER_Q its query.
// CHROMIUM_PATH names a browser; else /opt/pw-browsers/chromium if present, else Playwright's own.
import { chromium } from 'playwright';
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';

const [mode, out, a, b] = process.argv.slice(2);
const url = (process.env.TRAILER_URL || 'http://127.0.0.1:8765/trailer.html') + (process.env.TRAILER_Q || '');
const exe = process.env.CHROMIUM_PATH || (existsSync('/opt/pw-browsers/chromium') ? '/opt/pw-browsers/chromium' : undefined);
const br = await chromium.launch({ executablePath: exe, args: ['--no-sandbox', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
const W = +(process.env.TRAILER_W || 1920), H = +(process.env.TRAILER_H || 1080);
const p = await br.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: 1 });
p.on('console', m => { if (!m.text().includes('WebGLRenderer')) console.log('page:', m.text()); });
p.on('pageerror', e => console.log('page error:', e.message));
p.on('response', r => { if (r.status() >= 400) console.log('page: http', r.status(), r.url()); });
await p.goto(url, { timeout: 300000 });   // a cold box compiles the page's shaders slowly
await p.waitForFunction(() => window.READY || window.FAILED, null, { timeout: 300000 });
const failed = await p.evaluate(() => window.FAILED);
if (failed) { console.error('trailer did not load:', failed); process.exit(1); }
const T = await p.evaluate(() => window.TRAILER);
const shot = (type = 'jpeg') => p.screenshot({ type, ...(type === 'jpeg' ? { quality: 95 } : {}), clip: { x: 0, y: 0, width: W, height: H } });

if (mode === 'cues') {
  writeFileSync(out, JSON.stringify(T, null, 1));
} else if (mode === 'stills') {
  mkdirSync(out, { recursive: true });
  for (const t of a.split(',').map(Number)) {
    const id = await p.evaluate(t => window.seek(t), t);
    writeFileSync(`${out}/t${t.toFixed(2)}-${id}.jpg`, await shot());
  }
} else if (mode === 'art') {
  mkdirSync(out, { recursive: true });
  for (const name of a.split(',')) {
    const ok = await p.evaluate(n => window.art(n), name);
    if (!ok) { console.error('no such art:', name); continue; }
    writeFileSync(`${out}/${name}.png`, await shot('png'));
    console.log(`art: ${out}/${name}.png`);
  }
} else if (mode === 'video') {
  const from = a ? Math.round(+a * T.FPS) : 0, to = b ? Math.round(+b * T.FPS) : Math.round(T.TOTAL * T.FPS);
  const step = +(process.env.TRAILER_STEP || 1);   // preview: every Nth frame, held N frames
  const ff = spawn('ffmpeg', ['-v', 'error', '-y', '-f', 'image2pipe', '-framerate', String(T.FPS / step), '-c:v', 'mjpeg', '-i', '-',
    '-c:v', 'libx264', '-preset', 'medium', '-crf', '12', '-pix_fmt', 'yuv420p', out], { stdio: ['pipe', 'inherit', 'inherit'] });
  const t0 = Date.now();
  for (let f = from; f < to; f += step) {
    await p.evaluate(t => window.seek(t), f / T.FPS);
    const buf = await shot();
    if (!ff.stdin.write(buf)) await new Promise(r => ff.stdin.once('drain', r));
    if (f % (300 * step) === 0) console.log(`render: frame ${f}/${to}, ${((Date.now() - t0) / 1000).toFixed(0)} s`);
  }
  ff.stdin.end();
  await new Promise(r => ff.on('close', r));
} else {
  console.error('render.mjs stills|video|cues|art …'); process.exit(2);
}
await br.close();

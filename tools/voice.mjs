// node tools/voice.mjs [all | slug…]: the announcer's lines, recorded by
// ElevenLabs as the kept takes were (text to dialogue, eleven_v4, the voice
// Kallixis, stability 0.5). The room they sit in is the voice's own: no setting,
// tag or effect makes it, so nothing is added here. The raw take goes in
// voicemusic/ (git-ignored, as the rest), and tools/voice.sh cuts it into
// crates/client/assets/sound/voice/<slug>.ogg.
//
//   node tools/voice.mjs                 # the list: nothing made, nothing spent
//   node tools/voice.mjs perfection      # a line's takes; say which to keep:
//   node tools/voice.mjs perfection 2    #   take 2 (or 0) kept, the rest deleted
//
// A take is voicemusic/<slug>_<n>.mp3. With no take named, each is measured and
// read back by Scribe (the words it heard, and how fast the last word dies
// away), to judge it without ears. One take in four or so comes out dry, its
// tail gone to -80 dB where the others hold near -30: pass it by, or it won't
// sit with the kept lines.

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const OUT = 'voicemusic';
const VOICE = 'cPoqAvGWCPfCfyPMwe4z';
const MODEL = 'eleven_v4';
const API = 'https://api.elevenlabs.io/v1';
// Takes to a line.
const TAKES = 3;

// slug (what `sound/mod.rs` and `proto::medal::slug` call it), the name
// tools/voice.sh files it under, and the text as typed: `!!` is how the kept
// ladder rungs were asked for, `...` a held pause.
const LINES = [
  ['perfection', 'Perfection', 'Perfection!!'],
  ['extermination', 'Extermination', 'Extermination!!'],
  ['slayer', 'Slayer', 'Slayer'],
  ['gun_game', 'Gun Game', 'Gun Game'],
  ['capture_the_flag', 'Capture the Flag', 'Capture the Flag'],
  ['flag_taken', 'Flag Taken', 'Flag taken!'],
  ['flag_dropped', 'Flag Dropped', 'Flag dropped!'],
  ['flag_returned', 'Flag Returned', 'Flag returned!'],
  ['flag_captured', 'Flag Captured', 'Flag captured!!'],
];

const key = () => {
  if (!process.env.ELEVENLABS_API_KEY && existsSync('.env.local')) process.loadEnvFile('.env.local');
  return process.env.ELEVENLABS_API_KEY;
};

async function record(text) {
  const r = await fetch(`${API}/text-to-dialogue?output_format=mp3_44100_128`, {
    method: 'POST',
    headers: { 'xi-api-key': key(), 'content-type': 'application/json' },
    body: JSON.stringify({ inputs: [{ text, voice_id: VOICE }], model_id: MODEL, settings: { stability: 0.5 } }),
  });
  if (!r.ok) throw new Error(`${r.status} ${await r.text()}`);
  return Buffer.from(await r.arrayBuffer());
}

async function heard(file) {
  const form = new FormData();
  form.append('model_id', 'scribe_v1');
  form.append('file', new Blob([readFileSync(file)]), 'take.mp3');
  const r = await fetch(`${API}/speech-to-text`, { method: 'POST', headers: { 'xi-api-key': key() }, body: form });
  return r.ok ? (await r.json()).text : `(${r.status})`;
}

/// How long the take runs, and how far it falls in the last 0.6 s of sound,
/// dB, 50 ms a step: the room's tail.
function tail(file) {
  const af =
    'aformat=channel_layouts=mono,areverse,silenceremove=start_periods=1:start_threshold=-60dB,atrim=0:0.6,areverse,' +
    'asetnsamples=2205,astats=metadata=1:reset=1,ametadata=print:key=lavfi.astats.Overall.RMS_level:file=-';
  const r = spawnSync('ffmpeg', ['-nostdin', '-hide_banner', '-i', file, '-af', af, '-f', 'null', '-'], { encoding: 'utf8' });
  return [...r.stdout.matchAll(/RMS_level=(-?[\d.]+|-inf)/g)].map((m) => Math.round(Number(m[1]))).join(' ');
}

const args = process.argv.slice(2);
const [slug] = args;
const keep = /^\d+$/.test(args[1] ?? '') ? args[1] : undefined;
if (!slug) {
  for (const [s, , text] of LINES) console.log(`${s.padEnd(18)} ${text}`);
} else if (keep !== undefined) {
  const line = LINES.find(([s]) => s === slug);
  const take = join(OUT, `${slug}_${keep}.mp3`);
  if (!line || !existsSync(take)) throw new Error(`no ${take}`);
  renameSync(take, join(OUT, `${slug}.mp3`));
  for (const f of readdirSync(OUT)) if (new RegExp(`^${slug}_\\d+\\.mp3$`).test(f)) rmSync(join(OUT, f));
  const cut = spawnSync('tools/voice.sh', ['cut', join(OUT, `${slug}.mp3`), line[1]], { stdio: 'inherit' });
  process.exit(cut.status ?? 1);
} else {
  const lines = LINES.filter(([s]) => args.includes('all') || args.includes(s));
  if (lines.length === 0) throw new Error(`no line ${args.join(', ')}`);
  mkdirSync(OUT, { recursive: true });
  for (const [s, , text] of lines) {
    for (let k = 0; k < TAKES; k++) {
      const file = join(OUT, `${s}_${k}.mp3`);
      writeFileSync(file, await record(text));
      console.log(`${file}  "${await heard(file)}"  tail ${tail(file)}`);
    }
  }
}

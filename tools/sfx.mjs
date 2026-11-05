// node tools/sfx.mjs [all | reel | name…]: our own sounds, made by ElevenLabs'
// sound-effects model from the prompts below: what Gates has no recording of
// (the shield's alarm, recharge, hits and pop; the alien guns, the vent and
// the charge; the engine and a rocket in flight; the wind; a grenade's
// bounce; the spawn; the countdown). Each is mono 44.1 kHz Vorbis in
// crates/client/assets/sound, cut and levelled here, written by ffmpeg.
//
//   node tools/sfx.mjs                   # the list: nothing made, nothing spent
//   node tools/sfx.mjs all               # every sound (a take is a call, and credits)
//   node tools/sfx.mjs shot_ion tink_1   # a sound's takes, or one take
//   node tools/sfx.mjs reel              # traces/sfx_reel.ogg: the files on disk, in a row
//
// No take comes out the same twice: making one again replaces it. The key is
// ELEVENLABS_API_KEY, read from .env.local (`npx @elevenlabs/cli onboard init`
// writes it). Made on a paid plan, so they are ours (NOTICE). Needs ffmpeg
// with libvorbis.

import { ElevenLabsClient } from '@elevenlabs/elevenlabs-js';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const SR = 44_100;
const OUT = 'crates/client/assets/sound';
const MODEL = 'eleven_text_to_sound_v3';
// What any paid plan may ask for (lossless at this rate is the Pro plan's).
const FORMAT = 'mp3_44100_128';
// Calls at once.
const AT_ONCE = 3;
// How closely a take follows its prompt (0 to 1): high, so a sound's takes
// are alike.
const SWAY = 0.8;
// Below this, in Hz, a take is cut: the model leaves a rumble nobody hears
// that takes the headroom. A small bright sound says `low` for more, a deep
// one for less.
const LOW_HZ = 80;
// Said after every prompt: the mixer places a sound and gives it its room.
const DRY = ' Isolated, close, dry, no reverb, no music, no voice.';

// A sound: its takes (`name_0` …), the seconds asked for (0.5 at least), the
// seconds kept from its onset (0: until it dies away), its peak in dB, and
// the prompt. `far` adds `name_far_0`, cut from take 0; `loop` is a bed the
// game holds and pitches; `low` is its own low cut.
const one = (name, takes, ask, keep, peak, text, more = {}) => ({ name, takes, ask, keep, peak, text, ...more });

const SOUNDS = [
  // The shield.
  one('shield_hit', 3, 0.5, 0.3, -3, 'A bullet stopped by a sci-fi energy shield: one short sharp electric crackle with a bright fizz, loudest at its very start.', { low: 300 }),
  one('shield_pop', 1, 0.8, 0.7, -1, 'A sci-fi energy shield breaking: one sharp electric burst with a deep thump, then a buzzing tone dropping fast in pitch as the shield collapses and fizzles out.', { low: 35 }),
  one('shield_beep', 1, 0.5, 0.09, -4, 'One single short high-pitched electronic alarm beep, a clean hard square-wave bip. One beep only.', { low: 300 }),
  one('shield_charge', 1, 1.6, 0, -3, 'An electric hum powering up: one smooth synthesizer tone gliding steadily upward in pitch from a low hum to a bright whine, growing louder as it rises. One rising glide only, no impact.', { low: 100 }),

  // The alien guns.
  one('shot_ion', 3, 0.5, 0.35, -1, 'One single shot from a sci-fi plasma rifle: a short punchy electric zap falling in pitch, with a hot sizzle. One shot only.', { far: true }),
  one('shot_ionp', 3, 0.5, 0.35, -1, 'One single shot from a small sci-fi plasma pistol: a quick bright high-pitched zap with a soft fizz. One shot only.', { far: true }),
  one('shot_ion_charged', 1, 1.0, 0, -1, 'A huge overcharged plasma bolt fired: one heavy deep booming whoomp of energy that roars and sizzles for a full second before it fades. One shot only.', { low: 35 }),
  one('charge_hold', 1, 2.0, 0, -6, 'The steady hum of a plasma weapon holding a charge: a constant buzzing electric tone with a slight tremble, never changing in pitch or loudness.', { loop: true }),
  one('shot_shards', 3, 0.5, 0.3, -1, 'One single shot from an alien crystal needle gun: a short glassy high-pitched thwip with a crystalline ping. One shot only.', { far: true, low: 300 }),
  one('shot_carbine', 3, 0.5, 0.45, -1, 'One single shot from an alien energy carbine: a sharp hard crack with a quick metallic zap and a short punchy body. One shot only.', { far: true }),
  one('shot_beam', 3, 1.2, 0, -1, 'One single shot from a sci-fi particle beam sniper rifle: a piercing crack of energy, then a searing high whine and sizzle that fades away. One shot only.', { far: true, low: 35 }),
  one('shot_cannon', 3, 1.0, 0, -1, 'One single shot from a heavy alien plasma cannon: a deep bassy thoomp throwing a bubbling ball of energy, with a low gurgle. One shot only.', { far: true, low: 35 }),
  one('shot_launcher', 3, 0.6, 0.55, -1, 'One single shot from a heavy grenade launcher: a deep hollow thunk with a metallic clank and a puff of gas. One shot only.', { far: true, low: 35 }),
  one('sword_swing', 2, 0.6, 0, -2, 'An energy sword swung once through the air: a fast humming electric whoosh of a plasma blade. One swing only.'),
  one('sword_on', 1, 0.7, 0, -3, 'An energy sword switched on: a sharp electric snap, then a quick rising hum as the plasma blade comes out.'),
  one('vent', 1, 1.2, 0, -3, 'An overheated plasma gun venting: a metal latch clacks open, then one sharp burst of hissing steam that fades.', { low: 150 }),

  // Things moving.
  one('engine', 1, 4.0, 0, -2, 'The engine of a light military off-road buggy held at a steady low speed: a constant throaty mechanical rumble at a fixed RPM. No revving, no gear changes, no tyres.', { loop: true, low: 35 }),
  one('rocket_fly', 1, 2.0, 0, -3, 'A rocket motor burning in flight: a constant roaring rush of flame with a hiss, never changing.', { loop: true, low: 35 }),
  one('wind', 1, 12.0, 0, -3, 'Soft steady wind over an open valley: a gentle airy outdoor breeze. No gusts, no birds, no leaves, nothing else.', { loop: true, low: 150 }),

  // Small things.
  one('tink', 3, 0.5, 0.25, -3, 'A heavy steel grenade casing knocking once against a concrete floor: one short solid metallic clink. One hit only.', { low: 300 }),
  one('spawn', 1, 1.0, 0, -4, 'A sci-fi respawn: a short bright shimmering chime rising as a body materializes out of energy.', { low: 200 }),
  one('tick', 1, 0.5, 0.1, -6, 'One single short soft electronic countdown beep at a middle pitch, a clean sine tone. One beep only.', { low: 300 }),
  one('go', 1, 1.0, 0.5, -4, 'One single long electronic beep, much higher in pitch than a normal beep and held for half a second, a clean bright piercing tone. One beep only.', { low: 300 }),
];

// A shot from a long way off: its highs gone, the land's echo.
const FAR = 'highpass=f=60,lowpass=f=1400,lowpass=f=1400,apad=pad_dur=0.6,aecho=1:1:170|340|510:0.4|0.16|0.064';
const FAR_PEAK = -6;

const RAW = ['-f', 'f32le', '-ar', String(SR), '-ac', '1'];
const db = (d) => 10 ** (d / 20);

/// ffmpeg, bytes in and bytes out.
function ff(args, input) {
  const r = spawnSync('ffmpeg', ['-loglevel', 'error', '-y', ...args], { input, maxBuffer: 1 << 28 });
  if (r.status !== 0) throw new Error(`ffmpeg: ${r.stderr || r.error}`);
  return r.stdout;
}

/// Any audio as mono samples at SR, through a filter.
function decode(audio, filter = 'anull', from = []) {
  const b = ff([...from, '-i', 'pipe:0', '-af', filter, ...RAW, 'pipe:1'], audio);
  return new Float32Array(b.buffer.slice(b.byteOffset, b.byteOffset + b.length));
}

const bytes = (x) => Buffer.from(x.buffer, x.byteOffset, x.byteLength);

function write(file, x, q = 4) {
  ff([...RAW, '-i', 'pipe:0', '-c:a', 'libvorbis', '-q:a', String(q), file], bytes(x));
}

function peak(x) {
  let p = 0;
  for (const v of x) p = Math.max(p, Math.abs(v));
  return p;
}

function level(x, to) {
  const p = peak(x);
  return p > 0 ? x.map((v) => (v * db(to)) / p) : x;
}

/// A take: from its onset (a trigger pulled is heard at once) until it has
/// died away, or `keep` seconds of it, faded at both ends.
function cut(x, keep) {
  const p = peak(x);
  const a = Math.max(0, x.findIndex((v) => Math.abs(v) > p * db(-20)) - Math.round(0.005 * SR));
  let b = x.findLastIndex((v) => Math.abs(v) > p * db(-45)) + 1;
  if (keep) b = Math.min(b, a + Math.round(keep * SR));
  const y = x.slice(a, b);
  const i = Math.round(0.002 * SR);
  const o = Math.round(Math.min(0.08 * SR, y.length * 0.3));
  for (let k = 0; k < i; k++) y[k] *= k / i;
  for (let k = 0; k < o; k++) y[y.length - 1 - k] *= k / o;
  return y;
}

/// A loop: its silent ends off (an MP3's padding), then the tail crossfaded
/// (equal power) into the head and cut, so the wrap is the loop point.
function seam(x) {
  const a = x.findIndex((v) => Math.abs(v) > 1e-3);
  const b = x.findLastIndex((v) => Math.abs(v) > 1e-3) + 1;
  x = x.slice(a, b);
  const k = Math.round(Math.min(SR, x.length * 0.1));
  const y = x.slice(0, x.length - k);
  for (let i = 0; i < k; i++) {
    const u = (i / k) * (Math.PI / 2);
    y[i] = y[i] * Math.sin(u) + x[x.length - k + i] * Math.cos(u);
  }
  return y;
}

/// A sound's files, by name.
const names = (s) => [...Array.from({ length: s.takes }, (_, k) => `${s.name}_${k}`), ...(s.far ? [`${s.name}_far_0`] : [])];

/// Credits the calls cost, as each one's reply says.
let credits = 0;

async function make(client, s, k) {
  const { data, rawResponse } = await client.textToSoundEffects
    .convert({
      text: s.text + DRY,
      modelId: MODEL,
      durationSeconds: s.ask,
      promptInfluence: SWAY,
      loop: !!s.loop,
      outputFormat: FORMAT,
    })
    .withRawResponse();
  credits += Number(rawResponse.headers.get('character-cost') ?? 0);
  const chunks = [];
  for await (const c of data) chunks.push(c);
  const low = `highpass=f=${s.low ?? LOW_HZ}`;
  const x = decode(Buffer.concat(chunks), `${low},${low}`);
  const y = level(s.loop ? seam(x) : cut(x, s.keep), s.peak);
  write(join(OUT, `${s.name}_${k}.ogg`), y);
  console.log(`${s.name}_${k}  ${(y.length / SR).toFixed(2)} s`);
  if (s.far && k === 0) {
    const f = level(cut(decode(bytes(y), FAR, RAW), 0), FAR_PEAK);
    write(join(OUT, `${s.name}_far_0.ogg`), f);
    console.log(`${s.name}_far_0  ${(f.length / SR).toFixed(2)} s`);
  }
}

/// Every file on disk in a row, to listen through.
function reel() {
  const gap = new Float32Array(Math.round(0.35 * SR));
  const parts = [];
  let at = 0;
  for (const n of SOUNDS.flatMap(names)) {
    const x = level(decode(readFileSync(join(OUT, `${n}.ogg`))), -3);
    console.log(`${(at / SR).toFixed(1).padStart(6)} s  ${n}`);
    parts.push(x, gap);
    at += x.length + gap.length;
  }
  const all = new Float32Array(at);
  let i = 0;
  for (const p of parts) {
    all.set(p, i);
    i += p.length;
  }
  mkdirSync('traces', { recursive: true });
  write('traces/sfx_reel.ogg', all, 5);
  console.log('traces/sfx_reel.ogg');
}

const args = process.argv.slice(2);
if (args.length === 0) {
  for (const s of SOUNDS) console.log(`${names(s).join(' ')}\n    ${s.text}`);
  console.log(`\n${SOUNDS.flatMap(names).length} sounds; node tools/sfx.mjs all | reel | name…`);
} else if (args[0] === 'reel') {
  reel();
} else {
  const jobs = SOUNDS.flatMap((s) =>
    Array.from({ length: s.takes }, (_, k) => [s, k]).filter(
      ([, k]) => args.includes('all') || args.includes(s.name) || args.includes(`${s.name}_${k}`),
    ),
  );
  const known = new Set(['all', ...SOUNDS.flatMap((s) => [s.name, ...names(s)])]);
  const unknown = args.filter((a) => !known.has(a));
  if (unknown.length > 0 || jobs.length === 0) {
    console.error(`no sound ${unknown.join(', ') || args.join(', ')} (a far take is made with its take 0)`);
    process.exit(1);
  }
  if (!process.env.ELEVENLABS_API_KEY && existsSync('.env.local')) process.loadEnvFile('.env.local');
  const client = new ElevenLabsClient();
  await Promise.all(
    Array.from({ length: AT_ONCE }, async () => {
      for (let j; (j = jobs.shift()); ) await make(client, ...j);
    }),
  );
  console.log(`${OUT}: ${credits} credits`);
}

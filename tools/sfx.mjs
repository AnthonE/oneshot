// node tools/sfx.mjs [all | reel | check | cut | name…]: every sound effect in
// the game, made by ElevenLabs' sound-effects model from the prompts below. Each
// is mono 44.1 kHz Vorbis in crates/client/assets/sound, cut and levelled here,
// written by ffmpeg; what the model gave is kept in sources/sfx (git-ignored).
//
//   node tools/sfx.mjs                   # the list: nothing made, nothing spent
//   node tools/sfx.mjs all               # every sound (a take is a call, and credits)
//   node tools/sfx.mjs shot_ion tink_1   # a sound's takes, or one take
//   node tools/sfx.mjs check [name…]     # each file measured, what looks wrong marked
//   node tools/sfx.mjs cut [name…]       # cut and level again from sources/sfx: free
//   node tools/sfx.mjs reel              # traces/sfx_reel.ogg: the files on disk, in a row
//
// No take comes out the same twice: making one again replaces it. The key is
// ELEVENLABS_API_KEY, read from .env.local (`npx @elevenlabs/cli onboard init`
// writes it). Made on a paid plan, so they are ours (NOTICE). Needs ffmpeg
// with libvorbis.

import { ElevenLabsClient } from '@elevenlabs/elevenlabs-js';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const SR = 44_100;
const OUT = 'crates/client/assets/sound';
const RAWS = 'sources/sfx';
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
// The same for a body's own noises, which are a voice but no words.
const DRY_BODY = ' Isolated, close, dry, no reverb, no music, no words.';

// A sound: its takes (`name_0` …), the seconds asked for (0.5 at least), the
// seconds kept from its onset (0: until it dies away), its peak in dB, and
// the prompt. `far` adds `name_far_0`, cut from take 0 (`true`, or its own peak);
// `loop` is a bed the game holds and pitches; `low` is its own low cut; `push` is
// dB more through a limiter, for level a peak can't give; `body` a body's noise, a
// voice in it; `many` more than one event by design, which `check` lets by.
const one = (name, takes, ask, keep, peak, text, more = {}) => ({ name, takes, ask, keep, peak, text, ...more });

const SOUNDS = [
  // The guns.
  one('shot_rifle', 3, 0.5, 0.4, -5, 'One single shot from a heavy military assault rifle: a hard sharp crack with a snappy mechanical punch and a quick metallic ring. One shot only.', { far: -6 }),
  one('shot_br', 3, 0.6, 0.5, -2, 'One single round from a heavy battle rifle: a deep powerful crack with a thump in the chest and a short hard tail. One shot only.', { far: -4 }),
  one('shot_magnum', 3, 0.8, 0.7, -1, 'One single shot from a huge magnum handgun: a loud booming crack with a hard punch and a short ringing tail. One shot only.', { far: -3, low: 50, push: 6 }),
  one('shot_sniper', 3, 1.0, 0, -3, 'One single shot from a large anti-materiel sniper rifle: an enormous sharp supersonic crack with a deep boom and a metallic ring that fades. One shot only.', { far: -6, low: 35 }),
  one('shot_shotgun', 3, 0.9, 0.8, -2, 'One single blast from a heavy combat shotgun: a wide thunderous boom with a hard punch and a deep low tail. One blast only.', { far: -6, low: 35 }),
  one('shot_rocket', 3, 1.1, 0, -4, 'A shoulder rocket launcher firing one rocket: a huge deep thump and blast at its very start, then a roaring whoosh of flame trailing away. One shot only.', { far: -6, low: 35, many: true }),
  one('shot_smg', 3, 0.5, 0.3, -3, 'One single round from a compact submachine gun: a fast light snappy crack with a quick mechanical clack. One shot only.', { far: -6 }),

  one('shot_minigun', 3, 0.5, 0.2, -5, 'One single round from a heavy rotary minigun on a truck: a very short hard metallic crack with a fast mechanical buzz. One shot only.', { far: -6 }),

  // The reloads, each as long as its gun's (`arms.rs`), played once and never cut off.
  one('reload_rifle', 1, 1.8, 1.76, -2, 'A soldier reloading an assault rifle: an empty magazine clicks out, a full magazine slaps in, the charging handle snaps back. Three quick metal sounds. No gunshot.', { low: 150, many: true, push: 4 }),
  one('reload_br', 1, 1.8, 1.76, -2, 'A soldier reloading a heavy battle rifle: a long magazine latch clacks open, a full magazine is rammed in with a solid thunk, a heavy bolt snaps forward. No gunshot.', { low: 150, many: true, push: 5 }),
  one('reload_magnum', 1, 1.3, 1.28, -2, 'A soldier reloading a big magnum handgun: the empty magazine drops out with a click, a new one slides into the grip, the slide snaps forward. No gunshot.', { low: 150, many: true, push: 3 }),
  one('reload_sniper', 1, 1.9, 1.92, -2, 'A soldier reloading a big sniper rifle: a heavy bolt pulled back, a heavy magazine locked in with a solid clank, the bolt shoved home. No gunshot.', { low: 150, many: true, push: 6 }),
  one('reload_shotgun', 1, 2.0, 2.0, -2, 'A soldier reloading a combat shotgun: heavy shells pushed into the tube with quick solid clicks, then the pump racked back and forward. No gunshot.', { low: 150, many: true, push: 5 }),
  one('reload_rocket', 1, 2.0, 2.0, -2, 'A soldier reloading a rocket launcher: a heavy breech latch swings open, a long heavy rocket slides in with a scrape and a deep clunk, the latch locks shut. No gunshot.', { low: 100, many: true, push: 2 }),
  one('reload_smg', 1, 1.6, 1.6, -2, 'A soldier reloading a small submachine gun: a light magazine clicks out, a new one clicks in, the small bolt snaps back. No gunshot.', { low: 150, many: true, push: 8 }),
  one('reload_carbine', 1, 1.8, 1.8, -2, 'An alien energy carbine being reloaded: an alien metal clasp clacks open, a glowing power cell slides in with a soft electric hum, the clasp clicks shut. No gunshot.', { low: 150, many: true, push: 3 }),
  one('reload_shards', 1, 2.0, 2.0, -2, 'An alien crystal gun being reloaded: a spent crystal cluster snaps out, a new cluster of glassy shards slides in with a tinkle and a soft hum, and locks. No gunshot.', { low: 150, many: true, push: 3 }),
  one('reload_launcher', 1, 2.2, 2.2, -3, 'A heavy alien grenade launcher being reloaded: a heavy drum unlatches with a clunk, heavy rounds drop in with hollow knocks, the drum slams shut. No gunshot.', { low: 100, many: true }),
  one('reload_cannon', 1, 2.4, 2.4, -3, 'A heavy alien plasma cannon being reloaded: a big hatch hisses open, a heavy power cell slides in with a deep clunk, the cannon hums back up. No gunshot.', { low: 80, many: true }),

  // What a gun does in the hands, and a fist.
  one('dry_fire', 1, 0.5, 0.25, -2, "An empty gun's trigger pulled: one small dry mechanical metal click. One click only.", { low: 300, push: 4 }),
  one('swap', 3, 0.6, 0.4, -4, 'A soldier swapping guns: a quick rustle of a sling, a small metal latch click, a gun settling into the grip. No gunshot.', { low: 150, many: true }),
  one('pickup', 2, 0.6, 0.45, -2, 'A soldier grabbing a heavy gun off a metal rack: a firm grab of the grip and one quick metal latch clack. No gunshot.', { low: 150, many: true, push: 2 }),
  one('grenades', 2, 0.5, 0.35, -4, 'Hand grenades clipped onto a webbing belt: two small heavy metal clinks and a snap. No explosion.', { low: 200, many: true }),
  one('gren_switch', 1, 0.5, 0.15, -6, 'A small metal selector switch flicked on a belt: one small dry click. One click only.', { low: 400 }),
  one('scope_in', 1, 0.5, 0.25, -6, 'A rifle scope zooming in: a quick small electronic servo whirr rising in pitch, ending on a soft click.', { low: 400, many: true }),
  one('scope_out', 1, 0.5, 0.2, -6, 'A rifle scope zooming out: a quick small electronic servo whirr falling in pitch, ending on a soft click.', { low: 400, many: true }),
  one('swing', 3, 0.5, 0.3, -2, 'A fast armoured arm swung hard through the air: one short heavy whoosh with a slight cloth rustle. No impact.', { low: 150, push: 3 }),
  one('melee_hit', 3, 0.5, 0.35, -2, 'A brutal melee blow: one heavy meaty thud of a rifle butt into a metal chest plate, with a deep thump and a dull clank. One hit only.', { low: 60 }),
  one('throw', 3, 0.6, 0.4, -4, 'A soldier throwing a grenade hard: a quick rustle of gear and one short fast whoosh of an arm. No explosion.', { low: 150 }),

  // Rounds landing, and what they tell us.
  one('hit', 1, 0.5, 0.07, -2, 'One single very short crisp dry electronic tick, like the tiny click of a relay. One tick only.', { low: 1000, push: 3 }),
  one('headshot', 2, 0.5, 0.4, -3, 'A bullet striking a metal helmet: one sharp high bright metallic ping with a hard crack. One hit only.', { low: 300 }),
  one('flesh', 3, 0.5, 0.25, -2, 'A bullet hitting an armoured body: one short wet meaty thwack with a dull thud. One hit only.', { low: 120 }),
  one('flyby', 4, 0.5, 0.3, -2, 'A bullet whizzing past the ear: one very fast sharp snap and zip going by. One pass only.', { low: 300, push: 3 }),
  one('ricochet', 4, 0.5, 0.45, -6, 'A bullet ricocheting off rock: one sharp high-pitched metallic ping whining down in pitch. One ricochet only.', { low: 300 }),
  one('impact_metal', 5, 0.5, 0.45, -3, 'A heavy metal panel struck hard: one loud short clang with a hard attack and a dull ring. One hit only.', { low: 100 }),
  one('bullet_concrete', 3, 0.5, 0.25, -4, 'A bullet hitting a concrete wall: one short sharp crack and a spray of chips and dust. One hit only.', { low: 200 }),
  one('bullet_metal', 3, 0.5, 0.25, -4, 'A bullet hitting a steel plate: one short sharp metallic spang. One hit only.', { low: 200 }),
  one('bullet_dirt', 3, 0.5, 0.25, -4, 'A bullet hitting packed dirt and sand: one short dull thwip with a spray of grit. One hit only.', { low: 150 }),
  one('bolt_hit', 2, 0.5, 0.3, -4, 'A small plasma bolt hitting a wall: one short hot electric splat and sizzle. One hit only.', { low: 200 }),
  one('blast', 3, 3.0, 2.6, -1, 'A frag grenade exploding: one instant hard crack and boom with a deep rolling blast and debris pattering away. One explosion only.', { low: 30, many: true }),

  // A body.
  one('step_concrete', 8, 0.5, 0.3, -3, 'One single heavy combat boot footfall on hard concrete: one short firm dry thud of the sole, no scuff and no second sound. One step only.', { low: 100 }),
  one('step_metal', 8, 0.5, 0.35, -3, 'One single heavy combat boot footstep on a steel deck plate: a short resonant metallic clank with a low ring. One step only.', { low: 100 }),
  one('step_grass', 8, 0.5, 0.4, -3, 'One single heavy boot footstep on short grass and soft earth: a soft muffled crunch with a rustle of blades. One step only.', { low: 100 }),
  one('step_rock', 8, 0.5, 0.35, -3, 'One single heavy boot footstep on bare rock with loose grit: a hard dull tap with a small scrape of stones. One step only.', { low: 100 }),
  one('step_sand', 8, 0.5, 0.4, -3, 'One single heavy boot footstep in dry sand: a soft grainy crunch and a short slide. One step only.', { low: 100 }),
  one('step_gravel', 8, 0.5, 0.4, -1, 'One single heavy boot footstep on loose gravel: a sharp crunch of small stones shifting. One step only.', { low: 100, push: 3 }),
  one('step_water', 8, 0.5, 0.45, -3, 'One single heavy boot stepping into shallow water: a short splash and a slosh. One step only.', { low: 100, many: true }),
  one('jump', 2, 0.5, 0.3, -5, 'An armoured soldier pushing off the ground to jump: a quick boot scuff and a light rattle of gear. No landing.', { low: 150 }),
  one('land', 2, 0.5, 0.3, -3, "An armoured soldier's boots landing on the ground after a jump: one solid low thud and a small rattle of gear.", { low: 60 }),
  one('launch', 1, 1.2, 0, -3, 'A powerful launch pad throwing a soldier into the air: one deep energetic electric whoomp rising fast into a rush of air.', { low: 60, many: true }),
  one('hurt', 2, 0.5, 0.4, -4, 'One short deep strained adult male pain grunt through clenched teeth, muffled inside a helmet.', { low: 100, body: true }),
  one('death', 3, 1.5, 0, -1, 'One short deep male death groan inside a helmet as a soldier falls, then right away a heavy armoured body hitting the ground with a clatter of metal.', { low: 60, body: true, many: true, push: 3 }),
  one('splat', 2, 0.6, 0.5, -2, 'A speeding truck running a soldier down: one sickening heavy meaty thud with crunching metal armour. One hit only.', { low: 60, many: true }),

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
  // The sticky: what it sounds like flying and on what it is stuck to, and it landing.
  one('sticky_sizzle', 1, 3.0, 0, -3, 'A fierce high-voltage electric arc burning without stopping: a loud full buzzing electrical crackle like a welding arc, a deep angry electric hum under a sharp frying sizzle and random violent snaps. Constant, never changing, never fading.', { loop: true, low: 90 }),
  one('sticky_stick', 1, 0.6, 0.45, -2, 'A blob of molten plasma hitting metal: one sharp bright wet spit and hot fizz, like water thrown on a red-hot pan. Quick, no low thud.', { low: 500 }),
  one('sticky_whine', 1, 1.2, 0, -5, 'An electronic energy whine rising steadily in pitch for one second, thin and high and getting faster and more urgent, ending abruptly at its peak. One rising tone only.', { low: 400 }),
  one('rocket_fly', 1, 2.0, 0, -3, 'A rocket motor burning in flight: a constant roaring rush of flame with a hiss, never changing.', { loop: true, low: 35 }),
  one('wind', 1, 12.0, 0, -3, 'Soft steady wind over an open valley: a gentle airy outdoor breeze. No gusts, no birds, no leaves, nothing else.', { loop: true, low: 150 }),
  one('surf', 1, 12.0, 0, -3, 'Ocean waves breaking steadily on a sandy beach from a short way off: a constant gentle wash and roll. No gulls, no wind, nothing else.', { loop: true, low: 120 }),
  one('room_hum', 1, 10.0, 0, -3, 'The low steady hum inside a large spacecraft: constant deep machinery rumble with soft air vents, never changing. No beeps, nothing else.', { loop: true, low: 40 }),

  // The truck.
  one('truck_land', 2, 0.8, 0.65, -1, 'A pickup truck slamming down on its suspension after a jump: one heavy metal crash with a deep thump and rattling springs.', { low: 40, many: true, push: 3 }),
  one('crash', 1, 1.0, 0.9, -1, 'A truck crashing hard into a wall at speed: one massive metal crunch of bending steel and a rattle of falling pieces.', { low: 40, many: true, push: 7 }),
  one('skid', 1, 3.0, 0, -1.5, 'Tyres skidding sideways on dirt and gravel at speed: one constant unbroken harsh scrape and spray of grit at a steady level, with no pauses and no changes. No engine.', { loop: true, low: 150, push: 3 }),
  one('fire', 1, 4.0, 0, -3, 'A big vehicle fire burning: steady crackling and roaring flames, never changing. Nothing else.', { loop: true, low: 80 }),
  one('ride', 2, 0.7, 0.55, -2, 'A soldier climbing into the seat of a light off-road truck: a boot on a metal step, a heavy body dropping into a seat, the springs creaking.', { low: 100, many: true }),

  // Power-ups.
  one('over_on', 1, 1.2, 0, -3, 'A powerful energy overshield switching on around a body: one deep rising electric surge with a bright shimmer that swells and settles.', { low: 60 }),
  one('camo_on', 1, 1.0, 0, -4, 'A cloaking field switching on: one soft glassy airy shimmer sweeping down and fading away to silence.', { low: 200 }),

  // The menus.
  one('ui_hover', 1, 0.5, 0.06, -12, 'One very soft tiny clean electronic tick from a game menu: one short subtle blip.', { low: 800 }),
  one('ui_click', 1, 0.5, 0.15, -6, 'One short soft electronic blip around 900 hertz, rounded and warm like a muted synth pluck, lasting a tenth of a second, with no noise. One blip only.', { low: 300 }),
  one('match_found', 1, 1.5, 0, -4, 'A sci-fi alert that a match has been found: three clean rising synthesizer notes, pure bright tones with no noise, confident and short.', { low: 100, many: true }),
  one('invite', 1, 0.8, 0, -6, 'A soft friendly notification: one short two-note electronic chime.', { low: 300, many: true }),

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

/// `d` dB louder for the same peak, as a recording mastered hot is.
const pushed = (x, d) => decode(bytes(x), `volume=${d}dB,alimiter=limit=0.89:attack=1:release=40:level=disabled`, RAW);

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
      text: s.text + (s.body ? DRY_BODY : DRY),
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
  const raw = Buffer.concat(chunks);
  mkdirSync(RAWS, { recursive: true });
  writeFileSync(join(RAWS, `${s.name}_${k}.mp3`), raw);
  finish(s, k, raw);
}

/// What the model gave, as the game has it.
function finish(s, k, raw) {
  const low = `highpass=f=${s.low ?? LOW_HZ}`;
  const x = decode(raw, `${low},${low}`);
  const whole = s.loop ? seam(x) : cut(x, s.keep);
  const y = level(s.push ? pushed(whole, s.push) : whole, s.peak);
  write(join(OUT, `${s.name}_${k}.ogg`), y);
  console.log(`${s.name}_${k}  ${(y.length / SR).toFixed(2)} s`);
  if (s.far && k === 0) {
    const f = level(cut(decode(bytes(y), FAR, RAW), 0), s.far === true ? FAR_PEAK : s.far);
    write(join(OUT, `${s.name}_far_0.ogg`), f);
    console.log(`${s.name}_far_0  ${(f.length / SR).toFixed(2)} s`);
  }
}

/// A file's numbers: what can be told of a clip without ears. Its length; how
/// long it takes to reach its peak; how many separate events are in it (the
/// level falling 20 dB under its loudest for 30 ms, then coming back); how bright
/// it is (zero crossings, in Hz); its level, and the most a loop's level moves
/// between its quarters.
function measure(n) {
  const x = decode(readFileSync(join(OUT, `${n}.ogg`)));
  const frame = Math.round(0.01 * SR);
  const rms = [];
  for (let i = 0; i + frame <= x.length; i += frame) {
    let e = 0;
    for (let j = i; j < i + frame; j++) e += x[j] * x[j];
    rms.push(Math.sqrt(e / frame));
  }
  const top = Math.max(...rms, 1e-9);
  const attack = rms.findIndex((v) => v > top * db(-3)) * 10;
  let events = 0;
  let quiet = 3;
  for (const v of rms) {
    if (v < top * db(-20)) quiet++;
    else {
      if (quiet >= 3) events++;
      quiet = 0;
    }
  }
  let zc = 0;
  for (let i = 1; i < x.length; i++) if (x[i] > 0 !== x[i - 1] > 0) zc++;
  const part = Math.max(1, Math.floor(rms.length / 4));
  const quarters = [0, 1, 2, 3].map((q) => {
    const r = rms.slice(q * part, (q + 1) * part);
    return 20 * Math.log10(Math.sqrt(r.reduce((a, v) => a + v * v, 0) / Math.max(1, r.length)) + 1e-9);
  });
  const all = 20 * Math.log10(Math.sqrt(rms.reduce((a, v) => a + v * v, 0) / Math.max(1, rms.length)) + 1e-9);
  return {
    secs: x.length / SR,
    attack,
    events,
    bright: Math.round((zc / 2 / x.length) * SR),
    level: all,
    moves: Math.max(...quarters) - Math.min(...quarters),
  };
}

/// Each take's numbers, and a mark on what looks wrong for what it is.
function check(wanted) {
  for (const s of SOUNDS.filter(wanted)) {
    for (const n of names(s)) {
      const m = measure(n);
      const far = n.includes('_far_');
      const bad = [];
      if (s.loop && m.moves > 4) bad.push('unsteady');
      if (!s.loop && !s.many && !far && m.events > 1) bad.push(`${m.events} events`);
      if (!s.loop && !s.many && !far && s.keep && s.keep < 1 && m.attack > 80) bad.push('slow attack');
      if (s.keep && m.secs < s.keep * 0.4) bad.push('short');
      console.log(
        `${n.padEnd(22)} ${m.secs.toFixed(2).padStart(6)} s  attack ${String(m.attack).padStart(4)} ms  ` +
          `${String(m.events).padStart(2)} ev  ${String(m.bright).padStart(5)} Hz  ${m.level.toFixed(1).padStart(6)} dB  ` +
          `${s.loop ? `moves ${m.moves.toFixed(1)}  ` : ''}${bad.length ? `<< ${bad.join(', ')}` : ''}`,
      );
    }
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
} else if (args[0] === 'check') {
  const want = args.slice(1);
  check((s) => want.length === 0 || want.includes(s.name));
} else if (args[0] === 'cut') {
  const want = args.slice(1);
  for (const s of SOUNDS.filter((s) => want.length === 0 || want.includes(s.name))) {
    for (let k = 0; k < s.takes; k++) {
      const raw = join(RAWS, `${s.name}_${k}.mp3`);
      if (existsSync(raw)) finish(s, k, readFileSync(raw));
      else console.log(`${s.name}_${k}: nothing in ${RAWS}, left as it is`);
    }
  }
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

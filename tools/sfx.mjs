// node tools/sfx.mjs [all | reel | check | cut | name…]: every sound effect in
// the game, made by ElevenLabs' sound-effects model from the prompts below. Each
// is mono 44.1 kHz Vorbis in crates/client/assets/sound, cut and levelled here,
// written by ffmpeg; what the model gave is kept in sources/sfx (git-ignored).
//
//   node tools/sfx.mjs                   # the list: nothing made, nothing spent
//   node tools/sfx.mjs all               # every sound (a take is up to ROLLS calls, and credits)
//   node tools/sfx.mjs shot_ion tink_1   # a sound's takes, or one take
//   node tools/sfx.mjs check [name…]     # each file measured, what looks wrong marked
//   node tools/sfx.mjs cut [name…]       # cut and level again from sources/sfx: free
//                                        #   (a sound with nothing there: the file the game has is put there first)
//   node tools/sfx.mjs reel              # traces/sfx_reel.ogg: the files on disk, in a row
//
// No take comes out the same twice: making one again replaces it. A take is
// the best of up to ROLLS calls (`fit`: loud enough from the model, struck at
// once, one event; a gun's shot neither thin nor dull). The key is
// ELEVENLABS_API_KEY, read from .env.local (`npx @elevenlabs/cli onboard init`
// writes it). Made on a paid plan, so they are ours (NOTICE). Needs ffmpeg
// with libvorbis.

import { ElevenLabsClient } from '@elevenlabs/elevenlabs-js';
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
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
// A gun firing bullets is not asked for dry: told so, the model gives a quiet
// pop with no bottom and nothing over 8 kHz (as it does asked for a
// "recording"). Its row says "close up" before what it sounds like.
const SHOT = ' Full frequency range, crisp and powerful. No music, no voice.';
// Calls a take may cost: the first that fits is kept, else the best of them.
const ROLLS = 4;
// A take the model gave quieter than this (dB, its peak) is an MP3 of almost
// nothing: brought up, its grit is heard.
const FAINT = -18;

// A sound: its takes (`name_0` …), the seconds asked for (0.5 at least; asked
// for under a second, a small sound mostly comes back faint), the seconds kept
// from its onset (0: until it dies away), its peak in dB, and
// the prompt. `far` adds `name_far_0`, cut from take 0 (`true`, or its own peak);
// `loop` is a bed the game holds and pitches; `low` is its own low cut; `push` is
// dB more through a limiter, for level a peak can't give; `body` a body's noise, a
// voice in it; `many` more than one event by design, which `check` lets by.
// `loud` is how loud a hit is mastered (`loudness`, its loudest 200 ms), driven into a
// soft clipper under its peak: a burst of noise takes that as a recording made
// hot does, where a limiter pumps. `shot` is a gun firing bullets: asked for
// with SHOT, and a take with no bottom or no top is rolled again. `thump` is
// laid under it, ours ([Hz it falls to, dB under the peak, ms it dies in]);
// `shape` is the blow it is made into ([dB down after its first 50 ms, dB more
// by its end]).
const one = (name, takes, ask, keep, peak, text, more = {}) => ({ name, takes, ask, keep, peak, text, ...more });

const SOUNDS = [
  // The guns. One round of an automatic is mastered quieter than a gun that
  // fires once: its rounds pile up.
  one('shot_rifle', 3, 1.0, 0.4, -4, 'A single gunshot from a heavy military assault rifle, close up: a hard sharp bright crack with a punchy low thump and a snappy mechanical clack. One shot only.', { far: -6, shot: true, loud: -16.5, thump: [70, -12, 45], shape: [6, 14] }),
  one('shot_br', 3, 1.0, 0.5, -3.5, 'A single gunshot from a heavy battle rifle, close up: a deep powerful bright crack with a hard bass punch felt in the chest and a short hard tail. One shot only.', { far: -4, shot: true, loud: -16, thump: [62, -9, 55], shape: [6, 14] }),
  one('shot_magnum', 3, 1.0, 0.7, -1, 'A single gunshot from a huge .50 caliber magnum handgun, close up: an explosive sharp bright crack with a massive bass thump felt in the chest, then a short roar dying away fast. One shot only.', { far: -3, low: 50, shot: true, loud: -11.5, thump: [58, -7, 70], shape: [5, 14] }),
  one('shot_sniper', 3, 1.2, 0, -1, 'A single gunshot from a large anti-materiel sniper rifle, close up: an enormous sharp bright supersonic crack with a huge deep boom, then a roar rolling away. One shot only.', { far: -4, low: 35, shot: true, loud: -11, thump: [48, -6, 110], shape: [4, 10] }),
  one('shot_shotgun', 3, 1.0, 0.8, -1, 'A single gunshot from a heavy 12-gauge pump-action combat shotgun, close up: a huge sharp bright crack with a wide thunderous boom under it, then a roar dying away. One shot only.', { far: -4, low: 35, shot: true, loud: -10.5, thump: [52, -6, 95], shape: [3, 10] }),
  one('shot_rocket', 3, 1.1, 0, -1, 'A shoulder rocket launcher firing one rocket: a huge deep thump and blast at its very start, then a roaring whoosh of flame trailing away. One shot only.', { far: -6, low: 35, many: true, loud: -12.5 }),
  one('shot_smg', 3, 1.0, 0.3, -4, 'A single gunshot from a compact submachine gun, close up: a fast tight snappy bright crack with a small punchy thump and a quick mechanical clack. One shot only.', { far: -6, shot: true, loud: -17.5, thump: [85, -14, 35], shape: [6, 12] }),

  one('shot_minigun', 3, 1.0, 0.2, -4, 'A single gunshot from a heavy rotary minigun on a truck, close up: a very short hard bright crack with a punchy thump and a fast mechanical buzz. One shot only.', { far: -6, shot: true, loud: -17.5, thump: [75, -12, 35], shape: [5, 10] }),

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
  one('grenades', 2, 1.0, 0.35, -4, 'Hand grenades clipped onto a webbing belt: two small heavy metal clinks and a snap. No explosion.', { low: 200, many: true }),
  one('gren_switch', 1, 0.5, 0.15, -6, 'A small metal selector switch flicked on a belt: one small dry click. One click only.', { low: 400 }),
  one('scope_in', 1, 0.5, 0.25, -6, 'A rifle scope zooming in: a quick small electronic servo whirr rising in pitch, ending on a soft click.', { low: 400, many: true }),
  one('scope_out', 1, 0.5, 0.2, -6, 'A rifle scope zooming out: a quick small electronic servo whirr falling in pitch, ending on a soft click.', { low: 400, many: true }),
  one('swing', 3, 0.5, 0.3, -2, 'A fast armoured arm swung hard through the air: one short heavy whoosh with a slight cloth rustle. No impact.', { low: 150, push: 3 }),
  one('melee_hit', 3, 0.5, 0.35, -2, 'A brutal melee blow: one heavy meaty thud of a rifle butt into a metal chest plate, with a deep thump and a dull clank. One hit only.', { low: 60, loud: -13.5, thump: [60, -5, 80] }),
  one('throw', 3, 0.6, 0.4, -4, 'A soldier throwing a grenade hard: a quick rustle of gear and one short fast whoosh of an arm. No explosion.', { low: 150 }),

  // Rounds landing, and what they tell us.
  one('hit', 1, 0.5, 0.07, -2, 'One single very short crisp dry electronic tick, like the tiny click of a relay. One tick only.', { low: 1000, push: 3 }),
  one('headshot', 2, 0.5, 0.4, -3, 'A bullet striking a metal helmet: one sharp high bright metallic ping with a hard crack. One hit only.', { low: 300 }),
  one('flesh', 3, 1.0, 0.25, -2, 'A bullet hitting an armoured body: one short wet meaty thwack with a dull thud. One hit only.', { low: 120 }),
  one('flyby', 4, 0.5, 0.3, -2, 'A bullet whizzing past the ear: one very fast sharp snap and zip going by. One pass only.', { low: 300, push: 3 }),
  one('ricochet', 4, 1.0, 0.45, -6, 'A bullet ricocheting off rock: one sharp high-pitched metallic ping whining down in pitch. One ricochet only.', { low: 300 }),
  one('impact_metal', 5, 1.0, 0.45, -3, 'A heavy metal panel struck hard: one loud short clang with a hard attack and a dull ring. One hit only.', { low: 100 }),
  one('bullet_concrete', 3, 1.0, 0.25, -4, 'A bullet hitting a concrete wall: one short sharp crack and a spray of chips and dust. One hit only.', { low: 200 }),
  one('bullet_metal', 3, 1.0, 0.25, -4, 'A bullet hitting a steel plate: one short sharp metallic spang. One hit only.', { low: 200 }),
  one('bullet_dirt', 3, 1.0, 0.25, -4, 'A bullet hitting packed dirt and sand: one short dull thwip with a spray of grit. One hit only.', { low: 150 }),
  one('bolt_hit', 2, 0.5, 0.3, -4, 'A small plasma bolt hitting a wall: one short hot electric splat and sizzle. One hit only.', { low: 200 }),
  one('blast', 3, 3.0, 2.6, -1, 'A frag grenade exploding: one instant hard crack and boom with a deep rolling blast and debris pattering away. One explosion only.', { low: 30, many: true, loud: -11 }),

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
  one('shield_pop', 1, 0.8, 0.7, -1, 'A sci-fi energy shield breaking: one sharp electric burst with a deep thump, then a buzzing tone dropping fast in pitch as the shield collapses and fizzles out.', { low: 35, loud: -11.5 }),
  one('shield_beep', 1, 0.5, 0.09, -4, 'One single short high-pitched electronic alarm beep, a clean hard square-wave bip. One beep only.', { low: 300 }),
  one('shield_charge', 1, 1.6, 0, -3, 'An electric hum powering up: one smooth synthesizer tone gliding steadily upward in pitch from a low hum to a bright whine, growing louder as it rises. One rising glide only, no impact.', { low: 100 }),

  // The alien guns.
  one('shot_ion', 3, 0.5, 0.35, -1, 'One single shot from a sci-fi plasma rifle: a short punchy electric zap falling in pitch, with a hot sizzle. One shot only.', { far: true, loud: -15.5 }),
  one('shot_ionp', 3, 0.5, 0.35, -1, 'One single shot from a small sci-fi plasma pistol: a quick bright high-pitched zap with a soft fizz. One shot only.', { far: true, loud: -16 }),
  one('shot_ion_charged', 1, 1.0, 0, -1, 'A huge overcharged plasma bolt fired: one heavy deep booming whoomp of energy that roars and sizzles for a full second before it fades. One shot only.', { low: 35, loud: -12 }),
  one('charge_hold', 1, 2.0, 0, -6, 'The steady hum of a plasma weapon holding a charge: a constant buzzing electric tone with a slight tremble, never changing in pitch or loudness.', { loop: true }),
  one('shot_shards', 3, 0.5, 0.3, -1, 'One single shot from an alien crystal needle gun: a short glassy high-pitched thwip with a crystalline ping. One shot only.', { far: true, low: 300, loud: -16 }),
  one('shot_carbine', 3, 0.5, 0.45, -1, 'One single shot from an alien energy carbine: a sharp hard crack with a quick metallic zap and a short punchy body. One shot only.', { far: true, loud: -14.5 }),
  one('shot_beam', 3, 1.2, 0, -1, 'One single shot from a sci-fi particle beam sniper rifle: a piercing crack of energy, then a searing high whine and sizzle that fades away. One shot only.', { far: true, low: 35, loud: -12 }),
  one('shot_cannon', 3, 1.0, 0, -1, 'One single shot from a heavy alien plasma cannon: a deep bassy thoomp throwing a bubbling ball of energy, with a low gurgle. One shot only.', { far: true, low: 35, loud: -12 }),
  one('shot_launcher', 3, 0.6, 0.55, -1, 'One single shot from a heavy grenade launcher: a deep hollow thunk with a metallic clank and a puff of gas. One shot only.', { far: true, low: 35, loud: -13 }),
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

/// A two-pole filter over samples: [b0, b1, b2, a1, a2].
function biquad(x, [b0, b1, b2, a1, a2]) {
  const y = new Float64Array(x.length);
  let [x1, x2, y1, y2] = [0, 0, 0, 0];
  for (let i = 0; i < x.length; i++) {
    const v = b0 * x[i] + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
    [x2, x1, y2, y1] = [x1, x[i], y1, v];
    y[i] = v;
  }
  return y;
}

/// BS.1770's K weighting at SR: its shelf (+4 dB over 1.7 kHz), its high-pass.
const K_WEIGHT = (() => {
  const of = (hz, q) => {
    const w = (2 * Math.PI * hz) / SR;
    return [Math.cos(w), Math.sin(w) / (2 * q)];
  };
  const [c, al] = of(1681.97, 0.7071752);
  const A = 10 ** (3.99984 / 40);
  const r = 2 * Math.sqrt(A) * al;
  const a0 = A + 1 - (A - 1) * c + r;
  const shelf = [A * (A + 1 + (A - 1) * c + r), -2 * A * (A - 1 + (A + 1) * c), A * (A + 1 + (A - 1) * c - r), 2 * (A - 1 - (A + 1) * c), A + 1 - (A - 1) * c - r];
  const [ch, ah] = of(38.1355, 0.500327);
  const high = [(1 + ch) / 2, -(1 + ch), (1 + ch) / 2, -2 * ch, 1 - ah];
  return [shelf.map((v) => v / a0), high.map((v) => v / (1 + ah))];
})();

/// The energy of the loudest `secs` of `x` (a shorter clip is itself and silence).
function loudest(x, secs) {
  const n = Math.round(secs * SR);
  let [e, top] = [0, 0];
  for (let i = 0; i < x.length; i++) {
    e += x[i] * x[i] - (i >= n ? x[i - n] * x[i - n] : 0);
    top = Math.max(top, e);
  }
  return top / n;
}

/// How loud a clip is to an ear, on LUFS' scale: its loudest 200 ms through
/// the K weighting (BS.1770's momentary loudness is over 400 ms, which calls a
/// shot with a long tail louder than one that hits as hard and is gone). A
/// peak says little of it.
const loudness = (x) => -0.691 + 10 * Math.log10(loudest(biquad(biquad(x, K_WEIGHT[0]), K_WEIGHT[1]), 0.2) + 1e-12);

/// The thump under a shot: a sine falling to `hz` from 2.4 times it, dying
/// in `ms`, `d` dB under full scale, from where `cut` leaves the onset.
function thump(n, hz, d, ms) {
  const y = new Float32Array(n);
  const from = Math.round(0.004 * SR);
  for (let i = from; i < n; i++) {
    const t = (i - from) / SR;
    const turns = hz * t + 1.4 * hz * 0.03 * (1 - Math.exp(-t / 0.03));
    y[i] = db(d) * Math.sin(2 * Math.PI * turns) * Math.exp((-t * 1000) / ms) * Math.min(1, t / 0.002);
  }
  return y;
}

/// A shot's shape, as gains: whole for its first 50 ms (the blow), then down
/// `sustain` dB in a few tens of ms and on down to `tail` dB more by its end.
/// The clipper lifts what follows a shot up to it; this is what makes it one
/// blow and not a roar.
function shaped(n, sustain, tail) {
  const hold = Math.round(0.054 * SR);
  return Float32Array.from({ length: n }, (_, i) => {
    const t = Math.max(0, i - hold) / SR;
    return db(-sustain * (1 - Math.exp(-t / 0.04)) - (tail * t * SR) / Math.max(1, n - hold));
  });
}

/// A hit as loud as `s.loud` under its peak: driven into a soft clipper
/// until it is (a burst of noise takes it as a recording made hot does; a
/// limiter's gain riding it is heard), then its shape, its thump laid under.
/// With how hard it was driven, dB.
function hot(x, s) {
  const n = level(x, 0);
  const under = s.thump ? thump(n.length, ...s.thump) : null;
  const fall = s.shape ? shaped(n.length, ...s.shape) : null;
  // Over 0.8, eased to 1: the shot and its thump seldom peak together.
  const eased = (v) => (Math.abs(v) <= 0.8 ? v : Math.sign(v) * (0.8 + 0.2 * Math.tanh((Math.abs(v) - 0.8) / 0.2)));
  const at = (d) => {
    const g = db(d);
    const y = n.map((v, i) => (Math.tanh(v * g) / Math.tanh(g)) * (fall ? fall[i] : 1));
    if (under) for (let i = 0; i < y.length; i++) y[i] = eased(y[i] + under[i]);
    return level(y, s.peak);
  };
  // Driven less than this it is as it came.
  const NONE = -12;
  const plain = at(NONE);
  const over = loudness(plain) - s.loud;
  if (over >= 0) return [plain.map((v) => v * db(-over)), -over];
  let [a, b] = [NONE, 30];
  for (let i = 0; i < 14; i++) {
    const m = (a + b) / 2;
    if (loudness(at(m)) < s.loud) a = m;
    else b = m;
  }
  return [at(b), b];
}

/// The energy in dB of `x`'s first `secs` between two pitches.
function band(x, lo, hi, secs = 0.25) {
  const f = [lo && `highpass=f=${lo},highpass=f=${lo}`, hi && `lowpass=f=${hi},lowpass=f=${hi}`].filter(Boolean).join(',');
  const y = decode(bytes(x), f, RAW).subarray(0, Math.round(secs * SR));
  return 10 * Math.log10(y.reduce((a, v) => a + v * v, 0) / Math.max(1, y.length) + 1e-12);
}

/// A clip's level in 5 ms frames.
function frames(x) {
  const n = Math.round(0.005 * SR);
  const env = [];
  for (let i = 0; i + n <= x.length; i += n) {
    let e = 0;
    for (let j = i; j < i + n; j++) e += x[j] * x[j];
    env.push(Math.sqrt(e / n));
  }
  return env;
}

/// What is wrong with what the model gave for `s`, as points (0: nothing) and
/// words. Too faint to be brought up clean; for one event, struck late (slow
/// from its onset to its full level) or struck twice; for a gun's shot, thin
/// (little under 150 Hz beside its middle) or dull (little over 5 kHz).
function fit(s, raw) {
  const x = decode(raw);
  const wrong = [];
  let points = 0;
  const mark = (p, why) => {
    if (p > 0) {
      points += p;
      wrong.push(why);
    }
  };
  const top = 20 * Math.log10(peak(x) + 1e-9);
  mark(FAINT - top, `faint (${top.toFixed(0)} dB)`);
  if (s.keep && !s.loop) mark(cut(x, s.keep).length / SR < s.keep * 0.4 ? 5 : 0, 'short');
  if (!s.loop && !s.many) {
    const env = frames(x);
    const most = Math.max(...env, 1e-9);
    const on = env.findIndex((v) => v > most * db(-20));
    const at = env.indexOf(most);
    // Struck late: long from its onset to within 6 dB of its loudest.
    const up = env.findIndex((v) => v > most * db(-6));
    if (s.keep && s.keep < 1) mark(((up - on) * 5 - 15) / 10, `struck late (${(up - on) * 5} ms)`);
    // Struck twice: the level back up 8 dB from where it had fallen to, loud.
    // (Over 15 ms: a tail of noise flickers more than that frame to frame.)
    const slow = env.map((_, i) => (env[i] + (env[i + 1] ?? env[i]) + (env[i + 2] ?? env[i])) / 3);
    let [floor, again] = [slow[at], 0];
    for (let i = at; i < slow.length; i++) {
      floor = Math.min(floor, slow[i]);
      if (slow[i] > most * db(-16)) again = Math.max(again, 20 * Math.log10(slow[i] / floor));
    }
    mark((again - 8) / 2, `struck twice (+${again.toFixed(0)} dB)`);
  }
  if (s.shot) {
    const from = x.subarray(Math.max(0, x.findIndex((v) => Math.abs(v) > peak(x) * db(-20))));
    const mid = band(from, 400, 2000);
    const thin = mid - band(from, 60, 150);
    const dull = mid - band(from, 5000, 0);
    mark((thin - 9) / 2, `thin (${thin.toFixed(0)} dB)`);
    mark((dull - 12) / 2, `dull (${dull.toFixed(0)} dB)`);
  }
  return { points, wrong };
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

/// What `n` is cut from: what the model gave, or (made before those were
/// kept) the file the game had, put by on its first cut.
function source(n) {
  return ['mp3', 'ogg'].map((e) => join(RAWS, `${n}.${e}`)).find((f) => existsSync(f));
}

/// A sound's files, by name.
const names = (s) => [...Array.from({ length: s.takes }, (_, k) => `${s.name}_${k}`), ...(s.far ? [`${s.name}_far_0`] : [])];

/// Credits the calls cost, as each one's reply says.
let credits = 0;

async function call(client, s) {
  const { data, rawResponse } = await client.textToSoundEffects
    .convert({
      text: s.text + (s.shot ? SHOT : s.body ? DRY_BODY : DRY),
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
  return Buffer.concat(chunks);
}

/// A take: the first call that fits, or the best of ROLLS.
async function make(client, s, k) {
  let best;
  for (let roll = 0; roll < ROLLS && !(best?.points === 0); roll++) {
    const raw = await call(client, s);
    const f = fit(s, raw);
    if (f.points > 0) console.log(`${s.name}_${k}  roll ${roll + 1}: ${f.wrong.join(', ')}`);
    if (!best || f.points < best.points) best = { ...f, raw };
  }
  mkdirSync(RAWS, { recursive: true });
  writeFileSync(join(RAWS, `${s.name}_${k}.mp3`), best.raw);
  finish(s, k, best.raw);
}

/// What the model gave, as the game has it (`had`: a file the game had, cut
/// already, only levelled again).
function finish(s, k, raw, had = false) {
  const low = `highpass=f=${s.low ?? LOW_HZ}`;
  const x = decode(raw, had ? 'anull' : `${low},${low}`);
  const whole = had ? x : s.loop ? seam(x) : cut(x, s.keep);
  const file = join(OUT, `${s.name}_${k}.ogg`);
  let [y, driven] = s.loud ? hot(whole, s) : [level(s.push ? pushed(whole, s.push) : whole, s.peak), 0];
  write(file, y);
  // Vorbis overshoots a clipped top by a dB or two, and the mixer's samples
  // stop at full scale: down by what it went over, driven that much harder
  // to be as loud still, and written again.
  for (let pass = 0, lift = 0; s.loud && pass < 3; pass++) {
    const over = 20 * Math.log10(peak(decode(readFileSync(file)))) - Math.max(s.peak, -0.5);
    if (over <= 0.1) break;
    lift += over;
    const [made, d] = hot(whole, { ...s, loud: s.loud + lift });
    [y, driven] = [made.map((v) => v * db(-lift)), d];
    write(file, y);
  }
  console.log(`${s.name}_${k}  ${(y.length / SR).toFixed(2)} s  ${loudness(y).toFixed(1)} LUFS${s.loud ? `  driven ${driven.toFixed(1)} dB` : ''}`);
  if (s.far && k === 0) {
    const f = level(cut(decode(bytes(y), FAR, RAW), 0), s.far === true ? FAR_PEAK : s.far);
    write(join(OUT, `${s.name}_far_0.ogg`), f);
    console.log(`${s.name}_far_0  ${(f.length / SR).toFixed(2)} s`);
  }
}

/// A file's numbers: what can be told of a clip without ears. Its length; how
/// long it takes to reach its peak; how many separate events are in it (the
/// level falling 20 dB under its loudest for 30 ms, then coming back); how bright
/// it is (zero crossings, in Hz); how loud (`loudness`), and the most a loop's
/// level moves between its quarters.
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
  return {
    secs: x.length / SR,
    attack,
    events,
    bright: Math.round((zc / 2 / x.length) * SR),
    loud: loudness(x),
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
      // (A shaped tail flickers across that line: `fit` says if it struck twice.)
      if (!s.loop && !s.many && !s.shape && !far && m.events > 1) bad.push(`${m.events} events`);
      if (!s.loop && !s.many && !far && s.keep && s.keep < 1 && m.attack > 80) bad.push('slow attack');
      if (s.keep && m.secs < s.keep * 0.4) bad.push('short');
      // What the model gave, where it is kept: `fit`'s word on it.
      const raw = source(n);
      if (raw && !far) bad.push(...fit(s, readFileSync(raw)).wrong);
      console.log(
        `${n.padEnd(22)} ${m.secs.toFixed(2).padStart(6)} s  attack ${String(m.attack).padStart(4)} ms  ` +
          `${String(m.events).padStart(2)} ev  ${String(m.bright).padStart(5)} Hz  ${m.loud.toFixed(1).padStart(6)} LUFS  ` +
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
      const n = `${s.name}_${k}`;
      if (!source(n) && existsSync(join(OUT, `${n}.ogg`))) {
        mkdirSync(RAWS, { recursive: true });
        copyFileSync(join(OUT, `${n}.ogg`), join(RAWS, `${n}.ogg`));
      }
      const raw = source(n);
      if (raw) finish(s, k, readFileSync(raw), raw.endsWith('.ogg'));
      else console.log(`${n}: nothing in ${RAWS} or ${OUT}`);
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

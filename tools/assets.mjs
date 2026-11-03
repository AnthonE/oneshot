// node tools/assets.mjs [GATES_DIR]: what the Bevy client draws and plays,
// from a Gates checkout into crates/client/assets (docs/RIP.md §3–5, §9):
// the models (Triple T and the guns) with their KTX2 textures re-embedded as
// JPEG by tools/glb, so no build needs a transcoder; the sounds; and the
// ground's and the boxes' textures. All CC0 or public domain (Gates'
// MANIFEST.md files); credited in NOTICE.

import { copyFileSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { basename, join } from 'node:path';

const gates = process.argv[2] || process.env.GATES_DIR || '../Gates';
const out = 'crates/client/assets';
if (!existsSync(join(gates, 'assets/models/stumpy.glb')) || !existsSync(join(gates, 'assets/textures/MANIFEST.md'))) {
  console.error(`no Gates checkout at ${gates} (pass its path)`);
  process.exit(1);
}

// Triple T, and the guns by role (docs/RIP.md §3).
const MODELS = {
  'triple-t.glb': 'models/stumpy.glb',
  'ar.glb': 'models/held/assault_rifle.glb',
  'br.glb': 'models/held/semi_automatic_rifle.glb',
  'magnum.glb': 'models/held/python_revolver.glb',
  'sniper.glb': 'models/held/bolt_action_rifle.glb',
  'shotgun.glb': 'models/held/pump_shotgun.glb',
  'rocket-launcher.glb': 'models/held/rocket_launcher.glb',
  'rocket.glb': 'models/held/rocket.glb',
  'smg.glb': 'models/held/custom_smg.glb',
  'frag.glb': 'models/held/beancan_grenade.glb',
};

// All CC0 or public domain; credits in NOTICE.
const SOUNDS = [
  ...['shot_rifle', 'shot_smg', 'shot_pistol', 'shot_shotgun', 'shot_bolt', 'shot_heli', 'shot_rocket'].flatMap((n) =>
    [0, 1, 2].map((i) => `pd/${n}_${i}.ogg`)),
  ...['shot_rifle_far', 'shot_smg_far', 'shot_pistol_far', 'shot_shotgun_far', 'shot_bolt_far', 'shot_heli_far'].flatMap((n) =>
    [0, 1].map((i) => `pd/${n}_${i}.ogg`)),
  'freesound/shot_gun_0.ogg', 'freesound/shot_gun_1.ogg', 'freesound/shot_gun_far_0.ogg',
  'pd/reload_rifle_0.ogg', 'pd/reload_pistol_0.ogg', 'pd/reload_shotgun_0.ogg', 'pd/reload_bolt_0.ogg',
  'pd/dry_fire_0.ogg', 'pd/headshot_0.ogg', 'pd/headshot_1.ogg',
  'kenney/hit_0.ogg', 'kenney/equip_0.ogg',
  'freesound/blast_0.ogg', 'freesound/blast_1.ogg', 'freesound/fuse_0.ogg',
  'freesound/flesh_0.ogg', 'freesound/flesh_1.ogg', 'freesound/hurt_0.ogg', 'freesound/death_0.ogg', 'freesound/swing_0.ogg',
  ...[0, 1, 2, 3].map((i) => `pd/step_concrete_${i}.ogg`),
  'freesound/land_0.ogg',
];

// The ground's four (sand, grass, the creek's gravel, rock) and the boxes'.
const SETS = ['sand', 'grass', 'gravel', 'rock', 'concrete', 'metal'];
const BOX_SETS = ['concrete', 'metal', 'rock'];

// The models: Gates' files as they are into a scratch folder, then tools/glb
// re-embeds their textures into ours.
const raw = 'traces/gates-models';
rmSync(raw, { recursive: true, force: true });
mkdirSync(raw, { recursive: true });
for (const [name, src] of Object.entries(MODELS)) {
  copyFileSync(join(gates, 'assets', src), join(raw, name));
  console.log(`models/${name} <- ${src}`);
}
mkdirSync(join(out, 'models'), { recursive: true });
const glb = spawnSync('cargo', ['run', '-q', '--release', '--manifest-path', 'tools/glb/Cargo.toml', '--', raw, join(out, 'models')], { stdio: 'inherit' });
if (glb.status !== 0) process.exit(glb.status ?? 1);

mkdirSync(join(out, 'sound'), { recursive: true });
for (const s of SOUNDS) {
  const from = join(gates, 'assets/sound', s);
  if (!existsSync(from)) {
    console.warn(`missing sound ${s}`);
    continue;
  }
  copyFileSync(from, join(out, 'sound', basename(s)));
}

const tex = join(gates, 'assets/textures');
mkdirSync(join(out, 'textures'), { recursive: true });
// The grass card atlas (Poly Haven grass_medium_01, baked by Gates).
copyFileSync(join(tex, 'grass_card_albedo.png'), join(out, 'textures/grass_card_albedo.png'));
for (const set of SETS) {
  for (const m of ['albedo', 'normal']) copyFileSync(join(tex, `${set}_${m}.jpg`), join(out, `textures/${set}_${m}.jpg`));
  // The boxes' roughness and occlusion (look/textures.rs packs them).
  if (BOX_SETS.includes(set)) for (const m of ['rough', 'ao']) copyFileSync(join(tex, `${set}_${m}.jpg`), join(out, `textures/${set}_${m}.jpg`));
}
console.log(`${Object.keys(MODELS).length} models, ${SOUNDS.length} sounds, ${SETS.length * 2 + 1} textures into ${out}`);

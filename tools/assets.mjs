// node tools/assets.mjs [GATES_DIR]: what the Bevy client draws, from a Gates
// checkout into crates/client/assets (docs/RIP.md §3–5, §9): the model
// (Triple T) with its KTX2 textures re-embedded as JPEG by tools/glb, so no
// build needs a transcoder; and the ground's and the boxes' textures. All CC0
// or public domain (Gates' MANIFEST.md files); credited in NOTICE. The sounds
// are ours: tools/sfx.mjs, tools/voice.mjs.

import { copyFileSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';

const gates = process.argv[2] || process.env.GATES_DIR || '../Gates';
const out = 'crates/client/assets';
if (!existsSync(join(gates, 'assets/models/stumpy.glb')) || !existsSync(join(gates, 'assets/textures/MANIFEST.md'))) {
  console.error(`no Gates checkout at ${gates} (pass its path)`);
  process.exit(1);
}

// Triple T (docs/RIP.md §3). The guns are ours: tools/models.
const MODELS = {
  'triple-t.glb': 'models/stumpy.glb',
};


// The rock pieces a mass out of bounds is drawn with (look/rock.rs): Gates'
// formation pieces remeshed low, geometry only (no texture to re-embed),
// commissioned from Meshy on a paid plan and owned (Gates'
// assets/models/MANIFEST.md, prop/rock/).
const ROCKS = ['a', 'b', 'c', 'd', 'e', 'f'].map((k) => `block_${k}_far.glb`);

// The boxes' two. The ground's four (sand, grass, the creek's gravel, and
// the rock, which the rock boxes wear too) are ours to pick now:
// tools/ground.sh cuts them from Poly Haven.
const SETS = ['concrete', 'metal'];
const BOX_SETS = ['concrete', 'metal'];

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
// Triple T's clips put right: Gates' file has them as they were.
const clips = spawnSync('blender', ['-b', '--python-exit-code', '1', '--python', 'tools/models/triple-t.py'], { stdio: 'inherit' });
if (clips.status !== 0) process.exit(clips.status ?? 1);

mkdirSync(join(out, 'models/rock'), { recursive: true });
for (const r of ROCKS) {
  copyFileSync(join(gates, 'assets/models/prop/rock', r), join(out, 'models/rock', r));
  console.log(`models/rock/${r} <- models/prop/rock/${r}`);
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
console.log(`${Object.keys(MODELS).length} models, ${ROCKS.length} rock pieces, ${SETS.length * 2 + 1} textures into ${out}`);

// node tools/assets.mjs [GATES_DIR]: copies what the page uses from a Gates
// checkout into web/public (docs/RIP.md §3–5), and patches each GLB so
// three.js reads its textures.
//
// Gates' GLBs embed KTX2 (UASTC, Zstd) images but name them only by
// mimeType, without declaring KHR_texture_basisu; three's GLTFLoader then
// skips them. The patch moves each KTX2 image reference into the
// extension and declares it. No pixel changes.

import { readFileSync, writeFileSync, mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { join, dirname, basename } from 'node:path';

const gates = process.argv[2] || process.env.GATES_DIR || '../Gates';
const out = 'web/public';
if (!existsSync(join(gates, 'assets/models/stumpy.glb'))) {
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

const TEXTURES = ['concrete_albedo.jpg', 'concrete_normal.jpg', 'metal_albedo.jpg', 'metal_normal.jpg',
  'aggregate_albedo.jpg', 'aggregate_normal.jpg'];

function patchGlb(buf) {
  if (buf.readUInt32LE(0) !== 0x46546c67) throw new Error('not a GLB');
  const jsonLen = buf.readUInt32LE(12);
  if (buf.readUInt32LE(16) !== 0x4e4f534a) throw new Error('first chunk is not JSON');
  const json = JSON.parse(buf.subarray(20, 20 + jsonLen).toString('utf8'));
  const rest = buf.subarray(20 + jsonLen);
  let n = 0;
  for (const t of json.textures || []) {
    const img = json.images?.[t.source];
    if (t.source !== undefined && img?.mimeType === 'image/ktx2') {
      t.extensions = { ...(t.extensions || {}), KHR_texture_basisu: { source: t.source } };
      delete t.source;
      n++;
    }
  }
  if (n) json.extensionsUsed = [...new Set([...(json.extensionsUsed || []), 'KHR_texture_basisu'])];
  let text = Buffer.from(JSON.stringify(json), 'utf8');
  const pad = (4 - (text.length % 4)) % 4;
  text = Buffer.concat([text, Buffer.alloc(pad, 0x20)]);
  const head = Buffer.alloc(20);
  head.writeUInt32LE(0x46546c67, 0);
  head.writeUInt32LE(2, 4);
  head.writeUInt32LE(20 + text.length + rest.length, 8);
  head.writeUInt32LE(text.length, 12);
  head.writeUInt32LE(0x4e4f534a, 16);
  return [Buffer.concat([head, text, rest]), n];
}

const put = (rel, data) => {
  const p = join(out, rel);
  mkdirSync(dirname(p), { recursive: true });
  writeFileSync(p, data);
};

for (const [name, src] of Object.entries(MODELS)) {
  const [glb, n] = patchGlb(readFileSync(join(gates, 'assets', src)));
  put(`models/${name}`, glb);
  console.log(`models/${name} ← ${src} (${n} textures declared)`);
}
for (const s of SOUNDS) {
  const from = join(gates, 'assets/sound', s);
  if (!existsSync(from)) {
    console.warn(`missing sound ${s}`);
    continue;
  }
  mkdirSync(join(out, 'sound'), { recursive: true });
  copyFileSync(from, join(out, 'sound', basename(s)));
}
for (const t of TEXTURES) {
  mkdirSync(join(out, 'textures'), { recursive: true });
  copyFileSync(join(gates, 'assets/textures', t), join(out, 'textures', t));
}
console.log(`${SOUNDS.length} sounds, ${TEXTURES.length} textures`);

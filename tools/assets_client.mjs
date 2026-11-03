// node tools/assets_client.mjs [GATES_DIR]: copies the textures the Bevy
// client draws with from a Gates checkout into crates/client/assets
// (docs/RIP.md §9). All CC0 (Gates' assets/textures/MANIFEST.md: Poly
// Haven and ambientCG); credited in NOTICE. The three.js page's are
// tools/assets.mjs's.

import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';

const gates = process.argv[2] || process.env.GATES_DIR || '../Gates';
const src = join(gates, 'assets/textures');
const out = 'crates/client/assets/textures';
if (!existsSync(join(src, 'MANIFEST.md'))) {
  console.error(`no Gates checkout at ${gates} (pass its path)`);
  process.exit(1);
}
// The ground's four (sand, grass, the creek's gravel, rock) and the boxes'.
const SETS = ['sand', 'grass', 'gravel', 'rock', 'concrete', 'metal'];
mkdirSync(out, { recursive: true });
for (const s of SETS) {
  for (const m of ['albedo', 'normal']) {
    copyFileSync(join(src, `${s}_${m}.jpg`), join(out, `${s}_${m}.jpg`));
    console.log(`${out}/${s}_${m}.jpg`);
  }
}

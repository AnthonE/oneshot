#!/usr/bin/env node
// The terrain probe: for every map with a terrain, a hillshade and a crease
// map of its raster as PGMs in traces/terrain/ (<map>.pgm, <map>-crease.pgm),
// to look at after a recompile. The shade is lit low from the north-west,
// which makes a crease pop; the crease map is |∇‖∇h‖|, the change in the
// slope: a C1 kink in a shaping curve is a step in the slope, which draws
// here as a bright line along one height, right across the map. Smooth
// relief is dark; cliffs are bright where they meet the floor and the mesa
// and nowhere else. After Gates' sim-core/examples/hillshade.rs (MIT).
//
//   node tools/maps/hillshade.mjs          every map
//   node tools/maps/hillshade.mjs canyon   one
//
// The numbers printed (the heights' range, the crease's peak and mean, in
// rise-over-run a metre) are for comparing one build against the last.

import { writeFileSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compileAll } from './compile.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const OUT = join(ROOT, 'traces', 'terrain');
/** sim::terrain::HEIGHT_Q. */
const HEIGHT_Q = 128;
/** The sun: low, from the north-west (−x, −z), as Gates lights its probe. */
const SUN = [-0.55, 0.62, -0.56];
/** How bright the crease map is: a slope change of 1 / CREASE_SCALE a metre is white. */
const CREASE_SCALE = 6;

function pgm(path, w, h, px) {
  writeFileSync(path, Buffer.concat([Buffer.from(`P5\n${w} ${h}\n255\n`, 'ascii'), Buffer.from(px)]));
}

function probe(m) {
  const t = m.terrain;
  const { nx, nz, cell } = t;
  const h = (i, j) => t.heights[Math.min(Math.max(j, 0), nz - 1) * nx + Math.min(Math.max(i, 0), nx - 1)] / HEIGHT_Q;
  const sea = m.light.sea;
  let lo = Infinity, hi = -Infinity;
  for (const q of t.heights) {
    lo = Math.min(lo, q / HEIGHT_Q);
    hi = Math.max(hi, q / HEIGHT_Q);
  }
  // The slope at every vertex, by central differences (one-sided at the edges).
  const gx = new Float32Array(nx * nz), gz = new Float32Array(nx * nz), grad = new Float32Array(nx * nz);
  const shade = new Uint8Array(nx * nz);
  for (let j = 0; j < nz; j++) {
    for (let i = 0; i < nx; i++) {
      const dx = (h(i + 1, j) - h(i - 1, j)) / (cell * (Math.min(i + 1, nx - 1) - Math.max(i - 1, 0)));
      const dz = (h(i, j + 1) - h(i, j - 1)) / (cell * (Math.min(j + 1, nz - 1) - Math.max(j - 1, 0)));
      const k = j * nx + i;
      gx[k] = dx;
      gz[k] = dz;
      grad[k] = Math.sqrt(dx * dx + dz * dz);
      // The normal (−dx, 1, −dz), lit.
      const il = 1 / Math.sqrt(dx * dx + 1 + dz * dz);
      const ndl = Math.max(0, (-dx * SUN[0] + SUN[1] - dz * SUN[2]) * il);
      let v = Math.min(1, 0.12 + 0.88 * ndl);
      // Under the water: dimmed, so the shore reads.
      if (sea !== null && h(i, j) < sea) v *= 0.45;
      shade[k] = Math.round(v * 255);
    }
  }
  const crease = new Uint8Array(nx * nz);
  let peak = 0, sum = 0, n = 0;
  for (let j = 1; j < nz - 1; j++) {
    for (let i = 1; i < nx - 1; i++) {
      const k = j * nx + i;
      const cx = (grad[k + 1] - grad[k - 1]) / (2 * cell);
      const cz = (grad[k + nx] - grad[k - nx]) / (2 * cell);
      const c = Math.sqrt(cx * cx + cz * cz);
      peak = Math.max(peak, c);
      sum += c;
      n++;
      crease[k] = Math.round(Math.min(1, c * CREASE_SCALE) * 255);
    }
  }
  mkdirSync(OUT, { recursive: true });
  pgm(join(OUT, `${m.module}.pgm`), nx, nz, shade);
  pgm(join(OUT, `${m.module}-crease.pgm`), nx, nz, crease);
  console.log(`${m.module}: ${nx} × ${nz} at ${cell} m, heights ${lo.toFixed(2)}..${hi.toFixed(2)} m, crease peak ${peak.toFixed(2)} mean ${(sum / n).toFixed(4)} a metre → traces/terrain/${m.module}.pgm, ${m.module}-crease.pgm`);
}

function main() {
  const only = process.argv.slice(2);
  let any = false;
  for (const m of compileAll()) {
    if (!m.terrain || (only.length && !only.includes(m.module))) continue;
    probe(m);
    any = true;
  }
  if (!any) console.log('no map with a terrain');
}

main();

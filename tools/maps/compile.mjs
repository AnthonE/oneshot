#!/usr/bin/env node
// The map compiler: maps/<name>/map.json in, crates/sim/src/maps/<name>.rs
// out, the tables the sim builds a `Map` from, and crates/sim/src/maps/mod.rs
// listing them by id. The JSON is what a person edits; the Rust is checked in
// so the sim needs no parser, and `--check` (run by CI) fails when it isn't
// what the JSON compiles to. After Fireplace's tools/map-compile.mjs (CC0):
// its boxes, prefabs and quarter turns, on our flat floor (no heightfield,
// nothing that moves).
//
//   node tools/maps/compile.mjs           write the Rust
//   node tools/maps/compile.mjs --check   fail if it is stale
//
// map.json:
//   id        u8, what the wire and replays name it by (0..254)
//   name      shown on the page
//   about     a sentence
//   bounds    [x0, x1, z0, z1]: where fighters can be (the walls stand
//             outside it, as boxes like any other)
//   mirror    "x": everything marked "mirror" is placed again at -x, red
//             swapped for blue; "xz": also at -z, and at -x -z
//   prefabs   { name: [box, ...] } about their own origin
//   structures [{ name, boxes | prefab, at: [x, y, z], turn: 0|90|180|270,
//             tint: { look: look }, mirror: bool }]
//   spawns    [{ at: [x, z, y?], team: "red"|"blue"|"any", yaw: degrees,
//             mirror: bool }]
//   pads      [{ weapon, at: [x, z, y?], respawn: seconds, mirror: bool }]
//   roam      [[x, z], ...]: where a bot with nobody in sight walks
// A box is [x0, y0, z0, x1, y1, z1, look]; a look (drawing only) is one of
// LOOKS. Metres, y up, the floor at y = 0.

import { readFileSync, writeFileSync, readdirSync, existsSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const MAPS = join(ROOT, 'maps');
const OUT = join(ROOT, 'crates', 'sim', 'src', 'maps');

/** How a box is drawn, as web/src/world.ts numbers them. */
export const LOOKS = ['wall', 'block', 'trim', 'red', 'blue', 'dark'];
/** sim::arms, by the names map.json uses. */
export const WEAPONS = { ar: 'AR', br: 'BR', magnum: 'MAGNUM', sniper: 'SNIPER', shotgun: 'SHOTGUN', rocket: 'ROCKET', smg: 'SMG' };
const TEAMS = { red: 0, blue: 1, any: 255 };
// Caps the sim holds (crates/sim/src/consts.rs MAX_PADS) and sanity.
const MAX_PADS = 8;
const MAX_BOXES = 256;

/** A box turned by `turn` degrees about y (+x toward +z at 90), then moved to `at`. */
export function place(box, at, turn) {
  const [x0, y0, z0, x1, y1, z1] = box;
  if (turn % 90) throw new Error(`turn ${turn}: quarter turns only`);
  const q = ((Math.round(turn / 90) % 4) + 4) % 4;
  const rot = ([x, z]) => [[x, z], [-z, x], [-x, -z], [z, -x]][q];
  const a = rot([x0, z0]), b = rot([x1, z1]);
  return [Math.min(a[0], b[0]) + at[0], y0 + at[1], Math.min(a[1], b[1]) + at[2], Math.max(a[0], b[0]) + at[0], y1 + at[1], Math.max(a[1], b[1]) + at[2]];
}

const f32 = (v) => Math.fround(v);

/** The mirror images a "mirror" item gets: [sx, sz, swap teams]. */
function images(mode) {
  if (!mode) return [];
  if (mode === 'x') return [[-1, 1, true]];
  if (mode === 'xz') return [[-1, 1, true], [1, -1, false], [-1, -1, true]];
  throw new Error(`mirror "${mode}": "x" or "xz"`);
}

export function compile(spec, where = 'map.json') {
  const fail = (s) => { throw new Error(`${where}: ${s}`); };
  if (!Number.isInteger(spec.id) || spec.id < 0 || spec.id > 254) fail('id: 0 to 254');
  if (typeof spec.name !== 'string' || !spec.name) fail('no name');
  const [bx0, bx1, bz0, bz1] = spec.bounds || fail('no bounds: [x0, x1, z0, z1]');
  if (!(bx0 < bx1 && bz0 < bz1)) fail('bounds: [x0, x1, z0, z1]');
  const inside = (x, z, what) => { if (!(x >= bx0 && x <= bx1 && z >= bz0 && z <= bz1)) fail(`${what} at ${x}, ${z} is out of bounds`); };
  const mirrors = images(spec.mirror);
  const swapLook = (l) => (l === 'red' ? 'blue' : l === 'blue' ? 'red' : l);

  const boxes = [];
  for (const [k, s] of (spec.structures || []).entries()) {
    const name = s.name || `structure ${k}`;
    const list = s.prefab ? ((spec.prefabs || {})[s.prefab] || fail(`${name}: no prefab "${s.prefab}"`)) : (s.boxes || fail(`${name}: no boxes`));
    const at = s.at || [0, 0, 0], turn = s.turn || 0;
    const placed = [];
    for (const b of list) {
      if (b.length < 6 || b.slice(0, 6).some((v) => typeof v !== 'number')) fail(`${name}: a box is [x0, y0, z0, x1, y1, z1, look]`);
      if (!(b[0] < b[3] && b[1] < b[4] && b[2] < b[5])) fail(`${name}: box ${JSON.stringify(b)} is empty or inside out`);
      if (b[1] < 0) fail(`${name}: box ${JSON.stringify(b)} goes under the floor`);
      const look = (s.tint && s.tint[b[6]]) || b[6] || 'block';
      if (!LOOKS.includes(look)) fail(`${name}: no look "${look}" (${LOOKS.join(', ')})`);
      placed.push({ box: place(b, at, turn), look });
    }
    boxes.push(...placed);
    if (s.mirror) {
      if (!mirrors.length) fail(`${name}: "mirror" needs the map's "mirror"`);
      for (const [sx, sz, swap] of mirrors) {
        for (const p of placed) {
          const [x0, y0, z0, x1, y1, z1] = p.box;
          const xs = [x0 * sx, x1 * sx].sort((a, b) => a - b), zs = [z0 * sz, z1 * sz].sort((a, b) => a - b);
          boxes.push({ box: [xs[0], y0, zs[0], xs[1], y1, zs[1]], look: swap ? swapLook(p.look) : p.look });
        }
      }
    }
  }
  if (boxes.length > MAX_BOXES) fail(`${boxes.length} boxes, at most ${MAX_BOXES}`);

  const point = (at, what) => {
    if (!Array.isArray(at) || at.length < 2 || at.some((v) => typeof v !== 'number')) fail(`${what}: "at" is [x, z] or [x, z, y]`);
    inside(at[0], at[1], what);
    return [at[0], at[2] ?? 0, at[1]];
  };
  const spawns = [];
  for (const [k, s] of (spec.spawns || []).entries()) {
    const what = `spawn ${k}`;
    const p = point(s.at, what);
    if (!(s.team in TEAMS)) fail(`${what}: team "red", "blue" or "any"`);
    const yaw = s.yaw ?? 0;
    spawns.push({ pos: p, yaw, team: s.team });
    if (s.mirror) {
      for (const [sx, sz, swap] of mirrors) {
        // Mirroring x turns a heading θ into 180 − θ; mirroring z into −θ.
        let y = yaw;
        if (sx < 0) y = 180 - y;
        if (sz < 0) y = -y;
        const team = swap && s.team !== 'any' ? (s.team === 'red' ? 'blue' : 'red') : s.team;
        spawns.push({ pos: [p[0] * sx, p[1], p[2] * sz], yaw: y, team });
      }
    }
  }
  if (!spawns.length) fail('no spawns');
  const pads = [];
  for (const [k, p] of (spec.pads || []).entries()) {
    const what = `pad ${k}`;
    const w = WEAPONS[p.weapon] || fail(`${what}: no weapon "${p.weapon}" (${Object.keys(WEAPONS).join(', ')})`);
    const at = point(p.at, what);
    const respawn = Math.round((p.respawn ?? 30) * 1000);
    if (!(respawn > 0)) fail(`${what}: respawn in seconds, over 0`);
    pads.push({ pos: at, arm: w, respawn });
    if (p.mirror) for (const [sx, sz] of mirrors) pads.push({ pos: [at[0] * sx, at[1], at[2] * sz], arm: w, respawn });
  }
  if (pads.length > MAX_PADS) fail(`${pads.length} pads, at most ${MAX_PADS}`);
  const roam = (spec.roam || []).map((r, k) => {
    if (!Array.isArray(r) || r.length !== 2) fail(`roam ${k}: [x, z]`);
    inside(r[0], r[1], `roam ${k}`);
    return r;
  });
  return { id: spec.id, name: spec.name, about: spec.about || '', lo: [bx0, bz0], hi: [bx1, bz1], boxes, spawns, pads, roam };
}

// ---------------------------------------------------------------- Rust out

/** The shortest decimal that is this f32, as a Rust f32 literal. */
const lit = (v) => {
  const x = f32(v);
  if (Object.is(x, -0)) return '-0.0';
  let s = String(x);
  for (let p = 1; p <= 9; p++) {
    const t = String(Number(x.toPrecision(p)));
    if (f32(Number(t)) === x) { s = t; break; }
  }
  return /[.e]/.test(s) ? s : `${s}.0`;
};
const yawU16 = (deg) => ((Math.round((deg / 360) * 65536) % 65536) + 65536) % 65536;

export function rust(m, src) {
  const L = [];
  L.push(`//! ${m.name}${m.about ? `: ${m.about}` : ''}`);
  L.push(`//!`);
  L.push(`//! Generated from \`${src}\` by \`tools/maps/compile.mjs\`: edit the JSON,`);
  L.push(`//! then run it. CI fails if this file is not what the JSON compiles to.`);
  L.push('');
  L.push('use super::Def;');
  L.push('#[allow(unused_imports)]');
  L.push('use crate::arms::*;');
  L.push('');
  L.push('#[rustfmt::skip]');
  L.push('pub const DEF: Def = Def {');
  L.push(`    id: ${m.id},`);
  L.push(`    name: ${JSON.stringify(m.name)},`);
  L.push(`    lo: (${lit(m.lo[0])}, ${lit(m.lo[1])}),`);
  L.push(`    hi: (${lit(m.hi[0])}, ${lit(m.hi[1])}),`);
  L.push('    boxes: &[');
  for (const b of m.boxes) L.push(`        [${b.box.map(lit).join(', ')}],`);
  L.push('    ],');
  L.push(`    looks: &[${m.boxes.map((b) => LOOKS.indexOf(b.look)).join(', ')}],`);
  L.push('    spawns: &[');
  for (const s of m.spawns) L.push(`        ([${s.pos.map(lit).join(', ')}], ${yawU16(s.yaw)}, ${TEAMS[s.team]}),`);
  L.push('    ],');
  L.push('    pads: &[');
  for (const p of m.pads) L.push(`        ([${p.pos.map(lit).join(', ')}], ${p.arm}, ${p.respawn}),`);
  L.push('    ],');
  L.push(`    roam: &[${m.roam.map((r) => `[${r.map(lit).join(', ')}]`).join(', ')}],`);
  L.push('};');
  return L.join('\n') + '\n';
}

function modRs(list) {
  const L = [];
  L.push('//! The maps, from `maps/*/map.json` by `tools/maps/compile.mjs`. Generated:');
  L.push('//! run the compiler, never edit by hand.');
  L.push('');
  for (const m of [...list].sort((a, b) => (a.module < b.module ? -1 : 1))) L.push(`pub mod ${m.module};`);
  L.push('');
  L.push('pub use crate::map::Def;');
  L.push('');
  L.push('/// Every map, in id order.');
  L.push(`pub const ALL: [&Def; ${list.length}] = [${list.map((m) => `&${m.module}::DEF`).join(', ')}];`);
  return L.join('\n') + '\n';
}

function main() {
  const check = process.argv.includes('--check');
  const dirs = readdirSync(MAPS).filter((d) => existsSync(join(MAPS, d, 'map.json'))).sort();
  const list = [];
  for (const d of dirs) {
    const src = `maps/${d}/map.json`;
    const m = compile(JSON.parse(readFileSync(join(MAPS, d, 'map.json'), 'utf8')), src);
    if (!/^[a-z][a-z0-9_]*$/.test(d)) throw new Error(`${src}: the directory names a Rust module: lower case, digits, _`);
    list.push({ ...m, module: d, src });
  }
  list.sort((a, b) => a.id - b.id);
  for (let i = 1; i < list.length; i++) if (list[i].id === list[i - 1].id) throw new Error(`${list[i].src}: id ${list[i].id} is ${list[i - 1].src}'s`);
  const files = list.map((m) => [join(OUT, `${m.module}.rs`), rust(m, m.src)]);
  files.push([join(OUT, 'mod.rs'), modRs(list)]);
  let stale = 0;
  if (!check) mkdirSync(OUT, { recursive: true });
  for (const [path, text] of files) {
    const have = existsSync(path) ? readFileSync(path, 'utf8') : null;
    if (have === text) continue;
    if (check) {
      console.error(`stale: ${path.slice(ROOT.length + 1)} (node tools/maps/compile.mjs)`);
      stale++;
    } else {
      writeFileSync(path, text);
      console.log(`wrote ${path.slice(ROOT.length + 1)}`);
    }
  }
  for (const m of list) console.log(`${m.id} ${m.name}: ${m.boxes.length} boxes, ${m.spawns.length} spawns, ${m.pads.length} pads, ${m.roam.length} roam points`);
  if (stale) process.exit(1);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();

#!/usr/bin/env node
// The map compiler: maps/<name>/map.json in, crates/sim/src/maps/<name>.rs
// out, the tables the sim builds a `Map` from, and crates/sim/src/maps/mod.rs
// listing them by id. The JSON is what a person edits; the Rust is checked in
// so the sim needs no parser, and `--check` (run by CI) fails when it isn't
// what the JSON compiles to. After Fireplace's tools/map-compile.mjs (CC0):
// its boxes, prefabs and quarter turns, on a flat floor or a terrain. The
// terrain's noise, its canyon and its boulder planner follow Gates'
// sim-core (MIT: crates/sim-core/src/terrain.rs, face.rs, boulder.rs),
// copied and trimmed, never depended on.
//
//   node tools/maps/compile.mjs           write the Rust
//   node tools/maps/compile.mjs --check   fail if it is stale
//
// map.json:
//   id        u8, what the wire and replays name it by (0..254)
//   name      shown on the page
//   about     a sentence
//   bounds    [x0, x1, z0, z1]: where fighters can be (the walls stand
//             outside it, as boxes like any other, or as the terrain)
//   mirror    "x": everything marked "mirror" is placed again at -x, red
//             swapped for blue; "xz": also at -z, and at -x -z
//   prefabs   { name: [box, ...] } about their own origin
//   structures [{ name, boxes | prefab, at: [x, y, z], turn: 0|90|180|270,
//             tint: { look: look }, mirror: bool, dress: bool }]: with
//             `dress`, its boxes are drawn and never collided (a vault's
//             curve, trim on a wall); a `vault` box is always dressing
//   spawns    [{ at: [x, z, y?], team: "red"|"blue"|"any", yaw: degrees,
//             mirror: bool }]
//   pads      [{ weapon, at: [x, z, y?], respawn: seconds, mirror: bool }]
//   roam      [[x, z], ...]: where a bot with nobody in sight walks
//   terrain   { cell: 0.5|1|2|4, base: m, margin: m | [-x, +x, -z, +z],
//             gen: "canyon", canyon: {...}, shapes: [...] }: ground that
//             isn't flat, over the bounds and `margin` past them (one
//             number, or one a side). A shape adds to the base, falling off
//             as 1 − fade(d / r) (the quintic fade):
//               { hill: [x, z], r, h }         a round rise (h < 0: a dip)
//               { ridge: [[x, z], [x, z]], w, h }  along a line, w to either side
//               { plateau: [x0, z0, x1, z1], h, edge }  flat inside, falling over `edge`
//               { pad: [x, z], r, h, blend }   flat at h out to r, then back
//                                              to the ground over `blend`: a
//                                              post-pass that replaces,
//                                              never adds (a base's floor)
//             each with mirror: bool. With gen "canyon" the `canyon` block
//             (see `canyon()` below) raises scree, cliffs and mesas around
//             the shapes, with pockets, gullies, strata and a sea end. The
//             heights are written as integers (1/128 m), so the sim never
//             reads a float made here.
//   rocks     { seed, ... }: boulder formations planned from the seed and
//             seated on the terrain, as rock boxes (see `planRocks()`)
//   launchers [{ at: [x, z, y?], to: [x, z, y?], time: s, size: [w, d],
//             mirror: bool }]: a pad that throws a fighter from `at` to land
//             on `to` after `time` seconds
//   vehicles  [{ at: [x, z, y?], yaw: degrees, respawn: seconds, mirror: bool }]:
//             where a vehicle starts and comes back
//   nav       true: bots find their way by waypoints (crates/sim/src/nav.rs)
//             rather than walking straight where they're going
//   callouts  [{ name, at: [x, z, y?] }]: places tools/flow names; not compiled
//   light     { ev100, sun, fill, lamps: [{ at: [x, z, y], color: [r, g, b],
//             lm, mirror: bool }], floor: look, sea: y }: drawing only
//             (sim::map::LightDef). ev100 0 is the client's noon; sun and
//             fill scale its sun and sky fill; `sea` is the water's surface
//             (the client draws it there; the sim never reads it)
// A box is [x0, y0, z0, x1, y1, z1, look]; a look (drawing only) is one of
// LOOKS. A `vault` box is drawn as a curved ceiling hung from its bottom edges
// (the sim still collides with the box: put `hidden` boxes under it that step
// with the curve); a `rib` box is drawn as a rounded bar. A structure's list may also hold solids that aren't boxes
// (crates/sim/src/solid.rs), placed, turned and mirrored as boxes are:
//   { round: [x, z], r, y: [y0, y1], look }          an upright cylinder
//   { round: [x, z], r, ground: true, h, look }      one standing on the
//                                                    terrain: y0 is the ground
//                                                    there (as the sim samples
//                                                    it), y1 = y0 + h (a tree)
//   { turned: [x, z], size: [w, d], angle: deg, y: [y0, y1], look }
//                                                    a box turned about y
//                                                    (+x toward +z)
//   { ramp: [x0, z0, x1, z1], up: "+x"|"-x"|"+z"|"-z", from: y, to: y,
//     base: y (default 0), look }                     a slope, `from` at its
//                                                    low side, `to` at its high
//   { arc: [x, z], r, from: deg, to: deg, thick: m, y: [y0, y1], look }
//                                                    a curved wall: turned
//                                                    segments along the circle
//                                                    of radius r, within 2 cm Metres, y up, the floor at y = 0. On a terrain a box stands at the
// y0 it says (below 0 to sink it into a dip), and a spawn, a pad or a
// roam point at height 0 is put on the ground by the sim.

import { readFileSync, writeFileSync, readdirSync, existsSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const MAPS = join(ROOT, 'maps');
const OUT = join(ROOT, 'crates', 'sim', 'src', 'maps');

/** How a box is drawn, as the client's level.rs numbers them. */
export const LOOKS = ['wall', 'block', 'trim', 'red', 'blue', 'dark', 'rock', 'hull', 'glow', 'red_glow', 'blue_glow', 'plate', 'alien', 'alien_dark', 'alien_red', 'alien_blue',
  // The ship's: its deck, its gold honeycomb windows, a box drawn as a curved ceiling (`vault`) or a rounded bar (`rib`),
  // a box that collides but isn't drawn (`hidden`: the steps under a vault), and its light strips.
  'deck', 'gold', 'vault', 'hidden', 'magenta_glow', 'cyan_glow', 'rib',
  // Trees: a round solid on the ground drawn as a palm or a dead tree (the client's look/trees.rs).
  'palm', 'dead_tree'];
/** sim::arms, by the names map.json uses. */
export const WEAPONS = { ar: 'AR', br: 'BR', magnum: 'MAGNUM', sniper: 'SNIPER', shotgun: 'SHOTGUN', rocket: 'ROCKET', smg: 'SMG', sword: 'SWORD',
  carbine: 'CARBINE', beam_rifle: 'BEAM_RIFLE', ion_rifle: 'ION_RIFLE', ion_pistol: 'ION_PISTOL', shards: 'SHARDS',
  launcher: 'LAUNCHER', cannon: 'CANNON' };
const TEAMS = { red: 0, blue: 1, any: 255 };
// Caps the sim holds (crates/sim/src/consts.rs MAX_PADS) and sanity.
const MAX_PADS = 16;
/** sim::consts::MAX_MAP_BOXES: boxes and solids together. */
const MAX_BOXES = 512;
/** How far an arc's segments may stray from its circle, m. */
const ARC_ERR = 0.02;
const DIRS = ['+x', '-x', '+z', '-z'];
const MAX_LAUNCHERS = 8;
/** sim::consts::MAX_VEHICLES. */
const MAX_VEHICLES = 8;
/** sim::consts::GRAVITY, for a launcher's throw. */
const GRAVITY = 20;
/** sim::terrain::HEIGHT_Q. */
const HEIGHT_Q = 128;
/** A terrain's most vertices (sanity: a 1 m raster of a Big Team map is ~90k). */
const MAX_VERTICES = 1 << 18;

/** A box turned by `turn` degrees about y (+x toward +z at 90), then moved to `at`. */
export function place(box, at, turn) {
  const [x0, y0, z0, x1, y1, z1] = box;
  if (turn % 90) throw new Error(`turn ${turn}: quarter turns only`);
  const q = ((Math.round(turn / 90) % 4) + 4) % 4;
  const rot = ([x, z]) => [[x, z], [-z, x], [-x, -z], [z, -x]][q];
  const a = rot([x0, z0]), b = rot([x1, z1]);
  return [Math.min(a[0], b[0]) + at[0], y0 + at[1], Math.min(a[1], b[1]) + at[2], Math.max(a[0], b[0]) + at[0], y1 + at[1], Math.max(a[1], b[1]) + at[2]];
}

/** A solid turned by `turn` degrees about y (as `place`), then moved to `at`. */
export function placeSolid(sol, at, turn) {
  const q = ((Math.round(turn / 90) % 4) + 4) % 4;
  const rot = ([x, z]) => [[x, z], [-z, x], [-x, -z], [z, -x]][q];
  const o = { ...sol };
  if (sol.kind === 'ramp') {
    const [x0, z0, x1, z1] = sol.rect;
    const a = rot([x0, z0]), b = rot([x1, z1]);
    o.rect = [Math.min(a[0], b[0]) + at[0], Math.min(a[1], b[1]) + at[2], Math.max(a[0], b[0]) + at[0], Math.max(a[1], b[1]) + at[2]];
    const v = rot([[1, 0], [-1, 0], [0, 1], [0, -1]][sol.dir]);
    o.dir = v[0] > 0.5 ? 0 : v[0] < -0.5 ? 1 : v[1] > 0.5 ? 2 : 3;
    o.base = sol.base + at[1]; o.low = sol.low + at[1]; o.high = sol.high + at[1];
  } else {
    const c = rot([sol.x, sol.z]);
    o.x = c[0] + at[0]; o.z = c[1] + at[2];
    o.y0 = sol.y0 + at[1]; o.y1 = sol.y1 + at[1];
    if (sol.kind === 'turned') o.angle = sol.angle + 90 * q;
  }
  return o;
}

/** A solid's mirror image across x (sx = −1) and/or z (sz = −1). */
function mirrorSolid(sol, sx, sz) {
  const o = { ...sol };
  if (sol.kind === 'ramp') {
    const [x0, z0, x1, z1] = sol.rect;
    const xs = [x0 * sx, x1 * sx].sort((a, b) => a - b), zs = [z0 * sz, z1 * sz].sort((a, b) => a - b);
    o.rect = [xs[0], zs[0], xs[1], zs[1]];
    let d = sol.dir;
    if (sx < 0 && d < 2) d = 1 - d;
    if (sz < 0 && d >= 2) d = 5 - d;
    o.dir = d;
  } else {
    o.x = sol.x * sx; o.z = sol.z * sz;
    if (sol.kind === 'turned') {
      const r = (sol.angle * Math.PI) / 180;
      o.angle = (Math.atan2(Math.sin(r) * sz, Math.cos(r) * sx) * 180) / Math.PI;
    }
  }
  return o;
}

/** A solid item of map.json as the compiler holds it (arcs become several). */
function readSolid(it, name, fail, terrain) {
  const look = it.look || 'block';
  const ys = (what) => {
    const y = it.y;
    if (!Array.isArray(y) || y.length !== 2 || !(y[0] < y[1])) fail(`${name}: ${what} needs y: [y0, y1], y0 < y1`);
    if (y[0] < 0 && !terrain) fail(`${name}: ${what} goes under the floor`);
    return y;
  };
  if (it.round) {
    if (!(it.r >= 0.1)) fail(`${name}: a round needs r of 0.1 m or more`);
    if (it.ground) {
      // Seated on the terrain once it is placed (`seatSolid`).
      if (!terrain) fail(`${name}: a round on the ground needs a terrain`);
      if (!(it.h > 0)) fail(`${name}: a round on the ground needs h, its height`);
      return [{ kind: 'round', x: it.round[0], z: it.round[1], r: it.r, ground: true, h: it.h, y0: 0, y1: it.h, look }];
    }
    const [y0, y1] = ys('a round');
    return [{ kind: 'round', x: it.round[0], z: it.round[1], r: it.r, y0, y1, look }];
  }
  if (it.turned) {
    const [y0, y1] = ys('a turned box');
    const [w, d] = it.size || fail(`${name}: a turned box needs size: [w, d]`);
    if (!(w >= 0.2 && d >= 0.2)) fail(`${name}: a turned box is 0.2 m or more each way`);
    return [{ kind: 'turned', x: it.turned[0], z: it.turned[1], hx: w / 2, hz: d / 2, angle: it.angle || 0, y0, y1, look }];
  }
  if (it.ramp) {
    const [x0, z0, x1, z1] = it.ramp;
    if (!(x0 < x1 && z0 < z1)) fail(`${name}: a ramp is [x0, z0, x1, z1], low to high`);
    const dir = DIRS.indexOf(it.up);
    if (dir < 0) fail(`${name}: a ramp's "up" is one of ${DIRS.join(', ')}`);
    const base = it.base ?? 0, low = it.from, high = it.to;
    if (typeof low !== 'number' || typeof high !== 'number' || !(high > low) || !(low >= base)) fail(`${name}: a ramp rises "from" (≥ base) "to" (higher)`);
    const run = dir < 2 ? x1 - x0 : z1 - z0;
    if ((high - low) / run > 1) fail(`${name}: a ramp steeper than 45°`);
    return [{ kind: 'ramp', rect: [x0, z0, x1, z1], dir, base, low, high, look }];
  }
  if (it.arc) {
    const [y0, y1] = ys('an arc');
    const R = it.r, t = it.thick ?? 0.4;
    if (!(R > t) || !(t >= 0.2)) fail(`${name}: an arc needs r over its thick (0.2 m or more)`);
    const a0 = it.from, a1 = it.to;
    if (typeof a0 !== 'number' || typeof a1 !== 'number' || a0 === a1) fail(`${name}: an arc runs "from" one angle "to" another (degrees)`);
    const span = ((a1 - a0) * Math.PI) / 180;
    const step = 2 * Math.acos(Math.max(-1, 1 - ARC_ERR / (R + t / 2)));
    const n = Math.max(1, Math.ceil(Math.abs(span) / step));
    const th = span / n;
    const out = [];
    for (let i = 0; i < n; i++) {
      const phi = (a0 * Math.PI) / 180 + th * (i + 0.5);
      // Each segment as long as the outer face's chord needs, so neighbours'
      // outer corners meet.
      const hx = (R + t / 2) * Math.tan(Math.abs(th) / 2);
      out.push({ kind: 'turned', x: it.arc[0] + R * Math.cos(phi), z: it.arc[1] + R * Math.sin(phi), hx, hz: t / 2, angle: (phi * 180) / Math.PI + 90, y0, y1, look });
    }
    return out;
  }
  fail(`${name}: an item is a box [..], or { round | turned | ramp | arc }`);
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
  const SWAP = { red: 'blue', blue: 'red', red_glow: 'blue_glow', blue_glow: 'red_glow', alien_red: 'alien_blue', alien_blue: 'alien_red' };
  const swapLook = (l) => SWAP[l] || l;

  const L0 = spec.light || {};
  const light = { ev100: L0.ev100 ?? 0, sun: L0.sun ?? 1, fill: L0.fill ?? 1, lamps: [], floor: 255, sea: null };
  if (L0.floor !== undefined) {
    if (!LOOKS.includes(L0.floor)) fail(`light.floor: no look "${L0.floor}"`);
    light.floor = LOOKS.indexOf(L0.floor);
  }
  if (L0.sea !== undefined) {
    if (typeof L0.sea !== 'number' || !Number.isFinite(L0.sea)) fail('light.sea: the water\'s height, m');
    light.sea = L0.sea;
  }
  for (const [k, l] of (L0.lamps || []).entries()) {
    const what = `lamp ${k}`;
    if (!Array.isArray(l.at) || l.at.length !== 3) fail(`${what}: at is [x, z, y]`);
    const c = l.color || [1, 1, 1];
    if (!Array.isArray(c) || c.length !== 3 || c.some((v) => !(v >= 0 && v <= 1))) fail(`${what}: color is [r, g, b], 0 to 1`);
    if (!(l.lm > 0)) fail(`${what}: lm, its lumens, over 0`);
    const p = [l.at[0], l.at[2], l.at[1]];
    light.lamps.push({ p, c, lm: l.lm });
    if (l.mirror) for (const [sx, sz] of mirrors) light.lamps.push({ p: [p[0] * sx, p[1], p[2] * sz], c, lm: l.lm });
  }

  // The ground first: solids may stand on it and the rocks are seated on it.
  const terrain = spec.terrain ? heightfield(spec.terrain, [bx0, bx1, bz0, bz1], mirrors, light.sea, fail) : null;
  const groundAt = (x, z, what) => {
    const g = terrain ? terrain.ground(x, z) : null;
    if (g === null) fail(`${what} at ${x}, ${z} is off the terrain`);
    return g;
  };
  /** A placed solid marked `ground` stands on the terrain where it is. */
  const seatSolid = (s, name) => {
    if (!s.ground) return s;
    const y0 = groundAt(s.x, s.z, name);
    return { ...s, y0, y1: y0 + s.h };
  };

  const boxes = [];
  const solids = [];
  // Drawing only: boxes and solids the sim never sees (sim::map::Def::dress,
  // dress_solids).
  const dress = [];
  const dressSolids = [];
  for (const [k, s] of (spec.structures || []).entries()) {
    const name = s.name || `structure ${k}`;
    const list = s.prefab ? ((spec.prefabs || {})[s.prefab] || fail(`${name}: no prefab "${s.prefab}"`)) : (s.boxes || fail(`${name}: no boxes`));
    const at = s.at || [0, 0, 0], turn = s.turn || 0;
    const placed = [];
    const placedSolids = [];
    for (const b of list) {
      if (!Array.isArray(b)) {
        for (const sol of readSolid(b, name, fail, spec.terrain)) {
          const look = (s.tint && s.tint[sol.look]) || sol.look;
          if (!LOOKS.includes(look)) fail(`${name}: no look "${look}" (${LOOKS.join(', ')})`);
          placedSolids.push(placeSolid({ ...sol, look }, at, turn));
        }
        continue;
      }
      if (b.length < 6 || b.slice(0, 6).some((v) => typeof v !== 'number')) fail(`${name}: a box is [x0, y0, z0, x1, y1, z1, look]`);
      if (!(b[0] < b[3] && b[1] < b[4] && b[2] < b[5])) fail(`${name}: box ${JSON.stringify(b)} is empty or inside out`);
      if (b[1] < 0 && !spec.terrain) fail(`${name}: box ${JSON.stringify(b)} goes under the floor`);
      const look = (s.tint && s.tint[b[6]]) || b[6] || 'block';
      if (!LOOKS.includes(look)) fail(`${name}: no look "${look}" (${LOOKS.join(', ')})`);
      placed.push({ box: place(b, at, turn), look, dress: !!s.dress || look === 'vault' });
    }
    const into = (p) => (p.dress ? dress : boxes);
    for (const p of placed) into(p).push(p);
    const solidsOf = s.dress ? dressSolids : solids;
    solidsOf.push(...placedSolids.map((p) => seatSolid(p, name)));
    if (s.mirror) {
      if (!mirrors.length) fail(`${name}: "mirror" needs the map's "mirror"`);
      for (const [sx, sz, swap] of mirrors) {
        for (const p of placedSolids) solidsOf.push(seatSolid({ ...mirrorSolid(p, sx, sz), look: swap ? swapLook(p.look) : p.look }, name));
      }
      for (const [sx, sz, swap] of mirrors) {
        for (const p of placed) {
          const [x0, y0, z0, x1, y1, z1] = p.box;
          const xs = [x0 * sx, x1 * sx].sort((a, b) => a - b), zs = [z0 * sz, z1 * sz].sort((a, b) => a - b);
          into(p).push({ box: [xs[0], y0, zs[0], xs[1], y1, zs[1]], look: swap ? swapLook(p.look) : p.look, dress: p.dress });
        }
      }
    }
  }
  if (dress.length + dressSolids.length > MAX_BOXES) fail(`${dress.length + dressSolids.length} dressing pieces, at most ${MAX_BOXES}`);

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
  const launchers = [];
  // Both ends of every throw, for the rocks to keep clear of.
  const throwEnds = [];
  for (const [k, l] of (spec.launchers || []).entries()) {
    const what = `launcher ${k}`;
    const a = point(l.at, what), b = point(l.to, `${what}'s landing`);
    const T = l.time;
    if (!(T > 0)) fail(`${what}: "time" in seconds, over 0`);
    const [w, d] = l.size || [2, 2];
    // A landing given as [x, z] on a terrain map is seated on the ground there, each mirror image on its own.
    const seated = l.to.length === 2 && terrain;
    const one = (a, b) => {
      if (seated) {
        const g = terrain.ground(b[0], b[2]);
        if (g == null) fail(`${what}'s landing is off the terrain`);
        b = [b[0], g + LAUNCH_LAND_M, b[2]];
      }
      throwEnds.push([a[0], a[2]], [b[0], b[2]]);
      return {
        area: [a[0] - w / 2, a[1], a[2] - d / 2, a[0] + w / 2, a[1] + 0.5, a[2] + d / 2],
        // Lands on b after T: x and z at speed, y against gravity.
        vel: [(b[0] - a[0]) / T, (b[1] - a[1] + 0.5 * GRAVITY * T * T) / T, (b[2] - a[2]) / T],
      };
    };
    launchers.push(one(a, b));
    if (l.mirror) for (const [sx, sz] of mirrors) launchers.push(one([a[0] * sx, a[1], a[2] * sz], [b[0] * sx, b[1], b[2] * sz]));
  }
  if (launchers.length > MAX_LAUNCHERS) fail(`${launchers.length} launchers, at most ${MAX_LAUNCHERS}`);
  const vehicles = [];
  for (const [k, v] of (spec.vehicles || []).entries()) {
    const what = `vehicle ${k}`;
    const p = point(v.at, what);
    const yaw = v.yaw ?? 0;
    const respawn = Math.round((v.respawn ?? 30) * 1000);
    if (!(respawn > 0)) fail(`${what}: respawn in seconds, over 0`);
    vehicles.push({ pos: p, yaw, respawn });
    if (v.mirror) {
      for (const [sx, sz] of mirrors) {
        let y = yaw;
        if (sx < 0) y = 180 - y;
        if (sz < 0) y = -y;
        vehicles.push({ pos: [p[0] * sx, p[1], p[2] * sz], yaw: y, respawn });
      }
    }
  }
  if (vehicles.length > MAX_VEHICLES) fail(`${vehicles.length} vehicles, at most ${MAX_VEHICLES}`);
  if (spec.nav !== undefined && typeof spec.nav !== 'boolean') fail('nav: true or false');

  // The boulders, last: they keep clear of everything above.
  let rocks = 0;
  if (spec.rocks) {
    if (!terrain) fail('rocks need a terrain to stand on');
    const keep = { spawns, pads, roam, throwEnds, vehicles, boxes, solids };
    const planned = planRocks(spec.rocks, keep, terrain, light.sea, mirrors, [bx0, bx1, bz0, bz1], fail);
    for (const box of planned) boxes.push({ box, look: 'rock', dress: false });
    rocks = planned.length;
  }
  if (boxes.length + solids.length > MAX_BOXES) fail(`${boxes.length} boxes and ${solids.length} solids, at most ${MAX_BOXES} together`);
  return { id: spec.id, name: spec.name, about: spec.about || '', practice: !!spec.practice, nav: !!spec.nav, light, lo: [bx0, bz0], hi: [bx1, bz1], boxes, solids, spawns, pads, roam, terrain, launchers, vehicles, dress, dressSolids, rocks };
}

// ----------------------------------------------------------- terrain maths
// What the raster is made of. Only + − × ÷, Math.sqrt, floor, round, min,
// max, abs and 32-bit integer mixing: each is exact by the spec, so the
// heights are the same on every machine and `--check` holds. No Math.cos,
// sin, exp, pow or hypot (an engine's own, to the last bit). Every curve is
// C1: a gate is `fade`d, never a bare clamp, because the client shades the
// raster and a slope step in a 1-D curve draws a contour line across the
// whole map at that height (Gates' tests/contour.rs lesson).

/** Quintic fade t³(10 − 15t + 6t²): C² at 0 and 1. */
const fade = (t) => t * t * t * (t * (t * 6 - 15) + 10);
const clamp01 = (t) => (t < 0 ? 0 : t > 1 ? 1 : t);
/** 1 at t = 0, 0 from |t| = 1, flat at both. */
const bump = (t) => 1 - fade(Math.min(Math.abs(t), 1));
/** max(a, b), rounded off within k/2 where they meet. */
const smoothmax = (a, b, k) => 0.5 * (a + b + Math.sqrt((a - b) * (a - b) + k * k));
/** How softly the wading shelf meets the beach and the seabed (m): a C1 join, a hand's width either side. */
const SHELF_BLEND_M = 0.6;
/** A launcher's landing authored as [x, z] on a terrain map aims this far over the ground there, so the body lands a step before its mark rather than clipping into a rise. */
const LAUNCH_LAND_M = 0.5;
const dist = (dx, dz) => Math.sqrt(dx * dx + dz * dz);
/** 1 in the middle, 0 from r: a shape's falloff. */
const fall = (d, r) => (d >= r ? 0 : 1 - fade(d / r));
/** How far (x, z) is from the segment a–b. */
function segDist(ax, az, bx, bz, x, z) {
  const vx = bx - ax, vz = bz - az, L = vx * vx + vz * vz;
  const u = L ? clamp01(((x - ax) * vx + (z - az) * vz) / L) : 0;
  return dist(x - (ax + u * vx), z - (az + u * vz));
}

/** A 32-bit hash of (ix, iz, key) to [0, 1): Gates' face.rs relief_hash. */
function hash01(ix, iz, key) {
  let h = (Math.imul(ix, 0x8da6b343) ^ Math.imul(iz, 0xd8163841) ^ Math.imul(key, 0xcb1ab31f)) >>> 0;
  h ^= h >>> 15;
  h = Math.imul(h, 0x2c1b3c6d) >>> 0;
  h ^= h >>> 12;
  h = Math.imul(h, 0x297a2d39) >>> 0;
  h ^= h >>> 15;
  return (h >>> 8) / 16777216;
}

/** Quintic-faded value noise in [0, 1) on a unit lattice (Gates' relief_noise). */
function vnoise(x, z, key) {
  const x0 = Math.floor(x), z0 = Math.floor(z);
  const u = fade(x - x0), v = fade(z - z0);
  const c00 = hash01(x0, z0, key), c10 = hash01(x0 + 1, z0, key);
  const c01 = hash01(x0, z0 + 1, key), c11 = hash01(x0 + 1, z0 + 1, key);
  const a = c00 + (c10 - c00) * u, b = c01 + (c11 - c01) * u;
  return a + (b - a) * v;
}

/** fBm of `vnoise`: lacunarity 2, gain ½, each octave its own channel; zero-mean, within −1..1. */
function fbm(x, z, key, freq, octaves) {
  let sum = 0, amp = 1, norm = 0, f = freq;
  for (let o = 0; o < octaves; o++) {
    sum += (vnoise(x * f, z * f, key + o) * 2 - 1) * amp;
    norm += amp;
    amp *= 0.5;
    f *= 2;
  }
  return sum / norm;
}

/** A field in [0, 1] that is 1 along the noise's crests, which meander as lines (Gates' face.rs ridged). */
function ridged(x, z, key) {
  const r = 1 - Math.abs(vnoise(x, z, key) * 2 - 1);
  return r * r;
}

/** A monotone cubic through `knots` [[x, y], ...]: Fritsch–Carlson tangents
 * inside (the secants' weighted harmonic mean, capped at 3× the smaller, which
 * keeps it monotone), zero at both ends so it meets a flat curve C1 on either
 * side (Gates' terrain.rs remap). Returns `at(x)`. */
function pchip(knots, fail, what) {
  const n = Array.isArray(knots) ? knots.length : 0;
  if (n < 2 || knots.some((k) => !Array.isArray(k) || k.length !== 2 || k.some((v) => typeof v !== 'number'))) fail(`${what}: two or more [x, y] knots`);
  const xs = knots.map((k) => k[0]), ys = knots.map((k) => k[1]);
  const h = [], d = [];
  for (let i = 0; i < n - 1; i++) {
    if (!(xs[i + 1] > xs[i])) fail(`${what}: knots in rising x`);
    h.push(xs[i + 1] - xs[i]);
    d.push((ys[i + 1] - ys[i]) / h[i]);
  }
  const m = new Array(n).fill(0);
  for (let i = 1; i < n - 1; i++) {
    if (d[i - 1] * d[i] > 0) {
      const w1 = 2 * h[i] + h[i - 1], w2 = h[i] + 2 * h[i - 1];
      let t = (w1 + w2) / (w1 / d[i - 1] + w2 / d[i]);
      const cap = 3 * Math.min(Math.abs(d[i - 1]), Math.abs(d[i]));
      if (Math.abs(t) > cap) t = cap * Math.sign(t);
      m[i] = t;
    }
  }
  return (x) => {
    if (x <= xs[0]) return ys[0];
    if (x >= xs[n - 1]) return ys[n - 1];
    let i = 0;
    while (i < n - 2 && x >= xs[i + 1]) i++;
    const u = (x - xs[i]) / h[i], u2 = u * u, u3 = u2 * u;
    // The Hermite basis written out.
    const h00 = 2 * u3 - 3 * u2 + 1, h10 = u3 - 2 * u2 + u, h01 = -2 * u3 + 3 * u2, h11 = u3 - u2;
    return h00 * ys[i] + h01 * ys[i + 1] + h[i] * (h10 * m[i] + h11 * m[i + 1]);
  };
}

/** A canyon around the shapes, from the terrain's `canyon` block: the
 * walls' scree, cliff and mesa by distance outside a floor whose width
 * wobbles, pockets and flares, warped so nothing runs straight; gullies
 * shifting the cliff line, crags on the mesa, strata benches on the cliff;
 * the +x end dropping under the sea with a beach terrace. After Gates'
 * terrain.rs (the fade'd gates, the PCHIP profile, `shore_terrace`) and
 * face.rs (the f³ − f beds, `ridged`). All of it in metres:
 *   seed      the noise's
 *   detail    { h, wave, octaves }: fBm over the whole floor, ±h at `wave` m
 *   warp      { m, wave, octaves }: the walls' coordinates moved by ±m
 *   floor     { half, wobble, wave, octaves }: the floor's half-width across
 *             z, ±wobble along x at `wave` m
 *   pockets   { w, half, at: [[x, side], ...] }: bays w deeper over ±half m
 *             around x on side −1 (−z) or +1 (+z)
 *   flare     { w, from, over }: the floor opening by w from x `from` over
 *             `over` m (the sea end)
 *   end       { x, blend }: the closed end's wall at x (the −x bound), its
 *             corner rounded over `blend`
 *   gullies   { m, wave, at, half }: the cliff line pushed back up to m along
 *             a ridged field at `wave` m, over the band ±half around `at`
 *   scree     { h, w }: the bank rising h over w m to the cliff's foot
 *   cliff     [[e, h], ...]: the cliff's profile, height by distance past the
 *             scree, a monotone cubic; starts at [0, scree.h]
 *   mesa      { rise, over }: over the cliff's top, rising `rise` more over
 *             `over` m
 *   crags     { h, wave, from, over }: a ridged relief on the mesa, h at
 *             `wave` m, fading in from `from` over `over` m past the cliff's
 *             foot
 *   strata    { h, bed, at, half }: benches every `bed` m up the cliff, ±h,
 *             over the band ±half around `at`
 *   sea       { drops: [[m, from, over], ...], walls: [from, over],
 *             terrace: [h, k, from, over] }: the +x end dropping by each m
 *             from x `from` over `over` m, the walls sinking from `walls[0]`
 *             over `walls[1]`, and the beach terrace: within h above the
 *             water (light.sea) the slope eases to k of itself, from x
 *             `from` over `over` m (the floor behind sits in the same band
 *             and keeps its authored shape) */
function canyon(c, sea, fail) {
  if (!c || typeof c !== 'object') fail('terrain: gen "canyon" needs a "canyon" block');
  if (typeof sea !== 'number') fail('canyon: needs light.sea, the water it ends in');
  const num = (o, k, what) => {
    const v = o?.[k];
    if (typeof v !== 'number' || !Number.isFinite(v)) fail(`canyon.${what}: a number`);
    return v;
  };
  const seed = num(c, 'seed', 'seed') | 0;
  const K = (n) => (Math.imul(seed, 0x9e3779b1) + n * 16) | 0;
  const [dH, dWave, dOct] = [num(c.detail, 'h', 'detail.h'), num(c.detail, 'wave', 'detail.wave'), c.detail.octaves ?? 2];
  const [wM, wWave, wOct] = [num(c.warp, 'm', 'warp.m'), num(c.warp, 'wave', 'warp.wave'), c.warp.octaves ?? 2];
  const [fHalf, fWobble, fWave, fOct] = [num(c.floor, 'half', 'floor.half'), num(c.floor, 'wobble', 'floor.wobble'), num(c.floor, 'wave', 'floor.wave'), c.floor.octaves ?? 2];
  const pockets = c.pockets?.at ?? [];
  if (!Array.isArray(pockets) || pockets.some((p) => !Array.isArray(p) || p.length !== 2 || typeof p[0] !== 'number' || ![-1, 1].includes(p[1]))) fail('canyon.pockets.at: [[x, -1 | 1], ...]');
  const [pW, pHalf] = pockets.length ? [num(c.pockets, 'w', 'pockets.w'), num(c.pockets, 'half', 'pockets.half')] : [0, 1];
  const [flW, flFrom, flOver] = [num(c.flare, 'w', 'flare.w'), num(c.flare, 'from', 'flare.from'), num(c.flare, 'over', 'flare.over')];
  const [endX, endBlend] = [num(c.end, 'x', 'end.x'), num(c.end, 'blend', 'end.blend')];
  const [gM, gWave, gAt, gHalf] = [num(c.gullies, 'm', 'gullies.m'), num(c.gullies, 'wave', 'gullies.wave'), num(c.gullies, 'at', 'gullies.at'), num(c.gullies, 'half', 'gullies.half')];
  const [screeH, screeW] = [num(c.scree, 'h', 'scree.h'), num(c.scree, 'w', 'scree.w')];
  const cliff = pchip(c.cliff, fail, 'canyon.cliff');
  const [cliffFoot, footH] = c.cliff[0], [cliffTop, topH] = c.cliff[c.cliff.length - 1];
  // The scree's flat top is the cliff's foot: continuous, and C1 with both
  // end tangents zero.
  if (cliffFoot !== 0 || footH !== screeH) fail('canyon.cliff: starts at [0, scree.h], where the scree ends');
  const [mRise, mOver] = [num(c.mesa, 'rise', 'mesa.rise'), num(c.mesa, 'over', 'mesa.over')];
  const [crH, crWave, crFrom, crOver] = [num(c.crags, 'h', 'crags.h'), num(c.crags, 'wave', 'crags.wave'), num(c.crags, 'from', 'crags.from'), num(c.crags, 'over', 'crags.over')];
  const [stH, bed, stAt, stHalf] = [num(c.strata, 'h', 'strata.h'), num(c.strata, 'bed', 'strata.bed'), num(c.strata, 'at', 'strata.at'), num(c.strata, 'half', 'strata.half')];
  const drops = c.sea?.drops;
  if (!Array.isArray(drops) || drops.some((d) => !Array.isArray(d) || d.length !== 3 || d.some((v) => typeof v !== 'number'))) fail('canyon.sea.drops: [[m, from, over], ...]');
  const walls = c.sea?.walls;
  if (!Array.isArray(walls) || walls.length !== 2 || walls.some((v) => typeof v !== 'number')) fail('canyon.sea.walls: [from, over]');
  const terr = c.sea?.terrace;
  if (!Array.isArray(terr) || terr.length !== 4 || !(terr[0] > 0) || !(terr[1] >= 0 && terr[1] <= 1) || typeof terr[2] !== 'number' || !(terr[3] > 0)) fail('canyon.sea.terrace: [h, k, from, over], k 0..1');
  const [tH, tK, tFrom, tOver] = terr;
  const shelf = c.sea?.shelf;
  if (!Array.isArray(shelf) || shelf.length !== 4 || !(shelf[0] > 0) || typeof shelf[1] !== 'number' || !(shelf[2] >= 0) || !(shelf[3] > 0)) fail('canyon.sea.shelf: [depth, to, drop, over]');
  const [shDepth, shTo, shDrop, shOver] = shelf;
  // Gates' shore_terrace: f(h) = h − C·H·u²(1−u)² over the band, identity
  // outside it, f(0) = 0, f'(0) = 1, slope k of itself at the berm, C =
  // (1 − k)·3√3.
  const tC = (1 - tK) * 3 * Math.sqrt(3);

  const detail = (x, z) => dH * fbm(x, z, K(1), 1 / dWave, dOct);
  const warp = (x, z) => [x + wM * fbm(x, z, K(2), 1 / wWave, wOct), z + wM * fbm(x, z, K(3), 1 / wWave, wOct)];
  /** The floor's half-width at xw on `side`. */
  const half = (xw, side) => {
    let W = fHalf + fWobble * fbm(xw / fWave, 0, K(4), 1, fOct);
    for (const [xp, s] of pockets) if (s === side) W += pW * bump((xw - xp) / pHalf);
    return W + flW * fade(clamp01((xw - flFrom) / flOver));
  };
  /** How far outside the floor (xw, zw) is: negative inside. */
  const outside = (xw, zw) => smoothmax(Math.abs(zw) - half(xw, zw < 0 ? -1 : 1), endX - xw, endBlend);
  /** The cliff line pushed back along the gullies. */
  const shifted = (xw, e) => e + gM * ridged(xw / gWave, 0, K(5)) * bump((e - gAt) / gHalf);
  /** The wall's height by distance past the scree's foot. */
  const profile = (e) => {
    if (e < -screeW) return 0;
    if (e < 0) return screeH * fade((e + screeW) / screeW);
    if (e < cliffTop) return cliff(e);
    return topH + mRise * fade(clamp01((e - cliffTop) / mOver));
  };
  const wall = (xw, zw, e) => {
    let h = profile(e) + crH * ridged(xw / crWave, zw / crWave, K(6)) * fade(clamp01((e - crFrom) / crOver));
    // Beds f³ − f: flat at each one's foot, sheer at its top, C1 across.
    const b = h / bed, p = 2 * (b - Math.floor(b)) - 1;
    return h + stH * (p * p * p - p) * bump((e - stAt) / stHalf);
  };
  const drop = (x) => {
    let d = 0;
    for (const [m, from, over] of drops) d += m * fade(clamp01((x - from) / over));
    return d;
  };
  const headland = (x) => 1 - fade(clamp01((x - walls[0]) / walls[1]));
  return {
    /** What the canyon adds to the floor at (x, z): the wall and the floor's detail, less the sea end's drop. */
    relief(x, z) {
      const [xw, zw] = warp(x, z);
      return wall(xw, zw, shifted(xw, outside(xw, zw))) * headland(x) - drop(x) + detail(x, z);
    },
    /** The beach: the finished height at x eased within the band over the water. */
    terrace(h, x) {
      const u = (h - sea) / tH;
      if (u <= 0 || u >= 1) return h;
      const a = u * (1 - u);
      return h - tC * tH * a * a * fade(clamp01((x - tFrom) / tOver));
    },
    /** The wading shelf: the seabed held to `depth` under the water as far as x `to` (the bound), falling `drop` m over `over` past it. */
    shelf(h, x) {
      return smoothmax(h, sea - shDepth - shDrop * fade(clamp01((x - shTo) / shOver)), SHELF_BLEND_M);
    },
    /** How far (x, z) stands outside the floor's edge (negative inside), as the walls see it. */
    outside(x, z) {
      const [xw, zw] = warp(x, z);
      return shifted(xw, outside(xw, zw));
    },
  };
}

/** A terrain block rasterised: vertex heights in 1/HEIGHT_Q m, a sampler
 * of them, and what the rocks planner keeps clear of. */
function heightfield(t, [bx0, bx1, bz0, bz1], mirrors, sea, fail) {
  const cell = t.cell ?? 2;
  if (![0.5, 1, 2, 4].includes(cell)) fail('terrain: cell is 0.5, 1, 2 or 4');
  let margin = t.margin ?? 4;
  if (typeof margin === 'number') margin = [margin, margin, margin, margin];
  if (!Array.isArray(margin) || margin.length !== 4 || margin.some((v) => !(v >= 0))) fail('terrain: margin is m, or [-x, +x, -z, +z] m');
  const [mx0, mx1, mz0, mz1] = margin;
  const x0 = Math.floor((bx0 - mx0) / cell) * cell, z0 = Math.floor((bz0 - mz0) / cell) * cell;
  const nx = Math.ceil((bx1 + mx1 - x0) / cell) + 1, nz = Math.ceil((bz1 + mz1 - z0) / cell) + 1;
  if (nx * nz > MAX_VERTICES || nx > 65535 || nz > 65535) fail(`terrain: ${nx} × ${nz} vertices, too many`);
  const shapes = [];
  const padsOf = [];
  // What the rocks keep clear of: the creeks (dips along a line) and the
  // bases' pads.
  const creeks = [];
  const padDiscs = [];
  for (const [k, s] of (t.shapes || []).entries()) {
    const imgs = [[1, 1]].concat(s.mirror ? mirrors.map(([sx, sz]) => [sx, sz]) : []);
    for (const [sx, sz] of imgs) {
      if (s.hill) {
        const [cx, cz] = s.hill, r = s.r, h = s.h;
        if (!(r > 0) || typeof h !== 'number') fail(`terrain shape ${k}: a hill needs r > 0 and h`);
        shapes.push((x, z) => h * fall(dist(x - cx * sx, z - cz * sz), r));
      } else if (s.ridge) {
        const [[ax, az], [qx, qz]] = s.ridge, w = s.w, h = s.h;
        if (!(w > 0) || typeof h !== 'number') fail(`terrain shape ${k}: a ridge needs w > 0 and h`);
        const [x1, z1, x2, z2] = [ax * sx, az * sz, qx * sx, qz * sz];
        shapes.push((x, z) => h * fall(segDist(x1, z1, x2, z2, x, z), w));
        if (h < 0) creeks.push([x1, z1, x2, z2, w]);
      } else if (s.plateau) {
        const [p0, q0, p1, q1] = s.plateau, h = s.h, e = s.edge ?? 4;
        if (typeof h !== 'number' || !(e > 0)) fail(`terrain shape ${k}: a plateau needs h and edge > 0`);
        const [xa, xb] = [p0 * sx, p1 * sx].sort((a, b) => a - b), [za, zb] = [q0 * sz, q1 * sz].sort((a, b) => a - b);
        shapes.push((x, z) => h * fall(dist(Math.max(xa - x, 0, x - xb), Math.max(za - z, 0, z - zb)), e));
      } else if (s.pad) {
        const [cx, cz] = s.pad, r = s.r, h = s.h, blend = s.blend;
        if (!(r > 0) || typeof h !== 'number' || !(blend > 0)) fail(`terrain shape ${k}: a pad needs r > 0, h and blend > 0`);
        const [px, pz] = [cx * sx, cz * sz];
        padsOf.push((x, z, raw) => {
          const d = dist(x - px, z - pz);
          if (d >= r + blend) return raw;
          return raw + (h - raw) * (1 - fade(clamp01((d - r) / blend)));
        });
        padDiscs.push([px, pz, r + blend]);
      } else fail(`terrain shape ${k}: a hill, a ridge, a plateau or a pad`);
    }
  }
  let gen = null;
  if (t.gen !== undefined) {
    if (t.gen !== 'canyon') fail(`terrain: gen "${t.gen}": "canyon" or none`);
    gen = canyon(t.canyon, sea, fail);
  }
  const base = t.base ?? 0;
  const heights = [];
  for (let j = 0; j < nz; j++) {
    for (let i = 0; i < nx; i++) {
      const x = x0 + i * cell, z = z0 + j * cell;
      let h = shapes.reduce((a, f) => a + f(x, z), base);
      if (gen) h = gen.shelf(gen.terrace(h + gen.relief(x, z), x), x);
      for (const p of padsOf) h = p(x, z, h);
      const q = Math.round(h * HEIGHT_Q);
      if (q < -32768 || q > 32767) fail(`terrain: ${h} m at ${x}, ${z} is out of range`);
      heights.push(q);
    }
  }
  /** The ground at (x, z) as the sim's Terrain::height samples it (each cell
   * two triangles split from its low corner), or null off the raster. */
  const ground = (x, z) => {
    const u = (x - x0) / cell, v = (z - z0) / cell;
    if (!(u >= 0 && v >= 0 && u <= nx - 1 && v <= nz - 1)) return null;
    const i = Math.min(Math.floor(u), nx - 2), j = Math.min(Math.floor(v), nz - 2);
    const fu = u - i, fv = v - j;
    const h = (a, b) => heights[b * nx + a] / HEIGHT_Q;
    const h00 = h(i, j), h10 = h(i + 1, j), h01 = h(i, j + 1), h11 = h(i + 1, j + 1);
    return fu >= fv ? h00 + (h10 - h00) * fu + (h11 - h10) * fv : h00 + (h11 - h01) * fu + (h01 - h00) * fv;
  };
  return { cell, x0, z0, nx, nz, heights, ground, gen, creeks, padDiscs };
}

// ------------------------------------------------------------ the boulders

/** Boulder formations planned from a seed and seated on the terrain, as
 * axis-aligned rock boxes: Gates' sim-core/src/boulder.rs (MIT) at a
 * fraction of its island's sizes. One formation a cell at most (an outcrop
 * of slabs strung along a line with a cap or two, a tor stacking up with a
 * fallen block at its foot, or a pile), and lone strays; a block's top
 * stands its height over the ground at its middle and 0.4 of it over every
 * corner, its bottom is buried under the lowest. Planned where the map's
 * mirror puts its first image, then mirrored, each image seated on its own
 * ground; a block too near anything the game needs clear (spawns, pads, a
 * launcher's ends, roam points, vehicles, the bases' pads, the creeks, the
 * trees, the structures) is dropped, and with it its formation if it was
 * the first. The `rocks` block, metres:
 *   seed      the planner's
 *   cell      the planning grid (40); inset: how far inside a cell an
 *             anchor keeps (8)
 *   x         [x0, x1]: where anchors may lie (the first image's half)
 *   floor     how far inside the floor's edge a block stays (4)
 *   density   the share of cells that grow a formation (0.6); strays: the
 *             share of stray candidates (four a cell) kept (0.25)
 *   big       a formation's biggest half-extent (6); body: how far from its
 *             anchor a formation reaches (16)
 *   stray     [lo, hi]: a stray's half-extent (quadratic skew small); strayMinH
 *   slope     the steepest ground a block seats on, rise over run (0.5)
 *   clear     { spawn, pad, launcher, roam, vehicle, tree, box, creek, base }:
 *             the gap kept to each, beyond the block's own radius
 *   max       the most boxes, all images together (60) */
function planRocks(rk, keep, t, sea, mirrors, [bx0, bx1, bz0, bz1], fail) {
  const num = (k, d) => {
    const v = rk[k] ?? d;
    if (typeof v !== 'number' || !Number.isFinite(v)) fail(`rocks.${k}: a number`);
    return v;
  };
  const seed = num('seed', 1) | 0;
  const cell = num('cell', 40), inset = num('inset', 8);
  const [rx0, rx1] = rk.x ?? [bx0, bx1];
  if (!(rx0 < rx1)) fail('rocks.x: [x0, x1]');
  const floorIn = num('floor', 4);
  const density = num('density', 0.6), strayDensity = num('strays', 0.25);
  const big = num('big', 6), body = num('body', 16);
  const [strayLo, strayHi] = rk.stray ?? [1.2, 2.4];
  if (!(strayLo > 0 && strayHi >= strayLo)) fail('rocks.stray: [lo, hi] half-extents');
  const strayMinH = num('strayMinH', 1.4);
  const maxSlope = num('slope', 0.5);
  const max = num('max', 60);
  const clr = { spawn: 5, pad: 4, launcher: 6, roam: 3, vehicle: 7, tree: 2, box: 2, creek: 3, base: 0, ...(rk.clear || {}) };
  /** The smallest half-extent a block keeps; the most blocks a formation holds. */
  const HALF_FLOOR = 0.5, BLOCKS = 8;
  const STRAY_INSET = 3;
  const K = (n) => (Math.imul(seed, 0x85ebca6b) + n) | 0;
  /** The eight compass directions, unit length (no trig: √½ is a constant). */
  const DIRS8 = [[1, 0], [Math.SQRT1_2, Math.SQRT1_2], [0, 1], [-Math.SQRT1_2, Math.SQRT1_2], [-1, 0], [-Math.SQRT1_2, -Math.SQRT1_2], [0, -1], [Math.SQRT1_2, -Math.SQRT1_2]];
  /** A cell's stream of uniform draws, one hash each, channel after channel. */
  const draw = (bx, bz, ch0) => {
    let ch = ch0;
    const unit = () => hash01(bx, bz, K(ch++));
    return { unit, range: (lo, hi) => lo + (hi - lo) * unit() };
  };
  const radius = (b) => dist(b.hx, b.hz);
  /** A block: its middle, half-extents (x, z), planned height, what it rests on (−1: the ground). */
  const block = (x, z, hx, hz, hgt, on) => ({ x, z, hx, hz, hgt, on });
  /** Half-extents along and across an axis, turned by `q` quarter turns. */
  const turned = (q, along, across) => (q & 1 ? [across, along] : [along, across]);
  const push = (f, b, ax, az) => {
    if (b.hx < HALF_FLOOR || b.hz < HALF_FLOOR || f.length >= BLOCKS) return -1;
    if (dist(b.x - ax, b.z - az) + radius(b) > body) return -1;
    f.push(b);
    return f.length - 1;
  };
  const stackOn = (f, d, parent, lo, hi, ax, az) => {
    const p = f[parent];
    const hx = p.hx * d.range(lo, hi), hz = p.hz * d.range(lo, hi);
    const x = p.x + d.range(-0.3, 0.3) * p.hx, z = p.z + d.range(-0.3, 0.3) * p.hz;
    const hgt = Math.min(hx, hz) * d.range(0.9, 1.6);
    return push(f, block(x, z, hx, hz, hgt, parent), ax, az);
  };
  /** Slabs strung out along an axis, biggest in the middle, overlapping, a smaller one or two on top. */
  const outcrop = (f, d, ax, az, big, q) => {
    const n = 2 + Math.floor(d.unit() * 3);
    const half = [];
    for (let k = 0; k < n; k++) {
      const mid = 1 - Math.abs(k - (n - 1) * 0.5) / n;
      half.push(big * d.range(0.5, 0.75) * (0.55 + 0.45 * mid));
    }
    const [cx, cz] = [[1, 0], [0, 1], [-1, 0], [0, -1]][q & 3];
    const [px, pz] = [-cz, cx];
    let along = -half.reduce((a, v) => a + v, 0) * 1.6 * 0.5;
    let main = -1, widest = 0;
    for (const hx of half) {
      along += hx * 0.8;
      const hz = hx * d.range(0.45, 0.8);
      const lat = d.range(-0.35, 0.35) * hz;
      const [ex, ez] = turned(q, hx, hz);
      const hgt = hz * d.range(0.8, 1.5);
      const i = push(f, block(ax + cx * along + px * lat, az + cz * along + pz * lat, ex, ez, hgt, -1), ax, az);
      if (i >= 0 && hx > widest) {
        widest = hx;
        main = i;
      }
      along += hx * 0.8;
    }
    if (main >= 0) {
      const tops = Math.floor(d.unit() * 3);
      for (let k = 0; k < tops; k++) stackOn(f, d, main, 0.3, 0.55, ax, az);
    }
  };
  /** A stack of shrinking blocks on a broad base, with a fallen block or two at its foot. */
  const tor = (f, d, ax, az, big, q) => {
    const hx = big * d.range(0.35, 0.55), hz = hx * d.range(0.7, 1.0);
    const hgt = hx * d.range(0.8, 1.4);
    const [ex, ez] = turned(q, hx, hz);
    let top = push(f, block(ax, az, ex, ez, hgt, -1), ax, az);
    if (top < 0) return;
    const tiers = 1 + Math.floor(d.unit() * 3);
    for (let k = 0; k < tiers; k++) {
      const i = stackOn(f, d, top, 0.6, 0.85, ax, az);
      if (i < 0) break;
      top = i;
    }
    const fallen = Math.floor(d.unit() * 3);
    for (let k = 0; k < fallen; k++) {
      const [dx, dz] = DIRS8[Math.floor(d.unit() * 8)];
      const r = Math.max(hx, hz) * d.range(1.2, 1.8);
      const fx = hx * d.range(0.25, 0.45);
      const [fex, fez] = turned(Math.floor(d.unit() * 4), fx, fx * d.range(0.6, 1.0));
      push(f, block(ax + dx * r, az + dz * r, fex, fez, fx * d.range(0.8, 1.3), -1), ax, az);
    }
  };
  /** A tumble of middling blocks, a few resting on the others. */
  const pile = (f, d, ax, az, big) => {
    const n = 4 + Math.floor(d.unit() * 4);
    const spread = big * 0.9;
    for (let k = 0; k < n; k++) {
      if (f.length > 0 && d.unit() < 0.3) {
        const on = Math.floor(d.unit() * f.length);
        if (f[on].on < 0) {
          stackOn(f, d, on, 0.4, 0.7, ax, az);
          continue;
        }
      }
      const [dx, dz] = DIRS8[Math.floor(d.unit() * 8)];
      const r = spread * Math.sqrt(d.unit());
      const hx = big * d.range(0.2, 0.42);
      const [ex, ez] = turned(Math.floor(d.unit() * 4), hx, hx * d.range(0.6, 1.0));
      push(f, block(ax + dx * r, az + dz * r, ex, ez, hx * d.range(0.8, 1.4), -1), ax, az);
    }
  };
  /** A cell's formation, from its hash alone: its roll against the density, and the blocks. */
  const plan = (bx, bz) => {
    const d = draw(bx, bz, 0);
    const roll = d.unit();
    const span = cell - 2 * inset;
    const ax = bx * cell + inset + d.unit() * span, az = bz * cell + inset + d.unit() * span;
    // Size skewed small: most formations are house-sized, a few are hills.
    const t = d.unit();
    const size = big * (0.3 + 0.7 * t * Math.sqrt(t));
    const shape = d.unit();
    const q = Math.floor(d.unit() * 4);
    const f = [];
    if (shape < 0.55) outcrop(f, d, ax, az, size, q);
    else if (shape < 0.85) tor(f, d, ax, az, size, q);
    else pile(f, d, ax, az, size);
    return { roll, ax, f };
  };

  // Every image of the map's mirror: a block is kept only where all of them
  // are, so both halves match.
  const imgs = [[1, 1]].concat(mirrors.map(([sx, sz]) => [sx, sz]));
  const perImage = Math.floor(max / imgs.length);
  const boxDist = (b, x, z) => dist(Math.max(b[0] - x, 0, x - b[3]), Math.max(b[2] - z, 0, z - b[5]));
  /** Whether a block at (x, z) of plan radius r keeps clear of everything the game needs. */
  const clear = (x, z, r) => {
    const near = (px, pz, gap) => dist(x - px, z - pz) < r + gap;
    if (keep.spawns.some((s) => near(s.pos[0], s.pos[2], clr.spawn))) return false;
    if (keep.pads.some((p) => near(p.pos[0], p.pos[2], clr.pad))) return false;
    if (keep.throwEnds.some(([px, pz]) => near(px, pz, clr.launcher))) return false;
    if (keep.roam.some(([px, pz]) => near(px, pz, clr.roam))) return false;
    if (keep.vehicles.some((v) => near(v.pos[0], v.pos[2], clr.vehicle))) return false;
    if (keep.boxes.some((b) => boxDist(b.box, x, z) < r + clr.box)) return false;
    if (keep.solids.some((s) => (s.kind === 'ramp' ? boxDist([s.rect[0], 0, s.rect[1], s.rect[2], 0, s.rect[3]], x, z) < r + clr.tree : near(s.x, s.z, (s.r ?? Math.max(s.hx, s.hz)) + clr.tree)))) return false;
    if (t.padDiscs.some(([px, pz, pr]) => near(px, pz, pr + clr.base))) return false;
    if (t.creeks.some(([ax, az, qx, qz, w]) => segDist(ax, az, qx, qz, x, z) < r + w + clr.creek)) return false;
    return true;
  };
  /** Seat a ground block of this footprint at (x, z): its box's y0 and y1, or null where it can't stand. */
  const seat = (x, z, hx, hz, hgt) => {
    if (!(x >= bx0 && x <= bx1 && z >= bz0 && z <= bz1)) return null;
    if (t.gen && t.gen.outside(x, z) >= -floorIn) return null;
    const g = (px, pz) => t.ground(px, pz);
    const gc = g(x, z);
    if (gc === null) return null;
    const gxp = g(x + 1, z), gxm = g(x - 1, z), gzp = g(x, z + 1), gzm = g(x, z - 1);
    if (gxp === null || gxm === null || gzp === null || gzm === null) return null;
    if (dist((gxp - gxm) * 0.5, (gzp - gzm) * 0.5) > maxSlope) return null;
    let lo = gc, y1 = gc + hgt;
    for (const [cx, cz] of [[hx, hz], [hx, -hz], [-hx, hz], [-hx, -hz]]) {
      const gk = g(x + cx, z + cz);
      if (gk === null) return null;
      lo = Math.min(lo, gk);
      y1 = Math.max(y1, gk + hgt * 0.4);
    }
    if (sea !== null && lo < sea + 1) return null;
    return [lo - (0.5 + hgt * 0.3), y1];
  };
  /** A block's images, seated: null if any image fails. */
  const settle = (b, parents) => {
    const out = [];
    for (const [k, [sx, sz]] of imgs.entries()) {
      const x = b.x * sx, z = b.z * sz;
      if (k === 0) {
        if (x < rx0 || x > rx1) return null;
        // Its images must not meet it across the mirror.
        for (const [mx, mz] of mirrors) {
          if (mx < 0 && x + b.hx > -0.5) return null;
          if (mz < 0 && z + b.hz > -0.5) return null;
        }
      }
      if (!clear(x, z, radius(b))) return null;
      let ys;
      if (parents) {
        const p = parents[k];
        ys = [p.y1 - 0.3, p.y1 + b.hgt];
      } else {
        ys = seat(x, z, b.hx, b.hz, b.hgt);
        if (!ys) return null;
      }
      out.push({ x, z, hx: b.hx, hz: b.hz, y0: ys[0], y1: ys[1] });
    }
    return out;
  };

  const kept = imgs.map(() => []);
  const cx0 = Math.floor(rx0 / cell), cx1 = Math.ceil(rx1 / cell) - 1;
  const cz0 = Math.floor(bz0 / cell), cz1 = Math.ceil(bz1 / cell) - 1;
  // The formations, cell by cell.
  for (let bz = cz0; bz <= cz1; bz++) {
    for (let bx = cx0; bx <= cx1; bx++) {
      const { roll, f } = plan(bx, bz);
      if (roll >= density || !f.length) continue;
      const got = [];
      const by = new Map();
      for (const [i, b] of f.entries()) {
        let parents = null;
        if (b.on >= 0) {
          parents = by.get(b.on);
          if (!parents) continue;
        }
        const s = settle(b, parents);
        if (!s) {
          // A first block that can't stand takes its formation with it.
          if (i === 0) {
            got.length = 0;
            break;
          }
          continue;
        }
        by.set(i, s);
        got.push(s);
      }
      if (!got.length || kept[0].length + got.length > perImage) continue;
      for (const s of got) for (const [k, img] of s.entries()) kept[k].push(img);
    }
  }
  // The strays: four candidates a cell, kept apart from every block.
  for (let bz = cz0; bz <= cz1; bz++) {
    for (let bx = cx0; bx <= cx1; bx++) {
      const d = draw(bx, bz, 1000);
      const span = cell - 2 * STRAY_INSET;
      for (let k = 0; k < 4; k++) {
        const roll = d.unit();
        const x = bx * cell + STRAY_INSET + d.unit() * span, z = bz * cell + STRAY_INSET + d.unit() * span;
        // Skewed small: most are a car, a few are a cottage.
        const u = d.unit();
        const hx = strayLo + (strayHi - strayLo) * u * u;
        const hz = hx * d.range(0.65, 1.0);
        const hgt = Math.max(hz * d.range(1.0, 1.5), strayMinH);
        const [ex, ez] = turned(Math.floor(d.unit() * 4), hx, hz);
        if (roll >= strayDensity || kept[0].length >= perImage) continue;
        const b = block(x, z, ex, ez, hgt, -1);
        if (kept[0].some((o) => dist(o.x - x, o.z - z) < radius(o) + radius(b))) continue;
        const s = settle(b, null);
        if (!s) continue;
        for (const [k, img] of s.entries()) kept[k].push(img);
      }
    }
  }
  const out = [];
  for (const list of kept) for (const b of list) out.push([b.x - b.hx, b.y0, b.z - b.hz, b.x + b.hx, b.y1, b.z + b.hz]);
  return out;
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

/** A solid as sim::solid::Solid::from_def reads it: kind, then 8 numbers. */
function solidRow(s) {
  let k, n;
  if (s.kind === 'round') [k, n] = [0, [s.x, s.z, s.r, 0, 0, 0, s.y0, s.y1]];
  else if (s.kind === 'turned') {
    const r = (s.angle * Math.PI) / 180;
    // Quarter turns exactly: no 6e-17 for a cosine that is 0.
    const snap = (v) => (Math.abs(v) < 1e-9 ? 0 : Math.abs(Math.abs(v) - 1) < 1e-9 ? Math.sign(v) : v);
    [k, n] = [1, [s.x, s.z, s.hx, s.hz, snap(Math.cos(r)), snap(Math.sin(r)), s.y0, s.y1]];
  } else [k, n] = [2, [...s.rect, s.dir, s.base, s.low, s.high]];
  return `${k}, [${n.map(lit).join(', ')}]`;
}

export function rust(m, src) {
  const L = [];
  L.push(`//! ${m.name}${m.about ? `: ${m.about}` : ''}`);
  L.push(`//!`);
  L.push(`//! Generated from \`${src}\` by \`tools/maps/compile.mjs\`: edit the JSON,`);
  L.push(`//! then run it. CI fails if this file is not what the JSON compiles to.`);
  L.push('');
  // In rustfmt's order, so `cargo fmt` leaves the file as written.
  L.push('use super::Def;');
  L.push('#[allow(unused_imports)]');
  L.push('use crate::arms::*;');
  L.push('#[allow(unused_imports)]');
  L.push('use crate::terrain::TerrainDef;');
  L.push('');
  L.push('#[rustfmt::skip]');
  L.push('pub const DEF: Def = Def {');
  L.push(`    id: ${m.id},`);
  L.push(`    name: ${JSON.stringify(m.name)},`);
  L.push(`    practice: ${m.practice},`);
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
  if (m.terrain) {
    const t = m.terrain;
    L.push('    terrain: Some(TerrainDef {');
    L.push(`        cell: ${lit(t.cell)},`);
    L.push(`        x0: ${lit(t.x0)},`);
    L.push(`        z0: ${lit(t.z0)},`);
    L.push(`        nx: ${t.nx},`);
    L.push(`        nz: ${t.nz},`);
    L.push('        heights: &[');
    for (let j = 0; j < t.nz; j++) L.push(`            ${t.heights.slice(j * t.nx, (j + 1) * t.nx).join(', ')},`);
    L.push('        ],');
    L.push('    }),');
  } else {
    L.push('    terrain: None,');
  }
  L.push('    launchers: &[');
  for (const l of m.launchers) L.push(`        ([${l.area.map(lit).join(', ')}], [${l.vel.map(lit).join(', ')}]),`);
  L.push('    ],');
  L.push('    vehicles: &[');
  for (const v of m.vehicles) L.push(`        ([${v.pos.map(lit).join(', ')}], ${yawU16(v.yaw)}, ${v.respawn}),`);
  L.push('    ],');
  L.push('    solids: &[');
  for (const s of m.solids) L.push(`        (${solidRow(s)}),`);
  L.push('    ],');
  L.push(`    solid_looks: &[${m.solids.map((s) => LOOKS.indexOf(s.look)).join(', ')}],`);
  L.push('    dress: &[');
  for (const b of m.dress) L.push(`        ([${b.box.map(lit).join(', ')}], ${LOOKS.indexOf(b.look)}),`);
  L.push('    ],');
  L.push('    dress_solids: &[');
  for (const s of m.dressSolids) L.push(`        (${solidRow(s)}, ${LOOKS.indexOf(s.look)}),`);
  L.push('    ],');
  L.push(`    nav: ${m.nav},`);
  const lt = m.light;
  if (lt.ev100 === 0 && lt.sun === 1 && lt.fill === 1 && !lt.lamps.length && lt.floor === 255 && lt.sea === null) L.push('    light: crate::map::LightDef::DAY,');
  else {
    L.push('    light: crate::map::LightDef {');
    L.push(`        ev100: ${lit(lt.ev100)},`);
    L.push(`        sun: ${lit(lt.sun)},`);
    L.push(`        fill: ${lit(lt.fill)},`);
    L.push('        lamps: &[');
    for (const l of lt.lamps) L.push(`            ([${l.p.map(lit).join(', ')}], [${l.c.map(lit).join(', ')}], ${lit(l.lm)}),`);
    L.push('        ],');
    L.push(`        floor: ${lt.floor},`);
    L.push(`        sea: ${lt.sea === null ? 'f32::NEG_INFINITY' : lit(lt.sea)},`);
    L.push('    },');
  }
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
  // As rustfmt writes it: on one line while the array is within its
  // array_width (60% of max_width 120), else one a line.
  const defs = list.map((m) => `&${m.module}::DEF`);
  const one = `[${defs.join(', ')}]`;
  if (one.length <= 72) L.push(`pub const ALL: [&Def; ${list.length}] = ${one};`);
  else L.push(`pub const ALL: [&Def; ${list.length}] = [`, ...defs.map((d) => `    ${d},`), '];');
  return L.join('\n') + '\n';
}

/** Every map compiled, in id order: [{ ...compile(), module, src }]. */
export function compileAll() {
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
  return list;
}

function main() {
  const check = process.argv.includes('--check');
  const list = compileAll();
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
  for (const m of list) {
    const t = m.terrain ? `, terrain ${m.terrain.nx} × ${m.terrain.nz}` : '';
    const r = m.rocks ? ` (${m.rocks} rocks)` : '';
    console.log(`${m.id} ${m.name}: ${m.boxes.length} boxes${r}, ${m.solids.length} solids, ${m.dress.length + m.dressSolids.length} dressing, ${m.spawns.length} spawns, ${m.pads.length} pads, ${m.roam.length} roam points, ${m.launchers.length} launchers, ${m.vehicles.length} vehicles${t}`);
  }
  if (stale) process.exit(1);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();

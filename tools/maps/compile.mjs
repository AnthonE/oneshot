#!/usr/bin/env node
// The map compiler: maps/<name>/map.json in, crates/sim/src/maps/<name>.rs
// out, the tables the sim builds a `Map` from, and crates/sim/src/maps/mod.rs
// listing them by id. The JSON is what a person edits; the Rust is checked in
// so the sim needs no parser, and `--check` (run by CI) fails when it isn't
// what the JSON compiles to. After Fireplace's tools/map-compile.mjs (CC0):
// its boxes, prefabs and quarter turns, on a flat floor or a terrain.
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
//   terrain   { cell: 0.5|1|2|4, base: m, margin: m, shapes: [...] }: ground
//             that isn't flat, over the bounds and `margin` past them. A
//             shape adds to the base, with a cosine falloff:
//               { hill: [x, z], r, h }         a round rise (h < 0: a dip)
//               { ridge: [[x, z], [x, z]], w, h }  along a line, w to either side
//               { plateau: [x0, z0, x1, z1], h, edge }  flat inside, falling over `edge`
//             each with mirror: bool. The heights are written as integers
//             (1/128 m), so the sim never reads a float made here.
//   launchers [{ at: [x, z, y?], to: [x, z, y?], time: s, size: [w, d],
//             mirror: bool }]: a pad that throws a fighter from `at` to land
//             on `to` after `time` seconds
//   vehicles  [{ at: [x, z, y?], yaw: degrees, respawn: seconds, mirror: bool }]:
//             where a vehicle starts and comes back
//   nav       true: bots find their way by waypoints (crates/sim/src/nav.rs)
//             rather than walking straight where they're going
//   callouts  [{ name, at: [x, z, y?] }]: places tools/flow names; not compiled
// A box is [x0, y0, z0, x1, y1, z1, look]; a look (drawing only) is one of
// LOOKS. A structure's list may also hold solids that aren't boxes
// (crates/sim/src/solid.rs), placed, turned and mirrored as boxes are:
//   { round: [x, z], r, y: [y0, y1], look }          an upright cylinder
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
export const LOOKS = ['wall', 'block', 'trim', 'red', 'blue', 'dark', 'rock', 'hull', 'glow', 'red_glow', 'blue_glow', 'plate'];
/** sim::arms, by the names map.json uses. */
export const WEAPONS = { ar: 'AR', br: 'BR', magnum: 'MAGNUM', sniper: 'SNIPER', shotgun: 'SHOTGUN', rocket: 'ROCKET', smg: 'SMG' };
const TEAMS = { red: 0, blue: 1, any: 255 };
// Caps the sim holds (crates/sim/src/consts.rs MAX_PADS) and sanity.
const MAX_PADS = 8;
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
/** rustfmt.toml's max_width. */
const MAX_WIDTH = 120;

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
    const [y0, y1] = ys('a round');
    if (!(it.r >= 0.1)) fail(`${name}: a round needs r of 0.1 m or more`);
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
  const SWAP = { red: 'blue', blue: 'red', red_glow: 'blue_glow', blue_glow: 'red_glow' };
  const swapLook = (l) => SWAP[l] || l;

  const boxes = [];
  const solids = [];
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
      placed.push({ box: place(b, at, turn), look });
    }
    boxes.push(...placed);
    solids.push(...placedSolids);
    if (s.mirror) {
      if (!mirrors.length) fail(`${name}: "mirror" needs the map's "mirror"`);
      for (const [sx, sz, swap] of mirrors) {
        for (const p of placedSolids) solids.push({ ...mirrorSolid(p, sx, sz), look: swap ? swapLook(p.look) : p.look });
      }
      for (const [sx, sz, swap] of mirrors) {
        for (const p of placed) {
          const [x0, y0, z0, x1, y1, z1] = p.box;
          const xs = [x0 * sx, x1 * sx].sort((a, b) => a - b), zs = [z0 * sz, z1 * sz].sort((a, b) => a - b);
          boxes.push({ box: [xs[0], y0, zs[0], xs[1], y1, zs[1]], look: swap ? swapLook(p.look) : p.look });
        }
      }
    }
  }
  if (boxes.length + solids.length > MAX_BOXES) fail(`${boxes.length} boxes and ${solids.length} solids, at most ${MAX_BOXES} together`);

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
  const terrain = spec.terrain ? heightfield(spec.terrain, [bx0, bx1, bz0, bz1], mirrors, fail) : null;
  const launchers = [];
  for (const [k, l] of (spec.launchers || []).entries()) {
    const what = `launcher ${k}`;
    const a = point(l.at, what), b = point(l.to, `${what}'s landing`);
    const T = l.time;
    if (!(T > 0)) fail(`${what}: "time" in seconds, over 0`);
    const [w, d] = l.size || [2, 2];
    const one = (a, b) => ({
      area: [a[0] - w / 2, a[1], a[2] - d / 2, a[0] + w / 2, a[1] + 0.5, a[2] + d / 2],
      // Lands on b after T: x and z at speed, y against gravity.
      vel: [(b[0] - a[0]) / T, (b[1] - a[1] + 0.5 * GRAVITY * T * T) / T, (b[2] - a[2]) / T],
    });
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
  return { id: spec.id, name: spec.name, about: spec.about || '', nav: !!spec.nav, lo: [bx0, bz0], hi: [bx1, bz1], boxes, solids, spawns, pads, roam, terrain, launchers, vehicles };
}

/** A terrain block rasterised: vertex heights in 1/HEIGHT_Q m. */
function heightfield(t, [bx0, bx1, bz0, bz1], mirrors, fail) {
  const cell = t.cell ?? 2;
  if (![0.5, 1, 2, 4].includes(cell)) fail('terrain: cell is 0.5, 1, 2 or 4');
  const margin = t.margin ?? 4;
  const x0 = Math.floor((bx0 - margin) / cell) * cell, z0 = Math.floor((bz0 - margin) / cell) * cell;
  const nx = Math.ceil((bx1 + margin - x0) / cell) + 1, nz = Math.ceil((bz1 + margin - z0) / cell) + 1;
  if (nx * nz > 65536) fail(`terrain: ${nx} × ${nz} vertices, too many`);
  const fall = (d, r) => (d >= r ? 0 : 0.5 + 0.5 * Math.cos((Math.PI * d) / r));
  const shapes = [];
  for (const [k, s] of (t.shapes || []).entries()) {
    const imgs = [[1, 1]].concat(s.mirror ? mirrors.map(([sx, sz]) => [sx, sz]) : []);
    for (const [sx, sz] of imgs) {
      if (s.hill) {
        const [cx, cz] = s.hill, r = s.r, h = s.h;
        if (!(r > 0) || typeof h !== 'number') fail(`terrain shape ${k}: a hill needs r > 0 and h`);
        shapes.push((x, z) => h * fall(Math.hypot(x - cx * sx, z - cz * sz), r));
      } else if (s.ridge) {
        const [[ax, az], [qx, qz]] = s.ridge, w = s.w, h = s.h;
        if (!(w > 0) || typeof h !== 'number') fail(`terrain shape ${k}: a ridge needs w > 0 and h`);
        const [x1, z1, x2, z2] = [ax * sx, az * sz, qx * sx, qz * sz];
        shapes.push((x, z) => {
          const vx = x2 - x1, vz = z2 - z1, L = vx * vx + vz * vz;
          const u = L ? Math.max(0, Math.min(1, ((x - x1) * vx + (z - z1) * vz) / L)) : 0;
          return h * fall(Math.hypot(x - (x1 + u * vx), z - (z1 + u * vz)), w);
        });
      } else if (s.plateau) {
        const [p0, q0, p1, q1] = s.plateau, h = s.h, e = s.edge ?? 4;
        if (typeof h !== 'number' || !(e > 0)) fail(`terrain shape ${k}: a plateau needs h and edge > 0`);
        const [xa, xb] = [p0 * sx, p1 * sx].sort((a, b) => a - b), [za, zb] = [q0 * sz, q1 * sz].sort((a, b) => a - b);
        shapes.push((x, z) => h * fall(Math.hypot(Math.max(xa - x, 0, x - xb), Math.max(za - z, 0, z - zb)), e));
      } else fail(`terrain shape ${k}: a hill, a ridge or a plateau`);
    }
  }
  const base = t.base ?? 0;
  const heights = [];
  for (let j = 0; j < nz; j++) {
    for (let i = 0; i < nx; i++) {
      const x = x0 + i * cell, z = z0 + j * cell;
      const h = shapes.reduce((a, f) => a + f(x, z), base);
      const q = Math.round(h * HEIGHT_Q);
      if (q < -32768 || q > 32767) fail(`terrain: ${h} m at ${x}, ${z} is out of range`);
      heights.push(q);
    }
  }
  return { cell, x0, z0, nx, nz, heights };
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
  L.push(`    nav: ${m.nav},`);
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
  // One line while rustfmt keeps it so (an array up to 60% of max_width,
  // its array_width), else one a line.
  const items = `[${list.map((m) => `&${m.module}::DEF`).join(', ')}]`;
  const one = `pub const ALL: [&Def; ${list.length}] = ${items};`;
  if (items.length <= MAX_WIDTH * 0.6 && one.length <= MAX_WIDTH) L.push(one);
  else {
    L.push(`pub const ALL: [&Def; ${list.length}] = [`);
    for (const m of list) L.push(`    &${m.module}::DEF,`);
    L.push('];');
  }
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
  for (const m of list) {
    const t = m.terrain ? `, terrain ${m.terrain.nx} × ${m.terrain.nz}` : '';
    console.log(`${m.id} ${m.name}: ${m.boxes.length} boxes, ${m.solids.length} solids, ${m.spawns.length} spawns, ${m.pads.length} pads, ${m.roam.length} roam points, ${m.launchers.length} launchers, ${m.vehicles.length} vehicles${t}`);
  }
  if (stale) process.exit(1);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();

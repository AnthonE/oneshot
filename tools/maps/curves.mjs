// Profile solids for the map compiler (sim::profile): a cross-section as
// knots (s, bottom, top), spun round an upright axis (`lathe`) or carried
// along a level one (`sweep`), and the items that write those knots so a
// map.json stays readable: a dome, a pod (a bulb on a foot), a column, a
// deck (a round slab or ring with rolled edges), a ring (a torus), a
// curved wall that bulges, a straight one, a barrel vault, an arch, a
// pipe, a walkway and a spiral ramp.
//
// Every curve is sampled so the straight line between two knots strays at
// most ERR from it; heights over the solid's own `y`. The sim checks the
// same rules again when it builds a map (sim::profile::check).

/** How far a profile's straight runs may stray from its curve, m. */
export const ERR = 0.02;
/** sim::consts::MAX_KNOTS. */
export const MAX_KNOTS = 32;
/** sim::consts::MAX_CLIMB: steeper than this is never walked up. */
const MAX_CLIMB = 1.0355304;

const f32 = (v) => Math.fround(v);
const rad = (d) => (d * Math.PI) / 180;
const ell = (u) => Math.sqrt(Math.max(0, 1 - u * u));

/**
 * Knots from pieces over s, in order, each starting where the last ended:
 * [{ s: [s0, s1], bot: s => y, top: s => y }]. Where two pieces meet with
 * different heights the knots step (the same s twice).
 */
export function knotsOf(pieces, fail, what) {
  for (let err = ERR; ; err *= 1.4) {
    const k = [];
    for (const p of pieces) {
      for (const s of sample(p, err)) {
        const knot = [s, p.bot(s), p.top(s)];
        const last = k[k.length - 1];
        if (last && Math.abs(last[0] - s) < 1e-9) {
          if (Math.abs(last[1] - knot[1]) < 1e-6 && Math.abs(last[2] - knot[2]) < 1e-6) continue;
          knot[0] = last[0];
        }
        k.push(knot);
      }
    }
    if (k.length <= MAX_KNOTS) return check(k, fail, what);
    if (err > 0.25) fail(`${what}: more than ${MAX_KNOTS} knots even at ${err.toFixed(2)} m`);
  }
}

/** Where to put knots along one piece: halved until each run is within `err` of both curves. */
function sample(p, err) {
  const [a0, b0] = p.s;
  const out = [a0];
  const off = (f, a, b) => {
    const m = (a + b) / 2;
    const slope = (f(b) - f(a)) / (b - a);
    return Math.abs(f(m) - (f(a) + f(b)) / 2) / Math.sqrt(1 + slope * slope);
  };
  const rec = (a, b, depth) => {
    if (depth < (p.min ?? 0) || (depth < 9 && (off(p.bot, a, b) > err || off(p.top, a, b) > err))) {
      const m = (a + b) / 2;
      rec(a, m, depth + 1);
      rec(m, b, depth + 1);
    } else out.push(b);
  };
  if (b0 > a0) rec(a0, b0, 0);
  return out;
}

/** The sim's rules on a profile, at f32: a fault names `what`. */
export function check(k, fail, what) {
  const r = k.map((p) => p.map(f32));
  const n = r.length;
  if (n < 2 || n > MAX_KNOTS) fail(`${what}: ${n} knots (2 to ${MAX_KNOTS})`);
  if (!(r[0][0] < r[1][0] && r[n - 2][0] < r[n - 1][0])) fail(`${what}: a profile's ends can't be steps`);
  for (let i = 0; i < n; i++) {
    if (!(r[i][1] <= r[i][2])) fail(`${what}: knot ${i}'s bottom is over its top`);
    if (i > 0) {
      if (r[i][0] < r[i - 1][0]) fail(`${what}: knot ${i}'s s falls`);
      if (r[i][0] === r[i - 1][0] && !(r[i - 1][1] <= r[i][2] && r[i][1] <= r[i - 1][2])) fail(`${what}: the step at knot ${i} doesn't overlap`);
    }
  }
  return r;
}

/** A sector's span split into pieces of at most half a turn: [[from, to], ...], each from < to. */
function sectors(from, to) {
  let [a, b] = from < to ? [from, to] : [to, from];
  if (b - a >= 360) return null;
  const n = Math.ceil((b - a) / 180 - 1e-9);
  const out = [];
  for (let i = 0; i < n; i++) out.push([a + ((b - a) * i) / n, a + ((b - a) * (i + 1)) / n]);
  return out;
}

/** One lathe, or a sector of one split in halves of a turn or less. */
function lathe(x, z, y, knots, look, from, to, fail, what) {
  if (knots[0][0] < 0) fail(`${what}: a spun profile starts at its axis or past it`);
  const r = knots[knots.length - 1][0];
  if (from === undefined && to === undefined) return [{ kind: 'lathe', x, z, y, knots, look, r }];
  if (typeof from !== 'number' || typeof to !== 'number' || from === to) fail(`${what}: a sector runs "from" one angle "to" another (degrees)`);
  const parts = sectors(from, to);
  if (!parts) return [{ kind: 'lathe', x, z, y, knots, look, r }];
  return parts.map(([a, b]) => ({ kind: 'lathe', x, z, y, from: a, to: b, knots, look, r }));
}

/** A sweep along the axis from (x0, z0) to (x1, z1). */
function sweepAlong([[x0, z0], [x1, z1]], y, knots, look, fail, what) {
  const dx = x1 - x0, dz = z1 - z0;
  const len = Math.hypot(dx, dz);
  if (!(len > 0.05)) fail(`${what}: a sweep runs from one point to another`);
  const s0 = knots[0][0], sn = knots[knots.length - 1][0];
  return { kind: 'sweep', x: (x0 + x1) / 2, z: (z0 + z1) / 2, y, angle: (Math.atan2(dz, dx) * 180) / Math.PI, l: len / 2, knots, look, hx: len / 2, hz: Math.max(Math.abs(s0), Math.abs(sn)) };
}

/** A sweep's axis: [[x0, z0], [x1, z1]], or a middle, an angle and a length. */
function axisOf(it, key, fail, what) {
  const a = it[key];
  if (Array.isArray(a) && Array.isArray(a[0])) return a;
  if (Array.isArray(a) && a.length === 2 && typeof it.length === 'number') {
    const h = it.length / 2, r = rad(it.angle || 0);
    return [[a[0] - h * Math.cos(r), a[1] - h * Math.sin(r)], [a[0] + h * Math.cos(r), a[1] + h * Math.sin(r)]];
  }
  fail(`${what}: "${key}" is [[x0, z0], [x1, z1]], or [x, z] with "angle" and "length"`);
}

const num = (v, what, fail, test = (x) => x > 0) => {
  if (typeof v !== 'number' || !test(v)) fail(what);
  return v;
};

/** A bulge on a wall's face: how deep it stands out, between two heights (a half ellipse). */
function bulge(b, fail, what) {
  if (!b) return null;
  const depth = num(b.depth, `${what}: a bulge's "depth", m`, fail);
  const [a, c] = b.y || fail(`${what}: a bulge's "y": [from, to]`);
  if (!(c > a)) fail(`${what}: a bulge's "y" runs up`);
  return { depth, a, c, f: (y) => (y <= a || y >= c ? 0 : depth * ell((2 * y - a - c) / (c - a))) };
}

/**
 * A wall's cross-section, s across it: its core from s0 to s1, from y0 to
 * y1, and on either face a bulge. Over a depth d off a face, the bulge is
 * the heights where it stands out d or more.
 */
function wallPieces(s0, s1, y0, y1, lo, hi) {
  const pieces = [];
  if (lo) {
    const h = (d) => {
      const u = Math.min(1, d / lo.depth);
      const w = ((lo.c - lo.a) / 2) * ell(u);
      const m = (lo.a + lo.c) / 2;
      return [m - w, m + w];
    };
    pieces.push({ s: [s0 - lo.depth, s0], bot: (s) => h(s0 - s)[0], top: (s) => h(s0 - s)[1], min: 2 });
  }
  pieces.push({ s: [s0, s1], bot: () => y0, top: () => y1 });
  if (hi) {
    const h = (d) => {
      const u = Math.min(1, d / hi.depth);
      const w = ((hi.c - hi.a) / 2) * ell(u);
      const m = (hi.a + hi.c) / 2;
      return [m - w, m + w];
    };
    pieces.push({ s: [s1, s1 + hi.depth], bot: (s) => h(s - s1)[0], top: (s) => h(s - s1)[1], min: 2 });
  }
  return pieces;
}

/**
 * A slab's cross-section from s0 to s1, its top at `top`, `thick` deep,
 * its edges rolled to radius `rim` (at s0 too unless it is the axis), its
 * underside sagging `belly` toward s0 (a saucer's).
 */
function slabPieces(s0, s1, top, thick, rim, belly, axis) {
  const bot0 = top - thick;
  const r = Math.min(rim, thick / 2);
  const inEdge = !axis && r > 0;
  const a = inEdge ? s0 + r : s0, b = r > 0 ? s1 - r : s1;
  const sag = (s) => (belly > 0 ? belly * Math.pow(1 - Math.min(1, Math.max(0, (s - s0) / (b - s0))) ** 2, 2) : 0);
  const pieces = [];
  if (inEdge) {
    pieces.push({ s: [s0, a], bot: (s) => bot0 + r - r * ell((a - s) / r), top: (s) => top - r + r * ell((a - s) / r), min: 2 });
  }
  pieces.push({ s: [a, b], bot: (s) => bot0 - sag(s), top: () => top, min: belly > 0 ? 2 : 0 });
  if (r > 0) pieces.push({ s: [b, s1], bot: (s) => bot0 + r - r * ell((s - b) / r), top: (s) => top - r + r * ell((s - b) / r), min: 2 });
  return pieces;
}

/** A ceiling's piece under `curve` over the span `s`: back to `roof`, or a `band` deep under the curve. */
function band(curve, s, it, spring, fail, what) {
  if (it.band !== undefined) {
    const b = num(it.band, `${what}: band, m`, fail);
    return { s, bot: (v) => curve(v) - b, top: curve, min: 3 };
  }
  if (!(spring < it.crown && it.crown < it.roof)) fail(`${what}: spring < crown < roof`);
  return { s, bot: curve, top: () => it.roof - spring, min: 3 };
}

/**
 * The curved items of map.json, as the compiler's solids (`lathe` or
 * `sweep`, or several), or null if `it` is none of them.
 */
export function readCurve(it, name, look, fail) {
  const what = `${name}: ${Object.keys(it)[0]}`;
  const ys = () => {
    const y = it.y;
    if (!Array.isArray(y) || y.length !== 2 || !(y[0] < y[1])) fail(`${what}: needs y: [y0, y1], y0 < y1`);
    return y;
  };
  const at = (key) => {
    const p = it[key];
    if (!Array.isArray(p) || p.length !== 2 || p.some((v) => typeof v !== 'number')) fail(`${what}: "${key}" is [x, z]`);
    return p;
  };
  if (it.lathe) {
    const [x, z] = at('lathe');
    const knots = check(it.profile || fail(`${what}: needs a profile: [[s, bottom, top], ...]`), fail, what);
    return lathe(x, z, it.y ?? 0, knots, look, it.from, it.to, fail, what);
  }
  if (it.sweep) {
    const knots = check(it.profile || fail(`${what}: needs a profile: [[s, bottom, top], ...]`), fail, what);
    return [sweepAlong(axisOf(it, 'sweep', fail, what), it.y ?? 0, knots, look, fail, what)];
  }
  if (it.dome) {
    // A cap: an ellipse's quarter from its top at the middle to the floor at r.
    const [x, z] = at('dome');
    const r = num(it.r, `${what}: r`, fail), h = num(it.h, `${what}: h`, fail);
    const k = knotsOf([{ s: [0, r], bot: () => 0, top: (s) => h * ell(s / r), min: 2 }], fail, what);
    return lathe(x, z, it.y ?? 0, k, look, it.from, it.to, fail, what);
  }
  if (it.pod) {
    // A bulb: widest at `belly` up, a domed top, its underside curving in
    // to a foot that stands it on the floor.
    const [x, z] = at('pod');
    const r = num(it.r, `${what}: r`, fail), h = num(it.h, `${what}: h`, fail);
    const c = it.belly ?? h * 0.45, foot = it.foot ?? r * 0.55;
    if (!(c > 0 && c < h && foot > 0 && foot < r)) fail(`${what}: belly within h, foot within r`);
    const top = (s) => c + (h - c) * ell(s / r);
    const k = knotsOf(
      [
        { s: [0, foot], bot: () => 0, top },
        { s: [foot, r], bot: (s) => c - c * ell(s / r), top, min: 2 },
      ],
      fail,
      what,
    );
    return lathe(x, z, it.y ?? 0, k, look, it.from, it.to, fail, what);
  }
  if (it.column) {
    // An upright round core, its foot (and its head, a second solid)
    // flaring out in a quarter round to `flare`'s radius over its height.
    const [x, z] = at('column');
    const [y0, y1] = ys();
    const r = num(it.r, `${what}: r`, fail);
    const out = [];
    const H = y1 - y0;
    const pieces = [{ s: [0, r], bot: () => 0, top: () => H }];
    if (it.flare) {
      const [rf, hf] = it.flare;
      if (!(rf > r && hf > 0 && hf < H)) fail(`${what}: flare: [radius past r, height under the column's]`);
      const w = rf - r;
      pieces.push({ s: [r, rf], bot: () => 0, top: (s) => hf - hf * ell((rf - s) / w), min: 2 });
    }
    out.push(...lathe(x, z, y0, knotsOf(pieces, fail, what), look, it.from, it.to, fail, what));
    if (it.cap) {
      const [rc, hc] = it.cap;
      if (!(rc > r && hc > 0 && hc < H)) fail(`${what}: cap: [radius past r, height under the column's]`);
      const w = rc - r;
      const k = knotsOf([{ s: [r, rc], bot: (s) => H - hc + hc * ell((rc - s) / w), top: () => H, min: 2 }], fail, what);
      out.push(...lathe(x, z, y0, k, it.cap_look || look, it.from, it.to, fail, what));
    }
    return out;
  }
  if (it.deck) {
    // A round slab (or a ring, with `hole`) with its top at y, rolled at
    // its edges; `belly` sags its underside toward the middle.
    const [x, z] = at('deck');
    const r = num(it.r, `${what}: r`, fail), y = num(it.y, `${what}: y, its top`, fail, () => true);
    const thick = num(it.thick ?? 0.4, `${what}: thick`, fail), hole = it.hole ?? 0;
    if (!(hole >= 0 && hole < r)) fail(`${what}: hole within r`);
    const k = knotsOf(slabPieces(hole, r, thick, thick, it.rim ?? 0, it.belly ?? 0, hole === 0), fail, what);
    return lathe(x, z, y - thick, k, look, it.from, it.to, fail, what);
  }
  if (it.fin) {
    // A low wall on the circle r about [x, z] (a sector with from/to),
    // `thick` thick, from y[0] to its ridge at y[1], its top two steep
    // curves meeting in a point: never stood on (sim::profile: a steep top
    // is never floor).
    const [x, z] = at('fin');
    const [y0, y1] = ys();
    const r = num(it.r, `${what}: r`, fail), t = num(it.thick ?? 0.5, `${what}: thick`, fail);
    const w = t / 2;
    // Steep enough at its shoulders: a drop of 1.2·w and more.
    const drop = Math.max(0.5, 1.25 * w);
    if (!(y1 - drop > y0)) fail(`${what}: taller than its ridge's drop (${drop.toFixed(2)} m)`);
    const top = (s) => y1 - y0 - drop * Math.pow(Math.min(1, Math.abs(s - r) / w), 0.7);
    const k = knotsOf(
      [
        { s: [r - w, r], bot: () => 0, top, min: 2 },
        { s: [r, r + w], bot: () => 0, top, min: 2 },
      ],
      fail,
      what,
    );
    return lathe(x, z, y0, k, look, it.from, it.to, fail, what);
  }
  if (it.ring) {
    // A torus about an upright axis: its middle circle r at height y, its
    // tube's radius `tube`.
    const [x, z] = at('ring');
    const r = num(it.r, `${what}: r`, fail), t = num(it.tube, `${what}: tube`, fail);
    if (!(t < r)) fail(`${what}: tube under r`);
    const k = knotsOf([{ s: [r - t, r + t], bot: (s) => -t * ell((s - r) / t), top: (s) => t * ell((s - r) / t), min: 2 }], fail, what);
    return lathe(x, z, it.y ?? 0, k, look, it.from, it.to, fail, what);
  }
  if (it.arc) {
    // A curved wall on the circle of radius r, `thick` thick, from y0 to
    // y1, its inner face (toward the middle) and its outer one each
    // bulging if given { depth, y: [from, to] }.
    const [x, z] = at('arc');
    const [y0, y1] = ys();
    const r = num(it.r, `${what}: r`, fail), t = it.thick ?? 0.4;
    if (!(r > t && t >= 0.2)) fail(`${what}: an arc needs r over its thick (0.2 m or more)`);
    const lo = bulge(it.inner, fail, what), hi = bulge(it.outer, fail, what);
    const shift = (b) => b && { ...b, a: b.a - y0, c: b.c - y0 };
    const k = knotsOf(wallPieces(r - t / 2, r + t / 2, 0, y1 - y0, shift(lo), shift(hi)), fail, what);
    if (typeof it.from !== 'number' || typeof it.to !== 'number' || it.from === it.to) fail(`${what}: an arc runs "from" one angle "to" another (degrees)`);
    return lathe(x, z, y0, k, look, it.from, it.to, fail, what);
  }
  if (it.wall) {
    // A straight wall along [[x0, z0], [x1, z1]], `thick` thick, either
    // face bulging: `plus` on the +s side (+z for a wall along +x),
    // `minus` on the other.
    const axis = axisOf(it, 'wall', fail, what);
    const [y0, y1] = ys();
    const t = num(it.thick ?? 0.4, `${what}: thick`, fail);
    const lo = bulge(it.minus, fail, what), hi = bulge(it.plus, fail, what);
    const shift = (b) => b && { ...b, a: b.a - y0, c: b.c - y0 };
    const k = knotsOf(wallPieces(-t / 2, t / 2, 0, y1 - y0, shift(lo), shift(hi)), fail, what);
    return [sweepAlong(axis, y0, k, look, fail, what)];
  }
  if (it.vault) {
    // A barrel ceiling over [x0, z0, x1, z1], along x or z (`along`; its
    // longer side unless given): it springs from the long edges at
    // `spring`, its crown at `crown`, its back at `roof`. With `band`, only
    // that much under the curve: a rib.
    const [x0, z0, x1, z1] = it.vault;
    if (!(x0 < x1 && z0 < z1)) fail(`${what}: a vault is over [x0, z0, x1, z1]`);
    const along = it.along ? it.along === 'x' : x1 - x0 >= z1 - z0;
    const w = along ? z1 - z0 : x1 - x0;
    const { spring, crown } = it;
    const curve = (s) => (crown - spring) * ell((2 * s) / w);
    const span = it.s || [-w / 2, w / 2];
    if (!(span[0] >= -w / 2 && span[0] < span[1] && span[1] <= w / 2)) fail(`${what}: s within the span`);
    const k = knotsOf([band(curve, span, it, spring, fail, what)], fail, what);
    const axis = along ? [[x0, (z0 + z1) / 2], [x1, (z0 + z1) / 2]] : [[(x0 + x1) / 2, z0], [(x0 + x1) / 2, z1]];
    return [sweepAlong(axis, spring, k, look, fail, what)];
  }
  if (it.cupola) {
    // A dome's ceiling (a half dome with from/to) about [x, z]: it springs
    // at r and `spring`, its crown over the middle at `crown`, its back at
    // `roof`; with `band`, a rib under it (over `s`: [from, to] of r only).
    const [x, z] = at('cupola');
    const r = num(it.r, `${what}: r`, fail);
    const { spring, crown } = it;
    const [a, b] = it.s || [0, r];
    if (!(a >= 0 && a < b && b <= r)) fail(`${what}: s within [0, r]`);
    const curve = (s) => (crown - spring) * ell(s / r);
    const k = knotsOf([band(curve, [a, b], it, spring, fail, what)], fail, what);
    return lathe(x, z, spring, k, look, it.from, it.to, fail, what);
  }
  if (it.arch) {
    // An arched opening through a wall: the passage's axis
    // [[x0, z0], [x1, z1]] (its length the wall's depth), `width` wide,
    // its sides standing to `spring`, the arch's crown at `crown`, the
    // wall over it to `top`, `jamb` of wall either side.
    const axis = axisOf(it, 'arch', fail, what);
    const w = num(it.width, `${what}: width`, fail), j = num(it.jamb ?? 0.6, `${what}: jamb`, fail);
    const { spring, crown, top } = it;
    if (!(spring > 0 && spring < crown && (it.frame !== undefined || crown < top))) fail(`${what}: 0 < spring < crown < top`);
    const h = w / 2;
    if (it.frame !== undefined) {
      // Only a band round the opening, `frame` wide, `off` out from it: up
      // each side from the floor and over (a door's frame, or a line of
      // light on one); its top and jamb are the opening's own.
      const t = num(it.frame, `${what}: frame`, fail), o = it.off ?? 0;
      if (!(o >= 0)) fail(`${what}: off, out from the opening`);
      const ih = h + o, oh = h + o + t;
      const inner = (s) => spring + (crown + o - spring) * ell(s / ih);
      const outer = (s) => spring + (crown + o + t - spring) * ell(s / oh);
      const k = knotsOf(
        [
          { s: [-oh, -ih], bot: () => 0, top: outer, min: 2 },
          { s: [-ih, ih], bot: inner, top: outer, min: 2 },
          { s: [ih, oh], bot: () => 0, top: outer, min: 2 },
        ],
        fail,
        what,
      );
      return [sweepAlong(axis, 0, k, look, fail, what)];
    }
    const k = knotsOf(
      [
        { s: [-h - j, -h], bot: () => 0, top: () => top },
        { s: [-h, h], bot: (s) => spring + (crown - spring) * ell(s / h), top: () => top, min: 2 },
        { s: [h, h + j], bot: () => 0, top: () => top },
      ],
      fail,
      what,
    );
    return [sweepAlong(axis, 0, k, look, fail, what)];
  }
  if (it.pipe) {
    // A round bar along [[x0, z0], [x1, z1]], its middle at height y.
    const axis = axisOf(it, 'pipe', fail, what);
    const r = num(it.r, `${what}: r`, fail);
    const k = knotsOf([{ s: [-r, r], bot: (s) => -r * ell(s / r), top: (s) => r * ell(s / r), min: 2 }], fail, what);
    return [sweepAlong(axis, it.y ?? r, k, look, fail, what)];
  }
  if (it.walk) {
    // A level walkway along [[x0, z0], [x1, z1]], `width` wide, its top at
    // y, `thick` deep, its edges rolled to `rim`.
    const axis = axisOf(it, 'walk', fail, what);
    const w = num(it.width, `${what}: width`, fail), y = num(it.y, `${what}: y, its top`, fail, () => true);
    const thick = num(it.thick ?? 0.4, `${what}: thick`, fail);
    const k = knotsOf(slabPieces(-w / 2, w / 2, thick, thick, it.rim ?? 0, it.belly ?? 0, false), fail, what);
    return [sweepAlong(axis, y - thick, k, look, fail, what)];
  }
  if (it.spiral) {
    // A ramp curving round [x, z] on the circle r, `width` wide, from the
    // angle `from` at y[0] to `to` at y[1]: short straight ramps, each
    // where the last ended, few enough that neither their edges nor their
    // heights stray more than a few centimetres from the true curve. Solid
    // down to `base`.
    const [x, z] = at('spiral');
    const r = num(it.r, `${what}: r`, fail), w = num(it.width, `${what}: width`, fail);
    const [ya, yb] = it.y || fail(`${what}: y: [at "from", at "to"]`);
    const base = it.base ?? 0;
    const span = rad(it.to - it.from);
    if (!(Math.abs(span) > 0 && Math.abs(span) <= 2 * Math.PI)) fail(`${what}: "from" one angle "to" another, within a turn`);
    if (!(r > w / 2)) fail(`${what}: r over half its width`);
    const run = Math.abs(span) * r;
    const m = (yb - ya) / run;
    if (Math.abs(m) > 1) fail(`${what}: steeper than 45°`);
    const ro = r + w / 2;
    const byEdge = 2 * Math.acos(Math.max(-1, 1 - ERR / ro));
    // The joints' mismatch across the width: 2·m·(w/2)·tan(θ/2), at most
    // `joint` (fewer pieces cost fog less; a fighter steps over far more).
    const joint = it.joint ?? 0.03;
    const byRise = Math.abs(m) > 0 ? 2 * Math.atan(joint / (Math.abs(m) * w)) : byEdge;
    const n = Math.max(1, Math.ceil(Math.abs(span) / Math.min(byEdge, byRise)));
    const th = span / n;
    const out = [];
    const dir = Math.sign(span);
    for (let i = 0; i < n; i++) {
      const phi = rad(it.from) + th * (i + 0.5);
      const ymid = ya + (yb - ya) * ((i + 0.5) / n);
      // Long enough that neighbours' outer corners meet; the slope's the
      // centre line's, so the joints meet there exactly.
      const hx = ro * Math.tan(Math.abs(th) / 2);
      const k = dir * m * r * Math.abs(th) / (2 * r * Math.tan(Math.abs(th) / 2));
      // The first and last straights' far corners would dip under the
      // base where it starts there: held on it.
      const lo = Math.max(base, ymid - k * hx), hi = Math.max(base, ymid + k * hx);
      const knots = check([[-hx, base, lo], [hx, base, hi]], fail, what);
      const c = Math.cos(phi), s = Math.sin(phi);
      const cx = x + r * c, cz = z + r * s;
      // The sweep's axis is radial (its length the width), s along the
      // tangent: (−sin φ, cos φ), the way the angle grows.
      out.push({ kind: 'sweep', x: cx, z: cz, y: 0, angle: (phi * 180) / Math.PI, l: w / 2, knots, look, hx: w / 2, hz: hx });
    }
    return out;
  }
  return null;
}

/** A profile solid as sim::solid::Solid::profile reads it: kind, 8 numbers, knots. */
export function profileRow(s, lit) {
  const snap = (v) => (Math.abs(v) < 1e-9 ? 0 : Math.abs(Math.abs(v) - 1) < 1e-9 ? Math.sign(v) : v);
  let k, n;
  if (s.kind === 'lathe' && s.from === undefined) [k, n] = [3, [s.x, s.z, s.y, 0, 0, 0, 0, 0]];
  else if (s.kind === 'lathe') {
    const a = rad(s.from), b = rad(s.to);
    [k, n] = [4, [s.x, s.z, s.y, snap(Math.cos(a)), snap(Math.sin(a)), snap(Math.cos(b)), snap(Math.sin(b)), 0]];
  } else {
    const a = rad(s.angle);
    [k, n] = [5, [s.x, s.z, s.y, snap(Math.cos(a)), snap(Math.sin(a)), s.l, 0, 0]];
  }
  return `${k}, [${n.map(lit).join(', ')}], &[${s.knots.map((p) => `[${p.map(lit).join(', ')}]`).join(', ')}]`;
}

/** A profile solid turned a quarter `q` times (+x toward +z) and moved to `at`. */
export function placeCurve(sol, at, q, rot) {
  const o = { ...sol };
  const c = rot([sol.x, sol.z]);
  o.x = c[0] + at[0];
  o.z = c[1] + at[2];
  o.y = sol.y + at[1];
  if (sol.kind === 'lathe' && sol.from !== undefined) {
    o.from = sol.from + 90 * q;
    o.to = sol.to + 90 * q;
  }
  if (sol.kind === 'sweep') o.angle = sol.angle + 90 * q;
  return o;
}

/** A profile solid's mirror image across x (sx = −1) and/or z (sz = −1). */
export function mirrorCurve(sol, sx, sz) {
  const o = { ...sol, x: sol.x * sx, z: sol.z * sz };
  const reflect = (deg) => {
    const r = rad(deg);
    return (Math.atan2(Math.sin(r) * sz, Math.cos(r) * sx) * 180) / Math.PI;
  };
  if (sol.kind === 'lathe' && sol.from !== undefined) {
    // A reflection turns the sector over: its edges swap. Two (a half turn) don't.
    let [a, b] = sx * sz < 0 ? [reflect(sol.to), reflect(sol.from)] : [reflect(sol.from), reflect(sol.to)];
    while (b <= a) b += 360;
    o.from = a;
    o.to = b;
  }
  if (sol.kind === 'sweep') {
    o.angle = reflect(sol.angle);
    // One reflection turns the frame over: s runs the other way.
    if (sx * sz < 0) o.knots = sol.knots.map(([s, b, t]) => [-s, b, t]).reverse();
  }
  return o;
}

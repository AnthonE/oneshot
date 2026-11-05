//! Profile solids: a cross-section (its knots) spun round an upright axis
//! (a dome, a bulb, a waisted column, a saucer, a torus, a round wall that
//! leans or bulges) or carried along a level one (a barrel vault, an
//! arched door, a pipe, a walkway with rolled edges, a ramp). What makes
//! them round in height and not only in plan.
//!
//! A profile is knots (s, bottom, top), s not decreasing, the bottom and
//! top linear between them: every upright line meets the solid in one
//! interval, [bot(s), top(s)]. A repeated s is an upright step, the two
//! intervals overlapping. s is the distance from the axis (spun) or across
//! the axis (carried); heights are the solid's own, over its `y`.
//!
//! A body (a circle of radius r, its feet to its head) covers a span of s:
//! [s − r, s + r] for a spun one, exactly; narrower past a carried one's
//! end or a sector's edge, where it meets the end cap. Over that span the
//! profile's lowest bottom and highest top are exact (the span's ends and
//! the knots inside it), and which pieces block decide the rest:
//!
//! - an upright face (an end, a step, an end cap the body is past) blocks
//!   over `lo`, as a round pillar does: a step on the ground, a wall in
//!   the air;
//! - a top steeper than [`MAX_CLIMB`] blocks wherever it stands over the
//!   feet: never climbed, a step at a time or otherwise;
//! - a gentle top is the ramp's rule: walked onto within a step, read
//!   under the body's middle; it is the only top stood on (with a face's,
//!   a step up).
//!
//! Every query is `+ − × ÷ sqrt min max` and a fixed walk over the knots:
//! the same bits native and wasm, and nothing allocated.

use crate::consts::{MAX_CLIMB, MAX_KNOTS};
use crate::fmath::fabs;
use crate::geom::V3;

/// A profile's knots: (s, bottom, top) over the solid's `y`.
pub type Knots = &'static [[f32; 3]];

/// How far over the feet a steep top must stand to block, m.
const STEEP_EPS: f32 = 1e-3;
/// Slack in the ray's tests that a root lies on its piece, m.
const ON_EPS: f32 = 1e-4;

/// The piece of a profile solid a ray met.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    /// The top of the segment from knot `i` to `i + 1`.
    Top(u8),
    /// Its bottom.
    Bot(u8),
    /// The upright face at knot `at`, facing +s (`out`) or −s.
    Face { at: u8, out: bool },
    /// An end cap: a sector's edge or a carried solid's end, 0 the first.
    End(u8),
    /// The ray started inside.
    Inside,
}

/// The band a body blocks in (`movement`'s, over the solid's `y`): its
/// feet, what blocks it from `lo` to `hi`, and how high a slope may stand
/// under its middle before it is a wall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub feet: f32,
    pub lo: f32,
    pub hi: f32,
    pub climb: f32,
}

impl Band {
    pub fn down_by(self, y: f32) -> Band {
        Band {
            feet: self.feet - y,
            lo: self.lo - y,
            hi: self.hi - y,
            climb: self.climb - y,
        }
    }
}

/// The span of s a body covers, within the profile; its middle's s, held
/// to the profile (where the gentle rule reads the top); whether it met
/// an end cap (then all of it is an upright face).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cover {
    pub sa: f32,
    pub sb: f32,
    pub sc: f32,
    pub end: bool,
}

/// Checks a profile when a map is built (the compiler checks it first).
pub fn check(k: Knots) {
    let n = k.len();
    assert!((2..=MAX_KNOTS).contains(&n), "a profile has 2 to MAX_KNOTS knots");
    assert!(
        k[0][0] < k[1][0] && k[n - 2][0] < k[n - 1][0],
        "a profile's ends aren't steps"
    );
    for (i, p) in k.iter().enumerate() {
        assert!(p[1] <= p[2], "a profile's bottom is under its top");
        if i > 0 {
            let q = k[i - 1];
            assert!(q[0] <= p[0], "a profile's s never falls");
            if q[0] == p[0] {
                assert!(q[1] <= p[2] && p[1] <= q[2], "a profile's step overlaps");
            }
        }
    }
}

fn lerp(a: [f32; 3], b: [f32; 3], s: f32) -> (f32, f32) {
    let u = (s - a[0]) / (b[0] - a[0]);
    (a[1] + (b[1] - a[1]) * u, a[2] + (b[2] - a[2]) * u)
}

/// The bottom and top at s (the union of a step's two), or None off the
/// profile.
pub fn at(k: Knots, s: f32) -> Option<(f32, f32)> {
    let n = k.len();
    if s < k[0][0] || s > k[n - 1][0] {
        return None;
    }
    let (mut b, mut t) = (f32::INFINITY, f32::NEG_INFINITY);
    for i in 0..n - 1 {
        let (p, q) = (k[i], k[i + 1]);
        if s < p[0] || s > q[0] {
            continue;
        }
        let (pb, pt) = if p[0] == q[0] {
            (p[1].min(q[1]), p[2].max(q[2]))
        } else if s == p[0] {
            (p[1], p[2])
        } else if s == q[0] {
            (q[1], q[2])
        } else {
            lerp(p, q, s)
        };
        b = b.min(pb);
        t = t.max(pt);
    }
    Some((b, t))
}

/// The lowest bottom and highest top over [sa, sb] (within the profile).
pub fn extremes(k: Knots, sa: f32, sb: f32) -> (f32, f32) {
    let (mut b, mut t) = (f32::INFINITY, f32::NEG_INFINITY);
    for s in [sa, sb] {
        if let Some((pb, pt)) = at(k, s) {
            b = b.min(pb);
            t = t.max(pt);
        }
    }
    for p in k {
        if p[0] > sa && p[0] < sb {
            b = b.min(p[1]);
            t = t.max(p[2]);
        }
    }
    (b, t)
}

/// The lowest bottom and highest top of all of it.
pub fn span_y(k: Knots) -> (f32, f32) {
    let mut b = f32::INFINITY;
    let mut t = f32::NEG_INFINITY;
    for p in k {
        b = b.min(p[1]);
        t = t.max(p[2]);
    }
    (b, t)
}

/// Whether the segment from `p` to `q` (s rising) is too steep to climb.
pub fn steep(p: [f32; 3], q: [f32; 3]) -> bool {
    fabs(q[2] - p[2]) > MAX_CLIMB * (q[0] - p[0])
}

/// The upright face at knot `j`, as (bottom, top), if there is one: an end
/// with height (the first knot only where `inner` says it isn't the
/// axis), or a step to the next knot.
fn face(k: Knots, j: usize, inner: bool) -> Option<(f32, f32)> {
    let n = k.len();
    let p = k[j];
    if j + 1 < n && k[j + 1][0] == p[0] {
        let q = k[j + 1];
        return Some((p[1].min(q[1]), p[2].max(q[2])));
    }
    if j > 0 && k[j - 1][0] == p[0] {
        // The step's second knot: its face is the first's.
        return None;
    }
    let is_end = j + 1 == n || (j == 0 && inner);
    (is_end && p[2] > p[1]).then_some((p[1], p[2]))
}

/// The gentle top and its bottom under s, if a gentle segment holds s.
fn gentle(k: Knots, s: f32) -> Option<(f32, f32)> {
    for i in 0..k.len() - 1 {
        let (p, q) = (k[i], k[i + 1]);
        if p[0] < q[0] && s >= p[0] && s <= q[0] {
            if steep(p, q) {
                return None;
            }
            let (b, t) = lerp(p, q, s);
            return Some((b, t));
        }
    }
    None
}

/// Whether a body covering `c` is blocked by the profile, in the band `b`
/// (both over the solid's `y`). `inner`: the first knot is an end, not the
/// axis.
pub fn blocks(k: Knots, c: Cover, b: Band, inner: bool) -> bool {
    let (mb, mt) = extremes(k, c.sa, c.sb);
    if mb >= b.hi {
        return false;
    }
    if c.end {
        return mt > b.lo;
    }
    let n = k.len();
    for j in 0..n {
        let s = k[j][0];
        if s < c.sa || s > c.sb {
            continue;
        }
        if let Some((fb, ft)) = face(k, j, inner) {
            if ft > b.lo && fb < b.hi {
                return true;
            }
        }
    }
    for i in 0..n - 1 {
        let (p, q) = (k[i], k[i + 1]);
        if p[0] == q[0] || !steep(p, q) {
            continue;
        }
        let (s0, s1) = (p[0].max(c.sa), q[0].min(c.sb));
        if s0 > s1 {
            continue;
        }
        let (b0, t0) = lerp(p, q, s0);
        let (b1, t1) = lerp(p, q, s1);
        if t0.max(t1) > b.feet + STEEP_EPS && b0.min(b1) < b.hi {
            return true;
        }
    }
    if let Some((gb, gt)) = gentle(k, c.sc) {
        if gt > b.climb && gb < b.hi {
            return true;
        }
    }
    false
}

/// The highest top a body covering `c` stands on: a gentle top under its
/// middle up to `ramp_reach`, a face's up to `reach`. A steep top is never
/// stood on: a body landing on a knife edge or a dome's flank is over it,
/// then blocked by it and pushed off (it slides, as off steep ground).
pub fn floor(k: Knots, c: Cover, reach: f32, ramp_reach: f32, inner: bool) -> Option<f32> {
    let mut f: Option<f32> = None;
    let mut put = |v: f32| f = Some(f.map_or(v, |g: f32| g.max(v)));
    if c.end {
        let (_, mt) = extremes(k, c.sa, c.sb);
        if mt <= reach {
            put(mt);
        }
        return f;
    }
    let n = k.len();
    for j in 0..n {
        let s = k[j][0];
        if s < c.sa || s > c.sb {
            continue;
        }
        if let Some((_, ft)) = face(k, j, inner) {
            if ft <= reach {
                put(ft);
            }
        }
    }
    if let Some((_, gt)) = gentle(k, c.sc) {
        if gt <= ramp_reach {
            put(gt);
        }
    }
    f
}

/// The s values where whether a body blocks can change: a knot past
/// either edge of its span or under its middle, a top or a bottom crossing the band's lines
/// past them, and a gentle top crossing `climb` under its middle. At most
/// `out.len()`; returns how many.
pub fn turns(k: Knots, r: f32, b: Band, out: &mut [f32]) -> usize {
    let e = r + crate::solid::PUSH_EPS;
    let mut m = 0;
    let mut add = |v: f32, m: &mut usize| {
        if *m < out.len() {
            out[*m] = v;
            *m += 1;
        }
    };
    for p in k {
        add(p[0] + e, &mut m);
        add(p[0] - e, &mut m);
        // The middle past a knot: where a gentle top under it meets a
        // steep one.
        add(p[0] + crate::solid::PUSH_EPS, &mut m);
        add(p[0] - crate::solid::PUSH_EPS, &mut m);
    }
    for i in 0..k.len() - 1 {
        let (p, q) = (k[i], k[i + 1]);
        if p[0] == q[0] {
            continue;
        }
        let cross = |a: f32, b: f32, line: f32| -> Option<f32> {
            if (a - line) * (b - line) < 0.0 {
                Some(p[0] + (q[0] - p[0]) * ((line - a) / (b - a)))
            } else {
                None
            }
        };
        if steep(p, q) {
            if let Some(s) = cross(p[2], q[2], b.feet + STEEP_EPS) {
                add(s + e, &mut m);
                add(s - e, &mut m);
            }
        } else if let Some(s) = cross(p[2], q[2], b.climb) {
            add(s + crate::solid::PUSH_EPS, &mut m);
            add(s - crate::solid::PUSH_EPS, &mut m);
        }
        if let Some(s) = cross(p[1], q[1], b.hi) {
            add(s + e, &mut m);
            add(s - e, &mut m);
        }
    }
    m
}

/// Whether (s, y) is in the profile, within `eps`.
pub fn holds(k: Knots, s: f32, y: f32, eps: f32) -> bool {
    let n = k.len();
    if s < k[0][0] - eps || s > k[n - 1][0] + eps {
        return false;
    }
    let s = s.clamp(k[0][0], k[n - 1][0]);
    match at(k, s) {
        Some((b, t)) => y >= b - eps && y <= t + eps,
        None => false,
    }
}

/// The roots of a·t² + b·t + c in the stable form; a line when a is 0.
pub fn roots(a: f32, b: f32, c: f32) -> [Option<f32>; 2] {
    if a == 0.0 {
        return if b != 0.0 { [Some(-c / b), None] } else { [None, None] };
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return [None, None];
    }
    let sq = disc.sqrt();
    let q = if b >= 0.0 { -0.5 * (b + sq) } else { -0.5 * (b - sq) };
    if q == 0.0 {
        return [Some(0.0), None];
    }
    [Some(q / a), Some(c / q)]
}

/// One half-space of a convex clip: `a + b·t <= 0`. Narrows [t0, t1] and
/// names the piece that last raised t0; false if nothing is left.
fn clip(a: f32, b: f32, piece: Piece, t0: &mut f32, t1: &mut f32, at0: &mut Piece) -> bool {
    if b == 0.0 {
        return a <= 0.0;
    }
    let t = -a / b;
    if b > 0.0 {
        if t < *t1 {
            *t1 = t;
        }
    } else if t > *t0 {
        *t0 = t;
        *at0 = piece;
    }
    *t0 <= *t1
}

/// A ray against a carried profile, in its own frame: `o` and `d` as (u
/// along the axis, v across it, y over the solid's `y`), the axis `l` to
/// either side. Each segment is a convex prism: the first entry over all
/// of them. A ray starting inside reports 0.
pub fn ray_carried(k: Knots, l: f32, o: [f32; 3], d: [f32; 3], tmax: f32) -> Option<(f32, Piece)> {
    // First the box round it in its own frame: most rays miss that, and
    // one that meets it crosses only some of the segments.
    let (yb, yt) = span_y(k);
    let n = k.len();
    let (mut t0, mut t1) = (0.0f32, tmax);
    for (o, d, lo, hi) in [
        (o[0], d[0], -l, l),
        (o[1], d[1], k[0][0], k[n - 1][0]),
        (o[2], d[2], yb, yt),
    ] {
        if d == 0.0 {
            if o < lo || o > hi {
                return None;
            }
        } else {
            let (a, b) = ((lo - o) / d, (hi - o) / d);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
    }
    if t0 > t1 {
        return None;
    }
    // The span of s the ray crosses in the box, with slack for its rounding.
    let (va, vb) = (o[1] + d[1] * t0, o[1] + d[1] * t1);
    let slack = 1e-3 + 1e-4 * (fabs(va) + fabs(vb));
    let (vlo, vhi) = (va.min(vb) - slack, va.max(vb) + slack);
    let mut best: Option<(f32, Piece)> = None;
    for i in 0..n - 1 {
        let (p, q) = (k[i], k[i + 1]);
        if p[0] == q[0] || q[0] < vlo || p[0] > vhi {
            continue;
        }
        let (mut t0, mut t1) = (0.0f32, best.map_or(tmax, |b| b.0));
        let mut at0 = Piece::Inside;
        let w = q[0] - p[0];
        let mt = (q[2] - p[2]) / w;
        let mb = (q[1] - p[1]) / w;
        let ok = clip(-l - o[0], -d[0], Piece::End(0), &mut t0, &mut t1, &mut at0)
            && clip(o[0] - l, d[0], Piece::End(1), &mut t0, &mut t1, &mut at0)
            && clip(
                p[0] - o[1],
                -d[1],
                Piece::Face {
                    at: i as u8,
                    out: false,
                },
                &mut t0,
                &mut t1,
                &mut at0,
            )
            && clip(
                o[1] - q[0],
                d[1],
                Piece::Face {
                    at: (i + 1) as u8,
                    out: true,
                },
                &mut t0,
                &mut t1,
                &mut at0,
            )
            && clip(
                o[2] - p[2] - mt * (o[1] - p[0]),
                d[2] - mt * d[1],
                Piece::Top(i as u8),
                &mut t0,
                &mut t1,
                &mut at0,
            )
            && clip(
                p[1] + mb * (o[1] - p[0]) - o[2],
                mb * d[1] - d[2],
                Piece::Bot(i as u8),
                &mut t0,
                &mut t1,
                &mut at0,
            );
        if ok && best.is_none_or(|b| t0 < b.0) {
            best = Some((t0, at0));
        }
    }
    best
}

/// A spun profile's sector: the edges' directions (cos, sin, from +x
/// toward +z), the sector between them, at most half a turn; and the box
/// round it from the axis (min x, min z, max x, max z), kept so the grid
/// and the rays' quick looks needn't work it out again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sector {
    pub e0: [f32; 2],
    pub e1: [f32; 2],
    pub bb: [f32; 4],
}

impl Sector {
    /// Between the edges `e0` and `e1`, from `s0` to `sn` off the axis.
    pub fn new(e0: [f32; 2], e1: [f32; 2], s0: f32, sn: f32) -> Sector {
        let mut sec = Sector { e0, e1, bb: [0.0; 4] };
        let mut b = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
        let mut put = |px: f32, pz: f32| {
            b = [b[0].min(px), b[1].min(pz), b[2].max(px), b[3].max(pz)];
        };
        for e in [e0, e1] {
            put(e[0] * sn, e[1] * sn);
            put(e[0] * s0, e[1] * s0);
        }
        for (ax, az) in [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)] {
            if sec.holds(ax, az, 0.0) {
                put(ax * sn, az * sn);
            }
        }
        sec.bb = b;
        sec
    }

    /// Whether (x, z) (from the axis) is within the sector, by `eps`.
    pub fn holds(&self, x: f32, z: f32, eps: f32) -> bool {
        self.e0[0] * z - self.e0[1] * x >= -eps && x * self.e1[1] - z * self.e1[0] >= -eps
    }

    /// The outward normal of edge `i`, in x and z.
    pub fn out(&self, i: u8) -> [f32; 2] {
        if i == 0 {
            [self.e0[1], -self.e0[0]]
        } else {
            [-self.e1[1], self.e1[0]]
        }
    }
}

/// A ray against a spun profile, in its own frame (from the axis, y over
/// the solid's `y`): the first point of the ray in the solid. Each top
/// and bottom segment is a cone (a disc where flat), each face a cylinder,
/// a sector's edges half-planes; the nearest root on its own piece, the
/// mirror cone's turned away.
pub fn ray_spun(k: Knots, sector: Option<Sector>, o: [f32; 3], d: [f32; 3], tmax: f32) -> Option<(f32, Piece)> {
    let rho = |p: [f32; 3]| (p[0] * p[0] + p[2] * p[2]).sqrt();
    let in_sector = |p: [f32; 3], eps: f32| sector.is_none_or(|s| s.holds(p[0], p[2], eps));
    let a2 = d[0] * d[0] + d[2] * d[2];
    let a1 = 2.0 * (o[0] * d[0] + o[2] * d[2]);
    let a0 = o[0] * o[0] + o[2] * o[2];
    // How far from the axis and how high the ray goes over [0, tmax]: the
    // distance's square is a parabola in t, least at its vertex or an end,
    // most at an end. A ray that stays in a ring's hole never meets it
    // (most of the lines in a round room against its own wall).
    let r2 = |t: f32| a0 + (a1 + a2 * t) * t;
    let (e0, e1) = (r2(0.0), r2(tmax));
    let s0 = k[0][0];
    if s0 > 0.0 && e0.max(e1) < s0 * s0 * (1.0 - 1e-4) {
        return None;
    }
    let tv = if a2 > 0.0 {
        (-a1 / (2.0 * a2)).clamp(0.0, tmax)
    } else {
        0.0
    };
    let (near, far) = (r2(tv).min(e0).min(e1).max(0.0).sqrt(), e0.max(e1).sqrt());
    let (ylo, yhi) = (o[1].min(o[1] + d[1] * tmax), o[1].max(o[1] + d[1] * tmax));
    // Slack past the ranges for the rounding in them.
    let slack = 1e-3 + 1e-4 * far;
    let ys = |a: f32, b: f32| a.max(b) >= ylo - slack && a.min(b) <= yhi + slack;
    let ss = |a: f32, b: f32| b >= near - slack && a <= far + slack;
    if holds(k, rho(o), o[1], 0.0) && in_sector(o, 0.0) {
        // Strictly: on its surface counts as inside.
        return Some((0.0, Piece::Inside));
    }
    let pt = |t: f32| [o[0] + d[0] * t, o[1] + d[1] * t, o[2] + d[2] * t];
    let mut best: Option<(f32, Piece)> = None;
    let take = |t: f32, piece: Piece, best: &mut Option<(f32, Piece)>| {
        if t >= 0.0 && t <= tmax && best.is_none_or(|b| t < b.0) {
            *best = Some((t, piece));
        }
    };
    let n = k.len();
    for i in 0..n - 1 {
        let (p, q) = (k[i], k[i + 1]);
        if p[0] == q[0] || !ss(p[0], q[0]) {
            continue;
        }
        for (top, piece) in [(true, Piece::Top(i as u8)), (false, Piece::Bot(i as u8))] {
            let (y0, y1) = if top { (p[2], q[2]) } else { (p[1], q[1]) };
            if !ys(y0, y1) {
                continue;
            }
            let m = (y1 - y0) / (q[0] - p[0]);
            let a = y0 - m * p[0];
            let on = |t: f32| {
                let x = pt(t);
                let r = rho(x);
                r >= p[0] - ON_EPS
                    && r <= q[0] + ON_EPS
                    && fabs(x[1] - (a + m * r)) <= 1e-3 * (1.0 + fabs(m))
                    && in_sector(x, ON_EPS)
            };
            if m == 0.0 {
                if d[1] != 0.0 {
                    let t = (a - o[1]) / d[1];
                    if on(t) {
                        take(t, piece, &mut best);
                    }
                }
                continue;
            }
            let w = o[1] - a;
            let m2 = m * m;
            for t in roots(d[1] * d[1] - m2 * a2, 2.0 * w * d[1] - m2 * a1, w * w - m2 * a0)
                .into_iter()
                .flatten()
            {
                if on(t) {
                    take(t, piece, &mut best);
                }
            }
        }
    }
    for j in 0..n {
        let inner = k[0][0] > 0.0;
        let Some((fb, ft)) = face(k, j, inner) else {
            continue;
        };
        let s = k[j][0];
        if s <= 0.0 || a2 == 0.0 || !ss(s, s) || !ys(fb, ft) {
            continue;
        }
        for t in roots(a2, a1, a0 - s * s).into_iter().flatten() {
            let x = pt(t);
            if x[1] >= fb - ON_EPS && x[1] <= ft + ON_EPS && in_sector(x, ON_EPS) {
                // Which way it faces: toward +s at an outer end, or where
                // the material at this height is on the inner side.
                let out = if j + 1 == n {
                    true
                } else if j == 0 && !(j + 1 < n && k[1][0] == s) {
                    false
                } else {
                    let (l, r) = (k[j], k[j + 1]);
                    let in_l = x[1] >= l[1] && x[1] <= l[2];
                    let in_r = x[1] >= r[1] && x[1] <= r[2];
                    in_l && !in_r
                };
                take(t, Piece::Face { at: j as u8, out }, &mut best);
            }
        }
    }
    if let Some(sec) = sector {
        for (i, e) in [(0u8, sec.e0), (1u8, sec.e1)] {
            let den = e[0] * d[2] - e[1] * d[0];
            if den == 0.0 {
                continue;
            }
            let t = -(e[0] * o[2] - e[1] * o[0]) / den;
            let x = pt(t);
            let along = x[0] * e[0] + x[2] * e[1];
            if along >= -ON_EPS && holds(k, along, x[1], ON_EPS) {
                take(t, Piece::End(i), &mut best);
            }
        }
    }
    best
}

/// A piece's outward normal at `p` in the profile's plane: (along s, y).
pub fn normal_2d(k: Knots, piece: Piece) -> (f32, f32) {
    match piece {
        Piece::Top(i) | Piece::Bot(i) => {
            let (p, q) = (k[i as usize], k[i as usize + 1]);
            let top = matches!(piece, Piece::Top(_));
            let (y0, y1) = if top { (p[2], q[2]) } else { (p[1], q[1]) };
            let m = (y1 - y0) / (q[0] - p[0]);
            let l = (1.0 + m * m).sqrt();
            if top {
                (-m / l, 1.0 / l)
            } else {
                (m / l, -1.0 / l)
            }
        }
        Piece::Face { out, .. } => (if out { 1.0 } else { -1.0 }, 0.0),
        Piece::End(_) | Piece::Inside => (0.0, 1.0),
    }
}

/// The world normal of a piece: `across` is +s's unit direction in x and
/// z, `end` an end cap's outward one.
pub fn normal(k: Knots, piece: Piece, across: [f32; 2], end: [f32; 2]) -> V3 {
    if let Piece::End(_) = piece {
        return V3::new(end[0], 0.0, end[1]);
    }
    let (ns, ny) = normal_2d(k, piece);
    V3::new(across[0] * ns, ny, across[1] * ns)
}

#[cfg(test)]
mod tests {
    use super::*;

    static DOME: [[f32; 3]; 4] = [[0.0, 0.0, 1.0], [1.0, 0.0, 0.9], [2.0, 0.0, 0.5], [2.5, 0.0, 0.0]];

    #[test]
    fn at_reads_between_knots_and_unions_a_step() {
        static K: [[f32; 3]; 4] = [[0.0, 0.0, 3.0], [1.0, 0.0, 3.0], [1.0, 2.0, 3.0], [2.0, 2.0, 3.0]];
        assert_eq!(at(&K, 0.5), Some((0.0, 3.0)));
        assert_eq!(at(&K, 1.0), Some((0.0, 3.0)));
        assert_eq!(at(&K, 1.5), Some((2.0, 3.0)));
        assert_eq!(at(&K, 2.5), None);
        assert_eq!(at(&DOME, 1.5), Some((0.0, 0.7)));
    }

    #[test]
    fn the_quadratic_roots_are_the_line_where_flat() {
        let [a, b] = roots(1.0, -3.0, 2.0);
        let mut v = [a.unwrap(), b.unwrap()];
        v.sort_by(f32::total_cmp);
        assert_eq!(v, [1.0, 2.0]);
        assert_eq!(roots(0.0, 2.0, -4.0), [Some(2.0), None]);
        assert_eq!(roots(1.0, 0.0, 1.0), [None, None]);
    }
}

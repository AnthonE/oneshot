//! The map's solids that aren't axis-aligned boxes: round pillars and pods
//! (upright cylinders), turned boxes (a box turned about y: angled walls,
//! and a curved wall as a chain of short ones, which the map compiler lays
//! along an arc), and ramps (a box whose top slopes from one side to the
//! opposite one). What the client draws is these exactly, so a shot stops
//! where it looks like it should.
//!
//! A body meets them as a circle in x and z (boxes keep their square
//! footprint and their sweep, `movement::step`): moved first, then pushed
//! back out along the shortest way and its speed into the face taken off,
//! which slides it along a curve. A ramp is floor (walked up, its height
//! under the footprint's middle) until it stands more than a step over the
//! feet; past that it is a wall like any other. All of it is `+ − × ÷
//! sqrt min max`, and none of it runs on a map without solids.
//!
//! Profile solids ([`crate::profile`]: a cross-section spun round an
//! upright axis or carried along a level one) are round in height too:
//! their own rules for what blocks, what is floor and where a body is
//! pushed, through [`Solid::blocks`], [`Solid::floor_for`],
//! [`Solid::bot_over`] and [`Solid::push_band`].

use crate::fmath::fabs;
use crate::geom::{ray_aabb, ray_cylinder, Aabb, V3};
use crate::profile::{self, Band, Cover, Knots, Piece, Sector};

/// How far past a face a push leaves a body: over the position grid's step
/// (1/1024 m), so the snap never puts it back inside.
pub const PUSH_EPS: f32 = 0.002;

/// A solid's shape in x and z, and its span in y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// An upright cylinder about (x, z) of radius `r`.
    Round { x: f32, z: f32, r: f32 },
    /// A box of half-sizes `hx` along its own x, `hz` along its own z,
    /// about (x, z), its own x axis along (`c`, `s`) in the world's x and z
    /// (cos and sin of its turn, from +x toward +z).
    Turned {
        x: f32,
        z: f32,
        hx: f32,
        hz: f32,
        c: f32,
        s: f32,
    },
    /// Over the rectangle (x0, z0)..(x1, z1), its top rising from `low` on
    /// one side to `high` on the opposite one along `dir` (0 +x, 1 −x, 2 +z,
    /// 3 −z: the way up).
    Ramp {
        x0: f32,
        z0: f32,
        x1: f32,
        z1: f32,
        dir: u8,
        low: f32,
        high: f32,
    },
    /// A profile spun about the upright axis at (x, z), its heights over
    /// `y`, s the distance from the axis; all the way round, or within a
    /// sector of at most half a turn.
    Revolve {
        x: f32,
        z: f32,
        y: f32,
        sector: Option<Sector>,
        k: Knots,
    },
    /// A profile carried along the level axis through (x, z), `l` to
    /// either side, the axis along (`c`, `s`) as a turned box's own x; s
    /// in the profile is across it, along (−`s`, `c`).
    Sweep {
        x: f32,
        z: f32,
        y: f32,
        c: f32,
        s: f32,
        l: f32,
        k: Knots,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Solid {
    pub shape: Shape,
    /// Bottom and top: a ramp's top is its high side's.
    pub y0: f32,
    pub y1: f32,
}

impl Solid {
    /// From the compiled table: (kind, numbers) as `tools/maps/compile.mjs`
    /// writes them. 0 round: x z r · · · y0 y1; 1 turned: x z hx hz c s y0
    /// y1; 2 ramp: x0 z0 x1 z1 dir base low high.
    pub fn from_def(kind: u8, n: [f32; 8]) -> Solid {
        match kind {
            0 => Solid {
                shape: Shape::Round {
                    x: n[0],
                    z: n[1],
                    r: n[2],
                },
                y0: n[6],
                y1: n[7],
            },
            1 => Solid {
                shape: Shape::Turned {
                    x: n[0],
                    z: n[1],
                    hx: n[2],
                    hz: n[3],
                    c: n[4],
                    s: n[5],
                },
                y0: n[6],
                y1: n[7],
            },
            _ => Solid {
                shape: Shape::Ramp {
                    x0: n[0],
                    z0: n[1],
                    x1: n[2],
                    z1: n[3],
                    dir: n[4] as u8,
                    low: n[6],
                    high: n[7],
                },
                y0: n[5],
                y1: n[6].max(n[7]),
            },
        }
    }

    /// A profile solid from the compiled table: (kind, numbers, knots) as
    /// `tools/maps/compile.mjs` writes them. 3 spun all the way round: x z
    /// y; 4 a spun sector: x z y, the edges' cos and sin; 5 carried: x z
    /// y c s l.
    pub fn profile(kind: u8, n: [f32; 8], k: Knots) -> Solid {
        profile::check(k);
        let (b, t) = profile::span_y(k);
        let shape = match kind {
            3 => Shape::Revolve {
                x: n[0],
                z: n[1],
                y: n[2],
                sector: None,
                k,
            },
            4 => Shape::Revolve {
                x: n[0],
                z: n[1],
                y: n[2],
                sector: Some(Sector::new([n[3], n[4]], [n[5], n[6]], k[0][0], k[k.len() - 1][0])),
                k,
            },
            _ => Shape::Sweep {
                x: n[0],
                z: n[1],
                y: n[2],
                c: n[3],
                s: n[4],
                l: n[5],
                k,
            },
        };
        if kind == 3 || kind == 4 {
            assert!(k[0][0] >= 0.0, "a spun profile starts at or past its axis");
        }
        Solid {
            shape,
            y0: n[2] + b,
            y1: n[2] + t,
        }
    }

    /// Whether it is a profile solid, round in height ([`crate::profile`]).
    pub fn is_profile(&self) -> bool {
        matches!(self.shape, Shape::Revolve { .. } | Shape::Sweep { .. })
    }

    /// The box around it, for the grid.
    pub fn bounds(&self) -> Aabb {
        let (lx, lz, hx, hz) = match self.shape {
            Shape::Round { x, z, r } => (x - r, z - r, x + r, z + r),
            Shape::Turned { x, z, hx, hz, c, s } => {
                let ex = fabs(c) * hx + fabs(s) * hz;
                let ez = fabs(s) * hx + fabs(c) * hz;
                (x - ex, z - ez, x + ex, z + ez)
            }
            Shape::Ramp { x0, z0, x1, z1, .. } => (x0, z0, x1, z1),
            Shape::Revolve { x, z, sector, k, .. } => {
                let sn = k[k.len() - 1][0];
                match sector {
                    None => (x - sn, z - sn, x + sn, z + sn),
                    Some(sec) => (x + sec.bb[0], z + sec.bb[1], x + sec.bb[2], z + sec.bb[3]),
                }
            }
            Shape::Sweep { x, z, c, s, l, k, .. } => {
                let (s0, sn) = (k[0][0], k[k.len() - 1][0]);
                let mut b = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
                for u in [-l, l] {
                    for v in [s0, sn] {
                        let (px, pz) = (u * c - v * s, u * s + v * c);
                        b = (b.0.min(px), b.1.min(pz), b.2.max(px), b.3.max(pz));
                    }
                }
                (x + b.0, z + b.1, x + b.2, z + b.3)
            }
        };
        Aabb::new(V3::new(lx, self.y0, lz), V3::new(hx, self.y1, hz))
    }

    /// A profile solid's knots, height, and whether its first knot is an
    /// end (not the axis).
    fn knots(&self) -> Option<(Knots, f32, bool)> {
        match self.shape {
            Shape::Revolve { y, k, .. } => Some((k, y, k[0][0] > 0.0)),
            Shape::Sweep { y, k, .. } => Some((k, y, true)),
            _ => None,
        }
    }

    /// (x, z) in a profile solid's own terms: its s, the unit way s grows
    /// there in x and z, and along the axis (carried) or 0.
    fn local(&self, x: f32, z: f32) -> (f32, [f32; 2], f32) {
        match self.shape {
            Shape::Revolve { x: cx, z: cz, .. } => {
                let (dx, dz) = (x - cx, z - cz);
                let s = (dx * dx + dz * dz).sqrt();
                let u = if s > 0.0 { [dx / s, dz / s] } else { [1.0, 0.0] };
                (s, u, 0.0)
            }
            Shape::Sweep { x: cx, z: cz, c, s, .. } => {
                let (dx, dz) = (x - cx, z - cz);
                (dz * c - dx * s, [-s, c], dx * c + dz * s)
            }
            _ => (0.0, [1.0, 0.0], 0.0),
        }
    }

    /// The span of a profile solid a body of radius `r` at (x, z) covers,
    /// if it covers any ([`Cover`]).
    pub fn cover(&self, x: f32, z: f32, r: f32) -> Option<Cover> {
        let (k, _, _) = self.knots()?;
        let (s0, sn) = (k[0][0], k[k.len() - 1][0]);
        match self.shape {
            Shape::Revolve {
                x: cx, z: cz, sector, ..
            } => {
                let (dx, dz) = (x - cx, z - cz);
                if let Some(sec) = sector {
                    let in0 = sec.e0[0] * dz - sec.e0[1] * dx >= 0.0;
                    let in1 = dx * sec.e1[1] - dz * sec.e1[0] >= 0.0;
                    if !(in0 && in1) {
                        // Past an edge: the chord on that edge's ray.
                        let mut best: Option<(f32, Cover)> = None;
                        for (e, inside) in [(sec.e0, in0), (sec.e1, in1)] {
                            if inside {
                                continue;
                            }
                            let along = dx * e[0] + dz * e[1];
                            let perp = fabs(e[0] * dz - e[1] * dx);
                            if perp >= r {
                                continue;
                            }
                            let h = (r * r - perp * perp).sqrt();
                            if along - h >= sn || along + h <= s0 {
                                continue;
                            }
                            if best.is_none_or(|b| perp < b.0) {
                                best = Some((
                                    perp,
                                    Cover {
                                        sa: (along - h).max(s0),
                                        sb: (along + h).min(sn),
                                        sc: along.clamp(s0, sn),
                                        end: true,
                                    },
                                ));
                            }
                        }
                        return best.map(|b| b.1);
                    }
                }
                let s = (dx * dx + dz * dz).sqrt();
                if s - r >= sn || s + r <= s0 {
                    return None;
                }
                Some(Cover {
                    sa: (s - r).max(s0),
                    sb: (s + r).min(sn),
                    sc: s.clamp(s0, sn),
                    end: false,
                })
            }
            Shape::Sweep { l, .. } => {
                let (v, _, u) = self.local(x, z);
                let au = fabs(u);
                if au >= l + r {
                    return None;
                }
                let (half, end) = if au <= l {
                    (r, false)
                } else {
                    let e = au - l;
                    ((r * r - e * e).sqrt(), true)
                };
                if v - half >= sn || v + half <= s0 {
                    return None;
                }
                Some(Cover {
                    sa: (v - half).max(s0),
                    sb: (v + half).min(sn),
                    sc: v.clamp(s0, sn),
                    end,
                })
            }
            _ => None,
        }
    }

    /// Whether a profile solid blocks a body of radius `r` at (x, z) in
    /// `band` ([`profile::blocks`]); false for the other kinds.
    pub fn blocks(&self, x: f32, z: f32, r: f32, band: Band) -> bool {
        let Some((k, y, inner)) = self.knots() else {
            return false;
        };
        self.cover(x, z, r)
            .is_some_and(|c| profile::blocks(k, c, band.down_by(y), inner))
    }

    /// The top of a profile solid a body of radius `r` at (x, z) stands
    /// on, if any: a gentle top under its middle up to `ramp_reach`, a
    /// face's or a steep slope's up to `reach`.
    pub fn floor_for(&self, x: f32, z: f32, r: f32, reach: f32, ramp_reach: f32) -> Option<f32> {
        let (k, y, inner) = self.knots()?;
        let c = self.cover(x, z, r)?;
        profile::floor(k, c, reach - y, ramp_reach - y, inner).map(|t| t + y)
    }

    /// The lowest bottom of a profile solid over the footprint, if it
    /// covers any.
    pub fn bot_over(&self, x: f32, z: f32, r: f32) -> Option<f32> {
        let (k, y, _) = self.knots()?;
        let c = self.cover(x, z, r)?;
        Some(profile::extremes(k, c.sa, c.sb).0 + y)
    }

    /// Whether the point `p` is inside a profile solid.
    pub fn contains(&self, p: V3) -> bool {
        let Some((k, y, _)) = self.knots() else {
            return false;
        };
        let Some(c) = self.cover(p.x, p.z, 0.0) else {
            return false;
        };
        profile::at(k, c.sc).is_some_and(|(b, t)| p.y - y > b && p.y - y < t)
    }

    /// Where a body of radius `r` at (x, z) that a profile solid blocks in
    /// `band` goes to be clear of it, and the push's unit direction: the
    /// nearest place it is free, among where the answer can change
    /// ([`profile::turns`]), the far side of the axis's ends and a
    /// sector's edges. Across, a place on the side the body came from
    /// (`back`) first, so a fast body is never pushed out through a thin
    /// wall. None if it isn't blocked (or nowhere is free).
    pub fn push_band(&self, x: f32, z: f32, r: f32, band: Band, back: (f32, f32)) -> Option<(f32, f32, f32, f32)> {
        let (k, y, inner) = self.knots()?;
        let b = band.down_by(y);
        let free = |px: f32, pz: f32| !self.cover(px, pz, r).is_some_and(|c| profile::blocks(k, c, b, inner));
        if free(x, z) {
            return None;
        }
        let (s, unit, u) = self.local(x, z);
        let (sb, _, _) = self.local(back.0, back.1);
        let side = sb - s;
        let mut buf = [0.0f32; 4 * crate::consts::MAX_KNOTS * 2];
        let m = profile::turns(k, r, b, &mut buf);
        let spun = matches!(self.shape, Shape::Revolve { .. });
        let mut best: Option<(f32, f32, f32)> = None;
        let try_at = |px: f32, pz: f32, penalty: f32, best: &mut Option<(f32, f32, f32)>| {
            let key = (px - x) * (px - x) + (pz - z) * (pz - z) + penalty;
            if best.is_some_and(|b| key >= b.0) || !free(px, pz) {
                return;
            }
            *best = Some((key, px, pz));
        };
        for &s2 in &buf[..m] {
            if spun && s2 < 0.0 {
                continue;
            }
            let ds = s2 - s;
            let penalty = if ds * side < 0.0 && fabs(side) > 1e-4 { 1e6 } else { 0.0 };
            try_at(x + unit[0] * ds, z + unit[1] * ds, penalty, &mut best);
        }
        let e = r + PUSH_EPS;
        match self.shape {
            Shape::Sweep { c, s: sw, l, .. } => {
                for to in [l + e, -l - e] {
                    let du = to - u;
                    try_at(x + c * du, z + sw * du, 0.0, &mut best);
                }
            }
            Shape::Revolve {
                x: cx,
                z: cz,
                sector: Some(sec),
                ..
            } => {
                for i in 0..2u8 {
                    let n = sec.out(i);
                    let d = (x - cx) * n[0] + (z - cz) * n[1];
                    if d < e {
                        try_at(x + n[0] * (e - d), z + n[1] * (e - d), 0.0, &mut best);
                    }
                }
            }
            _ => {}
        }
        let (_, px, pz) = best?;
        let (dx, dz) = (px - x, pz - z);
        let l = (dx * dx + dz * dz).sqrt();
        if l == 0.0 {
            return None;
        }
        Some((px, pz, dx / l, dz / l))
    }

    /// Where a ray from `o` along unit `d` first meets a profile solid,
    /// within `[0, tmax]`, and the piece it met; the other kinds report
    /// [`Piece::Inside`] for the piece.
    pub fn ray_hit(&self, o: V3, d: V3, tmax: f32) -> Option<(f32, Piece)> {
        match self.shape {
            Shape::Revolve { x, z, y, sector, k } => {
                // A ray whose ends are both in a ring's hole never meets it
                // (the farthest it gets from the axis is at an end).
                let s0 = k[0][0];
                if s0 > 0.0 {
                    let (ax, az) = (o.x - x, o.z - z);
                    let (bx, bz) = (ax + d.x * tmax, az + d.z * tmax);
                    let lim = s0 * s0 * (1.0 - 1e-4);
                    if ax * ax + az * az < lim && bx * bx + bz * bz < lim {
                        return None;
                    }
                }
                // From where the ray enters the box round it to where it
                // leaves: nearer the axis, so the quadratics keep their
                // bits, and no longer than it is in there.
                let (t0, t1) = slabs(o, d, &self.bounds(), tmax)?;
                let p = o + d * t0;
                profile::ray_spun(k, sector, [p.x - x, p.y - y, p.z - z], [d.x, d.y, d.z], t1 - t0)
                    .map(|(t, piece)| (t0 + t, piece))
            }
            Shape::Sweep { x, z, y, c, s, l, k } => {
                let (dx, dz) = (o.x - x, o.z - z);
                let lo = [dx * c + dz * s, dz * c - dx * s, o.y - y];
                let ld = [d.x * c + d.z * s, d.z * c - d.x * s, d.y];
                profile::ray_carried(k, l, lo, ld, tmax)
            }
            _ => self.ray_exact(o, d, tmax).map(|t| (t, Piece::Inside)),
        }
    }

    /// A profile solid's outward normal at `p` on `piece` (a ray's hit);
    /// inside, its nearer of the top and the bottom. Straight up for the
    /// other kinds.
    pub fn normal(&self, piece: Piece, p: V3) -> V3 {
        let Some((k, y, _)) = self.knots() else {
            return V3::new(0.0, 1.0, 0.0);
        };
        let (s, across, _) = self.local(p.x, p.z);
        let piece = match piece {
            Piece::Inside => {
                let n = k.len();
                let sc = s.clamp(k[0][0], k[n - 1][0]);
                let mut i = 0;
                while i + 2 < n && !(k[i][0] < k[i + 1][0] && sc <= k[i + 1][0]) {
                    i += 1;
                }
                let (b, t) = profile::at(k, sc).unwrap_or((0.0, 0.0));
                if fabs(p.y - y - t) <= fabs(p.y - y - b) {
                    Piece::Top(i as u8)
                } else {
                    Piece::Bot(i as u8)
                }
            }
            other => other,
        };
        let end = match (self.shape, piece) {
            (Shape::Revolve { sector: Some(sec), .. }, Piece::End(i)) => sec.out(i),
            (Shape::Sweep { c, s, .. }, Piece::End(0)) => [-c, -s],
            (Shape::Sweep { c, s, .. }, Piece::End(_)) => [c, s],
            _ => [0.0, 0.0],
        };
        profile::normal(k, piece, across, end)
    }

    /// The top's height at (x, z): flat but on a ramp, where it is the
    /// slope's at the nearest point of the rectangle.
    pub fn top_at(&self, x: f32, z: f32) -> f32 {
        match self.shape {
            Shape::Ramp {
                x0,
                z0,
                x1,
                z1,
                dir,
                low,
                high,
            } => {
                let u = match dir {
                    0 => (x.clamp(x0, x1) - x0) / (x1 - x0),
                    1 => (x1 - x.clamp(x0, x1)) / (x1 - x0),
                    2 => (z.clamp(z0, z1) - z0) / (z1 - z0),
                    _ => (z1 - z.clamp(z0, z1)) / (z1 - z0),
                };
                low + (high - low) * u
            }
            Shape::Revolve { y, k, .. } | Shape::Sweep { y, k, .. } => {
                let (s, _, _) = self.local(x, z);
                let s = s.clamp(k[0][0], k[k.len() - 1][0]);
                profile::at(k, s).map_or(self.y1, |(_, t)| t + y)
            }
            _ => self.y1,
        }
    }

    /// The bottom's height at (x, z): flat but on a profile solid.
    pub fn bot_at(&self, x: f32, z: f32) -> f32 {
        match self.shape {
            Shape::Revolve { y, k, .. } | Shape::Sweep { y, k, .. } => {
                let (s, _, _) = self.local(x, z);
                let s = s.clamp(k[0][0], k[k.len() - 1][0]);
                profile::at(k, s).map_or(self.y0, |(b, _)| b + y)
            }
            _ => self.y0,
        }
    }

    /// The point of the footprint nearest (x, z) and whether (x, z) is
    /// inside it; for a body of radius `r`, the overlap test and the push.
    fn nearest(&self, x: f32, z: f32) -> (f32, f32, bool) {
        match self.shape {
            Shape::Round { x: cx, z: cz, r } => {
                let (dx, dz) = (x - cx, z - cz);
                let d2 = dx * dx + dz * dz;
                if d2 <= r * r {
                    return (x, z, true);
                }
                let k = r / d2.sqrt();
                (cx + dx * k, cz + dz * k, false)
            }
            Shape::Turned {
                x: cx,
                z: cz,
                hx,
                hz,
                c,
                s,
            } => {
                let (dx, dz) = (x - cx, z - cz);
                let (lx, lz) = (dx * c + dz * s, dz * c - dx * s);
                let (qx, qz) = (lx.clamp(-hx, hx), lz.clamp(-hz, hz));
                (cx + qx * c - qz * s, cz + qx * s + qz * c, qx == lx && qz == lz)
            }
            Shape::Ramp { x0, z0, x1, z1, .. } => {
                let (qx, qz) = (x.clamp(x0, x1), z.clamp(z0, z1));
                (qx, qz, qx == x && qz == z)
            }
            // `over` and `push` answer these first.
            Shape::Revolve { .. } | Shape::Sweep { .. } => (x, z, false),
        }
    }

    /// Whether a circle of radius `r` at (x, z) overlaps the footprint
    /// (touching isn't).
    pub fn over(&self, x: f32, z: f32, r: f32) -> bool {
        if self.is_profile() {
            return self.cover(x, z, r).is_some();
        }
        let (qx, qz, inside) = self.nearest(x, z);
        inside || (x - qx) * (x - qx) + (z - qz) * (z - qz) < r * r
    }

    /// Where a circle of radius `r` at (x, z) is pushed to so it no longer
    /// overlaps the footprint, and the push's unit direction; None if it
    /// doesn't overlap. From inside, out through the nearest side.
    pub fn push(&self, x: f32, z: f32, r: f32) -> Option<(f32, f32, f32, f32)> {
        if self.is_profile() {
            return self.push_clear(x, z, r);
        }
        let (qx, qz, inside) = self.nearest(x, z);
        if !inside {
            let (dx, dz) = (x - qx, z - qz);
            let d2 = dx * dx + dz * dz;
            if d2 >= r * r {
                return None;
            }
            let d = d2.sqrt();
            let (nx, nz) = if d > 0.0 { (dx / d, dz / d) } else { (1.0, 0.0) };
            let k = r + PUSH_EPS;
            return Some((qx + nx * k, qz + nz * k, nx, nz));
        }
        // Inside: out the nearest side.
        let k = r + PUSH_EPS;
        match self.shape {
            Shape::Round { x: cx, z: cz, r: rr } => {
                let (dx, dz) = (x - cx, z - cz);
                let d = (dx * dx + dz * dz).sqrt();
                let (nx, nz) = if d > 0.0 { (dx / d, dz / d) } else { (1.0, 0.0) };
                Some((cx + nx * (rr + k), cz + nz * (rr + k), nx, nz))
            }
            Shape::Turned {
                x: cx,
                z: cz,
                hx,
                hz,
                c,
                s,
            } => {
                let (dx, dz) = (x - cx, z - cz);
                let (lx, lz) = (dx * c + dz * s, dz * c - dx * s);
                let (px, pz) = (hx - fabs(lx), hz - fabs(lz));
                let (ox, oz, nlx, nlz) = if px <= pz {
                    let sx = if lx < 0.0 { -1.0 } else { 1.0 };
                    (sx * (hx + k), lz, sx, 0.0)
                } else {
                    let sz = if lz < 0.0 { -1.0 } else { 1.0 };
                    (lx, sz * (hz + k), 0.0, sz)
                };
                Some((
                    cx + ox * c - oz * s,
                    cz + ox * s + oz * c,
                    nlx * c - nlz * s,
                    nlx * s + nlz * c,
                ))
            }
            Shape::Ramp { x0, z0, x1, z1, .. } => {
                let d = [x - x0, x1 - x, z - z0, z1 - z];
                let mut m = 0;
                for i in 1..4 {
                    if d[i] < d[m] {
                        m = i;
                    }
                }
                Some(match m {
                    0 => (x0 - k, z, -1.0, 0.0),
                    1 => (x1 + k, z, 1.0, 0.0),
                    2 => (x, z0 - k, 0.0, -1.0),
                    _ => (x, z1 + k, 0.0, 1.0),
                })
            }
            Shape::Revolve { .. } | Shape::Sweep { .. } => None,
        }
    }

    /// A profile solid's [`Self::push`]: out of all of its footprint, past
    /// the nearer of its ends (or a sector's edge, or a carried axis's end).
    fn push_clear(&self, x: f32, z: f32, r: f32) -> Option<(f32, f32, f32, f32)> {
        let (k, _, _) = self.knots()?;
        self.cover(x, z, r)?;
        let (s, unit, u) = self.local(x, z);
        let e = r + PUSH_EPS;
        let (s0, sn) = (k[0][0], k[k.len() - 1][0]);
        let spun = matches!(self.shape, Shape::Revolve { .. });
        let mut best: Option<(f32, f32, f32)> = None;
        let mut try_at = |px: f32, pz: f32| {
            let key = (px - x) * (px - x) + (pz - z) * (pz - z);
            if best.is_some_and(|b| key >= b.0) || self.cover(px, pz, r).is_some() {
                return;
            }
            best = Some((key, px, pz));
        };
        for s2 in [sn + e, s0 - e] {
            if spun && s2 < 0.0 {
                continue;
            }
            try_at(x + unit[0] * (s2 - s), z + unit[1] * (s2 - s));
        }
        match self.shape {
            Shape::Sweep { c, s: sw, l, .. } => {
                for to in [l + e, -l - e] {
                    try_at(x + c * (to - u), z + sw * (to - u));
                }
            }
            Shape::Revolve {
                x: cx,
                z: cz,
                sector: Some(sec),
                ..
            } => {
                for i in 0..2u8 {
                    let n = sec.out(i);
                    let d = (x - cx) * n[0] + (z - cz) * n[1];
                    if d < e {
                        try_at(x + n[0] * (e - d), z + n[1] * (e - d));
                    }
                }
            }
            _ => {}
        }
        let (_, px, pz) = best?;
        let (dx, dz) = (px - x, pz - z);
        let l = (dx * dx + dz * dz).sqrt();
        let (nx, nz) = if l > 0.0 { (dx / l, dz / l) } else { (1.0, 0.0) };
        Some((px, pz, nx, nz))
    }

    /// Where a ray from `o` along unit `d` first meets it, within `[0, tmax]`.
    pub fn ray(&self, o: V3, d: V3, tmax: f32) -> Option<f32> {
        if self.wide_of(o, d) {
            return None;
        }
        self.ray_exact(o, d, tmax)
    }

    /// [`Self::ray`] without the quick look first: what the tests hold it to.
    pub fn ray_exact(&self, o: V3, d: V3, tmax: f32) -> Option<f32> {
        match self.shape {
            Shape::Round { x, z, r } => ray_cylinder(o, d, V3::new(x, self.y0, z), r, self.y1 - self.y0, tmax),
            Shape::Turned { x, z, hx, hz, c, s } => {
                let (dx, dz) = (o.x - x, o.z - z);
                let lo = V3::new(dx * c + dz * s, o.y, dz * c - dx * s);
                let ld = V3::new(d.x * c + d.z * s, d.y, d.z * c - d.x * s);
                let b = Aabb::new(V3::new(-hx, self.y0, -hz), V3::new(hx, self.y1, hz));
                ray_aabb(lo, ld, &b, tmax)
            }
            Shape::Ramp {
                x0,
                z0,
                x1,
                z1,
                dir,
                low,
                high,
            } => {
                // The box's slabs, then the half-space under the slope:
                // y − low − k·(u − u0) ≤ 0 along the way up.
                let b = Aabb::new(V3::new(x0, self.y0, z0), V3::new(x1, self.y1, z1));
                let (u0, du, len, ou) = match dir {
                    0 => (x0, d.x, x1 - x0, o.x),
                    1 => (-x1, -d.x, x1 - x0, -o.x),
                    2 => (z0, d.z, z1 - z0, o.z),
                    _ => (-z1, -d.z, z1 - z0, -o.z),
                };
                let k = (high - low) / len;
                // f(t) = a + b·t ≤ 0 inside.
                let a = o.y - low - k * (ou - u0);
                let bb = d.y - k * du;
                let (mut t0, mut t1) = (0.0f32, tmax);
                if bb == 0.0 {
                    if a > 0.0 {
                        return None;
                    }
                } else if bb > 0.0 {
                    t1 = t1.min(-a / bb);
                } else {
                    t0 = t0.max(-a / bb);
                }
                // Inside the box from its entry to its exit, and under the
                // slope over [t0, t1]: the first moment both hold.
                let (e0, e1) = slabs(o, d, &b, tmax)?;
                let lo = e0.max(t0);
                let hi = e1.min(t1);
                if lo <= hi {
                    Some(lo)
                } else {
                    None
                }
            }
            Shape::Revolve { .. } | Shape::Sweep { .. } => self.ray_hit(o, d, tmax).map(|h| h.0),
        }
    }
}

impl Solid {
    /// Whether a ray passes wide of the circle around a round or turned
    /// solid in x and z, so it can't meet it: a few products and no
    /// division, where most of the rays near a curved wall end (fog asks
    /// tens per pair). Never turns away a ray the exact test would stop, so
    /// the answer is the exact test's, bit for bit: the miss is measured by
    /// the cross product, whose rounding grows with the distance and not
    /// its square, and the circle is widened past that rounding at any
    /// distance on a map (`a_ray_past_a_curve_is_answered_as_the_exact_test_does`).
    fn wide_of(&self, o: V3, d: V3) -> bool {
        // A sector: also the circle round its box, far smaller than the
        // whole turn's for a short stretch of a big circle.
        if let Shape::Revolve {
            x,
            z,
            sector: Some(sec),
            ..
        } = self.shape
        {
            let (cx, cz) = (x + (sec.bb[0] + sec.bb[2]) * 0.5, z + (sec.bb[1] + sec.bb[3]) * 0.5);
            let (hx, hz) = ((sec.bb[2] - sec.bb[0]) * 0.5, (sec.bb[3] - sec.bb[1]) * 0.5);
            if Self::wide_of_circle(o, d, cx, cz, hx * hx + hz * hz) {
                return true;
            }
        }
        let (x, z, r2) = match self.shape {
            Shape::Round { x, z, r } => (x, z, r * r),
            Shape::Turned { x, z, hx, hz, .. } => (x, z, hx * hx + hz * hz),
            Shape::Ramp { .. } => return false,
            Shape::Revolve { x, z, k, .. } => {
                let sn = k[k.len() - 1][0];
                (x, z, sn * sn)
            }
            Shape::Sweep { x, z, l, k, .. } => {
                let (s0, sn) = (k[0][0], k[k.len() - 1][0]);
                (x, z, l * l + (s0 * s0).max(sn * sn))
            }
        };
        Self::wide_of_circle(o, d, x, z, r2)
    }

    /// Whether a ray passes wide of the circle of radius² `r2` about (x, z)
    /// ([`Self::wide_of`]'s test).
    fn wide_of_circle(o: V3, d: V3, x: f32, z: f32, r2: f32) -> bool {
        let dd = d.x * d.x + d.z * d.z;
        // Nearly straight up or down: the exact test.
        if dd < 1e-4 {
            return false;
        }
        let (mx, mz) = (o.x - x, o.z - z);
        let s = mx * mx + mz * mz;
        let wide = r2 * 1.001 + 1e-4 + s * 1e-6;
        // The line passes the middle at |cross| / |d| in x and z.
        let cross = mx * d.z - mz * d.x;
        if cross * cross > dd * wide {
            return true;
        }
        // Outside the circle and heading away from it.
        s > wide && mx * d.x + mz * d.z > 0.001
    }
}

/// A ray's entry and exit of a box within `[0, tmax]`.
fn slabs(o: V3, d: V3, b: &Aabb, tmax: f32) -> Option<(f32, f32)> {
    let mut t0 = 0.0f32;
    let mut t1 = tmax;
    for (o, d, lo, hi) in [
        (o.x, d.x, b.min.x, b.max.x),
        (o.y, d.y, b.min.y, b.max.y),
        (o.z, d.z, b.min.z, b.max.z),
    ] {
        if d == 0.0 {
            if o < lo || o > hi {
                return None;
            }
        } else {
            let a = (lo - o) / d;
            let c = (hi - o) / d;
            t0 = t0.max(a.min(c));
            t1 = t1.min(a.max(c));
        }
    }
    (t0 <= t1).then_some((t0, t1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmath::fabs;

    fn pillar() -> Solid {
        Solid::from_def(0, [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 3.0])
    }

    /// A 4 × 1 wall turned 45° about the origin.
    fn wall() -> Solid {
        let h = core::f32::consts::FRAC_1_SQRT_2;
        Solid::from_def(1, [0.0, 0.0, 2.0, 0.5, h, h, 0.0, 3.0])
    }

    /// Up +x from 0 at x = 0 to 2 m at x = 4, 2 m wide.
    fn ramp() -> Solid {
        Solid::from_def(2, [0.0, -1.0, 4.0, 1.0, 0.0, 0.0, 0.0, 2.0])
    }

    #[test]
    fn a_body_is_pushed_out_of_a_pillar_round_its_edge() {
        let p = pillar();
        let (x, z, nx, nz) = p.push(1.2, 0.0, 0.35).unwrap();
        assert!(fabs(x - (1.35 + PUSH_EPS)) < 1e-5 && z == 0.0, "{x} {z}");
        assert_eq!((nx, nz), (1.0, 0.0));
        assert!(!p.over(x, z, 0.35));
        assert!(p.push(2.0, 0.0, 0.35).is_none());
    }

    #[test]
    fn a_turned_wall_pushes_along_its_own_normal() {
        let w = wall();
        // Just off the wall's face, on its +z side (local z = 0.5).
        let (x, z, nx, nz) = w.push(-0.3, 0.3, 0.35).unwrap();
        let h = core::f32::consts::FRAC_1_SQRT_2;
        assert!(fabs(nx + h) < 1e-5 && fabs(nz - h) < 1e-5, "{nx} {nz}");
        assert!(!w.over(x, z, 0.35));
        // Far along the wall's length, on its axis, it is still inside.
        assert!(w.over(1.3, 1.3, 0.0));
        assert!(!w.over(1.3, -1.3, 0.35));
    }

    #[test]
    fn a_ramp_rises_along_its_way_up() {
        let r = ramp();
        assert_eq!(r.top_at(0.0, 0.0), 0.0);
        assert_eq!(r.top_at(2.0, 0.0), 1.0);
        assert_eq!(r.top_at(9.0, 0.0), 2.0);
        assert_eq!(r.y1, 2.0);
    }

    #[test]
    fn rays_meet_each_shape_where_it_is() {
        let d = V3::new(1.0, 0.0, 0.0);
        let t = pillar().ray(V3::new(-5.0, 1.0, 0.0), d, 100.0).unwrap();
        assert!(fabs(t - 4.0) < 1e-5);
        // The wall at 45°: a ray along x through the origin enters it where
        // the face 0.5 m off its axis crosses z = 0, at x = −0.5·√2.
        let t = wall().ray(V3::new(-5.0, 1.0, 0.0), d, 100.0).unwrap();
        assert!(fabs(t - (5.0 - core::f32::consts::SQRT_2 * 0.5)) < 1e-4, "{t}");
        // Over the ramp at 1.5 m: it meets the slope at x = 3.
        let t = ramp().ray(V3::new(-5.0, 1.5, 0.0), d, 100.0).unwrap();
        assert!(fabs(t - 8.0) < 1e-4, "{t}");
        // Down onto the ramp at x = 1: its top there is 0.5.
        let t = ramp()
            .ray(V3::new(1.0, 5.0, 0.0), V3::new(0.0, -1.0, 0.0), 100.0)
            .unwrap();
        assert!(fabs(t - 4.5) < 1e-4, "{t}");
        // Above the high end: nothing.
        assert!(ramp().ray(V3::new(-5.0, 2.5, 0.0), d, 100.0).is_none());
    }
}

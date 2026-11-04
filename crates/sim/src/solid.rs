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

use crate::fmath::fabs;
use crate::geom::{ray_aabb, ray_cylinder, Aabb, V3};

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
        };
        Aabb::new(V3::new(lx, self.y0, lz), V3::new(hx, self.y1, hz))
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
            _ => self.y1,
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
        }
    }

    /// Whether a circle of radius `r` at (x, z) overlaps the footprint
    /// (touching isn't).
    pub fn over(&self, x: f32, z: f32, r: f32) -> bool {
        let (qx, qz, inside) = self.nearest(x, z);
        inside || (x - qx) * (x - qx) + (z - qz) * (z - qz) < r * r
    }

    /// Where a circle of radius `r` at (x, z) is pushed to so it no longer
    /// overlaps the footprint, and the push's unit direction; None if it
    /// doesn't overlap. From inside, out through the nearest side.
    pub fn push(&self, x: f32, z: f32, r: f32) -> Option<(f32, f32, f32, f32)> {
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
        }
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
        let (x, z, r2) = match self.shape {
            Shape::Round { x, z, r } => (x, z, r * r),
            Shape::Turned { x, z, hx, hz, .. } => (x, z, hx * hx + hz * hz),
            Shape::Ramp { .. } => return false,
        };
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

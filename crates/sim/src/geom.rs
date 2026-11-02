//! Vectors, boxes and the ray tests shots and sight lines are made of. Plain
//! `+ − × ÷ sqrt min max` only: Rust never fuses a multiply-add on its own.

use core::ops::{Add, Mul, Neg, Sub};

/// y is up; x and z are the ground.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl V3 {
    pub const ZERO: V3 = V3 { x: 0.0, y: 0.0, z: 0.0 };

    pub const fn new(x: f32, y: f32, z: f32) -> V3 {
        V3 { x, y, z }
    }

    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }

    /// The unit vector, or zero for zero.
    pub fn norm(self) -> V3 {
        let l = self.len();
        if l > 0.0 {
            self * (1.0 / l)
        } else {
            V3::ZERO
        }
    }

    pub fn cross(self, o: V3) -> V3 {
        V3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn flat(self) -> V3 {
        V3::new(self.x, 0.0, self.z)
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        V3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        V3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, k: f32) -> V3 {
        V3::new(self.x * k, self.y * k, self.z * k)
    }
}

impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        V3::new(-self.x, -self.y, -self.z)
    }
}

/// An axis-aligned box: the map is made of these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: V3,
    pub max: V3,
}

impl Aabb {
    pub const fn new(min: V3, max: V3) -> Aabb {
        Aabb { min, max }
    }

    pub fn contains(&self, p: V3) -> bool {
        p.x > self.min.x
            && p.x < self.max.x
            && p.y > self.min.y
            && p.y < self.max.y
            && p.z > self.min.z
            && p.z < self.max.z
    }

    /// Overlaps the square footprint of half-width `r` at (x, z).
    pub fn over_xz(&self, x: f32, z: f32, r: f32) -> bool {
        self.min.x < x + r && self.max.x > x - r && self.min.z < z + r && self.max.z > z - r
    }
}

/// The slab interval of one axis, or None if the ray misses it.
fn slab(o: f32, d: f32, lo: f32, hi: f32) -> Option<(f32, f32)> {
    if d == 0.0 {
        if o < lo || o > hi {
            None
        } else {
            Some((f32::NEG_INFINITY, f32::INFINITY))
        }
    } else {
        let a = (lo - o) / d;
        let b = (hi - o) / d;
        Some((a.min(b), a.max(b)))
    }
}

/// Where a ray from `o` along unit `d` first enters `b`, within `[0, tmax]`.
/// A ray starting inside reports 0.
pub fn ray_aabb(o: V3, d: V3, b: &Aabb, tmax: f32) -> Option<f32> {
    let (x0, x1) = slab(o.x, d.x, b.min.x, b.max.x)?;
    let (y0, y1) = slab(o.y, d.y, b.min.y, b.max.y)?;
    let (z0, z1) = slab(o.z, d.z, b.min.z, b.max.z)?;
    let t0 = x0.max(y0).max(z0).max(0.0);
    let t1 = x1.min(y1).min(z1).min(tmax);
    if t0 <= t1 {
        Some(t0)
    } else {
        None
    }
}

/// A ray against a sphere: the nearest t in `[0, tmax]`.
pub fn ray_sphere(o: V3, d: V3, c: V3, r: f32, tmax: f32) -> Option<f32> {
    let m = o - c;
    let b = m.dot(d);
    let cc = m.dot(m) - r * r;
    if cc > 0.0 && b > 0.0 {
        return None;
    }
    let disc = b * b - cc;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()).max(0.0);
    if t <= tmax {
        Some(t)
    } else {
        None
    }
}

/// A ray against an upright cylinder from `base` up `h`, radius `r`.
pub fn ray_cylinder(o: V3, d: V3, base: V3, r: f32, h: f32, tmax: f32) -> Option<f32> {
    // The xz circle's interval.
    let mx = o.x - base.x;
    let mz = o.z - base.z;
    let a = d.x * d.x + d.z * d.z;
    let c = mx * mx + mz * mz - r * r;
    let (c0, c1) = if a == 0.0 {
        if c > 0.0 {
            return None;
        }
        (f32::NEG_INFINITY, f32::INFINITY)
    } else {
        let b = mx * d.x + mz * d.z;
        let disc = b * b - a * c;
        if disc < 0.0 {
            return None;
        }
        let s = disc.sqrt();
        ((-b - s) / a, (-b + s) / a)
    };
    let (y0, y1) = slab(o.y, d.y, base.y, base.y + h)?;
    let t0 = c0.max(y0).max(0.0);
    let t1 = c1.min(y1).min(tmax);
    if t0 <= t1 {
        Some(t0)
    } else {
        None
    }
}

/// A unit aim direction from yaw (1/65536 turn, 0 along +x, growing toward
/// +z) and pitch (1/65536 turn, up is positive).
pub fn aim_dir(yaw: u16, pitch: i16) -> V3 {
    let (sy, cy) = yaw_sincos(yaw);
    let (sp, cp) = crate::fmath::sin_cos(turn_to_rad(pitch as f32));
    V3::new(cp * cy, sp, cp * sy)
}

/// (sin, cos) of a yaw.
pub fn yaw_sincos(yaw: u16) -> (f32, f32) {
    crate::fmath::sin_cos(turn_to_rad(yaw as f32))
}

/// 1/65536 turn in radians.
pub fn turn_to_rad(t: f32) -> f32 {
    t * (core::f32::consts::TAU / 65536.0)
}

/// Radians as a yaw, wrapped into a turn.
pub fn rad_to_yaw(a: f32) -> u16 {
    let t = crate::fmath::round(a * (65536.0 / core::f32::consts::TAU));
    (t as i32 & 0xffff) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rays_hit_what_they_should() {
        let b = Aabb::new(V3::new(1.0, 0.0, -1.0), V3::new(2.0, 2.0, 1.0));
        let x = V3::new(1.0, 0.0, 0.0);
        assert_eq!(ray_aabb(V3::new(0.0, 1.0, 0.0), x, &b, 10.0), Some(1.0));
        assert_eq!(ray_aabb(V3::new(0.0, 3.0, 0.0), x, &b, 10.0), None);
        assert_eq!(ray_aabb(V3::new(0.0, 1.0, 0.0), x, &b, 0.5), None);
        let t = ray_sphere(V3::ZERO, x, V3::new(5.0, 0.0, 0.0), 1.0, 10.0).unwrap();
        assert!(crate::fmath::fabs(t - 4.0) < 1e-6);
        let t = ray_cylinder(V3::new(0.0, 1.0, 0.0), x, V3::new(5.0, 0.0, 0.0), 0.5, 1.5, 10.0);
        assert!(crate::fmath::fabs(t.unwrap() - 4.5) < 1e-6);
        // over the top
        assert!(ray_cylinder(V3::new(0.0, 2.0, 0.0), x, V3::new(5.0, 0.0, 0.0), 0.5, 1.5, 10.0).is_none());
        // straight down into the cap
        let down = V3::new(0.0, -1.0, 0.0);
        let t = ray_cylinder(V3::new(5.0, 3.0, 0.0), down, V3::new(5.0, 0.0, 0.0), 0.5, 1.5, 10.0);
        assert!(crate::fmath::fabs(t.unwrap() - 1.5) < 1e-6);
    }

    #[test]
    fn yaw_round_trips() {
        for y in [0u16, 1, 16384, 32768, 49152, 65535] {
            let d = aim_dir(y, 0);
            assert_eq!(rad_to_yaw(crate::fmath::atan2(d.z, d.x)), y);
        }
    }
}

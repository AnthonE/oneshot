//! The map as it is drawn, at a point: what a ray meets there (the ground, a
//! box's face, a round pillar's facet, a turned box's face, a ramp's slope or
//! side), which way that face looks, and what it is made of. The sim's
//! `Map::ray` says how far a round went; this says what it stopped on, so an
//! impact throws off the face it struck and a mark lies on that face
//! (`marks.rs`), on a pillar as on a crate. Drawing only: the sim's own
//! shapes read back, never a second opinion on where anything stopped.
//!
//! After Gates' `crates/client/src/render/skin.rs` and `surface.rs` (MIT,
//! © 2026 MoreRight DAO), which ray the drawn meshes; ours are the sim's
//! shapes exactly (`level.rs`), so the rays are the shapes' own, a round
//! one's against the facets it is drawn with (`level::round_sides`).

use bevy::prelude::*;
use sim::geom::V3;
use sim::map::Map;
use sim::solid::{Shape, Solid};

use super::table::Matter;

/// Box and solid looks drawn in metal (`level.rs::dress`).
pub const METAL_LOOKS: [u8; 4] = [1, 3, 4, 5];

/// What a ray met.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum On {
    Ground,
    /// `Map::boxes[i]`.
    Box(u32),
    /// `Map::solids[i]`.
    Solid(u32),
}

/// Where a ray met the drawn map: how far along, the point, the face's
/// outward normal (as it is shaded), and what it was.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub t: f32,
    pub at: Vec3,
    pub normal: Vec3,
    pub on: On,
}

/// What `on` is made of: a metal look is metal, any other box concrete; the
/// ground soil on a terrain and the arena's floor elsewhere.
pub fn matter(map: &Map, on: On) -> Matter {
    let look = match on {
        On::Ground => {
            return if map.terrain.is_some() {
                Matter::Dirt
            } else {
                Matter::Concrete
            };
        }
        On::Box(i) => map.looks.get(i as usize),
        On::Solid(i) => map.solid_looks.get(i as usize),
    };
    match look {
        Some(l) if METAL_LOOKS.contains(l) => Matter::Metal,
        _ => Matter::Concrete,
    }
}

/// What a hit met is made of, as drawn: [`matter`], but the ground where
/// the rock skin stands out of it (`cliffs.rs`, its `Lift`) is stone, so a
/// round into a drawn cliff chips stone, not soil.
pub fn matter_of(map: &Map, hit: &Hit, lift: Option<&crate::look::cliffs::Lift>) -> Matter {
    let rock = matches!(hit.on, On::Ground)
        && lift
            .and_then(|l| l.at(hit.at.x, hit.at.z))
            .is_some_and(|(_, l)| l > 0.0);
    if rock {
        Matter::Concrete
    } else {
        matter(map, hit.on)
    }
}

/// How far down [`floor_under`] looks, metres.
const FLOOR_PROBE_M: f32 = 30.0;

/// The height of the floor under a blow at `at` on a face facing `away`:
/// the first thing straight down from just off the face, so a wall's
/// chips land on the floor at its foot and a ceiling's on the one below.
/// `f32::NEG_INFINITY` over nothing.
pub fn floor_under(map: &Map, at: Vec3, away: Vec3) -> f32 {
    let from = at + away.normalize_or(Vec3::Y) * 0.05;
    let t = map.ray(v3(from), V3::new(0.0, -1.0, 0.0), FLOOR_PROBE_M);
    if t < FLOOR_PROBE_M {
        from.y - t
    } else {
        f32::NEG_INFINITY
    }
}

/// The round solid `on` is, and its radius, if it is one: a mark on its
/// side is bent round it (`marks.rs`).
pub fn round(map: &Map, on: On) -> Option<(Vec2, f32)> {
    let On::Solid(i) = on else {
        return None;
    };
    match map.solids.get(i as usize)?.shape {
        Shape::Round { x, z, r } => Some((Vec2::new(x, z), r)),
        _ => None,
    }
}

fn v3(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

/// Where a ray from `o` along unit `d` enters the box `lo..hi` within
/// `[0, tmax]`, and the face it enters by; from inside, at once, through
/// the face nearest the start.
fn aabb_face(o: Vec3, d: Vec3, lo: Vec3, hi: Vec3, tmax: f32) -> Option<(f32, Vec3)> {
    let (mut t0, mut t1) = (f32::NEG_INFINITY, f32::INFINITY);
    let mut n = Vec3::ZERO;
    for k in 0..3 {
        let (o, d, l, h) = (o[k], d[k], lo[k], hi[k]);
        if d == 0.0 {
            if o < l || o > h {
                return None;
            }
            continue;
        }
        let (a, b) = ((l - o) / d, (h - o) / d);
        let (near, far) = if a < b { (a, b) } else { (b, a) };
        if near > t0 {
            t0 = near;
            n = Vec3::ZERO;
            n[k] = if d > 0.0 { -1.0 } else { 1.0 };
        }
        t1 = t1.min(far);
    }
    if t0 > t1 || t1 < 0.0 || t0 > tmax {
        return None;
    }
    if t0 >= 0.0 {
        return Some((t0, n));
    }
    let faces = [
        (o.x - lo.x, Vec3::NEG_X),
        (hi.x - o.x, Vec3::X),
        (o.y - lo.y, Vec3::NEG_Y),
        (hi.y - o.y, Vec3::Y),
        (o.z - lo.z, Vec3::NEG_Z),
        (hi.z - o.z, Vec3::Z),
    ];
    let near = faces.iter().min_by(|a, b| a.0.total_cmp(&b.0)).map_or(Vec3::Y, |f| f.1);
    Some((0.0, near))
}

/// A solid's hit and the shaded normal there.
fn solid_face(s: &Solid, o: Vec3, d: Vec3, tmax: f32) -> Option<(f32, Vec3)> {
    match s.shape {
        Shape::Round { x, z, r } => {
            let t = s.ray_exact(v3(o), v3(d), tmax)?;
            let p = o + d * t;
            let (dx, dz) = (p.x - x, p.z - z);
            let rr = (dx * dx + dz * dz).sqrt();
            // A cap, when the hit is on it and not on the rim.
            let side = r - rr;
            let top = (s.y1 - p.y).abs();
            let bottom = (p.y - s.y0).abs();
            if top < side.abs().max(1e-3) && top <= bottom {
                return Some((t, Vec3::Y));
            }
            if bottom < side.abs().max(1e-3) {
                return Some((t, Vec3::NEG_Y));
            }
            // The side as drawn: the facet under the circle's hit, a
            // sagitta (under 8 mm) further in.
            let n = crate::level::round_sides(r) as f32;
            let step = core::f32::consts::TAU / n;
            let mut a = dz.atan2(dx);
            if a < 0.0 {
                a += core::f32::consts::TAU;
            }
            let mid = ((a / step).floor() + 0.5) * step;
            let nf = Vec3::new(mid.cos(), 0.0, mid.sin());
            let reach = r * (step * 0.5).cos();
            let denom = d.dot(nf);
            let rel = Vec3::new(o.x - x, 0.0, o.z - z);
            let t = if denom < -1e-4 {
                let tf = (reach - rel.dot(nf)) / denom;
                if (0.0..=tmax).contains(&tf) && (tf - t).abs() < r * 0.1 {
                    tf
                } else {
                    t
                }
            } else {
                t
            };
            let p = o + d * t;
            let radial = Vec3::new(p.x - x, 0.0, p.z - z).normalize_or(nf);
            Some((t, radial))
        }
        Shape::Turned { x, z, hx, hz, c, s: sn } => {
            let rel = o - Vec3::new(x, 0.0, z);
            let lo = Vec3::new(rel.x * c + rel.z * sn, o.y, rel.z * c - rel.x * sn);
            let ld = Vec3::new(d.x * c + d.z * sn, d.y, d.z * c - d.x * sn);
            let (t, n) = aabb_face(lo, ld, Vec3::new(-hx, s.y0, -hz), Vec3::new(hx, s.y1, hz), tmax)?;
            Some((t, Vec3::new(n.x * c - n.z * sn, n.y, n.x * sn + n.z * c)))
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
            let t = s.ray_exact(v3(o), v3(d), tmax)?;
            let p = o + d * t;
            let len = if dir < 2 { x1 - x0 } else { z1 - z0 };
            let k = (high - low) / len.max(1e-4);
            let slope = match dir {
                0 => Vec3::new(-k, 1.0, 0.0),
                1 => Vec3::new(k, 1.0, 0.0),
                2 => Vec3::new(0.0, 1.0, -k),
                _ => Vec3::new(0.0, 1.0, k),
            }
            .normalize();
            // The face the point is on: the nearest of them that faces the
            // ray, the slope measured along its own normal.
            let faces = [
                ((p.y - s.top_at(p.x, p.z)).abs() * slope.y, slope),
                ((p.x - x0).abs(), Vec3::NEG_X),
                ((p.x - x1).abs(), Vec3::X),
                ((p.z - z0).abs(), Vec3::NEG_Z),
                ((p.z - z1).abs(), Vec3::Z),
                ((p.y - s.y0).abs(), Vec3::NEG_Y),
            ];
            let facing = faces.iter().filter(|f| f.1.dot(d) < 0.0);
            let n = facing
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .or_else(|| faces.iter().min_by(|a, b| a.0.total_cmp(&b.0)))
                .map_or(Vec3::Y, |f| f.1);
            Some((t, n))
        }
    }
}

/// The ground's hit: the terrain's triangles, or the plane y = 0.
fn ground_face(map: &Map, o: Vec3, d: Vec3, tmax: f32) -> Option<(f32, Vec3)> {
    let t = match &map.terrain {
        None => {
            if d.y >= 0.0 || o.y < 0.0 {
                return None;
            }
            let t = -o.y / d.y;
            if t > tmax {
                return None;
            }
            t
        }
        Some(g) => {
            let t = g.ray(v3(o), v3(d), tmax);
            if t >= tmax {
                return None;
            }
            t
        }
    };
    let p = o + d * t;
    let n = map.ground_normal(p.x, p.z);
    Some((t, Vec3::new(n.x, n.y, n.z)))
}

/// The faces near a place, gathered once and cast against many times (a
/// mark's every vertex): indices into the map's boxes and solids, the
/// buffers kept between uses.
#[derive(Default)]
pub struct Near {
    boxes: Vec<u32>,
    solids: Vec<u32>,
}

impl Near {
    /// The boxes and solids whose bounds come within `lo..hi`, but those
    /// in `skip` (boxes drawn as something else: `Marks::bare`).
    pub fn gather(&mut self, map: &Map, lo: Vec3, hi: Vec3, skip: &[(Vec3, Vec3)]) {
        self.boxes.clear();
        self.solids.clear();
        let over = |a: Vec3, b: Vec3| a.cmple(hi).all() && b.cmpge(lo).all();
        for (i, b) in map.boxes.iter().enumerate() {
            let (a, c) = (
                Vec3::new(b.min.x, b.min.y, b.min.z),
                Vec3::new(b.max.x, b.max.y, b.max.z),
            );
            if !over(a, c) {
                continue;
            }
            if skip
                .iter()
                .any(|&(s0, s1)| s0.abs_diff_eq(a, 1e-3) && s1.abs_diff_eq(c, 1e-3))
            {
                continue;
            }
            self.boxes.push(i as u32);
        }
        for (i, s) in map.solids.iter().enumerate() {
            let b = s.bounds();
            if over(
                Vec3::new(b.min.x, b.min.y, b.min.z),
                Vec3::new(b.max.x, b.max.y, b.max.z),
            ) {
                self.solids.push(i as u32);
            }
        }
    }

    /// The first face a ray from `o` along unit `d` meets within `tmax`:
    /// the ground, or one of the gathered boxes and solids.
    pub fn cast(&self, map: &Map, o: Vec3, d: Vec3, tmax: f32) -> Option<Hit> {
        let mut best = ground_face(map, o, d, tmax).map(|(t, n)| (t, n, On::Ground));
        let mut nearer = |t: f32, n: Vec3, on: On| {
            if best.is_none_or(|b| t < b.0) {
                best = Some((t, n, on));
            }
        };
        for &i in &self.boxes {
            let b = &map.boxes[i as usize];
            let (lo, hi) = (
                Vec3::new(b.min.x, b.min.y, b.min.z),
                Vec3::new(b.max.x, b.max.y, b.max.z),
            );
            if let Some((t, n)) = aabb_face(o, d, lo, hi, tmax) {
                nearer(t, n, On::Box(i));
            }
        }
        for &i in &self.solids {
            if let Some((t, n)) = solid_face(&map.solids[i as usize], o, d, tmax) {
                nearer(t, n, On::Solid(i));
            }
        }
        best.map(|(t, normal, on)| Hit {
            t,
            at: o + d * t,
            normal,
            on,
        })
    }
}

/// [`Near::cast`] over whatever the ray could reach, gathered into `near`.
pub fn cast(map: &Map, near: &mut Near, o: Vec3, d: Vec3, tmax: f32) -> Option<Hit> {
    let e = o + d * tmax;
    let pad = Vec3::splat(0.01);
    near.gather(map, o.min(e) - pad, o.max(e) + pad, &[]);
    near.cast(map, o, d, tmax)
}

/// The face a round that stopped at `at`, flying along `dir`, stopped on:
/// a short ray up to it and a little past.
pub fn struck(map: &Map, near: &mut Near, at: Vec3, dir: Vec3) -> Option<Hit> {
    cast(map, near, at - dir * 0.3, dir, 0.45)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::geom::Aabb;

    fn map_with(solids: Vec<Solid>, boxes: Vec<Aabb>) -> Map {
        let mut m = Map::open();
        m.boxes = boxes;
        m.looks = vec![0; m.boxes.len()];
        m.solid_looks = vec![1; solids.len()];
        m.solids = solids;
        m.terrain = None;
        m.indexed()
    }

    /// A box, a pillar, a turned wall and a ramp each answer with the face
    /// the ray met, turned the way it is drawn.
    #[test]
    fn each_shape_answers_with_its_face() {
        let h = core::f32::consts::FRAC_1_SQRT_2;
        let map = map_with(
            vec![
                Solid::from_def(0, [10.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 3.0]),
                Solid::from_def(1, [0.0, 10.0, 2.0, 0.5, h, h, 0.0, 3.0]),
                Solid::from_def(2, [-10.0, -1.0, -6.0, 1.0, 0.0, 0.0, 0.0, 2.0]),
            ],
            vec![Aabb::new(V3::new(-1.0, 0.0, -11.0), V3::new(1.0, 2.0, -9.0))],
        );
        let mut near = Near::default();
        // The box, from +z: its +z face.
        let hit = cast(&map, &mut near, Vec3::new(0.0, 1.0, -5.0), Vec3::NEG_Z, 10.0).unwrap();
        assert_eq!(hit.on, On::Box(0));
        assert!(hit.normal.abs_diff_eq(Vec3::Z, 1e-5), "{:?}", hit.normal);
        assert!((hit.t - 4.0).abs() < 1e-4);
        // The pillar, from −x: radial, on the facet a hair inside the circle.
        let hit = cast(&map, &mut near, Vec3::new(5.0, 1.0, 0.3), Vec3::X, 10.0).unwrap();
        assert_eq!(hit.on, On::Solid(0));
        assert!(hit.normal.x < -0.9 && hit.normal.y == 0.0);
        let r = Vec2::new(hit.at.x - 10.0, hit.at.z).length();
        assert!(r <= 1.0 + 1e-4 && r > 0.99, "on its facets: {r}");
        assert_eq!(matter(&map, hit.on), Matter::Metal);
        assert_eq!(round(&map, hit.on).map(|r| r.1), Some(1.0));
        // Its top, from above.
        let hit = cast(&map, &mut near, Vec3::new(10.2, 6.0, 0.1), Vec3::NEG_Y, 10.0).unwrap();
        assert!(hit.normal.abs_diff_eq(Vec3::Y, 1e-5));
        // The wall turned 45°, met head on along its own normal.
        let n = Vec3::new(-h, 0.0, h);
        let hit = cast(&map, &mut near, Vec3::new(0.0, 1.0, 10.0) + n * 4.0, -n, 10.0).unwrap();
        assert_eq!(hit.on, On::Solid(1));
        assert!(hit.normal.abs_diff_eq(n, 1e-4), "{:?}", hit.normal);
        assert!((hit.t - 3.5).abs() < 1e-3, "{}", hit.t);
        // The ramp, up +x from 0 to 2 m over 4 m: its slope from above.
        let hit = cast(&map, &mut near, Vec3::new(-8.0, 5.0, 0.0), Vec3::NEG_Y, 10.0).unwrap();
        assert_eq!(hit.on, On::Solid(2));
        let want = Vec3::new(-0.5, 1.0, 0.0).normalize();
        assert!(hit.normal.abs_diff_eq(want, 1e-4), "{:?}", hit.normal);
        assert!((hit.at.y - 1.0).abs() < 1e-3);
        // Its high end from +x: the end face.
        let hit = cast(&map, &mut near, Vec3::new(-3.0, 1.0, 0.0), Vec3::NEG_X, 10.0).unwrap();
        assert!(hit.normal.abs_diff_eq(Vec3::X, 1e-5), "{:?}", hit.normal);
        // And the floor, anywhere else.
        let hit = cast(&map, &mut near, Vec3::new(30.0, 2.0, 30.0), Vec3::NEG_Y, 10.0).unwrap();
        assert_eq!(hit.on, On::Ground);
        assert_eq!(matter(&map, hit.on), Matter::Concrete);
    }

    /// From inside a box a ray meets it at once, through its nearest face.
    #[test]
    fn inside_a_box_is_its_nearest_face() {
        let (t, n) = aabb_face(Vec3::new(0.9, 0.5, 0.0), Vec3::NEG_Y, Vec3::splat(-1.0), Vec3::ONE, 5.0).unwrap();
        assert_eq!(t, 0.0);
        assert_eq!(n, Vec3::X);
        assert!(aabb_face(Vec3::new(3.0, 0.0, 0.0), Vec3::X, Vec3::splat(-1.0), Vec3::ONE, 5.0).is_none());
    }
}

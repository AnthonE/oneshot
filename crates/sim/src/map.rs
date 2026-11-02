//! A map: a floor at y = 0, boxes, spawn points and weapon pads. Built once
//! at load; the tick only reads it.

use crate::arms::{ROCKET, SHOTGUN, SMG, SNIPER};
use crate::geom::{ray_aabb, Aabb, V3};

/// Any team may spawn here.
pub const ANY_TEAM: u8 = 255;

#[derive(Clone, Copy, Debug)]
pub struct Spawn {
    pub pos: V3,
    pub yaw: u16,
    /// 0 red, 1 blue, or [`ANY_TEAM`].
    pub team: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct PadDef {
    pub pos: V3,
    pub arm: u8,
    pub respawn_ms: u32,
}

#[derive(Clone, Debug)]
pub struct Map {
    /// Walkable area in x and z: (min x, min z) to (max x, max z).
    pub lo: (f32, f32),
    pub hi: (f32, f32),
    pub boxes: Vec<Aabb>,
    pub spawns: Vec<Spawn>,
    pub pads: Vec<PadDef>,
}

impl Map {
    /// Distance along unit `d` from `o` to the first solid (box or floor),
    /// or `tmax` if nothing is nearer.
    pub fn ray(&self, o: V3, d: V3, tmax: f32) -> f32 {
        let mut t = tmax;
        if d.y < 0.0 && o.y >= 0.0 {
            t = t.min(-o.y / d.y);
        }
        for b in &self.boxes {
            if let Some(h) = ray_aabb(o, d, b, t) {
                t = h;
            }
        }
        t
    }

    /// Whether the straight line from `a` to `b` is clear.
    pub fn clear(&self, a: V3, b: V3) -> bool {
        let v = b - a;
        let l = v.len();
        l == 0.0 || self.ray(a, v * (1.0 / l), l) >= l
    }

    /// The highest floor under a footprint of half-width `r` at (x, z) whose
    /// top is at or below `reach`.
    pub fn floor(&self, x: f32, z: f32, r: f32, reach: f32) -> f32 {
        let mut f = 0.0f32;
        for b in &self.boxes {
            if b.max.y <= reach && b.max.y > f && b.over_xz(x, z, r) {
                f = b.max.y;
            }
        }
        f
    }

    /// The lowest box bottom over the footprint at or above `from`.
    pub fn ceiling(&self, x: f32, z: f32, r: f32, from: f32) -> f32 {
        let mut c = f32::INFINITY;
        for b in &self.boxes {
            if b.min.y >= from && b.min.y < c && b.over_xz(x, z, r) {
                c = b.min.y;
            }
        }
        c
    }

    /// An open floor 100 m square with walls, a spawn at each end and the
    /// shotgun on a pad in the middle: for tests that place fighters by hand.
    pub fn open() -> Map {
        let b = |x0: f32, z0: f32, x1: f32, z1: f32| Aabb::new(V3::new(x0, 0.0, z0), V3::new(x1, 8.0, z1));
        Map {
            lo: (-50.0, -50.0),
            hi: (50.0, 50.0),
            boxes: vec![
                b(-51.0, -51.0, 51.0, -50.0),
                b(-51.0, 50.0, 51.0, 51.0),
                b(-51.0, -50.0, -50.0, 50.0),
                b(50.0, -50.0, 51.0, 50.0),
            ],
            spawns: vec![
                Spawn {
                    pos: V3::new(-40.0, 0.0, 0.0),
                    yaw: 0,
                    team: 0,
                },
                Spawn {
                    pos: V3::new(40.0, 0.0, 0.0),
                    yaw: 32768,
                    team: 1,
                },
            ],
            pads: vec![PadDef {
                pos: V3::new(0.0, 0.0, -30.0),
                arm: SHOTGUN,
                respawn_ms: 10_000,
            }],
        }
    }

    /// Proving Grounds: the test arena, 48 m square, mirrored between red
    /// (−x) and blue (+x). A raised centre with the rocket, a platform on
    /// each side reached by steps, pillars and waist-high walls for cover,
    /// the sniper and the shotgun at the ends of the middle lane.
    pub fn proving_grounds() -> Map {
        let mut boxes = Vec::new();
        let b =
            |x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32| Aabb::new(V3::new(x0, y0, z0), V3::new(x1, y1, z1));
        // Walls.
        boxes.push(b(-25.0, 0.0, -25.0, 25.0, 8.0, -24.0));
        boxes.push(b(-25.0, 0.0, 24.0, 25.0, 8.0, 25.0));
        boxes.push(b(-25.0, 0.0, -24.0, -24.0, 8.0, 24.0));
        boxes.push(b(24.0, 0.0, -24.0, 25.0, 8.0, 24.0));
        // The centre: two tiers, 0.5 m each.
        boxes.push(b(-3.0, 0.0, -3.0, 3.0, 0.5, 3.0));
        boxes.push(b(-2.0, 0.5, -2.0, 2.0, 1.0, 2.0));
        for s in [-1.0f32, 1.0] {
            // A platform 2.5 m up, steps on its inner face.
            let (x0, x1) = if s < 0.0 { (-20.0, -12.0) } else { (12.0, 20.0) };
            boxes.push(b(x0, 0.0, -4.0, x1, 2.5, 4.0));
            for k in 0..4 {
                let h = 2.0 - 0.5 * k as f32;
                let a = 12.0 - k as f32;
                let (sx0, sx1) = if s < 0.0 { (-a, -a + 1.0) } else { (a - 1.0, a) };
                boxes.push(b(sx0, 0.0, -1.5, sx1, h, 1.5));
            }
            // Pillars.
            for t in [-1.0f32, 1.0] {
                let (px, pz) = (8.0 * s, 12.0 * t);
                boxes.push(b(px - 0.75, 0.0, pz - 0.75, px + 0.75, 3.0, pz + 0.75));
            }
            // Waist-high cover across each end lane.
            boxes.push(b(-6.0, 0.0, 16.0 * s - 0.25, 6.0, 1.2, 16.0 * s + 0.25));
        }
        let mut spawns = Vec::new();
        for (team, s) in [(0u8, -1.0f32), (1, 1.0)] {
            // Facing the middle: red looks +x (yaw 0), blue −x.
            let yaw = if s < 0.0 { 0 } else { 32768 };
            for (x, z) in [(22.0, 20.0), (22.0, -20.0), (22.0, 8.0), (22.0, -8.0)] {
                spawns.push(Spawn {
                    pos: V3::new(x * s, 0.0, z),
                    yaw,
                    team,
                });
            }
        }
        let pads = vec![
            PadDef {
                pos: V3::new(0.0, 1.0, 0.0),
                arm: ROCKET,
                respawn_ms: 120_000,
            },
            PadDef {
                pos: V3::new(0.0, 0.0, 20.0),
                arm: SNIPER,
                respawn_ms: 90_000,
            },
            PadDef {
                pos: V3::new(0.0, 0.0, -20.0),
                arm: SHOTGUN,
                respawn_ms: 60_000,
            },
            PadDef {
                pos: V3::new(-16.0, 2.5, 0.0),
                arm: SMG,
                respawn_ms: 30_000,
            },
            PadDef {
                pos: V3::new(16.0, 2.5, 0.0),
                arm: SMG,
                respawn_ms: 30_000,
            },
        ];
        Map {
            lo: (-24.0, -24.0),
            hi: (24.0, 24.0),
            boxes,
            spawns,
            pads,
        }
    }
}

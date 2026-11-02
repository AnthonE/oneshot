//! A map: a floor at y = 0, boxes, spawn points and weapon pads. Built once
//! at load; the tick only reads it. The maps themselves are data
//! (`maps/*/map.json`), compiled to the tables in [`crate::maps`].

use crate::arms::SHOTGUN;
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

/// A map as `tools/maps/compile.mjs` writes it: tables a [`Map`] is built
/// from. Boxes are (min x y z, max x y z); spawns (x y z, yaw, team); pads
/// (x y z, arm, respawn ms); roam points (x, z).
pub struct Def {
    pub id: u8,
    pub name: &'static str,
    pub lo: (f32, f32),
    pub hi: (f32, f32),
    pub boxes: &'static [[f32; 6]],
    pub looks: &'static [u8],
    pub spawns: &'static [([f32; 3], u16, u8)],
    pub pads: &'static [([f32; 3], u8, u32)],
    pub roam: &'static [[f32; 2]],
}

impl Def {
    pub fn build(&self) -> Map {
        let v = |p: [f32; 3]| V3::new(p[0], p[1], p[2]);
        Map {
            id: self.id,
            name: self.name,
            lo: self.lo,
            hi: self.hi,
            boxes: self
                .boxes
                .iter()
                .map(|b| Aabb::new(V3::new(b[0], b[1], b[2]), V3::new(b[3], b[4], b[5])))
                .collect(),
            looks: self.looks.to_vec(),
            spawns: self
                .spawns
                .iter()
                .map(|&(p, yaw, team)| Spawn { pos: v(p), yaw, team })
                .collect(),
            pads: self
                .pads
                .iter()
                .map(|&(p, arm, respawn_ms)| PadDef {
                    pos: v(p),
                    arm,
                    respawn_ms,
                })
                .collect(),
            roam: self.roam.iter().map(|r| V3::new(r[0], 0.0, r[1])).collect(),
        }
    }
}

/// The map a room, a replay or the wire names by `id`.
pub fn by_id(id: u8) -> Option<Map> {
    crate::maps::ALL.iter().find(|d| d.id == id).map(|d| d.build())
}

/// Not a map anyone plays: [`Map::open`], for tests.
pub const TEST_MAP: u8 = 255;

#[derive(Clone, Debug)]
pub struct Map {
    /// What the wire and replays call it.
    pub id: u8,
    pub name: &'static str,
    /// Walkable area in x and z: (min x, min z) to (max x, max z).
    pub lo: (f32, f32),
    pub hi: (f32, f32),
    pub boxes: Vec<Aabb>,
    /// How each box is drawn (web/src/world.ts): drawing only.
    pub looks: Vec<u8>,
    pub spawns: Vec<Spawn>,
    pub pads: Vec<PadDef>,
    /// Where a bot with nobody in sight walks; none: the middle, then the
    /// first pad.
    pub roam: Vec<V3>,
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
            id: TEST_MAP,
            name: "open",
            looks: vec![0; 4],
            roam: Vec::new(),
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

    /// Proving Grounds (`maps/proving_grounds/map.json`): the test arena,
    /// and the one the determinism golden plays.
    pub fn proving_grounds() -> Map {
        crate::maps::proving_grounds::DEF.build()
    }
}

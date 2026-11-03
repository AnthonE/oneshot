//! A map: a floor at y = 0, boxes, spawn points and weapon pads. Built once
//! at load; the tick only reads it. The maps themselves are data
//! (`maps/*/map.json`), compiled to the tables in [`crate::maps`].

use crate::arms::SHOTGUN;
use crate::geom::{ray_aabb, Aabb, V3};
use crate::grid::Grid;

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
            grid: Grid::default(),
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
        .indexed()
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
    /// The boxes by cell, for the queries below.
    grid: Grid,
}

impl Map {
    /// Builds the box grid; every constructor ends here.
    fn indexed(mut self) -> Map {
        self.grid = Grid::build(&self.boxes);
        self
    }

    /// Calls `f` for each box that may overlap the rectangle `lo..hi` in x
    /// and z, each once, in index order (and some that don't: `f` checks).
    pub fn each_box_in(&self, lo: (f32, f32), hi: (f32, f32), mut f: impl FnMut(&Aabb)) {
        self.grid.each_in(lo, hi, |i| f(&self.boxes[i]));
    }

    /// Distance along unit `d` from `o` to the first solid (box or floor),
    /// or `tmax` if nothing is nearer.
    pub fn ray(&self, o: V3, d: V3, tmax: f32) -> f32 {
        let mut t = tmax;
        if d.y < 0.0 && o.y >= 0.0 {
            t = t.min(-o.y / d.y);
        }
        let reach = t;
        self.grid.each_on_ray(o, d, reach, |i| {
            if let Some(h) = ray_aabb(o, d, &self.boxes[i], t) {
                t = h;
            }
            t
        });
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
        self.each_box_in((x - r, z - r), (x + r, z + r), |b| {
            if b.max.y <= reach && b.max.y > f && b.over_xz(x, z, r) {
                f = b.max.y;
            }
        });
        f
    }

    /// The lowest box bottom over the footprint at or above `from`.
    pub fn ceiling(&self, x: f32, z: f32, r: f32, from: f32) -> f32 {
        let mut c = f32::INFINITY;
        self.each_box_in((x - r, z - r), (x + r, z + r), |b| {
            if b.min.y >= from && b.min.y < c && b.over_xz(x, z, r) {
                c = b.min.y;
            }
        });
        c
    }

    /// An open floor 100 m square with walls, a spawn at each end and the
    /// shotgun on a pad in the middle: for tests that place fighters by hand.
    pub fn open() -> Map {
        let b = |x0: f32, z0: f32, x1: f32, z1: f32| Aabb::new(V3::new(x0, 0.0, z0), V3::new(x1, 8.0, z1));
        Map {
            grid: Grid::default(),
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
        .indexed()
    }

    /// Proving Grounds (`maps/proving_grounds/map.json`): the test arena,
    /// and the one the determinism golden plays.
    pub fn proving_grounds() -> Map {
        crate::maps::proving_grounds::DEF.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Pcg32;

    /// The scans the grid replaced: every box, in order.
    fn ray_linear(m: &Map, o: V3, d: V3, tmax: f32) -> f32 {
        let mut t = tmax;
        if d.y < 0.0 && o.y >= 0.0 {
            t = t.min(-o.y / d.y);
        }
        for b in &m.boxes {
            if let Some(h) = ray_aabb(o, d, b, t) {
                t = h;
            }
        }
        t
    }

    fn floor_linear(m: &Map, x: f32, z: f32, r: f32, reach: f32) -> f32 {
        let mut f = 0.0f32;
        for b in &m.boxes {
            if b.max.y <= reach && b.max.y > f && b.over_xz(x, z, r) {
                f = b.max.y;
            }
        }
        f
    }

    fn ceiling_linear(m: &Map, x: f32, z: f32, r: f32, from: f32) -> f32 {
        let mut c = f32::INFINITY;
        for b in &m.boxes {
            if b.min.y >= from && b.min.y < c && b.over_xz(x, z, r) {
                c = b.min.y;
            }
        }
        c
    }

    fn unit(r: &mut Pcg32, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (r.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    #[test]
    fn the_grid_answers_as_the_linear_scan_does_bit_for_bit() {
        let mut maps: Vec<Map> = crate::maps::ALL.iter().map(|d| d.build()).collect();
        maps.push(Map::open());
        let mut r = Pcg32::new(11, 3);
        for m in &maps {
            let (lx, lz) = (m.lo.0 - 6.0, m.lo.1 - 6.0);
            let (hx, hz) = (m.hi.0 + 6.0, m.hi.1 + 6.0);
            for k in 0..40_000 {
                let o = V3::new(unit(&mut r, lx, hx), unit(&mut r, -1.0, 9.0), unit(&mut r, lz, hz));
                // Some rays flat, some straight down or up, most anywhere.
                let d = match k % 10 {
                    0 => V3::new(unit(&mut r, -1.0, 1.0), 0.0, unit(&mut r, -1.0, 1.0)),
                    1 => V3::new(0.0, if k % 20 == 1 { -1.0 } else { 1.0 }, 0.0),
                    2 => V3::new(1.0, 0.0, 0.0),
                    _ => V3::new(
                        unit(&mut r, -1.0, 1.0),
                        unit(&mut r, -1.0, 1.0),
                        unit(&mut r, -1.0, 1.0),
                    ),
                };
                if d.len() == 0.0 {
                    continue;
                }
                let d = d.norm();
                let tmax = unit(&mut r, 0.0, 120.0);
                let (g, l) = (m.ray(o, d, tmax), ray_linear(m, o, d, tmax));
                assert_eq!(
                    g.to_bits(),
                    l.to_bits(),
                    "{}: ray {o:?} {d:?} {tmax}: {g} vs {l}",
                    m.name
                );
                let (x, z) = (o.x, o.z);
                let rad = [0.0, 0.35, 1.0][k % 3];
                let y = o.y;
                assert_eq!(m.floor(x, z, rad, y).to_bits(), floor_linear(m, x, z, rad, y).to_bits());
                assert_eq!(
                    m.ceiling(x, z, rad, y).to_bits(),
                    ceiling_linear(m, x, z, rad, y).to_bits()
                );
            }
        }
    }

    #[test]
    fn rays_along_cell_edges_and_through_corners_meet_every_box() {
        // The edges of 8 m cells, exactly: the walk must not slip past a box
        // whose face lies on one.
        let maps: Vec<Map> = crate::maps::ALL.iter().map(|d| d.build()).collect();
        for m in &maps {
            for e in -6..=6 {
                let c = e as f32 * crate::grid::CELL;
                for (o, d) in [
                    (V3::new(c, 1.0, m.lo.1 - 4.0), V3::new(0.0, 0.0, 1.0)),
                    (V3::new(m.lo.0 - 4.0, 1.0, c), V3::new(1.0, 0.0, 0.0)),
                    (V3::new(c, 1.0, c), V3::new(1.0, 0.0, 1.0).norm()),
                    (V3::new(c, 1.0, -c), V3::new(-1.0, 0.0, 1.0).norm()),
                ] {
                    assert_eq!(m.ray(o, d, 200.0).to_bits(), ray_linear(m, o, d, 200.0).to_bits());
                }
            }
        }
    }
}

//! A map: a floor at y = 0 (or a terrain, [`crate::terrain`]), boxes,
//! spawn points, weapon pads and launchers. Built once at load; the tick
//! only reads it. The maps themselves are data
//! (`maps/*/map.json`), compiled to the tables in [`crate::maps`].

use crate::arms::SHOTGUN;
use crate::geom::{ray_aabb, Aabb, V3};
use crate::grid::Grid;
use crate::solid::Solid;
use crate::terrain::{Terrain, TerrainDef};

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

/// Where a vehicle starts, and comes back after it is destroyed.
#[derive(Clone, Copy, Debug)]
pub struct VehicleSpawn {
    pub pos: V3,
    pub yaw: u16,
    pub respawn_ms: u32,
}

/// A launcher: a grounded fighter whose feet are in `area` is thrown at
/// `vel` (movement::step), on the server and in the prediction alike.
#[derive(Clone, Copy, Debug)]
pub struct Launcher {
    pub area: Aabb,
    pub vel: V3,
}

/// A map as `tools/maps/compile.mjs` writes it: tables a [`Map`] is built
/// from. Boxes are (min x y z, max x y z); spawns (x y z, yaw, team); pads
/// (x y z, arm, respawn ms); roam points (x, z); launchers (min x y z, max
/// x y z of the pad, velocity). On a terrain, a spawn, a pad or a roam
/// point at height 0 stands on the ground, and one given a height never
/// under it: the ground's height is found here, so the compiler never
/// computes a height the sim stands on.
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
    pub terrain: Option<TerrainDef>,
    pub launchers: &'static [([f32; 6], [f32; 3])],
    /// (x y z, yaw, respawn ms) each.
    pub vehicles: &'static [([f32; 3], u16, u32)],
    /// The solids that aren't boxes ([`Solid::from_def`]: kind, numbers),
    /// and how each is drawn.
    pub solids: &'static [(u8, [f32; 8])],
    pub solid_looks: &'static [u8],
}

/// A map this big (in m²) is for Big Team; smaller ones are arenas.
pub const BIG_M2: f32 = 4000.0;

impl Def {
    /// Whether this is a Big Team map ([`BIG_M2`] or more inside its
    /// bounds), not an arena.
    pub fn big(&self) -> bool {
        (self.hi.0 - self.lo.0) * (self.hi.1 - self.lo.1) >= BIG_M2
    }

    pub fn build(&self) -> Map {
        let terrain = self.terrain.as_ref().map(Terrain::build);
        let ground = |x: f32, z: f32| terrain.as_ref().and_then(|t| t.height(x, z)).unwrap_or(0.0);
        let v = |p: [f32; 3]| {
            let g = ground(p[0], p[2]);
            V3::new(p[0], if p[1] == 0.0 { g } else { p[1].max(g) }, p[2])
        };
        Map {
            grid: Grid::default(),
            launchers: self
                .launchers
                .iter()
                .map(|&(b, vel)| Launcher {
                    area: Aabb::new(V3::new(b[0], b[1], b[2]), V3::new(b[3], b[4], b[5])),
                    vel: V3::new(vel[0], vel[1], vel[2]),
                })
                .collect(),
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
            solids: self.solids.iter().map(|&(k, n)| Solid::from_def(k, n)).collect(),
            solid_looks: self.solid_looks.to_vec(),
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
            roam: self
                .roam
                .iter()
                .map(|r| V3::new(r[0], ground(r[0], r[1]), r[1]))
                .collect(),
            vehicles: self
                .vehicles
                .iter()
                .map(|&(p, yaw, respawn_ms)| VehicleSpawn {
                    pos: v(p),
                    yaw,
                    respawn_ms,
                })
                .collect(),
            terrain,
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
    /// How each box is drawn (the client's `level.rs`): drawing only.
    pub looks: Vec<u8>,
    /// Round pillars, turned boxes and ramps ([`crate::solid`]), and their
    /// looks. In the grid after the boxes.
    pub solids: Vec<Solid>,
    pub solid_looks: Vec<u8>,
    pub spawns: Vec<Spawn>,
    pub pads: Vec<PadDef>,
    /// Where a bot with nobody in sight walks; none: the middle, then the
    /// first pad.
    pub roam: Vec<V3>,
    /// The boxes by cell, for the queries below.
    grid: Grid,
    /// The ground, if it isn't the plane y = 0.
    pub terrain: Option<Terrain>,
    pub launchers: Vec<Launcher>,
    pub vehicles: Vec<VehicleSpawn>,
}

impl Map {
    /// Builds the box grid; every constructor ends here.
    fn indexed(mut self) -> Map {
        let mut all = self.boxes.clone();
        all.extend(self.solids.iter().map(Solid::bounds));
        self.grid = Grid::build(&all);
        self
    }

    /// Calls `f` for each box that may overlap the rectangle `lo..hi` in x
    /// and z, each once, in index order (and some that don't: `f` checks).
    pub fn each_box_in(&self, lo: (f32, f32), hi: (f32, f32), mut f: impl FnMut(&Aabb)) {
        let n = self.boxes.len();
        self.grid.each_in(lo, hi, |i| {
            if i < n {
                f(&self.boxes[i])
            }
        });
    }

    /// The same for the solids.
    pub fn each_solid_in(&self, lo: (f32, f32), hi: (f32, f32), mut f: impl FnMut(&Solid)) {
        if self.solids.is_empty() {
            return;
        }
        let n = self.boxes.len();
        self.grid.each_in(lo, hi, |i| {
            if i >= n {
                f(&self.solids[i - n])
            }
        });
    }

    /// Distance along unit `d` from `o` to the first solid (box or floor),
    /// or `tmax` if nothing is nearer.
    pub fn ray(&self, o: V3, d: V3, tmax: f32) -> f32 {
        let mut t = tmax;
        match &self.terrain {
            None => {
                if d.y < 0.0 && o.y >= 0.0 {
                    t = t.min(-o.y / d.y);
                }
            }
            Some(g) => t = g.ray(o, d, t),
        }
        let reach = t;
        let n = self.boxes.len();
        self.grid.each_on_ray(o, d, reach, |i| {
            let hit = if i < n {
                ray_aabb(o, d, &self.boxes[i], t)
            } else {
                self.solids[i - n].ray(o, d, t)
            };
            if let Some(h) = hit {
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

    /// The ground's height at (x, z): the terrain's, or 0.
    pub fn ground(&self, x: f32, z: f32) -> f32 {
        match &self.terrain {
            None => 0.0,
            Some(g) => g.height(x, z).unwrap_or(0.0),
        }
    }

    /// The ground's unit normal at (x, z): straight up but on a terrain.
    pub fn ground_normal(&self, x: f32, z: f32) -> V3 {
        match &self.terrain {
            None => V3::new(0.0, 1.0, 0.0),
            Some(g) => g.normal(x, z),
        }
    }

    /// The highest floor under a footprint of half-width `r` at (x, z) whose
    /// top is at or below `reach`. The ground counts wherever it is: under
    /// the footprint's middle, at its own height (a slope too steep to
    /// stand on is movement's to refuse, not the floor's).
    pub fn floor(&self, x: f32, z: f32, r: f32, reach: f32) -> f32 {
        self.floor_with(x, z, r, reach, reach)
    }

    /// [`Map::floor`], with a ramp's slope counting up to `ramp_reach`
    /// (movement lets a body a step under a slope onto it, rising or
    /// falling: the slope moves under it between ticks as a flat top
    /// can't). A solid's footprint is met as a circle.
    pub fn floor_with(&self, x: f32, z: f32, r: f32, reach: f32, ramp_reach: f32) -> f32 {
        let mut f = self.ground(x, z);
        self.each_box_in((x - r, z - r), (x + r, z + r), |b| {
            if b.max.y <= reach && b.max.y > f && b.over_xz(x, z, r) {
                f = b.max.y;
            }
        });
        self.each_solid_in((x - r, z - r), (x + r, z + r), |s| {
            let (top, lim) = match s.shape {
                crate::solid::Shape::Ramp { .. } => (s.top_at(x, z), ramp_reach),
                _ => (s.y1, reach),
            };
            if top <= lim && top > f && s.over(x, z, r) {
                f = top;
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
        self.each_solid_in((x - r, z - r), (x + r, z + r), |s| {
            if s.y0 >= from && s.y0 < c && s.over(x, z, r) {
                c = s.y0;
            }
        });
        c
    }

    /// Whether a body of radius `r` with its feet at `feet` and its head at
    /// `head` at (x, z) is inside something: a box or a solid past its top
    /// (a ramp's slope where it stands). For tests and tools; movement
    /// keeps bodies out on its own.
    pub fn occupied(&self, x: f32, z: f32, r: f32, feet: f32, head: f32) -> bool {
        let mut hit = false;
        self.each_box_in((x - r, z - r), (x + r, z + r), |b| {
            hit |= b.over_xz(x, z, r) && b.min.y < head && b.max.y > feet + 1e-3;
        });
        self.each_solid_in((x - r, z - r), (x + r, z + r), |s| {
            hit |= s.over(x, z, r) && s.y0 < head && s.top_at(x, z) > feet + 1e-3;
        });
        hit
    }

    /// Pushes a body of radius `r` at (x, z) back out of every solid in its
    /// band, its speed into each face taken off (it slides along). `lo` and
    /// `hi`: the band a box blocks in (movement's); a ramp blocks only where
    /// its slope stands more than `climb` over the feet. Three rounds, for
    /// a body in a corner where two solids meet. A push that would put the
    /// body in a box is undone: it stays where it started the tick, `back`.
    #[allow(clippy::too_many_arguments)]
    pub fn push_out(
        &self,
        x: &mut f32,
        z: &mut f32,
        v: &mut V3,
        r: f32,
        lo: f32,
        hi: f32,
        climb: f32,
        back: (f32, f32),
    ) {
        if self.solids.is_empty() {
            return;
        }
        let mut pushed = false;
        for _ in 0..3 {
            let mut moved = false;
            let (qx, qz) = (*x, *z);
            self.each_solid_in((qx - r, qz - r), (qx + r, qz + r), |s| {
                let blocks = match s.shape {
                    crate::solid::Shape::Ramp { .. } => s.y0 < hi && s.top_at(*x, *z) > climb,
                    _ => s.y0 < hi && s.y1 > lo,
                };
                if !blocks {
                    return;
                }
                if let Some((nx, nz, ux, uz)) = s.push(*x, *z, r) {
                    *x = nx;
                    *z = nz;
                    let into = v.x * ux + v.z * uz;
                    if into < 0.0 {
                        v.x -= ux * into;
                        v.z -= uz * into;
                    }
                    moved = true;
                }
            });
            pushed |= moved;
            if !moved {
                break;
            }
        }
        if !pushed {
            return;
        }
        let mut in_box = false;
        self.each_box_in((*x - r, *z - r), (*x + r, *z + r), |b| {
            in_box |= b.over_xz(*x, *z, r) && b.min.y < hi && b.max.y > lo;
        });
        if in_box {
            *x = back.0;
            *z = back.1;
            v.x = 0.0;
            v.z = 0.0;
        }
    }

    /// An open floor 100 m square with walls, a spawn at each end and the
    /// shotgun on a pad in the middle: for tests that place fighters by hand.
    pub fn open() -> Map {
        let b = |x0: f32, z0: f32, x1: f32, z1: f32| Aabb::new(V3::new(x0, 0.0, z0), V3::new(x1, 8.0, z1));
        Map {
            grid: Grid::default(),
            terrain: None,
            launchers: Vec::new(),
            vehicles: Vec::new(),
            id: TEST_MAP,
            name: "open",
            looks: vec![0; 4],
            solids: Vec::new(),
            solid_looks: Vec::new(),
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

    /// The scans the grid replaced: every box, in order (over the same
    /// ground: the grid is the boxes').
    fn ray_linear(m: &Map, o: V3, d: V3, tmax: f32) -> f32 {
        let mut t = tmax;
        match &m.terrain {
            None => {
                if d.y < 0.0 && o.y >= 0.0 {
                    t = t.min(-o.y / d.y);
                }
            }
            Some(g) => t = g.ray(o, d, t),
        }
        for b in &m.boxes {
            if let Some(h) = ray_aabb(o, d, b, t) {
                t = h;
            }
        }
        for s in &m.solids {
            if let Some(h) = s.ray(o, d, t) {
                t = h;
            }
        }
        t
    }

    fn floor_linear(m: &Map, x: f32, z: f32, r: f32, reach: f32) -> f32 {
        let mut f = m.ground(x, z);
        for b in &m.boxes {
            if b.max.y <= reach && b.max.y > f && b.over_xz(x, z, r) {
                f = b.max.y;
            }
        }
        for s in &m.solids {
            let top = s.top_at(x, z);
            if top <= reach && top > f && s.over(x, z, r) {
                f = top;
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
        for s in &m.solids {
            if s.y0 >= from && s.y0 < c && s.over(x, z, r) {
                c = s.y0;
            }
        }
        c
    }

    /// Solids of every kind across cell edges, some overlapping boxes.
    fn with_solids() -> Map {
        Def {
            id: 252,
            name: "with solids",
            lo: (-20.0, -20.0),
            hi: (20.0, 20.0),
            boxes: &[[-3.0, 0.0, -3.0, 3.0, 1.0, 3.0], [9.0, 0.0, -1.0, 12.0, 4.0, 1.0]],
            looks: &[0, 0],
            spawns: &[([0.0, 0.0, -10.0], 0, 0)],
            pads: &[],
            roam: &[],
            terrain: None,
            launchers: &[],
            vehicles: &[],
            solids: &[
                (0, [8.0, 8.0, 1.5, 0.0, 0.0, 0.0, 0.0, 3.0]),
                (0, [-8.0, 0.0, 0.6, 0.0, 0.0, 0.0, 2.0, 6.0]),
                (1, [0.0, 8.0, 5.0, 0.3, 0.8, 0.6, 0.0, 4.0]),
                (1, [-12.0, -12.0, 3.0, 0.2, 0.0, 1.0, 1.0, 2.5]),
                (2, [-16.0, -4.0, -9.0, 4.0, 0.0, 0.0, 0.0, 3.0]),
                (2, [4.0, -18.0, 8.0, -9.0, 3.0, 0.0, 1.0, 2.5]),
            ],
            solid_looks: &[0; 6],
        }
        .build()
    }

    fn unit(r: &mut Pcg32, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (r.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    #[test]
    fn the_grid_answers_as_the_linear_scan_does_bit_for_bit() {
        let mut maps: Vec<Map> = crate::maps::ALL.iter().map(|d| d.build()).collect();
        maps.push(Map::open());
        maps.push(with_solids());
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

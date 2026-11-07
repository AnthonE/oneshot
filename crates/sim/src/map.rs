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

/// A profile solid as the compiler writes it: kind, numbers and knots
/// ([`Solid::profile`]), and how it is drawn.
pub type ProfileDef = (u8, [f32; 8], &'static [[f32; 3]], u8);

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
    /// What its bed carries (`vehicle::NO_TURRET`, `vehicle::MINIGUN`).
    pub turret: u8,
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
    /// Out of the rotation (the Range; a new map not yet played): played
    /// only when a server or a private match names it.
    pub practice: bool,
    /// The rotations it is played in ([`ROT_DUEL`] and the rest, by bit);
    /// 0: by its size, as before there were any (Big Team on a big map,
    /// every other mode on an arena, Capture the Flag only with flags).
    pub modes: u16,
    pub lo: (f32, f32),
    pub hi: (f32, f32),
    /// [`Map::void`].
    pub void: f32,
    pub boxes: &'static [[f32; 6]],
    pub looks: &'static [u8],
    pub spawns: &'static [([f32; 3], u16, u8)],
    pub pads: &'static [([f32; 3], u8, u32)],
    pub roam: &'static [[f32; 2]],
    pub terrain: Option<TerrainDef>,
    pub launchers: &'static [([f32; 6], [f32; 3])],
    /// (x y z, yaw, respawn ms, turret) each.
    pub vehicles: &'static [([f32; 3], u16, u32, u8)],
    /// The solids that aren't boxes ([`Solid::from_def`]: kind, numbers),
    /// and how each is drawn.
    pub solids: &'static [(u8, [f32; 8])],
    pub solid_looks: &'static [u8],
    /// Boxes drawn and never collided (a vault's curve over the hidden
    /// steps that are its real ceiling): the client's, never read here.
    pub dress: &'static [([f32; 6], u8)],
    /// And solids drawn only (kind, numbers as `solids`, look).
    pub dress_solids: &'static [(u8, [f32; 8], u8)],
    /// Profile solids ([`Solid::profile`]: kind, numbers, knots), and how
    /// each is drawn: in [`Map::solids`] after the others.
    pub profiles: &'static [ProfileDef],
    /// And profile solids drawn only.
    pub dress_profiles: &'static [ProfileDef],
    /// Bots find their way by waypoints ([`crate::nav`]) rather than
    /// walking straight at where they're going.
    pub nav: bool,
    pub light: LightDef,
    /// Capture the Flag's bases, red's then blue's (x y z each); empty on a
    /// map that plays no CTF.
    pub flags: &'static [[f32; 3]],
}

/// How a map is lit: the client's, never read by the sim (like looks).
/// `ev100` 0 is the client's noon; `sun` and `fill` scale its sun and its
/// sky's fill (an interior under a roof wants less of both); `lamps` are
/// point lights: position, colour (0..1 each), lumens; `floor` is how a
/// flat map's floor is drawn (a look, or 255: the client's own); `sea` is
/// the height of the water the client draws (`f32::NEG_INFINITY`: none).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightDef {
    pub ev100: f32,
    pub sun: f32,
    pub fill: f32,
    pub lamps: &'static [([f32; 3], [f32; 3], f32)],
    pub floor: u8,
    pub sea: f32,
}

impl LightDef {
    /// Noon on a clear day, nothing else.
    pub const DAY: LightDef = LightDef {
        ev100: 0.0,
        sun: 1.0,
        fill: 1.0,
        lamps: &[],
        floor: 255,
        sea: f32::NEG_INFINITY,
    };
}

/// How far down a slope too steep to stand on [`Map::lost_floor`] looks
/// for the void: a cliff's foot, past the cell or two it falls over.
pub const CLIFF_LOOK: f32 = 2.0;

/// The rotations a map can be in (`Def::modes`, `map.json`'s `modes`): the
/// server's modes, by bit.
pub const ROT_CASUAL: u16 = 1 << 0;
pub const ROT_DUEL: u16 = 1 << 1;
pub const ROT_DOUBLES: u16 = 1 << 2;
pub const ROT_FFA: u16 = 1 << 3;
pub const ROT_GUN_GAME: u16 = 1 << 4;
pub const ROT_CUP: u16 = 1 << 5;
pub const ROT_BIG_TEAM: u16 = 1 << 6;
pub const ROT_CTF: u16 = 1 << 7;

/// A map this big (in m²) is for Big Team; smaller ones are arenas.
pub const BIG_M2: f32 = 4000.0;

impl Def {
    /// Whether this is a Big Team map ([`BIG_M2`] or more inside its
    /// bounds), not an arena.
    pub fn big(&self) -> bool {
        (self.hi.0 - self.lo.0) * (self.hi.1 - self.lo.1) >= BIG_M2
    }

    /// Whether it is in rotation `rot` (a `ROT_` bit): never out of the
    /// rotation ([`Def::practice`]) or without flags for Capture the Flag;
    /// else by its `modes`, or by its size when it names none.
    pub fn plays(&self, rot: u16) -> bool {
        if self.practice || (rot & ROT_CTF != 0 && self.flags.len() != 2) {
            return false;
        }
        if self.modes != 0 {
            return self.modes & rot != 0;
        }
        self.big() == (rot == ROT_BIG_TEAM)
    }

    pub fn build(&self) -> Map {
        let terrain = self.terrain.as_ref().map(Terrain::build);
        let ground = |x: f32, z: f32| terrain.as_ref().and_then(|t| t.height(x, z)).unwrap_or(0.0);
        let v = |p: [f32; 3]| {
            let g = ground(p[0], p[2]);
            V3::new(p[0], if p[1] == 0.0 { g } else { p[1].max(g) }, p[2])
        };
        let mut map = Map {
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
            practice: self.practice,
            name: self.name,
            lo: self.lo,
            hi: self.hi,
            void: self.void,
            boxes: self
                .boxes
                .iter()
                .map(|b| Aabb::new(V3::new(b[0], b[1], b[2]), V3::new(b[3], b[4], b[5])))
                .collect(),
            looks: self.looks.to_vec(),
            solids: self
                .solids
                .iter()
                .map(|&(k, n)| Solid::from_def(k, n))
                .chain(self.profiles.iter().map(|&(k, n, kn, _)| Solid::profile(k, n, kn)))
                .collect(),
            solid_looks: self
                .solid_looks
                .iter()
                .copied()
                .chain(self.profiles.iter().map(|p| p.3))
                .collect(),
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
                .map(|&(p, yaw, respawn_ms, turret)| VehicleSpawn {
                    pos: v(p),
                    yaw,
                    respawn_ms,
                    turret,
                })
                .collect(),
            flags: match self.flags {
                [r, b] => Some([v(*r), v(*b)]),
                _ => None,
            },
            terrain,
            nav: None,
            light: self.light,
        }
        .indexed();
        if self.nav {
            let nav = crate::nav::Nav::build(&map);
            map.nav = Some(nav);
        }
        map
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
    /// Out of the rotation, to practise on ([`Def::practice`]).
    pub practice: bool,
    pub name: &'static str,
    /// Walkable area in x and z: (min x, min z) to (max x, max z).
    pub lo: (f32, f32),
    pub hi: (f32, f32),
    /// Below this height there is nothing to stand on: a fighter whose feet
    /// pass it has fallen to its death, and a round, a drop or a vehicle is
    /// lost (`World::falls`). The ground under it is a terrain dug far
    /// down (an island's cliffs), never stood on. `f32::NEG_INFINITY`: no
    /// void, the ground holds everywhere.
    pub void: f32,
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
    /// Waypoints for bots, on a map that asks for them.
    pub nav: Option<crate::nav::Nav>,
    pub light: LightDef,
    /// Capture the Flag's bases, red's then blue's: where each flag sits
    /// at home and where its side scores. None: no CTF here.
    pub flags: Option<[V3; 2]>,
}

impl Map {
    /// Builds the grid (the boxes, then the solids); every constructor
    /// ends here, and a test that adds a box calls it again.
    pub fn indexed(mut self) -> Map {
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

    /// [`Self::ray`] asking every box and solid: a NaN direction crosses no
    /// grid cell past its first, so its walk would answer by what stands there.
    fn ray_all(&self, o: V3, d: V3, tmax: f32) -> f32 {
        let mut t = tmax;
        match &self.terrain {
            None => {
                if d.y < 0.0 && o.y >= 0.0 {
                    t = t.min(-o.y / d.y);
                }
            }
            Some(g) => t = g.ray(o, d, t),
        }
        for b in &self.boxes {
            if let Some(h) = ray_aabb(o, d, b, t) {
                t = h;
            }
        }
        for s in &self.solids {
            if let Some(h) = s.ray(o, d, t) {
                t = h;
            }
        }
        t
    }

    /// Whether the straight line from `a` to `b` is clear: what
    /// `self.ray(a, (b − a)/l, l) >= l` says, but the first thing in the way
    /// ends the walk, where [`Self::ray`] goes on for the nearest. Fog asks
    /// this tens of times per pair per snapshot.
    pub fn clear(&self, a: V3, b: V3) -> bool {
        let v = b - a;
        let l = v.len();
        if l == 0.0 {
            return true;
        }
        // A NaN or endless line answers as the nearest hit says (blocked,
        // for NaN): fog never shows what it couldn't measure.
        if !l.is_finite() {
            return self.ray_all(a, v * (1.0 / l), l) >= l;
        }
        let d = v * (1.0 / l);
        // Clear of both the boxes and solids and the ground: the boxes'
        // grid first, as it is the cheaper walk, so a line a rock or a wall
        // stops never walks the terrain. The answer is the same either way.
        if !self.clear_of_solids(a, b, d, l) {
            return false;
        }
        let ground = match &self.terrain {
            None if d.y < 0.0 && a.y >= 0.0 => -a.y / d.y,
            None => l,
            Some(g) => g.ray(a, d, l),
        };
        // As before for a NaN: only a hit nearer than the end blocks.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        let clear = !(ground < l);
        clear
    }

    /// Whether the line from `a` to `b` (direction `d`, length `l`) misses
    /// every box and solid.
    fn clear_of_solids(&self, a: V3, b: V3, d: V3, l: f32) -> bool {
        let n = self.boxes.len();
        // A box or solid wholly over or under the line can't be in its way
        // (a roof, a platform): skipped, with a millimetre to spare so no
        // rounding in the exact test could have said otherwise.
        let lo = a.y.min(b.y) - 0.001;
        let hi = a.y.max(b.y) + 0.001;
        let mut hit = false;
        self.grid.each_on_ray(a, d, l, |i| {
            let h = if i < n {
                let bx = &self.boxes[i];
                if bx.min.y > hi || bx.max.y < lo {
                    return l;
                }
                ray_aabb(a, d, bx, l)
            } else {
                let s = &self.solids[i - n];
                if s.y0 > hi || s.y1 < lo {
                    return l;
                }
                s.ray(a, d, l)
            };
            hit |= h.is_some_and(|h| h < l);
            // Zero ends the walk.
            if hit {
                0.0
            } else {
                l
            }
        });
        !hit
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

    /// Whether a body with its feet at `y` would find nothing under (x, z)
    /// but the void: [`Map::lost_floor`] of the floor there, as a step from
    /// `y` reaches it. Never on a map without one.
    pub fn over_void(&self, x: f32, z: f32, y: f32) -> bool {
        self.void > f32::NEG_INFINITY && self.lost_floor(x, z, self.floor(x, z, 0.0, y + crate::consts::STEP))
    }

    /// Whether `f`, the floor found at (x, z), holds nobody on a map with a
    /// void: it is under the void, or it is the ground of a cliff down into
    /// it, too steep to stand on with the ground [`CLIFF_LOOK`] down its
    /// fall line under the void (a body there slides off: the cliff starts
    /// up to a cell inside the coast). Never on a map without one.
    pub fn lost_floor(&self, x: f32, z: f32, f: f32) -> bool {
        if self.void == f32::NEG_INFINITY || f < self.void {
            return f < self.void;
        }
        if f != self.ground(x, z) {
            return false;
        }
        let n = self.ground_normal(x, z);
        if n.y >= crate::consts::WALK_NY {
            return false;
        }
        // Too steep: downhill is the normal's lean.
        let k = CLIFF_LOOK / (n.x * n.x + n.z * n.z).sqrt();
        self.ground(x + n.x * k, z + n.z * k) < self.void
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
        self.floor_of(x, z, r, reach, ramp_reach, true)
    }

    /// [`Map::floor`] without the profile solids: a bouncing round meets
    /// those along their own normals (`World::bounce_at`).
    pub fn floor_flat(&self, x: f32, z: f32, r: f32, reach: f32) -> f32 {
        self.floor_of(x, z, r, reach, reach, false)
    }

    fn floor_of(&self, x: f32, z: f32, r: f32, reach: f32, ramp_reach: f32, profiles: bool) -> f32 {
        let mut f = self.ground(x, z);
        self.each_box_in((x - r, z - r), (x + r, z + r), |b| {
            if b.max.y <= reach && b.max.y > f && b.over_xz(x, z, r) {
                f = b.max.y;
            }
        });
        self.each_solid_in((x - r, z - r), (x + r, z + r), |s| {
            let (top, lim) = match s.shape {
                crate::solid::Shape::Ramp { .. } => (s.top_at(x, z), ramp_reach),
                crate::solid::Shape::Revolve { .. } | crate::solid::Shape::Sweep { .. } => {
                    if profiles {
                        if let Some(t) = s.floor_for(x, z, r, reach, ramp_reach) {
                            f = f.max(t);
                        }
                    }
                    return;
                }
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
            if s.is_profile() {
                if let Some(b) = s.bot_over(x, z, r) {
                    if b >= from && b < c {
                        c = b;
                    }
                }
                return;
            }
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
            hit |= if s.is_profile() {
                let e = feet + 1e-3;
                s.blocks(
                    x,
                    z,
                    r,
                    crate::profile::Band {
                        feet,
                        lo: e,
                        hi: head,
                        climb: e,
                    },
                )
            } else {
                s.over(x, z, r) && s.y0 < head && s.top_at(x, z) > feet + 1e-3
            };
        });
        hit
    }

    /// Pushes a body of radius `r` at (x, z) back out of every solid in its
    /// band, its speed into each face taken off (it slides along). `lo` and
    /// `hi`: the band a box blocks in (movement's); a ramp blocks only where
    /// its slope stands more than `climb` over the feet, at `feet`; a
    /// profile solid by its own rules ([`crate::profile`]). Three rounds,
    /// for a body in a corner where two solids meet. A push that would put
    /// the body in a box is undone: it stays where it started the tick,
    /// `back`.
    #[allow(clippy::too_many_arguments)]
    pub fn push_out(
        &self,
        x: &mut f32,
        z: &mut f32,
        v: &mut V3,
        r: f32,
        feet: f32,
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
                let pushed = if s.is_profile() {
                    s.push_band(*x, *z, r, crate::profile::Band { feet, lo, hi, climb }, back)
                } else {
                    let blocks = match s.shape {
                        crate::solid::Shape::Ramp { .. } => s.y0 < hi && s.top_at(*x, *z) > climb,
                        _ => s.y0 < hi && s.y1 > lo,
                    };
                    if !blocks {
                        return;
                    }
                    s.push(*x, *z, r)
                };
                if let Some((nx, nz, ux, uz)) = pushed {
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
    /// shotgun on a pad 30 m off the middle (at z -30): for tests that
    /// place fighters by hand.
    pub fn open() -> Map {
        let b = |x0: f32, z0: f32, x1: f32, z1: f32| Aabb::new(V3::new(x0, 0.0, z0), V3::new(x1, 8.0, z1));
        Map {
            grid: Grid::default(),
            terrain: None,
            launchers: Vec::new(),
            vehicles: Vec::new(),
            nav: None,
            light: LightDef::DAY,
            flags: None,
            id: TEST_MAP,
            practice: false,
            name: "open",
            looks: vec![0; 4],
            solids: Vec::new(),
            solid_looks: Vec::new(),
            roam: Vec::new(),
            lo: (-50.0, -50.0),
            hi: (50.0, 50.0),
            void: f32::NEG_INFINITY,
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
            if let Some(h) = s.ray_exact(o, d, t) {
                t = h;
            }
        }
        t
    }

    /// What [`Map::clear`] must say: the nearest hit, by the linear scan,
    /// no nearer than the far end.
    fn clear_linear(m: &Map, a: V3, b: V3) -> bool {
        let v = b - a;
        let l = v.len();
        l == 0.0 || ray_linear(m, a, v * (1.0 / l), l) >= l
    }

    fn floor_linear(m: &Map, x: f32, z: f32, r: f32, reach: f32) -> f32 {
        let mut f = m.ground(x, z);
        for b in &m.boxes {
            if b.max.y <= reach && b.max.y > f && b.over_xz(x, z, r) {
                f = b.max.y;
            }
        }
        for s in &m.solids {
            if s.is_profile() {
                if let Some(t) = s.floor_for(x, z, r, reach, reach) {
                    f = f.max(t);
                }
                continue;
            }
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
            if s.is_profile() {
                if let Some(b) = s.bot_over(x, z, r) {
                    if b >= from && b < c {
                        c = b;
                    }
                }
                continue;
            }
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
            practice: false,
            modes: 0,
            name: "with solids",
            lo: (-20.0, -20.0),
            hi: (20.0, 20.0),
            void: f32::NEG_INFINITY,
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
            dress: &[],
            dress_solids: &[],
            profiles: PROFILES,
            dress_profiles: &[],
            nav: false,
            light: LightDef::DAY,
            flags: &[],
        }
        .build()
    }

    /// A dome, a bulb on its foot, a ring's sector, a vault and an arch.
    const PROFILES: &[ProfileDef] = &[
        (
            3,
            [14.0, -12.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            &[[0.0, 0.0, 1.6], [1.0, 0.0, 1.45], [2.0, 0.0, 0.9], [2.6, 0.0, 0.0]],
            0,
        ),
        (
            3,
            [-14.0, 12.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            &[
                [0.0, 0.0, 2.4],
                [0.6, 0.0, 2.3],
                [0.6, 0.35, 2.3],
                [1.0, 0.6, 2.0],
                [1.2, 1.2, 1.4],
            ],
            0,
        ),
        (
            4,
            [4.0, 12.0, 3.0, 1.0, 0.0, 0.0, 1.0, 0.0],
            &[[3.0, 0.0, 0.3], [3.5, -0.2, 0.4], [4.0, 0.0, 0.3]],
            0,
        ),
        (
            5,
            [0.0, -14.0, 0.0, 1.0, 0.0, 6.0, 0.0, 0.0],
            &[
                [-3.0, 3.0, 5.0],
                [-2.0, 3.8, 5.0],
                [0.0, 4.2, 5.0],
                [2.0, 3.8, 5.0],
                [3.0, 3.0, 5.0],
            ],
            0,
        ),
        (
            5,
            [16.0, 4.0, 0.0, 0.0, 1.0, 0.4, 0.0, 0.0],
            &[
                [-2.0, 0.0, 3.0],
                [-1.2, 0.0, 3.0],
                [-1.2, 2.0, 3.0],
                [0.0, 2.6, 3.0],
                [1.2, 2.0, 3.0],
                [1.2, 0.0, 3.0],
                [2.0, 0.0, 3.0],
            ],
            0,
        ),
    ];

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
    fn clear_answers_as_the_nearest_hit_does() {
        // Lines between two places a fighter could be, as fog draws them
        // (eye to body), on every map: low, high, across roofs and pits.
        let mut maps: Vec<Map> = crate::maps::ALL.iter().map(|d| d.build()).collect();
        maps.push(with_solids());
        let mut r = Pcg32::new(17, 5);
        for m in &maps {
            let (lx, lz) = (m.lo.0 - 2.0, m.lo.1 - 2.0);
            let (hx, hz) = (m.hi.0 + 2.0, m.hi.1 + 2.0);
            let mut blocked = 0;
            for _ in 0..40_000 {
                let mut p = || {
                    let (x, z) = (unit(&mut r, lx, hx), unit(&mut r, lz, hz));
                    V3::new(x, m.ground(x, z) + unit(&mut r, -0.5, 9.0), z)
                };
                let (a, b) = (p(), p());
                let want = clear_linear(m, a, b);
                assert_eq!(m.clear(a, b), want, "{}: {a:?} to {b:?}", m.name);
                blocked += usize::from(!want);
            }
            assert!(blocked > 1000, "{}: the lines must meet things", m.name);
        }
    }

    #[test]
    fn clear_answers_a_nan_or_endless_line_as_the_nearest_hit_does() {
        let n = f32::NAN;
        let i = f32::INFINITY;
        for m in crate::maps::ALL.iter().map(|d| d.build()) {
            for (a, b) in [
                (V3::new(0.0, 1.5, 0.0), V3::new(n, 1.5, 3.0)),
                (V3::new(n, n, n), V3::new(1.0, 1.5, 3.0)),
                (V3::new(0.0, 1.5, 0.0), V3::new(i, 1.5, 3.0)),
                (V3::new(-i, 1.5, 0.0), V3::new(i, 1.5, 3.0)),
                (V3::new(0.0, 1.5, 0.0), V3::new(1e30, 1.5, 3.0)),
            ] {
                assert_eq!(m.clear(a, b), clear_linear(&m, a, b), "{}: {a:?} to {b:?}", m.name);
            }
            assert!(!m.clear(V3::new(0.0, 1.5, 0.0), V3::new(n, 1.5, 3.0)));
        }
    }

    #[test]
    fn a_ray_past_a_curve_is_answered_as_the_exact_test_does() {
        // Aimed at points just around each round and turned solid, so many
        // graze it: the quick look first must never change an answer.
        let mut maps: Vec<Map> = crate::maps::ALL.iter().map(|d| d.build()).collect();
        maps.push(with_solids());
        let mut r = Pcg32::new(23, 9);
        for m in &maps {
            for s in &m.solids {
                let bb = s.bounds();
                let c = (bb.min + bb.max) * 0.5;
                let reach = (bb.max - bb.min).len() * 0.5 + 0.2;
                for _ in 0..2_000 {
                    let at = c + V3::new(
                        unit(&mut r, -reach, reach),
                        unit(&mut r, -1.0, 1.0),
                        unit(&mut r, -reach, reach),
                    );
                    let o = at
                        + V3::new(
                            unit(&mut r, -20.0, 20.0),
                            unit(&mut r, -2.0, 4.0),
                            unit(&mut r, -20.0, 20.0),
                        );
                    let v = at - o;
                    if v.len() == 0.0 {
                        continue;
                    }
                    let d = v.norm();
                    let tmax = unit(&mut r, 0.0, 60.0);
                    assert_eq!(
                        s.ray(o, d, tmax).map(f32::to_bits),
                        s.ray_exact(o, d, tmax).map(f32::to_bits),
                        "{}: {s:?} {o:?} {d:?}",
                        m.name
                    );
                }
                // From far across a big map, aimed within a tenth of a
                // millimetre of an edge: where rounding is largest and a
                // ray only grazes.
                for e in edges(s) {
                    for _ in 0..200 {
                        let at = V3::new(
                            e.x + unit(&mut r, -1e-4, 1e-4),
                            unit(&mut r, s.y0 + 0.01, s.y1 - 0.01),
                            e.z + unit(&mut r, -1e-4, 1e-4),
                        );
                        let far = unit(&mut r, 40.0, 150.0);
                        let turn = unit(&mut r, 0.0, core::f32::consts::TAU);
                        let o = at
                            + V3::new(
                                far * crate::fmath::cos(turn),
                                unit(&mut r, -3.0, 3.0),
                                far * crate::fmath::sin(turn),
                            );
                        let d = (at - o).norm();
                        let tmax = (at - o).len() + unit(&mut r, -0.01, 0.01);
                        assert_eq!(
                            s.ray(o, d, tmax).map(f32::to_bits),
                            s.ray_exact(o, d, tmax).map(f32::to_bits),
                            "{}: {s:?} {o:?} {d:?} {tmax}",
                            m.name
                        );
                    }
                }
            }
        }
    }

    /// A round solid's rim every 10°, a turned one's corners.
    fn edges(s: &crate::solid::Solid) -> Vec<V3> {
        use crate::solid::Shape;
        match s.shape {
            Shape::Round { x, z, r } => (0..36)
                .map(|k| {
                    let a = k as f32 * (core::f32::consts::TAU / 36.0);
                    V3::new(x + r * crate::fmath::cos(a), 0.0, z + r * crate::fmath::sin(a))
                })
                .collect(),
            Shape::Turned { x, z, hx, hz, c, s } => [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)]
                .iter()
                .map(|&(a, b)| V3::new(x + c * a * hx - s * b * hz, 0.0, z + s * a * hx + c * b * hz))
                .collect(),
            Shape::Ramp { .. } => Vec::new(),
            Shape::Revolve { x, z, k, .. } => {
                let r = k[k.len() - 1][0];
                (0..36)
                    .map(|i| {
                        let a = i as f32 * (core::f32::consts::TAU / 36.0);
                        V3::new(x + r * crate::fmath::cos(a), 0.0, z + r * crate::fmath::sin(a))
                    })
                    .collect()
            }
            Shape::Sweep { x, z, c, s, l, k, .. } => {
                let (s0, sn) = (k[0][0], k[k.len() - 1][0]);
                [(l, s0), (l, sn), (-l, s0), (-l, sn)]
                    .iter()
                    .map(|&(u, v)| V3::new(x + u * c - v * s, 0.0, z + u * s + v * c))
                    .collect()
            }
        }
    }

    #[test]
    fn the_rays_the_review_found_meet_the_wall() {
        // Two lines across the old Chrysalis that grazed a curved wall's
        // corner (a turned segment of an arc) from over 40 m, and every
        // line within 5 mm of them.
        let wall = |x: f32, z: f32, c: f32| Solid::from_def(1, [x, z, 0.46488917, 0.25, c, -0.479249, 0.0, 8.0]);
        for (s, a, b) in [
            (
                wall(-16.995306, 12.485494, -0.877679),
                V3::new(19.827059, 1.3204271, -11.240811),
                V3::new(-16.70709, 1.5, 12.927709),
            ),
            (
                wall(16.995306, -12.485494, -0.877679),
                V3::new(-17.321032, 1.1206951, 10.461417),
                V3::new(17.283522, 1.5, -12.043298),
            ),
        ] {
            for i in -50..=50 {
                for k in -50..=50 {
                    let b = b + V3::new(i as f32 * 1e-4, 0.0, k as f32 * 1e-4);
                    let v = b - a;
                    let (d, l) = (v.norm(), v.len());
                    assert_eq!(
                        s.ray(a, d, l).map(f32::to_bits),
                        s.ray_exact(a, d, l).map(f32::to_bits),
                        "{a:?} to {b:?}"
                    );
                }
            }
            assert!(s.ray_exact(a, (b - a).norm(), (b - a).len()).is_some());
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

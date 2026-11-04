//! Marks the world keeps: bullet holes, a rifle butt's chips and dents, a
//! sword's gashes, scuffs in the soil, blood behind a body that was hit, and
//! blast scorches.
//!
//! After Gates' `crates/client/src/render/decal.rs` (MIT, © 2026 MoreRight
//! DAO), the whole of it but the weak-spot cross (a survival game's node):
//!
//! - **One mesh, one draw, every target.** Every mark is a small
//!   [`MARK_GRID`]² patch in ONE dynamic mesh drawn with ONE lit, blended
//!   `StandardMaterial` over a generated atlas (grey value × alpha, the
//!   matter's [`tint`] colouring it) and a second atlas of each mark's relief
//!   as a normal map, so a hole is a pit the sun lights one side of and a
//!   gash a groove, not a sticker. Two textures, no storage buffer, no
//!   compute: the desktop, WebGPU and WebGL2 draw it the same way.
//! - **On the drawn surface.** A mark is laid on the face the blow met
//!   (`surface.rs`: the sim's shapes as `level.rs` draws them), then every
//!   vertex of its grid is cast back onto the map ([`Marks::conform`]): it
//!   follows a turned wall's segments round a curve, a ramp's slope, the
//!   terrain's triangles and the rock skin over them (`Marks::lift`), and
//!   fades out over an edge it cannot reach instead of hanging in the air
//!   past it. On a round pillar it is first bent about the pillar's axis
//!   (Gates' trunk bend) and then fitted to the facets it is drawn with.
//! - **What a blow leaves** ([`kind`]): a round a hole whose rim is the
//!   matter's (a soil crater, chipped stone with cracks, bright bare metal),
//!   a rifle butt a chip in concrete, a dent in metal or a scuff in the
//!   soil, the sword a gash, a blast a scorch. Blood ([`Marks::bleed`]) is a
//!   splat on whatever stands a short way behind the body along the blow,
//!   else on the floor behind them, and goes sooner than a hole.
//! - **A fixed pool**, [`super::MARKS`] slots (the same on every rung:
//!   marks are the world's, `quality.rs`' fairness), drop-oldest; the mesh is
//!   rewritten only when a mark is placed, a fade step moves or the pool is
//!   forgotten, and a free slot is a collapsed patch.
//!
//! **Only what we were told.** A mark is laid only for a blow the session
//! handed us as an event with its place (`mod.rs`): a shot we saw (fog sends
//! an unseen one as a rounded sound, which draws nothing), a melee swung by
//! someone we can see, a hit on a body we draw, a blast we saw, a round of
//! light we were sent. A mark never says more than the event did.
//!
//! The holes', scorch's and blood's texels are Gates' cells, seeded as there.

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::ImageSampler;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use sim::map::Map;

use super::atlas::{fbm, hash01, smooth, vnoise};
use super::surface::{self, Near, On};
use super::table::{Matter, Weapon};

/// Seconds a mark stays, then how long it fades.
pub const LIFE_S: f32 = 30.0;
/// Blood's life: shorter, so a fight's blood doesn't outlast it.
pub const BLOOD_LIFE_S: f32 = 20.0;
pub const FADE_S: f32 = 4.0;
/// Lifted this far off its surface along the normal (with the material's
/// depth bias): at zero the depth test is a coin toss per pixel.
const LIFT_M: f32 = 0.008;
/// Fade steps: the mesh is rewritten when one moves, not every frame.
const ALPHA_STEPS: f32 = 32.0;
/// A melee chip's or scuff's nominal width, metres; the rest are sized by
/// what made them ([`kind`]).
pub const SIZE_M: f32 = 0.22;
/// A blood splat's width, metres.
pub const BLOOD_M: f32 = 0.55;
/// How far behind a body along the blow blood reaches a wall, metres.
pub const BLOOD_WALL_M: f32 = 2.0;
/// How far behind a body, along the ground, blood falls when no wall is
/// that near (metres, the least and the most).
pub const BLOOD_BACK_M: (f32, f32) = (0.2, 0.7);
/// How far under the blow blood looks for a floor, metres: past a body's
/// chest to its feet and a step below.
pub const BLOOD_DROP_M: f32 = 2.6;
/// A round of light's burn on a wall, metres (a charged one twice it).
pub const BURN_M: f32 = 0.32;

/// Vertices across and up one mark: a grid, so it can follow a curve or a
/// slope both ways.
pub const MARK_GRID: usize = 5;
const VERTS: usize = MARK_GRID * MARK_GRID;

/// The atlas: [`ATLAS_COLS`] × [`ATLAS_ROWS`] cells of [`CELL_TEX`]² texels.
pub const ATLAS_COLS: u32 = 8;
pub const ATLAS_ROWS: u32 = 3;
pub const CELL_TEX: u32 = 128;

/// What a mark is: a run of variant cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    HoleSoil,
    HoleStone,
    HoleMetal,
    /// The sword's cut.
    Gash,
    /// A rifle butt on concrete.
    ChipStone,
    /// A rifle butt on metal.
    DentMetal,
    /// A melee blow in the soil.
    ScuffSoil,
    Scorch,
    Blood,
}

impl Kind {
    pub const ALL: [Kind; 9] = [
        Kind::HoleSoil,
        Kind::HoleStone,
        Kind::HoleMetal,
        Kind::Gash,
        Kind::ChipStone,
        Kind::DentMetal,
        Kind::ScuffSoil,
        Kind::Scorch,
        Kind::Blood,
    ];

    /// The first atlas cell and how many variants follow it.
    pub fn cells(self) -> (u32, u32) {
        match self {
            Kind::HoleSoil => (0, 2),
            Kind::HoleStone => (2, 2),
            Kind::HoleMetal => (4, 2),
            Kind::Gash => (6, 2),
            Kind::ChipStone => (8, 2),
            Kind::DentMetal => (10, 2),
            Kind::ScuffSoil => (12, 2),
            Kind::Scorch => (14, 2),
            Kind::Blood => (16, 4),
        }
    }

    fn life(self) -> f32 {
        match self {
            Kind::Blood => BLOOD_LIFE_S,
            _ => LIFE_S,
        }
    }
}

/// Which mark a blow leaves where it struck and how wide it is, metres:
/// none on a body or a hull (they move; a body's blood is laid behind it,
/// [`Marks::bleed`]).
pub fn kind(weapon: Weapon, matter: Matter) -> Option<(Kind, f32)> {
    let k = match weapon {
        Weapon::Sniper => 1.5,
        Weapon::Pellet => 0.7,
        _ => 1.0,
    };
    Some(match (weapon, matter) {
        (_, Matter::Shield | Matter::Flesh | Matter::Hull) => return None,
        (Weapon::Blast, _) => (Kind::Scorch, 2.6),
        (Weapon::Melee, Matter::Concrete) => (Kind::ChipStone, SIZE_M),
        (Weapon::Melee, Matter::Metal) => (Kind::DentMetal, SIZE_M),
        (Weapon::Melee, Matter::Dirt) => (Kind::ScuffSoil, SIZE_M * 1.3),
        (Weapon::Blade, Matter::Dirt) => (Kind::ScuffSoil, SIZE_M * 1.5),
        (Weapon::Blade, _) => (Kind::Gash, SIZE_M * 1.4),
        (_, Matter::Concrete) => (Kind::HoleStone, 0.15 * k),
        (_, Matter::Metal) => (Kind::HoleMetal, 0.12 * k),
        (_, Matter::Dirt) => (Kind::HoleSoil, 0.2 * k),
    })
}

/// A mark's colour on its matter; the atlas carries the value. Pale where
/// the blow bares something pale (fresh concrete, bare metal), dark where it
/// turns soil over.
pub fn tint(kind: Kind, matter: Matter) -> Color {
    match (kind, matter) {
        (Kind::Blood, _) => Color::srgb(0.40, 0.03, 0.03),
        (Kind::Scorch, _) => Color::srgb(0.17, 0.15, 0.13),
        (_, Matter::Metal | Matter::Hull) => Color::srgb(0.84, 0.84, 0.86),
        (_, Matter::Dirt) => Color::srgb(0.36, 0.27, 0.19),
        _ => Color::srgb(0.80, 0.78, 0.74),
    }
}

/// One vertex of a mark: position, normal, uv, linear colour, tangent.
pub type MarkVertex = ([f32; 3], [f32; 3], [f32; 2], [f32; 4], [f32; 4]);

/// One grid vertex of a mark, on its surface (the lift is added at write).
#[derive(Clone, Copy)]
struct MarkVert {
    p: Vec3,
    n: Vec3,
    /// The world direction of the atlas's +u here.
    t: Vec3,
    /// The tangent's handedness, Bevy's `w`.
    w: f32,
    /// The cell-local uv, 0..1.
    uv: Vec2,
    /// How much of the mark shows here: 0 past an edge it could not reach.
    a: f32,
}

impl Default for MarkVert {
    fn default() -> Self {
        Self {
            p: Vec3::ZERO,
            n: Vec3::Y,
            t: Vec3::X,
            w: 1.0,
            uv: Vec2::ZERO,
            a: 1.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Mark {
    /// Seconds left through life and fade; zero is a free slot.
    left: f32,
    /// The fade step last written.
    step: i32,
    size: f32,
    cell: u32,
    /// Linear tint.
    tint: [f32; 3],
    grid: [MarkVert; VERTS],
}

impl Default for Mark {
    fn default() -> Self {
        Self {
            left: 0.0,
            step: -1,
            size: 0.0,
            cell: 0,
            tint: [1.0; 3],
            grid: [MarkVert::default(); VERTS],
        }
    }
}

/// A mark's grid, laid flat or bent about an upright axis at `bend_r`
/// (a round pillar): +u along `tangent`, +v along `normal × tangent` (flat)
/// or up the pillar (bent), the cell turned by `flip`'s dihedral bits (bit 0
/// swaps u and v, bits 1 and 2 mirror them). A bent patch is turned by
/// `spin` radians on the surface before it is wrapped round; a flat one
/// takes its turn in `tangent`. Gates' `grid_of`.
fn grid_of(at: Vec3, normal: Vec3, tangent: Vec3, size: f32, bend_r: f32, flip: u8, spin: f32) -> [MarkVert; VERTS] {
    let mut out = [MarkVert::default(); VERTS];
    let b = normal.cross(tangent);
    let (ss, cs) = spin.sin_cos();
    let last = (MARK_GRID - 1) as f32;
    for j in 0..MARK_GRID {
        for i in 0..MARK_GRID {
            let (u, v) = (i as f32 / last, j as f32 / last);
            let (x, y) = ((u - 0.5) * size, (v - 0.5) * size);
            let (p, n, du, dv) = if bend_r > 0.0 {
                // Round the pillar and up it, after the turn.
                let (round, up) = (x * cs - y * ss, x * ss + y * cs);
                let (s, c) = (round / bend_r).sin_cos();
                let arc = tangent * c - normal * s;
                (
                    at + tangent * (bend_r * s) + normal * (bend_r * (c - 1.0)) + Vec3::Y * up,
                    tangent * s + normal * c,
                    arc * cs + Vec3::Y * ss,
                    Vec3::Y * cs - arc * ss,
                )
            } else {
                (at + tangent * x + b * y, normal, tangent, b)
            };
            let (mut tu, mut tv, mut au, mut av) = (u, v, du, dv);
            if flip & 1 != 0 {
                std::mem::swap(&mut tu, &mut tv);
                std::mem::swap(&mut au, &mut av);
            }
            if flip & 2 != 0 {
                tu = 1.0 - tu;
                au = -au;
            }
            if flip & 4 != 0 {
                tv = 1.0 - tv;
                av = -av;
            }
            // Bevy's bitangent is `cross(n, t) · w` and points toward −v.
            let w = if n.cross(au).dot(av) > 0.0 { -1.0 } else { 1.0 };
            out[j * MARK_GRID + i] = MarkVert {
                p,
                n,
                t: au,
                w,
                uv: Vec2::new(tu, tv),
                a: 1.0,
            };
        }
    }
    out
}

/// How far a mark's vertices are searched for its surface, metres: past a
/// facet's sagitta and a ramp's kink, not as far as the next face (a step
/// further down than this is an edge the mark fades at).
fn conform_reach(size: f32) -> f32 {
    (size * 0.3).clamp(0.05, 0.25)
}

pub struct Marks {
    slots: Vec<Mark>,
    /// The slot to recycle when every one is busy: a rotating hand.
    hand: usize,
    rng: u32,
    dirty: bool,
    mesh: Handle<Mesh>,
    /// The faces a mark is being fitted to, reused.
    near: Near,
    /// Marks laid, and live ones recycled for want of a slot.
    pub placed: u64,
    pub stolen: u64,
    /// Collision boxes drawn as rock that stands back from them
    /// (`rock.rs`' masses, out of bounds): a round stopped there stopped
    /// behind the drawn rock, and its mark would hang in the air.
    pub bare: Vec<(Vec3, Vec3)>,
    /// The rock skin over the terrain (`cliffs.rs`): a mark on the ground
    /// is laid on the shell drawn over it, not under it.
    pub lift: Option<Arc<crate::look::cliffs::Lift>>,
}

impl Marks {
    pub fn new(cap: usize) -> Marks {
        Marks {
            slots: vec![Mark::default(); cap.max(1)],
            hand: 0,
            rng: 0x2545_f491,
            dirty: false,
            mesh: Handle::default(),
            near: Near::default(),
            placed: 0,
            stolen: 0,
            bare: Vec::new(),
            lift: None,
        }
    }

    fn roll(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1 << 24) as f32
    }

    /// A free slot, or the oldest live one: drop-oldest, the oldest is the
    /// faintest.
    fn claim(&mut self) -> usize {
        if let Some(i) = self.slots.iter().position(|m| m.left <= 0.0) {
            return i;
        }
        self.stolen += 1;
        let i = self.hand;
        self.hand = (self.hand + 1) % self.slots.len();
        i
    }

    /// How many marks are drawing.
    pub fn live(&self) -> usize {
        self.slots.iter().filter(|m| m.left > 0.0).count()
    }

    /// Whether `at` is on a box drawn as something standing back from it.
    fn on_bare(&self, at: Vec3) -> bool {
        let e = Vec3::splat(0.01);
        self.bare
            .iter()
            .any(|&(lo, hi)| at.cmpge(lo - e).all() && at.cmple(hi + e).all())
    }

    /// The rock skin's height over the sim's ground at (x, z), if a skin is
    /// drawn there.
    fn skin(&self, x: f32, z: f32) -> f32 {
        self.lift
            .as_ref()
            .and_then(|l| l.at(x, z))
            .map_or(0.0, |(_, lift)| lift.max(0.0))
    }

    /// One mark in a slot: a random variant of `kind`, sized and turned at
    /// random (along `along` on the surface when given), flat or bent about
    /// a pillar of radius `bend_r`. Pure: no map, no assets.
    #[allow(clippy::too_many_arguments)]
    fn put(
        &mut self,
        kind: Kind,
        size: f32,
        at: Vec3,
        normal: Vec3,
        matter: Matter,
        bend_r: f32,
        along: Option<Vec3>,
    ) -> usize {
        let n = normal.normalize_or(Vec3::Y);
        let (first, variants) = kind.cells();
        let cell = first + ((self.roll() * variants as f32) as u32).min(variants - 1);
        let size = size * (0.8 + 0.4 * self.roll());
        let flat = |v: Vec3| (v - n * n.dot(v)).try_normalize();
        let (tangent, flip, spin) = if bend_r > 0.0 {
            let t = Vec3::Y.cross(n).normalize_or(Vec3::X);
            let spin = match along {
                Some(a) => a.dot(Vec3::Y).atan2(a.dot(t)),
                None => self.roll() * core::f32::consts::TAU,
            };
            let flip = if along.is_some() {
                0
            } else {
                (self.roll() * 4.0) as u8 * 2
            };
            (t, flip, spin)
        } else {
            let base = if n.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
            let t0 = base.cross(n).normalize_or(Vec3::X);
            let t = match along.and_then(flat) {
                Some(t) => t,
                None => Quat::from_axis_angle(n, self.roll() * core::f32::consts::TAU) * t0,
            };
            (t, 0, 0.0)
        };
        // A little brightness jitter, so a row of holes isn't one sticker.
        let lin = tint(kind, matter).to_linear();
        let j = 0.88 + 0.24 * self.roll();
        let i = self.claim();
        self.slots[i] = Mark {
            left: kind.life() + FADE_S,
            step: -1,
            size,
            cell,
            tint: [lin.red * j, lin.green * j, lin.blue * j],
            grid: grid_of(at, n, tangent, size, bend_r, flip, spin),
        };
        self.placed += 1;
        self.dirty = true;
        i
    }

    /// Leave a mark of `kind`, `size` metres, flat at `at` on a plane facing
    /// `normal`: where no map is drawn (the fx stage's cards).
    pub fn place(&mut self, kind: Kind, size: f32, at: Vec3, normal: Vec3, matter: Matter) -> Option<usize> {
        if self.on_bare(at) {
            return None;
        }
        Some(self.put(kind, size, at, normal, matter, 0.0, None))
    }

    /// Leave a mark of `kind`, `size` metres, on the map's drawn surface at
    /// `at` (facing about `normal`): laid on the face found there, bent
    /// round a pillar, then fitted to the map vertex by vertex. None where
    /// nothing drawn is there.
    pub fn lay(&mut self, map: &Map, kind: Kind, size: f32, at: Vec3, normal: Vec3, matter: Matter) -> Option<usize> {
        self.lay_along(map, kind, size, at, normal, matter, None)
    }

    /// [`Marks::lay`], its cell's +u along `along` on the surface (blood's
    /// spray, along the blow) when given.
    #[allow(clippy::too_many_arguments)]
    pub fn lay_along(
        &mut self,
        map: &Map,
        kind: Kind,
        size: f32,
        at: Vec3,
        normal: Vec3,
        matter: Matter,
        along: Option<Vec3>,
    ) -> Option<usize> {
        let n = normal.normalize_or(Vec3::Y);
        if self.on_bare(at) {
            return None;
        }
        let mut near = std::mem::take(&mut self.near);
        let hit = surface::cast(map, &mut near, at + n * 0.15, -n, 0.3);
        self.near = near;
        let hit = hit?;
        // On the sim's ground: the rock skin over it is added per vertex by
        // the fit, which casts from here.
        let p = hit.at;
        // A round pillar's side: bent about its axis at the radius struck.
        let bend_r = match surface::round(map, hit.on) {
            Some((c, _)) if hit.normal.y.abs() < 0.5 => Vec2::new(p.x - c.x, p.z - c.y).length(),
            _ => 0.0,
        };
        let i = self.put(kind, size, p, hit.normal, matter, bend_r, along);
        self.conform(i, map);
        Some(i)
    }

    /// Fit slot `i` to the map: each vertex cast back along its own normal
    /// onto the faces near the mark. A vertex the map doesn't meet within
    /// reach, or meets turned away, fades to nothing: the mark ends at an
    /// edge rather than hanging past it.
    fn conform(&mut self, i: usize, map: &Map) {
        let Some(m) = self.slots.get(i).copied() else {
            return;
        };
        if m.left <= 0.0 {
            return;
        }
        let reach = conform_reach(m.size);
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in &m.grid {
            lo = lo.min(v.p);
            hi = hi.max(v.p);
        }
        let pad = Vec3::splat(reach + 0.01);
        let mut near = std::mem::take(&mut self.near);
        near.gather(map, lo - pad, hi + pad, &self.bare);
        let mut grid = m.grid;
        for v in grid.iter_mut() {
            let n = v.n;
            match near.cast(map, v.p + n * reach, -n, reach * 2.0) {
                Some(h) => {
                    v.a *= smooth(0.15, 0.45, h.normal.dot(n));
                    v.p = h.at;
                    if h.on == On::Ground {
                        v.p.y += self.skin(h.at.x, h.at.z);
                    }
                    v.n = h.normal;
                    v.t = (v.t - h.normal * h.normal.dot(v.t)).normalize_or(v.t);
                }
                None => v.a = 0.0,
            }
        }
        self.near = near;
        self.slots[i].grid = grid;
        self.dirty = true;
    }

    /// Blood for a body hit at `at` by a blow along `dir`: a splat on what
    /// stands within [`BLOOD_WALL_M`] behind them along the blow, or on the
    /// floor a little behind them. None when neither is near (a body high
    /// in the air).
    pub fn bleed(&mut self, map: &Map, at: Vec3, dir: Vec3) -> Option<usize> {
        let dir = dir.normalize_or(Vec3::X);
        let size = BLOOD_M * (0.75 + 0.25 * self.roll());
        let mut near = std::mem::take(&mut self.near);
        let wall = surface::cast(map, &mut near, at, dir, BLOOD_WALL_M);
        let flat = dir.with_y(0.0).normalize_or(Vec3::X);
        let back = BLOOD_BACK_M.0 + (BLOOD_BACK_M.1 - BLOOD_BACK_M.0) * self.roll();
        let p = at + flat * back;
        let floor = surface::cast(map, &mut near, p + Vec3::Y * 0.1, Vec3::NEG_Y, BLOOD_DROP_M);
        self.near = near;
        match (wall, floor) {
            (Some(h), _) if h.normal.dot(dir) < -0.2 && !self.on_bare(h.at) => {
                self.lay_along(map, Kind::Blood, size, h.at, h.normal, Matter::Flesh, Some(dir))
            }
            (_, Some(h)) if h.normal.y > 0.5 => {
                self.lay_along(map, Kind::Blood, size, h.at, h.normal, Matter::Flesh, Some(flat))
            }
            _ => None,
        }
    }

    /// Forget every mark (a new map).
    pub fn clear(&mut self) {
        for m in &mut self.slots {
            *m = Mark::default();
        }
        self.hand = 0;
        self.dirty = true;
    }

    /// The mesh the marks are drawn in.
    pub fn mesh(&self) -> &Handle<Mesh> {
        &self.mesh
    }

    /// Age every mark; whether the mesh must be rewritten.
    pub fn fade(&mut self, dt: f32) -> bool {
        for m in &mut self.slots {
            if m.left <= 0.0 {
                continue;
            }
            m.left -= dt;
            if m.left <= 0.0 {
                *m = Mark::default();
                self.dirty = true;
                continue;
            }
            let s = step(m.left);
            if s != m.step {
                m.step = s;
                self.dirty = true;
            }
        }
        std::mem::take(&mut self.dirty)
    }

    /// Slot `i`'s vertices: a collapsed patch for a free slot.
    pub fn vertices(&self, i: usize) -> [MarkVertex; VERTS] {
        let mut out = [([0.0; 3], [0.0, 1.0, 0.0], [0.0; 2], [0.0; 4], [1.0, 0.0, 0.0, 1.0]); VERTS];
        let Some(m) = self.slots.get(i) else {
            return out;
        };
        if m.left <= 0.0 {
            return out;
        }
        let a = (step(m.left) as f32 / ALPHA_STEPS).clamp(0.0, 1.0);
        let (cu, cv) = ((m.cell % ATLAS_COLS) as f32, (m.cell / ATLAS_COLS) as f32);
        // Half a texel in from the cell's edge, so no mip reads the
        // neighbour.
        let inset = 0.5 / CELL_TEX as f32;
        for (k, g) in m.grid.iter().enumerate() {
            let p = g.p + g.n * LIFT_M;
            let uv = [
                (cu + inset + g.uv.x * (1.0 - 2.0 * inset)) / ATLAS_COLS as f32,
                (cv + inset + g.uv.y * (1.0 - 2.0 * inset)) / ATLAS_ROWS as f32,
            ];
            out[k] = (
                p.to_array(),
                g.n.to_array(),
                uv,
                [m.tint[0], m.tint[1], m.tint[2], a * g.a],
                [g.t.x, g.t.y, g.t.z, g.w],
            );
        }
        out
    }

    /// Copy the marks into `mesh`, in place, one pass over its attributes.
    pub fn write(&self, mesh: &mut Mesh) {
        let (mut pos, mut nrm, mut uv, mut col, mut tan) = (None, None, None, None, None);
        for (attr, values) in mesh.attributes_mut() {
            let id = attr.id;
            match values {
                VertexAttributeValues::Float32x3(v) => {
                    if id == Mesh::ATTRIBUTE_POSITION.id {
                        pos = Some(v);
                    } else if id == Mesh::ATTRIBUTE_NORMAL.id {
                        nrm = Some(v);
                    }
                }
                VertexAttributeValues::Float32x2(v) if id == Mesh::ATTRIBUTE_UV_0.id => uv = Some(v),
                VertexAttributeValues::Float32x4(v) => {
                    if id == Mesh::ATTRIBUTE_COLOR.id {
                        col = Some(v);
                    } else if id == Mesh::ATTRIBUTE_TANGENT.id {
                        tan = Some(v);
                    }
                }
                _ => {}
            }
        }
        let (Some(pos), Some(nrm), Some(uv), Some(col), Some(tan)) = (pos, nrm, uv, col, tan) else {
            return;
        };
        for i in 0..self.slots.len() {
            for (k, (p, n, t, c, g)) in self.vertices(i).into_iter().enumerate() {
                let at = i * VERTS + k;
                if at >= pos.len() {
                    return;
                }
                pos[at] = p;
                nrm[at] = n;
                uv[at] = t;
                col[at] = c;
                tan[at] = g;
            }
        }
    }
}

fn step(left: f32) -> i32 {
    ((left / FADE_S).min(1.0) * ALPHA_STEPS).ceil() as i32
}

/// `marks` collapsed grids and their fixed indices, counter-clockwise from
/// each mark's normal side.
pub fn grid_mesh(marks: usize) -> Mesh {
    let n = marks * VERTS;
    let g = MARK_GRID as u32;
    let mut idx = Vec::with_capacity(marks * (MARK_GRID - 1) * (MARK_GRID - 1) * 6);
    for m in 0..marks as u32 {
        let base = m * VERTS as u32;
        for j in 0..g - 1 {
            for i in 0..g - 1 {
                let a = base + j * g + i;
                let (b, c, d) = (a + 1, a + g, a + g + 1);
                idx.extend_from_slice(&[a, b, c, b, d, c]);
            }
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0f32, 0.0, 0.0, 1.0]; n]);
    mesh.insert_indices(Indices::U32(idx));
    mesh
}

/// The mark mesh, its material over the two atlases, always drawn
/// (collapsed patches until something is marked, so its pipeline compiles
/// at load and not on the first shot of a fight).
pub fn setup(
    commands: &mut Commands,
    marks: &mut Marks,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    marks.mesh = meshes.add(grid_mesh(marks.slots.len()));
    let (albedo, relief) = atlas_images();
    let mat = mats.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(images.add(albedo)),
        normal_map_texture: Some(images.add(relief)),
        // Dirt, chips and soot: never a highlight the surface under it
        // didn't have.
        perceptual_roughness: 0.9,
        metallic: 0.0,
        reflectance: crate::look::fresnel::DIELECTRIC,
        alpha_mode: AlphaMode::Blend,
        depth_bias: 8.0,
        // Both faces, but not `double_sided` (which flips the normal on a
        // back face): a patch a conform bent is lit by the normal it got.
        double_sided: false,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        Mesh3d(marks.mesh.clone()),
        MeshMaterial3d(mat),
        Transform::IDENTITY,
        Visibility::Visible,
        NoFrustumCulling,
        NotShadowCaster,
    ));
}

// ─── The atlas (Gates' texels) ──────────────────────────────────────────────

fn around(a: f32, k: f32, seed: u32) -> f32 {
    vnoise(a.cos() * k + 7.0, a.sin() * k + 7.0, seed)
}

/// A thin line's coverage: 1 on it, 0 past `w`.
fn line(d: f32, w: f32) -> f32 {
    smooth(w, w * 0.35, d.abs())
}

/// Radial rays: coverage of `n` jittered spokes whose length is rolled per
/// spoke and stretched along `grain` (1 = along ±u), tapering to their tips.
fn spokes(u: f32, v: f32, n: f32, len: f32, width: f32, grain: f32, seed: u32) -> f32 {
    let r = (u * u + v * v).sqrt();
    let a = v.atan2(u);
    let t = (a / core::f32::consts::TAU + 0.5) * n;
    let id = t.floor();
    let f = t - id;
    let jit = hash01(id as u32, seed);
    let l = len * (0.45 + 0.55 * hash01(id as u32, seed ^ 0x33)) * (1.0 - grain + grain * a.cos().abs());
    if r >= l {
        return 0.0;
    }
    let w = width * (1.0 - r / l);
    let centre = 0.3 + 0.4 * jit;
    smooth(w, w * 0.4, (f - centre).abs() * r.max(0.05) * 6.0)
}

/// Gates' cell for ours: its generator and its seed.
fn gates_cell(cell: u32) -> u32 {
    match cell {
        0 | 1 => cell,       // soil crater
        2..=5 => cell + 2,   // stone, metal holes
        6..=13 => cell + 2,  // chop, chip, dent, scuff
        14 | 15 => cell + 4, // scorch
        16..=19 => cell + 4, // blood
        _ => u32::MAX,
    }
}

/// Value, alpha and relief (a height in cell widths) of `cell` at
/// `(u, v) ∈ [-1, 1]²`.
fn texel(cell: u32, u: f32, v: f32) -> (f32, f32, f32) {
    let g = gates_cell(cell);
    let seed = 0x5EED_0000 ^ g.wrapping_mul(0x0101_0101);
    let r = (u * u + v * v).sqrt();
    let a = v.atan2(u);
    let n = fbm(u * 5.0, v * 5.0, seed);
    let (value, alpha, height) = match g {
        // A soil crater: a dark pit, the slope lighter, clods thrown out.
        0 | 1 => {
            let rn = r / (0.6 + 0.3 * (around(a, 1.6, seed) - 0.5));
            let pit = smooth(0.4, 0.0, rn);
            let body = 1.0 - smooth(0.5, 0.95, rn);
            let clods = smooth(0.62, 0.74, fbm(u * 9.0 + 3.0, v * 9.0, seed ^ 7))
                * smooth(0.6, 0.85, rn)
                * (1.0 - smooth(0.95, 1.35, rn));
            let rim = smooth(0.3, 0.5, rn) * (1.0 - smooth(0.5, 0.85, rn));
            (
                (0.7 - 0.55 * pit + 0.25 * (n - 0.5)).clamp(0.05, 1.0),
                (body * (0.7 + 0.3 * n)).max(clods * 0.9),
                -0.06 * (1.0 - smooth(0.0, 0.5, rn)) + 0.012 * rim + 0.015 * clods,
            )
        }
        // Stone: a dark pit in a pale chipped crater, cracks running out.
        4 | 5 => {
            let pit = smooth(0.14, 0.07, r / (0.8 + 0.4 * around(a, 2.5, seed)));
            let chip = 1.0 - smooth(0.26, 0.42, r / (0.75 + 0.5 * around(a, 2.0, seed ^ 2)));
            let cracks = spokes(u, v, 7.0, 0.95, 0.12, 0.0, seed ^ 5);
            let dust = (1.0 - smooth(0.3, 0.75, r)) * 0.3;
            let base = 0.9 - 0.2 * n;
            let value = if pit > 0.5 {
                0.08
            } else if cracks > chip {
                0.22
            } else {
                base
            };
            (
                value,
                pit.max(chip * 0.95).max(cracks * 0.9).max(dust),
                -0.07 * pit - 0.025 * chip * (1.0 - pit) - 0.006 * cracks,
            )
        }
        // Metal: a dark centre, a bright ring of bare metal, a scuffed halo.
        6 | 7 => {
            let hole = smooth(0.1, 0.06, r);
            let rim = 1.0 - smooth(0.17, 0.24, r / (0.9 + 0.2 * around(a, 3.0, seed)));
            let scratch = vnoise(a * 9.0, r * 2.0, seed ^ 4);
            let halo = (1.0 - smooth(0.24, 0.58, r)) * (0.35 + 0.65 * scratch);
            let value = if hole > 0.5 { 0.05 } else { 0.8 + 0.2 * rim };
            (
                value,
                hole.max(rim).max(halo * 0.75),
                -0.06 * hole + 0.012 * rim * (1.0 - hole) - 0.02 * (1.0 - smooth(0.0, 0.5, r)),
            )
        }
        // A cut: a notch whose upper face is the steep one, pale fresh
        // matter shadowed toward the groove, a lip of crushed surface and
        // flakes lifting off it, splinters off the lips (Gates' chop).
        8 | 9 => {
            let len = 0.86;
            let ue = u / len;
            let lean = 1.0 + 0.35 * ue;
            let span = ((1.0 - ue * ue) * lean).max(0.0);
            let rag = 0.75 + 0.25 * vnoise(u * 4.0, 1.7, seed) + 0.2 * vnoise(u * 17.0, v * 3.0, seed ^ 0x2b);
            let w = 0.19 * span.sqrt() * span * rag + 1e-4;
            let d = v - 0.05 * (vnoise(u * 2.5, 0.5, seed) - 0.5);
            let ad = d.abs();
            let ends = 1.0 - smooth(0.8, 0.92, u.abs());
            let inside = smooth(w, w * 0.8, ad) * ends;
            let face = (1.0 - ad / w).clamp(0.0, 1.0);
            let upper = d < 0.0;
            let depth = 0.075 * span.min(1.0);
            let h_cut = -depth * face.powf(if upper { 0.6 } else { 1.5 });
            let tear = fbm(u * 7.0, v * 7.0, seed ^ 0x51);
            let edge = smooth(w * 1.45, w * 1.05, ad) * (1.0 - inside) * ends;
            let flakes = smooth(w * (1.6 + 1.6 * tear), w * 1.2, ad) * smooth(0.45, 0.6, tear) * (1.0 - inside) * ends;
            let mut spl: f32 = 0.0;
            for k in 0..12u32 {
                let su = (hash01(k, seed) - 0.5) * 1.4;
                let se = su / len;
                let sspan = ((1.0 - se * se) * (1.0 + 0.35 * se)).max(0.0);
                let sw = 0.19 * sspan.sqrt() * sspan;
                let side = if hash01(k, seed ^ 0x5) < 0.5 { -1.0 } else { 1.0 };
                let l = 0.05 + 0.17 * hash01(k, seed ^ 0x9);
                let wid = 0.01 + 0.012 * hash01(k, seed ^ 0x13);
                let tilt = (hash01(k, seed ^ 0x21) - 0.5) * 0.6;
                let out = (v * side) - sw * 0.85;
                if !(0.0..=l).contains(&out) {
                    continue;
                }
                let taper = 1.0 - out / l;
                let bow = tilt * out + 0.6 * tilt * out * out / l;
                spl = spl.max(line(u - su - bow, wid * taper + 1e-4));
            }
            let chips = smooth(0.74, 0.8, fbm(u * 10.0, v * 10.0, seed ^ 3))
                * (1.0 - smooth(0.22, 0.5, ad))
                * ends
                * (1.0 - inside);
            let fibre = 0.74 + 0.26 * vnoise(u * 34.0, v * 2.5, seed ^ 9);
            let shade = if upper { 0.55 } else { 1.0 };
            let cut_v = fibre * shade * (1.0 - 0.6 * face.powi(4));
            let mut out_v = 0.2 + 0.1 * tear;
            out_v += (0.55 - out_v) * flakes;
            out_v += (0.9 - out_v) * spl.max(chips);
            (
                out_v + (cut_v - out_v) * inside,
                inside
                    .max(edge * 0.9)
                    .max(flakes * 0.85)
                    .max(spl * 0.95)
                    .max(chips * 0.75),
                h_cut * inside + 0.008 * edge + 0.012 * flakes + 0.01 * spl + 0.004 * chips,
            )
        }
        // A blow on stone: a shallow conchoidal scar, rippled and faceted
        // with a crushed white heart, cracks past it and flakes round it.
        10 | 11 => {
            let rn = r / (0.5 + 0.35 * (around(a, 2.2, seed) - 0.5) + 0.06 * (around(a, 7.0, seed ^ 0x77) - 0.5));
            let scar = 1.0 - smooth(0.9, 1.0, rn);
            let bowl = (1.0 - rn * rn).max(0.0);
            let ripple = 0.5 + 0.5 * (rn * 14.0 + 3.0 * vnoise(u * 3.0, v * 3.0, seed)).sin();
            let facet = ((a / core::f32::consts::TAU + 0.5) * 5.0 + 0.8 * vnoise(u * 2.0, v * 2.0, seed ^ 4)).floor();
            let tilt = hash01(facet as i32 as u32, seed ^ 0x3c) - 0.5;
            let grit = fbm(u * 15.0, v * 15.0, seed ^ 0x6d) - 0.5;
            let powder = 1.0 - smooth(0.0, 0.4, rn);
            let cracks = spokes(u, v, 6.0, 1.0, 0.07, 0.0, seed ^ 5) * smooth(0.55, 0.9, rn);
            let flake = smooth(0.62, 0.7, fbm(u * 6.0, v * 6.0, seed ^ 8))
                * smooth(0.8, 1.0, rn)
                * (1.0 - smooth(1.0, 1.35, rn));
            let inner = 0.74 + 0.26 * powder - 0.14 * (n - 0.5) - 0.07 * ripple;
            let value = if cracks > scar.max(flake) {
                0.18
            } else if flake > scar {
                0.86
            } else {
                inner
            };
            (
                value,
                (scar * (0.85 + 0.15 * n)).max(cracks * 0.85).max(flake * 0.7),
                (-0.03 * bowl - 0.016 - 0.002 * ripple + 0.006 * tilt * rn + 0.005 * grit) * scar - 0.008 * cracks
                    + 0.004 * flake,
            )
        }
        // A blow on metal: bright scratches across a darker dent.
        12 | 13 => {
            let (s, c) = (0.35f32).sin_cos();
            let (ru, rv) = (u * c - v * s, u * s + v * c);
            let mut scratch: f32 = 0.0;
            for k in 0..5 {
                let o = (hash01(k, seed) - 0.5) * 0.7;
                let l = 0.3 + 0.55 * hash01(k, seed ^ 0x77);
                let cover = line(rv - o, 0.025) * (1.0 - smooth(l * 0.8, l, ru.abs()));
                scratch = scratch.max(cover);
            }
            let dent = 1.0 - smooth(0.12, 0.3, (ru * ru * 0.5 + rv * rv * 2.0).sqrt());
            let value = if scratch > dent * 0.8 { 1.0 } else { 0.35 };
            (value, scratch.max(dent * 0.6), -0.03 * dent - 0.003 * scratch)
        }
        // A blow in the soil: a smear of turned earth.
        14 | 15 => {
            let rn = (u * u * 0.5 + v * v * 2.2).sqrt() / (0.7 + 0.25 * around(a, 1.8, seed));
            let body = 1.0 - smooth(0.6, 1.0, rn);
            let lumps = fbm(u * 6.0, v * 6.0, seed ^ 2) - 0.5;
            (0.6 - 0.3 * n, body * (0.55 + 0.45 * n), (0.03 * lumps - 0.012) * body)
        }
        // Scorch: soot, darkest at the centre, ragged at the edge.
        18 | 19 => {
            let rn = r / (0.82 + 0.3 * (around(a, 2.0, seed) - 0.5));
            let body = 1.0 - smooth(0.35, 1.0, rn);
            (0.25 + 0.5 * rn.min(1.0) * n, body * (0.6 + 0.4 * n), 0.0)
        }
        // Blood: a splat with satellite drops; the last variant a spray
        // along +u.
        20..=23 => {
            let spray = g == 23;
            let (su, sv) = if spray { (u * 0.55, v * 1.6) } else { (u, v) };
            let sr = (su * su + sv * sv).sqrt();
            let rn = sr / (0.4 + 0.22 * (around(sv.atan2(su), 2.4, seed) - 0.5));
            let body = 1.0 - smooth(0.8, 1.0, rn);
            let mut drops: f32 = 0.0;
            for k in 0..12 {
                let ang = hash01(k, seed) * core::f32::consts::TAU;
                let dist = 0.45 + 0.45 * hash01(k, seed ^ 0x11);
                let rad = 0.02 + 0.05 * hash01(k, seed ^ 0x22);
                let (cx, cy) = if spray {
                    (dist * ang.cos().signum() * 0.95, 0.12 * ang.sin())
                } else {
                    (dist * ang.cos(), dist * ang.sin())
                };
                let d = ((u - cx).powi(2) + (v - cy).powi(2)).sqrt();
                drops = drops.max(smooth(rad, rad * 0.6, d));
            }
            let wet = body.max(drops);
            (0.75 + 0.25 * n, wet * 0.92, 0.004 * wet)
        }
        _ => (0.0, 0.0, 0.0),
    };
    // Nothing reaches the cell's edge, so no mip bleeds a neighbour in.
    let pad = 1.0 - smooth(0.88, 0.97, u.abs().max(v.abs()));
    (value.clamp(0.0, 1.0), (alpha * pad).clamp(0.0, 1.0), height * pad)
}

/// Both atlases' level 0: value (sRGB, grey in RGB, coverage in A) and
/// relief (a linear tangent-space normal, +x along +u, +y toward −v).
pub fn atlas_maps() -> (Vec<u8>, Vec<u8>, u32, u32) {
    let (w, h) = (ATLAS_COLS * CELL_TEX, ATLAS_ROWS * CELL_TEX);
    let mut albedo = vec![0u8; (w * h * 4) as usize];
    let mut relief = vec![0u8; (w * h * 4) as usize];
    for px in relief.chunks_exact_mut(4) {
        px.copy_from_slice(&[128, 128, 255, 255]);
    }
    let used = Kind::ALL.iter().map(|k| k.cells().0 + k.cells().1).max().unwrap_or(0);
    let n = CELL_TEX as usize;
    let mut heights = vec![0.0f32; n * n];
    for cell in 0..used.min(ATLAS_COLS * ATLAS_ROWS) {
        let (cx, cy) = (cell % ATLAS_COLS, cell / ATLAS_COLS);
        for y in 0..CELL_TEX {
            for x in 0..CELL_TEX {
                let u = (x as f32 + 0.5) / CELL_TEX as f32 * 2.0 - 1.0;
                let v = (y as f32 + 0.5) / CELL_TEX as f32 * 2.0 - 1.0;
                let (val, a, ht) = texel(cell, u, v);
                heights[y as usize * n + x as usize] = ht;
                let i = (((cy * CELL_TEX + y) * w + cx * CELL_TEX + x) * 4) as usize;
                let g = (val * 255.0 + 0.5) as u8;
                albedo[i..i + 4].copy_from_slice(&[g, g, g, (a * 255.0 + 0.5) as u8]);
            }
        }
        // Slope per cell width: a central difference spans two texels.
        let k = CELL_TEX as f32 * 0.5;
        for y in 0..n {
            for x in 0..n {
                let at = |xx: usize, yy: usize| heights[yy.min(n - 1) * n + xx.min(n - 1)];
                let sx = (at(x + 1, y) - at(x.saturating_sub(1), y)) * k;
                let sy = (at(x, y + 1) - at(x, y.saturating_sub(1))) * k;
                let nv = Vec3::new(-sx, sy, 1.0).normalize();
                let i = (((cy as usize * n + y) * w as usize) + cx as usize * n + x) * 4;
                relief[i] = ((nv.x * 0.5 + 0.5) * 255.0 + 0.5) as u8;
                relief[i + 1] = ((nv.y * 0.5 + 0.5) * 255.0 + 0.5) as u8;
                relief[i + 2] = ((nv.z * 0.5 + 0.5) * 255.0 + 0.5) as u8;
            }
        }
    }
    (albedo, relief, w, h)
}

/// Both atlases, each with its mip chain: a mark at 40 m is a few texels and
/// must not shimmer.
fn atlas_images() -> (Image, Image) {
    let (albedo, relief, w, h) = atlas_maps();
    let image = |data: Vec<u8>, format: TextureFormat, normal: bool| {
        let mut img = Image::new(
            Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            format,
            RenderAssetUsages::RENDER_WORLD,
        );
        crate::look::textures::mip_in_place(&mut img, normal);
        img.sampler = ImageSampler::linear();
        img
    };
    (
        image(albedo, TextureFormat::Rgba8UnormSrgb, false),
        image(relief, TextureFormat::Rgba8Unorm, true),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::geom::{Aabb, V3};
    use sim::solid::Solid;

    fn map_with(solids: Vec<Solid>, boxes: Vec<Aabb>) -> Map {
        let mut m = Map::open();
        m.boxes = boxes;
        m.looks = vec![0; m.boxes.len()];
        m.solid_looks = vec![0; solids.len()];
        m.solids = solids;
        m.indexed()
    }

    /// A full pool recycles its oldest rather than refusing the newest,
    /// walking every slot.
    #[test]
    fn the_pool_is_bounded_and_recycles() {
        let mut pool = Marks::new(16);
        for _ in 0..16 {
            pool.place(Kind::HoleSoil, 0.2, Vec3::ZERO, Vec3::Y, Matter::Dirt);
        }
        assert_eq!(pool.live(), 16);
        let mut seen = [0u32; 16];
        for _ in 0..32 {
            seen[pool.claim()] += 1;
        }
        assert!(seen.iter().all(|&n| n == 2), "recycling walks the pool");
        pool.slots[7].left = 0.0;
        assert_eq!(pool.claim(), 7, "a free slot before a live one");
    }

    /// A mark lives, fades and frees its slot; blood goes sooner; a free
    /// slot is a collapsed patch.
    #[test]
    fn a_mark_fades_out_and_collapses() {
        let mut pool = Marks::new(4);
        let i = pool
            .place(
                Kind::HoleStone,
                0.14,
                Vec3::new(3.0, 1.0, 2.0),
                Vec3::Y,
                Matter::Concrete,
            )
            .unwrap();
        pool.fade(0.0);
        assert!(
            pool.vertices(i).iter().all(|v| (v.3[3] - 1.0).abs() < 1e-6),
            "a new mark is opaque"
        );
        pool.fade(LIFE_S + FADE_S * 0.5);
        let half = pool.vertices(i)[0].3[3];
        assert!(half > 0.3 && half < 0.7, "halfway through the fade: {half}");
        pool.fade(FADE_S);
        assert_eq!(pool.live(), 0);
        assert!(pool.vertices(i).iter().all(|v| v.0 == [0.0; 3] && v.3[3] == 0.0));
        let b = pool
            .place(Kind::Blood, 0.5, Vec3::ZERO, Vec3::Y, Matter::Flesh)
            .unwrap();
        pool.fade(BLOOD_LIFE_S + FADE_S + 0.1);
        assert!(
            pool.vertices(b).iter().all(|v| v.3[3] == 0.0),
            "blood goes before a hole would"
        );
    }

    /// Every triangle winds counter-clockwise seen from its normal, flat or
    /// bent, whatever its uv turn.
    #[test]
    fn a_mark_faces_its_normal() {
        let mesh = grid_mesh(8);
        let Some(Indices::U32(idx)) = mesh.indices() else {
            panic!("u32 indices");
        };
        let mut pool = Marks::new(8);
        for (n, bend) in [(Vec3::Y, 0.0), (Vec3::new(0.6, 0.0, 0.8), 0.4)] {
            for _ in 0..8 {
                let i = pool.put(Kind::Gash, 0.3, Vec3::ONE, n, Matter::Concrete, bend, None);
                let v = pool.vertices(i);
                let base = i * VERTS;
                let per = (MARK_GRID - 1) * (MARK_GRID - 1) * 6;
                for t in idx[i * per..(i + 1) * per].chunks_exact(3) {
                    let p = |k: u32| Vec3::from_array(v[k as usize - base].0);
                    let face = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0]));
                    let nv = Vec3::from_array(v[t[0] as usize - base].1);
                    assert!(face.dot(nv) > 0.0, "a back face toward the normal");
                }
            }
        }
    }

    /// On a round pillar a mark wraps its facets: every vertex that shows is
    /// a lift off the drawn surface, none hangs off the circle's tangent
    /// plane the way a flat quad would.
    #[test]
    fn a_mark_on_a_pillar_wraps_it() {
        let r = 0.4;
        let map = map_with(vec![Solid::from_def(0, [0.0, 0.0, r, 0.0, 0.0, 0.0, 0.0, 3.0])], vec![]);
        let mut pool = Marks::new(4);
        let i = pool
            .lay(&map, Kind::Blood, 0.5, Vec3::new(r, 1.5, 0.0), Vec3::X, Matter::Flesh)
            .expect("a pillar takes a mark");
        let mut shown = 0;
        for (p, _, _, c, _) in pool.vertices(i) {
            if c[3] < 0.05 {
                continue;
            }
            shown += 1;
            let d = Vec2::new(p[0], p[2]).length();
            assert!(d <= r + LIFT_M + 1e-3 && d >= r - 0.01, "off the pillar: {d}");
        }
        assert!(shown >= VERTS - 5, "most of it shows: {shown}");
    }

    /// Over a crate's edge the mark fades, not hangs: the vertices past it
    /// find nothing within reach.
    #[test]
    fn a_mark_fades_over_an_edge() {
        let map = map_with(
            vec![],
            vec![Aabb::new(V3::new(-1.0, 0.0, -1.0), V3::new(1.0, 1.0, 1.0))],
        );
        let mut pool = Marks::new(4);
        // On the top, a hand from its +x edge, big enough to overhang it.
        let i = pool
            .lay(
                &map,
                Kind::HoleStone,
                0.6,
                Vec3::new(0.95, 1.0, 0.0),
                Vec3::Y,
                Matter::Concrete,
            )
            .unwrap();
        for (p, _, _, c, _) in pool.vertices(i) {
            if p[0] > 1.02 {
                assert!(c[3] < 0.05, "a vertex past the edge shows: {p:?} {}", c[3]);
            } else if c[3] > 0.05 {
                assert!((p[1] - 1.0 - LIFT_M).abs() < 1e-3, "on the top: {p:?}");
            }
        }
    }

    /// Blood lands on the wall behind a body when one is near, on the floor
    /// behind them when not, and nowhere when they're high in the air.
    #[test]
    fn blood_lands_behind_the_body() {
        let wall = Aabb::new(V3::new(1.5, 0.0, -3.0), V3::new(2.0, 3.0, 3.0));
        let map = map_with(vec![], vec![wall]);
        let mut pool = Marks::new(8);
        let i = pool
            .bleed(&map, Vec3::new(0.5, 1.2, 0.0), Vec3::X)
            .expect("on the wall");
        let v = pool.vertices(i);
        assert!(v.iter().all(|v| v.1[0] < -0.9), "facing back along the blow");
        assert!(v.iter().filter(|v| v.3[3] > 0.05).all(|v| (v.0[0] - 1.5).abs() < 0.02));
        let i = pool
            .bleed(&map, Vec3::new(-5.0, 1.2, 0.0), Vec3::NEG_X)
            .expect("on the floor");
        let v = pool.vertices(i);
        assert!(v.iter().all(|v| v.1[1] > 0.99));
        let mid = Vec3::from_array(v[VERTS / 2].0);
        assert!(
            mid.x < -5.0 - BLOOD_BACK_M.0 * 0.5 && mid.y < 0.05,
            "behind them: {mid}"
        );
        assert!(pool.bleed(&map, Vec3::new(-5.0, 9.0, 0.0), Vec3::NEG_X).is_none());
    }

    /// A blow leaves the mark of what it hit and what hit it.
    #[test]
    fn a_blow_leaves_its_own_mark() {
        let k = |w, m| kind(w, m).map(|k| k.0);
        assert_eq!(k(Weapon::Bullet, Matter::Concrete), Some(Kind::HoleStone));
        assert_eq!(k(Weapon::Bullet, Matter::Metal), Some(Kind::HoleMetal));
        assert_eq!(k(Weapon::Pellet, Matter::Dirt), Some(Kind::HoleSoil));
        assert_eq!(k(Weapon::Melee, Matter::Concrete), Some(Kind::ChipStone));
        assert_eq!(k(Weapon::Melee, Matter::Metal), Some(Kind::DentMetal));
        assert_eq!(k(Weapon::Melee, Matter::Dirt), Some(Kind::ScuffSoil));
        assert_eq!(k(Weapon::Blade, Matter::Metal), Some(Kind::Gash));
        assert_eq!(k(Weapon::Blast, Matter::Dirt), Some(Kind::Scorch));
        for w in [Weapon::Bullet, Weapon::Melee, Weapon::Blade, Weapon::Blast] {
            for m in [Matter::Flesh, Matter::Shield, Matter::Hull] {
                assert!(kind(w, m).is_none(), "{w:?} marks {m:?} where it struck");
            }
        }
    }

    /// Every cell a kind uses is drawn and transparent at its border.
    #[test]
    fn every_atlas_cell_is_padded_and_drawn() {
        let (data, _, w, _) = atlas_maps();
        for k in Kind::ALL {
            let (first, n) = k.cells();
            for cell in first..first + n {
                assert!(cell < ATLAS_COLS * ATLAS_ROWS);
                let (cx, cy) = (cell % ATLAS_COLS, cell / ATLAS_COLS);
                let alpha = |x: u32, y: u32| data[(((cy * CELL_TEX + y) * w + cx * CELL_TEX + x) * 4 + 3) as usize];
                for i in 0..CELL_TEX {
                    for (x, y) in [(i, 0), (i, CELL_TEX - 1), (0, i), (CELL_TEX - 1, i)] {
                        assert_eq!(alpha(x, y), 0, "{k:?} cell {cell} bleeds at its edge");
                    }
                }
                let max = (0..CELL_TEX * CELL_TEX)
                    .map(|i| alpha(i % CELL_TEX, i / CELL_TEX))
                    .max()
                    .unwrap_or(0);
                assert!(max > 200, "{k:?} cell {cell} is empty (max alpha {max})");
            }
        }
    }
}

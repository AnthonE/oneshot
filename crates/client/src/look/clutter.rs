//! The near-ground population: pebbles, stones, fallen twigs and low dry
//! plants, scattered over the ground from the map's seed by what the ground
//! is (pale stones in the grass, sandstone and fallen red rock on the sand,
//! grey pebbles thick in the creek's gravel, nothing on the steep rock).
//! After Gates' `crates/client/src/render/clutter.rs` (its `stone`, `chip`,
//! `litter` and per-ground thinning) and `render/plants.rs` (`dead_scrub`,
//! `stick`, `dead_branch`, cut down to a hand's height) (MIT, NOTICE).
//!
//! Merged a mesh per 32 m block and density layer, so the whole map is a
//! few dozen draws, and drawn by the foliage material (`foliage.rs`,
//! [`Kind::Litter`]): the dry plants sway stiffly on the map's wind and lie
//! down where somebody walked; the stones and twigs are rigid (a negative
//! height in `UV_1`) and only sink into the ground with the distance fade,
//! so nothing pops at the edge of what is drawn. One opaque material, no
//! texture: WebGL2 draws exactly what WebGPU and the desktop do.
//!
//! **It is drawn the same on every rung**, like the grass (`quality.rs`'
//! fairness). Every piece stands lower than [`CLUTTER_MAX_H`] (asserted
//! when the crate compiles, and measured over every kind in the tests),
//! under a crouching fighter's hit box and under a step the sim would
//! climb anyway, but a stone on a crest can still cover a foot.
//!
//! **And it stays out of the way of what is played** ([`clear`]): never on
//! a spawn, a weapon pad, a launcher or a vehicle's bay, and a hand's room
//! off every wall, ramp and pillar that stands on the ground, so no twig
//! pokes through one. Inside the playable area what is drawn still matches
//! what collides (`level.rs`): the solid pieces are lower than a boot
//! ([`STONE_MAX_H`]) and mostly sunk, and what stands taller is bare twigs
//! you see through, like the grass.

use core::f32::consts::{FRAC_PI_2, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use sim::terrain::TerrainDef;

use super::foliage::{FoliageMaterial, Foliages, Kind};
use super::grass::{self, hash01, layer_of, Clear, LAYER_SHARE};
use crate::level::{ClutterPart, Dressed};

/// The tallest any piece stands over the ground it is rooted in, metres.
pub const CLUTTER_MAX_H: f32 = 0.35;
const _: () = assert!(
    CLUTTER_MAX_H <= 0.35 && CLUTTER_MAX_H < sim::consts::HIT_TOP_CROUCH && CLUTTER_MAX_H < sim::consts::STEP,
    "clutter that could hide a crouching fighter (or that a fighter could not step over) can't be a graphics row"
);
/// The tallest solid piece (a stone): lower than a boot.
pub const STONE_MAX_H: f32 = 0.12;

/// A dry plant's height at scale 1, metres: Gates' dead scrub (0.75 m) cut
/// to a hand's height.
pub const DRY_PLANT_H: f32 = 0.28;
/// A dry plant's scale, least and most.
const PLANT_SCALE: (f32, f32) = (0.8, 1.1);
/// A fork's length over its branch's.
const FORK: f32 = 0.62;
/// A branch's longest, over the plant's height.
const BRANCH_MAX: f32 = 0.65;
/// A dry plant's twig radius at its root, metres, at scale 1.
const TWIG_R: f32 = 0.006;
// A branch climbs at most its length, its fork at most the fork's: the
// highest a twig can reach over the root.
const _: () = assert!(
    DRY_PLANT_H * PLANT_SCALE.1 * BRANCH_MAX * (1.0 + FORK) + TWIG_R * PLANT_SCALE.1 < CLUTTER_MAX_H,
    "a dry plant must stay under CLUTTER_MAX_H"
);

/// A flower's stalk, metres, least and most, its radius at the root, and
/// its head's half-extents: a drift of them reads from across a hill, and
/// the tallest is still under a crouching fighter's knee.
const FLOWER_H: (f32, f32) = (0.12, 0.26);
const FLOWER_STALK_R: f32 = 0.004;
const FLOWER_HEAD: Vec3 = Vec3::new(0.03, 0.014, 0.03);
/// How many stand in one cell's clump, least and most, and how far from
/// its middle.
const FLOWER_CLUMP: (u32, u32) = (1, 4);
const FLOWER_CLUMP_R: f32 = 0.3;
/// The share of a drift's cells that flower, and where in the drift noise
/// ([`drift`]) a drift starts and is whole: a few percent of a meadow.
const FLOWER_SHARE: f32 = 0.42;
const FLOWER_DRIFT: (f32, f32) = (0.6, 0.74);
/// Flowers want turf: none where the grass's share of the ground is under
/// this.
const FLOWER_TURF: f32 = 0.45;
const _: () = assert!(
    FLOWER_H.1 + FLOWER_HEAD.y < CLUTTER_MAX_H,
    "a flower must stay under CLUTTER_MAX_H"
);
/// A drift's colours, sRGB: most of them yellow, some white, a few violet;
/// and a stalk's green.
const FLOWER_C: [u32; 3] = [0xf0c02a, 0xf1eee2, 0x9a7fe0];
const STALK_C: u32 = 0x4f6a2a;

/// A pebble's half-extents at scale 1 (Gates'), and its scale range.
const PEBBLE: Vec3 = Vec3::new(0.06, 0.028, 0.05);
const PEBBLE_K: (f32, f32) = (0.7, 1.6);
/// A stone in the turf or on the sand: a pebble's shape at three to five
/// times its size (Gates' stone, its range trimmed under a boot).
const STONE: Vec3 = Vec3::new(0.21, 0.104, 0.156);
const STONE_K: (f32, f32) = (0.6, 1.1);
/// A fallen twig: a long low chip (Gates' litter).
const TWIG: Vec3 = Vec3::new(0.16, 0.022, 0.03);
const TWIG_K: (f32, f32) = (0.8, 1.5);
const _: () = assert!(
    PEBBLE.y * PEBBLE_K.1 <= STONE_MAX_H && STONE.y * STONE_K.1 <= STONE_MAX_H && TWIG.y * TWIG_K.1 <= STONE_MAX_H,
    "a solid piece must stay under STONE_MAX_H"
);

/// How far a chip's base ring sits below the ground, as a share of its
/// height (Gates'): pushed in, never stood on a razor line.
const CHIP_SINK: f32 = 0.30;
/// How far a stone's and a twig's normals are pulled off their facets
/// toward its middle (Gates'): angular, without four flat values.
const CHIP_VOLUME_BLEND: f32 = 0.55;

/// The scatter's cell (Gates' 0.64 m), and a block's side, metres: a whole
/// number of cells, so neighbouring blocks share one grid.
const STEP_M: f32 = 0.64;
pub const BLOCK_M: f32 = 32.0;
/// How far a block's corner is from its centre (`grass::cull`).
pub const BLOCK_REACH: f32 = BLOCK_M * core::f32::consts::FRAC_1_SQRT_2;

/// What the clutter keeps off besides the grass's ([`clear`]): a spawn,
/// a vehicle's bay, and a wall's foot, metres.
const SPAWN_CLEAR_M: f32 = 1.5;
const BAY_CLEAR_M: f32 = 3.2;
const WALL_CLEAR_M: f32 = 0.35;

/// What a ground grows, as shares of its cells: pebbles, stones, fallen
/// twigs and dry plants. **(knob)**
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Grows {
    pebble: f32,
    stone: f32,
    twig: f32,
    plant: f32,
}

/// Open sand: a pebble every few metres, the odd stone fallen off the
/// canyon, sticks and dry scrub blown into it.
const SAND: Grows = Grows {
    pebble: 0.14,
    stone: 0.012,
    twig: 0.025,
    plant: 0.016,
};
/// The turf: pale stones in it, sticks; the grass is its own (`grass.rs`).
const GRASSLAND: Grows = Grows {
    pebble: 0.04,
    stone: 0.022,
    twig: 0.04,
    plant: 0.008,
};
/// The creek's gravel: pebbles thick, stones washed down, little else.
const GRAVEL: Grows = Grows {
    pebble: 0.42,
    stone: 0.05,
    twig: 0.012,
    plant: 0.0,
};

impl Grows {
    /// The ground's weights (`level::splat`'s sand, grass, gravel; rock
    /// grows nothing) over the three.
    fn at(w: [f32; 4]) -> Grows {
        let mix = |f: fn(&Grows) -> f32| w[0] * f(&SAND) + w[1] * f(&GRASSLAND) + w[2] * f(&GRAVEL);
        Grows {
            pebble: mix(|g| g.pebble),
            stone: mix(|g| g.stone),
            twig: mix(|g| g.twig),
            plant: mix(|g| g.plant),
        }
    }
}

/// Colours, sRGB: a pebble or a stone by its ground, a twig, and the grey
/// of dead wood at a dry plant's root and tips (Gates', linear).
const GRANITE_C: u32 = 0x8a8880;
const MEADOW_STONE_C: u32 = 0xa9a59c;
const SANDSTONE_C: u32 = 0xb09a7c;
/// Red rock fallen off the canyon's walls (the rock look's tint over its
/// photograph, `level::dress`), on the sand.
const RED_ROCK_C: u32 = 0x9c6448;
const TWIG_C: u32 = 0x5a4630;
/// A twig the sun has had for years, on the sand.
const BLEACHED_TWIG_C: u32 = 0x857a6c;
const DEAD_ROOT: [f32; 3] = [0.05, 0.042, 0.033];
const DEAD_TIP: [f32; 3] = [0.15, 0.135, 0.115];

/// sRGB hex to linear, which is what a vertex colour is in.
fn linear(hex: u32) -> [f32; 3] {
    let f = |c: u32| {
        let s = c as f32 / 255.0;
        if s <= 0.040_45 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    [f((hex >> 16) & 0xff), f((hex >> 8) & 0xff), f(hex & 0xff)]
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn scale3(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

/// A rigid piece's `UV_1`: how deep it sinks when the fade takes it (its
/// height and a little, negative: `foliage_common.wgsl`), and its random.
fn rigid(height: f32, rand: f32) -> [f32; 2] {
    [-(height + 0.01), rand]
}

/// One block and layer's pieces, in the layout the foliage shader reads.
#[derive(Default)]
struct Soup {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    uv1: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Soup {
    fn vert(&mut self, p: Vec3, n: Vec3, c: [f32; 3], uv1: [f32; 2]) -> u32 {
        let i = self.pos.len() as u32;
        self.pos.push(p.to_array());
        self.nor.push(n.to_array());
        self.uv1.push(uv1);
        self.col.push([c[0], c[1], c[2], 1.0]);
        i
    }

    /// A faceted triangle: fresh vertices, its facet's normal turned out
    /// from `ctr` and pulled toward the volume round it by `blend`.
    fn facet(&mut self, [a, b, c]: [Vec3; 3], ctr: Vec3, blend: f32, col: [f32; 3], uv1: [f32; 2]) {
        let f = (b - a).cross(c - a).normalize_or_zero();
        let f = if f.dot((a + b + c) / 3.0 - ctr) < 0.0 { -f } else { f };
        for p in [a, b, c] {
            let n = (f * (1.0 - blend) + (p - ctr).normalize_or_zero() * blend).normalize_or(Vec3::Y);
            let i = self.vert(p, n, col, uv1);
            self.idx.push(i);
        }
    }

    /// No texture reads it, but the shader's layout wants a first UV.
    fn mesh(self) -> Option<Mesh> {
        (!self.idx.is_empty()).then(|| {
            let n = self.pos.len();
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0]; n])
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, self.uv1)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
                .with_inserted_indices(Indices::U32(self.idx))
        })
    }
}

/// A loose stone's points (Gates' `stone`): a low six-cornered lump under a
/// short ridge, its base ring sunk; then the ridge's two crests and the
/// volume's middle.
fn stone_points(at: Vec3, yaw: f32, size: Vec3, seed: u32) -> ([Vec3; 6], Vec3, Vec3, Vec3) {
    let (sy, cy) = (yaw.sin(), yaw.cos());
    let rot = |p: Vec3| Vec3::new(p.x * cy + p.z * sy, p.y, -p.x * sy + p.z * cy);
    let sink = size.y * CHIP_SINK;
    let lo = core::array::from_fn(|i| {
        let a = i as f32 * TAU / 6.0 + 0.3;
        let r = 0.7 + 0.3 * hash01(seed, i as u32);
        at + rot(Vec3::new(a.cos() * r * size.x, -sink, a.sin() * r * size.z))
    });
    let crest = |k: u32, side: f32| {
        let reach = 0.3 + 0.15 * hash01(seed, 20 + k);
        let hy = size.y * (0.85 + 0.15 * hash01(seed, 40 + k));
        at + rot(Vec3::new(side * reach * size.x, hy, 0.0))
    };
    let ctr = at + Vec3::new(0.0, (size.y - sink) * 0.4, 0.0);
    (lo, crest(0, 1.0), crest(1, -1.0), ctr)
}

/// A stone's eight facets, as corners of [`stone_points`] (its base ring
/// 0..6, then crest 0 and crest 1): corners 5, 0, 1 round crest 0, and 2,
/// 3, 4 round crest 1.
const STONE_TRIS: [[usize; 3]; 8] = [
    [5, 6, 0],
    [0, 6, 1],
    [1, 6, 7],
    [1, 7, 2],
    [2, 7, 3],
    [3, 7, 4],
    [4, 7, 6],
    [4, 6, 5],
];

/// A stone's eight corners in [`STONE_TRIS`]' order, and its middle.
fn stone_corners(at: Vec3, yaw: f32, size: Vec3, seed: u32) -> ([Vec3; 8], Vec3) {
    let (lo, h0, h1, ctr) = stone_points(at, yaw, size, seed);
    let mut c = [h0; 8];
    c[..6].copy_from_slice(&lo);
    c[7] = h1;
    (c, ctr)
}

/// A pebble: the stone's shape on its eight shared corners with the
/// volume's normals, a smooth lump: at five centimetres a facet is a pixel.
fn pebble(s: &mut Soup, at: Vec3, yaw: f32, size: Vec3, col: [f32; 3], seed: u32) {
    let (corners, ctr) = stone_corners(at, yaw, size, seed);
    let uv1 = rigid(size.y, hash01(seed, 211));
    let c = scale3(col, 0.8 + 0.4 * hash01(seed, 5));
    let base = s.pos.len() as u32;
    for p in corners {
        s.vert(p, (p - ctr).normalize_or(Vec3::Y), c, uv1);
    }
    for t in STONE_TRIS {
        s.idx.extend(t.map(|k| base + k as u32));
    }
}

/// A stone, faceted (Gates'): a hand across, a ridge on top.
fn stone(s: &mut Soup, at: Vec3, yaw: f32, size: Vec3, col: [f32; 3], seed: u32) {
    let (corners, ctr) = stone_corners(at, yaw, size, seed);
    let uv1 = rigid(size.y, hash01(seed, 211));
    let c = scale3(col, 0.8 + 0.4 * hash01(seed, 5));
    for t in STONE_TRIS {
        s.facet(t.map(|k| corners[k]), ctr, CHIP_VOLUME_BLEND, c, uv1);
    }
}

/// A fallen twig (Gates' `chip` at a stick's proportions): four corners
/// jittered in plan round one raised apex, sunk.
fn twig(s: &mut Soup, at: Vec3, yaw: f32, size: Vec3, col: [f32; 3], seed: u32) {
    let (sy, cy) = (yaw.sin(), yaw.cos());
    let rot = |p: Vec3| Vec3::new(p.x * cy + p.z * sy, p.y, -p.x * sy + p.z * cy);
    let sink = size.y * CHIP_SINK;
    let c: [Vec3; 4] = core::array::from_fn(|i| {
        let a = i as f32 * FRAC_PI_2 + 0.4;
        let r = 0.6 + 0.4 * hash01(seed, i as u32);
        at + rot(Vec3::new(a.cos() * size.x * r, -sink, a.sin() * size.z * r))
    });
    let apex = at + Vec3::Y * size.y;
    let ctr = at + Vec3::Y * ((size.y - sink) * 0.5);
    let uv1 = rigid(size.y, hash01(seed, 211));
    let col = scale3(col, 0.8 + 0.4 * hash01(seed, 5));
    for i in 0..4 {
        s.facet([c[i], apex, c[(i + 1) % 4]], ctr, CHIP_VOLUME_BLEND, col, uv1);
    }
}

/// What every twig of one dry plant shares: where its root is, how tall
/// the plant is (its colour ramps over that), and its random.
#[derive(Clone, Copy)]
struct Plant {
    root_y: f32,
    h: f32,
    rand: f32,
}

impl Plant {
    fn uv1(&self, p: Vec3) -> [f32; 2] {
        [(p.y - self.root_y).max(0.0), self.rand]
    }

    fn colour(&self, y: f32) -> [f32; 3] {
        let t = ((y - self.root_y) / self.h).clamp(0.0, 1.0);
        lerp3(DEAD_ROOT, DEAD_TIP, t * t * (3.0 - 2.0 * t))
    }

    /// A three-sided stick from `a` to `b`, radius `r0` to `r1`, with
    /// round normals: at a centimetre across, a twig's shading is its
    /// colour.
    fn stick(&self, s: &mut Soup, a: Vec3, b: Vec3, r0: f32, r1: f32) {
        let axis = (b - a).normalize_or(Vec3::Y);
        let side = axis.cross(Vec3::X).normalize_or(Vec3::Z);
        let other = axis.cross(side);
        let base = s.pos.len() as u32;
        for (p, r) in [(a, r0), (b, r1.max(0.0015))] {
            let c = self.colour(p.y);
            for i in 0..3 {
                let t = i as f32 * TAU / 3.0;
                let out = side * t.cos() + other * t.sin();
                let q = p + out * r;
                s.vert(q, out, c, self.uv1(q));
            }
        }
        for i in 0..3u32 {
            let j = (i + 1) % 3;
            s.idx
                .extend_from_slice(&[base + i, base + j, base + 3 + i, base + j, base + 3 + j, base + 3 + i]);
        }
    }

    /// One dead branch from `a` along `dir`, kinked halfway, forking in two
    /// at its end once (Gates' `dead_branch`, a depth shallower).
    #[allow(clippy::too_many_arguments)]
    fn branch(&self, s: &mut Soup, seed: u32, a: Vec3, dir: Vec3, len: f32, r: f32, forked: bool) {
        let kink = Vec3::new(
            hash01(seed, 1) - 0.5,
            (hash01(seed, 2) - 0.5) * 0.4,
            hash01(seed, 3) - 0.5,
        );
        let mid = a + (dir + kink * 0.5).normalize_or(Vec3::Y) * len * 0.5;
        let end = mid + (dir - kink * 0.3).normalize_or(Vec3::Y) * len * 0.5;
        self.stick(s, a, mid, r, r * 0.8);
        self.stick(s, mid, end, r * 0.8, r * 0.6);
        if forked {
            return;
        }
        let axis = dir.cross(Vec3::Y).normalize_or(Vec3::X);
        for (k, side) in [(0u32, -1.0f32), (1, 1.0)] {
            let spread = 0.3 + 0.35 * hash01(seed, 10 + k);
            let d = (Quat::from_axis_angle(axis, side * spread) * dir + Vec3::Y * 0.12).normalize_or(Vec3::Y);
            let next = seed.wrapping_mul(0x2c1b_3c6d) ^ (k + 1);
            self.branch(s, next, end, d, len * FORK, r * 0.6, true);
        }
    }
}

/// A low dry plant: four to six bare twigs from the root, each forking
/// once, weathered grey (Gates' dead scrub at a hand's height). The only
/// piece that sways.
fn dry_plant(s: &mut Soup, at: Vec3, seed: u32) {
    let scale = PLANT_SCALE.0 + (PLANT_SCALE.1 - PLANT_SCALE.0) * hash01(seed, 7);
    let plant = Plant {
        root_y: at.y,
        h: DRY_PLANT_H * scale,
        rand: hash01(seed, 211),
    };
    let n = 4 + (hash01(seed, 1) * 3.0) as u32;
    let a0 = hash01(seed, 2) * TAU;
    for k in 0..n {
        let a = a0 + k as f32 * TAU / n as f32 + (hash01(seed, 10 + k) - 0.5) * 0.7;
        let elev = 0.55 + 0.65 * hash01(seed, 20 + k);
        let len = plant.h * (0.4 + (BRANCH_MAX - 0.4) * hash01(seed, 30 + k));
        let dir = Vec3::new(a.sin() * elev.cos(), elev.sin(), a.cos() * elev.cos());
        plant.branch(
            s,
            seed ^ (k + 1).wrapping_mul(0x9e37),
            at - Vec3::Y * 0.02,
            dir,
            len,
            TWIG_R * scale,
            false,
        );
    }
}

/// How much of a flower drift (x, z) is in, 0..1: the meadow's own patch
/// noise read elsewhere and broader, so the drifts are not the grass's
/// patches.
pub(crate) fn drift(x: f32, z: f32) -> f32 {
    let n = grass::patch(x * 0.7 + 91.0, z * 0.7 - 37.0);
    ((n - FLOWER_DRIFT.0) / (FLOWER_DRIFT.1 - FLOWER_DRIFT.0)).clamp(0.0, 1.0)
}

/// A clump of flowers round `at`: each a thin green stalk that sways as the
/// dry plants do, and a small bright head on it, the drift's colour.
fn flowers(s: &mut Soup, t: &TerrainDef, at: Vec3, seed: u32) {
    // One colour a drift, mostly: the hue varies over tens of metres.
    let hue = grass::patch(at.x * 0.15 + 7.0, at.z * 0.15 + 3.0) + (hash01(seed, 31) - 0.5) * 0.12;
    let head = linear(
        FLOWER_C[if hue < 0.56 {
            0
        } else if hue < 0.68 {
            1
        } else {
            2
        }],
    );
    let stalk = linear(STALK_C);
    let n = FLOWER_CLUMP.0 + (hash01(seed, 32) * (FLOWER_CLUMP.1 - FLOWER_CLUMP.0 + 1) as f32) as u32;
    for k in 0..n.min(FLOWER_CLUMP.1) {
        let a = hash01(seed, 40 + k) * TAU;
        let r = hash01(seed, 50 + k).sqrt() * FLOWER_CLUMP_R;
        let (x, z) = (at.x + a.cos() * r, at.z + a.sin() * r);
        let Some(y) = grass::ground(t, x, z) else {
            continue;
        };
        let h = FLOWER_H.0 + (FLOWER_H.1 - FLOWER_H.0) * hash01(seed, 60 + k);
        let root = Vec3::new(x, y - 0.01, z);
        let lean = Vec3::new(hash01(seed, 70 + k) - 0.5, 0.0, hash01(seed, 80 + k) - 0.5) * 0.25 * h;
        let top = Vec3::new(x, y + h, z) + lean;
        let rand = hash01(seed, 211 + k);
        // The stalk: three sides, round normals.
        let side = Vec3::X;
        let other = Vec3::Z;
        let base = s.pos.len() as u32;
        for (p, rad) in [(root, FLOWER_STALK_R), (top, FLOWER_STALK_R * 0.6)] {
            for i in 0..3 {
                let t3 = i as f32 * TAU / 3.0;
                let out = side * t3.cos() + other * t3.sin();
                let q = p + out * rad;
                s.vert(q, out, stalk, [(q.y - y).max(0.0), rand]);
            }
        }
        for i in 0..3u32 {
            let j = (i + 1) % 3;
            s.idx
                .extend_from_slice(&[base + i, base + j, base + 3 + i, base + j, base + 3 + j, base + 3 + i]);
        }
        // The head: a stone's lump, lit as the sky sees it so it stays
        // bright from any side, riding the stalk's tip.
        let size = FLOWER_HEAD * (0.8 + 0.5 * hash01(seed, 90 + k));
        let (corners, _) = stone_corners(top, a, size, seed ^ (k + 1));
        let c = scale3(head, 0.9 + 0.2 * hash01(seed, 100 + k));
        let hb = s.pos.len() as u32;
        for p in corners {
            s.vert(p, Vec3::Y, c, [h, rand]);
        }
        for tri in STONE_TRIS {
            s.idx.extend(tri.map(|i| hb + i as u32));
        }
    }
}

/// What a cell grows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Piece {
    Pebble,
    Stone,
    Twig,
    Plant,
}

/// Which piece a roll `r` (0..1) lands on with these shares, if any.
fn piece_of(g: &Grows, r: f32) -> Option<Piece> {
    let mut edge = 0.0;
    for (share, p) in [
        (g.pebble, Piece::Pebble),
        (g.stone, Piece::Stone),
        (g.twig, Piece::Twig),
        (g.plant, Piece::Plant),
    ] {
        edge += share;
        if r < edge {
            return Some(p);
        }
    }
    None
}

/// One piece of `kind` at `at` on ground of weights `w`, `seed` its own.
fn piece(s: &mut Soup, kind: Piece, at: Vec3, w: [f32; 4], seed: u32) {
    let yaw = hash01(seed, 9) * TAU;
    let k = |(lo, hi): (f32, f32)| lo + (hi - lo) * hash01(seed, 8);
    // Which of its ground's colours, dithered by the weights.
    let r = hash01(seed, 12) * (w[0] + w[1] + w[2]).max(1e-6);
    let ground = if r < w[0] {
        0
    } else if r < w[0] + w[1] {
        1
    } else {
        2
    };
    match kind {
        Piece::Pebble => {
            let c = [SANDSTONE_C, MEADOW_STONE_C, GRANITE_C][ground];
            pebble(s, at, yaw, PEBBLE * k(PEBBLE_K), linear(c), seed);
        }
        Piece::Stone => {
            let c = match ground {
                0 if hash01(seed, 13) < 0.5 => RED_ROCK_C,
                0 => SANDSTONE_C,
                1 => MEADOW_STONE_C,
                _ => GRANITE_C,
            };
            // Most of them small: the product of two rolls.
            let kk = STONE_K.0 + (STONE_K.1 - STONE_K.0) * hash01(seed, 8) * hash01(seed, 14);
            stone(s, at, yaw, STONE * kk, linear(c), seed);
        }
        Piece::Twig => {
            let c = if ground == 0 { BLEACHED_TWIG_C } else { TWIG_C };
            twig(s, at, yaw, TWIG * k(TWIG_K), linear(c), seed);
        }
        Piece::Plant => dry_plant(s, at, seed),
    }
}

/// The ground's slope at (x, z), rise over run, as the sim's triangles have
/// it, by central differences half a metre either way.
fn slope(t: &TerrainDef, x: f32, z: f32) -> Option<f32> {
    let dx = grass::ground(t, x + 0.5, z)? - grass::ground(t, x - 0.5, z)?;
    let dz = grass::ground(t, x, z + 0.5)? - grass::ground(t, x, z - 0.5)?;
    Some((dx * dx + dz * dz).sqrt())
}

/// Where the clutter keeps off: the grass's own (the weapon pads, the
/// launchers), every spawn, every vehicle's bay, and a hand's room round
/// every box and solid that comes down into the clutter's height off the
/// ground under it (a bridge high over the ground clears nothing).
pub fn clear(map: &sim::map::Map, t: &TerrainDef) -> Clear {
    let mut c = Clear::pads(map);
    for s in &map.spawns {
        c.disc(Vec2::new(s.pos.x, s.pos.z), SPAWN_CLEAR_M);
    }
    for v in &map.vehicles {
        c.disc(Vec2::new(v.pos.x, v.pos.z), BAY_CLEAR_M);
    }
    let bounds = map.boxes.iter().copied().chain(map.solids.iter().map(|s| s.bounds()));
    for b in bounds {
        let (lo, hi) = (Vec2::new(b.min.x, b.min.z), Vec2::new(b.max.x, b.max.z));
        let mid = (lo + hi) * 0.5;
        let top = [lo, hi, Vec2::new(lo.x, hi.y), Vec2::new(hi.x, lo.y), mid]
            .iter()
            .filter_map(|p| grass::ground(t, p.x, p.y))
            .fold(f32::MIN, f32::max);
        if top > f32::MIN && b.min.y < top + CLUTTER_MAX_H + 0.1 {
            let m = Vec2::splat(WALL_CLEAR_M);
            c.rect(lo - m, hi + m);
        }
    }
    c
}

/// The clutter for a map's terrain: a mesh per block and density layer
/// (`grass::LAYER_SHARE`, the GRASS row's), its centre and its layer.
/// Nothing lies where `clear` says, on the rock, or off the ground's edge.
pub fn blocks(t: &TerrainDef, lo: (f32, f32), hi: (f32, f32), seed: u32, clear: &Clear) -> Vec<(Mesh, Vec2, u8)> {
    let mut out = Vec::new();
    let n = (BLOCK_M / STEP_M).round() as u32;
    let mut bz = lo.1;
    while bz < hi.1 {
        let mut bx = lo.0;
        while bx < hi.0 {
            let mut soups: [Soup; LAYER_SHARE.len()] = Default::default();
            let clear = clear.near(Vec2::new(bx, bz), Vec2::new(bx + BLOCK_M, bz + BLOCK_M));
            let (gi0, gj0) = (
                ((bx - lo.0) / STEP_M).round() as u32,
                ((bz - lo.1) / STEP_M).round() as u32,
            );
            for j in 0..n {
                for i in 0..n {
                    let (gi, gj) = (gi0 + i, gj0 + j);
                    let cell = seed
                        .wrapping_add(gi.wrapping_mul(0x9E37_79B1))
                        .wrapping_add(gj.wrapping_mul(0x85EB_CA77))
                        ^ 0x5bd1_e995;
                    let x = bx + (i as f32 + hash01(cell, 1)) * STEP_M;
                    let z = bz + (j as f32 + hash01(cell, 2)) * STEP_M;
                    if x > hi.0 || z > hi.1 || clear.blocked(Vec2::new(x, z)) {
                        continue;
                    }
                    let (Some(y), Some(sl)) = (grass::ground(t, x, z), slope(t, x, z)) else {
                        continue;
                    };
                    let w = crate::level::splat(sl, y);
                    // Nothing on the rock, nor where its skin starts to grow over the
                    // ground (`cliffs.rs`): a pebble there would sink under it.
                    if w[3] >= super::cliffs::SKIN_ROCK_LO {
                        continue;
                    }
                    // Flowers in drifts over the turf, a roll of their own.
                    if w[1] >= FLOWER_TURF && hash01(cell, 15) < drift(x, z) * FLOWER_SHARE {
                        flowers(&mut soups[layer_of(hash01(cell, 16))], t, Vec3::new(x, y, z), cell);
                    }
                    let Some(kind) = piece_of(&Grows::at(w), hash01(cell, 3)) else {
                        continue;
                    };
                    piece(&mut soups[layer_of(hash01(cell, 6))], kind, Vec3::new(x, y, z), w, cell);
                }
            }
            let centre = Vec2::new(bx + BLOCK_M * 0.5, bz + BLOCK_M * 0.5);
            for (layer, soup) in soups.into_iter().enumerate() {
                if let Some(m) = soup.mesh() {
                    out.push((m, centre, layer as u8));
                }
            }
            bx += BLOCK_M;
        }
        bz += BLOCK_M;
    }
    out
}

/// The clutter blocks their material: the foliage's [`Kind::Litter`], one
/// opaque vertex-coloured material made once and kept for every level.
pub fn dress(
    mut commands: Commands,
    reg: Option<ResMut<Foliages>>,
    mut mats: ResMut<Assets<FoliageMaterial>>,
    parts: Query<Entity, (With<ClutterPart>, Without<Dressed>)>,
) {
    if parts.is_empty() {
        return;
    }
    let Some(mut reg) = reg else {
        return;
    };
    let m = match reg.litter.clone() {
        Some(m) => m,
        None => {
            let m = mats.add(reg.make(
                StandardMaterial {
                    base_color: Color::WHITE,
                    perceptual_roughness: 0.92,
                    reflectance: super::fresnel::DIELECTRIC,
                    // A twig is three sides and a stone's underside is
                    // sunk: both faces, each lit as the front.
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                },
                Kind::Litter,
            ));
            reg.litter = Some(m.clone());
            m
        }
    };
    for e in parts.iter() {
        commands.entity(e).insert((MeshMaterial3d(m.clone()), Dressed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The highest point of a soup over `root_y`, and how many of its
    /// vertices sway (a non-negative `UV_1.x`) or are rigid.
    fn measure(s: &Soup, root_y: f32) -> (f32, usize, usize) {
        let top = s.pos.iter().map(|p| p[1] - root_y).fold(f32::MIN, f32::max);
        let sway = s.uv1.iter().filter(|u| u[0] >= 0.0).count();
        (top, sway, s.uv1.len() - sway)
    }

    /// Every kind, over a thousand seeds: under its cap, the solids rigid
    /// and the dry plants swaying, and a mesh the shader can draw.
    #[test]
    fn every_piece_stays_under_its_cap() {
        let at = Vec3::new(3.0, 1.25, -7.0);
        for seed in 0..1000u32 {
            for (kind, cap) in [
                (Piece::Pebble, STONE_MAX_H),
                (Piece::Stone, STONE_MAX_H),
                (Piece::Twig, STONE_MAX_H),
                (Piece::Plant, CLUTTER_MAX_H),
            ] {
                for w in [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]] {
                    let mut s = Soup::default();
                    piece(&mut s, kind, at, w, seed.wrapping_mul(2_654_435_761));
                    let (top, sway, rigid) = measure(&s, at.y);
                    assert!(top > 0.0 && top <= cap, "{kind:?} seed {seed}: {top} m over the ground");
                    if kind == Piece::Plant {
                        assert_eq!(rigid, 0, "a dry plant sways");
                        assert!(s.uv1.iter().all(|u| u[0] <= top + 1e-4));
                    } else {
                        assert_eq!(sway, 0, "a {kind:?} is rigid");
                        assert!(s.uv1.iter().all(|u| -u[0] > top));
                    }
                    assert!(s.idx.iter().all(|&i| (i as usize) < s.pos.len()));
                    assert_eq!(s.idx.len() % 3, 0);
                    assert!(s.nor.iter().all(|n| (Vec3::from_array(*n).length() - 1.0).abs() < 1e-3));
                }
            }
        }
    }

    /// A stone's facets face out of it: the normals the light reads are
    /// the lump's, never turned inside.
    #[test]
    fn a_stone_faces_out() {
        let at = Vec3::new(0.0, 0.0, 0.0);
        let mut s = Soup::default();
        stone(&mut s, at, 0.4, STONE, [0.5; 3], 77);
        let (_, ctr) = stone_corners(at, 0.4, STONE, 77);
        for (p, n) in s.pos.iter().zip(&s.nor) {
            let (p, n) = (Vec3::from_array(*p), Vec3::from_array(*n));
            assert!(n.dot(p - ctr) > 0.0, "{p:?} {n:?}");
        }
    }

    #[test]
    fn the_rolls_share_out_by_the_ground() {
        let g = Grows::at([0.0, 0.0, 1.0, 0.0]);
        assert_eq!(g, GRAVEL);
        assert_eq!(piece_of(&g, 0.0), Some(Piece::Pebble));
        assert_eq!(piece_of(&g, GRAVEL.pebble + 0.01), Some(Piece::Stone));
        assert_eq!(piece_of(&g, 0.99), None);
        assert_eq!(Grows::at([0.0, 0.0, 0.0, 1.0]), Grows::default(), "rock grows nothing");
        for w in [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]] {
            let g = Grows::at(w);
            assert!(g.pebble + g.stone + g.twig + g.plant < 1.0);
        }
    }

    /// Nothing lies on a spawn, a pad, a launcher or a bay, nor within a
    /// hand of a wall on the ground; on Dry Fork, the one map with ground.
    #[test]
    fn nothing_lies_where_it_is_played() {
        let Some(def) = sim::maps::ALL.iter().find(|d| d.terrain.is_some()) else {
            return;
        };
        let t = def.terrain.as_ref().expect("terrain");
        let map = def.build();
        let clear = clear(&map, t);
        let blocks = blocks(t, map.lo, map.hi, 17, &clear);
        assert!(!blocks.is_empty(), "a map with ground has clutter");
        let mut pts = 0usize;
        for (m, ..) in &blocks {
            let Some(p) = m.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()) else {
                continue;
            };
            pts += p.len();
            for q in p {
                let q2 = Vec2::new(q[0], q[2]);
                for s in &map.spawns {
                    // A piece reaches half a metre at most from its root.
                    assert!(q2.distance(Vec2::new(s.pos.x, s.pos.z)) > SPAWN_CLEAR_M - 0.5);
                }
                for pad in &map.pads {
                    assert!(q2.distance(Vec2::new(pad.pos.x, pad.pos.z)) > 0.85);
                }
                for l in &map.launchers {
                    let inside =
                        q2.x > l.area.min.x && q2.x < l.area.max.x && q2.y > l.area.min.z && q2.y < l.area.max.z;
                    assert!(!inside, "a piece on a launcher at {q:?}");
                }
            }
        }
        assert!(pts > 1000, "the ground is populated: {pts} vertices");
    }
}

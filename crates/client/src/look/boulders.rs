//! Boulders: what a rock box a fighter can reach is drawn as, and the rock
//! that stands out of reach as scenery (Dry Fork's sea stacks, the stones in
//! its shallows past the bound).
//!
//! The models are ours, commissioned from Meshy and cut in Blender
//! (`tools/models/rocks.py`): three blocks of bedded red sandstone and a
//! sea stack with a green top, each a few thousand triangles with a colour
//! and a normal map baked for it.
//!
//! **A boulder is drawn inside the box the sim collides.** The planner's
//! rock boxes (`tools/maps/compile.mjs`) were drawn as their own six faces,
//! faceted (`rock.rs`): exact, and a field of bricks. Each is now one of
//! the three blocks, which `rocks.py` squares up to fill a unit cube and
//! this stretches to the box ([`fit`] picks the block whose own shape the
//! box is nearest, turned by quarter turns, so none is pulled far out of
//! shape). Nothing of it is outside the box, so a round never passes
//! through drawn stone and nobody stands on air beside it; it sits inside
//! by [`INSET_MEAN`] of the box's side on average and [`INSET_MAX`] at its
//! most cut-back corner (a hand's width on a 2.5 m rock, 40 cm at the worst
//! corner), where a round stops a little short of what is drawn and leaves
//! no mark (`rock::Bare`). The server's fog reads the box, so nothing is
//! seen past a cut corner that the box hides. Every rung draws the same
//! mesh: a rock is cover.
//!
//! Until the models load a boulder is its faceted box, and stays one if
//! they cannot.

use bevy::gltf::GltfAssetLabel;
use bevy::prelude::*;

use super::fresnel;
use super::rock::Bare;
use crate::level::{Dressed, LevelPart};

/// The models: three blocks, the sea stack, a base's pylon, an island's three
/// wind-carved blocks, two stacks and a hanging island, and the ship's props: a
/// seed husk in three paints and a font.
pub const GLB: [&str; 15] = [
    "models/rock/boulder_a.glb",
    "models/rock/boulder_b.glb",
    "models/rock/boulder_c.glb",
    "models/rock/sea_stack.glb",
    "models/base/pylon.glb",
    "models/rock/isle_boulder_a.glb",
    "models/rock/isle_boulder_b.glb",
    "models/rock/isle_boulder_c.glb",
    "models/rock/isle_stack_a.glb",
    "models/rock/isle_stack_b.glb",
    "models/rock/isle_float.glb",
    "models/ship/husk.glb",
    "models/ship/husk.glb",
    "models/ship/husk.glb",
    "models/ship/font.glb",
];
/// Where an island's blocks start among them, and its stacks and hanging
/// island (each 1 m tall on its base at the origin, as the sea stack; the
/// stacks 0.5 by 0.22 and 1.63 by 1.07 across, the island 0.89 by 0.53).
pub const ISLE_BLOCKS: usize = 5;
pub const ISLE_SPIRE: usize = 8;
pub const ISLE_BUTTE: usize = 9;
pub const ISLE_FLOAT: usize = 10;
/// How many of them are blocks, cut to a unit cube about its middle.
pub const BLOCKS: usize = 3;
/// The stack: 1 m tall on its base at the origin, scaled evenly.
pub const STACK: usize = 3;
/// The pylon: a slab of gunmetal cut to a unit cube like a block, its
/// face along +z and its width along x. It sits [`PYLON_INSET_MEAN`] of its
/// box's side inside it on average and [`PYLON_INSET_MAX`] at its chamfered
/// top corners (`rocks.py`'s numbers for it).
pub const PYLON_MODEL: usize = 4;
/// The husk, in the hull's dark, red's and blue's; the font. Each a pale
/// cast (the paint is in its tint, [`PROP_TINT`]), enamel over metal.
pub const HUSK_MODEL: usize = 11;
pub const FONT_MODEL: usize = 14;
/// Linear: the husk dark, red's, blue's, and the font.
pub const PROP_TINT: [[f32; 3]; 4] = [
    [0.16, 0.1, 0.32],
    [0.5, 0.03, 0.09],
    [0.05, 0.09, 0.55],
    [0.4, 0.28, 0.7],
];
/// A prop's width in its round solid, as a share of the cube round it: what
/// keeps its farthest point inside the circle (`rocks.py`'s `radial`, the
/// husk's fins and the font's crest).
pub const PROP_FIT: [f32; 2] = [0.77, 0.965];
pub const PROP_ROUGH: f32 = 0.26;
pub const PROP_METAL: f32 = 0.8;
pub const PYLON_INSET_MEAN: f32 = 0.093;
pub const PYLON_INSET_MAX: f32 = 0.201;
/// The size each block came at before it was squared up (x, up, z;
/// `rocks.py`'s `native`): what says which boxes it suits.
pub const NATIVE: [[f32; 3]; BLOCKS] = [[0.918, 1.0, 0.852], [0.734, 0.2, 0.304], [0.585, 1.0, 0.513]];
/// An island's blocks, the same (`rocks.py`'s `native`): wind-carved round,
/// so inside their boxes by [`ISLE_INSET_MEAN`] on average and
/// [`ISLE_INSET_MAX`] at a box's corner, where a round stops a little short.
pub const ISLE_NATIVE: [[f32; 3]; BLOCKS] = [[0.251, 0.3, 0.155], [0.409, 0.18, 0.427], [0.094, 0.2, 0.103]];
pub const ISLE_INSET_MEAN: f32 = 0.126;
pub const ISLE_INSET_MAX: f32 = 0.301;
/// How far inside its cube a block's surface sits, as shares of the
/// cube's side: the mean over its surface and the worst corner
/// (`rocks.py`'s `inset_mean` and `inset_max`, the largest of the three).
pub const INSET_MEAN: f32 = 0.028;
pub const INSET_MAX: f32 = 0.161;
/// A box stretched further than this off every block's own shape is drawn
/// as its faceted faces: a slab pulled 4:1 is a smear.
pub const FIT_MAX: f32 = 2.6;
/// Among blocks that fit within this of the best, the box's key picks.
const FIT_TIE: f32 = 1.2;
/// The models' paint over Meshy's (linear): toward the cliffs' own red
/// (`ground::TINT`), a little darker so a boulder sits in the ground.
pub const TINT: [f32; 3] = [0.74, 0.62, 0.52];
/// An island's paint over Meshy's (linear): cooler and darker than it came,
/// its orange pits brought back into the sand's family.
pub const ISLE_TINT: [f32; 3] = [0.8, 0.8, 0.84];
/// Stone the sea washes: darker and glossier.
pub const WET_TINT: [f32; 3] = [0.4, 0.35, 0.31];
pub const WET_ROUGH: f32 = 0.45;
pub const DRY_ROUGH: f32 = 0.92;
/// The pylon's metal: brushed, not polished (a mirror under the open sky
/// is white).
pub const PYLON_ROUGH: f32 = 0.55;
pub const PYLON_METAL: f32 = 0.4;
/// Its paint over Meshy's (linear): lifted into the wall's grey. Near
/// full metal its own paint hardly showed and it stood black beside it.
pub const PYLON_TINT: f32 = 1.5;

/// A rock box waiting for its block.
#[derive(Component, Clone, Copy, Debug)]
pub struct Boulder {
    pub min: Vec3,
    pub max: Vec3,
    pub key: u32,
    /// An island's: one of its own blocks.
    pub pale: bool,
}

/// A base's buttress waiting for its pylon: a turned box the sim collides
/// (a `pylon` look's), where the model goes in it, and its bounds, kept
/// clear of marks.
#[derive(Component, Clone, Copy, Debug)]
pub struct Pylon {
    /// Which of [`GLB`].
    pub model: usize,
    pub at: Transform,
    pub min: Vec3,
    pub max: Vec3,
}

/// Where a pylon is drawn in a turned box standing at `at` (its middle and
/// its turn, the box's own x pointing out from the base): the model's face
/// (+z) turned to the box's x, stretched over the box's half-extents `hx`
/// out and `hz` across and its height `h`.
pub fn pylon_transform(at: Transform, hx: f32, h: f32, hz: f32) -> Transform {
    Transform {
        translation: at.translation,
        rotation: at.rotation * Quat::from_rotation_y(core::f32::consts::FRAC_PI_2),
        scale: Vec3::new(2.0 * hz, h, 2.0 * hx),
    }
}

/// A look's model among [`GLB`] (a `level::HUSK..=FONT`).
pub fn prop_model(look: u8) -> usize {
    match look {
        crate::level::HUSK => HUSK_MODEL,
        crate::level::HUSK_RED => HUSK_MODEL + 1,
        crate::level::HUSK_BLUE => HUSK_MODEL + 2,
        _ => FONT_MODEL,
    }
}

/// Where a prop is drawn in its round solid at (`x`, `z`), `r` across, from
/// `y0` to `y1`: stretched to its height and to [`PROP_FIT`] of the cube
/// round it, its face (+z) to the map's middle.
pub fn round_prop_transform(look: u8, x: f32, y0: f32, y1: f32, z: f32, r: f32) -> Transform {
    let fit = PROP_FIT[usize::from(look == crate::level::FONT)];
    Transform {
        translation: Vec3::new(x, (y0 + y1) * 0.5, z),
        rotation: Quat::from_rotation_y((-x).atan2(-z)),
        scale: Vec3::new(2.0 * r * fit, y1 - y0, 2.0 * r * fit),
    }
}

/// A piece of scenery waiting for its model: which, and whether the sea
/// washes it.
#[derive(Component, Clone, Copy, Debug)]
pub struct Stone {
    pub model: usize,
    pub wet: bool,
    /// An island's: in [`ISLE_TINT`].
    pub pale: bool,
}

/// Rock nobody reaches, by map: the model, where its middle (a block) or
/// its foot (the stack) is, its size (a block's box; a stack's height in
/// `y`, the rest unread), its yaw, how far it is tipped over (radians: a
/// block lying as it fell, not standing square) and whether it stands in
/// the sea.
#[derive(Clone, Copy, Debug)]
pub struct Prop {
    pub model: usize,
    pub at: [f32; 3],
    pub size: [f32; 3],
    pub yaw: f32,
    pub tilt: f32,
    pub wet: bool,
}

const fn stack(x: f32, z: f32, foot: f32, h: f32, yaw: f32) -> Prop {
    Prop {
        model: STACK,
        at: [x, foot, z],
        size: [h, h, h],
        yaw,
        tilt: 0.0,
        wet: false,
    }
}

const fn stone(model: usize, at: [f32; 3], size: [f32; 3], yaw: f32, tilt: f32) -> Prop {
    Prop {
        model,
        at,
        size,
        yaw,
        tilt,
        wet: true,
    }
}

/// Dry Fork's sea: stacks standing off the canyon's mouth, their dark feet
/// at the waterline (the sea is at −6), one of them far out in the haze,
/// and stones in the shallows past the bound at x 150, where nobody wades:
/// three clumps and a few alone, most of each under the water.
const DRY_FORK: [Prop; 19] = [
    stack(252.0, -58.0, -11.0, 46.0, 0.6),
    stack(214.0, 70.0, -9.0, 26.0, 4.0),
    stack(228.0, 88.0, -9.0, 17.0, 2.2),
    stack(430.0, 40.0, -14.0, 58.0, 2.4),
    stack(196.0, -104.0, -9.0, 22.0, 1.2),
    stone(0, [156.0, -6.3, -20.0], [4.2, 3.4, 3.4], 0.5, 0.42),
    stone(2, [160.0, -6.5, -16.0], [2.4, 2.8, 2.2], 1.9, -0.6),
    stone(0, [158.5, -6.7, -25.0], [2.0, 1.8, 1.8], 3.0, 0.8),
    stone(1, [153.5, -6.6, -13.0], [3.0, 1.2, 1.8], 2.3, 0.3),
    stone(2, [162.0, -6.0, 31.0], [4.6, 5.0, 4.0], 4.1, 0.5),
    stone(0, [166.5, -6.6, 27.0], [3.0, 2.6, 2.6], 0.2, -0.7),
    stone(1, [158.5, -6.6, 36.0], [4.0, 1.6, 2.6], 5.2, 0.35),
    stone(0, [173.0, -6.0, -50.0], [5.6, 5.4, 5.0], 2.6, 0.55),
    stone(2, [168.5, -6.6, -55.0], [3.2, 3.4, 3.0], 1.0, -0.5),
    stone(2, [155.0, -6.5, 58.0], [3.4, 3.6, 3.0], 3.6, 0.65),
    stone(0, [181.0, -6.2, 4.0], [5.0, 4.4, 4.6], 1.4, -0.45),
    stone(1, [164.0, -6.6, 2.0], [3.6, 1.4, 2.2], 0.7, 0.25),
    stone(0, [160.5, -5.6, -68.0], [5.8, 5.6, 5.0], 5.6, 0.5),
    stone(2, [157.5, -5.9, 70.0], [5.0, 5.2, 4.4], 0.8, -0.6),
];

const fn tower(model: usize, x: f32, z: f32, h: f32, yaw: f32) -> Prop {
    Prop {
        model,
        at: [x, ISLE_FOOT_Y, z],
        size: [h, h, h],
        yaw,
        tilt: 0.0,
        wet: false,
    }
}

const fn ledge(model: usize, at: [f32; 3], size: [f32; 3], yaw: f32, tilt: f32) -> Prop {
    Prop {
        model,
        at,
        size,
        yaw,
        tilt,
        wet: false,
    }
}

const fn shelf(model: usize, x: f32, foot: f32, z: f32, h: f32, yaw: f32) -> Prop {
    Prop {
        model,
        at: [x, foot, z],
        size: [h, h, h],
        yaw,
        tilt: 0.0,
        wet: false,
    }
}

const fn hanging(top: f32, x: f32, z: f32, size: [f32; 3], yaw: f32) -> Prop {
    Prop {
        model: ISLE_FLOAT,
        at: [x, top - size[1], z],
        size,
        yaw,
        tilt: 0.0,
        wet: false,
    }
}

/// Cloudbreak's sea of cloud (`light.sea` in its map), m.
const ISLE_SEA_Y: f32 = -60.0;
/// Where Cloudbreak's stacks stand from, m: under its sea of cloud (at -60).
const ISLE_FOOT_Y: f32 = -75.0;

/// Cloudbreak's sky: sandstone standing out of the sea of cloud all round,
/// spires and buttes, the near ones looked down on over the edge and the far
/// ones over the island's own height so the skyline is never empty, and more
/// islands hanging in the distance, each further thing paler in the air.
const CLOUDBREAK: [Prop; 60] = [
    tower(ISLE_BUTTE, -168.0, -112.0, 62.0, 3.7),
    tower(ISLE_BUTTE, 176.0, -94.0, 54.0, 2.9),
    tower(ISLE_SPIRE, -86.0, 112.0, 72.0, 0.4),
    tower(ISLE_BUTTE, -26.0, 182.0, 50.0, 4.4),
    tower(ISLE_SPIRE, 52.0, -160.0, 84.0, 2.0),
    tower(ISLE_BUTTE, -210.0, 50.0, 74.0, 5.3),
    tower(ISLE_SPIRE, 152.0, 140.0, 112.0, 3.7),
    tower(ISLE_SPIRE, 218.0, -186.0, 118.0, 0.9),
    tower(ISLE_SPIRE, -150.0, -232.0, 106.0, 0.6),
    tower(ISLE_BUTTE, 262.0, 92.0, 92.0, 1.3),
    tower(ISLE_SPIRE, -262.0, 162.0, 128.0, 3.3),
    tower(ISLE_SPIRE, 120.0, -300.0, 96.0, 4.0),
    tower(ISLE_SPIRE, 62.0, 382.0, 132.0, 5.0),
    tower(ISLE_BUTTE, -292.0, -382.0, 104.0, 2.1),
    tower(ISLE_SPIRE, 382.0, -302.0, 124.0, 2.7),
    tower(ISLE_BUTTE, -462.0, -62.0, 112.0, 0.2),
    tower(ISLE_BUTTE, -92.0, -482.0, 100.0, 1.8),
    hanging(52.0, -345.0, -135.0, [84.0, 92.0, 96.0], 0.7),
    hanging(78.0, 330.0, -60.0, [72.0, 80.0, 84.0], 2.4),
    hanging(128.0, -90.0, 420.0, [108.0, 118.0, 124.0], 4.1),
    // Its own: the rock it hangs from, under the void and inside the keel's
    // first rings so their flat tops don't show, a cluster so no one mass is
    // stretched past what its paint holds.
    hanging(-14.0, -9.0, 5.0, [26.0, 58.0, 32.0], 0.4),
    hanging(-14.0, 10.0, -5.0, [28.0, 64.0, 30.0], 2.2),
    hanging(-16.0, 0.0, 10.0, [22.0, 48.0, 24.0], 4.0),
    hanging(-16.0, 2.0, -11.0, [22.0, 44.0, 22.0], 5.3),
    // And what hangs under its rim, out of anyone's reach, off the coast's
    // outline and set back so the lip overhangs it: buttes along the coast
    // and low domed blocks, the pitted one now and then, and a lower row.
    shelf(ISLE_BUTTE, -1.2, -12.2, 18.8, 8.6, -0.22),
    ledge(ISLE_BLOCKS + 1, [4.1, -7.7, 19.2], [11.2, 6.7, 6.8], -0.04, -0.12),
    shelf(ISLE_BUTTE, 4.9, -13.7, 16.6, 9.8, 3.90),
    ledge(ISLE_BLOCKS + 1, [13.0, -7.3, 15.2], [10.4, 7.5, 7.8], 0.85, 0.01),
    shelf(ISLE_BUTTE, 15.0, -15.3, 9.6, 11.8, 0.68),
    ledge(ISLE_BLOCKS + 1, [21.1, -7.8, 7.7], [11.5, 8.1, 7.8], 0.76, -0.04),
    shelf(ISLE_BUTTE, 20.2, -14.9, 3.3, 10.5, 4.66),
    ledge(ISLE_BLOCKS, [23.1, -8.0, -1.8], [10.5, 8.2, 6.2], 1.93, -0.13),
    shelf(ISLE_BUTTE, 18.3, -13.9, -6.6, 10.5, 1.87),
    ledge(ISLE_BLOCKS + 1, [19.2, -8.1, -11.1], [10.6, 8.2, 6.1], 2.58, 0.13),
    shelf(ISLE_BUTTE, 14.9, -13.8, -11.7, 9.8, 6.06),
    ledge(ISLE_BLOCKS + 1, [12.7, -8.9, -12.3], [10.8, 9.2, 7.5], -2.86, 0.12),
    shelf(ISLE_BUTTE, -0.2, -15.4, -14.6, 11.3, 2.45),
    ledge(ISLE_BLOCKS + 1, [0.3, -7.1, -18.9], [10.2, 7.1, 6.3], 2.97, 0.01),
    shelf(ISLE_BUTTE, 1.7, -14.9, -18.0, 11.1, 0.99),
    ledge(ISLE_BLOCKS, [-8.5, -8.1, -12.5], [10.0, 8.5, 6.4], -2.95, 0.02),
    shelf(ISLE_BUTTE, -13.6, -15.5, -10.8, 11.9, -2.93),
    ledge(ISLE_BLOCKS + 1, [-17.3, -7.9, -12.0], [9.7, 7.9, 6.9], -2.39, 0.10),
    shelf(ISLE_BUTTE, -18.0, -15.5, -7.7, 11.7, 0.94),
    ledge(ISLE_BLOCKS + 1, [-22.4, -8.4, -2.5], [11.1, 8.0, 6.1], -1.74, 0.14),
    shelf(ISLE_BUTTE, -20.8, -14.6, 1.8, 10.3, -1.35),
    ledge(ISLE_BLOCKS + 1, [-21.0, -7.9, 6.6], [11.6, 8.0, 7.2], -1.21, 0.05),
    shelf(ISLE_BUTTE, -18.7, -13.2, 8.2, 8.9, 2.64),
    ledge(ISLE_BLOCKS, [-15.1, -7.3, 14.2], [10.9, 6.8, 6.4], -0.61, -0.13),
    shelf(ISLE_BUTTE, -12.6, -14.1, 13.2, 9.9, 0.20),
    ledge(ISLE_BLOCKS + 1, [-5.0, -8.5, 20.0], [9.7, 9.1, 6.2], -0.42, 0.08),
    shelf(ISLE_BUTTE, 1.2, -27.2, 11.3, 15.0, 0.67),
    shelf(ISLE_BUTTE, 9.7, -31.2, 5.6, 18.8, 0.43),
    shelf(ISLE_BUTTE, 15.2, -27.2, 1.1, 14.2, 1.34),
    shelf(ISLE_BUTTE, 12.5, -26.8, -6.6, 14.4, 2.43),
    shelf(ISLE_BUTTE, 1.4, -23.9, -8.8, 13.1, 2.85),
    shelf(ISLE_BUTTE, 4.1, -24.2, -12.1, 13.4, -1.85),
    shelf(ISLE_BUTTE, -13.4, -28.8, -4.9, 16.5, -2.56),
    shelf(ISLE_BUTTE, -12.9, -29.6, -0.5, 18.2, -1.51),
    shelf(ISLE_BUTTE, -13.4, -28.5, 3.6, 16.3, -0.32),
    shelf(ISLE_BUTTE, 0.4, -23.6, 15.1, 13.5, -0.87),
];

/// The scenery of map `id`.
pub fn scenery(id: u8) -> &'static [Prop] {
    match id {
        3 => &DRY_FORK,
        7 => &CLOUDBREAK,
        _ => &[],
    }
}

/// Whether a model is 1 m tall on its base, scaled evenly (a hanging island
/// is stretched to the size it is given).
pub const fn is_stack(model: usize) -> bool {
    matches!(model, STACK | ISLE_SPIRE | ISLE_BUTTE)
}

/// Where a prop is drawn.
pub fn prop_transform(p: &Prop) -> Transform {
    let at = Vec3::from_array(p.at);
    let scale = if is_stack(p.model) {
        Vec3::splat(p.size[1])
    } else {
        Vec3::from_array(p.size)
    };
    Transform {
        translation: at,
        // Tipped about one level axis and half as far about the other.
        rotation: Quat::from_rotation_y(p.yaw) * Quat::from_rotation_x(p.tilt) * Quat::from_rotation_z(p.tilt * 0.5),
        scale,
    }
}

/// Which block draws a box of `size`, and how many quarter turns about the
/// vertical it is given: the one whose own shape the box is nearest (the
/// least stretch between its three sides), the box's `key` picking among
/// near ties and which of the two ways round it faces. `None` when every
/// block would be pulled past [`FIT_MAX`].
pub fn fit(size: Vec3, key: u32) -> Option<(usize, u8)> {
    fit_among(size, key, &NATIVE, 0)
}

/// [`fit`] among an island's blocks.
pub fn fit_isle(size: Vec3, key: u32) -> Option<(usize, u8)> {
    fit_among(size, key, &ISLE_NATIVE, ISLE_BLOCKS)
}

fn fit_among(size: Vec3, key: u32, native: &[[f32; 3]; BLOCKS], first: usize) -> Option<(usize, u8)> {
    let stretch = |n: [f32; 3], turned: bool| {
        let (nx, nz) = if turned { (n[2], n[0]) } else { (n[0], n[2]) };
        let r = [size.x / nx, size.y / n[1], size.z / nz];
        let hi = r[0].max(r[1]).max(r[2]);
        let lo = r[0].min(r[1]).min(r[2]);
        hi / lo.max(1e-6)
    };
    let mut all = [(0usize, false, 0.0f32); BLOCKS * 2];
    for (i, n) in native.iter().enumerate() {
        all[i * 2] = (i, false, stretch(*n, false));
        all[i * 2 + 1] = (i, true, stretch(*n, true));
    }
    let best = all.iter().map(|c| c.2).fold(f32::INFINITY, f32::min);
    if best > FIT_MAX {
        return None;
    }
    let near = all.iter().filter(|c| c.2 <= best * FIT_TIE).count();
    let pick = (key >> 3) as usize % near;
    let (model, turned, _) = *all.iter().filter(|c| c.2 <= best * FIT_TIE).nth(pick)?;
    let flip = (key >> 1) & 1 == 1;
    Some((first + model, u8::from(turned) + 2 * u8::from(flip)))
}

/// Where the block of a box is drawn: its middle, turned `quarters`, and
/// stretched so its cube is the box.
pub fn block_transform(min: Vec3, max: Vec3, quarters: u8) -> Transform {
    let size = max - min;
    let scale = if quarters % 2 == 1 {
        Vec3::new(size.z, size.y, size.x)
    } else {
        size
    };
    Transform {
        translation: (min + max) * 0.5,
        rotation: Quat::from_rotation_y(f32::from(quarters) * core::f32::consts::FRAC_PI_2),
        scale,
    }
}

/// The models, loading; then their meshes and the two paints of each.
#[derive(Resource, Default)]
pub struct Stones {
    meshes: Vec<Handle<Mesh>>,
    paints: Vec<Handle<StandardMaterial>>,
    dry: Vec<Handle<StandardMaterial>>,
    wet: Vec<Handle<StandardMaterial>>,
    pale: Vec<Handle<StandardMaterial>>,
    /// An island's scenery (`isle_stone.rs`): `pale` going into shade under
    /// its rim and lost in its cloud.
    hung: Vec<Handle<super::isle_stone::IsleStoneMaterial>>,
    failed: bool,
}

pub fn load(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(Stones {
        meshes: GLB
            .iter()
            .map(|p| server.load(GltfAssetLabel::Primitive { mesh: 0, primitive: 0 }.from_asset(*p)))
            .collect(),
        paints: GLB
            .iter()
            .map(|p| {
                server.load(
                    GltfAssetLabel::Material {
                        index: 0,
                        is_scale_inverted: false,
                    }
                    .from_asset(*p),
                )
            })
            .collect(),
        ..default()
    });
}

impl Stones {
    /// `Some(true)` once every model is in and painted, `Some(false)` if
    /// one cannot be, `None` while they load.
    fn ready(
        &mut self,
        server: &AssetServer,
        meshes: &Assets<Mesh>,
        mats: &mut Assets<StandardMaterial>,
        hung: &mut Assets<super::isle_stone::IsleStoneMaterial>,
        far: Vec3,
    ) -> Option<bool> {
        if self.failed {
            return Some(false);
        }
        if !self.dry.is_empty() {
            return Some(true);
        }
        let lost = |s: Option<bevy::asset::LoadState>| matches!(s, Some(bevy::asset::LoadState::Failed(_)));
        if self.meshes.iter().any(|h| lost(server.get_load_state(h)))
            || self.paints.iter().any(|h| lost(server.get_load_state(h)))
        {
            warn!("a rock model failed to load: the boulders are drawn as their boxes");
            self.failed = true;
            return Some(false);
        }
        if !self.meshes.iter().all(|h| meshes.contains(h)) || !self.paints.iter().all(|h| mats.contains(h)) {
            return None;
        }
        // The stack keeps Meshy's paint whole: its top is green.
        for (i, h) in self.paints.clone().iter().enumerate() {
            let base = mats.get(h)?.clone();
            let prop = i >= HUSK_MODEL;
            let tinted = |t: [f32; 3], rough: f32| StandardMaterial {
                // The stack keeps its own paint (its top is green), and
                // the pylon is metal, as its own material says; a prop is
                // enamel in its own colour, wet or dry.
                base_color: if i == STACK {
                    Color::WHITE
                } else if i == PYLON_MODEL {
                    Color::linear_rgb(PYLON_TINT, PYLON_TINT, PYLON_TINT)
                } else if prop {
                    let c = PROP_TINT[i - HUSK_MODEL];
                    Color::linear_rgb(c[0], c[1], c[2])
                } else {
                    Color::linear_rgb(t[0], t[1], t[2])
                },
                perceptual_roughness: if i == PYLON_MODEL {
                    PYLON_ROUGH
                } else if prop {
                    PROP_ROUGH
                } else {
                    rough
                },
                metallic: if i == PYLON_MODEL {
                    PYLON_METAL
                } else if prop {
                    PROP_METAL
                } else {
                    0.0
                },
                reflectance: fresnel::DIELECTRIC,
                ..base.clone()
            };
            let (dry, wet) = (tinted(TINT, DRY_ROUGH), tinted(WET_TINT, WET_ROUGH));
            let pale = if i == PYLON_MODEL || prop {
                dry.clone()
            } else {
                tinted(ISLE_TINT, DRY_ROUGH)
            };
            self.dry.push(mats.add(dry));
            self.wet.push(mats.add(wet));
            self.hung
                .push(hung.add(super::isle_stone::material(pale.clone(), ISLE_SEA_Y, far)));
            self.pale.push(mats.add(pale));
        }
        Some(true)
    }
}

/// Draw each boulder and each stone once the models are in: a boulder's
/// block in place of its faceted box, kept clear of marks. If they cannot
/// load, the boulders are handed back to `level::dress_level` as boxes.
#[allow(clippy::too_many_arguments)]
pub fn dress(
    mut commands: Commands,
    stones: Option<ResMut<Stones>>,
    server: Res<AssetServer>,
    meshes: Res<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut hung: ResMut<Assets<super::isle_stone::IsleStoneMaterial>>,
    tier: Res<super::quality::Tier>,
    boulders: Query<(Entity, &Boulder)>,
    pylons: Query<(Entity, &Pylon)>,
    props: Query<(Entity, &Stone), Without<Dressed>>,
) {
    if boulders.is_empty() && pylons.is_empty() && props.is_empty() {
        return;
    }
    let Some(mut stones) = stones else {
        return;
    };
    // The cloud an island's stone goes to: its mood's fog (the only island's).
    let far = super::rig::fog_for(&super::mood::OVERCAST, tier.atmosphere, 0.0)
        .color
        .to_linear();
    let far = Vec3::new(far.red, far.green, far.blue);
    let Some(ok) = stones.ready(&server, &meshes, &mut mats, &mut hung, far) else {
        return;
    };
    for (e, b) in &boulders {
        let mut ec = commands.entity(e);
        ec.remove::<Boulder>();
        let fitted = if b.pale {
            fit_isle(b.max - b.min, b.key)
        } else {
            fit(b.max - b.min, b.key)
        };
        let Some((model, quarters)) = fitted.filter(|_| ok) else {
            continue;
        };
        ec.remove::<(
            crate::level::BoxLook,
            super::weathering::Block,
            MeshMaterial3d<super::weathering::LevelMaterial>,
        )>()
        .insert((
            Mesh3d(stones.meshes[model].clone()),
            MeshMaterial3d(if b.pale { &stones.pale } else { &stones.dry }[model].clone()),
            block_transform(b.min, b.max, quarters),
            Bare(b.min, b.max),
            Dressed,
        ));
    }
    for (e, p) in &pylons {
        let mut ec = commands.entity(e);
        ec.remove::<Pylon>();
        if !ok {
            continue;
        }
        ec.remove::<(
            crate::level::BoxLook,
            super::weathering::Block,
            MeshMaterial3d<super::weathering::LevelMaterial>,
        )>()
        .insert((
            Mesh3d(stones.meshes[p.model].clone()),
            MeshMaterial3d(stones.dry[p.model].clone()),
            p.at,
            Bare(p.min, p.max),
            Dressed,
        ));
    }
    for (e, s) in &props {
        let mut ec = commands.entity(e);
        ec.insert(Dressed);
        if !ok {
            continue;
        }
        if s.pale {
            // Far off or under the rim: nothing a shadow of it would fall on.
            ec.insert((
                Mesh3d(stones.meshes[s.model].clone()),
                MeshMaterial3d(stones.hung[s.model].clone()),
                bevy::light::NotShadowCaster,
            ));
            continue;
        }
        let paint = if s.wet { &stones.wet } else { &stones.dry };
        ec.insert((
            Mesh3d(stones.meshes[s.model].clone()),
            MeshMaterial3d(paint[s.model].clone()),
        ));
    }
}

/// The scenery of map `id`, each piece waiting for its model; `pale` on an
/// island, whose cloud is at `sea`.
pub fn spawn_scenery(commands: &mut Commands, id: u8, pale: bool) {
    for p in scenery(id) {
        commands.spawn((
            LevelPart,
            Stone {
                model: p.model,
                wet: p.wet,
                pale,
            },
            prop_transform(p),
            Visibility::default(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every rock box a map's players can reach gets a block that is not
    /// pulled far out of shape, and a box no block suits is left a box.
    #[test]
    fn a_box_gets_the_block_it_is_nearest() {
        // A cube: the cube-like block, either way round.
        let (m, _) = fit(Vec3::new(2.0, 2.2, 1.9), 7).unwrap();
        assert_eq!(m, 0);
        // A long low slab, along x and along z: the slab, turned for z.
        let (m, q) = fit(Vec3::new(5.0, 1.4, 2.0), 0).unwrap();
        assert_eq!((m, q % 2), (1, 0));
        let (m, q) = fit(Vec3::new(2.0, 1.4, 5.0), 0).unwrap();
        assert_eq!((m, q % 2), (1, 1));
        // Taller than wide: the upright one.
        let (m, _) = fit(Vec3::new(1.6, 3.2, 1.5), 3).unwrap();
        assert_eq!(m, 2);
        // A wall of rock is nobody's shape.
        assert!(fit(Vec3::new(30.0, 2.0, 1.0), 0).is_none());
    }

    /// Dry Fork's own rock boxes all find a block.
    #[test]
    fn every_rock_on_dry_fork_is_a_boulder() {
        let def = sim::maps::ALL.iter().find(|d| d.id == 3).unwrap();
        let map = def.build();
        let mut rocks = 0;
        for (b, &l) in map.boxes.iter().zip(&map.looks) {
            if l != 6 {
                continue;
            }
            rocks += 1;
            let size = Vec3::new(b.max.x - b.min.x, b.max.y - b.min.y, b.max.z - b.min.z);
            assert!(fit(size, rocks).is_some(), "no block for a rock of {size:?}");
        }
        assert!(rocks > 20);
    }

    /// A turned block's cube is still its box: the quarter turns swap the
    /// sides they stretch.
    #[test]
    fn a_turned_block_fills_its_box() {
        let (min, max) = (Vec3::new(1.0, 0.0, -2.0), Vec3::new(5.0, 2.0, 0.0));
        for q in 0..4u8 {
            let t = block_transform(min, max, q);
            let mut lo = Vec3::splat(f32::INFINITY);
            let mut hi = Vec3::splat(f32::NEG_INFINITY);
            for c in 0..8 {
                let p = Vec3::new(
                    if c & 1 == 0 { -0.5 } else { 0.5 },
                    if c & 2 == 0 { -0.5 } else { 0.5 },
                    if c & 4 == 0 { -0.5 } else { 0.5 },
                );
                let w = t.transform_point(p);
                lo = lo.min(w);
                hi = hi.max(w);
            }
            assert!(
                (lo - min).abs().max_element() < 1e-4 && (hi - max).abs().max_element() < 1e-4,
                "{q}: {lo} {hi}"
            );
        }
    }

    /// A pylon's cube is its turned box: its face along the box's own x
    /// (out from the base), its width across.
    #[test]
    fn a_pylon_fills_its_turned_box() {
        // A box 1.5 out and 2.0 across, turned a quarter (its x along +z).
        let (hx, hz, h) = (0.75, 1.0, 6.0);
        let rot = Quat::from_rotation_y(-core::f32::consts::FRAC_PI_2);
        let at = Transform::from_xyz(4.0, 3.0, 9.0).with_rotation(rot);
        let t = pylon_transform(at, hx, h, hz);
        // The model's face (+z, half a unit out) lands on the box's outer
        // face: hx along the box's x, which is the world's +z here.
        let face = t.transform_point(Vec3::new(0.0, 0.0, 0.5));
        assert!((face - Vec3::new(4.0, 3.0, 9.0 + hx)).length() < 1e-4, "{face}");
        // Its width (x, half a unit) reaches the box's side, hz across.
        let side = t.transform_point(Vec3::new(0.5, 0.0, 0.0));
        assert!(((side - Vec3::new(4.0, 3.0, 9.0)).length() - hz).abs() < 1e-4, "{side}");
        // And its top the box's.
        assert!((t.transform_point(Vec3::new(0.0, 0.5, 0.0)).y - 6.0).abs() < 1e-4);
    }

    /// The scenery is out of reach: nothing of it inside the playable
    /// rectangle, where it would be stone that stops nothing.
    #[test]
    fn scenery_stands_out_of_bounds() {
        for def in sim::maps::ALL.iter() {
            for p in scenery(def.id) {
                // An island's own rock hangs under its rim, nobody alive there:
                // the top of a block about its middle, of the rest off its foot.
                let up = if p.model == ISLE_FLOAT || is_stack(p.model) {
                    1.0
                } else {
                    0.5
                };
                if def.void.is_finite() && p.at[1] + p.size[1] * up <= 0.0 {
                    continue;
                }
                let r = if is_stack(p.model) {
                    p.size[1] * 0.45
                } else {
                    p.size[0].max(p.size[2]) * 0.75
                };
                let (x, z) = (p.at[0], p.at[2]);
                let inside = x + r > def.lo.0 && x - r < def.hi.0 && z + r > def.lo.1 && z - r < def.hi.1;
                assert!(!inside, "{}: a prop at {x}, {z} reaches into the map", def.name);
            }
        }
    }
}

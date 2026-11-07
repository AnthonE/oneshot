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

/// The models: three blocks, the sea stack, a base's pylon, and the ship's
/// props: a seed husk in three paints and a font.
pub const GLB: [&str; 9] = [
    "models/rock/boulder_a.glb",
    "models/rock/boulder_b.glb",
    "models/rock/boulder_c.glb",
    "models/rock/sea_stack.glb",
    "models/base/pylon.glb",
    "models/ship/husk.glb",
    "models/ship/husk.glb",
    "models/ship/husk.glb",
    "models/ship/font.glb",
];
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
pub const HUSK_MODEL: usize = 5;
pub const FONT_MODEL: usize = 8;
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
/// An island's (`level::Land::Isle`): pale cream sandstone, its boulders
/// of the sand's family. Its stacks are drawn in the island's own ground
/// material instead (`ground.wgsl` over the stack's mesh carrying sand
/// weights, [`STACK_SPLAT`]: every face too steep for sand turns to bedded
/// rock per pixel, its ledges keep the sand), so they are the keel's
/// stone, not a clay model; `PALE_STACK` is the plain cream they wear
/// until the ground's textures are in.
pub const PALE_TINT: [f32; 3] = [2.3, 3.0, 3.1];
pub const PALE_STACK: [f32; 3] = [0.5, 0.41, 0.3];
pub const STACK_SPLAT: [f32; 4] = [1.0, 0.0, 0.0, 0.0];
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
    /// An island's: in [`PALE_TINT`].
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
    /// An island's: in [`PALE_TINT`] ([`PALE_STACK`] for a stack).
    pub pale: bool,
    /// The map's sea (an island's cloud), for the lap up a stack's foot.
    pub sea: f32,
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

/// Cloudbreak's sky: stacks standing out of the sea of cloud (at −60) all
/// round, their feet under it, each further one paler in the air, some
/// over the island's own height so the skyline is never empty. A near
/// ring of them within two hundred metres, mostly under the island's rim
/// so they are looked down on over the edge, and a far ring over it.
const CLOUDBREAK: [Prop; 17] = [
    stack(-150.0, -230.0, -75.0, 85.0, 0.6),
    stack(-290.0, -380.0, -75.0, 110.0, 2.1),
    stack(120.0, -300.0, -75.0, 75.0, 4.0),
    stack(260.0, 90.0, -75.0, 105.0, 1.3),
    stack(-260.0, 160.0, -75.0, 100.0, 3.3),
    stack(60.0, 380.0, -75.0, 105.0, 5.0),
    stack(-460.0, -60.0, -75.0, 125.0, 0.2),
    stack(380.0, -300.0, -75.0, 100.0, 2.7),
    stack(-90.0, -480.0, -75.0, 115.0, 1.8),
    stack(-105.0, -70.0, -75.0, 62.0, 1.1),
    stack(118.0, -62.0, -75.0, 48.0, 2.9),
    stack(-72.0, 95.0, -75.0, 55.0, 0.4),
    stack(150.0, 140.0, -75.0, 90.0, 3.7),
    stack(-185.0, 40.0, -75.0, 70.0, 5.3),
    stack(40.0, -150.0, -75.0, 58.0, 2.0),
    stack(-20.0, 130.0, -75.0, 42.0, 4.4),
    stack(205.0, -175.0, -75.0, 95.0, 0.9),
];

/// The scenery of map `id`.
pub fn scenery(id: u8) -> &'static [Prop] {
    match id {
        3 => &DRY_FORK,
        7 => &CLOUDBREAK,
        _ => &[],
    }
}

/// Where a prop is drawn.
pub fn prop_transform(p: &Prop) -> Transform {
    let at = Vec3::from_array(p.at);
    let scale = if p.model == STACK {
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
    let stretch = |n: [f32; 3], turned: bool| {
        let (nx, nz) = if turned { (n[2], n[0]) } else { (n[0], n[2]) };
        let r = [size.x / nx, size.y / n[1], size.z / nz];
        let hi = r[0].max(r[1]).max(r[2]);
        let lo = r[0].min(r[1]).min(r[2]);
        hi / lo.max(1e-6)
    };
    let mut all = [(0usize, false, 0.0f32); BLOCKS * 2];
    for (i, n) in NATIVE.iter().enumerate() {
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
    Some((model, u8::from(turned) + 2 * u8::from(flip)))
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
    /// The stack's mesh with the ground's splat on it, for an island's
    /// stacks ([`STACK_SPLAT`]); made with the first one.
    ground_stack: Option<Handle<Mesh>>,
    failed: bool,
}

/// `mesh` carrying the ground's splat weights `w` on every vertex, so the
/// ground material draws it (`level::dress_level`).
pub fn with_splat(mesh: &Mesh, w: [f32; 4]) -> Mesh {
    let mut m = mesh.clone();
    let n = m.count_vertices();
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![w; n]);
    m
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
            let pale = if i == STACK {
                StandardMaterial {
                    base_color: Color::linear_rgb(PALE_STACK[0], PALE_STACK[1], PALE_STACK[2]),
                    base_color_texture: None,
                    ..dry.clone()
                }
            } else if i >= PYLON_MODEL {
                dry.clone()
            } else {
                tinted(PALE_TINT, DRY_ROUGH)
            };
            self.dry.push(mats.add(dry));
            self.wet.push(mats.add(wet));
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
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
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
    let Some(ok) = stones.ready(&server, &meshes, &mut mats) else {
        return;
    };
    for (e, b) in &boulders {
        let mut ec = commands.entity(e);
        ec.remove::<Boulder>();
        let Some((model, quarters)) = fit(b.max - b.min, b.key).filter(|_| ok) else {
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
        if ok && s.pale && s.model == STACK {
            // An island's stack in the island's own stone: the ground's
            // material over the stack's mesh carrying sand weights, which
            // `level::dress_level` gives it (and `Dressed` with it).
            let mesh = match &stones.ground_stack {
                Some(h) => h.clone(),
                None => {
                    let Some(made) = meshes.get(&stones.meshes[STACK]).map(|b| with_splat(b, STACK_SPLAT)) else {
                        continue;
                    };
                    let h = meshes.add(made);
                    stones.ground_stack = Some(h.clone());
                    h
                }
            };
            ec.remove::<Stone>()
                .insert((Mesh3d(mesh), crate::level::GroundMesh { sea: s.sea, isle: true }));
            continue;
        }
        ec.insert(Dressed);
        if !ok {
            continue;
        }
        let paint = if s.pale {
            &stones.pale
        } else if s.wet {
            &stones.wet
        } else {
            &stones.dry
        };
        ec.insert((
            Mesh3d(stones.meshes[s.model].clone()),
            MeshMaterial3d(paint[s.model].clone()),
        ));
    }
}

/// The scenery of map `id`, each piece waiting for its model; `pale` on an
/// island, whose cloud is at `sea`.
pub fn spawn_scenery(commands: &mut Commands, id: u8, pale: bool, sea: f32) {
    for p in scenery(id) {
        commands.spawn((
            LevelPart,
            Stone {
                model: p.model,
                wet: p.wet,
                pale,
                sea,
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
                let r = if p.model == STACK {
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

//! Trees: a palm for the beach and, for the canyon floor, a dead tree and
//! a live one (the dead tree's limbs under a crown of leaf cards), drawn
//! where a map puts a round solid with a tree look (`palm`, `dead_tree`:
//! which of that look's trees are in leaf is [`in_leaf`]'s, the same for a
//! tree and its twin across the map). level.rs spawns [`tree`]'s parts at
//! the solid's base with the solid's radius as the trunk's. Nothing here
//! reaches the sim: a tree collides as the cylinder the map says, and its
//! limbs, fronds and leaves are scenery, the same on every rung.
//!
//! After Gates' `render/tree.rs` (`palm`, `palm_crown`, `palm_bark_rgba`)
//! and `render/plants.rs` (`frond`, `frond_rgba`, `dead_scrub` and its
//! sticks), MIT, NOTICE: copied and trimmed, the dead tree grown from the
//! scrub to a tree's height. Both cards are drawn the first time a map
//! grows a tree ([`TreeCards`]), nothing on disk. Every tree is a pure function of its seed (a hash, no
//! rand), so every client builds the same trees. No wind yet.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use core::f32::consts::{PI, TAU};

use super::textures::{bake, Texel};

/// What a tree is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Palm,
    Dead,
    /// A dead tree's limbs in leaf.
    Live,
}

/// Whether the canyon tree standing at (x, z) is in leaf: about two in
/// three, by where it stands, read off `|x|` so a tree and its twin across
/// the map's mirror are the same kind and neither side has the shade.
pub fn in_leaf(x: f32, z: f32) -> bool {
    hash01((x.abs() * 4.0) as u32, (z * 4.0 + 4096.0) as u32 ^ 0x1eaf) < LIVE_SHARE
}

/// Trees along the rim: where a terrain stands high and near level just
/// outside the playable rectangle (a canyon's mesas over its walls), one
/// to a cell of [`RIM_STEP_M`] in a band [`RIM_OUT_M`] out from the bound,
/// most of the cells ([`RIM_SHARE`]), most of them in leaf. Nobody reaches
/// them and they stand behind the walls' crest, so they hide no one: they
/// are the skyline. `ground` is the terrain's height. Each: its foot, its
/// height, its kind and its seed.
pub fn rim(
    ground: impl Fn(f32, f32) -> Option<f32>,
    lo: (f32, f32),
    hi: (f32, f32),
    seed: u32,
) -> Vec<(Vec3, f32, Kind, u32)> {
    let mut out = Vec::new();
    let (x0, z0) = (lo.0 - RIM_OUT_M.1, lo.1 - RIM_OUT_M.1);
    let (nx, nz) = (
        ((hi.0 - lo.0 + 2.0 * RIM_OUT_M.1) / RIM_STEP_M).ceil() as u32,
        ((hi.1 - lo.1 + 2.0 * RIM_OUT_M.1) / RIM_STEP_M).ceil() as u32,
    );
    for j in 0..nz {
        for i in 0..nx {
            let cell = hash2(seed ^ 0x51a7_0b1e, i * 8191 + j);
            let x = x0 + (i as f32 + hash01(cell, 1)) * RIM_STEP_M;
            let z = z0 + (j as f32 + hash01(cell, 2)) * RIM_STEP_M;
            // Clear of the bound by the band's near edge, on some side.
            let out_by = (lo.0 - x).max(x - hi.0).max(lo.1 - z).max(z - hi.1);
            if !(RIM_OUT_M.0..=RIM_OUT_M.1).contains(&out_by) || hash01(cell, 3) >= RIM_SHARE {
                continue;
            }
            let (Some(y), Some(yx), Some(yz)) = (ground(x, z), ground(x + 1.5, z), ground(x, z + 1.5)) else {
                continue;
            };
            let slope = ((yx - y) / 1.5).abs().max(((yz - y) / 1.5).abs());
            if y < RIM_MIN_Y || slope > RIM_SLOPE {
                continue;
            }
            let kind = if hash01(cell, 4) < 0.8 { Kind::Live } else { Kind::Dead };
            let height = RIM_H.0 + (RIM_H.1 - RIM_H.0) * hash01(cell, 5);
            out.push((Vec3::new(x, y, z), height, kind, cell));
        }
    }
    out
}

/// One drawn part of a tree: its mesh, its material, and where it goes
/// relative to the trunk's base (the origin).
pub type Part = (Handle<Mesh>, Handle<StandardMaterial>, Transform);

/// The trees' shared cards: the frond (an alpha cutout) and the palm's
/// bark (albedo and normal, tiling).
pub struct Cards {
    pub frond: Handle<Image>,
    pub leaf: Handle<Image>,
    pub bark: Handle<Image>,
    pub bark_normal: Handle<Image>,
    /// The materials trees wear, by part and shade ([`Cards::wear`]): made
    /// the first time one is worn and shared from then on, so trees of a
    /// shade batch rather than each drawing on a material of its own.
    worn: [Option<Handle<StandardMaterial>>; WORN],
}

/// A tree's parts, each with its own material.
#[derive(Clone, Copy)]
enum Wear {
    PalmBark,
    Frond,
    DeadWood,
    LiveBark,
    Leaves,
}

/// Each tree is a shade lighter or darker than the next, from
/// `1 - SHADE_SPREAD` to `1 + SHADE_SPREAD`, on this many steps.
const SHADES: usize = 5;
const SHADE_SPREAD: f32 = 0.1;
const WORN: usize = 5 * SHADES;

impl Cards {
    /// The material `part` wears at `shade` (of [`SHADES`]).
    fn wear(&mut self, part: Wear, shade: usize, mats: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
        let slot = &mut self.worn[part as usize * SHADES + shade.min(SHADES - 1)];
        if let Some(h) = slot {
            return h.clone();
        }
        let v = 1.0 - SHADE_SPREAD + 2.0 * SHADE_SPREAD * shade as f32 / (SHADES - 1) as f32;
        let tint = Color::linear_rgb(v, v, v);
        let m = match part {
            Wear::PalmBark => StandardMaterial {
                base_color: tint,
                base_color_texture: Some(self.bark.clone()),
                normal_map_texture: Some(self.bark_normal.clone()),
                perceptual_roughness: 0.92,
                ..default()
            },
            Wear::Frond => StandardMaterial {
                base_color: tint,
                base_color_texture: Some(self.frond.clone()),
                perceptual_roughness: 0.8,
                specular_tint: Color::linear_rgb(0.3, 0.3, 0.3),
                alpha_mode: AlphaMode::Mask(0.5),
                cull_mode: None,
                double_sided: true,
                ..default()
            },
            Wear::DeadWood => StandardMaterial {
                base_color: tint,
                perceptual_roughness: 0.95,
                ..default()
            },
            Wear::LiveBark => StandardMaterial {
                base_color: Color::linear_rgb(v * LIVE_BARK[0], v * LIVE_BARK[1], v * LIVE_BARK[2]),
                perceptual_roughness: 0.95,
                ..default()
            },
            Wear::Leaves => StandardMaterial {
                base_color: tint,
                base_color_texture: Some(self.leaf.clone()),
                perceptual_roughness: 0.85,
                specular_tint: Color::linear_rgb(0.3, 0.3, 0.3),
                alpha_mode: AlphaMode::Mask(0.5),
                cull_mode: None,
                double_sided: true,
                ..default()
            },
        };
        slot.insert(mats.add(m)).clone()
    }
}

/// The cards, drawn the first time a map grows a tree and kept: a map with
/// no tree never pays for them, and the page's first seconds are what the
/// netcode's clock settles on.
#[derive(Resource, Default)]
pub struct TreeCards(Option<Cards>);

impl TreeCards {
    /// The cards, drawn now if they never were.
    pub fn get(&mut self, images: &mut Assets<Image>) -> &mut Cards {
        self.0.get_or_insert_with(|| {
            let [bark, bark_normal] = palm_bark_images();
            Cards {
                frond: images.add(frond_image()),
                leaf: images.add(cutout_image(leaf_rgba(LEAF_PX), LEAF_PX)),
                bark: images.add(bark),
                bark_normal: images.add(bark_normal),
                worn: Default::default(),
            }
        })
    }
}

/// Ceilings on one tree's triangles (`tests`): a palm, a dead tree.
pub const PALM_MAX_TRIS: usize = 8_000;
pub const DEAD_MAX_TRIS: usize = 2_000;
pub const LIVE_MAX_TRIS: usize = 3_000;

/// A tree's foot sinks this far below the origin, so a slope under it
/// shows no gap.
const SINK_M: f32 = 0.15;

/// Gates' palm is 12 m; its lean and its fronds are scaled from that.
const GATES_PALM_H: f32 = 12.0;
/// How far off the root the trunk's top leans, metres, by form: near
/// straight, an S, bowed over.
const PALM_LEAN_M: [(f32, f32); 3] = [(0.4, 1.0), (2.0, 3.0), (2.4, 3.4)];
/// The first stretch of trunk is straight, so the foot sits in the solid.
const PALM_FOOT_M: f32 = 1.5;
/// The crown rises this far over the trunk's top (before the fit).
const PALM_CROWN_RISE_M: f32 = 1.3;
/// A frond's length and width, metres, at Gates' height.
const PALM_FROND_M: f32 = 3.9;
const PALM_FROND_W: f32 = 1.0;
/// The frond card is a dark cycad green: a live frond lifts it to a sunlit
/// yellow-green, a dead one hangs straw-brown.
const PALM_FROND_TINT: [f32; 3] = [1.45, 1.35, 0.95];
const PALM_DEAD_TINT: [f32; 3] = [2.2, 1.45, 0.75];
/// The leaf-scar bands up the trunk, metres, and how far each one's lip
/// stands out of the trunk.
const PALM_BAND_M: f32 = 0.42;
const PALM_BAND_LIP: f32 = 0.06;
/// The trunk's radius under the solid's: with its lip on it just meets it.
const PALM_TRUNK_FIT: f32 = 0.94;
const PALM_SIDES: usize = 10;
const FROND_SEGS: usize = 10;
/// The cards' edges, texels.
const FROND_PX: u32 = 256;
const PALM_BARK_PX: usize = 256;

/// Weathered dead wood at the root and at the tips, linear: the grey of
/// wood the sun has had for years, silvered toward the tips (lighter than
/// Gates' scrub: against the canyon's sunlit sand the scrub's read as a
/// silhouette).
const DEAD_ROOT: [f32; 3] = [0.075, 0.064, 0.052];
const DEAD_TIP: [f32; 3] = [0.24, 0.225, 0.2];
/// The trunk's share of a dead tree's height; the limbs reach the rest.
const DEAD_TRUNK_SHARE: f32 = 0.62;
/// The trunk is straight (inside the solid) this far up, then wanders.
const DEAD_STRAIGHT_M: f32 = 2.5;
/// How many limbs leave the trunk, and how many times each forks.
const DEAD_LIMBS: (u32, u32) = (4, 6);
const DEAD_FORKS: u32 = 3;
const DEAD_TRUNK_SIDES: usize = 8;
/// The rim's trees ([`rim`]): a cell's side, how far out from the bound
/// the band runs, how high and how near level the ground must be, the
/// share of cells that grow one, and a tree's height and trunk.
pub const RIM_STEP_M: f32 = 14.0;
pub const RIM_OUT_M: (f32, f32) = (4.0, 28.0);
pub const RIM_MIN_Y: f32 = 20.0;
pub const RIM_SLOPE: f32 = 0.45;
pub const RIM_SHARE: f32 = 0.8;
pub const RIM_H: (f32, f32) = (8.0, 13.0);
pub const RIM_TRUNK_R: f32 = 0.3;
/// The share of the canyon's trees that are in leaf ([`in_leaf`]).
const LIVE_SHARE: f32 = 0.66;
/// A live tree's bark over the dead wood's grey: darker and browner.
const LIVE_BARK: [f32; 3] = [0.62, 0.5, 0.42];
/// A leaf card's half-size, metres, least and most, on a tree 8 m tall;
/// how many stand at each twig's end, and how far the cards' normals are
/// turned out from the crown's middle, so it is lit as one mass.
const LEAF_CARD_M: (f32, f32) = (0.62, 1.0);
const LEAF_CARDS: u32 = 3;
const LEAF_VOLUME: f32 = 0.75;
/// The leaf card's texels a side and how many leaves it carries.
const LEAF_PX: u32 = 256;
const LEAF_COUNT: u32 = 110;
/// A leaf's colours, sRGB: the blade and where the sun is on it.
const LEAF_BLADE: [f32; 3] = [44.0, 72.0, 30.0];
const LEAF_SUN: [f32; 3] = [98.0, 124.0, 52.0];

/// The parts of a `kind` of tree grown from `seed`, its trunk `trunk_r`
/// across at the foot (the solid's radius) and `height` tall, relative to
/// the trunk's base at the origin.
pub fn tree(
    kind: Kind,
    seed: u32,
    trunk_r: f32,
    height: f32,
    cards: &mut Cards,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
) -> Vec<Part> {
    // Each tree wears the cards a shade lighter or darker than the next.
    let shade = ((hash01(seed, 0x7e11) * SHADES as f32) as usize).min(SHADES - 1);
    let mut wear = |part| cards.wear(part, shade, mats);
    match kind {
        Kind::Palm => {
            let (bark, crown) = palm(seed, trunk_r, height);
            vec![
                (meshes.add(bark), wear(Wear::PalmBark), Transform::IDENTITY),
                (meshes.add(crown), wear(Wear::Frond), Transform::IDENTITY),
            ]
        }
        Kind::Dead => vec![(
            meshes.add(dead(seed, trunk_r, height)),
            wear(Wear::DeadWood),
            Transform::IDENTITY,
        )],
        Kind::Live => {
            let (wood, crown) = live(seed, trunk_r, height);
            vec![
                (meshes.add(wood), wear(Wear::LiveBark), Transform::IDENTITY),
                (meshes.add(crown), wear(Wear::Leaves), Transform::IDENTITY),
            ]
        }
    }
}

/// A palm, `(bark, crown)`: the seed picks one of three forms, near
/// straight with a slight lean, curved in an S, or bowed well over, and the
/// trunk follows that spine as a tube ringed with leaf-scar lips wearing
/// the diamond bark card. The crown is [`palm_crown`]. Fitted to `height`
/// exactly, the crown's tip at the top.
fn palm(seed: u32, trunk_r: f32, height: f32) -> (Mesh, Mesh) {
    let seed = hash2(0x9a1e_77a0, seed);
    let form = (hash2(seed, 0) % 3) as usize;
    let a = hash01(seed, 1) * TAU;
    let dir = Vec3::new(a.sin(), 0.0, a.cos());
    let (lo, hi) = PALM_LEAN_M[form];
    let lean = (lo + (hi - lo) * hash01(seed, 2)) * height / GATES_PALM_H;
    let h = (height - PALM_CROWN_RISE_M).max(2.0 * PALM_FOOT_M);
    let foot = PALM_FOOT_M / h;
    let spine = |t: f32| -> Vec3 {
        let u = ((t - foot) / (1.0 - foot)).clamp(0.0, 1.0);
        let bend = match form {
            0 => u * u,
            1 => (1.0 - (u * PI).cos()) * 0.5,
            _ => u.powf(1.6),
        };
        dir * (lean * bend) + Vec3::Y * (t * h)
    };
    let n = ((h / PALM_BAND_M).round() as usize).max(2) * 3;
    let r0 = trunk_r * PALM_TRUNK_FIT;
    let rings: Vec<Ring> = (0..=n)
        .map(|j| {
            let t = j as f32 / n as f32;
            let y = t * h;
            // Each band's lip at its foot, easing back up to the next.
            let b = (y / PALM_BAND_M).fract();
            let lip = 1.0 + PALM_BAND_LIP * (1.0 - b) * (1.0 - b);
            Ring {
                c: spine(t) - Vec3::Y * SINK_M,
                r: r0 * (1.0 - 0.2 * t) * lip,
                // The card is two bands tall, its v growing down the trunk.
                v: -y / (2.0 * PALM_BAND_M),
            }
        })
        .collect();
    let mut bark = Soup::default();
    // The foot is grimed: darker over the first stretch of trunk.
    let grime = move |p: Vec3| {
        let k = 0.78 + 0.22 * smooth01(p.y / (0.35 * h));
        [k, k, k, 1.0]
    };
    tube(&mut bark, &rings, PALM_SIDES, 2.0, grime);
    // The crown's foot: a cap of the trunk under the fronds.
    let (top, top_r) = (rings[n].c, rings[n].r);
    let (side, other) = (dir, Vec3::new(dir.z, 0.0, -dir.x));
    for i in 0..PALM_SIDES {
        let p = |k: usize| {
            let a = k as f32 / PALM_SIDES as f32 * TAU;
            top + (side * a.cos() + other * a.sin()) * top_r
        };
        bark.tri_uv(
            [
                (p(i), [0.1, 0.0]),
                (p(i + 1), [0.0, 0.0]),
                (top + Vec3::Y * 0.15, [0.05, 0.1]),
            ],
            grime,
            None,
            0.0,
        );
    }
    let mut bark = bark.mesh();
    let mut crown = palm_crown(seed, top, height);
    fit_height(&mut [&mut bark, &mut crown], height);
    (bark, crown)
}

/// A palm's crown: thirty-odd long fronds of the frond card arching off
/// `top`, the young ones upright at the heart and the old ones drooping
/// under them, a few of the oldest dead and hanging straw-brown against
/// the trunk.
fn palm_crown(seed: u32, top: Vec3, height: f32) -> Mesh {
    let seed = hash2(0x9a1e_c400, seed);
    let mut s = Soup::default();
    // A shorter palm's fronds are a little shorter, not by as much.
    let frond_m = PALM_FROND_M * (height / GATES_PALM_H).sqrt();
    let dome = top - Vec3::Y * frond_m;
    let n = 30 + (hash01(seed, 1) * 9.0) as u32;
    let yaw0 = hash01(seed, 2) * TAU;
    for k in 0..n {
        // 0 the youngest frond at the heart, 1 the oldest under the crown.
        let age = k as f32 / (n - 1) as f32;
        let a = yaw0 + k as f32 * 2.399_963 + (hash01(seed, 10 + k) - 0.5) * 0.3;
        let out = Vec3::new(a.sin(), 0.0, a.cos());
        let side = Vec3::new(out.z, 0.0, -out.x);
        let dead = age > 0.82 && hash01(seed, 20 + k) < 0.55;
        let len = frond_m * (0.62 + 0.38 * age.min(0.8) / 0.8) * (0.88 + 0.24 * hash01(seed, 30 + k));
        let (rise, droop) = if dead {
            (-1.15, 0.2)
        } else {
            (
                1.15 - 1.25 * age + (hash01(seed, 40 + k) - 0.5) * 0.3,
                0.6 + 0.9 * age + (hash01(seed, 50 + k) - 0.5) * 0.3,
            )
        };
        let base = top + out * 0.18 - Vec3::Y * (0.9 * age);
        let mut spine = [base; FROND_SEGS + 1];
        for i in 1..=FROND_SEGS {
            let t = (i as f32 - 0.5) / FROND_SEGS as f32;
            let e = rise - droop * t * t;
            spine[i] = spine[i - 1] + (out * e.cos() + Vec3::Y * e.sin()) * (len / FROND_SEGS as f32);
        }
        let tint = if dead { PALM_DEAD_TINT } else { PALM_FROND_TINT };
        let value = 0.85 + 0.25 * hash01(seed, 60 + k) - 0.2 * age;
        let col = move |p: Vec3| {
            // Darker toward the heart, where the crown shades itself.
            let r = (p - top).length() / frond_m;
            let k = value * (0.6 + 0.4 * (r * 1.5).min(1.0));
            [tint[0] * k, tint[1] * k, tint[2] * k, 1.0]
        };
        let fold = 0.4 + 0.25 * hash01(seed, 70 + k);
        let width = PALM_FROND_W * if dead { 0.7 } else { 1.0 };
        // A dead frond hangs close under the crown, not down the trunk.
        if dead {
            for p in spine.iter_mut() {
                *p = base + (*p - base) * 0.55;
            }
        }
        // The leaflets' tips hang off the arch.
        let tip = -0.75 - 0.35 * hash01(seed, 80 + k);
        frond(&mut s, &spine, side, width, fold, tip, col, dome, 0.6);
    }
    s.mesh()
}

/// One frond card bent along `spine` (base first), `width` across and
/// folded `fold` radians up off its rachis either side, `side` the
/// horizontal across it; the outer half of each side hangs `tip` more (a
/// palm frond's arch). Normals are pulled `blend` of the way toward `dome`.
#[allow(clippy::too_many_arguments)]
fn frond(
    s: &mut Soup,
    spine: &[Vec3],
    side: Vec3,
    width: f32,
    fold: f32,
    tip: f32,
    col: impl Fn(Vec3) -> [f32; 4] + Copy,
    dome: Vec3,
    blend: f32,
) {
    let segs = spine.len() - 1;
    let up = |i: usize| {
        let along = (spine[(i + 1).min(segs)] - spine[i.saturating_sub(1)]).normalize_or(Vec3::Y);
        along.cross(side).normalize_or(Vec3::Y)
    };
    // Across one side of the card: the rachis, then two strips out to the
    // leaflets' tips, each `(u, point)`.
    let strips = [(fold, 0.25), (fold + tip, 0.25)];
    let across = |i: usize, sign: f32| {
        let mut p = spine[i];
        let mut u = 0.5;
        let mut out = [(0.5, p); 3];
        for (k, &(f, share)) in strips.iter().enumerate() {
            let w = width * share;
            p += side * (sign * w * f.cos()) + up(i) * (w * f.sin());
            u += sign * share;
            out[k + 1] = (u, p);
        }
        out
    };
    for i in 0..segs {
        let (v0, v1) = (1.0 - i as f32 / segs as f32, 1.0 - (i + 1) as f32 / segs as f32);
        for sign in [-1.0, 1.0] {
            let (a, b) = (across(i, sign), across(i + 1, sign));
            for k in 0..strips.len() {
                let ((ua0, a0), (ua1, a1)) = (a[k], a[k + 1]);
                let ((ub0, b0), (ub1, b1)) = (b[k], b[k + 1]);
                s.tri_uv(
                    [(a0, [ua0, v0]), (a1, [ua1, v0]), (b0, [ub0, v1])],
                    col,
                    Some(dome),
                    blend,
                );
                s.tri_uv(
                    [(a1, [ua1, v0]), (b1, [ub1, v1]), (b0, [ub0, v1])],
                    col,
                    Some(dome),
                    blend,
                );
            }
        }
    }
}

/// A dead tree: a bare trunk, straight through the solid and wandering a
/// little above it, and four to six limbs off its upper half, each forking
/// three times into thinner sticks, the topmost carrying the trunk on as
/// its leader; grey wood in the vertices, no card. Fitted to `height`.
fn dead(seed: u32, trunk_r: f32, height: f32) -> Mesh {
    let (s, _) = skeleton(seed, trunk_r, height);
    let mut m = s.mesh();
    fit_height(&mut [&mut m], height);
    m
}

/// A live tree, `(wood, crown)`: the dead tree's limbs, and at every
/// twig's end a few leaf cards crossed through it, their normals turned
/// out from the crown's middle. Fitted to `height` together.
fn live(seed: u32, trunk_r: f32, height: f32) -> (Mesh, Mesh) {
    let (wood, tips) = skeleton(seed, trunk_r, height);
    let seed = hash2(0x1eaf_c4a2, seed);
    let mut crown = Soup::default();
    let middle = tips.iter().copied().sum::<Vec3>() / tips.len().max(1) as f32;
    let white = |_: Vec3| [1.0, 1.0, 1.0, 1.0];
    let k = height / 8.0;
    for (i, &tip) in tips.iter().enumerate() {
        let i = i as u32;
        let half = (LEAF_CARD_M.0 + (LEAF_CARD_M.1 - LEAF_CARD_M.0) * hash01(seed, i * 7 + 1)) * k;
        let a0 = hash01(seed, i * 7 + 2) * TAU;
        for c in 0..LEAF_CARDS {
            let a = a0 + c as f32 * PI / LEAF_CARDS as f32;
            let lean = (hash01(seed, i * 7 + 3 + c) - 0.5) * 0.9;
            let across = Vec3::new(a.cos(), 0.0, a.sin());
            let up = (Vec3::Y + Vec3::new(-a.sin(), 0.0, a.cos()) * lean).normalize_or(Vec3::Y);
            let (u, v) = (across * half, up * half);
            // Hung a little under the twig's end, so the twig is in it.
            let o = tip + up * half * 0.35;
            let q = [
                (o - u - v, [0.0, 1.0]),
                (o + u - v, [1.0, 1.0]),
                (o + u + v, [1.0, 0.0]),
                (o - u + v, [0.0, 0.0]),
            ];
            crown.tri_uv([q[0], q[1], q[2]], white, Some(middle), LEAF_VOLUME);
            crown.tri_uv([q[0], q[2], q[3]], white, Some(middle), LEAF_VOLUME);
        }
    }
    let (mut wood, mut crown) = (wood.mesh(), crown.mesh());
    fit_height(&mut [&mut wood, &mut crown], height);
    (wood, crown)
}

/// The dead tree's wood before it is fitted, and where each of its twigs
/// ends (a live tree's leaves hang there).
fn skeleton(seed: u32, trunk_r: f32, height: f32) -> (Soup, Vec<Vec3>) {
    let seed = hash2(0xdead_7ee5, seed);
    let mut s = Soup::default();
    let mut tips = Vec::new();
    let col = move |p: Vec3| {
        let c = lerp3(DEAD_ROOT, DEAD_TIP, smooth01(p.y / height));
        // Streaks and flecks of weathering, at a hand's scale.
        let cell = ((p.x * 9.0 + 500.0) as u32) ^ (((p.y * 23.0 + 500.0) as u32) << 8);
        let f = 0.9 + 0.2 * hash01(cell, (p.z * 9.0 + 500.0) as u32);
        [c[0] * f, c[1] * f, c[2] * f, 1.0]
    };
    let top_y = height * DEAD_TRUNK_SHARE;
    let r0 = trunk_r * 0.95;
    let mut rings = vec![
        Ring {
            c: Vec3::new(0.0, -SINK_M, 0.0),
            r: r0 * 1.12,
            v: 0.0,
        },
        Ring {
            c: Vec3::new(0.0, 0.5, 0.0),
            r: r0,
            v: 0.0,
        },
        Ring {
            c: Vec3::new(0.0, DEAD_STRAIGHT_M.min(top_y * 0.6), 0.0),
            r: r0 * 0.9,
            v: 0.0,
        },
    ];
    let steps = 4;
    let mut c = rings[2].c;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        c = Vec3::new(
            c.x + (hash01(seed, 100 + i) - 0.5) * 0.5,
            rings[2].c.y + (top_y - rings[2].c.y) * t,
            c.z + (hash01(seed, 110 + i) - 0.5) * 0.5,
        );
        rings.push(Ring {
            c,
            r: r0 * (0.9 - 0.55 * t),
            v: 0.0,
        });
    }
    tube(&mut s, &rings, DEAD_TRUNK_SIDES, 1.0, col);
    let n = DEAD_LIMBS.0 + (hash01(seed, 1) * (DEAD_LIMBS.1 - DEAD_LIMBS.0 + 1) as f32) as u32;
    let n = n.min(DEAD_LIMBS.1);
    let a0 = hash01(seed, 2) * TAU;
    for k in 0..n {
        // 0 the lowest limb, 1 the leader at the top.
        let t = k as f32 / (n - 1) as f32;
        let a = a0 + k as f32 * 2.399_963 + (hash01(seed, 10 + k) - 0.5) * 0.6;
        let elev = if k == n - 1 {
            1.25
        } else {
            0.5 + 0.55 * hash01(seed, 20 + k)
        };
        let (base, r) = trunk_at(&rings, top_y * (0.45 + 0.55 * t));
        // The leader carries the trunk on; a limb is thinner than the trunk it leaves.
        let r = if k == n - 1 { r * 0.9 } else { r * 0.55 };
        let len = height * (0.18 + 0.08 * hash01(seed, 30 + k)) * (0.85 + 0.15 * t);
        let dir = Vec3::new(a.sin() * elev.cos(), elev.sin(), a.cos() * elev.cos());
        branch(
            &mut s,
            &mut tips,
            seed ^ (k + 1).wrapping_mul(0x9e37),
            base,
            dir,
            len,
            r,
            0,
            col,
        );
    }
    (s, tips)
}

/// One limb from `a` along `dir`, kinked halfway, forking in two at its
/// end until [`DEAD_FORKS`] deep; thinner and fewer-sided as it goes.
#[allow(clippy::too_many_arguments)]
fn branch(
    s: &mut Soup,
    tips: &mut Vec<Vec3>,
    seed: u32,
    a: Vec3,
    dir: Vec3,
    len: f32,
    r: f32,
    depth: u32,
    col: impl Fn(Vec3) -> [f32; 4] + Copy,
) {
    let kink = Vec3::new(
        hash01(seed, 1) - 0.5,
        (hash01(seed, 2) - 0.5) * 0.4,
        hash01(seed, 3) - 0.5,
    );
    let mid = a + (dir + kink * 0.5).normalize_or(dir) * len * 0.5;
    let end = mid + (dir - kink * 0.3).normalize_or(dir) * len * 0.5;
    let sides = match depth {
        0 => 6,
        1 | 2 => 5,
        _ => 4,
    };
    let rings = [
        Ring { c: a, r, v: 0.0 },
        Ring {
            c: mid,
            r: r * 0.8,
            v: 0.5,
        },
        Ring {
            c: end,
            r: (r * 0.6).max(0.006),
            v: 1.0,
        },
    ];
    tube(s, &rings, sides, 1.0, col);
    // A twig's end, from the first fork out.
    if depth >= 1 {
        tips.push(end);
    }
    if depth >= DEAD_FORKS {
        return;
    }
    // The fork's plane is rolled about the limb, so forks go every way.
    let on = (end - mid).normalize_or(dir);
    let roll = Quat::from_axis_angle(on, hash01(seed, 4) * PI);
    let axis = roll * on.cross(Vec3::Y).normalize_or(Vec3::X);
    for (k, side) in [(0u32, -1.0f32), (1, 1.0)] {
        let spread = 0.3 + 0.35 * hash01(seed, 10 + k);
        let d = (Quat::from_axis_angle(axis, side * spread) * on + Vec3::Y * 0.12).normalize_or(on);
        let next = seed.wrapping_mul(0x2c1b_3c6d) ^ (k + 1);
        branch(s, tips, next, end, d, len * 0.62, r * 0.6, depth + 1, col);
    }
}

/// Where the trunk through `rings` passes height `y`, and its radius there.
fn trunk_at(rings: &[Ring], y: f32) -> (Vec3, f32) {
    for w in rings.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if y <= b.c.y && b.c.y > a.c.y {
            let t = ((y - a.c.y) / (b.c.y - a.c.y)).clamp(0.0, 1.0);
            return (a.c.lerp(b.c, t), a.r + (b.r - a.r) * t);
        }
    }
    let last = &rings[rings.len() - 1];
    (last.c, last.r)
}

/// Scales the meshes vertically about the origin so the tallest point is
/// at `height` (Gates' `grow` stretch: height is a parameter, width a
/// consequence).
fn fit_height(meshes: &mut [&mut Mesh], height: f32) {
    let mut top = 0.0f32;
    for m in meshes.iter() {
        if let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) {
            for v in p {
                top = top.max(v[1]);
            }
        }
    }
    if top <= f32::EPSILON {
        return;
    }
    let k = height / top;
    for m in meshes.iter_mut() {
        if let Some(VertexAttributeValues::Float32x3(p)) = m.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
            for v in p.iter_mut() {
                v[1] *= k;
            }
        }
    }
}

/// A ring of a tube: its centre, its radius, and the texture's v there.
struct Ring {
    c: Vec3,
    r: f32,
    v: f32,
}

/// A tube through `rings`, `sides` round, its texture `u_rep` times round
/// and `v` as each ring says, its normals radial, wound outward. The
/// rings' frames are carried along the spine (parallel transport), so a
/// bend never twists the facets.
fn tube(s: &mut Soup, rings: &[Ring], sides: usize, u_rep: f32, col: impl Fn(Vec3) -> [f32; 4] + Copy) {
    let n = rings.len();
    if n < 2 {
        return;
    }
    let along = |i: usize| (rings[(i + 1).min(n - 1)].c - rings[i.saturating_sub(1)].c).normalize_or(Vec3::Y);
    let mut side = along(0).cross(Vec3::X).normalize_or(Vec3::Z);
    let frames: Vec<(Vec3, Vec3)> = (0..n)
        .map(|i| {
            let a = along(i);
            side = (side - a * side.dot(a)).normalize_or(a.cross(Vec3::X).normalize_or(Vec3::Z));
            (side, a.cross(side))
        })
        .collect();
    let at = |i: usize, k: usize| -> (Vec3, Vec3) {
        let ang = k as f32 / sides as f32 * TAU;
        let (sd, ot) = frames[i];
        let rad = sd * ang.cos() + ot * ang.sin();
        (rings[i].c + rad * rings[i].r, rad)
    };
    for i in 0..n - 1 {
        let (v0, v1) = (rings[i].v, rings[i + 1].v);
        for k in 0..sides {
            let (u0, u1) = (k as f32 / sides as f32 * u_rep, (k + 1) as f32 / sides as f32 * u_rep);
            let (a0, n0) = at(i, k);
            let (a1, n1) = at(i, k + 1);
            let (b0, m0) = at(i + 1, k);
            let (b1, m1) = at(i + 1, k + 1);
            s.tri_n([(a0, n0, [u0, v0]), (a1, n1, [u1, v0]), (b0, m0, [u0, v1])], col);
            s.tri_n([(a1, n1, [u1, v0]), (b1, m1, [u1, v1]), (b0, m0, [u0, v1])], col);
        }
    }
}

/// Triangles as they are built: a vertex per corner, with its normal,
/// colour and texture coordinates (Gates' `props::Soup`, the two calls the
/// trees need).
#[derive(Default)]
struct Soup {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
    uv: Vec<[f32; 2]>,
}

impl Soup {
    /// A triangle with each corner's normal and texture coordinates given.
    fn tri_n(&mut self, corners: [(Vec3, Vec3, [f32; 2]); 3], col: impl Fn(Vec3) -> [f32; 4]) {
        for (p, n, uv) in corners {
            self.pos.push(p.to_array());
            self.nor.push(n.to_array());
            self.col.push(col(p));
            self.uv.push(uv);
        }
    }

    /// A triangle with its corners' texture coordinates given and a facet
    /// normal, blended `blend` of the way toward the direction out of
    /// `centre`: 0 a flat plate, 1 a soft volume (a crown of cards lit as
    /// a mass, not as thirty plates).
    fn tri_uv(
        &mut self,
        corners: [(Vec3, [f32; 2]); 3],
        col: impl Fn(Vec3) -> [f32; 4],
        centre: Option<Vec3>,
        blend: f32,
    ) {
        let (a, b, c) = (corners[0].0, corners[1].0, corners[2].0);
        let facet = (b - a).cross(c - a).normalize_or(Vec3::Y);
        for (p, uv) in corners {
            let n = match centre {
                Some(ctr) => (facet * (1.0 - blend) + (p - ctr).normalize_or_zero() * blend).normalize_or(facet),
                None => facet,
            };
            self.pos.push(p.to_array());
            self.nor.push(n.to_array());
            self.col.push(col(p));
            self.uv.push(uv);
        }
    }

    fn mesh(self) -> Mesh {
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv);
        // The bark's normal map needs tangents; the rest keep the one
        // vertex layout. Never a panic at boot over a thin triangle.
        let _ = m.generate_tangents();
        m
    }
}

/// The frond card: a rachis up the middle (base at the bottom edge) and
/// some forty pairs of stiff, narrow leaflets angled toward the tip,
/// shortest at the base and the tip. An alpha cutout with its mip chain.
fn frond_image() -> Image {
    cutout_image(frond_rgba(FROND_PX), FROND_PX)
}

/// A cutout card (`level0`, RGBA8 sRGB, `n` texels square) as an image with
/// its mip chain.
fn cutout_image(level0: Vec<u8>, n: u32) -> Image {
    let (chain, levels) = cutout_chain(&level0, n);
    let mut img = Image::new(
        Extent3d {
            width: n,
            height: n,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        level0,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.data = Some(chain);
    img.texture_descriptor.mip_level_count = levels;
    // Trilinear, anisotropic (a frond is mostly seen at an angle), and not
    // tiling: a wrapped cutout bleeds its far edge into the near.
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: if cfg!(webgl2) { 4 } else { 16 },
        ..default()
    });
    img
}

/// The frond card's level 0, RGBA8 sRGB, `n` texels square: coverage
/// supersampled into the alpha. The clear texels carry the blade's colour,
/// so a mip never fringes.
fn frond_rgba(n: u32) -> Vec<u8> {
    // sRGB: the leaflet, its sunlit tip, and the rachis.
    let blade = Vec3::new(52.0, 70.0, 28.0);
    let tip = Vec3::new(88.0, 100.0, 44.0);
    let rib = Vec3::new(104.0, 100.0, 56.0);
    // Leaflet pairs, the angle a leaflet leaves the rachis at (card space,
    // measured off the rachis toward the tip), and its half-width.
    const PAIRS: f32 = 40.0;
    let ang: f32 = 1.2;
    let (dir_u, dir_v) = (ang.sin(), ang.cos());
    let leaf_w = 0.011;
    // A leaflet's reach off the rachis at `t` along the frond (0 the base).
    let reach = |t: f32| 0.47 * smooth01(t / 0.28) * (1.0 - 0.75 * smooth01((t - 0.6) / 0.4));
    // `Some((f, id))`: how far out along leaflet `id` the point lies, or
    // `f` negative on the rachis.
    let sample = |u: f32, v: f32| -> Option<(f32, u32)> {
        let t = 1.0 - v;
        let x = u - 0.5;
        // The rachis, tapering toward the tip.
        if x.abs() < 0.012 * (1.0 - 0.6 * t) && (0.0..0.985).contains(&t) {
            return Some((-1.0, 0));
        }
        // Which leaflet's line passes nearest: walk back along its slant to
        // where it left the rachis.
        let ax = x.abs();
        let t0 = t - ax / dir_u * dir_v;
        let i = (t0 * PAIRS).round();
        let mut best: Option<(f32, u32)> = None;
        for di in [-1.0, 0.0, 1.0] {
            let left = x < 0.0;
            let tl = (i + di) / PAIRS + if left { 0.5 / PAIRS } else { 0.0 };
            if !(0.04..0.99).contains(&tl) {
                continue;
            }
            let r = reach(tl);
            // Along and across this leaflet, card units.
            let (du, dv) = (ax, t - tl);
            let along = du * dir_u + dv * dir_v;
            let across = (du * dir_v - dv * dir_u).abs();
            let f = along / (r / dir_u).max(1e-4);
            if !(0.0..=1.0).contains(&f) {
                continue;
            }
            // Narrow, and pointed at the tip.
            let w = leaf_w * (1.0 - f * f).max(0.0).sqrt().max(0.25 * (1.0 - f));
            if across <= w && best.is_none_or(|b| f < b.0) {
                best = Some((f, (i + di + 2.0) as u32 * 2 + u32::from(left)));
            }
        }
        best
    };
    const SS: u32 = 4;
    let nf = n as f32;
    let mut out = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            let mut cover = 0u32;
            let mut hit: Option<(f32, u32)> = None;
            for sy in 0..SS {
                for sx in 0..SS {
                    let u = (x as f32 + (sx as f32 + 0.5) / SS as f32) / nf;
                    let v = (y as f32 + (sy as f32 + 0.5) / SS as f32) / nf;
                    if let Some(h) = sample(u, v) {
                        cover += 1;
                        hit = hit.or(Some(h));
                    }
                }
            }
            let i = ((y * n + x) * 4) as usize;
            let c = match hit {
                None => blade,
                Some((f, _)) if f < 0.0 => rib,
                Some((f, id)) => {
                    let jitter = 0.88 + 0.24 * hash01(0xf20d, id);
                    blade.lerp(tip, smooth01((f - 0.4) / 0.6)) * jitter
                }
            };
            let a = (cover * 255 / (SS * SS)) as u8;
            out[i..i + 4].copy_from_slice(&[c.x.min(255.0) as u8, c.y.min(255.0) as u8, c.z.min(255.0) as u8, a]);
        }
    }
    out
}

/// The leaf card's level 0, RGBA8 sRGB, `n` texels square: a clump of
/// small pointed leaves every way round the card's middle, thick there and
/// ragged at its rim, each its own green. The clear texels carry the
/// blade's colour, so a mip never fringes.
fn leaf_rgba(n: u32) -> Vec<u8> {
    let blade = Vec3::from_array(LEAF_BLADE);
    let sun = Vec3::from_array(LEAF_SUN);
    let nf = n as f32;
    let mut out = vec![0u8; (n * n * 4) as usize];
    for p in out.chunks_mut(4) {
        p.copy_from_slice(&[blade.x as u8, blade.y as u8, blade.z as u8, 0]);
    }
    const SS: u32 = 2;
    for l in 0..LEAF_COUNT {
        // Where it is, which way it points, how long and how wide.
        let r = 0.42 * hash01(0x1eaf, l * 5).sqrt();
        let at = hash01(0x1eaf, l * 5 + 1) * TAU;
        let (cx, cy) = (0.5 + r * at.cos(), 0.5 + r * at.sin());
        let dir = hash01(0x1eaf, l * 5 + 2) * TAU;
        let (dx, dy) = (dir.cos(), dir.sin());
        let len = 0.05 + 0.035 * hash01(0x1eaf, l * 5 + 3);
        let wid = len * 0.36;
        let lit = hash01(0x1eaf, l * 5 + 4);
        let (x0, x1) = (
            ((cx - len) * nf).floor().max(0.0) as u32,
            ((cx + len) * nf).ceil().min(nf - 1.0) as u32,
        );
        let (y0, y1) = (
            ((cy - len) * nf).floor().max(0.0) as u32,
            ((cy + len) * nf).ceil().min(nf - 1.0) as u32,
        );
        for y in y0..=y1 {
            for x in x0..=x1 {
                let mut cover = 0u32;
                let mut along_sum = 0.0;
                for sy in 0..SS {
                    for sx in 0..SS {
                        let u = (x as f32 + (sx as f32 + 0.5) / SS as f32) / nf - cx;
                        let v = (y as f32 + (sy as f32 + 0.5) / SS as f32) / nf - cy;
                        let along = (u * dx + v * dy) / len;
                        let across = (-u * dy + v * dx) / wid;
                        // Pointed at both ends: a lens.
                        if along.abs() < 1.0 && across.abs() < 1.0 - along * along {
                            cover += 1;
                            along_sum += along;
                        }
                    }
                }
                if cover == 0 {
                    continue;
                }
                let i = ((y * n + x) * 4) as usize;
                let a = (cover * 255 / (SS * SS)) as u8;
                if a < out[i + 3] {
                    continue;
                }
                // Lighter toward its tip, and each leaf its own value.
                let t = smooth01(0.5 + 0.5 * along_sum / cover as f32);
                let c = blade.lerp(sun, 0.25 * t + 0.75 * lit * lit) * (0.85 + 0.3 * hash01(0xb1ade, l));
                out[i..i + 4].copy_from_slice(&[c.x.min(255.0) as u8, c.y.min(255.0) as u8, c.z.min(255.0) as u8, a]);
            }
        }
    }
    out
}

/// A cutout's mip chain from its level 0 (`n` square): box filtered, each
/// level's alpha scaled so it keeps level 0's coverage at the 0.5 test
/// (Gates' `mipmap::Filter::Mask`, as textures.rs does for the grass):
/// averaged alone, a frond thins to nothing a few levels down.
fn cutout_chain(level0: &[u8], n: u32) -> (Vec<u8>, u32) {
    let covered = |d: &[u8], k: f32| d.chunks(4).filter(|p| p[3] as f32 * k >= 128.0).count() as f32;
    let want = covered(level0, 1.0) / (n * n) as f32;
    let mut out = level0.to_vec();
    let (mut cur, mut w, mut levels) = (level0.to_vec(), n as usize, 1u32);
    while w > 1 {
        let h = w / 2;
        let mut next = vec![0u8; h * h * 4];
        for y in 0..h {
            for x in 0..h {
                for c in 0..4 {
                    let sum: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                        .iter()
                        .map(|(dx, dy)| cur[((2 * y + dy) * w + 2 * x + dx) * 4 + c] as u32)
                        .sum();
                    next[(y * h + x) * 4 + c] = ((sum + 2) / 4) as u8;
                }
            }
        }
        // The scale that brings the coverage back, by bisection.
        let (mut lo, mut hi) = (0.25f32, 8.0f32);
        for _ in 0..16 {
            let k = (lo + hi) * 0.5;
            if covered(&next, k) / ((h * h) as f32) < want {
                lo = k;
            } else {
                hi = k;
            }
        }
        let k = (lo + hi) * 0.5;
        for p in next.chunks_mut(4) {
            p[3] = (p[3] as f32 * k).min(255.0) as u8;
        }
        out.extend_from_slice(&next);
        cur = next;
        w = h;
        levels += 1;
    }
    (out, levels)
}

/// The palm's bark, `[albedo, normal]`, tiling: rows of diamond leaf-scars,
/// each a plate whose lower lip stands proud over the one below, in
/// fibrous grey-brown. Four across and two down per tile, matching the
/// trunk's UVs (two repeats round, one tile per two bands), so a scar is
/// about [`PALM_BAND_M`] tall. Baked as the ship's plating is.
fn palm_bark_images() -> [Image; 2] {
    let n = PALM_BARK_PX;
    let nf = n as f32;
    let hmap: Vec<(f32, u32)> = (0..n * n)
        .map(|i| scar_height(((i % n) as f32 + 0.5) / nf, ((i / n) as f32 + 0.5) / nf))
        .collect();
    // Streaks along the plates: per column, broken every few texels.
    let fibre =
        |x: usize, y: usize| hash01((x * 7 + 3) as u32, (y / 9) as u32) * 0.6 + hash01(x as u32, y as u32) * 0.4;
    let height = |x: usize, y: usize| hmap[y * n + x].0 + 0.08 * fibre(x, y);
    // sRGB: a crevice, a plate's face, its lip (the crevice a shade lighter
    // than Gates': at ten metres its chevrons were too hard).
    let crevice = Vec3::new(56.0, 48.0, 40.0);
    let plate = Vec3::new(118.0, 104.0, 86.0);
    let lip = Vec3::new(150.0, 136.0, 112.0);
    let texel = |x: usize, y: usize, _: f32| -> Texel {
        let (h, id) = hmap[y * n + x];
        let c = if h < 0.12 {
            crevice.lerp(plate, h / 0.12)
        } else {
            plate.lerp(lip, ((h - 0.12) / 0.88).powf(1.5))
        } * (0.86 + 0.28 * fibre(x, y))
            * (0.9 + 0.2 * hash01(id, 0x0ba4));
        Texel {
            albedo: [srgb_to_linear(c.x), srgb_to_linear(c.y), srgb_to_linear(c.z)],
            ao: 1.0,
            rough: 0.92,
            metal: 0.0,
            glow: 0.0,
        }
    };
    let [albedo, normal, _, _] = bake(n, 4.0, &height, &texel);
    [albedo, normal]
}

/// How high the bark stands at `(u, v)` of its tile, 0 in a crevice to 1
/// on a lip, and which scar it is on.
fn scar_height(u: f32, v: f32) -> (f32, u32) {
    const COLS: f32 = 4.0;
    const ROWS: f32 = 2.0;
    let (hw, hh) = (0.5 / COLS, 0.5 / ROWS);
    let mut best = (f32::MAX, 0.0f32, 0u32);
    let row = (v * ROWS).floor();
    for dr in -1..=1 {
        let r = row + dr as f32;
        let shift = if (r as i32).rem_euclid(2) == 1 { 0.5 } else { 0.0 };
        let col = (u * COLS - shift).floor();
        for dc in -1..=1 {
            let c = col + dc as f32;
            let (cx, cy) = ((c + 0.5 + shift) / COLS, (r + 0.5) / ROWS);
            let (dx, dy) = ((u - cx) / hw, (v - cy) / hh);
            // Diamonds a little wider than their lattice, so each plate
            // overlaps its neighbours and the one below shows under it.
            let d = dx.abs() / 1.15 + dy.abs() / 1.3;
            if d < best.0 {
                let id = ((r as i32).rem_euclid(ROWS as i32) as u32) * 8 + (c as i32).rem_euclid(COLS as i32) as u32;
                best = (d, dy, id);
            }
        }
    }
    let (d, dy, id) = best;
    // A plate tilts out toward its lower lip (dy grows down the card).
    let tilt = 0.55 + 0.45 * (dy * 0.5 + 0.5).clamp(0.0, 1.0);
    ((1.0 - d).max(0.0).powf(0.6) * tilt, id)
}

/// A 32-bit mix (Gates' `props::hash2`), and a number in 0..1 from it.
fn hash2(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x85eb_ca6b) ^ b.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_f491);
    h ^= h >> 13;
    h
}

fn hash01(a: u32, b: u32) -> f32 {
    (hash2(a, b) >> 8) as f32 / 16_777_216.0
}

fn smooth01(x: f32) -> f32 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// An sRGB value in 0..255 to linear.
fn srgb_to_linear(v: f32) -> f32 {
    let s = v.clamp(0.0, 255.0) / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positions(m: &Mesh) -> Vec<Vec3> {
        match m.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(p)) => p.iter().map(|v| Vec3::from_array(*v)).collect(),
            _ => panic!("no positions"),
        }
    }

    fn tris(m: &Mesh) -> usize {
        positions(m).len() / 3
    }

    /// The foot at the origin inside the solid, the top at the height.
    fn stands(parts: &[&Mesh], trunk_r: f32, height: f32, what: &str) {
        let (mut lo, mut hi, mut foot_r) = (f32::MAX, f32::MIN, 0.0f32);
        for m in parts {
            for p in positions(m) {
                lo = lo.min(p.y);
                hi = hi.max(p.y);
                if p.y < 0.3 {
                    foot_r = foot_r.max(p.xz().length());
                }
            }
        }
        assert!(
            (-SINK_M - 0.01..=0.0).contains(&lo),
            "{what}: the foot is at {lo}, not sunk {SINK_M}"
        );
        assert!((hi - height).abs() < 1e-2, "{what}: the top is at {hi}, not {height}");
        assert!(
            foot_r <= trunk_r * 1.2,
            "{what}: the foot reaches {foot_r} out of a {trunk_r} solid"
        );
    }

    #[test]
    fn a_palm_stands_in_its_solid_under_its_cap() {
        for seed in 0..6u32 {
            let height = 8.0 + 0.4 * seed as f32;
            let (bark, crown) = palm(seed, 0.3, height);
            let n = tris(&bark) + tris(&crown);
            assert!(n <= PALM_MAX_TRIS, "palm {seed}: {n} triangles");
            assert!(n > 2_000, "palm {seed}: only {n} triangles");
            stands(&[&bark, &crown], 0.3, height, "palm");
            assert!(
                bark.attribute(Mesh::ATTRIBUTE_TANGENT).is_some(),
                "the bark has no tangents"
            );
            // The crown is up at the top, and it is a crown: wider than the trunk by far.
            let crown_lo = positions(&crown).iter().map(|p| p.y).fold(f32::MAX, f32::min);
            let crown_r = positions(&crown).iter().map(|p| p.xz().length()).fold(0.0, f32::max);
            assert!(crown_lo > height * 0.3, "palm {seed}: fronds down at {crown_lo}");
            assert!(
                crown_r > 2.0 && crown_r < 8.0,
                "palm {seed}: the crown reaches {crown_r}"
            );
        }
    }

    #[test]
    fn a_dead_tree_stands_in_its_solid_under_its_cap() {
        for seed in 0..8u32 {
            let height = 7.0 + 0.3 * seed as f32;
            let m = dead(seed, 0.28, height);
            let n = tris(&m);
            assert!(n <= DEAD_MAX_TRIS, "dead {seed}: {n} triangles");
            assert!(n > 800, "dead {seed}: only {n} triangles");
            stands(&[&m], 0.28, height, "dead tree");
            let r = positions(&m).iter().map(|p| p.xz().length()).fold(0.0, f32::max);
            assert!(r > 1.0 && r < 6.0, "dead {seed}: the limbs reach {r}");
        }
    }

    #[test]
    fn a_live_tree_is_the_dead_one_in_leaf() {
        for seed in 0..8u32 {
            let height = 7.0 + 0.3 * seed as f32;
            let (wood, crown) = live(seed, 0.28, height);
            let n = tris(&wood) + tris(&crown);
            assert!(n <= LIVE_MAX_TRIS, "live {seed}: {n} triangles");
            stands(&[&wood, &crown], 0.28, height, "live tree");
            // Its leaves are up in its limbs, over a standing fighter's head.
            let lo = positions(&crown).iter().map(|p| p.y).fold(f32::MAX, f32::min);
            assert!(lo > 2.0, "live {seed}: leaves down at {lo}");
            let r = positions(&crown).iter().map(|p| p.xz().length()).fold(0.0, f32::max);
            assert!(r > 1.5 && r < 7.0, "live {seed}: the crown reaches {r}");
        }
        // A tree and its twin across the mirror are the same kind, and
        // the canyon has both kinds.
        let trees = [
            (-80.0, -22.0),
            (-45.0, 32.0),
            (-30.0, -38.0),
            (10.0, 27.0),
            (50.0, -30.0),
        ];
        for (x, z) in trees {
            assert_eq!(in_leaf(x, z), in_leaf(-x, z));
        }
        let card = leaf_rgba(64);
        let cover = card.chunks(4).filter(|p| p[3] >= 128).count() as f32 / (64.0 * 64.0);
        assert!(cover > 0.2 && cover < 0.75, "the leaves cover {cover} of their card");
    }

    #[test]
    fn the_rim_trees_stand_out_of_bounds_on_high_ground() {
        // A mesa 40 m up all round a 100 by 60 m floor.
        let ground = |x: f32, z: f32| Some(if x.abs() < 50.0 && z.abs() < 30.0 { 0.0 } else { 40.0 });
        let trees = rim(ground, (-50.0, -30.0), (50.0, 30.0), 3);
        assert!(trees.len() > 8, "only {} trees on the rim", trees.len());
        for (foot, h, _, _) in &trees {
            assert!(foot.x.abs() >= 50.0 + RIM_OUT_M.0 - 1e-3 || foot.z.abs() >= 30.0 + RIM_OUT_M.0 - 1e-3);
            assert!(foot.y >= RIM_MIN_Y && (RIM_H.0..=RIM_H.1).contains(h));
        }
        // A flat arena has no rim to stand them on.
        assert!(rim(|_, _| Some(0.0), (-50.0, -30.0), (50.0, 30.0), 3).is_empty());
    }

    #[test]
    fn the_same_seed_grows_the_same_tree() {
        let (a, _) = palm(5, 0.3, 9.0);
        let (b, _) = palm(5, 0.3, 9.0);
        assert_eq!(positions(&a), positions(&b));
        let (c, _) = palm(6, 0.3, 9.0);
        assert_ne!(positions(&a), positions(&c));
        assert_eq!(positions(&dead(3, 0.28, 8.0)), positions(&dead(3, 0.28, 8.0)));
    }

    #[test]
    fn the_cards_are_cut_out_and_tile() {
        let n = 64;
        let card = frond_rgba(n);
        let cover = |d: &[u8]| d.chunks(4).filter(|p| p[3] >= 128).count() as f32;
        let c0 = cover(&card) / (n * n) as f32;
        assert!(c0 > 0.1 && c0 < 0.6, "the frond covers {c0} of its card");
        let (chain, levels) = cutout_chain(&card, n);
        assert_eq!(levels, 7);
        assert_eq!(
            chain.len(),
            (64 * 64 + 32 * 32 + 16 * 16 + 8 * 8 + 4 * 4 + 2 * 2 + 1) * 4
        );
        // Three levels down the coverage holds.
        let l3 = &chain[(64 * 64 + 32 * 32 + 16 * 16) * 4..(64 * 64 + 32 * 32 + 16 * 16 + 8 * 8) * 4];
        let c3 = cover(l3) / 64.0;
        assert!((c3 - c0).abs() < 0.1, "level 3 covers {c3}, level 0 {c0}");
        let [albedo, normal] = palm_bark_images();
        assert_eq!(albedo.texture_descriptor.mip_level_count, 9);
        assert_eq!(normal.texture_descriptor.size.width, PALM_BARK_PX as u32);
        // A lip is lighter than a crevice.
        let (lip, _) = scar_height(0.125, 0.25 + 0.2);
        let (crevice, _) = scar_height(0.25, 0.25);
        assert!(lip > crevice, "lip {lip}, crevice {crevice}");
    }
}

//! Grass: tufts of crossed cards cut from a photographed atlas, scattered
//! over the ground from the map's seed (thick on the grassy hills, a dry
//! tuft here and there on the sand, none on rock or in the creek), merged
//! a mesh per 16 m block, and drawn only near the eye. Cosmetic: never in
//! the sim, never in the way of a shot.
//!
//! Built as Gates' `render/clutter.rs` builds a tuft (`card_dry`, `patch`;
//! MIT, NOTICE): three cards a tuft at a hashed yaw, each from a random cell
//! of the 2 × 2 atlas, height jittered, roots sunk a little, bedded by its
//! normals (fully the volume's at the root, a quarter its own card's at the
//! tip), tinted by the meadow's patches, straw where it's dry. It moves on
//! Gates' foliage shader (`foliage.rs`): one wind with a flutter on top, a
//! smooth fade with distance, cards seen edge-on hidden, light through from
//! behind, trails where fighters walk; each card carries its height above
//! its root and a random of its own in `UV_1` for it.
//!
//! None grows on a weapon pad or a launcher ([`Clear`]): a tuft standing
//! through a pad would half hide its weapon on the rungs that draw grass
//! and not on POTATO's.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use sim::terrain::{TerrainDef, HEIGHT_Q};

use super::foliage::{FoliageMaterial, Foliages, Kind};
use crate::level::{Dressed, GrassPart};

/// Gates' tuft: 0.4 m tall, three cards, twice as wide as tall, sunk 4%.
pub const TUFT_H: f32 = 0.40;
/// A tuft's height jitter (`0.75 + 0.5 × hash`) and a card's (`0.8 + 0.4 ×
/// hash`), each at its most.
const TUFT_JITTER_MAX: f32 = 1.25;
const CARD_JITTER_MAX: f32 = 1.2;
/// The tallest card any tuft can grow (a green one: dry tufts are 0.8).
pub const TUFT_MAX_H: f32 = TUFT_H * TUFT_JITTER_MAX * CARD_JITTER_MAX;

// The graphics ladder thins the grass and draws it nearer on a lower rung
// (`quality::Cover`), which is fair only because grass hides no one: it
// never reaches the top of a crouching fighter's hit box.
const _: () = assert!(
    TUFT_MAX_H < sim::consts::HIT_TOP_CROUCH,
    "grass that can hide a crouching fighter can't be a graphics row: every rung must draw it the same"
);

/// The share of the tufts in each density layer, in order: GRASS LOW draws
/// the first, MEDIUM the first two, HIGH all three (`quality::Cover`). A
/// tuft's layer is hashed from its cell, so a rung is the same tufts every
/// time, and thinning keeps the scatter even.
pub const LAYER_SHARE: [f32; 3] = [0.5, 0.25, 0.25];
const CARDS_PER_TUFT: u32 = 3;
const CARD_ASPECT: f32 = 2.0;
const CARD_SINK: f32 = 0.04;
/// How much of the volume normal a card's TIP keeps (Gates'): its root is
/// bedded in the ground and shades with it, its tip stands in the light
/// and shades a quarter as itself.
pub const BLADE_TIP_BLEND: f32 = 0.75;
/// How deep a card's bedding point sits below its root, in half-widths:
/// the root's normal is then within a degree of the ground's whatever the
/// card's size (Gates' `CARD_BED`).
const CARD_BED: f32 = 8.0;
/// How finely a card's dryness is carried: whole steps of `UV_1.y` over its
/// random, which `foliage.wgsl` divides back out (its literal 15).
pub const DRY_STEPS: f32 = 15.0;
/// The atlas's cells.
const COLS: u32 = 2;
const ROWS: u32 = 2;
/// The scatter's cell, and a block's side.
const STEP_M: f32 = 0.7;
pub const BLOCK_M: f32 = 16.0;
/// Past this a block isn't drawn at HIGH (Gates fades grass out by 46 m);
/// the GRASS DISTANCE row moves it (`quality::COVER_M_LADDER`).
pub const DRAW_M: f32 = 46.0;
/// What a dry tuft's photograph is multiplied by once the foliage shader
/// has taken it to its own grey (luma ~0.108): Gates' golden straw. A green
/// one's photograph keeps its colour, times this and the meadow's patches.
const STRAW: [f32; 3] = [3.1, 2.3, 1.0];
const GREEN: [f32; 3] = [0.95, 1.0, 0.85];
/// How far a grass block's corner is from its centre: what `cull` reaches
/// past the cover's distance.
pub const BLOCK_REACH: f32 = BLOCK_M * core::f32::consts::FRAC_1_SQRT_2;

/// A block of the ground cover (grass here, `clutter.rs`'s pieces): where
/// it is, which density layer it holds, and how far its corners are from
/// its centre.
#[derive(Component)]
pub struct CoverBlock {
    pub centre: Vec2,
    /// Its density layer ([`LAYER_SHARE`]).
    pub layer: u8,
    pub reach: f32,
}

/// Where the ground cover keeps off, in plan: discs and rectangles.
#[derive(Clone, Debug, Default)]
pub struct Clear {
    discs: Vec<(Vec2, f32)>,
    rects: Vec<(Vec2, Vec2)>,
}

/// How far the cover keeps from a weapon pad's centre (the pad is 0.8 m
/// across its radius), and from a launcher's edge, metres.
const PAD_CLEAR_M: f32 = 1.4;
const LAUNCHER_CLEAR_M: f32 = 0.5;

impl Clear {
    /// Off the weapon pads and the launchers: what the grass keeps off.
    pub fn pads(map: &sim::map::Map) -> Clear {
        let mut c = Clear::default();
        for p in &map.pads {
            c.disc(Vec2::new(p.pos.x, p.pos.z), PAD_CLEAR_M);
        }
        for l in &map.launchers {
            let m = Vec2::splat(LAUNCHER_CLEAR_M);
            c.rect(
                Vec2::new(l.area.min.x, l.area.min.z) - m,
                Vec2::new(l.area.max.x, l.area.max.z) + m,
            );
        }
        c
    }

    pub fn disc(&mut self, at: Vec2, r: f32) {
        self.discs.push((at, r));
    }

    pub fn rect(&mut self, lo: Vec2, hi: Vec2) {
        self.rects.push((lo, hi));
    }

    /// What of it touches the square `lo..hi`: a block's own, so its
    /// cells test a handful of shapes and not the whole map's.
    pub fn near(&self, lo: Vec2, hi: Vec2) -> Clear {
        Clear {
            discs: self
                .discs
                .iter()
                .copied()
                .filter(|&(c, r)| c.cmpge(lo - r).all() && c.cmple(hi + r).all())
                .collect(),
            rects: self
                .rects
                .iter()
                .copied()
                .filter(|&(a, b)| b.cmpge(lo).all() && a.cmple(hi).all())
                .collect(),
        }
    }

    /// Whether `p` is kept clear.
    pub fn blocked(&self, p: Vec2) -> bool {
        self.discs.iter().any(|&(c, r)| c.distance_squared(p) < r * r)
            || self
                .rects
                .iter()
                .any(|&(lo, hi)| p.cmpge(lo).all() && p.cmple(hi).all())
    }
}

/// The layer a tuft with hash `r` (0..1) is in.
pub(crate) fn layer_of(r: f32) -> usize {
    let mut edge = 0.0;
    for (i, share) in LAYER_SHARE.iter().enumerate() {
        edge += share;
        if r < edge {
            return i;
        }
    }
    LAYER_SHARE.len() - 1
}

pub(crate) fn hash01(seed: u32, k: u32) -> f32 {
    let mut h = seed.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 13;
    (h >> 8) as f32 / 16_777_216.0
}

/// The ground's height at (x, z), as the sim's triangles have it.
pub fn ground(t: &TerrainDef, x: f32, z: f32) -> Option<f32> {
    let (nx, nz) = (t.nx as usize, t.nz as usize);
    let u = (x - t.x0) / t.cell;
    let v = (z - t.z0) / t.cell;
    if u < 0.0 || v < 0.0 || u > (nx - 1) as f32 || v > (nz - 1) as f32 {
        return None;
    }
    let (i, j) = ((u as usize).min(nx - 2), (v as usize).min(nz - 2));
    let (fu, fv) = (u - i as f32, v - j as f32);
    let h = |a: usize, b: usize| t.heights[b * nx + a] as f32 / HEIGHT_Q;
    let (h00, h10, h01, h11) = (h(i, j), h(i + 1, j), h(i, j + 1), h(i + 1, j + 1));
    Some(if fu >= fv {
        h00 + (h10 - h00) * fu + (h11 - h10) * fv
    } else {
        h00 + (h11 - h01) * fu + (h01 - h00) * fv
    })
}

/// The grass is patchy: its patches ([`patch`], metres across) bare the
/// ground this far, in the splat and the tufts alike.
const PATCH_BARE: f32 = 0.75;

/// Where grass grows by height: none on the creek's floor, full from
/// this height, and gone again above the next (the scree's top and the
/// cliffs' ledges are bare).
const GREEN_FROM: (f32, f32) = (1.0, 2.5);
const GREEN_TO: (f32, f32) = (6.0, 10.0);

/// How much grass the height `y` allows, 0..1.
pub fn by_height(y: f32) -> f32 {
    let up = ((y - GREEN_FROM.0) / (GREEN_FROM.1 - GREEN_FROM.0)).clamp(0.0, 1.0);
    let down = 1.0 - ((y - GREEN_TO.0) / (GREEN_TO.1 - GREEN_TO.0)).clamp(0.0, 1.0);
    up * down
}

/// How much grass grows at (x, z) at height `y`, by its height and its
/// patch, 0..1: the splat's share (`level::ground_point`) and the tufts'.
pub fn cover(x: f32, z: f32, y: f32) -> f32 {
    by_height(y) * (1.0 - PATCH_BARE + PATCH_BARE * patch(x, z))
}

/// How much grass grows at (x, z): (green, dry), each 0..1. The splat's own
/// rule (level.rs): grass up the hills, none on steep rock or in the creek,
/// in patches.
fn growth(t: &TerrainDef, x: f32, z: f32) -> Option<(f32, f32)> {
    let y = ground(t, x, z)?;
    let dx = ground(t, x + 0.5, z)? - ground(t, x - 0.5, z)?;
    let dz = ground(t, x, z + 0.5)? - ground(t, x, z - 0.5)?;
    let slope = (dx * dx + dz * dz).sqrt();
    if slope > 0.55 || y < -0.12 {
        return None;
    }
    let green = cover(x, z, y);
    Some((green, 0.07))
}

/// Smooth value noise over the ground plane, in [0, 1]: the patches a
/// meadow is made of, metres across rather than a tuft wide (Gates').
pub(crate) fn patch(x: f32, z: f32) -> f32 {
    let octave = |x: f32, z: f32| {
        let (ix, iz) = (x.floor(), z.floor());
        let (fx, fz) = (x - ix, z - iz);
        let (ux, uz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
        let h = |dx: i32, dz: i32| hash01((ix as i32 + dx) as u32, ((iz as i32 + dz) as u32) ^ 0x5eed_0f1e);
        let a = h(0, 0) + (h(1, 0) - h(0, 0)) * ux;
        let b = h(0, 1) + (h(1, 1) - h(0, 1)) * ux;
        a + (b - a) * uz
    };
    octave(x / 11.0, z / 11.0) * 0.65 + octave(x / 4.3 + 17.0, z / 4.3 + 5.0) * 0.35
}

/// A green card's colour multiplier where it stands: yellower in the dry
/// patches, deeper green in the lush ones, the photograph's own between. A
/// mean-1 shift of the photograph's, never a second colour for grass.
fn patch_tint(x: f32, z: f32) -> [f32; 3] {
    let t = patch(x, z);
    let dry = ((t - 0.55) / 0.25).clamp(0.0, 1.0);
    let lush = ((0.42 - t) / 0.25).clamp(0.0, 1.0);
    [
        1.0 + 0.16 * dry - 0.08 * lush,
        1.0 + 0.03 * dry + 0.06 * lush,
        1.0 - 0.30 * dry - 0.02 * lush,
    ]
}

#[derive(Default)]
struct Soup {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    /// The foliage shader's handle on each card: its corner's height above
    /// the root, and the card's random over whole steps of its dryness.
    uv1: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Soup {
    /// One tuft rooted at `at`, `h` tall, `dry` of the way (0..1) to straw.
    fn tuft(&mut self, at: Vec3, yaw: f32, seed: u32, h: f32, tint: [f32; 3], dry: f32) {
        let root = at - Vec3::Y * h * CARD_SINK;
        let (du, dv) = (1.0 / COLS as f32, 1.0 / ROWS as f32);
        for i in 0..CARDS_PER_TUFT {
            let a = yaw + i as f32 * core::f32::consts::PI / CARDS_PER_TUFT as f32;
            let side = Vec3::new(a.sin(), 0.0, a.cos());
            let hj = h * (0.8 + 0.4 * hash01(seed, i + 5));
            let half_w = hj * CARD_ASPECT * 0.5;
            let cell = (hash01(seed, i + 91) * (COLS * ROWS) as f32) as u32 % (COLS * ROWS);
            let (cu, cv) = ((cell % COLS) as f32 * du, (cell / COLS) as f32 * dv);
            let (b0, b1) = (root - side * half_w, root + side * half_w);
            let (t0, t1) = (b0 + Vec3::Y * hj, b1 + Vec3::Y * hj);
            let v = 0.86 + 0.28 * hash01(seed, i + 13);
            // The card's own facing, and the point its normals fan out
            // from: below the root by `CARD_BED` half-widths.
            let facet = Vec3::Y.cross(side);
            let bed = root - Vec3::Y * half_w * CARD_BED;
            let rand = hash01(seed, i + 211) + (dry * DRY_STEPS).round();
            let base = self.pos.len() as u32;
            // Roots at the cell's bottom edge (v grows downward).
            for (p, uv) in [
                (b0, [cu, cv + dv]),
                (t0, [cu, cv]),
                (b1, [cu + du, cv + dv]),
                (t1, [cu + du, cv]),
            ] {
                let up = (p.y - root.y).max(0.0);
                let blend = 1.0 - (1.0 - BLADE_TIP_BLEND) * (up / hj).clamp(0.0, 1.0);
                let n = (facet * (1.0 - blend) + (p - bed).normalize_or_zero() * blend).normalize_or(Vec3::Y);
                self.pos.push(p.to_array());
                self.nor.push(n.to_array());
                self.uv.push(uv);
                self.uv1.push([up, rand]);
                self.col.push([v * tint[0], v * tint[1], v * tint[2], 1.0]);
            }
            self.idx
                .extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
        }
    }

    fn mesh(self) -> Option<Mesh> {
        (!self.idx.is_empty()).then(|| {
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, self.uv1)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
                .with_inserted_indices(Indices::U32(self.idx))
        })
    }
}

/// The blocks of grass for a map's terrain: a mesh per block and density
/// layer, its centre and its layer. Nothing grows where `clear` says.
pub fn blocks(t: &TerrainDef, lo: (f32, f32), hi: (f32, f32), seed: u32, clear: &Clear) -> Vec<(Mesh, Vec2, u8)> {
    let mut out = Vec::new();
    let mut bz = lo.1;
    while bz < hi.1 {
        let mut bx = lo.0;
        while bx < hi.0 {
            let mut soups: [Soup; LAYER_SHARE.len()] = Default::default();
            let n = (BLOCK_M / STEP_M) as u32;
            for j in 0..n {
                for i in 0..n {
                    let cell = ((bx.to_bits() ^ bz.to_bits().rotate_left(16)) ^ (j * 977 + i)).wrapping_add(seed);
                    let x = bx + (i as f32 + hash01(cell, 1)) * STEP_M;
                    let z = bz + (j as f32 + hash01(cell, 2)) * STEP_M;
                    if x > hi.0 || z > hi.1 || clear.blocked(Vec2::new(x, z)) {
                        continue;
                    }
                    let Some((green, dry)) = growth(t, x, z) else {
                        continue;
                    };
                    let r = hash01(cell, 3);
                    let (tint, scale, dried) = if r < green * 0.9 {
                        let p = patch_tint(x, z);
                        ([GREEN[0] * p[0], GREEN[1] * p[1], GREEN[2] * p[2]], 1.0, 0.0)
                    } else if r < green * 0.9 + dry {
                        (STRAW, 0.8, 1.0)
                    } else {
                        continue;
                    };
                    let Some(y) = ground(t, x, z) else { continue };
                    let h = TUFT_H * scale * (0.75 + 0.5 * hash01(cell, 4));
                    soups[layer_of(hash01(cell, 6))].tuft(
                        Vec3::new(x, y, z),
                        hash01(cell, 5) * core::f32::consts::TAU,
                        cell,
                        h,
                        tint,
                        dried,
                    );
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

/// The grass blocks their material, once the atlas is mipped: the grass
/// cards' foliage material, made once and kept for every level.
pub fn dress(
    mut commands: Commands,
    tex: Res<super::textures::Textures>,
    reg: Option<ResMut<Foliages>>,
    mut mats: ResMut<Assets<FoliageMaterial>>,
    grass: Query<Entity, (With<GrassPart>, Without<Dressed>)>,
) {
    if grass.is_empty() {
        return;
    }
    let (Some(mut reg), Some(atlas)) = (reg, tex.grass.clone()) else {
        return;
    };
    let m = match reg.grass.clone() {
        Some(m) => m,
        None => {
            let m = mats.add(reg.make(
                StandardMaterial {
                    base_color_texture: Some(atlas),
                    alpha_mode: AlphaMode::Mask(0.5),
                    double_sided: true,
                    cull_mode: None,
                    perceptual_roughness: 0.9,
                    reflectance: super::fresnel::DIELECTRIC,
                    ..default()
                },
                Kind::Grass,
            ));
            reg.grass = Some(m.clone());
            m
        }
    };
    for e in grass.iter() {
        commands.entity(e).insert((MeshMaterial3d(m.clone()), Dressed));
    }
}

/// Only the blocks near the eye are drawn, and only the layers the tier's
/// cover draws (`quality::Cover`), out to its distance: the grass and the
/// clutter alike. Past the distance the foliage shader has already sunk
/// every piece (`foliage::FADE_END`), so a block is never seen going.
pub fn cull(
    eye: Query<&GlobalTransform, With<super::rig::EyeCam>>,
    tier: Res<super::quality::Tier>,
    mut blocks: Query<(&CoverBlock, &mut Visibility)>,
) {
    let Ok(e) = eye.single() else {
        return;
    };
    let p = e.translation().xz();
    let layers = tier.gfx.cover.layers();
    for (b, mut v) in blocks.iter_mut() {
        let want = if b.layer < layers && b.centre.distance(p) < tier.gfx.cover_m + b.reach {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *v != want {
            *v = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layers_share_out_every_tuft() {
        assert!((LAYER_SHARE.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        assert_eq!(layer_of(0.0), 0);
        assert_eq!(layer_of(0.49), 0);
        assert_eq!(layer_of(0.5), 1);
        assert_eq!(layer_of(0.8), 2);
        assert_eq!(layer_of(0.999_999), 2);
        assert_eq!(
            usize::from(crate::look::quality::Cover::High.layers()),
            LAYER_SHARE.len(),
            "HIGH draws every layer"
        );
    }

    /// A tuft's roots are bedded (their normal the ground's to a degree),
    /// its tips lean a quarter toward the card's own facing, and `UV_1`
    /// carries each corner's height above the root and the card's random
    /// over its whole steps of dryness, as `foliage.wgsl` reads them.
    #[test]
    fn a_tuft_carries_what_the_foliage_shader_reads() {
        let mut s = Soup::default();
        s.tuft(Vec3::new(2.0, 1.0, -3.0), 0.7, 99, TUFT_H, GREEN, 1.0);
        assert_eq!(s.pos.len(), (CARDS_PER_TUFT * 4) as usize);
        for ((p, n), uv1) in s.pos.iter().zip(&s.nor).zip(&s.uv1) {
            let n = Vec3::from_array(*n);
            assert!((n.length() - 1.0).abs() < 1e-4);
            if uv1[0] == 0.0 {
                assert!(n.y > 0.99, "a root is bedded: {n:?}");
            } else {
                assert!(n.y < 0.99 && n.y > 0.8, "a tip leans, a little: {n:?}");
                assert!((uv1[0] - (p[1] - (1.0 - TUFT_H * CARD_SINK))).abs() < 1e-4);
            }
            assert_eq!(uv1[1].floor(), DRY_STEPS, "dry all the way");
            assert!(uv1[1].fract() >= 0.0 && uv1[1] < DRY_STEPS + 1.0);
        }
    }

    #[test]
    fn nothing_grows_on_a_pad_or_a_launcher() {
        let mut c = Clear::default();
        c.disc(Vec2::new(0.0, 0.0), PAD_CLEAR_M);
        c.rect(Vec2::new(10.0, -1.0), Vec2::new(12.0, 1.0));
        assert!(c.blocked(Vec2::new(0.5, 0.5)));
        assert!(!c.blocked(Vec2::new(1.5, 0.0)));
        assert!(c.blocked(Vec2::new(11.0, 0.9)));
        assert!(!c.blocked(Vec2::new(12.5, 0.0)));
        // A block's own share keeps what touches it and only that.
        let here = c.near(Vec2::new(-4.0, -4.0), Vec2::new(4.0, 4.0));
        assert!(here.blocked(Vec2::new(0.5, 0.5)) && here.rects.is_empty());
        let there = c.near(Vec2::new(11.5, 0.5), Vec2::new(20.0, 8.0));
        assert!(there.discs.is_empty() && there.rects.len() == 1);
        // A green hill: 2.5 m up, flat.
        const UP: i16 = (2.5 * HEIGHT_Q) as i16;
        static HILL: [i16; 81] = [UP; 81];
        let t = TerrainDef {
            x0: -8.0,
            z0: -8.0,
            cell: 2.0,
            nx: 9,
            nz: 9,
            heights: &HILL,
        };
        let all = blocks(&t, (-8.0, -8.0), (8.0, 8.0), 7, &Clear::default());
        let cleared = blocks(&t, (-8.0, -8.0), (8.0, 8.0), 7, &c);
        let near = |b: &[(Mesh, Vec2, u8)]| {
            b.iter()
                .flat_map(|(m, ..)| {
                    m.attribute(Mesh::ATTRIBUTE_POSITION)
                        .and_then(|a| a.as_float3())
                        .unwrap_or(&[])
                        .to_vec()
                })
                // A card's corners are at most 0.6 m from its root, so a
                // corner this near is a tuft rooted inside the pad's clear.
                .filter(|p| Vec2::new(p[0], p[2]).length() < 0.7)
                .count()
        };
        assert!(near(&all) > 0, "a green hill grows grass at the pad's middle");
        assert_eq!(near(&cleared), 0, "and none once it is a pad");
    }
}

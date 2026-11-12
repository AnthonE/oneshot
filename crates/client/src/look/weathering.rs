//! Weathering on the level's boxes: what a photographed tile cannot say
//! about a wall that has stood out in the weather. After Gates'
//! `crates/client/src/render/weathering.rs` and `shaders/weathering.wgsl`
//! (MIT, NOTICE).
//!
//! The boxes wear photographed (or drawn) sets at a few metres a tile. Up
//! close that is concrete; from thirty metres every block of a wall is the
//! same value and the tile repeats. This is the variation layer over it, in
//! world space so no two walls share it:
//!
//! - **Value**: each block its own (a hash of where it stands), and a
//!   mottle at about 1.5 m and 6 m across every face.
//! - **Grime** toward the ground: splash and soil in a ragged band.
//! - **Streaks** down the sides: rain stains, rust off steel, desert
//!   varnish down rock.
//! - **Edge wear** along the block's own edges (its box or its round,
//!   is in the uniform): chipped concrete and rock, paint worn bright.
//! - **Dust** settled on what faces up: the map's sand outdoors, grey
//!   under a roof.
//!
//! `StandardMaterial` plus a fragment stage (`weathering.wgsl`); its prepass
//! and shadows are the base material's, since nothing moves. One uniform
//! and arithmetic, no texture or buffer, so WebGL2 draws exactly what the
//! desktop and WebGPU do. Drawn on every rung of the graphics ladder: a
//! wall a fighter stands against looks the same to everyone.

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

/// A weathered box: `StandardMaterial` plus the stains.
pub type LevelMaterial = ExtendedMaterial<StandardMaterial, Weathering>;

pub const SHADER: &str = "shaders/weathering.wgsl";

/// The per-block uniform; each lane is named in `weathering.wgsl`.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct WeatherParams {
    pub mottle: Vec4,
    pub streak: Vec4,
    pub streak_tint: Vec4,
    pub grime: Vec4,
    pub grime_tint: Vec4,
    pub wear: Vec4,
    pub dust_tint: Vec4,
    pub centre: Vec4,
    pub half: Vec4,
    pub turn: Vec4,
    /// The ground under a long block, end to end ([`Block::run`]).
    pub ground_run: [Vec4; 2],
    /// WebGL2's light shafts (`shafts::WebShafts`, `shaft_march.wgsl`): the
    /// same in every box, zero where none are drawn.
    pub shaft_lo: Vec4,
    pub shaft_hi: Vec4,
    pub shaft_tint: Vec4,
    /// The hall the block stands in or bounds ([`Block::hall`]), its box
    /// let out [`HALL_OUT_M`] so a wall's inner face is well inside it;
    /// `hall_hi.w` 0: none.
    pub hall_lo: Vec4,
    pub hall_hi: Vec4,
}

/// A block's shaft lanes, to compare with `shafts::WebShafts`.
pub fn shaft_lanes(p: &WeatherParams) -> (Vec4, Vec4, Vec4) {
    (p.shaft_lo, p.shaft_hi, p.shaft_tint)
}

/// The extension. **Not `#[bindless]`**, as the ground's isn't: one
/// uniform per block, the base's own bindings beside it.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct Weathering {
    #[uniform(100)]
    pub params: WeatherParams,
}

impl MaterialExtension for Weathering {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    /// Forward only: this client never runs deferred.
    fn deferred_fragment_shader() -> ShaderRef {
        ShaderRef::Default
    }
}

/// A block's shape, as its edges are found: where it stands and how big,
/// its turn about y and what it is.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub mid: Vec3,
    pub half: Vec3,
    /// cos and sin of its turn (local +x in the world is (c, 0, s)).
    pub turn: (f32, f32),
    pub shape: Shape,
    /// The ground's height under its middle, where its grime starts.
    pub ground: f32,
    /// A long block on uneven ground: the ground under it at [`RUN`]
    /// points end to end along its local x (`Run::X`) or z, so its grime
    /// follows the ground, not the height under its middle.
    pub run: Option<(Run, [f32; RUN])>,
    /// Its edges another box hides or carries on (`rock::hidden_edges`,
    /// bits as `weathering.wgsl`'s `edge_distance`): no wear along them.
    pub hidden: u8,
    /// Whether it has a value of its own ([`block_value`]): a box does,
    /// its faces cut where another box draws the same plane (`rock::covers`);
    /// a solid does not, since an arc's turned segments overlap at their
    /// ends and two values on one plane would flicker.
    pub own_value: bool,
    /// Whether its top is open to the sky (nothing built over its middle):
    /// where snow settles on a map that snows.
    pub open: bool,
    /// The hall it stands in or bounds, as a box (lo, hi), on a map whose
    /// halls the sky does not light (a snowed valley's hangars).
    pub hall: Option<(Vec3, Vec3)>,
}

/// The ground's samples along a long block ([`Block::run`]).
pub const RUN: usize = 8;

/// The axis a block's ground samples run along, in its own frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    X,
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A box, upright, turned by `turn`.
    Box,
    /// An upright cylinder: `half.x` its radius.
    Round,
    /// No edges a box would have (a ramp): no wear.
    Other,
}

/// What a surface is, as it weathers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Concrete,
    Steel,
    /// Painted plating (the panel set).
    Paint,
    Rock,
    /// The arena's floor: a top, no sides.
    Floor,
    /// The alien hull: a ship's skin, kept, with only its value broken.
    Hull,
    /// A base's plate: kept metal, looked after. Soil splashed at its foot
    /// and dust on its tops, its seams its own photograph's; no rust
    /// running down it (`Steel`'s streaks on a curved wall read as grain).
    Plate,
    /// Cut stone (Cloudbreak's observatory): concrete's wear and grime over
    /// the masonry's own photograph.
    Stone,
}

/// Rain stains on concrete: darker and a little warm. **(knob)**
const GRIME: [f32; 3] = [0.55, 0.52, 0.47];
/// Rust run off a fixing. **(knob)**
const RUST: [f32; 3] = [0.78, 0.5, 0.32];
/// Desert varnish: the dark manganese stain down a rock face. **(knob)**
const VARNISH: [f32; 3] = [0.5, 0.4, 0.33];
/// Soil and splash at a block's foot. **(knob)**
const SOIL: [f32; 3] = [0.62, 0.55, 0.47];
/// Dust settled outdoors on a dusty map (the canyon's sand), and anywhere
/// else; linear albedo, inside the dull band. **(knob)**
const SAND_DUST: [f32; 3] = [0.42, 0.29, 0.19];
const GREY_DUST: [f32; 3] = [0.3, 0.29, 0.27];
/// Snow settled on what faces up under an open sky, in a valley that
/// snows (`level::Land::Alpine`): a step under the ground's own snow, and
/// how much of a top it takes (the patches between are scoured bare).
const SNOW_SETTLE: [f32; 3] = [0.8, 0.81, 0.83];
const SNOW_SETTLE_SHARE: f32 = 1.0;
/// How far one block's value may sit from the next, either way.
const BLOCK_VALUE: f32 = 0.07;
/// How far up the grime reaches from the ground, m.
const GRIME_REACH_M: f32 = 1.3;
/// Under a roof there is less dust and no weather to carry it off.
const ROOFED_DUST: f32 = 0.6;
/// How far a hall's box is let out in the plan: past its walls' inner
/// faces by more than the shader's fade, short of the thinnest wall's far
/// face (a facade is a metre and a half through).
const HALL_OUT_M: f32 = 0.75;
/// What grime at a block's foot multiplies by where snow lies against it:
/// wet and cold, not the canyon's soil.
const COLD_GRIME: [f32; 3] = [0.42, 0.46, 0.52];
/// And how strong it is on a snowed valley's cast concrete.
const SNOWY_GRIME: f32 = 0.75;

/// How a surface weathers.
#[derive(Clone, Copy, Debug)]
struct Recipe {
    /// The mottle's swing, either way.
    mottle: f32,
    /// How strong the streaks are, and what they multiply by.
    streak: f32,
    tint: [f32; 3],
    /// How strong the grime at the foot is.
    grime: f32,
    /// Edge wear: how strong, how wide (m), how much lighter (a
    /// multiplier) and how much rougher what is worn is.
    wear: f32,
    width: f32,
    lift: f32,
    rough: f32,
    /// Dust on the tops.
    dust: f32,
}

const fn r(mottle: f32, streak: f32, tint: [f32; 3], grime: f32, wear: (f32, f32, f32, f32), dust: f32) -> Recipe {
    Recipe {
        mottle,
        streak,
        tint,
        grime,
        wear: wear.0,
        width: wear.1,
        lift: wear.2,
        rough: wear.3,
        dust,
    }
}

fn recipe(s: Surface) -> Recipe {
    match s {
        Surface::Concrete => r(0.16, 0.85, GRIME, 0.45, (0.6, 0.07, 1.28, 0.1), 0.5),
        Surface::Steel => r(0.12, 0.7, RUST, 0.35, (0.7, 0.04, 1.35, -0.15), 0.35),
        Surface::Paint => r(0.08, 0.5, GRIME, 0.3, (0.8, 0.05, 1.45, 0.0), 0.3),
        Surface::Rock => r(0.2, 0.75, VARNISH, 0.25, (0.5, 0.09, 1.22, 0.05), 0.6),
        Surface::Floor => r(0.1, 0.0, GRIME, 0.0, (0.0, 0.05, 1.0, 0.0), 0.4),
        Surface::Hull => r(0.12, 0.0, GRIME, 0.35, (0.0, 0.05, 1.0, 0.0), 0.0),
        Surface::Plate => r(0.13, 0.1, GRIME, 0.45, (0.55, 0.05, 1.4, -0.1), 0.3),
        Surface::Stone => r(0.14, 0.6, GRIME, 0.4, (0.5, 0.05, 1.22, 0.1), 0.45),
    }
}

/// Integer hash to [0, 1).
fn h01(a: u32, b: u32) -> f32 {
    let mut x = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x >> 8) as f32 * (1.0 / 16_777_216.0)
}

/// A block's own value, a multiplier: a hash of where it stands, to the
/// centimetre, so a mirrored map's two halves differ too.
pub fn block_value(mid: Vec3) -> f32 {
    let q = |v: f32| (v * 100.0).round() as i32 as u32;
    let k = q(mid.x).wrapping_mul(73_856_093) ^ q(mid.y).wrapping_mul(19_349_663) ^ q(mid.z).wrapping_mul(83_492_791);
    1.0 + (h01(k, 0x5eed) * 2.0 - 1.0) * BLOCK_VALUE
}

/// The uniform for a block of `surface`: `dusty` outdoors on a dusty map,
/// `roofed` under a roof, `snowy` in a valley that snows (its open tops
/// carry snow, not dust).
pub fn params(surface: Surface, b: &Block, dusty: bool, roofed: bool, snowy: bool) -> WeatherParams {
    let Recipe {
        mottle,
        streak,
        tint,
        grime,
        wear,
        width,
        lift,
        rough,
        dust,
    } = recipe(surface);
    let settles = snowy && b.open && !roofed;
    let dust_rgb = if settles {
        SNOW_SETTLE
    } else if dusty && !roofed {
        SAND_DUST
    } else {
        GREY_DUST
    };
    let dust = if settles {
        SNOW_SETTLE_SHARE
    } else if roofed {
        dust * ROOFED_DUST
    } else {
        dust
    };
    let shape = match b.shape {
        Shape::Box => 1.0,
        Shape::Round => 2.0,
        Shape::Other => 0.0,
    };
    WeatherParams {
        mottle: Vec4::new(mottle, 0.65, 0.17, if b.own_value { block_value(b.mid) } else { 1.0 }),
        streak: Vec4::new(streak, 2.2, if settles { 1.0 } else { 0.0 }, 0.0),
        streak_tint: Vec4::new(tint[0], tint[1], tint[2], 0.0),
        grime: Vec4::new(
            // A snowed valley's concrete is damp and dark over the snow at its foot.
            if snowy && surface == Surface::Stone {
                SNOWY_GRIME
            } else {
                grime
            },
            GRIME_REACH_M,
            b.ground,
            match b.run {
                None => 0.0,
                Some((Run::X, _)) => 1.0,
                Some((Run::Z, _)) => 2.0,
            },
        ),
        grime_tint: if snowy {
            Vec4::new(COLD_GRIME[0], COLD_GRIME[1], COLD_GRIME[2], 0.0)
        } else {
            Vec4::new(SOIL[0], SOIL[1], SOIL[2], 0.0)
        },
        wear: Vec4::new(dust, wear, width, lift),
        dust_tint: Vec4::new(dust_rgb[0], dust_rgb[1], dust_rgb[2], rough),
        centre: b.mid.extend(shape),
        half: b.half.extend(f32::from(b.hidden)),
        // A snowed valley's stone is its bases' cast concrete: in panels.
        turn: Vec4::new(
            b.turn.0,
            b.turn.1,
            if seal(b) { 1.0 } else { 0.0 },
            if snowy && surface == Surface::Stone { 2.0 } else { 0.0 },
        ),
        ground_run: b.run.map_or([Vec4::splat(b.ground); 2], |(_, g)| {
            [Vec4::new(g[0], g[1], g[2], g[3]), Vec4::new(g[4], g[5], g[6], g[7])]
        }),
        hall_lo: b.hall.map_or(Vec4::ZERO, |(lo, _)| {
            Vec4::new(lo.x - HALL_OUT_M, lo.y, lo.z - HALL_OUT_M, 0.0)
        }),
        hall_hi: b.hall.map_or(Vec4::ZERO, |(_, hi)| {
            Vec4::new(hi.x + HALL_OUT_M, hi.y, hi.z + HALL_OUT_M, 1.0)
        }),
        // Painted in by `shafts::paint_web` where WebGL2 marches.
        ..default()
    }
}

/// A plaza's round slab ([`Shape::Round`], at least [`SEAL_R_M`] across
/// and no taller than [`SEAL_H_M`]: Cloudbreak's ring) has its top carved
/// as a seal in its shading (`weathering.wgsl`).
pub const SEAL_R_M: f32 = 5.0;
pub const SEAL_H_M: f32 = 2.5;

pub fn seal(b: &Block) -> bool {
    matches!(b.shape, Shape::Round) && b.half.x >= SEAL_R_M && b.half.y * 2.0 <= SEAL_H_M
}

/// `base` weathered as a block of `surface`.
pub fn material(
    base: StandardMaterial,
    surface: Surface,
    b: &Block,
    dusty: bool,
    roofed: bool,
    snowy: bool,
) -> LevelMaterial {
    ExtendedMaterial {
        base,
        extension: Weathering {
            params: params(surface, b, dusty, roofed, snowy),
        },
    }
}

pub fn plugin(app: &mut App) {
    app.add_plugins(MaterialPlugin::<LevelMaterial>::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Surface; 8] = [
        Surface::Concrete,
        Surface::Steel,
        Surface::Paint,
        Surface::Rock,
        Surface::Floor,
        Surface::Hull,
        Surface::Plate,
        Surface::Stone,
    ];

    fn block() -> Block {
        Block {
            mid: Vec3::new(3.0, 1.0, -2.0),
            half: Vec3::new(1.0, 1.0, 2.0),
            turn: (1.0, 0.0),
            shape: Shape::Box,
            ground: 0.0,
            run: None,
            hidden: 0,
            own_value: true,
            open: true,
            hall: None,
        }
    }

    /// No stain can push a surface out of the dull band on its own: every
    /// multiply is by at most 1 but the wear's (a lighter edge, bounded),
    /// the dust is a dull albedo, and the mottle swings under a quarter.
    #[test]
    fn the_stains_stay_in_the_band() {
        for s in ALL {
            for (dusty, roofed) in [(false, false), (true, false), (true, true)] {
                let p = params(s, &block(), dusty, roofed, false);
                assert!(p.mottle.x > 0.0 && p.mottle.x < 0.25, "{s:?}");
                assert!(p.streak_tint.truncate().max_element() <= 1.0, "{s:?}");
                assert!(p.grime_tint.truncate().max_element() <= 1.0, "{s:?}");
                assert!(p.dust_tint.truncate().max_element() <= 0.5, "{s:?}");
                assert!((1.0..=1.5).contains(&p.wear.w), "{s:?}");
                assert!((0.0..=1.0).contains(&p.wear.x), "{s:?}");
            }
        }
    }

    /// Each block its own value, inside its band; the same block the same.
    #[test]
    fn every_block_has_its_own_value() {
        let mut seen = Vec::new();
        for i in 0..64 {
            let v = block_value(Vec3::new(i as f32 * 1.5 - 40.0, 0.5, (i % 7) as f32 * 2.0));
            assert!((1.0 - BLOCK_VALUE..=1.0 + BLOCK_VALUE).contains(&v));
            seen.push(v);
        }
        let mean = seen.iter().sum::<f32>() / seen.len() as f32;
        let spread = seen.iter().map(|v| (v - mean).abs()).sum::<f32>() / seen.len() as f32;
        assert!(spread > BLOCK_VALUE * 0.25, "the blocks are one value: {spread}");
        assert_eq!(
            block_value(Vec3::new(1.0, 2.0, 3.0)),
            block_value(Vec3::new(1.0, 2.0, 3.0))
        );
        // A mirrored pair differs.
        assert_ne!(
            block_value(Vec3::new(-56.0, 2.0, 0.0)),
            block_value(Vec3::new(56.0, 2.0, 0.0))
        );
    }
}

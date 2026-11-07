//! The sky's arithmetic: the seeded cloud field, how the deck is lit
//! (`DeckLight`), the clear sky behind it where no atmosphere draws one, and
//! the weather's air over it (`SkyFog`). The deck itself is drawn per pixel
//! (`deck.rs`) from this field and these numbers, under the map's mood
//! (`mood.rs`). What follows the GPU is decided at run time ([`baked`]);
//! what follows the browser's one thread (the field's size) stays
//! `target_arch = "wasm32"`.
//!
//! Ported from Gates' `crates/client/src/render/sky.rs` (MIT, © 2026
//! MoreRight DAO; NOTICE). Its composer and the cube it composed (nothing
//! here reflects the sky), its underwater fog and the Verdigris are left
//! out: the per-pixel deck's shaders (`cloud.wgsl`, `deck.wgsl`) are that
//! composer's arithmetic, a pixel at a time. What remains is Gates' code
//! as it was, so the rest of its sky lands on the same numbers.

/// Gates' share of night light a new moon leaves (rig.rs).
const NEW_MOON_SHARE: f32 = 0.35;

/// The night's light off the moon (Gates' `rig::night_light`).
fn night_light(moonlight: f32) -> f32 {
    NEW_MOON_SHARE + (1.0 - NEW_MOON_SHARE) * moonlight.clamp(0.0, 1.0)
}

// Clouds — the frame's top stop.
//
// **The measurement that forced this, and the two things it ruled out.** Our
// captures read whole-frame p90 143.6 against the reference photographs'
// 170.2, and `ART.md` §4 says where the difference comes from: *"Cloudless
// cannot reach a p90 of 189 with a sky mean of 143; the references get there
// with lit cumulus."* Two cheaper answers were tried against the arithmetic
// and both fail:
//
//   · **The sun disk cannot do it.** `SunDisk::EARTH` is 9.31 mrad, and at
//     75° horizontal over 1280 px that is ~9 px across — about 65 px, or
//     0.007% of the frame. A p90 needs ~92,000 pixels to move. Raising its
//     intensity is measuring the wrong thing.
//   · **Bloom cannot do it either.** `Bloom::NATURAL` is
//     `BloomCompositeMode::EnergyConserving`, whose final composite is
//     `{src: Constant, dst: OneMinusConstant}` — a lerp between the scene and
//     its blur. It redistributes energy and never adds any.
//
// **A top stop must come from AREA.** So: clouds, and they have to be their
// own draw.
//
// **Why a `Skybox` cubemap rather than a dome or a fullscreen pass**, which
// is the load-bearing decision in this file:
//
//   · `Skybox` draws at the end of `MainOpaquePass`, which is *before*
//     `AtmosphereNode::RenderSky`, so the atmosphere composites the clouds
//     itself as `dst = inscattering + transmittance·dst`. The clouds get the
//     same aerial perspective every other distant thing gets, from the same
//     owner. **`ART.md` rule 5 is satisfied by construction rather than by
//     tuning** — which is exactly what the browser client could not manage,
//     and it spent 200 lines of comment hand-fitting one horizon seam.
//   · An `AlphaMode::Blend` sky dome draws in `MainTransparentPass`, strictly
//     *after* the sky is resolved, so it would composite over finished sky
//     with no attenuation at all — and the fix for that is a `DistanceFog`,
//     which is a second owner of haze. That is the coupled-lighting failure
//     this repo has already paid for once.
//   · `FullscreenMaterial` (new in 0.18) binds only `(texture, sampler,
//     uniform)`: no depth, no view uniform. It cannot tell sky from geometry
//     and cannot even build a ray direction. It is a post-process API.
//   · Ordering a custom render node against the atmosphere is not possible:
//     `AtmosphereNode` lives in a private `mod node;` in `bevy_pbr` and is
//     never re-exported, so there is no edge to add.
//
// **What the eye sees of it is drawn per pixel since 2026-10-15**
// (`deck.rs`): wherever the atmosphere draws, a fullscreen triangle at depth
// 0 in the opaque pass takes the `Skybox`'s place — the same compositing,
// by the same owner, so everything above still holds. There is no `Skybox`
// and no cube any more, on any target.
//
// The deck is made from the world seed. No asset, no download: a tileable
// noise field ([`CloudField`]) is built at boot, and the deck's shader reads
// it wherever a ray meets the drifting deck — the wind drifting it, the
// weather thickening and darkening it ([`ComposeParams`], [`DeckLight`]),
// its lit side facing the sun it is actually lit by, and the night's moon
// behind it. The stars are their own draw (`stars.rs`).

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::curve::Curve;
use bevy::pbr::{Falloff, ScatteringMedium};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Cloud-deck altitude, metres. Cumulus bases sit near a kilometre; the exact
/// value only sets how fast the deck compresses toward the horizon.
pub const CLOUD_ALT_M: f32 = 1400.0;
/// Horizontal scale of the deck's noise, metres per unit.
pub const CLOUD_SCALE_M: f32 = 900.0;
/// How much of the sky carries cloud, 0..1. `ART.md` §1 asks for cumulus with
/// gaps — a solid overcast has no top stop either, because the bright tops are
/// what carry the range.
pub const CLOUD_COVER: f32 = 0.57;

/// Sunlit cumulus, cd/m². `Skybox::brightness` is documented in cd/m² and is
/// multiplied by `Exposure::exposure()` when the component is extracted, and
/// `exposure() = exp2(-ev100)/1.2` — so at the rig's ev100 14.2 this lands a
/// 1.0 texel at 1.0 linear, pre-tonemap. The top stop in physical units
/// rather than as a number somebody liked.
///
/// **It also must be set explicitly: `Skybox::brightness` defaults to 0.0**,
/// and a skybox at zero brightness is an invisible one.
pub const CLOUD_NITS: f32 = 26_000.0;

/// Lit top and grey base — the two values `ART.md` §1 names. A cumulus that
/// is one value reads as fog, and it is the lit top that carries the p90.
const CLOUD_TOP: [f32; 3] = [1.0, 0.99, 0.97];
const CLOUD_BASE: [f32; 3] = [0.42, 0.45, 0.52];

// ── The WebGL2 sky (browser sky v0 — DECISIONS.md §open) ───────────────────
//
// **Natively the atmosphere paints the sky and every clear pixel of the
// deck must stay ZERO** — the deck draws where a `Skybox` would, with no
// blend, so a non-zero clear pixel is a second sky added to
// `AtmosphereNode`'s own. The
// browser's WebGPU module has the atmosphere and is "natively" here. Under
// WebGL2 there is no atmosphere at all (`rig.rs`: `mesh_view_layout_
// atmosphere` wants a storage buffer and WebGL2 allows none), so the same
// zero pixel is the CLEAR COLOUR, and the first island a browser drew had
// white cumulus floating on black (`findings/web-build-20260909.md` §15.7).
// A page therefore bakes its sky INTO the deck: clouds composited over a
// clear-sky radiance rather than over nothing. Same clouds, same shader.
//
// What the browser still does not get, said out loud: the atmosphere's own
// aerial perspective (a browser gets `rig::fog_for`'s instead — the same air
// as a `DistanceFog`, and the only haze on that target) and a sun disk. Its
// dusk now colours rather than only dims: the deck greys the backdrop
// under a heavy sky and warms it toward a low sun (weather v0).

/// Whether the deck carries a clear sky behind the clouds. Zero
/// wherever the atmosphere draws (the desktop, and the browser's WebGPU
/// module), for the reason above; a sky under WebGL2, which has none.
pub const BAKE_BACKDROP: bool = cfg!(webgl2);

/// [`BAKE_BACKDROP`] as this device decides it: no atmosphere draws here
/// (`gpu::atmosphere`), so the deck draws the sky behind its clouds and the
/// air is a fog. What the fog's colour and glow follow at run time.
pub fn baked() -> bool {
    !super::gpu::atmosphere()
}

/// The horizon's luminance over the zenith's. A clear sky is brightest at
/// the horizon, where the eye looks through the most air: measured skies
/// put it at two to three times the zenith (Preetham et al.'s clear-sky
/// model at moderate turbidity), and the reference frames read the same way
/// — `ART.md` §1's "distant hills lighten" is this gradient landing on the
/// ground rather than the sky.
pub const HORIZON_GAIN: f32 = 2.4;

/// How far the horizon's colour is pulled toward neutral, `0..1`. Haze
/// scatters every wavelength, so the horizon is greyer than the zenith's
/// blue as well as brighter; at 0 the sky is one hue top to bottom.
pub const HORIZON_DESAT: f32 = 0.65;

/// The floor on `1 / (elevation + AIR_FLOOR)`, the air-mass curve the
/// gradient follows. A plane-parallel atmosphere's path length is `1/sin(e)`
/// and diverges at the horizon; the floor is what a curved one does to it —
/// Kasten & Young's air mass tops out near 38 at the horizon, i.e. a floor
/// near `1/38`. Larger reads as a broader, softer horizon band.
pub const AIR_FLOOR: f32 = 0.12;

/// Clear-sky radiance in world direction `d`, linear, **in the deck's texel
/// units** — `1.0` is one `CLOUD_NITS`, the same scale the clouds are stored
/// in, so the two composite in one space.
///
/// Zenith first, and it is DERIVED: `fill::sky_lux()` is the irradiance the
/// sky delivers to an up-facing surface, and a uniform sky of radiance `L`
/// delivers `π·L`, so the zenith's LUMINANCE is `sky_lux / π`, and its colour
/// is the air's own ([`air_chroma`]) rather than the fill's near-white tint,
/// which read as a grey sky on the first GPU that drew it. The horizon
/// is that, `HORIZON_GAIN` brighter and `HORIZON_DESAT` greyer, reached
/// along the air-mass curve; below the horizon the backdrop holds the
/// horizon colour flat (the sea and the land cover nearly all of it).
pub fn backdrop_at(d: Vec3) -> [f32; 3] {
    // Brightness from the fill, colour from the air. The fill's tint is an
    // ambient term's and nearly white on purpose; a sky that borrowed it
    // measured saturation 0.02 and read as overcast (2026-09-13).
    let lum = super::fill::luminance(super::fill::sky_lux()) / core::f32::consts::PI;
    let chroma = air_chroma();
    let zenith = [chroma[0] * lum, chroma[1] * lum, chroma[2] * lum];
    // The horizon: the zenith's luminance times the gain, with its chroma
    // pulled toward grey by the desaturation.
    let mut horizon = [0.0f32; 3];
    for c in 0..3 {
        let grey = zenith[c] + (lum - zenith[c]) * HORIZON_DESAT;
        horizon[c] = grey * HORIZON_GAIN;
    }
    // Air mass, normalised so `w` is 0 at the zenith and 1 at the horizon.
    let e = d.y.clamp(0.0, 1.0);
    let m = 1.0 / (e + AIR_FLOOR);
    let w = (m - 1.0 / (1.0 + AIR_FLOOR)) / (1.0 / AIR_FLOOR - 1.0 / (1.0 + AIR_FLOOR));
    let w = w.clamp(0.0, 1.0);
    [
        (zenith[0] + (horizon[0] - zenith[0]) * w) / CLOUD_NITS,
        (zenith[1] + (horizon[1] - zenith[1]) * w) / CLOUD_NITS,
        (zenith[2] + (horizon[2] - zenith[2]) * w) / CLOUD_NITS,
    ]
}

/// The air at ground level, per metre, as `(extinction, inscattering)`: each
/// term of `medium` at its density where the falloff parameter is `1`, the
/// dense end — each arm below is `bevy_pbr::medium::Falloff::sample` (private)
/// evaluated at `p = 1`, written out because that function is not `pub`: the
/// exponential's formula is exactly `1` there, a tent is wherever `1` sits on
/// its slope, and a curve is asked. Extinction is what a term removes,
/// absorption plus scattering; inscattering is what it sends on, scattering.
///
/// **Earthlike at the ground is Rayleigh and Mie, and not ozone**: its tent
/// peaks at `0.75` and is `0.3` wide, so it is zero at `1`. The browser's haze
/// (`rig::fog_for`) and its sky colour both read this off `rig::medium`, the
/// medium the desktop's atmosphere renders — a browser frame is thinner than a desktop
/// one, but it is made of the same air.
pub fn ground_air(medium: &ScatteringMedium) -> (Vec3, Vec3) {
    let (mut extinction, mut inscattering) = (Vec3::ZERO, Vec3::ZERO);
    for term in &medium.terms {
        let k = match term.falloff {
            Falloff::Linear | Falloff::Exponential { .. } => 1.0,
            Falloff::Tent { center, width } => (1.0 - (1.0 - center).abs() / (width * 0.5).max(f32::EPSILON)).max(0.0),
            Falloff::Curve(ref curve) => curve.sample(1.0).unwrap_or(0.0),
        };
        extinction += (term.absorption + term.scattering) * k;
        inscattering += term.scattering * k;
    }
    (extinction, inscattering)
}

/// The colour the air scatters, linear, at luminance 1: [`ground_air`]'s
/// inscattering normalised. Density scales how much air there is and not what
/// colour it is, so this does not move with `rig::AIR_DENSITY`. Computed once.
pub fn air_chroma() -> [f32; 3] {
    static CHROMA: std::sync::OnceLock<[f32; 3]> = std::sync::OnceLock::new();
    *CHROMA.get_or_init(|| {
        let (_, s) = ground_air(&super::rig::medium());
        let l = super::fill::luminance(s.to_array());
        [s.x / l, s.y / l, s.z / l]
    })
}

fn hash3(seed: u64, x: i32, y: i32, z: i32) -> f32 {
    let mut h = seed
        ^ (x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (z as i64 as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    (h >> 40) as f32 / 16_777_216.0
}

/// Value noise on a 2D lattice that wraps every `period` cells,
/// smoothstep-interpolated.
fn value_tiled(seed: u64, x: f32, y: f32, period: i32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (i, j) = (xi as i32, yi as i32);
    let w = |k: i32| k.rem_euclid(period);
    let a = hash3(seed, w(i), w(j), 0);
    let b = hash3(seed, w(i + 1), w(j), 0);
    let c = hash3(seed, w(i), w(j + 1), 0);
    let d = hash3(seed, w(i + 1), w(j + 1), 0);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sy
}

/// Four octaves, each half the amplitude and twice the frequency — and twice
/// the lattice period, so every octave tiles on the same [`FIELD_PERIOD`]
/// square and the deck can drift forever without a seam.
fn fbm_tiled(seed: u64, x: f32, y: f32) -> f32 {
    let mut f = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    let mut period = FIELD_PERIOD as i32;
    for o in 0..4 {
        f += amp * value_tiled(seed ^ (o as u64).wrapping_mul(0x9E37_79B9), x * freq, y * freq, period);
        amp *= 0.5;
        freq *= 2.0;
        period *= 2;
    }
    f
}

/// Billowed octaves of the same lattice: `|2v − 1|` folds each octave into
/// rounded lumps with creases between them, the cauliflower a cumulus edge
/// is made of. Starts an octave up, so it only roughens the puffs [`fbm_tiled`]
/// lays out and never moves where a cloud is.
fn billow_tiled(seed: u64, x: f32, y: f32) -> f32 {
    let mut f = 0.0;
    let mut amp = 0.5;
    let mut freq = 2.0;
    let mut period = FIELD_PERIOD as i32 * 2;
    for o in 0..3 {
        let v = value_tiled(
            seed ^ 0xb111_0000 ^ (o as u64).wrapping_mul(0x9E37_79B9),
            x * freq,
            y * freq,
            period,
        );
        f += amp * (2.0 * v - 1.0).abs();
        amp *= 0.5;
        freq *= 2.0;
        period *= 2;
    }
    f
}

// ── The deck's field and light (weather v0) ─────────────────────────────

/// Lattice period of the deck's noise, in noise units: the field tiles every
/// `FIELD_PERIOD × CLOUD_SCALE_M` ≈ 29 km, about the distance to the deck's
/// own cutoff at the horizon, so no repeat sits inside one view.
pub const FIELD_PERIOD: u32 = 32;
/// The field's texels per side: ~28 m each on a desktop release, where the
/// highest octave's ~110 m wavelength is four texels across. Half that in a
/// browser and in a debug build, where the boot pays for every texel twice.
pub const FIELD_N: usize = if cfg!(target_arch = "wasm32") || cfg!(debug_assertions) {
    512
} else {
    1024
};
/// How soft the deck's edge is where its shadow falls, metres: the sun's
/// own disk blurs a shadow cast from 1.4 km up by 13–40 m, and a cumulus
/// edge is a few tens of metres more. The field the rig dims the sun by
/// (`deck::SkyField::cover_at`) and the ground's cloud shadows read
/// (`cloud_shadow.rs`) has a texel about this size ([`SOFT_FIELD_N`]), so
/// the two agree at the eye.
pub const SOFT_M: f32 = 60.0;

/// That field's texels per side, the same on every target: a desktop
/// release draws its sky from a field twice as fine, but the light a
/// fighter stands in is read off this one, so a desktop player and a
/// browser player see the same sun on him at the same moment. Its texel is
/// 56 m, about [`SOFT_M`].
pub const SOFT_FIELD_N: usize = 512;

/// Below this `d.y` (~2.6°) there is no deck: `1/d.y` explodes.
pub const DECK_CUTOFF: f32 = 0.045;
/// The last stretch above the cutoff the deck fades over.
pub const HORIZON_FADE: f32 = 0.10;
/// How far the light march steps toward the sun, noise units.
const LIT_STEP: f32 = 0.35;
/// The coverage threshold's softness. Narrow enough that a cumulus has an
/// edge you could draw round, wide enough that the edge is still soft.
pub const EDGE: f32 = 0.13;
/// The billowed share of the field ([`billow_tiled`]).
const BILLOW: f32 = 0.3;
/// How much a thick cloud's underside darkens at its core: light that came
/// through a kilometre of cloud, against the thin rim the sun shines through.
pub const CORE_SHADE: f32 = 0.55;
/// How much brighter a thin rim reads than the underside, as a share of the
/// top-to-base step: the silver lining.
pub const RIM_GLOW: f32 = 0.45;
/// Below this `d.y` (~20°) the eye starts to see a cloud's lit side rather
/// than its base, and by [`DECK_CUTOFF`] it sees almost only that.
pub const SIDE_VIEW_Y: f32 = 0.34;
/// The moon's disk, radians (~1.7°) — drawn big, the way a person
/// remembers it. The sky's moon is `moon.rs`'s, a quad sharp at any
/// resolution: as texels it was five across in a browser and read as a
/// smudge with a stepped rim.
pub const MOON_R: f32 = 0.03;
/// The halo (~8.6°).
pub const HALO_COS: f32 = 0.988_77;
pub const MOON_RGB: [f32; 3] = [0.78, 0.8, 0.84];
pub const HALO_RGB: [f32; 3] = [0.02, 0.024, 0.032];
/// Moonlight scattered forward through the deck near the moon, deck units
/// at the moon itself: a thin edge drifting past it lights up silver
/// instead of passing as a dark smudge.
const MOON_SCATTER: f32 = 0.06;
/// How far round the moon that reaches, as a cosine (~20°).
pub const MOON_SCATTER_COS: f32 = 0.94;
/// How brightly a full moon lights the deck's underside at deep night, as a
/// share of the sun: dark shapes against the stars rather than nothing. A
/// thinner moon lights it less, down to the night's floor
/// (`rig::night_light`).
const NIGHT_CLOUD_LIGHT: f32 = 0.012;
/// Moonlight's colour on the deck, over the sun's: a little cool.
const MOONLIT: [f32; 3] = [0.8, 0.88, 1.0];
/// How much brighter the desktop draws its deck than its colours say, and so
/// the headroom they keep: `cloud.wgsl`'s `deck_hue` lifts a cloud top's
/// blue by 1.5 at the zenith and more below, and a top that clipped would
/// come out warm again. A browser has no atmosphere to undo and needs none.
pub const DECK_GAIN: f32 = deck_gain(BAKE_BACKDROP);

/// [`DECK_GAIN`] for a deck with (`backdrop`) or without a sky behind it.
pub const fn deck_gain(backdrop: bool) -> f32 {
    if backdrop {
        1.0
    } else {
        2.0
    }
}
/// The browser's night sky floor (the desktop's atmosphere has its own).
pub const NIGHT_SKY: [f32; 3] = [0.0015, 0.002, 0.0035];
/// The warm glow a low sun puts on the browser's horizon near it.
pub const DUSK_GLOW: [f32; 3] = [0.10, 0.045, 0.02];
/// What fog and haze are lit by after dark, so night fog is a dark grey
/// wall rather than a hole.
const NIGHT_FOG: [f32; 3] = [0.0025, 0.003, 0.0045];

/// The deck's noise, built once per world: a tileable fBm field the deck's
/// shader samples wherever a ray meets the drifting deck ([`Self::image`]).
pub struct CloudField {
    n: usize,
    data: Box<[f32]>,
}

impl CloudField {
    pub fn new(seed: u64, n: usize) -> Self {
        let mut data = vec![0.0f32; n * n];
        let step = FIELD_PERIOD as f32 / n as f32;
        for j in 0..n {
            for i in 0..n {
                let (x, y) = (i as f32 * step, j as f32 * step);
                data[j * n + i] = fbm_tiled(seed, x, y) * (1.0 - BILLOW) + billow_tiled(seed, x, y) * BILLOW;
            }
        }
        Self {
            n,
            data: data.into_boxed_slice(),
        }
    }

    /// Texels per side.
    pub fn n(&self) -> usize {
        self.n
    }

    /// The field as a repeating single-channel texture — what the stars,
    /// the moon and the per-pixel deck sample it from. Texel `i` is at
    /// `i · FIELD_PERIOD / n` noise units, so a sampler's centre is half a
    /// texel over (the shaders add it).
    ///
    /// **Mipped**, box-filtered and wrapping: toward the horizon a pixel
    /// spans hundreds of texels, and a sky drawn per pixel from level 0
    /// there shimmers as the deck drifts.
    pub fn image(&self) -> Image {
        let mut level = self.bytes();
        let mut data = level.clone();
        let mut n = self.n;
        let mut levels = 1;
        while n > 1 {
            let next = halve(&level, n);
            data.extend_from_slice(&next);
            level = next;
            n /= 2;
            levels += 1;
        }
        // `Image::new` asserts one level's worth of data; the chain follows it.
        let mut image = Image::new_uninit(
            Extent3d {
                width: self.n as u32,
                height: self.n as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            TextureFormat::R8Unorm,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.data = Some(data);
        image.texture_descriptor.mip_level_count = levels;
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            ..default()
        });
        image
    }

    /// Level 0 as [`Self::image`] stores it, eight bits a texel.
    fn bytes(&self) -> Vec<u8> {
        self.data
            .iter()
            .map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
            .collect()
    }

    /// Mip `level` of [`Self::image`], texel for texel: what the rig and
    /// the ground's cloud shadows read the deck over the sun from.
    pub fn soft(&self, level: u32) -> SoftField {
        let mut data = self.bytes();
        let mut n = self.n;
        for _ in 0..level {
            if n == 1 {
                break;
            }
            data = halve(&data, n);
            n /= 2;
        }
        let lod = (self.n / n).trailing_zeros();
        SoftField {
            n,
            offset: 0.5 - 0.5 / (1u32 << lod) as f32,
            data: data.into_boxed_slice(),
        }
    }
}

/// One mip step of the field: each texel the rounded mean of the four
/// under it.
fn halve(level: &[u8], n: usize) -> Vec<u8> {
    let h = n / 2;
    let mut next = vec![0u8; h * h];
    for j in 0..h {
        for i in 0..h {
            let at = |x: usize, y: usize| level[(y % n) * n + x % n] as u32;
            let sum = at(2 * i, 2 * j) + at(2 * i + 1, 2 * j) + at(2 * i, 2 * j + 1) + at(2 * i + 1, 2 * j + 1);
            next[j * h + i] = ((sum + 2) / 4) as u8;
        }
    }
    next
}

/// One mip of the deck's field ([`CloudField::soft`]), read the way the
/// ground's cloud shadows read it on the GPU (`cloud_shadow.wgsl`'s
/// `field_at`, from these very bytes): the same texels, the same centres,
/// the same bilinear. So the rig's sun at the eye and the shader's
/// reference for the eye are one number, and a far hill is lit by what
/// reaches it, not by the gap between two readings of one cloud.
pub struct SoftField {
    n: usize,
    /// Level-`L` texel `i`'s centre is level-0 texel `2^L·i + (2^L − 1)/2`:
    /// `(1 − 2^−L)/2` of its own texel on.
    offset: f32,
    data: Box<[u8]>,
}

impl SoftField {
    /// Texels per side.
    pub fn n(&self) -> usize {
        self.n
    }

    /// How far a texel's centre sits from its noise coordinate, texels.
    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// The texels, row by row.
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// Bilinear at noise-unit coordinates, wrapping.
    pub fn at(&self, q: Vec2) -> f32 {
        let u = q * (self.n as f32 / FIELD_PERIOD as f32) - Vec2::splat(self.offset);
        let i = u.floor();
        let f = u - i;
        let n = self.n as i64;
        let texel = |di: i64, dj: i64| {
            let x = (i.x as i64 + di).rem_euclid(n) as usize;
            let y = (i.y as i64 + dj).rem_euclid(n) as usize;
            self.data[y * self.n + x] as f32 / 255.0
        };
        let (a, b) = (texel(0, 0), texel(1, 0));
        let (c, d) = (texel(0, 1), texel(1, 1));
        let top = a + (b - a) * f.x;
        let bot = c + (d - c) * f.x;
        top + (bot - top) * f.y
    }
}

/// Everything one deck depends on: what the deck's uniform is made from
/// (`deck.rs`). Two equal params draw equal decks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComposeParams {
    /// How much of the sky carries cloud, `0..=1`.
    pub cover: f32,
    /// How heavy the cloud is: greyer bases, dimmer tops.
    pub dark: f32,
    /// The field's offset, noise units: the deck is read at `plane +
    /// drift`, so this is MINUS how far it has drifted downwind — a cloud
    /// moves the way the rain slants (`rain::params_for`), not against it.
    pub drift: Vec2,
    /// Toward the sun, unit.
    pub sun: Vec3,
    /// The light on the deck, a share of full sun (`rig::sun_lux`).
    pub light: f32,
    /// How far into the night, `0..=1`: stars and moon.
    pub night: f32,
    /// Toward the moon, unit.
    pub moon: Vec3,
    /// The weather's air over the sky ([`SkyFog`]).
    pub fog: SkyFog,
    /// How deep in the Verdigris the eye is, in sixteenths (`WeatherNow::fel`)
    /// — so walking in rewrites the deck at most sixteen times.
    pub fel: f32,
    /// Draw a clear sky behind the cloud (no atmosphere).
    pub backdrop: bool,
    /// How much of the moon's disk is lit, `0..=1` (`moon::lit_fraction`).
    pub moon_lit: f32,
    /// What the moon gives the night, `0..=1` (`WeatherNow::moonlight`).
    pub moonlight: f32,
    /// Toward the sun the moon is lit by, unit (`moon::lighting_sun`).
    pub moon_sun: Vec3,
    /// How dark the deck's underside goes where it is thickest, `0..=1`
    /// (`Mood::deep`).
    pub deep: f32,
    /// How much of the sky the mood's fog veils, `0..=1` (`Mood::veil`).
    pub veil: f32,
}

/// The Verdigris' cloud tops and bases, as multipliers on the deck's own:
/// lit from below by green fire, dark green in their bellies. **(knob)**
pub const FEL_CLOUD_TOP: [f32; 3] = [0.40, 0.80, 0.32];
pub const FEL_CLOUD_BASE: [f32; 3] = [0.16, 0.32, 0.12];
/// Its fog by day, as a share of the horizon's luminance, and its floor by
/// night, linear. **(knob)**
pub const FEL_FOG: [f32; 3] = [0.10, 0.22, 0.08];
pub const FEL_FOG_NIGHT: [f32; 3] = [0.004, 0.014, 0.004];

/// The Verdigris' fog colour under `light` (`rig::sun_lux`), linear: green by
/// day at the horizon's own brightness, a faint green floor by night.
pub fn fel_fog_rgb(light: f32) -> [f32; 3] {
    let lum = super::fill::luminance(backdrop_at(Vec3::X));
    core::array::from_fn(|c| FEL_FOG[c] * lum * light + FEL_FOG_NIGHT[c] * (1.0 - light))
}

/// [`fog_rgb_dust`] greened by the Verdigris `fel` deep ([`fel_fog_rgb`]):
/// the ground's flat fog (`rig::fog_for`), the deck's fog band, the sky's
/// fog layer and the shafts all take this one, so the horizon seam holds
/// in the green too.
pub fn fog_rgb_fel(light: f32, dark: f32, sigma: f32, dust: f32, fel: f32) -> [f32; 3] {
    let fog = fog_rgb_dust(light, dark, sigma, dust);
    let green = fel_fog_rgb(light);
    core::array::from_fn(|c| fog[c] + (green[c] - fog[c]) * fel)
}

/// How deep the ground fog lies, metres: a valley fog, so overhead the sky
/// still shows through it faintly while the horizon is gone.
pub const FOG_LAYER_M: f32 = 60.0;
/// How deep the rain's haze reads, metres: the rain falls from the cloud
/// base, but past this the deck above it still shows its shapes.
pub const RAIN_LAYER_M: f32 = 150.0;
/// How deep the dust lies, metres: a wind lifts sand a few hundred metres,
/// so a dusty sky is pale overhead and gone at the horizon.
pub const DUST_LAYER_M: f32 = 300.0;

/// The weather's fog over the sky, as optical depths: straight up through
/// each layer, and level across the farthest land the eye can see
/// (`rig::FAR_M`). A sky ray `y` above the horizon crosses `up / y` of a
/// layer, never more than the level reach — so the sky at the horizon is
/// fogged exactly as much as the island's far edge below it, and the seam
/// the old hand-set band left there is closed by construction. How much of
/// a sky ray it takes is `deck.wgsl`'s `deck_haze`, off [`Self::lanes`] and
/// [`Self::dust_lanes`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SkyFog {
    /// `σ × depth` of the ground fog and of the rain haze.
    pub up_fog: f32,
    pub up_rain: f32,
    /// `σ × rig::FAR_M` of each.
    pub far_fog: f32,
    pub far_rain: f32,
    /// The dust haze's (`WeatherNow::dust_part`), through [`DUST_LAYER_M`]
    /// and across the island.
    pub up_dust: f32,
    pub far_dust: f32,
}

impl SkyFog {
    pub const CLEAR: Self = Self {
        up_fog: 0.0,
        up_rain: 0.0,
        far_fog: 0.0,
        far_rain: 0.0,
        up_dust: 0.0,
        far_dust: 0.0,
    };

    /// The sky's fog for extinctions per metre `fog` and `rain`
    /// (`WeatherNow::fog_part`, `rain_part`).
    pub fn new(fog: f32, rain: f32) -> Self {
        let far = super::rig::FAR_M;
        Self {
            up_fog: fog * FOG_LAYER_M,
            up_rain: rain * RAIN_LAYER_M,
            far_fog: fog * far,
            far_rain: rain * far,
            up_dust: 0.0,
            far_dust: 0.0,
        }
    }

    /// This fog with a dust haze of extinction `dust` per metre in it.
    pub fn with_dust(self, dust: f32) -> Self {
        Self {
            up_dust: dust * DUST_LAYER_M,
            far_dust: dust * super::rig::FAR_M,
            ..self
        }
    }

    /// The fog and the rain as the shaders take them.
    pub fn lanes(&self) -> Vec4 {
        Vec4::new(self.up_fog, self.up_rain, self.far_fog, self.far_rain)
    }

    /// The dust as the shaders take it: x straight up, y across the island.
    pub fn dust_lanes(&self) -> Vec4 {
        Vec4::new(self.up_dust, self.far_dust, 0.0, 0.0)
    }

    /// The whole extinction per metre — what the ground's fog is greyed and
    /// tinted by ([`fog_rgb_dust`], `rig::fog_for`), so the sky's band is
    /// the same colour.
    pub fn sigma(&self) -> f32 {
        (self.far_fog + self.far_rain + self.far_dust) / super::rig::FAR_M
    }

    /// The dust's part of [`Self::sigma`], per metre — what the ground's
    /// fog is tinted by ([`fog_rgb_dust`]).
    pub fn dust(&self) -> f32 {
        self.far_dust / super::rig::FAR_M
    }
}

/// Air masses along a ray `y` above the horizon, over straight up (Kasten
/// & Young 1989): 1 at the zenith, ~38 at the horizon.
fn airmass(y: f32) -> f32 {
    let y = y.clamp(0.0, 1.0);
    let deg = y.asin().to_degrees();
    1.0 / (y + 0.505_72 * (deg + 6.079_95).powf(-1.636_4))
}

/// What reaches the ground of a sun `y` above the horizon through the
/// island's air, per channel: the transmittance `render_sky`'s LUT hands
/// every sunlit surface and Bevy's volumetric fog (`shafts.rs`), on the same
/// air mass `cloud.wgsl`'s `deck_hue` undoes.
pub fn sun_transmittance(y: f32) -> [f32; 3] {
    let tau = column_depth();
    let m = airmass(y);
    core::array::from_fn(|c| (-tau[c] * m).exp())
}

/// [`zenith_depth`] of the island's air, worked out once: what the per-pixel
/// deck (`cloud.wgsl`'s `deck_hue`) undoes the atmosphere's hue with.
///
/// `render_sky` composites the deck as `inscattering + T·deck`, with `T`
/// the whole column of air to space — warm even at the zenith, since this
/// air is 1.6× Earth's over a 100 km shell, and deep red at the deck's
/// horizon. So a grey overcast arrived tan and a storm at noon sepia. The
/// deck is 1.4 km up, under little of that column. The shader takes the
/// hue of `T` out of a cloud and leaves its dimming: distant cloud still
/// goes dark toward the horizon, which is the aerial perspective it is
/// there to get, and not orange. Wherever the atmosphere draws — not under
/// WebGL2, which has none.
///
/// **Per channel only where the device blends per channel.** `render_sky`
/// multiplies by `T` channel by channel through dual-source blending, and
/// by `T`'s MEAN on a device without it (Bevy's own fallback, and the case
/// for some browsers' WebGPU). Lifted against a grey multiply, a deck keeps
/// the blue it was given to cancel the red and every cloud comes out
/// periwinkle — measured 26 levels bluer than the desktop's in a SwiftShader
/// browser. There it only evens the brightness: `grey / mean(T)`, the same
/// in all three channels, so a cloud lands as bright as on the desktop and
/// as grey. [`super::gpu::GpuCaps::dual_source_blending`] says which.
pub fn column_depth() -> [f32; 3] {
    static TAU: std::sync::OnceLock<[f32; 3]> = std::sync::OnceLock::new();
    *TAU.get_or_init(|| zenith_depth(&super::rig::medium()))
}

/// The medium's optical depth from the ground straight up, per channel —
/// the zenith of `render_sky`'s transmittance LUT. A term's falloff runs
/// over the shell's `p` (1 at the ground, 0 at the top), so its column is
/// its density's mean over `p` times the shell's depth.
fn zenith_depth(medium: &ScatteringMedium) -> [f32; 3] {
    // `Atmosphere::earthlike`'s shell: 6 360 km to 6 460 km.
    const SHELL_M: f32 = 100_000.0;
    let mut tau = Vec3::ZERO;
    for term in &medium.terms {
        let mean = match term.falloff {
            Falloff::Linear => 0.5,
            Falloff::Exponential { scale } => scale,
            Falloff::Tent { width, .. } => width * 0.5,
            Falloff::Curve(ref curve) => {
                (0..64)
                    .map(|i| curve.sample((i as f32 + 0.5) / 64.0).unwrap_or(0.0))
                    .sum::<f32>()
                    / 64.0
            }
        };
        tau += (term.absorption + term.scattering) * mean * SHELL_M;
    }
    tau.to_array()
}

/// What a deck is lit with, worked out once from its params: the deck's
/// uniform (`deck.rs`) is made from it, so the targets cannot light one
/// deck two ways.
pub struct DeckLight {
    /// Toward the sun, horizontal, unit.
    pub sun_h: Vec2,
    /// The light march's step toward the sun, noise units.
    pub toward: Vec2,
    /// How strongly a low sun warms the horizon near it, `0..=1`.
    pub glow: f32,
    /// A lit top and a grey base, linear, deck units.
    pub top: [f32; 3],
    pub base: [f32; 3],
    /// Moonlit forward scatter at the moon, deck units.
    pub scatter: [f32; 3],
    /// What the deck's colours are scaled by ([`DECK_GAIN`]).
    pub gain: f32,
    /// The fog band's colour.
    pub fog_rgb: [f32; 3],
    /// The field value a cloud starts at.
    pub thresh: f32,
}

impl DeckLight {
    pub fn new(p: &ComposeParams) -> Self {
        let sun_h = Vec2::new(p.sun.x, p.sun.z).normalize_or_zero();
        // A low sun warms what it lights; one under the horizon lights nothing.
        let dusk = if p.sun.y > -0.1 {
            ((0.25 - p.sun.y) / 0.25).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let warm = [1.0, 1.0 - 0.35 * dusk, 1.0 - 0.6 * dusk];
        let cloud_light = p.light.max(NIGHT_CLOUD_LIGHT * p.night * night_light(p.moonlight));
        // The moon takes over from the sun as the night deepens.
        let tint: [f32; 3] = core::array::from_fn(|c| warm[c] + (MOONLIT[c] - warm[c]) * p.night);
        // The Verdigris greens the deck: lit tops, dark bellies.
        let top_k: [f32; 3] = core::array::from_fn(|c| 1.0 + (FEL_CLOUD_TOP[c] - 1.0) * p.fel);
        let base_k: [f32; 3] = core::array::from_fn(|c| 1.0 + (FEL_CLOUD_BASE[c] - 1.0) * p.fel);
        Self {
            sun_h,
            toward: sun_h * LIT_STEP,
            glow: dusk * ((p.sun.y + 0.1) / 0.1).clamp(0.0, 1.0),
            top: core::array::from_fn(|c| CLOUD_TOP[c] * (1.0 - 0.6 * p.dark) * tint[c] * cloud_light * top_k[c]),
            base: core::array::from_fn(|c| CLOUD_BASE[c] * (1.0 - 0.7 * p.dark) * tint[c] * cloud_light * base_k[c]),
            scatter: core::array::from_fn(|c| MOON_SCATTER * MOONLIT[c] * p.night * p.moonlight * (1.0 - 0.7 * p.dark)),
            gain: if p.backdrop { 1.0 } else { 1.0 / DECK_GAIN },
            fog_rgb: fog_rgb_fel(p.light, p.dark, p.fog.sigma(), p.fog.dust(), p.fel),
            thresh: 1.0 - p.cover,
        }
    }
}

/// The colour weather fog fades to, linear, in deck units (and, as the
/// browser's haze always was, as a `DistanceFog` colour): the sky's own
/// horizon, greyed as the fog thickens, dimmed by a heavy sky and the hour,
/// never quite black after dark.
pub fn fog_rgb(light: f32, dark: f32, grey: f32) -> [f32; 3] {
    let h = backdrop_at(Vec3::X);
    let lum = super::fill::luminance(h);
    let dim = light * (1.0 - 0.45 * dark);
    core::array::from_fn(|c| (h[c] + (lum - h[c]) * grey) * dim + NIGHT_FOG[c] * (1.0 - light))
}

/// How grey a fog of extinction `sigma` is, `0..=1`: thin weather keeps
/// the sky's blue, thick fog is neutral.
pub fn fog_grey(sigma: f32) -> f32 {
    (sigma * 400.0).clamp(0.0, 1.0)
}

/// The colour sand in the air scatters, linear, at luminance 1: a pale
/// ochre, the reference's beige. Dust takes blue out of what it sends on
/// and leaves the brightness, so the haze tints the fog it is part of and
/// does not light it.
pub const DUST_RGB: [f32; 3] = [1.28, 0.957, 0.6];

/// [`fog_rgb`] for a fog of extinction `sigma` per metre of which `dust`
/// is sand (`WeatherNow::dust_part`): pulled toward [`DUST_RGB`] at its own
/// luminance by dust's share of the air. The share is of everything that
/// fogs the frame — under WebGL2 that includes the island's own air, which
/// is the haze's floor there (`rig::fog_for`), so the first grain of sand
/// tints a grain's worth and not the whole sky.
pub fn fog_rgb_dust(light: f32, dark: f32, sigma: f32, dust: f32) -> [f32; 3] {
    let rgb = fog_rgb(light, dark, fog_grey(sigma));
    if dust <= 0.0 {
        return rgb;
    }
    let floor = if baked() {
        static AIR: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
        *AIR.get_or_init(|| super::fill::luminance(ground_air(&super::rig::medium()).0.to_array()))
    } else {
        0.0
    };
    let share = (dust / (sigma + floor).max(f32::EPSILON)).min(1.0);
    let lum = super::fill::luminance(rgb);
    core::array::from_fn(|c| rgb[c] + (DUST_RGB[c] * lum - rgb[c]) * share)
}

/// How much of the sun a fog throws forward toward the eye, as Bevy's
/// `DistanceFog::directional_light_color` alpha: droplets scatter mostly
/// forward, so a fog bank with the sun behind it glows. Bevy weighs it by
/// each directional light's own illuminance and its shadow at the fogged
/// point, so it is nothing at night, a sheet toward a lightning bolt, and
/// dark where the trees shade the fog.
pub const FOG_SUN_GLOW: f32 = 0.05;
/// How tight the glow is round the sun (`directional_light_exponent`).
pub const FOG_SUN_EXPONENT: f32 = 10.0;

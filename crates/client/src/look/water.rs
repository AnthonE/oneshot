//! The sea: a plane at a map's sea level over its terrain, drawn as a
//! graded volume with a swell on it, the sky in its reflection, the sun on
//! its waves and a wash standing off the shore.
//!
//! After Gates' `crates/client/src/render/water.rs` and
//! `assets/shaders/water.wgsl` (MIT, © 2026 MoreRight DAO; NOTICE): its
//! waves, optics, shore wash, ripple map and material, verbatim where they
//! are kept; without the island's eye-centred streaming grid, breakers,
//! whitecaps, weather, lakes, rain, wakes, hour and underwater, since this
//! sea is one still body a few hundred metres across at noon and nobody
//! swims in it.
//!
//! Drawing only: the sim knows nothing of the sea (a map's sea level is the
//! client's, like its light). The mesh is built ONCE over the map's own
//! terrain grid with the ground mesh's split of each cell, so the water
//! column interpolated under a pixel is exactly the ground's height there
//! and the waterline falls where the two meshes cross; past the terrain a
//! skirt of geometric rings reaches the camera's far plane. The vertex
//! stage draws the swell (`water.wgsl`); the CPU writes the phases, once a
//! frame, into the seas' motion texture (`sea_motion.rs`), and nothing else.

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use bevy::shader::ShaderRef;
use sim::terrain::{TerrainDef, HEIGHT_Q};

/// Both stages: the swell in the vertex, the optics in the fragment.
const SHADER: &str = "shaders/water.wgsl";

// ---------------------------------------------------------------------------
// The mesh: the terrain's wet cells, and a skirt of geometric rings.
// ---------------------------------------------------------------------------

/// A terrain cell with every corner this far above the sea is dry and left
/// out of the mesh (the swell never climbs a metre).
pub const CULL_M: f32 = 1.2;
/// The first skirt ring's gap past the terrain, metres, and how much each
/// ring grows over the last: 1.7 puts the next at 3.4 m, barely coarser
/// than the core, so the wave weights fade rather than step at the join.
pub const SKIRT_FROM_M: f32 = 2.0;
pub const SKIRT_RATIO: f32 = 1.7;
/// How far past the terrain the skirt reaches: to the horizon from any
/// eye in play (from 14 m up, a sea ending at 1 km leaves a band of the
/// planet under the sky; at 5 km the two meet within a pixel).
pub const SKIRT_REACH_M: f32 = 5_000.0;
/// The water column a skirt vertex is given, metres: deep by fiat, past
/// where any curve here still moves (`water.wgsl`'s `DEEP_M`).
pub const DEEP_M: f32 = 64.0;

/// The skirt's gaps, first ring outward, until the reach passes
/// [`SKIRT_REACH_M`].
pub fn skirt_gaps() -> Vec<f32> {
    let mut gaps = Vec::new();
    let (mut gap, mut reach) = (SKIRT_FROM_M, 0.0);
    while reach < SKIRT_REACH_M {
        gap *= SKIRT_RATIO;
        reach += gap;
        gaps.push(gap);
    }
    gaps
}

// ---------------------------------------------------------------------------
// The waves.
// ---------------------------------------------------------------------------

/// One directional wave: a unit direction in world XZ, its wavelength and
/// its amplitude, metres (the peak displacement is [`SHAPE_PEAK`] times
/// the amplitude).
#[derive(Clone, Copy)]
pub struct Wave {
    pub dir: [f32; 2],
    pub len_m: f32,
    pub amp_m: f32,
}

pub const WAVE_COUNT: usize = 4;

/// The swell: four lengths roughly an octave apart on four bearings inside
/// a ~50° fan (a real sea state is a narrow directional spectrum; four
/// trains crossing at right angles read as a bathtub). Directions are
/// authored as unit vectors: no trig at run time.
pub const WAVES: [Wave; WAVE_COUNT] = [
    Wave {
        dir: [0.9397, 0.3420],
        len_m: 52.0,
        amp_m: 0.42,
    },
    Wave {
        dir: [0.7660, 0.6428],
        len_m: 31.0,
        amp_m: 0.26,
    },
    Wave {
        dir: [0.9848, -0.1736],
        len_m: 17.0,
        amp_m: 0.13,
    },
    Wave {
        dir: [0.6428, -0.7660],
        len_m: 9.0,
        amp_m: 0.06,
    },
];

/// Gravity, m/s²: the wave speeds are derived (`ω = sqrt(g·k)`), never
/// chosen, so the long waves outrun the short ones by construction.
pub const GRAVITY: f32 = 9.81;
/// Peak of the crest shape `2u² − ¾` (`u = ½ + ½ sin θ`) in amplitudes, and
/// its slope variance over a cycle (`E[((1 + sin θ) cos θ)²] = ⅝`).
pub const SHAPE_PEAK: f32 = 1.25;
pub const SHAPE_SLOPE_VAR: f32 = 0.625;
/// The least `N·V` a wave face may show the eye: a face turned past the
/// view at the horizon is bent back to graze, never past the flat sea's.
pub const HORIZON_NDOTV: f32 = 0.05;
/// Depth at which the swell is at full height, metres.
pub const SHOAL_FULL_M: f32 = 3.5;

/// Angular frequency of a deep-water wave of this length, rad/s.
pub fn omega(len_m: f32) -> f32 {
    let k = core::f32::consts::TAU / len_m;
    (GRAVITY * k).sqrt()
}

/// How much of a wave survives on a mesh with this vertex spacing: full at
/// a quarter wavelength or finer, gone at a half, smooth between. A wave
/// sampled at half its own wavelength is not a coarse wave but a different
/// one, a slow wrong one crawling across the horizon; retiring each wave
/// on the spacing that samples it is what flattens the far sea without a
/// distance fade.
pub fn band_weight(spacing_m: f32, len_m: f32) -> f32 {
    let quarter = len_m * 0.25;
    let half = len_m * 0.5;
    if spacing_m <= quarter {
        return 1.0;
    }
    if spacing_m >= half {
        return 0.0;
    }
    let t = (half - spacing_m) / (half - quarter);
    t * t * (3.0 - 2.0 * t)
}

/// How much of the swell survives in water this deep: zero at the
/// waterline, or the sea climbs the beach and sits there.
pub fn shoal(depth_m: f32) -> f32 {
    let t = (depth_m / SHOAL_FULL_M).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far the surface can stand above and fall below the sea level,
/// metres: every wave at [`SHAPE_PEAK`] up and ¾ of an amplitude down. The
/// culling box, since the CPU mesh is flat.
pub fn surface_bounds() -> (f32, f32) {
    let a: f32 = WAVES.iter().map(|w| w.amp_m).sum();
    (SHAPE_PEAK * a, 0.75 * a)
}

/// The whole swell's slope variance, `SHAPE_SLOPE_VAR · Σ (aᵢ·kᵢ)²`: what a
/// pixel too far off to resolve any wave sees as roughness in the glint.
pub fn hidden_slope() -> f32 {
    let v: f32 = WAVES
        .iter()
        .map(|w| {
            let ak = w.amp_m * core::f32::consts::TAU / w.len_m;
            ak * ak
        })
        .sum();
    SHAPE_SLOPE_VAR * v
}

// ---------------------------------------------------------------------------
// The optics: a coastal sea, as a volume.
// ---------------------------------------------------------------------------

/// Extinction per metre, per channel: red goes first, green carries
/// furthest (an Atlantic/Baltic green-blue, not a tropical one).
pub const EXTINCT: [f32; 3] = [0.62, 0.16, 0.22];
/// Single-scattering albedo: of what [`EXTINCT`] takes out of a beam, the
/// share sent back to the eye. The sea's colour, accumulating with depth.
pub const SCATTER_ALBEDO: [f32; 3] = [0.018, 0.125, 0.100];
/// The most opaque the sea gets: short of 1 so a slope under it stays
/// readable.
pub const ALPHA_MAX: f32 = 0.94;
/// `StandardMaterial::reflectance` for water: Bevy maps it to
/// `F0 = 0.16·r²`, and water's is 2 % (`((1.333 − 1)/(1.333 + 1))²`).
pub const WATER_REFLECTANCE: f32 = 0.353_553_4;
pub const WATER_IOR: f32 = 1.333;
/// Not zero: a mirror sea has one specular pixel of sun and nothing else.
pub const WATER_ROUGHNESS: f32 = 0.08;
/// Metres of water over which the reflection and the foam fade in from
/// the line where the surface meets the ground: a polygon intersection
/// drawn at full strength is a drawn line.
pub const EDGE_M: f32 = 0.12;
/// The glint's GGX `α²` at the eye and at [`GLINT_FAR_M`]: near, the ripple
/// map supplies the slope; far, its mips have averaged the ripples flat and
/// the slope they no longer resolve comes back as roughness.
pub const GLINT_A2_NEAR: f32 = 0.0016;
pub const GLINT_A2_FAR: f32 = 0.0141;
pub const GLINT_FAR_M: f32 = 400.0;
/// How far the reflected clear sky's chroma is pulled toward grey: the
/// air's raw single-scatter blue is a little bluer than the sky the
/// desktop draws (less so since its daylight was pushed toward blue,
/// `rig::SKY_LIFT_SATURATION`; at 0.45 the sea lay grey under a blue sky).
pub const SKY_DESAT: f32 = 0.15;

/// Water's normal-incidence Fresnel reflectance as Bevy derives it.
pub fn water_f0() -> f32 {
    0.16 * WATER_REFLECTANCE * WATER_REFLECTANCE
}

// ---------------------------------------------------------------------------
// The shore wash: a band standing off the waterline.
// ---------------------------------------------------------------------------

/// How deep the wash reaches, metres (its outer edge; Gates' 2.6 m,
/// narrowed so a wading shelf keeps clear water past its surf), and
/// where it is thickest: offshore of the waterline, so the brightest
/// thing on the sea is not the polygon edge where it meets the sand.
pub const FOAM_DEPTH_M: f32 = 1.4;
pub const FOAM_PEAK_M: f32 = 0.60;
/// Shore slope (rise over run) at which the wash is full, and what a flat
/// shore still gets.
pub const FOAM_SLOPE_FULL: f32 = 0.30;
pub const FOAM_SLOPE_FLOOR: f32 = 0.35;
/// How far the noise moves the band's contour, metres of depth: an
/// iso-depth contour of a smooth seabed reads as drafting; displaced, both
/// edges become lobes and fingers.
pub const FOAM_JITTER_M: f32 = 0.55;
/// The coarse octave of that noise, metres: one lobe of wash.
pub const FOAM_NOISE_M: f32 = 9.0;
/// The quietest the wash gets between surges, of its peak.
pub const FOAM_SURGE_FLOOR: f32 = 0.45;
/// Foam colour, linear, and the alpha of a fully foamed pixel: aerated
/// water is bright but it is not paper.
pub const FOAM_BODY: [f32; 3] = [0.55, 0.60, 0.60];
pub const FOAM_ALPHA: f32 = 0.96;

/// A 32-bit mix (Gates' `props::hash2`), and it in `[0, 1)`.
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

/// Two octaves of value noise in world metres, in `[0, 1]`: the field the
/// wash band's contour is displaced by.
pub fn foam_noise(x: f32, z: f32) -> f32 {
    fn lattice(x: f32, z: f32) -> f32 {
        let (xi, zi) = (x.floor(), z.floor());
        let (fx, fz) = (x - xi, z - zi);
        let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
        let (i, j) = (xi as i32 as u32, zi as i32 as u32);
        let h = hash01;
        let top = h(i, j) + (h(i.wrapping_add(1), j) - h(i, j)) * sx;
        let bot = h(i, j.wrapping_add(1)) + (h(i.wrapping_add(1), j.wrapping_add(1)) - h(i, j.wrapping_add(1))) * sx;
        top + (bot - top) * sz
    }
    let coarse = lattice(x / FOAM_NOISE_M, z / FOAM_NOISE_M);
    let fine = lattice(x / (FOAM_NOISE_M * 0.34) + 31.0, z / (FOAM_NOISE_M * 0.34) - 17.0);
    (coarse * 0.68 + fine * 0.32).clamp(0.0, 1.0)
}

/// The wash at a vertex: a band from the waterline to [`FOAM_DEPTH_M`],
/// peaking at [`FOAM_PEAK_M`], read at a depth displaced by `noise`,
/// weighted by how steeply the LAND rises there (`slope`, rise over run).
pub fn shore_foam(depth_m: f32, slope: f32, noise: f32) -> f32 {
    let d = depth_m + FOAM_JITTER_M * (noise.clamp(0.0, 1.0) - 0.5) * 2.0;
    if d <= 0.0 || d >= FOAM_DEPTH_M {
        return 0.0;
    }
    let t = if d < FOAM_PEAK_M {
        d / FOAM_PEAK_M
    } else {
        1.0 - (d - FOAM_PEAK_M) / (FOAM_DEPTH_M - FOAM_PEAK_M)
    };
    let band = t * t * (3.0 - 2.0 * t);
    let steep = (slope / FOAM_SLOPE_FULL).clamp(0.0, 1.0);
    band * (FOAM_SLOPE_FLOOR + (1.0 - FOAM_SLOPE_FLOOR) * steep) * (0.55 + 0.45 * noise)
}

// ---------------------------------------------------------------------------
// The ripple map: capillary detail below the shortest wave, a texture.
// ---------------------------------------------------------------------------

/// Edge in texels, world size of one tile, and how the layer drifts over
/// the world, tiles a second (slow: it rides the swell, it is not one).
pub const RIPPLE_TEX: u32 = 256;
pub const RIPPLE_TILE_M: f32 = 12.0;
pub const RIPPLE_DRIFT: [f32; 2] = [0.028, 0.011];

/// The ripple field as integer harmonics of the tile, so it tiles exactly:
/// `(nx, nz, amplitude in metres)`.
const RIPPLES: [(f32, f32, f32); 6] = [
    (3.0, 1.0, 0.030),
    (1.0, -2.0, 0.024),
    (5.0, 3.0, 0.016),
    (-2.0, 5.0, 0.014),
    (7.0, -4.0, 0.009),
    (4.0, 8.0, 0.007),
];

/// The field's gradient at `(u, v)` in tile units, per metre.
fn ripple_at(u: f32, v: f32) -> (f32, f32) {
    let (mut du, mut dv) = (0.0f32, 0.0f32);
    for (nx, nz, a) in RIPPLES {
        let w = core::f32::consts::TAU * (nx * u + nz * v);
        let c = w.cos();
        du += a * c * core::f32::consts::TAU * nx;
        dv += a * c * core::f32::consts::TAU * nz;
    }
    (du / RIPPLE_TILE_M, dv / RIPPLE_TILE_M)
}

/// The tiling ripple normal map with its whole mip chain, built by hand:
/// Bevy builds none for an image made in code, and a one-level normal map
/// on a surface hundreds of metres across is a shimmering carpet of
/// aliased highlights. Each level is a box filter of the last with the
/// normals renormalised, so distant water goes smooth on its own.
pub fn ripple_map() -> Image {
    let n = RIPPLE_TEX;
    let levels = n.trailing_zeros() + 1;
    let mut level: Vec<[f32; 3]> = Vec::with_capacity((n * n) as usize);
    for y in 0..n {
        for x in 0..n {
            let u = (x as f32 + 0.5) / n as f32;
            let v = (y as f32 + 0.5) / n as f32;
            let (du, dv) = ripple_at(u, v);
            let l = (du * du + dv * dv + 1.0).sqrt();
            level.push([-du / l, -dv / l, 1.0 / l]);
        }
    }
    let mut data: Vec<u8> = Vec::with_capacity(ripple_bytes(n));
    let mut size = n;
    for lvl in 0..levels {
        if lvl > 0 {
            let half = size / 2;
            let mut next = Vec::with_capacity((half * half) as usize);
            for y in 0..half {
                for x in 0..half {
                    let mut acc = [0.0f32; 3];
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let s = level[((y * 2 + dy) * size + (x * 2 + dx)) as usize];
                            for (a, c) in acc.iter_mut().zip(s) {
                                *a += c * 0.25;
                            }
                        }
                    }
                    let l = (acc[0] * acc[0] + acc[1] * acc[1] + acc[2] * acc[2]).sqrt().max(1e-6);
                    next.push([acc[0] / l, acc[1] / l, acc[2] / l]);
                }
            }
            level = next;
            size = half;
        }
        for texel in &level {
            for c in texel {
                data.push(((c * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            data.push(255);
        }
    }
    // `Image::new` knows level 0 alone: build from it, then declare the
    // chain and swap the whole buffer in (wgpu reads the levels back out in
    // the order they were written).
    let mut image = Image::new(
        Extent3d {
            width: n,
            height: n,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data[..(n * n * 4) as usize].to_vec(),
        // Unorm, not sRGB: a normal map is a vector, not a colour.
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = levels;
    image.data = Some(data);
    // Repeat: the UVs are world metres over the tile and reach the
    // hundreds; Bevy's default clamps.
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

/// How many bytes [`ripple_map`] holds for an edge: the whole chain.
pub fn ripple_bytes(edge: u32) -> usize {
    let mut total = 0usize;
    let mut n = edge;
    loop {
        total += (n * n * 4) as usize;
        if n == 1 {
            break;
        }
        n /= 2;
    }
    total
}

// ---------------------------------------------------------------------------
// The material.
// ---------------------------------------------------------------------------

/// The per-material uniform: one struct of vec4 lanes (WebGL2's 16-byte
/// rule). Field order and names mirror `struct Water` in `water.wgsl`.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub struct WaterParams {
    /// xyz [`EXTINCT`], w [`ALPHA_MAX`].
    pub optics: Vec4,
    /// xyz [`SCATTER_ALBEDO`], w [`WATER_IOR`].
    pub scatter: Vec4,
    /// xyz [`FOAM_BODY`], w [`FOAM_ALPHA`].
    pub foam: Vec4,
    /// xyz the clear sky's zenith, cd/m²; w the analytic sky's share of the
    /// reflection: 1 (the deck's sky is drawn per pixel and cannot be
    /// sampled by a forward material, so the clear sky stands in).
    pub zenith: Vec4,
    /// xyz the clear sky's horizon, cd/m²; w `sky::AIR_FLOOR`.
    pub horizon: Vec4,
    /// xyz toward the sun (the map's mood's); w cd/m² per texel of the sky
    /// cube: 0, the cube is black.
    pub sun: Vec4,
    /// x `F0`, y [`GLINT_A2_NEAR`], z [`GLINT_A2_FAR`], w [`GLINT_FAR_M`].
    pub glint: Vec4,
    /// x [`EDGE_M`], y [`FOAM_SURGE_FLOOR`], z [`HORIZON_NDOTV`], w
    /// [`hidden_slope`].
    pub shore: Vec4,
    /// A wave of [`WAVES`] a slot: xy its direction, z one over its length
    /// (cycles a metre), w its amplitude. Their phases and the ripples'
    /// drift move every frame, so they are not here but in
    /// `sea_motion::MOTION` ([`animate`]).
    pub waves: [Vec4; WAVE_COUNT],
}

/// The clear sky the sea reflects, cd/m²: the browser's backdrop
/// (`sky::backdrop_at`), desaturated by [`SKY_DESAT`], `(zenith, horizon)`.
fn sky_lanes() -> ([f32; 3], [f32; 3]) {
    let nits = super::sky::CLOUD_NITS;
    let grade = |b: [f32; 3]| -> [f32; 3] {
        let lum = super::fill::luminance(b);
        core::array::from_fn(|c| (b[c] + (lum - b[c]) * SKY_DESAT) * nits)
    };
    (
        grade(super::sky::backdrop_at(Vec3::Y)),
        grade(super::sky::backdrop_at(Vec3::X)),
    )
}

impl WaterParams {
    /// A clear day under the sun at `sun`.
    pub fn noon(sun: Vec3) -> Self {
        let (zenith, horizon) = sky_lanes();
        Self {
            optics: Vec4::new(EXTINCT[0], EXTINCT[1], EXTINCT[2], ALPHA_MAX),
            scatter: Vec4::new(SCATTER_ALBEDO[0], SCATTER_ALBEDO[1], SCATTER_ALBEDO[2], WATER_IOR),
            foam: Vec4::new(FOAM_BODY[0], FOAM_BODY[1], FOAM_BODY[2], FOAM_ALPHA),
            zenith: Vec4::new(zenith[0], zenith[1], zenith[2], 1.0),
            horizon: Vec4::new(horizon[0], horizon[1], horizon[2], super::sky::AIR_FLOOR),
            sun: sun.normalize_or(Vec3::Y).extend(0.0),
            glint: Vec4::new(water_f0(), GLINT_A2_NEAR, GLINT_A2_FAR, GLINT_FAR_M),
            shore: Vec4::new(EDGE_M, FOAM_SURGE_FLOOR, HORIZON_NDOTV, hidden_slope()),
            waves: WAVES.map(|w| Vec4::new(w.dir[0], w.dir[1], 1.0 / w.len_m, w.amp_m)),
        }
    }
}

/// The extension over `StandardMaterial`: the uniform and the sky's cube.
/// Not bindless (a non-bindless extension makes the whole material so).
#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct Water {
    #[uniform(100)]
    pub params: WaterParams,
    /// The sky the surface reflects: the camera's `Skybox` image in a
    /// browser ([`animate`] copies it in), a black 1×1 cube on the desktop.
    #[texture(101, dimension = "cube")]
    #[sampler(102)]
    pub sky: Handle<Image>,
    /// The waves' phases and the ripples' drift (`sea_motion.rs`).
    #[texture(103, sample_type = "float", filterable = false)]
    pub motion: Handle<Image>,
}

pub type WaterMaterial = ExtendedMaterial<StandardMaterial, Water>;

impl MaterialExtension for Water {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    /// Translucent, so the prepass skips it anyway; off, so no pipeline is
    /// specialised against a vertex stage that does not displace.
    fn enable_prepass() -> bool {
        false
    }
    /// The seabed must not wear the sea's silhouette.
    fn enable_shadows() -> bool {
        false
    }
    fn deferred_fragment_shader() -> ShaderRef {
        ShaderRef::Default
    }
}

/// A 1×1 black cube: the reflection's deck where there is no `Skybox`.
fn black_cube() -> Image {
    let mut image = Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        vec![0u8; 6 * 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image
}

/// The sea's material over `ripples` ([`ripple_map`]) and `sky` (a cube),
/// under the sun toward `sun`.
pub fn material(ripples: Handle<Image>, sky: Handle<Image>, sun: Vec3) -> WaterMaterial {
    ExtendedMaterial {
        base: StandardMaterial {
            // White: the shader writes the body colour itself, off the depth.
            base_color: Color::WHITE,
            normal_map_texture: Some(ripples),
            perceptual_roughness: WATER_ROUGHNESS,
            metallic: 0.0,
            // Zeroed in the shader before Bevy's lighting: the sky and the
            // glint are drawn by hand, and Bevy's own specular would add
            // the fill's reflection on top. Kept physical so it says what
            // water is.
            reflectance: WATER_REFLECTANCE,
            ior: WATER_IOR,
            // What keeps the sky in the shallows: the body carries its own
            // alpha and the Fresnel sky is added at full strength.
            alpha_mode: AlphaMode::Premultiplied,
            // One side (the default): the eye never goes under, since the
            // map holds its wading shelf to 0.9 m (the canyon's `sea.shelf`
            // in compile.mjs) against a crouched eye at 1.1, and from above
            // a back face is the far slope of a crest.
            ..default()
        },
        extension: Water {
            params: WaterParams::noon(sun),
            sky,
            motion: super::sea_motion::MOTION,
        },
    }
}

// ---------------------------------------------------------------------------
// The mesh.
// ---------------------------------------------------------------------------

/// The sea over `t` at `sea_y`, in world coordinates (identity transform),
/// and the box its vertex stage can reach.
///
/// The core is the terrain's own grid, the cells with a corner under
/// `sea_y + CULL_M`, split as the ground mesh splits them (low corner to
/// high corner). Round it on the +x and ±z sides, rings of geometric
/// spacing ([`skirt_gaps`]) carry the sea to the far plane, [`DEEP_M`]
/// deep; the −x end is a closed canyon. Per vertex: position flat at
/// `sea_y`, normal up, tangent along +x (the ripple map's u), UV_0 world
/// xz in ripple tiles, UV_1 `(wash, water column)`, and in the colour each
/// wave's share, `band_weight(spacing) · shoal(depth)`.
pub fn mesh(t: &TerrainDef, sea_y: f32) -> (Mesh, Aabb) {
    let (nx, nz) = (t.nx as usize, t.nz as usize);
    let h = |i: usize, j: usize| t.heights[j * nx + i] as f32 / HEIGHT_Q;
    let gaps = skirt_gaps();
    let rings = gaps.len();
    // The extended grid's coordinates: the terrain's, then the rings.
    let mut xs: Vec<f32> = (0..nx).map(|i| t.x0 + i as f32 * t.cell).collect();
    let mut zs: Vec<f32> = Vec::with_capacity(nz + 2 * rings);
    let mut reach = 0.0;
    let mut ring = Vec::with_capacity(rings);
    for g in &gaps {
        reach += g;
        xs.push(t.x0 + (nx - 1) as f32 * t.cell + reach);
        ring.push(reach);
    }
    zs.extend(ring.iter().rev().map(|r| t.z0 - r));
    zs.extend((0..nz).map(|j| t.z0 + j as f32 * t.cell));
    zs.extend(ring.iter().map(|r| t.z0 + (nz - 1) as f32 * t.cell + r));
    let (gx, gz) = (xs.len(), zs.len());
    // The terrain's vertex under a grid vertex, if it is one.
    let core = |ix: usize, jz: usize| -> Option<(usize, usize)> {
        if ix < nx && jz >= rings && jz < rings + nz {
            Some((ix, jz - rings))
        } else {
            None
        }
    };
    // Whether the cell whose low corner is this grid vertex is drawn.
    let wet = |ix: usize, jz: usize| -> bool {
        let Some((i, j)) = core(ix, jz) else {
            return true;
        };
        if i + 1 >= nx || j + 1 >= nz {
            // A cell of the skirt's first ring.
            return true;
        }
        let top = sea_y + CULL_M;
        h(i, j) < top || h(i + 1, j) < top || h(i, j + 1) < top || h(i + 1, j + 1) < top
    };
    // The wider of a vertex's two gaps on each axis: at a join it is
    // judged by the coarser side.
    let gap = |c: &[f32], k: usize| -> f32 {
        let lo = if k > 0 { c[k] - c[k - 1] } else { 0.0 };
        let hi = if k + 1 < c.len() { c[k + 1] - c[k] } else { 0.0 };
        lo.max(hi)
    };

    const NONE: u32 = u32::MAX;
    let mut id = vec![NONE; gx * gz];
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nrm: Vec<[f32; 3]> = Vec::new();
    let mut tan: Vec<[f32; 4]> = Vec::new();
    let mut uv0: Vec<[f32; 2]> = Vec::new();
    let mut uv1: Vec<[f32; 2]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
    let mut vertex = |ix: usize, jz: usize| -> u32 {
        let slot = jz * gx + ix;
        if id[slot] != NONE {
            return id[slot];
        }
        let (x, z) = (xs[ix], zs[jz]);
        let (depth, wash) = match core(ix, jz) {
            Some((i, j)) => {
                let depth = sea_y - h(i, j);
                let dx = (h((i + 1).min(nx - 1), j) - h(i.saturating_sub(1), j)) / (2.0 * t.cell);
                let dz = (h(i, (j + 1).min(nz - 1)) - h(i, j.saturating_sub(1))) / (2.0 * t.cell);
                let slope = (dx * dx + dz * dz).sqrt();
                let wash = if depth > -FOAM_JITTER_M && depth < FOAM_DEPTH_M + FOAM_JITTER_M {
                    shore_foam(depth, slope, foam_noise(x, z))
                } else {
                    0.0
                };
                (depth, wash)
            }
            None => (DEEP_M, 0.0),
        };
        let spacing = gap(&xs, ix).max(gap(&zs, jz));
        let share = shoal(depth);
        pos.push([x, sea_y, z]);
        nrm.push([0.0, 1.0, 0.0]);
        tan.push([1.0, 0.0, 0.0, -1.0]);
        uv0.push([x / RIPPLE_TILE_M, z / RIPPLE_TILE_M]);
        uv1.push([wash, depth]);
        col.push(WAVES.map(|w| band_weight(spacing, w.len_m) * share));
        lo = lo.min(Vec2::new(x, z));
        hi = hi.max(Vec2::new(x, z));
        let n = (pos.len() - 1) as u32;
        id[slot] = n;
        n
    };
    for jz in 0..gz - 1 {
        for ix in 0..gx - 1 {
            if !wet(ix, jz) {
                continue;
            }
            let a = vertex(ix, jz);
            let b = vertex(ix + 1, jz);
            let c = vertex(ix, jz + 1);
            let d = vertex(ix + 1, jz + 1);
            // The ground mesh's split: upper (x offset ≥ z offset) a, b, d;
            // lower a, d, c. Up-facing.
            idx.extend_from_slice(&[a, d, b, a, c, d]);
        }
    }
    if pos.is_empty() {
        lo = Vec2::ZERO;
        hi = Vec2::ZERO;
    }
    let (up, down) = surface_bounds();
    let aabb = Aabb::from_min_max(Vec3::new(lo.x, sea_y - down, lo.y), Vec3::new(hi.x, sea_y + up, hi.y));
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tan)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv0)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uv1)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx));
    (mesh, aabb)
}

// ---------------------------------------------------------------------------
// The entity and its one system.
// ---------------------------------------------------------------------------

/// The sea's entity, its waves moved by [`animate`]. A `LevelPart`, so it
/// goes down with its map.
#[derive(Component, Default)]
pub struct WaterPart;

/// Spawns the sea over `terrain` at `sea_y` (the map's `light.sea`), under
/// the sun toward `sun` (the map's mood's).
pub fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<WaterMaterial>,
    images: &mut Assets<Image>,
    terrain: &TerrainDef,
    sea_y: f32,
    sun: Vec3,
) -> Entity {
    let (mesh, aabb) = mesh(terrain, sea_y);
    let ripples = images.add(ripple_map());
    let sky = images.add(black_cube());
    commands
        .spawn((
            crate::level::LevelPart,
            WaterPart,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(material(ripples, sky, sun))),
            Transform::IDENTITY,
            // The box the vertex stage can reach, and `calculate_bounds`
            // kept off it: the CPU mesh is flat.
            aabb,
            NoAutoAabb,
            // A translucent surface has no business in the shadow map.
            NotShadowCaster,
        ))
        .id()
}

/// Once a frame, where there is a sea: each wave's phase on by `ω·dt` (in
/// cycles, wrapped, never `k·x − ω·t` off a clock: f32 seconds times a
/// wavenumber loses its low bits within an hour) and the ripple layer's
/// drift wrapped to one tile, into the seas' motion texture, not the
/// material (`sea_motion.rs`).
pub fn animate(time: Res<Time>, mut motion: ResMut<super::sea_motion::SeaMotion>, seas: Query<(), With<WaterPart>>) {
    if seas.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    for (p, w) in motion.phase.iter_mut().zip(WAVES) {
        *p = (*p - omega(w.len_m) / core::f32::consts::TAU * dt).rem_euclid(1.0);
    }
    motion.ripples = (motion.ripples + Vec2::from_array(RIPPLE_DRIFT) * dt).rem_euclid(Vec2::ONE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    fn terrain(nx: u16, nz: u16, h: impl Fn(usize, usize) -> f32) -> TerrainDef {
        let h = &h;
        let heights: Vec<i16> = (0..nz as usize)
            .flat_map(|j| (0..nx as usize).map(move |i| (h(i, j) * HEIGHT_Q) as i16))
            .collect();
        TerrainDef {
            cell: 2.0,
            x0: -4.0,
            z0: -2.0,
            nx,
            nz,
            heights: Box::leak(heights.into_boxed_slice()),
        }
    }

    fn positions(m: &Mesh) -> Vec<[f32; 3]> {
        match m.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(v)) => v.clone(),
            _ => panic!("no positions"),
        }
    }

    fn uv1(m: &Mesh) -> Vec<[f32; 2]> {
        match m.attribute(Mesh::ATTRIBUTE_UV_1) {
            Some(VertexAttributeValues::Float32x2(v)) => v.clone(),
            _ => panic!("no uv_b"),
        }
    }

    fn at(m: &Mesh, x: f32, z: f32) -> usize {
        positions(m)
            .iter()
            .position(|p| (p[0] - x).abs() < 1e-3 && (p[2] - z).abs() < 1e-3)
            .unwrap_or_else(|| panic!("no vertex at {x}, {z}"))
    }

    #[test]
    fn a_wave_is_whole_at_a_quarter_wavelength_and_gone_at_a_half() {
        for w in WAVES {
            assert_eq!(
                band_weight(2.0, w.len_m),
                1.0,
                "the 2 m core carries the {} m wave",
                w.len_m
            );
            assert_eq!(band_weight(w.len_m * 0.25, w.len_m), 1.0);
            assert_eq!(band_weight(w.len_m * 0.5, w.len_m), 0.0);
            let mid = band_weight(w.len_m * 0.375, w.len_m);
            assert!((mid - 0.5).abs() < 1e-6, "{mid}");
            assert!(band_weight(w.len_m * 0.3, w.len_m) > band_weight(w.len_m * 0.45, w.len_m));
            let l = (w.dir[0] * w.dir[0] + w.dir[1] * w.dir[1]).sqrt();
            assert!((l - 1.0).abs() < 1e-3, "{:?} is not unit", w.dir);
        }
    }

    #[test]
    fn the_swell_shoals_to_nothing_at_the_waterline() {
        assert_eq!(shoal(0.0), 0.0);
        assert_eq!(shoal(-2.0), 0.0);
        assert_eq!(shoal(SHOAL_FULL_M), 1.0);
        assert_eq!(shoal(DEEP_M), 1.0);
        assert!((shoal(SHOAL_FULL_M * 0.5) - 0.5).abs() < 1e-6);
        assert!(shoal(1.0) < shoal(2.0));
    }

    #[test]
    fn the_wash_stands_off_the_waterline() {
        // Read at an undisplaced depth (noise 0.5): nothing on the line,
        // most at the peak, nothing past the outer edge.
        assert_eq!(shore_foam(0.0, 0.3, 0.5), 0.0);
        assert_eq!(shore_foam(FOAM_DEPTH_M, 0.3, 0.5), 0.0);
        let peak = shore_foam(FOAM_PEAK_M, 0.3, 0.5);
        assert!(peak > shore_foam(0.2, 0.3, 0.5) && peak > shore_foam(1.8, 0.3, 0.5));
        // A flat shore still gets its floor.
        assert!((shore_foam(FOAM_PEAK_M, 0.0, 0.5) / peak - FOAM_SLOPE_FLOOR).abs() < 1e-5);
        for (x, z) in [(0.0, 0.0), (17.3, -400.2), (1e4, 1e4)] {
            let n = foam_noise(x, z);
            assert!((0.0..=1.0).contains(&n));
        }
    }

    #[test]
    fn the_mesh_is_the_terrain_s_wet_cells_and_a_skirt() {
        // A step: 3 m under the sea for x < 2, 3 m over it from there.
        let t = terrain(6, 5, |i, _| if i < 3 { -3.0 } else { 3.0 });
        let (m, aabb) = mesh(&t, 0.0);
        let rings = skirt_gaps().len();
        let (gx, gz) = (6 + rings, 5 + 2 * rings);
        // Every grid vertex but the dry interior ones no wet cell and no
        // skirt cell touches: column i = 4, rows 1..=3.
        assert_eq!(positions(&m).len(), gx * gz - 3);
        let p = positions(&m);
        let d = uv1(&m);
        // The column under a vertex is the sea over the terrain's height.
        let wet = at(&m, -4.0, 0.0);
        assert_eq!(d[wet][1], 3.0);
        let dry = at(&m, 2.0, 0.0);
        assert_eq!(d[dry][1], -3.0);
        assert_eq!(p[wet][1], 0.0, "flat at the sea level");
        // The skirt is deep by fiat, with no wash, and reaches the far plane.
        let far_x = p.iter().map(|q| q[0]).fold(f32::MIN, f32::max);
        let far = at(&m, far_x, 0.0);
        assert_eq!(d[far], [0.0, DEEP_M]);
        assert!(far_x - 6.0 >= SKIRT_REACH_M);
        assert!(aabb.max().x >= 6.0 + SKIRT_REACH_M && aabb.min().z <= -2.0 - SKIRT_REACH_M);
        assert!(aabb.max().y > 1.0 && aabb.min().y < -0.5, "the swell's reach, {aabb:?}");
        // Dry cells draw nothing: cells ix 3, 4 of each terrain row are out.
        let cells = (gx - 1) * (gz - 1) - 2 * 4;
        assert_eq!(m.indices().map(|i| i.len()), Some(cells * 6));
        // Flooded everywhere, every vertex is in.
        let t = terrain(6, 5, |_, _| -2.0);
        let (m, _) = mesh(&t, 0.0);
        assert_eq!(positions(&m).len(), gx * gz);
        assert!(uv1(&m).iter().all(|d| d[1] == 2.0 || d[1] == DEEP_M));
    }

    #[test]
    fn the_ripple_map_carries_its_whole_mip_chain() {
        let img = ripple_map();
        assert_eq!(img.texture_descriptor.mip_level_count, 9);
        assert_eq!(img.data.as_ref().map(|d| d.len()), Some(ripple_bytes(RIPPLE_TEX)));
        assert_eq!(ripple_bytes(RIPPLE_TEX), 349_524);
    }
}

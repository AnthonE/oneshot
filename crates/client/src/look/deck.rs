//! The sky's clouds, drawn per pixel, on every target. After Gates'
//! `render/deck.rs` (MIT, NOTICE).
//!
//! **Why.** The deck was a composed cube (`sky.rs`): 256² texels a face on
//! the desktop, 128² in a browser. At 75° of view a texel is three to six
//! pixels across, so every cloud edge arrived as a bilinear smear, and where
//! the atmosphere draws, the cube's clouds were left to its compositing to
//! show at all (a reviewer saw the desktop's sky over Dry Fork as a flat
//! slate). One material draws it per pixel instead, [`CloudMaterial`], in
//! one of two ways the device decides (`sky::baked`, after `gpu.rs`):
//!
//!   · **Wherever the atmosphere draws** (the desktop, the browser's WebGPU
//!     module): one fullscreen triangle at depth 0 in the OPAQUE pass, where
//!     a `Skybox` would draw, so `AtmosphereNode::RenderSky` reads it as sky
//!     and composites it as `inscattering + T·cloud`: the clouds get the
//!     aerial perspective everything else gets, from the same owner. Its
//!     prepass writes the sky's motion vectors for TAA
//!     (`cloud_prepass.wgsl`). The weather's fog over the sky is a second
//!     layer, after the atmosphere ([`SkyFogMaterial`]).
//!   · **Under WebGL2**, which has no atmosphere: transparent, at alpha 1,
//!     the clear sky, the sun's disk, the clouds and the fog band composed
//!     together (Gates' `sky_deck.wgsl`, folded into `cloud.wgsl` as
//!     `BACKDROP`).
//!
//! **The same clouds on both, at every rung the device has.** The CLOUDS row
//! (`quality::Clouds`) is the flat deck at LOW and a kilometre-thick slab of
//! cumulus raymarched at MEDIUM and HIGH, on every target: Gates drew the
//! slab only where the atmosphere draws, but nothing in it wants compute (a
//! 3D texture and a loop), so WebGL2 marches it too, at its HIGH or by the
//! row (its default MEDIUM keeps the flat deck it drew, `quality.rs`). Only
//! the sky moves with the row; no cloud is ever in front of a fighter.
//!
//! **Written rarely.** The drift runs on in the shader from where it was
//! last taken (`mood::Anchor`), so the material is written once a map, when
//! the tier moves, and when the sky's clock is re-anchored, never a frame.
//! There is no `Skybox` and no cube: oneshot has no sea to reflect the sky.

use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::asset::{Asset, RenderAssetUsages};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::camera::Exposure;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{NotShadowCaster, NotShadowReceiver, SunDisk};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError, TextureDimension,
    TextureFormat,
};
use bevy::shader::ShaderRef;

use super::mood::{Anchor, Drift, Mood, MoodNow};
use super::quality::{Clouds, Tier};
use super::rig::EyeCam;
use super::sky::{self, ComposeParams, DeckLight};

/// The deck's shader, on every target (`BACKDROP` where no atmosphere draws).
pub const CLOUD_SHADER: &str = "shaders/cloud.wgsl";
/// Its prepass: the motion vectors a `Skybox` would write.
pub const CLOUD_PREPASS: &str = "shaders/cloud_prepass.wgsl";
/// The weather's fog over the sky, where the atmosphere draws.
pub const SKY_FOG_SHADER: &str = "shaders/sky_fog.wgsl";

/// The one cloud field every map's sky is read from, each at its own place
/// (`Mood::field_at`): built once, at boot.
pub const SKY_SEED: u64 = 0x0e5c_0dec_c10d;

/// How far finer octaves move the field (`deck.wgsl`'s `deck_detail`):
/// about a quarter of a cloud edge's width, enough to give the rim lumps.
pub const DETAIL: f32 = 0.06;

/// Draw order in the transparent pass, as `Material::depth_bias` (more
/// negative draws first): the sky, then the stars over it, then the moon,
/// added over them (the stars behind its disk are left out where they are
/// drawn, `stars.wgsl`), the rainbow, and the sky's fog over all of it.
pub const DECK_ORDER: f32 = -3.0e8;
pub const STARS_ORDER: f32 = -2.0e8;
pub const MOON_ORDER: f32 = -1.0e8;
pub const SKY_FOG_ORDER: f32 = -0.5e8;

/// How thick the raymarched slab is, metres, from `sky::CLOUD_ALT_M` up.
pub const SLAB_M: f32 = 1000.0;
/// Extinction per metre at density 1: thin enough that the edges read as
/// soft, while three hundred metres of core still stops the sun.
pub const CLOUD_SIGMA: f32 = 0.018;
/// One tile of the detail noise, metres. It divides the field's own period
/// (`FIELD_PERIOD × CLOUD_SCALE_M`, 28.8 km), so when the drift wraps the
/// detail riding on it lands where it was.
pub const DETAIL_TILE_M: f32 = 480.0;
/// The detail volume's texels a side: 7.5 m each in a release. A debug
/// build pays for every texel at boot unoptimized and takes an eighth.
pub const DETAIL_N: usize = if cfg!(debug_assertions) { 32 } else { 64 };
/// How fast a cumulus boils, m/s: the detail rising through the slab. An
/// hour of it (`globals.time` wraps hourly) is six tiles exactly.
pub const BOIL_MS: f32 = 0.8;
/// The sunlit term's weight in the slab's light over the sky term's: what
/// sets a lit top against its grey belly (Gates measured 2.4 against its
/// reference frames' tops).
pub const SLAB_SUN: f32 = 2.4;
/// The brightest the WebGL2 sun disk is drawn, linear after exposure: past
/// white, so it reads as the sun with no bloom to spread it.
pub const DISK_PEAK: f32 = 30.0;
/// How much wider than the disk its soft edge reaches.
pub const DISK_SOFT: f32 = 1.6;

const _: () = assert!(DETAIL_TILE_M * 6.0 == BOIL_MS * 3600.0);

/// Coarse view steps and light steps for a CLOUDS rung. `(0, 0)` is the
/// flat deck. A coarse step reads the coverage alone; where it finds cloud
/// the march walks it at a quarter of the step with the detail and the
/// light march's reads (`cloud.wgsl`), so most of the sky costs one read a
/// step and a cloud about four times its share.
pub fn steps(c: Clouds) -> (u32, u32) {
    match c {
        Clouds::Low => (0, 0),
        Clouds::Medium => (16, 3),
        Clouds::High => (32, 5),
    }
}

/// The raymarched slab as the stars and the moon read it for their cover
/// (`deck.wgsl`'s `slab_shape`): x its floor, y its ceiling, metres; z
/// metres per noise unit; w the extinction per metre. Zero where the deck is
/// drawn flat, so they read the flat deck there.
pub fn slab_lanes(steps: (u32, u32)) -> Vec4 {
    if steps.0 == 0 {
        return Vec4::ZERO;
    }
    Vec4::new(
        sky::CLOUD_ALT_M,
        sky::CLOUD_ALT_M + SLAB_M,
        sky::CLOUD_SCALE_M,
        CLOUD_SIGMA,
    )
}

/// The deck's uniform, laid out to match `Cloud` in `cloud.wgsl`, whose
/// comments say what each lane holds.
#[derive(Clone, Copy, Default, ShaderType, Debug, PartialEq)]
pub struct CloudParams {
    pub top: Vec4,
    pub base: Vec4,
    pub light: Vec4,
    pub sun: Vec4,
    pub moon: Vec4,
    pub halo: Vec4,
    pub scatter: Vec4,
    pub shape: Vec4,
    pub drift: Vec4,
    pub shade: Vec4,
    pub hue: Vec4,
    pub slab: Vec4,
    pub march: Vec4,
    pub texel: Vec4,
    pub wind: Vec4,
    pub lift: Vec4,
    pub flow: Vec4,
    pub zenith: Vec4,
    pub horizon: Vec4,
    pub night: Vec4,
    pub glow: Vec4,
    pub fog: Vec4,
    pub air: Vec4,
    pub dust: Vec4,
    pub disk: Vec4,
    pub disk_rgb: Vec4,
}

/// What the WebGL2 deck draws its sun disk with: toward the sun, and its
/// radiance in deck units. None where the atmosphere draws one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Disk {
    pub dir: Vec3,
    pub rgb: Vec3,
}

impl Disk {
    /// The disk of `mood`'s sun as seen at `ev100`: [`DISK_PEAK`] at full
    /// sun, its light's colour (`rig::sun_light`), in deck units.
    pub fn of(mood: &Mood, ev100: f32) -> Disk {
        let per_unit = sky::CLOUD_NITS * Exposure { ev100 }.exposure();
        let rgb = super::rig::sun_light(mood, false) * (DISK_PEAK * mood.sun / per_unit.max(1e-6));
        Disk {
            dir: mood.to_sun(),
            rgb,
        }
    }
}

impl CloudParams {
    /// The deck for a compose's params at `nits` cd/m² per deck unit,
    /// marched `steps` ([`steps`]), its grain averaged by TAA when
    /// `temporal`, its drift running from `at`; over its own clear sky
    /// (`p.backdrop`) with `disk` in it.
    pub fn new(p: &ComposeParams, nits: f32, steps: (u32, u32), temporal: bool, at: &Anchor, disk: Disk) -> Self {
        let l = DeckLight::new(p);
        let v3 = Vec3::from_array;
        // The slab is shaped by the sun, held a little above the horizon so
        // a light march leaves the slab inside its steps.
        let toward = if p.sun.y > -0.02 { p.sun } else { p.moon };
        let toward = Vec3::new(toward.x, toward.y.max(0.15), toward.z).normalize_or(Vec3::Y);
        let field_m = sky::CLOUD_SCALE_M * sky::FIELD_PERIOD as f32;
        // The backdrop's grade is linear in its colour, so grading the two
        // ends grades every blend of them.
        let grade = |b: [f32; 3]| -> Vec3 {
            let lum = super::fill::luminance(b);
            Vec3::from_array(core::array::from_fn(|c| {
                (b[c] + (lum - b[c]) * 0.7 * p.dark) * (1.0 - 0.4 * p.dark) * p.light
            }))
        };
        let r = SunDisk::EARTH.angular_size * 0.5;
        let (lift, zenith, horizon, night, glow, fog, disk_dir, disk_rgb) = if p.backdrop {
            (
                Vec3::ZERO,
                grade(sky::backdrop_at(Vec3::Y)).extend(sky::AIR_FLOOR),
                grade(sky::backdrop_at(Vec3::X)).extend((r * DISK_SOFT).cos()),
                (v3(sky::NIGHT_SKY) * p.night).extend(0.0),
                (v3(sky::DUSK_GLOW) * l.glow * (1.0 - 0.7 * p.dark)).extend(r.cos()),
                (v3(l.fog_rgb) * l.gain).extend(0.0),
                disk.dir,
                disk.rgb,
            )
        } else {
            // Daylight's, so it fades with the sun and under an overcast,
            // and is gone as the sun drops toward the horizon (where
            // `DeckLight` starts warming the deck), so a dusk sky is the
            // atmosphere's orange, not greyed by blue.
            let lift = Vec3::from_array(sky::air_chroma())
                * super::rig::SKY_LIFT
                * p.light
                * (p.sun.y / 0.25).clamp(0.0, 1.0)
                * (1.0 - p.dark);
            (
                lift,
                Vec4::ZERO,
                Vec4::ZERO,
                Vec4::ZERO,
                Vec4::ZERO,
                Vec4::ZERO,
                Vec3::ZERO,
                Vec3::ZERO,
            )
        };
        Self {
            top: v3(l.top).extend(l.thresh),
            base: v3(l.base).extend(sky::EDGE),
            light: toward.extend(SLAB_SUN),
            sun: Vec4::new(l.sun_h.x, l.sun_h.y, l.toward.x, l.toward.y),
            moon: p.moon.extend(sky::HALO_COS),
            halo: (v3(sky::HALO_RGB) * p.night * p.moonlight).extend(sky::MOON_SCATTER_COS),
            scatter: v3(l.scatter).extend(0.0),
            shape: Vec4::new(
                sky::CLOUD_ALT_M / sky::CLOUD_SCALE_M,
                sky::DECK_CUTOFF,
                sky::HORIZON_FADE,
                DETAIL,
            ),
            drift: Vec4::new(
                at.off.x,
                at.off.y,
                1.0 / sky::FIELD_PERIOD as f32,
                0.5 / sky::FIELD_N as f32,
            ),
            shade: Vec4::new(sky::CORE_SHADE, sky::RIM_GLOW, sky::SIDE_VIEW_Y, nits),
            hue: Vec3::from_array(sky::column_depth()).extend(if super::gpu::caps().dual_source_blending {
                1.0
            } else {
                0.0
            }),
            slab: Vec4::new(
                sky::CLOUD_ALT_M,
                sky::CLOUD_ALT_M + SLAB_M,
                sky::CLOUD_SCALE_M,
                DETAIL_TILE_M,
            ),
            march: Vec4::new(
                steps.0 as f32,
                steps.1 as f32,
                CLOUD_SIGMA,
                if temporal { 1.0 } else { 0.0 },
            ),
            texel: Vec4::new(field_m / sky::FIELD_N as f32, DETAIL_TILE_M / DETAIL_N as f32, 0.0, 0.0),
            wind: Vec4::new(BOIL_MS, 0.0, 0.0, 0.0),
            lift: lift.extend(0.0),
            flow: at.flow(),
            zenith,
            horizon,
            night,
            glow,
            fog,
            air: p.fog.lanes(),
            dust: p.fog.dust_lanes(),
            disk: disk_dir.extend(0.0),
            disk_rgb: disk_rgb.extend(0.0),
        }
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct CloudMaterial {
    #[uniform(0)]
    pub params: CloudParams,
    #[texture(1)]
    #[sampler(2)]
    pub field: Handle<Image>,
    #[texture(3, dimension = "3d")]
    #[sampler(4)]
    pub detail: Handle<Image>,
}

impl Material for CloudMaterial {
    fn vertex_shader() -> ShaderRef {
        CLOUD_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        CLOUD_SHADER.into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        CLOUD_PREPASS.into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        CLOUD_PREPASS.into()
    }

    /// Where the atmosphere draws: **masked, for the ORDER and not for any
    /// mask.** The opaque pass draws its opaque phase, then its alpha-mask
    /// phase; in the second the level is already in the depth buffer, so
    /// the march's fragments behind a wall fail the depth test before they
    /// run, with or without a depth prepass. The shader never discards.
    /// Under WebGL2: premultiplied at alpha 1, over the clear colour, in
    /// the transparent pass before the stars and the moon.
    fn alpha_mode(&self) -> AlphaMode {
        if sky::baked() {
            AlphaMode::Premultiplied
        } else {
            AlphaMode::Mask(0.5)
        }
    }

    fn depth_bias(&self) -> f32 {
        DECK_ORDER
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if sky::baked() {
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("BACKDROP".into());
            }
        }
        Ok(())
    }
}

fn hash3(seed: u64, x: i32, y: i32, z: i32) -> u64 {
    let mut h = seed
        ^ (x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (z as i64 as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    h
}

/// One octave of tiling cellular noise at `p` (cell units, `0..cells`):
/// one minus the distance to the nearest of one point per cell, so each
/// cell is a rounded lump — the cauliflower a cumulus is made of.
fn worley(seed: u64, p: Vec3, cells: i32) -> f32 {
    let c = p.floor();
    let mut best = f32::MAX;
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cx = c.x as i32 + dx;
                let cy = c.y as i32 + dy;
                let cz = c.z as i32 + dz;
                let h = hash3(seed, cx.rem_euclid(cells), cy.rem_euclid(cells), cz.rem_euclid(cells));
                let jitter = Vec3::new(
                    (h & 0xFFFF) as f32 / 65_536.0,
                    ((h >> 16) & 0xFFFF) as f32 / 65_536.0,
                    ((h >> 32) & 0xFFFF) as f32 / 65_536.0,
                );
                let f = Vec3::new(cx as f32, cy as f32, cz as f32) + jitter;
                best = best.min(f.distance_squared(p));
            }
        }
    }
    1.0 - best.sqrt().min(1.0)
}

/// The detail volume's values, `n³` of them x-fastest, spread over `0..=1`:
/// three octaves of [`worley`], each twice as fine, tiling on the cube.
pub fn detail_noise(seed: u64, n: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(n * n * n);
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let u = Vec3::new(x as f32, y as f32, z as f32) / n as f32;
                let v = 0.625 * worley(seed ^ 0x0c1d, u * 4.0, 4)
                    + 0.25 * worley(seed ^ 0x0c2d, u * 8.0, 8)
                    + 0.125 * worley(seed ^ 0x0c3d, u * 16.0, 16);
                out.push(v);
            }
        }
    }
    let lo = out.iter().copied().fold(f32::MAX, f32::min);
    let hi = out.iter().copied().fold(f32::MIN, f32::max);
    let span = (hi - lo).max(1e-6);
    for v in out.iter_mut() {
        *v = (*v - lo) / span;
    }
    out
}

/// [`detail_noise`] as a repeating, mipped 3D texture: a far step reads a
/// coarser level instead of aliasing.
pub fn detail_volume(seed: u64, n: usize) -> Image {
    let mut level: Vec<u8> = detail_noise(seed, n).iter().map(|v| (v * 255.0 + 0.5) as u8).collect();
    let mut data = level.clone();
    let mut size = n;
    let mut levels = 1;
    while size > 1 {
        let h = size / 2;
        let mut next = vec![0u8; h * h * h];
        for z in 0..h {
            for y in 0..h {
                for x in 0..h {
                    let mut sum = 0u32;
                    for (dx, dy, dz) in [
                        (0, 0, 0),
                        (1, 0, 0),
                        (0, 1, 0),
                        (1, 1, 0),
                        (0, 0, 1),
                        (1, 0, 1),
                        (0, 1, 1),
                        (1, 1, 1),
                    ] {
                        let (sx, sy, sz) = (2 * x + dx, 2 * y + dy, 2 * z + dz);
                        sum += level[(sz * size + sy) * size + sx] as u32;
                    }
                    next[(z * h + y) * h + x] = ((sum + 4) / 8) as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        level = next;
        size = h;
        levels += 1;
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width: n as u32,
            height: n as u32,
            depth_or_array_layers: n as u32,
        },
        TextureDimension::D3,
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// The sky fog's uniform, laid out to match `SkyFog` in `sky_fog.wgsl`.
#[derive(Clone, Copy, Default, ShaderType, Debug, PartialEq)]
pub struct SkyFogParams {
    /// xyz the ground fog's colour, exposed linear, as `DistanceFog` takes it.
    pub color: Vec4,
    /// `sky::SkyFog::lanes`.
    pub air: Vec4,
    /// xyz the ground fog's sun glow (its colour times its alpha); w its
    /// exponent.
    pub glow: Vec4,
    /// `sky::SkyFog::dust_lanes`.
    pub dust: Vec4,
}

impl SkyFogParams {
    /// The fog over a mood's sky, the colour and glow the rig gives the
    /// ground's (`rig::fog_for`), with `flat_mist` of its mist in the flat
    /// fog (the rest is the shafts' volume, which lies over the sky too).
    pub fn new(mood: &Mood, flat_mist: f32) -> Self {
        let f = super::rig::fog_for(mood, true, 0.0);
        let c = LinearRgba::from(f.color);
        let g = LinearRgba::from(f.directional_light_color);
        Self {
            color: Vec4::new(c.red, c.green, c.blue, 0.0),
            air: sky::SkyFog::new(flat_mist, mood.rain_sigma()).lanes(),
            glow: (Vec3::new(g.red, g.green, g.blue) * g.alpha).extend(sky::FOG_SUN_EXPONENT),
            dust: mood.sky_fog().dust_lanes(),
        }
    }
}

/// The weather's fog over the sky where the atmosphere draws
/// (`sky_fog.wgsl`). Under WebGL2 the deck folds it in, having no
/// atmosphere to sit in front of.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct SkyFogMaterial {
    #[uniform(0)]
    pub params: SkyFogParams,
}

impl Material for SkyFogMaterial {
    fn vertex_shader() -> ShaderRef {
        SKY_FOG_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SKY_FOG_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn depth_bias(&self) -> f32 {
        SKY_FOG_ORDER
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// The deck's field, built once: the image every sky shader reads it from,
/// the detail volume the slab is eroded by, and the field at the shadows'
/// softness, which the rig dims the sun by and the ground's shadows read
/// (`cloud_shadow.rs`), the same bytes on the CPU and the GPU.
#[derive(Resource)]
pub struct SkyField {
    pub image: Handle<Image>,
    pub detail: Handle<Image>,
    pub soft: sky::SoftField,
}

impl SkyField {
    /// How much of the sky toward `dir` a deck of `cover` at field offset
    /// `drift` (noise units) covers, `0..=1`, read as the ground's shadows
    /// read it (Gates' `SkyComposer::cover_at`).
    pub fn cover_at(&self, dir: Vec3, cover: f32, drift: Vec2) -> f32 {
        if dir.y <= sky::DECK_CUTOFF {
            return 0.0;
        }
        let t = sky::CLOUD_ALT_M / dir.y;
        let q = Vec2::new(dir.x * t, dir.z * t) / sky::CLOUD_SCALE_M + drift;
        let fade = ((dir.y - sky::DECK_CUTOFF) / sky::HORIZON_FADE).clamp(0.0, 1.0);
        let f = self.soft.at(q);
        ((f - (1.0 - cover)) / sky::EDGE).clamp(0.0, 1.0) * fade
    }
}

/// The deck's material and its fog's, which [`drive`] writes.
#[derive(Resource)]
pub struct DeckHandles {
    pub cloud: Handle<CloudMaterial>,
    pub fog: Option<Handle<SkyFogMaterial>>,
}

/// One triangle over the whole screen, in clip space.
pub fn fullscreen_mesh() -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[-1.0f32, -1.0, 0.0], [3.0, -1.0, 0.0], [-1.0, 3.0, 0.0]],
    );
    mesh.insert_indices(Indices::U16(vec![0, 1, 2]));
    mesh
}

/// The fullscreen triangle's entity, whichever layer of the sky it draws.
#[derive(Component)]
pub struct SkyLayer;

/// What every sky layer is spawned with.
pub fn layer() -> impl Bundle {
    (
        SkyLayer,
        Transform::default(),
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    )
}

/// Build the field and the deck, once, at boot (after `gpu::GpuPlugin`
/// has read the device, which decides how the deck is drawn).
pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut clouds: ResMut<Assets<CloudMaterial>>,
    mut fogs: ResMut<Assets<SkyFogMaterial>>,
) {
    let field = sky::CloudField::new(SKY_SEED, sky::FIELD_N);
    let image = images.add(field.image());
    let detail = images.add(detail_volume(SKY_SEED, DETAIL_N));
    let soft = field.soft(sky::soft_level());
    let triangle = meshes.add(fullscreen_mesh());
    let mood = super::mood::NOON;
    let at = Anchor::default();
    let cloud = clouds.add(CloudMaterial {
        params: CloudParams::new(
            &mood.compose(at.off, sky::baked()),
            sky::CLOUD_NITS,
            steps(Clouds::Low),
            false,
            &at,
            Disk::of(&mood, mood.ev100),
        ),
        field: image.clone(),
        detail: detail.clone(),
    });
    commands.spawn((layer(), Mesh3d(triangle.clone()), MeshMaterial3d(cloud.clone())));
    let fog = (!sky::baked()).then(|| {
        let fog = fogs.add(SkyFogMaterial {
            params: SkyFogParams::new(&mood, mood.mist),
        });
        commands.spawn((layer(), Mesh3d(triangle), MeshMaterial3d(fog.clone())));
        fog
    });
    commands.insert_resource(DeckHandles { cloud, fog });
    commands.insert_resource(SkyField { image, detail, soft });
}

/// The deck for this map's mood, this tier and this drift. Worked out each
/// frame (a few hundred flops), written only when it differs.
#[allow(clippy::too_many_arguments)]
pub fn drive(
    mood: Res<MoodNow>,
    drift: Res<Drift>,
    tier: Res<Tier>,
    shafts: Res<super::shafts::Shafts>,
    deck: Option<Res<DeckHandles>>,
    cam: Query<(Has<TemporalAntiAliasing>, &Exposure), With<EyeCam>>,
    mut clouds: ResMut<Assets<CloudMaterial>>,
    mut fogs: ResMut<Assets<SkyFogMaterial>>,
) {
    let Some(deck) = deck else {
        return;
    };
    let m = &mood.mood;
    let (temporal, ev) = cam.single().map_or((false, m.ev100), |(t, e)| (t, e.ev100));
    let at = drift.anchor;
    let want = CloudParams::new(
        &m.compose(at.off, sky::baked()),
        sky::CLOUD_NITS,
        tier.cloud_steps(),
        temporal,
        &at,
        Disk::of(m, ev),
    );
    if clouds.get(&deck.cloud).is_some_and(|c| c.params != want) {
        if let Some(c) = clouds.get_mut(&deck.cloud) {
            c.params = want;
        }
    }
    if let Some(fog) = &deck.fog {
        let want = SkyFogParams::new(m, (m.mist - shafts.volume).max(0.0));
        if fogs.get(fog).is_some_and(|f| f.params != want) {
            if let Some(f) = fogs.get_mut(fog) {
                f.params = want;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The detail tiles on its cube (so the slab's erosion has no seam where
    /// the drift wraps) and spans `0..=1`.
    #[test]
    fn the_detail_tiles_and_spans_the_range() {
        let n = 8;
        let v = detail_noise(7, n);
        assert_eq!(v.len(), n * n * n);
        let (lo, hi) = v.iter().fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(*x), b.max(*x)));
        assert!(lo.abs() < 1e-6 && (hi - 1.0).abs() < 1e-6);
        // Cellular noise at a point and a whole period over agree.
        let p = Vec3::new(0.3, 1.7, 2.2);
        assert!((worley(3, p, 4) - worley(3, p + Vec3::splat(4.0), 4)).abs() < 1e-6);
    }

    /// The uniform holds every lane `cloud.wgsl` declares, in order: 26
    /// four-float lanes.
    #[test]
    fn the_deck_uniform_matches_the_shader() {
        assert_eq!(CloudParams::min_size().get(), 26 * 16);
        let src = include_str!("../../assets/shaders/cloud.wgsl");
        let body = &src[src.find("struct Cloud {").unwrap()..];
        let body = &body[..body.find('}').unwrap()];
        assert_eq!(body.matches(": vec4<f32>,").count(), 26);
    }

    /// The flat deck at LOW, the slab above it, and the stars and the moon
    /// read whichever the sky draws.
    #[test]
    fn the_slab_is_marched_over_the_flat_rung() {
        assert_eq!(steps(Clouds::Low), (0, 0));
        assert_eq!(slab_lanes(steps(Clouds::Low)), Vec4::ZERO);
        for c in [Clouds::Medium, Clouds::High] {
            assert!(steps(c).0 > 0 && slab_lanes(steps(c)).w > 0.0);
        }
        assert!(steps(Clouds::High).0 > steps(Clouds::Medium).0);
    }
}

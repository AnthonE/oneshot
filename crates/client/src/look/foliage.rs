//! Ground cover that moves: wind, trails, a blast's shock and a distance
//! fade for the grass (`grass.rs`) and the ground clutter (`clutter.rs`).
//! After Gates' `crates/client/src/render/foliage.rs` and
//! `assets/shaders/foliage*.wgsl` (MIT, NOTICE), which rebuild Rust's
//! (Facepunch's) foliage shader: everything green sways on one wind with a
//! fast flutter on top, grass fades out smoothly with distance, cards seen
//! edge-on are hidden, leaves pass light from behind, and whoever walks
//! through the grass leaves a trail of it flattened.
//!
//! - **One material type, [`FoliageMaterial`]:** `StandardMaterial` with
//!   `foliage.wgsl`'s vertex stage (and `foliage_prepass.wgsl`'s twin, so
//!   the depth and motion prepass move the same vertex the same way).
//! - **Per-material shape, global state.** A material's uniform is its
//!   stiffness and fade ([`Kind::params`]) and never changes: Bevy
//!   re-extracts every mesh wearing a material that changed. The wind, the
//!   clock, the trails, a blast and the cover's distance live in two
//!   textures written straight into GPU memory each frame
//!   ([`write_textures`]), which no material sees change.
//! - **Baked, world-space.** A grass block's cards and a clutter block's
//!   pieces carry their height above their root and a random of their own
//!   in `UV_1`; a negative height is a rigid piece (a stone, a fallen twig)
//!   that never sways and only sinks with the fade.
//!
//! **Every target draws it alike.** No compute and no storage buffer: two
//! textures (a 4×1 `Rgba32Float` read with `textureLoad`, an `R8Unorm`
//! trail map sampled in the vertex stage) beside `StandardMaterial`'s six,
//! which is the ground's own count (`gpu::FRAGMENT_TEXTURES`), so WebGL2's
//! 16 a draw holds it and WebGPU and the desktop draw the same frame.
//!
//! **The client draws, never decides; and nothing here hides anyone.** The
//! trails come from where this client draws the fighters and the hulls
//! (`Walkers`, from the session's own snapshots), the wind is the map's
//! mood, and the blast is one the fog showed (`fx::blast`). Grass and
//! clutter stand lower than a crouching fighter's hit box
//! (`grass::TUFT_MAX_H`, `clutter::CLUTTER_MAX_H`), so a rung that draws
//! less of them (the GRASS rows) uncovers no one; a trail is shorter-lived
//! than Rust's, a lead on someone no rung should hold for long.

use std::sync::Arc;

use bevy::asset::{Asset, RenderAssetUsages};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, StandardMaterial};
use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, ShaderType, TexelCopyBufferLayout, TextureDimension, TextureFormat,
};
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::shader::ShaderRef;

use super::mood::MoodNow;
use super::quality::Tier;
use super::rig::EyeCam;

/// Grass and clutter: `StandardMaterial` plus the sway.
pub type FoliageMaterial = ExtendedMaterial<StandardMaterial, Foliage>;

/// The main-pass shader, resolved against the asset root.
pub const SHADER: &str = "shaders/foliage.wgsl";
/// Its prepass twin (depth, normals, motion).
pub const PREPASS_SHADER: &str = "shaders/foliage_prepass.wgsl";

/// Where the cover starts to thin out, as a share of its drawn distance
/// (the GRASS DISTANCE row, `quality::Gfx::cover_m`). **(knob)**
///
/// Each piece sinks into the ground at its own distance between this and
/// [`FADE_END`], so the turf thins rather than ending on a line; Gates'
/// 30 m of 46.
pub const FADE_START: f32 = 0.65;
/// Where the last piece has sunk: the drawn distance itself, inside the
/// nearest edge of a block `grass::cull` stops drawing, so a block is never
/// seen going.
pub const FADE_END: f32 = 1.0;

/// Texels per side of the trail map.
pub const TRAMPLE_TEXELS: usize = 128;
/// Metres one trail texel covers: a footprint is a few of them.
pub const TRAMPLE_TEXEL_M: f32 = 0.25;
/// The trail window's edge, metres, centred on the eye.
pub const TRAMPLE_M: f32 = TRAMPLE_TEXELS as f32 * TRAMPLE_TEXEL_M;
/// How long a trail takes to spring back, seconds. **(knob)** Rust's is 60;
/// in an arena a trail is a lead on whoever made it, and the rungs without
/// grass have none, so it fades in a third of that.
pub const TRAIL_S: f32 = 20.0;
/// A fighter's footprint radius on the trail map, metres.
pub const STAMP_R_M: f32 = 0.5;
/// A hull's wheel, the same way.
pub const WHEEL_R_M: f32 = 0.45;
/// How fast a walker flattens what they stand on, full per second: walking
/// at 5 m/s over a 1 m footprint presses ~0.5, so a second pass presses
/// further.
pub const STAMP_RATE: f32 = 2.5;

/// The wind's floor: grass never stands perfectly still.
const WIND_FLOOR: f32 = 0.25;
/// The mood's wind (`Mood::wind_ms`) at which the grass bends its most.
const WIND_FULL_MS: f32 = 12.0;
/// Seconds the shader's wind strength takes to follow the map's.
const WIND_EASE_S: f32 = 4.0;
/// The shader clock wraps here, seconds: one small jump every ~68 minutes
/// instead of sines losing precision as `f32` time grows.
const CLOCK_WRAP_S: f64 = 4096.0;

/// How hard a blast's shock bends what it passes, in units of the plant's
/// own wind bend at full wind (`foliage_common.wgsl::displace`).
pub const BLAST_PUSH: f32 = 4.0;
/// How long after a blast its shock is still worth uploading, seconds.
pub const BLAST_SHOCK_S: f32 = 2.5;
/// How far a blast can be from the eye and still be uploaded, metres: past
/// the shock's own reach (45 m) plus the cover's farthest row.
pub const BLAST_UPLOAD_M: f32 = 140.0;

/// The per-material uniform. See `foliage_common.wgsl` for each lane.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct FoliageParams {
    pub sway: Vec4,
    pub fade: Vec4,
    pub misc: Vec4,
}

/// The extension: the uniform, the state texels and the trail map.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct Foliage {
    #[uniform(100)]
    pub params: FoliageParams,
    #[texture(101, sample_type = "float", filterable = false)]
    pub state: Handle<Image>,
    #[texture(102)]
    #[sampler(103)]
    pub trample: Handle<Image>,
}

impl MaterialExtension for Foliage {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    // The same displacement in the prepass, or a card's depth lands where
    // the main pass does not draw it.
    fn prepass_vertex_shader() -> ShaderRef {
        PREPASS_SHADER.into()
    }
    fn prepass_fragment_shader() -> ShaderRef {
        PREPASS_SHADER.into()
    }
    /// Forward only: this client never runs deferred.
    fn deferred_vertex_shader() -> ShaderRef {
        ShaderRef::Default
    }
    fn deferred_fragment_shader() -> ShaderRef {
        ShaderRef::Default
    }
}

/// What a material dresses: its stiffness, its fade and how much light it
/// lets through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A grass block's cards (`grass.rs`): baked, faded, trampled, cut
    /// edge-on.
    Grass,
    /// A clutter block (`clutter.rs`): its dry plants sway stiffly and lie
    /// down where walked; its stones and twigs only sink with the fade.
    /// Solids, not cards: never cut edge-on.
    Litter,
}

/// A dry plant's sway: amplitude at the reference height, the reference
/// height, the angular frequency. Dead wood is stiff and quick.
const LITTER_SWAY: [f32; 3] = [0.035, super::clutter::DRY_PLANT_H, 2.2];

impl Kind {
    /// The uniform for this kind. Static: see the module doc for why it
    /// must never change at runtime.
    pub fn params(self) -> FoliageParams {
        let (sway, fade, misc) = match self {
            Kind::Grass => (
                Vec4::new(0.06, super::grass::TUFT_H, 2.6, 0.012),
                // Half a leaf's edge-on cut: you look DOWN at grass, and
                // every upright card is near edge-on from above.
                Vec4::new(FADE_START, FADE_END, 0.5, 0.0),
                Vec4::new(1.0, 0.0, 0.0, 0.0),
            ),
            Kind::Litter => {
                let [a, h, w] = LITTER_SWAY;
                (
                    Vec4::new(a, h, w, 0.004),
                    Vec4::new(FADE_START, FADE_END, 0.0, 0.0),
                    Vec4::new(1.0, 0.0, 0.0, 0.0),
                )
            }
        };
        FoliageParams { sway, fade, misc }
    }

    /// Light let through from behind (`StandardMaterial::diffuse_transmission`).
    /// Bevy spends it from DIRECT light only, Rust's own fix ("disabled
    /// translucency coming from indirect lighting").
    pub fn transmission(self) -> f32 {
        match self {
            Kind::Grass => 0.3,
            Kind::Litter => 0.0,
        }
    }
}

/// The foliage textures, and the two materials once made.
#[derive(Resource)]
pub struct Foliages {
    pub state: Handle<Image>,
    pub trample: Handle<Image>,
    /// The grass cards' material, once the atlas is mipped (`grass::dress`).
    pub grass: Option<Handle<FoliageMaterial>>,
    /// The clutter's, made with the first clutter block (`clutter::dress`).
    pub litter: Option<Handle<FoliageMaterial>>,
}

impl Foliages {
    /// The foliage twin of a `StandardMaterial`.
    ///
    /// **Transmission is paid for, not subtracted.** Bevy's diffuse
    /// transmission is energy-conserving (it takes `t` out of the lit side
    /// to give it to the back), so a card at `t` = 0.3 would draw 30% darker
    /// face-on than before. Dividing the base colour by `1 − t` gives the lit
    /// side back exactly and leaves the back lit at `t / (1 − t)` of it.
    pub fn make(&self, mut base: StandardMaterial, kind: Kind) -> FoliageMaterial {
        let t = kind.transmission();
        if t > 0.0 {
            let c = base.base_color.to_linear();
            let k = 1.0 / (1.0 - t);
            base.base_color = Color::LinearRgba(LinearRgba::new(c.red * k, c.green * k, c.blue * k, c.alpha));
            base.diffuse_transmission = t;
        }
        FoliageMaterial {
            base,
            extension: Foliage {
                params: kind.params(),
                state: self.state.clone(),
                trample: self.trample.clone(),
            },
        }
    }
}

/// The CPU half of the trail map: one float per texel, world-anchored and
/// wrapping, so the window follows the eye without copying a texel.
#[derive(Resource)]
pub struct Trample {
    px: Vec<f32>,
    /// The world column (row) each texel column (row) currently holds: a
    /// slot whose world cell moved out of the window is cleared before reuse.
    cols: Vec<i64>,
    rows: Vec<i64>,
    /// The window's minimum world cell.
    origin: IVec2,
    /// The shader's wind strength, eased toward the map's.
    wind: f32,
}

impl Default for Trample {
    fn default() -> Self {
        Self {
            px: vec![0.0; TRAMPLE_TEXELS * TRAMPLE_TEXELS],
            cols: vec![i64::MIN; TRAMPLE_TEXELS],
            rows: vec![i64::MIN; TRAMPLE_TEXELS],
            origin: IVec2::ZERO,
            wind: WIND_FLOOR,
        }
    }
}

impl Trample {
    /// Move the window to be centred on `centre` (world XZ), clearing every
    /// texel column and row whose world cell changed.
    pub fn recentre(&mut self, centre: Vec2) {
        let n = TRAMPLE_TEXELS as i64;
        let c = (centre / TRAMPLE_TEXEL_M).floor().as_ivec2();
        self.origin = c - IVec2::splat(TRAMPLE_TEXELS as i32 / 2);
        let (ox, oz) = (self.origin.x as i64, self.origin.y as i64);
        for i in 0..TRAMPLE_TEXELS {
            let want = ox + (i as i64 - ox).rem_euclid(n);
            if self.cols[i] != want {
                self.cols[i] = want;
                for r in 0..TRAMPLE_TEXELS {
                    self.px[r * TRAMPLE_TEXELS + i] = 0.0;
                }
            }
            let want = oz + (i as i64 - oz).rem_euclid(n);
            if self.rows[i] != want {
                self.rows[i] = want;
                self.px[i * TRAMPLE_TEXELS..(i + 1) * TRAMPLE_TEXELS].fill(0.0);
            }
        }
    }

    /// Spring every trail back by `dt` seconds' worth.
    pub fn decay(&mut self, dt: f32) {
        let d = dt / TRAIL_S;
        for v in &mut self.px {
            *v = (*v - d).max(0.0);
        }
    }

    /// Press the cover under a footprint of radius `r` at `at` (world XZ)
    /// for `dt`.
    pub fn stamp(&mut self, at: Vec2, r: f32, dt: f32) {
        let reach = (r / TRAMPLE_TEXEL_M).ceil() as i32;
        let c = (at / TRAMPLE_TEXEL_M).floor().as_ivec2();
        let n = TRAMPLE_TEXELS as i32;
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                let cell = c + IVec2::new(dx, dz);
                let rel = cell - self.origin;
                if rel.x < 0 || rel.y < 0 || rel.x >= n || rel.y >= n {
                    continue;
                }
                let mid = (cell.as_vec2() + 0.5) * TRAMPLE_TEXEL_M;
                let fall = 1.0 - (mid - at).length() / r.max(1e-3);
                if fall <= 0.0 {
                    continue;
                }
                let i = cell.y.rem_euclid(n) as usize * TRAMPLE_TEXELS + cell.x.rem_euclid(n) as usize;
                self.px[i] = (self.px[i] + fall.min(1.0) * STAMP_RATE * dt).min(1.0);
            }
        }
    }

    /// How flat the cover is at a world point, 0..1: what the shader samples.
    pub fn at(&self, p: Vec2) -> f32 {
        let c = (p / TRAMPLE_TEXEL_M).floor().as_ivec2();
        let rel = c - self.origin;
        let n = TRAMPLE_TEXELS as i32;
        if rel.x < 0 || rel.y < 0 || rel.x >= n || rel.y >= n {
            return 0.0;
        }
        self.px[c.y.rem_euclid(n) as usize * TRAMPLE_TEXELS + c.x.rem_euclid(n) as usize]
    }

    /// The window's centre, world XZ: `state` texel 1, the shader's edge.
    pub fn centre(&self) -> Vec2 {
        (self.origin.as_vec2() + TRAMPLE_TEXELS as f32 * 0.5) * TRAMPLE_TEXEL_M
    }
}

/// What presses the cover this frame: each footprint's world XZ and
/// radius. Written by the client's draw systems from what they draw (our
/// own feet, the fighters and the hulls the session shows), read by
/// [`update`]. Drawing only.
#[derive(Resource, Default)]
pub struct Walkers {
    pub feet: Vec<(Vec2, f32)>,
}

impl Walkers {
    /// A fighter standing on the ground at `p`.
    pub fn fighter(&mut self, p: Vec3) {
        self.feet.push((p.xz(), STAMP_R_M));
    }

    /// A hull at `p` turned by `yaw`: its four wheels, at `wheels` in its
    /// own frame (+X forward).
    pub fn hull(&mut self, p: Vec3, yaw: f32, wheels: &[(f32, f32)]) {
        let turn = Quat::from_rotation_y(-yaw);
        for &(x, z) in wheels {
            let w = p + turn * Vec3::new(x, 0.0, z);
            self.feet.push((w.xz(), WHEEL_R_M));
        }
    }
}

/// This frame's texels, handed to the render world.
#[derive(Resource, Clone, ExtractResource)]
pub struct FoliageUpload {
    pub state: Handle<Image>,
    pub trample: Handle<Image>,
    pub state_px: [f32; STATE_FLOATS],
    pub trample_px: Arc<Vec<u8>>,
}

/// Texels in the per-frame state texture (`foliage_common.wgsl`): the
/// wind, the trail window, a blast, the cover's distance.
pub const STATE_TEXELS: usize = 4;
const STATE_FLOATS: usize = STATE_TEXELS * 4;

/// Register the material, the textures and the GPU writer.
pub fn plugin(app: &mut App) {
    app.add_plugins(MaterialPlugin::<FoliageMaterial>::default());
    app.add_plugins(ExtractResourcePlugin::<FoliageUpload>::default());
    app.init_resource::<Trample>();
    app.init_resource::<Walkers>();
    app.add_systems(PreStartup, init);
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app.add_systems(Render, write_textures.in_set(RenderSystems::PrepareResources));
    }
}

/// The two textures, at boot.
pub fn init(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let state = images.add(Image::new_fill(
        Extent3d {
            width: STATE_TEXELS as u32,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0u8; 16],
        TextureFormat::Rgba32Float,
        RenderAssetUsages::RENDER_WORLD,
    ));
    let mut trample = Image::new_fill(
        Extent3d {
            width: TRAMPLE_TEXELS as u32,
            height: TRAMPLE_TEXELS as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0u8],
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Wrapping, so world cell `c` lives at texel `c mod N` (`Trample`).
    trample.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    let trample = images.add(trample);
    commands.insert_resource(FoliageUpload {
        state: state.clone(),
        trample: trample.clone(),
        state_px: [0.0; STATE_FLOATS],
        trample_px: Arc::new(vec![0; TRAMPLE_TEXELS * TRAMPLE_TEXELS]),
    });
    commands.insert_resource(Foliages {
        state,
        trample,
        grass: None,
        litter: None,
    });
}

/// The shader's wind strength for a mood's wind, before easing: a floor so
/// nothing stands still, full at [`WIND_FULL_MS`]; still under a roof.
pub fn wind_goal(wind_ms: f32, roofed: bool) -> f32 {
    if roofed {
        return WIND_FLOOR;
    }
    WIND_FLOOR + (1.0 - WIND_FLOOR) * (wind_ms / WIND_FULL_MS).clamp(0.0, 1.0)
}

/// This frame's wind, clock, trails, blast and distance, into
/// [`FoliageUpload`]. After the fighters and the hulls are drawn, so
/// [`Walkers`] is this frame's.
#[allow(clippy::too_many_arguments)]
pub fn update(
    time: Res<Time>,
    mood: Res<MoodNow>,
    tier: Res<Tier>,
    walkers: Res<Walkers>,
    eye: Query<&GlobalTransform, With<EyeCam>>,
    fx: Option<ResMut<super::fx::Fx>>,
    mut trample: ResMut<Trample>,
    mut up: ResMut<FoliageUpload>,
    mut blast: Local<Option<(Vec2, f64)>>,
) {
    let dt = time.delta_secs().min(0.25);
    let m = &mood.mood;
    let dir = m.wind();
    let goal = wind_goal(m.wind_ms, mood.roofed);
    trample.wind += (goal - trample.wind) * (dt / WIND_EASE_S).min(1.0);

    let at = eye.single().map_or(Vec3::ZERO, |g| g.translation());
    let centre = at.xz();
    trample.recentre(centre);
    trample.decay(dt);
    let reach = TRAMPLE_M * 0.5;
    for &(p, r) in &walkers.feet {
        if (p - centre).abs().max_element() < reach + r {
            trample.stamp(p, r, dt);
        }
    }

    // The most recent blast in reach, and how long ago.
    let now = time.elapsed_secs_f64();
    if let Some(mut fx) = fx {
        if let Some(b) = fx.shock.take() {
            if b.distance(at) < BLAST_UPLOAD_M {
                *blast = Some((b.xz(), now));
            }
        }
    }
    let shock = match *blast {
        Some((p, t0)) if now - t0 < BLAST_SHOCK_S as f64 => [p.x, p.y, BLAST_PUSH, (now - t0) as f32],
        _ => [0.0; 4],
    };

    let clock = (now % CLOCK_WRAP_S) as f32;
    let window = trample.centre();
    let state = [
        [dir.x, dir.y, trample.wind, clock],
        [window.x, window.y, TRAMPLE_M, time.delta_secs()],
        shock,
        [tier.gfx.cover_m, 0.0, 0.0, 0.0],
    ];
    for (texel, v) in state.iter().enumerate() {
        up.state_px[texel * 4..texel * 4 + 4].copy_from_slice(v);
    }
    let px = Arc::make_mut(&mut up.trample_px);
    for (dst, v) in px.iter_mut().zip(&trample.px) {
        *dst = (v * 255.0 + 0.5) as u8;
    }
}

/// Write this frame's texels into the textures the materials already bind.
///
/// **In place, on purpose.** Modifying the `Image` assets would make Bevy
/// build fresh GPU textures, and every foliage bind group would go on
/// pointing at the old ones; a write into the same texture is invisible to
/// everything but the shader.
pub fn write_textures(up: Option<Res<FoliageUpload>>, images: Res<RenderAssets<GpuImage>>, queue: Res<RenderQueue>) {
    let Some(up) = up else { return };
    if let Some(img) = images.get(&up.state) {
        let mut bytes = [0u8; STATE_FLOATS * 4];
        for (i, f) in up.state_px.iter().enumerate() {
            bytes[i * 4..i * 4 + 4].copy_from_slice(&f.to_le_bytes());
        }
        queue.write_texture(
            img.texture.as_image_copy(),
            &bytes,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((STATE_FLOATS * 4) as u32),
                rows_per_image: None,
            },
            Extent3d {
                width: STATE_TEXELS as u32,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
    }
    if let Some(img) = images.get(&up.trample) {
        queue.write_texture(
            img.texture.as_image_copy(),
            &up.trample_px,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TRAMPLE_TEXELS as u32),
                rows_per_image: None,
            },
            Extent3d {
                width: TRAMPLE_TEXELS as u32,
                height: TRAMPLE_TEXELS as u32,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// Where a warm draw of the cover stands: far under every map, the
/// warm fighter's place (`prewarm.rs`).
const WARM_UNDER_M: f32 = -500.0;

/// What a foliage pipeline is specialised on: its material and its mesh's
/// layout.
type WarmKey = (AssetId<FoliageMaterial>, bool, bool, bool);

/// A warm draw of each foliage pipeline the frame the first block wearing
/// it is dressed (after `prewarm.rs`): a block is hidden until the eye
/// comes near it (`grass::cull`), and its pipelines would otherwise be
/// built mid-fight. On the block's own mesh, at full size far under the
/// map and never culled, so it is queued (main pass and prepasses) where
/// nobody sees it; that far out, the fade has sunk every piece anyway.
pub fn warm(
    mut commands: Commands,
    mut seen: Local<Vec<WarmKey>>,
    meshes: Res<Assets<Mesh>>,
    fresh: Query<(&Mesh3d, &MeshMaterial3d<FoliageMaterial>), Added<MeshMaterial3d<FoliageMaterial>>>,
) {
    for (mesh, mat) in &fresh {
        let Some(me) = meshes.get(&mesh.0) else {
            continue;
        };
        // A RENDER_WORLD mesh already sent to the GPU can't be read here
        // (`attribute` panics): its pipeline is built when it draws.
        let has = |id| me.try_attribute_option(id).map(|a| a.is_some());
        let (Ok(uv0), Ok(uv1), Ok(color)) = (
            has(Mesh::ATTRIBUTE_UV_0.id),
            has(Mesh::ATTRIBUTE_UV_1.id),
            has(Mesh::ATTRIBUTE_COLOR.id),
        ) else {
            continue;
        };
        let key = (mat.0.id(), uv0, uv1, color);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        commands.spawn((
            super::prewarm::Warming(super::prewarm::WARM_FRAMES),
            super::prewarm::SceneDone,
            Mesh3d(mesh.0.clone()),
            MeshMaterial3d(mat.0.clone()),
            Transform::from_xyz(0.0, WARM_UNDER_M, 0.0),
            NoFrustumCulling,
            NotShadowCaster,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trail_is_pressed_where_walked_and_springs_back() {
        let mut t = Trample::default();
        t.recentre(Vec2::ZERO);
        let p = Vec2::new(3.1, -2.2);
        for _ in 0..30 {
            t.stamp(p, STAMP_R_M, 1.0 / 60.0);
        }
        let pressed = t.at(p);
        assert!(pressed > 0.5, "half a second on a spot presses it: {pressed}");
        assert_eq!(t.at(p + Vec2::new(2.0, 0.0)), 0.0, "and nothing a stride away");
        t.decay(TRAIL_S * 0.5);
        assert!(
            t.at(p) < pressed && t.at(p) > 0.0,
            "half the spring later, still there, less"
        );
        t.decay(TRAIL_S);
        assert_eq!(t.at(p), 0.0, "a whole spring later, gone");
    }

    #[test]
    fn the_window_forgets_what_it_leaves() {
        let mut t = Trample::default();
        t.recentre(Vec2::ZERO);
        let p = Vec2::new(-10.0, 4.0);
        t.stamp(p, STAMP_R_M, 1.0);
        assert!(t.at(p) > 0.0);
        // The window walks away and back: the trail's texels were reused.
        t.recentre(Vec2::new(TRAMPLE_M, 0.0));
        t.recentre(Vec2::ZERO);
        assert_eq!(t.at(p), 0.0);
        // A trail the window keeps stays put as it moves a little.
        t.stamp(p, STAMP_R_M, 1.0);
        let had = t.at(p);
        t.recentre(Vec2::new(3.0, -2.0));
        assert_eq!(t.at(p), had);
        assert!(t.centre().distance(Vec2::new(3.0, -2.0)) <= TRAMPLE_TEXEL_M);
    }

    #[test]
    fn a_hull_presses_its_four_wheels() {
        let mut w = Walkers::default();
        let wheels = [(1.5, 1.05), (1.5, -1.05), (-1.5, 1.05), (-1.5, -1.05)];
        w.hull(Vec3::new(10.0, 0.0, 5.0), 0.0, &wheels);
        assert_eq!(w.feet.len(), 4);
        assert!(w.feet.iter().any(|(p, _)| p.distance(Vec2::new(11.5, 6.05)) < 1e-4));
        // A quarter turn swaps the long axis onto z.
        w.feet.clear();
        w.hull(Vec3::ZERO, core::f32::consts::FRAC_PI_2, &wheels);
        assert!(w
            .feet
            .iter()
            .all(|(p, _)| (p.x.abs() - 1.05).abs() < 1e-4 && (p.y.abs() - 1.5).abs() < 1e-4));
    }

    #[test]
    fn the_wind_never_stops_and_never_overshoots() {
        assert_eq!(wind_goal(0.0, false), WIND_FLOOR);
        assert_eq!(wind_goal(100.0, false), 1.0);
        assert_eq!(wind_goal(7.0, true), WIND_FLOOR);
        assert!(wind_goal(3.0, false) < wind_goal(6.0, false));
    }
}

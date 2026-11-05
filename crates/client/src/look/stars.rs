//! Stars: points of light behind the deck, on a mood that leans to dusk
//! (`mood::Mood::stars`). After Gates' `render/stars.rs` (MIT, NOTICE).
//!
//! **Points, not texels.** Each star is a quad the vertex stage sizes in
//! pixels, far out along its direction, so it stays a point at any
//! resolution and the level hides it by depth. **Behind the deck, off the
//! deck's own field**: the fragment stage puts a star behind cloud where the
//! sky draws that cloud, flat or as the raymarched slab, at the deck's drift.
//! They draw in the transparent pass, after the sky is painted, on every
//! target; the vertex stage fades the low ones itself.
//!
//! The same count on every target and every rung: a star is a look, and a
//! sky that showed more of them on one machine would be a sky two players
//! do not share.

use bevy::asset::Asset;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;

use super::deck::{self, SkyField};
use super::mood::{Drift, MoodNow};
use super::quality::Tier;
use super::sky;

pub const SHADER: &str = "shaders/stars.wgsl";

/// Stars in the sky: enough that a dusk shows its first few dozen, a few
/// bright enough to find first.
pub const STARS: u32 = 1_800;
/// A star's quad at its brightest, pixels across.
pub const STAR_PX: f32 = 3.0;
/// The share of the starfield a full moon washes out of the sky, faintest
/// first.
pub const MOON_WASH: f32 = 0.75;

/// The uniform, laid out to match `Stars` in the shader.
#[derive(Clone, Copy, Default, ShaderType, Debug, PartialEq)]
pub struct StarParams {
    /// x how much of the starfield shows; y the quad's size, pixels; z the
    /// share the moon washes out, faintest first; w unused.
    pub look: Vec4,
    /// x the deck's altitude over its noise scale, y the horizon cutoff, z
    /// the fade above it.
    pub deck: Vec4,
    /// xy the deck's drift when it was taken (noise units), z the field
    /// threshold (`1 − cover`), w the edge's width.
    pub cloud: Vec4,
    /// x one over the field's period (noise units), y half a texel (uv).
    pub field: Vec4,
    /// xyz toward the moon, w the cosine of its disk's radius: the moon adds
    /// to the sky, so the stars leave out the ones it passes over.
    pub moon: Vec4,
    /// The raymarched slab the sky draws (`deck::slab_lanes`).
    pub slab: Vec4,
    /// The drift's run (`mood::Anchor::flow`).
    pub flow: Vec4,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct StarMaterial {
    #[uniform(0)]
    pub params: StarParams,
    #[texture(1)]
    #[sampler(2)]
    pub field: Handle<Image>,
}

impl Material for StarMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn depth_bias(&self) -> f32 {
        deck::STARS_ORDER
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

/// The starfield's entity.
#[derive(Component)]
pub struct Stars;

/// The handle `drive` writes.
#[derive(Resource)]
pub struct StarHandle(pub Handle<StarMaterial>);

/// Deterministic per-star randomness — a look, not a sim draw.
fn hash01(i: u32, k: u32) -> f32 {
    let mut h = ((i as u64) << 32 | k as u64) ^ 0x5354_4152_5321;
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^= h >> 33;
    (h >> 40) as f32 / 16_777_216.0
}

/// The starfield mesh: `n` quads, each its star's direction (POSITION, unit,
/// above the horizon), its corners (UV_0), and its brightness, twinkle
/// phase, warmth and rank (COLOR).
pub fn star_mesh(n: u32) -> Mesh {
    let count = n as usize;
    let mut pos = Vec::with_capacity(count * 4);
    let mut uv = Vec::with_capacity(count * 4);
    let mut col = Vec::with_capacity(count * 4);
    let mut idx: Vec<u16> = Vec::with_capacity(count * 6);
    for i in 0..n {
        // Uniform over the cap above ~2°: y uniform is area uniform.
        let y = 0.03 + 0.97 * hash01(i, 0);
        let a = std::f32::consts::TAU * hash01(i, 1);
        let r = (1.0 - y * y).sqrt();
        let dir = [r * a.cos(), y, r * a.sin()];
        // Mostly faint, a few bright.
        let rank = hash01(i, 2);
        let bright = 0.12 + 0.88 * rank.powi(12);
        let phase = hash01(i, 3);
        let warmth = hash01(i, 4);
        let base = (i * 4) as u16;
        for corner in [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]] {
            pos.push(dir);
            uv.push(corner);
            col.push([bright, phase, warmth, rank]);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U16(idx));
    mesh
}

/// The uniform for a mood, behind its deck as the shaders have it.
pub fn params_for(now: &MoodNow, drift: &Drift, tier: &Tier) -> StarParams {
    let m = &now.mood;
    // The mist hides them as it hides the far wall.
    let show = (m.stars * (1.0 - (m.fog_sigma() * 200.0).min(1.0))).clamp(0.0, 1.0);
    StarParams {
        look: Vec4::new(show, STAR_PX, (MOON_WASH * m.moon).clamp(0.0, 1.0), 0.0),
        deck: Vec4::new(
            sky::CLOUD_ALT_M / sky::CLOUD_SCALE_M,
            sky::DECK_CUTOFF,
            sky::HORIZON_FADE,
            0.0,
        ),
        cloud: Vec4::new(drift.anchor.off.x, drift.anchor.off.y, 1.0 - m.cover, sky::EDGE),
        field: Vec4::new(1.0 / sky::FIELD_PERIOD as f32, 0.5 / sky::FIELD_N as f32, 0.0, 0.0),
        moon: m.to_moon().extend(sky::MOON_R.cos()),
        slab: deck::slab_lanes(tier.cloud_steps()),
        flow: drift.anchor.flow(),
    }
}

/// Build the starfield, hidden until a mood asks for it. After the deck's
/// field is built.
pub fn setup(
    mut commands: Commands,
    field: Res<SkyField>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StarMaterial>>,
) {
    let material = materials.add(StarMaterial {
        params: StarParams::default(),
        field: field.image.clone(),
    });
    commands.spawn((
        Stars,
        Mesh3d(meshes.add(star_mesh(STARS))),
        MeshMaterial3d(material.clone()),
        Transform::default(),
        Visibility::Hidden,
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    ));
    commands.insert_resource(StarHandle(material));
}

/// Show the stars where the mood has them, behind its deck. Writes the
/// material only when something moved.
pub fn drive(
    now: Res<MoodNow>,
    drift: Res<Drift>,
    tier: Res<Tier>,
    handle: Option<Res<StarHandle>>,
    mut materials: ResMut<Assets<StarMaterial>>,
    mut stars: Query<&mut Visibility, With<Stars>>,
) {
    let want = params_for(&now, &drift, &tier);
    let show = want.look.x > 0.0;
    for mut v in stars.iter_mut() {
        let vis = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *v != vis {
            *v = vis;
        }
    }
    let Some(handle) = handle else {
        return;
    };
    if show && materials.get(&handle.0).is_some_and(|m| m.params != want) {
        if let Some(m) = materials.get_mut(&handle.0) {
            m.params = want;
        }
    }
}

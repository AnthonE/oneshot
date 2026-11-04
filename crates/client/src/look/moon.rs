//! The moon: a disk sharp at any resolution, with its seas on it, on a mood
//! that has one up (`mood::Mood::moon`). After Gates' `render/moon.rs` (MIT,
//! NOTICE).
//!
//! **Points, not texels**: one quad far out along its direction, its rim
//! cut per pixel. **Behind the deck, off the deck's own field**, where the
//! sky draws that cloud (flat or the slab). **It adds to the sky, on every
//! target**: the air's glow lies in front of it and a gibbous moon's dark
//! limb shows it; the stars behind its disk are left out where they are
//! drawn (`stars.wgsl`). Its phase is the mood's (`Mood::moon_lit`), its
//! terminator facing the mood's sun.

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

pub const SHADER: &str = "shaders/moon.wgsl";

/// The quad over the disk, so the antialiased rim has room.
const QUAD_OVER: f32 = 1.1;

/// How bright the earth lights the moon's dark side, as a share of the
/// sunlit disk, at new moon: far over the real ~1/1000, so a crescent holds
/// the rest of its disk ashen, the way a person remembers it.
pub const EARTHSHINE: f32 = 0.03;

/// How much of the moon is over the horizon, `0..=1`: a setting moon takes
/// its light with it over a few degrees rather than at once.
pub fn above(moon: Vec3) -> f32 {
    let t = ((moon.y + 0.02) / 0.08).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The sun's light on the disk, in the frame the vertex stage builds (x
/// right and y up as the eye sees it, z toward the eye), for a disk `lit`
/// of the way full, lit from toward `sun`: the terminator faces that sun.
pub fn sunlight_on_disk(moon: Vec3, sun: Vec3, lit: f32) -> Vec3 {
    let d = moon.normalize_or(Vec3::Y);
    let c = d.cross(Vec3::Y);
    let right = if c.length_squared() > 1e-6 {
        c.normalize()
    } else {
        Vec3::X
    };
    let up = right.cross(d);
    let side = Vec2::new(sun.dot(right), sun.dot(up)).normalize_or(Vec2::X);
    // The phase angle's cosine: 1 full, -1 new.
    let cos_a = 2.0 * lit.clamp(0.0, 1.0) - 1.0;
    let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();
    (side * sin_a).extend(cos_a)
}

/// The uniform, laid out to match `Moon` in the shader.
#[derive(Clone, Copy, Default, ShaderType, Debug, PartialEq)]
pub struct MoonParams {
    pub dir: Vec4,
    pub light: Vec4,
    pub deck: Vec4,
    pub cloud: Vec4,
    pub field: Vec4,
    pub lit: Vec4,
    /// The raymarched slab the sky draws (`deck::slab_lanes`).
    pub slab: Vec4,
    /// The drift's run (`mood::Anchor::flow`).
    pub flow: Vec4,
}

impl MoonParams {
    /// The moon of a mood, behind its deck as the shaders have it.
    pub fn new(now: &MoodNow, drift: &Drift, tier: &Tier) -> Self {
        let m = &now.mood;
        let moon = m.to_moon();
        let rgb = Vec3::from_array(sky::MOON_RGB) * sky::CLOUD_NITS;
        Self {
            dir: moon.extend((sky::MOON_R * QUAD_OVER).tan()),
            light: rgb.extend(m.moon.clamp(0.0, 1.0) * above(moon)),
            deck: Vec4::new(
                sky::CLOUD_ALT_M / sky::CLOUD_SCALE_M,
                sky::DECK_CUTOFF,
                sky::HORIZON_FADE,
                deck::DETAIL,
            ),
            cloud: Vec4::new(drift.anchor.off.x, drift.anchor.off.y, 1.0 - m.cover, sky::EDGE),
            field: Vec4::new(
                1.0 / sky::FIELD_PERIOD as f32,
                0.5 / sky::FIELD_N as f32,
                0.0,
                QUAD_OVER,
            ),
            lit: sunlight_on_disk(moon, m.to_sun(), m.moon_lit).extend(EARTHSHINE * (1.0 - m.moon_lit)),
            slab: deck::slab_lanes(tier.cloud_steps()),
            flow: drift.anchor.flow(),
        }
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct MoonMaterial {
    #[uniform(0)]
    pub params: MoonParams,
    #[texture(1)]
    #[sampler(2)]
    pub field: Handle<Image>,
}

impl Material for MoonMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    /// Premultiplied, and the shader's alpha is 0: the disk adds.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn depth_bias(&self) -> f32 {
        deck::MOON_ORDER
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

/// The moon's entity.
#[derive(Component)]
pub struct Moon;

/// The handle `drive` writes.
#[derive(Resource)]
pub struct MoonHandle(pub Handle<MoonMaterial>);

/// One quad, corners -1..1; the shader puts it on the sky.
pub fn quad_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-1.0f32, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
        ],
    );
    mesh.insert_indices(Indices::U16(vec![0, 1, 2, 0, 2, 3]));
    mesh
}

/// Build the moon, hidden until a mood has one up.
pub fn setup(
    mut commands: Commands,
    field: Res<SkyField>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MoonMaterial>>,
) {
    let material = materials.add(MoonMaterial {
        params: MoonParams::default(),
        field: field.image.clone(),
    });
    commands.spawn((
        Moon,
        Mesh3d(meshes.add(quad_mesh())),
        MeshMaterial3d(material.clone()),
        Transform::default(),
        Visibility::Hidden,
        NoFrustumCulling,
        NotShadowCaster,
        NotShadowReceiver,
    ));
    commands.insert_resource(MoonHandle(material));
}

/// Put the mood's moon up, behind its deck.
pub fn drive(
    now: Res<MoodNow>,
    drift: Res<Drift>,
    tier: Res<Tier>,
    handle: Option<Res<MoonHandle>>,
    mut materials: ResMut<Assets<MoonMaterial>>,
    mut moon: Query<&mut Visibility, With<Moon>>,
) {
    let want = MoonParams::new(&now, &drift, &tier);
    let show = want.light.w > 0.0;
    for mut v in moon.iter_mut() {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A full disk faces the eye; a half one's terminator faces its sun.
    #[test]
    fn the_terminator_faces_the_sun() {
        let moon = Vec3::new(0.0, 0.3, 1.0).normalize();
        let full = sunlight_on_disk(moon, Vec3::X, 1.0);
        assert!((full.z - 1.0).abs() < 1e-5);
        let half = sunlight_on_disk(moon, Vec3::NEG_Z, 0.5);
        assert!(half.z.abs() < 1e-5 && half.length() > 0.99);
        assert!(above(Vec3::NEG_Y) == 0.0 && above(Vec3::Y) == 1.0);
    }
}

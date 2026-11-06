//! The sticky, drawn: a ball of plasma, no model. A sphere under its own
//! shader (`assets/shaders/plasma.wgsl`: flowing noise, white-hot filaments, a
//! heart burning through, thinning to nothing at its edge, its silhouette
//! boiling) in a larger shell of glow, one mesh and two materials for all of
//! them, each one's heat (how far its fuse has run) in its [`MeshTag`]. Around
//! it `flying.rs` hangs the rest: a glow for the bloom, a light that flickers on
//! the walls, sparks behind it in flight, arcs and vapour once it has stuck.
//! Drawing only: what its fuse is is read off when it was seen to stick.

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::MeshTag;
use bevy::pbr::Material;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub const SHADER: &str = "shaders/plasma.wgsl";

/// Its radius, metres: a thing you can see coming and pick out on a body.
pub const RADIUS_M: f32 = 0.085;
/// How much bigger it is with its fuse run out.
pub const SWELL: f32 = 0.55;
/// How bright it is, and how far its surface boils (a share of its radius).
const GAIN: f32 = 1.1;
const BOIL: f32 = 0.14;
/// Its corona: how many times its radius, and how bright.
const CORONA_SIZE: f32 = 2.3;
const CORONA_GAIN: f32 = 0.34;
/// Its heat in flight, of 1 at the end of its fuse.
pub const FLIGHT_HEAT: f32 = 0.12;
/// Its light: lumens at rest, how many times that with its fuse run out, how
/// far it reaches, metres, and its colour.
pub const LUMENS: f32 = 420_000.0;
pub const LUMENS_HOT: f32 = 3.0;
pub const LIGHT_M: f32 = 9.0;
pub const LIGHT_RGB: [f32; 3] = [0.34, 0.58, 1.0];
/// Its heat on a pad, lying there: the flow without the fuse.
const LYING_HEAT: f32 = 0.05;

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct PlasmaMaterial {
    /// x how bright, y how far its surface boils, z 1 for its corona.
    #[uniform(0)]
    pub knobs: Vec4,
}

impl Material for PlasmaMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// The one mesh and two materials every ball is.
#[derive(Resource, Clone)]
pub struct Kit {
    pub mesh: Handle<Mesh>,
    pub material: Handle<PlasmaMaterial>,
    pub corona: Handle<PlasmaMaterial>,
}

pub fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<PlasmaMaterial>>) {
    commands.insert_resource(Kit {
        mesh: meshes.add(Sphere::new(1.0).mesh().ico(4).expect("a small icosphere")),
        material: materials.add(PlasmaMaterial {
            knobs: Vec4::new(GAIN, BOIL, 0.0, 0.0),
        }),
        corona: materials.add(PlasmaMaterial {
            knobs: Vec4::new(CORONA_GAIN, BOIL, 1.0, 0.0),
        }),
    });
}

/// `heat` (0 to 1) as a ball's tag.
pub fn tag(heat: f32) -> MeshTag {
    MeshTag((heat.clamp(0.0, 1.0) * 255.0) as u32)
}

/// A ball of `heat`, for a parent to carry: its transform is what scales it,
/// and its corona comes with it as a child. Its entity.
pub fn spawn_ball(c: &mut ChildSpawnerCommands, kit: &Kit, heat: f32) -> Entity {
    c.spawn((
        Mesh3d(kit.mesh.clone()),
        MeshMaterial3d(kit.material.clone()),
        tag(heat),
        Transform::from_scale(Vec3::splat(RADIUS_M)),
        NotShadowCaster,
        NotShadowReceiver,
    ))
    .with_child((
        Mesh3d(kit.mesh.clone()),
        MeshMaterial3d(kit.corona.clone()),
        tag(heat),
        Transform::from_scale(Vec3::splat(CORONA_SIZE)),
        NotShadowCaster,
        NotShadowReceiver,
    ))
    .id()
}

/// One lying on a pad.
pub fn spawn_lying(c: &mut ChildSpawnerCommands, kit: &Kit) {
    spawn_ball(c, kit, LYING_HEAT);
}

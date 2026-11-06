//! Plasma, drawn: no model, a sphere under its own shader
//! (`assets/shaders/plasma.wgsl`: flowing noise, white-hot filaments, a heart
//! burning through, thinning to nothing at its edge, its silhouette boiling) in
//! a larger shell of glow. One mesh and a few materials for all of them, a
//! [`Tone`] each, each one's heat (how far its fuse has run, how full its charge)
//! in its [`MeshTag`]: a sticky is blue, a charged bolt and the cannon's ball
//! green. The same shader is a hot gun's skin (`vent.rs`).
//!
//! What hangs round a ball (its glow for the bloom, its light, what comes off
//! it) is `flying.rs`'s. Drawing only.

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::MeshTag;
use bevy::pbr::Material;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub const SHADER: &str = "shaders/plasma.wgsl";

/// A sticky's radius, metres: a thing you can see coming and pick out on a body.
pub const RADIUS_M: f32 = 0.085;
/// How much bigger it is with its fuse run out.
pub const SWELL: f32 = 0.55;
/// How bright a ball is, and how far its surface boils (a share of its radius).
const GAIN: f32 = 1.1;
const BOIL: f32 = 0.14;
/// Its corona: how many times its radius, and how bright.
const CORONA_SIZE: f32 = 2.3;
const CORONA_GAIN: f32 = 0.34;
/// A hot gun's skin: how bright, how far off the gun (metres), how fine its crawl.
const SKIN_GAIN: f32 = 0.55;
const SKIN_OFF_M: f32 = 0.0015;
const SKIN_CRAWL: f32 = 42.0;
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

/// What colour it burns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Blue,
    Green,
    Pink,
}

impl Tone {
    pub const ALL: [Tone; 3] = [Tone::Blue, Tone::Green, Tone::Pink];

    /// Linear HDR: in its gaps, in its body, in its filaments and heart.
    const fn palette(self) -> [Vec4; 3] {
        match self {
            Tone::Blue => [
                Vec4::new(0.006, 0.035, 0.42, 1.0),
                Vec4::new(0.03, 0.26, 1.15, 1.0),
                Vec4::new(0.85, 1.25, 2.1, 1.0),
            ],
            Tone::Green => [
                Vec4::new(0.002, 0.15, 0.025, 1.0),
                Vec4::new(0.025, 0.8, 0.13, 1.0),
                Vec4::new(0.7, 2.2, 0.85, 1.0),
            ],
            Tone::Pink => [
                Vec4::new(0.22, 0.01, 0.3, 1.0),
                Vec4::new(0.95, 0.1, 1.1, 1.0),
                Vec4::new(2.1, 1.0, 2.2, 1.0),
            ],
        }
    }
}

/// Field for field `Plasma` in the shader.
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct PlasmaParams {
    /// x how bright, y how far it boils or stands off, z 0 a ball, 1 a corona,
    /// 2 a skin, w how fine a skin's crawl is.
    pub knobs: Vec4,
    pub deep: Vec4,
    pub mid: Vec4,
    pub hot: Vec4,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct PlasmaMaterial {
    #[uniform(0)]
    pub params: PlasmaParams,
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

/// The one mesh, and each tone's ball, corona and skin.
#[derive(Resource, Clone)]
pub struct Kit {
    pub mesh: Handle<Mesh>,
    balls: [Handle<PlasmaMaterial>; 3],
    coronas: [Handle<PlasmaMaterial>; 3],
    skins: [Handle<PlasmaMaterial>; 3],
}

impl Kit {
    /// Every material, to warm their pipelines (`prewarm.rs`).
    pub fn all(&self) -> impl Iterator<Item = &Handle<PlasmaMaterial>> {
        self.balls.iter().chain(&self.coronas).chain(&self.skins)
    }

    pub fn skin(&self, tone: Tone) -> Handle<PlasmaMaterial> {
        self.skins[tone as usize].clone()
    }
}

pub fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<PlasmaMaterial>>) {
    let mut each = |knobs: Vec4| {
        Tone::ALL.map(|tone| {
            let [deep, mid, hot] = tone.palette();
            materials.add(PlasmaMaterial {
                params: PlasmaParams { knobs, deep, mid, hot },
            })
        })
    };
    let kit = Kit {
        mesh: meshes.add(Sphere::new(1.0).mesh().ico(4).expect("a small icosphere")),
        balls: each(Vec4::new(GAIN, BOIL, 0.0, 0.0)),
        coronas: each(Vec4::new(CORONA_GAIN, 0.0, 1.0, 0.0)),
        skins: each(Vec4::new(SKIN_GAIN, SKIN_OFF_M, 2.0, SKIN_CRAWL)),
    };
    commands.insert_resource(kit);
}

/// `heat` (0 to 1) as a tag.
pub fn tag(heat: f32) -> MeshTag {
    MeshTag((heat.clamp(0.0, 1.0) * 255.0) as u32)
}

/// A ball of `heat`, `radius` metres, for a parent to carry: its transform is
/// what scales it, and its corona comes with it as a child. Its entity.
pub fn spawn_ball(c: &mut ChildSpawnerCommands, kit: &Kit, tone: Tone, radius: f32, heat: f32) -> Entity {
    let i = tone as usize;
    c.spawn((
        Mesh3d(kit.mesh.clone()),
        MeshMaterial3d(kit.balls[i].clone()),
        tag(heat),
        Transform::from_scale(Vec3::splat(radius)),
        NotShadowCaster,
        NotShadowReceiver,
    ))
    .with_child((
        Mesh3d(kit.mesh.clone()),
        MeshMaterial3d(kit.coronas[i].clone()),
        tag(heat),
        Transform::from_scale(Vec3::splat(CORONA_SIZE)),
        NotShadowCaster,
        NotShadowReceiver,
    ))
    .id()
}

/// A sticky lying on a pad.
pub fn spawn_lying(c: &mut ChildSpawnerCommands, kit: &Kit) {
    spawn_ball(c, kit, Tone::Blue, RADIUS_M, LYING_HEAT);
}

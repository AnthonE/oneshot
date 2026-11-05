//! The shock of a blast: a sphere blown out from it in a third of a
//! second, drawn as a rim of light (bright where its surface is seen edge
//! on, nothing face on), additive, fading as it grows. A few live at once;
//! each is an entity with its own material, scaled and faded from `Fx`
//! every frame. Drawing only.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::pbr::Material;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub const SHADER: &str = "shaders/fx_shock.wgsl";

/// Shells in the air at once.
pub const SHELLS: usize = 4;

/// One shock in flight.
#[derive(Clone, Copy, Debug, Default)]
pub struct Shock {
    pub at: Vec3,
    pub left: f32,
    pub life: f32,
    /// Its radius at birth and at death, metres.
    pub r0: f32,
    pub r1: f32,
    /// Its rim's colour, linear HDR.
    pub color: Vec3,
}

/// A fire blast's shell: a warm rim, 0.3 to `4 × big` metres in 0.3 s.
pub fn fire(at: Vec3, big: f32) -> Shock {
    Shock {
        at,
        left: 0.26,
        life: 0.26,
        r0: 0.3,
        r1: 4.0 * big,
        color: Vec3::new(1.6, 1.2, 0.75),
    }
}

/// A sticky's: ion blue, quicker.
pub fn ion(at: Vec3) -> Shock {
    Shock {
        at,
        left: 0.24,
        life: 0.24,
        r0: 0.3,
        r1: 4.0,
        color: Vec3::new(0.7, 1.1, 2.1),
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct ShockMaterial {
    /// rgb: the rim's colour; a: how far through its life (0 born, 1 gone).
    #[uniform(0)]
    pub color: Vec4,
}

impl Material for ShockMaterial {
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

/// A shell's entity: its index into `Fx::shocks`, and its material.
#[derive(Component)]
pub struct ShockShell {
    pub index: usize,
    pub material: Handle<ShockMaterial>,
}

/// The shells, hidden, each on a unit sphere.
pub fn setup(commands: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<ShockMaterial>) {
    let sphere = meshes.add(
        Sphere::new(1.0)
            .mesh()
            .ico(3)
            .unwrap_or_else(|_| Sphere::new(1.0).mesh().uv(24, 16)),
    );
    for index in 0..SHELLS {
        let material = mats.add(ShockMaterial {
            color: Vec4::new(1.0, 1.0, 1.0, 1.0),
        });
        commands.spawn((
            ShockShell {
                index,
                material: material.clone(),
            },
            Mesh3d(sphere.clone()),
            MeshMaterial3d(material),
            Transform::from_scale(Vec3::splat(0.001)),
            Visibility::Hidden,
            NoFrustumCulling,
            NotShadowCaster,
        ));
    }
}

/// A free shock slot, or the one nearest done.
pub fn claim(shocks: &mut [Shock; SHELLS]) -> &mut Shock {
    let i = (0..SHELLS)
        .min_by(|&a, &b| shocks[a].left.total_cmp(&shocks[b].left))
        .unwrap_or(0);
    &mut shocks[i]
}

/// Each shell this frame: grown, faded, placed; hidden once gone.
pub fn draw<F: bevy::ecs::query::QueryFilter>(
    shocks: &mut [Shock; SHELLS],
    dt: f32,
    shells: &mut Query<(&ShockShell, &mut Transform, &mut Visibility), F>,
    mats: &mut Assets<ShockMaterial>,
) {
    for (shell, mut tf, mut vis) in shells {
        let s = &mut shocks[shell.index];
        if s.left <= 0.0 {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        }
        s.left = (s.left - dt).max(0.0);
        let age = 1.0 - s.left / s.life.max(1e-4);
        // Fast out of the blast, slowing as it goes.
        let k = 1.0 - (1.0 - age) * (1.0 - age);
        let r = s.r0 + (s.r1 - s.r0) * k;
        tf.translation = s.at;
        tf.scale = Vec3::splat(r.max(1e-3));
        *vis = Visibility::Visible;
        if let Some(m) = mats.get_mut(&shell.material) {
            m.color = s.color.extend(age);
        }
    }
}

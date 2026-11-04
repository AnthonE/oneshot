//! The shield, drawn: a shell over a fighter's body, the body's own skinned
//! meshes again pushed out along their normals, gold, additive, a fresnel
//! rim and a crawl of hexagons, as bright as the shield's flare
//! (`fx/bodies.rs`: a round taken, the break, the climb back). Hidden at
//! rest. Drawing only: a level the snapshots already imply.
//!
//! An `ExtendedMaterial` over `StandardMaterial` with its own vertex
//! (Bevy's mesh vertex shader with skinning, plus the push) and fragment
//! shaders (`assets/shaders/shield.wgsl`). WebGL2 runs it: no compute.

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub type ShieldMaterial = ExtendedMaterial<StandardMaterial, Shell>;

/// How far the shell stands off the body, metres.
pub const INFLATE_M: f32 = 0.035;
/// The shell's gold at a flare of 1, linear HDR.
pub const GOLD: Vec3 = Vec3::new(3.2, 2.1, 0.55);

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct Shell {
    /// rgb: the colour at full; a: the level now (0 hides it).
    #[uniform(100)]
    pub color: Vec4,
    /// x: the push, metres; y: hexagons per uv unit.
    #[uniform(101)]
    pub params: Vec4,
}

impl MaterialExtension for Shell {
    fn vertex_shader() -> ShaderRef {
        "shaders/shield.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/shield.wgsl".into()
    }
}

/// A shell's material, at rest.
pub fn material() -> ShieldMaterial {
    ExtendedMaterial {
        base: StandardMaterial {
            unlit: true,
            alpha_mode: AlphaMode::Add,
            cull_mode: None,
            ..default()
        },
        extension: Shell {
            color: GOLD.extend(0.0),
            params: Vec4::new(INFLATE_M, 22.0, 0.0, 0.0),
        },
    }
}

/// A shell mesh: hidden while its body's shield is quiet.
#[derive(Component)]
pub struct ShellPart;

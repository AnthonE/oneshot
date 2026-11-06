//! The shield, drawn: a shell over a fighter's body, the body's own skinned
//! meshes again pushed out along their normals, gold, additive, a fresnel
//! rim and a crawl of hexagons, as bright as the shield's flare
//! (`fx/bodies.rs`: a round taken, the break, the climb back). Hidden at
//! rest. Drawing only: a level the snapshots already imply.
//!
//! An `ExtendedMaterial` over `StandardMaterial` with its own vertex
//! (Bevy's mesh vertex shader with skinning, plus the push) and fragment
//! shaders (`assets/shaders/shield.wgsl`). WebGL2 runs it: no compute.

use bevy::mesh::MeshTag;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub type ShieldMaterial = ExtendedMaterial<StandardMaterial, Shell>;

/// How far the shell stands off the body, metres.
pub const INFLATE_M: f32 = 0.035;
/// The shell's gold at a flare of 1, linear HDR.
pub const GOLD: Vec3 = Vec3::new(3.2, 2.1, 0.55);

/// The shell's three colours at full, linear HDR (rgb; w unused); which,
/// and how bright, is each mesh's [`Flare`] tag.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub struct Tones {
    pub gold: Vec4,
    pub over: Vec4,
    pub camo: Vec4,
}

/// Which colour a shell is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Gold = 0,
    Over = 1,
    Camo = 2,
}

/// The highest flare a tag carries; [`Flare::tag`] keeps 16 bits of it.
const LEVEL_MAX: f32 = 4.0;

/// A shell's (or a body's) flare as its meshes' `MeshTag`: the level in
/// the low 16 bits, the tone in the next two, and whether the body is
/// hidden (camouflage) in the one after (`shield.wgsl`, `body_paint.wgsl`
/// read it back). A tag, not a material: it changes every frame of a hit or
/// a recharge, and a material written that often is a bind group made
/// again that often, for every body flaring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flare {
    pub level: f32,
    pub tone: Tone,
    pub hidden: bool,
}

impl Flare {
    pub fn tag(self) -> MeshTag {
        let q = (self.level.clamp(0.0, LEVEL_MAX) / LEVEL_MAX * 65535.0).round() as u32;
        MeshTag(q | (self.tone as u32) << 16 | (self.hidden as u32) << 18)
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct Shell {
    /// Its colours: each mesh's tag says which, and how bright.
    #[uniform(100)]
    pub tones: Tones,
    /// x: the push, metres; y: hexagons per uv unit; z: 1 drops the
    /// fill and the band, leaving the rim and the lines (the arms in view).
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

/// A shell's material: one a body (its tags say how it flares).
pub fn material() -> ShieldMaterial {
    ExtendedMaterial {
        base: StandardMaterial {
            unlit: true,
            alpha_mode: AlphaMode::Add,
            cull_mode: None,
            ..default()
        },
        extension: Shell {
            tones: Tones {
                gold: GOLD.extend(0.0),
                over: super::bodies::OVER.extend(0.0),
                camo: super::bodies::CAMO.extend(0.0),
            },
            params: Vec4::new(INFLATE_M, 22.0, 0.0, 0.0),
        },
    }
}

/// A shell mesh: hidden while its body's shield is quiet.
#[derive(Component)]
pub struct ShellPart;

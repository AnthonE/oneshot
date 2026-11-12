//! A snowed valley's boulder: `bevy_pbr`'s own fragment over Meshy's paint,
//! with snow lying on what faces the sky (`snow_stone.wgsl`). Its flanks stay
//! the cold dark stone they were (`boulders::COLD_TINT`), so a boulder is
//! still dark cover against white; only its crown is capped, as every stone
//! in a moraine is.
//!
//! Drawing only: the box that collides is the sim's. A uniform and
//! arithmetic, so it binds and runs the same on WebGL2 and on every rung.

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

const SHADER: &str = "shaders/snow_stone.wgsl";

/// How far a facet turns to the sky (its normal's y) where the cap starts
/// and where it is whole: its crown. From 0.52 a boulder seen from a perch
/// or the air was all cap, a snow mound, and cover lost its dark; from 0.74
/// a walker saw a hairline, a bare boulder among capped stones.
pub const CAP_UP: (f32, f32) = (0.62, 0.84);
/// The cap's colour, a clear step under the field's snow so the boulder
/// keeps its round top against it, and its roughness.
pub const CAP_RGB: [f32; 3] = [0.66, 0.69, 0.75];
pub const CAP_ROUGH: f32 = 0.92;

/// The uniform; each lane is named in `snow_stone.wgsl`.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct SnowStoneParams {
    pub cap: Vec4,
    pub snow: Vec4,
}

/// The extension. Not bindless, as the level's isn't.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct SnowStone {
    #[uniform(100)]
    pub params: SnowStoneParams,
}

pub type SnowStoneMaterial = ExtendedMaterial<StandardMaterial, SnowStone>;

impl MaterialExtension for SnowStone {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    /// Forward only: this client never runs deferred.
    fn deferred_fragment_shader() -> ShaderRef {
        ShaderRef::Default
    }
}

/// `base` under snow: capped where `capped` (a rock), as it is where not (a
/// pylon, a prop, a wreck: made things, which the level's own paint snows).
pub fn material(base: StandardMaterial, capped: bool) -> SnowStoneMaterial {
    SnowStoneMaterial {
        base,
        extension: SnowStone {
            params: SnowStoneParams {
                cap: Vec4::new(CAP_UP.0, CAP_UP.1, if capped { 1.0 } else { 0.0 }, CAP_ROUGH),
                snow: Vec4::new(CAP_RGB[0], CAP_RGB[1], CAP_RGB[2], 0.0),
            },
        },
    }
}

pub fn plugin(app: &mut App) {
    app.add_plugins(MaterialPlugin::<SnowStoneMaterial>::default());
}

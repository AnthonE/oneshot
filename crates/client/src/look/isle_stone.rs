//! An island's scenery stone: the wind-carved rock under its rim and standing
//! out of its sea of cloud. `bevy_pbr`'s own fragment over Meshy's paint, going
//! into shade the further it hangs under the rim and lost in the cloud's colour
//! as it comes down to the cloud, so nothing stands on the sheet with a hard
//! line (`isle_stone.wgsl`).
//!
//! Drawing only, and nobody reaches it. A uniform and arithmetic, so it binds
//! and runs the same on WebGL2.

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

const SHADER: &str = "shaders/isle_stone.wgsl";

/// From here down, m (the island's floor is 0), the stone goes into shade, by
/// [`SHADE_DARK`] of its light at [`SHADE_TO_Y`]: the rim stays in the sun and
/// the keel falls away dark.
pub const SHADE_FROM_Y: f32 = -2.0;
pub const SHADE_TO_Y: f32 = -34.0;
pub const SHADE_DARK: f32 = 0.34;
/// How far over the sea of cloud it starts going to the cloud's colour, m.
pub const LAP_M: f32 = 24.0;

/// The uniform; each lane is named in `isle_stone.wgsl`.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct IsleStoneParams {
    pub shade: Vec4,
    pub lap: Vec4,
}

/// The extension. Not bindless, as the level's isn't.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct IsleStone {
    #[uniform(100)]
    pub params: IsleStoneParams,
}

pub type IsleStoneMaterial = ExtendedMaterial<StandardMaterial, IsleStone>;

impl MaterialExtension for IsleStone {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    /// Forward only: this client never runs deferred.
    fn deferred_fragment_shader() -> ShaderRef {
        ShaderRef::Default
    }
}

/// `base` as an island's stone, over a sea of cloud at `sea` going to `far`
/// (the mood's fog, in the frame's lit units, as the sea's own rim goes).
pub fn material(base: StandardMaterial, sea: f32, far: Vec3) -> IsleStoneMaterial {
    IsleStoneMaterial {
        base,
        extension: IsleStone {
            params: IsleStoneParams {
                shade: Vec4::new(SHADE_FROM_Y, SHADE_TO_Y, SHADE_DARK, 0.0),
                lap: far.extend(sea + LAP_M),
            },
        },
    }
}

pub fn plugin(app: &mut App) {
    app.add_plugins(MaterialPlugin::<IsleStoneMaterial>::default());
}

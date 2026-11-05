//! The ground: four photographed sets (sand, grass, the creek's gravel,
//! rock) blended by the splat weights the terrain mesh carries in its
//! vertex colour, each at its own photographed size, over Bevy's PBR, wet
//! where the map's mood rains and crossed by its cloud deck's shadows
//! (`cloud_shadow.rs`). After Gates' `render/ground_splat.rs` (MIT, NOTICE):
//! its extension of `StandardMaterial`, its texture arrays, its sizes;
//! without the island's roads, blight, snow and caustics. The rock skin
//! over its rock faces (`cliffs.rs`) wears it too, its value off `UV_1`.

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub type GroundMaterial = ExtendedMaterial<StandardMaterial, Ground>;

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct Ground {
    #[texture(100, dimension = "2d_array")]
    #[sampler(101)]
    pub albedo: Handle<Image>,
    #[texture(102, dimension = "2d_array")]
    #[sampler(103)]
    pub normal: Handle<Image>,
    /// Everything else in one uniform block: under WebGL2 a fragment shader
    /// may read only twelve on many devices (ANGLE on D3D11 and on Metal),
    /// and Bevy's view, mesh and `StandardMaterial` take eleven of them.
    #[uniform(104)]
    pub params: GroundParams,
}

/// The ground's uniform, laid out as `GroundLook` in `cloud_shadow.wgsl`.
#[derive(Clone, Copy, Default, ShaderType, Debug, PartialEq)]
pub struct GroundParams {
    /// Metres each set repeats over, in splat order.
    pub tile_m: Vec4,
    /// What each set's photograph is multiplied by: the map's own soil (a
    /// red-rock canyon's sand is not a beach's).
    pub tint: [Vec4; 4],
    /// The deck's shadows and the rain's wet (`cloud_shadow::drive` keeps
    /// it): a uniform, not a texture or a buffer, so the ground binds what
    /// it did on every target.
    pub cloud: super::cloud_shadow::CloudShadowParams,
}

/// Dry Fork's soils over Gates' photographs: warm desert sand, dry grass,
/// the creek's tan gravel, red rock.
pub const TINT: [Vec4; 4] = [
    Vec4::new(1.18, 0.96, 0.74, 1.0),
    Vec4::new(1.05, 0.98, 0.8, 1.0),
    Vec4::new(1.1, 0.9, 0.72, 1.0),
    Vec4::new(1.12, 0.66, 0.48, 1.0),
];

impl MaterialExtension for Ground {
    fn fragment_shader() -> ShaderRef {
        "shaders/ground.wgsl".into()
    }
}

pub fn material(
    albedo: Handle<Image>,
    normal: Handle<Image>,
    cloud: super::cloud_shadow::CloudShadowParams,
) -> GroundMaterial {
    ExtendedMaterial {
        base: StandardMaterial {
            perceptual_roughness: 0.92,
            reflectance: super::fresnel::DIELECTRIC,
            ..default()
        },
        extension: Ground {
            albedo,
            normal,
            params: GroundParams {
                tile_m: Vec4::from_array(super::textures::GROUND_TILE_M),
                tint: TINT,
                cloud,
            },
        },
    }
}

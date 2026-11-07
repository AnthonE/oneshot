//! Still water under a roof (Cistern): a dark sheet at the water line and, under
//! it, the architecture over it again upside down.
//!
//! **A reflection that costs a mesh.** A mirrored copy of the built things
//! (`arch.py`'s `mirror_parts`, one GLB a look) stands under the sheet, unlit in
//! its own photograph and painted occlusion, so looking down one sees the vault
//! and the columns from below, with true parallax, on WebGL2 as on the desktop;
//! its vertex stage shakes it a little, as the sheet's ripples would. The sheet
//! over it lets it through, less at a glancing look, and catches the lamps.
//!
//! Drawing only: fighters aren't in it, so it can't show anyone. The sheet is at
//! the height of an ankle, so it hides no one either.

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

use super::fresnel;

const WATER_SHADER: &str = "shaders/pool_water.wgsl";
const REFLECT_SHADER: &str = "shaders/pool_reflect.wgsl";

/// The sheet's colour, linear: nearly black, a little green.
const WATER_RGB: [f32; 3] = [0.006, 0.012, 0.012];
const WATER_ROUGH: f32 = 0.06;
const WATER_REFLECTANCE: f32 = 0.08;
/// How much of the reflection the sheet lets through, looking straight down and
/// at a glancing look (`pool_water.wgsl`), how big its ripples are, m, and how
/// hard they bend it.
const SHEET_DOWN: f32 = 0.35;
const SHEET_GLANCE: f32 = 0.95;
const RIPPLE_M: f32 = 1.4;
const RIPPLE_BEND: f32 = 0.09;
/// The lights' reflection, over their painted colour; the rest's dimming is in
/// its COLOR_0 (`cistern.py`'s `DIM`), a Blender run away.
const REFLECT_LIGHT: [f32; 3] = [1.6, 1.6, 1.6];
/// How far the reflection shakes, m, at its deepest, and over how many metres.
const SHAKE_M: f32 = 0.035;
const SHAKE_OVER_M: f32 = 2.6;

/// A part waiting to be a wet road: a sheet in a look's photograph, a road's
/// rain on it, letting what's under it through as the sheet does.
const ROAD_THROUGH_DOWN: f32 = 0.2;
const ROAD_THROUGH_GLANCE: f32 = 0.78;
/// Its colour over its photograph: wet asphalt is dark.
const ROAD_TINT: [f32; 3] = [0.62, 0.62, 0.66];
const ROAD_ROUGH: f32 = 0.34;

/// A part waiting to be the sheet.
#[derive(Component)]
pub struct WaterWanted;

/// A part waiting to be a wet road, in a look's photograph.
#[derive(Component)]
pub struct WetWanted(pub u8);

/// A part waiting to be a reflection, in a look's photograph.
#[derive(Component)]
pub struct ReflectWanted(pub Option<u8>);

#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct PoolWaterParams {
    /// x through looking down, y at a glance, z the ripple's size, w its bend.
    pub sheet: Vec4,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct PoolWater {
    #[uniform(100)]
    pub params: PoolWaterParams,
}

pub type PoolWaterMaterial = ExtendedMaterial<StandardMaterial, PoolWater>;

impl MaterialExtension for PoolWater {
    fn fragment_shader() -> ShaderRef {
        WATER_SHADER.into()
    }
    fn deferred_fragment_shader() -> ShaderRef {
        ShaderRef::Default
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct PoolReflectParams {
    /// x how far it shakes, y over how many metres, zw unused.
    pub shake: Vec4,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct PoolReflect {
    #[uniform(100)]
    pub params: PoolReflectParams,
}

pub type PoolReflectMaterial = ExtendedMaterial<StandardMaterial, PoolReflect>;

impl MaterialExtension for PoolReflect {
    fn vertex_shader() -> ShaderRef {
        REFLECT_SHADER.into()
    }
    /// Its vertex stage moves it, so a prepass of it in place would stand in
    /// front of it and cut holes in it.
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn deferred_fragment_shader() -> ShaderRef {
        ShaderRef::Default
    }
}

/// Gives what's waiting its material, once the photographs are in.
pub fn dress(
    mut commands: Commands,
    tex: Res<super::textures::Textures>,
    water: Query<Entity, With<WaterWanted>>,
    wet: Query<(Entity, &WetWanted)>,
    reflections: Query<(Entity, &ReflectWanted)>,
    mut waters: ResMut<Assets<PoolWaterMaterial>>,
    mut reflects: ResMut<Assets<PoolReflectMaterial>>,
) {
    for e in &water {
        let m = waters.add(PoolWaterMaterial {
            base: StandardMaterial {
                base_color: Color::linear_rgba(WATER_RGB[0], WATER_RGB[1], WATER_RGB[2], 1.0),
                perceptual_roughness: WATER_ROUGH,
                // The room it reflects is the one under it: the hall's cube would
                // lay a sheen over that.
                reflectance: WATER_REFLECTANCE,
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
            extension: PoolWater {
                params: PoolWaterParams {
                    sheet: Vec4::new(SHEET_DOWN, SHEET_GLANCE, RIPPLE_M, RIPPLE_BEND),
                },
            },
        });
        commands.entity(e).remove::<WaterWanted>().insert(MeshMaterial3d(m));
    }
    if !tex.ready() {
        return;
    }
    for (e, wanted) in &wet {
        let Some((albedo, normal)) = crate::level::set_of(wanted.0).and_then(|name| tex.set(name)) else {
            continue;
        };
        let m = waters.add(PoolWaterMaterial {
            base: StandardMaterial {
                base_color: Color::linear_rgba(ROAD_TINT[0], ROAD_TINT[1], ROAD_TINT[2], 1.0),
                base_color_texture: Some(albedo),
                normal_map_texture: Some(normal),
                perceptual_roughness: ROAD_ROUGH,
                reflectance: fresnel::DIELECTRIC,
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
            extension: PoolWater {
                params: PoolWaterParams {
                    // No ripple of its own: its photograph's the surface.
                    sheet: Vec4::new(ROAD_THROUGH_DOWN, ROAD_THROUGH_GLANCE, 1.0, 0.0),
                },
            },
        });
        commands.entity(e).remove::<WetWanted>().insert(MeshMaterial3d(m));
    }
    for (e, wanted) in &reflections {
        let set = wanted.0.and_then(crate::level::set_of);
        let albedo = set.and_then(|name| tex.set(name)).map(|s| s.0);
        if wanted.0.is_some() && albedo.is_none() {
            continue;
        }
        let tint = if wanted.0.is_some() { [1.0; 3] } else { REFLECT_LIGHT };
        let m = reflects.add(PoolReflectMaterial {
            base: StandardMaterial {
                base_color: Color::linear_rgb(tint[0], tint[1], tint[2]),
                base_color_texture: albedo,
                unlit: true,
                ..default()
            },
            extension: PoolReflect {
                params: PoolReflectParams {
                    shake: Vec4::new(SHAKE_M, SHAKE_OVER_M, 0.0, 0.0),
                },
            },
        });
        commands.entity(e).remove::<ReflectWanted>().insert(MeshMaterial3d(m));
    }
}

pub fn plugin(app: &mut App) {
    app.add_plugins(MaterialPlugin::<PoolWaterMaterial>::default())
        .add_plugins(MaterialPlugin::<PoolReflectMaterial>::default());
}

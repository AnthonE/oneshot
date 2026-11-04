//! The camera and the light, and their post stack: one owner for exposure,
//! tone, fill, sun and haze, so they can't drift apart.
//!
//! Ported from Gates' `crates/client/src/render/rig.rs` (MIT, © 2026
//! MoreRight DAO) at its noon. The sky is the GPU's to decide (`gpu.rs`:
//! Bevy's atmosphere where there is compute, else a baked sky and the air
//! as a fog); the post stack (SSAO, anti-aliasing, bloom) and the sun's
//! cascades are the graphics ladder's (`quality::apply` puts them on this
//! camera and every sun the frame they appear, and again whenever the tier
//! moves). Gates' day cycle, weather, moon and shafts are left out: a match
//! is noon on a clear day.

use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{light_consts::lux, CascadeShadowConfigBuilder, EnvironmentMapLight, SunDisk};
use bevy::pbr::{Atmosphere, AtmosphereSettings, ScatteringMedium};
use bevy::prelude::*;
use bevy::render::view::{ColorGrading, Msaa};

/// The sun at noon: elevation above the horizon and azimuth, radians
/// (Gates' `RIG_SUN_ELEVATION`, `RIG_SUN_AZIMUTH`).
pub const SUN_ELEVATION: f32 = 0.61;
pub const SUN_AZIMUTH: f32 = 2.35;
/// Sunlit exterior, 0.8 stop under Bevy's `Exposure::SUNLIGHT` (Gates
/// measured it against its reference frames).
pub const DAY_EV100: f32 = 14.2;
pub const FOV_DEG: f32 = 75.0;
pub const FAR_M: f32 = 600.0;
/// The air: Gates' Mie term, thickened for sightlines of hundreds of
/// metres rather than tens of kilometres, and how many metres a scene unit
/// counts for in the atmosphere.
pub const MIE_SCATTERING: f32 = 3.996e-6;
pub const MIE_ABSORPTION: f32 = 0.444e-6;
pub const AIR_DENSITY: f32 = 0.85;
pub const HAZE_SCALE: f32 = 10.0;
/// The shadow cascades: from here, the first one's far edge, and their
/// overlap (Gates' `CASCADE_*`); how far they reach and how many is the
/// graphics tier's (`quality::Tier`).
pub const CASCADE_MIN_M: f32 = 0.2;
pub const CASCADE_FIRST_M: f32 = 12.0;
pub const CASCADE_OVERLAP: f32 = 0.2;
/// The sun disk's brightness, held under what the tone curve can roll off.
pub const SUN_DISK_INTENSITY: f32 = 1.6;
pub const SUN_DISK_PEAK: f32 = 2000.0;

/// The world's camera, whatever else marks it: what the graphics tier
/// (`quality::apply`) and the render scale (`render_scale.rs`) dress.
#[derive(Component)]
pub struct EyeCam;

/// The level's sun: `quality::apply` gives it the tier's cascades.
#[derive(Component)]
pub struct Sun;

/// The direction toward the sun.
pub fn to_sun() -> Vec3 {
    let (se, ce) = SUN_ELEVATION.sin_cos();
    let (sa, ca) = SUN_AZIMUTH.sin_cos();
    Vec3::new(sa * ce, se, ca * ce)
}

/// Earthlike air with Gates' corrected Mie term, thickened.
pub fn medium() -> ScatteringMedium {
    let mut m = ScatteringMedium::default();
    for term in m.terms.iter_mut() {
        if matches!(term.phase, bevy::pbr::PhaseFunction::Mie { .. }) {
            term.scattering = Vec3::splat(MIE_SCATTERING);
            term.absorption = Vec3::splat(MIE_ABSORPTION);
        }
    }
    m.with_density_multiplier(AIR_DENSITY)
}

fn sun_disk_intensity() -> f32 {
    let half = SunDisk::EARTH.angular_size * 0.5;
    let solid = core::f32::consts::PI * half * half;
    let per = lux::DIRECT_SUNLIGHT / solid * Exposure { ev100: DAY_EV100 }.exposure();
    if per <= 0.0 {
        SUN_DISK_INTENSITY
    } else {
        SUN_DISK_INTENSITY.min(SUN_DISK_PEAK / per)
    }
}

/// The camera, with its whole post stack.
pub fn camera<M: Component>(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    media: &mut Assets<ScatteringMedium>,
    marker: M,
) -> Entity {
    let fill = images.add(super::fill::cubemap(super::fill::FILL_FACE));
    let eye = commands
        .spawn((
            marker,
            EyeCam,
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                fov: FOV_DEG.to_radians(),
                // The gun in view sits a few centimetres out (viewmodel.rs).
                near: 0.02,
                far: FAR_M,
                ..default()
            }),
            (
                Exposure { ev100: DAY_EV100 },
                Tonemapping::TonyMcMapface,
                ColorGrading::default(),
            ),
            EnvironmentMapLight {
                diffuse_map: fill.clone(),
                specular_map: fill,
                intensity: super::fill::peak_lux(),
                rotation: Quat::IDENTITY,
                affects_lightmapped_mesh_diffuse: false,
            },
            AmbientLight {
                brightness: 0.0,
                ..default()
            },
            Msaa::Off,
            Transform::from_xyz(0.0, 2.0, 0.0),
        ))
        .id();
    // The sky this device can draw (`gpu.rs`): inserted where it can't,
    // the atmosphere's layout kills a WebGL2 module.
    if super::gpu::atmosphere() {
        let medium = media.add(medium());
        commands.entity(eye).insert((
            Atmosphere::earthlike(medium),
            AtmosphereSettings {
                aerial_view_lut_max_distance: FAR_M * HAZE_SCALE,
                scene_units_to_m: HAZE_SCALE,
                ..default()
            },
        ));
    } else {
        // No atmosphere: its air as a fog (Gates' browser haze), and its
        // sky baked into the cube `sky_for` hangs on the camera.
        commands.entity(eye).insert(super::sky::browser_haze(1.0));
    }
    eye
}

/// The map's exposure and sky fill (`sim::map::LightDef`: an interior
/// is exposed for its lamps, its fill turned down under its roof).
pub fn expose_for(light: &sim::map::LightDef, exposure: &mut Exposure, env: &mut EnvironmentMapLight) {
    exposure.ev100 = if light.ev100 > 0.0 { light.ev100 } else { DAY_EV100 };
    env.intensity = super::fill::peak_lux() * light.fill;
}

/// The sky for map `seed`: the noon cloud deck as a `Skybox`. Where the
/// atmosphere draws the sky (the desktop, the WebGPU module) its clear
/// texels are zero and the atmosphere composites the clouds itself, with
/// the same aerial perspective as everything else (`sky.rs`'s header);
/// where there is none (WebGL2) the sky is baked in behind them.
pub fn sky_for(commands: &mut Commands, images: &mut Assets<Image>, eye: Entity, seed: u64) {
    use super::sky::{cloud_cubemap_with, deck_gain, CLOUD_NITS};
    let backdrop = !super::gpu::atmosphere();
    let image = images.add(cloud_cubemap_with(seed, backdrop));
    commands.entity(eye).insert(bevy::core_pipeline::Skybox {
        image,
        brightness: CLOUD_NITS * deck_gain(backdrop),
        ..default()
    });
}

/// The sun at noon, `k` of its full strength. Its cascades here are a
/// placeholder: `quality::apply` gives a new [`Sun`] the tier's the frame
/// it appears.
pub fn sun(k: f32) -> impl Bundle {
    (
        Sun,
        DirectionalLight {
            color: Color::WHITE,
            illuminance: lux::DIRECT_SUNLIGHT * k,
            shadows_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: CASCADE_MIN_M,
            maximum_distance: super::quality::SHADOW_M_LADDER[0],
            first_cascade_far_bound: CASCADE_FIRST_M,
            overlap_proportion: CASCADE_OVERLAP,
        }
        .build(),
        SunDisk {
            intensity: sun_disk_intensity(),
            ..SunDisk::EARTH
        },
        Transform::default().looking_at(-to_sun(), Vec3::Y),
    )
}

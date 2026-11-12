//! The camera and the light, and their post stack: one owner for exposure,
//! tone, fill, sun and haze, so they can't drift apart.
//!
//! Ported from Gates' `crates/client/src/render/rig.rs` (MIT, © 2026
//! MoreRight DAO). The sky is the GPU's to decide (`gpu.rs`: Bevy's
//! atmosphere where there is compute, else the deck draws a sky and the
//! air is a fog); the post stack (SSAO, anti-aliasing, bloom) and the sun's
//! cascades are the graphics ladder's (`quality::apply` puts them on this
//! camera and every sun the frame they appear, and again whenever the tier
//! moves). Gates' day cycle is a map's mood here (`mood.rs`): the sun's
//! place, colour and strength, the exposure, the fill and the weather's
//! fog are set once a map, the same for every player on it.

use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{light_consts::lux, EnvironmentMapLight, SunDisk};
use bevy::pbr::{Atmosphere, AtmosphereSettings, DistanceFog, FogFalloff, ScatteringMedium};
use bevy::prelude::*;
use bevy::render::view::{ColorGrading, Msaa};

/// The sun at noon: elevation above the horizon and azimuth, radians
/// (Gates' `RIG_SUN_ELEVATION`, `RIG_SUN_AZIMUTH`).
pub const SUN_ELEVATION: f32 = 0.61;
pub const SUN_AZIMUTH: f32 = 2.35;
/// Sunlit exterior, 0.8 stop under Bevy's `Exposure::SUNLIGHT` (Gates
/// measured it against its reference frames).
pub const DAY_EV100: f32 = 14.2;
/// The field of view by default, as the settings name it: degrees across
/// a 16:9 screen, as PC shooters label it. Nearer Halo's 78 than a twitch
/// shooter's 103: a body across a room reads big enough to hit, and a run
/// doesn't blur past. The camera holds its height on any other shape
/// (Hor+): Bevy's `fov` is the height.
pub const FOV_DEG: f32 = 90.0;

/// The height, degrees, of a view `across` degrees wide on a 16:9 screen.
pub fn fov_tall(across: f32) -> f32 {
    2.0 * ((across.to_radians() * 0.5).tan() * 9.0 / 16.0).atan().to_degrees()
}

/// The width on a 16:9 screen, degrees, of a view `tall` degrees high.
pub fn fov_across(tall: f32) -> f32 {
    2.0 * ((tall.to_radians() * 0.5).tan() * 16.0 / 9.0).atan().to_degrees()
}
/// The camera's near plane, metres: the gun in view sits a few centimetres
/// out (`viewmodel.rs`, which holds every gun off it on screen).
pub const NEAR_M: f32 = 0.012;
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
/// Clear sky added behind the cloud deck, in deck units at full sun (Gates'
/// `SKY_LIFT`, measured against its reference frames: the atmosphere alone
/// read a slate sky, rgb(105, 127, 159) where theirs was rgb(153, 189, 231),
/// and no exposure brightens the sky without moving the ground). Added to
/// WebGL2's own clear sky too (`deck.rs`), which read that same slate.
pub const SKY_LIFT: f32 = 0.42;
/// How far that daylight's colour is pushed from grey toward the air's own
/// blue (1: the air's scatter as it is, haze and all, which lifted the sky
/// to a pale lavender; the references' is a deep clear blue).
pub const SKY_LIFT_SATURATION: f32 = 1.9;

/// The camera's two fills: the sky's (outdoors) and a hall's (a roofed
/// map, `fill::hall`), each a diffuse and a specular cube; [`expose_for`]
/// hands the map's to the camera.
#[derive(Component, Clone)]
pub struct Fills {
    pub sky: Handle<Image>,
    /// The sky's over snow (a snowed valley: `fill::snow_cubemap`).
    pub snow: Handle<Image>,
    /// By `Mood::hall`.
    pub hall: Vec<(Handle<Image>, Handle<Image>)>,
}

/// The world's camera, whatever else marks it: what the graphics tier
/// (`quality::apply`) and the render scale (`render_scale.rs`) dress.
#[derive(Component)]
pub struct EyeCam;

/// The level's sun: `quality::apply` gives it the tier's cascades.
#[derive(Component)]
pub struct Sun;

/// The direction toward the noon sun, the reference every mood is read
/// against (`mood::NOON`).
pub fn to_sun() -> Vec3 {
    to_sun_at(SUN_ELEVATION, SUN_AZIMUTH)
}

/// A unit vector at an elevation and a bearing, radians (bearing 0 is +Z,
/// toward +X): the sun's, the moon's (Gates' `to_sun_at`).
pub fn to_sun_at(elevation: f32, azimuth: f32) -> Vec3 {
    let (se, ce) = elevation.sin_cos();
    let (sa, ca) = azimuth.sin_cos();
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

/// [`sky::ground_air`] of [`medium`]: the same air every time, worked out
/// once (fog is asked for every frame on some screens, and the medium's
/// terms are on the heap).
///
/// [`sky::ground_air`]: super::sky::ground_air
fn ground_air() -> (Vec3, Vec3) {
    static AIR: std::sync::OnceLock<(Vec3, Vec3)> = std::sync::OnceLock::new();
    *AIR.get_or_init(|| super::sky::ground_air(&medium()))
}

/// `SunDisk::intensity` for a light of `illuminance` lux seen at `ev100`:
/// [`SUN_DISK_INTENSITY`], lowered where the disk would pass
/// [`SUN_DISK_PEAK`] (TAA's half-float history rounds it to a flicker past
/// ~4096, and bloom spreads that over the sky).
pub fn sun_disk_intensity(illuminance: f32, ev100: f32) -> f32 {
    let half = SunDisk::EARTH.angular_size * 0.5;
    let solid = core::f32::consts::PI * half * half;
    let per = illuminance / solid * Exposure { ev100 }.exposure();
    if per <= 0.0 {
        SUN_DISK_INTENSITY
    } else {
        SUN_DISK_INTENSITY.min(SUN_DISK_PEAK / per)
    }
}

/// The colour of the daylight added behind the deck ([`SKY_LIFT`]): the
/// air's own scatter at luminance 1, pushed [`SKY_LIFT_SATURATION`] from
/// grey and brought back to luminance 1.
pub fn sky_lift_chroma() -> Vec3 {
    let c = Vec3::from_array(super::sky::air_chroma());
    let s = (Vec3::ONE + (c - Vec3::ONE) * SKY_LIFT_SATURATION).max(Vec3::splat(0.02));
    s / super::fill::luminance(s.to_array()).max(1e-4)
}

/// The camera, with its whole post stack.
pub fn camera<M: Component>(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    media: &mut Assets<ScatteringMedium>,
    marker: M,
) -> Entity {
    let fill = images.add(super::fill::cubemap(super::fill::FILL_FACE));
    let fills = Fills {
        sky: fill.clone(),
        snow: images.add(super::fill::snow_cubemap(super::fill::FILL_FACE)),
        hall: super::fill::hall::PALETTES
            .iter()
            .map(|p| {
                let (d, s) = super::fill::hall::cubemaps(p);
                (images.add(d), images.add(s))
            })
            .collect(),
    };
    let eye = commands
        .spawn((
            marker,
            EyeCam,
            fills,
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                fov: fov_tall(FOV_DEG).to_radians(),
                near: NEAR_M,
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
    }
    // The air's fog, inserted ONCE: `DistanceFog` is a shader variant, and
    // adding or removing it re-specialises every pipeline. Where the
    // atmosphere draws it is the weather's alone, at zero in clear weather;
    // without one it is the atmosphere's own air (Gates' browser haze) with
    // the weather's on top. `fog_for` owns it from the first map on.
    commands
        .entity(eye)
        .insert(fog_for(&super::mood::NOON, super::gpu::atmosphere(), 0.0));
    eye
}

/// The map's exposure and sky fill: the mood's, under the map's own
/// `sim::map::LightDef` (an interior is exposed for its lamps, its fill
/// turned down under its roof, and its fill its hall's, not the sky's; a
/// `snowed` valley's sky fill is the one over snow).
pub fn expose_for(
    light: &sim::map::LightDef,
    mood: &super::mood::Mood,
    snowed: bool,
    fills: &Fills,
    exposure: &mut Exposure,
    env: &mut EnvironmentMapLight,
) {
    let roofed = light.sun <= 0.0;
    let (d, s) = if roofed {
        fills.hall[mood.hall].clone()
    } else if snowed {
        (fills.snow.clone(), fills.snow.clone())
    } else {
        (fills.sky.clone(), fills.sky.clone())
    };
    if env.diffuse_map != d {
        env.diffuse_map = d;
    }
    if env.specular_map != s {
        env.specular_map = s;
    }
    let ev = if light.ev100 > 0.0 { light.ev100 } else { mood.ev100 };
    if exposure.ev100 != ev {
        exposure.ev100 = ev;
    }
    let gain = if roofed { super::fill::hall::GAIN } else { 1.0 };
    let fill = super::fill::peak_lux() * light.fill * mood.fill * gain;
    if env.intensity != fill {
        env.intensity = fill;
    }
}

/// The air over a mood: the weather's fog (the mist less the `carried`
/// share the shafts' volume draws, `shafts.rs`; the rain's haze; the dust), the
/// colour it fades to, and its glow toward the sun. On a device with no
/// atmosphere the atmosphere's own air is the floor under it (the
/// `Atmospheric` falloff below), so every tier sees the same air; there the
/// glow is the mist's share of it, the sunward light the shafts would carry
/// where they are drawn.
pub fn fog_for(mood: &super::mood::Mood, atmosphere: bool, carried: f32) -> DistanceFog {
    use super::sky;
    let sigma = (mood.fog_sigma() - carried).max(0.0);
    let all = mood.fog_sigma();
    let c = sky::fog_rgb_dust(mood.light, mood.dark, all, mood.dust);
    let k = mood.exposure_gain();
    let glow = if atmosphere {
        sky::FOG_SUN_GLOW
    } else {
        let floor = super::fill::luminance(ground_air().0.to_array()) * HAZE_SCALE;
        sky::FOG_SUN_GLOW * all / (all + floor).max(f32::EPSILON)
    };
    let tint = sun_light(mood, atmosphere);
    DistanceFog {
        color: Color::linear_rgb(c[0] * k, c[1] * k, c[2] * k),
        falloff: if atmosphere {
            bevy::pbr::FogFalloff::Exponential { density: sigma }
        } else {
            let (extinction, inscattering) = ground_air();
            bevy::pbr::FogFalloff::Atmospheric {
                extinction: extinction * HAZE_SCALE + Vec3::splat(sigma),
                inscattering: inscattering * HAZE_SCALE + Vec3::splat(sigma),
            }
        },
        directional_light_color: Color::linear_rgba(tint.x, 0.95 * tint.y, 0.85 * tint.z, glow),
        directional_light_exponent: sky::FOG_SUN_EXPONENT,
    }
}

/// A hall's air (a roofed map, lit by its lamps): a faint haze in its palette's
/// colour, where the sky's fog would paint the hall in the horizon's blue.
/// Post-exposure, before the tonemap.
pub fn hall_haze(mood: &super::mood::Mood) -> DistanceFog {
    let p = super::fill::hall::PALETTES[mood.hall];
    DistanceFog {
        color: Color::linear_rgb(p.haze[0], p.haze[1], p.haze[2]),
        falloff: FogFalloff::ExponentialSquared {
            density: p.haze_density,
        },
        ..default()
    }
}

/// Where a grade's midtones start, a level (the mean of a colour's three
/// lanes, white 1): under any there is, so a frame has no shadows of Bevy's
/// to grade apart. At the foot of its midtones Bevy 0.18 blends them with
/// the highlights where it means the shadows (`sectional_color_grading`,
/// `tonemapping_shared.wgsl`), so shadows graded apart from the highlights
/// are a step at a tenth of white: every dim wall on a graded map in two
/// flat tones with a hard line between, Pithead's halls most of all.
const GRADE_MIDTONES_FROM: f32 = -0.2;

/// A mood's grade (`mood::Grade`) on a camera's.
pub fn grade(g: super::mood::Grade, c: &mut ColorGrading) {
    c.global.temperature = g.temperature;
    c.global.tint = g.tint;
    c.global.post_saturation = g.saturation;
    c.global.midtones_range = GRADE_MIDTONES_FROM..c.global.midtones_range.end;
    c.midtones.contrast = g.contrast;
    c.midtones.lift = g.lift;
}

/// A mood's grade on the eye's camera. The film's own shots grade their own
/// camera over it.
pub fn grade_for(mood: Res<super::mood::MoodNow>, mut cams: Query<&mut ColorGrading, With<EyeCam>>) {
    if !mood.is_changed() {
        return;
    }
    for mut c in &mut cams {
        grade(mood.mood.grade, &mut c);
    }
}

/// The sun's light at the ground where nothing else reddens it: the mood's
/// colour, and where no atmosphere draws (it reddens and dims the sun on
/// its way down itself, `pbr_lighting.wgsl`), the same air's transmittance
/// toward this sun over noon's (`sky::sun_transmittance`), so a low sun is
/// as warm and as dim on every tier, and noon is what it was.
pub fn sun_light(mood: &super::mood::Mood, atmosphere: bool) -> Vec3 {
    let rgb = Vec3::from_array(mood.sun_rgb);
    if atmosphere {
        return rgb;
    }
    let now = super::sky::sun_transmittance(mood.sun_elev.sin());
    let noon = super::sky::sun_transmittance(SUN_ELEVATION.sin());
    rgb * Vec3::from_array(core::array::from_fn(|c| now[c] / noon[c].max(1e-4)))
}

/// What the sun was spawned at, before any cloud over it: the light the
/// deck's shade dims (`cloud_shadow::sun`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct SunBase {
    pub lux: f32,
    pub disk: f32,
}

/// The map's sun, `k` of its mood's strength (`LightDef::sun`), seen at
/// `ev100`, with `tier`'s cascades from its first frame (`quality::apply`
/// keeps them the tier's after).
pub fn sun(k: f32, mood: &super::mood::Mood, tier: &super::quality::Tier, ev100: f32) -> impl Bundle {
    let lux = lux::DIRECT_SUNLIGHT * k * mood.sun;
    let rgb = sun_light(mood, tier.atmosphere);
    let disk = sun_disk_intensity(lux, ev100);
    (
        Sun,
        SunBase { lux, disk },
        DirectionalLight {
            color: Color::linear_rgb(rgb.x, rgb.y, rgb.z),
            illuminance: lux,
            shadows_enabled: true,
            ..default()
        },
        tier.cascades(),
        SunDisk {
            intensity: disk,
            ..SunDisk::EARTH
        },
        Transform::default().looking_at(-mood.to_sun(), Vec3::Y),
    )
}

/// The mood's fog on the camera, once a map, and again when the shafts'
/// share of the mist moves (`shafts.rs` carries it while it draws).
pub fn fog(
    mood: Res<super::mood::MoodNow>,
    tier: Res<super::quality::Tier>,
    shafts: Res<super::shafts::Shafts>,
    mut cam: Query<&mut DistanceFog, With<EyeCam>>,
) {
    if !mood.is_changed() && !tier.is_changed() && !shafts.is_changed() {
        return;
    }
    let want = if mood.roofed {
        hall_haze(&mood.mood)
    } else {
        fog_for(&mood.mood, tier.atmosphere, shafts.volume)
    };
    for mut f in cam.iter_mut() {
        *f = want.clone();
    }
}

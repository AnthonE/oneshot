//! Light shafts through a misty map's air: a mist lit by the sun and shaded
//! by what stands in it, so the walls and the pillars between the eye and a
//! low sun cut the air into beams (Cistern's morning, `mood::MIST`). After
//! Gates' `render/shafts.rs` (MIT, NOTICE).
//!
//! **Bevy's volumetric fog, and only a share of the mist.** `bevy_pbr`'s
//! volumetric pass raymarches a box (`FogVolume`) against the sun's shadow
//! map after the main pass, and composites as `src + (1 − α)·dst`. It is
//! dark past the shadow map and has no sky light, so the mist is SPLIT: the
//! volume carries the sun's share of the light a droplet receives
//! ([`sun_share`]) and the flat fogs (`DistanceFog`, the sky's fog layer)
//! keep the rest. Where the volume is lit it is normalised to the flat
//! fog's own colour seen side-on ([`volume`]), so a lit view reads as the
//! mist it was; in a shadow it adds nothing and absorbs its share, and that
//! difference is the shaft.
//!
//! **The box is the map.** Gates' box rides the eye across an island; an
//! arena is small, so the box is the map's bounds, set once a map, inside
//! the sun's last cascade from anywhere on it (`quality::MapShade`), and
//! its extinction is the flat fog's taken off it ([`Shafts::flat`]): inside
//! the box the two add up to the mist exactly, so a fighter across the map
//! is fogged as much with shafts as without. The march length is the hit's
//! view depth, not its distance (Bevy's), so toward the frame's edges the
//! volume is a little short; that is Bevy's and stays.
//!
//! **Where it runs**: wherever the atmosphere draws (the desktop, the
//! browser's WebGPU module) at the CLOUDS row's top rung
//! (`quality::Tier::shafts`). **Not under WebGL2, and not by choice**:
//! Bevy's volumetric pass is WebGPU-only on the web ("Requires using WebGPU
//! on Wasm builds"), and a march of the shadow map would be a render node of
//! our own reading Bevy's private shadow textures. The best WebGL2 draws is
//! the same mist's sunward glow (`rig::fog_for`): the brightening toward the
//! sun a forward-scattering mist gives, unshadowed, so no beams. The mist
//! itself is the same on every tier, so no rung sees a fighter another
//! doesn't.

use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::camera::Exposure;
use bevy::light::{FogVolume, VolumetricFog, VolumetricLight};
use bevy::prelude::*;

use super::mood::{Mood, MoodNow};
use super::quality::Tier;
use super::rig::{EyeCam, Sun, SunBase};

/// Henyey–Greenstein asymmetry of the volume's droplets: ×3.3 into the
/// light and ×0.5 with it behind ([`volume`] normalises side-on), so the
/// beams are strongest where they are looked for and a fog bank is not a
/// lamp.
pub const SHAFT_G: f32 = 0.3;
/// Raymarch steps per pixel at most, and the step the march aims for,
/// metres: the map's box is marched in [`steps`].
pub const SHAFT_STEPS: u32 = 64;
pub const SHAFT_STEP_M: f32 = 2.0;
/// How far each pixel's march start is jittered, metres, under TAA: about a
/// step, so the history resolves the stepping into a smooth beam. Zero
/// without TAA, where per-frame noise would shimmer instead.
pub const SHAFT_JITTER_M: f32 = 1.5;
/// How high the box reaches over the map's floor, metres: over its walls,
/// so the beams are seen coming over them.
pub const BOX_TOP_M: f32 = 20.0;
/// The sun's elevation at which the shafts carry their whole share (~6°).
pub const SHAFT_FULL_ELEV: f32 = 0.10;
/// The least of the air's transmittance toward the sun the volume's colour
/// is lifted against.
pub const SHAFT_MIN_T: f32 = 1e-3;

/// This frame's split, which the flat fogs read (`rig::fog`, `deck::drive`).
/// `volume == 0` is off: the flat fogs carry the whole mist.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Shafts {
    /// The mist's extinction the flat fogs keep, per metre.
    pub flat: f32,
    /// The volume's, per metre.
    pub volume: f32,
}

/// The volume's entity.
#[derive(Component)]
pub struct ShaftVolume;

fn smooth(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The sun's share of the light a droplet receives, `0..=1`: its direct
/// illuminance (`sun_lux`, after the air's transmittance) against the
/// scalar irradiance of the hemisphere fill around it (twice each half's
/// irradiance on a facing surface, `fill.rs`), at `fill` of its noon
/// strength. ~0.55 at a clear noon.
pub fn sun_share(sun_lux: f32, fill: f32) -> f32 {
    let lum = super::fill::luminance;
    let around = 2.0 * (lum(super::fill::sky_lux()) + lum(super::fill::bounce_lux())) * fill;
    let sun = sun_lux.max(0.0);
    if sun + around <= 0.0 {
        return 0.0;
    }
    sun / (sun + around)
}

/// How a mood's mist divides between the flat fogs and the box, for a sun
/// of `sun_lux` lux at the ground.
pub fn split(m: &Mood, sun_lux: f32) -> Shafts {
    let share = sun_share(sun_lux, m.fill) * smooth(0.035, SHAFT_FULL_ELEV, m.sun_elev);
    let volume = m.mist * share.clamp(0.0, 1.0);
    Shafts {
        flat: m.mist - volume,
        volume,
    }
}

/// The map's box: its bounds, from under its floor to [`BOX_TOP_M`].
pub fn box_transform(map: u8) -> Transform {
    let def = sim::maps::ALL.iter().find(|d| d.id == map);
    let (lo, hi) = def.map_or(((-20.0, -20.0), (20.0, 20.0)), |d| (d.lo, d.hi));
    let (bottom, top) = (-1.0, BOX_TOP_M);
    Transform::from_translation(Vec3::new(
        (lo.0 + hi.0) * 0.5,
        (top + bottom) * 0.5,
        (lo.1 + hi.1) * 0.5,
    ))
    .with_scale(Vec3::new(hi.0 - lo.0, top - bottom, hi.1 - lo.1))
}

/// March steps for a box of `scale`: its diagonal at [`SHAFT_STEP_M`].
pub fn steps(scale: Vec3) -> u32 {
    ((scale.length() / SHAFT_STEP_M).ceil() as u32).clamp(16, SHAFT_STEPS)
}

/// The volume for a box of `scale`, extinction `sigma` per metre, lit to the
/// flat fog's colour `rgb` (exposed linear) by a sun of `sun_lux` seen at
/// `exposure`, `t_sun` of which the air lets through
/// (`sky::sun_transmittance`).
///
/// The shader's in-scatter is `fog_color · light_tint · light_intensity ·
/// exposure · sun · HG(θ) · e^(−σR) · T` per unit of optical depth, `T` the
/// atmosphere's transmittance toward the sun. The flat fog's colour has no
/// `T` in it, so the tint is `1 / T` and the intensity `1 / (exposure · sun
/// · HG(90°))`: a lit volume seen side-on reads as the flat fog, brighter
/// looking into the light and darker with it behind. `e^(−σR)` is Bevy
/// dimming the sun as though it crossed the whole box (`R` its bounding
/// radius) to reach every point, which is undone.
pub fn volume(rgb: [f32; 3], sigma: f32, scale: Vec3, sun_lux: f32, exposure: f32, t_sun: [f32; 3]) -> FogVolume {
    let radius = 0.5 * scale.length();
    let lift = t_sun.map(|t| 1.0 / t.max(SHAFT_MIN_T));
    let g2 = SHAFT_G * SHAFT_G;
    let side = (1.0 - g2) / (4.0 * std::f32::consts::PI * (1.0 + g2) * (1.0 + g2).sqrt());
    let per_lux = 1.0 / (sun_lux * exposure * side).max(1e-9);
    FogVolume {
        fog_color: Color::linear_rgb(rgb[0], rgb[1], rgb[2]),
        density_factor: sigma,
        density_texture: None,
        density_texture_offset: Vec3::ZERO,
        // Droplets scatter and hardly absorb: extinction IS scattering.
        absorption: 0.0,
        scattering: 1.0,
        scattering_asymmetry: SHAFT_G,
        light_tint: Color::linear_rgb(lift[0], lift[1], lift[2]),
        light_intensity: per_lux * (sigma * radius).exp(),
    }
}

/// The volume at rest: drawn by nothing until the sun carries a
/// `VolumetricLight`. A unit box, never a zero one: Bevy inverts its
/// transform every frame.
pub fn idle_volume() -> FogVolume {
    FogVolume {
        density_factor: 0.0,
        ..default()
    }
}

/// Spawn the volume, idle.
pub fn setup(mut commands: Commands) {
    commands.spawn((ShaftVolume, idle_volume(), Transform::default()));
}

/// The camera's half, on the tier that draws shafts: no ambient (the volume
/// has no sky light, its ambient term is blind to density), and jitter only
/// where TAA resolves it.
pub fn camera_fog(temporal: bool, steps: u32) -> VolumetricFog {
    VolumetricFog {
        ambient_color: Color::WHITE,
        ambient_intensity: 0.0,
        jitter: if temporal { SHAFT_JITTER_M } else { 0.0 },
        step_count: steps,
    }
}

type Cam = (
    Entity,
    Option<&'static VolumetricFog>,
    Has<TemporalAntiAliasing>,
    &'static Exposure,
);

/// Light the box for the map's mist on the tier that draws it, and say what
/// the flat fogs keep. Writes only when the map, the tier, the split or the
/// exposure moves; the sun's light is a fresh entity each map.
#[allow(clippy::too_many_arguments)]
pub fn drive(
    mut commands: Commands,
    mood: Res<MoodNow>,
    tier: Res<Tier>,
    mut state: ResMut<Shafts>,
    mut written: Local<Option<(u8, Shafts, u32)>>,
    cam: Query<Cam, With<EyeCam>>,
    sun: Query<(Entity, Has<VolumetricLight>, &SunBase), With<Sun>>,
    mut vol: Query<(&mut Transform, &mut FogVolume), With<ShaftVolume>>,
) {
    let m = &mood.mood;
    let sun_now = sun.iter().next();
    let lux = sun_now.map_or(0.0, |(_, _, b)| b.lux);
    let t_sun = super::sky::sun_transmittance(m.sun_elev.sin());
    let cut = split(m, lux * super::fill::luminance(t_sun));
    let on = tier.shafts() && !mood.roofed && m.mist > 0.0 && lux > 0.0 && cut.volume > 0.0;
    let cut = if on {
        cut
    } else {
        Shafts {
            flat: m.mist,
            volume: 0.0,
        }
    };
    state.set_if_neq(cut);

    // Bevy draws the pass only while a light is volumetric.
    if let Some((e, lit, _)) = sun_now {
        if on && !lit {
            commands.entity(e).insert(VolumetricLight);
        } else if !on && lit {
            commands.entity(e).remove::<VolumetricLight>();
        }
    }
    let box_t = box_transform(mood.map);
    let Ok((e, fog, temporal, exposure)) = cam.single() else {
        return;
    };
    let want = camera_fog(temporal, steps(box_t.scale));
    match (on, fog) {
        (true, Some(f)) if f.jitter == want.jitter && f.step_count == want.step_count => {}
        (true, _) => {
            commands.entity(e).insert(want);
        }
        (false, Some(_)) => {
            commands.entity(e).remove::<VolumetricFog>();
        }
        (false, None) => {}
    }
    let key = (mood.map, cut, exposure.ev100.to_bits());
    if *written == Some(key) {
        return;
    }
    *written = Some(key);
    for (mut t, mut v) in vol.iter_mut() {
        if on {
            let f = super::rig::fog_for(m, true, 0.0);
            let c = LinearRgba::from(f.color);
            *t = box_t;
            *v = volume(
                [c.red, c.green, c.blue],
                cut.volume,
                box_t.scale,
                lux,
                exposure.exposure(),
                t_sun,
            );
        } else if v.density_factor != 0.0 {
            *v = idle_volume();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The misty morning carries a share of its mist in the box and keeps
    /// the rest flat, adding up to the mist; a clear map carries none.
    #[test]
    fn the_mist_is_split_and_conserved() {
        let m = super::super::mood::MIST;
        let lux = bevy::light::light_consts::lux::DIRECT_SUNLIGHT * m.sun;
        let s = split(&m, lux * 0.8);
        assert!(s.volume > 0.0 && s.flat > 0.0);
        assert!((s.volume + s.flat - m.mist).abs() < 1e-7);
        assert_eq!(split(&super::super::mood::NOON, lux).volume, 0.0);
        // The box is inside the last cascade from anywhere on the map.
        let t = box_transform(2);
        let shade = super::super::quality::MapShade::of(sim::maps::ALL[2]);
        assert!(t.scale.length() < shade.floor_m);
    }
}

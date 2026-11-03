//! The gun in our own view, after `web/src/art.ts`'s `Viewmodel`: Gates'
//! hold (`VIEWMODEL_HOLD`), a kick on each of our shots, a bob with our
//! speed, a dip through a reload, and a muzzle flash that lights the world
//! around us. Hidden when dead, at the wheel or behind a scope.
//!
//! Drawn by the world's camera, not a second one: the gun is shrunk by
//! [`SHRINK`] and brought that much closer to the eye, which leaves the
//! picture unchanged and puts all of it nearer than any wall can be (a
//! body's radius), so it never pokes through one. It is lit and shadowed
//! by the world as everything else is.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use sim::fighter::Fighter;

use super::guns::{spawn_posed, view_basis, Models};

/// How much smaller, and nearer, the gun is than it looks.
const SHRINK: f32 = 0.25;
/// Gates' `VIEWMODEL_HOLD`: where the gun sits in view space, as it looks.
const HOLD: Vec3 = Vec3::new(0.3, -0.29, -0.5);
/// The flash's peak, in lumens: a few thousand lux on a wall a metre off,
/// enough to read against noon.
const FLASH_LM: f32 = 400_000.0;
const FLASH_RANGE_M: f32 = 6.0;

#[derive(Component, Default)]
pub struct ViewGun {
    arm: u8,
    shots: u32,
    kick: f32,
    bob: f32,
    flash: f32,
}

#[derive(Component)]
pub struct Flash;

/// The hold and the flash, on the camera.
pub fn spawn(commands: &mut Commands, eye: Entity) {
    commands.spawn((
        ViewGun::default(),
        Transform::from_translation(HOLD * SHRINK).with_scale(Vec3::splat(SHRINK)),
        Visibility::Hidden,
        ChildOf(eye),
    ));
    commands.spawn((
        Flash,
        PointLight {
            intensity: 0.0,
            range: FLASH_RANGE_M,
            color: Color::srgb(1.0, 0.75, 0.4),
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.3, -0.2, -1.0),
        ChildOf(eye),
    ));
}

/// What the frame knows about us.
pub struct Own<'a> {
    pub f: &'a Fighter,
    /// Ground speed, m/s.
    pub speed: f32,
    pub shown: bool,
}

/// Once a frame.
pub fn update(
    commands: &mut Commands,
    models: &Models,
    own: Own,
    dt: f32,
    hold: &mut Query<(Entity, &mut ViewGun, &mut Transform, &mut Visibility)>,
    flash: &mut Query<&mut PointLight, With<Flash>>,
) {
    let Ok((e, mut v, mut t, mut vis)) = hold.single_mut() else {
        return;
    };
    let f = own.f;
    let arm = f.gun().arm;
    if arm != v.arm {
        commands.entity(e).despawn_related::<Children>();
        if let Some((_, scene)) = spawn_posed(commands, models, arm, view_basis(), e) {
            commands.entity(scene).observe(no_shadows);
        }
        v.arm = arm;
    }
    // A shot of ours since the last frame: kick and flash.
    if f.shots != v.shots {
        if f.shots > v.shots {
            v.kick = 1.0;
            v.flash = 1.0;
        }
        v.shots = f.shots;
    }
    *vis = if own.shown {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    v.kick = (v.kick - dt * 9.0).max(0.0);
    v.flash = (v.flash - dt * 25.0).max(0.0);
    v.bob += dt * own.speed * 1.6;
    let sway = (own.speed / 7.0).min(1.0);
    let whole = sim::consts::ticks(sim::arms::arm(arm).reload_ms).max(1) as f32;
    let reloading = f.reload as f32 / whole;
    let dip = if reloading > 0.0 {
        (reloading.min(1.0) * core::f32::consts::PI).sin()
    } else {
        0.0
    };
    let at = Vec3::new(
        HOLD.x + v.bob.sin() * 0.012 * sway,
        HOLD.y + v.bob.cos().abs() * 0.012 * sway - dip * 0.12,
        HOLD.z + v.kick * 0.06,
    );
    t.translation = at * SHRINK;
    t.rotation = Quat::from_euler(EulerRot::XYZ, v.kick * 0.09 - dip * 0.5, 0.06, dip * 0.4);
    if let Ok(mut l) = flash.single_mut() {
        l.intensity = if own.shown { v.flash * FLASH_LM } else { 0.0 };
    }
}

fn no_shadows(
    ev: On<SceneInstanceReady>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    mut commands: Commands,
) {
    for e in children.iter_descendants(ev.entity) {
        if meshes.contains(e) {
            commands.entity(e).insert(NotShadowCaster);
        }
    }
}

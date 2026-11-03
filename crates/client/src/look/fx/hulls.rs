//! Vehicles: dust off the wheels at speed, and smoke, then fire, from a
//! hull badly hurt (its health is sent with it). Drawn only.

use bevy::prelude::*;

use super::{Fx, Particle, Tone};

/// Dust from this speed, m/s, thicker up to `DUST_FULL_MPS`.
const DUST_MPS: f32 = 4.0;
const DUST_FULL_MPS: f32 = 14.0;
/// Smoke below this share of health (of 255), fire below the second.
const SMOKE_BELOW: u8 = 128;
const FIRE_BELOW: u8 = 50;
/// Puffs per second at most.
const PUFF_HZ: f32 = 14.0;

#[derive(Clone, Copy, Default)]
pub struct Hull {
    at: Option<Vec3>,
    acc: f32,
}

/// One vehicle this frame: `slot`, where, which way, its health of 255.
pub fn roll(commands: &mut Commands, fx: &mut Fx, slot: u8, at: Vec3, yaw: f32, health: u8, dt: f32) {
    let i = slot as usize % fx.hulls.len();
    let h = fx.hulls[i];
    let speed = match h.at {
        Some(p) if dt > 0.0 => (at - p).with_y(0.0).length() / dt,
        _ => 0.0,
    };
    let mut acc = h.acc + dt * PUFF_HZ;
    let fwd = Vec3::new(yaw.cos(), 0.0, yaw.sin());
    let side = Vec3::new(-fwd.z, 0.0, fwd.x);
    // A jump in place (a respawn, a teleport) isn't speed.
    let speed = if speed > 60.0 { 0.0 } else { speed };
    while acc >= 1.0 {
        acc -= 1.0;
        if speed > DUST_MPS {
            let k = ((speed - DUST_MPS) / (DUST_FULL_MPS - DUST_MPS)).min(1.0);
            let wheel = at - fwd * 1.5 + side * if fx.rand() < 0.5 { 1.05 } else { -1.05 };
            fx.emit(
                commands,
                wheel + Vec3::Y * 0.2,
                Particle::new(Tone::Dirt, fx.range(0.8, 1.3))
                    .vel(-fwd * speed * 0.15 + Vec3::Y * fx.range(0.4, 1.0) + fx.unit() * 0.4)
                    .drag(1.5)
                    .size(0.4, 1.6 + k)
                    .alpha(0.35 + 0.3 * k)
                    .fade_in(0.08),
            );
        }
        if health > 0 && health < SMOKE_BELOW {
            let top = at + Vec3::Y * 1.2 + fwd * 0.9;
            let dark = 1.0 - health as f32 / SMOKE_BELOW as f32;
            fx.emit(
                commands,
                top,
                Particle::new(Tone::Smoke, fx.range(1.4, 2.0))
                    .vel(Vec3::Y * fx.range(1.2, 2.0) - fwd * speed * 0.3)
                    .drag(0.6)
                    .size(0.3, 1.6 + dark)
                    .alpha(0.35 + 0.4 * dark)
                    .fade_in(0.1),
            );
            if health < FIRE_BELOW {
                fx.emit(
                    commands,
                    top,
                    Particle::new(Tone::Fire, fx.range(0.25, 0.4))
                        .vel(Vec3::Y * fx.range(1.0, 2.0))
                        .size(0.35, 0.1),
                );
            }
        }
    }
    fx.hulls[i] = Hull { at: Some(at), acc };
}

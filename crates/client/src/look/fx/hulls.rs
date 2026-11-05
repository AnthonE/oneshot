//! Vehicles: dust off the wheels at speed, and smoke, then fire, from a
//! hull badly hurt (its health is sent with it). Drawn only.

use bevy::prelude::*;

use super::table::{dust_tint, Layer, Matter};
use super::Fx;

/// Dust from this speed, m/s, thicker up to `DUST_FULL_MPS`.
const DUST_MPS: f32 = 4.0;
const DUST_FULL_MPS: f32 = 14.0;
/// Smoke below this share of health (of 255), fire below the second.
const SMOKE_BELOW: u8 = 128;
pub const FIRE_BELOW: u8 = 50;
/// Puffs per second at most.
const PUFF_HZ: f32 = 14.0;

#[derive(Clone, Copy, Default)]
pub struct Hull {
    at: Option<Vec3>,
    acc: f32,
}

/// One vehicle this frame: `slot`, where, which way, its health of 255.
pub fn roll(fx: &mut Fx, slot: u8, at: Vec3, yaw: f32, health: u8, dt: f32) {
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
    let dirt = dust_tint(Matter::Dirt);
    while acc >= 1.0 {
        acc -= 1.0;
        if speed > DUST_MPS {
            let k = ((speed - DUST_MPS) / (DUST_FULL_MPS - DUST_MPS)).min(1.0);
            let wheel = at - fwd * 1.5 + side * if fx.rand() < 0.5 { 1.05 } else { -1.05 };
            let back = (-fwd + Vec3::Y * 0.4).normalize();
            fx.emit_tint(Layer::Dust, 1, wheel + Vec3::Y * 0.2, back, dirt, 5.0 + 4.0 * k);
        }
        if health > 0 && health < SMOKE_BELOW {
            let top = at + Vec3::Y * 1.2 + fwd * 0.9;
            let dark = 1.0 - health as f32 / SMOKE_BELOW as f32;
            fx.emit_tint(Layer::Smoke, 1, top, Vec3::Y, Color::WHITE, 0.6 + 0.6 * dark);
            if health < FIRE_BELOW {
                fx.emit_tint(Layer::Fireball, 1, top, Vec3::Y, Color::WHITE, 0.5);
            }
        }
    }
    fx.hulls[i] = Hull { at: Some(at), acc };
}

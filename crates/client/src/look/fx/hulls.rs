//! Vehicles: dust off the wheels at speed (a one-seater's lift blows its
//! own off the dirt under it), and smoke, then fire, from a hull badly hurt
//! (its health is sent with it); what `look/truck.rs` asks
//! for as it drives: tyre tracks, a landing's dust, a scrape's sparks, the
//! exhaust, a wreck's husk smouldering. Drawn only.

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
                engine_sparks(fx, top);
            }
        }
    }
    fx.hulls[i] = Hull { at: Some(at), acc };
}

/// A one-seater's lift blows dust off dirt from this near it, metres, and
/// how often at rest (a share of the puffs it blows at full speed).
const HOVER_DUST_M: f32 = 3.2;
const HOVER_IDLE: f32 = 0.18;

/// One one-seater this frame (`look/craft.rs`): `slot`, where, which way,
/// how far its hull reaches aft and to each side, its health of 255. Its
/// lift blows the dust off the dirt under it while it is low, a wake of it
/// the faster it goes; and a hull badly hurt smokes, then burns, as a
/// truck's.
#[allow(clippy::too_many_arguments)]
pub fn hover(fx: &mut Fx, map: &sim::map::Map, slot: u8, at: Vec3, yaw: f32, half: (f32, f32), health: u8, dt: f32) {
    let i = slot as usize % fx.hulls.len();
    let h = fx.hulls[i];
    let speed = match h.at {
        Some(p) if dt > 0.0 => (at - p).with_y(0.0).length() / dt,
        _ => 0.0,
    };
    let speed = if speed > 60.0 { 0.0 } else { speed };
    let mut acc = h.acc + dt * PUFF_HZ;
    let fwd = Vec3::new(yaw.cos(), 0.0, yaw.sin());
    let side = Vec3::new(-fwd.z, 0.0, fwd.x);
    let (matter, _, floor) = ground(map, at);
    let low = (1.0 - (at.y - floor) / HOVER_DUST_M).clamp(0.0, 1.0);
    let k = (speed / DUST_FULL_MPS).min(1.0);
    while acc >= 1.0 {
        acc -= 1.0;
        if health > 0 && matter == Matter::Dirt && fx.rand() < low * (HOVER_IDLE + (1.0 - HOVER_IDLE) * k) {
            // From under its after half, thrown out and back.
            let off = side * fx.range(-half.1, half.1) - fwd * fx.range(0.0, half.0);
            let from = Vec3::new(at.x + off.x, floor + 0.12, at.z + off.z);
            let out = (off.normalize_or(-fwd) * 0.7 - fwd * k + Vec3::Y * 0.4).normalize_or(Vec3::Y);
            fx.emit_tint(Layer::Dust, 1, from, out, dust_tint(matter), 2.5 + 4.5 * k);
        }
        if health > 0 && health < SMOKE_BELOW {
            let top = at + Vec3::Y * 0.9 - fwd * half.0 * 0.6;
            let dark = 1.0 - health as f32 / SMOKE_BELOW as f32;
            fx.emit_tint(Layer::Smoke, 1, top, Vec3::Y, Color::WHITE, 0.5 + 0.5 * dark);
            if health < FIRE_BELOW {
                fx.emit_tint(Layer::Fireball, 1, top, Vec3::Y, Color::WHITE, 0.4);
                engine_sparks(fx, top);
            }
        }
    }
    fx.hulls[i] = Hull { at: Some(at), acc };
}

/// The ground's matter and its normal under `at`: dirt on a terrain, else
/// the boxes' concrete.
fn ground(map: &sim::map::Map, at: Vec3) -> (Matter, Vec3, f32) {
    let floor = map.floor(at.x, at.z, 0.2, at.y + 0.6);
    let on_ground = map.terrain.is_some() && (floor - map.ground(at.x, at.z)).abs() < 0.05;
    if on_ground {
        let n = map.ground_normal(at.x, at.z);
        (Matter::Dirt, Vec3::new(n.x, n.y, n.z), floor)
    } else {
        (Matter::Concrete, Vec3::Y, floor)
    }
}

/// A tyre's track under `wheel` along `along` (a skid, hard braking, a
/// landing), and the earth it throws up.
pub fn tread(fx: &mut Fx, map: &sim::map::Map, wheel: Vec3, along: Vec3, k: f32) {
    let (matter, up, floor) = ground(map, wheel);
    if wheel.y - floor > 0.6 {
        return;
    }
    let at = Vec3::new(wheel.x, floor, wheel.z);
    fx.marks
        .lay_along(map, super::marks::Kind::Tread, TREAD_M, at, up, matter, Some(along));
    if matter == Matter::Dirt && fx.rand() < 0.6 * k {
        let back = (-along + up * 0.6).normalize_or(up);
        fx.emit_tint(Layer::Dust, 1, at + up * 0.15, back, dust_tint(matter), 3.0 + 3.0 * k);
    }
}

/// A tread mark's length, metres.
pub const TREAD_M: f32 = 0.85;

/// Down on its wheels from a height, `k` of the hardest: dust rolled out
/// from under each wheel, grit thrown, a track where each one struck.
pub fn touchdown(fx: &mut Fx, map: &sim::map::Map, wheels: &[Vec3; 4], fwd: Vec3, k: f32) {
    for (i, &w) in wheels.iter().enumerate() {
        let (matter, up, floor) = ground(map, w);
        let at = Vec3::new(w.x, floor, w.z);
        let tint = dust_tint(matter);
        for _ in 0..(1.0 + 2.0 * k) as usize {
            let a = fx.range(0.0, core::f32::consts::TAU);
            let out = Vec3::new(a.cos(), 0.15, a.sin());
            fx.emit_tint(Layer::DustCloud, 1, at + out * 0.3 + up * 0.2, out, tint, 2.5 + 3.5 * k);
        }
        fx.emit(Layer::Grit, (3.0 + 6.0 * k) as usize, at + up * 0.1, up, matter);
        if i >= 2 || k > 0.5 {
            fx.marks
                .lay_along(map, super::marks::Kind::Tread, TREAD_M, at, up, matter, Some(fwd));
        }
    }
}

/// Its steel meeting a wall, `k` of the hardest, at `at`, the wall's way
/// back toward the truck `away`: sparks off the metal, flecks of paint.
pub fn scrape(fx: &mut Fx, at: Vec3, away: Vec3, k: f32) {
    let n = (6.0 + 24.0 * k) as usize;
    fx.emit(Layer::Sparks, n, at, away, Matter::Hull);
    fx.emit(Layer::Flash, 1, at, away, Matter::Hull);
    fx.chips
        .throw(at, away, Matter::Hull, (2.0 + 5.0 * k) as usize, at.y - 1.5);
    fx.emit_tint(Layer::Dust, 1, at, away, dust_tint(Matter::Concrete), 3.0 + 3.0 * k);
}

/// Its tailpipe at `at`, pointing `back`, the throttle `k` of open: a puff.
pub fn exhaust(fx: &mut Fx, at: Vec3, back: Vec3, k: f32) {
    let dir = (back + Vec3::Y * 0.35).normalize_or(back);
    fx.emit_tint(
        Layer::GunSmoke,
        1,
        at,
        dir,
        Color::srgb(0.36, 0.35, 0.34),
        0.5 + 0.8 * k,
    );
}

/// A wreck's husk at `at`, `k` of the way from fresh to cold: the fire
/// down to a smoulder, black smoke going up off it, embers in it.
pub fn smolder(fx: &mut Fx, at: Vec3, k: f32) {
    let warm = 1.0 - k;
    let spread = Vec3::new(fx.range(-1.0, 1.0), 0.0, fx.range(-0.6, 0.6));
    if fx.rand() < 0.35 + 0.5 * warm {
        fx.emit_tint(
            Layer::Cloud,
            1,
            at + spread + Vec3::Y * 0.6,
            Vec3::Y,
            SMOLDER,
            0.6 + 0.8 * warm,
        );
    }
    if fx.rand() < 0.5 * warm {
        fx.emit_tint(
            Layer::Flame,
            1,
            at + spread * 0.7,
            Vec3::Y,
            Color::WHITE,
            0.3 + 0.4 * warm,
        );
    }
    if fx.rand() < 0.4 * warm {
        fx.emit_tint(Layer::Embers, 2, at + spread * 0.5, Vec3::Y, EMBER, 0.7);
    }
}

/// A husk's smoke: soot, a little lighter as it cools.
const SMOLDER: Color = Color::srgb(0.16, 0.15, 0.14);
const EMBER: Color = Color::srgb(1.0, 0.45, 0.1);

/// A hull burning on its wheels: sparks and embers out of its engine,
/// besides the smoke and fire `roll` already puts there.
pub fn engine_sparks(fx: &mut Fx, at: Vec3) {
    fx.emit_tint(Layer::Embers, 1, at, Vec3::Y, EMBER, 0.8);
    if fx.rand() < 0.25 {
        fx.emit_tint(Layer::Sparks, 3, at, Vec3::Y, Color::WHITE, 0.8);
    }
}

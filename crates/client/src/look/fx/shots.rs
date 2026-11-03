//! Gunfire. **Ours is drawn the frame its trigger breaks**, as the gun we
//! predict fires: its flash and smoke at the muzzle, its tracer down the
//! view (DESIGN §5; after Gates' `fx/gun.rs`). The server's word on that
//! shot arrives a round trip later and draws only what the round met.
//! Everyone else's is drawn off the event: a star flash, a tongue of fire
//! and smoke at their muzzle, a tracer down the line the round really took
//! (every round, or every few for an automatic), and what it met: sparks
//! and grit and a hole in a wall, dust in the ground, lightning off a
//! shield, a red mist off health, sparks off a hull.

use bevy::prelude::*;
use proto::EventState;
use sim::arms::{arm, ROCKET, SHOTGUN};
use sim::consts::{HEAD_Y, HIT_R};
use sim::fighter::Fighter;
use sim::geom::V3;

use super::table::{self, GunLook, Layer, Matter, Weapon};
use super::{Fx, Scene, MUZZLE_FLASH};
use crate::look::shake;

/// From a shooter's eye to their muzzle: forward, then down.
const MUZZLE_FWD_M: f32 = 0.75;
const MUZZLE_DOWN_M: f32 = 0.28;
/// Our own muzzle, in view space: ahead, right, down (where the gun in
/// view holds it), and how far ahead of it our tracer starts.
const OWN_MUZZLE: Vec3 = Vec3::new(0.12, -0.1, 0.7);
const OWN_TRACER_SKIP_M: f32 = 1.0;
/// Our shotgun's pellets drawn in a cone this wide (radians): the real
/// scatter is the server's secret, and its marks are where they went.
const PELLET_DRAW_RAD: f32 = 0.09;
/// Box looks drawn in metal (`level.rs::dress`).
const METAL_LOOKS: [u8; 4] = [1, 3, 4, 5];

fn v3(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

/// Someone's shot, off the server's event.
pub fn shot(fx: &mut Fx, scene: &Scene, e: &EventState, from: Vec3, to: Vec3) {
    let arm = e.c as u8;
    let look = table::gun(arm);
    let v = to - from;
    let len = v.length();
    if len < 0.05 {
        return;
    }
    let dir = v / len;
    let own = e.a == scene.me;
    let bit = 1u64 << (e.a % 64);
    let first = fx.flashed & bit == 0;
    fx.flashed |= bit;
    if own {
        // Drawn when it broke (`own`): only what it met is new.
        if arm != ROCKET {
            land(fx, scene, arm, from, dir, len, to);
        }
        return;
    }
    let muzzle = from + dir * MUZZLE_FWD_M - Vec3::Y * MUZZLE_DOWN_M;
    if first {
        flash(fx, muzzle, dir, &look);
    }
    if arm == ROCKET {
        if first {
            backblast(fx, muzzle, dir);
        }
        return;
    }
    let n = &mut fx.rounds[e.a as usize % 64];
    *n = n.wrapping_add(1);
    if (*n).is_multiple_of(look.every) {
        let start = muzzle + dir * 0.6;
        let d = (to - start).normalize_or(dir);
        table::tracer(&mut fx.glow, &look, start, d, start.distance(to));
    }
    if look.wake_s > 0.0 {
        table::wake(&mut fx.soft, muzzle, to, look.wake_s);
    }
    land(fx, scene, arm, from, dir, len, to);
}

/// Our own gun, this frame: each round it fired since the last, drawn now.
pub fn own(fx: &mut Fx, scene: &Scene, f: &Fighter, eye: Vec3, cam: Quat) {
    let shots = f.shots;
    if shots <= fx.own_shots || !f.alive || f.riding().is_some() {
        fx.own_shots = shots;
        return;
    }
    let rounds = (shots - fx.own_shots).min(3);
    fx.own_shots = shots;
    let id = f.gun().arm;
    let a = arm(id);
    let look = table::gun(id);
    let fwd = cam * Vec3::NEG_Z;
    let right = cam * Vec3::X;
    let up = cam * Vec3::Y;
    let muzzle = eye + fwd * OWN_MUZZLE.z + right * OWN_MUZZLE.x + up * OWN_MUZZLE.y;
    fx.emit_tint(
        Layer::Flash,
        look.stars as usize,
        muzzle,
        fwd,
        Color::WHITE,
        1.6 * look.flash_scale,
    );
    fx.emit(Layer::GunSmoke, 2, muzzle, fwd, Matter::Concrete);
    fx.flash(muzzle, MUZZLE_FLASH);
    // The stock's jolt: light for an automatic's every round, full for a
    // heavy single shot.
    let heavy = !a.auto || a.pellets > 1 || id == ROCKET;
    let k = if heavy { 0.6 } else { 0.3 };
    let side = fx.shake.rand() * shake::SHOT_SIDE_RAD;
    fx.shake.kick(Vec3::new(shake::SHOT_KICK_RAD * k, side * k, 0.0));
    fx.shake.add(shake::SHOT_TRAUMA * if heavy { 1.0 } else { 0.35 });
    if id == ROCKET {
        backblast(fx, muzzle, fwd);
        return;
    }
    let pellets = if id == SHOTGUN { 6 } else { rounds as usize };
    for k in 0..pellets {
        let dir = if id == SHOTGUN {
            let (sx, sy) = (fx.range(-1.0, 1.0), fx.range(-1.0, 1.0));
            (fwd + (right * sx + up * sy) * PELLET_DRAW_RAD).normalize()
        } else {
            fwd
        };
        // Down the view to the first wall, or as far as the gun reaches.
        let reach = scene.map.ray(v3(eye), v3(dir), a.range.max(30.0));
        let start = eye + dir * OWN_TRACER_SKIP_M + right * OWN_MUZZLE.x * 0.5 + up * OWN_MUZZLE.y * 0.5;
        if k == 0 || id == SHOTGUN {
            table::tracer(&mut fx.glow, &look, start, dir, (reach - OWN_TRACER_SKIP_M).max(0.0));
        }
        if look.wake_s > 0.0 {
            table::wake(&mut fx.soft, start, eye + dir * reach, look.wake_s);
        }
    }
}

/// The far end of a round's line: what it met, and what that throws.
fn land(fx: &mut Fx, scene: &Scene, arm: u8, from: Vec3, dir: Vec3, len: f32, to: Vec3) {
    if let Some((matter, away)) = met(scene, from, dir, len, to) {
        fx.impact(Weapon::of_arm(arm), matter, to, away);
    }
}

/// What a round met at `to`, and the way its effect is thrown: a surface's
/// normal, or back along the line off a body. None: its range ran out, or
/// the body was ours.
fn met(scene: &Scene, from: Vec3, dir: Vec3, len: f32, to: Vec3) -> Option<(Matter, Vec3)> {
    let t = scene.map.ray(v3(from), v3(dir), len + 0.1);
    if t <= len + 0.06 && t >= len - 0.3 {
        let g = scene.map.ground(to.x, to.z);
        if (to.y - g).abs() < 0.08 {
            let n = scene.map.ground_normal(to.x, to.z);
            // An arena's floor is concrete; a terrain is soil.
            let m = if scene.map.terrain.is_some() {
                Matter::Dirt
            } else {
                Matter::Concrete
            };
            return Some((m, Vec3::new(n.x, n.y, n.z)));
        }
        return Some(face(scene, to, dir));
    }
    for d in scene.others.iter().filter(|d| d.alive) {
        let (dx, dz) = (to.x - d.pos.x, to.z - d.pos.z);
        let up = to.y - d.pos.y;
        if dx * dx + dz * dz < (HIT_R + 0.3) * (HIT_R + 0.3) && (-0.2..HEAD_Y + 0.4).contains(&up) {
            let m = if d.shield > 0 { Matter::Shield } else { Matter::Flesh };
            return Some((m, -dir));
        }
    }
    for v in scene.vehicles {
        let c = Vec3::new(v.pos.x, v.pos.y + 1.0, v.pos.z);
        if c.distance_squared(to) < 3.4 * 3.4 {
            return Some((Matter::Hull, -dir));
        }
    }
    None
}

/// The box `at` lies on: its matter and the nearest face the round came at.
fn face(scene: &Scene, at: Vec3, dir: Vec3) -> (Matter, Vec3) {
    const E: f32 = 0.1;
    let mut best = (f32::MAX, -dir, Matter::Concrete);
    for (i, b) in scene.map.boxes.iter().enumerate() {
        if at.x < b.min.x - E
            || at.x > b.max.x + E
            || at.y < b.min.y - E
            || at.y > b.max.y + E
            || at.z < b.min.z - E
            || at.z > b.max.z + E
        {
            continue;
        }
        let matter = match scene.map.looks.get(i) {
            Some(l) if METAL_LOOKS.contains(l) => Matter::Metal,
            _ => Matter::Concrete,
        };
        for (d, n) in [
            ((at.x - b.min.x).abs(), Vec3::NEG_X),
            ((at.x - b.max.x).abs(), Vec3::X),
            ((at.y - b.min.y).abs(), Vec3::NEG_Y),
            ((at.y - b.max.y).abs(), Vec3::Y),
            ((at.z - b.min.z).abs(), Vec3::NEG_Z),
            ((at.z - b.max.z).abs(), Vec3::Z),
        ] {
            // A face the round came at, not one behind it.
            if d < best.0 && n.dot(dir) < 0.0 {
                best = (d, n, matter);
            }
        }
    }
    (best.2, best.1)
}

/// A third-person muzzle: a star flash, a tongue of fire, smoke, a light.
pub fn flash(fx: &mut Fx, at: Vec3, dir: Vec3, look: &GunLook) {
    fx.emit_tint(
        Layer::Flash,
        look.stars as usize,
        at,
        dir,
        Color::WHITE,
        3.0 * look.flash_scale,
    );
    fx.emit(Layer::Tongue, 1, at, dir, Matter::Concrete);
    fx.emit(Layer::GunSmoke, 2, at, dir, Matter::Concrete);
    fx.flash(
        at,
        super::FlashDef {
            lumens: look.light_lm,
            ..MUZZLE_FLASH
        },
    );
}

/// A rocket leaving the tube: fire out of the front, smoke out of both ends.
fn backblast(fx: &mut Fx, at: Vec3, dir: Vec3) {
    fx.emit_tint(Layer::Flash, 3, at + dir * 0.4, dir, Color::WHITE, 5.0);
    fx.emit_tint(Layer::Fireball, 3, at + dir * 0.3, dir, Color::WHITE, 0.6);
    fx.emit_tint(Layer::GunSmoke, 4, at - dir * 0.6, -dir, Color::WHITE, 4.0);
    fx.emit_tint(Layer::GunSmoke, 3, at + dir * 0.5, dir, Color::WHITE, 3.0);
    fx.flash(
        at,
        super::FlashDef {
            lumens: 300_000.0,
            range: 8.0,
            life: 0.1,
            ..MUZZLE_FLASH
        },
    );
}

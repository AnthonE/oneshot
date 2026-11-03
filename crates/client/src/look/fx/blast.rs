//! Explosions: a white core, a fireball, debris, smoke that climbs and
//! spreads, a ring along the ground and a scorch on it, a light; a wreck's
//! is bigger and leaves its hull burning. A blast near us shakes the
//! camera (the drawn camera only: never where we aim).

use bevy::prelude::*;
use sim::events::Cause;

use super::{glow, Fx, Particle, Scene, Shape, Tone};

/// The ring's reach, metres: about a blast's.
const RING_M: f32 = 5.0;
/// The light at its brightest, lumens, and how far it reaches.
const BLAST_LM: f32 = 3_000_000.0;
const BLAST_RANGE_M: f32 = 16.0;
const BLAST_LIGHT_S: f32 = 0.45;
/// Shake: at most this, falling to none this far off; and how fast it
/// settles (per second).
const SHAKE_MAX: f32 = 1.0;
const SHAKE_REACH_M: f32 = 28.0;
const SHAKE_SETTLE: f32 = 5.0;
/// A wreck burns this long.
const BURN_S: f32 = 4.5;

pub fn blast(commands: &mut Commands, fx: &mut Fx, scene: &Scene, at: Vec3, c: u16, heard: bool) {
    let d = at.distance(fx.eye);
    fx.shake = fx.shake.max(shake_at(d));
    // Behind a wall: felt, not seen.
    if heard {
        return;
    }
    let big = if c == Cause::Grenade as u16 { 0.85 } else { 1.0 };
    boom(commands, fx, scene, at, big);
}

/// A vehicle blowing up: a bigger blast, then its hull burns.
pub fn wreck(commands: &mut Commands, fx: &mut Fx, scene: &Scene, at: Vec3) {
    let at = at + Vec3::Y * 0.8;
    fx.shake = fx.shake.max(shake_at(at.distance(fx.eye)) * 1.3);
    boom(commands, fx, scene, at, 1.5);
    commands.spawn((Transform::from_translation(at), Burning { left: BURN_S, acc: 0.0 }));
}

fn boom(commands: &mut Commands, fx: &mut Fx, scene: &Scene, at: Vec3, big: f32) {
    fx.emit(commands, at, Particle::new(Tone::Hot, 0.12).size(1.2 * big, 4.5 * big));
    for _ in 0..14 {
        let mut v = fx.unit() * fx.range(4.0, 11.0) * big;
        v.y = v.y.abs() * 0.8 + 1.0;
        fx.emit(
            commands,
            at + v * 0.03,
            Particle::new(Tone::Fire, fx.range(0.3, 0.6))
                .vel(v)
                .drag(5.0)
                .gravity(-1.5)
                .size(fx.range(0.6, 1.0) * big, fx.range(1.8, 2.8) * big)
                .hold(0.2),
        );
    }
    for _ in 0..12 {
        let v = fx.around(Vec3::Y, 1.1) * fx.range(7.0, 16.0) * big;
        fx.emit(
            commands,
            at,
            Particle::new(Tone::Spark, fx.range(0.5, 1.1))
                .shape(Shape::Streak {
                    len: 0.05,
                    stretch: 0.02,
                })
                .vel(v)
                .gravity(14.0)
                .drag(0.6)
                .size(0.03, 0.015),
        );
    }
    for _ in 0..8 {
        let v = fx.unit() * fx.range(0.5, 2.0) + Vec3::Y * fx.range(1.0, 2.2);
        let off = fx.unit() * 0.8;
        fx.emit(
            commands,
            at + off,
            Particle::new(Tone::Smoke, fx.range(2.0, 3.2))
                .vel(v)
                .drag(0.8)
                .size(fx.range(1.0, 1.6), fx.range(3.0, 4.5) * big)
                .alpha(0.7)
                .fade_in(0.12),
        );
    }
    // On or near the ground: a ring, a scorch, dust thrown along it.
    let floor = scene.map.floor(at.x, at.z, 0.2, at.y + 0.5);
    if at.y - floor < 2.5 {
        let on_ground = (floor - scene.map.ground(at.x, at.z)).abs() < 0.05;
        let n = if on_ground {
            let g = scene.map.ground_normal(at.x, at.z);
            Vec3::new(g.x, g.y, g.z)
        } else {
            Vec3::Y
        };
        let base = Vec3::new(at.x, floor, at.z);
        fx.emit(
            commands,
            base + n * 0.06,
            Particle::new(Tone::Shock, 0.3)
                .shape(Shape::Flat { normal: n })
                .size(0.5, RING_M * 2.2 * big)
                .alpha(0.6),
        );
        let mut scorch = Particle::new(Tone::Mark, 25.0)
            .shape(Shape::Flat { normal: n })
            .size(2.6 * big, 2.6 * big)
            .alpha(0.9)
            .hold(0.7);
        scorch.keep = true;
        fx.emit(commands, base + n * 0.02, scorch);
        for k in 0..8 {
            let a = k as f32 / 8.0 * core::f32::consts::TAU + fx.range(0.0, 0.5);
            let out = Vec3::new(a.cos(), 0.15, a.sin());
            fx.emit(
                commands,
                base + out * 0.5 + Vec3::Y * 0.2,
                Particle::new(Tone::Dirt, fx.range(0.9, 1.4))
                    .vel(out * fx.range(5.0, 9.0) * big)
                    .drag(3.0)
                    .size(0.5, 2.0 * big)
                    .alpha(0.65)
                    .fade_in(0.05),
            );
        }
    }
    glow(
        commands,
        fx,
        at,
        BLAST_LM * big,
        BLAST_RANGE_M * big,
        Color::srgb(1.0, 0.7, 0.4),
        BLAST_LIGHT_S,
    );
}

/// A wreck's hull, burning: fire and black smoke a while.
#[derive(Component)]
pub struct Burning {
    left: f32,
    acc: f32,
}

/// Each burning hull's fire and smoke this frame.
#[allow(clippy::type_complexity)]
pub fn burn(
    commands: &mut Commands,
    fx: &mut Fx,
    dt: f32,
    q: &mut Query<(Entity, &mut Burning, &Transform), Without<Particle>>,
) {
    const EVERY_S: f32 = 0.08;
    for (e, mut b, t) in q.iter_mut() {
        b.left -= dt;
        if b.left <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        b.acc += dt;
        let fade = (b.left / BURN_S).min(1.0);
        while b.acc >= EVERY_S {
            b.acc -= EVERY_S;
            let at = t.translation + Vec3::new(fx.range(-1.2, 1.2), 0.0, fx.range(-0.6, 0.6));
            fx.emit(
                commands,
                at,
                Particle::new(Tone::Fire, fx.range(0.3, 0.5))
                    .vel(Vec3::Y * fx.range(1.5, 3.0))
                    .size(0.5 * fade + 0.2, 0.15)
                    .alpha(0.9 * fade),
            );
            fx.emit(
                commands,
                at + Vec3::Y * 0.6,
                Particle::new(Tone::Smoke, fx.range(2.2, 3.0))
                    .vel(Vec3::new(fx.range(-0.3, 0.3), fx.range(1.8, 2.6), fx.range(-0.3, 0.3)))
                    .drag(0.3)
                    .size(0.6, 3.0)
                    .alpha(0.75 * fade)
                    .fade_in(0.1),
            );
        }
    }
}

/// How hard a blast `d` metres off shakes us.
fn shake_at(d: f32) -> f32 {
    let k = (1.0 - d / SHAKE_REACH_M).max(0.0);
    SHAKE_MAX * k * k
}

/// The shake a frame later.
pub fn settle(s: f32, dt: f32) -> f32 {
    let s = s * (1.0 - SHAKE_SETTLE * dt).max(0.0);
    if s < 1e-3 {
        0.0
    } else {
        s
    }
}

/// The camera's nudge for shake `s` at time `t` (s): a turn and a move,
/// drawn only.
pub fn nudge(s: f32, t: f32) -> (Quat, Vec3) {
    if s <= 0.0 {
        return (Quat::IDENTITY, Vec3::ZERO);
    }
    let a = s * 0.025;
    let turn = Quat::from_euler(
        EulerRot::YXZ,
        a * (t * 47.0 + 1.0).sin(),
        a * (t * 53.0).sin(),
        a * 0.6 * (t * 37.0 + 2.0).sin(),
    );
    let m = s * 0.05;
    let shift = Vec3::new(
        m * (t * 61.0).sin(),
        m * (t * 43.0 + 0.5).sin(),
        m * (t * 57.0 + 1.5).sin(),
    );
    (turn, shift)
}

//! Gunfire: a muzzle's flash on whoever we see fire (ours is the gun in
//! view's), a bolt down each tracer round's line and the line it leaves, a
//! sniper round's wake, and at the far end what the round met: sparks and
//! dust and a mark on a wall or the ground, gold on a shield, red on
//! health, sparks off a hull.

use bevy::prelude::*;
use proto::EventState;
use sim::arms::{ROCKET, SNIPER};
use sim::consts::{HEAD_Y, HIT_R};
use sim::geom::V3;

use super::arms::{self, ArmFx};
use super::{glow, Fx, Particle, Scene, Shape, Tone};

/// Our own tracer starts this far out, past the gun.
const OWN_SKIP_M: f32 = 0.8;
/// From a shooter's eye to their muzzle: forward, then down.
const MUZZLE_FWD_M: f32 = 0.75;
const MUZZLE_DOWN_M: f32 = 0.28;
const GRAVITY: f32 = 9.8;

/// What a round met at the end of its line.
enum Met {
    Ground(Vec3),
    Wall(Vec3),
    Shield,
    Health,
    Hull,
    /// Nothing we can name: its range ran out, or it was us.
    Air,
}

pub fn shot(commands: &mut Commands, fx: &mut Fx, scene: &Scene, e: &EventState, from: Vec3, to: Vec3) {
    let arm = e.c as u8;
    let a = arms::of(arm);
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
    let muzzle = from + dir * MUZZLE_FWD_M - Vec3::Y * MUZZLE_DOWN_M;
    if !own && first {
        flash(commands, fx, muzzle, dir, &a);
    }
    if arm == ROCKET {
        if first {
            backblast(commands, fx, if own { from } else { muzzle }, dir);
        }
        return;
    }
    let n = &mut fx.rounds[e.a as usize % 64];
    *n = n.wrapping_add(1);
    let start = if own { from + dir * OWN_SKIP_M.min(len) } else { muzzle };
    if (*n).is_multiple_of(a.every) || arm == SNIPER {
        tracer(commands, fx, start, to, &a);
    }
    match met(scene, from, dir, len, to) {
        Met::Ground(n) => wall(commands, fx, to, n, dir, &a, true),
        Met::Wall(n) => wall(commands, fx, to, n, dir, &a, false),
        Met::Shield => shield(commands, fx, to, dir),
        Met::Health => hurt(commands, fx, to, dir),
        Met::Hull => hull(commands, fx, to, dir, &a),
        Met::Air => {}
    }
}

fn v3(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

fn met(scene: &Scene, from: Vec3, dir: Vec3, len: f32, to: Vec3) -> Met {
    let t = scene.map.ray(v3(from), v3(dir), len + 0.1);
    if t <= len + 0.06 && t >= len - 0.3 {
        let g = scene.map.ground(to.x, to.z);
        if (to.y - g).abs() < 0.08 {
            let n = scene.map.ground_normal(to.x, to.z);
            return Met::Ground(Vec3::new(n.x, n.y, n.z));
        }
        return Met::Wall(face(scene, to, dir));
    }
    for d in scene.others.iter().filter(|d| d.alive) {
        let (dx, dz) = (to.x - d.pos.x, to.z - d.pos.z);
        let up = to.y - d.pos.y;
        if dx * dx + dz * dz < (HIT_R + 0.3) * (HIT_R + 0.3) && (-0.2..HEAD_Y + 0.4).contains(&up) {
            return if d.shield > 0 { Met::Shield } else { Met::Health };
        }
    }
    for v in scene.vehicles {
        let c = Vec3::new(v.pos.x, v.pos.y + 1.0, v.pos.z);
        if c.distance_squared(to) < 3.4 * 3.4 {
            return Met::Hull;
        }
    }
    Met::Air
}

/// The face of the box `at` lies on: the nearest of its six.
fn face(scene: &Scene, at: Vec3, dir: Vec3) -> Vec3 {
    const E: f32 = 0.1;
    let mut best = (f32::MAX, -dir);
    scene.map.each_box_in((at.x - E, at.z - E), (at.x + E, at.z + E), |b| {
        if at.x < b.min.x - E
            || at.x > b.max.x + E
            || at.y < b.min.y - E
            || at.y > b.max.y + E
            || at.z < b.min.z - E
            || at.z > b.max.z + E
        {
            return;
        }
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
                best = (d, n);
            }
        }
    });
    best.1
}

/// A third-person muzzle flash: a burst, a tongue of fire, a light.
pub fn flash(commands: &mut Commands, fx: &mut Fx, at: Vec3, dir: Vec3, a: &ArmFx) {
    fx.emit(
        commands,
        at,
        Particle::new(Tone::Muzzle, 0.05).size(a.flash_m, a.flash_m * 0.5),
    );
    fx.emit(
        commands,
        at + dir * a.flash_m * 0.45,
        Particle::new(Tone::Muzzle, 0.04)
            .shape(Shape::Streak {
                len: a.flash_m * 1.1,
                stretch: 0.0,
            })
            .vel(dir * 0.01)
            .size(a.flash_m * 0.3, a.flash_m * 0.15),
    );
    glow(commands, fx, at, a.flash_lm, 6.0, Color::srgb(1.0, 0.75, 0.4), 0.06);
}

/// A rocket leaving the tube: a flash and smoke out of both ends.
fn backblast(commands: &mut Commands, fx: &mut Fx, at: Vec3, dir: Vec3) {
    fx.emit(commands, at + dir * 0.6, Particle::new(Tone::Fire, 0.08).size(0.9, 0.4));
    for k in 0..6 {
        let back = k % 2 == 0;
        let d = fx.around(if back { -dir } else { dir }, 0.35);
        let v = d * fx.range(2.0, 5.0) + Vec3::Y * 0.4;
        fx.emit(
            commands,
            at + d * 0.5,
            Particle::new(Tone::Smoke, fx.range(0.9, 1.4))
                .vel(v)
                .drag(2.5)
                .size(0.3, 1.3)
                .alpha(0.5)
                .fade_in(0.1),
        );
    }
    glow(commands, fx, at, 300_000.0, 8.0, Color::srgb(1.0, 0.6, 0.3), 0.1);
}

/// A bolt running down the line, and the line itself, briefly; a sniper
/// round leaves a wake of smoke.
fn tracer(commands: &mut Commands, fx: &mut Fx, start: Vec3, to: Vec3, a: &ArmFx) {
    let v = to - start;
    let len = v.length();
    if len < 0.1 {
        return;
    }
    let dir = v / len;
    let bolt = a.bolt_m.min(len);
    fx.emit(
        commands,
        start + dir * bolt * 0.5,
        Particle::new(a.tone, ((len - bolt) / a.bolt_mps).max(0.016))
            .shape(Shape::Streak {
                len: bolt,
                stretch: 0.0,
            })
            .vel(dir * a.bolt_mps)
            .size(a.width, a.width)
            .hold(0.8),
    );
    let rot = Quat::from_rotation_arc(Vec3::Z, dir);
    let mid = (start + to) * 0.5;
    let w = a.width * 0.6;
    fx.emit(
        commands,
        mid,
        Particle::new(a.tone, a.line_s)
            .shape(Shape::Fixed {
                rot,
                from: Vec3::new(w, w, len),
                to: Vec3::new(w * 0.4, w * 0.4, len),
            })
            .alpha(a.line_alpha),
    );
    if a.wake_s > 0.0 {
        fx.emit(
            commands,
            mid,
            Particle::new(Tone::Smoke, a.wake_s)
                .shape(Shape::Fixed {
                    rot,
                    from: Vec3::new(0.04, 0.04, len),
                    to: Vec3::new(0.4, 0.4, len),
                })
                .alpha(0.3)
                .fade_in(0.05),
        );
    }
}

/// Off a wall or the ground: sparks glancing off, a hot point, a puff of
/// dust or grit, a few clods off the ground, and a mark.
fn wall(commands: &mut Commands, fx: &mut Fx, at: Vec3, n: Vec3, dir: Vec3, a: &ArmFx, ground: bool) {
    let bounce = (dir - 2.0 * dir.dot(n) * n + n).normalize_or(n);
    let sparks = if ground { a.sparks / 2 } else { a.sparks };
    for _ in 0..sparks {
        let v = fx.around(bounce, 0.7) * fx.range(4.0, 11.0);
        fx.emit(
            commands,
            at + n * 0.02,
            Particle::new(Tone::Spark, fx.range(0.1, 0.28))
                .shape(Shape::Streak {
                    len: 0.03,
                    stretch: 0.012,
                })
                .vel(v)
                .gravity(GRAVITY)
                .drag(1.5)
                .size(0.014, 0.006),
        );
    }
    let k = a.puff_m / 0.45;
    fx.emit(
        commands,
        at + n * 0.03,
        Particle::new(Tone::Hot, 0.06).size(0.1 * k, 0.25 * k),
    );
    let tone = if ground { Tone::Dirt } else { Tone::Grit };
    for _ in 0..if ground { 2 } else { 1 } {
        let v = fx.around(n, 0.5) * fx.range(0.5, 1.2) + Vec3::Y * 0.3;
        fx.emit(
            commands,
            at + n * 0.08,
            Particle::new(tone, fx.range(0.6, 0.95))
                .vel(v)
                .drag(2.0)
                .size(a.puff_m * 0.3, a.puff_m * 1.2)
                .alpha(if ground { 0.7 } else { 0.55 })
                .fade_in(0.05),
        );
    }
    if ground {
        for _ in 0..3 {
            let v = fx.around(n, 0.6) * fx.range(2.0, 4.5);
            fx.emit(
                commands,
                at + n * 0.03,
                Particle::new(Tone::Dirt, fx.range(0.3, 0.5))
                    .shape(Shape::Streak {
                        len: 0.03,
                        stretch: 0.0,
                    })
                    .vel(v)
                    .gravity(GRAVITY)
                    .size(0.03, 0.025)
                    .alpha(0.9),
            );
        }
    }
    let mut mark = Particle::new(Tone::Mark, 12.0)
        .shape(Shape::Flat { normal: n })
        .size(a.mark_m, a.mark_m)
        .alpha(0.85)
        .hold(0.85);
    mark.keep = true;
    fx.emit(commands, at + n * 0.01, mark);
}

/// A shield taking a round: gold sparks thrown back at the shooter. The
/// body's own flare is `bodies.rs`'s, from the shield's drop.
fn shield(commands: &mut Commands, fx: &mut Fx, at: Vec3, dir: Vec3) {
    fx.emit(commands, at, Particle::new(Tone::Shield, 0.09).size(0.2, 0.55));
    for _ in 0..7 {
        let v = fx.around(-dir, 0.9) * fx.range(2.5, 6.5);
        fx.emit(
            commands,
            at,
            Particle::new(Tone::Shield, fx.range(0.12, 0.26))
                .shape(Shape::Streak {
                    len: 0.04,
                    stretch: 0.015,
                })
                .vel(v)
                .drag(3.0)
                .gravity(3.0)
                .size(0.012, 0.005),
        );
    }
}

/// A round into health: a dark red puff.
fn hurt(commands: &mut Commands, fx: &mut Fx, at: Vec3, dir: Vec3) {
    fx.emit(commands, at, Particle::new(Tone::Hot, 0.04).size(0.08, 0.14));
    for _ in 0..2 {
        let v = fx.around(-dir, 0.8) * fx.range(0.6, 1.6);
        fx.emit(
            commands,
            at,
            Particle::new(Tone::Hurt, fx.range(0.3, 0.45))
                .vel(v)
                .gravity(3.0)
                .drag(2.0)
                .size(0.1, 0.35)
                .alpha(0.75),
        );
    }
}

/// Off a hull: a shower of hot sparks.
fn hull(commands: &mut Commands, fx: &mut Fx, at: Vec3, dir: Vec3, a: &ArmFx) {
    fx.emit(commands, at, Particle::new(Tone::Hot, 0.06).size(0.12, 0.3));
    for _ in 0..a.sparks * 2 {
        let v = fx.around(-dir, 0.9) * fx.range(3.0, 9.0);
        fx.emit(
            commands,
            at,
            Particle::new(Tone::Spark, fx.range(0.15, 0.4))
                .shape(Shape::Streak {
                    len: 0.03,
                    stretch: 0.015,
                })
                .vel(v)
                .gravity(GRAVITY)
                .drag(1.0)
                .size(0.014, 0.006),
        );
    }
}

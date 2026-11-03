//! What's in flight, drawn with its model: a rocket with its exhaust's
//! glow and a trail of smoke, a frag with its fuse blinking.

use bevy::prelude::*;
use proto::ProjState;

use super::atlas::{GLOW, SHARD};
use super::pool::{Orient, Particle};
use super::table::{Layer, PINK};
use super::Fx;
use crate::look::guns::Models;

/// A rocket's trail: a puff every this many metres.
const PUFF_EVERY_M: f32 = 0.5;
/// The frag's fuse: blinks per second.
const BLINK_HZ: f32 = 5.0;

#[derive(Component)]
pub struct Flying {
    kind: u8,
    /// Where its last puff was.
    last: Vec3,
    /// A frag's fuse was lit last frame.
    lit: bool,
}

pub fn projectiles(
    commands: &mut Commands,
    fx: &mut Fx,
    models: &Models,
    proj: &[ProjState],
    t: f32,
    have: &mut Query<(Entity, &mut Flying, &mut Transform)>,
) {
    // SEEKER crystals are light, not models: drawn straight into the glow
    // pool from where they are this frame.
    for p in proj.iter().filter(|p| p.kind == CRYSTAL) {
        crystal(fx, p);
    }
    let proj: Vec<ProjState> = proj.iter().filter(|p| p.kind != CRYSTAL).copied().collect();
    let proj = &proj[..];
    let mut drawn: Vec<(Entity, u8)> = have.iter().map(|(e, f, _)| (e, f.kind)).collect();
    // Same count of each kind as last frame, in order: move them; else
    // respawn the lot (cheap: a handful at most).
    let same = drawn.len() == proj.len() && drawn.iter().zip(proj).all(|(d, p)| d.1 == p.kind);
    if !same {
        for (e, _) in drawn.drain(..) {
            commands.entity(e).despawn();
        }
        for p in proj {
            let rocket = p.kind == 0;
            let scene = if rocket { &models.rocket } else { &models.frag };
            let place = place(p);
            let mut e = commands.spawn((
                Flying {
                    kind: p.kind,
                    last: place.translation,
                    lit: false,
                },
                place,
                Visibility::default(),
            ));
            e.with_child((
                SceneRoot(scene.clone()),
                // Both models stand on their base, +Y up: centre them.
                Transform::from_xyz(0.0, if rocket { -0.3 } else { -0.08 }, 0.0),
            ));
            if rocket && fx.caps.lights > 2 {
                e.with_child((
                    Transform::from_xyz(0.0, -0.5, 0.0),
                    PointLight {
                        intensity: 200_000.0,
                        range: 7.0,
                        color: Color::srgb(1.0, 0.6, 0.3),
                        shadows_enabled: false,
                        ..default()
                    },
                ));
            }
        }
        return;
    }
    for ((e, _), p) in drawn.iter().zip(proj) {
        let Ok((_, mut f, mut tr)) = have.get_mut(*e) else {
            continue;
        };
        *tr = place(p);
        let at = tr.translation;
        if p.kind == 0 {
            let back = tr.rotation * Vec3::NEG_Y;
            // The exhaust: a white-hot core in an orange glow.
            fx.glow.spawn(Particle {
                left: 0.05,
                life: 0.05,
                pos: at + back * 0.45,
                size0: 0.35,
                size1: 0.15,
                c0: [14.0, 6.0, 1.6, 1.0],
                c1: [3.0, 0.8, 0.1, 0.0],
                cell: GLOW,
                ..Particle::default()
            });
            let gone = at.distance(f.last);
            if gone >= PUFF_EVERY_M {
                // A puff for each step it went, along the way.
                let n = ((gone / PUFF_EVERY_M) as usize).min(8);
                for k in 0..n {
                    let q = f.last.lerp(at, (k + 1) as f32 / n as f32) + back * 0.5;
                    fx.emit_tint(Layer::GunSmoke, 1, q, Vec3::Y, Color::WHITE, 4.0);
                }
                f.last = at;
            }
        } else {
            let lit = (t * BLINK_HZ).fract() < 0.5;
            if lit && !f.lit {
                fx.glow.spawn(Particle {
                    left: 0.1,
                    life: 0.1,
                    pos: at + Vec3::Y * 0.1,
                    size0: 0.12,
                    size1: 0.06,
                    c0: [10.0, 1.5, 0.8, 1.0],
                    c1: [2.0, 0.2, 0.1, 0.0],
                    cell: GLOW,
                    ..Particle::default()
                });
            }
            f.lit = lit;
        }
    }
}

/// `ProjState::kind` of a SEEKER crystal.
const CRYSTAL: u8 = 2;

/// One crystal this frame: a pink shard along its flight in a glow, and a
/// sparkle left behind.
pub fn crystal(fx: &mut Fx, p: &ProjState) {
    let at = Vec3::new(p.pos.x, p.pos.y, p.pos.z);
    let v = Vec3::new(p.vel.x, p.vel.y, p.vel.z);
    let one = |cell, size, c0, orient, vel, stretch, len_max| Particle {
        left: 0.001,
        life: 0.001,
        pos: at,
        vel,
        size0: size,
        size1: size,
        c0,
        c1: c0,
        cell,
        orient,
        stretch,
        len_max,
        ..Particle::default()
    };
    fx.glow.spawn(one(SHARD, 0.045, PINK, Orient::Stretch, v, 0.01, 0.36));
    fx.glow.spawn(one(
        GLOW,
        0.2,
        [3.0, 0.6, 2.0, 0.8],
        Orient::Billboard,
        Vec3::ZERO,
        0.0,
        0.0,
    ));
    let life = 0.25;
    let drift = Vec3::new(fx.range(-0.3, 0.3), fx.range(-0.3, 0.3), fx.range(-0.3, 0.3));
    fx.glow.spawn(Particle {
        left: life,
        life,
        pos: at - v * 0.01,
        vel: drift,
        size0: 0.02,
        size1: 0.005,
        c0: [5.0, 1.2, 3.4, 0.9],
        c1: [1.0, 0.2, 0.7, 0.0],
        cell: GLOW,
        ..Particle::default()
    });
}

fn place(p: &ProjState) -> Transform {
    let at = Vec3::new(p.pos.x, p.pos.y, p.pos.z);
    let v = Vec3::new(p.vel.x, p.vel.y, p.vel.z);
    let r = if p.kind == 0 && v.length_squared() > 1e-4 {
        Quat::from_rotation_arc(Vec3::Y, v.normalize())
    } else {
        Quat::IDENTITY
    };
    Transform::from_translation(at).with_rotation(r)
}

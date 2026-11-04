//! What's in flight. Things with weight are drawn with their models: a
//! rocket with its exhaust's glow and a trail of smoke, a frag or a
//! launcher's grenade with its fuse blinking, a sticky glowing blue. Light
//! is drawn as light, straight into the glow pool from where it is this
//! frame: an ion bolt's blue core and streak, a charged bolt's bigger one
//! with its light, the cannon's green ball, the SHARD GUN's pink crystals.
//!
//! No event says where a bolt or a shard ended (a hit is the two fighters'
//! business), so it is inferred, cosmetically: one that was flying last
//! frame and is gone now ended a frame's flight on. A shard that ended in
//! a body we see sticks there (`bodies.rs` glints it); anywhere else it
//! shatters. A bolt splashes. Drawing only.

use bevy::prelude::*;
use client_net::core::Drawn;
use proto::ProjState;
use sim::consts::{HEAD_Y, HIT_R};

use super::atlas::{GLOW, SHARD, STREAK};
use super::pool::{Orient, Particle};
use super::table::{Layer, PINK};
use super::{FlashDef, Fx};
use crate::look::guns::Models;

/// `ProjState::kind`, as `sim::world::ProjKind` numbers them.
pub const ROCKET: u8 = 0;
pub const FRAG: u8 = 1;
pub const BOLT: u8 = 2;
pub const CHARGE: u8 = 3;
pub const NEEDLE: u8 = 4;
pub const LOB: u8 = 5;
pub const CANNON: u8 = 6;
pub const STICKY: u8 = 7;

/// A rocket's trail: a puff every this many metres.
const PUFF_EVERY_M: f32 = 0.5;
/// A fuse: blinks per second.
const BLINK_HZ: f32 = 5.0;
/// An ion bolt's blue, a charged bolt's, the cannon's green: linear HDR.
const ION: [f32; 4] = [2.0, 5.5, 14.0, 1.0];
const ION_HALO: [f32; 4] = [0.5, 1.4, 4.0, 0.7];
const GREEN: [f32; 4] = [3.0, 12.0, 4.0, 1.0];
const GREEN_HALO: [f32; 4] = [0.8, 3.5, 1.0, 0.7];

/// Drawn with a model (the rest are light).
fn modelled(kind: u8) -> bool {
    matches!(kind, ROCKET | FRAG | LOB | STICKY)
}

#[derive(Component)]
pub struct Flying {
    kind: u8,
    /// Where its last puff was.
    last: Vec3,
    /// A fuse was lit last frame.
    lit: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn projectiles(
    commands: &mut Commands,
    fx: &mut Fx,
    models: &Models,
    proj: &[ProjState],
    others: &[Drawn],
    t: f32,
    dt: f32,
    have: &mut Query<(Entity, &mut Flying, &mut Transform)>,
) {
    for p in proj.iter().filter(|p| !modelled(p.kind)) {
        light(fx, p);
    }
    ended(fx, proj, others, t, dt);
    let proj: Vec<ProjState> = proj.iter().filter(|p| modelled(p.kind)).copied().collect();
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
            let rocket = p.kind == ROCKET;
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
        if p.kind == ROCKET {
            let back = tr.rotation * Vec3::NEG_Y;
            // The exhaust: a white-hot core in an orange glow.
            fx.glow.spawn(Particle {
                left: 0.05,
                life: 0.05,
                pos: (at + back * 0.45).into(),
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
            continue;
        }
        // A sticky glows blue the whole while; a fuse blinks red.
        if p.kind == STICKY {
            fx.glow.spawn(dot(at, 0.18, ION_HALO));
            fx.glow.spawn(dot(at, 0.06, ION));
        }
        let lit = (t * BLINK_HZ).fract() < 0.5;
        if lit && !f.lit {
            let c = if p.kind == STICKY { ION } else { [10.0, 1.5, 0.8, 1.0] };
            fx.glow.spawn(Particle {
                left: 0.1,
                life: 0.1,
                pos: (at + Vec3::Y * 0.1).into(),
                size0: 0.12,
                size1: 0.06,
                c0: c,
                c1: [c[0] * 0.2, c[1] * 0.2, c[2] * 0.2, 0.0],
                cell: GLOW,
                ..Particle::default()
            });
        }
        f.lit = lit;
    }
}

/// A glow that lives one frame.
fn dot(at: Vec3, size: f32, c: [f32; 4]) -> Particle {
    Particle {
        left: 0.001,
        life: 0.001,
        pos: at.into(),
        size0: size,
        size1: size,
        c0: c,
        c1: c,
        cell: GLOW,
        ..Particle::default()
    }
}

/// A streak along `v` that lives one frame.
fn streak(at: Vec3, v: Vec3, cell: u8, width: f32, len: f32, c: [f32; 4]) -> Particle {
    Particle {
        left: 0.001,
        life: 0.001,
        pos: at.into(),
        vel: v.into(),
        size0: width,
        size1: width,
        c0: c,
        c1: c,
        cell,
        orient: Orient::Stretch,
        stretch: len / v.length().max(1.0),
        len_max: len,
        ..Particle::default()
    }
}

/// One round of light this frame.
pub fn light(fx: &mut Fx, p: &ProjState) {
    let at = Vec3::new(p.pos.x, p.pos.y, p.pos.z);
    let v = Vec3::new(p.vel.x, p.vel.y, p.vel.z);
    match p.kind {
        NEEDLE => crystal(fx, p),
        BOLT => {
            fx.glow.spawn(dot(at, 0.3, ION_HALO));
            fx.glow.spawn(streak(at, v, STREAK, 0.05, 0.6, ION));
            fx.glow.spawn(dot(at, 0.08, [8.0, 10.0, 14.0, 1.0]));
            trail(fx, at, v, ION);
        }
        CHARGE => {
            fx.glow.spawn(dot(at, 0.7, ION_HALO));
            fx.glow.spawn(streak(at, v, STREAK, 0.12, 1.0, ION));
            fx.glow.spawn(dot(at, 0.18, [8.0, 10.0, 14.0, 1.0]));
            trail(fx, at, v, ION);
            fx.flash(
                at,
                FlashDef {
                    lumens: 150_000.0,
                    range: 6.0,
                    life: 0.08,
                    color: [0.4, 0.6, 1.0],
                },
            );
        }
        CANNON => {
            fx.glow.spawn(dot(at, 1.0, GREEN_HALO));
            fx.glow.spawn(dot(at, 0.35, GREEN));
            fx.glow.spawn(dot(at, 0.12, [9.0, 14.0, 9.0, 1.0]));
            trail(fx, at, v, GREEN);
            fx.flash(
                at,
                FlashDef {
                    lumens: 250_000.0,
                    range: 8.0,
                    life: 0.08,
                    color: [0.4, 1.0, 0.45],
                },
            );
        }
        _ => {}
    }
}

/// A spark left behind a round of light.
fn trail(fx: &mut Fx, at: Vec3, v: Vec3, c: [f32; 4]) {
    let life = 0.2;
    let drift = Vec3::new(fx.range(-0.3, 0.3), fx.range(-0.3, 0.3), fx.range(-0.3, 0.3));
    fx.glow.spawn(Particle {
        left: life,
        life,
        pos: (at - v * 0.01).into(),
        vel: drift.into(),
        size0: 0.025,
        size1: 0.005,
        c0: [c[0] * 0.6, c[1] * 0.6, c[2] * 0.6, 0.9],
        c1: [c[0] * 0.1, c[1] * 0.1, c[2] * 0.1, 0.0],
        cell: GLOW,
        ..Particle::default()
    });
}

/// One shard this frame: a pink sliver along its flight in a glow, and a
/// sparkle left behind.
pub fn crystal(fx: &mut Fx, p: &ProjState) {
    let at = Vec3::new(p.pos.x, p.pos.y, p.pos.z);
    let v = Vec3::new(p.vel.x, p.vel.y, p.vel.z);
    fx.glow.spawn(streak(at, v, SHARD, 0.045, 0.36, PINK));
    fx.glow.spawn(dot(at, 0.2, [3.0, 0.6, 2.0, 0.8]));
    trail(fx, at, v, [5.0, 1.2, 3.4, 0.9]);
}

/// Rounds of light that were flying last frame and are gone now: where they
/// ended, and what that looks like.
fn ended(fx: &mut Fx, proj: &[ProjState], others: &[Drawn], t: f32, dt: f32) {
    let last = std::mem::take(&mut fx.last_light);
    let mut taken = vec![false; proj.len()];
    for was in &last {
        let wv = Vec3::new(was.vel.x, was.vel.y, was.vel.z);
        let wp = Vec3::new(was.pos.x, was.pos.y, was.pos.z);
        let ahead = wp + wv * dt;
        let reach = wv.length() * dt * 2.0 + 0.5;
        // Still flying: the nearest of its kind and owner, near where it
        // would be now.
        let still = proj
            .iter()
            .enumerate()
            .filter(|(i, p)| !taken[*i] && p.kind == was.kind && p.owner == was.owner)
            .map(|(i, p)| (i, Vec3::new(p.pos.x, p.pos.y, p.pos.z).distance(ahead)))
            .filter(|(_, d)| *d < reach)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = still {
            taken[i] = true;
            continue;
        }
        let at = wp + wv * (dt * 0.5);
        match was.kind {
            NEEDLE => match in_body(others, at) {
                Some(slot) => {
                    super::bodies::needle_in(fx, slot, t);
                    super::table::shatter(&mut fx.glow, at, 3, 1.5);
                }
                None => super::table::shatter(&mut fx.glow, at, 8, 3.0),
            },
            BOLT | CHARGE => {
                let big = if was.kind == CHARGE { 2.0 } else { 1.0 };
                splash(fx, at, -wv.normalize_or(Vec3::Y), big);
            }
            // The cannon's ball ends in a Blast event (`blast.rs`).
            _ => {}
        }
    }
    fx.last_light.clear();
    fx.last_light.extend(proj.iter().filter(|p| !modelled(p.kind)).copied());
}

/// The fighter whose body `at` is in, if we draw them.
fn in_body(others: &[Drawn], at: Vec3) -> Option<u8> {
    others
        .iter()
        .filter(|d| d.alive)
        .find(|d| {
            let (dx, dz) = (at.x - d.pos.x, at.z - d.pos.z);
            let up = at.y - d.pos.y;
            dx * dx + dz * dz < (HIT_R + 0.35) * (HIT_R + 0.35) && (-0.2..HEAD_Y + 0.4).contains(&up)
        })
        .map(|d| d.slot)
}

/// An ion bolt's end: a blue crack of light, sparks thrown back.
pub fn splash(fx: &mut Fx, at: Vec3, back: Vec3, big: f32) {
    fx.glow.spawn(Particle {
        left: 0.1,
        life: 0.1,
        pos: at.into(),
        size0: 0.25 * big,
        size1: 0.6 * big,
        c0: ION,
        c1: [0.3, 0.8, 2.0, 0.0],
        cell: GLOW,
        ..Particle::default()
    });
    fx.emit_tint(Layer::Arcs, (2.0 * big) as usize, at, back, Color::WHITE, 0.6 * big);
    for _ in 0..(6.0 * big) as usize {
        let d = (back + fx.unit() * 0.9).normalize_or(back);
        let life = fx.range(0.12, 0.25);
        let speed = fx.range(2.0, 5.0);
        fx.glow.spawn(Particle {
            left: life,
            life,
            pos: at.into(),
            vel: (d * speed).into(),
            gravity: 4.0,
            drag: 3.0,
            size0: 0.008,
            size1: 0.004,
            c0: ION,
            c1: [0.3, 0.8, 2.0, 0.0],
            cell: STREAK,
            orient: Orient::Stretch,
            stretch: 0.02,
            ..Particle::default()
        });
    }
}

fn place(p: &ProjState) -> Transform {
    let at = Vec3::new(p.pos.x, p.pos.y, p.pos.z);
    let v = Vec3::new(p.vel.x, p.vel.y, p.vel.z);
    let r = if p.kind == ROCKET && v.length_squared() > 1e-4 {
        Quat::from_rotation_arc(Vec3::Y, v.normalize())
    } else {
        Quat::IDENTITY
    };
    Transform::from_translation(at).with_rotation(r)
}

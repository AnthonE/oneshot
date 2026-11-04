//! Explosions: star flashes, a fireball, sparks and debris flung out,
//! smoke that climbs and spreads, a ring along the ground and a scorch on
//! it, a light; a wreck's is bigger and leaves its hull burning. A blast
//! near us shakes the drawn camera (`shake.rs`; never where we aim).

use bevy::prelude::*;
use sim::events::Cause;

use super::table::{Layer, Matter, Weapon};
use super::{Fx, Scene, BLAST_FLASH};
use crate::look::shake;

/// A wreck burns this long.
const BURN_S: f32 = 4.5;
/// Fire and smoke puffs a burning hull throws a second.
const BURN_HZ: f32 = 12.0;

/// A hull burning.
#[derive(Clone, Copy, Default)]
pub struct Burn {
    at: Vec3,
    left: f32,
    acc: f32,
}

pub fn blast(fx: &mut Fx, scene: &Scene, at: Vec3, c: u16, heard: bool) {
    let eye = fx.eye;
    fx.shake.add_at(1.0, at, eye, shake::BLAST_FULL_M, shake::BLAST_ZERO_M);
    // Behind a wall: felt, not seen.
    if heard {
        return;
    }
    // The SHARD GUN's shards going off together; the cannon's ball.
    if c == sim::arms::SHARDS as u16 {
        super::bodies::burst_at(fx, scene.others, at);
        burst(fx, at);
        return;
    }
    if c == sim::arms::CANNON as u16 {
        plasma(fx, at);
        return;
    }
    let big = if c == Cause::Grenade as u16 { 0.85 } else { 1.0 };
    boom(fx, scene, at, big);
}

/// The ion cannon's ball bursting: green light, a green ring, sparks,
/// no fire or smoke.
fn plasma(fx: &mut Fx, at: Vec3) {
    let green = super::pool::Particle {
        left: 0.25,
        life: 0.25,
        pos: at.into(),
        size0: 1.0,
        size1: 4.0,
        c0: [3.0, 12.0, 4.0, 1.0],
        c1: [0.3, 1.5, 0.4, 0.0],
        cell: super::atlas::GLOW,
        ..Default::default()
    };
    fx.glow.spawn(green);
    fx.glow.spawn(super::pool::Particle {
        size0: 0.5,
        size1: 9.0,
        left: 0.35,
        life: 0.35,
        cell: super::atlas::RING,
        orient: super::pool::Orient::Flat,
        pos: (at - Vec3::Y * 0.4).into(),
        c0: [1.5, 6.0, 2.0, 0.8],
        c1: [0.2, 0.8, 0.3, 0.0],
        ..green
    });
    for _ in 0..24 {
        let d = fx.unit();
        let life = fx.range(0.3, 0.6);
        let speed = fx.range(4.0, 11.0);
        fx.glow.spawn(super::pool::Particle {
            left: life,
            life,
            pos: at.into(),
            vel: (d * speed).into(),
            gravity: 6.0,
            drag: 1.5,
            size0: 0.012,
            size1: 0.006,
            c0: [3.0, 12.0, 4.0, 1.0],
            c1: [0.3, 1.5, 0.4, 0.0],
            cell: super::atlas::STREAK,
            orient: super::pool::Orient::Stretch,
            stretch: 0.02,
            ..Default::default()
        });
    }
    let eye = fx.eye;
    fx.shake.add_at(0.8, at, eye, shake::BLAST_FULL_M, shake::BLAST_ZERO_M);
    fx.flash(
        at,
        super::FlashDef {
            lumens: 1_500_000.0,
            range: 14.0,
            life: 0.35,
            color: [0.4, 1.0, 0.45],
        },
    );
}

/// Shards in one body going off together: a pink star, shards
/// everywhere, a pink light. Glass, not fire.
fn burst(fx: &mut Fx, at: Vec3) {
    let pink = Color::LinearRgba(LinearRgba::rgb(2.4, 0.5, 1.6));
    for k in 0..3u8 {
        let roll = fx.range(0.0, core::f32::consts::TAU);
        fx.glow.spawn(super::pool::Particle {
            left: 0.12,
            life: 0.12,
            pos: at.into(),
            size0: 0.6,
            size1: 1.3,
            c0: super::table::PINK,
            c1: [1.5, 0.3, 1.0, 0.0],
            roll,
            cell: super::atlas::STAR + k,
            ..Default::default()
        });
    }
    fx.emit_tint(Layer::Zap, 2, at, Vec3::Y, pink, 6.0);
    super::table::shatter(&mut fx.glow, at, 40, 7.0);
    fx.emit_tint(Layer::GunSmoke, 6, at, Vec3::Y, pink, 5.0);
    fx.flash(
        at,
        super::FlashDef {
            lumens: 900_000.0,
            range: 9.0,
            life: 0.3,
            color: [1.0, 0.35, 0.8],
        },
    );
}

/// A vehicle blowing up: a bigger blast, then its hull burns.
pub fn wreck(fx: &mut Fx, scene: &Scene, at: Vec3) {
    let at = at + Vec3::Y * 0.8;
    let eye = fx.eye;
    fx.shake
        .add_at(1.0, at, eye, shake::BLAST_FULL_M * 1.5, shake::BLAST_ZERO_M * 1.3);
    boom(fx, scene, at, 1.5);
    let i = (0..fx.burning.len())
        .min_by(|&a, &b| fx.burning[a].left.total_cmp(&fx.burning[b].left))
        .unwrap_or(0);
    fx.burning[i] = Burn {
        at,
        left: BURN_S,
        acc: 0.0,
    };
}

fn boom(fx: &mut Fx, scene: &Scene, at: Vec3, big: f32) {
    // Its shock through the grass and the dry scrub (`foliage.rs`).
    fx.shock = Some(at);
    let dust = if scene.map.terrain.is_some() {
        Matter::Dirt
    } else {
        Matter::Concrete
    };
    fx.flash(
        at + Vec3::Y * 0.5,
        super::FlashDef {
            lumens: BLAST_FLASH.lumens * big,
            range: BLAST_FLASH.range * big,
            ..BLAST_FLASH
        },
    );
    let dist = at.distance(fx.eye);
    let lod = super::table::lod_far(dist);
    let n = |k: usize| ((k as f32 * lod * big).ceil() as usize).max(1);
    let tint = super::table::dust_tint(dust);
    fx.emit_tint(Layer::Flash, n(3), at, Vec3::Y, Color::WHITE, 14.0 * big);
    fx.emit_tint(Layer::Fireball, n(16), at, Vec3::Y, Color::WHITE, 1.4 * big);
    fx.emit_tint(Layer::Sparks, n(30), at, Vec3::Y, Color::WHITE, 1.6 * big);
    fx.emit_tint(Layer::Smoke, n(10), at + Vec3::Y * 0.4, Vec3::Y, tint, 1.4 * big);
    fx.emit_tint(Layer::Grit, n(16), at, Vec3::Y, tint, 2.5 * big);
    // On or near the ground: a ring, a scorch, dust thrown along it.
    let floor = scene.map.floor(at.x, at.z, 0.2, at.y + 0.5);
    if at.y - floor < 2.5 {
        let base = Vec3::new(at.x, floor + 0.06, at.z);
        fx.emit_tint(Layer::Shock, 1, base, Vec3::Y, Color::WHITE, big);
        fx.emit_tint(Layer::Dust, n(12), base + Vec3::Y * 0.2, Vec3::Y, tint, 5.0 * big);
        let on_ground = (floor - scene.map.ground(at.x, at.z)).abs() < 0.05;
        let up = if on_ground {
            let g = scene.map.ground_normal(at.x, at.z);
            Vec3::new(g.x, g.y, g.z)
        } else {
            Vec3::Y
        };
        // Lit debris off the ground it tore up (the table's Blast row).
        let chips = n(usize::from(super::table::effect(Weapon::Blast, dust).chips));
        fx.chips.throw(base, up, dust, chips);
        // Laid on the floor as drawn: over a ramp's slope, the terrain's
        // folds, and faded off a platform's edge.
        if let Some((k, size)) = super::marks::kind(Weapon::Blast, dust) {
            fx.marks
                .lay(scene.map, k, size * big, Vec3::new(at.x, floor, at.z), up, dust);
        }
    }
}

/// Each burning hull's fire and black smoke this frame.
pub fn burn(fx: &mut Fx, dt: f32) {
    for i in 0..fx.burning.len() {
        let mut b = fx.burning[i];
        if b.left <= 0.0 {
            continue;
        }
        b.left -= dt;
        b.acc += dt * BURN_HZ;
        let fade = (b.left / BURN_S).clamp(0.0, 1.0);
        while b.acc >= 1.0 {
            b.acc -= 1.0;
            let at = b.at + Vec3::new(fx.range(-1.2, 1.2), 0.0, fx.range(-0.6, 0.6));
            fx.emit_tint(Layer::Fireball, 1, at, Vec3::Y, Color::WHITE, 0.4 + 0.6 * fade);
            fx.emit_tint(
                Layer::Smoke,
                1,
                at + Vec3::Y * 0.6,
                Vec3::Y,
                Color::srgb(0.3, 0.3, 0.3),
                1.0 + fade,
            );
        }
        fx.burning[i] = b;
    }
}

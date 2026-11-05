//! Explosions. A charge going off is a star flash and a hot glow for a
//! frame, a baked fireball (`sheets::EXPLOSION`: fire that rolls up and
//! turns to soot in place) with smaller ones flung off it, smoke that
//! outlasts the fire, sparks, embers that drift and fall, debris trailing
//! smoke, a ring along the ground and dust rolled out along it, a scorch,
//! a light; a wreck's is bigger and leaves its hull burning in baked
//! flames. A sticky's is energy: no fire, a blue-white crack, a plasma
//! burst, lightning, blue sparks and a little vapour. A blast near us
//! shakes the drawn camera (`shake.rs`; never where we aim).

use bevy::prelude::*;
use sim::events::Cause;

use super::sheets::{TONE_GREEN, TONE_PINK, TONE_PLASMA};
use super::table::{Layer, Matter, Weapon};
use super::{Fx, Scene, BLAST_FLASH};
use crate::look::shake;

/// A wreck burns this long.
const BURN_S: f32 = 4.5;
/// Flames and smoke puffs a burning hull throws a second.
const BURN_HZ: f32 = 9.0;

/// A hull burning.
#[derive(Clone, Copy, Default)]
pub struct Burn {
    at: Vec3,
    left: f32,
    acc: f32,
}

/// A piece of debris in the air, leaving smoke behind it.
#[derive(Clone, Copy, Default)]
pub struct Trailer {
    at: Vec3,
    vel: Vec3,
    left: f32,
    /// Where it stops: the floor under the blast.
    floor: f32,
    /// Metres since its last puff.
    gone: f32,
}

/// A trailer puffs every this many metres.
const TRAIL_EVERY_M: f32 = 0.22;

/// The soot a charge's smoke is tinted, sRGB.
const SOOT: Color = Color::srgb(0.2, 0.19, 0.18);
/// A blast's shock ring and its light: fire's colours.
const FIRE_RING: Color = Color::srgb(1.0, 0.8, 0.55);
/// Ion blue, for the sticky's.
const ION_RING: Color = Color::srgb(0.45, 0.65, 1.0);

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
    if c == Cause::Sticky as u16 {
        sticky(fx, scene, at);
        return;
    }
    let big = if c == Cause::Grenade as u16 { 0.85 } else { 1.0 };
    boom(fx, scene, at, big);
}

/// The ion cannon's ball bursting: green light, a green ring, a baked
/// burst of green plasma, sparks, no fire or smoke.
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
    fx.energy(at, TONE_GREEN, 2.0, 3);
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

/// Shards in one body going off together: a pink star, a pink burst,
/// shards everywhere, a pink light. Glass, not fire.
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
    fx.energy(at, TONE_PINK, 1.2, 2);
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

/// A sticky going off: a blue-white crack of light, a baked burst of
/// plasma, lightning crawling out of it, blue sparks, a blue ring on the
/// ground and a little vapour. No fire: energy.
fn sticky(fx: &mut Fx, scene: &Scene, at: Vec3) {
    fx.shock = Some(at);
    fx.flash(
        at + Vec3::Y * 0.4,
        super::FlashDef {
            lumens: 2_600_000.0,
            range: 17.0,
            life: 0.4,
            color: [0.4, 0.62, 1.0],
        },
    );
    let dist = at.distance(fx.eye);
    let lod = super::table::lod_far(dist);
    let n = |k: usize| ((k as f32 * lod).ceil() as usize).max(1);
    fx.emit_tint(Layer::Zap, n(2), at, Vec3::Y, Color::WHITE, 9.0);
    *super::shell::claim(&mut fx.shocks) = super::shell::ion(at);
    fx.energy(at, TONE_PLASMA, 2.6, n(3));
    for _ in 0..n(12) {
        let d = fx.unit();
        fx.emit_tint(Layer::Arcs, 1, at + d * 0.6, d, Color::WHITE, 2.8);
    }
    super::flying::splash(fx, at, Vec3::Y, 4.0);
    fx.emit_tint(Layer::Embers, n(16), at, Vec3::Y, Color::srgb(0.5, 0.7, 1.0), 1.0);
    // The vapour the heat leaves: thin, pale, quick.
    fx.emit_tint(
        Layer::Cloud,
        n(3),
        at + Vec3::Y * 0.4,
        Vec3::Y,
        Color::srgb(0.6, 0.68, 0.85),
        1.0,
    );
    let floor = scene.map.floor(at.x, at.z, 0.2, at.y + 0.5);
    if at.y - floor < 2.5 {
        let base = Vec3::new(at.x, floor + 0.06, at.z);
        fx.emit_tint(Layer::Shock, 1, base, Vec3::Y, ION_RING, 1.1);
        let up = ground_up(scene, at, floor);
        if let Some((k, size)) = super::marks::kind(Weapon::Blast, Matter::Concrete) {
            fx.marks.lay(
                scene.map,
                k,
                size * 0.8,
                Vec3::new(at.x, floor, at.z),
                up,
                Matter::Concrete,
            );
        }
    }
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

/// The ground's normal under a blast on it, or straight up off a platform.
fn ground_up(scene: &Scene, at: Vec3, floor: f32) -> Vec3 {
    let on_ground = (floor - scene.map.ground(at.x, at.z)).abs() < 0.05;
    if on_ground {
        let g = scene.map.ground_normal(at.x, at.z);
        Vec3::new(g.x, g.y, g.z)
    } else {
        Vec3::Y
    }
}

/// A charge going off, `big` times a rocket's.
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
    // The first frames: a hard star and a hot glow over everything, and
    // the shock blown out through the air.
    fx.emit_tint(Layer::Flash, n(3), at, Vec3::Y, Color::WHITE, 16.0 * big);
    *super::shell::claim(&mut fx.shocks) = super::shell::fire(at, big);
    // The fireball, and smaller ones flung off it.
    fx.emit_tint(Layer::Boom, 1, at + Vec3::Y * 0.5 * big, Vec3::Y, Color::WHITE, big);
    fx.emit_tint(Layer::Fireballs, n(3), at, Vec3::Y, Color::WHITE, big);
    // Smoke that outlasts the fire, rising off it, soot and the ground's
    // dust in it.
    let smoke = SOOT.mix(&tint, 0.35);
    fx.emit_tint(Layer::Cloud, n(5), at + Vec3::Y * 0.7 * big, Vec3::Y, smoke, 1.5 * big);
    fx.emit_tint(Layer::Sparks, n(36), at, Vec3::Y, Color::WHITE, 1.7 * big);
    fx.emit_tint(Layer::Embers, n(22), at, Vec3::Y, Color::WHITE, big);
    // On or near the ground: a ring, dust rolled out along it, debris off
    // it trailing smoke, a scorch.
    let floor = scene.map.floor(at.x, at.z, 0.2, at.y + 0.5);
    if at.y - floor < 2.5 {
        let base = Vec3::new(at.x, floor + 0.06, at.z);
        fx.emit_tint(Layer::Shock, 1, base, Vec3::Y, FIRE_RING, big);
        for _ in 0..n(7) {
            let a = fx.range(0.0, core::f32::consts::TAU);
            let out = Vec3::new(a.cos(), 0.12, a.sin());
            fx.emit_tint(
                Layer::DustCloud,
                1,
                base + out * 0.5 * big + Vec3::Y * 0.25,
                out,
                tint,
                5.0 * big,
            );
        }
        let up = ground_up(scene, at, floor);
        // Lit debris off the ground it tore up (the table's Blast row).
        let chips = n(usize::from(super::table::effect(Weapon::Blast, dust).chips));
        fx.chips.throw(base, up, dust, chips, floor);
        for _ in 0..n(5) {
            let d = (Vec3::Y * 1.2 + fx.unit()).normalize_or(Vec3::Y);
            let speed = fx.range(6.0, 12.0) * big.sqrt();
            launch(fx, at, d * speed, floor);
        }
        // Laid on the floor as drawn: over a ramp's slope, the terrain's
        // folds, and faded off a platform's edge.
        if let Some((k, size)) = super::marks::kind(Weapon::Blast, dust) {
            fx.marks
                .lay(scene.map, k, size * big, Vec3::new(at.x, floor, at.z), up, dust);
        }
    } else {
        for _ in 0..n(4) {
            let d = fx.unit();
            let speed = fx.range(5.0, 10.0) * big.sqrt();
            launch(fx, at, d * speed, f32::NEG_INFINITY);
        }
    }
}

/// A piece of debris flung from `at` at `vel`, trailing smoke until it
/// meets `floor` or burns out.
fn launch(fx: &mut Fx, at: Vec3, vel: Vec3, floor: f32) {
    let i = (0..fx.trailers.len())
        .min_by(|&a, &b| fx.trailers[a].left.total_cmp(&fx.trailers[b].left))
        .unwrap_or(0);
    if fx.trailers[i].left > 0.3 {
        return;
    }
    fx.trailers[i] = Trailer {
        at,
        vel,
        left: fx.range(0.7, 1.3),
        floor,
        gone: 0.0,
    };
}

/// Each trailer this frame: flies, falls, puffs smoke along its way and
/// glows at its head.
pub fn trail(fx: &mut Fx, dt: f32) {
    for i in 0..fx.trailers.len() {
        let mut t = fx.trailers[i];
        if t.left <= 0.0 {
            continue;
        }
        t.left -= dt;
        t.vel.y -= 9.81 * dt;
        t.vel *= (-0.6 * dt).exp();
        let step = t.vel * dt;
        t.at += step;
        if t.at.y < t.floor {
            t.left = 0.0;
        }
        t.gone += step.length();
        let fade = (t.left / 1.3).clamp(0.0, 1.0);
        while t.gone >= TRAIL_EVERY_M {
            t.gone -= TRAIL_EVERY_M;
            fx.emit_tint(
                Layer::GunSmoke,
                1,
                t.at - step * (t.gone / TRAIL_EVERY_M.max(1e-3)).min(1.0),
                -t.vel,
                Color::srgb(0.55, 0.55, 0.55),
                1.6 + 1.0 * fade,
            );
        }
        fx.glow.spawn(super::pool::Particle {
            left: 0.04,
            life: 0.04,
            pos: t.at.into(),
            size0: 0.04 * fade + 0.01,
            size1: 0.02,
            c0: [9.0, 3.5, 0.8, 1.0],
            c1: [2.0, 0.5, 0.1, 0.0],
            cell: super::atlas::GLOW,
            ..Default::default()
        });
        fx.trailers[i] = t;
    }
}

/// Each burning hull's flames and black smoke this frame.
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
            fx.emit_tint(Layer::Flame, 1, at, Vec3::Y, Color::WHITE, 0.5 + 0.7 * fade);
            if fx.rand() < 0.5 {
                fx.emit_tint(Layer::Cloud, 1, at + Vec3::Y * 0.9, Vec3::Y, SOOT, 0.8 + 0.6 * fade);
            }
        }
        fx.burning[i] = b;
    }
}

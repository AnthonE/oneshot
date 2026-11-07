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

use super::atlas::{GLOW, RING, STAR, STREAK};
use super::pool::{Orient, Particle};
use super::sheets::{TONE_GREEN, TONE_PINK, TONE_PLASMA};
use super::table::{Layer, Matter, Weapon};
use super::{FlashDef, Fx, Scene, BLAST_FLASH};
use crate::look::shake;

/// A wreck burns this long (its husk smoulders after: `look/truck.rs`).
const BURN_S: f32 = 7.0;
/// A wreck's fire rides its husk (`carry`) if the husk is within this of it.
const CARRY_M: f32 = 2.5;
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
const TRAIL_EVERY_M: f32 = 0.09;

/// A blast's sparks, against a round's quick ones (`table.rs`): how fast
/// (m/s) and how long (s).
const BLAST_SPARK_MPS: (f32, f32) = (5.0, 16.0);
const BLAST_SPARK_S: (f32, f32) = (0.25, 0.7);

/// The soot a charge's smoke is tinted, sRGB.
const SOOT: Color = Color::srgb(0.2, 0.19, 0.18);
/// A blast's shock ring and its light: fire's colours.
const FIRE_RING: Color = Color::srgb(1.0, 0.8, 0.55);
/// Ion blue, for the sticky's.
const ION_RING: Color = Color::srgb(0.45, 0.65, 1.0);
/// What an arc's blue is tinted in a green burst.
const GREEN_ARC: Color = Color::srgb(0.45, 1.0, 0.5);
/// An ember's orange: what the Embers layer's white is tinted.
const EMBER: Color = Color::srgb(1.0, 0.45, 0.1);

/// A glow drawn by hand where it is (not a layer's recipe, `table.rs`):
/// its life (s), its half-size at birth and at death (m, before the
/// blast's scale) and its colour at birth and at death (linear HDR).
#[derive(Clone, Copy)]
struct Glow {
    life: f32,
    size: [f32; 2],
    c0: [f32; 4],
    c1: [f32; 4],
}

impl Glow {
    /// The particle, `k` times its size, at `at`.
    fn at(self, at: Vec3, k: f32, cell: u8) -> Particle {
        Particle {
            left: self.life,
            life: self.life,
            pos: at.into(),
            size0: self.size[0] * k,
            size1: self.size[1] * k,
            c0: self.c0,
            c1: self.c1,
            cell,
            ..Default::default()
        }
    }
}

/// How many of a thing a burst `k` times the cannon's throws, of a count
/// `(per k, at least)`.
fn scaled((per, least): (f32, usize), k: f32) -> usize {
    ((per * k).ceil() as usize).max(least)
}

// ── Green plasma bursting (`burst_green`), at the cannon's size ──────────

/// The mint glow, and the ring across the floor under it (dropped this far).
const GREEN_GLOW: Glow = Glow {
    life: 0.25,
    size: [1.0, 4.0],
    c0: [3.0, 12.0, 4.0, 1.0],
    c1: [0.3, 1.5, 0.4, 0.0],
};
const GREEN_RING: Glow = Glow {
    life: 0.35,
    size: [0.5, 9.0],
    c0: [1.5, 6.0, 2.0, 0.8],
    c1: [0.2, 0.8, 0.3, 0.0],
};
const GREEN_RING_DROP_M: f32 = 0.4;
/// Its first frame, white inside.
const GREEN_CORE: Glow = Glow {
    life: 0.05,
    size: [1.5, 0.9],
    c0: [14.0, 22.0, 15.0, 1.0],
    c1: [2.0, 8.0, 3.0, 0.0],
};
/// Its baked plasma: size, and how many.
const GREEN_ENERGY: f32 = 2.0;
const GREEN_ENERGIES: (f32, usize) = (3.0, 2);
/// The lightning crawling out of it: how many, how far out they start (m),
/// their size.
const GREEN_ARCS: (f32, usize) = (9.0, 4);
const GREEN_ARC_OUT_M: f32 = 0.4;
const GREEN_ARC_SIZE: f32 = 2.2;
/// Its sparks: how many, their life (s) and speed (m/s), fall (m/s²),
/// drag (1/s), half-width at birth and death (m), the seconds of velocity
/// a streak is drawn over; in the glow's colours.
const GREEN_SPARKS: (f32, usize) = (24.0, 8);
const GREEN_SPARK_S: (f32, f32) = (0.3, 0.6);
const GREEN_SPARK_MPS: (f32, f32) = (4.0, 11.0);
const GREEN_SPARK_FALL: f32 = 6.0;
const GREEN_SPARK_DRAG: f32 = 1.5;
const GREEN_SPARK_W: [f32; 2] = [0.012, 0.006];
const GREEN_SPARK_STRETCH_S: f32 = 0.02;
/// How hard it shakes the camera, a blast's share.
const GREEN_SHAKE: f32 = 0.8;
/// Its light: lumens `k` times this, range `√k` times.
const GREEN_FLASH: FlashDef = FlashDef {
    lumens: 1_500_000.0,
    range: 14.0,
    life: 0.35,
    color: [0.4, 1.0, 0.45],
};

// ── Shards going off together (`burst`) ──────────────────────────────────

/// The pink the zap, the vapour and the light are tinted.
const PINK_TINT: Color = Color::LinearRgba(LinearRgba::rgb(2.4, 0.5, 1.6));
/// Its stars, one of each star cell from `atlas::STAR`.
const PINK_STAR: Glow = Glow {
    life: 0.12,
    size: [0.6, 1.3],
    c0: super::table::PINK,
    c1: [1.5, 0.3, 1.0, 0.0],
};
const PINK_STARS: u8 = 3;
/// How many of each and how big: the zap, the baked burst, the shards
/// (their speed, m/s) and the vapour.
const PINK_ZAPS: (usize, f32) = (2, 6.0);
const PINK_ENERGY: (usize, f32) = (2, 1.2);
const PINK_SHARDS: (usize, f32) = (40, 7.0);
const PINK_VAPOUR: (usize, f32) = (6, 5.0);
const PINK_FLASH: FlashDef = FlashDef {
    lumens: 900_000.0,
    range: 9.0,
    life: 0.3,
    color: [1.0, 0.35, 0.8],
};

// ── A sticky going off (`sticky`) ────────────────────────────────────────

/// Its crack of light, this far over where it went off.
const STICKY_FLASH: FlashDef = FlashDef {
    lumens: 2_600_000.0,
    range: 17.0,
    life: 0.4,
    color: [0.4, 0.62, 1.0],
};
const STICKY_FLASH_UP_M: f32 = 0.4;
/// How many of each (before the distance's thinning) and how big: the
/// zap, the baked plasma, the arcs (starting this far out), the splash,
/// the blue embers, the vapour (this far up).
const STICKY_ZAPS: (usize, f32) = (2, 4.0);
const STICKY_ENERGY: (usize, f32) = (2, 1.7);
const STICKY_ARCS: (usize, f32) = (12, 2.8);
const STICKY_ARC_OUT_M: f32 = 0.6;
const STICKY_SPLASH: f32 = 4.0;
const STICKY_EMBERS: (usize, f32) = (16, 1.0);
const STICKY_EMBER: Color = Color::srgb(0.5, 0.7, 1.0);
const STICKY_VAPOUR: (usize, f32) = (3, 2.5);
const STICKY_VAPOUR_UP_M: f32 = 0.4;
const STICKY_VAPOUR_TINT: Color = Color::srgb(0.6, 0.68, 0.85);
/// Its ring's size, and its scorch's share of a blast's.
const STICKY_RING: f32 = 1.1;
const STICKY_SCORCH: f32 = 0.8;

// ── A charge going off (`boom`), at a rocket's size ──────────────────────

/// A frag's, a rocket's and a wreck's, and how much harder and further a
/// wreck shakes the camera than a blast.
const GRENADE_BIG: f32 = 0.85;
const WRECK_BIG: f32 = 1.5;
const WRECK_SHAKE_FULL: f32 = 1.5;
const WRECK_SHAKE_ZERO: f32 = 1.3;
/// A wreck's blast is this far over its hull's origin.
const WRECK_UP_M: f32 = 0.8;
/// Within this of the floor a blast is on the ground (a ring, dust, a
/// scorch), drawn this far over it.
const GROUND_NEAR_M: f32 = 2.5;
const GROUND_LIFT_M: f32 = 0.06;
/// Its light, this far over where it went off.
const BOOM_FLASH_UP_M: f32 = 0.5;
/// Its first frames' stars: how many, and their size (`a + b·big`).
const BOOM_STARS: usize = 3;
const BOOM_STAR_SIZE: (f32, f32) = (10.0, 4.0);
/// The fireball, lifted this far (`× big`) off the ground it sits on, and
/// how many smaller ones are flung off it.
const BOOM_LIFT_M: f32 = 0.6;
const BOOM_FIREBALLS: usize = 3;
/// The smoke: how many puffs, lifted this far (`× big`), their size
/// (`× big`), and how much of the ground's dust is in the soot.
const BOOM_SMOKES: usize = 8;
const BOOM_SMOKE_LIFT_M: f32 = 0.7;
const BOOM_SMOKE_SIZE: f32 = 2.1;
const BOOM_SMOKE_DUST: f32 = 0.35;
/// Its sparks: how wide they spray, their drag, how many, their size
/// (`× big`; their speed and life are [`BLAST_SPARK_MPS`], [`BLAST_SPARK_S`]).
const BOOM_SPARK_SPRAY: f32 = 0.3;
const BOOM_SPARK_DRAG: f32 = 2.2;
const BOOM_SPARKS: usize = 36;
const BOOM_SPARK_SIZE: f32 = 1.7;
const BOOM_EMBERS: usize = 22;
/// The dust rolled out along the ground: how many, how much they rise as
/// they go, how far out (`× big`) and up they start, their size (`× big`).
const BOOM_DUSTS: usize = 7;
const BOOM_DUST_RISE: f32 = 0.12;
const BOOM_DUST_OUT_M: f32 = 0.5;
const BOOM_DUST_UP_M: f32 = 0.25;
const BOOM_DUST_SIZE: f32 = 6.0;
/// Debris trailing smoke: off the ground (how many, how much it leans up,
/// how fast, m/s `× √big`), and in the air.
const BOOM_DEBRIS: usize = 5;
const BOOM_DEBRIS_UP: f32 = 1.2;
const BOOM_DEBRIS_MPS: (f32, f32) = (6.0, 12.0);
const BOOM_DEBRIS_AIR: usize = 4;
const BOOM_DEBRIS_AIR_MPS: (f32, f32) = (5.0, 10.0);

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
    let big = if c == Cause::Grenade as u16 { GRENADE_BIG } else { 1.0 };
    boom(fx, scene, at, big);
}

/// The ion cannon's ball bursting.
fn plasma(fx: &mut Fx, at: Vec3) {
    burst_green(fx, at, 1.0, true);
}

/// Green plasma bursting, `k` of the cannon's (a charged bolt's is smaller): a
/// hard white-green flash, a mint shell, a baked burst of plasma, lightning
/// crawling out of it, sparks. Light and lines, no fire or smoke. `ring`: a ring
/// across the floor under it too, for what goes off on the ground; on a wall or
/// a body, a flat ring is a line in the air.
pub fn burst_green(fx: &mut Fx, at: Vec3, k: f32, ring: bool) {
    fx.glow.spawn(GREEN_GLOW.at(at, k, GLOW));
    if ring {
        fx.glow.spawn(Particle {
            orient: Orient::Flat,
            ..GREEN_RING.at(at - Vec3::Y * GREEN_RING_DROP_M * k, k, RING)
        });
    }
    fx.glow.spawn(GREEN_CORE.at(at, k, GLOW));
    *super::shell::claim(&mut fx.shocks) = super::shell::mint(at, k);
    fx.energy(at, TONE_GREEN, GREEN_ENERGY * k, scaled(GREEN_ENERGIES, k));
    for _ in 0..scaled(GREEN_ARCS, k) {
        let d = fx.unit();
        fx.emit_tint(
            Layer::Arcs,
            1,
            at + d * GREEN_ARC_OUT_M * k,
            d,
            GREEN_ARC,
            GREEN_ARC_SIZE * k,
        );
    }
    for _ in 0..scaled(GREEN_SPARKS, k) {
        let d = fx.unit();
        let life = fx.range(GREEN_SPARK_S.0, GREEN_SPARK_S.1);
        let speed = fx.range(GREEN_SPARK_MPS.0, GREEN_SPARK_MPS.1);
        fx.glow.spawn(Particle {
            left: life,
            life,
            pos: at.into(),
            vel: (d * speed).into(),
            gravity: GREEN_SPARK_FALL,
            drag: GREEN_SPARK_DRAG,
            size0: GREEN_SPARK_W[0],
            size1: GREEN_SPARK_W[1],
            c0: GREEN_GLOW.c0,
            c1: GREEN_GLOW.c1,
            cell: STREAK,
            orient: Orient::Stretch,
            stretch: GREEN_SPARK_STRETCH_S,
            ..Default::default()
        });
    }
    let eye = fx.eye;
    fx.shake
        .add_at(GREEN_SHAKE * k, at, eye, shake::BLAST_FULL_M, shake::BLAST_ZERO_M);
    fx.flash(
        at,
        FlashDef {
            lumens: GREEN_FLASH.lumens * k,
            range: GREEN_FLASH.range * k.sqrt(),
            ..GREEN_FLASH
        },
    );
}

/// Shards in one body going off together: a pink star, a pink burst,
/// shards everywhere, a pink light. Glass, not fire.
fn burst(fx: &mut Fx, at: Vec3) {
    for k in 0..PINK_STARS {
        let roll = fx.range(0.0, core::f32::consts::TAU);
        fx.glow.spawn(Particle {
            roll,
            ..PINK_STAR.at(at, 1.0, STAR + k)
        });
    }
    fx.emit_tint(Layer::Zap, PINK_ZAPS.0, at, Vec3::Y, PINK_TINT, PINK_ZAPS.1);
    fx.energy(at, TONE_PINK, PINK_ENERGY.1, PINK_ENERGY.0);
    super::table::shatter(&mut fx.glow, at, PINK_SHARDS.0, PINK_SHARDS.1);
    fx.emit_tint(Layer::GunSmoke, PINK_VAPOUR.0, at, Vec3::Y, PINK_TINT, PINK_VAPOUR.1);
    fx.flash(at, PINK_FLASH);
}

/// A sticky going off: a blue-white crack of light, a baked burst of
/// plasma, lightning crawling out of it, blue sparks, a blue ring on the
/// ground and a little vapour. No fire: energy.
fn sticky(fx: &mut Fx, scene: &Scene, at: Vec3) {
    fx.shock = Some(at);
    fx.flash(at + Vec3::Y * STICKY_FLASH_UP_M, STICKY_FLASH);
    let dist = at.distance(fx.eye);
    let lod = super::table::lod_far(dist);
    let n = |k: usize| ((k as f32 * lod).ceil() as usize).max(1);
    fx.emit_tint(Layer::Zap, n(STICKY_ZAPS.0), at, Vec3::Y, Color::WHITE, STICKY_ZAPS.1);
    *super::shell::claim(&mut fx.shocks) = super::shell::ion(at);
    fx.energy(at, TONE_PLASMA, STICKY_ENERGY.1, n(STICKY_ENERGY.0));
    for _ in 0..n(STICKY_ARCS.0) {
        let d = fx.unit();
        fx.emit_tint(
            Layer::Arcs,
            1,
            at + d * STICKY_ARC_OUT_M,
            d,
            Color::WHITE,
            STICKY_ARCS.1,
        );
    }
    super::flying::splash(fx, at, Vec3::Y, STICKY_SPLASH);
    fx.emit_tint(
        Layer::Embers,
        n(STICKY_EMBERS.0),
        at,
        Vec3::Y,
        STICKY_EMBER,
        STICKY_EMBERS.1,
    );
    // The vapour the heat leaves: thin, pale, quick.
    fx.emit_tint(
        Layer::Wisp,
        n(STICKY_VAPOUR.0),
        at + Vec3::Y * STICKY_VAPOUR_UP_M,
        Vec3::Y,
        STICKY_VAPOUR_TINT,
        STICKY_VAPOUR.1,
    );
    let floor = scene.map.floor(at.x, at.z, 0.2, at.y + 0.5);
    if at.y - floor < GROUND_NEAR_M {
        let base = Vec3::new(at.x, floor + GROUND_LIFT_M, at.z);
        fx.emit_tint(Layer::Shock, 1, base, Vec3::Y, ION_RING, STICKY_RING);
        let up = ground_up(scene, at, floor);
        if let Some((k, size)) = super::marks::kind(Weapon::Blast, Matter::Concrete) {
            fx.marks.lay(
                scene.map,
                k,
                size * STICKY_SCORCH,
                Vec3::new(at.x, floor, at.z),
                up,
                Matter::Concrete,
            );
        }
    }
}

/// A vehicle blowing up: a bigger blast, then its hull burns.
pub fn wreck(fx: &mut Fx, scene: &Scene, at: Vec3) {
    let at = at + Vec3::Y * WRECK_UP_M;
    let eye = fx.eye;
    fx.shake.add_at(
        1.0,
        at,
        eye,
        shake::BLAST_FULL_M * WRECK_SHAKE_FULL,
        shake::BLAST_ZERO_M * WRECK_SHAKE_ZERO,
    );
    boom(fx, scene, at, WRECK_BIG);
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
        at + Vec3::Y * BOOM_FLASH_UP_M,
        FlashDef {
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
    fx.emit_tint(
        Layer::Flash,
        n(BOOM_STARS),
        at,
        Vec3::Y,
        Color::WHITE,
        BOOM_STAR_SIZE.0 + BOOM_STAR_SIZE.1 * big,
    );
    *super::shell::claim(&mut fx.shocks) = super::shell::fire(at, big);
    // The fireball, lifted off the ground it sits on (a billboard its size
    // would cut into it), and smaller ones flung off it.
    fx.emit_tint(
        Layer::Boom,
        1,
        at + Vec3::Y * BOOM_LIFT_M * big,
        Vec3::Y,
        Color::WHITE,
        big,
    );
    fx.emit_tint(Layer::Fireballs, n(BOOM_FIREBALLS), at, Vec3::Y, Color::WHITE, big);
    // Smoke that outlasts the fire, rising off it, soot and the ground's
    // dust in it.
    let smoke = SOOT.mix(&tint, BOOM_SMOKE_DUST);
    fx.emit_tint(
        Layer::Cloud,
        n(BOOM_SMOKES),
        at + Vec3::Y * BOOM_SMOKE_LIFT_M * big,
        Vec3::Y,
        smoke,
        BOOM_SMOKE_SIZE * big,
    );
    let sparks = ::fx::recipe::Recipe {
        speed: BLAST_SPARK_MPS,
        spray: BOOM_SPARK_SPRAY,
        drag: BOOM_SPARK_DRAG,
        life: BLAST_SPARK_S,
        ..super::table::recipe(Layer::Sparks)
    };
    let pool = fx.pool(Layer::Sparks.pool());
    super::table::emit_recipe(
        pool,
        &sparks,
        n(BOOM_SPARKS),
        at,
        Vec3::Y,
        Color::WHITE,
        BOOM_SPARK_SIZE * big,
    );
    fx.emit_tint(Layer::Embers, n(BOOM_EMBERS), at, Vec3::Y, EMBER, big);
    // On or near the ground: a ring, dust rolled out along it, debris off
    // it trailing smoke, a scorch.
    let floor = scene.map.floor(at.x, at.z, 0.2, at.y + 0.5);
    if at.y - floor < GROUND_NEAR_M {
        let base = Vec3::new(at.x, floor + GROUND_LIFT_M, at.z);
        fx.emit_tint(Layer::Shock, 1, base, Vec3::Y, FIRE_RING, big);
        for _ in 0..n(BOOM_DUSTS) {
            let a = fx.range(0.0, core::f32::consts::TAU);
            let out = Vec3::new(a.cos(), BOOM_DUST_RISE, a.sin());
            fx.emit_tint(
                Layer::DustCloud,
                1,
                base + out * BOOM_DUST_OUT_M * big + Vec3::Y * BOOM_DUST_UP_M,
                out,
                tint,
                BOOM_DUST_SIZE * big,
            );
        }
        let up = ground_up(scene, at, floor);
        // Lit debris off the ground it tore up (the table's Blast row).
        let chips = n(usize::from(super::table::effect(Weapon::Blast, dust).chips));
        fx.chips.throw(base, up, dust, chips, floor);
        for _ in 0..n(BOOM_DEBRIS) {
            let d = (Vec3::Y * BOOM_DEBRIS_UP + fx.unit()).normalize_or(Vec3::Y);
            let speed = fx.range(BOOM_DEBRIS_MPS.0, BOOM_DEBRIS_MPS.1) * big.sqrt();
            launch(fx, at, d * speed, floor);
        }
        // Laid on the floor as drawn: over a ramp's slope, the terrain's
        // folds, and faded off a platform's edge.
        if let Some((k, size)) = super::marks::kind(Weapon::Blast, dust) {
            fx.marks
                .lay(scene.map, k, size * big, Vec3::new(at.x, floor, at.z), up, dust);
        }
    } else {
        for _ in 0..n(BOOM_DEBRIS_AIR) {
            let d = fx.unit();
            let speed = fx.range(BOOM_DEBRIS_AIR_MPS.0, BOOM_DEBRIS_AIR_MPS.1) * big.sqrt();
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
        left: fx.range(0.35, 0.75),
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
                Color::srgb(0.42, 0.42, 0.42),
                0.6 + 0.6 * fade,
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

/// The fire burning nearest `from` (within [`CARRY_M`]), moved to `to`: a
/// wreck's, riding its husk as it flies. How much of its burn is left (0:
/// none was there).
pub fn carry(fx: &mut Fx, from: Vec3, to: Vec3) -> f32 {
    let near = (0..fx.burning.len())
        .filter(|&i| fx.burning[i].left > 0.0)
        .min_by(|&a, &b| {
            fx.burning[a]
                .at
                .distance_squared(from)
                .total_cmp(&fx.burning[b].at.distance_squared(from))
        });
    match near {
        Some(i) if fx.burning[i].at.distance(from) < CARRY_M => {
            fx.burning[i].at = to;
            fx.burning[i].left / BURN_S
        }
        _ => 0.0,
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

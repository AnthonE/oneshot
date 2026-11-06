//! What's in flight. Things with weight are drawn with their models: a
//! rocket with its exhaust's glow and a trail of smoke, a frag or a
//! launcher's grenade with its fuse blinking, a sticky in its own cage glowing blue. Light
//! is drawn as light, straight into the glow pool from where it is this
//! frame: an ion bolt's blue core and streak, a charged bolt's bigger one
//! with its light, the cannon's green ball, the SHARD GUN's pink crystals.
//!
//! Rounds come drawn at the render tick, each with an id that holds while
//! it flies (`client_net::core::DrawnProj`): a model keeps its round by id.
//! No event says where a bolt or a shard ended (a hit is the two fighters'
//! business), so it is inferred, cosmetically: one whose id was flying last
//! frame and is gone now ended where it was last drawn. A shard that ended in
//! a body we see sticks there (`bodies.rs` glints it); anywhere else it
//! shatters. A bolt splashes. Drawing only.

use bevy::prelude::*;
use client_net::core::{Drawn, DrawnProj};
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
/// A sticky's glow round its cage, and its core's, metres across: it reads from
/// across a room, and on whoever it is stuck to.
const STICKY_HALO_M: f32 = 0.34;
const STICKY_CORE_M: f32 = 0.11;
/// How far each model stands from its base to its middle.
const ROCKET_MID_M: f32 = 0.3;
const FRAG_MID_M: f32 = 0.08;
const STICKY_MID_M: f32 = 0.067;

/// Which model draws a kind of round: a hidden one goes to any round of its own.
fn model_of(kind: u8) -> u8 {
    match kind {
        ROCKET => ROCKET,
        STICKY => STICKY,
        _ => FRAG,
    }
}

/// A new model for `p`, kept from then on.
fn spawn(commands: &mut Commands, fx: &Fx, models: &Models, p: &DrawnProj) -> Entity {
    let rocket = p.kind == ROCKET;
    let (scene, mid) = match model_of(p.kind) {
        ROCKET => (&models.rocket, ROCKET_MID_M),
        STICKY => (&models.sticky, STICKY_MID_M),
        _ => (&models.frag, FRAG_MID_M),
    };
    let place = place(&p.state());
    let mut e = commands.spawn((
        Flying {
            id: p.id,
            kind: p.kind,
            last: place.translation,
            lit: false,
        },
        place,
        Visibility::default(),
    ));
    e.with_child((
        SceneRoot(scene.clone()),
        // Each model stands on its base, +Y up: centre it.
        Transform::from_xyz(0.0, -mid, 0.0),
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
    e.id()
}

/// Drawn with a model (the rest are light).
fn modelled(kind: u8) -> bool {
    matches!(kind, ROCKET | FRAG | LOB | STICKY)
}

#[derive(Component)]
pub struct Flying {
    /// The round it draws (`DrawnProj::id`).
    id: u16,
    kind: u8,
    /// Where its last puff was.
    last: Vec3,
    /// A fuse was lit last frame.
    lit: bool,
}

/// [`projectiles`]' lists: the models there are and which are taken, the
/// rounds that get one, and which kept theirs.
#[derive(Default)]
pub struct Scratch {
    pool: Vec<(Entity, u8, u16, bool)>,
    taken: Vec<bool>,
    go: Vec<(Entity, DrawnProj, bool)>,
    live: Vec<DrawnProj>,
    matched: Vec<bool>,
}

/// The rounds in flight this frame, as drawn at the render tick.
#[allow(clippy::too_many_arguments)]
pub fn projectiles(
    commands: &mut Commands,
    fx: &mut Fx,
    models: &Models,
    map: &sim::map::Map,
    proj: &[DrawnProj],
    others: &[Drawn],
    t: f32,
    dt: f32,
    have: &mut Query<(Entity, &mut Flying, &mut Transform, &mut Visibility)>,
) {
    fx.trail_clock(dt);
    for p in proj.iter().filter(|p| !modelled(p.kind)) {
        light(fx, &p.state());
    }
    ended(fx, map, proj, others, t, dt);
    // The models in flight, kept: each round keeps the model that drew its
    // id (so a rocket's trail stays its own), else takes a hidden one of
    // its kind, else a new one; the rest are hidden. Never despawned: a
    // model made mid-fight is a scene spawned mid-fight.
    let mut scratch = std::mem::take(&mut fx.fly);
    let Scratch {
        pool,
        taken,
        go,
        live,
        matched,
    } = &mut scratch;
    pool.clear();
    pool.extend(
        have.iter()
            .map(|(e, f, _, v)| (e, model_of(f.kind), f.id, *v != Visibility::Hidden)),
    );
    taken.clear();
    taken.resize(pool.len(), false);
    go.clear();
    live.clear();
    live.extend(proj.iter().filter(|p| modelled(p.kind)));
    matched.clear();
    matched.resize(live.len(), false);
    // First every shown model to its own round, then the rest a free
    // model, hidden ones first, then a new one.
    for (i, p) in live.iter().enumerate() {
        let model = model_of(p.kind);
        if let Some(k) = (0..pool.len()).find(|&k| !taken[k] && pool[k].3 && pool[k].1 == model && pool[k].2 == p.id) {
            matched[i] = true;
            taken[k] = true;
            go.push((pool[k].0, *p, false));
        }
    }
    for (i, p) in live.iter().enumerate() {
        if matched[i] {
            continue;
        }
        let model = model_of(p.kind);
        let free = (0..pool.len())
            .filter(|&k| !taken[k] && pool[k].1 == model)
            .min_by_key(|&k| pool[k].3);
        match free {
            Some(k) => {
                taken[k] = true;
                go.push((pool[k].0, *p, true));
            }
            None => {
                let e = spawn(commands, fx, models, p);
                pool.push((e, model, p.id, true));
                taken.push(true);
            }
        }
    }
    for (k, (e, ..)) in pool.iter().enumerate() {
        if !taken[k] {
            if let Ok((_, _, _, mut v)) = have.get_mut(*e) {
                if *v != Visibility::Hidden {
                    *v = Visibility::Hidden;
                }
            }
        }
    }
    for &(e, p, fresh) in go.iter() {
        let Ok((_, mut f, mut tr, mut v)) = have.get_mut(e) else {
            continue;
        };
        if *v == Visibility::Hidden {
            *v = Visibility::Inherited;
        }
        f.id = p.id;
        f.kind = p.kind;
        let p = &p.state();
        if fresh {
            // Another round now: its trail starts where it is.
            f.last = place(p).translation;
            f.lit = false;
        }
        *tr = place(p);
        let at = tr.translation;
        if p.kind == ROCKET {
            let back = tr.rotation * Vec3::NEG_Y;
            // The exhaust: a white-hot core in an orange glow, left every
            // frame so it stays on the nozzle, each as bright as its share
            // of a 60th of a second, so the glows it leaves add up the same
            // on any screen.
            let k = (dt * super::TRAIL_HZ).min(1.0);
            fx.glow.spawn(Particle {
                left: 0.05,
                life: 0.05,
                pos: (at + back * 0.45).into(),
                size0: 0.35,
                size1: 0.15,
                c0: [14.0 * k, 6.0 * k, 1.6 * k, 1.0],
                c1: [3.0 * k, 0.8 * k, 0.1 * k, 0.0],
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
            fx.glow.spawn(dot(at, STICKY_HALO_M, ION_HALO));
            fx.glow.spawn(dot(at, STICKY_CORE_M, ION));
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
    fx.fly = scratch;
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
            ride(
                fx,
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
            ride(
                fx,
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

/// The light that rides a round, lit again on each tick of the trail
/// clock: one light a 60th of a second, however fast the screen, so a bolt
/// never takes more of the flash lights than it does at 60.
fn ride(fx: &mut Fx, at: Vec3, def: FlashDef) {
    if fx.trail_ticks() > 0 {
        fx.flash(at, def);
    }
}

/// The sparks left behind a round of light: one on each tick of the trail
/// clock, a 60th of a second's flight apart.
fn trail(fx: &mut Fx, at: Vec3, v: Vec3, c: [f32; 4]) {
    let life = 0.2;
    for k in 0..fx.trail_ticks() {
        let drift = Vec3::new(fx.range(-0.3, 0.3), fx.range(-0.3, 0.3), fx.range(-0.3, 0.3));
        let back = v * (0.01 + k as f32 / super::TRAIL_HZ);
        fx.glow.spawn(Particle {
            left: life,
            life,
            pos: (at - back).into(),
            vel: drift.into(),
            size0: 0.025,
            size1: 0.005,
            c0: [c[0] * 0.6, c[1] * 0.6, c[2] * 0.6, 0.9],
            c1: [c[0] * 0.1, c[1] * 0.1, c[2] * 0.1, 0.0],
            cell: GLOW,
            ..Particle::default()
        });
    }
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

/// Rounds of light that were flying last frame and are gone now: ended
/// where they were last drawn, and what that looks like; an ion bolt that
/// ended on the map leaves a burn on the face it met (`marks.rs`), only
/// ever for a bolt the server sent us.
fn ended(fx: &mut Fx, map: &sim::map::Map, proj: &[DrawnProj], others: &[Drawn], t: f32, dt: f32) {
    let mut last = std::mem::take(&mut fx.last_light);
    {
        for was in &last {
            // Still flying: its id, and its kind (an id may come round again).
            if proj.iter().any(|p| p.id == was.id && p.kind == was.kind) {
                continue;
            }
            let wv = Vec3::new(was.vel.x, was.vel.y, was.vel.z);
            let at = Vec3::new(was.pos.x, was.pos.y, was.pos.z);
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
                    if in_body(others, at).is_none() {
                        burn(fx, map, at, wv, dt, big);
                    }
                }
                // The cannon's ball ends in a Blast event (`blast.rs`).
                _ => {}
            }
        }
    }
    // The same Vec again, refilled: taken above only to read it beside `fx`.
    last.clear();
    last.extend(proj.iter().filter(|p| !modelled(p.kind)).copied());
    fx.last_light = last;
}

/// The face a bolt from `wp` flying `wv` met in its last frame, if it met
/// one near enough to be where it ended, and the burn it left there.
fn burn(fx: &mut Fx, map: &sim::map::Map, wp: Vec3, wv: Vec3, dt: f32, big: f32) {
    let speed = wv.length();
    if speed < 1e-3 || super::table::lod(wp.distance(fx.eye)) <= 0.0 {
        return;
    }
    let dir = wv / speed;
    let reach = speed * dt * 2.0 + 0.5;
    if let Some(hit) = super::surface::cast(map, &mut fx.near, wp - dir * 0.2, dir, reach) {
        let matter = super::surface::matter_of(map, &hit, fx.marks.lift.as_deref());
        fx.marks.lay(
            map,
            super::marks::Kind::Scorch,
            super::marks::BURN_M * big,
            hit.at,
            hit.normal,
            matter,
        );
    }
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

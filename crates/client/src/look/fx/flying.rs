//! What's in flight. Things with weight are drawn with their models: a
//! rocket with its exhaust's glow and a trail of smoke, a frag or a
//! launcher's grenade with its fuse blinking, a sticky a ball of plasma
//! (`plasma.rs`) with its light, its sparks and, once stuck, its arcs. Light
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

use bevy::ecs::system::SystemParam;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use client_net::core::{Drawn, DrawnProj};
use client_net::proj::STUCK_V;
use proto::ProjState;
use sim::consts::{HEAD_Y, HIT_R};

use super::atlas::{GLOW, SHARD, STREAK};
use super::plasma;
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
/// A sticky's glow round the ball and at its heart, metres across, and how
/// much bigger with its fuse run out: it reads from across a room, and on
/// whoever it is stuck to.
const STICKY_HALO_M: f32 = 0.42;
const STICKY_CORE_M: f32 = 0.12;
const STICKY_GLOW_SWELL: f32 = 0.7;
/// Once stuck: arcs a second, from its fuse's start to its end, how big, and
/// the vapour it boils off, a second.
const STICKY_ARCS_HZ: (f32, f32) = (9.0, 34.0);
const STICKY_ARC_SIZE: f32 = 0.32;
const STICKY_ARC_OUT_M: f32 = 0.07;
const STICKY_VAPOUR_HZ: f32 = 7.0;
const STICKY_VAPOUR_SIZE: f32 = 0.4;
const STICKY_VAPOUR: Color = Color::srgb(0.6, 0.72, 0.95);
/// How long its fuse runs once it has stuck, seconds (`sim`'s, most throws).
const STICKY_FUSE_S: f32 = sim::consts::STICKY_FUSE_MS as f32 / 1000.0;
/// How far each model stands from its base to its middle.
const ROCKET_MID_M: f32 = 0.3;
const FRAG_MID_M: f32 = 0.08;

/// Which model draws a kind of round: a hidden one goes to any round of its own.
fn model_of(kind: u8) -> u8 {
    match kind {
        ROCKET => ROCKET,
        STICKY => STICKY,
        _ => FRAG,
    }
}

/// A ball's parts, moved as its fuse runs.
#[derive(SystemParam)]
pub struct BallParts<'w, 's> {
    balls: Query<'w, 's, (&'static mut Transform, &'static mut MeshTag), Without<Flying>>,
    lights: Query<'w, 's, &'static mut PointLight, Without<Flying>>,
}

/// A new model for `p`, kept from then on.
fn spawn(commands: &mut Commands, fx: &Fx, models: &Models, kit: Option<&plasma::Kit>, p: &DrawnProj) -> Entity {
    let place = place(&p.state());
    let mut e = commands.spawn((
        Flying {
            id: p.id,
            kind: p.kind,
            last: place.translation,
            lit: false,
            ball: None,
            glow: None,
            stuck: 0.0,
            arcs: 0.0,
            vapour: 0.0,
        },
        place,
        Visibility::default(),
    ));
    let mut ball = None;
    let mut glow = None;
    match model_of(p.kind) {
        ROCKET => {
            // It stands on its base, +Y up: centre it.
            e.with_child((
                SceneRoot(models.rocket.clone()),
                Transform::from_xyz(0.0, -ROCKET_MID_M, 0.0),
            ));
            if fx.caps.lights > 2 {
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
        STICKY => {
            if let Some(kit) = kit {
                e.with_children(|c| {
                    ball = Some(plasma::spawn_ball(c, kit, plasma::FLIGHT_HEAT));
                });
            }
            if fx.caps.lights > 2 {
                let [r, g, b] = plasma::LIGHT_RGB;
                e.with_children(|c| {
                    glow = Some(
                        c.spawn((
                            Transform::default(),
                            PointLight {
                                intensity: plasma::LUMENS,
                                range: plasma::LIGHT_M,
                                color: Color::linear_rgb(r, g, b),
                                shadows_enabled: false,
                                ..default()
                            },
                        ))
                        .id(),
                    );
                });
            }
        }
        _ => {
            e.with_child((
                SceneRoot(models.frag.clone()),
                Transform::from_xyz(0.0, -FRAG_MID_M, 0.0),
            ));
        }
    }
    let id = e.id();
    commands.entity(id).queue(move |mut e: EntityWorldMut| {
        if let Some(mut f) = e.get_mut::<Flying>() {
            f.ball = ball;
            f.glow = glow;
        }
    });
    id
}

/// A frame of a sticky: its heat, the ball and its light at it, its glow, and
/// what comes off it.
#[allow(clippy::too_many_arguments)]
fn sticky_frame(fx: &mut Fx, f: &mut Flying, parts: &mut BallParts, at: Vec3, vel: Vec3, fresh: bool, t: f32, dt: f32) {
    // A stuck one's velocity on the wire is its offset from what it is on.
    let still = vel.length() < STUCK_V;
    f.stuck = if still && !fresh { f.stuck + dt } else { 0.0 };
    let heat = if still {
        plasma::FLIGHT_HEAT.max((f.stuck / STICKY_FUSE_S).min(1.0))
    } else {
        plasma::FLIGHT_HEAT
    };
    let hot = heat * heat;
    if let Some((mut tr, mut tag)) = f.ball.and_then(|b| parts.balls.get_mut(b).ok()) {
        tr.scale = Vec3::splat(plasma::RADIUS_M * (1.0 + plasma::SWELL * hot));
        let want = plasma::tag(heat);
        if tag.0 != want.0 {
            *tag = want;
        }
    }
    // Two beats that never line up: it never settles to a steady lamp.
    if let Some(mut light) = f.glow.and_then(|g| parts.lights.get_mut(g).ok()) {
        let flicker = 0.78 + 0.22 * (t * 37.0).sin() * (t * 53.0 + 1.3).sin();
        light.intensity = plasma::LUMENS * (1.0 + (plasma::LUMENS_HOT - 1.0) * hot) * flicker;
    }
    let glow = 1.0 + STICKY_GLOW_SWELL * hot;
    fx.glow.spawn(dot(at, STICKY_HALO_M * glow, ION_HALO));
    fx.glow.spawn(dot(at, STICKY_CORE_M * glow, ION));
    if !still {
        trail(fx, at, vel, ION);
        return;
    }
    if super::table::lod(at.distance(fx.eye)) <= 0.0 {
        return;
    }
    f.arcs += dt * (STICKY_ARCS_HZ.0 + (STICKY_ARCS_HZ.1 - STICKY_ARCS_HZ.0) * hot);
    while f.arcs >= 1.0 {
        f.arcs -= 1.0;
        let d = fx.unit();
        fx.emit_tint(
            Layer::Arcs,
            1,
            at + d * STICKY_ARC_OUT_M,
            d,
            Color::WHITE,
            STICKY_ARC_SIZE,
        );
    }
    f.vapour += dt * STICKY_VAPOUR_HZ;
    while f.vapour >= 1.0 {
        f.vapour -= 1.0;
        fx.emit_tint(Layer::Wisp, 1, at, Vec3::Y, STICKY_VAPOUR, STICKY_VAPOUR_SIZE);
    }
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
    /// A sticky's ball and light, and how long it has been seen stuck, with
    /// what its arcs and vapour have owing.
    ball: Option<Entity>,
    glow: Option<Entity>,
    stuck: f32,
    arcs: f32,
    vapour: f32,
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
    parts: &mut BallParts,
    kit: Option<&plasma::Kit>,
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
                let e = spawn(commands, fx, models, kit, p);
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
        if p.kind == STICKY {
            let vel = Vec3::new(p.vel.x, p.vel.y, p.vel.z);
            sticky_frame(fx, &mut f, parts, at, vel, fresh, t, dt);
            continue;
        }
        // A fuse blinks red.
        let lit = (t * BLINK_HZ).fract() < 0.5;
        if lit && !f.lit {
            let c = [10.0, 1.5, 0.8, 1.0];
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

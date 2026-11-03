//! `?fxdemo=1` / `--fxdemo`: the effects staged in front of the camera, one
//! kind at a time, for screenshots and for tuning (`ci/client_shot.mjs`).
//! It makes up events and hands them to the effects only, never to the
//! session: the match is untouched. `?fxstage=N` plays only stage `N`:
//! 0 rifles, 1 sniper, 2 shotgun, 3 grenade, 4 a shield taking rounds and
//! breaking, 5 rounds into a wall, 6 a rocket and a wreck, 7 the shield
//! shell flaring on every fighter in view, 8 the SHARD GUN's shards curving
//! into a body, sticking, and bursting together, 9 ion bolts splashing, a
//! charged bolt and the cannon's ball.

use bevy::prelude::*;
use client_net::core::Drawn;
use proto::EventState;
use sim::arms::{AR, BR, MAGNUM, ROCKET, SHOTGUN, SNIPER};
use sim::events::{Cause, Kind};
use sim::geom::V3;

use super::{bodies, on_events, Fx, Scene};
use crate::LinkRes;

/// Each stage plays this long, firing every `EVERY_S`.
const STAGE_S: f32 = 2.5;
const EVERY_S: f32 = 0.3;
const STAGES: u32 = 10;
/// A slot no fighter has: its shots flash like anyone else's.
const SHOOTER: u8 = 200;

#[derive(Default)]
pub struct Clock {
    t: f32,
    next: f32,
    hits: u32,
}

pub fn run(
    link: NonSend<LinkRes>,
    time: Res<Time>,
    opts: Res<crate::Opts>,
    mut fx: ResMut<Fx>,
    mut clock: Local<Clock>,
) {
    clock.t += time.delta_secs();
    let stage = opts.fxstage.unwrap_or((clock.t / STAGE_S) as u32) % STAGES;
    if stage == 8 {
        shards(&link, &mut fx, &mut clock, time.elapsed_secs());
        return;
    }
    if stage == 9 {
        bolts(&link, &mut fx, &mut clock, time.elapsed_secs());
        return;
    }
    if clock.t < clock.next {
        return;
    }
    clock.next = clock.t + if stage == 3 || stage == 6 { 1.2 } else { EVERY_S };
    let core = &link.0.session().core;
    let fwd = (fx.cam * Vec3::NEG_Z).with_y(0.0).normalize_or(Vec3::X);
    let side = Vec3::new(-fwd.z, 0.0, fwd.x);
    let eye = fx.eye;
    let ground = |p: Vec3| Vec3::new(p.x, core.map.floor(p.x, p.z, 0.1, eye.y - 1.0), p.z);
    let shooter = eye + side * 3.5 + fwd * 4.0;
    let mut ev = Vec::new();
    let mut others = Vec::new();
    let shot = |arm: u8, from: Vec3, to: Vec3| EventState {
        kind: Kind::Shot as u8,
        a: SHOOTER,
        c: arm as u16,
        pos: v3(from),
        end: v3(to),
        ..default()
    };
    match stage {
        0 => {
            for k in 0..3 {
                let to = ground(eye + fwd * 10.0 + side * fx.range(-1.5, 1.5)) + Vec3::Y * 0.01 * k as f32;
                ev.push(shot(if k == 2 { BR } else { AR }, shooter, to));
            }
        }
        1 => {
            let from = eye - side * 5.0 + fwd * 5.0;
            ev.push(shot(SNIPER, from, ground(eye + fwd * 11.0 + side * 1.0)));
        }
        2 => {
            let at = eye + fwd * 10.0;
            for _ in 0..15 {
                let to = ground(at + side * fx.range(-1.5, 1.5) + fwd * fx.range(-1.0, 1.0));
                ev.push(shot(SHOTGUN, shooter, to));
            }
        }
        3 => ev.push(EventState {
            kind: Kind::Blast as u8,
            a: SHOOTER,
            c: Cause::Grenade as u16,
            pos: v3(ground(eye + fwd * 12.0) + Vec3::Y * 0.3),
            ..default()
        }),
        // A shielded body taking rounds, its shield breaking every few.
        4 => {
            let feet = ground(eye + fwd * 9.0);
            others.push(Drawn {
                slot: SHOOTER + 1,
                alive: true,
                shield: 200,
                pos: v3(feet),
                ..default()
            });
            clock.hits += 1;
            if clock.hits.is_multiple_of(5) {
                bodies::pop(&mut fx, feet);
            } else {
                let at = feet + Vec3::new(fx.range(-0.2, 0.2), fx.range(0.6, 1.6), fx.range(-0.2, 0.2));
                ev.push(shot(BR, shooter, at));
            }
        }
        // Every fighter in view: the shell flares, and breaks every few.
        7 => {
            clock.hits += 1;
            let now = time.elapsed_secs();
            for d in core.others(link.0.now_ms()) {
                if !d.alive {
                    continue;
                }
                if clock.hits.is_multiple_of(4) {
                    fx.was.raise(d.slot, 1.6, now);
                    bodies::pop(&mut fx, Vec3::new(d.pos.x, d.pos.y, d.pos.z));
                } else {
                    fx.was.raise(d.slot, 1.0, now);
                }
            }
        }
        // Rounds into whatever wall is ahead.
        5 => {
            for _ in 0..2 {
                let d = (fwd + side * fx.range(-0.25, 0.25) + Vec3::Y * fx.range(-0.15, 0.1)).normalize();
                let t = core.map.ray(v3(eye), v3(d), 60.0);
                ev.push(shot(MAGNUM, shooter, eye + d * t));
            }
        }
        _ => {
            let to = eye + fwd * 30.0;
            ev.push(shot(ROCKET, shooter, shooter + (to - shooter).normalize()));
            ev.push(EventState {
                kind: Kind::Wreck as u8,
                a: SHOOTER,
                pos: v3(ground(eye + fwd * 13.0 - side * 3.0)),
                ..default()
            });
        }
    }
    let scene = Scene {
        map: &core.map,
        others: &others,
        vehicles: &[],
        me: core.slot,
        t: time.elapsed_secs(),
    };
    on_events(&mut fx, &scene, &ev);
}

/// Where the demo's camera stands: west of the map's middle, a little
/// above the floor, looking east and a touch down, wherever we spawned.
pub fn pose(map: &sim::map::Map) -> Transform {
    let (cx, cz) = ((map.lo.0 + map.hi.0) * 0.5, (map.lo.1 + map.hi.1) * 0.5);
    let x = cx - 9.0;
    let floor = map.floor(x, cz, 0.3, 50.0);
    let eye = Vec3::new(x, floor + 2.2, cz);
    let to = Vec3::new(cx + 4.0, map.floor(cx, cz, 0.3, 50.0) + 0.6, cz);
    Transform::from_translation(eye).looking_at(to, Vec3::Y)
}

/// Stage 8, every frame: three shards curving from the right into a body
/// ahead, each arrival stuck in it, the gun's count of them going off.
fn shards(link: &LinkRes, fx: &mut Fx, clock: &mut Clock, now: f32) {
    const FLIGHT_S: f32 = 0.6;
    let core = &link.0.session().core;
    let fwd = (fx.cam * Vec3::NEG_Z).with_y(0.0).normalize_or(Vec3::X);
    let side = Vec3::new(-fwd.z, 0.0, fwd.x);
    let eye = fx.eye;
    let at = eye + fwd * 9.0;
    let feet = Vec3::new(at.x, core.map.floor(at.x, at.z, 0.1, eye.y - 1.0), at.z);
    let chest = feet + Vec3::Y * 1.1;
    let from = eye + side * 4.0 + fwd * 3.0;
    for k in 0..3 {
        let u = ((clock.t / FLIGHT_S) + k as f32 / 3.0).fract();
        // A curve: out to the side, then in.
        let bend = side * (2.5 * u * (1.0 - u)) + Vec3::Y * (1.5 * u * (1.0 - u));
        let p = from.lerp(chest, u) + bend;
        let ahead = from.lerp(chest, (u + 0.02).min(1.0)) + side * (2.5 * (u + 0.02) * (0.98 - u)) - p;
        super::flying::crystal(
            fx,
            &proto::ProjState {
                kind: super::flying::NEEDLE,
                owner: SHOOTER,
                pos: v3(p),
                vel: v3(ahead.normalize_or(fwd) * 30.0),
            },
        );
    }
    let d = Drawn {
        slot: SHOOTER + 1,
        alive: true,
        pos: v3(feet),
        ..default()
    };
    // An arrival every third of a flight.
    let arrivals = (clock.t / (FLIGHT_S / 3.0)) as u32;
    if arrivals != clock.hits {
        clock.hits = arrivals;
        let stick = sim::arms::arm(sim::arms::SHARDS).stick as u32;
        if arrivals % stick == stick - 1 {
            let scene = Scene {
                map: &core.map,
                others: &[d],
                vehicles: &[],
                me: core.slot,
                t: now,
            };
            super::blast::blast(fx, &scene, chest, sim::arms::SHARDS as u16, false);
        } else {
            super::bodies::needle_in(fx, d.slot, now);
            super::table::shatter(&mut fx.glow, chest, 3, 1.5);
        }
    }
    // The body the shards sit in (drawn as their glints).
    super::bodies::watch(fx, &[d], 0.0, now);
}

/// Stage 9, every frame: ion bolts from the right into the ground ahead,
/// splashing; every few, a charged bolt; and the cannon's ball going off.
fn bolts(link: &LinkRes, fx: &mut Fx, clock: &mut Clock, now: f32) {
    const FLIGHT_S: f32 = 0.35;
    let core = &link.0.session().core;
    let fwd = (fx.cam * Vec3::NEG_Z).with_y(0.0).normalize_or(Vec3::X);
    let side = Vec3::new(-fwd.z, 0.0, fwd.x);
    let eye = fx.eye;
    let at = eye + fwd * 10.0 - side * 1.5;
    let to = Vec3::new(at.x, core.map.floor(at.x, at.z, 0.1, eye.y - 1.0) + 0.05, at.z);
    let from = eye + side * 3.5 + fwd * 3.0;
    let n = (clock.t / (FLIGHT_S / 3.0)) as u32;
    for k in 0..3 {
        let u = ((clock.t / FLIGHT_S) + k as f32 / 3.0).fract();
        let charged = (n + k).is_multiple_of(6);
        let p = from.lerp(to, u);
        let v = (to - from).normalize_or(fwd) * if charged { 23.0 } else { 40.0 };
        let kind = if charged {
            super::flying::CHARGE
        } else {
            super::flying::BOLT
        };
        super::flying::light(
            fx,
            &proto::ProjState {
                kind,
                owner: SHOOTER,
                pos: v3(p),
                vel: v3(v),
            },
        );
    }
    if n != clock.hits {
        clock.hits = n;
        super::flying::splash(fx, to, (from - to).normalize_or(Vec3::Y), 1.0);
        // Every few arrivals, the cannon's ball goes off to the left.
        if n.is_multiple_of(9) {
            let scene = Scene {
                map: &core.map,
                others: &[],
                vehicles: &[],
                me: core.slot,
                t: now,
            };
            let ball = to - side * 4.0 + fwd * 2.0 + Vec3::Y * 0.5;
            super::blast::blast(fx, &scene, ball, sim::arms::CANNON as u16, false);
        }
    }
    // The ball itself, on its way, between bursts.
    let u = (clock.t / (FLIGHT_S * 3.0)).fract();
    let ball_to = to - side * 4.0 + fwd * 2.0 + Vec3::Y * 0.5;
    super::flying::light(
        fx,
        &proto::ProjState {
            kind: super::flying::CANNON,
            owner: SHOOTER,
            pos: v3(from.lerp(ball_to, u)),
            vel: v3((ball_to - from).normalize_or(fwd) * 20.0),
        },
    );
}

fn v3(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

//! `?fxdemo=1` / `--fxdemo`: the effects staged in front of the camera, one
//! kind at a time, for screenshots and for tuning (`ci/client_shot.mjs`).
//! It makes up events and hands them to the effects only, never to the
//! session: the match is untouched. `?fxstage=N` plays only stage `N`:
//! 0 rifles, 1 sniper, 2 shotgun, 3 grenade, 4 a shield taking rounds and
//! breaking, 5 rounds into a wall, 6 a rocket and a wreck, 7 the shield
//! shell flaring on every fighter in view.

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
const STAGES: u32 = 8;
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
    if clock.t < clock.next {
        return;
    }
    let stage = opts.fxstage.unwrap_or((clock.t / STAGE_S) as u32) % STAGES;
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

fn v3(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

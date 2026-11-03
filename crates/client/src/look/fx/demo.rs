//! `?fxdemo=1` / `--fxdemo`: the effects staged in front of the camera, one
//! kind at a time, for screenshots and for tuning (`ci/client_shot.mjs`).
//! It makes up events and hands them to the effects only, never to the
//! session: the match is untouched.

use bevy::prelude::*;
use proto::EventState;
use sim::arms::{AR, BR, ROCKET, SHOTGUN, SNIPER};
use sim::events::{Cause, Kind};
use sim::geom::V3;

use super::{bodies, on_events, Fx, Scene};
use crate::LinkRes;

/// Each stage plays this long, firing every `EVERY_S`.
const STAGE_S: f32 = 2.5;
const EVERY_S: f32 = 0.3;
const STAGES: u32 = 6;
/// A slot no fighter has: its shots flash like anyone else's.
const SHOOTER: u8 = 200;

#[derive(Default)]
pub struct Clock {
    t: f32,
    next: f32,
}

pub fn run(
    link: NonSend<LinkRes>,
    time: Res<Time>,
    mut fx: ResMut<Fx>,
    mut commands: Commands,
    mut clock: Local<Clock>,
) {
    clock.t += time.delta_secs();
    if clock.t < clock.next {
        return;
    }
    let stage = (clock.t / STAGE_S) as u32 % STAGES;
    clock.next = clock.t + if stage == 3 || stage == 5 { 1.2 } else { EVERY_S };
    let core = &link.0.session().core;
    let fwd = (fx.cam * Vec3::NEG_Z).with_y(0.0).normalize_or(Vec3::X);
    let side = Vec3::new(-fwd.z, 0.0, fwd.x);
    let eye = fx.eye;
    let ground = |p: Vec3| Vec3::new(p.x, core.map.floor(p.x, p.z, 0.1, eye.y - 1.0), p.z);
    let shooter = eye + side * 3.0 + fwd * 2.0;
    let mut ev = Vec::new();
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
                let to = ground(eye + fwd * 5.0 + side * fx.range(-1.5, 1.5)) + Vec3::Y * 0.01 * k as f32;
                ev.push(shot(if k == 2 { BR } else { AR }, shooter, to));
            }
        }
        1 => {
            let from = eye - side * 5.0 + fwd * 3.0;
            ev.push(shot(SNIPER, from, ground(eye + fwd * 6.0 + side * 1.5)));
        }
        2 => {
            let at = eye + fwd * 5.0;
            for _ in 0..15 {
                let to = ground(at + side * fx.range(-1.5, 1.5) + fwd * fx.range(-1.0, 1.0));
                ev.push(shot(SHOTGUN, shooter, to));
            }
        }
        3 => ev.push(EventState {
            kind: Kind::Blast as u8,
            a: SHOOTER,
            c: Cause::Grenade as u16,
            pos: v3(ground(eye + fwd * 8.0) + Vec3::Y * 0.3),
            ..default()
        }),
        4 => {
            let feet = ground(eye + fwd * 4.0);
            bodies::pop(&mut commands, &mut fx, feet);
        }
        _ => {
            let to = eye + fwd * 30.0;
            ev.push(shot(ROCKET, shooter, shooter + (to - shooter).normalize()));
            ev.push(EventState {
                kind: Kind::Wreck as u8,
                a: SHOOTER,
                pos: v3(ground(eye + fwd * 10.0 - side * 3.0)),
                ..default()
            });
        }
    }
    let scene = Scene {
        map: &core.map,
        others: &[],
        vehicles: &[],
        me: core.slot,
    };
    on_events(&mut commands, &mut fx, &scene, &ev);
}

fn v3(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

//! `?fxdemo=1` / `--fxdemo`: the effects staged in front of the camera, one
//! kind at a time, for screenshots and for tuning (`ci/client_shot.mjs`).
//! It makes up events and hands them to the effects only, never to the
//! session: the match is untouched. `?fxstage=N` plays only stage `N`:
//! 0 rifles, 1 sniper, 2 shotgun, 3 grenade, 4 a shield taking rounds and
//! breaking, 5 rounds into a wall, 6 a rocket and a wreck, 7 the shield
//! shell flaring on every fighter in view, 8 the SHARD GUN's shards curving
//! into a body, sticking, and bursting together, 9 ion bolts splashing, a
//! charged bolt and the cannon's ball, 10 marks on the face ahead (a round
//! pillar's, stood in front of with `fxstage=10`): blood sprayed onto it
//! from a body shot in front of it, a rifle butt's chip or dent, a sword's
//! gash, rounds' holes, all bent round it and fitted to its facets.

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
const STAGES: u32 = 11;
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
        10 => marks(core, &mut fx, &mut clock, &mut ev, &mut others, eye, fwd),
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
        aim: fx.cam * Vec3::NEG_Z,
        t: time.elapsed_secs(),
        on_body: None,
        arm: sim::arms::NONE,
    };
    on_events(&mut fx, &scene, &ev);
}

/// The face stage 10 marks: a round pillar in view, else the wall ahead,
/// else the ground.
#[derive(Clone, Copy)]
enum Face {
    Round { c: Vec2, r: f32 },
    Flat { p: Vec3, n: Vec3 },
}

impl Face {
    fn find(map: &sim::map::Map, eye: Vec3, fwd: Vec3) -> Face {
        let round = map
            .solids
            .iter()
            .filter_map(|s| match s.shape {
                sim::solid::Shape::Round { x, z, r } if s.y1 - s.y0 > 1.5 => Some((Vec2::new(x, z), r)),
                _ => None,
            })
            .filter(|(c, _)| {
                let to = Vec3::new(c.x - eye.x, 0.0, c.y - eye.z);
                to.length() < 30.0 && to.normalize_or(fwd).dot(fwd) > 0.7
            })
            .min_by(|a, b| {
                let d = |c: Vec2| c.distance(Vec2::new(eye.x, eye.z));
                d(a.0).total_cmp(&d(b.0))
            });
        if let Some((c, r)) = round {
            return Face::Round { c, r };
        }
        let mut near = super::surface::Near::default();
        match super::surface::cast(map, &mut near, eye, fwd, 40.0) {
            Some(h) => Face::Flat { p: h.at, n: h.normal },
            None => Face::Flat {
                p: Vec3::new(eye.x, map.ground(eye.x, eye.z), eye.z) + fwd * 8.0,
                n: Vec3::Y,
            },
        }
    }

    /// The point `k` metres along the face from its middle as seen from
    /// `eye` (round the pillar, or along the wall), and its outward normal.
    fn at(self, eye: Vec3, k: f32) -> (Vec3, Vec3) {
        match self {
            Face::Round { c, r } => {
                let a = (eye.z - c.y).atan2(eye.x - c.x) + k / r;
                let n = Vec3::new(a.cos(), 0.0, a.sin());
                (Vec3::new(c.x, eye.y, c.y) + n * r, n)
            }
            Face::Flat { p, n } => {
                let t = Vec3::Y.cross(n).normalize_or(Vec3::X);
                (p + t * k, n)
            }
        }
    }
}

/// Stage 10, every `EVERY_S`: in turn, a body without shields shot in front
/// of the face (its blood on it), a rifle butt and a sword swung at it, and
/// rounds into it.
#[allow(clippy::too_many_arguments)]
fn marks(
    core: &client_net::core::NetCore,
    fx: &mut Fx,
    clock: &mut Clock,
    ev: &mut Vec<EventState>,
    others: &mut Vec<Drawn>,
    eye: Vec3,
    fwd: Vec3,
) {
    let map = &core.map;
    let face = Face::find(map, eye, fwd);
    let floor = |p: Vec3| map.floor(p.x, p.z, 0.1, p.y + 1.0);
    let out = |n: Vec3| n.with_y(0.0).normalize_or(-fwd);
    clock.hits += 1;
    // The body, always there for the swings to keep clear of.
    let (p, n) = face.at(eye, -0.7);
    let body = p + out(n) * 1.0;
    let feet = Vec3::new(body.x, floor(body), body.z);
    others.push(Drawn {
        slot: SHOOTER + 1,
        alive: true,
        pos: v3(feet),
        ..default()
    });
    let shot = |arm: u8, from: Vec3, to: Vec3| EventState {
        kind: Kind::Shot as u8,
        a: SHOOTER,
        c: arm as u16,
        pos: v3(from),
        end: v3(to),
        ..default()
    };
    match clock.hits % 4 {
        0 => {
            let chest = feet + Vec3::Y * fx.range(0.9, 1.4);
            let from = chest + out(n) * 7.0 + Vec3::Y * 0.4;
            let at = chest + (from - chest).with_y(0.0).normalize_or(out(n)) * 0.3;
            ev.push(shot(BR, from, at));
        }
        k @ (1 | 2) => {
            let (p, n) = face.at(eye, if k == 1 { 0.6 } else { 1.5 });
            let stand = p + out(n) * 1.2;
            let feet = Vec3::new(stand.x, floor(stand), stand.z);
            let swing = feet + Vec3::Y * sim::consts::EYE;
            let target = Vec3::new(p.x, feet.y + fx.range(0.8, 1.5), p.z);
            let aim = (target - swing).normalize_or(-out(n));
            others.push(Drawn {
                slot: SHOOTER + 2,
                alive: true,
                pos: v3(feet),
                yaw: aim.z.atan2(aim.x),
                pitch: aim.y.clamp(-1.0, 1.0).asin(),
                ..default()
            });
            ev.push(EventState {
                kind: Kind::Melee as u8,
                a: SHOOTER + 2,
                c: if k == 2 { sim::arms::SWORD as u16 } else { 0 },
                pos: v3(swing),
                ..default()
            });
        }
        _ => {
            let (p, n) = face.at(eye, 0.0);
            let from = Vec3::new(p.x, feet.y + 1.6, p.z) + out(n) * 7.0;
            for _ in 0..2 {
                let (q, _) = face.at(eye, fx.range(-1.2, 1.2));
                let aim = Vec3::new(q.x, feet.y + fx.range(0.4, 2.2), q.z);
                let d = (aim - from).normalize_or(-out(n));
                let t = map.ray(v3(from), v3(d), 30.0);
                ev.push(shot(MAGNUM, from, from + d * t));
            }
        }
    }
}

/// Where the demo's camera stands: west of the map's middle, a little
/// above the floor, looking east and a touch down, wherever we spawned;
/// for stage 10 alone, in front of a round pillar near the middle, if the
/// map has one.
pub fn pose(map: &sim::map::Map, stage: Option<u32>) -> Transform {
    if stage == Some(10) {
        if let Some(t) = before_pillar(map) {
            return t;
        }
    }
    let (cx, cz) = ((map.lo.0 + map.hi.0) * 0.5, (map.lo.1 + map.hi.1) * 0.5);
    let x = cx - 9.0;
    let floor = map.floor(x, cz, 0.3, 50.0);
    let eye = Vec3::new(x, floor + 2.2, cz);
    let to = Vec3::new(cx + 4.0, map.floor(cx, cz, 0.3, 50.0) + 0.6, cz);
    Transform::from_translation(eye).looking_at(to, Vec3::Y)
}

/// Five metres out from the round pillar nearest the map's middle, at its
/// foot's level, where the view of it is clear.
fn before_pillar(map: &sim::map::Map) -> Option<Transform> {
    let mid = Vec2::new((map.lo.0 + map.hi.0) * 0.5, (map.lo.1 + map.hi.1) * 0.5);
    let mut rounds: Vec<(Vec2, f32, f32)> = map
        .solids
        .iter()
        .filter_map(|s| match s.shape {
            sim::solid::Shape::Round { x, z, r } if s.y1 - s.y0 > 1.5 => Some((Vec2::new(x, z), r, s.y0)),
            _ => None,
        })
        .collect();
    rounds.sort_by(|a, b| a.0.distance(mid).total_cmp(&b.0.distance(mid)));
    for (c, r, y0) in rounds {
        for k in 0..8 {
            let a = k as f32 / 8.0 * core::f32::consts::TAU;
            let at = c + Vec2::new(a.cos(), a.sin()) * (r + 5.0);
            let feet = map.floor(at.x, at.y, 0.3, y0 + 0.5);
            if (feet - y0).abs() > 1.0 || map.occupied(at.x, at.y, 0.3, feet + 0.1, feet + 1.9) {
                continue;
            }
            let eye = Vec3::new(at.x, feet + 1.7, at.y);
            let on = Vec3::new(c.x, feet + 1.2, c.y) + Vec3::new(a.cos(), 0.0, a.sin()) * r;
            if !map.clear(v3(eye), v3(on + Vec3::new(a.cos(), 0.0, a.sin()) * 0.05)) {
                continue;
            }
            return Some(Transform::from_translation(eye).looking_at(Vec3::new(c.x, feet + 1.2, c.y), Vec3::Y));
        }
    }
    None
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
                aim: fwd,
                t: now,
                on_body: None,
                arm: sim::arms::NONE,
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
                aim: fwd,
                t: now,
                on_body: None,
                arm: sim::arms::NONE,
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

//! Rounds in flight as drawn (DESIGN §6): on the render tick, from the
//! same snapshots as their shooters; run on as the sim flies them, stopped
//! at the map; never before they were sent; ids that hold; a stuck sticky
//! on its host as drawn; the cosmetic events played on the same tick; and
//! our own from the muzzle, gone at its ack if the server never made it.

use client_net::core::{step_own, DrawnProj, NetCore, EXTRAPOLATE_MS, LERP_SPAN_MS};
use client_net::proj::{self, coast, HOLD_MAX_MS, K_FRAG, K_NEEDLE, K_ROCKET, K_STICKY};
use proto::{EventState, ProjState, Remote, Snapshot};
use sim::arms::{CANNON, ION_RIFLE, LAUNCHER, MAGNUM, ROCKET, SHARDS};
use sim::consts::{ticks, MAX_FIGHTERS, TICK_HZ};
use sim::events::Kind;
use sim::fighter::Fighter;
use sim::geom::V3;
use sim::input::{Input, FIRE, GRENADE};
use sim::map::Map;
use sim::movement::Body;
use sim::vehicle::{Vehicle, PASSENGER};
use sim::world::{Phase, Projectile, Rules, World};

const TICK_MS: f64 = 1000.0 / TICK_HZ as f64;
/// Each snapshot lands this long after the server made it.
const LATE_MS: f64 = 40.0;

fn core() -> NetCore {
    NetCore::new(0, 0, 0, Rules::duel(), Map::open())
}

fn arrives(t: u32) -> f64 {
    t as f64 * TICK_MS + LATE_MS
}

fn base(t: u32) -> Snapshot {
    Snapshot {
        tick: t,
        phase: 1,
        ..Snapshot::default()
    }
}

/// Seat 1 running along x at 5 m/s.
fn runner_x(t: f64) -> f32 {
    (5.0 * t / TICK_HZ as f64) as f32
}

fn with_runner(t: u32) -> Snapshot {
    let mut s = base(t);
    s.remotes[0] = Remote {
        slot: 1,
        team: 1,
        alive: true,
        pos: V3::new(runner_x(t as f64), 0.0, 0.0),
        vel: V3::new(5.0, 0.0, 0.0),
        ..Remote::default()
    };
    s.n_remote = 1;
    s
}

fn add(s: &mut Snapshot, p: ProjState) {
    s.proj[s.n_proj as usize] = p;
    s.n_proj += 1;
}

fn add_event(s: &mut Snapshot, e: EventState) {
    s.events[s.n_events as usize] = e;
    s.n_events += 1;
}

/// Snapshots every other tick, each landing `LATE_MS` after it was made
/// (`snap` None: lost or never sent), and `each` every millisecond to
/// `until_ms`.
fn drive(
    c: &mut NetCore,
    until_ms: f64,
    snap: impl Fn(u32) -> Option<Snapshot>,
    mut each: impl FnMut(&mut NetCore, f64),
) {
    let mut t = 2;
    let mut now = 0.0;
    while now < until_ms {
        now += 1.0;
        while arrives(t) <= now {
            if let Some(s) = snap(t) {
                c.on_snapshot(now, &s);
            }
            t += 2;
        }
        each(c, now);
    }
}

fn rocket_of(owner: u8, pos: V3, vel: V3) -> ProjState {
    ProjState {
        kind: K_ROCKET,
        owner,
        pos,
        vel,
    }
}

/// The runner's rocket, fired from tick 100 at 30 m/s along x, flying to
/// tick 300.
fn rocket_x(t: f64) -> V3 {
    V3::new(runner_x(100.0) + (30.0 * (t - 100.0) / TICK_HZ as f64) as f32, 1.5, 0.0)
}

fn runner_and_rocket(t: u32) -> Option<Snapshot> {
    let mut s = with_runner(t);
    if (100..=300).contains(&t) {
        add(&mut s, rocket_of(1, rocket_x(t as f64), V3::new(30.0, 0.0, 0.0)));
    }
    Some(s)
}

fn remote(c: &NetCore, now: f64) -> Vec<DrawnProj> {
    c.projectiles(now).into_iter().filter(|p| p.owner != c.slot).collect()
}

#[test]
fn a_remote_rocket_is_drawn_on_its_shooters_tick() {
    let mut c = core();
    let mut checked = 0;
    drive(&mut c, 3000.0, runner_and_rocket, |c, now| {
        let rt = c.render_tick(now);
        if !(100.0..=300.0).contains(&rt) || rt > c.latest().tick as f64 {
            return;
        }
        let r = remote(c, now);
        assert_eq!(r.len(), 1, "at {rt}");
        let runner = c.others(now).into_iter().find(|d| d.slot == 1).unwrap();
        let truth = rocket_x(rt);
        let off = truth - V3::new(runner_x(rt), 0.0, 0.0);
        assert!(
            (r[0].pos - truth).len() < 2.0 / 64.0,
            "{:?} vs {truth:?} at {rt}",
            r[0].pos
        );
        assert!(
            (r[0].pos - runner.pos - off).len() < 2.0 / 64.0,
            "off its shooter: {:?} at {rt}",
            r[0].pos - runner.pos
        );
        checked += 1;
    });
    assert!(checked > 1000, "{checked}");
}

#[test]
fn a_rocket_moves_every_frame_not_every_snapshot() {
    let mut c = core();
    let mut last: Option<V3> = None;
    let mut steps = 0;
    drive(&mut c, 3000.0, runner_and_rocket, |c, now| {
        let rt = c.render_tick(now);
        let r = remote(c, now);
        if !(101.0..=299.0).contains(&rt) {
            last = r.first().map(|p| p.pos);
            return;
        }
        let p = r[0].pos;
        if let Some(l) = last {
            let step = (p - l).len();
            assert!(step > 0.0, "stood still at {rt}");
            assert!(step <= 30.0 * 0.001 * 1.6 + 0.01, "{step} at {rt}");
            steps += 1;
        }
        last = Some(p);
    });
    assert!(steps > 1000);
}

#[test]
fn a_round_whose_snapshots_stop_runs_on_then_holds() {
    let map = Map::open();
    let at = |t: u32| V3::new(-30.0 + 30.0 * (t as f32 - 300.0) / TICK_HZ as f32, 1.0, 0.0);
    let snap = |t: u32| {
        (t <= 400).then(|| {
            let mut s = with_runner(t);
            if t >= 300 {
                add(&mut s, rocket_of(1, at(t), V3::new(30.0, 0.0, 0.0)));
            }
            s
        })
    };
    let far = coast(
        K_ROCKET,
        at(400),
        V3::new(30.0, 0.0, 0.0),
        ticks(EXTRAPOLATE_MS) as f64,
        &map,
    )
    .0;
    let mut c = core();
    let end = arrives(400);
    drive(&mut c, end + 2500.0, snap, |c, now| {
        if now < end {
            return;
        }
        let r = remote(c, now);
        assert_eq!(r.len(), 1);
        assert!(r[0].pos.x <= far.x + 1e-4, "{:?} past {far:?}", r[0].pos);
        if (now - end - 400.0).abs() < 0.5 || (now - end - 2000.0).abs() < 0.5 {
            assert_eq!(r[0].pos, far, "held where it ran out");
        }
    });
}

#[test]
fn running_on_stops_at_the_map() {
    let map = Map::open();
    // A rocket at the wall at x = 50.
    let (p, v) = (V3::new(45.0, 1.0, 0.0), V3::new(40.0, 0.0, 0.0));
    let stop = 45.0 + map.ray(p, V3::new(1.0, 0.0, 0.0), 100.0) - 0.05;
    let mut held = false;
    for k in 0..400 {
        let (at, _, clipped) = coast(K_ROCKET, p, v, k as f64 * 0.25, &map);
        assert!(at.x <= stop + 1e-4, "{at:?} past {stop}");
        held |= clipped;
    }
    assert!(held);
    assert!((coast(K_ROCKET, p, v, 100.0, &map).0.x - stop).abs() < 0.05);
    // A frag falling to the floor.
    for k in 0..800 {
        let (at, _, _) = coast(
            K_FRAG,
            V3::new(0.0, 2.0, 0.0),
            V3::new(3.0, 0.0, 0.0),
            k as f64 * 0.5,
            &map,
        );
        assert!(at.y >= 0.0, "{at:?} under the floor");
    }
}

#[test]
fn a_blast_plays_the_frame_its_rocket_goes() {
    // Toward the wall at x = 50: in the frames to tick 298, gone from 300,
    // which carries its blast.
    const S: u32 = 300;
    let wall = V3::new(49.95, 1.0, 0.0);
    let at = |t: u32| wall - V3::new(30.0 * (S - 1 - t) as f32 / TICK_HZ as f32, 0.0, 0.0);
    let snap = |t: u32| {
        let mut s = with_runner(t);
        if (200..S).contains(&t) {
            add(&mut s, rocket_of(1, at(t), V3::new(30.0, 0.0, 0.0)));
        }
        if t == S {
            add_event(
                &mut s,
                EventState {
                    id: 1,
                    kind: Kind::Blast as u8,
                    a: 1,
                    c: ROCKET as u16,
                    pos: wall,
                    ..EventState::default()
                },
            );
        }
        Some(s)
    };
    let mut c = core();
    let (mut seen, mut gone, mut blast, mut last) = (false, None, None, V3::ZERO);
    drive(&mut c, 3500.0, snap, |c, now| {
        let r = remote(c, now);
        if let Some(p) = r.first() {
            assert!(gone.is_none(), "back after it went");
            seen = true;
            last = p.pos;
        } else if seen && gone.is_none() {
            gone = Some(now);
        }
        if c.drain_events(now).iter().any(|e| e.kind == Kind::Blast as u8) {
            assert!(blast.is_none());
            blast = Some(now);
        }
    });
    assert!(gone.is_some());
    assert_eq!(gone, blast, "the rocket goes the frame its blast plays");
    assert!((last - wall).len() < 0.1, "last drawn {last:?}, blast at {wall:?}");
}

#[test]
fn a_round_is_never_drawn_before_it_was_first_sent() {
    // Into sight at tick 200, mid-flight.
    let at = |t: u32| V3::new((30.0 * (t as f64 - 150.0) / TICK_HZ as f64) as f32, 1.0, 3.0);
    let snap = |t: u32| {
        let mut s = with_runner(t);
        if (200..=400).contains(&t) {
            add(&mut s, rocket_of(1, at(t), V3::new(30.0, 0.0, 0.0)));
        }
        Some(s)
    };
    let mut c = core();
    let mut first = None;
    drive(&mut c, 2500.0, snap, |c, now| {
        let rt = c.render_tick(now);
        let r = remote(c, now);
        if rt < 200.0 {
            assert!(r.is_empty(), "drawn at {rt}, before it was sent");
        } else if first.is_none() && !r.is_empty() {
            first = Some(r[0].pos);
        }
    });
    let p = first.expect("drawn");
    assert!((p - at(200)).len() < 0.05, "{p:?} vs {:?}", at(200));
}

/// Six rounds born on tick 100, for two seconds: two shards of one owner
/// 4 m apart, a dual pair of bolts 3 cm apart, and two owners' rockets
/// crossing.
fn crowd(t: u32) -> Option<Snapshot> {
    let mut s = with_runner(t);
    if !(100..=356).contains(&t) {
        return Some(s);
    }
    let dt = (t as f32 - 100.0) / TICK_HZ as f32;
    let mut put = |kind: u8, owner: u8, p0: V3, v: V3| {
        add(
            &mut s,
            ProjState {
                kind,
                owner,
                pos: p0 + v * dt,
                vel: v,
            },
        )
    };
    let x = V3::new(30.0, 0.0, 0.0);
    put(K_NEEDLE, 1, V3::new(-30.0, 1.0, 0.0), x);
    put(K_NEEDLE, 1, V3::new(-30.0, 1.0, 4.0), x);
    let bolt = proj::K_BOLT;
    put(bolt, 2, V3::new(-30.0, 1.5, -10.0), V3::new(20.0, 0.0, 0.0));
    put(bolt, 2, V3::new(-30.0, 1.5, -9.97), V3::new(20.0, 0.0, 0.05));
    put(K_ROCKET, 1, V3::new(-20.0, 2.0, -20.0), V3::new(15.0, 0.0, 15.0));
    put(K_ROCKET, 3, V3::new(20.0, 2.0, -20.0), V3::new(-15.0, 0.0, 15.0));
    Some(s)
}

#[test]
fn ids_hold_and_paths_never_cross() {
    let mut c = core();
    let mut ids: Vec<(u16, u8, u8)> = Vec::new();
    let mut last: Vec<(u16, V3)> = Vec::new();
    drive(&mut c, 3500.0, crowd, |c, now| {
        let r = remote(c, now);
        for p in &r {
            match ids.iter().find(|(id, _, _)| *id == p.id) {
                Some(&(_, kind, owner)) => assert_eq!((kind, owner), (p.kind, p.owner), "id {} changed hands", p.id),
                None => ids.push((p.id, p.kind, p.owner)),
            }
            if let Some((_, l)) = last.iter().find(|(id, _)| *id == p.id) {
                let step = (p.pos - *l).len();
                let v = p.vel.len();
                assert!(step <= v * 0.001 * 1.5 + 0.01, "id {} stepped {step} at {v} m/s", p.id);
            }
        }
        last = r.iter().map(|p| (p.id, p.pos)).collect();
    });
    assert_eq!(ids.len(), 6, "{ids:?}");
}

#[test]
fn a_one_frame_gap_is_bridged() {
    let snap = |t: u32| {
        let mut s = runner_and_rocket(t)?;
        if t == 200 {
            s.n_proj = 0;
        }
        Some(s)
    };
    let mut c = core();
    let mut ids = Vec::new();
    let mut drawn = 0;
    drive(&mut c, 3000.0, snap, |c, now| {
        let rt = c.render_tick(now);
        let r = remote(c, now);
        if (100.5..=300.0).contains(&rt) {
            assert_eq!(r.len(), 1, "not drawn at {rt}");
            drawn += 1;
        }
        for p in r {
            if !ids.contains(&p.id) {
                ids.push(p.id);
            }
        }
    });
    assert!(drawn > 1000);
    assert_eq!(ids.len(), 1, "{ids:?}");
}

#[test]
fn a_long_gap_is_not_drawn_across() {
    // To tick 200, then nothing for 400 ms, then on where it got to.
    let back = 200 + ticks(400) + 1;
    assert!(back - 200 > ticks(LERP_SPAN_MS));
    let snap = |t: u32| {
        if t > 200 && t < back {
            return None;
        }
        let mut s = with_runner(t);
        if t >= 150 {
            add(&mut s, rocket_of(1, rocket_x(t as f64), V3::new(30.0, 0.0, 0.0)));
        }
        Some(s)
    };
    let map = Map::open();
    let far = coast(
        K_ROCKET,
        rocket_x(200.0),
        V3::new(30.0, 0.0, 0.0),
        ticks(EXTRAPOLATE_MS) as f64,
        &map,
    )
    .0;
    let again = rocket_x(back as f64);
    assert!(again.x > far.x + 1.0);
    let mut c = core();
    let mut ids: Vec<u16> = Vec::new();
    drive(&mut c, 4000.0, snap, |c, now| {
        let r = remote(c, now);
        assert!(r.len() <= 1, "both drawn: {r:?}");
        for p in r {
            if !ids.contains(&p.id) {
                ids.push(p.id);
            }
            if ids.len() == 1 {
                assert!(p.pos.x <= far.x + 1e-4, "{:?} ran past {far:?}", p.pos);
            } else {
                assert!(p.pos.x >= again.x - 1e-4, "{:?} drawn across the gap", p.pos);
            }
        }
    });
    assert_eq!(ids.len(), 2, "{ids:?}");
}

#[test]
fn a_stuck_sticky_rides_its_host_as_drawn() {
    let off = V3::new(0.1, 1.2, 0.05);
    let sticky = |owner: u8, host: V3| ProjState {
        kind: K_STICKY,
        owner,
        pos: host + off,
        vel: off,
    };
    // On the runner: lerped, then run on and held when the snapshots stop.
    let snap = |t: u32| {
        (t <= 300).then(|| {
            let mut s = with_runner(t);
            let p = sticky(2, s.remotes[0].pos);
            add(&mut s, p);
            s
        })
    };
    let mut c = core();
    let mut checked = 0;
    drive(&mut c, 3000.0, snap, |c, now| {
        let r = remote(c, now);
        let Some(host) = c.others(now).into_iter().find(|d| d.slot == 1) else {
            return;
        };
        if c.render_tick(now) < 2.0 {
            return; // not yet at the first snapshot it came in
        }
        assert_eq!(r.len(), 1);
        assert!(
            (r[0].pos - (host.pos + off)).len() < 0.05,
            "{:?} off {:?}",
            r[0].pos,
            host.pos
        );
        checked += 1;
    });
    assert!(checked > 2000);
    // On us.
    let snap = |t: u32| {
        let mut s = base(t);
        s.own = Fighter {
            seated: true,
            alive: true,
            body: Body {
                pos: V3::new((2.0 * t as f64 / TICK_HZ as f64) as f32, 0.0, 1.0),
                grounded: true,
                ..Body::default()
            },
            ..Fighter::default()
        };
        let p = sticky(1, s.own.body.pos);
        add(&mut s, p);
        Some(s)
    };
    let mut c = core();
    let mut checked = 0;
    drive(&mut c, 2000.0, snap, |c, now| {
        let r = remote(c, now);
        if c.render_tick(now) < 2.0 {
            return;
        }
        assert_eq!(r.len(), 1);
        let me = c.own().1;
        assert!((r[0].pos - (me + off)).len() < 0.05, "{:?} off {me:?}", r[0].pos);
        checked += 1;
    });
    assert!(checked > 1000);
}

#[test]
fn events_play_on_the_render_tick() {
    let ev = |id: u16, kind: Kind, a: u8| EventState {
        id,
        kind: kind as u8,
        a,
        b: 0,
        ..EventState::default()
    };
    let snap = |t: u32| {
        let mut s = with_runner(t);
        if t == 200 {
            add_event(&mut s, ev(1, Kind::Shot, 1));
            add_event(&mut s, ev(2, Kind::Hit, 1));
            add_event(&mut s, ev(3, Kind::Kill, 1));
        }
        if t == 240 {
            add_event(&mut s, ev(4, Kind::Blast, 1));
            add_event(&mut s, ev(5, Kind::Spawn, 1));
            add_event(&mut s, ev(6, Kind::Blast, 0));
            // A hull we wrecked: drawn on the render tick till then.
            add_event(&mut s, ev(7, Kind::Wreck, 0));
        }
        Some(s)
    };
    let mut c = core();
    let mut got: Vec<(u16, f64, f64)> = Vec::new();
    drive(&mut c, 3000.0, snap, |c, now| {
        let rt = c.render_tick(now);
        for e in c.drain_events(now) {
            got.push((e.id, now, rt));
        }
    });
    let ids: Vec<u16> = got.iter().map(|g| g.0).collect();
    // Hits and kills at once, then what was held, in order.
    assert_eq!(ids, vec![2, 3, 1, 4, 5, 6, 7]);
    let when = |id: u16| got.iter().find(|g| g.0 == id).copied().unwrap();
    let came = |t: u32| (arrives(t)).ceil();
    for (id, t) in [(2, 200), (3, 200)] {
        assert_eq!(when(id).1, came(t), "event {id} at once");
    }
    for (id, t) in [(1, 200), (4, 240), (5, 240), (6, 240), (7, 240)] {
        let (_, now, rt) = when(id);
        assert!(now - came(t) <= HOLD_MAX_MS, "event {id} held too long");
        if now - came(t) < HOLD_MAX_MS {
            // On the first millisecond it was due.
            assert!(rt >= t as f64, "event {id} early at {rt}");
            assert!(rt - t as f64 <= 0.2, "event {id} late: {rt} for {t}");
        }
    }

    // The wreck of the hull we ride (drawn in the present) plays at once,
    // whoever did it.
    let snap = |t: u32| {
        let mut s = base(t);
        let hull = Vehicle {
            seats: [2, 1],
            ..Vehicle::new(V3::new(4.0, 0.0, 0.0), 0, 0, 0)
        };
        s.own = Fighter {
            ride: Fighter::ride_of(3, PASSENGER),
            body: hull.seat_body(PASSENGER),
            ..fighter(MAGNUM)
        };
        s.own_vehicle = Some(hull);
        if t == 200 {
            add_event(
                &mut s,
                EventState {
                    part: 3,
                    ..ev(1, Kind::Wreck, 1)
                },
            );
        }
        Some(s)
    };
    let mut c = core();
    let mut got = None;
    drive(&mut c, 2000.0, snap, |c, now| {
        if c.drain_events(now).iter().any(|e| e.kind == Kind::Wreck as u8) {
            got = Some(now);
        }
    });
    assert_eq!(got, Some(came(200)), "our hull's wreck at once");

    // Our own predicted shot plays at once.
    let me = fighter(MAGNUM);
    let mut c = NetCore::new(0, 0, 0, rules(MAGNUM), Map::open());
    let mut s = base(2);
    s.own = me;
    c.on_snapshot(10.0, &s);
    c.update(10.0, Input::default());
    c.drain_events(10.0);
    c.update(
        30.0,
        Input {
            buttons: FIRE,
            ..Input::default()
        },
    );
    let e = c.drain_events(30.0);
    assert!(e.iter().any(|e| e.kind == Kind::Shot as u8 && e.a == 0), "{e:?}");
}

fn rules(gun: u8) -> Rules {
    Rules {
        teams: false,
        score_limit: 1000,
        time_limit_ms: 0,
        loadout: [gun, MAGNUM],
        ..Rules::duel()
    }
}

/// A world on the open map, the countdown over, seat 0 at its spawn
/// facing +x with `gun`, seat 1 well out of the way.
fn world(gun: u8) -> World {
    let mut w = World::new(Map::open(), rules(gun), 1, (7, 8));
    w.seat(0, 0);
    w.seat(1, 1);
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &[0; MAX_FIGHTERS]);
    }
    w.fighters[1].body.pos = V3::new(0.0, 0.0, 45.0);
    w.fighters[0].grenades = 2;
    w.fighters[0].stickies = 2;
    w
}

fn fighter(gun: u8) -> Fighter {
    world(gun).fighters[0]
}

fn only(w: &World) -> Option<Projectile> {
    let mut live = w.proj.live().map(|(_, p)| *p);
    let p = live.next();
    assert!(live.next().is_none());
    p
}

/// What each launch does: a gun fired, or a throw (sticky or not).
#[derive(Clone, Copy, Debug)]
enum Shot {
    Gun(u8),
    Throw(bool),
}

const SHOTS: [Shot; 7] = [
    Shot::Gun(ROCKET),
    Shot::Gun(CANNON),
    Shot::Gun(ION_RIFLE),
    Shot::Gun(LAUNCHER),
    Shot::Gun(SHARDS),
    Shot::Throw(false),
    Shot::Throw(true),
];

/// Seat 0 launches `shot` aimed `pitch` up: the world after the tick it
/// went, and the round's seed as the client makes it from the fighter.
fn launched(shot: Shot, pitch: i16) -> (World, proj::Seed) {
    let mut w = world(match shot {
        Shot::Gun(g) => g,
        Shot::Throw(_) => MAGNUM,
    });
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0] = Input {
        pitch,
        ..Input::default()
    };
    w.step(&inputs, &[0; MAX_FIGHTERS]);
    if let Shot::Throw(true) = shot {
        w.fighters[0].gren_kind = 1;
    }
    inputs[0].buttons = match shot {
        Shot::Gun(_) => FIRE,
        Shot::Throw(_) => GRENADE,
    };
    w.step(&inputs, &[0; MAX_FIGHTERS]);
    // A throw leaves the hand a moment after the press.
    if let Shot::Throw(_) = shot {
        inputs[0].buttons = 0;
        for _ in 0..sim::consts::ticks(sim::consts::GREN_RELEASE_MS) {
            w.step(&inputs, &[0; MAX_FIGHTERS]);
        }
    }
    let f = &w.fighters[0];
    let seed = match shot {
        Shot::Gun(_) => proj::seed_fire(f, false, false).expect("a round"),
        Shot::Throw(sticky) => proj::seed_throw(f, sticky),
    };
    (w, seed)
}

#[test]
fn coast_steps_like_the_sim() {
    for shot in SHOTS {
        for pitch in [0i16, 2000] {
            let (mut w, _) = launched(shot, pitch);
            let mut p = only(&w).unwrap_or_else(|| panic!("{shot:?} launched nothing"));
            let idle = [Input::default(); MAX_FIGHTERS];
            let mut n = 0;
            loop {
                let (cp, cv, clipped) = coast(p.kind as u8, p.pos, p.vel, 1.0, &w.map);
                w.step(&idle, &[0; MAX_FIGHTERS]);
                let Some(q) = only(&w) else { break };
                if clipped {
                    break;
                }
                assert_eq!((cp, cv), (q.pos, q.vel), "{shot:?} tick {n}");
                p = q;
                n += 1;
            }
            // A bolt's life is its range (64 m/s, 18 ticks).
            assert!(n >= 15, "{shot:?}: only {n} ticks in the open");
        }
    }
}

#[test]
fn coasting_on_from_the_last_ask_is_coasting_from_the_seed() {
    // The rocket meets the far wall, the frag the floor.
    let map = Map::open();
    let memo = std::cell::Cell::new(None);
    for (kind, vel) in [(K_ROCKET, V3::new(0.0, 0.0, 30.0)), (K_FRAG, V3::new(3.0, 8.0, 12.0))] {
        let pos = V3::new(0.0, 1.5, 0.0);
        // On, back, on again, past where it stops, fractions between.
        let mut stopped = false;
        for t in [0.0, 1.0, 2.5, 2.5, 7.25, 3.0, 40.0, 41.5, 12.0, 300.0, 299.0, 1e9] {
            let want = coast(kind, pos, vel, t, &map);
            assert_eq!(proj::coast_on(&memo, kind, pos, vel, t, &map), want, "{kind} at {t}");
            stopped |= want.2;
        }
        assert!(stopped, "{kind} never met the map");
    }
}

#[test]
fn the_predicted_launch_is_the_sims() {
    for shot in SHOTS {
        let (w, seed) = launched(shot, 1000);
        let first = only(&w).unwrap();
        assert_eq!(seed.kind, first.kind as u8, "{shot:?}");
        let (p, v, _) = coast(seed.kind, seed.pos, seed.vel, 1.0, &w.map);
        if seed.cone == 0.0 {
            assert_eq!((p, v), (first.pos, first.vel), "{shot:?}");
        } else {
            let cos = v.norm().dot(first.vel.norm());
            assert!(cos >= (seed.cone * 1.01 + 1e-3).cos(), "{shot:?}: {cos}");
            assert!((v.len() - first.vel.len()).abs() < 1e-3, "{shot:?}");
            assert!((p - first.pos).len() < 1.0 * seed.cone + 0.01, "{shot:?}");
        }
    }
}

#[test]
fn a_launch_the_server_never_made_goes_at_its_ack() {
    // A server that runs our seq `tick + 2` on each tick and never fires.
    let me = fighter(ROCKET);
    let snap = move |t: u32| {
        let mut s = base(t);
        s.last_seq = t + 2;
        s.depth = 1;
        s.own = me;
        Some(s)
    };
    let mut c = NetCore::new(0, 0, 0, rules(ROCKET), Map::open());
    let mut q = None;
    let mut ack = None;
    let mut drawn = 0;
    let mut after = 0;
    let mut held: Option<V3> = None;
    let mut acked = 0;
    drive(&mut c, 2000.0, snap, |c, now| {
        if c.latest().tick == 0 {
            return;
        }
        let fire = q.is_none() && now > 500.0;
        let before = c.tick();
        c.update(
            now,
            Input {
                buttons: if fire { FIRE } else { 0 },
                ..Input::default()
            },
        );
        let mine: Vec<DrawnProj> = c.projectiles(now).into_iter().filter(|p| p.owner == 0).collect();
        if fire && c.tick() != before {
            assert_eq!(mine.len(), 1, "drawn the frame it fired");
            q = Some(before);
            // The first snapshot that ran it.
            let s = (before - 2).next_multiple_of(2);
            ack = Some(s);
        }
        let (Some(_), Some(ack)) = (q, ack) else {
            assert!(c.projectiles(now).is_empty());
            return;
        };
        if c.render_tick(now) < ack as f64 {
            assert_eq!(mine.len(), 1, "gone early");
            assert_eq!(mine[0].kind, K_ROCKET);
            drawn += 1;
            // Once the server has said it made nothing, it is held where it
            // was drawn: never flown on through whatever it met.
            if q.is_some_and(|q| c.latest().last_seq >= q) {
                let at = *held.get_or_insert(mine[0].pos);
                assert_eq!(mine[0].pos, at, "flown on after its ack");
                acked += 1;
            }
        } else {
            assert!(c.projectiles(now).is_empty(), "still drawn after its ack");
            after += 1;
        }
    });
    assert!(drawn > 10 && after > 100, "{drawn} {after}");
    assert!(acked > 10, "{acked}");
}

/// A server that runs our seq `t + 2` on tick `t` and echoes the fighter
/// we predict running along -x: `ours(q)` is it after seq `q`.
fn running_us() -> impl Fn(u32) -> Fighter {
    let map = Map::open();
    let mut after = vec![fighter(MAGNUM); 1024];
    for q in 8..after.len() {
        let mut f = after[q - 1];
        step_own(&mut f, &mut None, Phase::Playing, &running(), None, &map);
        after[q] = f;
    }
    move |q| after[q as usize]
}

fn running() -> Input {
    // Toward +x: from the spawn at x = -40 the open map's far edge is a
    // long run away (toward -x it is 10 m, and the run stops there).
    Input {
        yaw: 0,
        mz: 127,
        ..Input::default()
    }
}

/// A remote's sticky flies onto us as we run, and sticks. Our body is drawn
/// a round trip and a playout ahead of where the server had it: the sticky
/// flies onto us as drawn, never to where the server had us and then a
/// jump onto our drawn body.
#[test]
fn a_sticky_that_sticks_to_us_lands_on_us_as_drawn() {
    let ours = running_us();
    let off = V3::new(0.1, 1.2, 0.05);
    let w = V3::new(0.0, 0.0, -12.0);
    const STUCK: u32 = 300;
    let stuck_at = |t: u32| ours(t + 2).body.pos + off;
    let snap = |t: u32| {
        let mut s = base(t);
        s.last_seq = t + 2;
        s.depth = 1;
        s.own = ours(t + 2);
        if t < STUCK {
            let p = stuck_at(STUCK) - w * ((STUCK - t) as f32 / TICK_HZ as f32);
            add(
                &mut s,
                ProjState {
                    kind: K_STICKY,
                    owner: 1,
                    pos: p,
                    vel: w,
                },
            );
        } else if t < 500 {
            let p = stuck_at(t);
            add(
                &mut s,
                ProjState {
                    kind: K_STICKY,
                    owner: 1,
                    pos: p,
                    vel: off,
                },
            );
        }
        Some(s)
    };
    let mut c = NetCore::new(0, 0, 0, rules(MAGNUM), Map::open());
    let mut last: Option<(V3, V3)> = None;
    let (mut flying, mut on) = (0, 0);
    let mut lead = 0.0f32;
    drive(&mut c, 4000.0, snap, |c, now| {
        if c.latest().tick == 0 {
            return;
        }
        c.update(now, running());
        let me = c.own().1;
        let r = remote(c, now);
        let rt = c.render_tick(now);
        if r.is_empty() {
            last = None;
            return;
        }
        assert_eq!(r.len(), 1);
        let p = r[0].pos;
        if let Some((p0, me0)) = last {
            // A millisecond's flight, and our own step: nothing more.
            let step = (p - p0).len();
            let most = w.len() * 0.0015 + (me - me0).len() + 0.005;
            assert!(step <= most, "jumped {step} (at most {most}) at {rt}");
        }
        last = Some((p, me));
        if rt >= STUCK as f64 {
            assert!((p - (me + off)).len() < 1e-3, "{p:?} off us at {me:?}");
            on += 1;
        } else if rt >= (STUCK - 2) as f64 {
            flying += 1;
            lead = lead.max((me - (stuck_at(STUCK) - off)).len());
        }
    });
    assert!(flying > 5 && on > 100, "{flying} {on}");
    // The case at all: we were drawn well ahead of where the server had us.
    assert!(lead > 0.3, "drawn only {lead} m ahead");
}

/// A server that drives a truck along +x at full throttle from rest (seat 1
/// at the wheel), us in its bed with a rocket, firing on our seq `fire`;
/// it runs our seq `t + 2` on tick `t`: the snapshot it sends after tick
/// `t`, the stick sent with it.
fn riding_server(fire: u32) -> impl Fn(u32) -> Option<Snapshot> {
    use sim::vehicle::{DRIVER, NO_TURRET};
    let mut w = world(ROCKET);
    let hull = Vehicle {
        seats: [2, 1],
        ..Vehicle::new(V3::new(-45.0, 0.0, 0.0), 0, 0, NO_TURRET)
    };
    w.vehicles[0] = hull;
    for (s, seat) in [(0, PASSENGER), (1, DRIVER)] {
        w.fighters[s].ride = Fighter::ride_of(0, seat);
        w.fighters[s].body = hull.seat_body(seat);
    }
    let mut snaps = Vec::new();
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[1].mz = 127;
    for t in 0..600u32 {
        inputs[0].buttons = if t + 2 == fire { FIRE } else { 0 };
        w.step(&inputs, &[0; MAX_FIGHTERS]);
        let mut s = base(t);
        s.last_seq = t + 2;
        s.depth = 1;
        s.own = w.fighters[0];
        s.own_vehicle = Some(w.vehicles[0]);
        s.drive = w.drive_stick(0);
        for (_, p) in w.proj.live() {
            add(&mut s, rocket_of(p.owner, p.pos, p.vel));
        }
        snaps.push(s);
    }
    move |t| snaps.get(t as usize).copied()
}

/// A passenger predicts the hull it rides on its driver's stick, so what it
/// fires leaves the muzzle it draws, at once, as on foot; and the server's
/// round takes it over without a pop.
#[test]
fn a_passengers_round_leaves_the_hull_it_predicts() {
    const FIRE_SEQ: u32 = 200;
    let snap = riding_server(FIRE_SEQ);
    let fastest = (0..600)
        .filter_map(&snap)
        .flat_map(|s| s.proj().to_vec())
        .map(|p| p.vel.len());
    let fastest = fastest.fold(0.0f32, f32::max);
    assert!(fastest > 20.0, "the server never fired: {fastest}");
    let mut c = NetCore::new(0, 0, 0, rules(ROCKET), Map::open());
    let mut fired = false;
    let mut ids: Vec<u16> = Vec::new();
    let mut last: Option<V3> = None;
    let mut drawn = 0;
    drive(&mut c, 3000.0, snap, |c, now| {
        if c.latest().tick == 0 {
            return;
        }
        let made = c.tick();
        let press = made == FIRE_SEQ;
        c.update(
            now,
            Input {
                buttons: if press { FIRE } else { 0 },
                ..Input::default()
            },
        );
        let mine: Vec<DrawnProj> = c.projectiles(now).into_iter().filter(|p| p.owner == 0).collect();
        if press && c.tick() != made {
            fired = true;
            assert_eq!(c.launches(), 1, "not launched from our muzzle");
            assert_eq!(mine.len(), 1, "not drawn the frame it fired");
            let seat = c.own().1;
            let off = (mine[0].pos - seat).len();
            assert!(off < 2.5, "drawn {off} m from the drawn seat");
        }
        for p in &mine {
            if !ids.contains(&p.id) {
                ids.push(p.id);
            }
            if let Some(p0) = last {
                let step = (p.pos - p0).len();
                assert!(step <= fastest * 0.0015 + 0.01, "jumped {step} at {now}");
            }
            last = Some(p.pos);
            drawn += 1;
        }
        if mine.is_empty() {
            last = None;
        }
    });
    assert!(fired, "never fired");
    assert!(drawn > 50, "drawn on only {drawn} frames");
    assert_eq!(ids.len(), 1, "{ids:?}");
    // The hull was the one predicted on its driver's stick every time.
    assert_eq!(c.stats.hull_corrections, 0, "{:?}", c.stats);
    assert!(c.stats.confirmations > 100, "{:?}", c.stats);
}

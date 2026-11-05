//! Vehicles as a player meets them: getting in and out, driving as the
//! driver's client predicts it, running people down, shooting a hull (as
//! the shooter saw it), blowing it up, and its coming back; and bots that
//! drive.

use sim::bot::Bot;
use sim::consts::*;
use sim::events::{Cause, Kind as Ev};
use sim::fmath;
use sim::geom::{rad_to_yaw, V3};
use sim::input::*;
use sim::map::Map;
use sim::vehicle::{DRIVER, PASSENGER};
use sim::world::{Phase, Rules, World};

const NO_LAG: [u32; MAX_FIGHTERS] = [0; MAX_FIGHTERS];

fn dry_fork() -> Map {
    sim::maps::canyon::DEF.build()
}

/// Two fighters on Dry Fork, countdown over: 0 (red) by red's first
/// vehicle, 1 (blue) far off.
fn world(teams: bool) -> World {
    let rules = Rules {
        teams,
        score_limit: 100,
        time_limit_ms: 0,
        ..Rules::doubles()
    };
    let mut w = World::new(dry_fork(), rules, 1, (5, 6));
    w.seat(0, 0);
    w.seat(1, 1);
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &NO_LAG);
    }
    let v = w.vehicles[0].pos;
    w.fighters[0].body.pos = V3::new(v.x - 2.0, w.map.ground(v.x - 2.0, v.z), v.z);
    w.fighters[1].body.pos = V3::new(50.0, 0.0, 0.0);
    w
}

fn press(w: &mut World, s: usize, inp: Input) {
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[s] = inp;
    w.step(&inputs, &NO_LAG);
}

fn tap_use(w: &mut World, s: usize) {
    press(
        w,
        s,
        Input {
            buttons: USE,
            ..Input::default()
        },
    );
    press(w, s, Input::default());
}

#[test]
fn use_gets_in_at_the_wheel_then_out_beside_it() {
    let mut w = world(false);
    tap_use(&mut w, 0);
    assert_eq!(w.fighters[0].riding(), Some((0, DRIVER)));
    assert_eq!(w.vehicles[0].seats[DRIVER], 1);
    assert_eq!(w.fighters[0].body.pos, w.vehicles[0].mount(DRIVER));
    // A second gets the passenger's seat.
    w.fighters[1].body.pos = w.vehicles[0].pos + V3::new(-2.5, 0.0, 0.0);
    tap_use(&mut w, 1);
    assert_eq!(w.fighters[1].riding(), Some((0, PASSENGER)));
    tap_use(&mut w, 0);
    assert_eq!(w.fighters[0].riding(), None);
    assert_eq!(w.vehicles[0].seats[DRIVER], 0);
    let d = (w.fighters[0].body.pos - w.vehicles[0].pos).flat().len();
    assert!(d > VEH_HALF_W && d < 3.0, "beside it: {d}");
}

#[test]
fn a_driver_steers_and_cannot_shoot_and_a_passenger_can() {
    let mut w = world(false);
    tap_use(&mut w, 0);
    let shots = w.fighters[0].shots;
    let start = w.vehicles[0].pos;
    for _ in 0..TICK_HZ * 2 {
        press(
            &mut w,
            0,
            Input {
                buttons: FIRE,
                mz: 127,
                mx: 60,
                ..Input::default()
            },
        );
    }
    let v = w.vehicles[0];
    assert!((v.pos - start).flat().len() > 10.0, "it drove: {:?}", v.pos);
    assert!(v.yaw != 0, "it turned");
    assert_eq!(w.fighters[0].shots, shots, "the driver's hands are on the wheel");
    assert_eq!(w.fighters[0].body.pos, v.mount(DRIVER));
}

/// What a driver's client runs (fighter, then vehicle, then the seat) must
/// match the server to the bit, over slopes, the creek and boxes.
#[test]
fn the_vehicle_prediction_matches_the_server_bit_for_bit() {
    let mut w = world(false);
    tap_use(&mut w, 0);
    let map = w.map.clone();
    let mut mine = w.fighters[0];
    let mut car = w.vehicles[0];
    let mut rng = sim::rng::Pcg32::new(3, 3);
    let mut moved = 0.0f32;
    // Stuck against something: back up, turning the other way.
    let (mut stuck, mut backing) = (0u32, 0u32);
    for t in 0..TICK_HZ * 40 {
        let r = rng.next_u32();
        // Toward a point that wanders round the middle, with a shaky hand
        // and now and then a burst of reverse.
        let goal = V3::new(
            fmath::sin(t as f32 * 0.002) * 50.0,
            0.0,
            fmath::cos(t as f32 * 0.003) * 20.0,
        );
        let to = goal - w.vehicles[0].pos;
        let mut diff = fmath::atan2(to.z, to.x) / core::f32::consts::TAU - w.vehicles[0].yaw as f32 / 65536.0;
        diff -= fmath::round(diff);
        let shake = ((r >> 16) % 61) as f32 - 30.0;
        stuck = if w.vehicles[0].vel.flat().len() < 1.0 {
            stuck + 1
        } else {
            0
        };
        if stuck > TICK_HZ / 2 {
            backing = TICK_HZ;
        }
        backing = backing.saturating_sub(1);
        let steer = (diff * 6.0).clamp(-1.0, 1.0) * 97.0 + shake;
        let inp = Input {
            buttons: (r as u16) & (RELOAD | SWAP),
            yaw: (t * 31) as u16,
            pitch: 0,
            mx: if backing > 0 { -steer } else { steer } as i8,
            mz: if backing > 0 || (r >> 8).is_multiple_of(16) {
                -127
            } else {
                127
            },
        }
        .sanitized();
        let before = w.vehicles[0].pos;
        press(&mut w, 0, inp);
        mine.step_local(&inp, &map);
        car.step_local(&inp, &map);
        mine.body = car.seat_body(DRIVER);
        assert_eq!(car, w.vehicles[0], "tick {t}");
        assert_eq!(mine.body, w.fighters[0].body, "tick {t}");
        moved += (w.vehicles[0].pos - before).flat().len();
    }
    assert!(moved > 300.0, "it went places: {moved} m");
}

#[test]
fn a_fast_vehicle_runs_down_an_enemy_on_foot() {
    let mut w = world(false);
    tap_use(&mut w, 0);
    let v = w.vehicles[0];
    let (f, _) = v.axes();
    // Blue stands 25 m ahead of the hood.
    let at = v.pos + f * 25.0;
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    let mut died = None;
    for _ in 0..TICK_HZ * 3 {
        press(
            &mut w,
            0,
            Input {
                mz: 127,
                ..Input::default()
            },
        );
        if let Some(e) = w.events.as_slice().iter().find(|e| e.kind == Ev::Kill) {
            died = Some(*e);
            break;
        }
    }
    let e = died.expect("ran down");
    assert_eq!((e.a, e.b, e.c), (0, 1, Cause::Splatter as u16));
}

#[test]
fn shots_wreck_a_hull_and_its_riders_and_it_comes_back() {
    let mut w = world(false);
    tap_use(&mut w, 0);
    // Blue with a rifle 20 m off its hood.
    let v = w.vehicles[0];
    let (f, _) = v.axes();
    let at = v.pos + f * 20.0;
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    let mut wrecked = false;
    for _ in 0..TICK_HZ * 30 {
        let hull = w.vehicles[0].pos + V3::new(0.0, 0.5, 0.0);
        let d = hull - w.fighters[1].eye();
        let yaw = rad_to_yaw(fmath::atan2(d.z, d.x));
        let pitch = fmath::round(fmath::atan2(d.y, d.flat().len()) * (65536.0 / core::f32::consts::TAU)) as i16;
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        inputs[1] = Input {
            buttons: if w.tick.is_multiple_of(2) { FIRE } else { 0 },
            yaw,
            pitch,
            ..Input::default()
        };
        if w.fighters[1].gun().mag == 0 {
            inputs[1].buttons |= RELOAD;
        }
        w.step(&inputs, &NO_LAG);
        if w.events.as_slice().iter().any(|e| e.kind == Ev::Wreck) {
            wrecked = true;
            break;
        }
    }
    assert!(wrecked, "health left {}", w.vehicles[0].health);
    assert!(!w.vehicles[0].active);
    assert!(!w.fighters[0].alive, "its driver went with it");
    assert_eq!(w.fighters[0].riding(), None);
    let idle = [Input::default(); MAX_FIGHTERS];
    for _ in 0..ticks(31_000) {
        w.step(&idle, &NO_LAG);
    }
    assert!(w.vehicles[0].active, "back after its respawn");
    assert_eq!(w.vehicles[0].pos, w.map.vehicles[0].pos);
}

/// The cab covers its driver: a shot at where they sit meets the hull.
/// The bed doesn't cover its passenger, whose chest and head stand over it.
#[test]
fn the_cab_covers_its_driver_and_the_bed_not_its_passenger() {
    let rules = Rules {
        teams: false,
        score_limit: 100,
        time_limit_ms: 0,
        ..Rules::doubles()
    };
    let mut w = World::new(dry_fork(), rules, 1, (5, 6));
    for s in 0..3 {
        w.seat(s, s as u8 & 1);
    }
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &NO_LAG);
    }
    let v = w.vehicles[0].pos;
    for s in [0, 2] {
        w.fighters[s].body.pos = V3::new(v.x - 2.0, w.map.ground(v.x - 2.0, v.z), v.z);
        tap_use(&mut w, s);
    }
    assert_eq!(w.fighters[0].riding(), Some((0, DRIVER)));
    assert_eq!(w.fighters[2].riding(), Some((0, PASSENGER)));
    // Blue 12 m off its flank, its eye level with what it aims at, fires
    // once.
    let shoot = |w: &mut World, at: V3| {
        let (_, r) = w.vehicles[0].axes();
        let from = at + r * 12.0;
        w.fighters[1].body.pos = V3::new(from.x, from.y - EYE, from.z);
        let d = at - w.fighters[1].eye();
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        inputs[1] = Input {
            buttons: FIRE,
            yaw: rad_to_yaw(fmath::atan2(d.z, d.x)),
            pitch: fmath::round(fmath::atan2(d.y, d.flat().len()) * (65536.0 / core::f32::consts::TAU)) as i16,
            ..Input::default()
        };
        w.step(&inputs, &NO_LAG);
        // The burst over and the trigger let go before the next.
        for _ in 0..TICK_HZ {
            w.step(&[Input::default(); MAX_FIGHTERS], &NO_LAG);
        }
    };
    let hull = w.vehicles[0].health;
    let head = w.vehicles[0].mount(DRIVER) + V3::new(0.0, HEAD_Y_CROUCH, 0.0);
    shoot(&mut w, head);
    assert!(w.vehicles[0].health < hull, "the hull took the shot at its driver");
    assert_eq!(w.fighters[0].shield, SHIELD_MAX, "and the driver none of it");
    let hull = w.vehicles[0].health;
    let chest = w.vehicles[0].mount(PASSENGER) + V3::new(0.0, CHEST_Y, 0.0);
    shoot(&mut w, chest);
    assert!(w.fighters[2].shield < SHIELD_MAX, "the passenger is over the hull");
    assert_eq!(w.vehicles[0].health, hull, "which that shot went over");
}

/// A shot is checked against the hull where the shooter saw it: rewound
/// with it, so a shot at where it was 25 ticks ago (4 m back at speed)
/// hits with that much lag and misses without.
#[test]
fn a_shot_meets_the_hull_where_the_shooter_saw_it() {
    let run = |lag_ticks: u32| {
        let mut w = world(false);
        tap_use(&mut w, 0);
        let go = Input {
            mz: 127,
            ..Input::default()
        };
        // Three seconds: up the hill beside red's base and over its top, at
        // speed again on the way down.
        for _ in 0..TICK_HZ * 3 {
            press(&mut w, 0, go);
        }
        let mut seen = Vec::new();
        for _ in 0..30 {
            press(&mut w, 0, go);
            seen.push(w.vehicles[0].pos);
        }
        // The pose written at the end of the tick 25 before the next.
        let then = seen[seen.len() - 25];
        let (f, r) = w.vehicles[0].axes();
        assert!((w.vehicles[0].pos - then).dot(f) > VEH_HALF_L + 1.0, "it moved on");
        // Blue high over its flank, aiming at where it was.
        let at = then + r * 12.0;
        w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z) + 6.0, at.z);
        let d = then + V3::new(0.0, 0.7, 0.0) - w.fighters[1].eye();
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        inputs[0] = go;
        inputs[1] = Input {
            buttons: FIRE,
            yaw: rad_to_yaw(fmath::atan2(d.z, d.x)),
            pitch: fmath::round(fmath::atan2(d.y, d.flat().len()) * (65536.0 / core::f32::consts::TAU)) as i16,
            ..Input::default()
        };
        let mut lag = NO_LAG;
        lag[1] = lag_ticks << 8;
        let health = w.vehicles[0].health;
        w.step(&inputs, &lag);
        w.vehicles[0].health < health
    };
    assert!(run(25), "the hull where blue saw it was hit");
    assert!(!run(0), "where it is now, it wasn't");
}

#[test]
fn bots_drive_on_dry_fork() {
    let rules = Rules {
        score_limit: 1000,
        time_limit_ms: 0,
        ..Rules::big_team()
    };
    let mut w = World::new(dry_fork(), rules, 3, (1, 2));
    let mut bots: Vec<Bot> = (0..MAX_FIGHTERS).map(|s| Bot::new(3, s, 0.6)).collect();
    for s in 0..MAX_FIGHTERS {
        w.seat(s, (s & 1) as u8);
    }
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let (mut driven, mut rides, mut kills) = (0u32, 0u32, 0u32);
    for _ in 0..30_000 {
        for (s, b) in bots.iter_mut().enumerate() {
            inputs[s] = b.think(&w, s);
        }
        w.step(&inputs, &NO_LAG);
        driven += w
            .vehicles
            .iter()
            .filter(|v| v.active && v.driver().is_some() && v.vel.flat().len() > 3.0)
            .count() as u32;
        for e in w.events.as_slice() {
            rides += (e.kind == Ev::Ride && e.c == 0) as u32;
            kills += (e.kind == Ev::Kill) as u32;
        }
    }
    assert!(rides >= 4, "bots got in {rides} times");
    assert!(driven > TICK_HZ * 20, "bots drove for {} s", driven / TICK_HZ);
    assert!(kills >= 20, "and still fought: {kills} kills");
}

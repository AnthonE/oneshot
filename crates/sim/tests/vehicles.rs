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

/// A rocket beside a truck throws it: up off its wheels and away from
/// the blast, a second or more in the air, and down again on the ground a
/// few metres off, hurt but whole.
#[test]
fn a_rocket_throws_a_truck() {
    let mut w = world(false);
    w.fighters[0].body.pos = V3::new(-50.0, 0.0, 0.0);
    let v = w.vehicles[0];
    let (_, r) = v.axes();
    // Blue with a rocket, 14 m off its right flank, aimed at the ground
    // under its door.
    let at = v.pos + r * 14.0;
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    w.fighters[1].guns[w.fighters[1].cur as usize & 1] = sim::fighter::Gun::full(sim::arms::ROCKET);
    let door = v.pos + r * (VEH_HALF_W + 0.3);
    let d = V3::new(door.x, w.map.ground(door.x, door.z) + 0.2, door.z) - w.fighters[1].eye();
    let yaw = rad_to_yaw(fmath::atan2(d.z, d.x));
    let pitch = fmath::round(fmath::atan2(d.y, d.flat().len()) * (65536.0 / core::f32::consts::TAU)) as i16;
    let aim = Input {
        yaw,
        pitch,
        ..Input::default()
    };
    press(&mut w, 1, Input { buttons: FIRE, ..aim });
    let (mut air, mut top, mut landed) = (0u32, 0.0f32, false);
    for _ in 0..TICK_HZ * 4 {
        press(&mut w, 1, aim);
        let h = &w.vehicles[0];
        let off = h.pos.y - w.map.ground(h.pos.x, h.pos.z);
        top = top.max(off);
        if !h.grounded {
            air += 1;
        } else if air > 0 {
            landed = true;
        }
    }
    let h = w.vehicles[0];
    assert!(h.active && h.health < VEH_HEALTH, "hurt, not wrecked: {}", h.health);
    assert!(top > 2.0, "thrown {top} m up");
    assert!(air as f32 * DT > 1.0, "{} s in the air", air as f32 * DT);
    assert!(landed && h.grounded, "down again");
    let gone = (h.pos - v.pos).flat();
    assert!(gone.len() > 5.0, "thrown {} m", gone.len());
    assert!(gone.dot(r) < 0.0, "away from the blast");
}

/// The truck's steel stops a shot and its glass doesn't: a driver's chest
/// is behind the door, their head behind the window; the bed's sides are
/// higher than the sill, and a passenger's chest stands over them.
#[test]
fn steel_stops_a_shot_and_glass_does_not() {
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
    let chest = w.vehicles[0].mount(DRIVER) + V3::new(0.0, CHEST_Y_CROUCH, 0.0);
    shoot(&mut w, chest);
    assert!(
        w.vehicles[0].health < hull,
        "the door took the shot at its driver's chest"
    );
    assert_eq!(w.fighters[0].shield, SHIELD_MAX, "and the driver none of it");
    let hull = w.vehicles[0].health;
    let head = w.vehicles[0].mount(DRIVER) + V3::new(0.0, HEAD_Y_CROUCH, 0.0);
    shoot(&mut w, head);
    assert!(w.fighters[0].shield < SHIELD_MAX, "the window stopped nothing");
    assert_eq!(w.vehicles[0].health, hull, "and the hull took none of that");
    // Between the sill and the bed's rail, behind the cab: steel.
    let hull = w.vehicles[0].health;
    let (f, _) = w.vehicles[0].axes();
    let rail = w.vehicles[0].pos + f * (VEH_BED_X - 1.5) + V3::new(0.0, (VEH_SILL + VEH_BED_H) / 2.0, 0.0);
    shoot(&mut w, rail);
    assert!(w.vehicles[0].health < hull, "the bed's side took it");
    assert_eq!(w.fighters[2].shield, SHIELD_MAX, "not the passenger standing behind it");
    let hull = w.vehicles[0].health;
    let chest = w.vehicles[0].mount(PASSENGER) + V3::new(0.0, CHEST_Y, 0.0);
    shoot(&mut w, chest);
    assert!(
        w.fighters[2].shield < SHIELD_MAX,
        "the passenger is over the bed's sides"
    );
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

/// Dry Fork's minigun trucks: its second vehicle and its mirror.
fn turret_truck(w: &World) -> usize {
    (0..w.vehicles.len())
        .find(|&i| w.vehicles[i].active && w.vehicles[i].turret == sim::vehicle::MINIGUN)
        .expect("a minigun truck on Dry Fork")
}

/// A gunner holds the minigun's trigger at an enemy off its hood: the
/// barrels spin up first, then it pours rounds (never its own gun's)
/// from the turret's muzzle and kills; held on, it runs hot and vents.
/// What its client predicts of the turret is the server's to the bit.
#[test]
fn a_gunner_spins_up_the_minigun_kills_and_it_runs_hot() {
    let mut w = world(false);
    let i = turret_truck(&w);
    let v = w.vehicles[i];
    // Seat 0 gets in at the bed: the wheel's taken by nobody, so put it
    // in the gunner's seat the way USE would with a driver in.
    w.vehicles[i].seats[DRIVER] = 2;
    w.fighters[0].body.pos = v.pos + v.axes().0 * -3.0;
    tap_use(&mut w, 0);
    w.vehicles[i].seats[DRIVER] = 0;
    assert_eq!(w.fighters[0].riding(), Some((i, PASSENGER)));
    assert_eq!(w.fighters[0].gunning(&w.map), sim::vehicle::MINIGUN);
    assert_eq!(w.fighters[0].body.pos, w.vehicles[i].mount(PASSENGER));
    // Blue 18 m off its hood.
    let (f, _) = v.axes();
    let at = v.pos + f * 18.0;
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    let map = w.map.clone();
    let mut mine = w.fighters[0];
    let mag = w.fighters[0].gun().mag;
    let (mut first, mut killed, mut rounds) = (None, None, 0);
    for t in 0..ticks(4000) {
        let d = w.fighters[1].body.pos + V3::new(0.0, CHEST_Y, 0.0) - w.fighters[0].eye();
        let inp = Input {
            buttons: FIRE,
            yaw: rad_to_yaw(fmath::atan2(d.z, d.x)),
            pitch: fmath::round(fmath::atan2(d.y, d.flat().len()) * (65536.0 / core::f32::consts::TAU)) as i16,
            ..Input::default()
        };
        press(&mut w, 0, inp);
        mine.step_local(&inp, &map);
        mine.body = w.fighters[0].body;
        assert_eq!(mine.mount, w.fighters[0].mount, "tick {t}");
        for e in w.events.as_slice() {
            if e.kind == Ev::Shot && e.a == 0 {
                assert_eq!(e.c, Cause::Minigun as u16);
                let muzzle = w.vehicles[i].muzzle((e.end - e.pos).norm());
                assert!((e.pos - muzzle).len() < 0.2, "from the muzzle");
                first.get_or_insert(t);
                rounds += 1;
            }
            if e.kind == Ev::Kill && e.b == 1 {
                assert_eq!(e.c, Cause::Minigun as u16);
                killed.get_or_insert(t);
            }
        }
        if killed.is_some() {
            break;
        }
    }
    let spin = ticks(sim::arms::MINIGUN.spin_ms);
    let first = first.expect("it fired");
    assert!(first + 1 >= spin, "spun up first: {first} of {spin}");
    let killed = killed.expect("it killed") - first;
    assert!(killed < ticks(2500), "{killed} ticks, {rounds} rounds");
    assert_eq!(w.fighters[0].gun().mag, mag, "its own gun never fired");
    // Held on at nothing, it runs hot and vents: a pause in its rounds.
    let mut quiet = 0;
    for _ in 0..ticks(8000) {
        press(
            &mut w,
            0,
            Input {
                buttons: FIRE,
                ..Input::default()
            },
        );
        quiet = if w.events.as_slice().iter().any(|e| e.kind == Ev::Shot && e.a == 0) {
            0
        } else {
            quiet + 1
        };
        if quiet > ticks(1000) {
            break;
        }
    }
    assert!(w.fighters[0].mount.vent, "vented: {:?}", w.fighters[0].mount);
}

//! Vehicles as a player meets them: getting in and out, driving as the
//! driver's client predicts it, running people down, shooting a hull (as
//! the shooter saw it), blowing it up, and its coming back; and bots that
//! drive.

use sim::bot::Bot;
use sim::consts::*;
use sim::contact::TOP_EPS;
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

/// From beside its cab, USE on a minigun truck takes the wheel; from beside
/// its bed, the gun; and with the gun taken, the wheel from anywhere.
#[test]
fn use_takes_the_seat_its_nearest_on_a_turret_truck() {
    let mut w = world(false);
    let i = turret_truck(&w);
    let v = w.vehicles[i];
    let (f, r) = v.axes();
    w.fighters[0].body.pos = v.pos + f * 0.5 + r * -2.0;
    tap_use(&mut w, 0);
    assert_eq!(w.fighters[0].riding(), Some((i, DRIVER)), "at the wheel from its cab");
    tap_use(&mut w, 0);
    assert_eq!(w.fighters[0].riding(), None);
    w.fighters[0].body.pos = v.pos + f * -2.0 + r * 2.0;
    tap_use(&mut w, 0);
    assert_eq!(w.fighters[0].riding(), Some((i, PASSENGER)), "on the gun from its bed");
    w.fighters[1].body.pos = v.pos + f * -2.0 + r * -2.0;
    tap_use(&mut w, 1);
    assert_eq!(w.fighters[1].riding(), Some((i, DRIVER)), "the wheel, the gun taken");
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
    // Seat 0 walks up to its bed and gets on the gun, though the wheel is
    // free: USE takes the seat it is nearest.
    w.fighters[0].body.pos = v.pos + v.axes().0 * -3.0;
    tap_use(&mut w, 0);
    assert_eq!(w.fighters[0].riding(), Some((i, PASSENGER)), "on the gun from its bed");
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

/// A driver drives a vehicle at a waypoint: steering toward it, the
/// throttle held to `top` m/s.
fn toward(v: &sim::vehicle::Vehicle, gx: f32, gz: f32, top: f32) -> Input {
    let to = V3::new(gx - v.pos.x, 0.0, gz - v.pos.z);
    let mut diff = fmath::atan2(to.z, to.x) / core::f32::consts::TAU - v.yaw as f32 / 65536.0;
    diff -= fmath::round(diff);
    Input {
        mx: ((diff * 8.0).clamp(-1.0, 1.0) * 127.0) as i8,
        mz: if v.vel.flat().len() < top { 127 } else { 0 },
        ..Input::default()
    }
}

/// Up red's ramp and onto its base's roof, at speed or slowly: the nose
/// rides the slope ahead of the middle, never shoved off its foot.
#[test]
fn a_truck_drives_up_the_ramp_onto_the_base() {
    for top in [8.0, VEH_TOP] {
        let mut w = world(false);
        tap_use(&mut w, 0);
        let start = (-135.0, 0.0);
        let path = [(-124.0, 0.0), (-115.0, 0.0), (-104.0, 0.0)];
        w.vehicles[0].pos = V3::new(start.0, w.map.ground(start.0, start.1), start.1);
        w.vehicles[0].yaw = 0;
        let mut k = 0;
        for _ in 0..TICK_HZ * 6 {
            let v = w.vehicles[0];
            let (gx, gz) = path[k];
            if (V3::new(gx, 0.0, gz) - v.pos).flat().len() < 2.0 {
                k += 1;
                if k == path.len() {
                    break;
                }
                continue;
            }
            press(&mut w, 0, toward(&v, gx, gz, top));
        }
        let v = w.vehicles[0];
        assert_eq!(k, path.len(), "at {top} m/s it stopped at {:?}", v.pos);
        assert!(v.pos.y > 4.4 && v.grounded, "on the roof: {:?}", v.pos);
    }
}

/// Two trucks driven into each other don't pass through: their hulls stay
/// apart, the one hit is shoved on, and the one that hit slows.
#[test]
fn trucks_crash_into_each_other() {
    let mut w = world(false);
    tap_use(&mut w, 0);
    let v = w.vehicles[0];
    let (f, _) = v.axes();
    let at = v.pos + f * 14.0;
    w.vehicles[1].pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    w.vehicles[1].yaw = v.yaw.wrapping_add(16384);
    w.vehicles[1].vel = V3::ZERO;
    let parked = w.vehicles[1].pos;
    let (mut top, mut hit) = (0.0f32, false);
    for _ in 0..TICK_HZ * 3 {
        press(
            &mut w,
            0,
            Input {
                mz: 127,
                ..Input::default()
            },
        );
        let (a, b) = (w.vehicles[0], w.vehicles[1]);
        let mut near = f32::MAX;
        for ca in a.circles() {
            for cb in b.circles() {
                near = near.min((cb - ca).flat().len());
            }
        }
        assert!(near > 2.0 * VEH_HULL_R - 0.05, "hulls through each other: {near}");
        let speed = a.vel.flat().len();
        top = top.max(speed);
        hit |= (b.pos - parked).flat().len() > 0.5;
    }
    assert!(hit, "the parked truck was shoved");
    assert!(top > 10.0, "it got going: {top}");
    assert!(w.vehicles[0].health < VEH_HEALTH, "the one that hit is hurt");
    assert!(w.vehicles[1].health < VEH_HEALTH, "and the one it hit");
    assert!(w.vehicles[0].active && w.vehicles[1].active, "not wrecked by one ram");
}

/// Head on at full speed, both go up and their riders with them.
#[test]
fn a_head_on_crash_at_speed_wrecks_both() {
    let mut w = world(false);
    tap_use(&mut w, 0);
    let v = w.vehicles[0];
    let (f, _) = v.axes();
    let at = v.pos + f * 30.0;
    w.vehicles[1].pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    w.vehicles[1].yaw = v.yaw.wrapping_add(32768);
    // Blue at the other's wheel.
    let b = w.vehicles[1].pos - f * 2.0;
    w.fighters[1].body.pos = V3::new(b.x, w.map.ground(b.x, b.z), b.z);
    tap_use(&mut w, 1);
    assert_eq!(w.fighters[1].riding(), Some((1, DRIVER)));
    // Both already at full speed, at each other.
    w.vehicles[0].vel = f * VEH_TOP;
    w.vehicles[1].vel = f * -VEH_TOP;
    let mut wrecks = 0;
    let go = Input {
        mz: 127,
        ..Input::default()
    };
    for _ in 0..TICK_HZ * 2 {
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        inputs[0] = go;
        inputs[1] = go;
        w.step(&inputs, &NO_LAG);
        wrecks += w.events.as_slice().iter().filter(|e| e.kind == Ev::Wreck).count();
    }
    assert_eq!(wrecks, 2, "both wrecked");
    assert!(!w.fighters[0].alive && !w.fighters[1].alive, "their drivers with them");
}

/// A slow truck hurts, not kills; the one it meets is carried off ahead,
/// hit once, not dragged.
#[test]
fn a_slow_truck_hurts_and_carries_off() {
    let mut w = world(false);
    tap_use(&mut w, 0);
    let v = w.vehicles[0];
    let (f, _) = v.axes();
    let at = v.pos + f * 8.0;
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    let mut hits = 0;
    for _ in 0..TICK_HZ * 2 {
        let fast = w.vehicles[0].vel.flat().len() > 3.6;
        press(
            &mut w,
            0,
            Input {
                // Held near 3.6 m/s till it hits, then braked.
                mz: if hits > 0 {
                    -127
                } else if fast {
                    0
                } else {
                    127
                },
                ..Input::default()
            },
        );
        hits += w
            .events
            .as_slice()
            .iter()
            .filter(|e| e.kind == Ev::Hit && e.b == 1)
            .count();
    }
    assert!(w.fighters[1].alive, "a slow hit is not a kill");
    assert!(
        w.fighters[1].health + w.fighters[1].shield < HEALTH_MAX + SHIELD_MAX,
        "but it hurts"
    );
    assert_eq!(hits, 1, "hit once");
}

/// Every tick's hash through five scripted contacts, folded: a fast truck
/// through a still enemy; the same at a driver's depth through one
/// strafing across (met where the driver saw them); a slow carry-off; a
/// teammate spared; two trucks head-on into a wreck. Pinned so a change
/// to how contacts are stepped that should change nothing changes nothing
/// (Dry Fork's golden has bots that seldom meet).
const CONTACTS: u64 = 0xde8c_6286_a99b_aee8;

#[test]
fn run_downs_and_crashes_hash_as_before() {
    let mut fold = 0u64;
    // Each scenario's splatters (kills or hits of seat 1) and wrecks.
    let mut run =
        |w: &mut World, ticks: u32, inputs: &dyn Fn(u32, &World) -> [Input; MAX_FIGHTERS], lag: [u32; MAX_FIGHTERS]| {
            let (mut splats, mut wrecks) = (0, 0);
            for t in 0..ticks {
                let i = inputs(t, w);
                w.step(&i, &lag);
                fold = sim::rng::splitmix64(fold ^ w.hash());
                for e in w.events.as_slice() {
                    // A hit's `c` is its damage; a kill's, its cause.
                    let splat = e.kind == Ev::Hit || (e.kind == Ev::Kill && e.c == Cause::Splatter as u16);
                    splats += (splat && e.b == 1) as u32;
                    wrecks += (e.kind == Ev::Wreck) as u32;
                }
            }
            (splats, wrecks)
        };
    let go = |mz: i8| Input { mz, ..Input::default() };
    // 1. Fast, through a still enemy 25 m ahead.
    let mut w = world(false);
    tap_use(&mut w, 0);
    let (f, r) = w.vehicles[0].axes();
    let at = w.vehicles[0].pos + f * 25.0;
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    let one = run(
        &mut w,
        TICK_HZ * 3,
        &|_, _| {
            let mut i = [Input::default(); MAX_FIGHTERS];
            i[0] = go(127);
            i
        },
        NO_LAG,
    );
    assert!(one.0 >= 1 && !w.fighters[1].alive, "1: {one:?}");
    // 2. At 8 ticks' depth, through one strafing across its path.
    let mut w = world(false);
    tap_use(&mut w, 0);
    let at = w.vehicles[0].pos + f * 20.0 - r * 0.6;
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    w.fighters[1].yaw = w.vehicles[0].yaw;
    let mut lag = NO_LAG;
    lag[0] = 8 << 8;
    let two = run(
        &mut w,
        TICK_HZ * 3,
        &|t, w| {
            let mut i = [Input::default(); MAX_FIGHTERS];
            i[0] = go(127);
            i[1] = Input {
                yaw: w.vehicles[0].yaw,
                mx: if (t / 20) % 2 == 0 { 127 } else { -127 },
                ..Input::default()
            };
            i
        },
        lag,
    );
    assert!(two.0 >= 1, "2: {two:?}");
    // 3. Slow, near 3.6 m/s, into one 8 m ahead, braked once it hurt them.
    let mut w = world(false);
    tap_use(&mut w, 0);
    let at = w.vehicles[0].pos + f * 8.0;
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    let three = run(
        &mut w,
        TICK_HZ * 2,
        &|_, w| {
            let mut i = [Input::default(); MAX_FIGHTERS];
            let fast = w.vehicles[0].vel.flat().len() > 3.6;
            let hurt = w.fighters[1].health + w.fighters[1].shield < HEALTH_MAX + SHIELD_MAX;
            i[0] = go(if hurt {
                -127
            } else if fast {
                0
            } else {
                127
            });
            i
        },
        NO_LAG,
    );
    assert!(three.0 >= 1 && w.fighters[1].alive, "3: {three:?}");
    // 4. Sides on, friendly fire off: a teammate 20 m ahead is spared.
    let mut w = world(true);
    w.rules.friendly_fire = false;
    w.fighters[1].team = 0;
    tap_use(&mut w, 0);
    let at = w.vehicles[0].pos + f * 20.0;
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    let four = run(
        &mut w,
        TICK_HZ * 3,
        &|_, _| {
            let mut i = [Input::default(); MAX_FIGHTERS];
            i[0] = go(127);
            i
        },
        NO_LAG,
    );
    assert!(four.0 == 0 && w.fighters[1].health == HEALTH_MAX, "4: {four:?}");
    // 5. Head-on at full speed, both driven.
    let mut w = world(false);
    tap_use(&mut w, 0);
    let v = w.vehicles[0];
    let at = v.pos + f * 30.0;
    w.vehicles[1].pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    w.vehicles[1].yaw = v.yaw.wrapping_add(32768);
    let b = w.vehicles[1].pos - f * 2.0;
    w.fighters[1].body.pos = V3::new(b.x, w.map.ground(b.x, b.z), b.z);
    tap_use(&mut w, 1);
    w.vehicles[0].vel = f * VEH_TOP;
    w.vehicles[1].vel = f * -VEH_TOP;
    let five = run(
        &mut w,
        TICK_HZ * 2,
        &|_, _| {
            let mut i = [Input::default(); MAX_FIGHTERS];
            i[0] = go(127);
            i[1] = go(127);
            i
        },
        NO_LAG,
    );
    assert_eq!(five.1, 2, "5: {five:?}");
    assert_eq!(fold, CONTACTS, "contacts fold to {fold:#018x}");
}

/// A run-down seen at share 0 (the victim drawn on the driver's present,
/// predicted) meets the body where it is, never where it stood the
/// driver's depth ago; at the whole share, where the driver saw it.
fn run_down_seen(share: u8) -> u32 {
    let mut w = world(false);
    tap_use(&mut w, 0);
    let (f, r) = w.vehicles[0].axes();
    let at = w.vehicles[0].pos + f * 14.0;
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    let mut lag = sim::world::Lag::past(NO_LAG);
    lag.depth_q8[0] = 10 << 8;
    lag.seen[0].share[1] = share;
    let (mut moved, mut splats) = (false, 0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0] = Input {
        mz: 127,
        ..Input::default()
    };
    for _ in 0..TICK_HZ * 2 {
        // Just before the hood reaches it, it is somewhere else (as if it
        // dived clear in the driver's last few ticks).
        let ahead = (w.fighters[1].body.pos - w.vehicles[0].pos).dot(f);
        if !moved && ahead < VEH_HALF_L + BODY_R + 0.8 {
            let p = w.fighters[1].body.pos + r * 4.0;
            w.fighters[1].body.pos = V3::new(p.x, w.map.ground(p.x, p.z), p.z);
            moved = true;
        }
        w.step_with(&inputs, &lag);
        splats += w
            .events
            .as_slice()
            .iter()
            .filter(|e| e.kind == Ev::Hit && e.b == 1)
            .count() as u32;
    }
    assert!(moved);
    splats
}

#[test]
fn a_run_down_at_share_zero_meets_the_body_where_it_is() {
    assert!(
        run_down_seen(SHARE_FULL) > 0,
        "the whole share: met where the driver saw it"
    );
    assert_eq!(run_down_seen(0), 0, "none of it: it is clear");
}

/// Whether a body at `p` overlaps hull `v`'s footprint (as solid hulls
/// put bodies out of it).
fn in_hull(v: &sim::vehicle::Vehicle, p: V3) -> bool {
    let (s, c) = sim::geom::yaw_sincos(v.yaw);
    let hull = sim::solid::Solid {
        shape: sim::solid::Shape::Turned {
            x: v.pos.x,
            z: v.pos.z,
            hx: VEH_HALF_L,
            hz: VEH_HALF_W,
            c,
            s,
        },
        y0: v.pos.y,
        y1: v.pos.y + VEH_H,
    };
    hull.push(p.x, p.z, BODY_R - 0.01).is_some()
}

/// Running at a parked truck's side: stopped at it, never through it, and
/// the truck never moves for it.
#[test]
fn a_parked_truck_stops_a_runner_at_its_side() {
    let mut w = world(false);
    let v = w.vehicles[0];
    let (_, r) = v.axes();
    let at = v.pos + r * 6.0;
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    let toward = rad_to_yaw(fmath::atan2(-r.z, -r.x));
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[1] = Input {
        yaw: toward,
        mz: 127,
        ..Input::default()
    };
    for _ in 0..TICK_HZ * 3 {
        w.step(&inputs, &NO_LAG);
        assert!(!in_hull(&w.vehicles[0], w.fighters[1].body.pos), "ran into the hull");
    }
    let side = (w.fighters[1].body.pos - v.pos).flat().dot(r);
    assert!(fmath::fabs(side - (VEH_HALF_W + BODY_R)) < 0.05, "stopped {side} m out");
    assert_eq!(w.vehicles[0].pos, v.pos, "the hull gave way");
}

/// A truck too slow to run anyone down pushes them ahead of it, unhurt.
#[test]
fn a_creeping_truck_pushes_without_hurting() {
    let mut w = world(false);
    tap_use(&mut w, 0);
    let (f, _) = w.vehicles[0].axes();
    let at = w.vehicles[0].pos + f * 6.0;
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    let start = w.fighters[1].body.pos;
    for _ in 0..TICK_HZ * 5 {
        let slow = w.vehicles[0].vel.flat().len() < 2.0;
        press(
            &mut w,
            0,
            Input {
                mz: if slow { 127 } else { 0 },
                ..Input::default()
            },
        );
        assert!(!in_hull(&w.vehicles[0], w.fighters[1].body.pos), "inside the hull");
    }
    let pushed = (w.fighters[1].body.pos - start).flat().dot(f);
    assert!(pushed > 4.0, "pushed only {pushed} m");
    let f1 = &w.fighters[1];
    assert!(
        f1.alive && f1.health == HEALTH_MAX && f1.shield == SHIELD_MAX,
        "hurt: {f1:?}"
    );
}

/// Pinned between a truck and a wall: left where the wall holds it,
/// unhurt; the truck drives as if it weren't there.
#[test]
fn a_body_against_a_wall_is_left_where_the_wall_holds_it() {
    let run = |body: bool| {
        let rules = Rules {
            score_limit: 100,
            time_limit_ms: 0,
            ..Rules::doubles()
        };
        let mut w = World::new(Map::open(), rules, 1, (5, 6));
        w.seat(0, 0);
        w.seat(1, 1);
        let idle = [Input::default(); MAX_FIGHTERS];
        while w.phase == Phase::Countdown {
            w.step(&idle, &NO_LAG);
        }
        // Truck 0 near the east wall (x = 50), creeping at it under
        // SPLAT_SPEED; seat 1 between, its back to the wall.
        let hull = sim::vehicle::Vehicle {
            seats: [1, 0],
            vel: V3::new(2.0, 0.0, 0.0),
            ..sim::vehicle::Vehicle::new(V3::new(44.0, 0.0, 0.0), 0, 0, sim::vehicle::NO_TURRET)
        };
        w.vehicles[0] = hull;
        w.fighters[0].ride = sim::fighter::Fighter::ride_of(0, DRIVER);
        w.fighters[0].body = hull.seat_body(DRIVER);
        w.fighters[1].body.pos = if body {
            V3::new(49.5, 0.0, 0.0)
        } else {
            V3::new(0.0, 0.0, 40.0)
        };
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        let mut track = Vec::new();
        for _ in 0..TICK_HZ * 3 {
            // Creeping: never fast enough to run anyone down.
            inputs[0].mz = if w.vehicles[0].vel.flat().len() < 2.0 { 30 } else { 0 };
            w.step(&inputs, &NO_LAG);
            track.push(w.vehicles[0].pos);
            assert!(w.fighters[1].body.pos.x <= 50.0 - BODY_R + 0.01, "into the wall");
        }
        (track, w.fighters[1])
    };
    let (with, f1) = run(true);
    let (without, _) = run(false);
    assert_eq!(with, without, "the truck gave way to a body");
    assert!(f1.alive && f1.health == HEALTH_MAX, "hurt: {f1:?}");
}

/// Getting out lands clear of the hull, from either seat; getting in works
/// from every side, the solid hull notwithstanding.
#[test]
fn getting_in_from_every_side_and_out_clear_of_the_hull() {
    for side in 0..4 {
        let mut w = world(false);
        let v = w.vehicles[0];
        let (f, r) = v.axes();
        let off = [f * (VEH_HALF_L + 0.5), f * -(VEH_HALF_L + 0.5), r * 1.8, r * -1.8][side];
        let at = v.pos + off;
        w.fighters[0].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
        press(&mut w, 0, Input::default());
        assert!(
            !in_hull(&w.vehicles[0], w.fighters[0].body.pos),
            "side {side}: in the hull to start"
        );
        tap_use(&mut w, 0);
        assert!(w.fighters[0].riding().is_some(), "side {side}: couldn't get in");
        out_clear(&mut w, 0, ["side 0", "side 1", "side 2", "side 3"][side]);
    }
    // From the passenger's seat, with someone at the wheel.
    let mut w = world(false);
    w.vehicles[0].seats[DRIVER] = 2;
    w.fighters[1].ride = sim::fighter::Fighter::ride_of(0, DRIVER);
    tap_use(&mut w, 0);
    assert_eq!(w.fighters[0].riding(), Some((0, PASSENGER)));
    out_clear(&mut w, 0, "the passenger");
}

/// Seat `s` gets out: on the tick it does, where getting out puts it
/// (beside the wheel, or behind the bed), clear of the hull with nothing
/// pushing it out; and clear still a moment on.
fn out_clear(w: &mut World, s: usize, what: &str) {
    let (_, seat) = w.fighters[s].riding().unwrap();
    press(
        w,
        s,
        Input {
            buttons: USE,
            ..Input::default()
        },
    );
    assert!(w.fighters[s].riding().is_none(), "{what}: couldn't get out");
    let v = w.vehicles[0];
    let (f, r) = v.axes();
    let spot = if seat == DRIVER {
        v.pos - r * (VEH_HALF_W + 0.9)
    } else {
        v.pos - f * (VEH_HALF_L + 0.9)
    };
    let at = w.fighters[s].body.pos;
    assert!((at - spot).flat().len() < 0.01, "{what}: out at {at:?}, not {spot:?}");
    assert!(!in_hull(&v, at), "{what}: out into the hull");
    for _ in 0..TICK_HZ / 2 {
        press(w, s, Input::default());
    }
    assert!(
        !in_hull(&w.vehicles[0], w.fighters[s].body.pos),
        "{what}: into the hull"
    );
}

/// Dropped onto a parked truck: on its top, never through it and pushed
/// out a side.
#[test]
fn a_body_dropped_onto_a_truck_lands_on_it() {
    let mut w = world(false);
    let v = w.vehicles[0];
    w.fighters[1].body.pos = v.pos + V3::new(0.3, VEH_H + 1.5, 0.2);
    w.fighters[1].body.grounded = false;
    let start = w.fighters[1].body.pos;
    let idle = [Input::default(); MAX_FIGHTERS];
    let mut landed = false;
    for _ in 0..TICK_HZ {
        w.step(&idle, &NO_LAG);
        let b = w.fighters[1].body;
        let flat = (b.pos - start).flat().len();
        assert!(flat < 0.1, "pushed {flat} m flat");
        assert!(b.pos.y >= v.pos.y + VEH_H - TOP_EPS - 0.01, "through the top: {b:?}");
        landed |= b.grounded && fmath::fabs(b.pos.y - (v.pos.y + VEH_H)) < 0.01;
    }
    assert!(landed, "never stood on it: {:?}", w.fighters[1].body);
    // Stood on it every tick, on it and never a hair inside.
    for _ in 0..TICK_HZ / 2 {
        w.step(&idle, &NO_LAG);
        let b = w.fighters[1].body;
        assert!(b.grounded && b.pos.y >= v.pos.y + VEH_H, "sinking: {b:?}");
    }
    // Its enemy drives it off flat out from under them: left standing where
    // they were, then down off its tail, never run down, shoved or hurt.
    let at = w.fighters[1].body.pos;
    tap_use(&mut w, 0);
    assert_eq!(w.fighters[0].riding(), Some((0, DRIVER)));
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0].mz = 127;
    let mut off = false;
    for _ in 0..TICK_HZ * 2 {
        w.step(&inputs, &NO_LAG);
        let f = w.fighters[1];
        assert!(f.alive && f.health == HEALTH_MAX, "hurt on the roof: {f:?}");
        let flat = (f.body.pos - at).flat().len();
        assert!(flat < 0.05, "shoved {flat} m: {:?}", f.body);
        off |= f.body.grounded && f.body.pos.y < at.y - 1.0;
    }
    assert!(w.vehicles[0].vel.flat().len() > SPLAT_SPEED, "never got going");
    assert!(off, "never came down: {:?}", w.fighters[1].body);
}

/// A truck nobody drives, rolling fast, runs someone down: their death,
/// nobody's kill, and no suicide off their score.
#[test]
fn a_truck_nobody_drives_runs_down_but_scores_for_nobody() {
    let mut w = world(false);
    let v = w.vehicles[0];
    let (f, _) = v.axes();
    w.fighters[0].body.pos = V3::new(50.0, 0.0, 50.0);
    let at = v.pos + f * (VEH_HALF_L + 2.0);
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    w.vehicles[0].vel = f * 18.0;
    let idle = [Input::default(); MAX_FIGHTERS];
    for _ in 0..TICK_HZ / 2 {
        w.step(&idle, &NO_LAG);
    }
    let (a, b) = (w.fighters[0], w.fighters[1]);
    assert!(!b.alive && b.deaths == 1, "never run down: {b:?}");
    assert_eq!((a.kills, b.kills), (0, 0), "somebody's kill");
    assert_eq!(w.score[1], 0, "a suicide off their score");
}

//! The one-seaters as their pilots fly them: the skimmer (a hover bike)
//! and the flier, each turning toward where its pilot looks, as Halo 2's
//! and 3's light craft do. Their pilots' guns, the flier's bomb and roll,
//! the boost, the ceiling, and a pilot's prediction of its own craft.

use sim::consts::*;
use sim::events::{Cause, Kind as Ev};
use sim::fmath;
use sim::geom::{rad_to_yaw, V3};
use sim::input::*;
use sim::vehicle::{DRIVER, FLIER, SKIMMER};
use sim::world::{Phase, Rules, World};

const NO_LAG: [u32; MAX_FIGHTERS] = [0; MAX_FIGHTERS];

/// Two fighters on Dry Fork, countdown over, each a side: 0 red, 1 blue,
/// blue far off.
fn world() -> World {
    let rules = Rules {
        teams: true,
        score_limit: 100,
        time_limit_ms: 0,
        ..Rules::doubles()
    };
    // Dry Fork carries only trucks; set the craft down where they stood.
    let mut map = sim::maps::canyon::DEF.build();
    for (x, z, yaw, respawn_ms, kind) in [
        (-92.0, -16.0, 0, 30_000, sim::vehicle::SKIMMER),
        (92.0, -16.0, 32768, 30_000, sim::vehicle::SKIMMER),
        (-112.0, 14.0, 0, 60_000, sim::vehicle::FLIER),
        (112.0, 14.0, 32768, 60_000, sim::vehicle::FLIER),
    ] {
        map.vehicles.push(sim::map::VehicleSpawn {
            pos: V3::new(x, 0.0, z),
            yaw,
            respawn_ms,
            turret: sim::vehicle::NO_TURRET,
            kind,
        });
    }
    let mut w = World::new(map, rules, 1, (5, 6));
    w.seat(0, 0);
    w.seat(1, 1);
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &NO_LAG);
    }
    w.fighters[1].body.pos = V3::new(60.0, w.map.ground(60.0, 0.0), 0.0);
    w
}

fn press(w: &mut World, s: usize, inp: Input) {
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[s] = inp;
    w.step(&inputs, &NO_LAG);
}

/// Red's craft of `kind`.
fn red(w: &World, kind: u8) -> usize {
    (0..MAX_VEHICLES)
        .find(|&i| w.vehicles[i].active && w.vehicles[i].kind == kind && w.vehicles[i].pos.x < 0.0)
        .expect("red has one")
}

/// Seat 0 into red's craft of `kind`, from beside it.
fn board(w: &mut World, kind: u8) -> usize {
    let i = red(w, kind);
    let v = w.vehicles[i];
    let (_, r) = v.axes();
    let at = v.pos + r * (v.spec().half_w + 0.5);
    w.fighters[0].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    w.fighters[0].body.vel = V3::ZERO;
    let tap = Input {
        buttons: USE,
        yaw: v.yaw,
        ..Input::default()
    };
    press(w, 0, tap);
    assert_eq!(w.fighters[0].riding(), Some((i, DRIVER)), "at its controls");
    i
}

fn turns(yaw: u16, by: f32) -> u16 {
    yaw.wrapping_add((by * 65536.0) as i32 as u16)
}

/// The yaw and pitch from `from` to `to`.
fn aim(from: V3, to: V3) -> (u16, i16) {
    let d = to - from;
    let pitch = fmath::atan2(d.y, d.flat().len()) * (65536.0 / core::f32::consts::TAU);
    (rad_to_yaw(fmath::atan2(d.z, d.x)), pitch as i16)
}

#[test]
fn a_skimmer_turns_to_its_pilots_aim_strafes_and_boosts_with_its_guns_cold() {
    let mut w = world();
    let i = board(&mut w, SKIMMER);
    let start = w.vehicles[i].yaw;
    // Boosting down the open middle: past its top speed, the tank going
    // down, and the trigger held fires nothing.
    let shots = |w: &World| {
        w.events
            .as_slice()
            .iter()
            .filter(|e| e.kind == Ev::Shot && e.a == 0)
            .count()
    };
    let mut fired = 0;
    for _ in 0..TICK_HZ * 3 / 2 {
        press(
            &mut w,
            0,
            Input {
                yaw: start,
                mz: 127,
                buttons: CROUCH | FIRE,
                ..Input::default()
            },
        );
        fired += shots(&w);
    }
    let v = w.vehicles[i];
    assert!(v.boosting && v.boost < ticks(BOOST_MS) as u16, "boosting: {v:?}");
    let speed = v.vel.flat().len();
    assert!(speed > SKIM_TOP + 3.0, "faster than its top: {speed}");
    assert_eq!(fired, 0, "its guns cold while it boosts");
    // Let go of the boost: the guns go, and it slides to a stop.
    for _ in 0..TICK_HZ * 3 {
        press(
            &mut w,
            0,
            Input {
                yaw: start,
                buttons: FIRE,
                ..Input::default()
            },
        );
        fired += shots(&w);
    }
    assert!(fired > 3, "{fired} bolts");
    assert!(w.vehicles[i].vel.flat().len() < 1.0, "stopped: {:?}", w.vehicles[i].vel);
    // Boost held standing still is no boost: the guns fire.
    let was = fired;
    for _ in 0..TICK_HZ {
        press(
            &mut w,
            0,
            Input {
                yaw: start,
                buttons: CROUCH | FIRE,
                ..Input::default()
            },
        );
        fired += shots(&w);
    }
    assert!(
        !w.vehicles[i].boosting && fired > was + 3,
        "{} bolts held still",
        fired - was
    );
    // A quarter turn to the left, full ahead: it comes round to the aim in
    // under a second (3.2 rad/s), and speeds up along it.
    let to = turns(start, -0.25);
    for _ in 0..TICK_HZ {
        press(
            &mut w,
            0,
            Input {
                yaw: to,
                mz: 127,
                ..Input::default()
            },
        );
    }
    let v = w.vehicles[i];
    assert_eq!(v.yaw, to, "turned to the aim");
    assert!(v.vel.dot(v.axes().0) > 10.0, "going its way: {:?}", v.vel);
    // Strafing right with the aim held: across its heading, its heading
    // kept.
    for _ in 0..TICK_HZ {
        press(
            &mut w,
            0,
            Input {
                yaw: to,
                mx: 127,
                ..Input::default()
            },
        );
    }
    let v = w.vehicles[i];
    assert_eq!(v.yaw, to);
    let across = v.vel.dot(v.axes().1);
    assert!(fmath::fabs(across - SKIM_STRAFE) < 0.5, "strafing at {across}");
}

#[test]
fn a_skimmers_bolts_kill_a_fighter_in_about_two_seconds() {
    let mut w = world();
    let i = board(&mut w, SKIMMER);
    let v = w.vehicles[i];
    let at = v.pos + v.axes().0 * 18.0;
    w.fighters[1].body.pos = V3::new(at.x, w.map.ground(at.x, at.z), at.z);
    let mut dead = None;
    for t in 0..TICK_HZ * 4 {
        let eye = w.fighters[0].eye();
        let chest = w.fighters[1].body.pos + V3::new(0.0, CHEST_Y, 0.0);
        let (yaw, pitch) = aim(eye, chest);
        press(
            &mut w,
            0,
            Input {
                yaw,
                pitch,
                buttons: FIRE,
                ..Input::default()
            },
        );
        if let Some(e) = w.events.as_slice().iter().find(|e| e.kind == Ev::Kill) {
            dead = Some((t, *e));
            break;
        }
    }
    let (t, e) = dead.expect("killed");
    assert_eq!((e.a, e.b, e.c), (0, 1, Cause::Bolts as u16));
    let s = t as f32 / TICK_HZ as f32;
    assert!((1.5..3.0).contains(&s), "in {s} s");
}

#[test]
fn a_flier_lifts_off_hangs_where_it_is_let_go_and_keeps_under_its_ceiling() {
    let mut w = world();
    let i = board(&mut w, FLIER);
    let ground = w.vehicles[i].pos.y;
    let yaw = w.vehicles[i].yaw;
    // Nose up a sixth of a right angle and full ahead: off the ground.
    for _ in 0..TICK_HZ * 2 {
        press(
            &mut w,
            0,
            Input {
                yaw,
                pitch: 2_700,
                mz: 127,
                ..Input::default()
            },
        );
    }
    let v = w.vehicles[i];
    assert!(!v.grounded && v.pos.y > ground + 4.0, "flying: {v:?}");
    assert!(v.pitch > 2_000, "its nose up: {}", v.pitch);
    // Let go, the nose level: it slows to a hover and holds its height.
    for _ in 0..TICK_HZ * 2 {
        press(
            &mut w,
            0,
            Input {
                yaw,
                ..Input::default()
            },
        );
    }
    let y = w.vehicles[i].pos.y;
    for _ in 0..TICK_HZ {
        press(
            &mut w,
            0,
            Input {
                yaw,
                ..Input::default()
            },
        );
    }
    let v = w.vehicles[i];
    assert!(v.vel.len() < 0.5, "hanging: {:?}", v.vel);
    assert!(fmath::fabs(v.pos.y - y) < 0.3, "held its height: {y} to {}", v.pos.y);
    // Straight up as steep as it goes, a long while: never past the
    // ceiling over its spawn.
    let ceiling = w.map.vehicles[v.home as usize].pos.y + FLY_CEIL;
    let mut top = 0.0f32;
    for _ in 0..TICK_HZ * 12 {
        press(
            &mut w,
            0,
            Input {
                yaw,
                pitch: PITCH_MAX,
                mz: 127,
                ..Input::default()
            },
        );
        top = top.max(w.vehicles[i].pos.y);
    }
    assert!(
        top > ceiling - 1.0 && top < ceiling + 1.0,
        "topped out at {top}, its ceiling {ceiling}"
    );
}

/// Seat 0 in red's flier, `up` metres over its pad and still.
fn aloft(up: f32) -> (World, usize) {
    let mut w = world();
    let i = board(&mut w, FLIER);
    let yaw = w.vehicles[i].yaw;
    while w.vehicles[i].pos.y < w.map.vehicles[w.vehicles[i].home as usize].pos.y + up {
        press(
            &mut w,
            0,
            Input {
                yaw,
                pitch: PITCH_MAX,
                mz: 127,
                ..Input::default()
            },
        );
    }
    for _ in 0..TICK_HZ * 2 {
        press(
            &mut w,
            0,
            Input {
                yaw,
                ..Input::default()
            },
        );
    }
    (w, i)
}

#[test]
fn a_flier_rolls_aside_on_jump_and_waits_before_the_next() {
    let (mut w, i) = aloft(8.0);
    let yaw = w.vehicles[i].yaw;
    let roll = Input {
        yaw,
        mx: 127,
        buttons: JUMP,
        ..Input::default()
    };
    press(&mut w, 0, roll);
    let v = w.vehicles[i];
    assert!(v.trick > 0, "rolling right: {}", v.trick);
    let across = v.vel.dot(v.axes().1);
    assert!(across > FLY_ROLL_KICK - 1.0, "thrown aside at {across}");
    // Let go and press again at once: no second roll yet.
    press(
        &mut w,
        0,
        Input {
            yaw,
            mx: 127,
            ..Input::default()
        },
    );
    let left = Input { mx: -127, ..roll };
    press(&mut w, 0, left);
    assert!(w.vehicles[i].trick > 0, "still the first roll");
    // After its wait, another.
    for _ in 0..ticks(FLY_ROLL_COOL_MS) {
        press(
            &mut w,
            0,
            Input {
                yaw,
                ..Input::default()
            },
        );
    }
    press(&mut w, 0, left);
    assert!(w.vehicles[i].trick < 0, "rolling left: {}", w.vehicles[i].trick);
}

#[test]
fn a_fliers_bomb_goes_once_a_while_and_its_pilot_is_out_of_a_rifles_reach() {
    let (mut w, i) = aloft(6.0);
    let yaw = w.vehicles[i].yaw;
    let bomb = Input {
        yaw,
        pitch: -4_000,
        buttons: FIRE_LEFT,
        ..Input::default()
    };
    let bombs = |w: &World| {
        w.events
            .as_slice()
            .iter()
            .filter(|e| e.kind == Ev::Shot && e.a == 0 && e.c == sim::arms::CANNON as u16)
            .count()
    };
    press(&mut w, 0, bomb);
    assert_eq!(bombs(&w), 1, "away");
    assert!(w.proj.iter().any(|p| p.active && p.owner == 0));
    press(&mut w, 0, Input { buttons: 0, ..bomb });
    press(&mut w, 0, bomb);
    assert_eq!(bombs(&w), 0, "not again so soon");
    // Blue, beside it at its height, puts a rifle round at the pilot's
    // head: the hull takes it.
    let v = w.vehicles[i];
    let side = v.pos + v.axes().1 * 12.0;
    w.fighters[1].body.pos = V3::new(side.x, v.pos.y - EYE + 1.27, side.z);
    w.fighters[1].body.vel = V3::ZERO;
    let head = w.fighters[0].body.pos + V3::new(0.0, HEAD_Y_CROUCH, 0.0);
    let (yaw, pitch) = aim(w.fighters[1].eye(), head);
    let before = (w.fighters[0].shield, w.fighters[0].health, w.vehicles[i].health);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0] = Input {
        yaw: v.yaw,
        ..Input::default()
    };
    inputs[1] = Input {
        yaw,
        pitch,
        buttons: FIRE,
        ..Input::default()
    };
    w.step(&inputs, &NO_LAG);
    assert!(
        w.events.as_slice().iter().any(|e| e.kind == Ev::VehicleHit && e.a == 1),
        "the hull is hit"
    );
    assert_eq!(
        (w.fighters[0].shield, w.fighters[0].health),
        (before.0, before.1),
        "not its pilot"
    );
    assert!(w.vehicles[i].health < before.2);
}

#[test]
fn out_of_a_flier_in_the_air_its_pilot_falls_and_so_does_it() {
    let (mut w, i) = aloft(10.0);
    let y = w.vehicles[i].pos.y;
    press(
        &mut w,
        0,
        Input {
            buttons: USE,
            ..Input::default()
        },
    );
    let f = w.fighters[0];
    assert!(f.riding().is_none());
    assert!(
        !f.body.grounded && fmath::fabs(f.body.pos.y - y) < 0.01,
        "out beside it: {:?}",
        f.body
    );
    for _ in 0..TICK_HZ * 3 {
        press(&mut w, 0, Input::default());
    }
    let v = w.vehicles[i];
    let floor = w.map.vehicles[v.home as usize].pos.y;
    assert!(
        v.grounded && fmath::fabs(v.pos.y - floor) < 1.0,
        "down on the ground: {v:?}"
    );
    assert!(w.fighters[0].body.grounded, "and so is its pilot");
}

/// What the pilot's client predicts of itself and its craft, tick for
/// tick, is what the server runs: shaky aim, the stick all over, boosts and
/// rolls and bombs.
fn predicts(kind: u8) {
    let mut w = world();
    let i = board(&mut w, kind);
    let map = w.map.clone();
    let mut mine = w.fighters[0];
    let mut craft = w.vehicles[i];
    let mut rng = sim::rng::Pcg32::new(77, 7);
    let mut yaw = craft.yaw;
    let mut pitch: i16 = 1_500;
    let mut travelled = 0.0;
    for t in 0..TICK_HZ * 15 {
        let r = rng.next_u32();
        yaw = yaw.wrapping_add(((r & 0x3ff) as i32 - 0x1ff) as u16);
        pitch = (pitch as i32 + ((r >> 10) & 0xff) as i32 - 0x7f).clamp(-6_000, 9_000) as i16;
        let inp = Input {
            buttons: ((r >> 18) as u16) & (CROUCH | JUMP | FIRE | FIRE_LEFT),
            yaw,
            pitch,
            mx: ((r >> 24) as u8 as i8).max(-127),
            mz: if (r >> 20).is_multiple_of(5) { -127 } else { 127 },
        }
        .sanitized();
        let before = w.vehicles[i].pos;
        press(&mut w, 0, inp);
        mine.step_local(&inp, &map);
        craft.step_local(&inp, &map);
        mine.body = craft.seat_body(DRIVER);
        if w.vehicles[i].health < craft.health || !w.vehicles[i].active {
            break;
        }
        craft.health = w.vehicles[i].health;
        assert_eq!(craft, w.vehicles[i], "tick {t}");
        assert_eq!(mine.body, w.fighters[0].body, "tick {t}");
        travelled += (w.vehicles[i].pos - before).len();
    }
    assert!(travelled > 100.0, "it went places: {travelled} m");
}

#[test]
fn a_pilots_prediction_of_its_skimmer_matches_the_server_bit_for_bit() {
    predicts(SKIMMER);
}

#[test]
fn a_pilots_prediction_of_its_flier_matches_the_server_bit_for_bit() {
    predicts(FLIER);
}

/// The flier's steel is the craft as it is drawn: a round from over it
/// stops on its fin, on a wing's tip where it hangs low, and on a gun's
/// prong, and falls through the air over the tip, where a level wing was.
#[test]
fn a_fliers_fin_its_wings_tips_and_its_guns_stop_a_shot() {
    let v = sim::vehicle::Vehicle::new_kind(V3::new(10.0, 3.0, -4.0), 9_000, 0, 0, FLIER);
    let (f, r) = v.axes();
    // Straight down from 10 m over the hull's ground, at (ahead, right) of
    // its middle: how high over that ground the round is stopped.
    let stopped = |ahead: f32, right: f32| {
        let from = v.pos + f * ahead + r * right + V3::new(0.0, 10.0, 0.0);
        v.ray(from, V3::new(0.0, -1.0, 0.0), 10.0).map(|t| 10.0 - t)
    };
    let near = |got: Option<f32>, want: f32| got.is_some_and(|y| fmath::fabs(y - want) < 1e-3);
    assert!(
        near(stopped(-1.5, 0.0), FLY_FIN_TOP),
        "its fin's tip: {:?}",
        stopped(-1.5, 0.0)
    );
    assert!(
        near(stopped(-1.0, 0.0), FLY_FIN_Y),
        "its fin's foot: {:?}",
        stopped(-1.0, 0.0)
    );
    assert!(near(stopped(0.4, 0.0), FLY_H), "its fuselage: {:?}", stopped(0.4, 0.0));
    for side in [-1.0, 1.0] {
        assert!(
            near(stopped(-0.7, 2.3 * side), FLY_TIP_Y1),
            "a tip: {:?}",
            stopped(-0.7, 2.3 * side)
        );
        assert!(
            near(stopped(0.9, 1.9 * side), FLY_GUN_Y1),
            "a prong: {:?}",
            stopped(0.9, 1.9 * side)
        );
        assert!(
            near(stopped(0.0, 1.4 * side), FLY_WING_Y + FLY_WING_T),
            "a blade: {:?}",
            stopped(0.0, 1.4 * side)
        );
        assert_eq!(stopped(0.3, 2.3 * side), None, "the air ahead of a tip");
        assert_eq!(stopped(-1.2, 1.4 * side), None, "the air behind a blade");
    }
    assert_eq!(stopped(-2.2, 0.5), None, "the air behind its tail");
}

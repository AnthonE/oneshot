//! Capture the Flag as a player meets it, Halo 3's way: taking the enemy's
//! flag with USE, not firing while carrying it, walking slower and killing
//! with a swing of it; letting go with SWAP, which tosses it ahead to be
//! taken again once it lands (juggling it); riding a passenger seat with
//! it, never driving, and scoring from the seat; scoring it at home only
//! while your own flag is there; dropping it on death; returning it by
//! standing by it (held by an enemy in reach) or on its clock; sudden death;
//! and bots that play it on every map with flags.

use sim::bot::{bot_match_on, Bot};
use sim::consts::*;
use sim::events::Kind as Ev;
use sim::geom::V3;
use sim::input::*;
use sim::map::{Map, VehicleSpawn};
use sim::vehicle::PASSENGER;
use sim::world::{Phase, Rules, World, FLAG_DOWN, FLAG_HELD, FLAG_HOME, NO_WINNER};

const NO_LAG: [u32; MAX_FIGHTERS] = [0; MAX_FIGHTERS];
const RED: V3 = V3::new(-30.0, 0.0, 0.0);
const BLUE: V3 = V3::new(30.0, 0.0, 0.0);

/// The open test map with flags at ±30 m, two a side, countdown over.
fn world() -> World {
    world_on(Map::open(), Rules::ctf())
}

fn world_on(mut map: Map, rules: Rules) -> World {
    map.flags = Some([RED, BLUE]);
    let mut w = World::new(map, rules, 1, (5, 6));
    for s in 0..4 {
        w.seat(s, (s & 1) as u8);
    }
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &NO_LAG);
    }
    // Everyone out of the way of both flags.
    for s in 0..4 {
        w.fighters[s].body.pos = V3::new(0.0, 0.0, -40.0 + 5.0 * s as f32);
    }
    w
}

fn step(w: &mut World, inputs: &[Input; MAX_FIGHTERS]) -> Vec<(u8, u8, u8)> {
    w.step(inputs, &NO_LAG);
    w.events
        .as_slice()
        .iter()
        .filter(|e| e.kind == Ev::Flag)
        .map(|e| (e.part, e.a, e.b))
        .collect()
}

fn idle(w: &mut World) -> Vec<(u8, u8, u8)> {
    step(w, &[Input::default(); MAX_FIGHTERS])
}

/// Seat `s` taps `b` (and lets go the tick after): what happened.
fn tap(w: &mut World, s: usize, b: u16) -> Vec<(u8, u8, u8)> {
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[s].buttons = b;
    let mut saw = step(w, &inputs);
    saw.extend(idle(w));
    saw
}

/// Seat `s` stands on `at` and takes the flag there.
fn take(w: &mut World, s: usize, at: V3) -> Vec<(u8, u8, u8)> {
    w.fighters[s].body.pos = at;
    tap(w, s, USE)
}

#[test]
fn an_enemy_takes_the_flag_carries_it_and_scores_at_home() {
    let mut w = world();
    // Standing on it isn't taking it: USE is (Halo 3's).
    w.fighters[1].body.pos = RED;
    assert_eq!(idle(&mut w), vec![]);
    assert_eq!(take(&mut w, 1, RED), vec![(0, 1, 0)], "blue takes red's flag");
    assert_eq!(w.flags[0].state, FLAG_HELD);
    assert!(w.fighters[1].flag);
    // A defender can't take their own.
    assert_eq!(take(&mut w, 3, BLUE), vec![]);

    // Both hands on it: the trigger does nothing, nor does a grenade.
    let mag = w.fighters[1].gun().mag;
    let shots = w.fighters[1].shots;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[1] = Input {
        buttons: FIRE | GRENADE,
        ..Input::default()
    };
    for _ in 0..10 {
        step(&mut w, &inputs);
        inputs[1].buttons ^= FIRE | GRENADE;
    }
    assert_eq!((w.fighters[1].gun().mag, w.fighters[1].shots), (mag, shots));
    assert!(w.proj.live().next().is_none());

    // The flag rides with its carrier.
    w.fighters[1].body.pos = V3::new(5.0, 0.0, 5.0);
    idle(&mut w);
    assert!((w.flags[0].pos - V3::new(5.0, 0.0, 5.0)).len() < 0.01);

    // At blue's base, with blue's flag home: a capture, and red's flag goes
    // home.
    w.fighters[1].body.pos = BLUE;
    assert_eq!(idle(&mut w), vec![(3, 1, 0)]);
    assert_eq!(w.score[1], 1);
    assert_eq!(w.flags[0].state, FLAG_HOME);
    assert_eq!(w.flags[0].pos, RED);
    assert!(!w.fighters[1].flag);
}

#[test]
fn no_capture_while_your_own_flag_is_away() {
    let mut w = world();
    take(&mut w, 1, RED);
    // Red takes blue's flag too.
    take(&mut w, 0, BLUE);
    assert_eq!(w.flags[1].state, FLAG_HELD);
    // Blue's carrier home: blue's flag is in red's hands, so no score.
    w.fighters[1].body.pos = BLUE;
    w.fighters[0].body.pos = V3::new(0.0, 0.0, 30.0);
    idle(&mut w);
    assert_eq!(w.score, [0; MAX_FIGHTERS]);
    assert!(w.fighters[1].flag);
}

/// How far seat 1 runs ahead in a second from `from`.
fn run(w: &mut World, from: V3) -> f32 {
    w.fighters[1].body.pos = from;
    w.fighters[1].body.vel = V3::ZERO;
    w.fighters[1].yaw = 0;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[1].mz = 127;
    for _ in 0..TICK_HZ {
        step(w, &inputs);
    }
    w.fighters[1].body.pos.x - from.x
}

#[test]
fn a_carrier_is_slower_and_juggling_it_is_how_it_walks() {
    let mut w = world();
    let free = run(&mut w, V3::new(0.0, 0.0, 20.0));
    take(&mut w, 1, RED);
    let held = run(&mut w, V3::new(0.0, 0.0, 20.0));
    assert!(
        sim::fmath::fabs(held / free - FLAG_PACE) < 0.05,
        "carried at {held:.2} m/s, free at {free:.2}"
    );

    // SWAP lets go of it: tossed ahead, along the aim.
    w.fighters[1].body.vel = V3::ZERO;
    let at = w.fighters[1].body.pos;
    assert_eq!(tap(&mut w, 1, SWAP)[0], (1, 1, 0));
    assert!(!w.fighters[1].flag);
    assert_eq!(w.flags[0].state, FLAG_DOWN);
    assert!(w.flags[0].flying, "in the air");
    // Nobody takes it in the air, not even standing in its way.
    w.fighters[1].body.pos = w.flags[0].pos;
    assert_eq!(tap(&mut w, 1, USE), vec![]);
    let mut n = 0;
    while w.flags[0].flying {
        idle(&mut w);
        n += 1;
        assert!(n < TICK_HZ * 2, "it lands");
    }
    let ahead = w.flags[0].pos.x - at.x;
    assert!(ahead > 1.5 && ahead < 5.0, "tossed ahead: {ahead}");
    assert_eq!(w.flags[0].pos.y, 0.0, "on the floor");
    // Landed, it is the dropper's again for a USE: a juggle.
    let landed = w.flags[0].pos;
    assert_eq!(take(&mut w, 1, landed), vec![(0, 1, 0)]);
    // And the gun wasn't swapped by it.
    assert_eq!(w.fighters[1].cur, 0);
}

#[test]
fn a_swing_of_the_flag_kills() {
    let mut w = world();
    take(&mut w, 1, RED);
    // Red stands in front of the carrier, face to face.
    w.fighters[1].body.pos = V3::new(10.0, 0.0, 10.0);
    w.fighters[1].yaw = 0;
    w.fighters[0].body.pos = V3::new(11.2, 0.0, 10.0);
    w.fighters[0].yaw = 32768;
    tap(&mut w, 1, MELEE);
    assert!(!w.fighters[0].alive, "one swing, full shields and all");
}

#[test]
fn a_carrier_rides_never_drives_and_scores_from_the_seat() {
    let mut map = Map::open();
    map.vehicles.push(VehicleSpawn {
        pos: V3::new(0.0, 0.0, 10.0),
        yaw: 0,
        respawn_ms: 30_000,
        turret: 0,
        kind: 0,
    });
    let mut w = world_on(map, Rules::ctf());
    take(&mut w, 1, RED);
    // At an empty truck, the carrier takes the passenger's seat.
    w.fighters[1].body.pos = V3::new(-2.0, 0.0, 10.0);
    tap(&mut w, 1, USE);
    assert_eq!(w.fighters[1].riding(), Some((0, PASSENGER)));
    assert!(w.fighters[1].flag, "still carrying it");
    // A teammate drives.
    w.fighters[3].body.pos = V3::new(-2.0, 0.0, 10.0);
    tap(&mut w, 3, USE);
    assert_eq!(w.fighters[3].riding(), Some((0, sim::vehicle::DRIVER)));
    // The flag goes where the truck goes.
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[3].mz = 127;
    for _ in 0..TICK_HZ {
        step(&mut w, &inputs);
    }
    assert!((w.flags[0].pos - w.fighters[1].body.pos).len() < 0.01);
    assert!(w.vehicles[0].pos.x > 3.0, "it drove");
    // Brought onto blue's base, the passenger scores from the seat.
    w.vehicles[0].pos = BLUE;
    w.vehicles[0].vel = V3::ZERO;
    let mut saw = Vec::new();
    for _ in 0..3 {
        saw.extend(idle(&mut w));
    }
    assert!(saw.contains(&(3, 1, 0)), "captured from the seat: {saw:?}");
    assert_eq!(w.score[1], 1);

    // Carrying, a lone truck's wheel is no seat for them.
    let mut w = world_on(
        {
            let mut m = Map::open();
            m.vehicles.push(VehicleSpawn {
                pos: V3::new(0.0, 0.0, 10.0),
                yaw: 0,
                respawn_ms: 30_000,
                turret: 0,
                kind: 0,
            });
            m
        },
        Rules::ctf(),
    );
    take(&mut w, 1, RED);
    w.fighters[0].body.pos = V3::new(-2.0, 0.0, 10.0);
    tap(&mut w, 0, USE);
    w.fighters[2].body.pos = V3::new(-2.0, 0.0, 10.0);
    tap(&mut w, 2, USE);
    // Both seats red's: blue's carrier stays on foot.
    w.fighters[1].body.pos = V3::new(-2.0, 0.0, 10.0);
    tap(&mut w, 1, USE);
    assert_eq!(w.fighters[1].riding(), None);
    assert!(w.fighters[1].flag);
}

#[test]
fn a_dead_carrier_drops_it_defenders_return_it_and_the_clock_does_too() {
    let mut w = world();
    take(&mut w, 1, RED);
    w.fighters[1].body.pos = V3::new(10.0, 0.0, 10.0);
    idle(&mut w);
    // Red kills the carrier with a melee from behind.
    w.fighters[0].body.pos = V3::new(8.5, 0.0, 10.0);
    w.fighters[0].yaw = 0;
    w.fighters[1].yaw = 0;
    let mut saw = Vec::new();
    for _ in 0..TICK_HZ {
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        if w.fighters[0].melee_cool == 0 {
            inputs[0].buttons = MELEE;
        }
        saw.extend(step(&mut w, &inputs));
        if !w.fighters[1].alive {
            break;
        }
    }
    assert!(!w.fighters[1].alive, "the carrier died");
    assert!(saw.contains(&(1, 1, 0)), "and dropped red's flag: {saw:?}");
    assert_eq!(w.flags[0].state, FLAG_DOWN);
    assert!(!w.flags[0].flying, "where they stood");

    // Red stands by it: home in FLAG_RETURN_MS, not at once.
    let at = w.flags[0].pos;
    w.fighters[0].body.pos = at;
    let full = ticks(FLAG_RETURN_MS);
    for _ in 0..full / 2 {
        assert_eq!(idle(&mut w), vec![]);
    }
    // An enemy comes into reach: the return holds.
    w.fighters[3].body.pos = at + V3::new(0.5, 0.0, 0.0);
    let held = w.flags[0].ret;
    for _ in 0..TICK_HZ {
        assert_eq!(idle(&mut w), vec![]);
    }
    assert_eq!(w.flags[0].ret, held, "contested");
    // Gone again, and a second defender doubles the pace.
    w.fighters[3].body.pos = V3::new(0.0, 0.0, 30.0);
    w.fighters[2].body.pos = at + V3::new(0.0, 0.0, 0.5);
    let mut back = Vec::new();
    let mut n = 0u32;
    while back.is_empty() {
        back = idle(&mut w);
        n += 1;
    }
    assert_eq!(back, vec![(2, 0, 0)], "returned by red");
    assert!(n.abs_diff((full - held).div_ceil(2)) <= 1, "two at it: {n} ticks");
    assert_eq!(w.flags[0].state, FLAG_HOME);

    // Dropped again and left alone: home on its clock.
    take(&mut w, 3, RED);
    w.fighters[3].body.pos = V3::new(-10.0, 0.0, 20.0);
    w.fighters[0].body.pos = V3::new(-10.0, 0.0, -20.0);
    w.fighters[2].body.pos = V3::new(-10.0, 0.0, -25.0);
    assert_eq!(tap(&mut w, 3, SWAP), vec![(1, 3, 0)], "SWAP lets go of it");
    w.fighters[3].body.pos = V3::new(-10.0, 0.0, 35.0);
    let mut back = Vec::new();
    for _ in 0..ticks(FLAG_RESET_MS) + 2 {
        back.extend(idle(&mut w));
    }
    assert_eq!(back, vec![(2, NO_WINNER, 0)]);
    assert_eq!(w.flags[0].pos, RED);
}

#[test]
fn time_runs_out_with_a_flag_away_and_it_is_sudden_death() {
    let rules = Rules {
        time_limit_ms: 2_000,
        ..Rules::ctf()
    };
    let mut w = world_on(Map::open(), rules);
    take(&mut w, 1, RED);
    for _ in 0..ticks(2_000) {
        idle(&mut w);
    }
    assert_eq!(w.phase, Phase::Playing, "on while red's flag is away");
    w.fighters[1].body.pos = BLUE;
    assert_eq!(idle(&mut w), vec![(3, 1, 0)]);
    idle(&mut w);
    assert_eq!(w.phase, Phase::Over, "settled");
    assert_eq!(w.winner, 1);

    // Held for ever, it ends all the same.
    let mut w = world_on(Map::open(), rules);
    take(&mut w, 1, RED);
    for _ in 0..ticks(2_000) + ticks(SUDDEN_DEATH_MS) + 2 {
        idle(&mut w);
    }
    assert_eq!(w.phase, Phase::Over);
}

#[test]
fn kills_dont_score_and_three_captures_win() {
    let mut w = world();
    for n in 1..=3 {
        take(&mut w, 1, RED);
        w.fighters[1].body.pos = BLUE;
        idle(&mut w);
        assert_eq!(w.score[1], n);
        w.fighters[1].body.pos = V3::new(0.0, 0.0, -35.0);
    }
    assert_eq!(w.phase, Phase::Over);
    assert_eq!(w.winner, 1);
}

#[test]
fn bots_go_for_flags_on_every_map_with_flags() {
    // Bots take flags on the arenas (CTF's playlist) within ten minutes;
    // carrying one home past defenders is rarer (a capture or two an hour)
    // and the capture itself is pinned above. On a Big Team map the flags
    // are too far apart for bots on foot, so there it only plays.
    for d in sim::maps::ALL.iter().filter(|d| d.flags.len() == 2) {
        let rules = Rules {
            score_limit: 1000,
            time_limit_ms: 0,
            ..Rules::ctf()
        };
        let mut w = World::new(d.build(), rules, 7, (1, 2));
        let n = if d.big() { MAX_FIGHTERS } else { rules.seats as usize };
        let mut bots = [Bot::new(7, 0, 0.5); MAX_FIGHTERS];
        for (s, b) in bots.iter_mut().enumerate().take(n) {
            w.seat(s, (s & 1) as u8);
            *b = Bot::new(7, s, 0.3 + 0.2 * (s % 4) as f32);
        }
        let mut taken = 0;
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        let minutes = if d.big() { 1 } else { 10 };
        for _ in 0..ticks(minutes * 60_000) {
            for s in 0..n {
                inputs[s] = bots[s].think(&w, s);
            }
            w.step(&inputs, &NO_LAG);
            taken += w
                .events
                .as_slice()
                .iter()
                .filter(|e| e.kind == Ev::Flag && e.part == 0)
                .count();
        }
        if !d.big() {
            assert!(taken > 0, "{}: bots never took a flag in {minutes} minutes", d.name);
        }
        // And the parity gate's run has a CTF match here.
        assert_ne!(bot_match_on(d.id, 3, 1, 600), 0);
    }
}

/// Big Team is Capture the Flag on Dry Fork (`Rules::big_ctf`): its flags
/// stand where a fighter can take one with USE and score with it at home.
#[test]
fn big_team_captures_on_dry_fork() {
    let mut w = World::new(sim::map::by_id(3).unwrap(), Rules::big_ctf(), 1, (5, 6));
    w.seat(0, 0);
    w.seat(1, 1);
    while w.phase == Phase::Countdown {
        idle(&mut w);
    }
    let [red, blue] = w.map.flags.expect("Dry Fork has flags");
    w.fighters[0].body.pos = V3::new(0.0, w.map.ground(0.0, 0.0), 0.0);
    assert_eq!(take(&mut w, 1, red), vec![(0, 1, 0)], "blue takes red's flag");
    w.fighters[1].body.pos = blue;
    assert_eq!(idle(&mut w), vec![(3, 1, 0)], "and scores with it at home");
    assert_eq!(w.score[1], 1);
}

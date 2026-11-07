//! Who is in a client's bubble (`room::bubble`): hulls near it, by how far
//! and how fast they close, held a while once in, brought in by what they
//! might crash into; never its own, never an unknown driver's, never past
//! the round trip a re-run can guess across.

use room::bubble::{Bubble, BUBBLE_M, CONTACT_M, DWELL_MS, HYST_M, LOOK_S, RTT_MAX_MS};
use sim::consts::{MAX_FIGHTERS, TICK_HZ};
use sim::geom::V3;
use sim::input::Input;
use sim::world::{Phase, Rules, World};

/// Dry Fork, everyone seated, the countdown over; seat 0 on foot at the
/// middle, standing; every hull parked far off.
fn world() -> World {
    let rules = Rules {
        score_limit: 1000,
        time_limit_ms: 0,
        ..Rules::big_team()
    };
    let mut w = World::new(sim::map::by_id(3).unwrap(), rules, 1, (2, 3));
    for s in 0..MAX_FIGHTERS {
        w.seat(s, (s & 1) as u8);
    }
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &[0; MAX_FIGHTERS]);
    }
    w.fighters[0].body.pos = V3::new(0.0, 0.0, 0.0);
    w.fighters[0].body.vel = V3::ZERO;
    for (i, v) in w.vehicles.iter_mut().enumerate() {
        v.pos = V3::new(-200.0 + 400.0 * (i & 1) as f32, 0.0, -150.0 + 300.0 * (i >> 1) as f32);
        v.vel = V3::ZERO;
    }
    w
}

/// Hull 0 at `x` m off along x, moving at `vel`.
fn put(w: &mut World, x: f32, vel: V3) {
    w.vehicles[0].pos = V3::new(x, 0.0, 0.0);
    w.vehicles[0].vel = vel;
}

fn update(b: &mut Bubble, w: &World, rtt_ms: f64) {
    b.update(w, 0, &|_| true, &|_| true, rtt_ms, 2);
}

/// Whether hull 0 comes in at `x` m moving at `vel`, from out.
fn joins(x: f32, vel: V3) -> bool {
    let mut w = world();
    put(&mut w, x, vel);
    let mut b = Bubble::default();
    update(&mut b, &w, 60.0);
    b.has(0)
}

#[test]
fn a_hull_comes_in_by_how_far_and_how_fast_it_closes() {
    // Parked: 16 m.
    assert!(joins(BUBBLE_M - 0.1, V3::ZERO));
    assert!(!joins(BUBBLE_M + 0.1, V3::ZERO));
    // At full speed, toward or away (closing speed is relative): 28.6 m.
    let fast = V3::new(-21.0, 0.0, 0.0);
    let r = BUBBLE_M + 21.0 * LOOK_S;
    assert!((r - 28.6).abs() < 1e-3);
    assert!(joins(r - 0.1, fast));
    assert!(!joins(r + 0.1, fast));
    // Head-on with us in another at full speed: 41.2 m.
    let mut w = world();
    w.vehicles[1].pos = V3::new(0.0, 0.0, 0.0);
    w.vehicles[1].vel = V3::new(21.0, 0.0, 0.0);
    w.vehicles[1].seats[0] = 1;
    w.fighters[0].ride = sim::fighter::Fighter::ride_of(1, sim::vehicle::DRIVER);
    let r = BUBBLE_M + 42.0 * LOOK_S;
    assert!((r - 41.2).abs() < 1e-3);
    for (x, want) in [(r - 0.1, true), (r + 0.1, false)] {
        put(&mut w, x, fast);
        let mut b = Bubble::default();
        update(&mut b, &w, 60.0);
        assert_eq!(b.has(0), want, "{x} m head-on");
        assert!(!b.has(1), "our own hull is never in our bubble");
    }
}

#[test]
fn a_hull_in_stays_a_while_and_until_well_past() {
    let mut w = world();
    let mut b = Bubble::default();
    put(&mut w, BUBBLE_M - 1.0, V3::ZERO);
    update(&mut b, &w, 60.0);
    assert!(b.has(0));
    // Just past where it came in: in still.
    put(&mut w, BUBBLE_M + HYST_M - 0.5, V3::ZERO);
    update(&mut b, &w, 60.0);
    assert!(b.has(0), "inside the hysteresis");
    // Well past, at once: held for the dwell, then out.
    put(&mut w, BUBBLE_M + HYST_M + 5.0, V3::ZERO);
    let dwell_snaps = DWELL_MS * TICK_HZ / 1000 / 2;
    let mut out_after = None;
    for k in 0..dwell_snaps + 4 {
        update(&mut b, &w, 60.0);
        if !b.has(0) {
            out_after = Some(k);
            break;
        }
    }
    let k = out_after.expect("never left");
    assert!(
        k + 2 >= dwell_snaps,
        "left after {k} snapshots, the dwell is {dwell_snaps}"
    );
}

#[test]
fn never_an_unknown_drivers_and_never_past_the_round_trip() {
    let mut w = world();
    put(&mut w, 5.0, V3::new(10.0, 0.0, 0.0));
    w.vehicles[0].seats[0] = 6;
    let mut b = Bubble::default();
    // A driver it may not know: its stick would tell where they are going.
    b.update(&w, 0, &|q| q != 5, &|_| true, 60.0, 2);
    assert!(!b.has(0), "an unknown driver's hull");
    b.update(&w, 0, &|_| true, &|_| true, 60.0, 2);
    assert!(b.has(0));
    // Not sent at all: never in.
    let mut b = Bubble::default();
    b.update(&w, 0, &|_| true, &|_| false, 60.0, 2);
    assert!(!b.has(0), "a hull fog hides");
    // Too far a round trip to guess across, moving; parked, still in.
    let mut b = Bubble::default();
    update(&mut b, &w, RTT_MAX_MS + 1.0);
    assert!(!b.has(0), "driven past the round trip");
    w.vehicles[0].seats[0] = 0;
    w.vehicles[0].vel = V3::ZERO;
    update(&mut b, &w, RTT_MAX_MS + 1.0);
    assert!(b.has(0), "parked, any round trip");
    // Dead: nothing.
    w.fighters[0].alive = false;
    update(&mut b, &w, 60.0);
    assert_eq!(b.hulls(), 0);
}

#[test]
fn a_hull_in_brings_in_what_it_might_crash_into() {
    let mut w = world();
    put(&mut w, 10.0, V3::new(5.0, 0.0, 0.0));
    // Hull 1 far from us but just ahead of hull 0, closing.
    w.vehicles[1].pos = V3::new(10.0 + CONTACT_M + 5.0 * LOOK_S - 0.5, 0.0, 0.0);
    w.vehicles[1].vel = V3::ZERO;
    assert!((w.vehicles[1].pos - w.fighters[0].body.pos).flat().len() > BUBBLE_M);
    let mut b = Bubble::default();
    update(&mut b, &w, 60.0);
    assert!(b.has(0) && b.has(1), "{:08b}", b.hulls());
    // Farther than it could reach: not.
    w.vehicles[1].pos.x += 2.0;
    let mut b = Bubble::default();
    update(&mut b, &w, 60.0);
    assert!(b.has(0) && !b.has(1), "{:08b}", b.hulls());
}

#[test]
fn an_arena_has_no_bubble() {
    let mut w = World::new(sim::map::Map::proving_grounds(), Rules::doubles(), 1, (2, 3));
    w.seat(0, 0);
    let mut b = Bubble::default();
    b.update(&w, 0, &|_| true, &|_| true, 60.0, 2);
    assert_eq!(b.hulls(), 0);
}

//! Who is in a client's bubble (`room::bubble`): hulls near it, by how far
//! and how fast they close, held a while once in, brought in by what they
//! might crash into; never its own, never an unknown driver's, never past
//! the round trip a re-run can guess across.

use room::bubble::{
    Bubble, BLEND_MAX_MS, BLEND_MIN_MS, BUBBLE_M, CONTACT_M, DWELL_MS, HYST_M, LEAVE_MS, LOOK_S, RTT_MAX_MS,
};
use sim::consts::{MAX_FIGHTERS, TICK_HZ};
use sim::consts::{SHARE_FULL, VEH_HALF_W};
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
    b.update(w, 0, &|_| true, &|_| true, &|_| true, rtt_ms, 2, true);
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
    // At full speed, toward or away (closing speed is relative): 41.2 m.
    let fast = V3::new(-21.0, 0.0, 0.0);
    let r = BUBBLE_M + 21.0 * LOOK_S;
    assert!((r - 41.2).abs() < 1e-3);
    assert!(joins(r - 0.1, fast));
    assert!(!joins(r + 0.1, fast));
    // Head-on with us in another at full speed: 66.4 m.
    let mut w = world();
    w.vehicles[1].pos = V3::new(0.0, 0.0, 0.0);
    w.vehicles[1].vel = V3::new(21.0, 0.0, 0.0);
    w.vehicles[1].seats[0] = 1;
    w.fighters[0].ride = sim::fighter::Fighter::ride_of(1, sim::vehicle::DRIVER);
    let r = BUBBLE_M + 42.0 * LOOK_S;
    assert!((r - 66.4).abs() < 1e-3);
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
    // Well past, at once: held for the dwell, then back onto the render
    // tick over a second, then out.
    put(&mut w, BUBBLE_M + HYST_M + 5.0, V3::ZERO);
    let snaps = |ms: u32| (ms * TICK_HZ / 1000 / 2) as usize;
    let mut shares = Vec::new();
    for _ in 0..snaps(DWELL_MS + LEAVE_MS as u32) + 8 {
        update(&mut b, &w, 60.0);
        if !b.has(0) {
            break;
        }
        shares.push(b.share(0));
    }
    assert!(!b.has(0), "never left");
    let k = shares.len();
    assert!(
        k + 2 >= snaps(DWELL_MS),
        "left after {k} snapshots, the dwell is {}",
        snaps(DWELL_MS)
    );
    assert!(k <= snaps(DWELL_MS + LEAVE_MS as u32) + 4, "{k} snapshots to leave");
    // Its share only ever climbs once it is leaving.
    let leaving = &shares[snaps(DWELL_MS).min(k)..];
    assert!(leaving.windows(2).all(|p| p[1] >= p[0]), "{shares:?}");
    // One on the present when it goes climbs to whole over the leave, and
    // is whole when it is out.
    let mut w = world();
    put(&mut w, 30.0, V3::ZERO);
    let mut b = Bubble::default();
    update(&mut b, &w, 60.0);
    put(&mut w, BUBBLE_M - 1.0, V3::ZERO);
    for _ in 0..snaps(DWELL_MS.max(BLEND_MAX_MS as u32)) + 4 {
        update(&mut b, &w, 60.0);
    }
    assert_eq!(b.share(0), 0, "settled on the present");
    put(&mut w, BUBBLE_M + HYST_M + 5.0, V3::ZERO);
    let mut shares = Vec::new();
    for _ in 0..snaps(LEAVE_MS as u32) + 8 {
        update(&mut b, &w, 60.0);
        if !b.has(0) {
            break;
        }
        shares.push(b.share(0));
    }
    assert!(!b.has(0), "never left: {shares:?}");
    assert_eq!(shares.last().copied(), Some(SHARE_FULL), "{shares:?}");
    assert!(shares.windows(2).all(|p| p[1] >= p[0]), "{shares:?}");
    let climb = shares.iter().filter(|&&x| x < SHARE_FULL).count();
    assert!(
        climb.abs_diff(snaps(LEAVE_MS as u32)) <= 2,
        "{climb} snapshots climbing: {shares:?}"
    );
}

/// Who is in tells nothing fog hides: a hull with a driver the viewer may
/// not know comes and goes exactly as one nobody drives (its stick then
/// goes as none: `room::build`), at any round trip, parked or rolling.
#[test]
fn a_hidden_driver_is_as_nobody_at_the_wheel() {
    for (rtt, vel) in [
        (60.0, V3::ZERO),
        (60.0, V3::new(10.0, 0.0, 0.0)),
        (RTT_MAX_MS + 50.0, V3::ZERO),
    ] {
        for x in [5.0, 20.0, 40.0] {
            let mut w = world();
            put(&mut w, x, vel);
            let mut hidden = Bubble::default();
            let mut empty = Bubble::default();
            for _ in 0..4 {
                w.vehicles[0].seats[0] = 6;
                hidden.update(&w, 0, &|q| q != 5, &|q| q != 5, &|_| true, rtt, 2, true);
                w.vehicles[0].seats[0] = 0;
                empty.update(&w, 0, &|q| q != 5, &|q| q != 5, &|_| true, rtt, 2, true);
                assert_eq!(
                    (hidden.has(0), hidden.share(0)),
                    (empty.has(0), empty.share(0)),
                    "{x} m at {vel:?}, rtt {rtt}"
                );
            }
        }
    }
}

#[test]
fn never_a_hidden_hull_and_never_past_the_round_trip() {
    let mut w = world();
    put(&mut w, 5.0, V3::new(10.0, 0.0, 0.0));
    w.vehicles[0].seats[0] = 6;
    let mut b = Bubble::default();
    b.update(&w, 0, &|_| true, &|_| true, &|_| true, 60.0, 2, true);
    assert!(b.has(0));
    // Not sent at all: never in.
    let mut b = Bubble::default();
    b.update(&w, 0, &|_| true, &|_| true, &|_| false, 60.0, 2, true);
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
    b.update(&w, 0, &|_| true, &|_| true, &|_| true, 60.0, 2, true);
    assert_eq!(b.hulls(), 0);
}

/// A hull coming in that was drawn in the past slides onto the present
/// from its whole share; one never drawn (unseen until now) is on the
/// present at once; one leaving climbs back to whole, then goes.
#[test]
fn a_hull_slides_onto_the_present_and_back() {
    let mut w = world();
    put(&mut w, 30.0, V3::ZERO);
    let mut b = Bubble::default();
    // Seen out of it first: drawn in the past.
    update(&mut b, &w, 60.0);
    assert!(!b.has(0));
    put(&mut w, BUBBLE_M - 1.0, V3::ZERO);
    update(&mut b, &w, 60.0);
    assert_eq!(b.share(0), SHARE_FULL, "comes in where it was drawn");
    let mut shares = vec![b.share(0)];
    let snaps = |ms: f32| (ms as u32 * TICK_HZ / 1000 / 2) as usize;
    for _ in 0..snaps(BLEND_MAX_MS) + 8 {
        update(&mut b, &w, 60.0);
        shares.push(b.share(0));
    }
    assert!(shares.windows(2).all(|p| p[1] <= p[0]), "only down: {shares:?}");
    assert_eq!(*shares.last().unwrap(), 0, "on the present: {shares:?}");
    // Never in less than the shortest blend, nor more than the longest.
    let to_zero = shares.iter().position(|&x| x == 0).unwrap();
    assert!(to_zero + 1 >= snaps(BLEND_MIN_MS), "{to_zero} snapshots: {shares:?}");
    assert!(to_zero <= snaps(BLEND_MAX_MS) + 2, "{to_zero} snapshots: {shares:?}");
    // Coming in fast and close (first seen in it a few metres off, at
    // 20 m/s), over the shortest blend, never quicker.
    let mut w = world();
    put(&mut w, 60.0, V3::ZERO);
    let mut b = Bubble::default();
    update(&mut b, &w, 60.0);
    put(&mut w, 17.0, V3::new(-20.0, 0.0, 0.0));
    let mut fast = vec![];
    for _ in 0..snaps(BLEND_MAX_MS) + 8 {
        update(&mut b, &w, 60.0);
        fast.push(b.share(0));
    }
    let fast_zero = fast.iter().position(|&x| x == 0).unwrap();
    assert!(fast_zero + 1 >= snaps(BLEND_MIN_MS), "{fast_zero} snapshots: {fast:?}");
    assert!(fast_zero <= snaps(BLEND_MIN_MS) + 1, "{fast_zero} snapshots: {fast:?}");
    assert!(fast_zero < to_zero, "{fast_zero} against {to_zero} parked");
    // Never sent before: on the present at once.
    let mut b = Bubble::default();
    b.update(&w, 0, &|_| true, &|_| true, &|_| true, 60.0, 2, true);
    assert_eq!(b.share(0), 0, "unseen till now");
    // Not live: whole, always.
    let mut b = Bubble::default();
    for _ in 0..snaps(BLEND_MAX_MS) + 8 {
        b.update(&w, 0, &|_| true, &|_| true, &|_| true, 60.0, 2, false);
    }
    assert!(b.has(0) && b.share(0) == SHARE_FULL);
}

/// What a snapshot said is what its acks' shots are met by.
#[test]
fn a_shot_after_a_snapshot_is_met_by_its_shares() {
    let mut w = world();
    put(&mut w, 5.0, V3::ZERO);
    w.vehicles[0].seats = [3, 0];
    let mut b = Bubble::default();
    for t in 0..40u32 {
        w.tick = 100 + 2 * t;
        update(&mut b, &w, 60.0);
        b.sent(&w, w.tick);
    }
    let seen = b.seen(Some(w.tick));
    assert_eq!(seen.share[MAX_FIGHTERS], b.share(0), "the hull");
    assert_eq!(seen.share[2], b.share(0), "its driver, with it");
    assert_eq!(seen.share[1], SHARE_FULL, "anyone else, whole");
    // An ack of a snapshot never sent, or long gone: whole.
    assert_eq!(b.seen(Some(w.tick + 1)), sim::world::Seen::PAST);
    assert_eq!(b.seen(None), sim::world::Seen::PAST);
}

/// A truck circling us slowly right at the edge (its distance wobbling a
/// metre and a half either side of where it comes in, a few times a
/// minute and faster) comes in once and stays: the hysteresis and the
/// dwell hold it, it never flaps.
#[test]
fn a_truck_circling_at_the_edge_never_flaps() {
    let mut w = world();
    let mut b = Bubble::default();
    let (mut flips, mut was) = (0, false);
    let speed = 5.0;
    let edge = BUBBLE_M + speed * LOOK_S;
    for k in 0..60 * TICK_HZ {
        let t = k as f32 * 2.0 / TICK_HZ as f32;
        let r = edge + 1.5 * sim::fmath::sin(t * 5.0);
        let a = t * speed / edge;
        let (s, c) = (sim::fmath::sin(a), sim::fmath::cos(a));
        w.vehicles[0].pos = V3::new(r * c, 0.0, r * s);
        w.vehicles[0].vel = V3::new(-s, 0.0, c) * speed;
        update(&mut b, &w, 60.0);
        flips += (b.has(0) != was) as u32;
        was = b.has(0);
    }
    assert_eq!(flips, 1, "{flips} flips in two minutes");
}

/// Who on foot comes in: one by a hull in that moves (or is driven), the
/// nearest six at most; never by a parked hull, never for standing by
/// someone else on foot.
#[test]
fn those_on_foot_come_in_by_a_moving_hull_only() {
    use room::bubble::MAX_NEAR_FOOT;
    let mut w = world();
    // Hull 0 rolling by us; seats 1..9 on foot around it, a metre off its side.
    put(&mut w, 6.0, V3::new(3.0, 0.0, 0.0));
    for q in 1..9 {
        w.fighters[q].body.pos = V3::new(6.0 + (q as f32 - 4.5) * 0.6, 0.0, VEH_HALF_W + 1.0);
        w.fighters[q].body.vel = V3::ZERO;
    }
    // Seat 9 by us, far from any hull.
    w.fighters[9].body.pos = V3::new(-1.0, 0.0, -1.0);
    let mut b = Bubble::default();
    update(&mut b, &w, 60.0);
    assert!(b.has(0));
    let on: Vec<usize> = (0..MAX_FIGHTERS).filter(|&q| b.has_foot(q)).collect();
    assert_eq!(on.len(), MAX_NEAR_FOOT, "{on:?}");
    assert!(!b.has_foot(9), "by us on foot, not by a hull");
    assert!(!b.has_foot(0), "never ourselves");
    // Parked: nobody comes in for it.
    put(&mut w, 6.0, V3::ZERO);
    let mut b = Bubble::default();
    update(&mut b, &w, 60.0);
    assert!(b.has(0) && (0..MAX_FIGHTERS).all(|q| !b.has_foot(q)));
}

/// Getting out of a hull in our bubble, on foot by it: at its share, never
/// a jump to the render tick.
#[test]
fn one_getting_out_comes_in_at_its_hulls_share() {
    let mut w = world();
    put(&mut w, 6.0, V3::new(-4.0, 0.0, 0.0));
    w.vehicles[0].seats = [3, 0];
    w.fighters[2].ride = sim::fighter::Fighter::ride_of(0, sim::vehicle::DRIVER);
    let mut b = Bubble::default();
    for _ in 0..20 {
        update(&mut b, &w, 60.0);
    }
    let hull = b.share(0);
    assert!(hull < SHARE_FULL, "the hull slid on: {hull}");
    // Out, beside it.
    w.vehicles[0].seats = [0, 0];
    w.fighters[2].ride = 0;
    w.fighters[2].body.pos = w.vehicles[0].pos + V3::new(0.0, 0.0, VEH_HALF_W + 1.0);
    w.vehicles[0].seats = [0, 0];
    w.vehicles[0].vel = V3::new(-4.0, 0.0, 0.0);
    update(&mut b, &w, 60.0);
    assert!(b.has_foot(2), "out by a moving hull: in");
    assert!(
        b.foot_share(2) <= hull + 1,
        "at {} against the hull's {hull}",
        b.foot_share(2)
    );
}

/// A truck wandering in and out across both edges (where it comes in and
/// where it stays to), slowly or quickly, or dipping in for a moment and
/// away, is in for the dwell and the leave at least each time it comes:
/// never a flap shorter than that.
#[test]
fn a_truck_crossing_the_stay_edge_is_held_each_time() {
    let speed = 5.0;
    let edge = BUBBLE_M + speed * LOOK_S;
    let snaps = |ms: f32| (ms * TICK_HZ as f32 / 1000.0 / 2.0) as usize;
    let least = snaps(DWELL_MS as f32 + LEAVE_MS) - 2;
    let wander = |period: f32| {
        move |t: f32| edge + HYST_M / 2.0 + (HYST_M / 2.0 + 2.0) * sim::fmath::sin(t * core::f32::consts::TAU / period)
    };
    // In 3 m for 0.3 s every 6 s, else well out.
    let dip = |t: f32| {
        let p = t % 6.0 - 3.0;
        if sim::fmath::fabs(p) < 0.15 {
            edge - 3.0
        } else {
            edge + HYST_M + 10.0
        }
    };
    let runs = |at: &dyn Fn(f32) -> f32| {
        let mut w = world();
        let mut b = Bubble::default();
        let (mut stretches, mut run) = (Vec::new(), 0usize);
        for k in 0..30 * TICK_HZ {
            let t = k as f32 * 2.0 / TICK_HZ as f32;
            let r = at(t);
            let a = t * speed / edge;
            let (s, c) = (sim::fmath::sin(a), sim::fmath::cos(a));
            w.vehicles[0].pos = V3::new(r * c, 0.0, r * s);
            w.vehicles[0].vel = V3::new(-s, 0.0, c) * speed;
            update(&mut b, &w, 60.0);
            if b.has(0) {
                run += 1;
            } else if run > 0 {
                stretches.push(run);
                run = 0;
            }
        }
        stretches
    };
    for period in [1.0f32, 3.0, 9.0] {
        let stretches = runs(&wander(period));
        assert!(
            stretches.iter().all(|&n| n >= least),
            "{period} s: {stretches:?}, at least {least}"
        );
        // Slowly, out past where it stays to for longer than the leave: it
        // goes, each time.
        if period == 9.0 {
            assert!(stretches.len() >= 5, "{period} s: never left: {stretches:?}");
        }
    }
    // In for a moment: held by the dwell, then the leave, then out.
    let stretches = runs(&dip);
    assert!(stretches.len() >= 9, "{stretches:?}");
    assert!(
        stretches.iter().all(|&n| (least..least + 6).contains(&n)),
        "{stretches:?}, at least {least}"
    );
}

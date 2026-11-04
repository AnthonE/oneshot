//! A correction eases out by its size: a big one quickly, a small one
//! slowly enough not to be seen, and what's left under a centimetre is
//! dropped (DESIGN §5; Gates' `Predictor::decay_error`).

use client_net::core::{NetCore, EASE_FAR_MS, EASE_NEAR_MS};
use proto::Snapshot;
use sim::consts::TICK_HZ;
use sim::fighter::Fighter;
use sim::geom::V3;
use sim::input::Input;
use sim::map::Map;
use sim::movement::Body;
use sim::world::Rules;

const TICK_MS: f64 = 1000.0 / TICK_HZ as f64;
const AT: V3 = V3::new(-30.0, 0.0, 0.0);

/// Standing still, then told we stand `by` metres along x from where we
/// predicted: how far the drawn body is from the server's after `ms`.
fn left_after(by: f32, ms: f64) -> (f32, NetCore) {
    let mut c = NetCore::new(0, 0, 0, Rules::duel(), Map::open());
    let me = Fighter {
        seated: true,
        alive: true,
        body: Body {
            pos: AT,
            grounded: true,
            ..Body::default()
        },
        ..Fighter::default()
    };
    c.on_snapshot(
        2.0 * TICK_MS,
        &Snapshot {
            tick: 2,
            phase: 1,
            last_seq: 7,
            own: me,
            ..Snapshot::default()
        },
    );
    let mut last = 0;
    let mut now = 0.0;
    while now < 200.0 {
        now += 1000.0 / 60.0;
        for m in c.update(now, Input::default()) {
            last = m.first_seq + m.n as u32 - 1;
        }
    }
    let mut told = *c.own().0;
    told.body.pos.x += by;
    c.on_snapshot(
        now,
        &Snapshot {
            tick: 40,
            phase: 1,
            last_seq: last,
            own: told,
            ..Snapshot::default()
        },
    );
    assert_eq!(c.stats.corrections, 1);
    let start = now;
    while now < start + ms {
        now += 1000.0 / 60.0;
        c.update(now, Input::default());
    }
    (told.body.pos.x - c.own().1.x, c)
}

#[test]
fn a_small_correction_eases_slowly() {
    // 20 cm: a third of a second to fall to a third.
    let (left, _) = left_after(0.2, 100.0);
    let want = 0.2 * (-100.0f64 / EASE_NEAR_MS).exp() as f32;
    assert!((left - want).abs() < 0.01, "{left} left, want about {want}");
}

#[test]
fn a_big_correction_eases_quickly() {
    // 2 m: a tenth of a second to fall to a third.
    let (left, _) = left_after(2.0, 100.0);
    let fast = 2.0 * (-100.0f64 / EASE_FAR_MS).exp() as f32;
    assert!(
        left > fast - 0.01 && left < fast + 0.1,
        "{left} left, want about {fast}"
    );
}

#[test]
fn what_is_left_under_a_centimetre_is_dropped() {
    // Drawn exactly where predicted (the position grid may have moved the
    // body off the server's number by a hair).
    let (_, c) = left_after(0.2, 3000.0);
    assert_eq!(c.own().1, c.own().0.body.pos);
}

#[test]
fn a_drivers_hull_eases_with_the_camera() {
    // Driving still on the open floor, told the vehicle sits 30 cm along
    // x: the hull and the seat ease there together, neither snaps.
    use sim::fighter::Fighter as F;
    use sim::vehicle::{Vehicle, DRIVER};
    let mut c = NetCore::new(0, 0, 0, Rules::duel(), Map::open());
    let hull = Vehicle {
        active: true,
        pos: AT,
        grounded: true,
        health: 100,
        seats: [1, 0],
        ..Vehicle::default()
    };
    let me = F {
        seated: true,
        alive: true,
        ride: F::ride_of(0, DRIVER),
        body: hull.seat_body(DRIVER),
        ..F::default()
    };
    c.on_snapshot(
        2.0 * TICK_MS,
        &Snapshot {
            tick: 2,
            phase: 1,
            last_seq: 7,
            own: me,
            own_vehicle: Some(hull),
            ..Snapshot::default()
        },
    );
    let mut last = 0;
    let mut now = 0.0;
    while now < 200.0 {
        now += 1000.0 / 60.0;
        for m in c.update(now, Input::default()) {
            last = m.first_seq + m.n as u32 - 1;
        }
    }
    let (_, hull0, _) = c.own_vehicle().unwrap();
    let seat0 = c.own().1;
    let mut v = c.own_vehicle().unwrap().0;
    v.pos.x += 0.3;
    let mut told = *c.own().0;
    told.body = v.seat_body(DRIVER);
    c.on_snapshot(
        now,
        &Snapshot {
            tick: 40,
            phase: 1,
            last_seq: last,
            own: told,
            own_vehicle: Some(v),
            ..Snapshot::default()
        },
    );
    assert_eq!(c.stats.corrections, 1);
    // Nothing moved on screen the moment it was told.
    let (_, hull1, _) = c.own_vehicle().unwrap();
    assert!((hull1 - hull0).len() < 1e-4, "the hull snapped {:?}", hull1 - hull0);
    assert!((c.own().1 - seat0).len() < 1e-4, "the seat snapped");
    // A tenth of a second on, both a quarter of the way there, together.
    let start = now;
    while now < start + 100.0 {
        now += 1000.0 / 60.0;
        c.update(now, Input::default());
    }
    let hull_moved = c.own_vehicle().unwrap().1.x - hull0.x;
    let seat_moved = c.own().1.x - seat0.x;
    assert!(hull_moved > 0.05 && hull_moved < 0.15, "hull {hull_moved}");
    assert!(
        (hull_moved - seat_moved).abs() < 0.01,
        "hull {hull_moved} seat {seat_moved}"
    );
}

#[test]
fn a_driver_stays_in_the_seat_of_the_drawn_hull_through_any_correction() {
    // The review's case: 3.8 m on and turned 0.575 rad, where the seat
    // moves over 4 m and the middle under. However it's drawn, snapped or
    // eased, the body is in the seat of the hull drawn.
    use sim::fighter::Fighter as F;
    use sim::vehicle::{Vehicle, DRIVER};
    for (dx, turn) in [(3.8f32, 6000u16), (-3.8, 59536), (4.1, 59536), (0.4, 9000), (6.0, 0)] {
        let mut c = NetCore::new(0, 0, 0, Rules::duel(), Map::open());
        let hull = Vehicle {
            active: true,
            pos: AT,
            grounded: true,
            health: 100,
            seats: [1, 0],
            ..Vehicle::default()
        };
        let me = F {
            seated: true,
            alive: true,
            ride: F::ride_of(0, DRIVER),
            body: hull.seat_body(DRIVER),
            ..F::default()
        };
        c.on_snapshot(
            2.0 * TICK_MS,
            &Snapshot {
                tick: 2,
                phase: 1,
                last_seq: 7,
                own: me,
                own_vehicle: Some(hull),
                ..Snapshot::default()
            },
        );
        let mut last = 0;
        let mut now = 0.0;
        while now < 200.0 {
            now += 1000.0 / 60.0;
            for m in c.update(now, Input::default()) {
                last = m.first_seq + m.n as u32 - 1;
            }
        }
        let mut v = c.own_vehicle().unwrap().0;
        v.pos.x += dx;
        v.yaw = v.yaw.wrapping_add(turn);
        let mut told = *c.own().0;
        told.body = v.seat_body(DRIVER);
        c.on_snapshot(
            now,
            &Snapshot {
                tick: 40,
                phase: 1,
                last_seq: last,
                own: told,
                own_vehicle: Some(v),
                ..Snapshot::default()
            },
        );
        for _ in 0..40 {
            let (hv, p, yaw) = c.own_vehicle().unwrap();
            let drawn = Vehicle {
                pos: p,
                yaw: (yaw as f64 * (65536.0 / core::f64::consts::TAU))
                    .round()
                    .rem_euclid(65536.0) as u16,
                ..hv
            };
            let apart = (c.own().1 - drawn.mount(DRIVER)).len();
            assert!(apart < 1e-3, "{dx} m, turn {turn}: the seat is {apart} m off the hull");
            now += 1000.0 / 60.0;
            c.update(now, Input::default());
        }
    }
}

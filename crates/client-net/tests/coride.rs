//! Riding together: each draws the other in its seat on the hull it draws,
//! ahead of the render tick, and what the other fires leaves from there.

mod common;

use common::{err, Lan};
use sim::events::Kind;
use sim::input::{Input, FIRE, USE};
use sim::vehicle::{DRIVER, PASSENGER};

/// Truck 0 driven on keys by A, B in its bed firing every 300 ms: how far
/// each of B's shots, as A is told of it, starts from B as A draws it.
fn shots_from_the_bed(delay_ms: f64, jitter_ms: f64) -> common::Err {
    let mut lan = Lan::dry_fork(&[0, 0], delay_ms, jitter_ms, 0.0, true);
    let v = lan.room.world.vehicles[0].pos;
    lan.place(0, v.x - 2.0, v.z);
    lan.place(1, v.x, v.z + 2.5);
    let press = Input {
        buttons: USE,
        ..Input::default()
    };
    let b = lan.cs[1].slot;
    let mut next_fire = 7000.0;
    let mut gaps = Vec::new();
    lan.run(
        16_000.0,
        |k, now| match k {
            0 if (4000.0..4040.0).contains(&now) => press,
            1 if (4500.0..4540.0).contains(&now) => press,
            0 if now > 6000.0 => Input {
                mz: 127,
                mx: [127, 0, -127, 0][((now - 6000.0) / 700.0) as usize % 4],
                ..Input::default()
            },
            1 if now >= next_fire => {
                next_fire += 300.0;
                Input {
                    buttons: FIRE,
                    ..Input::default()
                }
            }
            _ => Input::default(),
        },
        |k, c, now, _| {
            if k != 0 {
                return;
            }
            let events = c.drain_events(now);
            let Some(drawn) = c.others(now).into_iter().find(|d| d.slot == b) else {
                return;
            };
            for e in events.iter().filter(|e| e.kind == Kind::Shot as u8 && e.a == b) {
                gaps.push((e.pos - drawn.pos).flat().len());
            }
        },
    );
    let riding: Vec<_> = lan.cs.iter().map(|c| c.core.own().0.riding()).collect();
    assert_eq!(riding, [Some((0, DRIVER)), Some((0, PASSENGER))]);
    err(gaps.into_iter())
}

#[test]
fn a_co_riders_shots_leave_where_it_is_drawn() {
    for (delay, jitter) in [(20.0, 0.0), (60.0, 10.0), (150.0, 10.0)] {
        let e = shots_from_the_bed(delay, jitter);
        println!("{delay} ms: shots from the drawn passenger {e:?}");
        assert!(e.n > 20, "{delay} ms: only {} shots seen", e.n);
        // From its eye on the drawn seat: under a metre (the muzzle is a
        // little ahead of the eye, the seat on a hull moving between).
        assert!(e.mean < 0.6 && e.max < 1.2, "{delay} ms: {e:?}");
    }
}

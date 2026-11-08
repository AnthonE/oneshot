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

/// Where A, driving truck 0 flat out, draws B as B gets out of its bed
/// (`bubbles` as `Room::bubbles`: on, B is then in A's on foot): the most
/// it is off where the server has it, from when A hears of it till A's
/// render tick reaches it (where the newest snapshot has it; in the
/// bubble, on A's present); the most it is off the server once there, at
/// the render tick; how close it ever came, in between, to its seat on the
/// hull at the render tick.
fn getting_out_of_the_bed(delay_ms: f64, jitter_ms: f64, bubbles: bool) -> (f32, f32, f32) {
    let mut lan = Lan::dry_fork(&[0, 0], delay_ms, jitter_ms, 0.0, bubbles);
    let v = lan.room.world.vehicles[0].pos;
    lan.place(0, v.x - 2.0, v.z);
    lan.place(1, v.x, v.z + 2.5);
    let press = Input {
        buttons: USE,
        ..Input::default()
    };
    let b = lan.cs[1].slot;
    let (mut out_at, mut rode): (Option<u32>, bool) = (None, false);
    let mut seats = Vec::new();
    let (mut told, mut then, mut seat_near, mut n_told) = (0.0f32, 0.0f32, f32::MAX, 0);
    let mut frames = Vec::new();
    lan.run(
        9_000.0,
        |k, now| match k {
            0 if (4000.0..4040.0).contains(&now) => press,
            1 if (4500.0..4540.0).contains(&now) => press,
            1 if (7500.0..7540.0).contains(&now) => press,
            0 if now > 6000.0 => Input {
                mz: 127,
                ..Input::default()
            },
            _ => Input::default(),
        },
        |k, c, now, room| {
            let w = &room.world;
            seats.push((w.tick, w.vehicles[0].mount(PASSENGER)));
            let rides = w.fighters[b as usize].riding().is_some();
            if out_at.is_none() && rode && !rides {
                out_at = Some(w.tick);
            }
            rode |= rides;
            if k != 0 {
                return;
            }
            if let Some(d) = c.others(now).into_iter().find(|d| d.slot == b) {
                let at = if bubbles {
                    c.present_tick()
                } else {
                    c.latest().tick as f64
                };
                frames.push((c.latest().tick, at, c.render_tick(now), d.pos));
            }
        },
    );
    let out = out_at.expect("B never got out");
    let seat_at = |t: f64| {
        let i = seats.iter().position(|&(tick, _)| tick as f64 >= t)?;
        Some(seats[i].1)
    };
    for &(latest, at, rt, p) in &frames {
        if latest < out {
            continue;
        }
        if rt < out as f64 {
            let q = lan.body_at(b as usize, at).unwrap();
            told = told.max((p - q).len());
            n_told += 1;
            if let Some(s) = seat_at(rt) {
                seat_near = seat_near.min((p - s).len());
            }
        } else if let Some(q) = lan.body_at(b as usize, rt) {
            then = then.max((p - q).len());
        }
    }
    assert!(n_told > 0, "{delay_ms} ms: no frames between");
    println!("{delay_ms} ms: {n_told} frames between");
    assert_eq!(lan.cs[1].core.own().0.riding(), None);
    (told, then, seat_near)
}

#[test]
fn a_co_rider_getting_out_is_drawn_where_it_got_out() {
    for ((delay, jitter), bubbles) in [(20.0, 0.0), (60.0, 10.0), (150.0, 10.0)]
        .into_iter()
        .flat_map(|d| [(d, false), (d, true)])
    {
        let (told, then, seat_near) = getting_out_of_the_bed(delay, jitter, bubbles);
        println!("{delay} ms, bubbles {bubbles}: off {told} m, off the server after {then} m, nearest the old seat {seat_near} m");
        // Where the server has it, from the moment we hear.
        assert!(told < 0.05, "{delay} ms, bubbles {bubbles}: {told} m off");
        // Never back in its seat a round trip behind.
        assert!(
            seat_near > 1.0,
            "{delay} ms, bubbles {bubbles}: {seat_near} m from its seat at the render tick"
        );
        // And on the render tick once that gets there.
        assert!(then < 0.2, "{delay} ms, bubbles {bubbles}: {then} m off");
    }
}

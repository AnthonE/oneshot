//! Hulls in each other's bubble are predicted together: two trucks driven
//! head-on into each other, each driver re-running the other's hull on its
//! stick, so the crash is predicted rather than corrected.

mod common;

use common::{drawn_tick, err, Err, Lan};
use sim::geom::V3;
use sim::input::{Input, USE};
use sim::vehicle::DRIVER;

#[derive(Debug)]
struct Crash {
    /// Both drivers' corrections, and their drawn hulls against the
    /// server's at the tick drawn, from the moment they drive.
    corrections: u32,
    drawn: [Err; 2],
    /// The closest the hulls came, and the ticks re-run in all.
    closest: f32,
    near_snaps: u32,
}

/// `keys`: once they meet, each driver steers as on keys while it pushes
/// (full one way, none, the other, none, every 0.4 s, out of step with each
/// other), so the other's stick is guessed wrong after each change.
fn head_on(delay_ms: f64, jitter_ms: f64, loss: f64, bubbles: bool, keys: bool) -> Crash {
    let mut lan = Lan::dry_fork(&[0, 1], delay_ms, jitter_ms, loss, bubbles);
    // Truck 1 brought 30 m ahead of truck 0, facing it, on open ground.
    let w = &mut lan.room.world;
    let (f, _) = w.vehicles[0].axes();
    let a = w.vehicles[0].pos;
    let d = a + f * 30.0;
    w.vehicles[1].pos = V3::new(d.x, w.map.ground(d.x, d.z), d.z);
    w.vehicles[1].yaw = w.vehicles[0].yaw.wrapping_add(32768);
    let back = a - f * 2.0;
    let front = d + f * 2.0;
    lan.place(0, back.x, back.z);
    lan.place(1, front.x, front.z);
    let press = Input {
        buttons: USE,
        ..Input::default()
    };
    let go = |k: usize, now: f64| Input {
        mz: 127,
        mx: if keys && now > 7600.0 {
            [127, 0, -127, 0][((now + 150.0 * k as f64) / 400.0) as usize % 4]
        } else {
            0
        },
        ..Input::default()
    };
    lan.run(
        5000.0,
        |_, now| {
            if (4000.0..4040.0).contains(&now) {
                press
            } else {
                Input::default()
            }
        },
        |_, _, _, _| {},
    );
    let at_wheel: Vec<_> = lan.cs.iter().map(|c| c.core.own().0.riding()).collect();
    assert_eq!(at_wheel, [Some((0, DRIVER)), Some((1, DRIVER))]);
    let before: Vec<u32> = lan.cs.iter().map(|c| c.core.stats.corrections).collect();
    let mut seen: [Vec<(f64, V3)>; 2] = [Vec::new(), Vec::new()];
    lan.run(
        16_000.0,
        |k, now| if now > 6000.0 { go(k, now) } else { Input::default() },
        |k, c, _, _| {
            if let Some((_, p, _)) = c.own_vehicle() {
                seen[k].push((drawn_tick(c), p));
            }
        },
    );
    let closest = lan
        .hulls
        .iter()
        .map(|(_, h)| (h[0].pos - h[1].pos).flat().len())
        .fold(f32::MAX, f32::min);
    Crash {
        corrections: lan
            .cs
            .iter()
            .zip(&before)
            .map(|(c, b)| c.core.stats.corrections - b)
            .sum(),
        drawn: [0, 1].map(|k| {
            err(seen[k]
                .iter()
                .filter_map(|&(t, p)| lan.hull_at(k, t).map(|q| (p - q).len())))
        }),
        closest,
        near_snaps: lan.cs.iter().map(|c| c.core.stats.near_snaps).sum(),
    }
}

#[test]
fn a_head_on_crash_is_predicted_by_both_drivers() {
    let links = [(20.0, 0.0, 0.0), (60.0, 10.0, 0.0), (100.0, 10.0, 0.05)];
    for (keys, (delay, jitter, loss)) in [false, true].into_iter().flat_map(|k| links.map(|l| (k, l))) {
        let off = head_on(delay, jitter, loss, false, keys);
        let on = head_on(delay, jitter, loss, true, keys);
        println!(
            "{} {delay} ms ±{jitter}, {:.0}% lost:\n  coarse {off:?}\n  bubble {on:?}",
            if keys { "keys" } else { "held" },
            loss * 100.0
        );
        // Bumper to bumper, their middles are two half-lengths apart.
        assert!(
            off.closest < 2.0 * sim::consts::VEH_HALF_L + 0.5,
            "they never met: {off:?}"
        );
        assert!(on.near_snaps > 100 && off.near_snaps == 0, "{on:?} {off:?}");
        // Predicted together, the crash is mostly confirmed, not corrected.
        assert!(
            on.corrections * 2 <= off.corrections,
            "{delay} ms: {on:?} against {off:?}"
        );
        for k in 0..2 {
            assert!(
                on.drawn[k].max < off.drawn[k].max,
                "{delay} ms, driver {k}: {on:?} against {off:?}"
            );
        }
    }
}

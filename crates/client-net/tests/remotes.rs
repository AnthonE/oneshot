//! Everyone else past the newest snapshot: drawn on their velocity for at
//! most `EXTRAPOLATE_MS`, then held; and a gap longer than `LERP_SPAN_MS`
//! isn't drawn across (DESIGN §6).

use client_net::core::{NetCore, EXTRAPOLATE_MS, LERP_SPAN_MS};
use proto::{Remote, Snapshot};
use sim::consts::{ticks, TICK_HZ};
use sim::geom::V3;
use sim::map::Map;
use sim::world::Rules;

const TICK_MS: f64 = 1000.0 / TICK_HZ as f64;
/// Each snapshot lands this long after the server made it.
const LATE_MS: f64 = 40.0;

fn snap(tick: u32, x: f32) -> Snapshot {
    let mut s = Snapshot {
        tick,
        phase: 1,
        ..Snapshot::default()
    };
    s.remotes[0] = Remote {
        slot: 1,
        team: 1,
        alive: true,
        pos: V3::new(x, 0.0, 0.0),
        vel: V3::new(5.0, 0.0, 0.0),
        ..Remote::default()
    };
    s.n_remote = 1;
    s
}

/// A runner along x at 5 m/s, snapshot every other tick up to `last`.
fn running(last: u32) -> NetCore {
    let mut c = NetCore::new(0, 0, 0, Rules::duel(), Map::open());
    for t in (2..=last).step_by(2) {
        c.on_snapshot(t as f64 * TICK_MS + LATE_MS, &snap(t, t as f32 * 5.0 / TICK_HZ as f32));
    }
    c
}

fn x_at(c: &NetCore, now: f64) -> f32 {
    c.others(now).iter().find(|d| d.slot == 1).unwrap().pos.x
}

#[test]
fn a_runner_whose_snapshots_stop_runs_on_a_little_then_holds() {
    let c = running(400);
    let last_x = 400.0 * 5.0 / TICK_HZ as f32;
    let far = last_x + 5.0 * ticks(EXTRAPOLATE_MS) as f32 / TICK_HZ as f32;
    let at = 400.0 * TICK_MS + LATE_MS;
    // Run on past the last snapshot...
    let soon = x_at(&c, at + 60.0);
    assert!(soon > last_x && soon < far, "{soon}: {last_x} to {far}");
    // ...then held where the extrapolation ran out, however long it waits.
    for later in [400.0, 2000.0] {
        assert!((x_at(&c, at + later) - far).abs() < 1e-3, "{}", x_at(&c, at + later));
    }
}

#[test]
fn a_long_gap_is_not_drawn_across() {
    // Snapshots to tick 200, then one a second later with them 30 m on:
    // drawn where they were until then, never sliding between.
    let mut c = running(200);
    let gap = 200 + ticks(1000);
    assert!(gap - 200 > ticks(LERP_SPAN_MS));
    c.on_snapshot(gap as f64 * TICK_MS + LATE_MS, &snap(gap, 30.0));
    let was = 200.0 * 5.0 / TICK_HZ as f32;
    let now = gap as f64 * TICK_MS + LATE_MS;
    let rt = c.render_tick(now);
    assert!(rt > 200.0 && rt < gap as f64, "render tick {rt} inside the gap");
    assert_eq!(x_at(&c, now), was);
}

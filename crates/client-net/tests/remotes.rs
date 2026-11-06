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
fn a_long_gap_is_not_drawn_across_nor_pulled_back() {
    // Snapshots to tick 200, then one a second later with them 30 m on:
    // run on and held as if it hadn't come, never sliding between, never
    // pulled back from where they were drawn.
    let mut c = running(200);
    let gap = 200 + ticks(1000);
    assert!(gap - 200 > ticks(LERP_SPAN_MS));
    let arrive = gap as f64 * TICK_MS + LATE_MS;
    let held = x_at(&c, arrive - 1.0);
    c.on_snapshot(arrive, &snap(gap, 30.0));
    let rt = c.render_tick(arrive);
    assert!(rt > 200.0 && rt < gap as f64, "render tick {rt} inside the gap");
    assert_eq!(x_at(&c, arrive), held);
    let was = 200.0 * 5.0 / TICK_HZ as f32;
    let far = was + 5.0 * ticks(EXTRAPOLATE_MS) as f32 / TICK_HZ as f32;
    assert!((held - far).abs() < 1e-3, "held at {held}, want {far}");
}

#[test]
fn a_stall_and_its_burst_never_pull_everyone_back() {
    // Steady snapshots, then 250 ms held up and let go at once (a Wi-Fi
    // hiccup), then steady again: the render tick never goes back.
    let mut c = NetCore::new(0, 0, 0, Rules::duel(), Map::open());
    let mut last_rt = f64::NEG_INFINITY;
    let mut now = 0.0;
    let mut t = 2;
    while now < 4000.0 {
        now += 1000.0 / 144.0;
        while (t as f64) * TICK_MS + LATE_MS <= now {
            let made = t as f64 * TICK_MS;
            // Held from 2.0 s to 2.25 s, then all at once.
            let late = if (2000.0..2250.0).contains(&made) {
                2250.0 - made
            } else {
                0.0
            };
            if made + LATE_MS + late <= now {
                c.on_snapshot(now, &snap(t, t as f32 * 5.0 / TICK_HZ as f32));
                t += 2;
            } else {
                break;
            }
        }
        // From the first snapshot on.
        if t > 2 {
            let rt = c.render_tick(now);
            assert!(
                rt >= last_rt,
                "at {now:.1} ms the render tick went back {last_rt} to {rt}"
            );
            last_rt = rt;
        }
    }
}

#[test]
fn a_latency_step_is_caught_up_without_going_back() {
    // 40 ms, then 140 ms from 2 s on: the render tick slows and catches up
    // within half a second, never back.
    let mut c = NetCore::new(0, 0, 0, Rules::duel(), Map::open());
    let mut last_rt = f64::NEG_INFINITY;
    let mut now = 0.0;
    let mut t = 2;
    let mut held = 0;
    while now < 5000.0 {
        now += 1000.0 / 144.0;
        loop {
            let made = t as f64 * TICK_MS;
            let late = if made < 2000.0 { LATE_MS } else { LATE_MS + 100.0 };
            if made + late > now {
                break;
            }
            c.on_snapshot(now, &snap(t, t as f32 * 5.0 / TICK_HZ as f32));
            t += 2;
        }
        if t > 2 {
            let rt = c.render_tick(now);
            assert!(
                rt >= last_rt,
                "at {now:.1} ms the render tick went back {last_rt} to {rt}"
            );
            last_rt = rt;
        }
        if now > 2500.0 && c.extrapolating(now) {
            held += 1;
        }
    }
    assert_eq!(held, 0, "still past the newest snapshot half a second after the step");
}

#[test]
fn the_server_is_told_how_far_behind_we_draw() {
    // Through a jittery spell the delay's target jumps and what is drawn
    // follows it slowly: the server must hear what is drawn.
    use sim::input::Input;
    let mut c = NetCore::new(0, 0, 0, Rules::duel(), Map::open());
    let mut now = 0.0;
    let mut t = 2;
    let mut checked = 0;
    while now < 3000.0 {
        now += 1000.0 / 60.0;
        loop {
            let made = t as f64 * TICK_MS;
            // From 1 s on, every other snapshot 30 ms late.
            let late = if made > 1000.0 && t % 4 == 0 {
                LATE_MS + 30.0
            } else {
                LATE_MS
            };
            if made + late > now {
                break;
            }
            c.on_snapshot(now, &snap(t, 0.0));
            t += 2;
        }
        let newest = c.latest().tick as f64;
        for m in c.update(now, Input::default()) {
            if t == 2 {
                continue; // nothing drawn yet
            }
            let behind = (newest - c.render_tick(now)).round().clamp(0.0, 255.0);
            assert_eq!(m.playout as f64, behind, "at {now:.0} ms");
            checked += 1;
        }
    }
    assert!(checked > 100);
}

#[test]
fn a_far_runner_updated_every_few_snapshots_is_drawn_as_smoothly() {
    // The same runner, updated in every fourth snapshot and held in the
    // others, its last update repeated (a far one on a big map): drawn
    // between its own updates, it stays on its path.
    let mut c = NetCore::new(0, 0, 0, Rules::duel(), Map::open());
    let mut full = NetCore::new(0, 0, 0, Rules::duel(), Map::open());
    let mut last = snap(0, 0.0).remotes[0];
    for t in (2..=400u32).step_by(2) {
        let at = t as f64 * TICK_MS + LATE_MS;
        let s = snap(t, t as f32 * 5.0 / TICK_HZ as f32);
        full.on_snapshot(at, &s);
        let s = if (t / 2).is_multiple_of(4) {
            last = s.remotes[0];
            s
        } else {
            let mut h = Snapshot { held: 1 << 1, ..s };
            h.remotes[0] = last;
            h
        };
        c.on_snapshot(at, &s);
        if t > 40 {
            let now = at + 3.0;
            let (x, want) = (x_at(&c, now), x_at(&full, now));
            assert!(
                (x - want).abs() < 0.05,
                "tick {t}: drawn at {x}, updated every snapshot {want}"
            );
        }
    }
    // Held, then gone from view: no longer drawn.
    let gone = Snapshot {
        n_remote: 0,
        held: 0,
        ..snap(402, 0.0)
    };
    c.on_snapshot(402.0 * TICK_MS + LATE_MS, &gone);
    assert!(c.others(402.0 * TICK_MS + LATE_MS + 200.0).iter().all(|d| d.slot != 1));
}

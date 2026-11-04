//! Our own fighter is drawn between its last two predicted ticks, so a
//! steady run moves the same distance every frame whatever the frame rate:
//! no 128 Hz staircase under the camera (Gates' `ClientClock::alpha`).

use client_net::core::NetCore;
use proto::Snapshot;
use sim::consts::TICK_HZ;
use sim::fighter::Fighter;
use sim::geom::V3;
use sim::input::Input;
use sim::map::Map;
use sim::movement::Body;
use sim::world::Rules;

const TICK_MS: f64 = 1000.0 / TICK_HZ as f64;

/// Runs along +x on the open floor at `fps` and returns the drawn x of
/// every frame once up to speed.
fn run(fps: f64) -> Vec<f32> {
    let mut c = NetCore::new(0, 0, 0, Rules::duel(), Map::open());
    let s = Snapshot {
        tick: 2,
        phase: 1,
        last_seq: 7,
        own: Fighter {
            seated: true,
            alive: true,
            body: Body {
                pos: V3::new(-30.0, 0.0, 0.0),
                grounded: true,
                ..Body::default()
            },
            ..Fighter::default()
        },
        ..Snapshot::default()
    };
    c.on_snapshot(2.0 * TICK_MS, &s);
    let go = Input {
        mz: 127,
        ..Input::default()
    };
    let frame = 1000.0 / fps;
    let mut xs = Vec::new();
    for i in 0..(fps as usize) {
        c.update(i as f64 * frame, go);
        // Half a second to get up to speed, then half a second measured.
        if i as f64 * frame > 500.0 {
            xs.push(c.own().1.x);
        }
    }
    xs
}

fn steps(xs: &[f32]) -> (f32, f32) {
    let d: Vec<f32> = xs.windows(2).map(|w| w[1] - w[0]).collect();
    let lo = d.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = d.iter().copied().fold(0.0, f32::max);
    (lo, hi)
}

#[test]
fn a_steady_run_moves_the_same_each_frame() {
    for fps in [60.0, 144.0, 75.0] {
        let (lo, hi) = steps(&run(fps));
        assert!(lo > 0.0, "{fps} fps: a frame that didn't move ({lo})");
        assert!(hi / lo < 1.1, "{fps} fps: steps from {lo} to {hi} m");
    }
}

#[test]
fn alpha_walks_the_tick() {
    let mut c = NetCore::new(0, 0, 0, Rules::duel(), Map::open());
    c.update(0.0, Input::default());
    assert_eq!(c.alpha(), 0.0);
    c.update(TICK_MS * 0.25, Input::default());
    assert!((c.alpha() - 0.25).abs() < 1e-3, "{}", c.alpha());
    c.update(TICK_MS * 0.9, Input::default());
    assert!((c.alpha() - 0.9).abs() < 1e-3, "{}", c.alpha());
    c.update(TICK_MS * 1.1, Input::default());
    assert!((c.alpha() - 0.1).abs() < 1e-3, "{}", c.alpha());
}

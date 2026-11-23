//! Skiing, an experiment on terrain maps (docs/DESIGN.md §16, milestone 9):
//! crouched at speed going down a slope, a body slides on, faster than it
//! runs; running down it, or crouched on the flat, it is what it was.

use sim::consts::*;
use sim::geom::{rad_to_yaw, V3};
use sim::input::*;
use sim::movement::{step, Body};

/// A spot on Dry Fork's ground that falls away at a skiable pitch, and
/// which way is down it (flat, unit).
fn slope(map: &sim::map::Map) -> (V3, V3) {
    for i in 0..150 {
        for j in 0..70 {
            let (x, z) = (-150.0 + 2.0 * i as f32, -70.0 + 2.0 * j as f32);
            let n = map.ground_normal(x, z);
            // Steady for 15 m down it.
            let down = V3::new(n.x, 0.0, n.z).norm();
            let steady = (0..=15).all(|k| {
                let p = V3::new(x, 0.0, z) + down * k as f32;
                let m = map.ground_normal(p.x, p.z);
                (0.8..0.95).contains(&m.y) && V3::new(m.x, 0.0, m.z).norm().dot(down) > 0.9
            });
            if (0.8..0.95).contains(&n.y) && steady {
                return (V3::new(x, map.ground(x, z), z), down);
            }
        }
    }
    panic!("no slope on Dry Fork")
}

/// The flat speed after `secs` going `down` from `at` at a run, crouched or not.
fn after(map: &sim::map::Map, at: V3, down: V3, crouch: bool, secs: f32) -> f32 {
    let mut b = Body {
        pos: at,
        vel: down * RUN,
        grounded: true,
        ..Body::default()
    };
    let inp = Input {
        buttons: if crouch { CROUCH } else { 0 },
        yaw: rad_to_yaw(sim::fmath::atan2(down.z, down.x)),
        mz: 127,
        ..Input::default()
    };
    for _ in 0..(secs * TICK_HZ as f32) as u32 {
        step(&mut b, &inp, map);
    }
    b.vel.flat().len()
}

#[test]
fn crouched_at_speed_down_a_slope_it_slides_faster_than_it_runs() {
    let map = sim::maps::canyon::DEF.build();
    let (at, down) = slope(&map);
    let ski = after(&map, at, down, true, 0.75);
    let run = after(&map, at, down, false, 0.75);
    assert!(run <= RUN + 0.5, "a run down it is a run: {run}");
    assert!(ski > RUN * 1.2, "a ski down it: {ski} (a run {run})");
}

#[test]
fn crouched_on_the_flat_it_is_a_crouch() {
    // Proving Grounds has no terrain: a crouch at a run slows to a crouch.
    let map = sim::maps::proving_grounds::DEF.build();
    let at = map.spawns[0].pos;
    let s = after(&map, at, V3::new(0.0, 0.0, 1.0), true, 0.5);
    assert!(s <= CROUCH_RUN + 0.1, "{s}");
}

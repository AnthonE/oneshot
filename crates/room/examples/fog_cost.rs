//! `cargo run --release -p room --example fog_cost`: what fog of war costs
//! a sixteen-seat Big Team room, per map. Bots play; every snapshot (every
//! other tick) each seat's fog looks again at everyone, and at every
//! vehicle, as `Room::step` does. Prints how long one snapshot's fog takes for the room: the mean
//! and the spread. With `--check` (`ci/quick.sh`) it fails past
//! [`MEAN_US`] or [`P999_US`]: twice what the box grid and the spread look
//! (milestone 9) measured, so a slow machine passes and losing either
//! doesn't.

use std::time::Instant;

use room::fog::{lead_s, Fog};
use sim::bot::Bot;
use sim::consts::MAX_FIGHTERS;
use sim::input::Input;
use sim::world::{Rules, World};

/// The most a snapshot's fog may take on average, at sixteen seats.
const MEAN_US: f64 = 600.0;
/// And at the 99.9th percentile (the maximum is the scheduler's).
const P999_US: f64 = 3000.0;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let check = args.iter().any(|a| a == "--check");
    // --map <id>: one map only (for a profiler); --ticks <n>.
    let opt = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1)?.parse::<u32>().ok())
    };
    let only = opt("--map");
    // A shared CI runner is slower and noisier than a workstation: CI
    // scales the budget (FOG_BUDGET_SCALE=2 in ci.yml).
    let scale: f64 = std::env::var("FOG_BUDGET_SCALE")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v: &f64| *v >= 1.0)
        .unwrap_or(1.0);
    let ticks = opt("--ticks").unwrap_or(20_000);
    let mut over = false;
    for d in sim::maps::ALL.iter().filter(|d| only.is_none_or(|m| m == d.id as u32)) {
        let mut w = World::new(d.build(), Rules::big_team(), 7, (1, 2));
        let mut bots = [Bot::new(7, 0, 0.5); MAX_FIGHTERS];
        for (s, b) in bots.iter_mut().enumerate() {
            w.seat(s, (s & 1) as u8);
            *b = Bot::new(7, s, 0.3 + 0.2 * (s % 4) as f32);
        }
        let mut fogs: Vec<Fog> = (0..MAX_FIGHTERS).map(|_| Fog::default()).collect();
        let lead = lead_s(60.0, 4, room::SNAP_EVERY);
        let mut times: Vec<f64> = Vec::new();
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        for t in 0..ticks {
            for s in 0..MAX_FIGHTERS {
                inputs[s] = bots[s].think(&w, s);
            }
            w.step(&inputs, &[0; MAX_FIGHTERS]);
            if t % 2 == 0 && t > 1_000 {
                let t0 = Instant::now();
                for (s, f) in fogs.iter_mut().enumerate() {
                    f.update(&w, s, lead, w.tick);
                }
                times.push(t0.elapsed().as_secs_f64() * 1e6);
            }
        }
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        times.sort_by(|a, b| a.total_cmp(b));
        let at = |q: f64| times[((times.len() - 1) as f64 * q) as usize];
        // 64 snapshots a second.
        println!(
            "{:16} {:3} boxes: fog mean {:6.1} µs, p50 {:6.1}, p99 {:6.1}, p99.9 {:6.1}, max {:7.1}; {:4.1}% of a core at 64 Hz",
            d.name,
            w.map.boxes.len(),
            mean,
            at(0.5),
            at(0.99),
            at(0.999),
            at(1.0),
            mean * 64.0 / 1e4
        );
        over |= mean > MEAN_US * scale || at(0.999) > P999_US * scale;
    }
    if check && over {
        eprintln!(
            "fog is over budget: mean {} µs, p99.9 {} µs",
            MEAN_US * scale,
            P999_US * scale
        );
        std::process::exit(1);
    }
}

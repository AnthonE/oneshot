//! `cargo run --release -p room --example fog_cost`: what fog of war costs
//! a sixteen-seat Big Team room, per map. Bots play; every snapshot (every
//! other tick) each seat's fog looks again at everyone, as `Room::step`
//! does. Prints the mean and worst time of one snapshot's fog for the room.

use std::time::Instant;

use room::fog::{lead_s, Fog};
use sim::bot::Bot;
use sim::consts::MAX_FIGHTERS;
use sim::input::Input;
use sim::world::{Rules, World};

fn main() {
    for d in sim::maps::ALL {
        let mut w = World::new(d.build(), Rules::big_team(), 7, (1, 2));
        let mut bots = [Bot::new(7, 0, 0.5); MAX_FIGHTERS];
        for (s, b) in bots.iter_mut().enumerate() {
            w.seat(s, (s & 1) as u8);
            *b = Bot::new(7, s, 0.3 + 0.2 * (s % 4) as f32);
        }
        let mut fogs: Vec<Fog> = (0..MAX_FIGHTERS).map(|_| Fog::default()).collect();
        let lead = lead_s(60.0, 4);
        let (mut total, mut worst, mut n) = (0f64, 0f64, 0u32);
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        for t in 0..20_000u32 {
            for s in 0..MAX_FIGHTERS {
                inputs[s] = bots[s].think(&w, s);
            }
            w.step(&inputs, &[0; MAX_FIGHTERS]);
            if t % 2 == 0 && t > 1_000 {
                let t0 = Instant::now();
                for (s, f) in fogs.iter_mut().enumerate() {
                    f.update(&w, s, lead, w.tick);
                }
                let us = t0.elapsed().as_secs_f64() * 1e6;
                total += us;
                worst = worst.max(us);
                n += 1;
            }
        }
        let mean = total / n as f64;
        // 64 snapshots a second.
        println!(
            "{:16} {:3} boxes: fog {:7.1} µs a snapshot (worst {:7.1}), {:4.1}% of a core at 64 Hz",
            d.name,
            w.map.boxes.len(),
            mean,
            worst,
            mean * 64.0 / 1e4
        );
    }
}

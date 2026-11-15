//! `cargo run --release -p sim --example bots -- <map> [minutes] [seed] [mode]`:
//! a bot match of Capture the Flag (mode 3, the default; 0 Doubles, 1 Gun
//! Game, 2 Big Team's Slayer, 4 Siege) on map `map`, and how the bots
//! played it: flags taken, dropped, home and captured, kills, vehicles
//! boarded and wrecked, and by job how long they stood still and spun
//! there; in Siege (served as it is, its clock and score), each node's
//! break, how long each ring rose, held and fell, and who stood in them.
//! What tells a change to the bots worked.
//!
//! `DEBUG=1`: every flag event. `KILLS=1`: who killed whom, by job.
//! `TRACE=seat,from_s,to_s`: that seat's every look. `TRUCK=i`: vehicle `i`
//! and whoever's job is about it, every second.

// A command-line tool, not the sim: it may print, time and use a map.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::needless_range_loop
)]

use std::collections::BTreeMap;

use sim::bot::{Bot, Job};
use sim::consts::*;
use sim::events::Kind as Ev;
use sim::geom::V3;
use sim::input::Input;
use sim::world::{Rules, World, NO_WINNER};

/// A job's name, without what it is about.
fn name(j: Job) -> String {
    format!("{j:?}").split('(').next().unwrap_or("").to_string()
}

/// Its vehicle, for a job about one.
fn vehicle_of(j: Job) -> Option<usize> {
    match j {
        Job::Board(i, _) | Job::Drive(i, ..) | Job::Ride(i) | Job::Carry(i) => Some(i as usize),
        _ => None,
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let map: u8 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);
    let minutes: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let mode: u8 = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let env = |k: &str| std::env::var(k).ok();
    let debug = env("DEBUG").is_some();
    let kills_by_job = env("KILLS").is_some();
    let trace: Option<(usize, u32, u32)> = env("TRACE").and_then(|v| {
        let p: Vec<u32> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        (p.len() == 3).then(|| (p[0] as usize, p[1], p[2]))
    });
    let truck: Option<usize> = env("TRUCK").and_then(|v| v.parse().ok());

    let t0 = std::time::Instant::now();
    let Some(def) = sim::maps::ALL.iter().find(|d| d.id == map) else {
        println!("no map {map}");
        return;
    };
    let m = def.build();
    let nav = sim::nav::Nav::of(&m).expect("a map of the game's");
    let built = t0.elapsed();
    println!(
        "waypoints: {} on foot, {} for trucks; {} spots",
        nav.foot.nodes.len(),
        nav.wheels.as_ref().map_or(0, |g| g.nodes.len()),
        nav.spots.len()
    );
    let base = match mode {
        0 => Rules::doubles(),
        1 => Rules::gun_game(),
        2 => Rules::big_team(),
        4 => Rules::siege(),
        _ if def.big() => Rules::big_ctf(),
        _ => Rules::ctf(),
    };
    // Siege plays as it is served (its score, its clock, sudden death), so
    // how a match ends shows; the rest never end.
    let siege = base.siege;
    let rules = if siege {
        base
    } else {
        Rules {
            score_limit: 1000,
            time_limit_ms: 0,
            ..base
        }
    };
    let n = rules.seats as usize;
    let mut w = World::new(m, rules, seed, (seed ^ 1, seed ^ 2));
    let mut bots = [Bot::new(seed, 0, 0.5); MAX_FIGHTERS];
    for (s, b) in bots.iter_mut().enumerate().take(n) {
        w.seat(s, (s & 1) as u8);
        *b = Bot::new(seed, s, if siege { SIEGE_BOT_SKILL } else { BOT_SKILL });
    }
    // Siege: each node's broken (when, by whom), and seconds each node's
    // ring stood rising, contested, falling; per side, seat-seconds in an
    // enemy ring and in its own.
    let mut broke: Vec<(f32, usize)> = Vec::new();
    let mut held_s = [[0u32; 4]; SIEGE_NODES];
    let mut in_ring_s = [[0u32; 2]; 2];
    let (mut minion_kills, mut tower_kills) = (0u32, 0u32);
    let mut ended: Option<f32> = None;

    let total = ticks(minutes * 60_000);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let (mut taken, mut dropped, mut home, mut caps, mut kills) = (0, 0, 0, [0; 2], 0);
    let (mut boarded, mut wrecks) = (0, 0);
    // Per seat, each second: where it was, and how far its yaw turned.
    let mut last = [V3::ZERO; MAX_FIGHTERS];
    let mut turned = [0f32; MAX_FIGHTERS];
    let mut prev_yaw = [0u16; MAX_FIGHTERS];
    // By job: seconds alive on foot, still, spinning.
    let mut by_job: BTreeMap<String, (u32, u32, u32)> = BTreeMap::new();
    let mut kills_by: BTreeMap<(String, String), u32> = BTreeMap::new();
    let (mut drive_t, mut ride_t, mut pilot_t, mut alive_t) = (0u64, 0u64, 0u64, 0u64);
    let t1 = std::time::Instant::now();
    for t in 0..total {
        for s in 0..n {
            inputs[s] = bots[s].think(&w, s);
        }
        if let Some((ts, from, to)) = trace {
            if t >= from * TICK_HZ && t < to * TICK_HZ && t % 4 == 0 {
                let f = &w.fighters[ts];
                let h = bots[ts].heading();
                println!(
                    "  t={:.2} {:?} pos ({:.2},{:.2},{:.2}) yaw {:.3} heading ({:.1},{:.1},{:.1}) mz {} mx {} b {:#x} speed {:.1} target {:?}",
                    t as f32 / TICK_HZ as f32,
                    bots[ts].job(),
                    f.body.pos.x,
                    f.body.pos.y,
                    f.body.pos.z,
                    f.yaw as f32 / 65536.0,
                    h.x,
                    h.y,
                    h.z,
                    inputs[ts].mz,
                    inputs[ts].mx,
                    inputs[ts].buttons,
                    f.body.vel.flat().len(),
                    bots[ts].target()
                );
            }
        }
        if let Some(vi) = truck.filter(|_| t % TICK_HZ == 0) {
            let v = &w.vehicles[vi];
            let mut who = String::new();
            for s in 0..n {
                let j = bots[s].job();
                let f = &w.fighters[s];
                if vehicle_of(j) == Some(vi) && f.active() {
                    who += &format!(
                        " [{s} {j:?} ({:.0},{:.0},{:.0}) flag {}]",
                        f.body.pos.x, f.body.pos.y, f.body.pos.z, f.flag
                    );
                }
            }
            println!(
                "  truck t={}s v{vi} ({:.0},{:.0},{:.0}) speed {:.1} seats {:?} hp {}{who}",
                t / TICK_HZ,
                v.pos.x,
                v.pos.y,
                v.pos.z,
                v.vel.flat().len(),
                v.seats,
                v.health
            );
        }
        w.step(&inputs, &[0; MAX_FIGHTERS]);
        for e in w.events.as_slice() {
            if debug && e.kind == Ev::Flag {
                println!(
                    "  flag t={:.1}s part {} by {} side {} at ({:.0},{:.0},{:.0})",
                    t as f32 / TICK_HZ as f32,
                    e.part,
                    e.a,
                    e.b,
                    e.pos.x,
                    e.pos.y,
                    e.pos.z
                );
            }
            if e.kind == Ev::Kill && (e.a as usize) < n && (e.b as usize) < n {
                *kills_by
                    .entry((name(bots[e.a as usize].job()), name(bots[e.b as usize].job())))
                    .or_insert(0) += 1;
            }
            match (e.kind, e.part) {
                (Ev::Flag, 0) => taken += 1,
                (Ev::Flag, 1) => dropped += 1,
                (Ev::Flag, 2) => home += 1,
                (Ev::Flag, 3) => caps[(e.b as usize & 1) ^ 1] += 1,
                (Ev::Kill, _) => {
                    kills += 1;
                    tower_kills += (e.a as usize >= n) as u32;
                }
                (Ev::Ride, _) if e.c == 0 => boarded += 1,
                (Ev::Wreck, _) => wrecks += 1,
                (Ev::Node, 0) => {
                    broke.push((t as f32 / TICK_HZ as f32, e.b as usize));
                    if debug {
                        println!(
                            "  node t={:.1}s {} broken, first in its ring seat {}",
                            t as f32 / TICK_HZ as f32,
                            e.b,
                            e.a
                        );
                    }
                }
                (Ev::Minion, 0) if e.a != NO_WINNER => minion_kills += 1,
                _ => {}
            }
        }
        if siege && ended.is_none() && w.phase == sim::world::Phase::Over {
            ended = Some(t as f32 / TICK_HZ as f32);
            break;
        }
        if siege && t % TICK_HZ == 0 {
            if let Some(at) = w.map.siege {
                for (k, node) in w.nodes.iter().enumerate() {
                    held_s[k][node.held as usize & 3] += 1;
                }
                for s in 0..n {
                    let f = &w.fighters[s];
                    if !f.active() || f.riding().is_some() {
                        continue;
                    }
                    let mine = f.team as usize & 1;
                    for (k, &p) in at.iter().enumerate() {
                        let d = f.body.pos - p;
                        if d.flat().len() < NODE_REACH && d.y.abs() < NODE_RISE {
                            let theirs = World::node_side(k) != mine;
                            in_ring_s[mine][theirs as usize] += 1;
                        }
                    }
                }
            }
        }
        for s in 0..n {
            let f = &w.fighters[s];
            if !f.active() {
                continue;
            }
            alive_t += 1;
            match f.riding() {
                Some((i, 0)) if w.vehicles[i].kind == sim::vehicle::TRUCK => drive_t += 1,
                Some((_, 0)) => pilot_t += 1,
                Some(_) => ride_t += 1,
                None => {}
            }
            turned[s] += (f.yaw.wrapping_sub(prev_yaw[s]) as i16 as f32 / 65536.0).abs();
            prev_yaw[s] = f.yaw;
        }
        if t % TICK_HZ == 0 {
            for s in 0..n {
                let f = &w.fighters[s];
                if f.active() && f.riding().is_none() {
                    let e = by_job.entry(name(bots[s].job())).or_default();
                    e.0 += 1;
                    if (f.body.pos - last[s]).flat().len() < 1.0 {
                        e.1 += 1;
                        if turned[s] > 0.5 {
                            e.2 += 1;
                        }
                    }
                }
                last[s] = f.body.pos;
                turned[s] = 0.0;
            }
        }
    }
    let ran = t1.elapsed();
    let pc = |a: u64, of: u64| 100.0 * a as f64 / of.max(1) as f64;
    println!(
        "{} ({n} seats, {minutes} min, seed {seed}): built in {:.2}s, ran in {:.1}s ({:.0} µs a tick)",
        def.name,
        built.as_secs_f64(),
        ran.as_secs_f64(),
        ran.as_secs_f64() * 1e6 / total as f64
    );
    println!(
        "  flags: taken {taken}, dropped {dropped}, home on the clock {home}, captured red {} blue {}",
        caps[0], caps[1]
    );
    println!("  kills {kills}, boarded {boarded}, wrecks {wrecks}");
    if siege {
        let side = |k: usize| if World::node_side(k) == 0 { "red" } else { "blue" };
        let what = |k: usize| {
            if World::is_core(k) {
                "core".to_string()
            } else {
                format!("gen {}", k % (SIEGE_GENS + 1))
            }
        };
        match ended {
            Some(e) => println!(
                "  siege: over at {:.0}s ({:.1} min), winner {}, score red {} blue {}",
                e,
                e / 60.0,
                ["red", "blue"].get(w.winner as usize).unwrap_or(&"none"),
                w.score[0],
                w.score[1]
            ),
            None => println!(
                "  siege: still on at the end, score red {} blue {}",
                w.score[0], w.score[1]
            ),
        }
        for (at, k) in &broke {
            println!("    {:>6.0}s {}'s {} broken", at, side(*k), what(*k));
        }
        for (k, h) in held_s.iter().enumerate() {
            println!(
                "    {:>4} {:>5}: rising {:>4}s, contested {:>4}s, falling {:>4}s",
                side(k),
                what(k),
                h[1],
                h[2],
                h[3]
            );
        }
        println!(
            "    seat-seconds in rings: red in blue's {} / own {}; blue in red's {} / own {}",
            in_ring_s[0][1], in_ring_s[0][0], in_ring_s[1][1], in_ring_s[1][0]
        );
        println!("    minions killed by fighters {minion_kills}; fighters killed by towers or minions {tower_kills}");
    }
    let (a, st, sp) = by_job.values().fold((0u64, 0u64, 0u64), |(a, b, c), &(x, y, z)| {
        (a + x as u64, b + y as u64, c + z as u64)
    });
    println!(
        "  on foot: still {:.0}% of seconds, spinning in place {:.0}%",
        pc(st, a),
        pc(sp, a)
    );
    for (j, &(a, st, sp)) in &by_job {
        println!(
            "    {j:>7}: {a:>5} s, still {:.0}%, spinning {:.0}%",
            pc(st as u64, a as u64),
            pc(sp as u64, a as u64)
        );
    }
    if kills_by_job {
        let mut v: Vec<_> = kills_by.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        for ((a, b), k) in v.iter().take(12) {
            println!("    {a:>7} killed {b:>7}: {k}");
        }
    }
    println!(
        "  alive: driving {:.1}%, riding {:.1}%, piloting {:.1}%",
        pc(drive_t, alive_t),
        pc(ride_t, alive_t),
        pc(pilot_t, alive_t)
    );
}

//! `cargo run --release -p sim --example bots -- <map> [minutes] [seed] [mode]`:
//! a bot match of Capture the Flag (mode 3, the default; 0 Doubles, 1 Gun
//! Game, 2 Big Team's Slayer, 4 Siege) on map `map`, and how the bots
//! played it: flags taken, dropped, home and captured, kills, vehicles
//! boarded and wrecked, and by job how long they stood still and spun
//! there; in Siege (served as it is, its clock and score), each node's
//! break, how long each ring rose, held and fell, who stood in them, how
//! many pushed together, how soon a rising ring was answered, what the
//! trucks delivered, and one `SIEGE` line to grep. What tells a change to
//! the bots worked.
//!
//! `DEBUG=1`: every flag event (in Siege, every break and each side's
//! target as it changes). `KILLS=1`: who killed whom, by job.
//! `TRACE=seat,from_s,to_s`: that seat's every look. `TRUCK=i`: vehicle `i`
//! and whoever's job is about it, every second. `STRONG=s1,s2`: those
//! seats aim at 0.5, stand-ins for better players.

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
use sim::events::{Cause, Kind as Ev};
use sim::geom::V3;
use sim::input::Input;
use sim::world::{Rules, World, NODE_CONTESTED, NODE_FALLING, NODE_QUIET, NODE_RISING, NO_WINNER};

/// A job's name, without what it is about.
fn name(j: Job) -> String {
    format!("{j:?}").split('(').next().unwrap_or("").to_string()
}

/// The node a Siege job is about, by the job's name ("Push", "Hold").
fn about(j: Job, what: &str) -> Option<usize> {
    let s = format!("{j:?}");
    let (n, rest) = s.split_once('(')?;
    (n == what).then(|| rest.trim_end_matches(')').parse().ok()).flatten()
}

/// Whether Siege node `n` can be overloaded: standing, and a core only
/// with its shield down.
fn standing(w: &World, n: usize) -> bool {
    !w.nodes[n].broken && (!World::is_core(n) || w.core_open(World::node_side(n)))
}

/// Whether `p` is in node `at`'s ring: as near as a fighter on foot counts.
fn in_ring(p: V3, at: V3) -> bool {
    let d = p - at;
    d.flat().len() < NODE_REACH && d.y.abs() < NODE_RISE
}

/// The middle of `v` (sorted here), or 0 for none.
fn median(v: &mut [u32]) -> u32 {
    v.sort_unstable();
    v.get(v.len() / 2).copied().unwrap_or(0)
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
    let strong: Vec<usize> = env("STRONG")
        .map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect())
        .unwrap_or_default();

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
        let skill = if strong.contains(&s) {
            0.5
        } else if siege {
            SIEGE_BOT_SKILL
        } else {
            BOT_SKILL
        };
        *b = Bot::new(seed, s, skill);
    }
    // Siege: each node's broken (when, by whom), and seconds each node's
    // ring stood rising, contested, falling; per side, seat-seconds in an
    // enemy ring (one that can be overloaded) and in its own.
    let mut broke: Vec<(f32, usize)> = Vec::new();
    let mut held_s = [[0u32; 4]; SIEGE_NODES];
    let mut in_ring_s = [[0u32; 2]; 2];
    let (mut minion_kills, mut by_tower, mut by_minion) = (0u32, 0u32, 0u32);
    let mut ended: Option<f32> = None;
    // Per side, seconds an enemy ring rose with nobody of the side in it
    // (its minions alone), one, two, three or more.
    let mut pushes = [[0u32; 4]; 2];
    // Per node, seconds it has been rising this spell; per side owning
    // them, spells of 2 s or more: how long each answered one rose, how
    // many emptied, how many broke.
    let mut spell = [0u32; SIEGE_NODES];
    let mut answered: [Vec<u32>; 2] = [Vec::new(), Vec::new()];
    let (mut spells, mut emptied, mut spell_broke) = ([0u32; 2], [0u32; 2], [0u32; 2]);
    // Attacker seats' truck rides, and those that ended with the rider
    // alive on foot by a standing enemy node.
    let mut in_truck = [false; MAX_FIGHTERS];
    let (mut rides, mut drops) = (0u32, 0u32);
    // Attacker seats' lives, and of those, how many came within 30 m of a
    // standing enemy node and how many stood in its ring.
    let mut was_alive = [false; MAX_FIGHTERS];
    let (mut came, mut stood) = ([false; MAX_FIGHTERS], [false; MAX_FIGHTERS]);
    let (mut lives, mut lives_near, mut lives_in) = (0u32, 0u32, 0u32);
    // Hold seconds, and those more than 30 m from the node; defender seats'
    // ticks alive and riding.
    let (mut hold_s, mut hold_away) = (0u32, 0u32);
    let (mut def_t, mut def_ride) = (0u64, 0u64);
    // Every seat's run at a truck's wheel, and the longest.
    let mut wheel = [0u32; MAX_FIGHTERS];
    let mut longest = 0u32;
    // Push seconds on foot out of its ring, and those it stood still.
    let (mut push_out, mut push_still) = (0u32, 0u32);

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
                let by = if e.c == Cause::Tower as u16 {
                    "tower".to_string()
                } else if e.c == Cause::Minion as u16 {
                    "minion".to_string()
                } else {
                    name(bots[e.a as usize].job())
                };
                *kills_by.entry((by, name(bots[e.b as usize].job()))).or_insert(0) += 1;
            }
            match (e.kind, e.part) {
                (Ev::Flag, 0) => taken += 1,
                (Ev::Flag, 1) => dropped += 1,
                (Ev::Flag, 2) => home += 1,
                (Ev::Flag, 3) => caps[(e.b as usize & 1) ^ 1] += 1,
                (Ev::Kill, _) => {
                    kills += 1;
                    by_tower += (e.c == Cause::Tower as u16) as u32;
                    by_minion += (e.c == Cause::Minion as u16) as u32;
                }
                (Ev::Ride, _) if e.c == 0 => boarded += 1,
                (Ev::Wreck, _) => wrecks += 1,
                (Ev::Node, 0) => {
                    let k = e.b as usize;
                    broke.push((t as f32 / TICK_HZ as f32, k));
                    if spell[k] >= 2 {
                        spells[World::node_side(k)] += 1;
                        spell_broke[World::node_side(k)] += 1;
                    }
                    spell[k] = 0;
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
        if let Some(at) = w.map.siege.filter(|_| siege) {
            // Every tick: lives begun, truck rides ended, runs at a wheel,
            // defenders' time riding.
            for s in 0..n {
                let f = &w.fighters[s];
                let att = (s / 2) % 2 == 0;
                let truck_seat = f.riding().filter(|&(i, _)| w.vehicles[i].kind == sim::vehicle::TRUCK);
                if f.active() && !was_alive[s] && att {
                    lives += 1;
                    came[s] = false;
                    stood[s] = false;
                }
                was_alive[s] = f.active();
                if in_truck[s] && truck_seat.is_none() && att {
                    rides += 1;
                    let mine = f.team as usize & 1;
                    let by = (0..SIEGE_NODES).any(|k| {
                        World::node_side(k) != mine && standing(&w, k) && (f.body.pos - at[k]).flat().len() < 40.0
                    });
                    drops += (f.active() && f.riding().is_none() && by) as u32;
                }
                in_truck[s] = truck_seat.is_some();
                if matches!(truck_seat, Some((_, 0))) {
                    wheel[s] += 1;
                    longest = longest.max(wheel[s]);
                } else {
                    wheel[s] = 0;
                }
                if !att && f.active() {
                    def_t += 1;
                    def_ride += f.riding().is_some() as u64;
                }
            }
        }
        if siege && t % TICK_HZ == 0 {
            if let Some(at) = w.map.siege {
                for (k, node) in w.nodes.iter().enumerate() {
                    held_s[k][node.held as usize & 3] += 1;
                    if node.broken {
                        continue;
                    }
                    // A rising spell: on while it rises, ended by an
                    // answer (held or winding back) or by nobody there.
                    let owner = World::node_side(k);
                    if node.held == NODE_RISING {
                        spell[k] += 1;
                        let by = owner ^ 1;
                        let count = (0..n)
                            .filter(|&s| {
                                let f = &w.fighters[s];
                                f.active()
                                    && f.riding().is_none()
                                    && f.team as usize & 1 == by
                                    && in_ring(f.body.pos, at[k])
                            })
                            .count();
                        pushes[by][count.min(3)] += 1;
                    } else if spell[k] > 0 {
                        if spell[k] >= 2 {
                            spells[owner] += 1;
                            match node.held {
                                NODE_CONTESTED | NODE_FALLING => answered[owner].push(spell[k]),
                                NODE_QUIET => emptied[owner] += 1,
                                _ => {}
                            }
                        }
                        spell[k] = 0;
                    }
                }
                for s in 0..n {
                    let f = &w.fighters[s];
                    if !f.active() {
                        continue;
                    }
                    let mine = f.team as usize & 1;
                    let job = bots[s].job();
                    if let Some(k) = about(job, "Hold") {
                        hold_s += 1;
                        hold_away += ((f.body.pos - at[k]).flat().len() > 30.0) as u32;
                    }
                    if (s / 2) % 2 == 0 {
                        for k in (0..SIEGE_NODES).filter(|&k| World::node_side(k) != mine && standing(&w, k)) {
                            if !came[s] && (f.body.pos - at[k]).flat().len() < 30.0 {
                                came[s] = true;
                                lives_near += 1;
                            }
                            if !stood[s] && f.riding().is_none() && in_ring(f.body.pos, at[k]) {
                                stood[s] = true;
                                lives_in += 1;
                            }
                        }
                    }
                    if f.riding().is_some() {
                        continue;
                    }
                    if about(job, "Push").is_some_and(|k| !in_ring(f.body.pos, at[k])) {
                        push_out += 1;
                        push_still += ((f.body.pos - last[s]).flat().len() < 1.0) as u32;
                    }
                    for (k, &p) in at.iter().enumerate() {
                        if in_ring(f.body.pos, p) && standing(&w, k) {
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
        println!("    minions killed by fighters {minion_kills}; fighters killed by towers {by_tower}, by minions {by_minion}");
        for (side, p) in pushes.iter().enumerate() {
            println!(
                "    pushes {}: minions only {} s, 1: {}, 2: {}, 3+: {} (grouped {:.0}%)",
                ["red", "blue"][side],
                p[0],
                p[1],
                p[2],
                p[3],
                pc((p[2] + p[3]) as u64, (p[1] + p[2] + p[3]) as u64)
            );
        }
        for side in 0..2 {
            let x = answered[side].len();
            println!(
                "    answered {}: {x} of {} rising spells >= 2 s, median after {} s; emptied {}, broke {}",
                ["red", "blue"][side],
                spells[side],
                median(&mut answered[side]),
                emptied[side],
                spell_broke[side]
            );
        }
        println!("    truck drops: {drops} of {rides} attacker truck rides");
        println!(
            "    near: {:.0}% of attacker lives came within 30 m of a standing enemy node, {:.0}% stood in its ring",
            pc(lives_near as u64, lives as u64),
            pc(lives_in as u64, lives as u64)
        );
        println!(
            "    holders away: {:.0}% of Hold seconds more than 30 m from their node; defenders riding {:.1}%",
            pc(hold_away as u64, hold_s as u64),
            pc(def_ride, def_t)
        );
        println!("    longest drive: {} s", longest / TICK_HZ);
        println!(
            "    push on foot outside rings: still {:.0}% of {push_out} s",
            pc(push_still as u64, push_out as u64)
        );
        let first = broke.first().map_or("-".to_string(), |b| format!("{:.0}", b.0));
        let (end, how) = match ended {
            Some(e) if broke.iter().any(|b| World::is_core(b.1)) => (e, "core"),
            Some(e) => (e, "clock"),
            None => ((total / TICK_HZ) as f32, "unfinished"),
        };
        let winner = match (ended, w.winner) {
            (Some(_), 0) => "red",
            (Some(_), 1) => "blue",
            _ => "none",
        };
        println!(
            "SIEGE map={map} seed={seed} end={end:.0} how={how} winner={winner} breaks={}-{} first={first}",
            w.score[0], w.score[1]
        );
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

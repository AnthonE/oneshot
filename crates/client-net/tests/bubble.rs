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

#[derive(Debug)]
struct Pass {
    /// The watcher's drawn truck against the server's at the tick it is
    /// drawn on (how true the drawing is to its own clock), and against the
    /// server's at the watcher's present while the truck is within 12 m.
    true_to_tick: Err,
    near_now: Err,
    /// How much faster than the truth the drawn truck went, frame to
    /// frame, in m/s: its 99th centile and its worst.
    surge: (f32, f32),
}

/// A truck at full throttle past someone standing 8 m off its path:
/// `live` as `Room::bubble_live`.
fn drive_by(delay_ms: f64, jitter_ms: f64, live: bool) -> Pass {
    let mut lan = Lan::dry_fork_live(&[0, 0], delay_ms, jitter_ms, 0.0, true, live);
    let v = lan.room.world.vehicles[0];
    let (f, r) = v.axes();
    let back = v.pos - f * 2.0;
    let watch = v.pos + f * 40.0 + r * 8.0;
    lan.place(0, back.x, back.z);
    lan.place(1, watch.x, watch.z);
    let press = Input {
        buttons: USE,
        ..Input::default()
    };
    lan.run(
        5000.0,
        |k, now| {
            if k == 0 && (4000.0..4040.0).contains(&now) {
                press
            } else {
                Input::default()
            }
        },
        |_, _, _, _| {},
    );
    let mut seen: Vec<(f64, f64, V3, f64)> = Vec::new();
    lan.run(
        10_000.0,
        |k, now| {
            if k == 0 && now > 5500.0 {
                Input {
                    mz: 127,
                    ..Input::default()
                }
            } else {
                Input::default()
            }
        },
        |k, c, now, _| {
            if k != 1 {
                return;
            }
            if let Some(d) = c.vehicles(now).into_iter().find(|d| d.slot == 0) {
                seen.push((c.vehicle_tick(0, now), c.present_tick(), d.pos, now));
            }
        },
    );
    let me = lan.room.world.fighters[lan.cs[1].slot as usize].body.pos;
    let true_to_tick = err(seen
        .iter()
        .filter_map(|&(t, _, p, _)| lan.hull_at(0, t).map(|q| (p - q).len())));
    let near_now = err(seen.iter().filter_map(|&(_, now_t, p, _)| {
        let q = lan.hull_at(0, now_t)?;
        ((q - me).flat().len() < 12.0).then(|| (p - q).len())
    }));
    let mut surges: Vec<f32> = seen
        .windows(2)
        .filter_map(|w| {
            let ((t0, _, p0, n0), (t1, _, p1, n1)) = (w[0], w[1]);
            let (q0, q1) = (lan.hull_at(0, t0)?, lan.hull_at(0, t1)?);
            Some(((p1 - p0).len() - (q1 - q0).len()) / ((n1 - n0) / 1000.0) as f32)
        })
        .collect();
    surges.sort_by(f32::total_cmp);
    let surge = (surges[surges.len() * 99 / 100], *surges.last().unwrap());
    Pass {
        true_to_tick,
        near_now,
        surge,
    }
}

#[test]
fn a_truck_in_the_bubble_is_drawn_on_the_present() {
    for (delay, jitter) in [(20.0, 0.0), (60.0, 10.0), (100.0, 10.0)] {
        let past = drive_by(delay, jitter, false);
        let live = drive_by(delay, jitter, true);
        println!("{delay} ms ±{jitter}:\n  past {past:?}\n  live {live:?}");
        // Close by, where the server has it now, not a round trip back.
        assert!(past.near_now.mean > 1.0, "{delay} ms: {past:?}");
        assert!(
            live.near_now.mean < 0.1 && live.near_now.max < 0.5,
            "{delay} ms: {live:?}"
        );
        // Drawn where it was at the tick it is drawn on.
        assert!(
            live.true_to_tick.mean < 0.05 && live.true_to_tick.max < 0.5,
            "{delay} ms: {live:?}"
        );
        // Sliding onto the present, never a lunge.
        assert!(
            live.surge.0 < 3.0 && live.surge.1 < 0.5 * sim::consts::VEH_TOP,
            "{delay} ms: {live:?}"
        );
    }
}

/// [`drive_by`]'s watcher firing a battle rifle at the truck as it draws
/// it, every 300 ms: the hull hits the server says it scored.
fn fire_at_drive_by(delay_ms: f64, jitter_ms: f64, live: bool) -> (u32, u32) {
    use sim::events::Kind;
    use sim::input::FIRE;
    let mut lan = Lan::dry_fork_live(&[0, 1], delay_ms, jitter_ms, 0.0, true, live);
    let v = lan.room.world.vehicles[0];
    let (f, r) = v.axes();
    let back = v.pos - f * 2.0;
    let watch = v.pos + f * 40.0 + r * 8.0;
    lan.place(0, back.x, back.z);
    lan.place(1, watch.x, watch.z);
    let press = Input {
        buttons: USE,
        ..Input::default()
    };
    lan.run(
        5000.0,
        |k, now| {
            if k == 0 && (4000.0..4040.0).contains(&now) {
                press
            } else {
                Input::default()
            }
        },
        |_, _, _, _| {},
    );
    let aim = std::cell::Cell::new((0u16, 0i16));
    let next = std::cell::Cell::new(5600.0f64);
    let (mut shots, mut hits) = (0, 0);
    let me = lan.cs[1].slot;
    lan.run(
        11_000.0,
        |k, now| match k {
            0 if now > 5500.0 => Input {
                mz: 127,
                ..Input::default()
            },
            1 => {
                let (yaw, pitch) = aim.get();
                let fire = now >= next.get();
                if fire {
                    next.set(next.get() + 300.0);
                }
                Input {
                    buttons: if fire { FIRE } else { 0 },
                    yaw,
                    pitch,
                    ..Input::default()
                }
            }
            _ => Input::default(),
        },
        |k, c, now, _| {
            if k != 1 {
                return;
            }
            for e in c.drain_events(now) {
                hits += (e.kind == Kind::VehicleHit as u8 && e.a == me) as u32;
                shots += (e.kind == Kind::Shot as u8 && e.a == me) as u32;
            }
            let Some(d) = c.vehicles(now).into_iter().find(|d| d.slot == 0) else {
                return;
            };
            let (f, at) = c.own();
            let eye = at + (f.eye() - f.body.pos);
            let to = d.pos + V3::new(0.0, 0.8, 0.0) - eye;
            let yaw = sim::geom::rad_to_yaw(sim::fmath::atan2(to.z, to.x));
            let flat = (to.x * to.x + to.z * to.z).sqrt();
            let pitch = sim::fmath::round(sim::fmath::atan2(to.y, flat) * (65536.0 / core::f32::consts::TAU)) as i16;
            aim.set((yaw, pitch));
        },
    );
    (shots, hits)
}

#[test]
fn shots_at_a_truck_in_the_bubble_meet_it_where_it_is_drawn() {
    for (delay, jitter) in [(20.0, 0.0), (60.0, 10.0), (100.0, 10.0)] {
        let past = fire_at_drive_by(delay, jitter, false);
        let live = fire_at_drive_by(delay, jitter, true);
        println!("{delay} ms ±{jitter}: past {past:?} live {live:?} (shots, hull hits)");
        assert!(past.1 > 5, "{delay} ms: the watcher never hit it: {past:?}");
        assert!(live.1 * 100 >= past.1 * 95, "{delay} ms: {live:?} against {past:?}");
    }
}

#[derive(Debug)]
struct Shove {
    /// The one on foot's own corrections, and its drawn body against where
    /// the server had it at the tick drawn, through the contact.
    corrections: u32,
    drawn: Err,
}

/// On foot against a truck: `creep` false, running into a parked one's
/// side; true, stood in front of one its driver creeps forward under the
/// speed that runs anyone down.
fn against_a_truck(delay_ms: f64, jitter_ms: f64, bubbles: bool, creep: bool) -> Shove {
    let mut lan = Lan::dry_fork(&[0, 0], delay_ms, jitter_ms, 0.0, bubbles);
    let v = lan.room.world.vehicles[0];
    let (f, r) = v.axes();
    let back = v.pos - f * 2.0;
    let me = if creep { v.pos + f * 6.0 } else { v.pos + r * 6.0 };
    lan.place(0, back.x, back.z);
    lan.place(1, me.x, me.z);
    let press = Input {
        buttons: USE,
        ..Input::default()
    };
    let toward = sim::geom::rad_to_yaw(sim::fmath::atan2(-r.z, -r.x));
    lan.run(
        5000.0,
        |k, now| {
            if creep && k == 0 && (4000.0..4040.0).contains(&now) {
                press
            } else {
                Input::default()
            }
        },
        |_, _, _, _| {},
    );
    let before = lan.cs[1].core.stats.corrections;
    let slot = lan.cs[1].slot as usize;
    let speed = std::cell::Cell::new(0.0f32);
    let mut seen = Vec::new();
    lan.run(
        10_000.0,
        |k, _| match (k, creep) {
            (0, true) => Input {
                mz: if speed.get() < 2.0 { 127 } else { 0 },
                ..Input::default()
            },
            (1, false) => Input {
                yaw: toward,
                mz: 127,
                ..Input::default()
            },
            _ => Input::default(),
        },
        |k, c, _, room| {
            speed.set(room.world.vehicles[0].vel.flat().len());
            if k == 1 {
                seen.push((drawn_tick(c), c.own().1));
            }
        },
    );
    Shove {
        corrections: lan.cs[1].core.stats.corrections - before,
        drawn: err(seen
            .iter()
            .filter_map(|&(t, p)| lan.body_at(slot, t).map(|q| (p - q).len()))),
    }
}

#[test]
fn a_truck_is_solid_to_our_own_prediction() {
    for creep in [false, true] {
        for (delay, jitter) in [(20.0, 0.0), (60.0, 10.0)] {
            let off = against_a_truck(delay, jitter, false, creep);
            let on = against_a_truck(delay, jitter, true, creep);
            let what = if creep {
                "pushed by a creeping truck"
            } else {
                "into a parked one"
            };
            println!("{what}, {delay} ms ±{jitter}:\n  coarse {off:?}\n  bubble {on:?}");
            assert!(off.corrections > 20, "{what}: never touched it? {off:?}");
            assert!(
                on.corrections * 10 <= off.corrections,
                "{what}, {delay} ms: {on:?} against {off:?}"
            );
            assert!(on.drawn.mean < 0.05 && on.drawn.max < 0.5, "{what}, {delay} ms: {on:?}");
        }
    }
}

/// Whether a body's feet at `p` are inside drawn hull `v`'s footprint.
fn in_drawn_hull(v: &client_net::core::DrawnVehicle, p: V3) -> bool {
    use sim::consts::{BODY_R, VEH_HALF_L, VEH_HALF_W};
    let (s, c) = (v.yaw.sin(), v.yaw.cos());
    let d = p - v.pos;
    let (lx, lz) = (d.x * c + d.z * s, d.z * c - d.x * s);
    lx.abs() < VEH_HALF_L + BODY_R - 0.1 && lz.abs() < VEH_HALF_W + BODY_R - 0.1
}

#[derive(Debug)]
struct Ghosts {
    /// Frames the watcher drew the body inside the drawn hull, of those it
    /// drew both; and its hits on the body, of its shots.
    inside: (u32, u32),
    hits: (u32, u32),
}

/// A truck creeping into someone on foot who strafes as it pushes them; a
/// watcher off to the side drawing both, firing at the body.
fn push_watched(delay_ms: f64, jitter_ms: f64, foot: bool) -> Ghosts {
    use sim::events::Kind;
    use sim::input::FIRE;
    let mut lan = Lan::dry_fork_live(&[0, 0, 1], delay_ms, jitter_ms, 0.0, true, true);
    lan.room.bubble_foot = foot;
    let v = lan.room.world.vehicles[0];
    let (f, r) = v.axes();
    let back = v.pos - f * 2.0;
    let body = v.pos + f * 5.0;
    let watch = v.pos + f * 8.0 + r * 12.0;
    lan.place(0, back.x, back.z);
    lan.place(1, body.x, body.z);
    lan.place(2, watch.x, watch.z);
    let press = Input {
        buttons: USE,
        ..Input::default()
    };
    lan.run(
        5000.0,
        |k, now| {
            if k == 0 && (4000.0..4040.0).contains(&now) {
                press
            } else {
                Input::default()
            }
        },
        |_, _, _, _| {},
    );
    let (b, me) = (lan.cs[1].slot, lan.cs[2].slot);
    let speed = std::cell::Cell::new(0.0f32);
    let aim = std::cell::Cell::new((0u16, 0i16));
    let next = std::cell::Cell::new(5200.0f64);
    let ahead = sim::geom::rad_to_yaw(sim::fmath::atan2(f.z, f.x));
    let (mut inside, mut both, mut shots, mut hits) = (0, 0, 0, 0);
    lan.run(
        20_000.0,
        |k, now| match k {
            0 => Input {
                mz: if speed.get() < 2.0 { 127 } else { 0 },
                ..Input::default()
            },
            // Facing along the truck's way, strafing across it.
            1 => Input {
                yaw: ahead,
                mx: [127, -127][((now / 450.0) as usize) % 2],
                ..Input::default()
            },
            _ => {
                let (yaw, pitch) = aim.get();
                let fire = now >= next.get();
                if fire {
                    next.set(next.get() + 200.0);
                }
                Input {
                    buttons: if fire { FIRE } else { 0 },
                    yaw,
                    pitch,
                    ..Input::default()
                }
            }
        },
        |k, c, now, room| {
            speed.set(room.world.vehicles[0].vel.flat().len());
            if k != 2 {
                return;
            }
            for e in c.drain_events(now) {
                hits += (e.kind == Kind::Hit as u8 && e.a == me && e.b == b) as u32;
                shots += (e.kind == Kind::Shot as u8 && e.a == me) as u32;
            }
            let (Some(h), Some(d)) = (
                c.vehicles(now).into_iter().find(|d| d.slot == 0),
                c.others(now).into_iter().find(|d| d.slot == b),
            ) else {
                return;
            };
            both += 1;
            inside += in_drawn_hull(&h, d.pos) as u32;
            let (fm, at) = c.own();
            let eye = at + (fm.eye() - fm.body.pos);
            let to = d.pos + V3::new(0.0, 1.1, 0.0) - eye;
            let yaw = sim::geom::rad_to_yaw(sim::fmath::atan2(to.z, to.x));
            let flat = (to.x * to.x + to.z * to.z).sqrt();
            let pitch = sim::fmath::round(sim::fmath::atan2(to.y, flat) * (65536.0 / core::f32::consts::TAU)) as i16;
            aim.set((yaw, pitch));
        },
    );
    Ghosts {
        inside: (inside, both),
        hits: (hits, shots),
    }
}

/// The gate for those on foot in the bubble: drawn inside a hull at least
/// half as rarely as without them, and shot at no worse (within five
/// points of the hit rate).
#[test]
fn those_on_foot_by_a_truck_are_drawn_with_it() {
    let (mut off_all, mut on_all) = ((0, 0, 0, 0), (0, 0, 0, 0));
    for (delay, jitter) in [(20.0, 0.0), (60.0, 10.0), (100.0, 10.0)] {
        let off = push_watched(delay, jitter, false);
        let on = push_watched(delay, jitter, true);
        println!("{delay} ms ±{jitter}:\n  hulls only {off:?}\n  with foot {on:?}");
        for (all, g) in [(&mut off_all, &off), (&mut on_all, &on)] {
            *all = (
                all.0 + g.inside.0,
                all.1 + g.inside.1,
                all.2 + g.hits.0,
                all.3 + g.hits.1,
            );
        }
    }
    println!("all: hulls only {off_all:?}, with foot {on_all:?} (inside, drawn, hits, shots)");
    assert!(off_all.0 > 20, "no ghosting to cut: {off_all:?}");
    assert!(on_all.0 * 2 <= off_all.0, "{on_all:?} against {off_all:?}");
    let rate = |(_, _, h, s): (u32, u32, u32, u32)| h as f64 / s.max(1) as f64;
    assert!(off_all.3 > 100 && on_all.3 > 100, "too few shots");
    assert!(rate(on_all) >= rate(off_all) - 0.05, "{on_all:?} against {off_all:?}");
}

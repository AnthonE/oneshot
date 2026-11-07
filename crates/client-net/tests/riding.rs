//! Riding a truck over a simulated network, three clients against the real
//! room on Dry Fork: one at the wheel, one in the bed behind it, one on
//! foot watching. How far each draws the truck from where the server has
//! it at the tick it draws it: the measure the netcode's vehicle work is
//! judged by (`-- --nocapture` prints it).

use client_net::core::NetCore;
use proto::{decode_input, decode_snapshot, encode_input, encode_snapshot, Look, Msg, Name, MAX_DATAGRAM};
use room::Room;
use sim::arms::{ION_PISTOL, MAGNUM};
use sim::consts::TICK_HZ;
use sim::geom::V3;
use sim::input::{Input, FIRE, USE};
use sim::rng::Pcg32;
use sim::vehicle::{Vehicle, DRIVER, PASSENGER};
use sim::world::Rules;

const TICK_MS: f64 = 1000.0 / TICK_HZ as f64;
/// Boarded by then; measured from then to the end.
const FROM_MS: f64 = 7000.0;
const END_MS: f64 = 20_000.0;
const FRAME_MS: f64 = 1000.0 / 60.0;

/// One client's link: latency, jitter and loss, each way.
struct Net {
    rng: Pcg32,
    delay_ms: f64,
    jitter_ms: f64,
    loss: f64,
    up: Vec<(f64, Vec<u8>)>,
    down: Vec<(f64, Vec<u8>)>,
}

impl Net {
    fn unit(&mut self) -> f64 {
        self.rng.next_u32() as f64 / 4_294_967_296.0
    }

    fn send(&mut self, now: f64, bytes: Vec<u8>, up: bool) {
        if self.unit() < self.loss {
            return;
        }
        let at = now + self.delay_ms + self.unit() * self.jitter_ms;
        if up {
            self.up.push((at, bytes));
        } else {
            self.down.push((at, bytes));
        }
    }

    fn due(q: &mut Vec<(f64, Vec<u8>)>, now: f64) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        q.retain(|(at, b)| {
            if *at <= now {
                out.push(b.clone());
                false
            } else {
                true
            }
        });
        out
    }
}

struct Client {
    core: NetCore,
    slot: u8,
    net: Net,
    next_frame: f64,
}

/// Mean and worst of a run's errors, in metres.
#[derive(Clone, Copy, Debug, Default)]
struct Err {
    n: usize,
    mean: f32,
    max: f32,
}

fn err(e: impl Iterator<Item = f32>) -> Err {
    let (mut n, mut sum, mut max) = (0, 0.0f32, 0.0f32);
    for x in e {
        n += 1;
        sum += x;
        max = max.max(x);
    }
    Err {
        n,
        mean: sum / n.max(1) as f32,
        max,
    }
}

#[derive(Debug)]
struct Measured {
    /// The driver and the passenger: their drawn hull against the server's
    /// at the tick it is drawn on.
    driver: Err,
    passenger: Err,
    /// The one on foot: the truck as drawn against the server's at the
    /// render tick (what interpolation means to show), and at the tick it
    /// draws itself on (how far behind the present the truck is).
    watcher: Err,
    watcher_now: Err,
    /// The passenger's stats from boarding on.
    hull_corrections: u32,
    corrections: u32,
    /// Firing from the bed: shots made, the ids our rounds were drawn
    /// under (one each, if each was bound to the server's), and the worst
    /// any of them was drawn to jump in a frame past its own flight.
    shots: u32,
    ids: usize,
    worst_jump: f32,
    /// Each rider's drawing of the other in its seat (the driver's of its
    /// passenger, the passenger's of its driver) against the server's seat
    /// at the drawn tick.
    co_seen: [Err; 2],
    /// The passenger's own seat, drawn, against the server's.
    own_seat: Err,
}

/// Where the server's truck 0 seats `seat` at fractional tick `t`.
fn seat_truth(h: &[(u32, Vehicle)], t: f64, seat: usize) -> Option<V3> {
    let lo = t.floor() as u32;
    let first = h.first()?.0;
    let a = h.get(lo.checked_sub(first)? as usize)?.1.mount(seat);
    let b = h.get((lo + 1 - first) as usize)?.1.mount(seat);
    Some(a + (b - a) * (t - lo as f64) as f32)
}

/// The server's truck 0 at fractional tick `t`, between whole ticks.
fn truth(h: &[(u32, Vehicle)], t: f64) -> Option<V3> {
    let lo = t.floor() as u32;
    let first = h.first()?.0;
    let a = h.get(lo.checked_sub(first)? as usize)?.1;
    let b = h.get((lo + 1 - first) as usize)?.1;
    let u = (t - lo as f64) as f32;
    Some(a.pos + (b.pos - a.pos) * u)
}

/// The tick a client's drawn body is on: `own()` draws between the
/// predictions after seqs `tick() - 2` and `tick() - 1`, and seq `q` runs
/// on the server's tick `q + latest.tick - latest.last_seq`.
fn drawn_tick(c: &NetCore) -> f64 {
    let l = c.latest();
    let k = l.tick.wrapping_sub(l.last_seq) as i32 as f64;
    c.tick() as f64 - 2.0 + c.alpha() as f64 + k
}

/// How the driver steers: smoothly, or on keys (full left, none, full
/// right, none, each held 0.7 s): the hardest stick to guess.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Steer {
    Smooth,
    Keys,
}

/// `fire`: the passenger taps an ion pistol every 300 ms once boarded.
fn ride(delay_ms: f64, jitter_ms: f64, loss: f64, steer: Steer, fire: bool) -> Measured {
    let map = sim::map::by_id(3).expect("Dry Fork");
    let rules = Rules {
        score_limit: 1000,
        time_limit_ms: 0,
        loadout: [ION_PISTOL, MAGNUM],
        ..Rules::big_team()
    };
    let mut room = Room::new(map.clone(), rules, 11, (5, 6));
    // Bots stand still: none takes the truck's seats.
    room.passive_bots = true;
    let mut cs: Vec<Client> = Vec::new();
    for k in 0..3u64 {
        let (slot, out) = room.join_team(Name::new("t"), Look::default(), Some(0)).unwrap();
        let Some((_, Msg::Welcome { tick, team, rules, .. })) = out.first().copied() else {
            panic!("no welcome");
        };
        cs.push(Client {
            core: NetCore::new(slot, team, tick, rules, map.clone()),
            slot,
            net: Net {
                rng: Pcg32::new(9, 9 + k),
                delay_ms,
                jitter_ms,
                loss,
                up: Vec::new(),
                down: Vec::new(),
            },
            next_frame: k as f64 * 5.0,
        });
    }
    // The driver and the passenger beside truck 0, the watcher off a way.
    let v = room.world.vehicles[0].pos;
    for (c, (dx, dz)) in cs.iter().zip([(-2.0, 0.0), (0.0, 2.5), (10.0, -12.0)]) {
        let (x, z) = (v.x + dx, v.z + dz);
        let y = room.world.map.ground(x, z);
        room.world.fighters[c.slot as usize].body.pos = V3::new(x, y, z);
    }
    let mut history: Vec<(u32, Vehicle)> = Vec::new();
    // (drawn tick, drawn middle) for the driver, the passenger and the
    // watcher, and the render tick the watcher drew it at.
    let mut seen: [Vec<(f64, V3)>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut seen_rt: Vec<(f64, V3)> = Vec::new();
    let mut co: [Vec<(f64, V3)>; 2] = [Vec::new(), Vec::new()];
    let mut own_seat: Vec<(f64, V3)> = Vec::new();
    let slots: Vec<u8> = cs.iter().map(|c| c.slot).collect();
    let mut at_from = (0, 0, 0);
    let mut ids: Vec<u16> = Vec::new();
    let mut last_drawn: Vec<(u16, V3)> = Vec::new();
    let mut worst_jump = 0.0f32;
    let mut next_fire = FROM_MS;
    let mut buf = [0u8; MAX_DATAGRAM];
    let (mut now, mut next_tick) = (0.0f64, 0.0f64);
    while now < END_MS {
        for c in cs.iter_mut() {
            for b in Net::due(&mut c.net.up, now) {
                room.on_input(c.slot, &decode_input(&b).unwrap(), now);
            }
            for b in Net::due(&mut c.net.down, now) {
                c.core.on_snapshot(now, &decode_snapshot(&b).unwrap());
            }
        }
        while next_tick <= now {
            let (snaps, _) = room.step(now);
            for (seat, s) in snaps {
                let n = encode_snapshot(&s, &mut buf).unwrap();
                if let Some(c) = cs.iter_mut().find(|c| c.slot == seat) {
                    c.net.send(now, buf[..n].to_vec(), false);
                }
            }
            history.push((room.world.tick, room.world.vehicles[0]));
            next_tick += TICK_MS;
        }
        for (k, c) in cs.iter_mut().enumerate() {
            if c.next_frame > now {
                continue;
            }
            c.next_frame += FRAME_MS;
            let press = Input {
                buttons: USE,
                ..Input::default()
            };
            // In at the wheel at 4 s, behind it at 4.5 s; driven from 6 s
            // at full throttle, swerving.
            let mx = match steer {
                Steer::Smooth => ((now / 900.0).sin() * 70.0) as i8,
                Steer::Keys => [127, 0, -127, 0][((now - 6000.0) / 700.0) as usize % 4],
            };
            let tap = k == 1 && fire && now >= next_fire;
            if tap {
                next_fire += 300.0;
            }
            let inp = match k {
                0 if (4000.0..4040.0).contains(&now) => press,
                1 if (4500.0..4540.0).contains(&now) => press,
                0 if now > 6000.0 => Input {
                    mz: 127,
                    mx,
                    ..Input::default()
                },
                1 if tap => Input {
                    buttons: FIRE,
                    ..Input::default()
                },
                _ => Input::default(),
            };
            for m in c.core.update(now, inp) {
                let n = encode_input(&m, &mut buf).unwrap();
                c.net.send(now, buf[..n].to_vec(), true);
            }
            if now < FROM_MS {
                if k == 1 {
                    let st = &c.core.stats;
                    at_from = (st.hull_corrections, st.corrections, c.core.own().0.shots);
                }
                continue;
            }
            let core = &c.core;
            if k == 1 {
                for p in core.projectiles(now).into_iter().filter(|p| p.owner == c.slot) {
                    if !ids.contains(&p.id) {
                        ids.push(p.id);
                    }
                    if let Some((_, p0)) = last_drawn.iter().find(|(id, _)| *id == p.id) {
                        let flight = p.vel.len() * (FRAME_MS / 1000.0) as f32;
                        worst_jump = worst_jump.max((p.pos - *p0).len() - flight);
                    }
                    last_drawn.retain(|(id, _)| *id != p.id);
                    last_drawn.push((p.id, p.pos));
                }
            }
            if k < 2 {
                if let Some((_, p, _)) = core.own_vehicle() {
                    seen[k].push((drawn_tick(core), p));
                }
                if let Some(d) = core.others(now).into_iter().find(|d| d.slot == slots[1 - k]) {
                    co[k].push((drawn_tick(core), d.pos));
                }
                if k == 1 && core.own().0.riding().is_some() {
                    own_seat.push((drawn_tick(core), core.own().1));
                }
            } else if let Some(d) = core.vehicles(now).into_iter().find(|d| d.slot == 0) {
                seen[2].push((drawn_tick(core), d.pos));
                seen_rt.push((core.render_tick(now), d.pos));
            }
        }
        now += 0.5;
    }
    let riding: Vec<_> = cs.iter().map(|c| c.core.own().0.riding()).collect();
    assert_eq!(riding, [Some((0, DRIVER)), Some((0, PASSENGER)), None]);
    let moved = (history.last().unwrap().1.pos - v).len();
    assert!(moved > 50.0, "the truck went only {moved} m");
    let off = |s: &[(f64, V3)]| err(s.iter().filter_map(|&(t, p)| truth(&history, t).map(|q| (p - q).len())));
    let p = &cs[1].core.stats;
    Measured {
        driver: off(&seen[0]),
        passenger: off(&seen[1]),
        watcher: off(&seen_rt),
        watcher_now: off(&seen[2]),
        hull_corrections: p.hull_corrections - at_from.0,
        corrections: p.corrections - at_from.1,
        shots: cs[1].core.own().0.shots - at_from.2,
        ids: ids.len(),
        worst_jump,
        co_seen: [PASSENGER, DRIVER].map(|seat| {
            let k = if seat == PASSENGER { 0 } else { 1 };
            err(co[k]
                .iter()
                .filter_map(|&(t, p)| seat_truth(&history, t, seat).map(|q| (p - q).len())))
        }),
        own_seat: err(own_seat
            .iter()
            .filter_map(|&(t, p)| seat_truth(&history, t, PASSENGER).map(|q| (p - q).len()))),
    }
}

#[test]
fn a_passenger_draws_its_hull_where_the_server_has_it() {
    // The passenger's drawn hull, mean and worst, in metres, by how the
    // driver steers and the link. Before it was predicted (the server's
    // newest hull, a round trip behind), these four were 0.48 and 1.6,
    // 1.2 and 4.6, 1.3 and 1.6, 3.1 and 4.1.
    let cases = [
        (Steer::Smooth, 20.0, 0.0, 0.0, 0.01, 0.05),
        (Steer::Smooth, 60.0, 10.0, 0.05, 0.01, 0.06),
        (Steer::Keys, 20.0, 0.0, 0.0, 0.02, 0.3),
        (Steer::Keys, 60.0, 10.0, 0.05, 0.25, 1.5),
    ];
    for (steer, delay, jitter, loss, mean, max) in cases {
        let m = ride(delay, jitter, loss, steer, false);
        let case = format!("{steer:?}, {delay} ms ±{jitter}, {:.0}% lost", loss * 100.0);
        println!("{case}: {m:#?}");
        for (who, e) in [
            ("driver", m.driver),
            ("passenger", m.passenger),
            ("watcher", m.watcher),
            ("watcher now", m.watcher_now),
        ] {
            assert!(e.n > 500, "{case}, {who}: only {} frames drawn", e.n);
        }
        // At the wheel the hull is the server's, exactly, but for loss.
        assert!(m.driver.max < 0.15, "{case}: driver {:?}", m.driver);
        // Behind it, off only while the driver's stick moves unseen.
        assert!(
            m.passenger.mean < mean && m.passenger.max < max,
            "{case}: passenger {:?}",
            m.passenger
        );
        // On foot, what interpolation draws is what was.
        assert!(m.watcher.max < 0.15, "{case}: watcher {:?}", m.watcher);
        // The stick was guessed wrong at times; our own fighter never.
        assert!(m.hull_corrections > 10, "{case}: {m:?}");
        assert_eq!(m.corrections, 0, "{case}: {m:?}");
    }
}

/// Firing from the bed while the driver swerves on keys, far from the
/// server: each round is drawn from the muzzle at once and taken over by
/// the server's under the same id, never held where the guessed hull left
/// it and popped a round trip back (its seed moves with each re-run).
#[test]
fn a_passengers_rounds_are_the_servers_through_a_swerve() {
    let m = ride(150.0, 10.0, 0.0, Steer::Keys, true);
    println!("{m:#?}");
    assert!(m.shots > 30, "{m:?}");
    assert_eq!(m.ids, m.shots as usize, "a round drawn under two ids: {m:?}");
    assert!(m.worst_jump < 0.5, "a round jumped {} m: {m:?}", m.worst_jump);
}

/// A hull nobody drives rolls to a stop (`Vehicle::step_parked`), and its
/// passenger, sent no stick, predicts it so: no hull corrections, and drawn
/// where the server has it.
#[test]
fn a_passenger_predicts_a_parked_hull_rolling_to_a_stop() {
    use proto::Snapshot;
    use sim::consts::MAX_FIGHTERS;
    use sim::fighter::Fighter;
    use sim::map::Map;
    use sim::world::{Phase, World};
    let rules = Rules {
        score_limit: 1000,
        time_limit_ms: 0,
        ..Rules::duel()
    };
    let mut w = World::new(Map::open(), rules, 1, (7, 8));
    w.seat(0, 0);
    w.seat(1, 1);
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &[0; MAX_FIGHTERS]);
    }
    let hull = Vehicle {
        vel: V3::new(15.0, 0.0, 0.0),
        seats: [0, 1],
        ..Vehicle::new(V3::new(-40.0, 0.0, 10.0), 0, 0, sim::vehicle::NO_TURRET)
    };
    w.vehicles[0] = hull;
    w.fighters[0].ride = Fighter::ride_of(0, PASSENGER);
    w.fighters[0].body = hull.seat_body(PASSENGER);
    w.fighters[1].body.pos = V3::new(0.0, 0.0, 45.0);
    // The server's snapshot after each tick `t`, which ran our seq `t + 2`.
    let snaps: Vec<Snapshot> = (0..400u32)
        .map(|t| {
            w.step(&idle, &[0; MAX_FIGHTERS]);
            Snapshot {
                tick: t,
                phase: 1,
                last_seq: t + 2,
                depth: 1,
                own: w.fighters[0],
                own_vehicle: Some(w.vehicles[0]),
                drive: w.drive_stick(0),
                ..Snapshot::default()
            }
        })
        .collect();
    let end = snaps[399].own_vehicle.unwrap();
    assert!(
        snaps.iter().all(|s| s.drive.is_none()),
        "a stick for a hull nobody drives"
    );
    assert!(
        end.pos.x - hull.pos.x > 3.0 && end.vel.len() < 0.5,
        "never rolled to a stop: {end:?}"
    );
    let hull_at = |t: f64| {
        let (a, b) = (snaps.get(t.floor() as usize)?, snaps.get(t.floor() as usize + 1)?);
        let (a, b) = (a.own_vehicle?.pos, b.own_vehicle?.pos);
        Some(a + (b - a) * (t - t.floor()) as f32)
    };
    let mut c = NetCore::new(0, 0, 0, rules, Map::open());
    // Every other snapshot, each 40 ms after its tick; a frame every 1 ms.
    let (mut t, mut now, mut worst, mut n) = (2u32, 0.0f64, 0.0f32, 0);
    while now < 2500.0 {
        now += 1.0;
        while (t as f64 * TICK_MS + 40.0) <= now && (t as usize) < snaps.len() {
            c.on_snapshot(now, &snaps[t as usize]);
            t += 2;
        }
        if c.latest().tick == 0 {
            continue;
        }
        c.update(now, Input::default());
        if c.latest().tick > 20 {
            if let (Some((_, p, _)), Some(truth)) = (c.own_vehicle(), hull_at(drawn_tick(&c))) {
                worst = worst.max((p - truth).len());
                n += 1;
            }
        }
    }
    assert!(n > 1000, "{n}");
    assert_eq!(c.stats.hull_corrections, 0, "{:?}", c.stats);
    assert!(worst < 1e-3, "drawn {worst} m off");
}

/// Riding together, each draws the other in its seat on the hull it draws:
/// never a round trip behind it (2 to 7 m at speed).
#[test]
fn a_rider_draws_its_co_rider_in_its_seat() {
    for (delay, jitter) in [(20.0, 0.0), (60.0, 10.0), (150.0, 10.0)] {
        let m = ride(delay, jitter, 0.0, Steer::Keys, false);
        let [of_passenger, of_driver] = m.co_seen;
        println!(
            "{delay} ms: the driver draws its passenger {of_passenger:?}, the passenger its driver {of_driver:?}, its own seat {:?}",
            m.own_seat
        );
        assert!(of_passenger.n > 500 && of_driver.n > 500, "{m:?}");
        // The driver's hull is the server's: so is its passenger's seat.
        assert!(of_passenger.max < 0.01, "{delay} ms: {of_passenger:?}");
        // The passenger's hull is a guess: its driver's seat on it is as
        // good as its own (at the cab, a turn's error weighs a little more).
        assert!(
            of_driver.mean < 2.0 * m.own_seat.mean + 0.01 && of_driver.max < 2.0 * m.own_seat.max + 0.01,
            "{delay} ms: {of_driver:?} on its own seat's {:?}",
            m.own_seat
        );
    }
}

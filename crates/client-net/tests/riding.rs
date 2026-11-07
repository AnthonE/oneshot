//! Riding a truck over a simulated network, three clients against the real
//! room on Dry Fork: one at the wheel, one in the bed behind it, one on
//! foot watching. How far each draws the truck from where the server has
//! it at the tick it draws it: the measure the netcode's vehicle work is
//! judged by (`-- --nocapture` prints it).

use client_net::core::NetCore;
use proto::{decode_input, decode_snapshot, encode_input, encode_snapshot, Look, Msg, Name, MAX_DATAGRAM};
use room::Room;
use sim::consts::TICK_HZ;
use sim::geom::V3;
use sim::input::{Input, USE};
use sim::rng::Pcg32;
use sim::vehicle::{Vehicle, DRIVER, PASSENGER};
use sim::world::Rules;

const TICK_MS: f64 = 1000.0 / TICK_HZ as f64;
/// Boarded by then; measured from then to the end.
const FROM_MS: f64 = 7000.0;
const END_MS: f64 = 20_000.0;

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

fn ride(delay_ms: f64, jitter_ms: f64, loss: f64, steer: Steer) -> Measured {
    let map = sim::map::by_id(3).expect("Dry Fork");
    let rules = Rules {
        score_limit: 1000,
        time_limit_ms: 0,
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
    let mut at_from = (0, 0);
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
            c.next_frame += 1000.0 / 60.0;
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
            let inp = match k {
                0 if (4000.0..4040.0).contains(&now) => press,
                1 if (4500.0..4540.0).contains(&now) => press,
                0 if now > 6000.0 => Input {
                    mz: 127,
                    mx,
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
                    at_from = (c.core.stats.hull_corrections, c.core.stats.corrections);
                }
                continue;
            }
            let core = &c.core;
            if k < 2 {
                if let Some((_, p, _)) = core.own_vehicle() {
                    seen[k].push((drawn_tick(core), p));
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
        let m = ride(delay, jitter, loss, steer);
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

//! The client's netcode against the real room over a simulated network:
//! latency, jitter and loss in both directions, in virtual time. No sockets,
//! so it runs anywhere and the same every time.

use client_net::core::NetCore;
use proto::{decode_input, decode_snapshot, encode_input, encode_snapshot, Msg, Name, MAX_DATAGRAM};
use room::{Room, Seat};
use sim::consts::{MAX_FIGHTERS, TICK_HZ};
use sim::input::{Input, FIRE};
use sim::map::Map;
use sim::rng::Pcg32;
use sim::world::Rules;

struct Net {
    rng: Pcg32,
    /// One-way latency, its jitter, and loss, each way.
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

struct Run {
    room: Room,
    client: NetCore,
    slot: u8,
    net: Net,
    now: f64,
    next_tick: f64,
    next_frame: f64,
}

fn start(delay_ms: f64, jitter_ms: f64, loss: f64, harmless: bool) -> Run {
    let rules = Rules {
        score_limit: 1000,
        time_limit_ms: 0,
        ..Rules::duel()
    };
    let mut room = Room::new(Map::proving_grounds(), rules, 11, (5, 6));
    let (slot, out) = room.join(Name::new("tester")).unwrap();
    let Some((_, Msg::Welcome { tick, team, rules, .. })) = out.first().copied() else {
        panic!("no welcome")
    };
    if harmless {
        // No bot: nothing but our own inputs may move our fighter.
        for s in 0..MAX_FIGHTERS {
            if matches!(room.seats[s], Seat::Bot(_)) {
                room.seats[s] = Seat::Empty;
                room.world.unseat(s);
            }
        }
    }
    let client = NetCore::new(slot, team, tick, rules, Map::proving_grounds());
    Run {
        room,
        client,
        slot,
        net: Net {
            rng: Pcg32::new(9, 9),
            delay_ms,
            jitter_ms,
            loss,
            up: Vec::new(),
            down: Vec::new(),
        },
        now: 0.0,
        next_tick: 0.0,
        next_frame: 0.0,
    }
}

impl Run {
    /// Runs `ms` of virtual time: the server on its tick, the page at 60
    /// frames a second, the network in between. `input(t)` is the player.
    fn run(&mut self, ms: f64, input: impl Fn(f64) -> Input) {
        let end = self.now + ms;
        let mut buf = [0u8; MAX_DATAGRAM];
        while self.now < end {
            for b in Net::due(&mut self.net.up, self.now) {
                let m = decode_input(&b).unwrap();
                self.room.on_input(self.slot, &m, self.now);
            }
            for b in Net::due(&mut self.net.down, self.now) {
                let s = decode_snapshot(&b).unwrap();
                self.client.on_snapshot(self.now, &s);
            }
            while self.next_tick <= self.now {
                let (snaps, _) = self.room.step(self.now);
                for (_, s) in snaps {
                    let n = encode_snapshot(&s, &mut buf).unwrap();
                    self.net.send(self.now, buf[..n].to_vec(), false);
                }
                self.next_tick += 1000.0 / TICK_HZ as f64;
            }
            if self.next_frame <= self.now {
                for m in self.client.update(self.now, input(self.now)) {
                    let n = encode_input(&m, &mut buf).unwrap();
                    self.net.send(self.now, buf[..n].to_vec(), true);
                }
                self.next_frame += 1000.0 / 60.0;
            }
            self.now += 0.5;
        }
    }
}

/// Runs in circles, jumps now and then, fires bursts.
fn player(t: f64) -> Input {
    let k = (t / 1000.0) as u32;
    Input {
        buttons: if (t as u32 / 700).is_multiple_of(3) { FIRE } else { 0 }
            | if k % 4 == 3 { sim::input::JUMP } else { 0 },
        yaw: (t * 20.0) as u32 as u16,
        pitch: 0,
        mx: 0,
        mz: 127,
    }
}

#[test]
fn a_clean_link_predicts_without_a_single_correction() {
    let mut r = start(20.0, 0.0, 0.0, true);
    r.run(4000.0, player);
    let warm = r.client.stats;
    let warm_repeats = r.room.client(r.slot).unwrap().repeats;
    r.run(10_000.0, player);
    let s = r.client.stats;
    assert!(s.snapshots - warm.snapshots > 600, "{s:?}");
    assert_eq!(s.corrections - warm.corrections, 0, "{s:?}");
    assert!(
        (1..=3).contains(&s.depth),
        "the clock settles near the target depth: {s:?}"
    );
    let c = r.room.client(r.slot).unwrap();
    assert_eq!(c.repeats, warm_repeats, "once settled, every input arrives in time");
}

#[test]
fn a_bad_link_still_mostly_confirms_and_converges() {
    let mut r = start(40.0, 25.0, 0.05, true);
    r.run(15_000.0, player);
    let s = r.client.stats;
    let ratio = s.confirmations as f64 / (s.confirmations + s.corrections) as f64;
    assert!(ratio > 0.9, "{ratio:.3}: {s:?}");
    // Let go of the controls, and the prediction lands on the server's
    // fighter exactly.
    r.net.loss = 0.0;
    r.net.jitter_ms = 0.0;
    r.run(3000.0, |_| Input::default());
    let (own, _) = r.client.own();
    let server = &r.room.world.fighters[r.slot as usize];
    assert!(client_net::core::same_local(own, server));
}

#[test]
fn others_are_drawn_between_where_the_server_had_them() {
    let mut r = start(30.0, 5.0, 0.0, false);
    let bot = 1 - r.slot as usize;
    let mut history = Vec::new();
    for _ in 0..400 {
        r.run(10.0, |_| Input::default());
        history.push((r.room.world.tick, r.room.world.fighters[bot].body.pos));
        let rt = r.client.render_tick(r.now);
        let Some(d) = r.client.others(r.now).into_iter().find(|d| d.slot as usize == bot) else {
            continue;
        };
        // The server's positions on the two ticks around the render tick.
        let lo = history.iter().rev().find(|(t, _)| (*t as f64) <= rt);
        let hi = history.iter().find(|(t, _)| (*t as f64) > rt);
        if let (Some(lo), Some(hi)) = (lo, hi) {
            let span = (hi.1 - lo.1).len() + 0.1;
            assert!(
                (d.pos - lo.1).len() <= span && (d.pos - hi.1).len() <= span,
                "{d:?} vs {lo:?} {hi:?}"
            );
        }
    }
}

#[test]
fn a_stalled_page_catches_up_within_a_second() {
    let mut r = start(20.0, 0.0, 0.0, true);
    r.run(5000.0, player);
    // Two seconds with no frames drawn: the page did nothing at all.
    let saved = r.next_frame;
    r.next_frame = r.now + 2000.0;
    r.run(2000.0, player);
    assert!(r.next_frame >= saved);
    r.run(1000.0, player);
    let settled = r.client.stats;
    r.run(4000.0, player);
    let s = r.client.stats;
    assert_eq!(
        s.corrections, settled.corrections,
        "corrections went on after the stall: {s:?}"
    );
}

//! Several clients against the real room over simulated links, in virtual
//! time: each its own latency, jitter and loss each way, routed by seat.

#![allow(dead_code)]

use client_net::core::NetCore;
use proto::{decode_input, decode_snapshot, encode_input, encode_snapshot, Look, Msg, Name, MAX_DATAGRAM};
use room::Room;
use sim::consts::TICK_HZ;
use sim::geom::V3;
use sim::input::Input;
use sim::rng::Pcg32;
use sim::vehicle::Vehicle;
use sim::world::Rules;

pub const TICK_MS: f64 = 1000.0 / TICK_HZ as f64;
pub const FRAME_MS: f64 = 1000.0 / 60.0;

/// One client's link: latency, jitter and loss, each way.
pub struct Net {
    rng: Pcg32,
    pub delay_ms: f64,
    pub jitter_ms: f64,
    pub loss: f64,
    up: Vec<(f64, Vec<u8>)>,
    down: Vec<(f64, Vec<u8>)>,
    pub bytes_down: u64,
}

impl Net {
    pub fn new(seed: u64, delay_ms: f64, jitter_ms: f64, loss: f64) -> Net {
        Net {
            rng: Pcg32::new(9, seed),
            delay_ms,
            jitter_ms,
            loss,
            up: Vec::new(),
            down: Vec::new(),
            bytes_down: 0,
        }
    }

    fn unit(&mut self) -> f64 {
        self.rng.next_u32() as f64 / 4_294_967_296.0
    }

    fn send(&mut self, now: f64, bytes: Vec<u8>, up: bool) {
        if !up {
            self.bytes_down += bytes.len() as u64;
        }
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

pub struct Client {
    pub core: NetCore,
    pub slot: u8,
    pub net: Net,
    next_frame: f64,
}

/// A room on Dry Fork and clients in it, by team, every bot standing still.
pub struct Lan {
    pub room: Room,
    pub cs: Vec<Client>,
    pub now: f64,
    next_tick: f64,
    /// Truck `i` at every server tick so far: (tick, the hull).
    pub hulls: Vec<(u32, [Vehicle; sim::consts::MAX_VEHICLES])>,
}

impl Lan {
    /// Clients on `teams` (red 0, blue 1), each on a link of `delay_ms`
    /// ± `jitter_ms` with `loss`; `bubbles` as `Room::bubbles`.
    pub fn dry_fork(teams: &[u8], delay_ms: f64, jitter_ms: f64, loss: f64, bubbles: bool) -> Lan {
        Lan::dry_fork_live(teams, delay_ms, jitter_ms, loss, bubbles, false)
    }

    /// [`Lan::dry_fork`], `live` as `Room::bubble_live`.
    pub fn dry_fork_live(teams: &[u8], delay_ms: f64, jitter_ms: f64, loss: f64, bubbles: bool, live: bool) -> Lan {
        let map = sim::map::by_id(3).expect("Dry Fork");
        let rules = Rules {
            score_limit: 1000,
            time_limit_ms: 0,
            ..Rules::big_team()
        };
        let mut room = Room::new(map.clone(), rules, 11, (5, 6));
        room.passive_bots = true;
        room.bubbles = bubbles;
        room.bubble_live = live;
        let mut cs = Vec::new();
        for (k, &t) in teams.iter().enumerate() {
            let (slot, out) = room.join_team(Name::new("t"), Look::default(), Some(t)).unwrap();
            let Some((_, Msg::Welcome { tick, team, rules, .. })) = out.first().copied() else {
                panic!("no welcome");
            };
            cs.push(Client {
                core: NetCore::new(slot, team, tick, rules, map.clone()),
                slot,
                net: Net::new(9 + k as u64, delay_ms, jitter_ms, loss),
                next_frame: k as f64 * 5.0,
            });
        }
        Lan {
            room,
            cs,
            now: 0.0,
            next_tick: 0.0,
            hulls: Vec::new(),
        }
    }

    /// Client `k`'s fighter on the ground at `x`, `z`.
    pub fn place(&mut self, k: usize, x: f32, z: f32) {
        let y = self.room.world.map.ground(x, z);
        let s = self.cs[k].slot as usize;
        self.room.world.fighters[s].body.pos = V3::new(x, y, z);
    }

    /// On to `until_ms`: each client's controls from `input(k, now)` once a
    /// page frame, and `frame(k, core, now)` after each.
    pub fn run(
        &mut self,
        until_ms: f64,
        mut input: impl FnMut(usize, f64) -> Input,
        mut frame: impl FnMut(usize, &mut NetCore, f64, &Room),
    ) {
        let mut buf = [0u8; MAX_DATAGRAM];
        while self.now < until_ms {
            let now = self.now;
            for c in self.cs.iter_mut() {
                for b in Net::due(&mut c.net.up, now) {
                    self.room.on_input(c.slot, &decode_input(&b).unwrap(), now);
                }
                for b in Net::due(&mut c.net.down, now) {
                    c.core.on_snapshot(now, &decode_snapshot(&b).unwrap());
                }
            }
            while self.next_tick <= now {
                let (snaps, _) = self.room.step(now);
                for (seat, s) in snaps {
                    let n = encode_snapshot(&s, &mut buf).unwrap();
                    if let Some(c) = self.cs.iter_mut().find(|c| c.slot == seat) {
                        c.net.send(now, buf[..n].to_vec(), false);
                    }
                }
                self.hulls.push((self.room.world.tick, *self.room.world.vehicles));
                self.next_tick += TICK_MS;
            }
            for (k, c) in self.cs.iter_mut().enumerate() {
                if c.next_frame > now {
                    continue;
                }
                c.next_frame += FRAME_MS;
                for m in c.core.update(now, input(k, now)) {
                    let n = encode_input(&m, &mut buf).unwrap();
                    c.net.send(now, buf[..n].to_vec(), true);
                }
                frame(k, &mut c.core, now, &self.room);
            }
            self.now += 0.5;
        }
    }

    /// Truck `i` where the server had it at fractional tick `t`.
    pub fn hull_at(&self, i: usize, t: f64) -> Option<V3> {
        let lo = t.floor() as u32;
        let first = self.hulls.first()?.0;
        let a = self.hulls.get(lo.checked_sub(first)? as usize)?.1[i].pos;
        let b = self.hulls.get((lo + 1 - first) as usize)?.1[i].pos;
        Some(a + (b - a) * (t - lo as f64) as f32)
    }
}

/// Mean and worst of a run's errors, in metres.
#[derive(Clone, Copy, Debug, Default)]
pub struct Err {
    pub n: usize,
    pub mean: f32,
    pub max: f32,
}

pub fn err(e: impl Iterator<Item = f32>) -> Err {
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

/// The tick a client's drawn body is on: `own()` draws between the
/// predictions after seqs `tick() - 2` and `tick() - 1`, and seq `q` runs
/// on the server's tick `q + latest.tick - latest.last_seq`.
pub fn drawn_tick(c: &NetCore) -> f64 {
    let l = c.latest();
    let k = l.tick.wrapping_sub(l.last_seq) as i32 as f64;
    c.tick() as f64 - 2.0 + c.alpha() as f64 + k
}

//! A client's bubble: the hulls near it that it is sent exactly, with the
//! stick each is driven by, so it predicts them on its own timeline (its
//! contacts with them, and their drawing). Vehicle-centric: a hull comes
//! in for being near the viewer or near a hull already in, never a fighter
//! on foot for being near another on foot (a duel stays drawn in the past
//! and lag-compensated). Only what fog shows; never the viewer's own hull
//! (it is `own_vehicle`), never while the viewer is dead.
//!
//! Each member has a share of the viewer's rewind (`sim::world::Seen`):
//! 15 drawn on the render tick, as everything outside the bubble is, 0 on
//! the viewer's own present. A hull coming in slides from one to the other
//! over a blend sized by how soon it could reach the viewer; one leaving
//! slides back over [`LEAVE_MS`]. The client draws each at its share
//! between its past and its present, and the server rewinds shots at it
//! by the same share of the shooter's depth, from the snapshot the shot's
//! frame was made after ([`Bubble::seen`]): what is drawn is what is met.

use sim::consts::{BODY_R, MAX_FIGHTERS, MAX_VEHICLES, SHARE_FULL, TICK_HZ, VEH_H, VEH_HALF_L};
use sim::geom::V3;
use sim::world::{Seen, World};

/// A hull comes in within this far of the viewer (flat), and that much
/// farther for each second of closing speed ([`LOOK_S`]).
pub const BUBBLE_M: f32 = 16.0;
pub const LOOK_S: f32 = 1.2;
/// It stays in until this much past where it came in.
pub const HYST_M: f32 = 8.0;
/// And for at least this long, once in: a hull at the edge never flaps.
pub const DWELL_MS: u32 = 1500;
/// A hull that moves or is driven comes in only for a client whose round
/// trip is under this: past it, a re-run would be too long a guess.
pub const RTT_MAX_MS: f64 = 250.0;
/// A hull in brings in another within this of it (flat), and that much
/// farther for each second of closing speed between them: whatever it
/// might crash into is predicted with it.
pub const CONTACT_M: f32 = 2.0 * VEH_HALF_L + 1.0;
/// Under this flat speed a hull nobody drives is still.
pub const STILL_MS: f32 = 0.5;
/// A hull coming in slides onto the present over the time it could take
/// to come within [`ON_PRESENT_M`] of the viewer, less a round trip, within
/// these: close by, where contact and shots are, it is on the present.
pub const ON_PRESENT_M: f32 = 12.0;
pub const BLEND_MIN_MS: f32 = 250.0;
pub const BLEND_MAX_MS: f32 = 1500.0;
/// A hull leaving slides back onto the render tick over this.
pub const LEAVE_MS: f32 = 1000.0;
/// Snapshots whose shares are kept, for the shot whose frame came after one.
pub const ROWS: usize = 64;
/// Someone on foot comes in within reach of a hull in that moves or is
/// driven (or of ours): its half length, a body's radius, this much more,
/// and as far as [`FOOT_LEAD_S`] of their closing speed; once in, this
/// much more to stay. Only so many, the nearest: a crowd stays in the past.
pub const FOOT_PAD_M: f32 = 1.0;
pub const FOOT_LEAD_S: f32 = 0.6;
pub const FOOT_HYST_M: f32 = 2.0;
pub const MAX_NEAR_FOOT: usize = 6;

const FULL_Q8: u16 = (SHARE_FULL as u16) << 8;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Member {
    on: bool,
    /// Ticks since it came in.
    since: u32,
    /// Its share of the viewer's rewind, in 1/256 of a [`SHARE_FULL`]th.
    share_q8: u16,
    /// How fast it slides onto the present, a snapshot: set as it comes in
    /// (by how soon it could reach the viewer then), so the slide is even,
    /// quickening only for one that has since sped up toward the viewer;
    /// 0 sliding back.
    step_q8: u16,
}

#[derive(Clone, Debug)]
pub struct Bubble {
    hull: [Member; MAX_VEHICLES],
    /// Those on foot in, by seat.
    foot: [Member; MAX_FIGHTERS],
    /// The last update's: who was sent at all, and the hull each rode.
    sent_last: u16,
    rode_last: [Option<u8>; MAX_FIGHTERS],
    /// The last update's: which hulls were sent at all, and the one ridden.
    shown_last: u8,
    mine_last: Option<u8>,
    /// Off: every share sent and kept is whole (`Room::bubble_live`).
    live: bool,
    /// How many on foot may be in at once (`Room::bubble_foot`).
    pub foot_max: usize,
    snap_every: u32,
    /// What each snapshot was sent, by its tick.
    rows: [(u32, Seen); ROWS],
}

impl Default for Bubble {
    fn default() -> Bubble {
        Bubble {
            hull: [Member::default(); MAX_VEHICLES],
            foot: [Member::default(); MAX_FIGHTERS],
            sent_last: 0,
            rode_last: [None; MAX_FIGHTERS],
            shown_last: 0,
            mine_last: None,
            live: true,
            foot_max: MAX_NEAR_FOOT,
            snap_every: 2,
            rows: [(u32::MAX, Seen::PAST); ROWS],
        }
    }
}

impl Bubble {
    /// Hull `i` is in.
    pub fn has(&self, i: usize) -> bool {
        self.hull.get(i).is_some_and(|m| m.on)
    }

    /// Which hulls are in, a bit a slot.
    pub fn hulls(&self) -> u8 {
        self.hull
            .iter()
            .enumerate()
            .fold(0, |m, (i, h)| m | ((h.on as u8) << i))
    }

    /// Hull `i`'s share as it goes on the wire (rounded up: never drawn
    /// nearer the present than the server will meet it).
    pub fn share(&self, i: usize) -> u8 {
        self.wire(self.hull.get(i))
    }

    /// Seat `q`, on foot, is in.
    pub fn has_foot(&self, q: usize) -> bool {
        self.foot.get(q).is_some_and(|m| m.on)
    }

    /// Seat `q`'s share on foot, as it goes on the wire.
    pub fn foot_share(&self, q: usize) -> u8 {
        self.wire(self.foot.get(q))
    }

    fn wire(&self, m: Option<&Member>) -> u8 {
        match m {
            Some(m) if m.on && self.live => m.share_q8.min(FULL_Q8).div_ceil(256) as u8,
            _ => SHARE_FULL,
        }
    }

    /// Who is in for seat `s` now, and at what share: `known(q)` whether
    /// it may know fighter `q` (a hull's seat is named to it), `sent(q)`
    /// whether this snapshot sends fighter `q`, `shown(i)` whether it is
    /// sent hull `i` at all; `rtt_ms` its round trip; `snap_every` ticks
    /// since the last update; `live` off, every share whole.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        w: &World,
        s: usize,
        known: &impl Fn(usize) -> bool,
        sent: &impl Fn(usize) -> bool,
        shown: &impl Fn(usize) -> bool,
        rtt_ms: f64,
        snap_every: u32,
        live: bool,
    ) {
        let me = &w.fighters[s];
        let mine = me.riding().map(|(i, _)| i);
        let (shown_last, mine_last) = (self.shown_last, self.mine_last);
        let (sent_last, rode_last) = (self.sent_last, self.rode_last);
        self.sent_last = (0..MAX_FIGHTERS).fold(0, |m, q| m | ((sent(q) as u16) << q));
        for (r, f) in self.rode_last.iter_mut().zip(&w.fighters) {
            *r = f.riding().map(|(i, _)| i as u8);
        }
        self.live = live;
        self.snap_every = snap_every.max(1);
        self.shown_last = (0..MAX_VEHICLES).fold(0, |m, i| m | ((shown(i) as u8) << i));
        self.mine_last = mine.map(|i| i as u8);
        if !me.active() || w.map.vehicles.is_empty() {
            self.hull = [Member::default(); MAX_VEHICLES];
            self.foot = [Member::default(); MAX_FIGHTERS];
            return;
        }
        let (at, at_v) = match mine {
            Some(i) => (w.vehicles[i].pos, w.vehicles[i].vel),
            None => (me.body.pos, me.body.vel),
        };
        let dwell = DWELL_MS * TICK_HZ / 1000;
        // Only by what the viewer is sent of it: a driver it may not know
        // is as nobody at the wheel (and its stick goes as none), so who is
        // in never tells a hidden driver from an empty seat.
        let eligible = |i: usize| {
            let v = &w.vehicles[i];
            let still = v.vel.flat().len() < STILL_MS && v.driver().filter(|&d| known(d)).is_none();
            v.active
                && mine != Some(i)
                && shown(i)
                && proto::exact_fits(v.pos, v.vel)
                && (still || rtt_ms <= RTT_MAX_MS)
        };
        // How far, how far it may come in, and how fast it closes.
        let reach = |a: V3, av: V3, b: V3, bv: V3, base: f32| {
            let rel = (bv - av).flat().len();
            ((b - a).flat().len(), base + rel * LOOK_S, rel)
        };
        // Snapshots in `ms`, at least one.
        let snaps = |ms: f32| ((ms * TICK_HZ as f32 / 1000.0) as u32 / self.snap_every).max(1) as u16;
        // In: falling toward the present, over the time it could take to
        // reach `d` metres closing at `rel`, less what a round trip shows.
        let come = |was: Member, i: usize, d: f32, rel: f32| {
            let start = if (shown_last & (1 << i)) == 0 || mine_last == Some(i as u8) {
                // Never drawn in the past (unseen, or ridden): on the present.
                0
            } else {
                FULL_Q8
            };
            let blend = (((d - ON_PRESENT_M).max(0.0) / rel.max(1.0)) * 1000.0 - (rtt_ms as f32 + 30.0))
                .clamp(BLEND_MIN_MS, BLEND_MAX_MS);
            // Even, as set coming in; quicker only if it now could reach the
            // viewer sooner than that would finish (it sped up toward it).
            let step = if was.on && was.step_q8 > 0 {
                was.step_q8.max(was.share_q8 / snaps(blend))
            } else {
                FULL_Q8 / snaps(blend)
            };
            Member {
                on: true,
                since: if was.on {
                    was.since.saturating_add(snap_every)
                } else {
                    0
                },
                share_q8: if was.on {
                    was.share_q8.saturating_sub(step)
                } else {
                    start
                },
                step_q8: step,
            }
        };
        let mut next = [Member::default(); MAX_VEHICLES];
        // Those out of reach of the viewer, sliding back: the closure may
        // hold them yet (then they slide on, not back).
        let mut leaving = [false; MAX_VEHICLES];
        for (i, m) in next.iter_mut().enumerate() {
            if !eligible(i) {
                continue;
            }
            let was = self.hull[i];
            let v = &w.vehicles[i];
            let (d, r, rel) = reach(at, at_v, v.pos, v.vel, BUBBLE_M);
            let stays = was.on && (d < r + HYST_M || was.since < dwell);
            if d < r || stays {
                *m = come(was, i, d, rel);
            } else if was.on {
                // Leaving: back onto the render tick, sent there once, then out.
                let share = was.share_q8.saturating_add(FULL_Q8 / snaps(LEAVE_MS)).min(FULL_Q8);
                if was.share_q8 < FULL_Q8 {
                    *m = Member {
                        on: true,
                        since: was.since.saturating_add(snap_every),
                        share_q8: share,
                        step_q8: 0,
                    };
                }
                leaving[i] = true;
            }
        }
        // Whatever a hull in might meet, in with it.
        for _ in 0..MAX_VEHICLES {
            let mut grew = false;
            for j in 0..MAX_VEHICLES {
                if (next[j].on && !leaving[j]) || !eligible(j) {
                    continue;
                }
                let vj = &w.vehicles[j];
                let holds = |i: usize| i != j && ((next[i].on && !leaving[i]) || mine == Some(i));
                let near = (0..MAX_VEHICLES).filter(|&i| holds(i)).any(|i| {
                    let vi = &w.vehicles[i];
                    let (d, r, _) = reach(vi.pos, vi.vel, vj.pos, vj.vel, CONTACT_M);
                    d < r
                });
                if near {
                    let (d, _, rel) = reach(at, at_v, vj.pos, vj.vel, BUBBLE_M);
                    next[j] = come(self.hull[j], j, d, rel);
                    leaving[j] = false;
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        self.hull = next;
        // Who is on foot within reach of a hull that moves or is driven: in
        // with it, the nearest few. Getting out of one in our bubble (or of
        // ours), at its share; unsent till now, on the present.
        let anchors: [Option<(V3, V3, u16)>; MAX_VEHICLES] = core::array::from_fn(|i| {
            let v = &w.vehicles[i];
            let moving = v.vel.flat().len() >= STILL_MS || v.driver().is_some();
            let share = if mine == Some(i) {
                Some(0)
            } else {
                next[i].on.then_some(next[i].share_q8)
            };
            (v.active && moving)
                .then_some(())
                .and(share)
                .map(|sh| (v.pos, v.vel, sh))
        });
        let mut near: [(f32, usize); MAX_FIGHTERS] = [(f32::MAX, 0); MAX_FIGHTERS];
        let mut n = 0;
        for (q, f) in w.fighters.iter().enumerate() {
            if q == s || !f.active() || f.ride != 0 || !sent(q) || !proto::exact_fits(f.body.pos, f.body.vel) {
                continue;
            }
            let was = self.foot[q];
            let best = anchors
                .iter()
                .flatten()
                .filter(|(p, ..)| sim::fmath::fabs(f.body.pos.y - p.y) < VEH_H + 2.0)
                .map(|&(p, pv, _)| {
                    let d = (f.body.pos - p).flat().len();
                    let reach = VEH_HALF_L + BODY_R + FOOT_PAD_M + (f.body.vel - pv).flat().len() * FOOT_LEAD_S;
                    d - reach - if was.on { FOOT_HYST_M } else { 0.0 }
                })
                .fold(f32::MAX, f32::min);
            if best <= 0.0 || (was.on && was.since < dwell) {
                near[n] = (best, q);
                n += 1;
            }
        }
        near[..n].sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut foot = [Member::default(); MAX_FIGHTERS];
        for &(_, q) in near[..n].iter().take(self.foot_max) {
            let was = self.foot[q];
            let start = match rode_last[q] {
                Some(i) if mine_last == Some(i) => 0,
                Some(i) if self.hull[i as usize].on => self.hull[i as usize].share_q8,
                _ if sent_last & (1 << q) == 0 => 0,
                _ => FULL_Q8,
            };
            foot[q] = Member {
                on: true,
                since: if was.on {
                    was.since.saturating_add(snap_every)
                } else {
                    0
                },
                share_q8: if was.on {
                    was.share_q8.saturating_sub(FULL_Q8 / snaps(BLEND_MIN_MS))
                } else {
                    start
                },
                step_q8: FULL_Q8 / snaps(BLEND_MIN_MS),
            };
        }
        // Out of reach: back onto the render tick, sent there once, then out.
        for (q, (m, (was, f))) in foot.iter_mut().zip(self.foot.iter().zip(&w.fighters)).enumerate() {
            let can = f.active() && f.ride == 0 && sent(q) && proto::exact_fits(f.body.pos, f.body.vel);
            if m.on || !was.on || !can {
                continue;
            }
            let share = was.share_q8.saturating_add(FULL_Q8 / snaps(LEAVE_MS)).min(FULL_Q8);
            if was.share_q8 < FULL_Q8 {
                *m = Member {
                    on: true,
                    since: was.since.saturating_add(snap_every),
                    share_q8: share,
                    step_q8: 0,
                };
            }
        }
        self.foot = foot;
    }

    /// The snapshot of `tick` went out: keep what it said of each target
    /// (each member hull's share, and its riders' with it; the rest whole).
    pub fn sent(&mut self, w: &World, tick: u32) {
        let mut row = Seen::PAST;
        for q in 0..MAX_FIGHTERS {
            if self.has_foot(q) {
                row.share[q] = self.foot_share(q);
            }
        }
        for i in 0..MAX_VEHICLES {
            if !self.has(i) {
                continue;
            }
            let share = self.share(i);
            row.share[MAX_FIGHTERS + i] = share;
            for &x in &w.vehicles[i].seats {
                if x != 0 && (x as usize) <= MAX_FIGHTERS {
                    row.share[x as usize - 1] = share;
                }
            }
        }
        self.rows[(tick / self.snap_every) as usize % ROWS] = (tick, row);
    }

    /// What a client saw of each target after the snapshot of `acked`: the
    /// shares it was sent then, or all of every rewind if it is too old.
    pub fn seen(&self, acked: Option<u32>) -> Seen {
        let Some(a) = acked else {
            return Seen::PAST;
        };
        match self.rows[(a / self.snap_every) as usize % ROWS] {
            (t, row) if t == a => row,
            _ => Seen::PAST,
        }
    }
}

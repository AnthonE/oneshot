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

use sim::consts::{MAX_FIGHTERS, MAX_VEHICLES, SHARE_FULL, TICK_HZ, VEH_HALF_L};
use sim::geom::V3;
use sim::world::{Seen, World};

/// A hull comes in within this far of the viewer (flat), and that much
/// farther for each second of closing speed ([`LOOK_S`]).
pub const BUBBLE_M: f32 = 16.0;
pub const LOOK_S: f32 = 0.6;
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
/// to reach the viewer, less a round trip, within these.
pub const BLEND_MIN_MS: f32 = 250.0;
pub const BLEND_MAX_MS: f32 = 1000.0;
/// A hull leaving slides back onto the render tick over this.
pub const LEAVE_MS: f32 = 1000.0;
/// Snapshots whose shares are kept, for the shot whose frame came after one.
pub const ROWS: usize = 64;

const FULL_Q8: u16 = (SHARE_FULL as u16) << 8;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Member {
    on: bool,
    /// Ticks since it came in.
    since: u32,
    /// Its share of the viewer's rewind, in 1/256 of a fifteenth.
    share_q8: u16,
}

#[derive(Clone, Debug)]
pub struct Bubble {
    hull: [Member; MAX_VEHICLES],
    /// The last update's: which hulls were sent at all, and the one ridden.
    shown_last: u8,
    mine_last: Option<u8>,
    /// Off: every share sent and kept is whole (`Room::bubble_live`).
    live: bool,
    snap_every: u32,
    /// What each snapshot was sent, by its tick.
    rows: [(u32, Seen); ROWS],
}

impl Default for Bubble {
    fn default() -> Bubble {
        Bubble {
            hull: [Member::default(); MAX_VEHICLES],
            shown_last: 0,
            mine_last: None,
            live: true,
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
        match self.hull.get(i) {
            Some(m) if m.on && self.live => (m.share_q8.div_ceil(256) as u8).min(SHARE_FULL),
            _ => SHARE_FULL,
        }
    }

    /// Who is in for seat `s` now, and at what share: `known(q)` whether
    /// it may know fighter `q`, `shown(i)` whether it is sent hull `i` at
    /// all; `rtt_ms` its round trip; `snap_every` ticks since the last
    /// update; `live` off, every share whole.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        w: &World,
        s: usize,
        known: &impl Fn(usize) -> bool,
        shown: &impl Fn(usize) -> bool,
        rtt_ms: f64,
        snap_every: u32,
        live: bool,
    ) {
        let me = &w.fighters[s];
        let mine = me.riding().map(|(i, _)| i);
        let (shown_last, mine_last) = (self.shown_last, self.mine_last);
        self.live = live;
        self.snap_every = snap_every.max(1);
        self.shown_last = (0..MAX_VEHICLES).fold(0, |m, i| m | ((shown(i) as u8) << i));
        self.mine_last = mine.map(|i| i as u8);
        if !me.active() || w.map.vehicles.is_empty() {
            self.hull = [Member::default(); MAX_VEHICLES];
            return;
        }
        let (at, at_v) = match mine {
            Some(i) => (w.vehicles[i].pos, w.vehicles[i].vel),
            None => (me.body.pos, me.body.vel),
        };
        let dwell = DWELL_MS * TICK_HZ / 1000;
        let eligible = |i: usize| {
            let v = &w.vehicles[i];
            let still = v.vel.flat().len() < STILL_MS && v.driver().is_none();
            v.active
                && mine != Some(i)
                && shown(i)
                && v.driver().is_none_or(known)
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
            let blend = (((d - CONTACT_M).max(0.0) / rel.max(1.0)) * 1000.0 - (rtt_ms as f32 + 30.0))
                .clamp(BLEND_MIN_MS, BLEND_MAX_MS);
            Member {
                on: true,
                since: if was.on {
                    was.since.saturating_add(snap_every)
                } else {
                    0
                },
                share_q8: if was.on {
                    was.share_q8.saturating_sub(FULL_Q8 / snaps(blend))
                } else {
                    start
                },
            }
        };
        let mut next = [Member::default(); MAX_VEHICLES];
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
                // Leaving: back onto the render tick, then out.
                let share = was.share_q8.saturating_add(FULL_Q8 / snaps(LEAVE_MS)).min(FULL_Q8);
                if share < FULL_Q8 {
                    *m = Member {
                        on: true,
                        since: was.since.saturating_add(snap_every),
                        share_q8: share,
                    };
                }
            }
        }
        // Whatever a hull in might meet, in with it.
        for _ in 0..MAX_VEHICLES {
            let mut grew = false;
            for j in 0..MAX_VEHICLES {
                if next[j].on || !eligible(j) {
                    continue;
                }
                let vj = &w.vehicles[j];
                let near = (0..MAX_VEHICLES).filter(|&i| next[i].on || mine == Some(i)).any(|i| {
                    let vi = &w.vehicles[i];
                    let (d, r, _) = reach(vi.pos, vi.vel, vj.pos, vj.vel, CONTACT_M);
                    d < r
                });
                if near {
                    let (d, _, rel) = reach(at, at_v, vj.pos, vj.vel, BUBBLE_M);
                    next[j] = come(self.hull[j], j, d, rel);
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        self.hull = next;
    }

    /// The snapshot of `tick` went out: keep what it said of each target
    /// (each member hull's share, and its riders' with it; the rest whole).
    pub fn sent(&mut self, w: &World, tick: u32) {
        let mut row = Seen::PAST;
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

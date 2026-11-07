//! A client's bubble: the hulls near it that it is sent exactly, with the
//! stick each is driven by, so it predicts them on its own timeline (its
//! contacts with them, and in time their drawing). Vehicle-centric: a hull
//! comes in for being near the viewer or near a hull already in, never a
//! fighter on foot for being near another on foot (a duel stays drawn in
//! the past and lag-compensated). Only what fog shows; never the viewer's
//! own hull (it is `own_vehicle`), never while the viewer is dead.

use sim::consts::{MAX_VEHICLES, TICK_HZ, VEH_HALF_L};
use sim::geom::V3;
use sim::world::World;

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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Member {
    on: bool,
    /// Ticks since it came in.
    since: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Bubble {
    hull: [Member; MAX_VEHICLES],
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

    /// Who is in for seat `s` now: `known(q)` whether it may know fighter
    /// `q`, `shown(i)` whether it is sent hull `i` at all; `rtt_ms` its
    /// round trip; `ticks` since the last update.
    pub fn update(
        &mut self,
        w: &World,
        s: usize,
        known: &impl Fn(usize) -> bool,
        shown: &impl Fn(usize) -> bool,
        rtt_ms: f64,
        ticks: u32,
    ) {
        let me = &w.fighters[s];
        let mine = me.riding().map(|(i, _)| i);
        if !me.active() || w.map.vehicles.is_empty() {
            *self = Bubble::default();
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
        let reach = |a: V3, av: V3, b: V3, bv: V3, base: f32| {
            let d = (b - a).flat().len();
            (d, base + (bv - av).flat().len() * LOOK_S)
        };
        let mut next = [Member::default(); MAX_VEHICLES];
        for (i, m) in next.iter_mut().enumerate() {
            if !eligible(i) {
                continue;
            }
            let was = self.hull[i];
            let v = &w.vehicles[i];
            let (d, r) = reach(at, at_v, v.pos, v.vel, BUBBLE_M);
            let stays = was.on && (d < r + HYST_M || was.since < dwell);
            if d < r || stays {
                *m = Member {
                    on: true,
                    since: if was.on { was.since.saturating_add(ticks) } else { 0 },
                };
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
                    let (d, r) = reach(vi.pos, vi.vel, vj.pos, vj.vel, CONTACT_M);
                    d < r
                });
                if near {
                    let was = self.hull[j];
                    next[j] = Member {
                        on: true,
                        since: if was.on { was.since.saturating_add(ticks) } else { 0 },
                    };
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        self.hull = next;
    }
}

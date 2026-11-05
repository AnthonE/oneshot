//! The aim review's numbers (docs/DESIGN.md §12): what each fighter's aim
//! did over a match, read from the world every tick on the server. Nothing
//! here goes on the wire, changes the match or bans anyone. A match whose
//! numbers cross a line is queued for a person to watch its replay; the
//! person decides.
//!
//! - **Head-shot rate:** hitscan hits on the head, shields or not, over all
//!   hitscan hits.
//! - **Reaction:** from an enemy coming into plain sight (its chest in clear
//!   line from the eye, after [`SIGHT_GAP_MS`] out of it) to the first hit on
//!   it in that sighting. The server's clock, so a player's latency and
//!   playout are in it: a person's is rarely under 200 ms.
//! - **Snap-hits:** a sighting's first hit after more than [`SNAP_DEG`] of
//!   turn in the [`SNAP_MS`] before it: a flick that lands at once. A share
//!   of sightings, not a count, so a long match or a fast gun doesn't raise
//!   it.
//! - **Pre-aim:** time spent aimed within [`PREAIM_DEG`] of an enemy that fog
//!   sent this player but that has been out of plain sight for over
//!   [`PREAIM_HIDDEN_MS`]: following someone through a wall with what the
//!   client was told early. Not while also aimed at an enemy in plain
//!   sight: then the wall behind is a coincidence.

use sim::consts::{ticks, MAX_FIGHTERS, TICK_HZ};
use sim::events::Kind as Ev;
use sim::geom::{aim_dir, V3};
use sim::world::World;

use crate::fog::Plain;

/// A sighting ends after this long out of plain sight.
pub const SIGHT_GAP_MS: u32 = 250;
/// A snap: more turn than this ...
pub const SNAP_DEG: f32 = 30.0;
/// ... in this long before a hit.
pub const SNAP_MS: u32 = 100;
/// Aimed at a hidden enemy: within this many degrees of its chest ...
pub const PREAIM_DEG: f32 = 2.0;
/// ... once it has been out of plain sight this long.
pub const PREAIM_HIDDEN_MS: u32 = 250;

/// The lines a match is queued for review past.
pub const HEAD_RATE: f32 = 0.6;
pub const HEAD_MIN_HITS: u32 = 20;
pub const SNAPS_MIN: u32 = 5;
pub const SNAP_SHARE: f32 = 0.5;
pub const REACTION_MS: u32 = 150;
pub const REACTION_MIN: usize = 5;
pub const PREAIM_MS: u32 = 3000;

/// Yaws kept per fighter: enough for [`SNAP_MS`].
const YAWS: usize = 32;

/// One fighter's numbers over a match.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    /// Hitscan rounds fired (a pellet is a round).
    pub shots: u32,
    pub hits: u32,
    pub head_hits: u32,
    pub kills: u32,
    /// Each sighting's reaction, in ms, for those that ended in a hit.
    pub reactions_ms: Vec<u32>,
    /// Sightings whose first hit was a snap.
    pub snaps: u32,
    pub preaim_ms: u32,
}

impl Stats {
    pub fn head_rate(&self) -> f32 {
        if self.hits == 0 {
            0.0
        } else {
            self.head_hits as f32 / self.hits as f32
        }
    }

    pub fn median_reaction_ms(&self) -> Option<u32> {
        if self.reactions_ms.is_empty() {
            return None;
        }
        let mut r = self.reactions_ms.clone();
        r.sort_unstable();
        Some(r[r.len() / 2])
    }

    /// Which lines these numbers cross.
    pub fn flags(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.hits >= HEAD_MIN_HITS && self.head_rate() >= HEAD_RATE {
            out.push("head_rate");
        }
        if self.snaps >= SNAPS_MIN && self.snaps as f32 >= SNAP_SHARE * self.reactions_ms.len() as f32 {
            out.push("snaps");
        }
        if self.reactions_ms.len() >= REACTION_MIN && self.median_reaction_ms().is_some_and(|m| m < REACTION_MS) {
            out.push("reaction");
        }
        if self.preaim_ms > PREAIM_MS {
            out.push("preaim");
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Sight {
    /// When the current sighting began.
    since: Option<u32>,
    /// The last tick in plain sight.
    last: Option<u32>,
    /// The current sighting already ended in a hit.
    hit: bool,
}

/// The numbers for every seat, kept as the match runs.
#[derive(Clone, Debug)]
pub struct Watch {
    /// `sight[viewer][target]`.
    sight: [[Sight; MAX_FIGHTERS]; MAX_FIGHTERS],
    yaws: [[u16; YAWS]; MAX_FIGHTERS],
    preaim_ticks: [u32; MAX_FIGHTERS],
    pub stats: [Stats; MAX_FIGHTERS],
}

impl Default for Watch {
    fn default() -> Self {
        Watch {
            sight: [[Sight::default(); MAX_FIGHTERS]; MAX_FIGHTERS],
            yaws: [[0; YAWS]; MAX_FIGHTERS],
            preaim_ticks: [0; MAX_FIGHTERS],
            stats: Default::default(),
        }
    }
}

fn ms_to_ticks(ms: u32) -> u32 {
    ticks(ms)
}

fn ticks_to_ms(t: u32) -> u32 {
    t * 1000 / TICK_HZ
}

/// Turn between two yaws, in degrees.
fn turn_deg(a: u16, b: u16) -> f32 {
    (a.wrapping_sub(b) as i16).unsigned_abs() as f32 * (360.0 / 65536.0)
}

impl Watch {
    /// Reads the tick the world just stepped. `shown(v, t)`: whether fog
    /// sent seat `t` to seat `v` this tick.
    pub fn observe(&mut self, w: &World, shown: impl Fn(usize, usize) -> bool) {
        self.observe_with(w, &mut Plain::default(), shown);
    }

    /// [`Watch::observe`], sharing this tick's plain tests (`plain`, of `w`).
    pub fn observe_with(&mut self, w: &World, plain: &mut Plain, shown: impl Fn(usize, usize) -> bool) {
        let tick = w.tick;
        let gap = ms_to_ticks(SIGHT_GAP_MS);
        let hidden_for = ms_to_ticks(PREAIM_HIDDEN_MS);
        let cos_pre = (PREAIM_DEG.to_radians()).cos();
        for (v, me) in w.fighters.iter().enumerate() {
            self.yaws[v][tick as usize % YAWS] = me.yaw;
            let aim = aim_dir(me.yaw, me.pitch);
            let on = |them: &sim::fighter::Fighter| {
                let to = them.body.pos + V3::new(0.0, 1.1, 0.0) - me.eye();
                let d = to.len();
                d > 1.0 && aim.dot(to * (1.0 / d)) >= cos_pre
            };
            // Aimed at an enemy in plain sight: one behind a wall on the
            // same line is no evidence (Doubles and free for all have many).
            // Asked only when it matters.
            let mut honest: Option<bool> = None;
            for (t, them) in w.fighters.iter().enumerate() {
                let enemy = t != v && them.seated && !(w.rules.teams && them.team == me.team);
                let s = &mut self.sight[v][t];
                if !enemy || !me.active() || !them.active() {
                    *s = Sight::default();
                    continue;
                }
                if plain.sees(w, v, t) {
                    if s.last.is_none_or(|l| tick.saturating_sub(l) > gap) {
                        s.since = Some(tick);
                        s.hit = false;
                    }
                    s.last = Some(tick);
                    continue;
                }
                let long_hidden = s.last.is_none_or(|l| tick.saturating_sub(l) > hidden_for);
                if long_hidden && shown(v, t) && on(them) {
                    let honest = *honest.get_or_insert_with(|| {
                        w.fighters.iter().enumerate().any(|(u, other)| {
                            u != v
                                && other.active()
                                && !(w.rules.teams && other.team == me.team)
                                && on(other)
                                && plain.sees(w, v, u)
                        })
                    });
                    if !honest {
                        self.preaim_ticks[v] += 1;
                    }
                }
            }
            self.stats[v].preaim_ms = ticks_to_ms(self.preaim_ticks[v]);
        }

        // A hitscan round is a Shot followed at once by its Hit, if any.
        let snap_back = ms_to_ticks(SNAP_MS) as usize;
        let mut prev_shot: Option<u8> = None;
        for e in w.events.as_slice() {
            let a = e.a as usize;
            match e.kind {
                Ev::Shot => {
                    if a < MAX_FIGHTERS {
                        self.stats[a].shots += 1;
                    }
                    prev_shot = Some(e.a);
                    continue;
                }
                Ev::Hit if prev_shot == Some(e.a) && e.a != e.b && a < MAX_FIGHTERS => {
                    let st = &mut self.stats[a];
                    st.hits += 1;
                    if e.part == 2 || e.part == 3 {
                        st.head_hits += 1;
                    }
                    let s = &mut self.sight[a][e.b as usize % MAX_FIGHTERS];
                    if let (Some(since), false) = (s.since, s.hit) {
                        st.reactions_ms.push(ticks_to_ms(tick - since));
                        s.hit = true;
                        let now = w.fighters[a].yaw;
                        let turned = (1..=snap_back.min(YAWS - 1))
                            .filter(|&k| tick as usize >= k)
                            .map(|k| turn_deg(now, self.yaws[a][(tick as usize - k) % YAWS]))
                            .fold(0.0f32, f32::max);
                        if turned > SNAP_DEG {
                            st.snaps += 1;
                        }
                    }
                }
                Ev::Kill if e.a != e.b && a < MAX_FIGHTERS => self.stats[a].kills += 1,
                _ => {}
            }
            prev_shot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_wraps() {
        assert!((turn_deg(65000, 500) - 5.69).abs() < 0.01);
        assert!((turn_deg(0, 16384) - 90.0).abs() < 1e-3);
    }

    #[test]
    fn the_lines() {
        let mut s = Stats {
            hits: 30,
            head_hits: 20,
            reactions_ms: vec![80, 90, 100, 300, 120],
            ..Stats::default()
        };
        assert_eq!(s.flags(), vec!["head_rate", "reaction"]);
        s.hits = 10;
        s.head_hits = 10;
        s.reactions_ms = vec![80, 90];
        assert!(s.flags().is_empty(), "too few to say");
    }
}

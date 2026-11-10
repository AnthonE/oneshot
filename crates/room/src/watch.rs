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
//!
//! Beside the lines, the rest of a profile (docs/ANTICHEAT.md §3, §7): how
//! much of the time an enemy is in plain sight the aim is on one, how rough
//! the aim is (its turn rate's change tick to tick), ground covered, time in
//! the air, grenades and melee, and deaths to someone never answered. An
//! aimbot raises the aim and none of the rest; nothing here is a line.

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
/// On target: within this many degrees of an enemy's chest.
pub const ON_DEG: f32 = 4.0;
/// A death is unanswered when the one who died hadn't hit the killer in
/// this long before.
pub const UNANSWERED_MS: u32 = 3000;
/// Moved further than this in a tick is a spawn or a teleport, not ground
/// covered.
const STEP_MAX_M: f32 = 1.0;

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

/// Ticks between looks at who is in plain sight of whom: the lines above
/// are 100 ms and more, so every other tick (16 ms) tells them apart as
/// well as every tick, for half the rays. Even ticks, which every
/// snapshot rate sends on, so fog shares the answers (`Plain`).
pub const SIGHT_EVERY: u32 = 2;

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
    /// Ticks standing.
    pub alive_ticks: u32,
    /// Ticks with an enemy in plain sight, and of those, aimed within
    /// [`ON_DEG`] of one.
    pub sight_ticks: u32,
    pub on_ticks: u32,
    /// Turn, degrees, and the change of turn rate tick to tick, summed.
    pub turn_deg: f32,
    pub jerk_deg: f32,
    /// Ground covered, metres, and ticks off it.
    pub moved_m: f32,
    pub air_ticks: u32,
    pub throws: u32,
    pub melees: u32,
    /// Deaths to an enemy, and those [`UNANSWERED_MS`] after any hit back.
    pub deaths: u32,
    pub unanswered: u32,
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

    /// The share of time with an enemy in plain sight the aim was on one.
    pub fn on_target(&self) -> f32 {
        self.on_ticks as f32 / self.sight_ticks.max(1) as f32
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
    /// Each seat's last tick standing: yaw, pitch, where, its turn.
    prev: [Option<(u16, i16, V3, f32)>; MAX_FIGHTERS],
    /// `hit_at[a][b]`: when `a` last hit `b`.
    hit_at: [[Option<u32>; MAX_FIGHTERS]; MAX_FIGHTERS],
    pub stats: [Stats; MAX_FIGHTERS],
}

impl Default for Watch {
    fn default() -> Self {
        Watch {
            sight: [[Sight::default(); MAX_FIGHTERS]; MAX_FIGHTERS],
            yaws: [[0; YAWS]; MAX_FIGHTERS],
            preaim_ticks: [0; MAX_FIGHTERS],
            prev: [None; MAX_FIGHTERS],
            hit_at: [[None; MAX_FIGHTERS]; MAX_FIGHTERS],
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
    /// Seat `s` has someone new in it: their numbers start from nothing.
    pub fn reset(&mut self, s: usize) {
        self.stats[s] = Stats::default();
        self.preaim_ticks[s] = 0;
        self.yaws[s] = [0; YAWS];
        self.sight[s] = [Sight::default(); MAX_FIGHTERS];
        for row in &mut self.sight {
            row[s] = Sight::default();
        }
        self.prev[s] = None;
        self.hit_at[s] = [None; MAX_FIGHTERS];
        for row in &mut self.hit_at {
            row[s] = None;
        }
    }

    /// Reads the tick the world just stepped. `shown(v, t)`: whether fog
    /// sent seat `t` to seat `v` this tick. Who is in plain sight of whom
    /// is looked at every [`SIGHT_EVERY`] ticks; the shots and hits every
    /// tick.
    pub fn observe(&mut self, w: &World, shown: impl Fn(usize, usize) -> bool) {
        self.observe_with(w, &mut Plain::default(), u16::MAX, shown);
    }

    /// [`Watch::observe`], sharing this tick's plain tests (`plain`, of `w`),
    /// and looking at who is in sight only for the seats in `people` (bit
    /// `s`): a room reviews its people, and a bot's row of sight tests
    /// (every enemy, every other tick, rays nobody reads) is most of a
    /// bot-filled room's. Everyone's shots, hits and turns are still counted.
    pub fn observe_with(&mut self, w: &World, plain: &mut Plain, people: u16, shown: impl Fn(usize, usize) -> bool) {
        let tick = w.tick;
        let gap = ms_to_ticks(SIGHT_GAP_MS);
        let hidden_for = ms_to_ticks(PREAIM_HIDDEN_MS);
        let cos_pre = (PREAIM_DEG.to_radians()).cos();
        let look = tick.is_multiple_of(SIGHT_EVERY);
        let cos_on = (ON_DEG.to_radians()).cos();
        for (v, me) in w.fighters.iter().enumerate() {
            self.yaws[v][tick as usize % YAWS] = me.yaw;
            // The aim's own motion every tick (no rays): its turn and jerk
            // are tick to tick.
            if me.active() {
                let st = &mut self.stats[v];
                st.alive_ticks += 1;
                st.air_ticks += !me.body.grounded as u32;
                let mut turn = 0.0;
                if let Some((py, pp, pos, pturn)) = self.prev[v] {
                    let dp = (me.pitch.wrapping_sub(pp) as i32).unsigned_abs() as f32 * (360.0 / 65536.0);
                    turn = turn_deg(me.yaw, py) + dp;
                    st.turn_deg += turn;
                    st.jerk_deg += (turn - pturn).abs();
                    let step = (me.body.pos - pos).flat().len();
                    if step < STEP_MAX_M {
                        st.moved_m += step;
                    }
                }
                self.prev[v] = Some((me.yaw, me.pitch, me.body.pos, turn));
            } else {
                self.prev[v] = None;
            }
            if !look || people & (1 << v) == 0 {
                continue;
            }
            let aim = aim_dir(me.yaw, me.pitch);
            let within = |them: &sim::fighter::Fighter, cos: f32| {
                let to = them.body.pos + V3::new(0.0, 1.1, 0.0) - me.eye();
                let d = to.len();
                d > 1.0 && aim.dot(to * (1.0 / d)) >= cos
            };
            let on = |them: &sim::fighter::Fighter| within(them, cos_pre);
            let (mut in_sight, mut on_one) = (false, false);
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
                    in_sight = true;
                    on_one |= within(them, cos_on);
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
                        // The ticks since the last look: all aimed so.
                        self.preaim_ticks[v] += SIGHT_EVERY;
                    }
                }
            }
            self.stats[v].preaim_ms = ticks_to_ms(self.preaim_ticks[v]);
            // The ticks since the last look: all spent so.
            self.stats[v].sight_ticks += in_sight as u32 * SIGHT_EVERY;
            self.stats[v].on_ticks += (in_sight && on_one) as u32 * SIGHT_EVERY;
        }

        // A hitscan round is a Shot followed at once by its Hit, if any.
        let snap_back = ms_to_ticks(SNAP_MS) as usize;
        let answer = ms_to_ticks(UNANSWERED_MS);
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
                Ev::Hit if e.a != e.b && a < MAX_FIGHTERS => {
                    if let Some(h) = self.hit_at[a].get_mut(e.b as usize) {
                        *h = Some(tick);
                    }
                    if prev_shot != Some(e.a) {
                        prev_shot = None;
                        continue;
                    }
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
                Ev::Kill if e.a != e.b && a < MAX_FIGHTERS => {
                    self.stats[a].kills += 1;
                    let b = e.b as usize;
                    if b < MAX_FIGHTERS {
                        let st = &mut self.stats[b];
                        st.deaths += 1;
                        if self.hit_at[b][a].is_none_or(|h| tick - h > answer) {
                            st.unanswered += 1;
                        }
                    }
                }
                Ev::Throw if a < MAX_FIGHTERS => self.stats[a].throws += 1,
                Ev::Melee if a < MAX_FIGHTERS => self.stats[a].melees += 1,
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

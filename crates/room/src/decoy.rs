//! Decoys: an enemy that isn't there (after Valve's Dota 2 honeypot and the
//! fake players of older anti-cheats). The aim review and the detector lean
//! on how a person's aim *looks*; a decoy asks a question an honest player
//! cannot answer. A client that draws only what it is sent, behind the
//! walls the map has, never shows a body standing where its viewer has no
//! line to it. A client that reads what it was sent does.
//!
//! **Every decoy is half of a coin flip.** For a viewer, two spots are
//! picked that it cannot see and could not soon; a keyed coin sends one of
//! them, in the place of an enemy fog is not showing, and holds the other
//! back. The held spot is the control: the viewer's own aim, on the same
//! map, in the same moment, toward a spot nothing was sent for. Over an
//! episode the aim drifts nearer one spot than the other; for an honest
//! player that is the sent one half the time ([`Tally`]): a null that is
//! 1/2 whatever the player's habits, the map or the picking, and a count a
//! sequential test adds up over matches (`tools/aim/players.py`). Every
//! episode counts, not only the few where the aim lands on a spot: a slow
//! assist that never gets there still leans the right way.
//!
//! **Offline for now.** `oneshot-aim league --decoys` plays it against the
//! zoo's own cheats ([`crate::assist::Assist::ghost`]): nothing here is on
//! a live match's wire. What putting it there takes is not in this file: a
//! decoy must stand still (the client voices a moving body's steps), and
//! the client must snap, not ease, an enemy from a decoy's spot to its own.
//!
//! Spots are where fighters have stood ([`Trail`]), so a decoy is never in
//! a wall or the air. Everything is a function of the world and a key, so
//! a replay and the match's salt give the same decoys again.

use sim::consts::{ticks, MAX_FIGHTERS, TICK_HZ};
use sim::fighter::Fighter;
use sim::geom::V3;
use sim::world::World;

use crate::assist::{angles_to, pitch_deg, wrap_deg, yaw_deg};
use crate::fog;

/// A viewer is offered an episode this often, and one lasts this long at
/// most.
pub const EVERY_MS: u32 = 4000;
pub const LAST_MS: u32 = 2500;
/// A spot is this far from its viewer, and within this of where it looks.
pub const NEAR_M: f32 = 5.0;
pub const FAR_M: f32 = 35.0;
pub const CONE_DEG: f32 = 40.0;
/// The two spots are at least this far apart as the viewer sees them.
pub const APART_DEG: f32 = 12.0;
/// Hidden with room to spare: fog's own test with this lead and peek, both
/// several times what fog sends an enemy by.
pub const LEAD_S: f32 = 0.6;
pub const PEEK_M: f32 = 1.5;
/// The aim came near a spot: within this of it for this long.
pub const TOUCH_DEG: f32 = 3.0;
pub const TOUCH_MS: u32 = 100;
/// Where on a standing body the aim is measured to, up from its feet.
const AIM_Y: f32 = 1.3;

/// Where fighters have stood lately: the spots a decoy may stand.
#[derive(Clone, Copy, Debug)]
pub struct Trail {
    spots: [V3; Trail::LEN],
    n: usize,
    at: usize,
}

impl Default for Trail {
    fn default() -> Self {
        Trail {
            spots: [V3::ZERO; Trail::LEN],
            n: 0,
            at: 0,
        }
    }
}

impl Trail {
    pub const LEN: usize = 96;

    /// Notes where everyone stands, once a second.
    pub fn note(&mut self, w: &World) {
        if !w.tick.is_multiple_of(TICK_HZ) {
            return;
        }
        for f in w.fighters.iter().filter(|f| f.active() && f.body.grounded) {
            self.spots[self.at] = f.body.pos;
            self.at = (self.at + 1) % Trail::LEN;
            self.n = (self.n + 1).min(Trail::LEN);
        }
    }

    fn spots(&self) -> &[V3] {
        &self.spots[..self.n]
    }
}

/// One viewer's episodes, counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub episodes: u32,
    /// The aim drew nearer the sent spot than the held one over the
    /// episode, or the other way: an honest player's are even.
    pub toward_sent: u32,
    pub toward_held: u32,
    /// The aim came to sit on the sent spot only, on the held one only.
    pub sent_only: u32,
    pub held_only: u32,
}

#[derive(Clone, Copy, Debug)]
struct Live {
    spots: [V3; 2],
    /// Which of them is sent, and as which seat.
    sent: usize,
    seat: usize,
    until: u32,
    /// Ticks the aim sat on each spot; how far off each it began; the sum
    /// of how far off each it was since, over `ticks`.
    near: [u32; 2],
    began: [f32; 2],
    off: [f32; 2],
    ticks: u32,
}

/// One viewer's decoys through a match.
#[derive(Clone, Copy, Debug, Default)]
pub struct Decoy {
    live: Option<Live>,
    pub tally: Tally,
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// `like` standing still at `spot`: what a decoy's viewer is sent.
pub fn standing(like: &Fighter, spot: V3) -> Fighter {
    let mut f = *like;
    f.body.pos = spot;
    f.body.vel = V3::ZERO;
    f
}

/// How far `viewer`'s aim is from a body standing at `spot`, degrees.
fn off_by(viewer: &Fighter, spot: V3) -> f32 {
    let (ty, tp) = angles_to(viewer.eye(), spot + V3::new(0.0, AIM_Y, 0.0));
    let (dy, dp) = (wrap_deg(ty - yaw_deg(viewer.yaw)), tp - pitch_deg(viewer.pitch));
    (dy * dy + dp * dp).sqrt()
}

/// Whether `viewer` has no line to a body like `like` at `spot`, nor would
/// soon.
fn hidden(w: &World, viewer: &Fighter, like: &Fighter, spot: V3) -> bool {
    !fog::sees(&w.map, viewer, &standing(like, spot), LEAD_S, PEEK_M)
}

impl Decoy {
    /// Seat `v`'s decoy for the tick after `w`: the seat it is sent as and
    /// where it stands, if one is out. `shown(t)`: fog is sending seat `t`
    /// to `v`. Call once a tick; it starts an episode on `v`'s turn if two
    /// hidden spots can be found, counts how near the aim sits to each
    /// while it lasts, and ends it when its time is up or it could no
    /// longer be told from an honest view: either spot could be seen, or
    /// the seat it borrows is one fog now shows.
    pub fn step(
        &mut self,
        w: &World,
        v: usize,
        key: u64,
        trail: &Trail,
        shown: impl Fn(usize) -> bool,
    ) -> Option<(usize, V3)> {
        let me = &w.fighters[v];
        if let Some(mut l) = self.live.take() {
            let them = &w.fighters[l.seat];
            let over = !me.active()
                || !them.active()
                || shown(l.seat)
                || w.tick >= l.until
                || l.spots.iter().any(|&s| !hidden(w, me, them, s));
            if !over {
                for i in 0..2 {
                    let off = off_by(me, l.spots[i]);
                    l.near[i] += (off <= TOUCH_DEG) as u32;
                    l.off[i] += off;
                }
                l.ticks += 1;
                self.live = Some(l);
                return Some((l.seat, l.spots[l.sent]));
            }
            if l.ticks == 0 {
                return None;
            }
            // How much nearer each spot the aim was, on average, than it
            // began.
            let nearer = [0, 1].map(|i| l.began[i] - l.off[i] / l.ticks as f32);
            let touched = l.near.map(|n| n >= ticks(TOUCH_MS));
            let (s, h) = (l.sent, 1 - l.sent);
            self.tally.episodes += 1;
            self.tally.toward_sent += (nearer[s] > nearer[h]) as u32;
            self.tally.toward_held += (nearer[h] > nearer[s]) as u32;
            self.tally.sent_only += (touched[s] && !touched[h]) as u32;
            self.tally.held_only += (touched[h] && !touched[s]) as u32;
            return None;
        }
        // Each viewer's turn comes round on its own tick of the slot.
        let every = ticks(EVERY_MS);
        if !me.active() || !(w.tick + (mix(key ^ v as u64) % every as u64) as u32).is_multiple_of(every) {
            return None;
        }
        let slot = mix(key ^ ((w.tick / every) as u64) << 8 ^ v as u64);
        // An enemy fog isn't showing and that couldn't be seen for a while.
        let seat = (0..MAX_FIGHTERS).find(|&t| {
            let them = &w.fighters[t];
            t != v
                && them.active()
                && !(w.rules.teams && them.team == me.team)
                && !shown(t)
                && !fog::sees(&w.map, me, them, LEAD_S, PEEK_M)
        })?;
        let like = &w.fighters[seat];
        // The trail from a keyed start, so which spots are tried first is
        // the key's choice.
        let spots = trail.spots();
        let mut picked = [V3::ZERO; 2];
        let mut n = 0;
        for i in 0..spots.len() {
            let s = spots[(i + slot as usize) % spots.len()];
            let d = (s - me.body.pos).flat().len();
            if !(NEAR_M..=FAR_M).contains(&d) {
                continue;
            }
            let (ty, _) = angles_to(me.eye(), s + V3::new(0.0, AIM_Y, 0.0));
            if wrap_deg(ty - yaw_deg(me.yaw)).abs() > CONE_DEG {
                continue;
            }
            if n == 1 {
                let (py, _) = angles_to(me.eye(), picked[0] + V3::new(0.0, AIM_Y, 0.0));
                if wrap_deg(ty - py).abs() < APART_DEG {
                    continue;
                }
            }
            if !hidden(w, me, like, s) {
                continue;
            }
            picked[n] = s;
            n += 1;
            if n == 2 {
                break;
            }
        }
        if n < 2 {
            return None;
        }
        let sent = (slot >> 40 & 1) as usize;
        self.live = Some(Live {
            spots: picked,
            sent,
            seat,
            until: w.tick + ticks(LAST_MS),
            near: [0; 2],
            began: picked.map(|s| off_by(me, s)),
            off: [0.0; 2],
            ticks: 0,
        });
        Some((seat, picked[sent]))
    }
}

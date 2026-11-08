//! Our own aim cheats, for the detector to learn from (docs/ANTICHEAT.md
//! §5). A real aimbot *assists* a person, so this is a function from a
//! seat's input to the same input bent: the person's own turn kept, the
//! assist's correction laid on top, the trigger pulled for them if it
//! does that. It knows what a cheating client knows and nothing more: the
//! enemies fog sent this seat this tick, where they stand.
//!
//! Laid over a recorded match (or a bot's play) and stepped on, it gives
//! the same player, the same match and the same moment with and without
//! the cheat (`tools/aim`). Nothing here runs in a live match.
//!
//! An assist is its [`Knobs`], and its knobs are a [`Genome`] in Gates'
//! encoding: each cheat in the zoo is one `oa1:` line that rebuilds
//! exactly.

use sim::arms::arm;
use sim::consts::{ticks, HEAD_Y, HEAD_Y_CROUCH, MAX_FIGHTERS, PITCH_MAX};
use sim::fighter::Fighter;
use sim::fmath;
use sim::geom::{rad_to_yaw, V3};
use sim::input::{Input, FIRE};
use sim::world::World;

use crate::fog::Plain;
use crate::genome::{soft, Draw, Genome};

/// The text form's prefix: an assist's knobs, as [`Knobs::from_genome`]
/// reads them. A change to that list is a new prefix.
pub const PREFIX: &[u8; 4] = b"oa1:";
/// How many values an `oa1:` genome describes.
pub const GENES: usize = 11;
/// Their spread before squashing: wide, so a fresh genome lands anywhere
/// in each knob's range.
const GENE_SCALE: f32 = 1.5;

/// Where the chest is aimed at, up from the feet.
const CHEST_Y: f32 = 1.1;
/// A lock is kept until the target is this many times the field of view
/// away (a flick past it doesn't drop it).
const KEEP_FOV: f32 = 1.5;

/// What one assist does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Knobs {
    /// Moves the aim at all (else it only pulls the trigger).
    pub aim: bool,
    /// Pulls the trigger when the aim is on the target.
    pub trigger: bool,
    /// Locks on an enemy only within this many degrees of the aim.
    pub fov_deg: f32,
    /// The share of what's left of the angle it closes each tick: 1 snaps.
    pub smooth: f32,
    /// The most it turns the view in a tick, degrees.
    pub max_deg: f32,
    /// After a lock, how long before it touches the aim.
    pub delay_ms: u32,
    /// After a lock, how long before it pulls the trigger.
    pub fire_ms: u32,
    /// Where it aims: 0 the chest, 1 the head.
    pub bone: f32,
    /// The share of locks it leaves alone (misses on purpose).
    pub miss: f32,
    /// How far off the point it aims, at most, degrees: a fresh offset each
    /// lock, so its hits don't all land on one spot.
    pub jitter_deg: f32,
    /// Locks on an enemy fog sent even when it isn't in plain sight (a
    /// wallhack using what fog lets through).
    pub walls: bool,
}

/// What a value from the genome is squashed to: (0, 1).
fn unit(g: f32) -> f32 {
    0.5 * (soft(g) + 1.0)
}

/// `lo` to `hi` on a log scale.
fn log_range(u: f32, lo: f32, hi: f32) -> f32 {
    lo * fmath::powf(hi / lo, u)
}

impl Knobs {
    /// Straight to the head the tick it is seen, and pull: the scripted
    /// aimbot of `room/tests/watch.rs`.
    pub const BLATANT: Knobs = Knobs {
        aim: true,
        trigger: true,
        fov_deg: 180.0,
        smooth: 1.0,
        max_deg: 180.0,
        delay_ms: 0,
        fire_ms: 0,
        bone: 1.0,
        miss: 0.0,
        jitter_deg: 0.0,
        walls: false,
    };

    /// The knobs an `oa1:` genome describes.
    pub fn from_genome(g: &Genome) -> Knobs {
        let mut v = [0.0f32; GENES];
        g.build(&mut v, GENE_SCALE);
        let u = v.map(unit);
        let trigger = u[7] > 0.5;
        Knobs {
            // Neither moving the aim nor pulling the trigger is no cheat.
            aim: u[8] > 0.25 || !trigger,
            trigger,
            fov_deg: log_range(u[0], 2.0, 60.0),
            smooth: log_range(u[1], 0.01, 1.0),
            max_deg: log_range(u[5], 0.05, 20.0),
            delay_ms: (u[2] * 400.0) as u32,
            fire_ms: (u[10] * 200.0) as u32,
            bone: u[3],
            miss: u[4] * 0.6,
            jitter_deg: u[6] * 1.5,
            walls: u[9] > 0.7,
        }
    }

    /// Its style, for reporting a detector by kind of cheat and for the
    /// zoo's grid: one of `trigger`, `walls`, `blatant`, `closet`, `smooth`.
    pub fn style(&self) -> &'static str {
        // The most it can turn in 100 ms of a fresh lock, degrees.
        let speed = self.max_deg * ticks(100) as f32;
        if !self.aim {
            "trigger"
        } else if self.walls {
            "walls"
        } else if self.smooth >= 0.3 && speed >= 20.0 && self.delay_ms < 100 {
            "blatant"
        } else if speed < 5.0 || self.fov_deg < 5.0 {
            "closet"
        } else {
            "smooth"
        }
    }
}

/// One seat's assist, with what it remembers between ticks.
#[derive(Clone, Copy, Debug)]
pub struct Assist {
    pub knobs: Knobs,
    seat: usize,
    rng: Draw,
    /// The enemy it is locked on, and since when.
    lock: Option<(usize, u32)>,
    /// This lock is one it leaves alone.
    leave: bool,
    /// This lock's offset from the point, degrees (yaw, pitch).
    off: (f32, f32),
    /// The seat wasn't standing last tick (or this is the first): its
    /// input is taken as it is, since a person's aim starts again at a
    /// spawn.
    fresh: bool,
}

/// Degrees between two turn counts, wrapped to (−180, 180].
pub fn wrap_deg(d: f32) -> f32 {
    d - 360.0 * fmath::round(d / 360.0)
}

fn yaw_deg(y: u16) -> f32 {
    y as f32 * (360.0 / 65536.0)
}

fn pitch_deg(p: i16) -> f32 {
    p as f32 * (360.0 / 65536.0)
}

/// The yaw and pitch, in degrees, from `eye` to `at`.
pub fn angles_to(eye: V3, at: V3) -> (f32, f32) {
    let to = at - eye;
    let flat = (to.x * to.x + to.z * to.z).sqrt();
    let deg = 180.0 / core::f32::consts::PI;
    (fmath::atan2(to.z, to.x) * deg, fmath::atan2(to.y, flat) * deg)
}

/// Where `them` is aimed at: `bone` 0 the chest, 1 the head.
pub fn aim_point(them: &Fighter, bone: f32) -> V3 {
    let head = if them.body.crouched { HEAD_Y_CROUCH } else { HEAD_Y };
    them.body.pos + V3::new(0.0, CHEST_Y + (head - CHEST_Y) * bone, 0.0)
}

/// Writes a yaw and pitch, degrees, into an input.
fn set_aim(inp: &mut Input, yaw: f32, pitch: f32) {
    inp.yaw = rad_to_yaw(yaw * (core::f32::consts::PI / 180.0));
    inp.pitch = fmath::round(pitch * (65536.0 / 360.0)).clamp(-PITCH_MAX as f32, PITCH_MAX as f32) as i16;
}

/// Whether seat `t` is an enemy of seat `v` the assist may lock on.
fn enemy(w: &World, v: usize, t: usize) -> bool {
    let (me, them) = (&w.fighters[v], &w.fighters[t]);
    t != v && them.seated && them.active() && !(w.rules.teams && them.team == me.team)
}

impl Assist {
    /// Seat `seat`'s assist; `seed` picks which locks it leaves and where
    /// off the point it aims.
    pub fn new(knobs: Knobs, seat: usize, seed: u32) -> Assist {
        Assist {
            knobs,
            seat,
            rng: Draw(seed ^ (seat as u32).wrapping_mul(0x9e37_79b9)),
            lock: None,
            leave: false,
            off: (0.0, 0.0),
            fresh: true,
        }
    }

    pub fn seat(&self) -> usize {
        self.seat
    }

    /// The enemy it is locked on and whether it is helping with it.
    pub fn locked(&self) -> Option<(usize, bool)> {
        self.lock.map(|(t, _)| (t, !self.leave))
    }

    /// Bends `inp`, this seat's input for the tick after `w`. `from` is the
    /// yaw and pitch the input's author turned from: the person's own turn
    /// is `inp` less `from`, and it is kept, on top of where the view
    /// really is. `shown(t)`: whether fog sent seat `t` to this seat for
    /// this tick. Whether it helped this tick: turned the aim or pulled.
    pub fn bend(
        &mut self,
        w: &World,
        plain: &mut Plain,
        shown: impl Fn(usize) -> bool,
        from: (u16, i16),
        inp: &mut Input,
    ) -> bool {
        let k = self.knobs;
        let s = self.seat;
        let me = &w.fighters[s];
        if !me.active() {
            self.lock = None;
            self.fresh = true;
            return false;
        }
        let from = if self.fresh { (me.yaw, me.pitch) } else { from };
        self.fresh = false;
        let mut yaw = yaw_deg(me.yaw) + wrap_deg(yaw_deg(inp.yaw) - yaw_deg(from.0));
        let mut pitch = pitch_deg(me.pitch) + (pitch_deg(inp.pitch) - pitch_deg(from.1));
        // The person's own turn, on top of where the view is (the same
        // input for an author that turned from the view itself, as a bot).
        set_aim(inp, yaw, pitch);
        let eye = me.eye();
        // How far the aim is from seat `t`'s point, degrees.
        let off_by = |t: usize, yaw: f32, pitch: f32| {
            let (ty, tp) = angles_to(eye, aim_point(&w.fighters[t], k.bone));
            let (dy, dp) = (wrap_deg(ty - yaw), tp - pitch);
            (dy * dy + dp * dp).sqrt()
        };
        let mut can = |t: usize| enemy(w, s, t) && shown(t) && (k.walls || plain.sees(w, s, t));

        // Keep the lock while it can, else the enemy nearest the aim.
        let kept = self
            .lock
            .filter(|&(t, _)| can(t) && off_by(t, yaw, pitch) <= k.fov_deg * KEEP_FOV);
        if kept.is_none() {
            let best = (0..MAX_FIGHTERS)
                .filter(|&t| can(t))
                .map(|t| (t, off_by(t, yaw, pitch)))
                .filter(|&(_, d)| d <= k.fov_deg)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            self.lock = best.map(|(t, _)| (t, w.tick));
            if self.lock.is_some() {
                self.leave = self.rng.draw() < k.miss;
                let r = k.jitter_deg;
                self.off = ((self.rng.draw() * 2.0 - 1.0) * r, (self.rng.draw() * 2.0 - 1.0) * r);
            }
        }
        let Some((t, since)) = self.lock else { return false };
        if self.leave {
            return false;
        }
        let mut helped = false;
        let held = w.tick.saturating_sub(since);
        let them = &w.fighters[t];

        if k.aim && held >= ticks(k.delay_ms) {
            let (ty, tp) = angles_to(eye, aim_point(them, k.bone));
            let (ty, tp) = (ty + self.off.0, tp + self.off.1);
            yaw += (wrap_deg(ty - yaw) * k.smooth).clamp(-k.max_deg, k.max_deg);
            pitch += ((tp - pitch) * k.smooth).clamp(-k.max_deg, k.max_deg);
            set_aim(inp, yaw, pitch);
            helped = true;
        }

        if k.trigger && held >= ticks(k.fire_ms) {
            // On target: within the part's width at that distance.
            let d = (aim_point(them, k.bone) - eye).len().max(1.0);
            let half = 0.3 + (0.12 - 0.3) * k.bone;
            let cone = fmath::atan2(half, d) * (180.0 / core::f32::consts::PI);
            let (py, pp) = angles_to(eye, aim_point(them, k.bone));
            let (ey, ep) = (wrap_deg(py - yaw), pp - pitch);
            if (ey * ey + ep * ep).sqrt() < cone {
                // A semi-automatic needs the trigger let go between pulls.
                if arm(me.gun().arm).auto || me.last_buttons & FIRE == 0 {
                    inp.buttons |= FIRE;
                } else {
                    inp.buttons &= !FIRE;
                }
                helped = true;
            }
        }
        helped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_genome_rebuilds_its_knobs() {
        let g = Genome::new(42).child(0x1234).unwrap();
        let text = g.text(PREFIX);
        let back = Genome::decode(PREFIX, text.as_bytes()).unwrap();
        assert_eq!(Knobs::from_genome(&g), Knobs::from_genome(&back));
    }

    #[test]
    fn the_zoo_has_every_style() {
        let mut seen = std::collections::BTreeSet::new();
        for b in 0..400 {
            let k = Knobs::from_genome(&Genome::new(b));
            assert!(k.aim || k.trigger);
            assert!((2.0..=60.0).contains(&k.fov_deg) && (0.01..=1.0).contains(&k.smooth));
            seen.insert(k.style());
        }
        assert_eq!(seen.len(), 5, "{seen:?}");
    }

    #[test]
    fn wrap() {
        assert!((wrap_deg(350.0) + 10.0).abs() < 1e-4);
        assert!((wrap_deg(-190.0) - 170.0).abs() < 1e-4);
    }
}

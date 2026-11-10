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
//! encoding: each cheat in the zoo is one `oa2:` line that rebuilds
//! exactly.
//!
//! **A bred net** (`on1:`, [`Cheat`]) is the other kind. Thirteen knobs are
//! a small family, and a detector shown only that family may learn the
//! family, not cheating: the published aimbot that got past detectors
//! learned its movement (GAN-Aimbots, Kanervisto et al. 2022). So here a
//! small net (Gates' wolves' own, `crate::genome::Net`) reads the moment
//! each tick (how far off the aim is, how the target moves across the
//! view, what the person's own hand is doing, how long the lock has been
//! held) and says whether to act, how much of the angle to close, how
//! fast, where off the point to aim and whether to pull. It can only ever
//! turn toward its target, so a fresh one is a cheat of some kind, and
//! breeding shapes the kind.

use sim::arms::arm;
use sim::consts::{ticks, HEAD_Y, HEAD_Y_CROUCH, MAX_FIGHTERS, PITCH_MAX, SHIELD_MAX};
use sim::fighter::Fighter;
use sim::fmath;
use sim::geom::{rad_to_yaw, V3};
use sim::input::{Input, FIRE};
use sim::world::World;

use crate::fog::Plain;
use crate::genome::{soft, Draw, Genome, GenomeError, Net};

/// The text form's prefix: an assist's knobs, as [`Knobs::from_genome`]
/// reads them. A change to that list is a new prefix.
pub const PREFIX: &[u8; 4] = b"oa2:";
/// How many values an `oa2:` genome describes (`oa1:` was the first
/// eleven, before an assist could pick its moments).
pub const GENES: usize = 13;
/// Their spread before squashing: wide, so a fresh genome lands anywhere
/// in each knob's range.
const GENE_SCALE: f32 = 1.5;

/// The text form's prefix of a bred net: its weights, then the four knobs
/// it leaves to its line ([`Cheat::net`]). A change to the net's shape,
/// to what it reads or to what its outputs mean is a new prefix.
pub const NET_PREFIX: &[u8; 4] = b"on1:";
/// What the net reads and says ([`Assist::think`]), and its hidden units.
const NET_IN: usize = 12;
const NET_HID: usize = 12;
const NET_OUT: usize = 6;
const NET_W: usize = Net::weights(NET_IN, NET_HID, NET_OUT);
/// The knobs after a net's weights: field of view, bone, walls, trigger.
const NET_GENES: usize = 4;
/// How far off the point a net may aim, degrees.
const NET_OFF_DEG: f32 = 1.5;

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
    /// Acts only on an enemy at least this far away, metres (0: any): the
    /// long shots a person misses most.
    pub range_m: f32,
    /// Acts only while its own shields are down.
    pub low: bool,
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
        range_m: 0.0,
        low: false,
    };

    /// The knobs an `oa2:` genome describes.
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
            range_m: if u[11] < 0.6 { 0.0 } else { 10.0 + (u[11] - 0.6) * 75.0 },
            low: u[12] > 0.75,
        }
    }

    /// It acts only in some moments, picked by what its client can see.
    /// Selection seldom finds such a condition from scratch (it settles on
    /// never or always), so the zoo's grid keeps these in cells of their own.
    pub fn selective(&self) -> bool {
        self.range_m > 0.0 || self.low
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

/// One of the zoo's cheats: its knobs, and for a bred net the net that
/// turns them tick by tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cheat {
    pub knobs: Knobs,
    pub net: Option<Net>,
}

impl Cheat {
    /// The net an `on1:` genome describes, with the knobs it still sets:
    /// whom it locks on (field of view, walls), where it aims (bone) and
    /// whether it may pull. The rest is the net's, each tick.
    pub fn net(g: &Genome) -> Cheat {
        let mut v = [0.0f32; NET_W + NET_GENES];
        g.build_by(&mut v, |i| {
            if i < NET_W {
                Net::scale(NET_IN, NET_HID, i)
            } else {
                GENE_SCALE
            }
        });
        let u = |i: usize| unit(v[NET_W + i]);
        let knobs = Knobs {
            trigger: u(3) > 0.5,
            fov_deg: log_range(u(0), 2.0, 60.0),
            bone: u(1),
            walls: u(2) > 0.7,
            ..Knobs::BLATANT
        };
        Cheat {
            knobs,
            net: Net::new(NET_IN, NET_HID, NET_OUT, &v[..NET_W]),
        }
    }

    /// Its style: a knob assist's, or `net`.
    pub fn style(&self) -> &'static str {
        if self.net.is_some() {
            "net"
        } else {
            self.knobs.style()
        }
    }
}

/// A line of the zoo: a genome and which kind of cheat it describes, by
/// its prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub net: bool,
    pub genome: Genome,
}

impl Line {
    pub fn decode(text: &[u8]) -> Result<Line, GenomeError> {
        match Genome::decode(PREFIX, text) {
            Err(GenomeError::Prefix) => Genome::decode(NET_PREFIX, text).map(|genome| Line { net: true, genome }),
            r => r.map(|genome| Line { net: false, genome }),
        }
    }

    pub fn text(&self) -> String {
        self.genome.text(if self.net { NET_PREFIX } else { PREFIX })
    }

    pub fn cheat(&self) -> Cheat {
        if self.net {
            Cheat::net(&self.genome)
        } else {
            Cheat {
                knobs: Knobs::from_genome(&self.genome),
                net: None,
            }
        }
    }
}

/// What an assist does with one tick of a lock.
struct Plan {
    aim: bool,
    smooth: f32,
    max_deg: f32,
    off: (f32, f32),
    fire: bool,
}

/// One seat's assist, with what it remembers between ticks.
#[derive(Clone, Copy, Debug)]
pub struct Assist {
    pub knobs: Knobs,
    /// A decoy its client was sent this tick (`crate::decoy`): this seat,
    /// standing here, though fog shows no such enemy. It is out of sight by
    /// its making, so only an assist that locks through walls takes it.
    pub ghost: Option<(usize, V3)>,
    net: Option<Net>,
    /// Where its target was in the view last tick (a net reads its motion).
    seen: Option<(f32, f32)>,
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

pub fn yaw_deg(y: u16) -> f32 {
    y as f32 * (360.0 / 65536.0)
}

pub fn pitch_deg(p: i16) -> f32 {
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
            ghost: None,
            net: None,
            seen: None,
            seat,
            rng: Draw(seed ^ (seat as u32).wrapping_mul(0x9e37_79b9)),
            lock: None,
            leave: false,
            off: (0.0, 0.0),
            fresh: true,
        }
    }

    /// Seat `seat`'s assist with one of the zoo's cheats.
    pub fn of(cheat: &Cheat, seat: usize, seed: u32) -> Assist {
        Assist {
            net: cheat.net,
            ..Assist::new(cheat.knobs, seat, seed)
        }
    }

    pub fn seat(&self) -> usize {
        self.seat
    }

    /// A net's plan for this tick of its lock: what it reads, scaled to
    /// about (−1, 1), and what its six outputs mean. `err` and `vel` are
    /// the target's point from the aim and its motion across the view,
    /// degrees (yaw, pitch); `own` the person's own turn this tick.
    #[allow(clippy::too_many_arguments)]
    fn think(
        &mut self,
        net: &Net,
        err: (f32, f32),
        vel: (f32, f32),
        own: (f32, f32),
        held: u32,
        dist: f32,
        plain: bool,
        shield: f32,
    ) -> Plan {
        let far = |x: f32| soft(x) * 2.0 - 1.0;
        let x = [
            soft(err.0 / 10.0),
            soft(err.1 / 10.0),
            far((err.0 * err.0 + err.1 * err.1).sqrt() / 2.0),
            soft(vel.0),
            soft(vel.1),
            soft(own.0 / 2.0),
            soft(own.1 / 2.0),
            far(held as f32 / ticks(250) as f32),
            far(dist / 20.0),
            if plain { 1.0 } else { -1.0 },
            shield * 2.0 - 1.0,
            self.rng.draw() * 2.0 - 1.0,
        ];
        let mut o = [0.0f32; NET_OUT];
        net.forward(&x, &mut o);
        let u = |v: f32| 0.5 * (v + 1.0);
        Plan {
            aim: o[0] > 0.0,
            smooth: log_range(u(o[1]), 0.005, 1.0),
            max_deg: log_range(u(o[2]), 0.05, 20.0),
            fire: self.knobs.trigger && o[3] > 0.0,
            off: (o[4] * NET_OFF_DEG, o[5] * NET_OFF_DEG),
        }
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
        let own = (
            wrap_deg(yaw_deg(inp.yaw) - yaw_deg(from.0)),
            pitch_deg(inp.pitch) - pitch_deg(from.1),
        );
        let mut yaw = yaw_deg(me.yaw) + own.0;
        let mut pitch = pitch_deg(me.pitch) + own.1;
        // The person's own turn, on top of where the view is (the same
        // input for an author that turned from the view itself, as a bot).
        set_aim(inp, yaw, pitch);
        let eye = me.eye();
        // How far the aim is from seat `t`'s point, degrees.
        let ghost = self.ghost;
        let is_ghost = |t: usize| ghost.is_some_and(|(g, _)| g == t);
        // Seat `t` as this client knows it: where a decoy says, if it is one.
        let body = |t: usize| match ghost {
            Some((g, at)) if g == t => crate::decoy::standing(&w.fighters[t], at),
            _ => w.fighters[t],
        };
        let off_by = |t: usize, yaw: f32, pitch: f32| {
            let (ty, tp) = angles_to(eye, aim_point(&body(t), k.bone));
            let (dy, dp) = (wrap_deg(ty - yaw), tp - pitch);
            (dy * dy + dp * dp).sqrt()
        };
        let mut can = |t: usize| {
            enemy(w, s, t)
                && if is_ghost(t) {
                    k.walls
                } else {
                    shown(t) && (k.walls || plain.sees(w, s, t))
                }
        };

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
            self.seen = None;
            if self.lock.is_some() {
                self.leave = self.rng.draw() < k.miss;
                let r = k.jitter_deg;
                self.off = ((self.rng.draw() * 2.0 - 1.0) * r, (self.rng.draw() * 2.0 - 1.0) * r);
            }
        }
        let Some((t, since)) = self.lock else { return false };
        if is_ghost(t) && !self.leave {
            // A person who sees a body through a wall looks at it: their
            // own hand leaves a decoy to the assist and doesn't turn away
            // (a bot under the assist would, knowing nothing of it).
            yaw = yaw_deg(me.yaw);
            pitch = pitch_deg(me.pitch);
            set_aim(inp, yaw, pitch);
        }
        let held = w.tick.saturating_sub(since);
        let them = &body(t);
        let point = aim_point(them, k.bone);
        let (py, pp) = angles_to(eye, point);
        let plan = if let Some(net) = self.net {
            let vel = self.seen.map_or((0.0, 0.0), |(ly, lp)| (wrap_deg(py - ly), pp - lp));
            self.seen = Some((py, pp));
            let err = (wrap_deg(py - yaw), pp - pitch);
            let dist = (point - eye).len();
            let shield = (me.shield as f32 / SHIELD_MAX as f32).clamp(0.0, 1.0);
            let in_plain = !is_ghost(t) && plain.sees(w, s, t);
            self.think(&net, err, vel, own, held, dist, in_plain, shield)
        } else {
            if self.leave {
                return false;
            }
            if (k.low && me.shield > 0) || (k.range_m > 0.0 && (them.body.pos - me.body.pos).len() < k.range_m) {
                return false;
            }
            Plan {
                aim: k.aim && held >= ticks(k.delay_ms),
                smooth: k.smooth,
                max_deg: k.max_deg,
                off: self.off,
                fire: k.trigger && held >= ticks(k.fire_ms),
            }
        };
        let mut helped = false;

        if plan.aim {
            let (ty, tp) = (py + plan.off.0, pp + plan.off.1);
            yaw += (wrap_deg(ty - yaw) * plan.smooth).clamp(-plan.max_deg, plan.max_deg);
            pitch += ((tp - pitch) * plan.smooth).clamp(-plan.max_deg, plan.max_deg);
            set_aim(inp, yaw, pitch);
            helped = true;
        }

        if plan.fire {
            // On target: within the part's width at that distance.
            let d = (point - eye).len().max(1.0);
            let half = 0.3 + (0.12 - 0.3) * k.bone;
            let cone = fmath::atan2(half, d) * (180.0 / core::f32::consts::PI);
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
    fn some_assists_pick_their_moments() {
        let ks: Vec<Knobs> = (0..400).map(|b| Knobs::from_genome(&Genome::new(b))).collect();
        let picky = ks.iter().filter(|k| k.selective()).count();
        assert!((40..360).contains(&picky), "{picky} of 400 selective");
        assert!(ks.iter().any(|k| k.low) && ks.iter().any(|k| k.range_m > 0.0));
        assert!(ks
            .iter()
            .all(|k| k.range_m == 0.0 || (10.0..=40.0).contains(&k.range_m)));
    }

    #[test]
    fn a_net_rebuilds_and_acts() {
        let line = Line {
            net: true,
            genome: Genome::new(7).child(0x4321).unwrap(),
        };
        let back = Line::decode(line.text().as_bytes()).unwrap();
        assert_eq!(line, back);
        assert_eq!(line.cheat(), back.cheat());
        assert_eq!(line.cheat().style(), "net");
        // A knob line is still a knob line.
        let knobs = Line::decode(Genome::new(7).text(PREFIX).as_bytes()).unwrap();
        assert!(!knobs.net && knobs.cheat().net.is_none());
        // Fresh nets are not all one cheat: over a few moments some act,
        // some hold, and how hard they turn differs.
        let (mut acts, mut shares) = (0, std::collections::BTreeSet::new());
        for b in 0..60u32 {
            let cheat = Cheat::net(&Genome::new(b));
            let mut a = Assist::of(&cheat, 0, b);
            let net = cheat.net.unwrap();
            for held in [0, 8, 30] {
                let p = a.think(&net, (4.0, -1.0), (0.3, 0.0), (0.5, 0.0), held, 15.0, true, 1.0);
                assert!((0.005..=1.0).contains(&p.smooth) && (0.05..=20.0).contains(&p.max_deg));
                assert!(p.off.0.abs() <= NET_OFF_DEG && p.off.1.abs() <= NET_OFF_DEG);
                acts += p.aim as usize;
                shares.insert((p.smooth * 200.0) as i32);
            }
        }
        assert!((30..150).contains(&acts), "{acts} of 180 moments acted on");
        assert!(shares.len() > 20, "{} kinds of turn", shares.len());
    }

    #[test]
    fn wrap() {
        assert!((wrap_deg(350.0) + 10.0).abs() < 1e-4);
        assert!((wrap_deg(-190.0) - 170.0).abs() < 1e-4);
    }
}

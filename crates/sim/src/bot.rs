//! A simple AI fighter that reads the true world, for the server's empty
//! seats and for the determinism gate. Deterministic: its choices come from
//! the world and its own PCG stream. Fireplace's `client/arena-bot.mjs` is
//! the fuller one to port next (docs/RIP.md §2).
//!
//! It looks around (the rays: who it can see, a gun to take, the way) only
//! every [`DECIDE_EVERY`] ticks, the bots staggered so they never all look
//! on one tick; between looks it aims, fires and walks on what it decided,
//! and minds the edge every tick.

use crate::arms::{arm, NONE};
use crate::consts::*;
use crate::fighter::Gun;
use crate::fmath;
use crate::geom::{rad_to_yaw, V3};
use crate::input::*;
use crate::rng::Pcg32;
use crate::world::{Phase, Rules, World, FLAG_HELD, FLAG_HOME};

/// What a bot decided to walk to at its last look, where `steer` is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Steer {
    /// Nothing decided yet (just spawned, or just out of a fight): decide
    /// now.
    None,
    /// A gun it wants: taken in reach.
    Loot,
    /// Its flag business (Capture the Flag, Siege).
    Objective,
    /// A free vehicle: boarded in reach.
    Vehicle,
    /// Its roam point, or the next waypoint toward it.
    Roam,
    /// A carrier's way home.
    Carry,
    /// In a fight: an attacker's way to its objective, fought on the move.
    Push,
    /// In a fight: standing its ground.
    Stand,
}

#[derive(Clone, Copy)]
pub struct Bot {
    rng: Pcg32,
    /// How well it aims, 0..1.
    skill: f32,
    strafe: i8,
    strafe_for: u16,
    /// Ticks since it first saw its current target.
    seen: u16,
    target: u8,
    /// Siege, with no fighter in sight: the enemy minion it saw (255: none).
    minion: u8,
    /// A wobble that drifts, in turns.
    wob: (f32, f32),
    /// The target's velocity as its aim has caught up with it: a turn of
    /// the target's throws the aim off until it has (`AIM_TRAIL_S`).
    caught: V3,
    /// The map's roam point it walks to with nobody in sight, and ticks
    /// left before it gives up on it (stuck on a wall).
    goal: u8,
    goal_for: u16,
    /// Where it decided to walk at its last look, and what that is.
    steer: V3,
    kind: Steer,
    /// The waypoint it was nearest at its last look (`u16::MAX`: none).
    way: u16,
    /// Ticks in the vehicle it rides, and ticks before it may get into one
    /// again after getting out.
    ride_for: u16,
    ride_cool: u16,
    /// At the wheel: ticks it has barely moved, and ticks left backing up.
    stuck: u16,
    backing: u16,
    /// On foot with nobody in sight: ticks it has pushed at a wall going
    /// nowhere, and ticks left walking along it instead.
    foot_stuck: u16,
    detour: u16,
    /// Ticks left going to its objective by the roam points rather than
    /// straight: it got stuck going straight (a doorway's corner).
    round: u16,
}

/// Every how many ticks a bot looks around afresh. 31 ms: it still reacts
/// like a person.
const DECIDE_EVERY: usize = 4;
/// How long a bot walks at one roam point before it picks another.
const GOAL_MS: u32 = 8000;
/// Close enough to a roam point to pick the next.
const GOAL_NEAR: f32 = 2.0;
/// How long a bot drives (or rides) before it gets out, how long before it
/// gets back in, and how far it walks to a free vehicle.
const RIDE_MS: u32 = 20_000;
const RIDE_COOL_MS: u32 = 15_000;
const VEHICLE_WALK: f32 = 25.0;
/// How far a bot goes out of its way for a gun it wants, and how far above
/// or below it may be: on its own level, as it walks there straight.
const LOOT_M: f32 = 20.0;
const LOOT_RISE: f32 = 0.6;
/// Capture the Flag: an attacker on its way ignores enemies farther than
/// this.
const OBJECTIVE_IGNORE_M: f32 = 30.0;
/// Its aim: how many turns a second it may turn (and more with skill), the
/// wobble's step in turns, and how far behind a target's change of going
/// it is thrown (seconds of it, all of it at no skill) and how long it
/// takes to catch up.
const AIM_TURN: f32 = 0.6;
const AIM_TURN_SKILL: f32 = 1.4;
const AIM_WOB: f32 = 0.004;
const AIM_TRAIL_S: f32 = 0.3;
const AIM_CATCH_S: f32 = 0.25;
/// A gun of pellets is fired to this many times its full-damage reach.
const SPREAD_REACH: f32 = 1.5;
/// A bot's waist, for a straight walk's line.
const WAIST: f32 = 0.9;
/// On a map with a void: how far ahead a bot looks for the edge the way
/// it means to go (a run stops in about a metre), and how far it keeps
/// from it to jump (a jump at a run carries some 9 m; air control turns
/// the rest).
const EDGE_LOOK: [f32; 4] = [0.5, 1.0, 1.5, 2.0];
const EDGE_JUMP: [f32; 4] = [1.5, 3.0, 4.5, 6.0];
/// In the air, along the way it is going: where it would come down.
const EDGE_AIR: [f32; 4] = [0.5, 1.5, 2.5, 3.5];
/// A ledge this far down or more is a jump of its own: walked off at a
/// run, the fall carries a body on past [`EDGE_LOOK`] before the air
/// turns it, so the void is looked for beyond it as far as a jump's.
const EDGE_DROP: f32 = 1.0;
/// The turns it tries, smallest first, when the way it means to go is
/// over the edge: along it, then back from it. (sin, cos) of ±45°, ±90°,
/// ±135°.
const H: f32 = core::f32::consts::FRAC_1_SQRT_2;
const EDGE_TURNS: [(f32, f32); 6] = [(H, H), (-H, H), (1.0, 0.0), (-1.0, 0.0), (H, -H), (-H, -H)];

impl Bot {
    pub fn new(seed: u64, slot: usize, skill: f32) -> Bot {
        Bot {
            rng: Pcg32::new(seed, 100 + slot as u64),
            skill,
            strafe: 1,
            strafe_for: 0,
            seen: 0,
            target: 255,
            minion: 255,
            wob: (0.0, 0.0),
            caught: V3::ZERO,
            goal: 0,
            goal_for: 0,
            steer: V3::ZERO,
            kind: Steer::None,
            way: u16::MAX,
            ride_for: 0,
            ride_cool: 0,
            stuck: 0,
            backing: 0,
            foot_stuck: 0,
            detour: 0,
            round: 0,
        }
    }

    /// The map's roam point to head for with nobody in sight (picking the
    /// next when it's reached or taking too long), or with none, toward the
    /// middle, then the first pad. `elapsed`: ticks since it last asked.
    fn roam_goal(&mut self, w: &World, at: V3, elapsed: u16) -> V3 {
        let roam = &w.map.roam;
        if !roam.is_empty() {
            let here = roam[self.goal as usize % roam.len()];
            if self.goal_for == 0 || (here - at).flat().len() < GOAL_NEAR {
                self.goal = self.rng.next_bounded(roam.len() as u32) as u8;
                self.goal_for = ticks(GOAL_MS) as u16;
            }
            self.goal_for = self.goal_for.saturating_sub(elapsed);
            roam[self.goal as usize % roam.len()]
        } else if at.flat().len() > 6.0 {
            V3::ZERO
        } else {
            w.map.pads.first().map_or(V3::ZERO, |p| p.pos)
        }
    }

    /// A gun (or a power-up) it wants, near, on its level and a straight
    /// walk away: on a pad that holds it, or on the floor. Wanted: ranked over the best it carries that has rounds, or
    /// rounds for one of its own running low.
    fn loot(w: &World, slot: usize) -> Option<V3> {
        if w.rules.gun_game {
            return None;
        }
        let me = &w.fighters[slot];
        let waist = me.body.pos + V3::new(0.0, WAIST, 0.0);
        let rank = |g: &Gun| if g.spent() { 0 } else { arm(g.arm).rank };
        let top = rank(&me.guns[0]).max(rank(&me.guns[1]));
        let wants = |id: u8| {
            // A power-up, unless it is already on.
            match id {
                crate::arms::OVERSHIELD => return !me.overshield(),
                crate::arms::CAMO => return me.camo == 0,
                crate::arms::FRAGS => return me.grenades < GREN_MAX,
                crate::arms::STICKIES => return me.stickies < GREN_MAX,
                _ => {}
            }
            let a = arm(id);
            match me.guns.iter().find(|g| g.arm == id) {
                Some(g) => !a.hot() && (g.mag as u32 + g.reserve as u32) < a.mag as u32,
                None => a.rank > top,
            }
        };
        let mut best = None;
        let mut near = LOOT_M;
        let mut consider = |at: V3, id: u8| {
            let d = (at - me.body.pos).flat().len();
            if d < near
                && fmath::fabs(at.y - me.body.pos.y) < LOOT_RISE
                && wants(id)
                && w.map.clear(waist, at + V3::new(0.0, WAIST, 0.0))
            {
                near = d;
                best = Some(at);
            }
        };
        for (i, p) in w.map.pads.iter().enumerate().take(MAX_PADS) {
            if w.pads[i].empty_for == 0 {
                consider(p.pos, p.arm);
            }
        }
        for (_, d) in w.drops.live() {
            if d.gun.arm != NONE {
                consider(d.pos, d.gun.arm);
            }
        }
        best
    }

    /// Capture the Flag: where this bot wants to be with nobody in sight.
    /// Half of each side attacks ([`attacker`]: the enemy's flag, or with
    /// the teammate carrying it), the other half defends (their own flag while it is away, else their base); a
    /// carrier runs home. None: nothing to do but roam.
    fn objective(w: &World, slot: usize) -> Option<V3> {
        if w.rules.siege {
            return Self::siege_goal(w, slot);
        }
        let bases = w.map.flags.filter(|_| w.rules.ctf)?;
        let me = &w.fighters[slot];
        let mine = me.team as usize & 1;
        if me.flag {
            return Some(bases[mine]);
        }
        let theirs = &w.flags[mine ^ 1];
        let ours = &w.flags[mine];
        // Held, the enemy's flag is where its carrier (one of ours) is: an
        // attacker goes along.
        if attacker(slot) {
            return Some(theirs.pos);
        }
        // A defender goes after its own flag while it is away, and
        // otherwise holds its base (a few metres off the flag, its own way).
        if ours.state != FLAG_HOME {
            return Some(ours.pos);
        }
        let off = (slot / 4) as f32 * 1.7;
        Some(bases[mine] + V3::new(fmath::cos(off) * 4.0, 0.0, fmath::sin(off) * 4.0))
    }

    /// Siege: an attacker goes for the enemy's core once it is open, else
    /// for a generator of theirs still standing, two attackers to one (one
    /// alone dies before it is overloaded); a defender holds a generator of
    /// its own (by seat; the core once they are down) and goes for whoever
    /// is overloading it. A few metres apart in a ring, each its own way.
    fn siege_goal(w: &World, slot: usize) -> Option<V3> {
        let at = w.map.siege?;
        let me = &w.fighters[slot];
        let mine = me.team as usize & 1;
        let off = (slot / 4) as f32 * 1.7;
        let spread = V3::new(fmath::cos(off) * 1.5, 0.0, fmath::sin(off) * 1.5);
        let node = |side: usize, k: usize| side * (SIEGE_GENS + 1) + k;
        let standing = |side: usize| {
            (0..SIEGE_GENS)
                .map(move |k| node(side, k))
                .filter(|&n| !w.nodes[n].broken)
        };
        let pick = |side: usize, by: usize| {
            let n = standing(side).count();
            (n > 0).then(|| standing(side).nth(by % n)).flatten()
        };
        let n = if attacker(slot) {
            let theirs = mine ^ 1;
            if w.core_open(theirs) {
                node(theirs, SIEGE_GENS)
            } else {
                pick(theirs, slot / 8 + 1)?
            }
        } else {
            pick(mine, slot / 4).unwrap_or(node(mine, SIEGE_GENS))
        };
        Some(at[n] + spread)
    }

    /// Where to steer for `goal` from `at`: straight if the way is clear at
    /// the waist, else the map's roam point in clear sight that is nearest
    /// the goal and nearer it than we are (a step around what's between).
    fn toward(w: &World, at: V3, goal: V3) -> V3 {
        Self::toward_by(w, at, goal, false)
    }

    /// [`Self::toward`], by the roam points even when the way looks clear
    /// with `round`.
    pub(crate) fn toward_by(w: &World, at: V3, goal: V3, round: bool) -> V3 {
        let waist = V3::new(0.0, WAIST, 0.0);
        if !round && w.map.clear(at + waist, goal + waist) {
            return goal;
        }
        let mut best = goal;
        let mut bd = (goal - at).flat().len() - 1.0;
        for &r in &w.map.roam {
            let d = (goal - r).flat().len();
            if d < bd && (r - at).flat().len() > GOAL_NEAR && w.map.clear(at + waist, r + waist) {
                bd = d;
                best = r;
            }
        }
        best
    }

    /// The enemy's flag is on the floor or at home, landed, in reach.
    fn flag_in_reach(w: &World, slot: usize) -> bool {
        if !w.rules.ctf || w.map.flags.is_none() {
            return false;
        }
        let me = &w.fighters[slot];
        let f = &w.flags[(me.team as usize & 1) ^ 1];
        let d = f.pos - me.body.pos;
        f.state != FLAG_HELD && !f.flying && d.flat().len() < FLAG_REACH && fmath::fabs(d.y) < FLAG_RISE
    }

    /// USE pressed this tick, as a tap: let go first if it was held.
    fn tap_use(inp: &mut Input, last: u16) {
        if last & USE == 0 {
            inp.buttons |= USE;
        }
    }

    /// At the wheel: toward the nearest enemy in sight or the roam goal,
    /// out after a while or to fight someone near.
    fn drive(&mut self, w: &World, slot: usize, vi: usize, best: u8, near: f32, mut inp: Input) -> Input {
        let me = &w.fighters[slot];
        let v = &w.vehicles[vi];
        self.ride_for = self.ride_for.saturating_add(1);
        // Bots drive trucks only: in a one-seater (got into by a USE meant
        // for a pad), straight out.
        let truck = v.kind == crate::vehicle::TRUCK;
        if !truck || self.ride_for as u32 > ticks(RIDE_MS) || (best != 255 && near < 18.0) {
            Self::tap_use(&mut inp, me.last_buttons);
            return inp;
        }
        let goal = if best != 255 {
            w.fighters[best as usize].body.pos
        } else {
            self.roam_goal(w, v.pos, 1)
        };
        let to = goal - v.pos;
        let want = fmath::atan2(to.z, to.x) * (1.0 / core::f32::consts::TAU);
        let mut diff = want - v.yaw as f32 * (1.0 / 65536.0);
        diff -= fmath::round(diff);
        // Stuck on something: back up a second, turning the other way.
        self.stuck = if v.vel.flat().len() < 1.0 { self.stuck + 1 } else { 0 };
        if self.stuck as u32 > TICK_HZ / 2 {
            self.backing = TICK_HZ as u16;
            self.stuck = 0;
        }
        let steer = (diff * 6.0).clamp(-1.0, 1.0) * 127.0;
        if self.backing > 0 {
            self.backing -= 1;
            inp.mx = -steer as i8;
            inp.mz = -127;
        } else {
            inp.mx = steer as i8;
            inp.mz = if fmath::fabs(diff) > 0.3 { 50 } else { 127 };
        }
        inp.yaw = v.yaw;
        inp.pitch = 0;
        inp
    }

    fn unit(&mut self) -> f32 {
        self.rng.next_u32() as f32 * (1.0 / 4_294_967_296.0)
    }

    /// The fighter it is fighting, if any (read by `tools/aim`'s hand).
    pub fn target(&self) -> Option<usize> {
        ((self.target as usize) < crate::consts::MAX_FIGHTERS).then_some(self.target as usize)
    }

    /// This tick's input for the bot in `slot`.
    pub fn think(&mut self, w: &World, slot: usize) -> Input {
        let mut inp = self.decide(w, slot);
        mind_the_edge(w, slot, &mut inp);
        inp
    }

    /// Looks for the nearest enemy it can see (one well camouflaged only
    /// near; an attacker with a flag to go for lets the far ones be), and
    /// in Siege with none, the nearest enemy minion in sight. The target,
    /// and how long it has had it.
    fn look(&mut self, w: &World, slot: usize, eye: V3) {
        let me = &w.fighters[slot];
        let objective = attacker(slot) && Self::objective(w, slot).is_some();
        let (sn, cs) = crate::geom::yaw_sincos(me.yaw);
        let hurt = me.since_hit < ticks(BOT_HURT_MS);
        let mut best = 255u8;
        let mut near = f32::INFINITY;
        for (q, v) in w.fighters.iter().enumerate() {
            if q == slot || !v.active() || (w.rules.teams && v.team == me.team) {
                continue;
            }
            let chest = v.body.pos + V3::new(0.0, 1.2, 0.0);
            let d = (chest - eye).len();
            if objective && d > OBJECTIVE_IGNORE_M {
                continue;
            }
            let to = (chest - eye).flat();
            let ahead = to.x * cs + to.z * sn >= BOT_VIEW_COS * to.len();
            let noticed = q as u8 == self.target || hurt || d < BOT_HEAR_M || ahead;
            let unseen = v.cloak() > CAMO_BOT_HIDDEN && d > CAMO_BOT_M;
            if d < near && noticed && !unseen && w.map.clear(eye, chest) {
                near = d;
                best = q as u8;
            }
        }
        self.minion = 255;
        if best == 255 && w.rules.siege {
            for (i, m) in w.minions.iter().enumerate() {
                if !m.alive || m.team == me.team & 1 {
                    continue;
                }
                let chest = m.body.pos + V3::new(0.0, 0.9, 0.0);
                let d = (chest - eye).len();
                if d < near && d < MINION_RANGE * 1.5 && w.map.clear(eye, chest) {
                    near = d;
                    self.minion = i as u8;
                }
            }
        }
        if best != self.target {
            self.target = best;
            self.seen = 0;
            // A new target: its aim has caught none of its going yet.
            self.caught = V3::ZERO;
        } else {
            self.seen = self.seen.saturating_add(1);
        }
    }

    /// Between looks: the target it had, while it stands (and is still an
    /// enemy); the minion it had, while it lives.
    fn keep(&mut self, w: &World, slot: usize) {
        let me = &w.fighters[slot];
        if self.target != 255 {
            let v = &w.fighters[self.target as usize];
            if !v.active() || (w.rules.teams && v.team == me.team) {
                self.target = 255;
                self.seen = 0;
            } else {
                self.seen = self.seen.saturating_add(1);
            }
        }
        if self.minion != 255 {
            let m = &w.minions[self.minion as usize];
            if !m.alive || m.team == me.team & 1 {
                self.minion = 255;
            }
        }
    }

    /// Decides where to walk with nobody in sight: a gun it wants (not on
    /// a detour), else its flag business, else a free vehicle near if it
    /// may, else the roam goal, by the waypoints on a map that has them.
    fn plan(&mut self, w: &World, slot: usize, detour: f32) {
        let me = &w.fighters[slot];
        if detour == 0.0 {
            if let Some(at) = Self::loot(w, slot) {
                self.kind = Steer::Loot;
                self.steer = at;
                return;
            }
        }
        if let Some(at) = Self::objective(w, slot) {
            self.kind = Steer::Objective;
            self.steer = Self::toward_by(w, me.body.pos, at, self.round > 0);
            return;
        }
        let free = (self.ride_cool == 0 && me.riding().is_none())
            .then(|| {
                w.vehicles
                    .iter()
                    .filter(|v| v.active && v.kind == crate::vehicle::TRUCK && v.seats[crate::vehicle::DRIVER] == 0)
                    .map(|v| (v.pos, (v.pos - me.body.pos).flat().len()))
                    .filter(|&(_, d)| d < VEHICLE_WALK)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
            })
            .flatten();
        match free {
            Some((at, _)) => {
                self.kind = Steer::Vehicle;
                self.steer = at;
            }
            None => {
                let g = self.roam_goal(w, me.body.pos, DECIDE_EVERY as u16);
                // On a map with waypoints, the way there rather than
                // straight at it.
                self.steer = match &w.map.nav {
                    Some(nav) if !w.map.roam.is_empty() => nav
                        .toward(
                            &w.map,
                            self.goal as usize % w.map.roam.len(),
                            me.body.pos,
                            &mut self.way,
                        )
                        .unwrap_or(g),
                    _ => g,
                };
                self.kind = Steer::Roam;
            }
        }
    }

    fn decide(&mut self, w: &World, slot: usize) -> Input {
        let me = &w.fighters[slot];
        let mut inp = Input {
            yaw: me.yaw,
            pitch: me.pitch,
            ..Input::default()
        };
        if !me.active() || w.phase != Phase::Playing {
            self.ride_for = 0;
            // Back from the dead, whoever it sees it sees anew, and it
            // decides anew where to go.
            self.target = 255;
            self.minion = 255;
            self.seen = 0;
            self.kind = Steer::None;
            return inp;
        }
        self.ride_cool = self.ride_cool.saturating_sub(1);
        let eye = me.eye();
        // Its tick to look around, the bots staggered.
        let look = (w.tick as usize + slot).is_multiple_of(DECIDE_EVERY);
        if look {
            self.look(w, slot, eye);
        } else {
            self.keep(w, slot);
        }
        let best = self.target;
        let minion_at = (self.minion != 255).then(|| w.minions[self.minion as usize].body.pos + V3::new(0.0, 0.9, 0.0));
        let near = if best != 255 {
            (w.fighters[best as usize].body.pos + V3::new(0.0, 1.2, 0.0) - eye).len()
        } else if let Some(chest) = minion_at {
            (chest - eye).len()
        } else {
            f32::INFINITY
        };

        match me.riding() {
            Some((vi, crate::vehicle::DRIVER)) => return self.drive(w, slot, vi, best, near, inp),
            Some(_) => {
                // A passenger fights as usual, and gets out after a while.
                self.ride_for = self.ride_for.saturating_add(1);
                if self.ride_for as u32 > ticks(RIDE_MS) {
                    Self::tap_use(&mut inp, me.last_buttons);
                    return inp;
                }
            }
            None => {
                if self.ride_for > 0 {
                    self.ride_for = 0;
                    self.ride_cool = ticks(RIDE_COOL_MS) as u16;
                }
            }
        }

        // The enemy's flag in reach, on foot: take it (USE, Halo 3's).
        if !me.flag && me.riding().is_none() && Self::flag_in_reach(w, slot) {
            Self::tap_use(&mut inp, me.last_buttons);
        }

        // A carrier can't shoot: it runs home, swinging at whoever is in
        // its way.
        if me.flag && me.riding().is_none() {
            if let Some(home) = Self::objective(w, slot) {
                if look || self.kind != Steer::Carry {
                    self.kind = Steer::Carry;
                    self.steer = Self::toward(w, me.body.pos, home);
                }
                let to = self.steer - me.body.pos;
                inp.yaw = rad_to_yaw(fmath::atan2(to.z, to.x));
                inp.pitch = 0;
                inp.mz = 127;
                if near < MELEE_RANGE {
                    inp.buttons |= MELEE;
                }
                if self.unit() < 0.01 {
                    inp.buttons |= JUMP;
                }
                return inp;
            }
        }

        if self.strafe_for == 0 {
            self.strafe = if self.unit() < 0.5 { -1 } else { 1 };
            self.strafe_for = (TICK_HZ / 2 + self.rng.next_bounded(TICK_HZ)) as u16;
        }
        self.strafe_for -= 1;
        let rough = 1.0 - self.skill;
        self.wob.0 = self.wob.0 * 0.98 + (self.unit() - 0.5) * AIM_WOB * rough;
        self.wob.1 = self.wob.1 * 0.98 + (self.unit() - 0.5) * AIM_WOB * 0.5 * rough;

        let g = me.gun();
        let other = &me.guns[(me.cur as usize & 1) ^ 1];
        if g.arm != NONE && g.mag == 0 {
            // Nothing left to load: to the other gun, if it has rounds.
            if g.reserve == 0 && other.arm != NONE && !other.spent() {
                if me.last_buttons & SWAP == 0 {
                    inp.buttons |= SWAP;
                }
            } else {
                inp.buttons |= RELOAD;
            }
        }
        if self.unit() < 0.004 {
            inp.buttons |= JUMP;
        }

        if best == 255 && minion_at.is_none() {
            // Pushing at a wall and going nowhere (a corner between it and
            // its goal, on a map it walks straight on): a second along the
            // wall, then another goal.
            if me.body.grounded && me.body.vel.flat().len() < 0.5 {
                self.foot_stuck = self.foot_stuck.saturating_add(1);
            } else {
                self.foot_stuck = 0;
            }
            if self.foot_stuck as u32 > TICK_HZ / 2 {
                self.foot_stuck = 0;
                self.detour = TICK_HZ as u16;
                self.goal_for = 0;
                self.strafe = -self.strafe;
                self.round = (3 * TICK_HZ) as u16;
            }
            self.round = self.round.saturating_sub(1);
            let detour = if self.detour > 0 {
                self.detour -= 1;
                self.strafe as f32 * core::f32::consts::FRAC_PI_2
            } else {
                0.0
            };
            // Where to, decided at a look, or now if it has nothing in
            // hand (out of a fight).
            if look || !matches!(self.kind, Steer::Loot | Steer::Objective | Steer::Vehicle | Steer::Roam) {
                self.plan(w, slot, detour);
            }
            let to = self.steer - me.body.pos;
            match self.kind {
                // Nobody in sight: to a gun it wants, taken in reach.
                Steer::Loot => {
                    inp.yaw = rad_to_yaw(fmath::atan2(to.z, to.x));
                    inp.pitch = 0;
                    inp.mz = 127;
                    if to.flat().len() < PAD_REACH * 0.7 {
                        Self::tap_use(&mut inp, me.last_buttons);
                    }
                }
                // Capture the Flag: to its flag business.
                Steer::Objective => {
                    inp.yaw = rad_to_yaw(fmath::atan2(to.z, to.x) + detour);
                    inp.pitch = 0;
                    inp.mz = 127;
                }
                // Else to a free vehicle near if it may, else the roam goal.
                _ => {
                    inp.yaw = rad_to_yaw(fmath::atan2(to.z, to.x) + detour);
                    inp.pitch = 0;
                    inp.mz = 127;
                    inp.mx = self.strafe * 40;
                    if self.kind == Steer::Vehicle {
                        // Close to it, straight at it (no strafe to slide
                        // along a solid hull), and in once within reach: a
                        // bumper's is farther from the middle than a door's.
                        let d = to.flat().len();
                        if d < ENTER_R + 1.5 {
                            inp.mx = 0;
                            if d < ENTER_R - 0.3 {
                                Self::tap_use(&mut inp, me.last_buttons);
                            }
                        }
                    } else {
                        inp.buttons |= USE;
                    }
                }
            }
            return inp;
        }

        let aim_at = match minion_at {
            Some(chest) => chest,
            None => {
                let v = &w.fighters[best as usize];
                // A person's aim follows a target's going, and a change in it (a
                // strafe turned) throws the aim off until it has caught up: it is
                // aimed where the target would be, had it gone on as it was.
                self.caught = self.caught + (v.body.vel - self.caught) * (DT / AIM_CATCH_S);
                let trail = AIM_TRAIL_S * (1.0 - self.skill);
                let off = (v.body.vel - self.caught) * trail;
                v.body.pos - off + V3::new(0.0, if self.skill > 0.7 { 1.55 } else { 1.2 }, 0.0)
            }
        };
        let to = aim_at - eye;
        let want_yaw = fmath::atan2(to.z, to.x) * (1.0 / core::f32::consts::TAU) + self.wob.0;
        let flat = (to.x * to.x + to.z * to.z).sqrt();
        let want_pitch = fmath::atan2(to.y, flat) * (1.0 / core::f32::consts::TAU) + self.wob.1;
        // Turn toward it at a human rate, not in one tick.
        let cur_yaw = me.yaw as f32 * (1.0 / 65536.0);
        let mut dy = want_yaw - cur_yaw;
        dy -= fmath::round(dy);
        let max_turn = (AIM_TURN + AIM_TURN_SKILL * self.skill) * DT;
        let ty = dy.clamp(-max_turn, max_turn);
        inp.yaw = rad_to_yaw((cur_yaw + ty) * core::f32::consts::TAU);
        let cur_pitch = me.pitch as f32 * (1.0 / 65536.0);
        let tp = (want_pitch - cur_pitch).clamp(-max_turn, max_turn);
        inp.pitch = fmath::round((cur_pitch + tp) * 65536.0).clamp(-PITCH_MAX as f32, PITCH_MAX as f32) as i16;

        // Keep a fighting distance and strafe; with a sword, close in. An
        // attacker in Capture the Flag fights on the move to the flag (the
        // way there decided at a look). A gunner fights with its turret's
        // gun.
        let a = crate::vehicle::turret(me.gunning(&w.map)).map_or(arm(g.arm), |t| t.0);
        if look || !matches!(self.kind, Steer::Push | Steer::Stand) {
            let push = if attacker(slot) { Self::objective(w, slot) } else { None };
            match push {
                Some(at) => {
                    self.kind = Steer::Push;
                    self.steer = Self::toward(w, me.body.pos, at);
                }
                None => self.kind = Steer::Stand,
            }
        }
        match (self.kind == Steer::Push)
            .then_some(self.steer)
            .map(|at| (at - me.body.pos).flat())
            .filter(|to| to.len() > 0.5)
        {
            Some(to) => {
                let (sn, cs) = crate::geom::yaw_sincos(inp.yaw);
                let d = to * (1.0 / to.len());
                inp.mz = ((cs * d.x + sn * d.z) * 127.0) as i8;
                inp.mx = ((-sn * d.x + cs * d.z) * 127.0) as i8;
            }
            None => {
                inp.mz = if a.kind == crate::arms::Kind::Sword || near > 14.0 {
                    127
                } else if near < 6.0 {
                    -100
                } else {
                    0
                };
                inp.mx = self.strafe * 127;
            }
        }

        let react = (TICK_HZ as f32 * (0.6 - 0.35 * self.skill)) as u16;
        let on = fmath::fabs(dy) < 0.006 + 0.01 * rough && self.seen > react;
        // A spread of pellets is fired where it kills, not to tickle.
        let reach = if a.pellets > 1 {
            a.falloff * SPREAD_REACH
        } else {
            a.range
        };
        if on && near < reach {
            // A semi-automatic needs the trigger let go between pulls.
            if a.auto || me.last_buttons & FIRE == 0 {
                inp.buttons |= FIRE;
            }
            // A gun in each hand: both triggers.
            let l = arm(me.left.arm);
            if me.dual() && (l.auto || me.last_buttons & FIRE_LEFT == 0) {
                inp.buttons |= FIRE_LEFT;
            }
        }
        if near < MELEE_RANGE {
            inp.buttons |= MELEE;
        }
        if on && near > 8.0 && near < 20.0 && self.unit() < 0.003 {
            inp.buttons |= GRENADE;
        }
        inp
    }
}

/// On a map with a void, on foot: never walk or jump over the edge. The way
/// the stick points is looked along [`EDGE_LOOK`]; over the void it turns
/// to the nearest way that holds ([`EDGE_TURNS`]), or lets go. No jump
/// with the edge within [`EDGE_JUMP`] the way the stick points or the way
/// the body is going; on foot or in the air headed over it ([`EDGE_LOOK`],
/// [`EDGE_AIR`]), the stick pulls back.
fn mind_the_edge(w: &World, slot: usize, inp: &mut Input) {
    let me = &w.fighters[slot];
    if w.map.void == f32::NEG_INFINITY || !me.active() || me.riding().is_some() {
        return;
    }
    let p = me.body.pos;
    let over = |dx: f32, dz: f32, look: &[f32]| look.iter().any(|&d| w.map.over_void(p.x + dx * d, p.z + dz * d, p.y));
    // The way holds nobody: the void in a stride, or past a ledge it would
    // run off ([`EDGE_DROP`]).
    let drop = |dx: f32, dz: f32| {
        EDGE_LOOK
            .iter()
            .any(|&d| w.map.floor(p.x + dx * d, p.z + dz * d, 0.0, p.y + STEP) < p.y - EDGE_DROP)
    };
    let lost = |dx: f32, dz: f32| over(dx, dz, &EDGE_LOOK) || (drop(dx, dz) && over(dx, dz, &EDGE_JUMP));
    let (sn, cs) = crate::geom::yaw_sincos(inp.yaw);
    // The stick in the world's x and z, as movement turns it.
    let fwd = inp.mz as f32 * (1.0 / 127.0);
    let side = inp.mx as f32 * (1.0 / 127.0);
    let (wx, wz) = (cs * fwd - sn * side, sn * fwd + cs * side);
    let len = (wx * wx + wz * wz).sqrt();
    let set = |inp: &mut Input, dx: f32, dz: f32, k: f32| {
        inp.mz = ((cs * dx + sn * dz) * k * 127.0) as i8;
        inp.mx = ((-sn * dx + cs * dz) * k * 127.0) as i8;
    };
    let v = me.body.vel.flat();
    let speed = v.len();
    let going_over = |look: &[f32]| speed > 0.5 && over(v.x / speed, v.z / speed, look);
    if !me.body.grounded {
        if going_over(&EDGE_AIR) {
            set(inp, -v.x / speed, -v.z / speed, 1.0);
        }
        return;
    }
    // Carried over it (a landing, a blast's push) whatever the stick says:
    // the stick pulls back against the way it is going.
    if going_over(&EDGE_LOOK) || (speed > 0.5 && lost(v.x / speed, v.z / speed)) {
        inp.buttons &= !JUMP;
        set(inp, -v.x / speed, -v.z / speed, 1.0);
        return;
    }
    if len < 0.01 {
        inp.buttons &= !JUMP;
        return;
    }
    let (ux, uz) = (wx / len, wz / len);
    if inp.held(JUMP) && (over(ux, uz, &EDGE_JUMP) || going_over(&EDGE_JUMP)) {
        inp.buttons &= !JUMP;
    }
    if !lost(ux, uz) {
        return;
    }
    inp.buttons &= !JUMP;
    for (s, c) in EDGE_TURNS {
        let (dx, dz) = (ux * c - uz * s, ux * s + uz * c);
        if !lost(dx, dz) {
            set(inp, dx, dz, len.min(1.0));
            return;
        }
    }
    inp.mz = 0;
    inp.mx = 0;
}

/// Capture the Flag: whether the bot in `slot` attacks or defends. Seats
/// alternate sides (`slot & 1`), so the role alternates within a side.
fn attacker(slot: usize) -> bool {
    (slot / 2).is_multiple_of(2)
}

/// A whole bot match, for the determinism gate: four bots, red against
/// blue, a high score limit so it never ends, each seat checked a different
/// depth back to exercise the rewind. Returns the world's hash, folded over
/// every tick, so any tick that differs shows.
pub fn bot_match(seed: u64, ticks_n: u32) -> u64 {
    bot_match_on(0, 0, seed, ticks_n)
}

/// [`bot_match`] on map `map` in `mode`: 0 Doubles, 1 an eight-seat Gun
/// Game, 2 a sixteen-seat Big Team, 3 Capture the Flag (on a map with
/// flags), 4 Siege (on a map with nodes), 5 Big Team with red's one-seaters
/// flown ([`pilots_match`], on a map with a skimmer and a flier). What
/// `ci/parity.sh` runs on every map.
/// 0 if there is no such map or mode.
pub fn bot_match_on(map: u8, mode: u8, seed: u64, ticks_n: u32) -> u64 {
    let Some(m) = crate::map::by_id(map) else {
        return 0;
    };
    if mode == 5 {
        return pilots_match(m, seed, ticks_n);
    }
    let base = match mode {
        0 => Rules::doubles(),
        1 => Rules::gun_game(),
        2 => Rules::big_team(),
        3 if m.flags.is_some() => Rules::ctf(),
        4 if m.siege.is_some() => Rules::siege(),
        _ => return 0,
    };
    let rules = Rules {
        score_limit: 100_000,
        time_limit_ms: 0,
        ..base
    };
    let n = rules.seats as usize;
    let salt = (crate::rng::splitmix64(seed), crate::rng::splitmix64(seed ^ 0x5a17));
    let mut w = World::new(m, rules, seed, salt);
    let mut bots = [Bot::new(seed, 0, 0.5); MAX_FIGHTERS];
    for (s, b) in bots.iter_mut().enumerate().take(n) {
        w.seat(s, (s & 1) as u8);
        *b = Bot::new(seed, s, 0.3 + 0.2 * (s % 4) as f32);
    }
    // Each seat its own rewind depth, as the server would choose them.
    let pattern = [0, 3 << 8, 7 << 8 | 128, 12 << 8, 0, 5 << 8, 1 << 8, 9 << 8];
    let mut depth = [0u32; MAX_FIGHTERS];
    for (s, d) in depth.iter_mut().enumerate() {
        *d = pattern[s % pattern.len()];
    }
    let mut fold = 0u64;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    for _ in 0..ticks_n {
        for s in 0..n {
            inputs[s] = bots[s].think(&w, s);
        }
        w.step(&inputs, &depth);
        fold = crate::rng::splitmix64(fold ^ w.hash());
    }
    fold
}

/// Seats flying red's skimmer and flier in [`pilots_match`].
const PILOTS: [(usize, u8); 2] = [(0, crate::vehicle::SKIMMER), (2, crate::vehicle::FLIER)];

/// Sixteen seats of Big Team on `m`, two of red's at the controls of its
/// skimmer and its flier on a shaky scripted stick (the aim wandering,
/// throttle and strafe, boosts, rolls, both triggers), the rest bots: the
/// one-seaters' flight in the parity gate, which bots never fly. Folded as
/// [`bot_match`]'s. 0 on a map without both.
fn pilots_match(m: crate::map::Map, seed: u64, ticks_n: u32) -> u64 {
    let rules = Rules {
        score_limit: 100_000,
        time_limit_ms: 0,
        ..Rules::big_team()
    };
    let n = rules.seats as usize;
    let salt = (crate::rng::splitmix64(seed), crate::rng::splitmix64(seed ^ 0x5a17));
    let mut w = World::new(m, rules, seed, salt);
    let mut bots = [Bot::new(seed, 0, 0.5); MAX_FIGHTERS];
    for (s, b) in bots.iter_mut().enumerate().take(n) {
        w.seat(s, (s & 1) as u8);
        *b = Bot::new(seed, s, 0.3 + 0.2 * (s % 4) as f32);
    }
    let mut craft = [0usize; 2];
    for (k, &(_, kind)) in PILOTS.iter().enumerate() {
        let Some(i) = (0..MAX_VEHICLES).find(|&i| {
            let v = &w.vehicles[i];
            v.active && v.kind == kind && w.map.vehicles.get(i).is_some_and(|d| d.pos.x < 0.0)
        }) else {
            return 0;
        };
        craft[k] = i;
    }
    let mut sticks = [Pcg32::new(seed, 11), Pcg32::new(seed, 12)];
    let mut aims = [(0u16, 0i16); 2];
    let mut fold = 0u64;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    for _ in 0..ticks_n {
        for s in 0..n {
            inputs[s] = bots[s].think(&w, s);
        }
        for (k, &(s, _)) in PILOTS.iter().enumerate() {
            let f = w.fighters[s];
            let v = w.vehicles[craft[k]];
            if f.riding().is_none() {
                // Once the countdown is over, alive and its craft free and
                // whole: stood beside it, and in.
                if w.phase == Phase::Playing && f.active() && v.active && v.seats[crate::vehicle::DRIVER] == 0 {
                    let (_, r) = v.axes();
                    let at = v.pos + r * (v.spec().half_w + 0.5);
                    let b = &mut w.fighters[s].body;
                    b.pos = V3::new(at.x, w.map.floor(at.x, at.z, BODY_R, v.pos.y + 1.0), at.z);
                    b.vel = V3::ZERO;
                    aims[k] = (v.yaw, 0);
                    inputs[s] = Input {
                        buttons: if f.last_buttons & USE == 0 { USE } else { 0 },
                        yaw: v.yaw,
                        ..Input::default()
                    };
                }
                continue;
            }
            let r = sticks[k].next_u32();
            let (yaw, pitch) = &mut aims[k];
            *yaw = yaw.wrapping_add(((r & 0x1ff) as i32 - 0xff) as u16);
            *pitch = (*pitch as i32 + ((r >> 9) & 0x7f) as i32 - 0x3f).clamp(-3_000, 6_000) as i16;
            inputs[s] = Input {
                buttons: ((r >> 18) as u16) & (CROUCH | JUMP | FIRE | FIRE_LEFT),
                yaw: *yaw,
                pitch: *pitch,
                mx: ((r >> 24) as u8 as i8).max(-127),
                mz: if (r >> 16).is_multiple_of(7) { -127 } else { 127 },
            };
        }
        w.step(&inputs, &[0; MAX_FIGHTERS]);
        fold = crate::rng::splitmix64(fold ^ w.hash());
    }
    fold
}

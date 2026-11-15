//! The AI fighter that reads the true world, for the server's empty seats
//! and for the determinism gate. Deterministic: its choices come from the
//! world and its own PCG stream.
//!
//! A side plays as one: every bot of it makes the same plan of the side
//! ([`plan`], as if its captain made it for all) on the same tick from the
//! same world, and takes its own job from it: in Capture the Flag who goes
//! for the enemy's flag, who carries it home and who keeps by the carrier,
//! who runs down the enemy carrying ours, who holds home and who stands
//! over our flag where it lies; anywhere with vehicles, who drives which,
//! who rides with them and where the driver takes them. Each finds its way
//! by the map's waypoints ([`crate::nav`]), on foot or at the wheel.
//!
//! It looks around (the rays: who it can see, a gun to take, the way) only
//! every [`DECIDE_EVERY`] ticks, the bots staggered so they never all look
//! on one tick; between looks it aims, fires and walks on what it decided,
//! and minds the edge every tick.

use crate::arms::{arm, NONE};
use crate::consts::*;
use crate::fighter::{Fighter, Gun};
use crate::fmath;
use crate::geom::{rad_to_yaw, V3};
use crate::input::*;
use crate::nav::Nav;
use crate::rng::Pcg32;
use crate::vehicle::{DRIVER, FLIER, PASSENGER, SKIMMER, TRUCK};
use crate::world::{Phase, Rules, World, FLAG_DOWN, FLAG_HELD, FLAG_HOME};

/// What its side's plan asks of a bot ([`plan`]). Seats and vehicles by
/// their slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    /// Nothing asked: its own way (a gun to take, its roam point; Siege's
    /// objective by its seat).
    Free,
    /// Take the enemy's flag, at home or where it lies.
    Attack,
    /// Carrying it: home, by the vehicle coming for it if one is
    /// ([`Trip::Fetch`]: its slot, or 255).
    Carry(u8),
    /// Keep by the teammate carrying the enemy's flag.
    Escort(u8),
    /// Run down the enemy carrying ours.
    Hunt(u8),
    /// Hold post `k`: about our flag's base while it is home, about it
    /// where it lies while it is down (nobody takes it home by hand).
    Guard(u8),
    /// Get into this vehicle's seat ([`DRIVER`] or [`PASSENGER`]).
    Board(u8, u8),
    /// At this vehicle's controls, on this trip; `true`: waiting for its
    /// crew to get in.
    Drive(u8, Trip, bool),
    /// In this vehicle's passenger seat, fighting from it.
    Ride(u8),
}

/// Where a driver takes its vehicle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trip {
    /// After whoever it sees, else about the map.
    Roam,
    /// To the enemy's flag, as near as it goes, to wait there for its
    /// crew to bring it back.
    Attack,
    /// To the teammate carrying the flag, to take it aboard.
    Fetch(u8),
    /// The carrier aboard: home.
    Home,
    /// After the enemy carrying our flag.
    Hunt(u8),
}

/// Where a bot walks: a spot of the map's ([`Nav`]'s, its field known), or
/// a point anywhere; how near is there; whether it fights on the way
/// there rather than standing to.
#[derive(Clone, Copy)]
struct Goal {
    at: V3,
    spot: Option<usize>,
    arrive: f32,
    urgent: bool,
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
    /// The map's roam point it walks to with nothing else to do, and ticks
    /// left before it gives up on it.
    goal: u8,
    goal_for: u16,
    /// Its job, and the tick its side's plan gave it ([`NEVER`]: none yet).
    job: Job,
    job_tick: u32,
    /// Where it decided to walk (or drive) at its last look.
    steer: V3,
    /// The waypoints it was nearest at its last look, on foot and at the
    /// wheel (`u16::MAX`: none).
    way: u16,
    wheel_way: u16,
    /// Holding a post: how far its look has swept.
    scan: u16,
    /// Its goal is far yet: on its way there, the far enemies let be.
    far: bool,
    /// Ticks before it may get into a vehicle again after getting out (or
    /// giving up getting into one), and ticks it has been on its way to
    /// one.
    ride_cool: u16,
    boarding: u16,
    /// At the wheel: ticks it has barely moved, ticks left backing up, how
    /// many times it has backed up lately, ticks parked where it was going,
    /// ticks waiting for its crew.
    stuck: u16,
    backing: u16,
    jams: u8,
    parked: u16,
    waited: u16,
    /// At the wheel: where it was at the last check of its progress, and
    /// ticks till the next; whether anyone has ridden with it.
    was: V3,
    was_for: u16,
    crewed: bool,
    /// On foot: ticks it has pushed at a wall going nowhere, and ticks
    /// left walking along it instead.
    foot_stuck: u16,
    detour: u16,
}

/// No tick yet.
const NEVER: u32 = u32::MAX;
/// Every how many ticks a bot looks around afresh. 31 ms: it still reacts
/// like a person.
const DECIDE_EVERY: usize = 4;
/// Every how many ticks its side's plan is made afresh: on the same tick
/// for all of it, so they agree.
const PLAN_EVERY: u32 = 32;
/// How long a bot walks at one roam point before it picks another.
const GOAL_MS: u32 = 8000;
/// Close enough to a roam point to pick the next.
const GOAL_NEAR: f32 = 2.0;
/// How far a bot walks to a free vehicle to crew it, and how far a driver
/// goes for the carrier.
const CREW_M: f32 = 35.0;
const FETCH_M: f32 = 70.0;
/// A carrier this near the vehicle coming for it walks the rest.
const FETCH_WAIT_M: f32 = 25.0;
/// A vehicle this near the enemy's flag waits there for its crew to come
/// back with it; nobody else takes it.
const AT_FLAG_M: f32 = 45.0;
/// Keeping by the carrier: how many, and how near.
const ESCORTS: usize = 2;
const ESCORT_M: f32 = 4.0;
/// Holding home: every how many seats of a side; on a Big Team map, where
/// the vehicles and the long way home are the attack's.
const DEFEND_EVERY: usize = 4;
const DEFEND_EVERY_BIG: usize = 8;
/// Standing over our flag where it lies: how many, and how far about it.
const GUARDS: usize = 2;
const GUARD_RING: f32 = 3.0;
/// The posts about home: roam points this near the base, and no nearer
/// than this.
const POST_M: f32 = 40.0;
const POST_OFF_M: f32 = 4.0;
/// After getting out, ticks before getting in again; on the way to get in
/// longer than this, it gives up for that long.
const RIDE_COOL_MS: u32 = 4000;
const BOARD_MS: u32 = 8000;
const BOARD_COOL_MS: u32 = 10_000;
/// At the wheel: how long it waits for its crew, how long parked by the
/// enemy's flag before it goes on foot, and how long stuck before it gets
/// out.
const CREW_WAIT_MS: u32 = 10_000;
const PARK_MS: u32 = 25_000;
const JAMS: u8 = 5;
/// At the wheel, going somewhere: it has got nowhere if it has gone less
/// than this in this long.
const PROGRESS_M: f32 = 3.0;
const PROGRESS_MS: u32 = 2500;
/// At the wheel: how near where it is going it slows, and parks.
const SLOW_M: f32 = 18.0;
const PARK_M: f32 = 6.0;
/// A rider by the enemy's flag gets out to go for it when the vehicle has
/// stopped this near it; a carrier aboard, as near home.
const DROP_M: f32 = 40.0;
/// A hull this broken is left.
const BAIL_HEALTH: i32 = 4;
/// How far a bot goes out of its way for a gun it wants, and how far above
/// or below it may be: on its own level, as it walks there straight.
const LOOT_M: f32 = 20.0;
const LOOT_RISE: f32 = 0.6;
/// A bot on its way somewhere that matters lets enemies farther than this
/// be, unless they shoot it or it is this near where it is going.
const OBJECTIVE_IGNORE_M: f32 = 30.0;
const NEAR_GOAL_M: f32 = 45.0;
/// A one-seater's pilot fights what is this near, and flies this high
/// over the ground.
const PILOT_FIGHT_M: f32 = 55.0;
const FLY_HIGH: f32 = 14.0;
/// Holding a post: how far its look sweeps to each side, in turns, and
/// how fast.
const SCAN_ARC: f32 = 0.11;
const SCAN_RATE: f32 = 0.012;
/// Its aim: how many turns a second it may turn (and more with skill), the
/// wobble's step in turns, and how far behind a target's change of going
/// it is thrown (seconds of it, all of it at no skill) and how long it
/// takes to catch up.
const AIM_TURN: f32 = 0.6;
const AIM_TURN_SKILL: f32 = 1.4;
const AIM_WOB: f32 = 0.004;
const AIM_TRAIL_S: f32 = 0.3;
const AIM_CATCH_S: f32 = 0.25;
/// Fighting on its way: how much it weaves across the way it goes.
const WEAVE: f32 = 0.6;
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

/// Its side's plan: every seat's job ([`Job`]), the side's bots and people
/// alike (a person's is what it would be asked, and they do as they
/// please). A pure function of the world, so every bot of the side makes
/// the same one.
pub fn plan(w: &World, side: usize) -> [Job; MAX_FIGHTERS] {
    let mut jobs = [Job::Free; MAX_FIGHTERS];
    let mut free = [false; MAX_FIGHTERS];
    for (s, f) in w.fighters.iter().enumerate() {
        free[s] = f.active() && (f.team as usize & 1) == side;
    }
    let ctf = w.rules.ctf && w.rules.teams && w.map.flags.is_some();
    let held = |t: usize| (ctf && w.flags[t].state == FLAG_HELD).then_some(w.flags[t].carrier as usize);
    // Ours carrying theirs; theirs carrying ours.
    let carrier = held(side ^ 1).filter(|&c| w.fighters[c].active());
    let thief = held(side).filter(|&c| w.fighters[c].active());
    let theirs_at = w.map.flags.map(|b| b[side ^ 1]);

    // Whoever rides stays in: drivers drive, riders ride.
    let mut fetching = 255u8;
    for (i, v) in w.vehicles.iter().enumerate() {
        if !v.active {
            continue;
        }
        let seat = |k: usize| v.seats[k].checked_sub(1).map(usize::from).filter(|&s| free[s]);
        let rider = seat(PASSENGER);
        if let Some(d) = seat(DRIVER) {
            free[d] = false;
            let trip = if !ctf {
                Trip::Roam
            } else if v.kind != TRUCK {
                // A pilot can't carry the flag: one-seaters fight.
                thief.map_or(Trip::Roam, |e| Trip::Hunt(e as u8))
            } else if rider.is_some() && rider == carrier {
                Trip::Home
            } else if let Some(c) = carrier.filter(|&c| {
                let f = &w.fighters[c];
                f.riding().is_none()
                    && rider.is_none()
                    && v.kind == TRUCK
                    && (f.body.pos - v.pos).flat().len() < FETCH_M
            }) {
                if fetching == 255 {
                    fetching = i as u8;
                }
                Trip::Fetch(c as u8)
            } else if let Some(e) = thief {
                Trip::Hunt(e as u8)
            } else if carrier.is_some() {
                Trip::Roam
            } else {
                Trip::Attack
            };
            jobs[d] = Job::Drive(i as u8, trip, false);
        }
        if let Some(p) = rider {
            free[p] = false;
            jobs[p] = if Some(p) == carrier {
                Job::Carry(i as u8)
            } else {
                Job::Ride(i as u8)
            };
        }
    }
    if let Some(c) = carrier.filter(|&c| free[c]) {
        free[c] = false;
        jobs[c] = Job::Carry(fetching);
    }
    let left = |free: &[bool; MAX_FIGHTERS]| free.iter().filter(|&&f| f).count();
    // The enemy carrying ours: all but two after them.
    if let Some(e) = thief {
        let at = w.fighters[e].body.pos;
        for _ in 0..left(&free).saturating_sub(2).max(2) {
            if let Some(s) = nearest_free(w, &free, at, f32::INFINITY) {
                free[s] = false;
                jobs[s] = Job::Hunt(e as u8);
            }
        }
    }
    if let Some(c) = carrier {
        let at = w.fighters[c].body.pos;
        for _ in 0..ESCORTS {
            if let Some(s) = nearest_free(w, &free, at, f32::INFINITY) {
                free[s] = false;
                jobs[s] = Job::Escort(c as u8);
            }
        }
    }
    if ctf {
        let ours = &w.flags[side];
        // Our flag on the floor: stood over till it goes home.
        if ours.state == FLAG_DOWN {
            for k in 0..GUARDS {
                if let Some(s) = nearest_free(w, &free, ours.pos, f32::INFINITY) {
                    free[s] = false;
                    jobs[s] = Job::Guard(k as u8);
                }
            }
        }
        // At home: a third of the side hold it, every third seat of it
        // (by seat, not by who is nearest, so a defender stays one as it
        // walks), each its own post.
        let mut ord = 0usize;
        let side_n = w
            .fighters
            .iter()
            .filter(|f| f.seated && (f.team as usize & 1) == side)
            .count();
        let every = if w.map.big() { DEFEND_EVERY_BIG } else { DEFEND_EVERY };
        for s in 0..MAX_FIGHTERS {
            let f = &w.fighters[s];
            if !f.seated || (f.team as usize & 1) != side {
                continue;
            }
            if free[s] && ours.state == FLAG_HOME && side_n >= 2 && ord.is_multiple_of(every) {
                free[s] = false;
                jobs[s] = Job::Guard((GUARDS + ord / every) as u8);
            }
            ord += 1;
        }
    }
    // Free vehicles near those left: crewed, the nearest at the wheel,
    // the next beside it; a driver alone takes a rider aboard.
    for (i, v) in w.vehicles.iter().enumerate() {
        // Nobody gets into a hull it would get straight out of.
        if !v.active || v.kind == FLIER && !flies(w) || v.health * BAIL_HEALTH < v.spec().health {
            continue;
        }
        if theirs_at.is_some_and(|t| ctf && (v.pos - t).flat().len() < AT_FLAG_M) {
            continue;
        }
        let two = v.spec().seats > 1;
        match (v.driver(), v.seats[PASSENGER]) {
            (None, 0) => {
                if let Some(d) = nearest_free(w, &free, v.pos, CREW_M) {
                    free[d] = false;
                    let p = if two {
                        nearest_free(w, &free, v.pos, CREW_M)
                    } else {
                        None
                    };
                    // A truck makes a flag run with two aboard (one to
                    // drive, one to take it and ride home with it), never
                    // one.
                    if ctf && two && p.is_none() {
                        free[d] = true;
                        continue;
                    }
                    jobs[d] = Job::Board(i as u8, DRIVER as u8);
                    if let Some(p) = p {
                        free[p] = false;
                        jobs[p] = Job::Board(i as u8, PASSENGER as u8);
                    }
                }
            }
            (Some(d), 0) if two && (w.fighters[d].team as usize & 1) == side => {
                if let Job::Drive(_, trip @ (Trip::Attack | Trip::Roam | Trip::Hunt(_)), _) = jobs[d] {
                    if let Some(p) = nearest_free(w, &free, v.pos, CREW_M) {
                        free[p] = false;
                        jobs[p] = Job::Board(i as u8, PASSENGER as u8);
                        jobs[d] = Job::Drive(i as u8, trip, true);
                    }
                }
            }
            _ => {}
        }
    }
    // The rest go for the flag; with it taken, along with it.
    if ctf {
        for s in 0..MAX_FIGHTERS {
            if free[s] {
                jobs[s] = match carrier {
                    Some(c) => Job::Escort(c as u8),
                    None => Job::Attack,
                };
            }
        }
    }
    jobs
}

/// Whether bots fly fliers here: on a map with somewhere to fly to.
fn flies(w: &World) -> bool {
    w.map.big()
}

/// The free seat nearest `at` (flat), within `within`; the lowest seat of
/// those as near.
fn nearest_free(w: &World, free: &[bool; MAX_FIGHTERS], at: V3, within: f32) -> Option<usize> {
    let mut best = None;
    let mut bd = within;
    for (s, f) in w.fighters.iter().enumerate() {
        if !free[s] {
            continue;
        }
        let d = (f.body.pos - at).flat().len();
        if d < bd {
            bd = d;
            best = Some(s);
        }
    }
    best
}

/// Turns `from` toward `want` (both in turns), at most `max` turns.
fn turn(from: f32, want: f32, max: f32) -> f32 {
    let mut d = want - from;
    d -= fmath::round(d);
    from + d.clamp(-max, max)
}

/// The yaw of the way `to` points, in turns.
fn turns(to: V3) -> f32 {
    fmath::atan2(to.z, to.x) * (1.0 / core::f32::consts::TAU)
}

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
            job: Job::Free,
            job_tick: NEVER,
            steer: V3::ZERO,
            way: u16::MAX,
            wheel_way: u16::MAX,
            scan: 0,
            far: false,
            ride_cool: 0,
            boarding: 0,
            stuck: 0,
            backing: 0,
            jams: 0,
            parked: 0,
            waited: 0,
            was: V3::ZERO,
            was_for: 0,
            crewed: false,
            foot_stuck: 0,
            detour: 0,
        }
    }

    /// The job its side's plan gave it.
    pub fn job(&self) -> Job {
        self.job
    }

    /// Where it decided to walk (or drive) at its last look: for tools.
    pub fn heading(&self) -> V3 {
        self.steer
    }

    /// The map's roam point to head for with nothing to do (picking the
    /// next when it's reached or taking too long), or with none, toward the
    /// middle, then the first pad. `elapsed`: ticks since it last asked.
    fn roam_goal(&mut self, w: &World, at: V3, elapsed: u16) -> Goal {
        let roam = &w.map.roam;
        let (at, spot) = if !roam.is_empty() {
            let here = roam[self.goal as usize % roam.len()];
            if self.goal_for == 0 || (here - at).flat().len() < GOAL_NEAR {
                self.goal = self.rng.next_bounded(roam.len() as u32) as u8;
                self.goal_for = ticks(GOAL_MS) as u16;
            }
            self.goal_for = self.goal_for.saturating_sub(elapsed);
            let g = self.goal as usize % roam.len();
            (roam[g], Some(g))
        } else if at.flat().len() > 6.0 {
            (V3::ZERO, None)
        } else {
            (w.map.pads.first().map_or(V3::ZERO, |p| p.pos), None)
        };
        Goal {
            at,
            spot,
            arrive: 0.0,
            urgent: false,
        }
    }

    /// A gun (or a power-up) it wants, near, on its level and a straight
    /// walk away: on a pad that holds it, or on the floor. Wanted: ranked
    /// over the best it carries that has rounds, or rounds for one of its
    /// own running low.
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

    /// Siege: an attacker goes for the enemy's core once it is open, else
    /// for a generator of theirs still standing, two attackers to one (one
    /// alone dies before it is overloaded); a defender holds a generator of
    /// its own (by seat; the core once they are down) and goes for whoever
    /// is overloading it. A few metres apart in a ring, each its own way.
    fn siege_goal(w: &World, slot: usize) -> Option<(V3, usize)> {
        let at = w.map.siege.filter(|_| w.rules.siege)?;
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
        Some((at[n] + spread, n))
    }

    /// Where to steer for `goal` from `at` on a map with no waypoints (a
    /// test's), and a Siege minion's way: straight if the way is clear at
    /// the waist (and not `round`), else the map's roam point in clear
    /// sight that is nearest the goal and nearer it than we are (a step
    /// around what's between).
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

    /// Where to steer on foot from `at` for `goal`: by the waypoints.
    fn path(&mut self, w: &World, at: V3, goal: &Goal) -> V3 {
        let Some(nav) = Nav::of(&w.map) else {
            return Self::toward_by(w, at, goal.at, false);
        };
        let g = &nav.foot;
        match goal.spot {
            Some(s) => g.toward(&w.map, s, at, &mut self.way).unwrap_or(goal.at),
            None => g.route(&w.map, goal.at, at, &mut self.way).unwrap_or(goal.at),
        }
    }

    /// Where to steer at the wheel from `at` for `goal`: by the trucks'
    /// waypoints, straight on a map without them.
    fn drive_path(&mut self, w: &World, at: V3, goal: &Goal) -> V3 {
        let Some(g) = Nav::of(&w.map).and_then(|n| n.wheels.as_ref()) else {
            return goal.at;
        };
        match goal.spot {
            Some(s) => g.toward(&w.map, s, at, &mut self.wheel_way).unwrap_or(goal.at),
            None => g.route(&w.map, goal.at, at, &mut self.wheel_way).unwrap_or(goal.at),
        }
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

    /// Whether its job has it going somewhere that matters, past fights
    /// on the way.
    fn focused(&self) -> bool {
        matches!(
            self.job,
            Job::Attack | Job::Carry(_) | Job::Board(..) | Job::Drive(..) | Job::Hunt(_)
        )
    }

    /// Looks for the nearest enemy it can see (one well camouflaged only
    /// near; on its way somewhere that matters, the far ones let be; the
    /// one it hunts before others), and in Siege with none, the nearest
    /// enemy minion in sight. The target, and how long it has had it.
    fn look(&mut self, w: &World, slot: usize, eye: V3) {
        let me = &w.fighters[slot];
        let (sn, cs) = crate::geom::yaw_sincos(me.yaw);
        let hurt = me.since_hit < ticks(BOT_HURT_MS);
        // Shot at, or nearly there, it fights whoever it sees.
        let focused = self.focused() && self.far && !hurt;
        let hunted = match self.job {
            Job::Hunt(e) => e,
            _ => 255,
        };
        let mut best = 255u8;
        let mut near = f32::INFINITY;
        for (q, v) in w.fighters.iter().enumerate() {
            if q == slot || !v.active() || (w.rules.teams && v.team == me.team) {
                continue;
            }
            let chest = v.body.pos + V3::new(0.0, 1.2, 0.0);
            let d = (chest - eye).len();
            if focused && d > OBJECTIVE_IGNORE_M && q as u8 != hunted {
                continue;
            }
            let to = (chest - eye).flat();
            let ahead = to.x * cs + to.z * sn >= BOT_VIEW_COS * to.len();
            let noticed = q as u8 == self.target || q as u8 == hunted || hurt || d < BOT_HEAR_M || ahead;
            let unseen = v.cloak() > CAMO_BOT_HIDDEN && d > CAMO_BOT_M;
            // The one it hunts counts as nearer than it is.
            let rank = if q as u8 == hunted { d * 0.3 } else { d };
            if rank < near && noticed && !unseen && w.map.clear(eye, chest) {
                near = rank;
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

    /// Post `k` of side `side`'s: its flag where it lies while it is down
    /// (held within [`GUARD_RING`] of it, from wherever it comes); else the `k`th nearest roam point to its base (within
    /// [`POST_M`], none on the flag itself: cover by it, as a person
    /// would hold it); or a ring about it.
    fn post(w: &World, side: usize, k: usize) -> V3 {
        let fl = &w.flags[side];
        let base = w.map.flags.map_or(V3::ZERO, |b| b[side]);
        let ring = |at: V3, r: f32| {
            let a = k as f32 * 2.4;
            at + V3::new(fmath::cos(a) * r, 0.0, fmath::sin(a) * r)
        };
        if fl.state == FLAG_DOWN {
            return fl.pos;
        }
        let k = k.saturating_sub(GUARDS) + 1;
        // The k-th nearest, without a list: each time the nearest farther
        // than the last.
        let mut last = (0.0f32, usize::MAX);
        for _ in 0..k {
            let mut next = None;
            for (i, &r) in w.map.roam.iter().enumerate() {
                let d = (r - base).flat().len();
                if d < POST_OFF_M {
                    continue;
                }
                let after = d > last.0 || (d == last.0 && last.1 != usize::MAX && i > last.1);
                if d < POST_M && after && next.is_none_or(|(nd, ni)| d < nd || (d == nd && i < ni)) {
                    next = Some((d, i));
                }
            }
            match next {
                Some(n) => last = n,
                None => return ring(base, 4.0),
            }
        }
        w.map.roam[last.1]
    }

    /// Where its job has it walk, on foot (a carrier and a rider aside).
    fn goal_of(&mut self, w: &World, slot: usize, look: bool) -> Goal {
        let me = &w.fighters[slot];
        let side = me.team as usize & 1;
        let nav = Nav::of(&w.map);
        let flag_spot = |t: usize| nav.map(|n| n.flag(t));
        let point = |at: V3, arrive: f32, urgent: bool| Goal {
            at,
            spot: None,
            arrive,
            urgent,
        };
        match self.job {
            Job::Attack => {
                let f = &w.flags[side ^ 1];
                match f.state {
                    FLAG_HOME => Goal {
                        at: f.pos,
                        spot: flag_spot(side ^ 1),
                        arrive: 0.4,
                        urgent: true,
                    },
                    _ => point(f.pos, 0.4, true),
                }
            }
            Job::Carry(_) => Goal {
                at: w.map.flags.map_or(V3::ZERO, |b| b[side]),
                spot: flag_spot(side),
                arrive: 0.5,
                urgent: true,
            },
            Job::Escort(c) => {
                let f = &w.fighters[c as usize];
                let d = (f.body.pos - me.body.pos).flat().len();
                point(f.body.pos + f.body.vel * 0.5, ESCORT_M, d > 3.0 * ESCORT_M)
            }
            Job::Hunt(e) => point(w.fighters[e as usize].body.pos, 1.5, true),
            Job::Guard(k) => {
                let at = Self::post(w, side, k as usize);
                let arrive = if w.flags[side].state == FLAG_DOWN {
                    GUARD_RING
                } else {
                    1.2
                };
                point(at, arrive, (at - me.body.pos).flat().len() > 8.0)
            }
            Job::Board(i, seat) => {
                let v = &w.vehicles[i as usize];
                let (f, r) = v.axes();
                let at = if seat as usize == PASSENGER {
                    v.pos - f * (v.spec().half_l * 0.75)
                } else {
                    v.pos + r * (v.spec().half_w + 0.8)
                };
                point(at, 0.0, true)
            }
            _ => {
                // Its own way: a gun it wants, Siege's objective, roaming.
                if look {
                    if let Some(at) = Self::loot(w, slot) {
                        return point(at, 0.0, false);
                    }
                }
                if let Some((at, n)) = Self::siege_goal(w, slot) {
                    return Goal {
                        at,
                        spot: nav.map(|nv| nv.siege(n)),
                        arrive: 1.0,
                        urgent: attacker(slot),
                    };
                }
                self.roam_goal(w, me.body.pos, 1)
            }
        }
    }

    /// Which way a bot holding where it is keeps watch: from home toward
    /// the enemy's; by the carrier, the way it goes.
    fn watch(&self, w: &World, slot: usize) -> Option<V3> {
        let me = &w.fighters[slot];
        let side = me.team as usize & 1;
        match self.job {
            Job::Guard(_) | Job::Carry(_) | Job::Attack => {
                let b = w.map.flags?;
                Some((b[side ^ 1] - me.body.pos).flat())
            }
            Job::Escort(c) => {
                let f = &w.fighters[c as usize];
                let v = f.body.vel.flat();
                Some(if v.len() > 0.5 {
                    v
                } else {
                    (f.body.pos - me.body.pos).flat()
                })
            }
            _ => None,
        }
    }

    /// The stick to go along `dir` (flat) while facing `yaw`.
    fn stick(inp: &mut Input, dir: V3, k: f32) {
        let l = dir.len();
        if l < 1e-3 {
            inp.mz = 0;
            inp.mx = 0;
            return;
        }
        let d = dir * (1.0 / l);
        let (sn, cs) = crate::geom::yaw_sincos(inp.yaw);
        inp.mz = ((cs * d.x + sn * d.z) * 127.0 * k) as i8;
        inp.mx = ((-sn * d.x + cs * d.z) * 127.0 * k) as i8;
    }

    /// Stands where it is, its look sweeping across `watch` (if any).
    fn hold(&mut self, me: &Fighter, watch: Option<V3>, inp: &mut Input) {
        inp.mz = 0;
        inp.mx = 0;
        if let Some(d) = watch.filter(|d| d.len() > 0.1) {
            self.scan = self.scan.wrapping_add(1);
            let sway = fmath::sin(self.scan as f32 * SCAN_RATE) * SCAN_ARC;
            let cur = me.yaw as f32 * (1.0 / 65536.0);
            let y = turn(cur, turns(d) + sway, AIM_TURN * DT);
            inp.yaw = rad_to_yaw(y * core::f32::consts::TAU);
            inp.pitch = 0;
        }
    }

    /// Its aim at `at` this tick, turning at a person's rate: the yaw and
    /// pitch it sets, and how far off it still is, in turns.
    fn aim(&mut self, me: &Fighter, eye: V3, at: V3, inp: &mut Input) -> f32 {
        let to = at - eye;
        let want_yaw = turns(to) + self.wob.0;
        let flat = (to.x * to.x + to.z * to.z).sqrt();
        let want_pitch = fmath::atan2(to.y, flat) * (1.0 / core::f32::consts::TAU) + self.wob.1;
        let cur_yaw = me.yaw as f32 * (1.0 / 65536.0);
        let mut dy = want_yaw - cur_yaw;
        dy -= fmath::round(dy);
        let max_turn = (AIM_TURN + AIM_TURN_SKILL * self.skill) * DT;
        let ty = dy.clamp(-max_turn, max_turn);
        inp.yaw = rad_to_yaw((cur_yaw + ty) * core::f32::consts::TAU);
        let cur_pitch = me.pitch as f32 * (1.0 / 65536.0);
        let tp = (want_pitch - cur_pitch).clamp(-max_turn, max_turn);
        inp.pitch = fmath::round((cur_pitch + tp) * 65536.0).clamp(-PITCH_MAX as f32, PITCH_MAX as f32) as i16;
        dy
    }

    /// Where it aims at its target: where a person would, its aim behind a
    /// change in the target's going until it has caught up (aimed where
    /// the target would be, had it gone on as it was).
    fn aim_point(&mut self, w: &World, minion_at: Option<V3>) -> V3 {
        match minion_at {
            Some(chest) => chest,
            None => {
                let v = &w.fighters[self.target as usize];
                self.caught = self.caught + (v.body.vel - self.caught) * (DT / AIM_CATCH_S);
                let trail = AIM_TRAIL_S * (1.0 - self.skill);
                let off = (v.body.vel - self.caught) * trail;
                v.body.pos - off + V3::new(0.0, if self.skill > 0.7 { 1.55 } else { 1.2 }, 0.0)
            }
        }
    }

    /// Fires when on target and in reach: the gun it works (a turret's, a
    /// one-seater's, else its own), both hands, a grenade now and then.
    fn trigger(&mut self, w: &World, slot: usize, near: f32, dy: f32, inp: &mut Input) {
        let me = &w.fighters[slot];
        let g = me.gun();
        let a = crate::vehicle::turret(me.gunning(&w.map)).map_or(arm(g.arm), |t| t.0);
        let rough = 1.0 - self.skill;
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
        if me.riding().is_some() {
            return;
        }
        if near < MELEE_RANGE {
            inp.buttons |= MELEE;
        }
        if on && near > 8.0 && near < 20.0 && self.unit() < 0.003 {
            inp.buttons |= GRENADE;
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
            // Back from the dead, whoever it sees it sees anew, and its
            // side tells it anew what to do.
            self.target = 255;
            self.minion = 255;
            self.seen = 0;
            self.job_tick = NEVER;
            self.parked = 0;
            self.waited = 0;
            self.jams = 0;
            return inp;
        }
        // Its side's plan, on the side's tick.
        if self.job_tick == NEVER || w.tick.is_multiple_of(PLAN_EVERY) {
            self.job = plan(w, me.team as usize & 1)[slot];
            self.job_tick = w.tick;
        }
        // A vehicle it can't get to (or into): let be a while.
        if matches!(self.job, Job::Board(..)) && me.riding().is_none() {
            self.boarding = self.boarding.saturating_add(1);
            if self.boarding as u32 > ticks(BOARD_MS) {
                self.boarding = 0;
                self.ride_cool = ticks(BOARD_COOL_MS) as u16;
            }
        } else {
            self.boarding = 0;
        }
        if self.ride_cool > 0 {
            self.ride_cool -= 1;
            if matches!(self.job, Job::Board(..)) && me.riding().is_none() {
                self.job = if w.rules.ctf && w.map.flags.is_some() {
                    Job::Attack
                } else {
                    Job::Free
                };
            }
        }
        let eye = me.eye();
        // Its tick to look around, the bots staggered.
        let look = (w.tick as usize + slot).is_multiple_of(DECIDE_EVERY);
        if look {
            self.look(w, slot, eye);
        } else {
            self.keep(w, slot);
        }
        let minion_at = (self.minion != 255).then(|| w.minions[self.minion as usize].body.pos + V3::new(0.0, 0.9, 0.0));
        let near = if self.target != 255 {
            (w.fighters[self.target as usize].body.pos + V3::new(0.0, 1.2, 0.0) - eye).len()
        } else if let Some(chest) = minion_at {
            (chest - eye).len()
        } else {
            f32::INFINITY
        };
        let fighting = self.target != 255 || minion_at.is_some();

        self.wobble();
        match me.riding() {
            Some((vi, DRIVER)) => return self.pilot(w, slot, vi, near, minion_at, look, inp),
            Some((vi, _)) => {
                if self.get_off(w, slot, vi) {
                    Self::tap_use(&mut inp, me.last_buttons);
                    self.ride_cool = ticks(RIDE_COOL_MS) as u16;
                    return inp;
                }
                // A rider fights from its seat (the flag's carrier just
                // rides).
                if fighting && !me.flag {
                    let at = self.aim_point(w, minion_at);
                    let dy = self.aim(me, eye, at, &mut inp);
                    self.trigger(w, slot, near, dy, &mut inp);
                }
                return inp;
            }
            None => self.crewed = false,
        }

        // The enemy's flag in reach, on foot: take it (USE, Halo 3's).
        if !me.flag && Self::flag_in_reach(w, slot) {
            Self::tap_use(&mut inp, me.last_buttons);
        }

        // A carrier can't shoot: it runs home (or to the vehicle coming
        // for it), swinging at whoever is in its way.
        if me.flag {
            return self.carry(w, slot, near, look, inp);
        }

        if self.strafe_for == 0 {
            self.strafe = if self.unit() < 0.5 { -1 } else { 1 };
            self.strafe_for = (TICK_HZ / 2 + self.rng.next_bounded(TICK_HZ)) as u16;
        }
        self.strafe_for -= 1;
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
        let jump = self.unit() < 0.004;

        let goal = self.goal_of(w, slot, look);
        let to_goal = (goal.at - me.body.pos).flat();
        let there = to_goal.len() <= goal.arrive.max(0.3) && fmath::fabs(goal.at.y - me.body.pos.y) < 1.5;
        self.far = to_goal.len() > NEAR_GOAL_M;

        if look || self.steer == V3::ZERO {
            self.steer = self.path(w, me.body.pos, &goal);
        }

        if let Job::Board(i, _) = self.job {
            // Close to it, in once within reach.
            if (w.vehicles[i as usize].pos - me.body.pos).flat().len() < ENTER_R - 0.4 {
                Self::tap_use(&mut inp, me.last_buttons);
            }
        }
        if !fighting {
            // Pushing at a wall and going nowhere: a second along it, and
            // the way found afresh.
            if !there && me.body.grounded && me.body.vel.flat().len() < 0.5 {
                self.foot_stuck = self.foot_stuck.saturating_add(1);
            } else {
                self.foot_stuck = 0;
            }
            if self.foot_stuck as u32 > TICK_HZ / 2 {
                self.foot_stuck = 0;
                self.detour = TICK_HZ as u16;
                self.goal_for = 0;
                self.strafe = -self.strafe;
                self.way = u16::MAX;
                if self.unit() < 0.5 {
                    inp.buttons |= JUMP;
                }
            }
            let detour = if self.detour > 0 {
                self.detour -= 1;
                self.strafe as f32 * 0.25
            } else {
                0.0
            };
            if there {
                let watch = self.watch(w, slot);
                self.hold(me, watch, &mut inp);
                return inp;
            }
            let mut to = (self.steer - me.body.pos).flat();
            if to.len() < 0.3 {
                to = to_goal;
            }
            // Over it, but a level off (in the air, or a floor above or
            // below with no way found): no way to face, so as it is.
            if to.len() < 0.3 {
                inp.mz = 0;
                return inp;
            }
            if jump {
                inp.buttons |= JUMP;
            }
            inp.yaw = rad_to_yaw((turns(to) + detour) * core::f32::consts::TAU);
            inp.pitch = 0;
            inp.mz = 127;
            // Walking to a vehicle, straight at it: no strafe to slide
            // along a solid hull.
            match self.job {
                Job::Free if goal.spot.is_none() && goal.arrive == 0.0 && to_goal.len() < PAD_REACH * 0.7 => {
                    // A gun it wants, taken in reach.
                    Self::tap_use(&mut inp, me.last_buttons);
                }
                Job::Free => {
                    inp.mx = self.strafe * 40;
                    inp.buttons |= USE;
                }
                _ => {}
            }
            return inp;
        }

        let at = self.aim_point(w, minion_at);
        let dy = self.aim(me, eye, at, &mut inp);
        // Fighting: on its way somewhere that matters, on the move there;
        // else it keeps a fighting distance and strafes (with a sword, it
        // closes in).
        let g = me.gun();
        let a = arm(g.arm);
        if jump {
            inp.buttons |= JUMP;
        }
        if goal.urgent && !there {
            let mut to = (self.steer - me.body.pos).flat();
            if to.len() < 0.3 {
                to = to_goal;
            }
            // On the way, weaving as it goes (a straight walk is an easy
            // shot).
            let l = to.len().max(1e-3);
            let across = V3::new(-to.z, 0.0, to.x) * (self.strafe as f32 * WEAVE / l);
            Self::stick(&mut inp, to * (1.0 / l) + across, 1.0);
        } else {
            inp.mz = if a.kind == crate::arms::Kind::Sword || near > 14.0 {
                127
            } else if near < 6.0 {
                -100
            } else {
                0
            };
            // Holding a post, no farther than a stride or two off it.
            if matches!(self.job, Job::Guard(_)) && to_goal.len() > 3.0 && near > 6.0 {
                Self::stick(&mut inp, to_goal, 1.0);
            }
            inp.mx = self.strafe * 127;
        }
        self.trigger(w, slot, near, dy, &mut inp);
        inp
    }

    /// The wobble in its aim drifts, more the less it can aim.
    fn wobble(&mut self) {
        let rough = 1.0 - self.skill;
        self.wob.0 = self.wob.0 * 0.98 + (self.unit() - 0.5) * AIM_WOB * rough;
        self.wob.1 = self.wob.1 * 0.98 + (self.unit() - 0.5) * AIM_WOB * 0.5 * rough;
    }

    /// The carrier, on foot: home by the waypoints (to the vehicle coming
    /// for it, if one has come near), and there it waits for its own flag
    /// to be home; a swing at whoever is in its way.
    fn carry(&mut self, w: &World, slot: usize, near: f32, look: bool, mut inp: Input) -> Input {
        let me = &w.fighters[slot];
        let ride = match self.job {
            Job::Carry(i) if (i as usize) < MAX_VEHICLES => {
                let v = &w.vehicles[i as usize];
                let ready = v.active && v.driver().is_some() && v.seats[PASSENGER] == 0;
                (ready && (v.pos - me.body.pos).flat().len() < CREW_M).then_some(v.pos)
            }
            _ => None,
        };
        let goal = match ride {
            Some(at) => Goal {
                at,
                spot: None,
                arrive: 0.0,
                urgent: true,
            },
            None => self.goal_of(w, slot, look),
        };
        let to_goal = (goal.at - me.body.pos).flat();
        if ride.is_some_and(|at| (at - me.body.pos).flat().len() < ENTER_R - 0.4) {
            Self::tap_use(&mut inp, me.last_buttons);
        }
        if look || self.steer == V3::ZERO {
            self.steer = self.path(w, me.body.pos, &goal);
        }
        if near < MELEE_RANGE {
            inp.buttons |= MELEE;
        }
        if to_goal.len() <= goal.arrive.max(0.3) {
            let watch = self.watch(w, slot);
            self.hold(me, watch, &mut inp);
            return inp;
        }
        let mut to = (self.steer - me.body.pos).flat();
        if to.len() < 0.3 {
            to = to_goal;
        }
        if to.len() < 0.3 {
            return inp;
        }
        inp.yaw = rad_to_yaw(turns(to) * core::f32::consts::TAU);
        inp.pitch = 0;
        inp.mz = 127;
        if me.body.grounded && me.body.vel.flat().len() < 0.5 {
            self.foot_stuck = self.foot_stuck.saturating_add(1);
            if self.foot_stuck as u32 > TICK_HZ / 2 {
                self.foot_stuck = 0;
                self.way = u16::MAX;
                inp.buttons |= JUMP;
            }
        } else {
            self.foot_stuck = 0;
        }
        if self.unit() < 0.01 {
            inp.buttons |= JUMP;
        }
        inp
    }

    /// A rider gets out: its vehicle stopped by the enemy's flag (to go
    /// for it), or by home with the flag aboard; nobody at the wheel; the
    /// hull nearly gone; or a job that isn't riding this.
    fn get_off(&mut self, w: &World, slot: usize, vi: usize) -> bool {
        let me = &w.fighters[slot];
        let v = &w.vehicles[vi];
        let side = me.team as usize & 1;
        let stopped = v.vel.flat().len() < 1.5;
        if v.driver().is_none() || v.health * BAIL_HEALTH < v.spec().health {
            return true;
        }
        let Some(b) = w.map.flags.filter(|_| w.rules.ctf) else {
            return false;
        };
        if me.flag {
            return stopped && (v.pos - b[side]).flat().len() < DROP_M;
        }
        let theirs = &w.flags[side ^ 1];
        stopped && theirs.state != FLAG_HELD && (v.pos - theirs.pos).flat().len() < DROP_M
    }

    /// At a vehicle's controls: where its trip takes it.
    fn trip_goal(&mut self, w: &World, slot: usize, trip: Trip, v: &crate::vehicle::Vehicle) -> Goal {
        let me = &w.fighters[slot];
        let side = me.team as usize & 1;
        let nav = Nav::of(&w.map);
        let point = |at: V3, arrive: f32| Goal {
            at,
            spot: None,
            arrive,
            urgent: true,
        };
        let enemy_in_sight = (self.target != 255).then(|| w.fighters[self.target as usize].body.pos);
        match trip {
            Trip::Attack => {
                let f = &w.flags[side ^ 1];
                if f.state == FLAG_HOME {
                    let spot = nav.map(|n| n.flag(side ^ 1));
                    // As near as a truck goes: its way's end.
                    let end = spot
                        .and_then(|s| nav.and_then(|n| n.wheels.as_ref()).and_then(|g| g.end(s)))
                        .unwrap_or(f.pos);
                    Goal {
                        at: end,
                        spot,
                        arrive: PARK_M,
                        urgent: true,
                    }
                } else {
                    point(f.pos, PARK_M)
                }
            }
            // Near enough for the carrier to walk to: it waits where it is
            // (out of the base it has come to, where a truck wedges);
            // farther, it goes to them.
            Trip::Fetch(c) => {
                let at = w.fighters[c as usize].body.pos;
                if (at - v.pos).flat().len() < FETCH_WAIT_M {
                    point(v.pos, f32::INFINITY)
                } else {
                    point(at, PARK_M)
                }
            }
            Trip::Home => {
                // As near home as a truck goes (onto the base itself, it
                // scores from the seat).
                let base = w.map.flags.map_or(V3::ZERO, |b| b[side]);
                let spot = nav.map(|n| n.flag(side));
                let end = spot
                    .and_then(|s| nav.and_then(|n| n.wheels.as_ref()).and_then(|g| g.end(s)))
                    .unwrap_or(base);
                Goal {
                    at: end,
                    spot,
                    arrive: 2.0,
                    urgent: true,
                }
            }
            Trip::Hunt(e) => point(w.fighters[e as usize].body.pos, 0.0),
            Trip::Roam => match enemy_in_sight {
                Some(at) if (at - v.pos).flat().len() < 60.0 => point(at, 0.0),
                _ => self.roam_goal(w, v.pos, 1),
            },
        }
    }

    /// At the controls of vehicle `vi`: a truck driven, a skimmer or a
    /// flier piloted, where its trip takes it; out when the hull is nearly
    /// gone, it is stuck for good, or it has waited by the enemy's flag
    /// long enough.
    #[allow(clippy::too_many_arguments)]
    fn pilot(
        &mut self,
        w: &World,
        slot: usize,
        vi: usize,
        near: f32,
        minion_at: Option<V3>,
        look: bool,
        mut inp: Input,
    ) -> Input {
        let me = &w.fighters[slot];
        let v = w.vehicles[vi];
        let (trip, wait) = match self.job {
            Job::Drive(i, t, wait) if i as usize == vi => (t, wait),
            _ => (Trip::Roam, false),
        };
        // Alone by the enemy's flag, nobody having come with it to go for
        // it: out, to go for it itself. (One who came went for it: it
        // waits to fetch them.)
        self.crewed |= v.seats[PASSENGER] != 0;
        let alone = self.parked as u32 > TICK_HZ && !self.crewed && !wait;
        let leave = v.health * BAIL_HEALTH < v.spec().health
            || self.jams >= JAMS
            || self.parked as u32 > ticks(PARK_MS)
            || alone
            || (v.kind == FLIER && !flies(w));
        if leave {
            self.jams = 0;
            self.parked = 0;
            self.crewed = false;
            self.ride_cool = ticks(RIDE_COOL_MS) as u16;
            Self::tap_use(&mut inp, me.last_buttons);
            return inp;
        }
        let goal = self.trip_goal(w, slot, trip, &v);
        if look || self.steer == V3::ZERO {
            self.steer = match v.kind {
                FLIER => goal.at,
                _ => self.drive_path(w, v.pos, &goal),
            };
        }
        match v.kind {
            TRUCK => self.drive_truck(w, &v, &goal, trip, wait, inp),
            SKIMMER | FLIER => {
                let fight = (self.target != 255 || minion_at.is_some()) && near < PILOT_FIGHT_M;
                if fight {
                    let at = self.aim_point(w, minion_at);
                    let dy = self.aim(me, me.eye(), at, &mut inp);
                    self.trigger(w, slot, near, dy, &mut inp);
                    inp.mz = if near > 20.0 { 127 } else { 50 };
                    inp.mx = self.strafe * 127;
                    if self.strafe_for == 0 {
                        self.strafe = -self.strafe;
                        self.strafe_for = TICK_HZ as u16;
                    }
                    self.strafe_for -= 1;
                } else {
                    let to = (self.steer - v.pos).flat();
                    inp.yaw = rad_to_yaw(turns(to) * core::f32::consts::TAU);
                    inp.pitch = 0;
                    let d = (goal.at - v.pos).flat().len();
                    inp.mz = if d < goal.arrive.max(3.0) {
                        0
                    } else if d < SLOW_M {
                        70
                    } else {
                        127
                    };
                    inp.mx = 0;
                }
                if v.kind == FLIER {
                    self.fly_level(w, &v, fight, &mut inp);
                } else {
                    self.unstick(&v, &mut inp);
                }
                inp
            }
            _ => inp,
        }
    }

    /// A flier keeps its height: over the ground ahead by [`FLY_HIGH`],
    /// its nose up or down to get there (and up off the pad); in a fight
    /// it aims as it must, but never into the ground.
    fn fly_level(&mut self, w: &World, v: &crate::vehicle::Vehicle, fight: bool, inp: &mut Input) {
        let (f, _) = v.axes();
        let mut ground = w.map.ground(v.pos.x, v.pos.z);
        for k in 1..=3 {
            let p = v.pos + f * (k as f32 * 12.0);
            ground = ground.max(w.map.ground(p.x, p.z));
        }
        let over = v.pos.y - ground;
        let want = ((FLY_HIGH - over) * 0.01).clamp(-0.06, 0.08);
        let climb = rad_to_yaw(want * core::f32::consts::TAU) as i16;
        if !fight || over < FLY_HIGH * 0.4 {
            inp.pitch = if fight { inp.pitch.max(climb) } else { climb };
        }
        if v.grounded {
            inp.pitch = inp.pitch.max(FLY_PITCH_MAX / 2);
            inp.mz = 127;
        }
    }

    /// At the wheel of a skimmer that barely moves: back off a second.
    fn unstick(&mut self, v: &crate::vehicle::Vehicle, inp: &mut Input) {
        if inp.mz > 60 && v.vel.flat().len() < 1.0 {
            self.stuck += 1;
        } else {
            self.stuck = 0;
        }
        if self.stuck as u32 > TICK_HZ / 2 {
            self.stuck = 0;
            self.backing = TICK_HZ as u16;
            self.jams = self.jams.saturating_add(1);
            self.wheel_way = u16::MAX;
        }
        if self.backing > 0 {
            self.backing -= 1;
            inp.mz = -127;
            inp.mx = self.strafe * 127;
        } else if v.vel.flat().len() > 4.0 {
            self.jams = 0;
        }
    }

    /// A truck's stick: steering at where it decided to go, slower into a
    /// turn and near the end, stopped there (or while its crew gets in);
    /// backed off what it is stuck on, turning the other way.
    fn drive_truck(
        &mut self,
        w: &World,
        v: &crate::vehicle::Vehicle,
        goal: &Goal,
        trip: Trip,
        wait: bool,
        mut inp: Input,
    ) -> Input {
        let me_yaw = v.yaw;
        inp.yaw = me_yaw;
        inp.pitch = 0;
        let speed = v.vel.flat().len();
        let left = (goal.at - v.pos).flat().len();
        let waiting = wait && self.waited as u32 <= ticks(CREW_WAIT_MS);
        if wait {
            self.waited = self.waited.saturating_add(1);
        } else {
            self.waited = 0;
        }
        let at_end = left <= goal.arrive.max(1.0);
        if waiting || at_end {
            // Stopped: the brake while it rolls (a throttle against its
            // going brakes).
            let (f, _) = v.axes();
            let vf = v.vel.dot(f);
            inp.mz = if vf > 0.5 {
                -127
            } else if vf < -0.5 {
                127
            } else {
                0
            };
            inp.mx = 0;
            self.stuck = 0;
            self.was = V3::ZERO;
            self.was_for = 0;
            if at_end && trip == Trip::Attack && w.rules.ctf {
                self.parked = self.parked.saturating_add(1);
            }
            return inp;
        }
        self.parked = 0;
        let to = self.steer - v.pos;
        let mut diff = turns(to) - me_yaw as f32 * (1.0 / 65536.0);
        diff -= fmath::round(diff);
        // Stuck on something (stopped, or scraping along it getting
        // nowhere): back up a second, turning the other way.
        self.stuck = if speed < 1.0 { self.stuck + 1 } else { 0 };
        let mut jammed = self.stuck as u32 > TICK_HZ / 2;
        if self.backing == 0 {
            if self.was_for == 0 {
                jammed |= self.was != V3::ZERO && (v.pos - self.was).flat().len() < PROGRESS_M;
                self.was = v.pos;
                self.was_for = ticks(PROGRESS_MS) as u16;
            }
            self.was_for -= 1;
        }
        if jammed {
            self.backing = TICK_HZ as u16;
            self.stuck = 0;
            self.jams = self.jams.saturating_add(1);
            self.wheel_way = u16::MAX;
            self.was = V3::ZERO;
        }
        if speed > 6.0 && self.backing == 0 {
            self.jams = 0;
        }
        let steer = (diff * 6.0).clamp(-1.0, 1.0) * 127.0;
        if self.backing > 0 {
            self.backing -= 1;
            inp.mx = -steer as i8;
            inp.mz = -127;
        } else {
            inp.mx = steer as i8;
            let slow = (left / SLOW_M).clamp(0.35, 1.0);
            let turning = if fmath::fabs(diff) > 0.3 { 0.4 } else { 1.0 };
            inp.mz = (127.0 * slow * turning) as i8;
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
/// one-seaters' flight in the parity gate every way a stick can go (bots
/// fly them only by rule). Folded as [`bot_match`]'s. 0 on a map without
/// both.
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

//! A simple AI fighter that reads the true world, for the server's empty
//! seats and for the determinism gate. Deterministic: its choices come from
//! the world and its own PCG stream. Fireplace's `client/arena-bot.mjs` is
//! the fuller one to port next (docs/RIP.md §2).

use crate::arms::{arm, NONE};
use crate::consts::*;
use crate::fighter::Gun;
use crate::fmath;
use crate::geom::{rad_to_yaw, V3};
use crate::input::*;
use crate::rng::Pcg32;
use crate::world::{Phase, Rules, World};

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
    /// A wobble that drifts, in turns.
    wob: (f32, f32),
    /// The map's roam point it walks to with nobody in sight, and ticks
    /// left before it gives up on it (stuck on a wall).
    goal: u8,
    goal_for: u16,
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
}

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
/// A bot's waist, for a straight walk's line.
const WAIST: f32 = 0.9;

impl Bot {
    pub fn new(seed: u64, slot: usize, skill: f32) -> Bot {
        Bot {
            rng: Pcg32::new(seed, 100 + slot as u64),
            skill,
            strafe: 1,
            strafe_for: 0,
            seen: 0,
            target: 255,
            wob: (0.0, 0.0),
            goal: 0,
            goal_for: 0,
            ride_for: 0,
            ride_cool: 0,
            stuck: 0,
            backing: 0,
            foot_stuck: 0,
            detour: 0,
        }
    }

    /// The map's roam point to head for with nobody in sight (picking the
    /// next when it's reached or taking too long), or with none, toward the
    /// middle, then the first pad.
    fn roam_goal(&mut self, w: &World, at: V3) -> V3 {
        let roam = &w.map.roam;
        if !roam.is_empty() {
            let here = roam[self.goal as usize % roam.len()];
            if self.goal_for == 0 || (here - at).flat().len() < GOAL_NEAR {
                self.goal = self.rng.next_bounded(roam.len() as u32) as u8;
                self.goal_for = ticks(GOAL_MS) as u16;
            }
            self.goal_for -= 1;
            roam[self.goal as usize % roam.len()]
        } else if at.flat().len() > 6.0 {
            V3::ZERO
        } else {
            w.map.pads.first().map_or(V3::ZERO, |p| p.pos)
        }
    }

    /// A gun it wants, near, on its level and a straight walk away: on a
    /// pad that holds it, or on the floor. Wanted: ranked over the best it carries that has rounds, or
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
        if self.ride_for as u32 > ticks(RIDE_MS) || (best != 255 && near < 18.0) {
            Self::tap_use(&mut inp, me.last_buttons);
            return inp;
        }
        let goal = if best != 255 {
            w.fighters[best as usize].body.pos
        } else {
            self.roam_goal(w, v.pos)
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

    pub fn think(&mut self, w: &World, slot: usize) -> Input {
        let me = &w.fighters[slot];
        let mut inp = Input {
            yaw: me.yaw,
            pitch: me.pitch,
            ..Input::default()
        };
        if !me.active() || w.phase != Phase::Playing {
            self.ride_for = 0;
            return inp;
        }
        self.ride_cool = self.ride_cool.saturating_sub(1);
        let eye = me.eye();
        // The nearest enemy it can see.
        let mut best = 255u8;
        let mut near = f32::INFINITY;
        for (q, v) in w.fighters.iter().enumerate() {
            if q == slot || !v.active() || (w.rules.teams && v.team == me.team) {
                continue;
            }
            let chest = v.body.pos + V3::new(0.0, 1.2, 0.0);
            let d = (chest - eye).len();
            if d < near && w.map.clear(eye, chest) {
                near = d;
                best = q as u8;
            }
        }
        if best != self.target {
            self.target = best;
            self.seen = 0;
        } else {
            self.seen = self.seen.saturating_add(1);
        }

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

        if self.strafe_for == 0 {
            self.strafe = if self.unit() < 0.5 { -1 } else { 1 };
            self.strafe_for = (TICK_HZ / 2 + self.rng.next_bounded(TICK_HZ)) as u16;
        }
        self.strafe_for -= 1;
        let rough = 1.0 - self.skill;
        self.wob.0 = self.wob.0 * 0.98 + (self.unit() - 0.5) * 0.002 * rough;
        self.wob.1 = self.wob.1 * 0.98 + (self.unit() - 0.5) * 0.001 * rough;

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

        if best == 255 {
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
            }
            let detour = if self.detour > 0 {
                self.detour -= 1;
                self.strafe as f32 * core::f32::consts::FRAC_PI_2
            } else {
                0.0
            };
            // Nobody in sight: to a gun it wants, taken in reach.
            if let Some(at) = Self::loot(w, slot).filter(|_| detour == 0.0) {
                let to = at - me.body.pos;
                inp.yaw = rad_to_yaw(fmath::atan2(to.z, to.x));
                inp.pitch = 0;
                inp.mz = 127;
                if to.flat().len() < PAD_REACH * 0.7 {
                    Self::tap_use(&mut inp, me.last_buttons);
                }
                return inp;
            }
            // Else to a free vehicle near if it may, else the roam goal.
            let free = (self.ride_cool == 0 && me.riding().is_none())
                .then(|| {
                    w.vehicles
                        .iter()
                        .filter(|v| v.active && v.seats[crate::vehicle::DRIVER] == 0)
                        .map(|v| (v.pos, (v.pos - me.body.pos).flat().len()))
                        .filter(|&(_, d)| d < VEHICLE_WALK)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                })
                .flatten();
            let goal = match free {
                Some((at, _)) => at,
                None => {
                    let g = self.roam_goal(w, me.body.pos);
                    // On a map with waypoints, the way there rather than
                    // straight at it.
                    match &w.map.nav {
                        Some(nav) if !w.map.roam.is_empty() => nav
                            .toward(&w.map, self.goal as usize % w.map.roam.len(), me.body.pos)
                            .unwrap_or(g),
                        _ => g,
                    }
                }
            };
            let to = goal - me.body.pos;
            inp.yaw = rad_to_yaw(fmath::atan2(to.z, to.x) + detour);
            inp.pitch = 0;
            inp.mz = 127;
            inp.mx = self.strafe * 40;
            match free {
                Some((_, d)) if d < ENTER_R - 0.5 => Self::tap_use(&mut inp, me.last_buttons),
                Some(_) => {}
                None => inp.buttons |= USE,
            }
            return inp;
        }

        let v = &w.fighters[best as usize];
        let aim_at = v.body.pos + V3::new(0.0, if self.skill > 0.7 { 1.55 } else { 1.2 }, 0.0);
        let to = aim_at - eye;
        let want_yaw = fmath::atan2(to.z, to.x) * (1.0 / core::f32::consts::TAU) + self.wob.0;
        let flat = (to.x * to.x + to.z * to.z).sqrt();
        let want_pitch = fmath::atan2(to.y, flat) * (1.0 / core::f32::consts::TAU) + self.wob.1;
        // Turn toward it at a human rate, not in one tick.
        let cur_yaw = me.yaw as f32 * (1.0 / 65536.0);
        let mut dy = want_yaw - cur_yaw;
        dy -= fmath::round(dy);
        let max_turn = (1.5 + 2.0 * self.skill) * DT;
        let ty = dy.clamp(-max_turn, max_turn);
        inp.yaw = rad_to_yaw((cur_yaw + ty) * core::f32::consts::TAU);
        let cur_pitch = me.pitch as f32 * (1.0 / 65536.0);
        let tp = (want_pitch - cur_pitch).clamp(-max_turn, max_turn);
        inp.pitch = fmath::round((cur_pitch + tp) * 65536.0).clamp(-PITCH_MAX as f32, PITCH_MAX as f32) as i16;

        // Keep a fighting distance and strafe; with a sword, close in.
        let a = arm(g.arm);
        inp.mz = if a.kind == crate::arms::Kind::Sword || near > 14.0 {
            127
        } else if near < 6.0 {
            -100
        } else {
            0
        };
        inp.mx = self.strafe * 127;

        let react = (TICK_HZ as f32 * (0.6 - 0.35 * self.skill)) as u16;
        let on = fmath::fabs(dy) < 0.006 + 0.01 * rough && self.seen > react;
        if on && near < a.range {
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

/// A whole bot match, for the determinism gate: four bots, red against
/// blue, a high score limit so it never ends, each seat checked a different
/// depth back to exercise the rewind. Returns the world's hash, folded over
/// every tick, so any tick that differs shows.
pub fn bot_match(seed: u64, ticks_n: u32) -> u64 {
    bot_match_on(0, 0, seed, ticks_n)
}

/// [`bot_match`] on map `map` in `mode`: 0 Doubles, 1 an eight-seat Gun
/// Game, 2 a sixteen-seat Big Team. What `ci/parity.sh` runs on every map.
/// 0 if there is no such map or mode.
pub fn bot_match_on(map: u8, mode: u8, seed: u64, ticks_n: u32) -> u64 {
    let Some(m) = crate::map::by_id(map) else {
        return 0;
    };
    let base = match mode {
        0 => Rules::doubles(),
        1 => Rules::gun_game(),
        2 => Rules::big_team(),
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

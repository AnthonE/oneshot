//! A simple AI fighter that reads the true world, for the server's empty
//! seats and for the determinism gate. Deterministic: its choices come from
//! the world and its own PCG stream. Fireplace's `client/arena-bot.mjs` is
//! the fuller one to port next (docs/RIP.md §2).

use crate::arms::{arm, NONE};
use crate::consts::*;
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
            wob: (0.0, 0.0),
        }
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
            return inp;
        }
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

        if self.strafe_for == 0 {
            self.strafe = if self.unit() < 0.5 { -1 } else { 1 };
            self.strafe_for = (TICK_HZ / 2 + self.rng.next_bounded(TICK_HZ)) as u16;
        }
        self.strafe_for -= 1;
        let rough = 1.0 - self.skill;
        self.wob.0 = self.wob.0 * 0.98 + (self.unit() - 0.5) * 0.002 * rough;
        self.wob.1 = self.wob.1 * 0.98 + (self.unit() - 0.5) * 0.001 * rough;

        let g = me.gun();
        if g.arm != NONE && g.mag == 0 {
            inp.buttons |= RELOAD;
        }
        if self.unit() < 0.004 {
            inp.buttons |= JUMP;
        }

        if best == 255 {
            // Nobody in sight: walk toward the middle, then the pads.
            let goal = if me.body.pos.flat().len() > 6.0 {
                V3::ZERO
            } else {
                w.map.pads.first().map_or(V3::ZERO, |p| p.pos)
            };
            let to = goal - me.body.pos;
            inp.yaw = rad_to_yaw(fmath::atan2(to.z, to.x));
            inp.pitch = 0;
            inp.mz = 127;
            inp.mx = self.strafe * 40;
            inp.buttons |= USE;
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

        // Keep a fighting distance and strafe.
        inp.mz = if near > 14.0 {
            127
        } else if near < 6.0 {
            -100
        } else {
            0
        };
        inp.mx = self.strafe * 127;

        let react = (TICK_HZ as f32 * (0.6 - 0.35 * self.skill)) as u16;
        let on = fmath::fabs(dy) < 0.006 + 0.01 * rough && self.seen > react;
        let a = arm(g.arm);
        if on && near < a.range {
            // A semi-automatic needs the trigger let go between pulls.
            if a.auto || me.last_buttons & FIRE == 0 {
                inp.buttons |= FIRE;
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
    let rules = Rules {
        score_limit: 100_000,
        time_limit_ms: 0,
        ..Rules::doubles()
    };
    let salt = (crate::rng::splitmix64(seed), crate::rng::splitmix64(seed ^ 0x5a17));
    let mut w = World::new(crate::map::Map::proving_grounds(), rules, seed, salt);
    let mut bots = [Bot::new(seed, 0, 0.5); MAX_FIGHTERS];
    for (s, b) in bots.iter_mut().enumerate().take(4) {
        w.seat(s, (s & 1) as u8);
        *b = Bot::new(seed, s, 0.3 + 0.2 * s as f32);
    }
    let depth = [0, 3 << 8, 7 << 8 | 128, 12 << 8, 0, 0, 0, 0];
    let mut fold = 0u64;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    for _ in 0..ticks_n {
        for s in 0..4 {
            inputs[s] = bots[s].think(&w, s);
        }
        w.step(&inputs, &depth);
        fold = crate::rng::splitmix64(fold ^ w.hash());
    }
    fold
}

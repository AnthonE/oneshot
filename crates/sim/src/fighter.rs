//! One fighter, and the part of its tick that depends on nothing but its own
//! state and its own input: moving, aiming, and its guns' timers and ammo.
//! [`Fighter::step_local`] is what the browser predicts (docs/DESIGN.md §5);
//! the world resolves what it fired against everyone else.

use crate::arms::{arm, Kind, NONE, SWAP_MS};
use crate::consts::*;
use crate::geom::V3;
use crate::input::*;
use crate::map::Map;
use crate::movement::{self, Body};
use crate::thing::Thing;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gun {
    pub arm: u8,
    pub mag: u16,
    pub reserve: u16,
}

impl Gun {
    pub fn full(id: u8) -> Gun {
        let a = arm(id);
        Gun {
            arm: id,
            mag: a.mag,
            reserve: a.reserve,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fighter {
    /// The seat is taken.
    pub seated: bool,
    pub team: u8,
    pub alive: bool,
    pub body: Body,
    pub yaw: u16,
    pub pitch: i16,
    pub shield: i32,
    pub health: i32,
    /// Ticks since this fighter last took damage.
    pub since_hit: u32,
    pub guns: [Gun; 2],
    /// Which of `guns` is in hand.
    pub cur: u8,
    /// Ticks until the gun in hand may fire.
    pub refire: u16,
    /// Ticks left of a reload; the magazine fills when it reaches 0.
    pub reload: u16,
    /// Rounds still to come in the current burst, and ticks until the next.
    pub burst_left: u8,
    pub burst_gap: u16,
    pub melee_cool: u16,
    pub gren_cool: u16,
    pub grenades: u8,
    /// Rounds fired since the match began: keys the salted spread.
    pub shots: u32,
    pub last_buttons: u16,
    /// Ticks until a dead fighter comes back.
    pub respawn: u16,
    pub kills: u16,
    pub deaths: u16,
    /// In a vehicle: its slot × 2 + the seat, plus 1; 0 on foot. The world
    /// puts a rider's body at its seat (`Vehicle::mount`).
    pub ride: u8,
    /// Lunging with the sword: at seat `lunge - 1`, for `lunge_left` more
    /// ticks; 0 not. The world's (it steers by where the target is), so
    /// not the prediction's: the client sees it as its body's velocity.
    pub lunge: u8,
    pub lunge_left: u16,
}

/// What a fighter's own tick did, for the world (or the prediction's
/// effects) to act on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Local {
    /// A round left the gun in hand (`shots` already counts it).
    pub fired: bool,
    pub melee: bool,
    pub throw: bool,
    /// USE went down this tick.
    pub use_pad: bool,
}

impl Fighter {
    pub fn gun(&self) -> &Gun {
        &self.guns[self.cur as usize & 1]
    }

    fn gun_mut(&mut self) -> &mut Gun {
        &mut self.guns[self.cur as usize & 1]
    }

    pub fn eye(&self) -> V3 {
        self.body.eye()
    }

    pub fn active(&self) -> bool {
        self.seated && self.alive
    }

    /// The vehicle and seat it rides in, if it does.
    pub fn riding(&self) -> Option<(usize, usize)> {
        (self.ride != 0).then(|| ((self.ride as usize - 1) / 2, (self.ride as usize - 1) % 2))
    }

    /// The `ride` value for `seat` of vehicle `vehicle`.
    pub fn ride_of(vehicle: usize, seat: usize) -> u8 {
        (vehicle * 2 + seat + 1) as u8
    }

    /// Back in play at a spawn point, full and with the loadout.
    pub fn spawn(&mut self, pos: V3, yaw: u16, loadout: [u8; 2]) {
        self.alive = true;
        self.body = Body {
            pos,
            vel: V3::ZERO,
            grounded: true,
            crouched: false,
        };
        self.yaw = yaw;
        self.pitch = 0;
        self.shield = SHIELD_MAX;
        self.health = HEALTH_MAX;
        self.since_hit = u32::MAX / 2;
        self.guns = [Gun::full(loadout[0]), Gun::full(loadout[1])];
        self.cur = 0;
        self.refire = 0;
        self.reload = 0;
        self.burst_left = 0;
        self.burst_gap = 0;
        self.melee_cool = 0;
        self.gren_cool = 0;
        self.grenades = GREN_START;
        self.respawn = 0;
        self.ride = 0;
        self.lunge = 0;
        self.lunge_left = 0;
    }

    /// One tick of this fighter in a match `phase`: what the browser runs to
    /// predict itself, so it freezes and thaws exactly when the server does.
    /// During the countdown only the aim and the buttons move; once it is
    /// over nothing does.
    pub fn step_in(&mut self, phase: crate::world::Phase, inp: &Input, map: &Map) -> Local {
        match phase {
            crate::world::Phase::Countdown => {
                self.yaw = inp.yaw;
                self.pitch = inp.pitch;
                self.last_buttons = inp.buttons;
                Local::default()
            }
            crate::world::Phase::Playing => self.step_local(inp, map),
            crate::world::Phase::Over => Local::default(),
        }
    }

    /// One tick of this fighter's own state under `inp` (already
    /// sanitized). Deterministic in (self, inp, map) alone. A rider doesn't
    /// walk (the world puts it at its seat); a driver doesn't fight, a
    /// passenger only shoots.
    pub fn step_local(&mut self, inp: &Input, map: &Map) -> Local {
        let mut out = Local::default();
        if !self.active() {
            self.last_buttons = inp.buttons;
            return out;
        }
        let pressed = inp.buttons & !self.last_buttons;
        self.yaw = inp.yaw;
        self.pitch = inp.pitch;
        let seat = self.riding().map(|(_, s)| s);
        if seat.is_none() {
            movement::step(&mut self.body, inp, map);
        }
        let shoots = seat != Some(crate::vehicle::DRIVER);
        let on_foot = seat.is_none();

        dec(&mut self.refire);
        dec(&mut self.burst_gap);
        dec(&mut self.melee_cool);
        dec(&mut self.gren_cool);
        if self.reload > 0 {
            self.reload -= 1;
            if self.reload == 0 {
                let a = arm(self.gun().arm);
                let g = self.gun_mut();
                let take = (a.mag - g.mag).min(g.reserve);
                g.mag += take;
                g.reserve -= take;
            }
        }

        // Swap: cancels a reload and a burst.
        if pressed & SWAP != 0 && self.guns[(self.cur as usize & 1) ^ 1].arm != NONE {
            self.cur ^= 1;
            self.reload = 0;
            self.burst_left = 0;
            self.refire = self.refire.max(ticks(SWAP_MS) as u16);
        }

        let a = arm(self.gun().arm);
        let g = *self.gun();
        let can_reload = g.arm != NONE && g.mag < a.mag && g.reserve > 0 && self.reload == 0;
        if can_reload && (pressed & RELOAD != 0 || (g.mag == 0 && inp.held(FIRE))) {
            self.reload = ticks(a.reload_ms) as u16;
            self.burst_left = 0;
        }

        // A burst keeps firing once pulled; otherwise a semi-automatic
        // fires on the pull, an automatic while held.
        let ready = shoots && self.reload == 0 && g.arm != NONE && g.mag > 0;
        if ready && self.burst_left > 0 && self.burst_gap == 0 {
            self.burst_left -= 1;
            out.fired = true;
            if self.burst_left == 0 {
                self.refire = self.refire.max(ticks(a.refire_ms) as u16);
            } else {
                self.burst_gap = ticks(a.burst_gap_ms) as u16;
            }
        } else if ready && self.burst_left == 0 && self.refire == 0 && inp.held(FIRE) && (a.auto || pressed & FIRE != 0)
        {
            out.fired = true;
            if a.burst > 1 {
                self.burst_left = a.burst - 1;
                self.burst_gap = ticks(a.burst_gap_ms) as u16;
            } else {
                self.refire = ticks(a.refire_ms) as u16;
            }
        }
        if out.fired {
            if a.kind != Kind::Sword {
                self.gun_mut().mag -= 1;
            }
            self.shots = self.shots.wrapping_add(1);
            if a.kind == Kind::Rocket {
                self.burst_left = 0;
            }
        } else if self.burst_left > 0 && !ready {
            self.burst_left = 0;
        }

        if on_foot && pressed & MELEE != 0 && self.melee_cool == 0 {
            out.melee = true;
            self.melee_cool = ticks(MELEE_COOL_MS) as u16;
            self.reload = 0;
            self.burst_left = 0;
            self.refire = self.refire.max(ticks(MELEE_COOL_MS / 2) as u16);
        }
        if on_foot && pressed & GRENADE != 0 && self.grenades > 0 && self.gren_cool == 0 {
            out.throw = true;
            self.grenades -= 1;
            self.gren_cool = ticks(GREN_COOL_MS) as u16;
        }
        out.use_pad = pressed & USE != 0;
        self.last_buttons = inp.buttons;
        out
    }
}

fn dec(t: &mut u16) {
    *t = t.saturating_sub(1);
}

impl Thing for Fighter {
    fn live(&self) -> bool {
        self.seated
    }

    fn hash(&self, h: &mut crate::hash::Fnv) {
        h.bool(self.seated);
        h.u8(self.team);
        h.bool(self.alive);
        h.v3(self.body.pos);
        h.v3(self.body.vel);
        h.bool(self.body.grounded);
        h.bool(self.body.crouched);
        h.u16(self.yaw);
        h.u16(self.pitch as u16);
        h.i32(self.shield);
        h.i32(self.health);
        h.u32(self.since_hit);
        for g in &self.guns {
            h.u8(g.arm);
            h.u16(g.mag);
            h.u16(g.reserve);
        }
        h.u8(self.cur);
        h.u16(self.refire);
        h.u16(self.reload);
        h.u8(self.burst_left);
        h.u16(self.burst_gap);
        h.u16(self.melee_cool);
        h.u16(self.gren_cool);
        h.u8(self.grenades);
        h.u32(self.shots);
        h.u16(self.last_buttons);
        h.u16(self.respawn);
        h.u16(self.kills);
        h.u16(self.deaths);
        h.u8(self.ride);
        h.u8(self.lunge);
        h.u16(self.lunge_left);
    }
}

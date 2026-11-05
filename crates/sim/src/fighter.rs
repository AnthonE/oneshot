//! One fighter, and the part of its tick that depends on nothing but its own
//! state and its own input: moving, aiming, and its guns' timers and ammo.
//! [`Fighter::step_local`] is what the browser predicts (docs/DESIGN.md §5);
//! the world resolves what it fired against everyone else.

use crate::arms::{arm, Kind, HEAT_FULL, NONE, SWAP_MS};
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
    /// A hot gun's heat, in 1/128 % ([`HEAT_FULL`] is 100).
    pub heat: u16,
}

impl Gun {
    pub fn full(id: u8) -> Gun {
        let a = arm(id);
        Gun {
            arm: id,
            mag: a.mag,
            reserve: a.reserve,
            heat: 0,
        }
    }

    /// Not a round left: nothing in the magazine (a hot gun's battery) or
    /// in reserve.
    pub fn spent(&self) -> bool {
        self.mag == 0 && self.reserve == 0
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
    /// The lunge is a melee's (Halo 2's), not the sword's: it ends in a
    /// melee blow, with any gun in hand.
    pub lunge_melee: bool,
    /// Ticks the trigger of a charge weapon has been held, to its full.
    pub charge: u16,
    /// Shards stuck in this fighter, and ticks since the last; the world's.
    pub needles: u8,
    pub needle_t: u16,
    /// Dual wielding: the gun in the left hand (`arms::NONE`: none), and
    /// its own timers. Only a gun that is `dual` goes there, beside one in
    /// the right; it fires on [`FIRE_LEFT`]. While dual: no grenades, and a
    /// melee or a swap drops it.
    pub left: Gun,
    pub left_refire: u16,
    pub left_reload: u16,
    pub left_charge: u16,
    /// Sticky grenades carried (frags are `grenades`), and which a throw
    /// takes: 0 frags, 1 stickies.
    pub stickies: u8,
    pub gren_kind: u8,
    /// The overshield is climbing (to `OVERSHIELD_MAX`); shields over full
    /// and not climbing drain back to full. The world's.
    pub charging: bool,
    /// Ticks of active camouflage left, and of being shown by a shot, a
    /// throw, a blow or a hit (`cloak`). The world's.
    pub camo: u16,
    pub flare: u16,
    /// Carrying the enemy's flag (Capture the Flag): both hands on it, so
    /// no gun fires and no grenade goes; a melee still swings. The world's
    /// to set; its own tick reads it.
    pub flag: bool,
    /// On a map with a void: who last hurt this fighter, their seat plus 1
    /// (0 nobody), and ticks left on its feet before a fall is its own
    /// again ([`FALL_CREDIT_MS`]). The world's.
    pub hurt_by: u8,
    pub hurt_t: u16,
}

/// What a fighter's own tick did, for the world (or the prediction's
/// effects) to act on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Local {
    /// A round left the gun in hand (`shots` already counts it).
    pub fired: bool,
    pub melee: bool,
    /// The round fired is a charge weapon's charged one.
    pub charged: bool,
    /// The same for the left hand's gun.
    pub fired_left: bool,
    pub charged_left: bool,
    pub throw: bool,
    /// The throw is a sticky, not a frag.
    pub sticky: bool,
    /// USE went down this tick.
    pub use_pad: bool,
}

impl Fighter {
    pub fn gun(&self) -> &Gun {
        &self.guns[self.cur as usize & 1]
    }

    /// Two guns in hand.
    pub fn dual(&self) -> bool {
        self.left.arm != NONE
    }

    /// The left hand empties.
    pub fn drop_left(&mut self) {
        self.left = Gun::default();
        self.left_refire = 0;
        self.left_reload = 0;
        self.left_charge = 0;
    }

    pub fn eye(&self) -> V3 {
        self.body.eye()
    }

    pub fn active(&self) -> bool {
        self.seated && self.alive
    }

    /// How hidden the camouflage keeps this body, 0 (seen plainly) to
    /// [`CAMO_BEST`]: less the faster it moves, none just after it fires,
    /// throws, swings or is hit. What the client draws it at and what a
    /// bot makes out.
    pub fn cloak(&self) -> f32 {
        if self.camo == 0 || !self.active() {
            return 0.0;
        }
        let moving = ((self.body.vel.flat().len() - CAMO_WALK) / (CAMO_RUN - CAMO_WALK)).clamp(0.0, 1.0);
        let shown = self.flare as f32 / ticks(CAMO_FLARE_MS) as f32;
        CAMO_BEST * (1.0 - moving.max(shown).min(1.0))
    }

    /// Over full shields: the overshield, climbing or draining.
    pub fn overshield(&self) -> bool {
        self.shield > SHIELD_MAX || self.charging
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
        self.flag = false;
        self.hurt_by = 0;
        self.hurt_t = 0;
        self.ride = 0;
        self.lunge = 0;
        self.lunge_left = 0;
        self.lunge_melee = false;
        self.charge = 0;
        self.needles = 0;
        self.needle_t = 0;
        self.drop_left();
        self.stickies = STICKY_START;
        self.gren_kind = 0;
        self.charging = false;
        self.camo = 0;
        self.flare = 0;
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
        let shoots = seat != Some(crate::vehicle::DRIVER) && !self.flag;
        let on_foot = seat.is_none();

        let mut spare = (0u8, 0u16);
        dec(&mut self.refire);
        dec(&mut self.burst_gap);
        dec(&mut self.left_refire);
        dec(&mut self.melee_cool);
        dec(&mut self.gren_cool);
        // Hot guns cool, in a hand or on the back; one venting doesn't (the
        // vent is its reload).
        let c = self.cur as usize & 1;
        for i in 0..2 {
            let venting = i == c && self.reload > 0;
            cool(&mut self.guns[i], venting);
        }
        cool(&mut self.left, self.left_reload > 0);
        self.right().tick_reload();
        self.left_hand(&mut spare).tick_reload();

        // Swap: cancels a reload, a burst and a charge. Dual, it drops the
        // left gun first (and takes the back one, if there is one).
        if pressed & SWAP != 0 {
            let had_left = self.dual();
            if had_left {
                self.drop_left();
            }
            if self.guns[c ^ 1].arm != NONE {
                self.cur ^= 1;
            }
            if had_left || self.guns[c ^ 1].arm != NONE {
                self.reload = 0;
                self.burst_left = 0;
                self.charge = 0;
                self.refire = self.refire.max(ticks(SWAP_MS) as u16);
            }
        }

        // A hot gun past full vents (again, if a swap cut its vent short).
        self.right().vent();
        self.left_hand(&mut spare).vent();
        let dual = self.dual();
        let empty_pull = |g: &Gun, b: u16| g.arm != NONE && g.mag == 0 && inp.held(b);
        let want = pressed & RELOAD != 0 || empty_pull(self.gun(), FIRE) || empty_pull(&self.left, FIRE_LEFT);
        if want {
            // Dual, both hands reload together, and slower.
            let k = if dual { DUAL_RELOAD_K } else { 1.0 };
            self.right().start_reload(k);
            self.left_hand(&mut spare).start_reload(k);
        }

        let (fired, charged) = self.right().pull(shoots, inp.held(FIRE), pressed & FIRE != 0);
        out.fired = fired;
        out.charged = charged;
        self.shots = self.shots.wrapping_add(fired as u32);
        if dual {
            let (fired, charged) =
                self.left_hand(&mut spare)
                    .pull(shoots, inp.held(FIRE_LEFT), pressed & FIRE_LEFT != 0);
            out.fired_left = fired;
            out.charged_left = charged;
            self.shots = self.shots.wrapping_add(fired as u32);
        }

        if pressed & GREN_SWITCH != 0 {
            self.gren_kind ^= 1;
        }
        if on_foot && pressed & MELEE != 0 && self.melee_cool == 0 {
            // Dual, the left gun is dropped for the swing (Halo 2's).
            self.drop_left();
            out.melee = true;
            self.melee_cool = ticks(MELEE_COOL_MS) as u16;
            self.reload = 0;
            self.burst_left = 0;
            self.refire = self.refire.max(ticks(MELEE_COOL_MS / 2) as u16);
        }
        // A throw takes the kind chosen, or the other if none of it is left;
        // never with a gun in each hand.
        if on_foot && !self.flag && !self.dual() && pressed & GRENADE != 0 && self.gren_cool == 0 {
            let sticky = match self.gren_kind {
                1 => self.stickies > 0 || self.grenades == 0,
                _ => self.grenades == 0,
            };
            let n = if sticky { &mut self.stickies } else { &mut self.grenades };
            if *n > 0 {
                *n -= 1;
                out.throw = true;
                out.sticky = sticky;
                self.gren_cool = ticks(GREN_COOL_MS) as u16;
            }
        }
        out.use_pad = pressed & USE != 0;
        self.last_buttons = inp.buttons;
        out
    }
}

fn dec(t: &mut u16) {
    *t = t.saturating_sub(1);
}

/// A hot gun's tick of cooling, unless it is venting.
fn cool(g: &mut Gun, venting: bool) {
    let a = arm(g.arm);
    if a.hot() && !venting {
        g.heat = g.heat.saturating_sub(a.heat_cool);
    }
}

/// One hand: its gun and its timers, borrowed from the fighter (the right
/// hand's are the fighter's own fields; the left's beside them, and a gun
/// that goes in the left never bursts).
struct Hand<'a> {
    gun: &'a mut Gun,
    refire: &'a mut u16,
    reload: &'a mut u16,
    burst_left: &'a mut u8,
    burst_gap: &'a mut u16,
    charge: &'a mut u16,
}

impl Fighter {
    fn right(&mut self) -> Hand<'_> {
        Hand {
            gun: &mut self.guns[self.cur as usize & 1],
            refire: &mut self.refire,
            reload: &mut self.reload,
            burst_left: &mut self.burst_left,
            burst_gap: &mut self.burst_gap,
            charge: &mut self.charge,
        }
    }

    /// The left hand; `spare` stands in for the burst it never fires.
    fn left_hand<'a>(&'a mut self, spare: &'a mut (u8, u16)) -> Hand<'a> {
        Hand {
            gun: &mut self.left,
            refire: &mut self.left_refire,
            reload: &mut self.left_reload,
            burst_left: &mut spare.0,
            burst_gap: &mut spare.1,
            charge: &mut self.left_charge,
        }
    }
}

impl Hand<'_> {
    /// The reload (or a hot gun's vent) counts down; at 0 the magazine
    /// fills (or the gun is cold).
    fn tick_reload(&mut self) {
        if *self.reload == 0 {
            return;
        }
        *self.reload -= 1;
        if *self.reload == 0 {
            let a = arm(self.gun.arm);
            let g = &mut *self.gun;
            if a.hot() {
                g.heat = 0;
            } else {
                let take = (a.mag - g.mag).min(g.reserve);
                g.mag += take;
                g.reserve -= take;
            }
        }
    }

    /// A hot gun at full heat starts its vent.
    fn vent(&mut self) {
        let a = arm(self.gun.arm);
        if a.hot() && self.gun.heat >= HEAT_FULL && *self.reload == 0 {
            *self.reload = a.vent_ticks() as u16;
            *self.charge = 0;
        }
    }

    /// A reload, `k` times as long, if one can start.
    fn start_reload(&mut self, k: f32) {
        let a = arm(self.gun.arm);
        let g = *self.gun;
        if g.arm != NONE && g.mag < a.mag && g.reserve > 0 && *self.reload == 0 {
            *self.reload = (ticks(a.reload_ms) as f32 * k) as u16;
            *self.burst_left = 0;
        }
    }

    /// This hand's trigger, `held` and `pressed` this tick: whether a round
    /// left the gun, and whether it was a charged one. Spends it (ammo, a
    /// battery, heat).
    fn pull(&mut self, shoots: bool, held: bool, pressed: bool) -> (bool, bool) {
        let a = arm(self.gun.arm);
        let g = *self.gun;
        let (mut fired, mut charged) = (false, false);
        // A burst keeps firing once pulled; otherwise a semi-automatic
        // fires on the pull, an automatic while held.
        let ready = shoots && *self.reload == 0 && g.arm != NONE && g.mag > 0;
        if ready && *self.burst_left > 0 && *self.burst_gap == 0 {
            *self.burst_left -= 1;
            fired = true;
            if *self.burst_left == 0 {
                *self.refire = (*self.refire).max(ticks(a.refire_ms) as u16);
            } else {
                *self.burst_gap = ticks(a.burst_gap_ms) as u16;
            }
        } else if a.charge_ms > 0 {
            // A charge weapon fires on the release: charged if held to full.
            let full = ticks(a.charge_ms) as u16;
            if !ready || *self.refire > 0 {
                *self.charge = 0;
            } else if held {
                *self.charge = (*self.charge + 1).min(full);
            } else if *self.charge > 0 {
                fired = true;
                charged = *self.charge >= full && g.mag >= a.charge_cost;
                *self.charge = 0;
                *self.refire = ticks(a.refire_ms) as u16;
            }
        } else if ready && *self.burst_left == 0 && *self.refire == 0 && held && (a.auto || pressed) {
            fired = true;
            if a.burst > 1 {
                *self.burst_left = a.burst - 1;
                *self.burst_gap = ticks(a.burst_gap_ms) as u16;
            } else {
                *self.refire = ticks(a.refire_ms) as u16;
            }
        }
        if fired {
            let cost = match a.kind {
                Kind::Sword => 0,
                _ if charged => a.charge_cost,
                _ => 1,
            };
            let heat = if charged { a.charge_heat } else { a.heat_shot };
            let g = &mut *self.gun;
            g.mag -= cost.min(g.mag);
            g.heat = g.heat.saturating_add(heat.saturating_mul(128));
            if a.hot() && g.heat >= HEAT_FULL {
                *self.reload = a.vent_ticks() as u16;
                *self.burst_left = 0;
            }
            if a.kind == Kind::Rocket {
                *self.burst_left = 0;
            }
        } else if *self.burst_left > 0 && !ready {
            *self.burst_left = 0;
        }
        (fired, charged)
    }
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
            h.u16(g.heat);
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
        h.bool(self.lunge_melee);
        h.u16(self.charge);
        h.u8(self.needles);
        h.u16(self.needle_t);
        h.u8(self.left.arm);
        h.u16(self.left.mag);
        h.u16(self.left.reserve);
        h.u16(self.left.heat);
        h.u16(self.left_refire);
        h.u16(self.left_reload);
        h.u16(self.left_charge);
        h.u8(self.stickies);
        h.u8(self.gren_kind);
        h.bool(self.charging);
        h.u16(self.camo);
        h.u16(self.flare);
        // Only while true, so a mode without flags hashes as it did.
        if self.flag {
            h.u8(0xF1);
        }
        // Only on a map with a void, the only place it is set.
        if self.hurt_by != 0 {
            h.u8(0xF2);
            h.u8(self.hurt_by);
            h.u16(self.hurt_t);
        }
    }
}

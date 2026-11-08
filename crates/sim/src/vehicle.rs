//! Vehicles (docs/DESIGN.md §16, milestone 10), of three kinds ([`TRUCK`],
//! [`SKIMMER`], [`FLIER`]). The truck is a light four-wheeler with a
//! driver, who steers and can't shoot, and a passenger, who shoots. It
//! turns only about y; pitch and roll are the renderer's, from the ground
//! under its wheels, and never shape a hitbox. The skimmer (a hover bike)
//! and the flier are one-seaters whose pilot flies and fires their guns:
//! each turns toward where its pilot looks, the flier pitching too, as
//! Halo 2's and 3's light craft do. Their hitboxes turn about y only, as
//! the truck's. Getting in and out, damage, running people down and
//! blowing up are the world's (`World::step`); driving is
//! [`Vehicle::step_local`], what a driver's client predicts, reading only
//! the vehicle, the driver's stick ([`stick`]) and the map.
//!
//! A truck may carry a turret on its bed (the map says which: [`turret`]).
//! Then its passenger is its gunner: it fires the turret, never its own
//! guns. The turret's spin and heat are the gunner's (`Fighter::mount`), so
//! a gunner predicts them as it does its own gun; where it points is where
//! the gunner aims.

use crate::arms::Arm;
use crate::consts::*;
use crate::fmath::{self, fabs};
use crate::geom::{aim_dir, ray_aabb, yaw_sincos, Aabb, V3};
use crate::hash::Fnv;
use crate::input::{Input, CROUCH, JUMP};
use crate::map::Map;
use crate::movement::{quant, sweep, too_steep};
use crate::rewind::Posed;
use crate::thing::Thing;

/// Seats: the driver, then the passenger (a one-seater has none).
pub const DRIVER: usize = 0;
pub const PASSENGER: usize = 1;

/// What a vehicle is (`VehicleSpawn::kind`): the truck, the skimmer (a
/// one-seat hover bike) or the flier (a one-seat attack craft).
pub const TRUCK: u8 = 0;
pub const SKIMMER: u8 = 1;
pub const FLIER: u8 = 2;
/// Kinds, the truck among them.
pub const KINDS: u8 = 3;

/// A kind's size and build: half its length and width and its height (its
/// footprint, what meets a body), its hull circles (`hull_r`, one at the
/// middle and one `hull_off` fore and aft), the ledge it drives onto and
/// the drop it stays down over, how far its nose and tail ride a rise,
/// the footprint they read it over, its hit points, and its seats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spec {
    pub half_l: f32,
    pub half_w: f32,
    pub h: f32,
    pub hull_off: f32,
    pub hull_r: f32,
    pub step: f32,
    pub snap: f32,
    pub climb: f32,
    pub wheel_r: f32,
    pub health: i32,
    pub seats: usize,
}

static SPECS: [Spec; KINDS as usize] = [
    Spec {
        half_l: VEH_HALF_L,
        half_w: VEH_HALF_W,
        h: VEH_H,
        hull_off: VEH_HULL_OFF,
        hull_r: VEH_HULL_R,
        step: VEH_STEP,
        snap: VEH_SNAP,
        climb: VEH_CLIMB,
        wheel_r: VEH_WHEEL_R,
        health: VEH_HEALTH,
        seats: 2,
    },
    Spec {
        half_l: SKIM_HALF_L,
        half_w: SKIM_HALF_W,
        h: SKIM_H,
        hull_off: SKIM_HULL_OFF,
        hull_r: SKIM_HULL_R,
        step: VEH_STEP,
        snap: VEH_SNAP,
        climb: VEH_CLIMB,
        wheel_r: VEH_WHEEL_R,
        health: SKIM_HEALTH,
        seats: 1,
    },
    Spec {
        half_l: FLY_HALF_L,
        half_w: FLY_HALF_W,
        h: FLY_H,
        hull_off: 0.0,
        hull_r: FLY_HULL_R,
        step: FLY_STEP,
        snap: 0.0,
        climb: 0.0,
        wheel_r: FLY_HULL_R,
        health: FLY_HEALTH,
        seats: 1,
    },
];

/// Kind `kind`'s build (an unknown kind is a truck's).
pub fn spec(kind: u8) -> &'static Spec {
    SPECS.get(kind as usize).unwrap_or(&SPECS[0])
}

/// The buttons a one-seater reads from its pilot: CROUCH boosts, JUMP rolls
/// (the flier's).
pub const DRIVE_BUTTONS: u16 = CROUCH | JUMP;

/// What of driver input `inp` a hull of `kind` drives by: all of what
/// [`Vehicle::step_local`] reads, and what a passenger or someone by it is
/// sent to predict it on (`World::drive_stick`). A truck's is the stick
/// alone; a one-seater's, its pilot's aim and its two buttons too.
pub fn stick(kind: u8, inp: &Input) -> Input {
    if kind == TRUCK {
        Input {
            mx: inp.mx,
            mz: inp.mz,
            ..Input::default()
        }
    } else {
        Input {
            mx: inp.mx,
            mz: inp.mz,
            yaw: inp.yaw,
            pitch: inp.pitch,
            buttons: inp.buttons & DRIVE_BUTTONS,
        }
    }
}

/// What a truck's bed carries (`VehicleSpawn::turret`): nothing, its
/// passenger fighting with its own guns; or a turret its gunner fires.
pub const NO_TURRET: u8 = 0;
pub const MINIGUN: u8 = 1;
/// Turret kinds, nothing among them.
pub const TURRETS: u8 = 2;
/// The guns a one-seater's pilot fires (`Fighter::gunning`), worked as a
/// turret is: the skimmer's twin bolt guns, the flier's wing guns.
pub const BOLTS: u8 = 2;
pub const WING_BOLTS: u8 = 3;
/// Every gun a seat may work, bed turrets and pilots' guns.
pub const GUNS: u8 = 4;

/// The guns a one-seater of `kind` gives its pilot, or [`NO_TURRET`].
pub fn pilot_gun(kind: u8) -> u8 {
    match kind {
        SKIMMER => BOLTS,
        FLIER => WING_BOLTS,
        _ => NO_TURRET,
    }
}

/// The seat USE takes on a hull of `kind` carrying `turret` with `seats`
/// taken (a slot + 1, or 0), from `ahead` metres ahead of its middle (along
/// its heading; negative, behind): a turret's gun from beside its bed, else
/// the wheel first. A flag carrier only ever rides, so never a one-seater.
/// None: no room for it.
pub fn boards(kind: u8, turret: u8, seats: [u8; 2], ahead: f32, carrier: bool) -> Option<usize> {
    let free = |k: usize| seats[k] == 0;
    if spec(kind).seats == 1 {
        return (!carrier && free(DRIVER)).then_some(DRIVER);
    }
    let gun = turret != NO_TURRET && ahead < TURRET_BOARD_X;
    if free(PASSENGER) && (carrier || gun) {
        Some(PASSENGER)
    } else if carrier {
        None
    } else if free(DRIVER) {
        Some(DRIVER)
    } else if free(PASSENGER) {
        Some(PASSENGER)
    } else {
        None
    }
}

/// A turret's gun, and the cause its shots and kills carry.
pub fn turret(kind: u8) -> Option<(&'static Arm, u16)> {
    match kind {
        MINIGUN => Some((&crate::arms::MINIGUN, crate::events::Cause::Minigun as u16)),
        BOLTS => Some((&crate::arms::BOLTS, crate::events::Cause::Bolts as u16)),
        WING_BOLTS => Some((&crate::arms::WING_BOLTS, crate::events::Cause::WingBolts as u16)),
        _ => None,
    }
}

/// `at` turned toward `to` (1/65536 turn) the short way, by at most
/// `rad_s` radians a second's tick.
fn toward(at: u16, to: u16, rad_s: f32) -> u16 {
    let max = fmath::round(rad_s * DT * (65536.0 / core::f32::consts::TAU)) as i32;
    let d = (to.wrapping_sub(at) as i16 as i32).clamp(-max, max);
    at.wrapping_add(d as u16)
}

/// A pitch `at` toward `to`, likewise.
fn toward_pitch(at: i16, to: i16, rad_s: f32) -> i16 {
    let max = fmath::round(rad_s * DT * (65536.0 / core::f32::consts::TAU)) as i32;
    (at as i32 + (to as i32 - at as i32).clamp(-max, max)) as i16
}

/// `x` moved toward `to` by at most `step`.
fn approach(x: f32, to: f32, step: f32) -> f32 {
    x + (to - x).clamp(-step, step)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vehicle {
    pub active: bool,
    /// The middle of the hull, at the ground under it.
    pub pos: V3,
    pub vel: V3,
    /// Heading, in 1/65536 turn: forward is (cos, sin) in x and z.
    pub yaw: u16,
    pub grounded: bool,
    pub health: i32,
    /// Who sits in each seat: a fighter's slot + 1, or 0 for nobody.
    pub seats: [u8; 2],
    /// Ticks until a destroyed one is back at its spawn.
    pub respawn: u32,
    /// Its spawn on the map (`Map::vehicles`).
    pub home: u8,
    /// What its bed carries: [`NO_TURRET`], or a turret's kind.
    pub turret: u8,
    /// [`TRUCK`], [`SKIMMER`] or [`FLIER`]. The rest is a one-seater's.
    pub kind: u8,
    /// The flier's nose, 1/65536 turn up (within ±`FLY_PITCH_MAX`).
    pub pitch: i16,
    /// Ticks of boost left in the tank, ticks before it refills, and
    /// whether it boosted this tick.
    pub boost: u16,
    pub rest: u16,
    pub boosting: bool,
    /// A roll under way: ticks left of it, its sign the way it goes; ticks
    /// till the next may start; whether JUMP was down last tick.
    pub trick: i16,
    pub trick_cool: u16,
    pub jump: bool,
}

impl Thing for Vehicle {
    fn live(&self) -> bool {
        self.active
    }

    fn hash(&self, h: &mut Fnv) {
        h.bool(self.active);
        h.v3(self.pos);
        h.v3(self.vel);
        h.u16(self.yaw);
        h.bool(self.grounded);
        h.i32(self.health);
        h.u8(self.seats[0]);
        h.u8(self.seats[1]);
        h.u32(self.respawn);
        h.u8(self.home);
        // Only on a hull that has one, so a map without hashes as it did.
        if self.turret != NO_TURRET {
            h.u8(0xF1);
            h.u8(self.turret);
        }
        // Likewise a one-seater's own.
        if self.kind != TRUCK {
            h.u8(0xF2);
            h.u8(self.kind);
            h.u16(self.pitch as u16);
            h.u16(self.boost);
            h.u16(self.rest);
            h.bool(self.boosting);
            h.u16(self.trick as u16);
            h.u16(self.trick_cool);
            h.bool(self.jump);
        }
    }
}

impl Vehicle {
    /// A truck new at `pos` facing `yaw`, whole and empty, its bed
    /// carrying `turret`.
    pub fn new(pos: V3, yaw: u16, home: u8, turret: u8) -> Vehicle {
        Vehicle::new_kind(pos, yaw, home, turret, TRUCK)
    }

    /// [`Vehicle::new`] of any kind, a one-seater's tank full.
    pub fn new_kind(pos: V3, yaw: u16, home: u8, turret: u8, kind: u8) -> Vehicle {
        Vehicle {
            active: true,
            pos,
            yaw,
            grounded: true,
            health: spec(kind).health,
            home,
            turret,
            kind,
            boost: if kind == TRUCK { 0 } else { ticks(BOOST_MS) as u16 },
            ..Vehicle::default()
        }
    }

    pub fn spec(&self) -> &'static Spec {
        spec(self.kind)
    }

    /// Where the guns it is fired by draw a round aimed along unit `dir`
    /// from (the round itself flies from its gunner's eye): a turret's
    /// muzzle; a one-seater's guns, left and right by turns (`shot`'s
    /// parity).
    pub fn gun_at(&self, dir: V3, shot: u32) -> V3 {
        let (f, r) = self.axes();
        let side = if shot & 1 == 0 { 1.0 } else { -1.0 };
        match self.kind {
            SKIMMER => self.pos + f * (SKIM_HALF_L - 0.1) + r * (0.42 * side) + V3::new(0.0, 0.55, 0.0),
            FLIER => {
                let nose = aim_dir(self.yaw, self.pitch);
                self.pos + nose * 1.1 + r * (1.9 * side) + V3::new(0.0, FLY_WING_Y + 0.15, 0.0)
            }
            _ => self.muzzle(dir),
        }
    }

    /// Where the flier's bomb leaves it: ahead of and under its nose.
    pub fn bomb_at(&self) -> V3 {
        let nose = aim_dir(self.yaw, self.pitch);
        self.pos + nose * (FLY_HALF_L + 0.6) + V3::new(0.0, 0.45, 0.0)
    }

    /// Where its turret's barrels pivot, in the world, aimed along unit
    /// `dir`: out on its ring from the gunner, the way it aims (straight
    /// up or down, ahead of the hull).
    pub fn pivot(&self, dir: V3) -> V3 {
        let (f, _) = self.axes();
        let flat = dir.flat();
        let out = if flat.len() > 1e-3 { flat.norm() } else { f };
        self.pos + f * TURRET_GUNNER_X + out * TURRET_RING + V3::new(0.0, TURRET_Y, 0.0)
    }

    /// Where a turret aimed along unit `dir` fires from.
    pub fn muzzle(&self, dir: V3) -> V3 {
        self.pivot(dir) + dir * TURRET_REACH
    }

    /// Forward and right, flat.
    pub fn axes(&self) -> (V3, V3) {
        let (s, c) = yaw_sincos(self.yaw);
        (V3::new(c, 0.0, s), V3::new(-s, 0.0, c))
    }

    /// Who drives (a slot), if anyone.
    pub fn driver(&self) -> Option<usize> {
        (self.seats[DRIVER] != 0).then(|| self.seats[DRIVER] as usize - 1)
    }

    /// Where a fighter in `seat` is (its feet), by the hull: the driver
    /// in the cab's left seat, the passenger standing in the bed (behind
    /// the turret's post, if it has one).
    pub fn mount(&self, seat: usize) -> V3 {
        let (f, r) = self.axes();
        let at = if self.kind == SKIMMER {
            // Astride it, sitting up out of its cowl.
            self.pos + f * -0.35 + V3::new(0.0, 0.5, 0.0)
        } else if self.kind == FLIER {
            // Down in its cockpit, under its canopy's top.
            self.pos + f * 0.4 + V3::new(0.0, 0.15, 0.0)
        } else if seat == DRIVER {
            self.pos + f * 0.45 + r * -0.45 + V3::new(0.0, 0.4, 0.0)
        } else if self.turret != NO_TURRET {
            self.pos + f * TURRET_GUNNER_X + V3::new(0.0, 0.9, 0.0)
        } else {
            self.pos + f * -1.7 + V3::new(0.0, 0.9, 0.0)
        };
        V3::new(quant(at.x), quant(at.y), quant(at.z))
    }

    /// A rider's body in `seat`: at its mount, moving with the hull; the
    /// driver sitting (a crouched body's height: under the roof, its head
    /// behind the glass), the passenger on its feet, chest and head over
    /// the bed's sides. What the world and a rider's prediction both put
    /// there.
    pub fn seat_body(&self, seat: usize) -> crate::movement::Body {
        crate::movement::Body {
            pos: self.mount(seat),
            vel: self.vel,
            grounded: true,
            crouched: seat == DRIVER,
            duck: if seat == DRIVER { crate::consts::CROUCH_TICKS } else { 0 },
        }
    }

    /// What of the hull stops a shot, in its own frame (x forward, z right,
    /// y up from the ground): the body to the windows' sill, and the bed's
    /// sides behind the cab. Over them is glass, or nothing: a shot goes
    /// through to whoever sits or stands there.
    ///
    /// A skimmer's is its body to its cowl, its rider sitting up out of it;
    /// a flier's, its fuselage and its wings, its pilot inside.
    pub fn hull(kind: u8) -> [Aabb; 2] {
        match kind {
            SKIMMER => {
                let b = Aabb::new(
                    V3::new(-SKIM_HALF_L, 0.0, -SKIM_HALF_W),
                    V3::new(SKIM_HALF_L, SKIM_SILL, SKIM_HALF_W),
                );
                [b, b]
            }
            FLIER => [
                Aabb::new(
                    V3::new(-FLY_HALF_L, 0.0, -FLY_BODY_HALF_W),
                    V3::new(FLY_HALF_L, FLY_H, FLY_BODY_HALF_W),
                ),
                Aabb::new(
                    V3::new(-FLY_HALF_L * 0.6, FLY_WING_Y, -FLY_HALF_W),
                    V3::new(FLY_HALF_L * 0.3, FLY_WING_Y + FLY_WING_T, FLY_HALF_W),
                ),
            ],
            _ => {
                let lo = V3::new(-VEH_HALF_L, 0.0, -VEH_HALF_W);
                [
                    Aabb::new(lo, V3::new(VEH_HALF_L, VEH_SILL, VEH_HALF_W)),
                    Aabb::new(lo, V3::new(VEH_BED_X, VEH_BED_H, VEH_HALF_W)),
                ]
            }
        }
    }

    /// A point or direction from the world into the hull's frame.
    fn local(&self, p: V3, (f, r): (V3, V3)) -> V3 {
        V3::new(p.x * f.x + p.z * f.z, p.y, p.x * r.x + p.z * r.z)
    }

    /// Where a ray from `o` along unit `d` first meets the hull's steel,
    /// within `tmax`.
    pub fn ray(&self, o: V3, d: V3, tmax: f32) -> Option<f32> {
        let ax = self.axes();
        let (o, d) = (self.local(o - self.pos, ax), self.local(d, ax));
        let mut near = None;
        for b in Vehicle::hull(self.kind) {
            if let Some(t) = ray_aabb(o, d, &b, near.unwrap_or(tmax)) {
                near = Some(t);
            }
        }
        near
    }

    /// Whether a fighter's body (radius `r`, from `feet` up `h`) touches the
    /// hull.
    pub fn touches(&self, feet: V3, r: f32, h: f32) -> bool {
        let s = self.spec();
        let p = self.local(feet - self.pos, self.axes());
        fabs(p.x) < s.half_l + r && fabs(p.z) < s.half_w + r && p.y < s.h && p.y + h > 0.0
    }

    /// Its hull circles' middles: the nose's, the middle's, the tail's.
    pub fn circles(&self) -> [V3; 3] {
        let fore = self.axes().0 * self.spec().hull_off;
        [self.pos + fore, self.pos, self.pos - fore]
    }

    /// Moved flat by `d` if every hull circle is clear of the map there
    /// (another hull pushing it); false, and left where it is, if not.
    pub fn nudge(&mut self, d: V3, map: &Map) -> bool {
        let at = self.pos + d.flat();
        let at = V3::new(
            quant(at.x.clamp(map.lo.0, map.hi.0)),
            at.y,
            quant(at.z.clamp(map.lo.1, map.hi.1)),
        );
        let moved = Vehicle { pos: at, ..*self };
        let s = self.spec();
        let (lo, hi) = (at.y + s.step, at.y + s.h);
        if moved.circles().iter().any(|c| map.occupied(c.x, c.z, s.hull_r, lo, hi)) {
            return false;
        }
        self.pos = at;
        true
    }

    /// One tick with nobody at the wheel: on the ground it brakes to a
    /// stop and stays there (no rolling off down a slope); off it, it
    /// falls as a driven one does.
    pub fn step_parked(&mut self, map: &Map) {
        if !self.active {
            return;
        }
        if self.kind != TRUCK {
            // Nobody at its controls: its tank refills, and its trick runs
            // out.
            self.tank(&Input::default());
            self.tricks(false);
        }
        if self.grounded {
            let speed = self.vel.flat().len();
            if speed < VEH_PARK {
                self.vel = V3::new(0.0, self.vel.y, 0.0);
                if self.vel.y == 0.0 {
                    return;
                }
            } else {
                let k = (speed - VEH_BRAKE * DT).max(0.0) / speed;
                self.vel = V3::new(quant(self.vel.x * k), self.vel.y, quant(self.vel.z * k));
            }
        }
        match self.kind {
            // Unpiloted, it falls, its nose levelling.
            FLIER => {
                self.pitch = toward_pitch(self.pitch, 0, FLY_PITCH_RATE);
                let mut v = self.vel;
                if !self.grounded {
                    v.y -= GRAVITY * DT;
                }
                self.travel(v, self.axes().0, map);
            }
            // Holding its heading.
            SKIMMER => {
                let f = self.axes().0;
                let v = self.skim(
                    &Input {
                        yaw: self.yaw,
                        ..Input::default()
                    },
                    map,
                );
                self.travel(v, f, map);
            }
            _ => self.step_local(&Input::default(), map),
        }
    }

    /// A one-seater's boost tank under its pilot's `inp`: CROUCH held with
    /// the stick forward burns a tick of it; let go, it rests, then fills.
    /// Whether it boosts this tick.
    fn tank(&mut self, inp: &Input) -> bool {
        let on = inp.held(CROUCH) && inp.mz > 0 && self.boost > 0;
        if on {
            self.boost -= 1;
            self.rest = ticks(BOOST_REST_MS) as u16;
        } else if self.rest > 0 {
            self.rest -= 1;
        } else {
            self.boost = (self.boost + BOOST_REFILL).min(ticks(BOOST_MS) as u16);
        }
        self.boosting = on;
        on
    }

    /// A tick of the flier's tricks: the roll under way runs down, the
    /// next one's wait too, and JUMP's edge is kept (`jump`). Whether JUMP
    /// went down this tick.
    fn tricks(&mut self, jump: bool) -> bool {
        self.trick -= self.trick.signum();
        self.trick_cool = self.trick_cool.saturating_sub(1);
        let pressed = jump && !self.jump;
        self.jump = jump;
        pressed
    }

    /// One tick of driving under the driver's `inp`: throttle and brake
    /// along the heading, sideways grip,
    /// steering that bites with speed, the ground's slope, gravity off it,
    /// and the boxes in the way. Deterministic in (self, inp, map) alone.
    pub fn step_local(&mut self, inp: &Input, map: &Map) {
        if !self.active {
            return;
        }
        // Its circles stand along the heading it had before this tick's
        // turn.
        let f = self.axes().0;
        let v = match self.kind {
            SKIMMER => self.skim(inp, map),
            FLIER => self.fly(inp, map),
            _ => self.wheel(inp, map),
        };
        self.travel(v, f, map);
    }

    /// The truck's tick of speed under the driver's stick (`mx`, `mz`): its
    /// heading turned, the velocity [`Self::travel`] moves it by.
    fn wheel(&mut self, inp: &Input, map: &Map) -> V3 {
        let (f, r) = self.axes();
        let mut v = self.vel;
        if self.grounded {
            let throttle = inp.mz as f32 * (1.0 / 127.0);
            let steer = inp.mx as f32 * (1.0 / 127.0);
            let mut vf = v.dot(f);
            let mut vr = v.dot(r);
            let rate = if vf * throttle < 0.0 { VEH_BRAKE } else { VEH_ACCEL };
            vf = (vf + throttle * rate * DT).clamp(-VEH_REVERSE, VEH_TOP);
            vf -= vf * (VEH_ROLL * DT);
            vr -= vr * (VEH_GRIP * DT).min(1.0);
            // Turning bites with speed, and backs up the other way.
            let bite = (fabs(vf) / VEH_TURN_FULL).min(1.0);
            let turn = steer * VEH_TURN * DT * if vf < 0.0 { -bite } else { bite };
            let dyaw = fmath::round(turn * (65536.0 / core::f32::consts::TAU)) as i32;
            self.yaw = self.yaw.wrapping_add(dyaw as u16);
            v = f * vf + r * vr + V3::new(0.0, v.y, 0.0);
            if map.terrain.is_some() {
                // Down the slope (the ground normal's flat part points
                // downhill): the wheels roll with it, the tyres hold most
                // of it sideways.
                let n = map.ground_normal(self.pos.x, self.pos.z);
                let down = V3::new(n.x, 0.0, n.z) * (GRAVITY * DT);
                v = v + f * down.dot(f) + r * (down.dot(r) * VEH_SIDE_SLIP);
            }
        } else {
            v.y -= GRAVITY * DT;
        }
        v
    }

    /// The skimmer's tick of speed under its pilot's stick: its heading
    /// turned toward the aim (slower boosting), thrust along it, strafe
    /// across it (none boosting), a slide that dies away, and the slope's
    /// pull, on the ground; gravity off it.
    fn skim(&mut self, inp: &Input, map: &Map) -> V3 {
        let boosting = self.tank(inp);
        let turn = if boosting {
            SKIM_TURN * SKIM_BOOST_TURN_K
        } else {
            SKIM_TURN
        };
        self.yaw = toward(self.yaw, inp.yaw, turn);
        let (f, r) = self.axes();
        let mut v = self.vel;
        if !self.grounded {
            v.y -= GRAVITY * DT;
            return v;
        }
        let throttle = inp.mz as f32 * (1.0 / 127.0);
        let mut vf = v.dot(f);
        let mut vr = v.dot(r);
        let to = if boosting {
            SKIM_BOOST_TOP
        } else if throttle >= 0.0 {
            throttle * SKIM_TOP
        } else {
            throttle * SKIM_REVERSE
        };
        let rate = if vf * to < 0.0 {
            SKIM_BRAKE
        } else if to == 0.0 || fabs(vf) > fabs(to) {
            SKIM_DRAG
        } else if boosting {
            SKIM_BOOST_ACCEL
        } else {
            SKIM_ACCEL
        };
        vf = approach(vf, to, rate * DT);
        if inp.mx != 0 && !boosting {
            vr = approach(vr, inp.mx as f32 * (1.0 / 127.0) * SKIM_STRAFE, SKIM_STRAFE_ACCEL * DT);
        } else {
            vr -= vr * (SKIM_SLIP * DT).min(1.0);
        }
        v = f * vf + r * vr + V3::new(0.0, v.y, 0.0);
        if map.terrain.is_some() {
            // Nothing holds it across a slope: it slides down it half as
            // fast as it would roll.
            let n = map.ground_normal(self.pos.x, self.pos.z);
            let down = V3::new(n.x, 0.0, n.z) * (GRAVITY * DT * 0.5);
            v = v + down;
        }
        v
    }

    /// The flier's tick of speed under its pilot's stick: its nose turned
    /// toward the aim, yaw and pitch (slower boosting; never down into the
    /// ground it sits on), thrust along the nose, strafe flat across it,
    /// what goes neither way damped out (it hangs where it is let go), a
    /// roll on JUMP with a strafe, and the ceiling over its spawn.
    fn fly(&mut self, inp: &Input, map: &Map) -> V3 {
        let jumped = self.tricks(inp.held(JUMP));
        let boosting = self.tank(inp);
        let k = if boosting { FLY_BOOST_TURN_K } else { 1.0 };
        self.yaw = toward(self.yaw, inp.yaw, FLY_TURN * k);
        let want = inp.pitch.clamp(-FLY_PITCH_MAX, FLY_PITCH_MAX);
        let want = if self.grounded { want.max(0) } else { want };
        self.pitch = toward_pitch(self.pitch, want, FLY_PITCH_RATE * k);
        let nose = aim_dir(self.yaw, self.pitch);
        let (_, r) = self.axes();
        let up = r.cross(nose);
        let v = self.vel;
        let (mut vn, mut vr, mut vu) = (v.dot(nose), v.dot(r), v.dot(up));
        let throttle = inp.mz as f32 * (1.0 / 127.0);
        let to = if boosting {
            FLY_BOOST_TOP
        } else if throttle >= 0.0 {
            throttle * FLY_TOP
        } else {
            throttle * FLY_REVERSE
        };
        let rate = if vn * to < 0.0 || to == 0.0 || fabs(vn) > fabs(to) {
            FLY_BRAKE
        } else if boosting {
            FLY_BOOST_ACCEL
        } else {
            FLY_ACCEL
        };
        vn = approach(vn, to, rate * DT);
        if jumped && !self.grounded && self.trick_cool == 0 && inp.mx != 0 {
            let side = if inp.mx > 0 { 1.0 } else { -1.0 };
            vr = FLY_ROLL_KICK * side;
            self.trick = ticks(FLY_ROLL_MS) as i16 * side as i16;
            self.trick_cool = ticks(FLY_ROLL_COOL_MS) as u16;
        } else if self.trick == 0 {
            let strafe = if boosting {
                0.0
            } else {
                inp.mx as f32 * (1.0 / 127.0) * FLY_STRAFE
            };
            vr = approach(vr, strafe, FLY_STRAFE_ACCEL * DT);
        }
        vu -= vu * (FLY_DAMP * DT).min(1.0);
        let mut v = nose * vn + r * vr + up * vu;
        let ceiling = map
            .vehicles
            .get(self.home as usize)
            .map_or(f32::MAX, |d| d.pos.y + FLY_CEIL);
        if self.pos.y > ceiling {
            v.y = v.y.min(-FLY_CEIL_PUSH);
        }
        v
    }

    /// Moved a tick along `v` as far as the map lets it: each axis as far
    /// as every hull circle may go, out of the solids, then onto the floor
    /// (or off it). A truck's and a skimmer's nose and tail ride the floor
    /// under them; a slope too steep stops them. A flier meets a hillside
    /// higher than its belly as a wall, a box over it as a ceiling, and
    /// never snaps down to the ground.
    /// Its hull circles stand along `f`.
    fn travel(&mut self, mut v: V3, f: V3, map: &Map) {
        let s = self.spec();
        let flying = self.kind == FLIER;
        let mut p = self.pos;
        let fore = f * s.hull_off;
        // Each hull circle's own feet: the middle's are the hull's; the
        // nose's and tail's the floor under them where it is higher, so up
        // a ramp the nose is over its slope before the middle is (and over
        // the wall the ramp passes above). Never lower: over a hole or a
        // drop, an end is as high as the middle.
        let tilt = s.hull_off * s.climb;
        let mut feet = [p.y; 3];
        for (i, k) in [(0, 1.0), (2, -1.0)] {
            if flying {
                break;
            }
            let c = p + fore * k;
            let up = p.y + s.step + tilt;
            feet[i] = map.floor_with(c.x, c.z, s.wheel_r, up, up).clamp(p.y, p.y + tilt);
        }
        // Each axis as far as every hull circle may go.
        for axis in 0..2 {
            let d = if axis == 0 { v.x * DT } else { v.z * DT };
            if d == 0.0 {
                continue;
            }
            let mut go = d;
            let mut blocked = false;
            for (c, ft) in [(p + fore, feet[0]), (p, feet[1]), (p - fore, feet[2])] {
                let (lo, hi) = (ft + s.step, ft + s.h);
                let (from, other) = if axis == 0 { (c.x, c.z) } else { (c.z, c.x) };
                let rr = s.hull_r;
                let area = if axis == 0 {
                    (
                        (from.min(from + d) - rr, other - rr),
                        (from.max(from + d) + rr, other + rr),
                    )
                } else {
                    (
                        (other - rr, from.min(from + d) - rr),
                        (other + rr, from.max(from + d) + rr),
                    )
                };
                let to = if axis == 0 {
                    sweep(
                        map,
                        rr,
                        from,
                        from + d,
                        area,
                        |b| (b.min.x, b.max.x),
                        |b| b.over_z(other, rr) && b.min.y < hi && b.max.y > lo,
                    )
                } else {
                    sweep(
                        map,
                        rr,
                        from,
                        from + d,
                        area,
                        |b| (b.min.z, b.max.z),
                        |b| b.over_x(other, rr) && b.min.y < hi && b.max.y > lo,
                    )
                };
                // Compared as positions, as a fighter's sweep is: the
                // difference of two would round.
                if to != from + d {
                    blocked = true;
                    let got = to - from;
                    if fabs(got) < fabs(go) {
                        go = got;
                    }
                }
            }
            let next = if axis == 0 { (p.x + go, p.z) } else { (p.x, p.z + go) };
            let steep = map.terrain.is_some()
                && if flying {
                    map.ground(next.0, next.1) > p.y + s.step
                } else {
                    too_steep(map, (p.x, p.z), next, p.y)
                };
            if steep || blocked {
                // Stopped on this axis: a wall or a slope too steep.
                if axis == 0 {
                    v.x = 0.0;
                } else {
                    v.z = 0.0;
                }
            }
            if !steep {
                p.x = next.0.clamp(map.lo.0, map.hi.0);
                p.z = next.1.clamp(map.lo.1, map.hi.1);
            }
        }

        if !map.solids.is_empty() {
            // Each hull circle out of the solids, in its own band; the
            // vehicle moves with it.
            for (k, ft) in [(1.0, feet[0]), (0.0, feet[1]), (-1.0, feet[2])] {
                let c = p + fore * k;
                let was = self.pos + fore * k;
                let (mut cx, mut cz) = (c.x, c.z);
                map.push_out(
                    &mut cx,
                    &mut cz,
                    &mut v,
                    s.hull_r,
                    ft,
                    ft + s.step,
                    ft + s.h,
                    ft + s.step,
                    (was.x, was.z),
                );
                p.x = (p.x + (cx - c.x)).clamp(map.lo.0, map.hi.0);
                p.z = (p.z + (cz - c.z)).clamp(map.lo.1, map.hi.1);
            }
        }
        if flying {
            let floor = map.floor_with(p.x, p.z, FLY_BODY_HALF_W, p.y + s.step, p.y + s.step);
            let mut ny = p.y + v.y * DT;
            if v.y > 0.0 && map.occupied(p.x, p.z, FLY_BODY_HALF_W, ny + s.step, ny + s.h) {
                ny = p.y;
                v.y = 0.0;
            }
            if ny <= floor {
                p.y = floor;
                v.y = v.y.max(0.0);
                self.grounded = true;
            } else {
                p.y = ny;
                self.grounded = false;
            }
            self.pos = V3::new(quant(p.x), quant(p.y), quant(p.z));
            self.vel = V3::new(quant(v.x), quant(v.y), quant(v.z));
            return;
        }
        let floor = map.floor_with(p.x, p.z, s.half_w, p.y + s.step, p.y + s.step);
        let ny = p.y + v.y * DT;
        if ny <= floor {
            p.y = floor;
            v.y = v.y.max(0.0);
            self.grounded = true;
        } else if self.grounded && v.y <= 0.0 && ny - floor < s.snap {
            p.y = floor;
            v.y = 0.0;
        } else {
            p.y = ny;
            self.grounded = false;
        }
        self.pos = V3::new(quant(p.x), quant(p.y), quant(p.z));
        self.vel = V3::new(quant(v.x), quant(v.y), quant(v.z));
    }
}

/// What the rewind ring keeps of a vehicle each tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VPose {
    pub pos: V3,
    pub yaw: u16,
    pub alive: bool,
    /// Its kind: which hull a ray meets.
    pub kind: u8,
}

impl VPose {
    pub fn of(v: &Vehicle) -> VPose {
        VPose {
            pos: v.pos,
            yaw: v.yaw,
            alive: v.active,
            kind: v.kind,
        }
    }

    /// The vehicle as it stood: for testing a ray against its hull then.
    pub fn as_vehicle(&self) -> Vehicle {
        Vehicle {
            active: self.alive,
            pos: self.pos,
            yaw: self.yaw,
            kind: self.kind,
            ..Vehicle::default()
        }
    }
}

impl Posed for VPose {
    fn alive(&self) -> bool {
        self.alive
    }
    fn gone(self) -> Self {
        VPose { alive: false, ..self }
    }
    fn between(a: &Self, b: &Self, frac: f32) -> Self {
        // The short way round.
        let dy = b.yaw.wrapping_sub(a.yaw) as i16 as f32;
        let yaw = a.yaw.wrapping_add(fmath::round(dy * frac) as i32 as u16);
        VPose {
            pos: a.pos + (b.pos - a.pos) * frac,
            yaw,
            alive: true,
            kind: b.kind,
        }
    }
}

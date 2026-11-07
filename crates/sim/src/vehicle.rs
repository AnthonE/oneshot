//! Vehicles (docs/DESIGN.md §16, milestone 10): one kind so far, a light
//! four-wheeler with a driver, who steers and can't shoot, and a passenger,
//! who shoots. It turns only about y; pitch and roll are the renderer's,
//! from the ground under its wheels, and never shape a hitbox. Getting in
//! and out, damage, running people down and blowing up are the world's
//! (`World::step`); driving is [`Vehicle::step_local`], what a driver's
//! client predicts, reading only the vehicle, the driver's input and the
//! map.
//!
//! A truck may carry a turret on its bed (the map says which: [`turret`]).
//! Then its passenger is its gunner: it fires the turret, never its own
//! guns. The turret's spin and heat are the gunner's (`Fighter::mount`), so
//! a gunner predicts them as it does its own gun; where it points is where
//! the gunner aims.

use crate::arms::Arm;
use crate::consts::*;
use crate::fmath::{self, fabs};
use crate::geom::{ray_aabb, yaw_sincos, Aabb, V3};
use crate::hash::Fnv;
use crate::input::Input;
use crate::map::Map;
use crate::movement::{quant, sweep, too_steep};
use crate::rewind::Posed;
use crate::thing::Thing;

/// Seats: the driver, then the passenger.
pub const DRIVER: usize = 0;
pub const PASSENGER: usize = 1;

/// What a truck's bed carries (`VehicleSpawn::turret`): nothing, its
/// passenger fighting with its own guns; or a turret its gunner fires.
pub const NO_TURRET: u8 = 0;
pub const MINIGUN: u8 = 1;
/// Turret kinds, nothing among them.
pub const TURRETS: u8 = 2;

/// The seat USE takes on a hull carrying `turret` with `seats` taken (a
/// slot + 1, or 0), from `ahead` metres ahead of its middle (along its
/// heading; negative, behind): a turret's gun from beside its bed, else the
/// wheel first. A flag carrier only ever rides. None: no room for it.
pub fn boards(turret: u8, seats: [u8; 2], ahead: f32, carrier: bool) -> Option<usize> {
    let free = |k: usize| seats[k] == 0;
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
        _ => None,
    }
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
    }
}

impl Vehicle {
    /// New at `pos` facing `yaw`, whole and empty, its bed carrying
    /// `turret`.
    pub fn new(pos: V3, yaw: u16, home: u8, turret: u8) -> Vehicle {
        Vehicle {
            active: true,
            pos,
            yaw,
            grounded: true,
            health: VEH_HEALTH,
            home,
            turret,
            ..Vehicle::default()
        }
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
        let at = if seat == DRIVER {
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
    pub fn hull() -> [Aabb; 2] {
        let lo = V3::new(-VEH_HALF_L, 0.0, -VEH_HALF_W);
        [
            Aabb::new(lo, V3::new(VEH_HALF_L, VEH_SILL, VEH_HALF_W)),
            Aabb::new(lo, V3::new(VEH_BED_X, VEH_BED_H, VEH_HALF_W)),
        ]
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
        for b in Vehicle::hull() {
            if let Some(t) = ray_aabb(o, d, &b, near.unwrap_or(tmax)) {
                near = Some(t);
            }
        }
        near
    }

    /// Whether a fighter's body (radius `r`, from `feet` up `h`) touches the
    /// hull.
    pub fn touches(&self, feet: V3, r: f32, h: f32) -> bool {
        let p = self.local(feet - self.pos, self.axes());
        fabs(p.x) < VEH_HALF_L + r && fabs(p.z) < VEH_HALF_W + r && p.y < VEH_H && p.y + h > 0.0
    }

    /// Its hull circles' middles: the nose's, the middle's, the tail's.
    pub fn circles(&self) -> [V3; 3] {
        let fore = self.axes().0 * VEH_HULL_OFF;
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
        let (lo, hi) = (at.y + VEH_STEP, at.y + VEH_H);
        if moved
            .circles()
            .iter()
            .any(|c| map.occupied(c.x, c.z, VEH_HULL_R, lo, hi))
        {
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
        self.step_local(&Input::default(), map);
    }

    /// One tick of driving under the driver's `inp`: throttle and brake
    /// along the heading, sideways grip,
    /// steering that bites with speed, the ground's slope, gravity off it,
    /// and the boxes in the way. Deterministic in (self, inp, map) alone.
    pub fn step_local(&mut self, inp: &Input, map: &Map) {
        if !self.active {
            return;
        }
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

        let mut p = self.pos;
        let fore = f * VEH_HULL_OFF;
        // Each hull circle's own feet: the middle's are the hull's; the
        // nose's and tail's the floor under them where it is higher, so up
        // a ramp the nose is over its slope before the middle is (and over
        // the wall the ramp passes above). Never lower: over a hole or a
        // drop, an end is as high as the middle.
        let tilt = VEH_HULL_OFF * VEH_CLIMB;
        let mut feet = [p.y; 3];
        for (i, k) in [(0, 1.0), (2, -1.0)] {
            let c = p + fore * k;
            let up = p.y + VEH_STEP + tilt;
            feet[i] = map.floor_with(c.x, c.z, VEH_WHEEL_R, up, up).clamp(p.y, p.y + tilt);
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
                let (lo, hi) = (ft + VEH_STEP, ft + VEH_H);
                let (from, other) = if axis == 0 { (c.x, c.z) } else { (c.z, c.x) };
                let rr = VEH_HULL_R;
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
            let steep = map.terrain.is_some() && too_steep(map, (p.x, p.z), next, p.y);
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
                    VEH_HULL_R,
                    ft,
                    ft + VEH_STEP,
                    ft + VEH_H,
                    ft + VEH_STEP,
                    (was.x, was.z),
                );
                p.x = (p.x + (cx - c.x)).clamp(map.lo.0, map.hi.0);
                p.z = (p.z + (cz - c.z)).clamp(map.lo.1, map.hi.1);
            }
        }
        let floor = map.floor_with(p.x, p.z, VEH_HALF_W, p.y + VEH_STEP, p.y + VEH_STEP);
        let ny = p.y + v.y * DT;
        if ny <= floor {
            p.y = floor;
            v.y = v.y.max(0.0);
            self.grounded = true;
        } else if self.grounded && v.y <= 0.0 && ny - floor < VEH_SNAP {
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
}

impl VPose {
    pub fn of(v: &Vehicle) -> VPose {
        VPose {
            pos: v.pos,
            yaw: v.yaw,
            alive: v.active,
        }
    }

    /// The vehicle as it stood: for testing a ray against its hull then.
    pub fn as_vehicle(&self) -> Vehicle {
        Vehicle {
            active: self.alive,
            pos: self.pos,
            yaw: self.yaw,
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
        }
    }
}

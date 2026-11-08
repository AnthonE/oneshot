//! The contact step: the physical part of a tick, as a few stages over a
//! [`Scene`] (bodies and hulls), each a pure function of the scene, the
//! inputs and the map. The server runs them inside `World::play` with its
//! own decisions between (a crash's damage, the void, a run-down's
//! damage); a client runs the same stages, in the same order, on what it
//! predicts (`client_net::core::step_near`), and decides nothing.

use crate::consts::*;
use crate::fighter::Fighter;
use crate::geom::{yaw_sincos, V3};
use crate::input::Input;
use crate::map::Map;
use crate::movement::{quant, Body};
use crate::solid::{Shape, Solid};
use crate::vehicle::Vehicle;

/// How far into a hull's top a body coming down may be and still land on
/// it (past this tick's fall), as a floor's step would take it.
pub const TOP_EPS: f32 = 0.05;

/// Hull `v`'s top on the sim's grid, at or above it: where a body on it
/// stands, never a hair inside it (where the hull would meet it).
pub fn top(v: &Vehicle) -> f32 {
    crate::fmath::ceil((v.pos.y + v.spec().h) * QUANT) * (1.0 / QUANT)
}

/// What of a fighter the contact step reads and moves.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Man {
    pub body: Body,
    /// `Fighter::ride`: 0 on foot.
    pub ride: u8,
    /// Seated and alive (`Fighter::active`).
    pub live: bool,
    /// Carrying a flag: walks at `FLAG_PACE`.
    pub flag: bool,
    pub team: u8,
}

impl Man {
    pub fn of(f: &Fighter) -> Man {
        Man {
            body: f.body,
            ride: f.ride,
            live: f.active(),
            flag: f.flag,
            team: f.team,
        }
    }

    pub fn riding(&self) -> Option<(usize, usize)> {
        (self.ride != 0).then(|| ((self.ride as usize - 1) / 2, (self.ride as usize - 1) % 2))
    }
}

/// A body's walk under `inp`, at `FLAG_PACE` carrying a flag: what
/// `Fighter::step_local` walks by, and the contact step walks others by.
pub fn walk(body: &mut Body, flag: bool, inp: &Input, map: &Map) {
    let pace = if flag { FLAG_PACE } else { 1.0 };
    crate::movement::step_paced(body, inp, map, pace);
}

/// Each hull's worst crash this tick, and whose doing.
pub type Crashes = [(i32, Option<usize>); MAX_VEHICLES];

/// Each hull's stick (`World::drive_stick`: `vehicle::stick`), or None:
/// parked.
pub type Sticks = [Option<Input>; MAX_VEHICLES];

/// What the stages can't know: where a driver saw someone, and what a
/// run-down does to them (the server's damage). A client's is [`Present`].
pub trait Witness {
    /// Where seat `q` stood as hull `i`'s driver saw it, if not where it is.
    fn then(&self, i: usize, q: usize) -> Option<V3>;
    /// Hull `i` ran seat `q` down closing at `closing`; `m` is its body,
    /// already shoved. The server hurts it and puts back what that left.
    fn run_down(&mut self, i: usize, q: usize, closing: f32, m: &mut Man);
}

/// A client's witness: everyone met where they are, nobody hurt.
pub struct Present;

impl Witness for Present {
    fn then(&self, _: usize, _: usize) -> Option<V3> {
        None
    }

    fn run_down(&mut self, _: usize, _: usize, _: f32, _: &mut Man) {}
}

/// The bodies and hulls the stages move: every seat's and every slot's (a
/// client leaves what it doesn't predict not live, or inactive).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scene {
    pub men: [Man; MAX_FIGHTERS],
    pub hulls: [Vehicle; MAX_VEHICLES],
    /// Teammates never run each other down (sides, friendly fire off).
    pub spare_mates: bool,
}

impl Default for Scene {
    fn default() -> Scene {
        Scene {
            men: [Man::default(); MAX_FIGHTERS],
            hulls: [Vehicle::default(); MAX_VEHICLES],
            spare_mates: false,
        }
    }
}

impl Scene {
    pub fn take(f: &[Fighter; MAX_FIGHTERS], v: &[Vehicle; MAX_VEHICLES], spare_mates: bool) -> Scene {
        let mut men = [Man::default(); MAX_FIGHTERS];
        for (m, f) in men.iter_mut().zip(f) {
            *m = Man::of(f);
        }
        Scene {
            men,
            hulls: *v,
            spare_mates,
        }
    }

    /// What the stages move, back: every body, and each live hull's place,
    /// speed, heading and footing, and a one-seater's nose, tank and roll.
    pub fn put(&self, f: &mut [Fighter; MAX_FIGHTERS], v: &mut [Vehicle; MAX_VEHICLES]) {
        for (f, m) in f.iter_mut().zip(&self.men) {
            f.body = m.body;
        }
        for (v, h) in v.iter_mut().zip(&self.hulls) {
            if v.active {
                v.pos = h.pos;
                v.vel = h.vel;
                v.yaw = h.yaw;
                v.grounded = h.grounded;
                v.pitch = h.pitch;
                v.boost = h.boost;
                v.rest = h.rest;
                v.boosting = h.boosting;
                v.trick = h.trick;
                v.trick_cool = h.trick_cool;
                v.jump = h.jump;
            }
        }
    }

    fn spared(&self, d: usize, q: usize) -> bool {
        self.spare_mates && q != d && self.men[q].team == self.men[d].team
    }

    /// The live ones on foot in `who` (a bit a seat) walk under their
    /// inputs, as their own ticks would walk them.
    pub fn walk(&mut self, inp: &[Input; MAX_FIGHTERS], who: u16, map: &Map) {
        for (q, (m, i)) in self.men.iter_mut().zip(inp).enumerate() {
            if who & (1 << q) != 0 && m.live && m.ride == 0 {
                walk(&mut m.body, m.flag, i, map);
            }
        }
    }

    /// Each hull under the stick it is driven by (its live driver's,
    /// `vehicle::stick`: all of an input a hull reads); with none, parked.
    pub fn drive(&mut self, sticks: &Sticks, map: &Map) {
        for (h, st) in self.hulls.iter_mut().zip(sticks) {
            if !h.active {
                continue;
            }
            match st {
                Some(st) => h.step_local(&crate::vehicle::stick(h.kind, st), map),
                None => h.step_parked(map),
            }
        }
    }

    /// Hulls that meet push apart along where their circles touch, half
    /// each (all of it the other's if one is against a wall), and trade
    /// their closing speed, a little of it kept as a bounce. What a hard
    /// crash would deal ([`VEH_CRASH_SPEED`]) is returned: the damage is
    /// the caller's.
    pub fn crash(&mut self, map: &Map) -> Crashes {
        let mut crash: Crashes = [(0, None); MAX_VEHICLES];
        for i in 0..MAX_VEHICLES {
            for j in i + 1..MAX_VEHICLES {
                let (a, b) = (self.hulls[i], self.hulls[j]);
                if !a.active || !b.active {
                    continue;
                }
                let (sa, sb) = (a.spec(), b.spec());
                // Clear over or under the other.
                if a.pos.y - b.pos.y >= sb.h || b.pos.y - a.pos.y >= sa.h {
                    continue;
                }
                let reach = sa.hull_r + sb.hull_r;
                // The deepest of the circles' overlaps, and its way from a
                // to b.
                let mut deep: Option<(f32, V3)> = None;
                for ca in a.circles() {
                    for cb in b.circles() {
                        let d = (cb - ca).flat();
                        let l2 = d.dot(d);
                        if l2 >= reach * reach {
                            continue;
                        }
                        let l = l2.sqrt();
                        let n = if l > 1e-4 {
                            d * (1.0 / l)
                        } else {
                            let m = (b.pos - a.pos).flat();
                            if m.dot(m) > 1e-8 {
                                m.norm()
                            } else {
                                V3::new(1.0, 0.0, 0.0)
                            }
                        };
                        if deep.is_none_or(|(p, _)| reach - l > p) {
                            deep = Some((reach - l, n));
                        }
                    }
                }
                let Some((pen, n)) = deep else {
                    continue;
                };
                let closing = (a.vel - b.vel).flat().dot(n);
                if closing > VEH_CRASH_SPEED {
                    let pts = ((closing - VEH_CRASH_SPEED) * VEH_CRASH_DMG) as i32 * PT;
                    crash[i] = crash[i].max((pts, b.driver().or(a.driver())));
                    crash[j] = crash[j].max((pts, a.driver().or(b.driver())));
                }
                if closing > 0.0 {
                    let k = closing * (1.0 + VEH_BOUNCE) * 0.5;
                    for (q, s) in [(i, -k), (j, k)] {
                        let v = &mut self.hulls[q];
                        let to = v.vel + n * s;
                        v.vel = V3::new(quant(to.x), v.vel.y, quant(to.z));
                    }
                }
                let half = n * (pen * 0.5);
                let a_went = self.hulls[i].nudge(-half, map);
                let b_went = self.hulls[j].nudge(half, map);
                if !a_went {
                    self.hulls[j].nudge(half * 2.0, map);
                } else if !b_went {
                    self.hulls[i].nudge(-half * 2.0, map);
                }
            }
        }
        crash
    }

    /// Everyone riding at their seat.
    pub fn seat(&mut self) {
        for m in self.men.iter_mut() {
            if let Some((i, seat)) = m.riding() {
                m.body = self.hulls[i].seat_body(seat);
            }
        }
    }

    /// A hull runs down whoever on foot it meets closing faster than
    /// [`SPLAT_SPEED`]: carried off ahead of it. Met where they are, or
    /// where its driver saw them (the witness's `then`); one who runs into
    /// its flank isn't run down. What else it does is the witness's (the
    /// server's damage: `World`'s `Rewound`).
    pub fn run_down(&mut self, w: &mut impl Witness) {
        for i in 0..MAX_VEHICLES {
            let v = self.hulls[i];
            if !v.active || v.vel.flat().len() < SPLAT_SPEED {
                continue;
            }
            let driver = v.driver();
            for q in 0..MAX_FIGHTERS {
                let f = self.men[q];
                if !f.live || f.ride != 0 || driver.is_some_and(|d| self.spared(d, q)) {
                    continue;
                }
                let h = f.body.height();
                // One on its top rides it out, never run down by it.
                let roof = top(&v) - TOP_EPS;
                let meets = |p: V3| v.touches(p, BODY_R, h) && p.y < roof;
                let met = if meets(f.body.pos) {
                    f.body.pos
                } else if let Some(p) = w.then(i, q).filter(|&p| meets(p)) {
                    p
                } else {
                    continue;
                };
                let off = (met - v.pos).flat();
                let away = if off.dot(off) > 1e-6 { off.norm() } else { v.axes().0 };
                // Its own speed at them, less theirs away from it.
                let closing = v.vel.flat().dot(away) - f.body.vel.flat().dot(away).max(0.0);
                if closing < SPLAT_SPEED {
                    continue;
                }
                let b = &mut self.men[q].body;
                // On the sim's grid, as every body's speed is.
                let to = b.vel + away * (closing + SPLAT_SHOVE) + V3::new(0.0, 3.0, 0.0);
                b.vel = V3::new(quant(to.x), quant(to.y), quant(to.z));
                b.grounded = false;
                w.run_down(i, q, closing, &mut self.men[q]);
            }
        }
    }

    /// Solid hulls ([`SOLID_HULLS`]): everyone live on foot out of each
    /// hull's footprint the nearest way, its speed into the face taken off,
    /// as a wall's. One the push would put into the map (pinned between a
    /// hull and a wall) is left where it is, unhurt; a hull never gives way
    /// to a body, so its motion is its driver's alone.
    pub fn push_out(&mut self, map: &Map) {
        for i in 0..MAX_VEHICLES {
            let v = self.hulls[i];
            if !v.active {
                continue;
            }
            let sp = v.spec();
            let reach = sp.half_l + sp.half_w + BODY_R;
            let (s, c) = yaw_sincos(v.yaw);
            let hull = Solid {
                shape: Shape::Turned {
                    x: v.pos.x,
                    z: v.pos.z,
                    hx: sp.half_l,
                    hz: sp.half_w,
                    c,
                    s,
                },
                y0: v.pos.y,
                y1: v.pos.y + sp.h,
            };
            let top = top(&v);
            for m in self.men.iter_mut() {
                if !m.live || m.ride != 0 {
                    continue;
                }
                let (feet, h) = (m.body.pos.y, m.body.height());
                let off = (m.body.pos - v.pos).flat();
                if feet >= top + TOP_EPS || feet + h <= hull.y0 || off.dot(off) >= reach * reach {
                    continue;
                }
                let Some((nx, nz, ux, uz)) = hull.push(m.body.pos.x, m.body.pos.z, BODY_R) else {
                    continue;
                };
                // On its top, or come down onto it from above (no deeper than
                // this tick's fall): stood there, every tick, never through it
                // and out a side.
                if m.body.vel.y <= 0.0 && top - feet <= -m.body.vel.y * DT + TOP_EPS {
                    let b = &mut m.body;
                    b.pos.y = top;
                    b.vel = V3::new(b.vel.x, 0.0, b.vel.z);
                    b.grounded = true;
                    continue;
                }
                // Above it, going up: clear of it.
                if feet >= hull.y1 {
                    continue;
                }
                let x = quant(nx.clamp(map.lo.0, map.hi.0));
                let z = quant(nz.clamp(map.lo.1, map.hi.1));
                if map.occupied(x, z, BODY_R, feet + STEP, feet + h) {
                    continue;
                }
                let b = &mut m.body;
                b.pos.x = x;
                b.pos.z = z;
                let into = b.vel.x * ux + b.vel.z * uz;
                if into < 0.0 {
                    b.vel = V3::new(quant(b.vel.x - ux * into), b.vel.y, quant(b.vel.z - uz * into));
                }
            }
        }
    }
}

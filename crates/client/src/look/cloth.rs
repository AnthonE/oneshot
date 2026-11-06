//! A flag's cloth: a sheet of particles stepped by position-based verlet,
//! its hoist edge pinned down a pole, blown by the map's wind and by the
//! air its pole moves through. Drawing only: the sim never sees it, so it
//! steps on the frame's clock, at a fixed substep so it is as stable at
//! 30 fps as at 240.
//!
//! - **Pinned, not parented.** The particles live in world space; each
//!   frame the hoist column is put where the pole now is and the rest
//!   follows by its constraints. A carrier running, a hull at speed or a
//!   camera turning drags the sheet through the air, and the air pushes
//!   back (the relative wind below), so it streams behind whoever has it.
//! - **Aerodynamics per quad:** the wind relative to the quad's own
//!   velocity, the part of it along the quad's normal, squared, pushes the
//!   quad along that normal (a flat plate's pressure drag), plus a little
//!   skin friction along it. That is what makes a sheet flutter: a flat
//!   flag in a steady wind is unstable, and ripples run down it on their
//!   own.
//! - **Gusts:** bands travelling downwind and a waver across it, the
//!   foliage shader's (`foliage_common.wgsl`), so a flag and the grass
//!   under it lean on the same breath.

use bevy::prelude::*;

/// Particles along the fly (away from the pole) and down the hoist.
pub const NX: usize = 16;
pub const NY: usize = 10;
const N: usize = NX * NY;

/// The substep, seconds, and the most of them one frame steps (a long
/// frame is let go rather than caught up).
const DT: f32 = 1.0 / 180.0;
const MAX_SUB: usize = 8;
/// Constraint passes per substep.
const ITERS: usize = 4;
/// Of the velocity kept each substep (air and the cloth's own damping).
const KEEP: f32 = 0.996;
/// The cloth's weight, kg per square metre (a heavy nylon), and the air's
/// density times a flat plate's drag coefficient over two.
const AREAL: f32 = 0.22;
const PRESSURE: f32 = 0.5 * 1.2 * 1.15;
/// Skin friction along the sheet, per m/s of the wind across it.
const FRICTION: f32 = 0.03;
/// How hard a bend (one particle over) is held, `0..=1`: a soft crease;
/// and a shear (the diagonals): weave gives that way, so a flag in still
/// air hangs down its pole instead of standing out as a plate.
const BEND_K: f32 = 0.25;
const SHEAR_K: f32 = 0.04;
/// A pin moving farther than this in one frame was a teleport (a return,
/// a respawn): the sheet is hung again, not dragged there.
const TELEPORT_M: f32 = 4.0;

/// A sheet `w` by `h`.
pub struct Cloth {
    p: [Vec3; N],
    q: [Vec3; N],
    w: f32,
    h: f32,
    /// Unspent time, seconds; the gust clock.
    acc: f32,
    t: f32,
    hung: bool,
}

/// What blows on a sheet this frame.
#[derive(Clone, Copy)]
pub struct Air {
    /// The wind, m/s, world space (horizontal).
    pub wind: Vec3,
    /// The ground under the sheet, metres: it lies on it rather than
    /// through it.
    pub floor: f32,
}

#[inline]
fn at(i: usize, j: usize) -> usize {
    j * NX + i
}

impl Cloth {
    pub fn new(w: f32, h: f32) -> Cloth {
        Cloth {
            p: [Vec3::ZERO; N],
            q: [Vec3::ZERO; N],
            w,
            h,
            acc: 0.0,
            t: 0.0,
            hung: false,
        }
    }

    fn dx(&self) -> f32 {
        self.w / (NX - 1) as f32
    }

    fn dy(&self) -> f32 {
        self.h / (NY - 1) as f32
    }

    /// The sheet laid out flat from `pins` along `fly`, at rest.
    fn hang(&mut self, pins: &[Vec3; NY], fly: Vec3) {
        let dx = self.dx();
        for (j, pin) in pins.iter().enumerate() {
            for i in 0..NX {
                let v = *pin + fly * (dx * i as f32);
                self.p[at(i, j)] = v;
                self.q[at(i, j)] = v;
            }
        }
        self.hung = true;
    }

    /// Steps the sheet `dt` seconds on: its hoist edge at `pins` (top
    /// first, `dy` apart), `fly` the way the sheet hangs out if it has to
    /// be hung again.
    pub fn step(&mut self, dt: f32, pins: &[Vec3; NY], fly: Vec3, air: Air) {
        if !self.hung || (self.p[0] - pins[0]).length() > TELEPORT_M {
            self.hang(pins, fly);
        }
        // The pins move across the frame in a line, substep by substep, so
        // a fast pole doesn't yank its column.
        let from: [Vec3; NY] = std::array::from_fn(|j| self.p[at(0, j)]);
        self.acc = (self.acc + dt).min(DT * MAX_SUB as f32);
        let subs = (self.acc / DT) as usize;
        for k in 0..subs {
            self.acc -= DT;
            self.t += DT;
            let u = (k + 1) as f32 / subs as f32;
            for j in 0..NY {
                let pin = from[j].lerp(pins[j], u);
                self.q[at(0, j)] = self.p[at(0, j)];
                self.p[at(0, j)] = pin;
            }
            self.sub(air);
        }
        if self.t > 4096.0 {
            self.t -= 4096.0;
        }
    }

    fn sub(&mut self, air: Air) {
        let (dx, dy) = (self.dx(), self.dy());
        let quad = dx * dy;
        let mass = AREAL * quad;
        let mut acc = [Vec3::new(0.0, -9.81, 0.0); N];
        // Gusts: the foliage's bands, wavering across the wind.
        let speed = air.wind.length();
        let dir = if speed > 1e-3 { air.wind / speed } else { Vec3::X };
        let perp = Vec3::new(-dir.z, 0.0, dir.x);
        let t = self.t;
        for j in 0..NY - 1 {
            for i in 0..NX - 1 {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
                let mid = (self.p[a] + self.p[b] + self.p[c] + self.p[d]) * 0.25;
                let vel = (self.p[a] - self.q[a] + self.p[b] - self.q[b] + self.p[c] - self.q[c] + self.p[d]
                    - self.q[d])
                    * (0.25 / DT);
                let along = mid.x * dir.x + mid.z * dir.z;
                let across = mid.x * perp.x + mid.z * perp.z;
                let gust = 0.7 + 0.3 * (along * 0.11 - t * 0.9 + (across * 0.05 + t * 0.13).sin() * 1.5).sin();
                let swirl = perp * (0.12 * speed * (t * 2.3 + along * 0.6).sin())
                    + Vec3::Y * (0.06 * speed * (t * 3.1 + across * 0.8).sin());
                let rel = dir * (speed * gust) + swirl - vel;
                let cross = (self.p[d] - self.p[a]).cross(self.p[b] - self.p[c]);
                let area = cross.length() * 0.5;
                if area < 1e-6 {
                    continue;
                }
                let n = cross / (2.0 * area);
                let vn = rel.dot(n);
                let f = n * (PRESSURE * area * vn * vn.abs()) + (rel - n * vn) * (FRICTION * area);
                let a4 = f / (4.0 * mass);
                for k in [a, b, c, d] {
                    acc[k] += a4;
                }
            }
        }
        let dt2 = DT * DT;
        for j in 0..NY {
            for i in 1..NX {
                let k = at(i, j);
                let v = (self.p[k] - self.q[k]) * KEEP;
                self.q[k] = self.p[k];
                self.p[k] += v + acc[k] * dt2;
            }
        }
        let diag = (dx * dx + dy * dy).sqrt();
        for _ in 0..ITERS {
            for j in 0..NY {
                for i in 0..NX {
                    if i + 1 < NX {
                        self.hold(at(i, j), at(i + 1, j), dx, 1.0);
                    }
                    if j + 1 < NY {
                        self.hold(at(i, j), at(i, j + 1), dy, 1.0);
                    }
                    if i + 1 < NX && j + 1 < NY {
                        self.hold(at(i, j), at(i + 1, j + 1), diag, SHEAR_K);
                        self.hold(at(i + 1, j), at(i, j + 1), diag, SHEAR_K);
                    }
                    if i + 2 < NX {
                        self.hold(at(i, j), at(i + 2, j), 2.0 * dx, BEND_K);
                    }
                    if j + 2 < NY {
                        self.hold(at(i, j), at(i, j + 2), 2.0 * dy, BEND_K);
                    }
                }
            }
        }
        for k in 0..N {
            if self.p[k].y < air.floor {
                self.p[k].y = air.floor;
                // Lying on the floor, it drags.
                let v = self.p[k] - self.q[k];
                self.q[k] = self.p[k] - Vec3::new(v.x * 0.6, 0.0, v.z * 0.6);
            }
        }
    }

    /// Pulls `a` and `b` toward `rest` apart; column 0 is pinned.
    fn hold(&mut self, a: usize, b: usize, rest: f32, k: f32) {
        let wa = if a.is_multiple_of(NX) { 0.0 } else { 1.0 };
        let wb = if b.is_multiple_of(NX) { 0.0 } else { 1.0 };
        let w = wa + wb;
        if w == 0.0 {
            return;
        }
        let d = self.p[b] - self.p[a];
        let len = d.length();
        if len < 1e-6 {
            return;
        }
        let c = d * (k * (len - rest) / (len * w));
        self.p[a] += c * wa;
        self.p[b] -= c * wb;
    }

    /// Positions and smooth normals into `pos`/`nor` (each `N` long).
    pub fn write(&self, pos: &mut Vec<[f32; 3]>, nor: &mut Vec<[f32; 3]>) {
        pos.clear();
        nor.clear();
        for j in 0..NY {
            for i in 0..NX {
                pos.push(self.p[at(i, j)].to_array());
                let l = self.p[at(i.saturating_sub(1), j)];
                let r = self.p[at((i + 1).min(NX - 1), j)];
                let u = self.p[at(i, j.saturating_sub(1))];
                let d = self.p[at(i, (j + 1).min(NY - 1))];
                let n = (r - l).cross(u - d).normalize_or(Vec3::Z);
                nor.push(n.to_array());
            }
        }
    }

    /// The sheet's middle: for a test, or anything that wants to know
    /// where it streams.
    pub fn middle(&self) -> Vec3 {
        self.p.iter().copied().sum::<Vec3>() / N as f32
    }
}

/// The sheet's triangles and UVs (u along the fly from the pole, v down
/// the hoist), for a mesh of `NX` by `NY`.
pub fn grid() -> (Vec<u16>, Vec<[f32; 2]>) {
    let mut idx = Vec::with_capacity((NX - 1) * (NY - 1) * 6);
    for j in 0..NY - 1 {
        for i in 0..NX - 1 {
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
            idx.extend([a, c, b, b, c, d].map(|k| k as u16));
        }
    }
    let uv = (0..N)
        .map(|k| [(k % NX) as f32 / (NX - 1) as f32, (k / NX) as f32 / (NY - 1) as f32])
        .collect();
    (idx, uv)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pins(top: Vec3, dy: f32) -> [Vec3; NY] {
        std::array::from_fn(|j| top - Vec3::Y * (dy * j as f32))
    }

    fn run(wind: Vec3, secs: f32) -> Cloth {
        let mut c = Cloth::new(1.0, 0.6);
        let p = pins(Vec3::new(0.0, 3.0, 0.0), c.dy());
        let air = Air { wind, floor: 0.0 };
        for _ in 0..(secs * 60.0) as usize {
            c.step(1.0 / 60.0, &p, Vec3::X, air);
        }
        c
    }

    #[test]
    fn still_air_it_hangs_down_the_pole() {
        let c = run(Vec3::ZERO, 6.0);
        let m = c.middle();
        assert!(m.x.abs() < 0.2, "hangs by the pole: {m}");
        assert!(m.y < 2.6, "hangs down: {m}");
        assert!(c.p.iter().all(|p| p.is_finite()));
    }

    #[test]
    fn a_wind_streams_it_downwind() {
        let c = run(Vec3::new(0.0, 0.0, 8.0), 6.0);
        let m = c.middle();
        assert!(m.z > 0.3, "streams downwind: {m}");
        assert!(c.p.iter().all(|p| p.is_finite()));
        // Its constraints hold: no edge stretched past half again.
        let dx = c.dx();
        for j in 0..NY {
            for i in 0..NX - 1 {
                assert!((c.p[at(i + 1, j)] - c.p[at(i, j)]).length() < dx * 1.5);
            }
        }
    }

    #[test]
    fn a_long_frame_is_let_go_not_caught_up() {
        let mut c = Cloth::new(1.0, 0.6);
        let p = pins(Vec3::new(0.0, 3.0, 0.0), c.dy());
        let air = Air {
            wind: Vec3::new(5.0, 0.0, 0.0),
            floor: 0.0,
        };
        c.step(2.0, &p, Vec3::X, air);
        assert!(c.acc < DT);
        assert!(c.p.iter().all(|p| p.is_finite()));
    }
}

//! Camera shake: the frame moves when something big happens near you.
//!
//! Ported from Gates' `crates/client/src/render/shake.rs` (MIT, © 2026
//! MoreRight DAO). Two parts. **Trauma** (Eiserloh, GDC 2016, "Juicing Your
//! Cameras With Math"): events add trauma in `0..=1`, it decays linearly,
//! and trauma² drives smooth noise on pitch and yaw plus a little translation,
//! reached over [`ATTACK_S`] and slow enough to read as a rumble: all at once
//! or quick, at a steady 60 fps it reads as the game stuttering. A landing's
//! dip rides a damped spring on top. **Nothing we fire and no hit on us
//! moves the frame**, as Halo's: the crosshair is drawn at its middle and
//! aim never moves with it, so a kick there put the crosshair off where
//! the rounds went (an automatic's climbed and stayed). The gun in hand
//! kicks instead (`viewmodel.rs`).
//! No roll and no field-of-view pulse (Gates' note: those are what make
//! players sick).
//!
//! Purely visual: `lib.rs::view` adds the offsets to the drawn camera after
//! placing it. Aim reads `Look`, which this never touches, and nothing sent
//! to the server reads the camera.

use bevy::prelude::*;

/// Largest rotation trauma 1 reaches, radians: pitch, yaw, roll (none).
const MAX_ROT: Vec3 = Vec3::new(0.018, 0.011, 0.0);
/// Largest translation trauma 1 reaches, metres.
const MAX_OFF_M: f32 = 0.02;
/// How fast the noise moves, samples per second.
const NOISE_HZ: f32 = 9.0;
/// Trauma lost per second: a full blast shakes for under a second.
const DECAY_PER_S: f32 = 1.4;
/// How long the shake takes to come up to its trauma, seconds (a time
/// constant), so an event never pops the frame in one step.
const ATTACK_S: f32 = 0.035;
/// The landing spring: slower and bouncier.
const DIP_HZ: f32 = 3.2;
const DIP_DAMP: f32 = 0.55;

/// A blast: full trauma inside `BLAST_FULL_M`, none past `BLAST_ZERO_M`.
pub const BLAST_FULL_M: f32 = 4.0;
pub const BLAST_ZERO_M: f32 = 28.0;
/// Landing: a fall faster than this (m/s) dips the camera.
const LAND_MIN_MPS: f32 = 3.5;
const LAND_DIP_M: f32 = 0.012;
const LAND_DIP_PER_MPS: f32 = 0.007;
const LAND_DIP_MAX_M: f32 = 0.11;
/// A landing past this speed shakes.
const LAND_HARD_MPS: f32 = 10.0;

/// The shake state. Pure: [`Shake::step`] advances it and returns the
/// offsets.
#[derive(Resource, Clone, Debug)]
pub struct Shake {
    trauma: f32,
    /// The trauma² the camera has come up to ([`ATTACK_S`]).
    level: f32,
    t: f32,
    dip: f32,
    dip_vel: f32,
    was_grounded: bool,
    air_vy: f32,
    rng: u32,
}

impl Default for Shake {
    fn default() -> Self {
        Self {
            trauma: 0.0,
            level: 0.0,
            t: 0.0,
            dip: 0.0,
            dip_vel: 0.0,
            was_grounded: true,
            air_vy: 0.0,
            rng: 0x9e37_79b9,
        }
    }
}

/// What [`Shake::step`] hands the camera this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Offsets {
    /// Pitch, yaw, roll, radians.
    pub rot: Vec3,
    /// Camera-local translation, metres.
    pub local: Vec3,
    /// World-down dip, metres.
    pub dip: f32,
}

impl Shake {
    /// Add trauma, clamped to 1.
    pub fn add(&mut self, trauma: f32) {
        self.trauma = (self.trauma + trauma.max(0.0)).min(1.0);
    }

    /// Add trauma for something at `at`, felt at `eye`: all of it inside
    /// `full_m`, none past `zero_m`, squared between.
    pub fn add_at(&mut self, trauma: f32, at: Vec3, eye: Vec3, full_m: f32, zero_m: f32) {
        self.add(trauma * near(at.distance(eye), full_m, zero_m));
    }

    /// Dip the camera down by about `metres` at the spring's peak.
    pub fn dip(&mut self, metres: f32) {
        self.dip_vel += metres * peak_to_speed(DIP_HZ, DIP_DAMP);
    }

    pub fn trauma(&self) -> f32 {
        self.trauma
    }

    /// Feed the body's vertical state; a landing dips and, if hard, shakes.
    pub fn body(&mut self, grounded: bool, vy: f32) {
        if !grounded {
            self.air_vy = vy;
        } else if !self.was_grounded {
            let v = -self.air_vy;
            if v > LAND_MIN_MPS {
                let dip = (LAND_DIP_M + (v - LAND_MIN_MPS) * LAND_DIP_PER_MPS).min(LAND_DIP_MAX_M);
                self.dip(dip);
                self.add(((v - LAND_HARD_MPS) / 8.0).clamp(0.0, 1.0) * 0.35);
            }
            self.air_vy = 0.0;
        }
        self.was_grounded = grounded;
    }

    /// In -1..1.
    pub fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Advance by `dt` seconds and return the offsets.
    pub fn step(&mut self, dt: f32) -> Offsets {
        let dt = dt.clamp(0.0, 0.1);
        self.trauma = (self.trauma - DECAY_PER_S * dt).max(0.0);
        self.t += dt * NOISE_HZ;
        spring(&mut self.dip, &mut self.dip_vel, DIP_HZ, DIP_DAMP, dt);
        self.level += (self.trauma * self.trauma - self.level) * (1.0 - (-dt / ATTACK_S).exp());
        let s = self.level;
        let t = self.t;
        let rot = MAX_ROT * s * Vec3::new(fbm(t, 11), fbm(t, 23), fbm(t, 37));
        let local = Vec3::new(fbm(t, 41), fbm(t, 53), fbm(t, 67)) * MAX_OFF_M * s;
        Offsets {
            rot,
            local,
            dip: self.dip,
        }
    }
}

/// How much of an event at `d` metres reaches you: 1 inside `full`, 0 past
/// `zero`, falling off as a square between.
pub fn near(d: f32, full: f32, zero: f32) -> f32 {
    let k = ((zero - d) / (zero - full).max(1e-3)).clamp(0.0, 1.0);
    k * k
}

/// Advance an underdamped spring at rest length 0 by `h` seconds, exactly
/// (`viewmodel.rs` rides the same arithmetic).
pub(crate) fn spring(x: &mut f32, v: &mut f32, hz: f32, damp: f32, h: f32) {
    let w = core::f32::consts::TAU * hz;
    let wd = w * (1.0 - damp * damp).sqrt();
    let e = (-damp * w * h).exp();
    let (sn, cs) = (wd * h).sin_cos();
    let a = *x;
    let b = (*v + damp * w * a) / wd;
    let pos = a * cs + b * sn;
    *x = e * pos;
    *v = e * (-damp * w * pos + wd * (b * cs - a * sn));
}

/// The initial speed that makes a damped spring at `hz`/`damp` peak at 1.
pub(crate) fn peak_to_speed(hz: f32, damp: f32) -> f32 {
    let w = core::f32::consts::TAU * hz;
    let wd = w * (1.0 - damp * damp).sqrt();
    let tp = ((1.0 - damp * damp).sqrt() / damp).atan() / wd;
    let peak = (-damp * w * tp).exp() * (wd * tp).sin() / wd;
    1.0 / peak
}

/// Smooth value noise in -1..1, two octaves.
fn fbm(t: f32, seed: u32) -> f32 {
    value(t, seed) * 0.7 + value(t * 2.17 + 13.1, seed ^ 0x5bd1) * 0.3
}

fn value(t: f32, seed: u32) -> f32 {
    let i = t.floor();
    let f = t - i;
    let a = hash(i as i32, seed);
    let b = hash(i as i32 + 1, seed);
    let u = f * f * (3.0 - 2.0 * f);
    a + (b - a) * u
}

fn hash(i: i32, seed: u32) -> f32 {
    let mut x = (i as u32).wrapping_mul(0x27d4_eb2d) ^ seed.wrapping_mul(0x1656_67b1);
    x ^= x >> 15;
    x = x.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 13;
    x = x.wrapping_mul(0xc2b2_ae35);
    x ^= x >> 16;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// Applies `o` to a camera placed at `t` this frame.
pub fn apply(t: &mut Transform, o: Offsets) {
    t.rotation *= Quat::from_euler(EulerRot::YXZ, o.rot.y, o.rot.x, o.rot.z);
    let local = t.rotation * o.local;
    t.translation += local - Vec3::Y * o.dip;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full blast comes on over a few frames, not in one: a pop of the whole
    /// frame at a steady 60 fps reads as the game stuttering.
    #[test]
    fn a_blast_never_pops_the_frame() {
        let dt = 1.0 / 60.0;
        for seed in 0..64 {
            let mut shake = Shake::default();
            for _ in 0..seed {
                shake.step(dt);
            }
            shake.add(1.0);
            let first = shake.step(dt);
            assert!(first.rot.x.abs() <= MAX_ROT.x * 0.4, "{}", first.rot.x);
            assert!(first.rot.y.abs() <= MAX_ROT.y * 0.4, "{}", first.rot.y);
            let mut last = first.rot;
            for _ in 0..30 {
                let now = shake.step(dt).rot;
                assert!((now.x - last.x).abs() <= MAX_ROT.x * 0.5, "{} to {}", last.x, now.x);
                last = now;
            }
        }
    }
}

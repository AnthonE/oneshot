//! What happens to bodies: a shield flaring gold as it takes a round,
//! popping when it breaks, shimmering as it comes back; dust where someone
//! lands hard; a shimmer where someone spawns, a glint at a pickup, a flash
//! where a melee lands. Read from what the snapshots say (a shield's level
//! is sent for everyone we can see, so a bystander sees the flare with no
//! hit event and no new field), drawn only.

use bevy::prelude::*;
use client_net::core::Drawn;
use sim::consts::HEAD_Y;

use super::{Fx, Particle, Scene, Shape, Tone};

/// A flare's peak when a shield takes a round, and when it breaks; how fast
/// it fades (per second).
const FLARE_HIT: f32 = 1.0;
const FLARE_POP: f32 = 1.6;
const FLARE_FADE: f32 = 5.0;
/// The shimmer while a shield climbs back.
const SHIMMER: f32 = 0.22;
/// A landing this hard (m/s down) kicks up dust.
const LAND_MPS: f32 = 6.0;

#[derive(Clone, Copy, Default)]
struct One {
    seen: bool,
    alive: bool,
    shield: u8,
    grounded: bool,
    fall: f32,
    flare: f32,
    /// The shield went up since last frame.
    charging: bool,
}

/// Each slot as it was last frame.
pub struct Was {
    slots: [One; 64],
}

impl Default for Was {
    fn default() -> Was {
        Was {
            slots: [One::default(); 64],
        }
    }
}

impl Was {
    /// How bright `slot`'s shield shows now, `t` the time in seconds.
    pub fn flare(&self, slot: u8, t: f32) -> f32 {
        let o = &self.slots[slot as usize % 64];
        let shimmer = if o.charging && o.alive {
            SHIMMER * (0.6 + 0.4 * (t * 18.0).sin())
        } else {
            0.0
        };
        o.flare.max(shimmer)
    }
}

/// This frame's bodies against last frame's: flares, pops, landings.
pub fn watch(commands: &mut Commands, fx: &mut Fx, others: &[Drawn], dt: f32) {
    let mut now = [false; 64];
    for d in others {
        let i = d.slot as usize % 64;
        now[i] = true;
        let was = fx.was.slots[i];
        let at = Vec3::new(d.pos.x, d.pos.y, d.pos.z);
        let mut o = One {
            seen: true,
            alive: d.alive,
            shield: d.shield,
            grounded: d.grounded,
            fall: if d.grounded { 0.0 } else { (-d.vel.y).max(0.0) },
            flare: (was.flare - FLARE_FADE * dt).max(0.0),
            charging: false,
        };
        // Only against a frame we saw them in, alive both times.
        if was.seen && was.alive && d.alive {
            if d.shield < was.shield {
                if d.shield == 0 {
                    o.flare = FLARE_POP;
                    pop(commands, fx, at);
                } else {
                    o.flare = o.flare.max(FLARE_HIT);
                }
            }
            o.charging = d.shield > was.shield && d.shield < 255;
            if d.grounded && !was.grounded && was.fall > LAND_MPS {
                land(commands, fx, at, was.fall);
            }
        }
        fx.was.slots[i] = o;
    }
    for (i, seen) in now.iter().enumerate() {
        if !seen {
            fx.was.slots[i].seen = false;
        }
    }
}

/// Our own landing: dust at our feet.
pub fn watch_own(commands: &mut Commands, fx: &mut Fx, slot: u8, at: Vec3, vel_y: f32, grounded: bool) {
    let i = slot as usize % 64;
    let was = fx.was.slots[i];
    if grounded && !was.grounded && was.fall > LAND_MPS {
        land(commands, fx, at, was.fall);
    }
    let o = &mut fx.was.slots[i];
    o.grounded = grounded;
    o.fall = if grounded { 0.0 } else { (-vel_y).max(0.0) };
}

/// A shield breaking: a gold burst and arcs crawling off it.
pub fn pop(commands: &mut Commands, fx: &mut Fx, feet: Vec3) {
    let mid = feet + Vec3::Y * HEAD_Y * 0.6;
    fx.emit(
        commands,
        mid,
        Particle::new(Tone::Shield, 0.15).size(0.6, 1.6).alpha(0.8),
    );
    for _ in 0..18 {
        let d = fx.unit();
        let v = d * fx.range(3.0, 7.0);
        fx.emit(
            commands,
            mid + d * 0.3,
            Particle::new(Tone::Shield, fx.range(0.15, 0.35))
                .shape(Shape::Streak {
                    len: 0.06,
                    stretch: 0.02,
                })
                .vel(v)
                .drag(2.5)
                .gravity(4.0)
                .size(0.016, 0.006),
        );
    }
    for _ in 0..6 {
        let d = fx.unit();
        let up = fx.range(0.0, HEAD_Y);
        fx.emit(
            commands,
            feet + Vec3::new(d.x * 0.35, up, d.z * 0.35),
            Particle::new(Tone::Hot, fx.range(0.05, 0.1))
                .shape(Shape::Streak {
                    len: fx.range(0.15, 0.35),
                    stretch: 0.0,
                })
                .vel(fx.unit() * 0.01)
                .size(0.012, 0.012),
        );
    }
}

/// Dust thrown out where someone lands hard.
fn land(commands: &mut Commands, fx: &mut Fx, feet: Vec3, fall: f32) {
    let k = ((fall - LAND_MPS) / 8.0).clamp(0.0, 1.0);
    for n in 0..6 {
        let a = n as f32 / 6.0 * core::f32::consts::TAU + fx.range(0.0, 0.6);
        let out = Vec3::new(a.cos(), 0.0, a.sin());
        fx.emit(
            commands,
            feet + out * 0.25 + Vec3::Y * 0.08,
            Particle::new(Tone::Dirt, fx.range(0.5, 0.8))
                .vel(out * fx.range(1.5, 3.0) * (0.6 + k) + Vec3::Y * 0.3)
                .drag(3.5)
                .size(0.2, 0.7 + 0.5 * k)
                .alpha(0.45)
                .fade_in(0.05),
        );
    }
}

/// Someone appeared: a ring on the ground and light rising through them.
pub fn spawned(commands: &mut Commands, fx: &mut Fx, feet: Vec3) {
    fx.emit(
        commands,
        feet + Vec3::Y * 0.05,
        Particle::new(Tone::TracerBlue, 0.5)
            .shape(Shape::Flat { normal: Vec3::Y })
            .size(1.8, 0.4)
            .alpha(0.7),
    );
    for _ in 0..12 {
        let d = fx.unit();
        fx.emit(
            commands,
            feet + Vec3::new(d.x * 0.4, fx.range(0.0, 0.5), d.z * 0.4),
            Particle::new(Tone::TracerBlue, fx.range(0.5, 0.9))
                .vel(Vec3::Y * fx.range(1.5, 3.0))
                .size(0.12, 0.02)
                .alpha(0.8),
        );
    }
}

/// A gun taken off a pad: a glint and a few sparks rising.
pub fn picked_up(commands: &mut Commands, fx: &mut Fx, at: Vec3) {
    let at = at + Vec3::Y * 0.7;
    fx.emit(commands, at, Particle::new(Tone::Hot, 0.18).size(0.15, 0.6).alpha(0.8));
    for _ in 0..6 {
        let v = fx.around(Vec3::Y, 0.6) * fx.range(1.0, 2.5);
        fx.emit(
            commands,
            at,
            Particle::new(Tone::Shield, fx.range(0.3, 0.5))
                .vel(v)
                .drag(2.0)
                .size(0.05, 0.01),
        );
    }
}

/// A melee swing we saw: a flash a reach in front of the swinger.
pub fn swung(commands: &mut Commands, fx: &mut Fx, scene: &Scene, slot: u8, feet: Vec3) {
    if slot == scene.me {
        return;
    }
    let Some(d) = scene.others.iter().find(|d| d.slot == slot) else {
        return;
    };
    let fwd = Vec3::new(d.yaw.cos(), 0.0, d.yaw.sin());
    let at = feet + Vec3::Y * 1.3 + fwd * 0.9;
    fx.emit(commands, at, Particle::new(Tone::Hot, 0.07).size(0.2, 0.5).alpha(0.7));
    for _ in 0..4 {
        let v = fx.around(fwd, 0.8) * fx.range(2.0, 4.0);
        fx.emit(
            commands,
            at,
            Particle::new(Tone::Spark, fx.range(0.1, 0.2))
                .shape(Shape::Streak {
                    len: 0.03,
                    stretch: 0.012,
                })
                .vel(v)
                .gravity(9.8)
                .size(0.012, 0.006),
        );
    }
}

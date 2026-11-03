//! What happens to bodies: a shield flaring gold as it takes a round,
//! breaking in a burst of lightning, crackling as it comes back; dust where
//! someone lands hard; a shimmer where someone spawns, a glint at a pickup,
//! a flash where a melee lands. Read from what the snapshots say (a
//! shield's level is sent for everyone we can see, so a bystander sees the
//! flare with no hit event and no new field), drawn only.

use bevy::prelude::*;
use client_net::core::Drawn;
use sim::consts::HEAD_Y;

use super::atlas::{GLOW, RING};
use super::pool::{Orient, Particle};
use super::table::{Layer, Matter};
use super::{Fx, Scene, ZAP_FLASH};

/// A flare's peak when a shield takes a round, and when it breaks; how fast
/// it fades (per second).
const FLARE_HIT: f32 = 1.0;
const FLARE_POP: f32 = 1.6;
const FLARE_FADE: f32 = 5.0;
/// The shimmer while a shield climbs back.
const SHIMMER: f32 = 0.22;
/// Arcs a second crawling over a shield as it climbs back.
const CHARGE_ARCS_HZ: f32 = 7.0;
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

/// A point on a standing body at `feet`, somewhere over its height.
fn on_body(fx: &mut Fx, feet: Vec3) -> Vec3 {
    let a = fx.range(0.0, core::f32::consts::TAU);
    let r = fx.range(0.2, 0.4);
    feet + Vec3::new(a.cos() * r, fx.range(0.2, HEAD_Y + 0.1), a.sin() * r)
}

/// This frame's bodies against last frame's: flares, breaks, landings.
pub fn watch(fx: &mut Fx, others: &[Drawn], dt: f32) {
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
                    pop(fx, at);
                } else {
                    o.flare = o.flare.max(FLARE_HIT);
                }
            }
            o.charging = d.shield > was.shield && d.shield < 255;
            if o.charging && fx.rand() < dt * CHARGE_ARCS_HZ {
                let p = on_body(fx, at);
                let dir = (p - at - Vec3::Y).normalize_or(Vec3::Y);
                fx.emit_tint(Layer::Arcs, 1, p, dir, Color::WHITE, 0.7);
            }
            if d.grounded && !was.grounded && was.fall > LAND_MPS {
                land(fx, at, was.fall);
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

/// Our own landing: dust at our feet, and the camera's dip (`shake.rs`).
pub fn watch_own(fx: &mut Fx, slot: u8, at: Vec3, vel_y: f32, grounded: bool) {
    fx.shake.body(grounded, vel_y);
    let i = slot as usize % 64;
    let was = fx.was.slots[i];
    if grounded && !was.grounded && was.fall > LAND_MPS {
        land(fx, at, was.fall);
    }
    let o = &mut fx.was.slots[i];
    o.grounded = grounded;
    o.fall = if grounded { 0.0 } else { (-vel_y).max(0.0) };
}

/// A shield breaking: a blue-white crack, lightning all over the body, a
/// spray of gold.
pub fn pop(fx: &mut Fx, feet: Vec3) {
    let mid = feet + Vec3::Y * HEAD_Y * 0.55;
    fx.emit_tint(Layer::Zap, 1, mid, Vec3::Y, Color::WHITE, 5.0);
    for _ in 0..10 {
        let p = on_body(fx, feet);
        let dir = (p - mid).normalize_or(Vec3::Y);
        fx.emit_tint(Layer::Arcs, 1, p, dir, Color::WHITE, 1.4);
    }
    for _ in 0..4 {
        let p = on_body(fx, feet);
        fx.emit_tint(
            Layer::ShieldSparks,
            5,
            p,
            (p - mid).normalize_or(Vec3::Y),
            Color::WHITE,
            1.0,
        );
    }
    fx.flash(
        mid,
        super::FlashDef {
            lumens: ZAP_FLASH.lumens * 4.0,
            range: 7.0,
            life: 0.18,
            ..ZAP_FLASH
        },
    );
}

/// Dust thrown out where someone lands hard.
fn land(fx: &mut Fx, feet: Vec3, fall: f32) {
    let k = ((fall - LAND_MPS) / 8.0).clamp(0.0, 1.0);
    for n in 0..6 {
        let a = n as f32 / 6.0 * core::f32::consts::TAU;
        let out = Vec3::new(a.cos(), 0.15, a.sin());
        fx.emit_tint(
            Layer::Dust,
            1,
            feet + out * 0.25 + Vec3::Y * 0.05,
            out,
            super::table::dust_tint(Matter::Dirt),
            2.5 + 2.0 * k,
        );
    }
}

/// Someone appeared: a ring on the ground and light rising through them.
pub fn spawned(fx: &mut Fx, feet: Vec3) {
    let blue = [1.5, 3.0, 7.0, 0.8];
    fx.glow.spawn(Particle {
        left: 0.5,
        life: 0.5,
        pos: feet + Vec3::Y * 0.05,
        size0: 1.2,
        size1: 0.3,
        c0: blue,
        c1: [0.3, 0.6, 1.4, 0.0],
        cell: RING,
        orient: Orient::Flat,
        ..Particle::default()
    });
    for _ in 0..14 {
        let a = fx.range(0.0, core::f32::consts::TAU);
        let r = fx.range(0.1, 0.45);
        let life = fx.range(0.5, 0.9);
        let pos = feet + Vec3::new(a.cos() * r, fx.range(0.0, 0.5), a.sin() * r);
        let vel = Vec3::Y * fx.range(1.5, 3.0);
        fx.glow.spawn(Particle {
            left: life,
            life,
            pos,
            vel,
            size0: 0.06,
            size1: 0.01,
            c0: blue,
            c1: [0.3, 0.6, 1.4, 0.0],
            cell: GLOW,
            ..Particle::default()
        });
    }
}

/// A gun taken off a pad: a glint and a few gold specks rising.
pub fn picked_up(fx: &mut Fx, at: Vec3) {
    let at = at + Vec3::Y * 0.7;
    fx.emit_tint(Layer::Flash, 2, at, Vec3::Y, Color::WHITE, 4.0);
    fx.emit_tint(Layer::ShieldSparks, 6, at, Vec3::Y, Color::WHITE, 1.0);
}

/// A melee swing we saw land: a flash and sparks a reach in front of the
/// swinger.
pub fn swung(fx: &mut Fx, scene: &Scene, slot: u8, feet: Vec3) {
    if slot == scene.me {
        return;
    }
    let Some(d) = scene.others.iter().find(|d| d.slot == slot) else {
        return;
    };
    let fwd = Vec3::new(d.yaw.cos(), 0.0, d.yaw.sin());
    let at = feet + Vec3::Y * 1.3 + fwd * 0.9;
    fx.emit_tint(Layer::Flash, 1, at, fwd, Color::WHITE, 3.0);
    fx.emit(Layer::Sparks, 5, at, fwd, Matter::Metal);
}

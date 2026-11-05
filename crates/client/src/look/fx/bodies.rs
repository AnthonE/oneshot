//! What happens to bodies: a shield flaring gold as it takes a round,
//! breaking in a burst of lightning, crackling as it comes back; dust where
//! someone lands hard; a shimmer where someone spawns, a glint at a pickup,
//! a flash where a melee lands, and the chip, dent or gash it leaves in a
//! wall it meets. Read from what the snapshots say (a shield's level is
//! sent for everyone we can see, so a bystander sees the flare with no hit
//! event and no new field) and from the events, drawn only.

use bevy::prelude::*;
use client_net::core::Drawn;
use proto::EventState;
use sim::consts::{HEAD_Y, HIT_R, LUNGE_PAD, MELEE_RANGE, SWORD_REACH};
use sim::geom::V3;

use super::atlas::{GLOW, RING};
use super::pool::{Orient, Particle};
use super::table::{Layer, Matter, Weapon};
use super::{Fx, Scene, ZAP_FLASH};

/// A flare's peak when a shield takes a round, and when it breaks; how fast
/// it fades (per second).
const FLARE_HIT: f32 = 1.0;
const FLARE_POP: f32 = 1.6;
const FLARE_FADE: f32 = 3.0;
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
    /// The flare's peak and when it was raised (s): its brightness is a
    /// function of the time since, so the frame's order can't lose it.
    peak: f32,
    at: f32,
    /// The shield went up since last frame.
    charging: bool,
    /// SHARD GUN shards stuck in them (as `flying.rs` saw them end), and
    /// when the last stuck (s).
    stuck: u8,
    stuck_at: f32,
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
    /// Raises `slot`'s flare to `level` at time `t`, unless it already
    /// shows brighter.
    pub fn raise(&mut self, slot: u8, level: f32, t: f32) {
        let o = &mut self.slots[slot as usize % 64];
        if level >= fade(o.peak, o.at, t) {
            o.peak = level;
            o.at = t;
        }
    }

    /// How bright `slot`'s shield shows now, `t` the time in seconds.
    pub fn flare(&self, slot: u8, t: f32) -> f32 {
        let o = &self.slots[slot as usize % 64];
        let shimmer = if o.charging && o.alive {
            SHIMMER * (0.6 + 0.4 * (t * 18.0).sin())
        } else {
            0.0
        };
        fade(o.peak, o.at, t).max(shimmer)
    }
}

fn fade(peak: f32, at: f32, t: f32) -> f32 {
    peak * (1.0 - (t - at) * FLARE_FADE).clamp(0.0, 1.0)
}

/// How long a shard stays stuck (`sim::consts::NEEDLE_KEEP_MS`).
const STUCK_S: f32 = sim::consts::NEEDLE_KEEP_MS as f32 / 1000.0;

/// The crystals stuck in a body this frame: pink shards poking out of its
/// chest, each in its own place, glowing hotter as they near the burst.
fn crystals(fx: &mut Fx, d: &Drawn, n: u8, t: f32) {
    let feet = Vec3::new(d.pos.x, d.pos.y, d.pos.z);
    let heat = n as f32 / sim::arms::arm(sim::arms::SHARDS).stick.max(1) as f32;
    let pulse = 0.7 + 0.3 * (t * (6.0 + 10.0 * heat)).sin();
    for k in 0..n as u32 {
        let h = |s: u32| super::atlas::hash01(d.slot as u32 * 31 + k, s);
        let a = h(1) * core::f32::consts::TAU - d.yaw;
        let up = 0.75 + 0.6 * h(2);
        let out = Vec3::new(a.cos(), 0.25 * (h(3) - 0.5), a.sin());
        let at = feet + Vec3::Y * up + out * 0.3;
        let c = [7.0 * pulse * (0.6 + heat), 1.6 * pulse, 5.0 * pulse * (0.6 + heat), 1.0];
        fx.glow.spawn(Particle {
            left: 0.001,
            life: 0.001,
            pos: at.into(),
            vel: out.into(),
            size0: 0.022,
            size1: 0.022,
            c0: c,
            c1: c,
            cell: super::atlas::SHARD,
            orient: Orient::Stretch,
            stretch: 0.12,
            len_max: 0.12,
            ..Particle::default()
        });
    }
}

/// A shard ended in `slot`'s body (`flying.rs`): one more stuck there.
pub fn needle_in(fx: &mut Fx, slot: u8, t: f32) {
    let o = &mut fx.was.slots[slot as usize % 64];
    if t - o.stuck_at >= STUCK_S {
        o.stuck = 0;
    }
    o.stuck = (o.stuck + 1).min(sim::arms::arm(sim::arms::SHARDS).stick);
    o.stuck_at = t;
}

/// A burst at `at`: whoever stood there has none stuck now.
pub fn burst_at(fx: &mut Fx, others: &[Drawn], at: Vec3) {
    for d in others {
        let p = Vec3::new(d.pos.x, d.pos.y, d.pos.z);
        if p.distance(at) < 2.0 {
            fx.was.slots[d.slot as usize % 64].stuck = 0;
        }
    }
}

/// A point on a standing body at `feet`, somewhere over its height.
fn on_body(fx: &mut Fx, feet: Vec3) -> Vec3 {
    let a = fx.range(0.0, core::f32::consts::TAU);
    let r = fx.range(0.2, 0.4);
    feet + Vec3::new(a.cos() * r, fx.range(0.2, HEAD_Y + 0.1), a.sin() * r)
}

/// This frame's bodies against last frame's: flares, breaks, landings.
pub fn watch(fx: &mut Fx, others: &[Drawn], dt: f32, t: f32) {
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
            peak: was.peak,
            at: was.at,
            charging: false,
            stuck: if d.alive { was.stuck } else { 0 },
            stuck_at: was.stuck_at,
        };
        if o.stuck > 0 && t - o.stuck_at < STUCK_S {
            crystals(fx, d, o.stuck, t);
        }
        // Only against a frame we saw them in, alive both times.
        if was.seen && was.alive && d.alive {
            if d.shield < was.shield {
                let level = if d.shield == 0 {
                    pop(fx, at);
                    FLARE_POP
                } else {
                    FLARE_HIT
                };
                if level >= fade(o.peak, o.at, t) {
                    o.peak = level;
                    o.at = t;
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
/// And our own shield (`shield`: its level, None dead): its flare, which
/// the arms and the gun in view wear (`viewmodel.rs`), and, as it breaks,
/// a crack of blue-white light about us.
pub fn watch_own(fx: &mut Fx, slot: u8, at: Vec3, vel_y: f32, grounded: bool, shield: Option<u8>, t: f32) {
    fx.shake.body(grounded, vel_y);
    let i = slot as usize % 64;
    let was = fx.was.slots[i];
    if grounded && !was.grounded && was.fall > LAND_MPS {
        land(fx, at, was.fall);
    }
    let mut peak = (was.peak, was.at);
    if let (Some(now), true) = (shield, was.alive) {
        if now < was.shield {
            let level = if now == 0 {
                fx.flash(
                    at + Vec3::Y * HEAD_Y,
                    super::FlashDef {
                        lumens: ZAP_FLASH.lumens * 2.0,
                        range: 5.0,
                        life: 0.2,
                        ..ZAP_FLASH
                    },
                );
                FLARE_POP
            } else {
                FLARE_HIT
            };
            if level >= fade(peak.0, peak.1, t) {
                peak = (level, t);
            }
        }
    }
    let o = &mut fx.was.slots[i];
    o.grounded = grounded;
    o.fall = if grounded { 0.0 } else { (-vel_y).max(0.0) };
    o.alive = shield.is_some();
    o.charging = shield.is_some_and(|s| s > was.shield && s < 255);
    o.shield = shield.unwrap_or(0);
    (o.peak, o.at) = peak;
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
        pos: (feet + Vec3::Y * 0.05).into(),
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
            pos: pos.into(),
            vel: vel.into(),
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

/// A melee swing we saw, or our own (`e.pos` the swinger's eye, `c` the
/// sword's arm id for its swing, `part` 2 a clash of swords): when the
/// swing's line meets the map within its reach and nobody we draw stands
/// on it first, what the blow throws off that face and the chip, dent or
/// gash it leaves (`marks.rs`). Someone else's: a blue slash of light
/// across a sword's swing, and a flash and sparks where a blow lands.
pub fn swung(fx: &mut Fx, scene: &Scene, e: &EventState, eye: Vec3) {
    let slot = e.a;
    let own = slot == scene.me;
    let d = scene.others.iter().find(|d| d.slot == slot).copied();
    let aim = match (own, d) {
        // Where we looked as the swing began, not now: the event comes
        // back a round trip after the press.
        (true, _) => fx.take_swing().unwrap_or(scene.aim),
        (false, Some(d)) => Vec3::new(d.pitch.cos() * d.yaw.cos(), d.pitch.sin(), d.pitch.cos() * d.yaw.sin()),
        (false, None) => return,
    };
    let sword = e.c == sim::arms::SWORD as u16;
    let walled = e.part != 2 && wall(fx, scene, slot, eye, aim, sword);
    let Some(d) = d.filter(|_| !own) else {
        return;
    };
    let fwd = Vec3::new(d.yaw.cos(), 0.0, d.yaw.sin());
    if sword {
        let side = Vec3::new(-fwd.z, 0.0, fwd.x);
        let mid = eye - Vec3::Y * 0.3 + fwd * 0.8;
        for k in 0..5 {
            let u = k as f32 / 4.0 - 0.5;
            fx.glow.spawn(Particle {
                left: 0.12,
                life: 0.12,
                pos: (mid + side * u * 1.1 + Vec3::Y * u * 0.4).into(),
                vel: (side * 6.0).into(),
                size0: 0.05,
                size1: 0.02,
                c0: [2.0, 5.0, 14.0, 1.0],
                c1: [0.4, 1.0, 3.0, 0.0],
                cell: super::atlas::STREAK,
                orient: Orient::Stretch,
                stretch: 0.06,
                len_max: 0.5,
                ..Particle::default()
            });
        }
        return;
    }
    if walled {
        return;
    }
    let at = eye - Vec3::Y * 0.3 + fwd * 0.9;
    fx.emit_tint(Layer::Flash, 1, at, fwd, Color::WHITE, 3.0);
    fx.emit(Layer::Sparks, 5, at, fwd, Matter::Metal);
}

/// A swing from `eye` along `aim` against the map: whether it met a face
/// in reach with nobody we draw (but the swinger) in the way, and if so
/// what that face throws and keeps.
fn wall(fx: &mut Fx, scene: &Scene, slot: u8, eye: Vec3, aim: Vec3, sword: bool) -> bool {
    let reach = if sword { SWORD_REACH } else { MELEE_RANGE };
    let pad = if sword { LUNGE_PAD } else { 0.25 };
    let t = scene
        .map
        .ray(V3::new(eye.x, eye.y, eye.z), V3::new(aim.x, aim.y, aim.z), reach);
    if t >= reach {
        return false;
    }
    let between = scene.others.iter().filter(|d| d.alive && d.slot != slot).any(|d| {
        let feet = Vec3::new(d.pos.x, d.pos.y, d.pos.z);
        let s = (feet + Vec3::Y * 0.9 - eye).dot(aim).clamp(0.0, t);
        let p = eye + aim * s;
        let flat = Vec2::new(p.x - feet.x, p.z - feet.z).length();
        flat < HIT_R + pad && (-0.2..HEAD_Y + 0.4).contains(&(p.y - feet.y))
    });
    let hull = scene.vehicles.iter().any(|v| {
        let c = Vec3::new(v.pos.x, v.pos.y + 1.0, v.pos.z);
        let s = (c - eye).dot(aim).clamp(0.0, t);
        (eye + aim * s).distance_squared(c) < 2.5 * 2.5
    });
    if between || hull {
        return false;
    }
    let Some(hit) = super::surface::cast(scene.map, &mut fx.near, eye, aim, t + 0.05) else {
        return false;
    };
    let matter = super::surface::matter_of(scene.map, &hit, fx.marks.lift.as_deref());
    let w = if sword { Weapon::Blade } else { Weapon::Melee };
    fx.strike(scene.map, w, matter, &hit);
    true
}

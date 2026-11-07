//! Gunfire. **Ours is drawn the frame its trigger breaks**, as the gun we
//! predict fires: its flash and smoke at the muzzle, its tracer down the
//! view (DESIGN §5; after Gates' `fx/gun.rs`). The server's word on that
//! shot arrives a round trip later and draws only what the round met.
//! Everyone else's is drawn off the event: a baked flash about a hot core, a
//! tongue of fire and smoke at their muzzle, a tracer down the line the round really took
//! (every round, or every few for an automatic), and what it met: sparks,
//! chips and grit and a hole in the face it struck (a pillar's, a ramp's,
//! a curved wall's segment: `surface.rs`), dust in the ground, lightning
//! off a shield, a red mist and drops off health with blood behind the
//! body, sparks off a hull.

use bevy::prelude::*;
use proto::EventState;
use sim::arms::{arm, Kind, ROCKET, SHOTGUN};
use sim::consts::{HEAD_Y, HIT_R};
use sim::fighter::Fighter;
use sim::geom::V3;

use super::atlas::{FLAME, GLOW};
use super::pool::{Orient, Particle};
use super::table::{self, Casing, Flash, GunLook, Layer, Matter, Weapon};
use super::{Fx, Scene, MUZZLE_FLASH};
use crate::look::shake;

/// From a shooter's eye to their muzzle: forward, then down.
const MUZZLE_FWD_M: f32 = 0.75;
const MUZZLE_DOWN_M: f32 = 0.28;
/// Our own muzzle, in view space: ahead, right, down (where the gun in
/// view holds it), and how far ahead of it our tracer starts.
const OWN_MUZZLE: Vec3 = Vec3::new(0.12, -0.1, 0.7);
/// Our own flash, times a third person's and the gun's `flash_scale`:
/// [`OWN_MUZZLE`] is a hand's length from the eye, where a third person's
/// size would cover a fifth of the view.
const OWN_FLASH_SCALE: f32 = 1.5;
/// How long our own flash burns, seconds: four or five frames at 60, the
/// first its fullest, so an automatic's fire all but overlaps. Powder's,
/// and an energy gun's with its afterglow.
const OWN_FLASH_S: (f32, f32) = (0.062, 0.078);
const OWN_ENERGY_S: f32 = 0.16;
/// How far off the gun an energy gun's flash burns, in view (`clear_of_gun`).
const ENERGY_CLEAR_M: f32 = 0.05;
/// The share of an energy flash's life its bloom plays in.
const OWN_BLOOM_SHARE: f32 = 0.5;
/// A sprite drawn for the frame it is spawned in and gone by the next
/// (the pools step after they are written, by more than this).
const ONE_FRAME_S: f32 = 0.001;
/// Our own flash's hot core, metres across at a `flash_scale` of 1.
const OWN_CORE_M: f32 = 0.05;
/// An energy gun's core against that: it is the flash, where powder has fire.
const ENERGY_CORE: f32 = 1.8;
/// The world's share of our flash, times the gun's `light_lm`. The gun in
/// view has a light of its own (`viewmodel.rs`), a hand's length off, that
/// lights nothing further.
const OWN_WORLD_LIGHT: f32 = 3.0;
const OWN_TRACER_SKIP_M: f32 = 1.0;
/// Our shotgun's pellets drawn in a cone this wide (radians): the real
/// scatter is the server's secret, and its marks are where they went.
const PELLET_DRAW_RAD: f32 = 0.09;
/// A standing shooter's eye over the floor, roughly: where their casings
/// land.
const EYE_OVER_FLOOR_M: f32 = 1.6;
/// A turret's port, this far behind its muzzle, and its bed this far
/// under its muzzle (`look/turret.rs`, `sim::consts::TURRET_Y`).
const TURRET_PORT_M: f32 = 0.95;
const TURRET_BED_M: f32 = 1.15;

fn v3(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

/// Someone's shot, off the server's event.
pub fn shot(fx: &mut Fx, scene: &Scene, e: &EventState, from: Vec3, to: Vec3) {
    // A turret's round leaves its muzzle (the event's start), ours too: no
    // gun in view fires it. It lands as a rifle's.
    let turret = e.c == sim::events::Cause::Minigun as u16;
    let arm = if turret { sim::arms::AR } else { e.c as u8 };
    let look = if turret { table::MINIGUN } else { table::gun(arm) };
    let v = to - from;
    let len = v.length();
    if len < 0.05 {
        return;
    }
    let dir = v / len;
    let own = e.a == scene.me;
    let bit = 1u64 << (e.a % 64);
    let first = fx.flashed & bit == 0;
    fx.flashed |= bit;
    // A ray is drawn as a tracer; anything else flies (`flying.rs`).
    let kind = sim::arms::arm(arm).kind;
    if own && !turret {
        // Drawn when it broke (`own`): only what it met is new.
        if kind.ray() {
            land(fx, scene, arm, from, dir, len, to);
        }
        return;
    }
    if kind == Kind::Sword {
        return;
    }
    let muzzle = if turret {
        from
    } else {
        from + dir * MUZZLE_FWD_M - Vec3::Y * MUZZLE_DOWN_M
    };
    if first {
        flash(fx, muzzle, dir, &look);
        // The round's casing, from the port at the shooter's right (a
        // shotgun's pellets are one round). A standing shooter's floor is
        // about their eye's height below them; a turret's brass comes out
        // of its receiver, behind the barrels, and falls to the bed.
        let (port, floor) = if turret {
            (muzzle - dir * TURRET_PORT_M, muzzle.y - TURRET_BED_M)
        } else {
            (muzzle - dir * 0.3, from.y - EYE_OVER_FLOOR_M)
        };
        let out = dir.cross(Vec3::Y).normalize_or(Vec3::X);
        eject(fx, &look, port, out, floor);
        if turret {
            // Two a frame: it fires faster than frames come.
            eject(fx, &look, port - dir * 0.05, out, floor);
        }
    }
    if !kind.ray() {
        if first && arm == ROCKET {
            backblast(fx, muzzle, dir);
        }
        return;
    }
    let n = &mut fx.rounds[e.a as usize % 64];
    *n = n.wrapping_add(1);
    if (*n).is_multiple_of(look.every) {
        let start = muzzle + dir * 0.6;
        let d = (to - start).normalize_or(dir);
        table::tracer(&mut fx.glow, &look, start, d, start.distance(to));
    }
    if look.wake_s > 0.0 {
        table::wake(&mut fx.soft, muzzle, to, look.wake_s);
    }
    land(fx, scene, arm, from, dir, len, to);
}

/// Siege: a tower's needle (the shard gun's pink, `to` where it met them),
/// or a minion's round (`arm`'s look); `hit`: it met them, else it flies on
/// past and lands where it lands.
pub fn siege_shot(fx: &mut Fx, scene: &Scene, arm: u8, from: Vec3, to: Vec3, hit: bool) {
    let v = to - from;
    let len = v.length();
    if len < 0.5 {
        return;
    }
    let dir = v / len;
    let look = table::gun(arm);
    table::tracer(&mut fx.glow, &look, from + dir * 0.4, dir, len - 0.4);
    if hit {
        land(fx, scene, arm, from, dir, len, to);
    }
}

/// `at` (in view space, on the line to a muzzle) moved `by` metres toward the
/// view's middle: the gun in view is nearer than what is drawn on its muzzle, so
/// a thing centred there is half behind it, a crescent.
pub(crate) fn clear_of_gun(at: Vec3, by: f32) -> Vec3 {
    at - Vec3::new(at.x, at.y, 0.0).normalize_or_zero() * by
}

/// Our own gun, this frame: each round it fired since the last, drawn now,
/// from the hand that fired it (`viewmodel::Rounds`) at the muzzle the gun
/// in view was drawn with (`muzzles`, view space as it looks; the fixed
/// [`OWN_MUZZLE`] while none is drawn, behind a scope).
pub fn own(fx: &mut Fx, scene: &Scene, f: &Fighter, eye: Vec3, cam: Quat, muzzles: [Option<Vec3>; 2]) {
    let fired = fx.own_rounds.take(f);
    // A driver doesn't shoot; a passenger does, from the seat.
    if !f.alive || f.riding().is_some_and(|(_, seat)| seat == sim::vehicle::DRIVER) {
        return;
    }
    for (i, rounds) in fired.into_iter().enumerate() {
        if rounds > 0 {
            hand(fx, scene, f, eye, cam, i == 1, rounds, muzzles[i]);
        }
    }
}

/// Our own rounds counted off and not drawn: a film's camera off the eye
/// sees them fired from the body, as anyone's (`film.rs`).
pub fn own_unseen(fx: &mut Fx, f: &Fighter) {
    let _ = fx.own_rounds.take(f);
}

/// Where our flash sits in view space: on the line to the drawn muzzle, at
/// [`OWN_MUZZLE`]'s depth.
pub(crate) fn own_muzzle(left: bool, drawn: Option<Vec3>) -> Vec3 {
    let at = Vec3::new(OWN_MUZZLE.x, OWN_MUZZLE.y, -OWN_MUZZLE.z);
    let fixed = if left { Vec3::new(-at.x, at.y, at.z) } else { at };
    match drawn {
        Some(m) if m.z < -0.05 => m * (OWN_MUZZLE.z / -m.z),
        _ => fixed,
    }
}

/// One hand's rounds.
#[allow(clippy::too_many_arguments)]
fn hand(fx: &mut Fx, scene: &Scene, f: &Fighter, eye: Vec3, cam: Quat, left: bool, rounds: u32, drawn: Option<Vec3>) {
    let id = if left { f.left.arm } else { f.gun().arm };
    let a = arm(id);
    if a.kind == Kind::Sword {
        return;
    }
    let look = table::gun(id);
    let fwd = cam * Vec3::NEG_Z;
    let right = cam * Vec3::X;
    let up = cam * Vec3::Y;
    let at = own_muzzle(left, drawn);
    // The flash waits for the camera and the gun the frame is drawn with
    // (`muzzle`), and burns there from now: a round on top of one still
    // burning starts it again, another shape.
    let life = match look.flash {
        Flash::Powder => fx.range(OWN_FLASH_S.0, OWN_FLASH_S.1),
        Flash::Energy(_) => OWN_ENERGY_S,
    };
    let (roll, seed) = (fx.range(0.0, core::f32::consts::TAU), fx.rand());
    fx.own_flash[usize::from(left)] = Some(OwnFlash {
        look,
        rocket: id == ROCKET,
        rounds,
        age: 0.0,
        life,
        fresh: true,
        roll,
        seed,
    });
    // The stock's jolt: light for an automatic's every round, full for a
    // heavy single shot.
    let heavy = !a.auto || a.pellets > 1 || id == ROCKET;
    // Crouched, the stock is braced: half the jolt (Gates' gunfeel).
    let braced = if f.body.crouched {
        crate::look::viewmodel::CROUCH_KICK
    } else {
        1.0
    };
    let k = if heavy { 0.6 } else { 0.3 } * braced;
    let jolt = fx.shake.rand() * shake::SHOT_SIDE_RAD;
    fx.shake.kick(Vec3::new(shake::SHOT_KICK_RAD * k, jolt * k, 0.0));
    fx.shake.add(shake::SHOT_TRAUMA * if heavy { 1.0 } else { 0.35 });
    if !a.kind.ray() {
        return;
    }
    let pellets = if id == SHOTGUN { 6 } else { rounds as usize };
    for k in 0..pellets {
        let dir = if id == SHOTGUN {
            let (sx, sy) = (fx.range(-1.0, 1.0), fx.range(-1.0, 1.0));
            (fwd + (right * sx + up * sy) * PELLET_DRAW_RAD).normalize()
        } else {
            fwd
        };
        // Down the view to the first wall, or as far as the gun reaches,
        // from half way between the muzzle's line and the view's.
        let reach = scene.map.ray(v3(eye), v3(dir), a.range.max(30.0));
        let start = eye + dir * OWN_TRACER_SKIP_M + (right * at.x + up * at.y) * 0.5;
        if k == 0 || id == SHOTGUN {
            table::tracer(&mut fx.glow, &look, start, dir, (reach - OWN_TRACER_SKIP_M).max(0.0));
        }
        if look.wake_s > 0.0 {
            table::wake(&mut fx.soft, start, eye + dir * reach, look.wake_s);
        }
    }
}

/// Our own gun's flash, by hand: lit the frame its round broke, burning
/// until it is spent, and drawn each frame of that at the muzzle as that
/// frame holds it.
#[derive(Clone, Copy, Debug)]
pub struct OwnFlash {
    look: GunLook,
    rocket: bool,
    rounds: u32,
    /// Seconds burnt, and how long it burns.
    age: f32,
    life: f32,
    /// Not yet drawn: its smoke, its light and its casings are still owed.
    fresh: bool,
    /// Its turn about the barrel and which of the sheet's flashes it is.
    roll: f32,
    seed: f32,
}

/// Our own flashes, at the muzzle as the frame draws it: off the camera
/// as placed this frame (`eye`: its kick, its shake, a film's roll and
/// easing), which carries the gun in view, and the gun as posed this frame
/// (`drawn`, `viewmodel::ViewMuzzles`), not the aim the shot was taken down
/// or last frame's pose.
///
/// **The fire is the gun's, not the world's**: it is drawn again every
/// frame it burns, where the muzzle is that frame (sprites that live one
/// frame, each as far through its life as the flash is). Left in the world
/// where it was lit, a flash two frames long stood off the barrel the
/// moment the view turned or the gun kicked, and read as a spark in the
/// air beside the gun. What a round leaves behind is the world's: its
/// smoke, its light, its casing.
pub fn muzzle(fx: &mut Fx, eye: &Transform, drawn: [Option<Vec3>; 2], dt: f32) {
    for (i, drawn) in drawn.into_iter().enumerate() {
        let Some(mut o) = fx.own_flash[i] else {
            continue;
        };
        let at = own_muzzle(i == 1, drawn);
        let at = match o.look.flash {
            Flash::Energy(_) => clear_of_gun(at, ENERGY_CLEAR_M),
            Flash::Powder => at,
        };
        let muzzle = eye.translation + eye.rotation * at;
        let fwd = eye.rotation * Vec3::NEG_Z;
        let right = eye.rotation * Vec3::X;
        let up = eye.rotation * Vec3::Y;
        let scale = OWN_FLASH_SCALE * o.look.flash_scale;
        if o.fresh {
            o.fresh = false;
            if o.look.flash == Flash::Powder {
                fx.emit_tint(
                    Layer::Wisp,
                    1,
                    muzzle + fwd * 0.12,
                    fwd,
                    Color::WHITE,
                    0.5 + 0.3 * scale,
                );
            }
            fx.flash(
                muzzle,
                super::FlashDef {
                    lumens: o.look.light_lm * OWN_WORLD_LIGHT,
                    color: tint(&o.look),
                    ..MUZZLE_FLASH
                },
            );
            // Out of the port to the right and a little back, quick enough
            // to be out of the view in a few frames.
            for _ in 0..o.rounds.min(3) {
                eject(
                    fx,
                    &o.look,
                    muzzle - fwd * 0.3 + right * 0.04,
                    (right - fwd * 0.3 - up * 0.15).normalize_or(right),
                    eye.translation.y - EYE_OVER_FLOOR_M,
                );
            }
            if o.rocket {
                backblast(fx, muzzle, fwd);
            }
        }
        // A frame held (the fx stage's clock stopped) keeps what it drew.
        if dt > 0.0 {
            burn(fx, &o, muzzle, fwd, scale);
            o.age += dt;
        }
        fx.own_flash[i] = (o.age < o.life).then_some(o);
    }
}

/// One frame of our own flash, at the muzzle as this frame holds it.
fn burn(fx: &mut Fx, o: &OwnFlash, at: Vec3, fwd: Vec3, scale: f32) {
    let u = (o.age / o.life.max(1e-4)).clamp(0.0, 0.98);
    // A sprite `u` through its life that is gone by the next frame.
    let frame = |u: f32, p: Particle| Particle {
        left: ONE_FRAME_S,
        life: ONE_FRAME_S / (1.0 - u.clamp(0.0, 0.98)),
        ..p
    };
    // A sheet's flash: the recipe's, this flash's own roll of it.
    let baked = |r: &::fx::recipe::Recipe, size: f32, tone: f32, at: Vec3| Particle {
        pos: at.into(),
        size0: size,
        size1: size * r.grow,
        c0: r.c0,
        c1: r.c1,
        roll: o.roll,
        cell: r
            .cell
            .wrapping_add(((o.seed * r.cells.max(1) as f32) as u8).min(r.cells.max(1) - 1) * r.frames.max(1)),
        frames: r.frames,
        seed: (o.seed * 7.0).fract(),
        tone,
        glow: r.glow,
        ..Particle::default()
    };
    let c = o.look.flash_c;
    match o.look.flash {
        Flash::Powder => {
            // The gas out of the barrel, fullest on its first frame; a
            // white-hot core at the bore for bloom to take hold of; and a
            // tongue down the line of fire while it is young.
            let r = table::recipe(Layer::MuzzleFlash);
            let size = (r.size.0 + (r.size.1 - r.size.0) * o.seed) * (0.5 + 0.4 * scale);
            let p = baked(&r, size, r.tone, at + fwd * 0.04);
            fx.pool(Layer::MuzzleFlash.pool()).spawn(frame(u, p));
            let core = OWN_CORE_M * (0.6 + 0.4 * scale);
            fx.glow.spawn(frame(
                u,
                Particle {
                    pos: at.into(),
                    size0: core,
                    size1: core * 1.8,
                    c0: c,
                    c1: [c[0] * 0.15, c[1] * 0.08, c[2] * 0.03, 0.0],
                    cell: GLOW,
                    ..Particle::default()
                },
            ));
            if u < 0.5 {
                let t = table::recipe(Layer::Tongue);
                fx.glow.spawn(frame(
                    u * 2.0,
                    Particle {
                        pos: at.into(),
                        vel: (fwd * t.speed.0.max(0.1)).into(),
                        size0: t.size.0 * (0.6 + 0.4 * scale),
                        size1: t.size.0 * (0.6 + 0.4 * scale) * t.grow,
                        c0: t.c0,
                        c1: t.c1,
                        cell: FLAME,
                        orient: Orient::Stretch,
                        stretch: t.stretch,
                        len_max: t.len_max,
                        ..Particle::default()
                    },
                ));
            }
        }
        Flash::Energy(tone) => {
            // The bloom, then a glow in the gun's colour that hangs on the
            // muzzle a moment: on the muzzle, wherever the gun goes.
            let r = table::recipe(Layer::Bloom);
            let size = (r.size.0 + (r.size.1 - r.size.0) * o.seed) * (0.5 + 0.4 * scale);
            if u < OWN_BLOOM_SHARE {
                let p = baked(&r, size, tone, at + fwd * 0.03);
                fx.pool(Layer::Bloom.pool()).spawn(frame(u / OWN_BLOOM_SHARE, p));
            }
            let after = Particle {
                size0: size * 0.45,
                size1: size * 0.25,
                frames: 1,
                glow: 0.3,
                c1: [1.0, 1.0, 1.0, 0.0],
                ..baked(&r, size, tone, at + fwd * 0.03)
            };
            fx.pool(Layer::Bloom.pool()).spawn(frame(u, after));
            let core = OWN_CORE_M * ENERGY_CORE * (0.6 + 0.4 * scale);
            fx.glow.spawn(frame(
                (u * 2.5).min(0.98),
                Particle {
                    pos: at.into(),
                    size0: core,
                    size1: core * 1.6,
                    c0: c,
                    c1: [c[0] * 0.1, c[1] * 0.1, c[2] * 0.1, 0.0],
                    cell: GLOW,
                    ..Particle::default()
                },
            ));
        }
    }
}

/// The far end of a round's line: what it met, what that throws, and the
/// mark it leaves.
fn land(fx: &mut Fx, scene: &Scene, arm: u8, from: Vec3, dir: Vec3, len: f32, to: Vec3) {
    let w = Weapon::of_arm(arm);
    match met(fx, scene, from, dir, len, to) {
        Some(Met::Face(hit, matter)) => fx.strike(scene.map, w, matter, &hit),
        Some(Met::Body(slot, Matter::Flesh)) => fx.bleed(scene.map, slot, w, to, dir),
        Some(Met::Body(_, matter)) => {
            fx.impact(w, matter, to, -dir, super::surface::floor_under(scene.map, to, -dir));
        }
        Some(Met::Hull) => {
            fx.impact(
                w,
                Matter::Hull,
                to,
                -dir,
                super::surface::floor_under(scene.map, to, -dir),
            );
        }
        Some(Met::Somewhere) => {
            fx.impact(
                w,
                Matter::Concrete,
                to,
                -dir,
                super::surface::floor_under(scene.map, to, -dir),
            );
        }
        None => {}
    }
}

/// What a round met.
enum Met {
    /// A face of the map: the face, its matter.
    Face(super::surface::Hit, Matter),
    /// A body we draw: whose, and whether its shield held.
    Body(u8, Matter),
    Hull,
    /// The map, though no face is found at the end (a rounding): its
    /// particles, no mark.
    Somewhere,
}

/// What a round met at `to`. None: its range ran out, or the body was
/// ours (or one we don't draw: the end of its line says no more).
fn met(fx: &mut Fx, scene: &Scene, from: Vec3, dir: Vec3, len: f32, to: Vec3) -> Option<Met> {
    let t = scene.map.ray(v3(from), v3(dir), len + 0.1);
    if t <= len + 0.06 && t >= len - 0.3 {
        return Some(match super::surface::struck(scene.map, &mut fx.near, to, dir) {
            Some(hit) => Met::Face(
                hit,
                super::surface::matter_of(scene.map, &hit, fx.marks.lift.as_deref()),
            ),
            None => Met::Somewhere,
        });
    }
    for d in scene.others.iter().filter(|d| d.alive) {
        let (dx, dz) = (to.x - d.pos.x, to.z - d.pos.z);
        let up = to.y - d.pos.y;
        if dx * dx + dz * dz < (HIT_R + 0.3) * (HIT_R + 0.3) && (-0.2..HEAD_Y + 0.4).contains(&up) {
            let m = if d.shield > 0 { Matter::Shield } else { Matter::Flesh };
            return Some(Met::Body(d.slot, m));
        }
    }
    for v in scene.vehicles {
        let c = Vec3::new(v.pos.x, v.pos.y + 1.0, v.pos.z);
        if c.distance_squared(to) < 3.4 * 3.4 {
            return Some(Met::Hull);
        }
    }
    None
}

/// A third-person muzzle. Powder: the baked flash of gas, a white-hot core
/// at the bore, a tongue of fire down the barrel, a breath of smoke.
/// Energy: a baked bloom in the gun's tone about a core, a glow that hangs
/// on the muzzle a moment, no smoke. And a light.
pub fn flash(fx: &mut Fx, at: Vec3, dir: Vec3, look: &GunLook) {
    let scale = look.flash_scale;
    match look.flash {
        Flash::Powder => {
            // The petals grow with the gun, the core less: a shotgun's
            // flash is wide, not a sun.
            fx.emit_tint(
                Layer::MuzzleFlash,
                1,
                at + dir * 0.05,
                dir,
                Color::WHITE,
                0.7 + 0.5 * scale,
            );
            table::core(&mut fx.glow, at, 0.8 + 0.5 * scale, look.flash_c);
            fx.emit(Layer::Tongue, 1, at, dir, Matter::Concrete);
            fx.emit_tint(Layer::Wisp, 1, at + dir * 0.15, dir, Color::WHITE, 0.6 + 0.4 * scale);
        }
        Flash::Energy(tone) => {
            bloom(fx, at + dir * 0.04, dir, tone, 0.6 + 0.5 * scale, 0.0);
            table::core(&mut fx.glow, at, 0.7 * scale, look.flash_c);
        }
    }
    fx.flash(
        at,
        super::FlashDef {
            lumens: look.light_lm,
            color: tint(look),
            ..MUZZLE_FLASH
        },
    );
}

/// An energy gun's muzzle: the baked bloom in `tone`, and a glow in the
/// gun's colour that lingers a moment after. `life` overrides the
/// bloom's (0: the recipe's).
fn bloom(fx: &mut Fx, at: Vec3, dir: Vec3, tone: f32, scale: f32, life: f32) {
    let mut r = ::fx::recipe::Recipe {
        tone,
        ..table::recipe(Layer::Bloom)
    };
    if life > 0.0 {
        r.life = (life * 0.8, life);
    }
    let pool = fx.pool(Layer::Bloom.pool());
    table::emit_recipe(pool, &r, 1, at, dir, Color::WHITE, scale);
    // The afterglow: a soft disc in the ramp's colour, dimmer, longer.
    let after = ::fx::recipe::Recipe {
        tone,
        glow: 0.3,
        life: (0.14, 0.2),
        size: (0.04, 0.06),
        grow: 0.6,
        frames: 1,
        cells: 1,
        ..table::recipe(Layer::Bloom)
    };
    let pool = fx.pool(Layer::Bloom.pool());
    table::emit_recipe(pool, &after, 1, at, dir, Color::WHITE, scale);
}

/// A gun's casing out of its port at `at` along `out`, onto `floor`.
fn eject(fx: &mut Fx, look: &GunLook, at: Vec3, out: Vec3, floor: f32) {
    match look.casing {
        Casing::None => {}
        Casing::Brass => fx.chips.casing(at, out, floor, false),
        Casing::Shell => fx.chips.casing(at, out, floor, true),
    }
}

/// How long a rocket's fire burns at the tube, seconds: a blink. Smoke is
/// what stays; left any longer, it hangs in the air like a lamp.
const BACKBLAST_FIRE_S: (f32, f32) = (0.05, 0.1);

/// A rocket leaving the tube: fire out of the front, smoke out of both ends.
fn backblast(fx: &mut Fx, at: Vec3, dir: Vec3) {
    fx.emit_tint(Layer::Flash, 2, at + dir * 0.4, dir, Color::WHITE, 3.5);
    let fire = ::fx::recipe::Recipe {
        life: BACKBLAST_FIRE_S,
        ..table::recipe(Layer::Fireball)
    };
    let pool = fx.pool(Layer::Fireball.pool());
    table::emit_recipe(pool, &fire, 3, at + dir * 0.3, dir, Color::WHITE, 0.6);
    fx.emit_tint(Layer::GunSmoke, 4, at - dir * 0.6, -dir, Color::WHITE, 4.0);
    fx.emit_tint(Layer::GunSmoke, 3, at + dir * 0.5, dir, Color::WHITE, 3.0);
    fx.flash(
        at,
        super::FlashDef {
            lumens: 300_000.0,
            range: 8.0,
            life: 0.1,
            ..MUZZLE_FLASH
        },
    );
}

/// The colour `arm`'s flash lights things.
pub fn light_of(arm: u8) -> Color {
    let [r, g, b] = tint(&table::gun(arm));
    Color::linear_rgb(r, g, b)
}

/// A muzzle light's colour: the flash's, at a peak of 1.
fn tint(look: &GunLook) -> [f32; 3] {
    let c = look.flash_c;
    let m = c[0].max(c[1]).max(c[2]).max(1e-3);
    [c[0] / m, c[1] / m, c[2] / m]
}

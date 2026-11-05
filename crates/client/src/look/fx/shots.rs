//! Gunfire. **Ours is drawn the frame its trigger breaks**, as the gun we
//! predict fires: its flash and smoke at the muzzle, its tracer down the
//! view (DESIGN §5; after Gates' `fx/gun.rs`). The server's word on that
//! shot arrives a round trip later and draws only what the round met.
//! Everyone else's is drawn off the event: a star flash, a tongue of fire
//! and smoke at their muzzle, a tracer down the line the round really took
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

use super::table::{self, Casing, Flash, GunLook, Layer, Matter, Weapon, OWN_STAR_LIFE_S};
use super::{Fx, Scene, MUZZLE_FLASH};
use crate::look::shake;

/// From a shooter's eye to their muzzle: forward, then down.
const MUZZLE_FWD_M: f32 = 0.75;
const MUZZLE_DOWN_M: f32 = 0.28;
/// Our own muzzle, in view space: ahead, right, down (where the gun in
/// view holds it), and how far ahead of it our tracer starts.
const OWN_MUZZLE: Vec3 = Vec3::new(0.12, -0.1, 0.7);
/// Our own flash's stars, times an impact's and the gun's `flash_scale`:
/// [`OWN_MUZZLE`] is a hand's length from the eye, where a third person's
/// size would cover a fifth of the view.
const OWN_STAR_SCALE: f32 = 0.8;
const OWN_TRACER_SKIP_M: f32 = 1.0;
/// Our shotgun's pellets drawn in a cone this wide (radians): the real
/// scatter is the server's secret, and its marks are where they went.
const PELLET_DRAW_RAD: f32 = 0.09;
/// A standing shooter's eye over the floor, roughly: where their casings
/// land.
const EYE_OVER_FLOOR_M: f32 = 1.6;

fn v3(v: Vec3) -> V3 {
    V3::new(v.x, v.y, v.z)
}

/// Someone's shot, off the server's event.
pub fn shot(fx: &mut Fx, scene: &Scene, e: &EventState, from: Vec3, to: Vec3) {
    let arm = e.c as u8;
    let look = table::gun(arm);
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
    if own {
        // Drawn when it broke (`own`): only what it met is new.
        if kind.ray() {
            land(fx, scene, arm, from, dir, len, to);
        }
        return;
    }
    if kind == Kind::Sword {
        return;
    }
    let muzzle = from + dir * MUZZLE_FWD_M - Vec3::Y * MUZZLE_DOWN_M;
    if first {
        flash(fx, muzzle, dir, &look);
        // The round's casing, from the port at the shooter's right (a
        // shotgun's pellets are one round). A standing shooter's floor is
        // about their eye's height below them.
        eject(
            fx,
            &look,
            muzzle - dir * 0.3,
            dir.cross(Vec3::Y).normalize_or(Vec3::X),
            from.y - EYE_OVER_FLOOR_M,
        );
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
fn own_muzzle(left: bool, drawn: Option<Vec3>) -> Vec3 {
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
    // (`muzzle`).
    fx.own_flash[usize::from(left)] = Some(OwnFlash {
        look,
        rocket: id == ROCKET,
        rounds,
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

/// Our own gun's flash this frame, by hand, waiting for the frame's camera
/// and gun: the gun's look, and how many rounds it fired this frame.
#[derive(Clone, Copy, Debug)]
pub struct OwnFlash {
    look: GunLook,
    rocket: bool,
    rounds: u32,
}

/// Our own flashes, at the muzzle as the frame draws it: off the camera
/// as placed this frame (`eye`: its kick, its shake, a film's roll and
/// easing), which carries the gun in view, and the gun as posed this frame
/// (`drawn`, `viewmodel::ViewMuzzles`), not the aim the shot was taken down
/// or last frame's pose. Placed off those, a star sat beside the barrel,
/// and on a slow frame (a film's, a weak browser's) well off it.
pub fn muzzle(fx: &mut Fx, eye: &Transform, drawn: [Option<Vec3>; 2]) {
    for (i, drawn) in drawn.into_iter().enumerate() {
        let Some(o) = fx.own_flash[i].take() else {
            continue;
        };
        let muzzle = eye.translation + eye.rotation * own_muzzle(i == 1, drawn);
        let fwd = eye.rotation * Vec3::NEG_Z;
        let right = eye.rotation * Vec3::X;
        let scale = OWN_STAR_SCALE * o.look.flash_scale;
        match o.look.flash {
            Flash::Powder => {
                // The baked petals for a frame or two (the view swings), a
                // hot star at the core, a tongue down the barrel, smoke.
                let r = ::fx::recipe::Recipe {
                    life: (OWN_STAR_LIFE_S.0 * 1.4, OWN_STAR_LIFE_S.1 * 1.4),
                    ..table::recipe(Layer::MuzzleFlash)
                };
                let pool = fx.pool(Layer::MuzzleFlash.pool());
                table::emit_recipe(pool, &r, 1, muzzle + fwd * 0.04, fwd, Color::WHITE, scale * 1.1);
                table::own_stars(&mut fx.glow, muzzle, o.look.stars as usize, scale * 0.7, o.look.flash_c);
                fx.emit(Layer::Tongue, 1, muzzle, fwd, Matter::Concrete);
                fx.emit_tint(Layer::Wisp, 1, muzzle + fwd * 0.12, fwd, Color::WHITE, scale.max(0.7));
                fx.emit(Layer::GunSmoke, 1, muzzle, fwd, Matter::Concrete);
            }
            Flash::Energy(tone) => {
                bloom(fx, muzzle + fwd * 0.03, fwd, tone, scale, OWN_STAR_LIFE_S.1 * 2.0);
                table::own_stars(&mut fx.glow, muzzle, 1, scale * 0.6, o.look.flash_c);
            }
        }
        fx.flash(
            muzzle,
            super::FlashDef {
                color: tint(&o.look),
                ..MUZZLE_FLASH
            },
        );
        for _ in 0..o.rounds.min(3) {
            eject(
                fx,
                &o.look,
                muzzle - fwd * 0.3 + right * 0.04,
                (right * 0.8 + eye.rotation * Vec3::Y * 0.5).normalize_or(right),
                eye.translation.y - EYE_OVER_FLOOR_M,
            );
        }
        if o.rocket {
            backblast(fx, muzzle, fwd);
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

/// A third-person muzzle. Powder: the baked flash of petals, a star at its
/// core, a tongue of fire down the barrel, a breath of smoke. Energy: a
/// baked bloom in the gun's tone, a star, a glow that hangs on the muzzle
/// a moment, no smoke. And a light.
pub fn flash(fx: &mut Fx, at: Vec3, dir: Vec3, look: &GunLook) {
    let scale = look.flash_scale;
    match look.flash {
        Flash::Powder => {
            fx.emit_tint(Layer::MuzzleFlash, 1, at + dir * 0.05, dir, Color::WHITE, 1.3 * scale);
            table::stars(&mut fx.glow, at, look.stars as usize, 1.6 * scale, look.flash_c);
            fx.emit(Layer::Tongue, 1, at, dir, Matter::Concrete);
            fx.emit_tint(Layer::Wisp, 1, at + dir * 0.15, dir, Color::WHITE, scale.max(0.8));
            fx.emit(Layer::GunSmoke, 1, at, dir, Matter::Concrete);
        }
        Flash::Energy(tone) => {
            bloom(fx, at + dir * 0.04, dir, tone, 1.2 * scale, 0.0);
            table::stars(&mut fx.glow, at, 1, 1.2 * scale, look.flash_c);
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
        glow: 0.6,
        life: (0.16, 0.22),
        size: (0.07, 0.09),
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

/// A rocket leaving the tube: fire out of the front, smoke out of both ends.
fn backblast(fx: &mut Fx, at: Vec3, dir: Vec3) {
    fx.emit_tint(Layer::Flash, 3, at + dir * 0.4, dir, Color::WHITE, 5.0);
    fx.emit_tint(Layer::Fireball, 3, at + dir * 0.3, dir, Color::WHITE, 0.6);
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

/// A muzzle light's colour: the flash's, at a peak of 1.
fn tint(look: &GunLook) -> [f32; 3] {
    let c = look.flash_c;
    let m = c[0].max(c[1]).max(c[2]).max(1e-3);
    [c[0] / m, c[1] / m, c[2] / m]
}

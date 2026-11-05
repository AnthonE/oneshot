//! The gun in our own view, and the motion that makes it read as held.
//!
//! After Gates' `crates/client/src/render/viewmodel.rs` (MIT): its hold
//! (`VIEWMODEL_HOLD`), the shoulder it turns about
//! (`VIEWMODEL_SWING_PIVOT`), its bob off distance walked, its sway off the
//! look rate, its heave off the body's vertical speed and its C¹ pulses
//! ([`bump`], the stroke's phases); and the visual half of its `gunfeel.rs`,
//! the kick a round gives the gun the frame its trigger breaks. Trimmed to a
//! gun with no arms, and timed by what our own predicted fighter says.
//!
//! **Each motion is a different input**, as Gates keeps them apart:
//!
//!   · **Bob** — am I walking? Off the distance the drawn body covers on the
//!     ground ([`BOB_PER_M`] cycles a metre; a crouch's shorter stride takes
//!     more), never off the clock, so it stops when the feet do. Sideways once
//!     a stride, down twice (`sin²`: lowest under each footfall), its size by
//!     the speed up to the sim's `RUN`.
//!   · **Sway** — am I turning? The look rate, through a spring about the
//!     shoulder, clamped so a flick cannot throw the gun across the frame;
//!     and a cant into a strafe.
//!   · **Heave** — did I leave the ground? A take-off lifts the gun, the air
//!     carries it up and floats its muzzle with the fall, and a landing dips
//!     it by how fast we came down; a crouch brings it in and down.
//!   · **Breath** — standing still, a slow figure of eight, gone at a run.
//!   · **Strokes** — did I just do something? Each off the fighter's own
//!     state, on the sim's own clocks: a round kicks and flashes the hand
//!     whose magazine it came from ([`Rounds`]); a change of gun lowers the
//!     old one out and raises the new in `arms::SWAP_MS`; a reload tilts the
//!     gun in, tugs the magazine out, slaps one in and racks it (a hot gun
//!     vents instead) over the reload's own ticks; a melee
//!     (`Fighter::melee_cool` jumping) bashes across the frame and is back up
//!     when the gun may fire again, a sword's swing slashes; a throw
//!     (`gren_cool` jumping) dips the gun out of the way; a charge trembles.
//!
//! Each layer ([`Lay`]) turns the arm about the shoulder, turns the gun in
//! the hand about its grip (a kick's climb, a reload's tilt), or shifts it.
//! Frame-rate independent: every ease is `1 − exp(−k·dt)`, every spring is
//! integrated exactly (`shake::spring`), every clock is in seconds. With a
//! gun in each hand the left is the right mirrored: its rest, its kick, its
//! reload, its change; what the body does (bob, sway, heave, breath) moves
//! both the same way. Hidden when dead, at the wheel or behind a scope; a
//! passenger, who fights from the seat, holds it.
//!
//! No stroke but a sword's slash brings any part of a steadily held gun
//! within 7° of the crosshair (`the_gun_keeps_off_the_crosshair`), and
//! none of it varies
//! by graphics tier, so it hides nothing from anyone that it doesn't hide
//! from everyone.
//!
//! Drawn by the world's camera, not a second one: the gun is shrunk by
//! [`SHRINK`] and brought that much closer to the eye, which leaves the
//! picture unchanged and puts all of it nearer than any wall can be (a
//! body's radius), so it never pokes through one. It is lit and shadowed by
//! the world as everything else is, and transforms are all it writes, so the
//! desktop, WebGPU and WebGL2 draw the same motion with nothing to fall back
//! from. **It decides nothing**: it reads the predicted fighter and the look
//! and writes only its own transforms, the flash and where its muzzles are
//! ([`ViewMuzzles`], for the flash sprite); the aim we send never sees it,
//! and the camera's kick on our shots is `shake.rs`'s, on the drawn camera.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use core::f32::consts::{FRAC_PI_2, PI, TAU};
use sim::arms::{arm, Arm, Kind, NONE, ROCKET, SWAP_MS};
use sim::consts::{ticks, GREN_COOL_MS, MELEE_COOL_MS, RUN, TICK_HZ};
use sim::fighter::{Fighter, Gun};

use super::guns::{gun, spawn_posed, view_basis, Models};
use super::shake::{peak_to_speed, spring};

/// How much smaller, and nearer, the gun is than it looks.
const SHRINK: f32 = 0.25;
/// Gates' `VIEWMODEL_HOLD`: where the right hand's grip sits in view space,
/// as it looks (metres, before [`SHRINK`]).
const HOLD: Vec3 = Vec3::new(0.3, -0.29, -0.5);
/// The rest yaw: the muzzle turned in toward the crosshair, radians.
const REST_YAW: f32 = 0.06;
/// Gates' `VIEWMODEL_SWING_PIVOT`: the right shoulder, behind and below the
/// eye. The arm turns about it, so a turn sweeps the gun across the frame
/// rather than into the lens.
const PIVOT: Vec3 = Vec3::new(0.05, -0.10, 0.30);
/// The longest frame the motion integrates, seconds: a hitch is a pause,
/// not a leap.
const MAX_DT: f32 = 0.1;

/// The flash's peak, in lumens: a few thousand lux on a wall a metre off,
/// enough to read against noon.
const FLASH_LM: f32 = 400_000.0;
const FLASH_RANGE_M: f32 = 6.0;
/// The flash's fall, per second: gone in 40 ms.
const FLASH_FALL_PER_S: f32 = 25.0;
/// Where the flash light sits before a gun has drawn a muzzle.
const FLASH_AT: Vec3 = Vec3::new(0.3, -0.2, -1.0);

// ── Bob ─────────────────────────────────────────────────────────────────
/// Bob cycles per metre walked: one cycle is two footfalls, a footfall
/// every ~2 m at the sim's 7 m/s run (Gates' 0.55 is for a 4 m/s walk).
pub const BOB_PER_M: f32 = 0.24;
/// A crouch's shorter stride.
pub const BOB_CROUCH_PER_M: f32 = 0.36;
/// Sideways and down at a full run, metres; the cant with the weight shift,
/// radians.
pub const BOB_X: f32 = 0.016;
pub const BOB_Y: f32 = 0.012;
pub const BOB_ROLL: f32 = 0.022;
/// A crouch bobs this much of a run's.
pub const BOB_CROUCH_K: f32 = 0.6;
/// How fast the drawn speed follows the body's, 1/s.
const SPEED_EASE: f32 = 10.0;
/// A step longer than this in a frame is a spawn or a correction, not a
/// stride, metres.
const TELEPORT_M: f32 = 3.0;

// ── Sway ────────────────────────────────────────────────────────────────
/// Radians of trail per radian a second of look rate (Gates' 0.055, halved:
/// a duel's flicks are faster than a survivor's glances).
pub const SWAY_GAIN: f32 = 0.028;
/// The most it trails, radians.
pub const SWAY_MAX: f32 = 0.09;
/// The trail's spring: a touch under critically damped, so it settles with
/// the faintest overshoot.
pub const SWAY_HZ: f32 = 2.4;
pub const SWAY_DAMP: f32 = 0.72;
/// How much it banks into a turn, per radian of yaw trail.
pub const SWAY_BANK: f32 = 0.5;
/// A look jump bigger than this in a frame is a respawn's facing, not a
/// turn, radians.
const SNAP_RAD: f32 = 1.0;
/// The cant and the lag of a strafe at a full run, radians and metres, and
/// how fast they follow, 1/s.
pub const STRAFE_ROLL: f32 = 0.035;
pub const STRAFE_SHIFT_M: f32 = 0.008;
const STRAFE_EASE: f32 = 8.0;

// ── Heave ───────────────────────────────────────────────────────────────
/// The heave's spring (Gates' ~1.7 Hz, well damped).
pub const HEAVE_HZ: f32 = 1.9;
pub const HEAVE_DAMP: f32 = 0.55;
/// A take-off faster than this (m/s) is a jump, and lifts the gun this far
/// at the spring's peak, metres.
const TAKEOFF_MPS: f32 = 1.5;
pub const TAKEOFF_LIFT_M: f32 = 0.012;
/// A landing faster than `LAND_MIN_MPS` dips it, more the harder.
const LAND_MIN_MPS: f32 = 3.0;
pub const LAND_DIP_M: f32 = 0.006;
pub const LAND_DIP_PER_MPS: f32 = 0.004;
pub const LAND_DIP_MAX_M: f32 = 0.05;
/// In the air: up, nearer and the muzzle raised, and how fast that comes
/// on, 1/s.
pub const AIR_LIFT_M: f32 = 0.014;
pub const AIR_TUCK_M: f32 = 0.012;
pub const AIR_PITCH: f32 = 0.025;
const AIR_EASE: f32 = 9.0;
/// The muzzle floats up as we fall and dips as we rise, radians per m/s,
/// to at most `AIR_FLOAT_MAX`.
pub const AIR_FLOAT_PER_MPS: f32 = 0.005;
pub const AIR_FLOAT_MAX: f32 = 0.06;
/// Crouched: in toward the middle, down and nearer (metres), the arm
/// turned in and the gun canted in the hand (radians), and how fast, 1/s.
pub const CROUCH_OFF: Vec3 = Vec3::new(-0.035, -0.03, 0.015);
pub const CROUCH_ANG: Vec3 = Vec3::new(0.0, 0.03, 0.0);
pub const CROUCH_CANT: f32 = 0.07;
const CROUCH_EASE: f32 = 10.0;

// ── Breath ──────────────────────────────────────────────────────────────
/// About thirteen breaths a minute; down and up (metres), the muzzle with
/// it (radians), and sideways at half the rate.
pub const BREATH_HZ: f32 = 0.22;
pub const BREATH_Y: f32 = 0.0035;
pub const BREATH_X: f32 = 0.0022;
pub const BREATH_PITCH: f32 = 0.006;
/// Crouched, steadier.
pub const BREATH_CROUCH_K: f32 = 0.6;

// ── Kick and flash ──────────────────────────────────────────────────────
/// The kick's spring: quick, a little overshoot forward as it settles.
pub const KICK_HZ: f32 = 6.5;
pub const KICK_DAMP: f32 = 0.55;
/// At a single shot's peak: back toward the eye and up (metres), the muzzle
/// climbing about the grip (radians), and a random cant either way.
pub const KICK_BACK_M: f32 = 0.055;
pub const KICK_UP_M: f32 = 0.012;
pub const KICK_CLIMB: f32 = 0.085;
pub const KICK_ROLL: f32 = 0.035;
/// An automatic's round kicks this much of a single shot's, and a burst's
/// round this much; a rocket, a cannon and a shotgun's slug of pellets this
/// much more. Under sustained fire the spring rides up, to at most
/// `KICK_MAX` of a single shot.
pub const KICK_AUTO: f32 = 0.4;
pub const KICK_BURST: f32 = 0.55;
pub const KICK_HEAVY: f32 = 1.3;
pub const KICK_MAX: f32 = 1.8;
/// Rounds counted in one frame at most (as `fx/shots.rs` draws).
const MAX_ROUNDS: u32 = 3;
/// A charge weapon held: drawn back to this at full (metres) and trembling
/// this much (radians), this fast.
pub const CHARGE_PULL_M: f32 = 0.025;
pub const CHARGE_SHAKE: f32 = 0.008;
const CHARGE_SHAKE_HZ: f32 = 31.0;

// ── Change ──────────────────────────────────────────────────────────────
/// A change of gun takes the sim's swap: the share of it spent lowering
/// the old gun, the rest raising the new, so the new is up when it may fire.
pub const LOWER_SHARE: f32 = 0.4;
pub const LOWER_S: f32 = SWAP_MS as f32 * LOWER_SHARE / 1000.0;
pub const RAISE_S: f32 = SWAP_MS as f32 * (1.0 - LOWER_SHARE) / 1000.0;
/// All the way down: out of the bottom of the frame, the muzzle dipped and
/// the top canted out.
pub const DROP_OFF: Vec3 = Vec3::new(0.04, -0.16, 0.06);
pub const DROP_ANG: Vec3 = Vec3::new(-0.45, 0.0, -0.3);

// ── Reload ──────────────────────────────────────────────────────────────
/// How a reload holds the gun at its middle, turned in the hand about its
/// grip: a long gun tipped in, muzzle up, the magazine toward us; a pistol
/// lower and turned further.
pub const RELOAD_LONG_OFF: Vec3 = Vec3::new(-0.05, -0.05, 0.03);
pub const RELOAD_LONG_ANG: Vec3 = Vec3::new(0.2, 0.15, 0.5);
pub const RELOAD_PISTOL_OFF: Vec3 = Vec3::new(-0.04, -0.07, 0.05);
pub const RELOAD_PISTOL_ANG: Vec3 = Vec3::new(0.3, 0.2, 0.75);
/// A hot gun vents instead: muzzle up, canted out, held off and buzzing.
pub const VENT_OFF: Vec3 = Vec3::new(0.02, 0.02, 0.05);
pub const VENT_ANG: Vec3 = Vec3::new(0.4, 0.0, -0.3);
pub const VENT_BUZZ: f32 = 0.004;
const VENT_BUZZ_HZ: f32 = 23.0;
/// The share of the reload spent tipping in and coming back.
const RELOAD_IN: f32 = 0.18;
const RELOAD_OUT: f32 = 0.2;
/// The beats, as shares of the reload: the magazine tugged out, a new one
/// slapped in, the action racked (a long gun's bolt, a pistol's slide).
const MAG_OUT_AT: f32 = 0.26;
const MAG_IN_AT: f32 = 0.56;
const RACK_AT: f32 = 0.74;
pub const MAG_OUT_DROP_M: f32 = 0.014;
pub const MAG_IN_SLAP_M: f32 = 0.018;
pub const RACK_M: f32 = 0.025;

// ── Melee and throw ─────────────────────────────────────────────────────
/// A melee takes the half of its cooldown the gun may not fire in (the
/// sim's `refire` after one), in its own whole ticks.
pub const MELEE_S: f32 = ticks(MELEE_COOL_MS / 2) as f32 / TICK_HZ as f32;
/// Its shape: the cock's peak at half the wind-up, the blow at the apex,
/// held, then back (shares of `MELEE_S`). The sim's blow lands the tick
/// the button goes down, so the apex is early.
const MELEE_WINDUP: f32 = 0.1;
const MELEE_APEX: f32 = 0.22;
const MELEE_HOLD: f32 = 0.18;
/// The bash: drawn back and out, then swung in across the bottom of the
/// frame, canted, and driven forward (radians about the shoulder, metres);
/// the grip stops under the crosshair, never on it.
pub const BASH_COCK: Vec3 = Vec3::new(-0.05, -0.12, -0.15);
pub const BASH_DRAW: Vec3 = Vec3::new(0.03, -0.02, 0.06);
pub const BASH_STRIKE: Vec3 = Vec3::new(0.0, 0.35, 0.35);
pub const BASH_THROW: Vec3 = Vec3::new(-0.04, 0.0, -0.08);
/// A sword's swing (its melee and its trigger): raised out, then slashed
/// right to left.
pub const SLASH_COCK: Vec3 = Vec3::new(0.25, -0.3, -0.4);
pub const SLASH_DRAW: Vec3 = Vec3::new(0.05, 0.08, 0.04);
pub const SLASH_STRIKE: Vec3 = Vec3::new(0.0, 0.6, 0.6);
pub const SLASH_THROW: Vec3 = Vec3::new(-0.06, 0.06, -0.06);
/// A grenade's throw takes most of its cooldown; the gun dips out of the
/// throwing arm's way and comes back.
pub const THROW_S: f32 = GREN_COOL_MS as f32 * 0.85 / 1000.0;
pub const THROW_OFF: Vec3 = Vec3::new(0.05, -0.12, 0.05);
pub const THROW_ANG: Vec3 = Vec3::new(-0.25, -0.1, -0.25);
const THROW_IN: f32 = 0.2;
const THROW_OUT: f32 = 0.45;

/// One hand's gun.
#[derive(Component, Default)]
pub struct ViewGun {
    /// The left hand's (dual wielding), not the right's.
    left: bool,
    /// The gun whose model is spawned: what is drawn, which a change lags.
    drawn: u8,
    /// How far down for a change, 0..=1.
    drop: f32,
    kick: Spring,
    /// The last round's cant, -1..1.
    cant: f32,
    flash: f32,
    /// The reload under way: its length in ticks as it began, and the ticks
    /// it had left last frame.
    reload_of: u16,
    reload_was: u16,
    /// The reload's pose last frame: a reload cut short (a melee, a swap,
    /// a pickup) eases back to rest from it rather than snapping.
    reload_lay: Lay,
    /// Rounds in the burst under way, and seconds since its last one.
    burst: u32,
    since_round: f32,
}

/// An automatic's burst drifts its kick to one side, growing to all of it
/// over this many rounds (Gates' gunfeel `DRIFT_SHOTS`); a trigger rested
/// [`BURST_REST_S`] starts a new burst.
pub const DRIFT_SHOTS: f32 = 6.0;
pub const BURST_REST_S: f32 = 0.3;

/// A round's cant, -1..1: random per round, an automatic's leaning toward
/// its hand's outside as the burst `n` rounds in grows.
pub fn burst_cant(auto: bool, n: u32, left: bool, rand: f32) -> f32 {
    if !auto {
        return rand;
    }
    let g = (n as f32 / DRIFT_SHOTS).min(1.0);
    let side = if left { -1.0 } else { 1.0 };
    side * g + rand * (1.0 - g) * 0.5
}

/// How much of a round's kick a crouched fighter's gun takes, in the hands
/// and on the camera (Gates' gunfeel `CROUCH_KICK`): braced, it climbs
/// half as far. Drawing only.
pub const CROUCH_KICK: f32 = 0.5;

/// How fast a reload cut short lets go of its pose, 1/s: two thirds of
/// the way back to rest in a fifteenth of a second.
const RELOAD_CUT_PER_S: f32 = 15.0;
/// A rise in the ticks left this big or bigger is a new reload, not a
/// correction moving the running one by a tick or two.
const RELOAD_RELATCH_TICKS: u16 = 8;

#[derive(Component)]
pub struct Flash;

/// Where each hand's muzzle is drawn, view space as it looks (metres,
/// before [`SHRINK`]), as last posed (this frame's once `draw_view` has
/// run): `[right, left]`, `None` where no gun is drawn. The flash sprite of
/// our own shot sits on it (`fx/shots.rs`).
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct ViewMuzzles(pub [Option<Vec3>; 2]);

/// The hold and the flash, on the camera.
pub fn spawn(commands: &mut Commands, eye: Entity) {
    for left in [false, true] {
        commands.spawn((
            ViewGun { left, ..default() },
            Transform::from_translation(HOLD * SHRINK).with_scale(Vec3::splat(SHRINK)),
            Visibility::Hidden,
            ChildOf(eye),
        ));
    }
    commands.spawn((
        Flash,
        PointLight {
            intensity: 0.0,
            range: FLASH_RANGE_M,
            color: Color::srgb(1.0, 0.75, 0.4),
            shadows_enabled: false,
            ..default()
        },
        Transform::from_translation(FLASH_AT),
        ChildOf(eye),
    ));
}

/// What the frame knows about us.
pub struct Own<'a> {
    pub f: &'a Fighter,
    /// Where the body is drawn this frame (its feet), world metres.
    pub at: Vec3,
    /// Where we look, radians: read, never written.
    pub yaw: f32,
    pub pitch: f32,
    /// In a seat: the stride and the footing are the vehicle's, not ours.
    pub riding: bool,
    pub shown: bool,
}

/// An exactly integrated spring about a moving rest.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spring {
    pub x: f32,
    v: f32,
}

impl Spring {
    /// `dt` seconds toward `to`.
    pub fn toward(&mut self, to: f32, hz: f32, damp: f32, dt: f32) {
        let mut x = self.x - to;
        spring(&mut x, &mut self.v, hz, damp, dt);
        self.x = x + to;
    }

    /// A blow that on its own peaks at `peak` from rest.
    pub fn hit(&mut self, peak: f32, hz: f32, damp: f32) {
        self.v += peak * peak_to_speed(hz, damp);
    }
}

/// One layer of motion: the arm turned about the shoulder, the gun turned
/// in the hand about its grip, and a shift. Angles are pitch (muzzle up +),
/// yaw (muzzle left +), roll (top left +), radians; the shift is metres as
/// it looks: right, up, back toward the eye.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lay {
    pub ang: Vec3,
    pub wrist: Vec3,
    pub off: Vec3,
}

impl core::ops::Add for Lay {
    type Output = Lay;
    fn add(self, o: Lay) -> Lay {
        Lay {
            ang: self.ang + o.ang,
            wrist: self.wrist + o.wrist,
            off: self.off + o.off,
        }
    }
}

impl core::ops::Mul<f32> for Lay {
    type Output = Lay;
    fn mul(self, k: f32) -> Lay {
        Lay {
            ang: self.ang * k,
            wrist: self.wrist * k,
            off: self.off * k,
        }
    }
}

fn turn(a: Vec3) -> Quat {
    Quat::from_euler(EulerRot::YXZ, a.y, a.x, a.z)
}

fn flip(a: Vec3) -> Vec3 {
    Vec3::new(a.x, -a.y, -a.z)
}

impl Lay {
    pub const ZERO: Lay = Lay {
        ang: Vec3::ZERO,
        wrist: Vec3::ZERO,
        off: Vec3::ZERO,
    };

    /// About the shoulder.
    fn arm(ang: Vec3, off: Vec3) -> Lay {
        Lay { ang, off, ..Lay::ZERO }
    }

    /// In the hand.
    fn hand(wrist: Vec3, off: Vec3) -> Lay {
        Lay {
            wrist,
            off,
            ..Lay::ZERO
        }
    }

    /// The same motion for the other hand: across the view's middle.
    pub fn mirror(self) -> Lay {
        Lay {
            ang: flip(self.ang),
            wrist: flip(self.wrist),
            off: Vec3::new(-self.off.x, self.off.y, self.off.z),
        }
    }
}

/// `q` reflected across the view's middle (x → −x): a turn's pitch kept,
/// its yaw and roll reversed.
pub fn mirror_q(q: Quat) -> Quat {
    Quat::from_xyzw(q.x, -q.y, -q.z, q.w)
}

/// Where a hand's gun is drawn, as it looks: the hand's own motion (the
/// right hand's, mirrored for the left) about that hand's shoulder, then
/// the body's, unmirrored, about the same shoulder.
pub fn place(hand: Lay, body: Lay, left: bool) -> (Vec3, Quat) {
    let side = |v: Vec3| if left { Vec3::new(-v.x, v.y, v.z) } else { v };
    let hand = if left { hand.mirror() } else { hand };
    let rest = Quat::from_rotation_y(if left { -REST_YAW } else { REST_YAW });
    let pivot = side(PIVOT);
    let hq = turn(hand.ang);
    let p = pivot + hq * (side(HOLD) - pivot) + hand.off;
    let bq = turn(body.ang);
    let wrist = turn(body.wrist) * turn(hand.wrist);
    (pivot + bq * (p - pivot) + body.off, bq * hq * rest * wrist)
}

/// The rounds each hand fired since the last look, `[right, left]`.
///
/// The fighter counts both hands' rounds in one `shots`; each left round
/// sets the left hand's refire again, so with two guns out a rise in it is
/// the left's round (a pickup sets it too, with a full magazine) and the
/// right takes the rest. Not the magazine alone: at the end of a reload the
/// left fills and fires in one tick, and a charged round spends several.
/// One a look at most from the left: a frame slow enough for two is rare,
/// and this is drawing only (the server's events say what really fired).
#[derive(Clone, Copy, Debug, Default)]
pub struct Rounds {
    shots: u32,
    /// The left gun, its magazine and its refire, last look.
    left: (u8, u16, u16),
    seen: bool,
}

impl Rounds {
    pub fn take(&mut self, f: &Fighter) -> [u32; 2] {
        let total = if self.seen && f.shots > self.shots {
            (f.shots - self.shots).min(MAX_ROUNDS)
        } else {
            0
        };
        let (arm, mag, refire) = self.left;
        let left = if f.dual() && arm == f.left.arm {
            let rose = f.left_refire > refire && f.left.mag < Gun::full(f.left.arm).mag;
            // A round whose refire ran out between two looks still spent.
            let spent = f.left.mag < mag;
            u32::from(rose || spent).min(total)
        } else {
            0
        };
        *self = Rounds {
            shots: f.shots,
            left: (f.left.arm, f.left.mag, f.left_refire),
            seen: true,
        };
        [total - left, left]
    }
}

/// The body's motion and the strokes' clocks: one per view, a frame at a
/// time.
#[derive(Default)]
pub struct Motion {
    started: bool,
    last_at: Vec3,
    last_yaw: f32,
    last_pitch: f32,
    alive: bool,
    /// Ground speed as drawn, eased, m/s.
    speed: f32,
    /// The bob's phase, radians, kept within a turn.
    bob: f32,
    /// The trail: yaw, pitch.
    sway: [Spring; 2],
    /// The strafe, -1..1 of a run across the view, eased.
    strafe: f32,
    heave: Spring,
    was_grounded: bool,
    air_vy: f32,
    /// How far into the air pose and the crouch, 0..=1, eased.
    air: f32,
    crouch: f32,
    /// The breath's phase, radians, kept within two turns.
    breath: f32,
    /// A clock for the trembles, seconds, kept within a minute.
    t: f32,
    rounds: Rounds,
    /// Seconds into the melee under way, and whether it is a sword's.
    melee: Option<f32>,
    slash: bool,
    melee_was: u16,
    /// Seconds into the throw under way.
    throw: Option<f32>,
    gren_was: u16,
}

/// `1 − exp(−k·dt)`: the share of the way a one-pole ease goes in `dt`.
fn ease(k: f32, dt: f32) -> f32 {
    1.0 - (-k * dt).exp()
}

/// Shortest signed arc into `-π..π`.
fn wrap_pi(a: f32) -> f32 {
    let mut x = a % TAU;
    if x > PI {
        x -= TAU;
    } else if x < -PI {
        x += TAU;
    }
    x
}

/// Gates' pulse: 0 → 1 → 0 over `u` ∈ [0, 1], rising across `attack` of
/// it. C¹ at all three joints (`sin²` up, `cos²` down).
pub fn bump(u: f32, attack: f32) -> f32 {
    if u <= 0.0 || u >= 1.0 {
        return 0.0;
    }
    let a = attack.clamp(1e-3, 1.0 - 1e-3);
    if u < a {
        let t = FRAC_PI_2 * (u / a);
        t.sin() * t.sin()
    } else {
        let t = FRAC_PI_2 * ((u - a) / (1.0 - a));
        t.cos() * t.cos()
    }
}

/// Up across the first `rise` of `u`, held, down across the last `fall`;
/// C¹ throughout.
pub fn envelope(u: f32, rise: f32, fall: f32) -> f32 {
    if u <= 0.0 || u >= 1.0 {
        return 0.0;
    }
    if u < rise {
        let t = FRAC_PI_2 * u / rise;
        t.sin() * t.sin()
    } else if u > 1.0 - fall {
        let t = FRAC_PI_2 * (u - (1.0 - fall)) / fall;
        t.cos() * t.cos()
    } else {
        1.0
    }
}

/// Gates' stroke (`swing_phases`) with its shares as arguments: `(cock,
/// strike)` at `u` ∈ [0, 1]. The cock peaks half way through the wind-up
/// and falls into the apex as the strike rises to it; the strike holds for
/// `hold` and eases back by the end.
pub fn stroke(u: f32, windup: f32, apex: f32, hold: f32) -> (f32, f32) {
    let held = (apex + hold).min(1.0);
    let cock = bump(u / apex, 0.5 * windup / apex);
    let strike = if u <= windup || u >= 1.0 {
        0.0
    } else if u < apex {
        let t = FRAC_PI_2 * (u - windup) / (apex - windup);
        t.sin() * t.sin()
    } else if u < held {
        1.0
    } else {
        let t = FRAC_PI_2 * (u - held) / (1.0 - held);
        t.cos() * t.cos()
    };
    (cock, strike)
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A change of gun, a frame on: `(drop, change the model now)`. The old gun
/// goes down over [`LOWER_S`]; at the bottom the model changes and the new
/// one comes up over [`RAISE_S`]. From an empty hand, or out of sight, the
/// model changes at once and comes up from the bottom.
pub fn drop_step(drop: f32, drawn: u8, want: u8, dt: f32, shown: bool) -> (f32, bool) {
    if drawn == want {
        return ((drop - dt / RAISE_S).max(0.0), false);
    }
    if !shown || drawn == NONE {
        return (1.0, true);
    }
    let d = drop + dt / LOWER_S;
    if d >= 1.0 {
        (1.0, true)
    } else {
        (d, false)
    }
}

/// How hard a round of `a` kicks, as a share of a single shot's.
pub fn kick_of(id: u8, a: &Arm) -> f32 {
    if id == ROCKET || matches!(a.kind, Kind::Cannon) || a.pellets > 1 {
        KICK_HEAVY
    } else if a.auto {
        KICK_AUTO
    } else if a.burst > 1 {
        KICK_BURST
    } else {
        1.0
    }
}

/// The reload's pose at `p` ∈ [0, 1] of it: tip in, the magazine out, a
/// new one in, the action racked, back; a hot gun's vent instead.
pub fn reload_lay(p: f32, long: bool, vent: bool, t: f32) -> Lay {
    let e = envelope(p, RELOAD_IN, RELOAD_OUT);
    if vent {
        let buzz = (t * VENT_BUZZ_HZ * TAU).sin() * VENT_BUZZ * e;
        return Lay::hand(
            VENT_ANG * e + Vec3::new(0.0, 0.0, buzz * 4.0),
            VENT_OFF * e + Vec3::new(buzz, 0.0, 0.0),
        );
    }
    let (ang, off) = if long {
        (RELOAD_LONG_ANG, RELOAD_LONG_OFF)
    } else {
        (RELOAD_PISTOL_ANG, RELOAD_PISTOL_OFF)
    };
    let tug = bump((p - MAG_OUT_AT) / 0.12, 0.35);
    let slap = bump((p - MAG_IN_AT) / 0.1, 0.3);
    let rack = bump((p - RACK_AT) / 0.12, 0.5);
    let (rack_m, rack_roll) = if long { (RACK_M, 0.04) } else { (RACK_M * 0.6, 0.0) };
    Lay::hand(
        ang * e + Vec3::new(-0.05 * tug + 0.08 * slap, 0.0, 0.05 * tug - rack_roll * rack),
        off * e
            + Vec3::new(
                0.0,
                -MAG_OUT_DROP_M * tug + MAG_IN_SLAP_M * slap,
                -0.01 * slap + rack_m * rack,
            ),
    )
}

/// A melee's pose `s` seconds in: a gun's bash, or a sword's slash.
pub fn melee_lay(s: f32, sword: bool) -> Lay {
    let (cock, strike) = stroke(s / MELEE_S, MELEE_WINDUP, MELEE_APEX, MELEE_HOLD);
    let (c, d, k, w) = if sword {
        (SLASH_COCK, SLASH_DRAW, SLASH_STRIKE, SLASH_THROW)
    } else {
        (BASH_COCK, BASH_DRAW, BASH_STRIKE, BASH_THROW)
    };
    Lay::arm(c * cock + k * strike, d * cock + w * strike)
}

/// A throw's pose `s` seconds in.
pub fn throw_lay(s: f32) -> Lay {
    let e = envelope(s / THROW_S, THROW_IN, THROW_OUT);
    Lay::arm(THROW_ANG * e, THROW_OFF * e)
}

impl Motion {
    /// What the body does to both guns this frame.
    fn body(&mut self, own: &Own, dt: f32) -> Lay {
        let f = own.f;
        let step = own.at - self.last_at;
        self.last_at = own.at;
        let mut flat = Vec2::new(step.x, step.z).length();
        if !self.started || flat > TELEPORT_M || own.riding {
            flat = 0.0;
        }
        let mut turn = Vec2::new(wrap_pi(own.yaw - self.last_yaw), own.pitch - self.last_pitch);
        self.last_yaw = own.yaw;
        self.last_pitch = own.pitch;
        if !self.started || turn.x.abs() > SNAP_RAD || turn.y.abs() > SNAP_RAD {
            turn = Vec2::ZERO;
        }
        self.t = (self.t + dt) % 60.0;

        // Bob, off the ground covered.
        let grounded = f.body.grounded || own.riding;
        let crouched = f.body.crouched && !own.riding;
        let speed = if dt > 0.0 { flat / dt } else { self.speed };
        self.speed += (speed.min(RUN * 1.5) - self.speed) * ease(SPEED_EASE, dt);
        if grounded {
            let per_m = if crouched { BOB_CROUCH_PER_M } else { BOB_PER_M };
            self.bob = (self.bob + flat * per_m * TAU) % TAU;
        }
        self.air += (f32::from(u8::from(!grounded)) - self.air) * ease(AIR_EASE, dt);
        self.crouch += (f32::from(u8::from(crouched)) - self.crouch) * ease(CROUCH_EASE, dt);
        let run = (self.speed / RUN).min(1.0);
        let amp = run * (1.0 - self.air) * (1.0 + (BOB_CROUCH_K - 1.0) * self.crouch);
        let (s, _) = self.bob.sin_cos();
        let bob = Lay::hand(
            Vec3::new(0.0, 0.0, -s * BOB_ROLL * amp),
            Vec3::new(s * BOB_X * amp, -s * s * BOB_Y * amp, 0.0),
        );

        // Sway, off the look rate: turning right trails the muzzle left and
        // banks right; looking up trails it down.
        let rate = if dt > 0.0 { turn / dt } else { Vec2::ZERO };
        let want = Vec2::new(
            (rate.x * SWAY_GAIN).clamp(-SWAY_MAX, SWAY_MAX),
            (-rate.y * SWAY_GAIN).clamp(-SWAY_MAX, SWAY_MAX),
        );
        self.sway[0].toward(want.x, SWAY_HZ, SWAY_DAMP, dt);
        self.sway[1].toward(want.y, SWAY_HZ, SWAY_DAMP, dt);
        let (yaw, pitch) = (self.sway[0].x, self.sway[1].x);
        let sway = Lay::arm(Vec3::new(pitch, yaw, -yaw * SWAY_BANK), Vec3::ZERO);

        // A strafe: the gun lags it and banks into it.
        let right = Vec2::new(-own.yaw.sin(), own.yaw.cos());
        let across = if own.riding {
            0.0
        } else {
            (Vec2::new(f.body.vel.x, f.body.vel.z).dot(right) / RUN).clamp(-1.0, 1.0)
        };
        self.strafe += (across - self.strafe) * ease(STRAFE_EASE, dt);
        let strafe = Lay::hand(
            Vec3::new(0.0, 0.0, -self.strafe * STRAFE_ROLL),
            Vec3::new(-self.strafe * STRAFE_SHIFT_M, 0.0, 0.0),
        );

        // Heave: a take-off lifts, a landing dips by the fall.
        if own.riding {
            self.was_grounded = true;
            self.air_vy = 0.0;
        } else {
            if !f.body.grounded {
                if self.was_grounded && self.started && f.body.vel.y > TAKEOFF_MPS {
                    self.heave.hit(TAKEOFF_LIFT_M, HEAVE_HZ, HEAVE_DAMP);
                }
                self.air_vy = f.body.vel.y;
            } else if !self.was_grounded {
                let v = -self.air_vy;
                if v > LAND_MIN_MPS {
                    let dip = (LAND_DIP_M + (v - LAND_MIN_MPS) * LAND_DIP_PER_MPS).min(LAND_DIP_MAX_M);
                    self.heave.hit(-dip, HEAVE_HZ, HEAVE_DAMP);
                }
                self.air_vy = 0.0;
            }
            self.was_grounded = f.body.grounded;
        }
        self.heave.toward(0.0, HEAVE_HZ, HEAVE_DAMP, dt);
        let vy = if grounded { 0.0 } else { f.body.vel.y };
        let float = (-vy * AIR_FLOAT_PER_MPS).clamp(-AIR_FLOAT_MAX, AIR_FLOAT_MAX) * self.air;
        let air = Lay {
            ang: Vec3::new(AIR_PITCH * self.air, 0.0, 0.0),
            wrist: Vec3::new(float, 0.0, 0.0),
            off: Vec3::new(0.0, AIR_LIFT_M * self.air + self.heave.x, AIR_TUCK_M * self.air),
        };

        // Breath, standing.
        self.breath = (self.breath + dt * BREATH_HZ * TAU) % (2.0 * TAU);
        let still = (1.0 - run) * (1.0 - self.air) * (1.0 + (BREATH_CROUCH_K - 1.0) * self.crouch);
        let (b, half) = (self.breath.sin(), (self.breath * 0.5).sin());
        let breath = Lay::hand(
            Vec3::new(b * BREATH_PITCH * still, 0.0, 0.0),
            Vec3::new(half * BREATH_X * still, b * BREATH_Y * still, 0.0),
        );

        self.started = true;
        bob + sway + strafe + air + breath
    }

    /// The right hand's strokes this frame (a melee, a throw), and the
    /// crouch both hands take: `(both, right only)`.
    fn strokes(&mut self, f: &Fighter, right_rounds: u32, dt: f32) -> (Lay, Lay) {
        let tick = 1.0 / TICK_HZ as f32;
        let advance = |c: &mut Option<f32>, len: f32| {
            *c = c.map(|s| s + dt).filter(|&s| s < len);
        };
        advance(&mut self.melee, MELEE_S);
        advance(&mut self.throw, THROW_S);
        let sword = arm(f.gun().arm).kind == Kind::Sword;
        // A melee: its cooldown jumping up, timed from the tick it began.
        // One under way already is the same melee, its cooldown corrected
        // by the server: the cooldown outlasts the stroke.
        if f.melee_cool > self.melee_was && self.alive && self.melee.is_none() {
            let full = ticks(MELEE_COOL_MS) as u16;
            self.melee = Some(f32::from(full.saturating_sub(f.melee_cool)) * tick);
            self.slash = sword;
        }
        self.melee_was = f.melee_cool;
        // A sword's trigger is a swing too.
        if sword && right_rounds > 0 {
            self.melee = Some(0.0);
            self.slash = true;
        }
        if f.gren_cool > self.gren_was && self.alive && self.throw.is_none() {
            let full = ticks(GREN_COOL_MS) as u16;
            self.throw = Some(f32::from(full.saturating_sub(f.gren_cool)) * tick);
        }
        self.gren_was = f.gren_cool;
        let crouch = Lay {
            ang: CROUCH_ANG,
            wrist: Vec3::new(0.0, 0.0, CROUCH_CANT),
            off: CROUCH_OFF,
        } * self.crouch;
        let mut right = Lay::ZERO;
        if let Some(s) = self.melee {
            right = right + melee_lay(s, self.slash);
        }
        if let Some(s) = self.throw {
            right = right + throw_lay(s);
        }
        (crouch, right)
    }
}

/// The two hands' guns, as `update` poses them.
pub type Hands<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut ViewGun,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    Without<Flash>,
>;
/// The flash's light.
pub type FlashLight<'w, 's> =
    Query<'w, 's, (&'static mut PointLight, &'static mut Transform), (With<Flash>, Without<ViewGun>)>;

/// Once a frame.
#[allow(clippy::too_many_arguments)]
pub fn update(
    commands: &mut Commands,
    models: &Models,
    own: Own,
    dt: f32,
    m: &mut Motion,
    muzzles: &mut ViewMuzzles,
    hold: &mut Hands,
    flash: &mut FlashLight,
) {
    let dt = dt.clamp(0.0, MAX_DT);
    let f = own.f;
    let rounds = m.rounds.take(f);
    let respawned = f.alive && !m.alive && m.started;
    let body = m.body(&own, dt);
    let (both, right) = m.strokes(f, rounds[0], dt);
    m.alive = f.alive;
    let mut lit = (0.0, FLASH_AT);
    *muzzles = ViewMuzzles::default();
    for (e, mut v, mut t, vis) in hold.iter_mut() {
        let left = v.left;
        let k = Hand {
            body,
            hand: if left { both } else { both + right },
            rounds: rounds[usize::from(left)],
            respawned,
            t: m.t,
        };
        let at = hand(commands, models, &own, &k, dt, e, &mut v, &mut t, vis);
        if let Some(mz) = at {
            muzzles.0[usize::from(left)] = Some(mz);
            if v.flash > lit.0 {
                lit = (v.flash, mz);
            }
        }
    }
    if let Ok((mut l, mut lt)) = flash.single_mut() {
        let lm = if own.shown { lit.0 * FLASH_LM } else { 0.0 };
        if l.intensity != lm {
            l.intensity = lm;
        }
        if lit.0 > 0.0 && lt.translation != lit.1 {
            lt.translation = lit.1;
        }
    }
}

/// What one hand is handed this frame.
struct Hand {
    body: Lay,
    /// The strokes this hand takes, before its own (a right hand's).
    hand: Lay,
    rounds: u32,
    respawned: bool,
    t: f32,
}

/// One hand's gun, posed for the frame: where its muzzle is drawn, if it is.
#[allow(clippy::too_many_arguments)]
fn hand(
    commands: &mut Commands,
    models: &Models,
    own: &Own,
    k: &Hand,
    dt: f32,
    e: Entity,
    v: &mut ViewGun,
    t: &mut Transform,
    mut vis: Mut<Visibility>,
) -> Option<Vec3> {
    let f = own.f;
    let want = if v.left { f.left.arm } else { f.gun().arm };
    if k.respawned {
        v.drop = 1.0;
        v.reload_lay = Lay::ZERO;
    }
    let (drop, change) = drop_step(v.drop, v.drawn, want, dt, own.shown);
    v.drop = drop;
    if change {
        commands.entity(e).despawn_related::<Children>();
        if let Some((_, scene)) = spawn_posed(commands, models, want, view_basis(), e) {
            commands.entity(scene).observe(no_shadows);
        }
        v.drawn = want;
        v.kick = Spring::default();
        v.flash = 0.0;
    }
    let up = v.drawn == want;
    let a = arm(v.drawn);
    // A round from this hand: kick and flash (a sword's is a swing).
    v.since_round += dt;
    if k.rounds > 0 && up && a.kind != Kind::Sword {
        let braced = if f.body.crouched { CROUCH_KICK } else { 1.0 };
        for _ in 0..k.rounds.min(2) {
            v.kick.hit(kick_of(v.drawn, a) * braced, KICK_HZ, KICK_DAMP);
        }
        v.flash = 1.0;
        if v.since_round > BURST_REST_S {
            v.burst = 0;
        }
        v.burst += k.rounds;
        v.since_round = 0.0;
        v.cant = burst_cant(a.auto, v.burst, v.left, cant_of(f.shots));
    }
    v.kick.toward(0.0, KICK_HZ, KICK_DAMP, dt);
    v.flash = (v.flash - dt * FLASH_FALL_PER_S).max(0.0);
    let drawn = own.shown && v.drawn != NONE;
    vis.set_if_neq(if drawn {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });

    // The reload: its length is what it had when it began. A correction
    // that moves the running one up a tick doesn't start it again.
    let left_ticks = if v.left { f.left_reload } else { f.reload };
    if left_ticks > 0 && (v.reload_was == 0 || left_ticks >= v.reload_was.saturating_add(RELOAD_RELATCH_TICKS)) {
        v.reload_of = left_ticks;
    }
    v.reload_of = v.reload_of.max(left_ticks);
    v.reload_was = left_ticks;
    let reload = if up && left_ticks > 0 && v.reload_of > 0 {
        let p = 1.0 - f32::from(left_ticks) / f32::from(v.reload_of);
        let long = gun(v.drawn).is_none_or(|g| g.long);
        reload_lay(p, long, a.hot(), k.t)
    } else {
        // Done (where the pose is at rest already) or cut short: let go.
        v.reload_lay * (-RELOAD_CUT_PER_S * dt).exp()
    };
    v.reload_lay = reload;
    let kick = v.kick.x.clamp(-0.4, KICK_MAX);
    let kick = Lay::hand(
        Vec3::new(KICK_CLIMB * kick, 0.0, KICK_ROLL * v.cant * kick),
        Vec3::new(0.0, KICK_UP_M * kick, KICK_BACK_M * kick),
    );
    let charge = {
        let c = if v.left { f.left_charge } else { f.charge };
        let full = ticks(a.charge_ms).max(1) as f32;
        let q = (f32::from(c) / full).min(1.0);
        let buzz = (k.t * CHARGE_SHAKE_HZ * TAU).sin() * CHARGE_SHAKE * q * q;
        Lay::hand(Vec3::new(buzz, 0.0, buzz * 0.7), Vec3::new(0.0, 0.0, CHARGE_PULL_M * q))
    };
    let d = smooth(v.drop);
    let change = Lay::arm(DROP_ANG * d, DROP_OFF * d);
    let (at, rot) = place(k.hand + reload + kick + charge + change, k.body, v.left);
    t.translation = at * SHRINK;
    t.rotation = rot;
    let g = gun(v.drawn).filter(|_| drawn)?;
    let reach = g.height * (1.0 - g.grip) * g.scale;
    Some(at + rot * (view_basis() * Vec3::Y * reach))
}

/// A round's cant, -1..1, off the round's count: looks only.
fn cant_of(shots: u32) -> f32 {
    let mut x = shots.wrapping_mul(0x9e37_79b1);
    x ^= x >> 15;
    x = x.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 13;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn no_shadows(
    ev: On<SceneInstanceReady>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    mut commands: Commands,
) {
    for e in children.iter_descendants(ev.entity) {
        if meshes.contains(e) {
            commands.entity(e).insert(NotShadowCaster);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::arms::{AR, MAGNUM};

    /// The bob is off distance: the same metres walked at 30 and at 144
    /// frames a second land on the same phase, and standing still moves it
    /// not at all.
    #[test]
    fn the_bob_follows_the_feet_not_the_clock() {
        let mut f = Fighter::default();
        f.body.grounded = true;
        let run = |hz: f32| {
            let mut m = Motion::default();
            let dt = 1.0 / hz;
            let mut x = 0.0;
            for i in 0..=(hz as usize) {
                if i > 0 {
                    x += 5.0 * dt;
                }
                let own = Own {
                    f: &f,
                    at: Vec3::new(x, 0.0, 0.0),
                    yaw: 0.0,
                    pitch: 0.0,
                    riding: false,
                    shown: true,
                };
                m.body(&own, dt);
            }
            m.bob
        };
        let (slow, fast) = (run(30.0), run(144.0));
        assert!((slow - fast).abs() < 0.05, "{slow} vs {fast}");
        let walked = 5.0 * BOB_PER_M * TAU % TAU;
        assert!((slow - walked).abs() < 0.05, "{slow} vs {walked}");
    }

    /// The sway's spring settles to the same trail whatever the frame rate.
    #[test]
    fn the_sway_is_frame_rate_independent() {
        let trail = |hz: f32| {
            let mut s = Spring::default();
            let dt = 1.0 / hz;
            for _ in 0..(hz as usize / 4) {
                s.toward(0.05, SWAY_HZ, SWAY_DAMP, dt);
            }
            s.x
        };
        let (a, b) = (trail(32.0), trail(256.0));
        assert!((a - b).abs() < 1e-3, "{a} vs {b}");
    }

    /// A blow peaks where it says, and the spring comes back to rest.
    #[test]
    fn a_kick_peaks_and_settles() {
        let mut s = Spring::default();
        s.hit(1.0, KICK_HZ, KICK_DAMP);
        let mut peak: f32 = 0.0;
        for _ in 0..240 {
            s.toward(0.0, KICK_HZ, KICK_DAMP, 1.0 / 240.0);
            peak = peak.max(s.x);
        }
        assert!((peak - 1.0).abs() < 0.02, "{peak}");
        assert!(s.x.abs() < 0.01, "{}", s.x);
    }

    /// Two guns out: the round goes to the hand whose magazine it came out
    /// of, and a lone gun takes them all.
    #[test]
    fn a_round_goes_to_the_hand_that_fired_it() {
        let mut f = Fighter::default();
        f.guns[0] = Gun::full(MAGNUM);
        f.left = Gun::full(MAGNUM);
        let mut r = Rounds::default();
        assert_eq!(r.take(&f), [0, 0]);
        f.shots += 1;
        f.left.mag -= 1;
        f.left_refire = 8;
        assert_eq!(r.take(&f), [0, 1]);
        f.shots += 1;
        f.guns[0].mag -= 1;
        f.left_refire = 5;
        assert_eq!(r.take(&f), [1, 0]);
        f.shots += 2;
        f.guns[0].mag -= 1;
        f.left.mag -= 1;
        f.left_refire = 8;
        assert_eq!(r.take(&f), [1, 1]);
        // The left's reload ends and it fires in the same tick: its
        // magazine rose, and the round is still the left's.
        f.left.mag = 0;
        f.left_refire = 0;
        r.take(&f);
        f.left.mag = Gun::full(MAGNUM).mag - 1;
        f.left_refire = 8;
        f.shots += 1;
        assert_eq!(r.take(&f), [0, 1]);
        // A pickup into the left hand while the right fires: the right's.
        f.left = Gun::full(MAGNUM);
        f.left_refire = 30;
        f.shots += 1;
        assert_eq!(r.take(&f), [1, 0]);
        f.drop_left();
        f.guns[0] = Gun::full(AR);
        r.take(&f);
        f.shots += 1;
        assert_eq!(r.take(&f), [1, 0]);
    }

    /// A change of gun lowers the old one, changes the model at the bottom
    /// and is back up in the sim's swap time.
    #[test]
    fn a_change_is_down_then_up_in_the_swap() {
        let dt = 1.0 / 240.0;
        let (mut drop, mut drawn) = (0.0, AR);
        let mut changed_at = None;
        let mut up_at = None;
        for i in 1..=240 {
            let (d, change) = drop_step(drop, drawn, MAGNUM, dt, true);
            drop = d;
            if change {
                drawn = MAGNUM;
                changed_at = Some(i as f32 * dt);
            }
            if drawn == MAGNUM && drop == 0.0 && up_at.is_none() {
                up_at = Some(i as f32 * dt);
            }
        }
        let swap = SWAP_MS as f32 / 1000.0;
        let changed = changed_at.unwrap();
        assert!((changed - LOWER_S).abs() < 2.0 * dt, "{changed}");
        let up = up_at.unwrap();
        assert!((up - swap).abs() < 3.0 * dt, "{up} vs {swap}");
        // Out of sight, or from an empty hand, it changes at once.
        assert_eq!(drop_step(0.0, AR, MAGNUM, dt, false), (1.0, true));
        assert_eq!(drop_step(0.0, NONE, AR, dt, true), (1.0, true));
    }

    /// The left hand is the right mirrored: across the middle, turned the
    /// other way, the same pitch.
    #[test]
    fn the_left_hand_is_the_right_mirrored() {
        let hand = Lay {
            ang: Vec3::new(0.2, 0.3, -0.4),
            wrist: Vec3::new(-0.1, 0.25, 0.6),
            off: Vec3::new(0.05, -0.02, 0.03),
        };
        let (pr, qr) = place(hand, Lay::ZERO, false);
        let (pl, ql) = place(hand, Lay::ZERO, true);
        assert!((pl - Vec3::new(-pr.x, pr.y, pr.z)).length() < 1e-5, "{pl} vs {pr}");
        let (fr, fl) = (qr * Vec3::NEG_Z, ql * Vec3::NEG_Z);
        assert!((fl - Vec3::new(-fr.x, fr.y, fr.z)).length() < 1e-5);
        assert!((mirror_q(qr) * Vec3::Y - ql * Vec3::Y).length() < 1e-5);
    }

    /// Every stroke starts and ends at rest, and the pulses are smooth.
    #[test]
    fn the_strokes_start_and_end_at_rest() {
        let size = |l: Lay| l.ang.length() + l.wrist.length() + l.off.length();
        for s in [0.0, MELEE_S] {
            assert!(size(melee_lay(s, false)) < 1e-4);
            assert!(size(melee_lay(s, true)) < 1e-4);
        }
        assert!(size(throw_lay(0.0)) < 1e-4);
        assert!(size(throw_lay(THROW_S)) < 1e-4);
        for vent in [false, true] {
            for long in [false, true] {
                assert!(size(reload_lay(0.0, long, vent, 0.3)) < 1e-4);
                assert!(size(reload_lay(1.0, long, vent, 0.3)) < 1e-4);
                assert!(size(reload_lay(0.5, long, vent, 0.3)) > 0.1, "a reload moves");
            }
        }
        let n = 1000;
        let mut last = (0.0, 0.0);
        for i in 0..=n {
            let u = i as f32 / n as f32;
            let (c, s) = stroke(u, MELEE_WINDUP, MELEE_APEX, MELEE_HOLD);
            assert!((c - last.0).abs() < 0.05 && (s - last.1).abs() < 0.05, "a jump at {u}");
            last = (c, s);
        }
    }

    /// No stroke but the sword's slash puts the gun on the crosshair: along
    /// the whole gun, grip to muzzle, it stays at least 7° off the view's
    /// middle, for the longest gun and the shortest.
    #[test]
    fn the_gun_keeps_off_the_crosshair() {
        let off_axis = |p: Vec3| Vec2::new(p.x, p.y).length().atan2(-p.z);
        let poses = |u: f32| {
            let kick = Lay::hand(
                Vec3::new(KICK_CLIMB * KICK_MAX, 0.0, KICK_ROLL * KICK_MAX),
                Vec3::new(0.0, KICK_UP_M * KICK_MAX, KICK_BACK_M * KICK_MAX),
            );
            let crouch = Lay {
                ang: CROUCH_ANG,
                wrist: Vec3::new(0.0, 0.0, CROUCH_CANT),
                off: CROUCH_OFF,
            };
            [
                melee_lay(u * MELEE_S, false),
                throw_lay(u * THROW_S),
                reload_lay(u, true, false, u),
                reload_lay(u, false, false, u),
                reload_lay(u, true, true, u),
                Lay::arm(DROP_ANG * u, DROP_OFF * u),
                kick * u,
                crouch + melee_lay(u * MELEE_S, false),
            ]
        };
        for (arm, _) in &crate::look::guns::GUNS {
            let g = gun(*arm).unwrap();
            let reach = g.height * (1.0 - g.grip) * g.scale;
            for i in 0..=200 {
                let u = i as f32 / 200.0;
                for (n, lay) in poses(u).into_iter().enumerate() {
                    let (at, rot) = place(lay, Lay::ZERO, false);
                    let muzzle = at + rot * (view_basis() * Vec3::Y * reach);
                    for k in 0..=8 {
                        let p = at.lerp(muzzle, k as f32 / 8.0);
                        let a = off_axis(p);
                        assert!(a > 0.12, "pose {n} at {u}: {a} rad off the middle at {p}");
                    }
                }
            }
        }
    }

    /// An automatic's burst leans its kick to its hand's outside as it
    /// goes; a single shot's cant stays its own.
    #[test]
    fn a_burst_drifts_to_one_side() {
        assert_eq!(burst_cant(false, 9, false, -0.4), -0.4);
        assert!((burst_cant(true, 0, false, 0.6) - 0.3).abs() < 1e-6);
        assert!(burst_cant(true, DRIFT_SHOTS as u32, false, -1.0) > 0.99);
        assert!(burst_cant(true, DRIFT_SHOTS as u32, true, 1.0) < -0.99);
        assert!(burst_cant(true, 3, false, -1.0) > burst_cant(true, 1, false, -1.0));
    }
}

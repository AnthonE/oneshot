//! Our own hands in view: Triple T's wooden hands and forearms, cut from its model
//! with the fingers as its hold has them (`tools/models/hand.py`), and held
//! rigid to the gun, as the guns in view of every shooter are held. No rig:
//! arms dragged from where they were skinned to reach a gun tear and twist up
//! close, and these are the nearest thing on the screen.
//!
//! **The right hand** is on the gun's grip, the wrist dropping under the stock
//! ([`FIST_TILT`]). **The left** wraps a long gun's fore-end from below ([`cup`]),
//! or holds its own gun with two out; a pistol's is out of the frame. It leaves the fore-end
//! only to reach the magazine and to throw ([`off_hand`]). So every motion the
//! gun makes (the bob, the sway, a kick, a reload, a melee, a change) moves
//! the hands with it. Each forearm runs out of the frame from there.
//!
//! Under the gun's own field of view, like the gun, and [`SHRINK`] small and
//! that much nearer (`viewmodel.rs`); lit by the world, casting nothing, and
//! wearing our shield's shell as the gun does. Drawing only: it reads what
//! `viewmodel.rs` posed and writes the hands' places.

use bevy::prelude::*;
use core::f32::consts::FRAC_PI_2;

use super::guns::{gun, Models};
use super::viewmodel::{no_shadows, SHRINK};

/// Where the fist's hole sits on the gun's grip, in the gun's frame (x right,
/// y along the barrel, z up), metres as it looks: the axis a gun is held by
/// runs just over the fist (`guns::Gun`).
pub const GRIP_IN_GUN: Vec3 = Vec3::new(0.0, -0.01, -0.04);
/// The fist turned on the grip: its knuckles up by this much about the gun's
/// own x, so the wrist drops under the stock, radians.
pub const FIST_TILT: f32 = 0.42;
/// The right hand round a sword's hilt, in the sword's frame: which way its
/// knuckles lie from its wrist.
pub const SWORD_KNUCKLES: Vec3 = Vec3::new(-0.95, 0.0, 0.3);
/// Where the off hand's hole sits on a long gun's fore-end, metres as it looks:
/// this far up into its belly (`Gun::under`) and to its right, so the fingers go
/// round the far side and the heel of the palm and the thumb show on ours.
pub const FORE_INTO: f32 = 0.03;
pub const FORE_ACROSS: f32 = 0.02;
/// The furthest up a gun from the grip the off hand goes, metres as it looks:
/// further and its arm lies flat across the frame, its cut end in view.
pub const FORE_MAX_M: f32 = 0.18;
/// The off hand round a fore-end, in the gun's frame: which way its knuckles lie
/// from its wrist (across under the gun, so the forearm comes in from the lower
/// left), and which way along them its thumb's side is.
pub const CUP_KNUCKLES: Vec3 = Vec3::new(0.85, 0.3, 0.4);
pub const CUP_THUMB: Vec3 = Vec3::new(-0.4, 0.8, 0.45);
/// The off hand's reach to the magazine in a reload: a share of the way
/// from the grip to the fore-end, and how far under the axis.
pub const MAG_SHARE: f32 = 0.22;
pub const MAG_UNDER_M: f32 = 0.12;
/// A throw: the off hand from below the frame, drawn back, then flung forward
/// and up out of it (as it looks).
pub const THROW_FROM: Vec3 = Vec3::new(0.0, -0.5, -0.5);
pub const THROW_BACK: Vec3 = Vec3::new(-0.08, 0.05, 0.12);
pub const THROW_OUT: Vec3 = Vec3::new(0.1, 0.3, -0.22);

/// One hand's gun this frame, as it looks: where its grip is and how its
/// model's frame is turned (x right, y along the barrel, z up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grip {
    pub at: Vec3,
    pub rot: Quat,
    /// The same for the fist on it, which a cut's blade turns ahead of.
    pub fist: Quat,
    pub arm: u8,
}

/// What `viewmodel.rs` posed this frame for the hands to hold.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub struct Grips {
    pub shown: bool,
    pub team: u8,
    pub right: Option<Grip>,
    pub left: Option<Grip>,
    /// The right hand's reload under way, 0..1, and its gun's vent (a hot
    /// gun vents: the off hand stays put).
    pub reload: Option<f32>,
    pub vent: bool,
    /// A throw under way, 0..1.
    pub throw: Option<f32>,
}

/// A hand in view.
#[derive(Component)]
pub struct ViewHand {
    left: bool,
}

/// The hands, hidden until they have something to hold.
pub fn spawn(commands: &mut Commands, models: &Models, in_view: Entity) {
    for (left, scene) in [(false, &models.hand), (true, &models.hand_left)] {
        commands
            .spawn((
                ViewHand { left },
                SceneRoot(scene.clone()),
                Transform::from_scale(Vec3::splat(SHRINK)),
                Visibility::Hidden,
                ChildOf(in_view),
            ))
            .observe(no_shadows);
    }
}

/// Where the off hand is this frame, as it looks, when it is not on the
/// fore-end (`None`: on it), and whether it is throwing (held as a throw is,
/// not turned with the gun). A reload takes it to the magazine as the old one
/// comes out, holds it there through the swap and brings it back for the rack
/// (`viewmodel.rs`'s beats); a throw draws it back and flings it out of the
/// frame.
pub fn off_hand(g: &Grips, fore: Vec3, mag: Vec3) -> Option<(Vec3, bool)> {
    let smooth = |a: f32, b: f32, x: f32| {
        let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    if let Some(u) = g.throw {
        let back = smooth(0.0, 0.25, u) * (1.0 - smooth(0.25, 0.45, u));
        let out = smooth(0.25, 0.45, u) * (1.0 - smooth(0.6, 0.95, u));
        return Some((THROW_FROM + THROW_BACK * back + THROW_OUT * out, true));
    }
    let u = g.reload.filter(|_| !g.vent)?;
    let k = smooth(0.06, 0.26, u) * (1.0 - smooth(0.6, 0.74, u));
    Some((fore.lerp(mag, k), false))
}

/// The right fist on a grip, in the gun's frame. The hand's model has x
/// against the thumb, y along the knuckles, z out of its back: thumb up, the
/// back of the hand to the gun's right. A sword's hilt goes through the fist
/// from the little finger to the thumb, its blade out of the thumb's side, the
/// palm to the eye and the forearm in from the frame's lower right
/// ([`SWORD_KNUCKLES`]).
fn fist(arm: u8) -> Quat {
    if arm == sim::arms::SWORD {
        let y = SWORD_KNUCKLES.normalize();
        return Quat::from_mat3(&Mat3::from_cols(Vec3::NEG_Y, y, Vec3::NEG_Y.cross(y)));
    }
    Quat::from_rotation_x(FIST_TILT) * Quat::from_rotation_y(FRAC_PI_2)
}

/// The left fist on a grip: as the right's, the back of the hand to the gun's
/// left (its model has x along the thumb, where the right's is against it).
fn fist_left() -> Quat {
    Quat::from_rotation_x(FIST_TILT) * Quat::from_rotation_y(-FRAC_PI_2)
}

/// The left hand round a fore-end, in the gun's frame ([`CUP_KNUCKLES`]).
fn cup() -> Quat {
    let knuckles = CUP_KNUCKLES.normalize();
    let thumb = (CUP_THUMB - knuckles * CUP_THUMB.dot(knuckles)).normalize();
    Quat::from_mat3(&Mat3::from_cols(thumb, knuckles, thumb.cross(knuckles)))
}

/// The left hand as it throws, in the view's frame: fist forward, thumb up.
fn thrown() -> Quat {
    Quat::from_mat3(&Mat3::from_cols(Vec3::Y, Vec3::NEG_Z, Vec3::NEG_X))
}

fn right_hand(g: Grip) -> (Vec3, Quat) {
    (g.at + g.rot * GRIP_IN_GUN, g.fist * fist(g.arm))
}

fn left_hand(g: &Grips) -> Option<(Vec3, Quat)> {
    if let Some(l) = g.left {
        return Some((l.at + l.rot * GRIP_IN_GUN, l.rot * fist_left()));
    }
    let right = g.right?;
    let held = gun(right.arm)?;
    if !held.long {
        return match off_hand(g, right.at, right.at) {
            Some((at, true)) => Some((at, thrown())),
            _ => None,
        };
    }
    let reach = held.height * (1.0 - held.grip) * held.scale;
    let fore = right.at
        + right.rot
            * Vec3::new(
                FORE_ACROSS,
                (reach * held.fore).min(FORE_MAX_M),
                FORE_INTO - held.under * held.scale,
            );
    let mag = right.at + right.rot * Vec3::new(0.0, reach * MAG_SHARE, -MAG_UNDER_M);
    Some(match off_hand(g, fore, mag) {
        Some((at, true)) => (at, thrown()),
        Some((at, false)) => (at, right.rot * cup()),
        None => (fore, right.rot * cup()),
    })
}

/// Every frame, once `draw_view` has posed the guns: each hand onto its own.
pub fn pose(grips: Res<Grips>, mut hands: Query<(&ViewHand, &mut Transform, &mut Visibility)>) {
    for (h, mut t, mut vis) in &mut hands {
        let place = if !grips.shown {
            None
        } else if h.left {
            left_hand(&grips)
        } else {
            grips.right.map(right_hand)
        };
        let Some((at, rot)) = place else {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        };
        t.translation = at * SHRINK;
        t.rotation = rot;
        vis.set_if_neq(Visibility::Inherited);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each hand's turn is a true rotation, the thumb where it is meant to be.
    #[test]
    fn the_hands_are_turned_as_they_are_meant_to_be() {
        // The right's thumb, its model's -x, stands up the grip.
        let t = Quat::from_rotation_y(FRAC_PI_2) * Vec3::NEG_X;
        assert!((t - Vec3::Z).length() < 1e-5, "{t}");
        // The left's, its model's +x, too; the same tilt either way.
        let t = Quat::from_rotation_y(-FRAC_PI_2) * Vec3::X;
        assert!((t - Vec3::Z).length() < 1e-5, "{t}");
        // Round a fore-end the forearm (the knuckles' opposite) runs away
        // down, left and back toward us, the back of the hand down; the turn
        // has no mirror in it.
        let q = cup();
        assert!((q.length() - 1.0).abs() < 1e-5);
        let forearm = q * Vec3::NEG_Y;
        assert!(forearm.x < 0.0 && forearm.y < 0.0 && forearm.z < 0.0, "{forearm}");
        assert!((q * Vec3::Z).z < 0.0, "{}", q * Vec3::Z);
    }

    /// A reload takes the off hand to the magazine and brings it back; a
    /// throw takes it off the gun altogether.
    #[test]
    fn the_off_hand_leaves_the_fore_end_only_to_reload_and_throw() {
        let fore = Vec3::new(0.0, 0.0, -1.0);
        let mag = Vec3::new(0.0, -0.1, -0.5);
        let g = Grips::default();
        assert!(off_hand(&g, fore, mag).is_none());
        let at = |u: f32| {
            off_hand(
                &Grips {
                    reload: Some(u),
                    ..default()
                },
                fore,
                mag,
            )
            .unwrap()
            .0
        };
        assert!((at(0.0) - fore).length() < 1e-4);
        assert!((at(0.5) - mag).length() < 0.05, "{}", at(0.5));
        assert!((at(1.0) - fore).length() < 1e-4);
        assert!((at(0.9) - fore).length() < 1e-4);
        let vent = Grips {
            reload: Some(0.5),
            vent: true,
            ..default()
        };
        assert!(off_hand(&vent, fore, mag).is_none());
        let t = Grips {
            throw: Some(0.5),
            ..default()
        };
        assert!(off_hand(&t, fore, mag).is_some());
    }
}

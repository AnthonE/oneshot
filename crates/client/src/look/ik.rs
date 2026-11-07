//! Two-bone reach: an arm onto a gun's fore-end, a leg onto the ground
//! under it. After Kong Royale's `client/src/actors/kong-rig.ts` (MIT,
//! NOTICE: planted feet by two-bone leg IK and a hip drop, hands on every
//! gun by IK) and our own `tools/models/triple-t.py` `under_the_gun`, which
//! solves the same arm offline for one gun.
//!
//! The bend stays in the plane the clip put it in (the clip's elbow or knee
//! is the pole), each bone keeps its twist about itself, and a point out of
//! reach is reached for in a straight line. Drawing only: run on the clip's
//! pose after the animation and before the transforms propagate.

use bevy::prelude::*;

/// A limb as the clip left it: the frame its first bone hangs from (in the
/// model's root frame), then each of its three bones' own transforms.
#[derive(Clone, Copy, Debug)]
pub struct Limb {
    pub parent: Transform,
    pub upper: Transform,
    pub lower: Transform,
    pub end: Transform,
}

/// A limb's three joints and its end's turn, in the root's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joints {
    pub root: Vec3,
    pub mid: Vec3,
    pub end: Vec3,
    pub end_turn: Quat,
}

impl Limb {
    /// Where its joints are, in the root's frame.
    pub fn joints(&self) -> Joints {
        let up = self.parent.mul_transform(self.upper);
        let lo = up.mul_transform(self.lower);
        let end = lo.mul_transform(self.end);
        Joints {
            root: up.translation,
            mid: lo.translation,
            end: end.translation,
            end_turn: end.rotation,
        }
    }

    /// Its upper and lower bones' lengths together, in the root's frame.
    pub fn span(&self) -> f32 {
        let j = self.joints();
        (j.mid - j.root).length() + (j.end - j.mid).length()
    }

    /// The three bones' new local turns that put its end at `target` (the
    /// root's frame), the end keeping the turn it had there unless `end_turn`
    /// names one. None for a limb laid straight by its clip, which has no
    /// plane to bend in: it is left as it is.
    pub fn reach(&self, target: Vec3, end_turn: Option<Quat>) -> Option<[Quat; 3]> {
        let j = self.joints();
        let (up_w, lo_w) = {
            let up = self.parent.rotation * self.upper.rotation;
            (up, up * self.lower.rotation)
        };
        let (d_up, d_lo) = two_bone(j.root, j.mid, j.end, target)?;
        let up2 = d_up * up_w;
        let lo2 = d_lo * lo_w;
        let end2 = end_turn.unwrap_or(j.end_turn);
        Some([
            (self.parent.rotation.inverse() * up2).normalize(),
            (up2.inverse() * lo2).normalize(),
            (lo2.inverse() * end2).normalize(),
        ])
    }
}

/// How far along a path from 0 to 1 a limb reaches: the furthest point on it
/// within [`REACH_SHARE`] of the limb's span from its root, found by halving
/// (the path is taken to run away from the root). The start if even that is
/// out of reach: the clip's own, as near as it gets.
pub fn furthest_in_reach(root: Vec3, span: f32, path: impl Fn(f32) -> Vec3) -> Vec3 {
    let reach = REACH_SHARE * span;
    let ok = |k: f32| (path(k) - root).length() <= reach;
    if ok(1.0) {
        return path(1.0);
    }
    if !ok(0.0) {
        return path(0.0);
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..12 {
        let mid = 0.5 * (lo + hi);
        if ok(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    path(lo)
}

/// The most of a limb's length a reach may take: an arm out dead straight
/// reads as broken.
pub const REACH_SHARE: f32 = 0.98;

/// Lengths kept off a joint's fully open and fully shut, as a share of the
/// limb: a knee or an elbow never snaps straight or folds through itself.
const OPEN_MARGIN: f32 = 1e-3;

/// The turns, each in the frame the points are given in, that take the
/// upper bone (`root`→`mid`) and the lower (`mid`→`end`) onto a bend that
/// puts `end` at `target`. The new elbow stays on the side of the line from
/// root to target that the old one was. None when the old limb is straight
/// (no plane) or a bone has no length.
pub fn two_bone(root: Vec3, mid: Vec3, end: Vec3, target: Vec3) -> Option<(Quat, Quat)> {
    let (a, b) = ((mid - root).length(), (end - mid).length());
    if a < 1e-6 || b < 1e-6 {
        return None;
    }
    let to = target - root;
    let along = to.try_normalize()?;
    let span = a + b;
    let d = to
        .length()
        .clamp((a - b).abs() + OPEN_MARGIN * span, span - OPEN_MARGIN * span);
    let pole = mid - root;
    let out = (pole - along * pole.dot(along)).try_normalize()?;
    let cos = ((a * a + d * d - b * b) / (2.0 * a * d)).clamp(-1.0, 1.0);
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    let mid2 = root + (along * cos + out * sin) * a;
    let end2 = root + along * d;
    let (du, df) = ((mid - root) / a, (end - mid) / b);
    let n = du.cross(df).try_normalize()?;
    let (du2, df2) = ((mid2 - root) / a, (end2 - mid2) / b);
    let n2 = du2.cross(df2).try_normalize()?;
    Some((frame_to(du, n, du2, n2), frame_to(df, n, df2, n2)))
}

/// The turn taking a direction and a normal to it onto another such pair.
pub fn frame_to(d: Vec3, n: Vec3, d2: Vec3, n2: Vec3) -> Quat {
    let a = Mat3::from_cols(d, n, d.cross(n));
    let b = Mat3::from_cols(d2, n2, d2.cross(n2));
    (Quat::from_mat3(&b) * Quat::from_mat3(&a).inverse()).normalize()
}

/// A leg's say in where the hips go and where its foot goes, on uneven
/// ground: how far the ground under its foot stands above the ground the
/// body stands on, metres, clamped to [`FOOT_REACH_M`].
pub fn foot_rise(ground_under_foot: f32, ground_under_body: f32) -> f32 {
    (ground_under_foot - ground_under_body).clamp(-FOOT_REACH_M, FOOT_REACH_M)
}

/// How far a foot is moved up or down to meet the ground, most, metres:
/// a step's worth. Past it the foot stays where the clip put it.
pub const FOOT_REACH_M: f32 = 0.35;

/// How far the hips come down so the lower of two feet reaches its ground:
/// the lower rise below zero, or nothing.
pub fn hip_drop(rises: [f32; 2]) -> f32 {
    (-rises[0].min(rises[1])).max(0.0)
}

/// Both legs as the clip left them, from the frame the hips hang in: the
/// hips' own transform, then each leg's thigh, shin and foot.
#[derive(Clone, Copy, Debug)]
pub struct Legs {
    pub hips_parent: Transform,
    pub hips: Transform,
    pub left: [Transform; 3],
    pub right: [Transform; 3],
}

/// What [`Legs::plant`] decided: the hips' new translation (their own
/// parent's frame) and each leg's new turns, if it bends.
#[derive(Clone, Copy, Debug)]
pub struct Planted {
    pub hips: Vec3,
    pub left: Option<[Quat; 3]>,
    pub right: Option<[Quat; 3]>,
}

impl Legs {
    fn limb(&self, hips: Transform, leg: [Transform; 3]) -> Limb {
        Limb {
            parent: self.hips_parent.mul_transform(hips),
            upper: leg[0],
            lower: leg[1],
            end: leg[2],
        }
    }

    /// Each ankle, left then right, in the root's frame.
    pub fn ankles(&self) -> [Vec3; 2] {
        [
            self.limb(self.hips, self.left).joints().end,
            self.limb(self.hips, self.right).joints().end,
        ]
    }

    /// Each foot moved `rises` metres up the root's +Y (left, right: how far
    /// the ground under it stands over the body's, [`foot_rise`]), the hips
    /// brought down by the lower one so it reaches, all of it by `w` (0..1).
    pub fn plant(&self, rises: [f32; 2], w: f32) -> Planted {
        let drop = hip_drop(rises) * w;
        let mut hips = self.hips;
        let p = self.hips_parent;
        hips.translation += p.rotation.inverse() * (Vec3::NEG_Y * drop) / p.scale;
        let ankles = self.ankles();
        let left = self
            .limb(hips, self.left)
            .reach(ankles[0] + Vec3::Y * rises[0] * w, None);
        let right = self
            .limb(hips, self.right)
            .reach(ankles[1] + Vec3::Y * rises[1] * w, None);
        Planted {
            hips: hips.translation,
            left,
            right,
        }
    }
}

/// A body's bones the reach moves (`bodies::pose_limbs`), found by name at
/// its scene's spawn, and what it wrote last frame.
#[derive(Component)]
pub struct LimbBones {
    /// LeftArm, LeftForeArm, LeftHand.
    pub arm: [Entity; 3],
    pub hips: Entity,
    /// UpLeg, Leg, Foot: left, then right.
    pub legs: [[Entity; 3]; 2],
    /// How much the feet follow the ground, eased in and out (0..1).
    pub feet_w: f32,
    /// Each bone written last frame, as the clip had it and as written: a bone
    /// no clip keys this frame still holds what was written, and is given back
    /// the clip's before it is reached again (or every frame would add on).
    pub wrote: Vec<(Entity, Transform, Transform)>,
}

impl LimbBones {
    /// Undo last frame's writes the animation did not overwrite.
    pub fn take_back(&mut self, bones: &mut Query<&mut Transform>) {
        for (e, clip, wrote) in self.wrote.drain(..) {
            if let Ok(mut t) = bones.get_mut(e) {
                if *t == wrote {
                    *t = clip;
                }
            }
        }
    }

    /// Write `t` over `e`, remembering what was there.
    pub fn write(&mut self, e: Entity, t: Transform, bones: &mut Query<&mut Transform>) {
        if let Ok(mut at) = bones.get_mut(e) {
            if *at != t {
                self.wrote.push((e, *at, t));
                *at = t;
            }
        }
    }
}

/// The bones [`LimbBones`] needs, under `model`, by name. None if one is
/// missing: no reach, the clip's pose as it is.
pub fn bind(model: Entity, names: &Query<&Name>, children: &Query<&Children>) -> Option<LimbBones> {
    let find = |name: &str| {
        children
            .iter_descendants(model)
            .take(512)
            .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == name))
    };
    let three = |a: &str, b: &str, c: &str| Some([find(a)?, find(b)?, find(c)?]);
    Some(LimbBones {
        arm: three("LeftArm", "LeftForeArm", "LeftHand")?,
        hips: find(super::anim::HIPS_BONE)?,
        legs: [
            three("LeftUpLeg", "LeftLeg", "LeftFoot")?,
            three("RightUpLeg", "RightLeg", "RightFoot")?,
        ],
        feet_w: 0.0,
        wrote: Vec::new(),
    })
}

/// Every bone's transform composed from `e` up to (not including) `root`:
/// `e`'s frame in `root`'s. None if `e` isn't under `root`.
pub fn frame_in(
    e: Entity,
    root: Entity,
    parents: &Query<&ChildOf>,
    transforms: &Query<&mut Transform>,
) -> Option<Transform> {
    let mut out = Transform::IDENTITY;
    let mut at = e;
    for _ in 0..64 {
        if at == root {
            return Some(out);
        }
        let t = transforms.get(at).ok()?;
        out = t.mul_transform(out);
        at = parents.get(at).ok()?.0;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limb() -> Limb {
        // A shoulder at the origin, an upper arm 0.3 down and a little
        // forward, a forearm 0.28 on, the hand at its end.
        Limb {
            parent: Transform::from_xyz(0.0, 1.4, 0.0),
            upper: Transform::from_rotation(Quat::from_rotation_x(0.2)),
            lower: Transform::from_xyz(0.0, -0.3, 0.0).with_rotation(Quat::from_rotation_x(0.9)),
            end: Transform::from_xyz(0.0, -0.28, 0.0).with_rotation(Quat::from_rotation_z(0.3)),
        }
    }

    fn apply(l: &Limb, q: [Quat; 3]) -> Limb {
        let mut l = *l;
        l.upper.rotation = q[0];
        l.lower.rotation = q[1];
        l.end.rotation = q[2];
        l
    }

    #[test]
    fn a_hand_in_reach_lands_on_its_point_and_keeps_its_turn() {
        let l = limb();
        let j = l.joints();
        for target in [
            j.end + Vec3::new(0.05, 0.1, 0.08),
            j.end + Vec3::new(-0.1, -0.02, 0.0),
            j.root + Vec3::new(0.2, -0.25, 0.25),
        ] {
            let q = l.reach(target, None).expect("bent");
            let k = apply(&l, q).joints();
            assert!((k.end - target).length() < 1e-4, "{} vs {target}", k.end);
            assert!(k.end_turn.angle_between(j.end_turn) < 1e-4);
            // The bones keep their lengths: the parent's frame is untouched.
            assert!(((k.mid - k.root).length() - (j.mid - j.root).length()).abs() < 1e-5);
            assert!(((k.end - k.mid).length() - (j.end - j.mid).length()).abs() < 1e-5);
        }
    }

    #[test]
    fn the_elbow_stays_on_its_side() {
        let l = limb();
        let j = l.joints();
        let target = j.end + Vec3::new(0.0, 0.12, 0.05);
        let k = apply(&l, l.reach(target, None).unwrap()).joints();
        let side = |m: Vec3, e: Vec3, r: Vec3| {
            let along = (e - r).normalize();
            let v = m - r;
            v - along * v.dot(along)
        };
        assert!(side(j.mid, j.end, j.root).dot(side(k.mid, k.end, k.root)) > 0.0);
    }

    #[test]
    fn out_of_reach_is_reached_for_in_a_line() {
        let l = limb();
        let j = l.joints();
        let far = j.root + Vec3::new(0.0, 0.0, 2.0);
        let k = apply(&l, l.reach(far, None).unwrap()).joints();
        let dir = (k.end - k.root).normalize();
        assert!(dir.dot(Vec3::Z) > 0.999, "{dir}");
        assert!((k.end - k.root).length() < 0.58);
    }

    #[test]
    fn a_target_where_it_already_is_moves_nothing() {
        let l = limb();
        let j = l.joints();
        let q = l.reach(j.end, None).unwrap();
        assert!(q[0].angle_between(l.upper.rotation) < 1e-3);
        assert!(q[1].angle_between(l.lower.rotation) < 1e-3);
    }

    #[test]
    fn a_straight_limb_is_left_alone() {
        let mut l = limb();
        l.upper.rotation = Quat::IDENTITY;
        l.lower.rotation = Quat::IDENTITY;
        assert!(l.reach(Vec3::new(0.0, 1.0, 0.2), None).is_none());
    }

    fn legs() -> Legs {
        let thigh = Transform::from_rotation(Quat::from_rotation_x(-0.1));
        let shin = Transform::from_xyz(0.0, -0.42, 0.0).with_rotation(Quat::from_rotation_x(0.25));
        let foot = Transform::from_xyz(0.0, -0.4, 0.0);
        Legs {
            hips_parent: Transform::IDENTITY,
            hips: Transform::from_xyz(0.0, 0.9, 0.0),
            left: [thigh.with_translation(Vec3::new(0.1, 0.0, 0.0)), shin, foot],
            right: [thigh.with_translation(Vec3::new(-0.1, 0.0, 0.0)), shin, foot],
        }
    }

    fn after(l: &Legs, p: &Planted) -> Legs {
        let mut l = *l;
        l.hips.translation = p.hips;
        for (leg, q) in [(&mut l.left, p.left), (&mut l.right, p.right)] {
            if let Some(q) = q {
                for (b, q) in leg.iter_mut().zip(q) {
                    b.rotation = q;
                }
            }
        }
        l
    }

    #[test]
    fn feet_meet_a_slope_and_the_hips_come_down_for_the_lower() {
        let l = legs();
        let a = l.ankles();
        let rises = [0.12, -0.08];
        let p = l.plant(rises, 1.0);
        let k = after(&l, &p).ankles();
        for i in 0..2 {
            let want = a[i] + Vec3::Y * rises[i];
            assert!((k[i] - want).length() < 1e-3, "{i}: {} vs {want}", k[i]);
        }
        assert!((p.hips.y - (0.9 - 0.08)).abs() < 1e-5);
    }

    #[test]
    fn flat_ground_changes_nothing() {
        let l = legs();
        let p = l.plant([0.0, 0.0], 1.0);
        let k = after(&l, &p).ankles();
        let a = l.ankles();
        assert!((k[0] - a[0]).length() < 1e-4 && (k[1] - a[1]).length() < 1e-4);
        assert_eq!(p.hips, l.hips.translation);
    }

    #[test]
    fn a_path_is_followed_as_far_as_the_limb_reaches() {
        let root = Vec3::ZERO;
        let path = |k: f32| Vec3::new(0.0, -0.2, 0.3 + 0.5 * k);
        let p = furthest_in_reach(root, 0.6, path);
        assert!(((p - root).length() - REACH_SHARE * 0.6).abs() < 1e-3, "{p}");
        assert_eq!(furthest_in_reach(root, 2.0, path), path(1.0));
        assert_eq!(furthest_in_reach(root, 0.1, path), path(0.0));
    }

    #[test]
    fn the_hips_drop_by_the_lower_foot_only() {
        assert_eq!(hip_drop([0.1, 0.2]), 0.0);
        assert!((hip_drop([0.1, -0.15]) - 0.15).abs() < 1e-6);
        assert_eq!(foot_rise(5.0, 0.0), FOOT_REACH_M);
    }
}

//! Our own arms in view: Triple T's own, from its own file. The model
//! comes in two meshes, `char1_arms` (shoulders to fingertips, every finger
//! boned) and `char1_body` (the trunk, the legs and the head with its face);
//! here only the arms are drawn, so no face ever reaches the camera.
//!
//! **The fingers** are the hold clip's (`anim::Hold`, its first frame held,
//! whole: [`ViewClips`]): `Rifle_Hold_Loop` round a long gun's grip and its
//! fore-end, `Pistol_Idle_Loop` round a pistol's. One pistol is held in
//! the right hand alone, the left arm folded away out of the frame; with a
//! gun in each hand the left hand's fingers are the right's, mirrored.
//!
//! **The arms** are solved every frame onto the guns as `viewmodel.rs`
//! poses them ([`Grips`]): each hand is the clip's hand moved rigidly so
//! its grip lands on the gun's, turned as the gun is turned from the way
//! the clip holds it (straight ahead, upright), and the shoulder and elbow
//! are a two-bone reach to its wrist, bent in the plane the clip bends them
//! in, the elbow led toward [`POLE_R`]. So every motion the gun makes (the
//! bob, the sway, a kick, a reload, a melee, a change) moves the hands with
//! it, and the off hand leaves the fore-end only to reach the magazine and
//! to throw ([`off_hand`]).
//!
//! The rig is placed so the right shoulder sits at [`SHOULDER_R`]; the left
//! shoulder is moved to where the off hand can reach ([`SHOULDER_L`]): a
//! viewmodel's arms are a picture, not a body, and Triple T's are short.
//! Like the gun it is drawn [`SHRINK`] small and that much nearer, by the
//! world's camera, lit and shadowed by the world, casting nothing. Drawing
//! only: it reads what `viewmodel.rs` posed and writes the bones.
//!
//! Sprayed as the bodies are (`paint.rs`, `arms_mask`): a black glove over
//! each hand and a band in the side's colour round each forearm, the wood
//! bare between; and it wears our own shield's shell as the gun does.

use std::collections::BTreeMap;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::gltf::Gltf;
use bevy::light::NotShadowCaster;
use bevy::math::Affine3A;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

use super::anim::Hold;
use super::guns::{gun, view_basis, Models};
use super::paint::{BodyMaterial, Paint};
use super::viewmodel::{mirror_q, ViewKit, ViewShell, SHRINK};

/// How big the arms are drawn against the guns: Triple T's hands are a
/// big man's, its arms a short one's.
pub const ARM_SCALE: f32 = 0.85;
/// The model turned to face where the camera does (it faces +Z in its
/// file, the camera −Z).
fn facing() -> Quat {
    Quat::from_rotation_y(core::f32::consts::PI)
}
/// Where each shoulder's joint sits, as it looks (view space, metres before
/// [`SHRINK`]): below the frame, the left brought forward to reach a long
/// gun's fore-end; with two guns, or one pistol, where the right's mirror is.
pub const SHOULDER_R: Vec3 = Vec3::new(0.3, -0.52, -0.06);
pub const SHOULDER_L: Vec3 = Vec3::new(0.0, -0.6, -0.56);
/// Which way each elbow is led, as it looks: down and out.
pub const POLE_R: Vec3 = Vec3::new(0.7, -1.0, 0.1);
pub const POLE_L: Vec3 = Vec3::new(-0.8, -1.0, 0.0);
/// Where the gun's grip sits in the right fist, in the gun's frame (x
/// right, y along the barrel, z up), metres as it looks: the axis it is
/// held by runs just over the fist (`guns::Gun`).
pub const GRIP_IN_GUN: Vec3 = Vec3::new(0.0, -0.01, -0.04);
/// The fist turned on the grip from the clip's (which points the hand down
/// the barrel, its wrist in the stock): its fingers up by this much about
/// the gun's own x, so the wrist drops under the stock, radians.
pub const FIST_PITCH: f32 = 0.4;
/// The off hand's turn on the fore-end, the same way.
pub const CUP_PITCH: f32 = 0.0;
/// How far under a long gun's belly (`Gun::under`) the off hand's grip
/// sits, metres as it looks.
pub const FORE_CUP: f32 = 0.025;
/// The off hand's reach to the magazine in a reload: a share of the way
/// from the grip to the fore-end, and how far under the axis.
pub const MAG_SHARE: f32 = 0.22;
pub const MAG_UNDER_M: f32 = 0.12;
/// A throw: the off hand drawn back past the shoulder, then flung forward
/// and up out of the frame (as it looks, from its own shoulder).
pub const THROW_BACK: Vec3 = Vec3::new(-0.08, 0.05, 0.12);
pub const THROW_OUT: Vec3 = Vec3::new(0.1, 0.3, -0.22);
/// The glove's black, the band's glow (a share of a white surface's
/// brightness in full sun) and the wood's lift (`bodies.rs`'s).
const GLOVE: [f32; 3] = [0.012, 0.013, 0.015];
const BAND_GLOW: f32 = 0.03;
const LIFT: f32 = 2.6;

/// One hand's gun this frame, as it looks: where its grip is and how its
/// model's frame is turned (x right, y along the barrel, z up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grip {
    pub at: Vec3,
    pub rot: Quat,
    pub arm: u8,
}

/// What `viewmodel.rs` posed this frame for the arms to hold.
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

/// The arms' own graph: each hold's clip whole, the hips with it, so the
/// arms are where the clip puts them against the hips (the bodies' holds
/// are masked off the legs, which here play nothing).
#[derive(Resource)]
pub struct ViewClips {
    graph: Handle<AnimationGraph>,
    holds: [Option<AnimationNodeIndex>; 2],
}

impl ViewClips {
    fn node(&self, h: Hold) -> Option<AnimationNodeIndex> {
        self.holds[usize::from(h == Hold::Pistol)]
    }
}

/// Builds the arms' graph once the model's clips are in.
pub fn build(
    mut commands: Commands,
    have: Option<Res<ViewClips>>,
    models: Option<Res<Models>>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let (None, Some(models)) = (have, models) else {
        return;
    };
    let Some(gltf) = gltfs.get(&models.body) else {
        return;
    };
    let mut graph = AnimationGraph::new();
    let root = graph.root;
    let holds = [Hold::Long, Hold::Pistol].map(|h| {
        let n = gltf
            .named_animations
            .get(h.name())
            .map(|c| graph.add_clip(c.clone(), 1.0, root));
        if n.is_none() {
            error!("hands: no {} in models/triple-t.glb: the arms hold nothing", h.name());
        }
        n
    });
    commands.insert_resource(ViewClips {
        graph: graphs.add(graph),
        holds,
    });
}

/// One arm's bones: from the scene's top node down to its shoulder, then
/// the arm, the forearm, the hand and the hand's fingers (the same order
/// on each side, for the mirror).
struct Side {
    chain: Vec<Entity>,
    arm: Entity,
    fore: Entity,
    hand: Entity,
    fingers: Vec<Entity>,
    /// The grip's fingers: the middle finger's three joints, the index's
    /// and the pinky's first.
    grip: [Entity; 5],
}

/// The arms in view: the rig's root (under the eye).
#[derive(Component, Default)]
pub struct ViewArms {
    scene: Option<Entity>,
    player: Option<Entity>,
    graphed: bool,
    sides: Option<[Side; 2]>,
    hold: Option<Hold>,
    mat: Option<Handle<BodyMaterial>>,
    team: Option<u8>,
}

const FINGERS: [&str; 15] = [
    "Thumb1", "Thumb2", "Thumb3", "Index1", "Index2", "Index3", "Middle1", "Middle2", "Middle3", "Ring1", "Ring2",
    "Ring3", "Pinky1", "Pinky2", "Pinky3",
];

/// The rig, under the eye, hidden until it has something to hold.
pub fn spawn(commands: &mut Commands, models: &Models, eye: Entity) {
    let root = commands
        .spawn((
            ViewArms::default(),
            Transform::from_scale(Vec3::splat(ARM_SCALE * SHRINK)),
            Visibility::Hidden,
            ChildOf(eye),
        ))
        .id();
    commands
        .spawn((SceneRoot(models.body_scene.clone()), Transform::IDENTITY, ChildOf(root)))
        .observe(dress);
}

/// The scene is in: find the bones, hide the trunk and the head, paint the
/// arms and give them our shield's shell.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dress(
    ev: On<SceneInstanceReady>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    names: Query<&Name>,
    players: Query<(), With<AnimationPlayer>>,
    meshes: Query<(
        &MeshMaterial3d<StandardMaterial>,
        &Mesh3d,
        Option<&SkinnedMesh>,
        &Transform,
    )>,
    mut arms: Query<&mut ViewArms>,
    (mats, mut body_mats, mut images, mesh_assets): (
        Res<Assets<StandardMaterial>>,
        ResMut<Assets<BodyMaterial>>,
        ResMut<Assets<Image>>,
        Res<Assets<Mesh>>,
    ),
    kit: Option<Res<ViewKit>>,
    mut commands: Commands,
) {
    let scene = ev.entity;
    let Ok(&ChildOf(root)) = parents.get(scene) else {
        return;
    };
    let Ok(mut rig) = arms.get_mut(root) else {
        return;
    };
    let mut by_name = BTreeMap::new();
    for e in children.iter_descendants(scene) {
        if let Ok(n) = names.get(e) {
            by_name.insert(n.as_str().to_owned(), e);
        }
        if players.contains(e) {
            rig.player = Some(e);
        }
    }
    let chain_to = |e: Entity| {
        let mut v = vec![e];
        let mut at = e;
        while let Ok(&ChildOf(p)) = parents.get(at) {
            if p == scene {
                break;
            }
            v.push(p);
            at = p;
        }
        v.reverse();
        v
    };
    let side = |s: &str| -> Option<Side> {
        let b = |n: &str| by_name.get(&format!("{s}{n}")).copied();
        let shoulder = b("Shoulder")?;
        Some(Side {
            chain: chain_to(shoulder),
            arm: b("Arm")?,
            fore: b("ForeArm")?,
            hand: b("Hand")?,
            fingers: FINGERS
                .iter()
                .map(|f| b(&format!("Hand{f}")))
                .collect::<Option<Vec<_>>>()?,
            grip: [
                b("HandMiddle1")?,
                b("HandMiddle2")?,
                b("HandMiddle3")?,
                b("HandIndex1")?,
                b("HandPinky1")?,
            ],
        })
    };
    match (side("Right"), side("Left")) {
        (Some(r), Some(l)) => rig.sides = Some([r, l]),
        _ => error!("hands: an arm's bone is missing or renamed in models/triple-t.glb: no arms in view"),
    }
    if let Some(&body) = by_name.get("char1_body") {
        commands.entity(body).insert(Visibility::Hidden);
    }
    let Some(&arms_node) = by_name.get("char1_arms") else {
        error!("hands: no char1_arms in models/triple-t.glb: no arms in view");
        return;
    };
    let arm_meshes: Vec<Entity> = std::iter::once(arms_node)
        .chain(children.iter_descendants(arms_node))
        .filter(|e| meshes.contains(*e))
        .collect();
    let cpu: Vec<&Mesh> = arm_meshes
        .iter()
        .filter_map(|e| meshes.get(*e).ok())
        .filter_map(|(_, m, ..)| mesh_assets.get(&m.0))
        .collect();
    let mask = match super::paint::arms_mask(&cpu) {
        Some(data) => images.add(super::textures::baked_image(data, super::paint::SIDE, false, false)),
        None => {
            error!("hands: models/triple-t.glb's arms have no positions, normals or UVs on the CPU: unpainted");
            images.add(super::textures::baked_image(
                vec![0; super::paint::SIDE * super::paint::SIDE * 4],
                super::paint::SIDE,
                false,
                false,
            ))
        }
    };
    for e in arm_meshes {
        let Ok((m, mesh, skin, at)) = meshes.get(e) else {
            continue;
        };
        let Some(mut base) = mats.get(&m.0).cloned() else {
            continue;
        };
        base.base_color = LinearRgba::rgb(LIFT, LIFT, LIFT).into();
        base.emissive = LinearRgba::BLACK;
        base.emissive_exposure_weight = 1.0;
        let h = body_mats.add(BodyMaterial {
            base,
            extension: Paint {
                params: params(0),
                mask: mask.clone(),
            },
        });
        rig.mat = Some(h.clone());
        rig.team = Some(0);
        commands.entity(e).remove::<MeshMaterial3d<StandardMaterial>>().insert((
            MeshMaterial3d(h),
            NoFrustumCulling,
            NotShadowCaster,
        ));
        if let (Some(kit), Ok(&ChildOf(parent))) = (&kit, parents.get(e)) {
            let mut s = commands.spawn((
                mesh.clone(),
                MeshMaterial3d(kit.shell.clone()),
                *at,
                Visibility::Hidden,
                ViewShell,
                NotShadowCaster,
                NoFrustumCulling,
                ChildOf(parent),
            ));
            if let Some(skin) = skin {
                s.insert(skin.clone());
            }
        }
    }
    rig.scene = Some(scene);
}

/// The paint for `team`'s side: its colour on the band, the glove black.
fn params(team: u8) -> super::paint::PaintParams {
    let sun = bevy::light::light_consts::lux::DIRECT_SUNLIGHT / core::f32::consts::PI;
    super::paint::params(
        super::bodies::team_color(team).to_linear(),
        proto::EMBLEM_NONE,
        Some(LinearRgba::rgb(GLOVE[0], GLOVE[1], GLOVE[2])),
        BAND_GLOW * sun,
        0.0,
        LIFT,
    )
}

/// `v` reflected across the view's middle.
fn flip(v: Vec3) -> Vec3 {
    Vec3::new(-v.x, v.y, v.z)
}

/// A bone's transform in the scene's frame: its chain composed.
fn global(chain: &[Entity], bones: &Query<&mut Transform, Without<ViewArms>>) -> Affine3A {
    chain.iter().fold(Affine3A::IDENTITY, |a, e| {
        a * bones.get(*e).map_or(Affine3A::IDENTITY, |t| t.compute_affine())
    })
}

fn rot_of(a: &Affine3A) -> Quat {
    a.to_scale_rotation_translation().1
}

/// A bone's local rotation for a wanted global one, under a parent's.
fn local(parent: Quat, want: Quat) -> Quat {
    (parent.inverse() * want).normalize()
}

/// The rotation taking the frame `(d, n)` (a direction, and a normal to
/// it) onto `(d2, n2)`.
fn frame_to(d: Vec3, n: Vec3, d2: Vec3, n2: Vec3) -> Quat {
    let a = Mat3::from_cols(d, n, d.cross(n));
    let b = Mat3::from_cols(d2, n2, d2.cross(n2));
    Quat::from_mat3(&(b * a.transpose())).normalize()
}

/// A two-bone reach from `s` to `w` with bones `a` and `b` long, the elbow
/// led toward `pole`: where the elbow goes. The wrist is pulled in to what
/// the arm can reach.
pub fn reach(s: Vec3, w: Vec3, a: f32, b: f32, pole: Vec3) -> (Vec3, Vec3) {
    let to = w - s;
    let d = to.length().clamp((a - b).abs() + 1e-4, a + b - 1e-4);
    let dir = to.normalize_or(Vec3::NEG_Z);
    let side = (pole - dir * pole.dot(dir)).normalize_or(Vec3::NEG_Y);
    let cos = ((a * a + d * d - b * b) / (2.0 * a * d)).clamp(-1.0, 1.0);
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    let elbow = s + (dir * cos + side * sin) * a;
    (elbow, s + dir * d)
}

/// Where the off hand is this frame, as it looks, when it is not on the
/// fore-end (`None`: on it), and whether it is throwing (held as the clip
/// holds it, not turned with the gun). A reload takes it to the magazine
/// as the old one comes out, holds it there through the swap and brings it
/// back for the rack (`viewmodel.rs`'s beats); a throw draws it back past
/// the shoulder and flings it out of the frame.
pub fn off_hand(g: &Grips, fore: Vec3, mag: Vec3) -> Option<(Vec3, bool)> {
    let smooth = |a: f32, b: f32, x: f32| {
        let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    if let Some(u) = g.throw {
        let back = smooth(0.0, 0.25, u) * (1.0 - smooth(0.25, 0.45, u));
        let out = smooth(0.25, 0.45, u) * (1.0 - smooth(0.6, 0.95, u));
        return Some((SHOULDER_L + THROW_BACK * back + THROW_OUT * out, true));
    }
    let u = g.reload.filter(|_| !g.vent)?;
    let k = smooth(0.06, 0.26, u) * (1.0 - smooth(0.6, 0.74, u));
    Some((fore.lerp(mag, k), false))
}

/// A bone's own transform.
fn lt(bones: &Query<&mut Transform, Without<ViewArms>>, e: Entity) -> Affine3A {
    bones.get(e).map_or(Affine3A::IDENTITY, |t| t.compute_affine())
}

/// One arm as the clip holds it, in the scene's frame: the arm's, the
/// forearm's and the hand's transforms, and where the hand's grip is.
struct Held {
    arm: Affine3A,
    fore: Affine3A,
    hand: Affine3A,
    grip: Vec3,
}

fn held(bones: &Query<&mut Transform, Without<ViewArms>>, s: &Side, at: Affine3A) -> Held {
    let arm = at * lt(bones, s.arm);
    let fore = arm * lt(bones, s.fore);
    let hand = fore * lt(bones, s.hand);
    // The grip, in the hand's own frame: amid the fist's joints.
    let m1 = lt(bones, s.grip[0]);
    let m2 = m1 * lt(bones, s.grip[1]);
    let m3 = m2 * lt(bones, s.grip[2]);
    let pts = [
        m1.translation,
        m2.translation,
        m3.translation,
        lt(bones, s.grip[3]).translation,
        lt(bones, s.grip[4]).translation,
    ];
    let mean = pts.iter().fold(Vec3::ZERO, |a, p| a + Vec3::from(*p)) / pts.len() as f32;
    Held {
        arm,
        fore,
        hand,
        grip: hand.transform_point3(mean),
    }
}

/// What the left arm does this frame.
enum Off {
    /// Its own gun (two guns out).
    Dual(Grip),
    /// A long gun's fore-end, or the magazine, or a throw.
    Fore,
    /// A pistol's off hand, up only to throw.
    Throw(Vec3),
    Folded,
}

/// Every frame between the clips and the transforms' propagation: hold the
/// clip, place the rig and solve both arms onto the guns.
#[allow(clippy::too_many_arguments)]
pub fn pose(
    grips: Res<Grips>,
    clips: Option<Res<ViewClips>>,
    mut arms: Query<(&mut ViewArms, &mut Transform, &mut Visibility)>,
    mut bones: Query<&mut Transform, Without<ViewArms>>,
    mut players: Query<&mut AnimationPlayer>,
    mut body_mats: ResMut<Assets<BodyMaterial>>,
    mut commands: Commands,
) {
    let Ok((mut rig, mut root, mut vis)) = arms.single_mut() else {
        return;
    };
    let (Some(clips), Some(right)) = (clips, grips.right.filter(|_| grips.shown)) else {
        vis.set_if_neq(Visibility::Hidden);
        return;
    };
    let (Some(player), Some(_)) = (rig.player, rig.scene) else {
        return;
    };
    if !rig.graphed {
        commands
            .entity(player)
            .insert(AnimationGraphHandle(clips.graph.clone()));
        rig.graphed = true;
        return;
    }
    if rig.team != Some(grips.team) {
        rig.team = Some(grips.team);
        if let Some(m) = rig.mat.as_ref().and_then(|h| body_mats.get_mut(h)) {
            m.extension.params = params(grips.team);
        }
    }
    let long = gun(right.arm).is_some_and(|g| g.long);
    let hold = if long { Hold::Long } else { Hold::Pistol };
    if rig.hold != Some(hold) {
        let (Ok(mut p), Some(node)) = (players.get_mut(player), clips.node(hold)) else {
            return;
        };
        p.stop_all();
        p.play(node).pause();
        rig.hold = Some(hold);
        // The bones show the old hold this frame: start next frame.
        return;
    }
    let Some([r, l]) = rig.sides.as_ref() else {
        return;
    };
    vis.set_if_neq(Visibility::Inherited);

    // The rig placed so the right shoulder is where it is wanted.
    let q = facing();
    let k = ARM_SCALE;
    let r_at = global(&r.chain, &bones);
    let rh = held(&bones, r, r_at);
    let o = SHOULDER_R - q * (Vec3::from(rh.arm.translation) * k);
    *root = Transform {
        translation: o * SHRINK,
        rotation: q,
        scale: Vec3::splat(k * SHRINK),
    };
    let to_scene = |v: Vec3| q.inverse() * (v - o) / k;
    let to_view = |p: Vec3| o + q * (p * k);
    let clip_gun = view_basis();
    // A hand moved rigidly from where the clip holds it so its grip lands
    // on `target`, turned as the gun is from the clip's (straight ahead):
    // its wrist and its turn, as it looks.
    let carry = |h: &Held, gun_rot: Quat, target: Vec3, pitch: f32| {
        let d = gun_rot * Quat::from_rotation_x(pitch) * clip_gun.inverse();
        let w = to_view(Vec3::from(h.hand.translation));
        (target - d * (to_view(h.grip) - w), d * (q * rot_of(&h.hand)))
    };

    // The right hand on its grip.
    let (w, turn) = carry(&rh, right.rot, right.at + right.rot * GRIP_IN_GUN, FIST_PITCH);
    let arm = Arm {
        parent: rot_of(&r_at),
        s: Vec3::from(rh.arm.translation),
        moved: None,
    };
    solve(
        &mut bones,
        r,
        &arm,
        &rh,
        to_scene(w),
        q.inverse() * turn,
        q.inverse() * POLE_R,
    );

    // The left: its own gun, a long gun's fore-end (or off it), or folded.
    let off = match grips.left {
        Some(g) => Off::Dual(g),
        None if long => Off::Fore,
        None => match off_hand(&grips, right.at, right.at) {
            Some((at, true)) => Off::Throw(at),
            _ => Off::Folded,
        },
    };
    if let Ok(mut t) = bones.get_mut(l.arm) {
        t.scale = if matches!(off, Off::Folded) {
            Vec3::ZERO
        } else {
            Vec3::ONE
        };
    }
    if let Off::Dual(_) = off {
        // The right hand's hold, mirrored.
        for (a, b) in r.fingers.iter().zip(&l.fingers) {
            if let Ok(rt) = bones.get(*a).map(|t| t.rotation) {
                if let Ok(mut t) = bones.get_mut(*b) {
                    t.rotation = mirror_q(rt);
                }
            }
        }
    }
    let l_at = global(&l.chain, &bones);
    let lh = held(&bones, l, l_at);
    // Where its wrist goes and how its hand turns, and its shoulder.
    let (w, turn, shoulder, pole) = match off {
        Off::Folded => return,
        Off::Dual(left) => {
            // As a right hand would hold the left gun's mirror, mirrored.
            let rot = mirror_q(left.rot);
            let (w, turn) = carry(&rh, rot, flip(left.at) + rot * GRIP_IN_GUN, FIST_PITCH);
            (flip(w), mirror_q(turn), flip(SHOULDER_R), flip(POLE_R))
        }
        Off::Fore => {
            let g = gun(right.arm);
            let (fore, mag) = g.map_or((right.at, right.at), |g| {
                let reach = g.height * (1.0 - g.grip) * g.scale;
                (
                    right.at + right.rot * Vec3::new(0.0, reach * g.fore, -(g.under * g.scale + FORE_CUP)),
                    right.at + right.rot * Vec3::new(0.0, reach * MAG_SHARE, -MAG_UNDER_M),
                )
            });
            let (w, turn) = match off_hand(&grips, fore, mag) {
                Some((at, true)) => thrown(&lh, at, q, to_view),
                Some((at, false)) => carry(&lh, right.rot, at, CUP_PITCH),
                None => carry(&lh, right.rot, fore, CUP_PITCH),
            };
            (w, turn, SHOULDER_L, POLE_L)
        }
        Off::Throw(at) => {
            let (w, turn) = thrown(&lh, at, q, to_view);
            (w, turn, SHOULDER_L, POLE_L)
        }
    };
    let arm = Arm {
        parent: rot_of(&l_at),
        s: to_scene(shoulder),
        moved: Some(l_at),
    };
    solve(
        &mut bones,
        l,
        &arm,
        &lh,
        to_scene(w),
        q.inverse() * turn,
        q.inverse() * pole,
    );
}

/// A throwing hand: as the clip holds it, turned with the view, its grip
/// at `at`.
fn thrown(h: &Held, at: Vec3, q: Quat, to_view: impl Fn(Vec3) -> Vec3) -> (Vec3, Quat) {
    let w = to_view(Vec3::from(h.hand.translation));
    (at - (to_view(h.grip) - w), q * rot_of(&h.hand))
}

/// An arm's shoulder: its parent's turn, where its joint goes (scene
/// frame), and its parent's transform when the joint is moved there.
struct Arm {
    parent: Quat,
    s: Vec3,
    moved: Option<Affine3A>,
}

/// Solve one arm in the scene's frame: its shoulder joint at `arm.s`, its
/// wrist at `w`, its hand turned to `turn`, the elbow toward `pole`; the
/// bend's plane and the bones' twist are the clip's.
fn solve(
    bones: &mut Query<&mut Transform, Without<ViewArms>>,
    side: &Side,
    arm: &Arm,
    h: &Held,
    w: Vec3,
    turn: Quat,
    pole: Vec3,
) {
    let s0 = Vec3::from(h.arm.translation);
    let e0 = Vec3::from(h.fore.translation);
    let w0 = Vec3::from(h.hand.translation);
    let (a, b) = ((e0 - s0).length(), (w0 - e0).length());
    if a < 1e-5 || b < 1e-5 {
        return;
    }
    let (du, df) = ((e0 - s0) / a, (w0 - e0) / b);
    let n = du.cross(df).normalize_or(pole.cross(du).normalize_or(Vec3::X));
    let s = arm.s;
    let (e, w) = reach(s, w, a, b, pole);
    let (du2, df2) = ((e - s).normalize_or(du), (w - e).normalize_or(df));
    let n2 = du2.cross(df2).normalize_or(n);
    let ru = frame_to(du, n, du2, n2) * rot_of(&h.arm);
    let rf = frame_to(df, n, df2, n2) * rot_of(&h.fore);
    if let Ok(mut t) = bones.get_mut(side.arm) {
        if let Some(p) = arm.moved {
            t.translation = p.inverse().transform_point3(s);
        }
        t.rotation = local(arm.parent, ru);
    }
    if let Ok(mut t) = bones.get_mut(side.fore) {
        t.rotation = local(ru, rf);
    }
    if let Ok(mut t) = bones.get_mut(side.hand) {
        t.rotation = local(rf, turn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reach puts the wrist where it is asked when it can, keeps both
    /// bones their lengths, and bends the elbow toward the pole.
    #[test]
    fn the_reach_lands_the_wrist_and_bends_toward_the_pole() {
        let s = Vec3::ZERO;
        let w = Vec3::new(0.0, 0.0, -0.3);
        let pole = Vec3::new(0.0, -1.0, 0.0);
        let (e, w2) = reach(s, w, 0.2, 0.2, pole);
        assert!((w2 - w).length() < 1e-3);
        assert!(((e - s).length() - 0.2).abs() < 1e-4);
        assert!(((w2 - e).length() - 0.2).abs() < 1e-4);
        assert!(e.y < 0.0, "{e}");
        // Out of reach: straight toward it, as far as it goes.
        let (_, w3) = reach(s, Vec3::new(0.0, 0.0, -1.0), 0.2, 0.2, pole);
        assert!((w3.z + 0.4).abs() < 1e-3, "{w3}");
    }

    /// The frame map takes a direction and its normal onto the new ones.
    #[test]
    fn the_frame_map_turns_one_frame_onto_another() {
        let (d, n) = (Vec3::X, Vec3::Y);
        let (d2, n2) = (Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, 1.0, 0.0));
        let m = frame_to(d, n, d2, n2);
        assert!((m * d - d2).length() < 1e-5);
        assert!((m * n - n2).length() < 1e-5);
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

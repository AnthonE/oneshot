//! Everyone else as Triple T, after the three.js page's `art.ts`'s
//! `bodyFactory` and Gates' `crates/client/src/render/anim.rs` (MIT,
//! NOTICE): the model lifted and tinted by side, moving the way its
//! snapshot says (`anim.rs`: the clip, the legs turned toward the travel and
//! the spine back to the aim, the head on the aim's pitch, the feet planted,
//! the hold on the arms), and the gun in its right hand, a child of the
//! hand's bone, so it goes wherever the clips take the hand. The wood is left
//! its own colour; the side, the emblem and a skin's trim are sprayed on it
//! (`paint.rs`): a headband in the side's colour, glowing a little so a
//! fighter reads in shade, the emblem on the back, the trim a stripe round
//! the waist. Its gear hangs on its bones, the gun's way: the headwear
//! (`proto::HEADWEAR`) on the head, a boot (`proto::FOOTWEAR`) on each
//! foot. No hitbox or visibility changes (DESIGN.md §15). Drawing only.
//!
//! The same body is your own shadow (`own_shadow.rs`, [`spawn_shadow`]) and
//! the title's Triple T (`portrait.rs`).

use sim::consts::MAX_FIGHTERS;

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{NoAutoAabb, NoFrustumCulling};
use bevy::light::NotShadowCaster;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use client_net::core::{Drawn, DrawnVehicle};

use super::anim::{self, Gait, Hold, Playing, Seen, SpineBones, SpinePose};
use super::guns::{body_basis, gun, spawn_posed, Models};
use super::paint::{BodyMaterial, Paint, Paints};
use super::shield::{Flare, ShellPart, ShieldMaterial, Tone};

/// Triple T faces +Z in its file; our bodies face +X.
const FACE: f32 = core::f32::consts::FRAC_PI_2;

/// The bone a gun is held in, and the gun's place on it: on the axis just over
/// the fist (`guns::Gun`), level and straight ahead where `Rifle_Hold_Loop` over
/// `Idle_Loop` has the hand, with the aim level (`anim::CHEST_HELD`). In the
/// bone's own units, which are centimetres (its scale takes that back out).
const HAND_BONE: &str = "RightHand";
const HELD_AT: Vec3 = Vec3::new(4.447, 13.763, -1.861);
const HELD_TURN: Quat = Quat::from_xyzw(0.65416, 0.57784, -0.45073, -0.18712);
const HELD_SCALE: f32 = 100.0;

/// A piece of gear's place on a bone of triple-t.glb, in the bone's own
/// units (centimetres, as the hand's). Each model's script prints its own
/// (FIT): `tools/models/shades.py`, `tools/models/boots.py`.
pub struct Fit {
    bone: &'static str,
    at: Vec3,
    turn: Quat,
    scale: f32,
}

/// The shades: the lens's middle over the eyes, tipped a little onto the
/// nose.
const SHADES: Fit = Fit {
    bone: "Head",
    at: Vec3::new(0.545, 36.774, 14.136),
    turn: Quat::from_xyzw(-0.12787, 0.00560, -0.00601, 0.99176),
    scale: 100.0,
};
/// The boots, a left and a right: each sole under its root foot, turned
/// out with the toes.
const BOOT_LEFT: Fit = Fit {
    bone: "LeftFoot",
    at: Vec3::new(1.531, 16.321, 5.974),
    turn: Quat::from_xyzw(-0.90881, -0.01571, -0.03615, 0.41535),
    scale: 100.0,
};
const BOOT_RIGHT: Fit = Fit {
    bone: "RightFoot",
    at: Vec3::new(-0.412, 16.452, 6.000),
    turn: Quat::from_xyzw(-0.90600, 0.04448, 0.00651, 0.42088),
    scale: 100.0,
};

/// Where a body can be drawn, metres in its root's frame (at its feet,
/// +Y up): standing, crouched, its legs tucked in a jump, or lying as
/// `Death01` leaves it. Its meshes are skinned, so Bevy's own box is the
/// bind pose's; this one is drawn round all of them, so a body behind the
/// camera or outside a shadow cascade is skipped. A rag doll, which can be
/// thrown metres from its root, is never culled ([`Body::cull`]).
const REACH_LO: Vec3 = Vec3::new(-2.2, -0.6, -2.2);
const REACH_HI: Vec3 = Vec3::new(2.2, 2.6, 2.2);

/// [`REACH_LO`]..[`REACH_HI`] as a box in `e`'s own frame, `e` being a
/// mesh under `root`: the root's box carried down through every transform
/// between them (a scene's scale and turn included).
fn reach(e: Entity, root: Entity, parents: &Query<&ChildOf>, transforms: &Query<&Transform>) -> Aabb {
    let mut root_from_e = bevy::math::Affine3A::IDENTITY;
    let mut at = e;
    while at != root {
        let (Ok(t), Ok(&ChildOf(up))) = (transforms.get(at), parents.get(at)) else {
            break;
        };
        root_from_e = t.compute_affine() * root_from_e;
        at = up;
    }
    let to_e = root_from_e.inverse();
    let (mut lo, mut hi) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
    for i in 0..8 {
        let c = Vec3::new(
            if i & 1 == 0 { REACH_LO.x } else { REACH_HI.x },
            if i & 2 == 0 { REACH_LO.y } else { REACH_HI.y },
            if i & 4 == 0 { REACH_LO.z } else { REACH_HI.z },
        );
        let p = to_e.transform_point3(c);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    Aabb::from_min_max(lo, hi)
}

/// The albedo is very dark (Gates' MANIFEST: luma 0.056): this much lift.
const LIFT: f32 = 2.6;
/// The headband's glow in the side's colour, as a share of a white
/// surface's brightness in full sun, so a fighter reads at a glance even in
/// shade.
const GLOW: f32 = 0.12;

/// Each skin's trim (`proto::SKINS` order); none for the standard look.
const SKIN_TRIM: [Option<u32>; 6] = [
    None,
    Some(0xf2f2f2),
    Some(0xff6a1a),
    Some(0x8fd8ff),
    Some(0x58d06a),
    Some(0xe8b931),
];
/// The trim's sheen, against the side's glow.
const TRIM_GLOW: f32 = 0.35;

fn trim(skin: u8) -> Option<Color> {
    SKIN_TRIM.get(skin as usize).copied().flatten().map(|c| {
        let [_, r, g, b] = c.to_be_bytes();
        Color::srgb_u8(r, g, b)
    })
}

#[derive(Component)]
pub struct Body {
    team: u8,
    skin: u8,
    emblem: proto::Emblem,
    /// Cast in the sun's shadow and drawn by no camera (`own_shadow.rs`):
    /// no shells, no band, and every mesh on the shadow's material.
    shadow: bool,
    /// Its scene has spawned and been dressed.
    dressed: bool,
    /// The model's animation player, once its scene is in.
    player: Option<Entity>,
    hand: Entity,
    /// Its gear, each piece on its bone once dressed.
    worn: Vec<(Entity, &'static Fit)>,
    held: u8,
    /// The guns it has held of late, each posed in its hand and hidden
    /// while another is out (by arm, the longest unheld first): a swap, a
    /// death and a respawn show one again rather than load its scene anew.
    rack: Vec<(u8, Entity)>,
    /// A rag doll poses it ([`Body::go_limp`]).
    limp: bool,
    /// The shield's flare last put on its meshes' tags (`Glow`), and with
    /// it whether camouflage hides the body's own meshes and its hand.
    flare: Flare,
    /// The shield's shell (`shield.rs`): its meshes.
    shells: Vec<Entity>,
    /// The body's own meshes, which camouflage hides.
    parts: Vec<Entity>,
    /// Shown again after hiding: its next clip starts at once.
    fresh: bool,
    /// How it moves (`anim.rs`), and what its player plays.
    gait: Gait,
    playing: Playing,
    /// Where its head is moved, metres in the model's frame
    /// (`own_shadow.rs`).
    head_push: Vec3,
    /// Whose body it was last drawn as, alive or not, whether it was the
    /// frame before (none the first frame it is drawn), and how it last
    /// moved alive: for the rag doll (`rag.rs`).
    slot: u8,
    alive: bool,
    was_alive: Option<bool>,
    drawn: bool,
    vel: Vec3,
}

impl Body {
    /// Its scene has spawned and been dressed: it draws.
    pub fn dressed(&self) -> bool {
        self.dressed
    }

    /// Hidden: the next clip starts at once, not faded from this pose, and
    /// its legs start from under it.
    pub fn afresh(&mut self) {
        self.gait = Gait::default();
        self.fresh = true;
        self.drawn = false;
        self.limp = false;
    }

    /// A rag doll has it now (`rag.rs`): its root is the doll's, not the
    /// snapshot's, till it gets up or is hidden (`afresh`).
    pub fn go_limp(&mut self) {
        self.limp = true;
    }

    /// Culled by its box ([`REACH_LO`]) while it stands, and not while a
    /// rag doll has it: that can be anywhere a blow throws it.
    pub fn cull(&self, commands: &mut Commands, on: bool) {
        for &e in self.parts.iter().chain(&self.shells) {
            if let Ok(mut c) = commands.get_entity(e) {
                if on {
                    c.try_remove::<NoFrustumCulling>();
                } else {
                    c.try_insert(NoFrustumCulling);
                }
            }
        }
    }

    /// Alive now, and the frame before (none: not drawn then).
    pub fn life(&self) -> (bool, Option<bool>) {
        (self.alive, self.was_alive)
    }

    /// The seat it was last drawn for.
    pub fn slot(&self) -> u8 {
        self.slot
    }

    /// How it moved the last frame it was drawn alive, m/s.
    pub fn last_vel(&self) -> Vec3 {
        self.vel
    }

    /// Your own shadow (`own_shadow.rs`).
    pub fn is_shadow(&self) -> bool {
        self.shadow
    }

    /// Where its head is moved, metres in the model's frame.
    pub fn set_head_push(&mut self, push: Vec3) {
        self.head_push = push;
    }
}

/// The rotation of a body's model (the Triple T scene) for a body facing
/// `yaw`: what turns a direction in the world into the model's frame.
pub fn model_rotation(yaw: f32) -> Quat {
    Quat::from_rotation_y(-yaw) * Quat::from_rotation_y(FACE)
}

/// A body spawned for `team` in `look`: its root, which the caller places.
pub fn spawn(commands: &mut Commands, team: u8, look: proto::Look) -> Entity {
    spawn_as(commands, team, look, false)
}

/// Your own body, for its shadow only (`own_shadow.rs`).
pub fn spawn_shadow(commands: &mut Commands, team: u8) -> Entity {
    spawn_as(commands, team, proto::Look::default(), true)
}

/// A body's model, till Triple T's file is in.
#[derive(Component)]
pub struct WantsScene;

/// A piece of gear's model, till the models are in (as [`WantsScene`]).
#[derive(Component, Clone, Copy)]
pub enum WantsGear {
    Shades,
    BootLeft,
    BootRight,
}

/// Every body waiting for its model gets it, the file once in, and every
/// piece of gear its own.
pub fn stand_up(
    mut commands: Commands,
    models: Option<Res<Models>>,
    gltfs: Res<Assets<bevy::gltf::Gltf>>,
    waiting: Query<Entity, With<WantsScene>>,
    gear: Query<(Entity, &WantsGear)>,
) {
    let Some(models) = models else { return };
    for (e, piece) in &gear {
        let scene = match piece {
            WantsGear::Shades => &models.shades,
            WantsGear::BootLeft => &models.boot_left,
            WantsGear::BootRight => &models.boot_right,
        };
        commands
            .entity(e)
            .remove::<WantsGear>()
            .insert(SceneRoot(scene.clone()));
    }
    let Some(scene) = gltfs.get(&models.body).and_then(|g| g.scenes.first().cloned()) else {
        return;
    };
    for e in &waiting {
        commands
            .entity(e)
            .remove::<WantsScene>()
            .insert(SceneRoot(scene.clone()));
    }
}

fn spawn_as(commands: &mut Commands, team: u8, look: proto::Look, shadow: bool) -> Entity {
    let root = commands.spawn((Transform::default(), Visibility::default())).id();
    let hand = commands
        .spawn((Transform::default(), Visibility::default(), BodyPart, ChildOf(root)))
        .id();
    // Its gear, each piece hidden till `dress` puts it on its bone.
    // Their models come once the models are in (`stand_up`).
    let mut pieces: Vec<(WantsGear, &'static Fit)> = Vec::new();
    if proto::gear_valid(&proto::HEADWEAR, look.headwear) == 1 {
        pieces.push((WantsGear::Shades, &SHADES));
    }
    if proto::gear_valid(&proto::FOOTWEAR, look.footwear) == 1 {
        pieces.push((WantsGear::BootLeft, &BOOT_LEFT));
        pieces.push((WantsGear::BootRight, &BOOT_RIGHT));
    }
    let worn = if shadow { Vec::new() } else { pieces }
        .into_iter()
        .map(|(piece, fit)| {
            let e = commands
                .spawn((piece, Transform::default(), Visibility::Hidden, BodyPart, ChildOf(root)))
                .id();
            (e, fit)
        })
        .collect();
    commands
        .spawn((
            WantsScene,
            Transform::from_rotation(Quat::from_rotation_y(FACE)),
            Visibility::default(),
            ChildOf(root),
        ))
        .observe(dress);
    commands.entity(root).insert(Body {
        team,
        skin: look.skin,
        emblem: proto::emblem_valid(look.emblem),
        shadow,
        dressed: false,
        player: None,
        hand,
        worn,
        held: 0,
        rack: Vec::with_capacity(RACK),
        limp: false,
        flare: Flare {
            level: 0.0,
            tone: Tone::Gold,
            hidden: false,
        },
        shells: Vec::new(),
        parts: Vec::new(),
        fresh: false,
        gait: Gait::default(),
        playing: Playing::default(),
        head_push: Vec3::ZERO,
        slot: 0,
        alive: true,
        was_alive: None,
        drawn: false,
        vel: Vec3::ZERO,
    });
    root
}

/// The stand-in: a capsule in the side's colour with a visor where it
/// looks (`Opts::capsules`).
pub fn spawn_capsule(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    team: u8,
) -> Entity {
    let body = mats.add(StandardMaterial {
        base_color: team_color(team),
        perceptual_roughness: 0.6,
        ..default()
    });
    let visor = mats.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.8, 0.2),
        ..default()
    });
    commands
        .spawn((Transform::default(), Visibility::default()))
        .with_children(|c| {
            c.spawn((
                Mesh3d(meshes.add(Capsule3d::new(0.35, 1.1))),
                MeshMaterial3d(body),
                Transform::from_xyz(0.0, 0.9, 0.0),
            ));
            c.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.36, 0.12, 0.12))),
                MeshMaterial3d(visor),
                Transform::from_xyz(0.3, 1.55, 0.0).with_rotation(Quat::from_rotation_y(core::f32::consts::FRAC_PI_2)),
            ));
        })
        .id()
}

/// A capsule this frame: where, which way, lower when crouched, flat dead.
pub fn pose_capsule(d: &Drawn, root: Entity, at: &mut Query<&mut Transform>) {
    if let Ok(mut t) = at.get_mut(root) {
        let s = if d.crouched { 0.72 } else { 1.0 };
        *t = Transform::from_xyz(d.pos.x, d.pos.y, d.pos.z)
            .with_rotation(Quat::from_rotation_y(-d.yaw))
            .with_scale(Vec3::new(1.0, if d.alive { s } else { 0.2 }, 1.0));
    }
}

pub fn team_color(team: u8) -> Color {
    if team & 1 == 0 {
        Color::srgb(0.85, 0.27, 0.23)
    } else {
        Color::srgb(0.23, 0.48, 0.85)
    }
}

/// The scene is in: paint it (`paint.rs`: the side's headband, the emblem,
/// the trim), find its player and its spine, and give each of its meshes a
/// shield shell beside it. Your own shadow's meshes take the shadow's
/// material instead, and no shells.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dress(
    ev: On<SceneInstanceReady>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    mut bodies: Query<&mut Body>,
    players: Query<(), With<AnimationPlayer>>,
    meshes: Query<(
        &MeshMaterial3d<StandardMaterial>,
        &Mesh3d,
        Option<&SkinnedMesh>,
        &Transform,
    )>,
    (names, transforms): (Query<&Name>, Query<&Transform>),
    (mats, mut body_mats): (Res<Assets<StandardMaterial>>, ResMut<Assets<BodyMaterial>>),
    mut shield_mats: ResMut<Assets<ShieldMaterial>>,
    shadow_mat: Option<Res<super::own_shadow::ShadowMat>>,
    warm: Query<(), With<super::prewarm::WarmBody>>,
    (mut paints, mut images, mesh_assets): (ResMut<Paints>, ResMut<Assets<Image>>, Res<Assets<Mesh>>),
    mut commands: Commands,
) {
    let model = ev.entity;
    let Ok(&ChildOf(root)) = parents.get(model) else {
        return;
    };
    let Ok(mut body) = bodies.get_mut(root) else {
        return;
    };
    commands
        .entity(root)
        .insert(anim::bind_spine(model, &names, &parents, &children, &transforms));
    match super::rag::bind(model, &names, &parents, &children) {
        Some(r) => {
            commands.entity(root).insert(r);
        }
        None if !body.shadow => error!("rag: a bone is missing or moved in models/triple-t.glb: no rag doll"),
        None => {}
    }
    // The gun goes in the hand: it goes where the hand goes.
    let bone = children
        .iter_descendants(model)
        .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == HAND_BONE));
    match bone {
        Some(bone) => {
            commands.entity(body.hand).insert((
                ChildOf(bone),
                Transform {
                    translation: HELD_AT,
                    rotation: HELD_TURN.normalize(),
                    scale: Vec3::splat(HELD_SCALE),
                },
            ));
        }
        None => error!("bodies: no {HAND_BONE} in models/triple-t.glb: its gun lies at its feet"),
    }
    // Its gear goes on its bones: it goes where they go.
    for &(worn, fit) in &body.worn {
        let bone = children
            .iter_descendants(model)
            .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == fit.bone));
        match bone {
            Some(bone) => {
                commands.entity(worn).insert((
                    ChildOf(bone),
                    Transform {
                        translation: fit.at,
                        rotation: fit.turn.normalize(),
                        scale: Vec3::splat(fit.scale),
                    },
                    Visibility::Inherited,
                ));
            }
            None => error!("bodies: no {} in models/triple-t.glb: gear left off", fit.bone),
        }
    }
    body.dressed = true;
    if body.shadow {
        let Some(shadow) = shadow_mat else {
            return;
        };
        for e in children.iter_descendants(model) {
            if players.contains(e) {
                body.player = Some(e);
            }
            if meshes.contains(e) {
                commands
                    .entity(e)
                    .remove::<MeshMaterial3d<StandardMaterial>>()
                    .insert((MeshMaterial3d(shadow.0.clone()), NoFrustumCulling));
            }
        }
        return;
    }
    // A body drawn only to warm shows its shell, so the shield's pipeline
    // is built with it (`prewarm.rs`).
    let shell_shown = if warm.contains(root) {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    // The paint's mask, the body unrolled from its meshes the first time.
    let model_meshes: Vec<&Mesh> = children
        .iter_descendants(model)
        .filter_map(|e| meshes.get(e).ok())
        .filter_map(|(_, mesh, ..)| mesh_assets.get(&mesh.0))
        .collect();
    let mask = paints.mask(body.emblem, &model_meshes, &mut images);
    let sun = bevy::light::light_consts::lux::DIRECT_SUNLIGHT / core::f32::consts::PI;
    let mut params = super::paint::params(
        team_color(body.team).to_linear(),
        body.emblem,
        trim(body.skin).map(|c| c.to_linear()),
        GLOW * sun,
        GLOW * TRIM_GLOW * sun,
        LIFT,
    );
    params.flare_gold = (Vec3::from_array(FLARE_GOLD) * sun).extend(0.0);
    params.flare_over = (Vec3::from_array(FLARE_OVER) * sun).extend(0.0);
    let shell = shield_mats.add(super::shield::material());
    for e in children.iter_descendants(model) {
        if players.contains(e) {
            body.player = Some(e);
        }
        if let Ok((m, mesh, skin, at)) = meshes.get(e) {
            let Some(mut base) = mats.get(&m.0).cloned() else {
                continue;
            };
            let bounds = reach(e, root, &parents, &transforms);
            if let Ok(&ChildOf(parent)) = parents.get(e) {
                let mut s = commands.spawn((
                    mesh.clone(),
                    MeshMaterial3d(shell.clone()),
                    *at,
                    shell_shown,
                    ShellPart,
                    MeshTag(0),
                    NotShadowCaster,
                    bounds,
                    NoAutoAabb,
                    ChildOf(parent),
                ));
                if let Some(skin) = skin {
                    s.insert(skin.clone());
                }
                body.shells.push(s.id());
            }
            base.base_color = LinearRgba::rgb(LIFT, LIFT, LIFT).into();
            base.emissive = LinearRgba::BLACK;
            // In nits, under the camera's exposure like everything lit.
            base.emissive_exposure_weight = 1.0;
            let h = body_mats.add(BodyMaterial {
                base,
                extension: Paint {
                    params,
                    mask: mask.clone(),
                },
            });
            body.parts.push(e);
            commands.entity(e).remove::<MeshMaterial3d<StandardMaterial>>().insert((
                MeshMaterial3d(h),
                bounds,
                NoAutoAabb,
                BodyPart,
                MeshTag(0),
            ));
        }
    }
}

/// A body's own mesh, or its hand (and the gun in it): what camouflage
/// hides.
#[derive(Component)]
pub struct BodyPart;

/// What shows and hides by itself on a body: its shield's shells, and its
/// own meshes and hand.
pub type Parts<'w, 's> = Query<'w, 's, &'static mut Visibility, Or<(With<ShellPart>, With<BodyPart>)>>;

/// How its shells and its meshes flare: their tags (`shield::Flare`).
pub type Tags<'w, 's> = Query<'w, 's, &'static mut MeshTag, Or<(With<ShellPart>, With<BodyPart>)>>;

/// The light the shell casts on the body under it at a flare of 1, gold
/// (the shield) or green (an overshield): a share of a white surface's
/// brightness in full sun.
const FLARE_GOLD: [f32; 3] = [0.25, 0.18, 0.05];
const FLARE_OVER: [f32; 3] = [0.06, 0.24, 0.12];

/// An overshield's shell: green, never under `OVER_REST` while it is on.
pub const OVER: Vec3 = Vec3::new(0.7, 3.0, 1.3);
const OVER_REST: f32 = 0.32;
/// Camouflage: hidden past `CAMO_HIDE`, the body is only a pale rim and
/// the crawl of lines (the shell with no fill), `CAMO_EDGE` bright just
/// past it, down to `CAMO_FAINT` at its best.
pub const CAMO: Vec3 = Vec3::new(1.4, 1.8, 2.3);
const CAMO_HIDE: f32 = 0.3;
const CAMO_EDGE: f32 = 0.6;
const CAMO_FAINT: f32 = 0.08;

/// What a body's shell shows this frame: the shield's flare (`fx/bodies.rs`,
/// 0 none), an overshield, and how hidden its camouflage keeps it (0..1).
#[derive(Clone, Copy, Debug, Default)]
pub struct Glow {
    pub hit: f32,
    pub over: bool,
    pub cloak: f32,
}

impl Glow {
    /// The shell's flare: its level, its colour, and whether the body
    /// itself is hidden.
    fn shell(self) -> Flare {
        if self.cloak > CAMO_HIDE {
            let best = sim::consts::CAMO_BEST;
            let k = ((self.cloak - CAMO_HIDE) / (best - CAMO_HIDE)).clamp(0.0, 1.0);
            let level = CAMO_EDGE + (CAMO_FAINT - CAMO_EDGE) * k;
            Flare {
                level: level.max(self.hit * 0.5),
                tone: Tone::Camo,
                hidden: true,
            }
        } else if self.over {
            Flare {
                level: self.hit.max(OVER_REST),
                tone: Tone::Over,
                hidden: false,
            }
        } else {
            Flare {
                level: self.hit,
                tone: Tone::Gold,
                hidden: false,
            }
        }
    }
}

/// A body's shell and skin this frame (`Glow`): the shell shows at its
/// level and colour, the body under it takes a little of the light, and
/// camouflage hides the body and its gun, leaving the shell's rim. All of
/// it by its meshes' tags and visibility: no material is written, so a
/// hit or a recharge costs a fight nothing but those.
pub fn flare(root: Entity, glow: Glow, bodies: &mut Query<&mut Body>, tags: &mut Tags, parts: &mut Parts) {
    // Its shells and meshes come with its scene: till then, nothing to set.
    let Ok(mut body) = bodies.get_mut(root) else {
        return;
    };
    if !body.dressed {
        return;
    }
    let f = glow.shell();
    let was = body.flare;
    if f == was {
        return;
    }
    let show = |on: bool| if on { Visibility::Inherited } else { Visibility::Hidden };
    let shown = f.level > 0.0;
    if shown != (was.level > 0.0) {
        for &e in &body.shells {
            if let Ok(mut v) = parts.get_mut(e) {
                *v = show(shown);
            }
        }
    }
    if f.hidden != was.hidden {
        for &e in body
            .parts
            .iter()
            .chain(core::iter::once(&body.hand))
            .chain(body.worn.iter().map(|(e, _)| e))
        {
            if let Ok(mut v) = parts.get_mut(e) {
                *v = show(!f.hidden);
            }
        }
    }
    let tag = f.tag();
    for &e in body.parts.iter().chain(&body.shells) {
        if let Ok(mut t) = tags.get_mut(e) {
            t.set_if_neq(MeshTag(tag.0));
        }
    }
    body.flare = f;
}

/// One fighter's body this frame: where it stands, how it moves (its gait,
/// read off the snapshot: `anim.rs`), which gun it holds. `seat` is its
/// hull's yaw and its seat, riding. `dt` is the frame's.
#[allow(clippy::too_many_arguments)]
pub fn pose(
    commands: &mut Commands,
    models: &Models,
    d: &Drawn,
    seat: Option<(f32, usize)>,
    root: Entity,
    bodies: &mut Query<&mut Body>,
    at: &mut Query<&mut Transform>,
    dt: f32,
) {
    let Ok(mut body) = bodies.get_mut(root) else {
        return;
    };
    body.was_alive = body.drawn.then_some(body.alive);
    body.drawn = true;
    body.alive = d.alive;
    body.slot = d.slot;
    if d.alive {
        body.vel = Vec3::new(d.vel.x, d.vel.y, d.vel.z);
    }
    let driving = seat.is_some_and(|(_, s)| s == sim::vehicle::DRIVER);
    // A driver faces the way the hull does.
    let yaw = match seat {
        Some((hull, s)) if s == sim::vehicle::DRIVER => hull,
        _ => d.yaw,
    };
    // A corpse's root is its rag doll's: written here as well, the whole
    // skeleton under it would go round the transforms again every frame.
    if !(body.limp && !d.alive) {
        if let Ok(mut t) = at.get_mut(root) {
            t.set_if_neq(Transform::from_xyz(d.pos.x, d.pos.y, d.pos.z).with_rotation(Quat::from_rotation_y(-yaw)));
        }
    }
    // Every gun is held in the long hold. The pistol clip is Gates'
    // first-person arms (hands up at the face): over a body it held nothing.
    // A pistol's reload is still its own clip.
    let gun_of = gun(d.arm);
    let hold = gun_of.map(|_| Hold::Long);
    let reload_clip = gun_of.map(|g| if g.long { Hold::Long } else { Hold::Pistol });
    body.gait.observe(
        &Seen {
            alive: d.alive,
            grounded: d.grounded,
            crouched: d.crouched,
            driving,
            riding: seat.is_some(),
            vel: Vec2::new(d.vel.x, d.vel.z),
            yaw,
            pitch: d.pitch,
            hold,
            reload: reload_clip
                .filter(|_| d.reloading && !sim::arms::arm(d.arm).hot())
                .map(|h| (h, sim::arms::arm(d.arm).reload_ms as f32 / 1000.0)),
        },
        dt,
    );
    // The gun: swapped when the arm changes, hidden on the dead and at the
    // wheel. It waits for the hand it goes in.
    let arm = if d.alive && !driving { d.arm } else { 0 };
    if arm != body.held && body.dressed {
        let held = body.held;
        if let Some(&(_, out)) = body.rack.iter().find(|r| r.0 == held) {
            commands.entity(out).insert(Visibility::Hidden);
        }
        if let Some(i) = body.rack.iter().position(|r| r.0 == arm) {
            // Racked: shown again, now the latest held.
            let r = body.rack.remove(i);
            commands.entity(r.1).insert(Visibility::Inherited);
            body.rack.push(r);
        } else if let Some((pivot, scene)) = spawn_posed(commands, models, arm, body_basis(), body.hand) {
            if body.shadow {
                // Your shadow's gun is a shadow too.
                commands.entity(scene).observe(super::own_shadow::shade_gun);
            } else {
                super::gun_skin::wear(commands, scene, body.skin, arm);
            }
            if body.rack.len() >= RACK {
                // The one held longest ago goes (never the one just put
                // away: it is the latest after this).
                let (_, old) = body.rack.remove(0);
                commands.entity(old).despawn();
            }
            body.rack.push((arm, pivot));
        }
        body.held = arm;
    }
}

/// Guns a body keeps posed in its hand: the two it carries and one more
/// (the last it dropped or the one on the floor it took).
const RACK: usize = 3;

/// Every body's player, crossfaded to what its gait wants (`anim::drive`),
/// once a frame after the bodies are posed. The first frame a body has a
/// player, the player is handed the rig's graph.
#[allow(clippy::type_complexity)]
pub fn drive(
    mut commands: Commands,
    rig: Option<Res<anim::Rig>>,
    time: Res<Time>,
    mut bodies: Query<(Entity, &mut Body, Has<super::rag::Ragdoll>)>,
    mut players: Query<(
        &mut AnimationPlayer,
        Option<&mut AnimationTransitions>,
        Has<AnimationGraphHandle>,
    )>,
) {
    let Some(rig) = rig else {
        return;
    };
    let dt = time.delta_secs();
    for (e, mut body, limp) in &mut bodies {
        let Some(p) = body.player else {
            continue;
        };
        let Ok((mut player, transitions, bound)) = players.get_mut(p) else {
            continue;
        };
        // Limp, the rag doll poses it (`rag.rs`): no clip may.
        if limp {
            if player.playing_animations().next().is_some() {
                player.stop_all();
            }
            continue;
        }
        let Some(mut transitions) = transitions.filter(|_| bound) else {
            commands
                .entity(p)
                .insert((AnimationGraphHandle(rig.graph.clone()), AnimationTransitions::new()));
            continue;
        };
        // Hidden (fog, a parked shadow): nothing plays. A clip left running
        // is sampled into a skeleton nobody sees every frame; shown again,
        // it starts afresh (`Body::afresh`).
        if !body.drawn {
            if player.playing_animations().next().is_some() {
                player.stop_all();
                body.playing = Playing::default();
            }
            continue;
        }
        let b = &mut *body;
        // Spent on the first clip it starts, not while it is still hidden.
        let fresh = b.fresh && b.gait.clip.is_some();
        if fresh {
            b.fresh = false;
        }
        anim::drive(
            &rig,
            &b.gait,
            &mut b.playing,
            &mut player,
            &mut transitions,
            (dt, fresh, anim::desync(e)),
        );
    }
}

/// Every body's spine turned (`anim::turn_spine`): in `PostUpdate`, after
/// the animation and before the transforms propagate.
/// Not a rag doll's (it poses every bone) nor a hidden body's.
pub fn pose_spines(
    mut q: Query<(&Body, &mut SpineBones), Without<super::rag::Ragdoll>>,
    mut bones: Query<&mut Transform>,
) {
    for (b, mut spine) in &mut q {
        if !b.drawn {
            continue;
        }
        let pose = SpinePose {
            clip: b.gait.clip,
            pitch: b.gait.pitch,
            hip_yaw: b.gait.hip_yaw,
            held: b.playing.held(),
            reload: b.playing.reload_share(),
            head_push: b.head_push,
        };
        anim::turn_spine(pose, &mut spine, &mut bones);
    }
}

/// Each rider's vehicle and seat, by slot.
pub fn seats(vehicles: &[DrawnVehicle]) -> [Option<(DrawnVehicle, usize)>; MAX_FIGHTERS] {
    let mut out = [None; MAX_FIGHTERS];
    for v in vehicles {
        for (k, &s) in v.seats.iter().enumerate() {
            if let Some(o) = s.checked_sub(1).and_then(|i| out.get_mut(i as usize)) {
                *o = Some((*v, k));
            }
        }
    }
    out
}

//! Everyone else as Triple T, after the three.js page's `art.ts`'s
//! `bodyFactory` and Gates' `crates/client/src/render/anim.rs` (MIT,
//! NOTICE): the model lifted and tinted by side, moving the way its
//! snapshot says (`anim.rs`: the clip, the legs turned toward the travel and
//! the spine back to the aim, the head on the aim's pitch, the feet planted,
//! the hold on the arms), and the gun they hold in hand. A skin is a faint
//! sheen of its trim and a band at the waist, an emblem (`emblem.rs`) paint
//! on the back plate that turns with the spine: the side's colour still
//! carries the fighter, and no hitbox or visibility changes (DESIGN.md
//! §15). Drawing only.
//!
//! The same body is your own shadow (`own_shadow.rs`, [`spawn_shadow`]) and
//! the title's Triple T (`portrait.rs`).

use std::collections::BTreeMap;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use client_net::core::{Drawn, DrawnVehicle};

use super::anim::{self, Gait, Hold, Playing, Seen, SpineBones, SpinePose};
use super::guns::{body_basis, gun, spawn_posed, Models};
use super::shield::{ShellPart, ShieldMaterial};

/// Triple T faces +Z in its file; our bodies face +X.
const FACE: f32 = core::f32::consts::FRAC_PI_2;

/// Where the gun hangs, in the body's frame (+X forward): art.ts's hand,
/// a little lower, at Triple T's chest rather than its eyes.
const HAND: Vec3 = Vec3::new(0.18, 1.18, 0.2);
const HAND_CROUCHED_Y: f32 = 0.85;

/// The albedo is very dark (Gates' MANIFEST: luma 0.056): this much lift,
/// and this much of it toward the side's colour.
const LIFT: f32 = 2.6;
const TINT: f32 = 0.42;
/// A faint glow of the side's colour, as a share of a white surface's
/// brightness in full sun, so a fighter reads at a glance even in shade.
const GLOW: f32 = 0.1;

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

/// The emblem on the back plate, in the frame of the `Spine` bone at rest
/// (centimetres: the rig is scaled 0.01): 30 cm square, 1.12 m up, its face
/// 1 cm proud of the back (which stands 22.7 cm behind the spine between
/// 0.7 m and 1.75 m), facing back. From the file's bind pose; a new model
/// needs these again.
const EMBLEM_BONE: &str = "Spine";
const EMBLEM_AT: Vec3 = Vec3::new(-0.463, 1.594, -19.391);
const EMBLEM_TURN: Quat = Quat::from_xyzw(-0.00452, -0.99971, -0.02303, 0.00452);
const EMBLEM_SCALE: f32 = 30.0;

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
    held: u8,
    /// Its materials (its own: `dress` copies them per body), their glow
    /// at rest, and the shield's flare last put on them.
    mats: Vec<Handle<StandardMaterial>>,
    glow: LinearRgba,
    lit: f32,
    /// The shield's shell (`shield.rs`): its material and its meshes.
    shell: Handle<ShieldMaterial>,
    shells: Vec<Entity>,
    /// Shown again after hiding: its next clip starts at once.
    fresh: bool,
    /// How it moves (`anim.rs`), and what its player plays.
    gait: Gait,
    playing: Playing,
    /// Where its head is moved, metres in the model's frame
    /// (`own_shadow.rs`).
    head_push: Vec3,
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
pub fn spawn(
    commands: &mut Commands,
    models: &Models,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    team: u8,
    look: proto::Look,
) -> Entity {
    let root = spawn_as(commands, models, team, look, false);
    if let Some(c) = trim(look.skin) {
        // The band at the waist, in the trim's colour.
        commands.spawn((
            Mesh3d(meshes.add(Torus::new(0.175, 0.225))),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: c,
                perceptual_roughness: 0.4,
                metallic: 0.5,
                ..default()
            })),
            Transform::from_xyz(0.0, 1.0, 0.0),
            ChildOf(root),
        ));
    }
    root
}

/// Your own body, for its shadow only (`own_shadow.rs`).
pub fn spawn_shadow(commands: &mut Commands, models: &Models, team: u8) -> Entity {
    spawn_as(commands, models, team, proto::Look::default(), true)
}

fn spawn_as(commands: &mut Commands, models: &Models, team: u8, look: proto::Look, shadow: bool) -> Entity {
    let root = commands.spawn((Transform::default(), Visibility::default())).id();
    let hand = commands
        .spawn((Transform::from_translation(HAND), Visibility::default(), ChildOf(root)))
        .id();
    commands
        .spawn((
            SceneRoot(models.body_scene.clone()),
            Transform::from_rotation(Quat::from_rotation_y(FACE)),
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
        held: 0,
        mats: Vec::new(),
        glow: LinearRgba::BLACK,
        lit: 0.0,
        shell: Handle::default(),
        shells: Vec::new(),
        fresh: false,
        gait: Gait::default(),
        playing: Playing::default(),
        head_push: Vec3::ZERO,
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

/// The scene is in: tint its materials by side, find its player and its
/// spine, and give each of its meshes a shield shell beside it. Your own
/// shadow's meshes take the shadow's material instead, and no shells.
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
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut shield_mats: ResMut<Assets<ShieldMaterial>>,
    shadow_mat: Option<Res<super::own_shadow::ShadowMat>>,
    warm: Query<(), With<super::prewarm::WarmBody>>,
    (mut emblems, mut images, mut quads): (
        ResMut<super::emblem::Emblems>,
        ResMut<Assets<Image>>,
        ResMut<Assets<Mesh>>,
    ),
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
    let tint = team_color(body.team).to_linear();
    let base = LinearRgba::rgb(
        LIFT * (1.0 - TINT) + tint.red * 3.0 * TINT,
        LIFT * (1.0 - TINT) + tint.green * 3.0 * TINT,
        LIFT * (1.0 - TINT) + tint.blue * 3.0 * TINT,
    );
    let glow = GLOW * bevy::light::light_consts::lux::DIRECT_SUNLIGHT / core::f32::consts::PI;
    let shell = shield_mats.add(super::shield::material());
    body.shell = shell.clone();
    let sheen = trim(body.skin).map_or(LinearRgba::BLACK, |c| c.to_linear() * TRIM_GLOW);
    for e in children.iter_descendants(model) {
        if players.contains(e) {
            body.player = Some(e);
        }
        if let Ok((m, mesh, skin, at)) = meshes.get(e) {
            let Some(mut mat) = mats.get(&m.0).cloned() else {
                continue;
            };
            if let Ok(&ChildOf(parent)) = parents.get(e) {
                let mut s = commands.spawn((
                    mesh.clone(),
                    MeshMaterial3d(shell.clone()),
                    *at,
                    shell_shown,
                    ShellPart,
                    NotShadowCaster,
                    NoFrustumCulling,
                    ChildOf(parent),
                ));
                if let Some(skin) = skin {
                    s.insert(skin.clone());
                }
                body.shells.push(s.id());
            }
            mat.base_color = base.into();
            mat.emissive = (tint + sheen) * glow;
            // In nits, under the camera's exposure like everything lit.
            mat.emissive_exposure_weight = 1.0;
            let h = mats.add(mat);
            body.mats.push(h.clone());
            commands.entity(e).insert((MeshMaterial3d(h), NoFrustumCulling));
        }
    }
    body.glow = (tint + sheen) * glow;
    // The emblem: paint on the back plate, on the spine so it turns with it.
    if body.emblem != proto::EMBLEM_NONE {
        let spine = children
            .iter_descendants(model)
            .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == EMBLEM_BONE));
        if let Some(spine) = spine {
            let mat = emblems.material(body.emblem, &mut images, &mut mats);
            commands.spawn((
                Mesh3d(emblems.quad(&mut quads)),
                MeshMaterial3d(mat),
                Transform {
                    translation: EMBLEM_AT,
                    rotation: EMBLEM_TURN,
                    scale: Vec3::splat(EMBLEM_SCALE),
                },
                NotShadowCaster,
                NoFrustumCulling,
                ChildOf(spine),
            ));
        }
    }
}

/// The light the shell casts on the body under it, gold, at a flare of 1:
/// a share of a white surface's brightness in full sun.
const FLARE_GOLD: [f32; 3] = [0.25, 0.18, 0.05];

/// The shield's flare on a body: `level` 0 is none (`fx/bodies.rs`). The
/// shell shows at that level, and the body under it takes a little gold.
pub fn flare(
    root: Entity,
    level: f32,
    bodies: &mut Query<&mut Body>,
    mats: &mut Assets<StandardMaterial>,
    shield_mats: &mut Assets<ShieldMaterial>,
    shells: &mut Query<&mut Visibility, With<ShellPart>>,
) {
    let Ok(mut body) = bodies.get_mut(root) else {
        return;
    };
    if (level - body.lit).abs() < 0.02 && (level > 0.0 || body.lit == 0.0) {
        return;
    }
    let shown = level > 0.0;
    if shown != (body.lit > 0.0) {
        for &e in &body.shells {
            if let Ok(mut v) = shells.get_mut(e) {
                *v = if shown {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
    body.lit = level;
    if let Some(m) = shield_mats.get_mut(&body.shell) {
        m.extension.color.w = level;
    }
    let sun = bevy::light::light_consts::lux::DIRECT_SUNLIGHT / core::f32::consts::PI;
    let k = level * sun;
    let e = LinearRgba::rgb(
        body.glow.red + FLARE_GOLD[0] * k,
        body.glow.green + FLARE_GOLD[1] * k,
        body.glow.blue + FLARE_GOLD[2] * k,
    );
    for h in &body.mats {
        if let Some(m) = mats.get_mut(h) {
            m.emissive = e;
        }
    }
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
    let driving = seat.is_some_and(|(_, s)| s == sim::vehicle::DRIVER);
    // A driver faces the way the hull does.
    let yaw = match seat {
        Some((hull, s)) if s == sim::vehicle::DRIVER => hull,
        _ => d.yaw,
    };
    if let Ok(mut t) = at.get_mut(root) {
        *t = Transform::from_xyz(d.pos.x, d.pos.y, d.pos.z).with_rotation(Quat::from_rotation_y(-yaw));
    }
    let hold = gun(d.arm).map(|g| if g.long { Hold::Long } else { Hold::Pistol });
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
        },
        dt,
    );
    // The gun: swapped when the arm changes, hidden on the dead and at the
    // wheel, pitched with the aim.
    let arm = if d.alive && !driving { d.arm } else { 0 };
    if arm != body.held {
        commands.entity(body.hand).despawn_related::<Children>();
        let posed = spawn_posed(commands, models, arm, body_basis(), body.hand);
        // Your shadow's gun is a shadow too.
        if let (true, Some((_, scene))) = (body.shadow, posed) {
            commands.entity(scene).observe(super::own_shadow::shade_gun);
        }
        body.held = arm;
    }
    if let Ok(mut t) = at.get_mut(body.hand) {
        t.translation = Vec3::new(HAND.x, if d.crouched { HAND_CROUCHED_Y } else { HAND.y }, HAND.z);
        t.rotation = Quat::from_rotation_z(d.pitch);
    }
}

/// Every body's player, crossfaded to what its gait wants (`anim::drive`),
/// once a frame after the bodies are posed. The first frame a body has a
/// player, the player is handed the rig's graph.
#[allow(clippy::type_complexity)]
pub fn drive(
    mut commands: Commands,
    rig: Option<Res<anim::Rig>>,
    time: Res<Time>,
    mut bodies: Query<(Entity, &mut Body)>,
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
    for (e, mut body) in &mut bodies {
        let Some(p) = body.player else {
            continue;
        };
        let Ok((mut player, transitions, bound)) = players.get_mut(p) else {
            continue;
        };
        let Some(mut transitions) = transitions.filter(|_| bound) else {
            commands
                .entity(p)
                .insert((AnimationGraphHandle(rig.graph.clone()), AnimationTransitions::new()));
            continue;
        };
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
pub fn pose_spines(mut q: Query<(&Body, &mut SpineBones)>, mut bones: Query<&mut Transform>) {
    for (b, mut spine) in &mut q {
        let pose = SpinePose {
            clip: b.gait.clip,
            pitch: b.gait.pitch,
            hip_yaw: b.gait.hip_yaw,
            head_push: b.head_push,
        };
        anim::turn_spine(pose, &mut spine, &mut bones);
    }
}

/// Each rider's vehicle and seat, by slot.
pub fn seats(vehicles: &[DrawnVehicle]) -> BTreeMap<u8, (DrawnVehicle, usize)> {
    let mut out = BTreeMap::new();
    for v in vehicles {
        for (k, &s) in v.seats.iter().enumerate() {
            if s > 0 {
                out.insert(s - 1, (*v, k));
            }
        }
    }
    out
}

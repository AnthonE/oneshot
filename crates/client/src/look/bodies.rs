//! Everyone else as Triple T, after `web/src/art.ts`'s `bodyFactory`: the
//! model lifted and tinted by side, an animation picked from what the
//! snapshot says (alive, grounded, crouched, speed, seated), with Gates'
//! 0.15 s crossfades, and the gun they hold in hand. Drawing only.

use std::collections::BTreeMap;
use std::time::Duration;

use bevy::animation::RepeatAnimation;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use client_net::core::{Drawn, DrawnVehicle};

use super::guns::{body_basis, gun, spawn_posed, Models};

/// Triple T faces +Z in its file; our bodies face +X.
const FACE: f32 = core::f32::consts::FRAC_PI_2;

/// The clips we play, by name in the file.
const CLIPS: [&str; 12] = [
    "Idle_Loop",
    "Walk_Loop",
    "Jog_Fwd_Loop",
    "Sprint_Loop",
    "Crouch_Idle_Loop",
    "Crouch_Fwd_Loop",
    "Jump_Loop",
    "Death01",
    "Rifle_Hold_Loop",
    "Pistol_Idle_Loop",
    "Driving_Loop",
    "Sitting_Idle_Loop",
];
const WALK: usize = 1;
const JOG: usize = 2;
const SPRINT: usize = 3;
const CROUCH_IDLE: usize = 4;
const CROUCH_FWD: usize = 5;
const JUMP: usize = 6;
const DEATH: usize = 7;
const RIFLE: usize = 8;
const PISTOL: usize = 9;
const DRIVING: usize = 10;

const FADE: Duration = Duration::from_millis(150);

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

/// The animation graph, once the model has loaded.
#[derive(Resource)]
pub struct Graph {
    handle: Handle<AnimationGraph>,
    nodes: Vec<AnimationNodeIndex>,
}

/// Builds the graph when the model's clips are in.
pub fn graph(
    mut commands: Commands,
    models: Option<Res<Models>>,
    have: Option<Res<Graph>>,
    gltfs: Res<Assets<bevy::gltf::Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let (Some(models), None) = (models, have) else {
        return;
    };
    let Some(g) = gltfs.get(&models.body) else {
        return;
    };
    let clips: Vec<Handle<AnimationClip>> = CLIPS
        .iter()
        .map(|n| g.named_animations.get(*n).cloned().unwrap_or_default())
        .collect();
    let (graph, nodes) = AnimationGraph::from_clips(clips);
    commands.insert_resource(Graph {
        handle: graphs.add(graph),
        nodes,
    });
}

#[derive(Component)]
pub struct Body {
    team: u8,
    skin: u8,
    /// The model's animation player, once its scene is in.
    player: Option<Entity>,
    clip: Option<usize>,
    hand: Entity,
    held: u8,
}

/// What every body shares: the stand-in capsule (`?capsules=1`: a software
/// GPU can't skin sixteen Triple Ts at a playable rate) with a material a
/// side, and the skins' waist band with a material a trim.
#[derive(Resource)]
pub struct Plain {
    mesh: Handle<Mesh>,
    sides: [Handle<StandardMaterial>; 2],
    band: Handle<Mesh>,
    trims: Vec<Option<Handle<StandardMaterial>>>,
}

pub fn setup_plain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let side = |t: u8| StandardMaterial {
        base_color: team_color(t),
        perceptual_roughness: 0.6,
        ..default()
    };
    let trims = (0..SKIN_TRIM.len() as u8)
        .map(|k| {
            trim(k).map(|c| {
                mats.add(StandardMaterial {
                    base_color: c,
                    perceptual_roughness: 0.4,
                    metallic: 0.5,
                    ..default()
                })
            })
        })
        .collect();
    commands.insert_resource(Plain {
        mesh: meshes.add(Capsule3d::new(0.35, 1.1)),
        sides: [mats.add(side(0)), mats.add(side(1))],
        band: meshes.add(Torus::new(0.175, 0.225)),
        trims,
    });
}

/// A body spawned for `team` in `skin`: its root, which the caller places.
/// With `capsule`, a capsule instead of Triple T.
pub fn spawn(commands: &mut Commands, models: &Models, plain: &Plain, team: u8, skin: u8, capsule: bool) -> Entity {
    let root = commands.spawn((Transform::default(), Visibility::default())).id();
    let hand = commands
        .spawn((Transform::from_translation(HAND), Visibility::default(), ChildOf(root)))
        .id();
    // A skin's band at the waist, in its trim.
    if let Some(Some(m)) = plain.trims.get(skin as usize) {
        commands.spawn((
            Mesh3d(plain.band.clone()),
            MeshMaterial3d(m.clone()),
            Transform::from_xyz(0.0, 1.0, 0.0),
            ChildOf(root),
        ));
    }
    match capsule {
        true => {
            commands.spawn((
                Mesh3d(plain.mesh.clone()),
                MeshMaterial3d(plain.sides[team as usize & 1].clone()),
                Transform::from_xyz(0.0, 0.9, 0.0),
                ChildOf(root),
            ));
        }
        false => {
            commands
                .spawn((
                    SceneRoot(models.body_scene.clone()),
                    Transform::from_rotation(Quat::from_rotation_y(FACE)),
                    ChildOf(root),
                ))
                .observe(dress);
        }
    }
    commands.entity(root).insert(Body {
        team,
        skin,
        player: None,
        clip: None,
        hand,
        held: 0,
    });
    root
}

impl Body {
    pub fn skin(&self) -> u8 {
        self.skin
    }
}

/// Each skin's trim (`proto::SKINS` order), drawn over the side's colour
/// and never instead of it: a skin changes no hitbox and no fighter's
/// visibility (web/src/players.ts's `SKIN_TRIM`).
pub const SKIN_TRIM: [Option<u32>; 6] = [
    None,
    Some(0xf2f2f2),
    Some(0xff6a1a),
    Some(0x8fd8ff),
    Some(0x58d06a),
    Some(0xe8b931),
];
/// The trim's sheen, as a share of the side's glow.
const TRIM_GLOW: f32 = 0.75;

pub fn trim(skin: u8) -> Option<Color> {
    let c = (*SKIN_TRIM.get(skin as usize)?)?;
    Some(Color::srgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8))
}

pub fn team_color(team: u8) -> Color {
    if team & 1 == 0 {
        Color::srgb(0.85, 0.27, 0.23)
    } else {
        Color::srgb(0.23, 0.48, 0.85)
    }
}

/// The scene is in: tint its materials by side, and find its player.
#[allow(clippy::too_many_arguments)]
fn dress(
    ev: On<SceneInstanceReady>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    mut bodies: Query<&mut Body>,
    players: Query<(), With<AnimationPlayer>>,
    meshes: Query<&MeshMaterial3d<StandardMaterial>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let model = ev.entity;
    let Ok(&ChildOf(root)) = parents.get(model) else {
        return;
    };
    let Ok(mut body) = bodies.get_mut(root) else {
        return;
    };
    let tint = team_color(body.team).to_linear();
    let base = LinearRgba::rgb(
        LIFT * (1.0 - TINT) + tint.red * 3.0 * TINT,
        LIFT * (1.0 - TINT) + tint.green * 3.0 * TINT,
        LIFT * (1.0 - TINT) + tint.blue * 3.0 * TINT,
    );
    let glow = GLOW * bevy::light::light_consts::lux::DIRECT_SUNLIGHT / core::f32::consts::PI;
    for e in children.iter_descendants(model) {
        if players.contains(e) {
            body.player = Some(e);
        }
        if let Ok(m) = meshes.get(e) {
            let Some(mut mat) = mats.get(&m.0).cloned() else {
                continue;
            };
            mat.base_color = base.into();
            mat.emissive = tint * glow;
            // A skin is a faint sheen of its trim on top.
            if let Some(t) = trim(body.skin) {
                mat.emissive += t.to_linear() * (glow * TRIM_GLOW);
            }
            // In nits, under the camera's exposure like everything lit.
            mat.emissive_exposure_weight = 1.0;
            commands
                .entity(e)
                .insert((MeshMaterial3d(mats.add(mat)), NoFrustumCulling));
        }
    }
}

/// One fighter's body this frame: where, which clip, which gun.
#[allow(clippy::too_many_arguments)]
pub fn pose(
    commands: &mut Commands,
    models: &Models,
    graph: Option<&Graph>,
    d: &Drawn,
    seat: Option<(&DrawnVehicle, usize)>,
    root: Entity,
    bodies: &mut Query<&mut Body>,
    at: &mut Query<&mut Transform>,
    players: &mut Query<(&mut AnimationPlayer, Option<&mut AnimationTransitions>), Without<AnimationGraphHandle>>,
    playing: &mut Query<(&mut AnimationPlayer, &mut AnimationTransitions), With<AnimationGraphHandle>>,
) {
    let Ok(mut body) = bodies.get_mut(root) else {
        return;
    };
    let driving = seat.is_some_and(|(_, s)| s == sim::vehicle::DRIVER);
    // A driver faces the way the hull does.
    let yaw = match seat {
        Some((v, s)) if s == sim::vehicle::DRIVER => v.yaw,
        _ => d.yaw,
    };
    if let Ok(mut t) = at.get_mut(root) {
        *t = Transform::from_xyz(d.pos.x, d.pos.y, d.pos.z).with_rotation(Quat::from_rotation_y(-yaw));
    }
    let speed = (d.vel.x * d.vel.x + d.vel.z * d.vel.z).sqrt();
    let long = gun(d.arm).is_none_or(|g| g.long);
    let clip = if !d.alive {
        DEATH
    } else if driving {
        DRIVING
    } else if seat.is_some() {
        RIFLE
    } else if !d.grounded {
        JUMP
    } else if d.crouched {
        if speed > 0.5 {
            CROUCH_FWD
        } else {
            CROUCH_IDLE
        }
    } else if speed > 6.5 {
        SPRINT
    } else if speed > 3.5 {
        JOG
    } else if speed > 0.4 {
        WALK
    } else if long {
        RIFLE
    } else {
        PISTOL
    };
    if let (Some(p), Some(graph)) = (body.player, graph) {
        // The first frame with a player: hand it the graph.
        if players.get_mut(p).is_ok() {
            commands
                .entity(p)
                .insert((AnimationGraphHandle(graph.handle.clone()), AnimationTransitions::new()));
        } else if let Ok((mut player, mut tr)) = playing.get_mut(p) {
            if body.clip != Some(clip) {
                let a = tr.play(&mut player, graph.nodes[clip], FADE);
                if clip == DEATH {
                    a.set_repeat(RepeatAnimation::Never);
                } else {
                    a.repeat();
                }
                body.clip = Some(clip);
            }
        }
    }
    // The gun: swapped when the arm changes, hidden on the dead and at the
    // wheel, pitched with the aim.
    let arm = if d.alive && !driving { d.arm } else { 0 };
    if arm != body.held {
        commands.entity(body.hand).despawn_related::<Children>();
        spawn_posed(commands, models, arm, body_basis(), body.hand);
        body.held = arm;
    }
    if let Ok(mut t) = at.get_mut(body.hand) {
        t.translation = Vec3::new(HAND.x, if d.crouched { HAND_CROUCHED_Y } else { HAND.y }, HAND.z);
        t.rotation = Quat::from_rotation_z(d.pitch);
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

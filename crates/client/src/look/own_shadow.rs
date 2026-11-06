//! Your own shadow: the whole body, cast and never drawn. After Gates'
//! `crates/client/src/render/own_shadow.rs` (MIT, NOTICE).
//!
//! The eye is inside your head, so your body is never drawn, and the gun in
//! view casts nothing (`viewmodel.rs`): the sun saw nothing of you. This is
//! the body a remote sees (`bodies.rs`), posed off the predicted fighter
//! every frame (its velocity, crouch, the air, the look's yaw and pitch: its
//! legs, twist and head move as anyone's), holding the gun you hold, cast in
//! the sun's shadow and drawn by no camera. None while you are dead, or at
//! the wheel (the chase camera sees the hull; a passenger still stands).
//!
//! **Not a render layer, as Gates has it.** Gates puts this body on a layer
//! only the sun shares; Bevy 0.18's shadow queue (`queue_shadows`) also
//! drops a caster whose layers miss the *camera's*, so on a layer no camera
//! draws it casts nothing either. Here it is on the world's layer with
//! [`ShadowOnly`]: no prepass (it is in no depth, normal or motion buffer)
//! and a fragment that discards everything, so every camera passes over it,
//! while the shadow pass, which draws depth alone off the vertex stage,
//! takes it in full. That is the same on the desktop, WebGPU and WebGL2.
//!
//! **The head is moved down the sun's ray** ([`HEAD_PUSH_M`]). Its volume
//! holds the eye, and the gun in view hangs a hand's width in front of it,
//! so a head cast where it is would shade that gun on every frame whatever
//! the sun. Moved along the light, the head throws the same shadow on
//! everything further than the push from it (a directional light's shadow
//! of a thing doesn't change as the thing slides along its rays), and none
//! on the eye or the gun.
//!
//! While there is no body of yours to shadow it waits far under the map,
//! idle, holding a rifle, still drawn (and discarded) there for
//! [`PARKED_WARM_S`], so its pipelines are built before the match needs
//! them, the way `prewarm.rs` builds everyone else's; then hidden and left
//! unposed until you have a body again.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::scene::SceneInstanceReady;
use bevy::shader::ShaderRef;
use client_net::core::Drawn;
use sim::geom::V3;

use super::guns::Models;
use crate::{LinkRes, Look, Opts};

pub const SHADER: &str = "shaders/shadow_only.wgsl";

/// How far the shadow's head is moved away from the sun, metres: Triple
/// T's head reaches 0.3 m from its middle (Gates measured), the gun in view
/// hangs 0.15 m from the eye, and the eye can sit 0.15 m off the head's
/// middle; this clears all three with room, and still leaves a head as low
/// as a crouch's well above the floor.
pub const HEAD_PUSH_M: f32 = 0.75;

/// Where the body waits with nobody to shadow: under every map, where
/// nothing it casts can land on anything (it is further from the sun than
/// every floor).
const PARKED: Vec3 = Vec3::new(0.0, -500.0, 0.0);
/// How long it is drawn there before it is hidden, seconds: its pipelines
/// are built within a few frames, a slow GPU's within a few seconds.
pub const PARKED_WARM_S: f32 = 6.0;

/// Drawn by no camera, cast in full. The uniform is never read but by the
/// discard (WebGL2 wants a binding of 16 bytes at least).
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug, Default)]
pub struct ShadowOnly {
    #[uniform(0)]
    pub unused: Vec4,
}

impl Material for ShadowOnly {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn enable_prepass() -> bool {
        false
    }
}

/// The one material every shadow mesh wears.
#[derive(Resource)]
pub struct ShadowMat(pub Handle<ShadowOnly>);

/// Your shadow's body.
#[derive(Component)]
pub struct OwnShadow {
    shown: bool,
    /// Seconds parked since it was last shown (or spawned).
    parked_s: f32,
}

pub fn setup(mut commands: Commands, mut mats: ResMut<Assets<ShadowOnly>>) {
    commands.insert_resource(ShadowMat(mats.add(ShadowOnly::default())));
}

/// A gun's scene under your shadow's hand has spawned: its meshes take the
/// shadow's material.
pub fn shade_gun(
    ev: On<SceneInstanceReady>,
    children: Query<&Children>,
    lit: Query<Option<&bevy::gltf::GltfMaterialName>, With<MeshMaterial3d<StandardMaterial>>>,
    shadow: Option<Res<ShadowMat>>,
    mut commands: Commands,
) {
    let Some(shadow) = shadow else {
        return;
    };
    for e in children.iter_descendants(ev.entity) {
        if let Ok(name) = lit.get(e) {
            // A sword's blade is light, and throws no shadow.
            if super::blade::is_blade(name) {
                commands.entity(e).insert(Visibility::Hidden);
                continue;
            }
            commands
                .entity(e)
                .remove::<MeshMaterial3d<StandardMaterial>>()
                // Never culled, so it is queued (and its pipelines built)
                // while the body waits under the map.
                .insert((MeshMaterial3d(shadow.0.clone()), NoFrustumCulling));
        }
    }
}

/// Spawn the body once the models are in, then stand it on the predicted
/// feet every frame, turned the way you look, moving the way you move.
#[allow(clippy::too_many_arguments)]
pub fn stream(
    mut commands: Commands,
    link: NonSend<LinkRes>,
    look: Res<Look>,
    time: Res<Time>,
    opts: Res<Opts>,
    models: Option<Res<Models>>,
    sun: Query<&GlobalTransform, With<super::rig::Sun>>,
    mut own: Query<(Entity, &mut OwnShadow)>,
    mut bodies: Query<&mut super::bodies::Body>,
    mut at: Query<&mut Transform>,
    film: Option<Res<crate::film::Film>>,
) {
    // Capsules are for a software GPU that can't skin a room (`Opts`).
    if opts.capsules {
        return;
    }
    let Some(models) = models else {
        return;
    };
    let s = link.0.session();
    let core = &s.core;
    let Ok((root, mut own)) = own.single_mut() else {
        let e = super::bodies::spawn_shadow(&mut commands, core.team);
        commands.entity(e).insert((
            OwnShadow {
                shown: false,
                parked_s: 0.0,
            },
            Transform::from_translation(PARKED),
        ));
        return;
    };
    let (f, p) = core.own();
    let seat = f.riding().map(|(_, seat)| (look.yaw, seat));
    let driving = seat.is_some_and(|(_, s)| s == sim::vehicle::DRIVER);
    // A film's camera off the eye draws the body itself, which casts its
    // own shadow (`film.rs`).
    let shown = s.ready && f.alive && !driving && !film.is_some_and(|f| f.third());
    if shown != own.shown {
        own.shown = shown;
        if let Ok(mut b) = bodies.get_mut(root) {
            b.afresh();
        }
        if shown {
            if own.parked_s >= PARKED_WARM_S {
                commands.entity(root).insert(Visibility::Inherited);
            }
            own.parked_s = 0.0;
        }
    }
    // Warmed under the map: hidden, and not posed, until there is a body
    // of yours again.
    if !shown {
        let was = own.parked_s;
        own.parked_s += time.delta_secs();
        if own.parked_s >= PARKED_WARM_S {
            if was < PARKED_WARM_S {
                commands.entity(root).insert(Visibility::Hidden);
            }
            return;
        }
    }
    // Parked, it stands idle holding a rifle, so its skinned and its gun's
    // pipelines are built and its player is bound before the match needs
    // them.
    let d = if shown {
        Drawn {
            slot: core.slot,
            team: core.team,
            alive: true,
            crouched: f.body.crouched,
            grounded: f.body.grounded,
            pos: p,
            vel: f.body.vel,
            yaw: look.yaw,
            pitch: look.pitch,
            arm: f.gun().arm,
            ..Drawn::default()
        }
    } else {
        Drawn {
            alive: true,
            grounded: true,
            pos: V3::new(PARKED.x, PARKED.y, PARKED.z),
            arm: sim::arms::AR,
            ..Drawn::default()
        }
    };
    let seat = seat.filter(|_| shown);
    super::bodies::pose(
        &mut commands,
        &models,
        &d,
        seat,
        root,
        &mut bodies,
        &mut at,
        time.delta_secs(),
    );
    // The head, down the sun's ray, in the model's frame.
    let light = sun.iter().next().map_or(Vec3::NEG_Y, |g| g.forward().as_vec3());
    if let Ok(mut b) = bodies.get_mut(root) {
        let push = if shown { light * HEAD_PUSH_M } else { Vec3::ZERO };
        b.set_head_push(super::bodies::model_rotation(d.yaw).inverse() * push);
    }
}

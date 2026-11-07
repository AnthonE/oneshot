//! Draw everything once, tiny, before the thing that wears it appears
//! (after Gates' `client/src/render/prewarm.rs`, MIT).
//!
//! A render pipeline is built the first time something using it is queued
//! for a draw. Without this, that was mid-fight: the first rocket, the
//! first gun swapped to, the first shield hit (its shell is hidden until
//! then), the first time fog showed another fighter. In a browser (no
//! threads to build on) a build stalls the frame; natively the draw is
//! skipped and the thing pops in late. So every `StandardMaterial` is
//! drawn the frame it's made, and every model (a fighter with its shield
//! shell showing, every gun, the rocket, the frag) once its scene is in, a
//! fraction of a pixel across in front of the camera, for a few frames.
//!
//! Specialisation happens when a draw is queued, not when a fragment runs,
//! so a warm draw only has to survive frustum culling.

use bevy::asset::LoadState;
use bevy::camera::primitives::MeshAabb;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

use super::guns::Models;
use crate::Eye;

/// Frames a warm draw stays once it can draw: long enough for what it
/// queued to be built.
pub const WARM_FRAMES: u32 = 8;
/// In front of the eye, past the near plane.
const WARM_AHEAD_M: f32 = 0.5;
/// Its size: under a tenth of a pixel at 4K at the narrowest view (Gates
/// measured; their first 1e-4 was a visible speck).
const WARM_SIZE_M: f32 = 0.000_02;

/// The skin the warm gun wears: any but the standard look.
const WARM_SKIN: u8 = 1;
/// Gear painted by no maps, whose pipelines the warm body's don't build: wire
/// glasses' tinted lenses, and a lit visor.
const WARM_GEAR: [&str; 2] = ["aviators", "visor"];
/// Where the warm fighter stands: far under every map.
const UNDER_M: f32 = -500.0;

/// A warm draw, and its frames left once it can draw.
#[derive(Component)]
pub struct Warming(pub u32);

/// A scene under a warm draw has spawned (until then it draws nothing).
#[derive(Component)]
pub struct SceneDone;

/// A fighter drawn only to warm: its shield shell shows (`bodies::dress`),
/// so the shield's skinned pipeline is built too.
#[derive(Component)]
pub struct WarmBody;

/// The triangle every material is warmed on.
#[derive(Resource)]
pub struct WarmMesh(Handle<Mesh>);

pub fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(WarmMesh(meshes.add(Triangle3d::default())));
}

fn at() -> Transform {
    Transform::from_xyz(0.0, 0.0, -WARM_AHEAD_M).with_scale(Vec3::splat(WARM_SIZE_M))
}

/// A warm draw of every material the frame it's made.
pub fn warm(
    mut commands: Commands,
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mesh: Option<Res<WarmMesh>>,
    eye: Query<Entity, With<Eye>>,
) {
    let (Some(mesh), Ok(eye)) = (mesh, eye.single()) else {
        events.clear();
        return;
    };
    for event in events.read() {
        let AssetEvent::Added { id } = event else {
            continue;
        };
        // Nothing holds it any more: nothing will draw it.
        let Some(handle) = materials.get_strong_handle(*id) else {
            continue;
        };
        commands.entity(eye).with_child((
            Warming(WARM_FRAMES),
            SceneDone,
            Mesh3d(mesh.0.clone()),
            MeshMaterial3d(handle),
            at(),
        ));
    }
}

/// What a weathered box's pipeline is specialised on: its maps (normal,
/// occlusion-roughness-metal, glow) and whether its mesh carries the
/// rock's value (`UV_1`).
type LevelKey = (bool, bool, bool, Option<bool>);

/// Whether `mesh` carries attribute `id`; `None` for a mesh whose data
/// went to the render world (`RenderAssetUsages::RENDER_WORLD`, extracted),
/// whose layout can't be read here: its warm is then once per material
/// (Gates' `layout_key`), as `Mesh::attribute` would panic on it.
pub fn carries(mesh: &Mesh, id: bevy::mesh::MeshVertexAttribute) -> Option<bool> {
    mesh.try_attribute_option(id).ok().map(|a| a.is_some())
}

/// A warm draw of each kind of weathered box (`weathering.rs`) the frame
/// the first of its kind is dressed, on that box's own mesh, shrunk: its
/// pipeline wants its mesh's layout (tangents, the rock's `UV_1`), which
/// the warm triangle doesn't carry, and a box behind the eye at the start
/// would otherwise build it mid-fight.
pub fn warm_level(
    mut commands: Commands,
    mut seen: Local<Vec<LevelKey>>,
    mats: Res<Assets<super::weathering::LevelMaterial>>,
    meshes: Res<Assets<Mesh>>,
    fresh: Query<
        (&Mesh3d, &MeshMaterial3d<super::weathering::LevelMaterial>),
        Added<MeshMaterial3d<super::weathering::LevelMaterial>>,
    >,
    eye: Query<Entity, With<Eye>>,
) {
    let Ok(eye) = eye.single() else {
        return;
    };
    for (mesh, mat) in &fresh {
        let (Some(m), Some(me)) = (mats.get(&mat.0), meshes.get(&mesh.0)) else {
            continue;
        };
        let key = (
            m.base.normal_map_texture.is_some(),
            m.base.metallic_roughness_texture.is_some(),
            m.base.emissive_texture.is_some(),
            carries(me, Mesh::ATTRIBUTE_UV_1),
        );
        let warmed = |k: &LevelKey| (k.0, k.1, k.2) == (key.0, key.1, key.2);
        if seen.contains(&key) || (key.3.is_none() && seen.iter().any(warmed)) {
            continue;
        }
        seen.push(key);
        let extent = me
            .compute_aabb()
            .map_or(1.0, |b| (Vec3::from(b.half_extents) * 2.0).max_element().max(1e-3));
        commands.entity(eye).with_child((
            Warming(WARM_FRAMES),
            SceneDone,
            Mesh3d(mesh.0.clone()),
            MeshMaterial3d(mat.0.clone()),
            Transform::from_xyz(0.0, 0.0, -WARM_AHEAD_M).with_scale(Vec3::splat(WARM_SIZE_M / extent)),
        ));
    }
}

/// Once, when every model is in: a fighter, every gun, the rocket, the
/// frag and the truck's minigun, drawn warm.
#[allow(clippy::too_many_arguments)]
pub fn warm_models(
    mut commands: Commands,
    mut done: Local<bool>,
    models: Option<Res<Models>>,
    assets: Res<AssetServer>,
    eye: Query<Entity, With<Eye>>,
    plasma: Option<Res<super::fx::plasma::Kit>>,
) {
    if *done {
        return;
    }
    let (Some(models), Ok(eye)) = (models, eye.single()) else {
        return;
    };
    let mut scenes: Vec<Handle<Scene>> = models.guns.iter().flatten().cloned().collect();
    scenes.extend([
        models.rocket.clone(),
        models.frag.clone(),
        models.turret.clone(),
        models.turret_barrels.clone(),
    ]);
    scenes.extend(
        WARM_GEAR
            .iter()
            .filter_map(|n| proto::HEADWEAR.iter().position(|g| g == n))
            .filter_map(|g| models.head_scene(g as u8)),
    );
    let settled = |h: &Handle<Scene>| matches!(assets.load_state(h), LoadState::Loaded | LoadState::Failed(_));
    let body = matches!(
        assets.load_state(&models.body),
        LoadState::Loaded | LoadState::Failed(_)
    );
    if !body || !scenes.iter().all(settled) || !settled(&models.truck) {
        return;
    }
    *done = true;
    // A plasma ball: its pipeline is built before the first one is thrown.
    if let Some(kit) = plasma {
        for material in kit.all() {
            commands.entity(eye).with_child((
                Warming(WARM_FRAMES),
                SceneDone,
                Mesh3d(kit.mesh.clone()),
                MeshMaterial3d(material.clone()),
                bevy::mesh::MeshTag(0),
                at(),
            ));
        }
    }
    // A truck, smaller still for its size: its steel and trim are materials
    // of its own that wear (`truck.rs`), made when its scene is in, which
    // says when this may count down.
    let small = Transform::from_xyz(0.0, 0.0, -WARM_AHEAD_M).with_scale(Vec3::splat(WARM_SIZE_M / 6.0));
    let truck = commands
        .spawn((Warming(WARM_FRAMES), small, Visibility::default(), ChildOf(eye)))
        .id();
    super::truck::spawn(&mut commands, &models, truck);
    let done = |ev: On<SceneInstanceReady>, mut commands: Commands| {
        commands.entity(ev.entity).insert(SceneDone);
    };
    for scene in &scenes {
        commands
            .spawn((Warming(WARM_FRAMES), SceneRoot(scene.clone()), at(), ChildOf(eye)))
            .observe(done);
    }
    // One gun in a coat: every coat is the one pipeline.
    if let Some(scene) = models.guns.get(sim::arms::AR as usize).cloned().flatten() {
        let coated = commands
            .spawn((Warming(WARM_FRAMES), SceneRoot(scene), at(), ChildOf(eye)))
            .observe(done)
            .id();
        super::gun_skin::wear(&mut commands, coated, WARM_SKIN, sim::arms::AR);
    }
    // A sword's blade lit: the shader is its own.
    if let Some(scene) = models.guns.get(sim::arms::SWORD as usize).cloned().flatten() {
        let lit = commands
            .spawn((Warming(WARM_FRAMES), SceneRoot(scene), at(), ChildOf(eye)))
            .observe(done)
            .id();
        super::blade::light(&mut commands, lit, sim::arms::SWORD);
    }
    // Far under the map at full size: a body's meshes and shells are never
    // culled (`bodies::dress`), so it is queued, shadow and all, where
    // neither it nor its shadow (cast down, away from the floor) can be
    // seen. In front of the eye, however small, its shell (pushed out along
    // the normals by a fixed distance, `shield.wgsl`) would be a disc.
    // In an emblem, so the paint's first mask is baked here too, and the
    // gear, so its maps are in before anyone wears it.
    let look = proto::Look {
        skin: 0,
        emblem: proto::emblem(1, 1, 0, 4),
        headwear: 1,
        footwear: 1,
    };
    let body = super::bodies::spawn(&mut commands, 0, look);
    commands
        .entity(body)
        .insert((Warming(WARM_FRAMES), WarmBody, Transform::from_xyz(0.0, UNDER_M, 0.0)));
}

/// The paint masks of everyone in the match (`paint.rs`), baked as the
/// roster comes in, one a frame, rather than the frame fog first shows each
/// of them: a mask is a few tens of milliseconds of the CPU, mid-fight.
pub fn bake_roster(
    st: Res<crate::hud::HudState>,
    paints: Option<ResMut<super::paint::Paints>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(mut paints) = paints else {
        return;
    };
    // Read first, so a frame with nothing to bake marks nothing changed.
    let next = st
        .names()
        .map(|(&slot, _)| st.look(slot).emblem)
        .find(|&e| paints.wants(e));
    if let Some(e) = next {
        paints.prebake(e, &mut images);
    }
}

/// Counts each warm draw down once it can draw, then lets it go.
pub fn retire(
    mut commands: Commands,
    mut warming: Query<(Entity, &mut Warming, Has<SceneDone>, Option<&super::bodies::Body>)>,
) {
    for (e, mut w, scene, body) in &mut warming {
        // A body draws once dressed; a scene once spawned.
        let ready = body.map_or(scene, |b| b.dressed());
        if !ready {
            continue;
        }
        match w.0.checked_sub(1) {
            Some(0) | None => commands.entity(e).despawn(),
            Some(left) => w.0 = left,
        }
    }
}

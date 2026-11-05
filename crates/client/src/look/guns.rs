//! The models: Triple T (`assets/models`, from Gates' GLB by `tools/glb`,
//! through `tools/assets.mjs`) and the guns, ours, each built by a Blender
//! script in `tools/models` (its numbers here are what `finish` printed),
//! and how each gun is held, after the three.js page's `art.ts`'s `GUNS`.

use bevy::prelude::*;
use sim::arms;

/// One gun's model and how it sits in a hand. Each model stands barrel up
/// (+Y), top toward +Z, stock at y = 0.
pub struct Gun {
    pub file: &'static str,
    /// The model's length, stock to muzzle, in its own units.
    pub height: f32,
    /// Where the grip is, as a fraction of `height` from the stock.
    pub grip: f32,
    pub scale: f32,
    /// A long gun (two hands) or a pistol.
    pub long: bool,
    pub sound: &'static str,
    pub takes: u8,
    pub far: &'static str,
    pub reload: &'static str,
}

#[allow(clippy::too_many_arguments)] // a table row
const fn g(
    file: &'static str,
    height: f32,
    grip: f32,
    scale: f32,
    long: bool,
    sound: &'static str,
    takes: u8,
    far: &'static str,
    reload: &'static str,
) -> Gun {
    Gun {
        file,
        height,
        grip,
        scale,
        long,
        sound,
        takes,
        far,
        reload,
    }
}

/// By arm id (`sim::arms`), from 1.
pub const GUNS: [(u8, Gun); 15] = [
    (
        arms::AR,
        g(
            "ar",
            0.79,
            0.3823,
            1.0,
            true,
            "shot_rifle",
            3,
            "shot_rifle_far",
            "reload_rifle_0",
        ),
    ),
    (
        arms::BR,
        g(
            "br",
            0.88,
            0.3489,
            1.0,
            true,
            "shot_heli",
            3,
            "shot_heli_far",
            "reload_rifle_0",
        ),
    ),
    (
        arms::MAGNUM,
        g(
            "magnum",
            0.305,
            0.2361,
            1.0,
            false,
            "shot_gun",
            2,
            "shot_gun_far",
            "reload_pistol_0",
        ),
    ),
    (
        arms::SNIPER,
        g(
            "sniper",
            1.25,
            0.2816,
            0.78,
            true,
            "shot_bolt",
            3,
            "shot_bolt_far",
            "reload_bolt_0",
        ),
    ),
    (
        arms::SHOTGUN,
        g(
            "shotgun",
            0.95,
            0.3389,
            1.0,
            true,
            "shot_shotgun",
            3,
            "shot_shotgun_far",
            "reload_shotgun_0",
        ),
    ),
    (
        arms::ROCKET,
        g(
            "rocket-launcher",
            1.054,
            0.3786,
            1.0,
            true,
            "shot_rocket",
            3,
            "shot_rifle_far",
            "reload_rifle_0",
        ),
    ),
    (
        arms::SMG,
        g(
            "smg",
            0.55,
            0.5673,
            1.0,
            true,
            "shot_smg",
            3,
            "shot_smg_far",
            "reload_rifle_0",
        ),
    ),
    // The ship's line (`tools/models`: the Veil guns), each with its own
    // model; the sounds still borrow the nearest human gun's.
    (
        arms::SWORD,
        g(
            "sword",
            1.0095,
            0.1288,
            1.0,
            false,
            "shot_bolt",
            3,
            "shot_bolt_far",
            "reload_pistol_0",
        ),
    ),
    (
        arms::CARBINE,
        g(
            "carbine",
            0.9544,
            0.3612,
            1.0,
            true,
            "shot_gun",
            2,
            "shot_gun_far",
            "reload_rifle_0",
        ),
    ),
    (
        arms::BEAM_RIFLE,
        g(
            "beam-rifle",
            1.3013,
            0.3468,
            0.82,
            true,
            "shot_bolt",
            3,
            "shot_bolt_far",
            "reload_bolt_0",
        ),
    ),
    (
        arms::ION_RIFLE,
        g(
            "ion-rifle",
            0.6164,
            0.389,
            1.0,
            true,
            "shot_smg",
            3,
            "shot_smg_far",
            "reload_rifle_0",
        ),
    ),
    (
        arms::ION_PISTOL,
        g(
            "ion-pistol",
            0.3665,
            0.2449,
            1.0,
            false,
            "shot_smg",
            3,
            "shot_smg_far",
            "reload_pistol_0",
        ),
    ),
    (
        arms::SHARDS,
        g(
            "shards",
            0.5773,
            0.4067,
            1.0,
            true,
            "shot_smg",
            3,
            "shot_smg_far",
            "reload_rifle_0",
        ),
    ),
    (
        arms::LAUNCHER,
        g(
            "launcher",
            0.9048,
            0.4087,
            1.0,
            true,
            "shot_rocket",
            3,
            "shot_rifle_far",
            "reload_shotgun_0",
        ),
    ),
    (
        arms::CANNON,
        g(
            "cannon",
            1.2498,
            0.4399,
            1.0,
            true,
            "shot_rocket",
            3,
            "shot_rifle_far",
            "reload_rifle_0",
        ),
    ),
];

pub fn gun(arm: u8) -> Option<&'static Gun> {
    GUNS.iter().find(|(a, _)| *a == arm).map(|(_, g)| g)
}

/// Model axes onto a holder's: barrel (+Y) along its forward, top (+Z) up.
/// A body faces +X.
pub fn body_basis() -> Quat {
    Quat::from_mat3(&Mat3::from_cols(Vec3::Z, Vec3::X, Vec3::Y))
}

/// The same for a camera, which faces −Z.
pub fn view_basis() -> Quat {
    Quat::from_mat3(&Mat3::from_cols(Vec3::X, Vec3::NEG_Z, Vec3::Y))
}

/// Every model, asked for at startup.
#[derive(Resource)]
pub struct Models {
    pub body: Handle<bevy::gltf::Gltf>,
    pub body_scene: Handle<Scene>,
    /// By arm id; index 0 (no gun) is unused.
    pub guns: Vec<Option<Handle<Scene>>>,
    pub rocket: Handle<Scene>,
    pub frag: Handle<Scene>,
    /// A vehicle's hull (`look/truck.rs`).
    pub truck: Handle<Scene>,
}

pub fn load(mut commands: Commands, assets: Res<AssetServer>) {
    let scene = |f: &str| assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/{f}.glb")));
    let mut guns = vec![None; arms::COUNT];
    for (arm, g) in &GUNS {
        guns[*arm as usize] = Some(scene(g.file));
    }
    commands.insert_resource(Models {
        body: assets.load("models/triple-t.glb"),
        body_scene: scene("triple-t"),
        guns,
        rocket: scene("rocket"),
        frag: scene("frag"),
        truck: scene("truck"),
    });
}

/// Where a posed gun's muzzle is, in its pivot's frame (for the flash and
/// the tracer's start).
#[derive(Component)]
pub struct Muzzle;

/// `arm`'s gun posed so its grip sits at the returned pivot's origin,
/// turned by `basis`, as a child of `parent`: `(pivot, scene)`. None for no
/// gun.
pub fn spawn_posed(
    commands: &mut Commands,
    models: &Models,
    arm: u8,
    basis: Quat,
    parent: Entity,
) -> Option<(Entity, Entity)> {
    let g = gun(arm)?;
    let scene = models.guns.get(arm as usize)?.clone()?;
    let pivot = commands
        .spawn((Transform::from_rotation(basis), Visibility::default(), ChildOf(parent)))
        .id();
    let scaler = commands
        .spawn((
            Transform::from_scale(Vec3::splat(g.scale)),
            Visibility::default(),
            ChildOf(pivot),
        ))
        .id();
    let scene = commands
        .spawn((
            SceneRoot(scene),
            Transform::from_xyz(0.0, -g.height * g.grip, 0.0),
            ChildOf(scaler),
        ))
        .id();
    commands.spawn((
        Muzzle,
        Transform::from_xyz(0.0, g.height * (1.0 - g.grip), 0.0),
        Visibility::default(),
        ChildOf(scaler),
    ));
    Some((pivot, scene))
}

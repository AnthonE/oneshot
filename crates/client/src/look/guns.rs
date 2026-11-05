//! The models: Triple T (`assets/models`, from Gates' GLB by `tools/glb`,
//! through `tools/assets.mjs`) and the guns, ours, each commissioned from
//! Meshy and cut by a Blender script in `tools/models` (its numbers here
//! are what `finish` printed: `traces/guns/<name>.json`),
//! and how each gun is held, after the three.js page's `art.ts`'s `GUNS`.

use bevy::prelude::*;
use sim::arms;

/// One gun's model and how it sits in a hand. Each model stands barrel up
/// (+Y), top toward +Z, stock at y = 0; the axis it is held by (x = z = 0)
/// runs just over the fist on its grip.
pub struct Gun {
    pub file: &'static str,
    /// The model's length, stock to muzzle, in its own units.
    pub height: f32,
    /// Where the grip is, as a fraction of `height` from the stock.
    pub grip: f32,
    /// The muzzle's height over the axis (the model's +Z), in its own units.
    pub rise: f32,
    /// A long gun's other hand: how far up the barrel from the grip (a share
    /// of the barrel's length past it), and how far under the axis the
    /// belly it cups is there.
    pub fore: f32,
    pub under: f32,
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
    rise: f32,
    fore: f32,
    under: f32,
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
        rise,
        fore,
        under,
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
            0.78,
            0.4487,
            0.0397,
            0.3721,
            0.0516,
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
            0.2898,
            0.0294,
            0.6,
            -0.0096,
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
            0.1705,
            0.0404,
            0.6,
            -0.0044,
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
            0.24,
            0.0446,
            0.6,
            0.0127,
            0.72,
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
            0.2947,
            0.0643,
            0.6,
            0.0109,
            0.96,
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
            0.4317,
            0.104,
            0.4541,
            0.03,
            0.96,
            true,
            "shot_rocket",
            3,
            "shot_launcher_far",
            "reload_rifle_0",
        ),
    ),
    (
        arms::SMG,
        g(
            "smg",
            0.55,
            0.4182,
            0.0471,
            0.6,
            0.0128,
            1.0,
            true,
            "shot_smg",
            3,
            "shot_smg_far",
            "reload_rifle_0",
        ),
    ),
    // The ship's line (`tools/models`: the Veil guns), each with its own
    // model; their sounds are ours (tools/sfx.mjs).
    (
        arms::SWORD,
        g(
            "sword",
            1.01,
            0.2475,
            -0.0529,
            0.6,
            0.1617,
            1.0,
            false,
            "sword_swing",
            2,
            "sword_swing",
            "sword_on_0",
        ),
    ),
    (
        arms::CARBINE,
        g(
            "carbine",
            0.954,
            0.304,
            0.0498,
            0.6,
            0.0736,
            1.0,
            true,
            "shot_carbine",
            3,
            "shot_carbine_far",
            "reload_rifle_0",
        ),
    ),
    (
        arms::BEAM_RIFLE,
        g(
            "beam-rifle",
            1.3,
            0.4,
            0.073,
            0.6,
            -0.0212,
            0.82,
            true,
            "shot_beam",
            3,
            "shot_beam_far",
            "vent_0",
        ),
    ),
    (
        arms::ION_RIFLE,
        g(
            "ion-rifle",
            0.616,
            0.3701,
            0.0547,
            0.6,
            0.0329,
            1.0,
            true,
            "shot_ion",
            3,
            "shot_ion_far",
            "vent_0",
        ),
    ),
    (
        arms::ION_PISTOL,
        g(
            "ion-pistol",
            0.366,
            0.123,
            0.0312,
            0.6,
            0.0284,
            1.0,
            false,
            "shot_ionp",
            3,
            "shot_ionp_far",
            "vent_0",
        ),
    ),
    (
        arms::SHARDS,
        g(
            "shards",
            0.577,
            0.5806,
            0.0327,
            0.6,
            0.0291,
            1.0,
            true,
            "shot_shards",
            3,
            "shot_shards_far",
            "reload_rifle_0",
        ),
    ),
    (
        arms::LAUNCHER,
        g(
            "launcher",
            0.905,
            0.3503,
            0.0882,
            0.4133,
            0.0563,
            1.0,
            true,
            "shot_launcher",
            3,
            "shot_launcher_far",
            "reload_shotgun_0",
        ),
    ),
    (
        arms::CANNON,
        g(
            "cannon",
            1.25,
            0.44,
            0.1085,
            0.4143,
            0.03,
            0.84,
            true,
            "shot_cannon",
            3,
            "shot_cannon_far",
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
        Transform::from_xyz(0.0, g.height * (1.0 - g.grip), g.rise),
        Visibility::default(),
        ChildOf(scaler),
    ));
    Some((pivot, scene))
}

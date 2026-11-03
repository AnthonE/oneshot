//! The models: Triple T and the guns (`assets/models`, made from
//! `web/public/models` by `tools/glb`), and how each gun is held, after
//! `web/src/art.ts`'s `GUNS` (Gates' grip numbers, `ui/hold.rs`).

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
pub const GUNS: [(u8, Gun); 8] = [
    (
        arms::AR,
        g(
            "ar",
            0.888,
            0.32,
            0.85,
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
            0.957,
            0.36,
            0.85,
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
            0.325,
            0.17,
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
            1.09,
            0.33,
            0.8,
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
            0.949,
            0.3,
            0.85,
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
            1.058,
            0.45,
            0.8,
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
            0.621,
            0.42,
            0.9,
            true,
            "shot_smg",
            3,
            "shot_smg_far",
            "reload_rifle_0",
        ),
    ),
    // The SEEKER holds and sounds like the SMG until it has its own model.
    (
        arms::SEEKER,
        g(
            "smg",
            0.621,
            0.42,
            0.9,
            true,
            "shot_smg",
            3,
            "shot_smg_far",
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

//! Cloudbreak's observatory: every built thing on the island, its tower's
//! three floors, its stairs and its rim walks among them, made in Blender in
//! the map's own coordinates (`tools/models/observatory.py`), one GLB a
//! material at the origin.
//!
//! **Drawn inside what collides.** The sim's boxes under it are `hidden` in
//! `maps/cloudbreak/map.json`; a body stands a little behind the plane they
//! collide on and its frames stand out to it, so a panel's depth is real and
//! nothing a fighter can reach is outside the box. Rounds leave no mark on
//! it (`rock::Bare`, in `level::build`), since a mark on the box's plane may
//! float over a recess. Above reach the tower tapers inside its boxes and
//! its cornices and rings may stand out. Every rung draws the same mesh: a
//! wall is cover.
//!
//! The stone wears the level's `stone` set and weathering through
//! `level::dress_level`, as a box does; its COLOR_0 carries the occlusion
//! and the sand's dust painted in Blender, per vertex. The rings of the crown
//! turn, slowly: nobody reaches them and the sim knows only the mast.

use bevy::gltf::GltfAssetLabel;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::fresnel;
use super::weathering::{Block, Shape};
use crate::level::{strip_material, BoxLook, LevelPart, STONE, STONE_DARK};

/// The `stone` set's tile, m: the UVs the Blender script writes.
pub const TILE_M: f32 = 4.0;
/// Where the crown's rings turn about (`observatory.py`'s `CROWN_AT`).
const CROWN_AT: Vec3 = Vec3::new(0.0, 33.0, 21.0);
/// How fast they turn, radians a second, and what they lean on.
const CROWN_RAD_S: f32 = 0.05;
/// The bronze: brushed and old, not polished (a mirror under the open sky is
/// white).
const BRONZE: Color = Color::linear_rgb(0.17, 0.105, 0.05);
const BRONZE_ROUGH: f32 = 0.48;
const BRONZE_METAL: f32 = 0.55;
/// The lights: the seams' cool white and each side's.
const LIGHT_WHITE: Color = Color::srgb(0.68, 0.86, 1.0);
const LIGHT_RED: Color = Color::srgb(1.0, 0.28, 0.3);
const LIGHT_BLUE: Color = Color::srgb(0.3, 0.55, 1.0);
const LIGHT_ROUGH: f32 = 0.4;

fn path(part: &str) -> String {
    format!("models/isle/observatory_{part}.glb")
}

/// The observatory, waiting for the asset server.
#[derive(Component)]
pub struct Wanted;

/// The rings that turn.
#[derive(Component)]
pub struct Crown;

/// Asks for the observatory on the level being built.
pub fn spawn(commands: &mut Commands) {
    commands.spawn((LevelPart, Wanted));
}

/// Its parts, once asked for. The stone's are a box's to dress
/// (`level::dress_level`), so a level isn't shown before they are.
pub fn dress(
    mut commands: Commands,
    wanted: Query<Entity, With<Wanted>>,
    server: Res<AssetServer>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    for e in &wanted {
        commands.entity(e).despawn();
        let mesh = |part: &str| -> Handle<Mesh> {
            server.load(GltfAssetLabel::Primitive { mesh: 0, primitive: 0 }.from_asset(path(part)))
        };
        // As far as the weathering is concerned, one block over the island
        // with no box's edges: the wear is cut in the mesh.
        let block = Block {
            mid: Vec3::new(0.0, 10.0, 1.0),
            half: Vec3::new(31.0, 21.0, 28.0),
            turn: (1.0, 0.0),
            shape: Shape::Other,
            ground: 0.0,
            run: None,
            hidden: 0,
            own_value: false,
        };
        for (part, look) in [("stone", STONE), ("stone_dark", STONE_DARK)] {
            let until = mats.add(StandardMaterial::default());
            commands.spawn((
                LevelPart,
                Mesh3d(mesh(part)),
                MeshMaterial3d(until),
                Transform::default(),
                BoxLook(look),
                block,
            ));
        }
        let bronze = mats.add(StandardMaterial {
            base_color: BRONZE,
            perceptual_roughness: BRONZE_ROUGH,
            metallic: BRONZE_METAL,
            reflectance: fresnel::DIELECTRIC,
            ..default()
        });
        commands.spawn((
            LevelPart,
            Mesh3d(mesh("bronze")),
            MeshMaterial3d(bronze.clone()),
            Transform::default(),
        ));
        commands.spawn((
            LevelPart,
            Crown,
            Mesh3d(mesh("crown")),
            MeshMaterial3d(bronze),
            Transform::default(),
        ));
        for (part, c) in [
            ("light_white", LIGHT_WHITE),
            ("light_red", LIGHT_RED),
            ("light_blue", LIGHT_BLUE),
        ] {
            commands.spawn((
                LevelPart,
                Mesh3d(mesh(part)),
                MeshMaterial3d(mats.add(strip_material(c, LIGHT_ROUGH))),
                Transform::default(),
                NotShadowCaster,
            ));
        }
    }
}

/// Turns the crown's rings about their middle.
pub fn turn(time: Res<Time>, mut rings: Query<&mut Transform, With<Crown>>) {
    let r = Quat::from_rotation_y(time.elapsed_secs() * CROWN_RAD_S);
    for mut t in &mut rings {
        t.rotation = r;
        t.translation = CROWN_AT - r * CROWN_AT;
    }
}

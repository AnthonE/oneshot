//! A map's built things, made in Blender in the map's own coordinates, one GLB a
//! material at the origin (`tools/models/observatory.py`, `cistern.py`, and the
//! rest on `arch.py`'s kit).
//!
//! **Drawn inside what collides.** The sim's solids under it are `hidden` in the
//! map's JSON; a body stands a little behind the plane they collide on and its
//! frames stand out to it, so a panel's depth is real and nothing a fighter can
//! reach is outside the box. Rounds leave no mark on it (`rock::Bare`, in
//! `level::build`), since a mark on the box's plane may float over a recess.
//! Above reach a structure may taper inside its boxes and its cornices may stand
//! out. Every rung draws the same mesh: a wall is cover.
//!
//! A part wears a box's set and weathering through `level::dress_level`, its
//! COLOR_0 carrying the occlusion and the tone painted in Blender per vertex; or
//! it is metal, or light, or one of the water's parts (`pool.rs`). What turns
//! nobody reaches, and the sim knows only what it turns about.

use bevy::gltf::GltfAssetLabel;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::fresnel;
use super::weathering::{Block, Shape};
use crate::level::{
    strip_material, BoxLook, LevelPart, BLAST, BRICK, MARBLE, ROAD, RUST, SNOW, STONE, STONE_DARK,
};

/// The `stone` set's tile, m: the UVs the Blender kit writes (`arch.py`'s `TILE_M`).
pub const TILE_M: f32 = 4.0;
/// The other sets', `arch.py`'s `TILES`.
pub const BRICK_TILE_M: f32 = 1.7;
pub const MARBLE_TILE_M: f32 = 3.0;
pub const ROAD_TILE_M: f32 = 5.0;
pub const BLAST_TILE_M: f32 = 3.6;
pub const SNOW_TILE_M: f32 = 6.0;
pub const RUST_TILE_M: f32 = 2.4;
/// The bronze: brushed and old, not polished (a mirror under the open sky is
/// white).
const BRONZE: Color = Color::linear_rgb(0.17, 0.105, 0.05);
const BRONZE_ROUGH: f32 = 0.48;
const BRONZE_METAL: f32 = 0.55;
/// The lights: the seams' cool white and each side's.
const LIGHT_WHITE: Color = Color::srgb(0.68, 0.86, 1.0);
const LIGHT_AMBER: Color = Color::srgb(1.0, 0.62, 0.28);
const LIGHT_RED: Color = Color::srgb(1.0, 0.28, 0.3);
const LIGHT_BLUE: Color = Color::srgb(0.3, 0.55, 1.0);
const LIGHT_ROUGH: f32 = 0.4;
/// A shaft of light down through the air: over the frame, never a wall.
const SHAFT: Color = Color::linear_rgba(0.5, 0.68, 0.95, 1.0);

/// What a part wears.
#[derive(Clone, Copy)]
pub enum Paint {
    /// A box's look, its set and weathering.
    Look(u8),
    /// Colour, roughness, metal.
    Metal(Color, f32, f32),
    /// Glowing.
    Light(Color),
    /// Added over the frame, its brightness in COLOR_0.
    Shaft(Color),
    /// The still water's sheet (`pool.rs`).
    Water,
    /// The architecture over the water, upside down under it, in a look's
    /// photograph (`pool.rs`); none: its COLOR_0 alone (the lights).
    Reflected(Option<u8>),
}

/// One GLB of a set.
pub struct Part {
    pub name: &'static str,
    pub paint: Paint,
    /// Whether it turns about the set's [`Set::turns`].
    pub turns: bool,
}

const fn part(name: &'static str, paint: Paint) -> Part {
    Part {
        name,
        paint,
        turns: false,
    }
}

/// How a map's lifts look (`level::build`): its pad's colour and glow, and its
/// beam, added over the frame.
#[derive(Clone, Copy)]
pub struct Lift {
    pub pad: Color,
    pub glow: LinearRgba,
    pub beam: Color,
}

/// One map's built things.
pub struct Set {
    /// `models/<dir>/<prefix>_<part>.glb`.
    pub dir: &'static str,
    pub prefix: &'static str,
    pub parts: &'static [Part],
    /// As far as the weathering is concerned, one block over the map with no
    /// box's edges: the wear is cut in the mesh. Its middle and half size.
    pub block: (Vec3, Vec3),
    /// Where what turns turns about, and how fast, radians a second.
    pub turns: (Vec3, f32),
    /// Its lifts' look, if not the game's own.
    pub lift: Option<Lift>,
}

const BRONZE_PAINT: Paint = Paint::Metal(BRONZE, BRONZE_ROUGH, BRONZE_METAL);

const OBSERVATORY: Set = Set {
    dir: "isle",
    prefix: "observatory",
    parts: &[
        part("stone", Paint::Look(STONE)),
        part("stone_dark", Paint::Look(STONE_DARK)),
        part("bronze", BRONZE_PAINT),
        Part {
            name: "crown",
            paint: BRONZE_PAINT,
            turns: true,
        },
        part("light_white", Paint::Light(LIGHT_WHITE)),
        part("light_red", Paint::Light(LIGHT_RED)),
        part("light_blue", Paint::Light(LIGHT_BLUE)),
    ],
    block: (Vec3::new(0.0, 10.0, 1.0), Vec3::new(31.0, 21.0, 28.0)),
    // `observatory.py`'s `CROWN_AT`.
    turns: (Vec3::new(0.0, 33.0, 21.0), 0.05),
    lift: None,
};

const CISTERN: Set = Set {
    dir: "cistern",
    prefix: "cistern",
    parts: &[
        part("stone", Paint::Look(STONE)),
        part("stone_dark", Paint::Look(STONE_DARK)),
        part("brick", Paint::Look(BRICK)),
        part("marble", Paint::Look(MARBLE)),
        part("bronze", BRONZE_PAINT),
        part("light_white", Paint::Light(LIGHT_WHITE)),
        part("light_amber", Paint::Light(LIGHT_AMBER)),
        part("light_red", Paint::Light(LIGHT_RED)),
        part("light_blue", Paint::Light(LIGHT_BLUE)),
        part("shaft", Paint::Shaft(SHAFT)),
        part("water", Paint::Water),
        part("reflect_stone", Paint::Reflected(Some(STONE))),
        part("reflect_brick", Paint::Reflected(Some(BRICK))),
        part("reflect_marble", Paint::Reflected(Some(MARBLE))),
        part("reflect_light", Paint::Reflected(None)),
    ],
    block: (Vec3::new(0.0, 5.0, 0.0), Vec3::new(24.0, 6.0, 24.0)),
    turns: (Vec3::ZERO, 0.0),
    // Up the same light as its oculi's, so a lift reads and a stone room stays one.
    lift: Some(Lift {
        pad: Color::srgb(0.5, 0.62, 0.72),
        glow: LinearRgba::rgb(0.5, 0.68, 0.9),
        beam: Color::srgba(0.5, 0.68, 0.95, 0.075),
    }),
};

/// The built set of map `id`, if it has one.
pub fn of(id: u8) -> Option<&'static Set> {
    match id {
        2 => Some(&CISTERN),
        7 => Some(&OBSERVATORY),
        _ => None,
    }
}

/// A set, waiting for the asset server.
#[derive(Component)]
pub struct Wanted(&'static Set);

/// What turns, about where and how fast.
#[derive(Component)]
pub struct Turns(Vec3, f32);

/// Asks for map `id`'s built things on the level being built.
pub fn spawn(commands: &mut Commands, id: u8) {
    if let Some(set) = of(id) {
        commands.spawn((LevelPart, Wanted(set)));
    }
}

/// Its parts, once asked for. A look's are a box's to dress
/// (`level::dress_level`), so a level isn't shown before they are.
pub fn dress(
    mut commands: Commands,
    wanted: Query<(Entity, &Wanted)>,
    server: Res<AssetServer>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    for (e, w) in &wanted {
        commands.entity(e).despawn();
        let set = w.0;
        for p in set.parts {
            let mesh: Handle<Mesh> = server.load(
                GltfAssetLabel::Primitive { mesh: 0, primitive: 0 }
                    .from_asset(format!("models/{}/{}_{}.glb", set.dir, set.prefix, p.name)),
            );
            let mut c = commands.spawn((LevelPart, Mesh3d(mesh), Transform::default()));
            match p.paint {
                Paint::Look(look) => {
                    let until = mats.add(StandardMaterial::default());
                    c.insert((
                        MeshMaterial3d(until),
                        BoxLook(look),
                        Block {
                            mid: set.block.0,
                            half: set.block.1,
                            turn: (1.0, 0.0),
                            shape: Shape::Other,
                            ground: 0.0,
                            run: None,
                            hidden: 0,
                            own_value: false,
                        },
                    ));
                }
                Paint::Metal(base_color, perceptual_roughness, metallic) => {
                    c.insert(MeshMaterial3d(mats.add(StandardMaterial {
                        base_color,
                        perceptual_roughness,
                        metallic,
                        reflectance: fresnel::DIELECTRIC,
                        ..default()
                    })));
                }
                Paint::Light(colour) => {
                    c.insert((MeshMaterial3d(mats.add(strip_material(colour, LIGHT_ROUGH))), NotShadowCaster));
                }
                Paint::Shaft(colour) => {
                    c.insert((
                        MeshMaterial3d(mats.add(StandardMaterial {
                            base_color: colour,
                            alpha_mode: AlphaMode::Add,
                            unlit: true,
                            double_sided: true,
                            cull_mode: None,
                            ..default()
                        })),
                        NotShadowCaster,
                        bevy::light::NotShadowReceiver,
                    ));
                }
                Paint::Water => {
                    c.insert((super::pool::WaterWanted, NotShadowCaster));
                }
                Paint::Reflected(look) => {
                    c.insert((super::pool::ReflectWanted(look), NotShadowCaster, bevy::light::NotShadowReceiver));
                }
            }
            if p.turns {
                c.insert(Turns(set.turns.0, set.turns.1));
            }
        }
    }
}

/// Turns what turns about its middle.
pub fn turn(time: Res<Time>, mut parts: Query<(&mut Transform, &Turns)>) {
    for (mut t, turns) in &mut parts {
        let r = Quat::from_rotation_y(time.elapsed_secs() * turns.1);
        t.rotation = r;
        t.translation = turns.0 - r * turns.0;
    }
}

/// A set's tile, m, by look (`level::dress`).
pub const fn tile(look: u8) -> f32 {
    match look {
        BRICK => BRICK_TILE_M,
        MARBLE => MARBLE_TILE_M,
        ROAD => ROAD_TILE_M,
        BLAST => BLAST_TILE_M,
        SNOW => SNOW_TILE_M,
        RUST => RUST_TILE_M,
        _ => TILE_M,
    }
}

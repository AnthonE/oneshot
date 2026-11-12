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
    strip_material, BoxLook, LevelPart, BLAST, BLAST_DARK, BRICK, DARK, HULL, MARBLE, PLATE, ROAD, RUST, SNOW,
    STEEL_BLUE, STEEL_RED, STONE, STONE_DARK, TREAD,
};

/// The `stone` set's tile, m: the UVs the Blender kit writes (`arch.py`'s `TILE_M`).
pub const TILE_M: f32 = 4.0;
/// The other sets', `arch.py`'s `TILES`.
pub const BRICK_TILE_M: f32 = 1.7;
pub const MARBLE_TILE_M: f32 = 3.0;
pub const ROAD_TILE_M: f32 = 5.0;
pub const BLAST_TILE_M: f32 = 3.6;
pub const SNOW_TILE_M: f32 = 2.6;
/// The snow's set is cut this many of those tiles across (`tools/snow.py`'s
/// `TILE_M`): its swells want the room, and a floor of it is fifty metres.
pub const SNOW_LAID: f32 = 2.0;
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
const LIGHT_SODIUM: Color = Color::srgb(1.0, 0.58, 0.22);
const LIGHT_PINK: Color = Color::srgb(1.0, 0.16, 0.56);
const LIGHT_CYAN: Color = Color::srgb(0.16, 0.86, 1.0);
const LIGHT_WARM: Color = Color::srgb(1.0, 0.72, 0.42);
const LIGHT_COLD: Color = Color::srgb(0.7, 0.84, 1.0);
const LIGHT_ROUGH: f32 = 0.4;
/// A window across a city, over a strip's glow.
const WINDOW_GLOW: f32 = 0.34;
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
    /// Glowing, dimmed by this: far windows.
    Dim(Color, f32),
    /// Lying on a floor: this colour, how much of it in COLOR_0's alpha.
    Decal(Color),
    /// Standing water on a road, its rim fading out in COLOR_0's alpha.
    Puddle,
    /// Far outside: its COLOR_0 alone, never fogged or lit.
    Far,
    /// Far outside: this picture, never fogged or lit.
    Picture(&'static str),
    /// A pane between here and out there.
    Glass,
    /// The still water's sheet (`pool.rs`).
    Water,
    /// A wet road in a look's photograph, over its reflection (`pool.rs`).
    Wet(u8),
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
    /// Or about a line of its own: a point on it, its way, radians a second.
    pub spins: Option<(Vec3, Vec3, f32)>,
}

const fn part(name: &'static str, paint: Paint) -> Part {
    Part {
        name,
        paint,
        turns: false,
        spins: None,
    }
}

const fn spinning(name: &'static str, paint: Paint, at: Vec3, way: Vec3, rate: f32) -> Part {
    Part {
        name,
        paint,
        turns: false,
        spins: Some((at, way, rate)),
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
            spins: None,
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
        beam: Color::srgba(0.5, 0.68, 0.95, 0.04),
    }),
};

const OVERPASS: Set = Set {
    dir: "overpass",
    prefix: "overpass",
    parts: &[
        part("road", Paint::Look(ROAD)),
        part("blast", Paint::Look(BLAST)),
        part("blast_dark", Paint::Look(BLAST_DARK)),
        part("rust", Paint::Look(RUST)),
        part("paint", Paint::Metal(Color::srgb(0.7, 0.7, 0.64), 0.55, 0.0)),
        part("paint_y", Paint::Metal(Color::srgb(0.72, 0.55, 0.14), 0.55, 0.0)),
        part("steel", Paint::Metal(Color::srgb(0.36, 0.37, 0.39), 0.5, 0.6)),
        part("puddle", Paint::Puddle),
        part("shaft", Paint::Shaft(Color::WHITE)),
        part("light_sodium", Paint::Light(LIGHT_SODIUM)),
        part("light_red", Paint::Light(LIGHT_RED)),
        part("light_blue", Paint::Light(LIGHT_BLUE)),
        part("light_pink", Paint::Light(LIGHT_PINK)),
        part("light_cyan", Paint::Light(LIGHT_CYAN)),
        part("light_warm", Paint::Dim(LIGHT_WARM, WINDOW_GLOW)),
        part("light_cold", Paint::Dim(LIGHT_COLD, WINDOW_GLOW)),
        part("city", Paint::Far),
        part("city_lights", Paint::Far),
        part("flood", Paint::Water),
        part("reflect_light", Paint::Reflected(None)),
        part("reflect_city", Paint::Reflected(None)),
        part("wet_road", Paint::Wet(ROAD)),
        part("wet_blast", Paint::Reflected(Some(BLAST))),
        part("wet_rust", Paint::Reflected(Some(RUST))),
        part("wet_light", Paint::Reflected(None)),
    ],
    block: (Vec3::new(0.0, 4.0, 0.0), Vec3::new(40.0, 8.0, 40.0)),
    turns: (Vec3::ZERO, 0.0),
    lift: None,
};

const DOCK: Set = Set {
    dir: "bulkhead",
    prefix: "dock",
    parts: &[
        part("hull", Paint::Look(HULL)),
        part("rust", Paint::Look(RUST)),
        part("dark", Paint::Look(DARK)),
        part("plate", Paint::Look(PLATE)),
        part("tread", Paint::Look(TREAD)),
        part("red", Paint::Look(STEEL_RED)),
        part("blue", Paint::Look(STEEL_BLUE)),
        part("pipe", Paint::Metal(Color::srgb(0.3, 0.31, 0.33), 0.45, 0.7)),
        part("crane", Paint::Metal(Color::srgb(0.68, 0.5, 0.1), 0.55, 0.2)),
        part("paint_y", Paint::Metal(Color::srgb(0.72, 0.55, 0.14), 0.6, 0.0)),
        part("paint_k", Paint::Metal(Color::srgb(0.03, 0.03, 0.03), 0.6, 0.0)),
        part("light_white", Paint::Light(LIGHT_WHITE)),
        part("light_amber", Paint::Light(LIGHT_AMBER)),
        part("light_cyan", Paint::Light(LIGHT_CYAN)),
        part("light_red", Paint::Light(LIGHT_RED)),
        part("light_blue", Paint::Light(LIGHT_BLUE)),
        part("light_cold", Paint::Dim(LIGHT_COLD, WINDOW_GLOW)),
        part("paint_w", Paint::Metal(Color::srgb(0.66, 0.66, 0.62), 0.6, 0.0)),
        part("ship", Paint::Far),
        part("ship_dark", Paint::Far),
        part("space", Paint::Far),
        part("stars", Paint::Far),
        part("ring", Paint::Far),
        part("planet", Paint::Picture("textures/planet.jpg")),
        part("shaft", Paint::Shaft(Color::WHITE)),
    ],
    block: (Vec3::new(0.0, 5.0, 0.0), Vec3::new(25.0, 6.0, 17.0)),
    turns: (Vec3::ZERO, 0.0),
    lift: None,
};

const RANGE: Set = Set {
    dir: "proving",
    prefix: "range",
    parts: &[
        part("blast", Paint::Look(BLAST)),
        part("blast_dark", Paint::Look(BLAST_DARK)),
        part("snow", Paint::Look(SNOW)),
        part("rust", Paint::Look(RUST)),
        part("steel", Paint::Metal(Color::srgb(0.34, 0.35, 0.37), 0.5, 0.6)),
        part("paint_y", Paint::Metal(Color::srgb(0.72, 0.55, 0.14), 0.6, 0.0)),
        part("paint_k", Paint::Metal(Color::srgb(0.03, 0.03, 0.03), 0.6, 0.0)),
        part("paint_w", Paint::Metal(Color::srgb(0.66, 0.66, 0.62), 0.6, 0.0)),
        part("decal", Paint::Decal(Color::srgb(0.02, 0.02, 0.025))),
        part("track", Paint::Decal(Color::srgb(0.2, 0.22, 0.28))),
        part("peaks", Paint::Metal(Color::WHITE, 0.85, 0.0)),
        part("shaft", Paint::Shaft(Color::WHITE)),
        part("light_amber", Paint::Light(LIGHT_AMBER)),
        part("light_white", Paint::Light(LIGHT_WHITE)),
        part("light_red", Paint::Light(LIGHT_RED)),
        part("light_blue", Paint::Light(LIGHT_BLUE)),
    ],
    block: (Vec3::new(0.0, 4.0, 0.0), Vec3::new(25.0, 6.0, 25.0)),
    turns: (Vec3::ZERO, 0.0),
    lift: None,
};

/// Pithead's paint: what the Blender kit colours per vertex (`pithead.py`'s
/// `tone`: each side's cladding, the steel and the rime the wind builds on it)
/// over white, and the ice.
/// Paint on steel is rough and not metal: a gloss puts the sky on near-black
/// steel as a teal sheen.
const TONED: Paint = Paint::Metal(Color::WHITE, 0.72, 0.0);
const TONED_METAL: Paint = Paint::Metal(Color::WHITE, 0.42, 0.55);
const ICE: Color = Color::srgb(0.72, 0.85, 0.96);
const SNOW_DRIFT: Color = Color::srgb(0.84, 0.88, 0.95);
/// Snow trodden to grey slush, and the lamps' warm light lying on the snow.
const SLUSH: Color = Color::srgb(0.38, 0.39, 0.41);
const POOL: Color = Color::srgb(1.0, 0.62, 0.3);
/// The sheave wheels' axles (`pithead.py`'s `SHEAVE`), one a side, turned
/// half about the middle; each turns as its winding engine hauls.
const SHEAVE_RED: Vec3 = Vec3::new(-15.6, 18.6, 0.0);
const SHEAVE_BLUE: Vec3 = Vec3::new(15.6, 18.6, 0.0);
const SHEAVE_RATE: f32 = 0.35;

const PITHEAD: Set = Set {
    dir: "pithead",
    prefix: "pithead",
    parts: &[
        part("blast", Paint::Look(BLAST)),
        part("blast_dark", Paint::Look(BLAST_DARK)),
        part("floor", Paint::Look(BLAST)),
        part("tread", Paint::Look(TREAD)),
        part("snow", Paint::Look(SNOW)),
        part("clad", TONED),
        part("clad_grey", TONED),
        part("lining", TONED),
        part("steel", TONED),
        part("steel_box", TONED),
        part("rusty", TONED),
        part("team_red", TONED),
        part("team_blue", TONED),
        part("galv", TONED_METAL),
        part("cable", TONED_METAL),
        part("crag", Paint::Metal(Color::WHITE, 0.85, 0.0)),
        part("peaks", Paint::Metal(Color::WHITE, 0.85, 0.0)),
        part("ice", Paint::Metal(ICE, 0.1, 0.0)),
        part("drift", Paint::Decal(SNOW_DRIFT)),
        part("track", Paint::Decal(SLUSH)),
        part("pool", Paint::Shaft(POOL)),
        part("paint_y", Paint::Metal(Color::srgb(0.72, 0.55, 0.14), 0.6, 0.0)),
        part("paint_k", Paint::Metal(Color::srgb(0.03, 0.03, 0.03), 0.6, 0.0)),
        part("window", Paint::Dim(LIGHT_WARM, 0.8)),
        part("light_sodium", Paint::Light(LIGHT_SODIUM)),
        part("light_red", Paint::Light(LIGHT_RED)),
        part("light_blue", Paint::Light(LIGHT_BLUE)),
        spinning("wheel_red", TONED, SHEAVE_RED, Vec3::Z, SHEAVE_RATE),
        spinning("wheel_blue", TONED, SHEAVE_BLUE, Vec3::Z, -SHEAVE_RATE),
    ],
    block: (Vec3::new(0.0, 4.0, 0.0), Vec3::new(22.0, 8.0, 15.0)),
    turns: (Vec3::ZERO, 0.0),
    lift: None,
};

/// The built set of map `id`, if it has one.
pub fn of(id: u8) -> Option<&'static Set> {
    match id {
        0 => Some(&RANGE),
        1 => Some(&OVERPASS),
        2 => Some(&CISTERN),
        5 => Some(&DOCK),
        7 => Some(&OBSERVATORY),
        8 => Some(&PITHEAD),
        _ => None,
    }
}

/// A set, waiting for the asset server.
#[derive(Component)]
pub struct Wanted(&'static Set);

/// What turns: a point on the line it turns about, the line's way, how fast.
#[derive(Component)]
pub struct Turns(Vec3, Vec3, f32);

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
    mut floors: Query<&mut super::floor_shade::Waits>,
) {
    for (e, w) in &wanted {
        commands.entity(e).despawn();
        let set = w.0;
        for p in set.parts {
            let mesh: Handle<Mesh> = server.load(
                GltfAssetLabel::Primitive { mesh: 0, primitive: 0 }
                    .from_asset(format!("models/{}/{}_{}.glb", set.dir, set.prefix, p.name)),
            );
            // A light lights the floor under it (`look/floor_shade.rs`).
            if let Paint::Light(colour) = p.paint {
                for mut w in &mut floors {
                    w.light(mesh.clone(), LinearRgba::from(colour) * crate::level::STRIP_GLOW);
                }
            }
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
                    c.insert((
                        MeshMaterial3d(mats.add(strip_material(colour, LIGHT_ROUGH))),
                        NotShadowCaster,
                    ));
                }
                Paint::Shaft(colour) => {
                    c.insert((
                        MeshMaterial3d(mats.add(StandardMaterial {
                            base_color: colour,
                            alpha_mode: AlphaMode::Add,
                            unlit: true,
                            fog_enabled: false,
                            double_sided: true,
                            cull_mode: None,
                            ..default()
                        })),
                        NotShadowCaster,
                        bevy::light::NotShadowReceiver,
                    ));
                }
                Paint::Decal(colour) => {
                    c.insert((
                        MeshMaterial3d(mats.add(StandardMaterial {
                            base_color: colour,
                            alpha_mode: AlphaMode::Blend,
                            perceptual_roughness: 0.9,
                            ..default()
                        })),
                        NotShadowCaster,
                    ));
                }
                Paint::Puddle => {
                    c.insert((
                        MeshMaterial3d(mats.add(StandardMaterial {
                            base_color: Color::srgb(0.012, 0.014, 0.017),
                            alpha_mode: AlphaMode::Blend,
                            perceptual_roughness: 0.08,
                            reflectance: fresnel::DIELECTRIC,
                            ..default()
                        })),
                        NotShadowCaster,
                    ));
                }
                Paint::Far => {
                    c.insert((
                        MeshMaterial3d(mats.add(StandardMaterial {
                            base_color: Color::WHITE,
                            unlit: true,
                            fog_enabled: false,
                            ..default()
                        })),
                        NotShadowCaster,
                        bevy::light::NotShadowReceiver,
                    ));
                }
                Paint::Picture(file) => {
                    c.insert((
                        MeshMaterial3d(mats.add(StandardMaterial {
                            base_color_texture: Some(server.load(file)),
                            unlit: true,
                            fog_enabled: false,
                            ..default()
                        })),
                        NotShadowCaster,
                        bevy::light::NotShadowReceiver,
                    ));
                }
                Paint::Glass => {
                    c.insert((
                        MeshMaterial3d(mats.add(StandardMaterial {
                            base_color: Color::srgba(0.55, 0.72, 0.85, 0.05),
                            alpha_mode: AlphaMode::Blend,
                            perceptual_roughness: 0.06,
                            fog_enabled: false,
                            ..default()
                        })),
                        NotShadowCaster,
                        bevy::light::NotShadowReceiver,
                    ));
                }
                Paint::Dim(colour, k) => {
                    let mut m = strip_material(colour, LIGHT_ROUGH);
                    m.emissive *= k;
                    c.insert((MeshMaterial3d(mats.add(m)), NotShadowCaster));
                }
                Paint::Water => {
                    c.insert((super::pool::WaterWanted, NotShadowCaster));
                }
                Paint::Wet(look) => {
                    c.insert((super::pool::WetWanted(look), NotShadowCaster));
                }
                Paint::Reflected(look) => {
                    c.insert((
                        super::pool::ReflectWanted(look),
                        NotShadowCaster,
                        bevy::light::NotShadowReceiver,
                    ));
                }
            }
            if p.turns {
                c.insert(Turns(set.turns.0, Vec3::Y, set.turns.1));
            }
            if let Some((at, way, rate)) = p.spins {
                c.insert(Turns(at, way, rate));
            }
        }
        // Every light the set has is named: its floors can be laid.
        for mut w in &mut floors {
            w.built();
        }
    }
}

/// Turns what turns about its line.
pub fn turn(time: Res<Time>, mut parts: Query<(&mut Transform, &Turns)>) {
    for (mut t, turns) in &mut parts {
        let r = Quat::from_axis_angle(turns.1, time.elapsed_secs() * turns.2);
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

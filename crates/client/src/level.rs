//! The level, from the sim's own map: its boxes, its solids (round, turned
//! and ramps: `sim::solid`), its ground, its launchers and its light. What
//! you see is what you collide with and what stops bullets.
//! Rebuilt whole on each welcome (a cup's next heat may be another map).
//!
//! The surfaces over that shape (`look/weathering.rs`, `look/rock.rs`,
//! `look/cliffs.rs`) keep it: a box is weathered in its shading only, a
//! rock box's faces are faceted in their shading only and stay its planes,
//! the terrain's rock skin lies within centimetres of the sim's ground,
//! and only a rock box nobody can reach (out of bounds, clear of its edge)
//! is drawn as a rock mass inside it.

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use sim::terrain::{TerrainDef, HEIGHT_Q};

use crate::look::fresnel;
use crate::look::rock::{self, Mass};
use crate::look::weathering::{self, Block, Run, Shape, Surface, RUN};

/// Everything a level spawned, to take down with it.
#[derive(Component)]
pub struct LevelPart;

/// The ground, waiting for its textured material (look::ground), with the
/// sea's height for its wet band (`f32::NEG_INFINITY`: no sea).
#[derive(Component)]
pub struct GroundMesh {
    pub sea: f32,
    /// An island's (`Land::Isle`): its own pale sandstone tints.
    pub isle: bool,
}

/// A box by its look, waiting for its textured material.
#[derive(Component)]
pub struct BoxLook(pub u8);

/// Given its textured material.
#[derive(Component)]
pub struct Dressed;

/// A block of grass, waiting for its atlas (`look/grass.rs`).
#[derive(Component)]
pub struct GrassPart;

/// A tree's drawn parts (`look/trees.rs`).
#[derive(Component)]
pub struct TreePart;

/// A block of ground clutter, waiting for its material (`look/clutter.rs`).
#[derive(Component)]
pub struct ClutterPart;

/// A material's roughness over the photograph's, which averages about 0.6.
const ROUGH_SCALE: f32 = 1.5;

/// The arena floor's look (not a map look: a flat map's ground).
const FLOOR: u8 = 255;

/// The ship's looks (tools/maps/compile.mjs LOOKS): its deck, its windows,
/// a box drawn as a vault or a rib, and one not drawn at all.
const DECK: u8 = 16;
const GOLD: u8 = 17;
const VAULT: u8 = 18;
const HIDDEN: u8 = 19;
const RIB: u8 = 22;
/// A tree's looks: grown by look::trees at the solid's foot, never drawn
/// as a cylinder.
const PALM: u8 = 23;
const DEAD_TREE: u8 = 24;
/// The hull's plates repeat over this many metres.
const HULL_TILE_M: f32 = 5.5;

/// A box's texture set, the metres it repeats over, and its tint (the
/// photograph's colour times this).
fn dress(look: u8) -> Option<(&'static str, f32, Color)> {
    Some(match look {
        // The arenas are steel: their walls a mid gunmetal plate, their
        // cover a paler one, their dark pieces a darker, a side's pieces
        // the plate under its paint. The floor keeps its big panels: tread
        // plate over forty metres is a field of noise.
        0 => ("steel", 5.0, Color::linear_rgb(0.62, 0.62, 0.64)),
        1 => ("steel", 4.0, Color::linear_rgb(0.86, 0.86, 0.87)),
        3 => ("steel", 4.0, Color::linear_rgb(1.5, 0.3, 0.24)),
        4 => ("steel", 4.0, Color::linear_rgb(0.26, 0.44, 1.35)),
        5 => ("steel", 4.0, Color::linear_rgb(0.3, 0.3, 0.32)),
        ROCK => ("rock", 2.5, Color::linear_rgb(1.45, 1.6, 2.1)),
        7 => ("steel", 4.0, Color::linear_rgb(0.55, 0.55, 0.57)),
        11 => ("steel", 4.0, Color::linear_rgb(1.0, 1.0, 1.0)),
        FLOOR => ("panel", 6.0, Color::srgb(0.34, 0.36, 0.39)),
        // The ship: its hull's plates are near white, the look is their
        // colour (pearl purple, its dark hull, red's and blue's).
        12 => ("alien", HULL_TILE_M, Color::srgb(0.47, 0.33, 0.68)),
        13 | HUSK | FONT => ("alien", HULL_TILE_M, Color::srgb(0.3, 0.22, 0.45)),
        14 | HUSK_RED => ("alien", HULL_TILE_M, Color::srgb(0.66, 0.19, 0.32)),
        15 | HUSK_BLUE => ("alien", HULL_TILE_M, Color::srgb(0.25, 0.37, 0.68)),
        DECK => ("deck", 8.0, Color::linear_rgb(0.55, 0.55, 0.55)),
        GOLD => ("gold", 3.0, Color::WHITE),
        // A base's metal: plates over a metre across (the set is four to
        // its tile) in a mid steel, the same under a side's paint, and
        // tread plate.
        STEEL => ("steel", 5.0, Color::linear_rgb(0.7, 0.7, 0.72)),
        STEEL_RED => ("steel", 5.0, Color::linear_rgb(1.5, 0.3, 0.24)),
        STEEL_BLUE => ("steel", 5.0, Color::linear_rgb(0.26, 0.44, 1.35)),
        TREAD => ("tread", 2.0, Color::linear_rgb(1.25, 1.25, 1.3)),
        // The masonry: cool, against the warm sand and the warm sun on it.
        STONE => ("stone", crate::look::built::TILE_M, Color::linear_rgb(1.0, 1.0, 1.02)),
        STONE_DARK => ("stone", crate::look::built::TILE_M, Color::linear_rgb(0.46, 0.46, 0.48)),
        BRICK => (
            "brick",
            crate::look::built::tile(BRICK),
            Color::linear_rgb(0.9, 0.86, 0.82),
        ),
        MARBLE => ("marble", crate::look::built::tile(MARBLE), Color::WHITE),
        ROAD => (
            "road",
            crate::look::built::tile(ROAD),
            Color::linear_rgb(0.8, 0.8, 0.82),
        ),
        BLAST => (
            "blast",
            crate::look::built::tile(BLAST),
            Color::linear_rgb(0.95, 0.95, 0.97),
        ),
        BLAST_DARK => (
            "blast",
            crate::look::built::tile(BLAST),
            Color::linear_rgb(0.45, 0.45, 0.47),
        ),
        // Its set is cut white (`tools/snow.py`'s `WHITE`).
        SNOW => ("snow", crate::look::built::tile(SNOW), Color::WHITE),
        RUST => ("rust", crate::look::built::tile(RUST), Color::linear_rgb(1.0, 1.0, 1.0)),
        _ => return None,
    })
}

/// A look's texture set, by name (the water's reflection wears it).
pub fn set_of(look: u8) -> Option<&'static str> {
    dress(look).map(|d| d.0)
}

/// A base's metal (tools/maps/compile.mjs LOOKS): its plate, its plate in
/// each side's paint, its tread plate; and a turned box drawn as a pylon.
pub const PYLON: u8 = 25;
const STEEL: u8 = 26;
pub const STEEL_RED: u8 = 27;
pub const STEEL_BLUE: u8 = 28;
pub const TREAD: u8 = 29;
/// The arenas' old steel: mid, dark, plate.
pub const HULL: u8 = 7;
pub const DARK: u8 = 5;
pub const PLATE: u8 = 11;
/// Dressed grey stone, pale and dark, in the concrete set.
pub const STONE: u8 = 30;
pub const STONE_DARK: u8 = 31;
/// The ship's props (tools/maps/compile.mjs LOOKS): a round solid drawn as a
/// model inside it (`look/boulders.rs`), its faceted self until that's in.
pub const HUSK: u8 = 32;
pub const HUSK_RED: u8 = 33;
pub const HUSK_BLUE: u8 = 34;
pub const FONT: u8 = 35;
/// The arenas' built things (`look/built.rs`): old brick, cut marble, asphalt,
/// cast concrete and its dark, snow, flaking steel.
pub const BRICK: u8 = 36;
pub const MARBLE: u8 = 37;
pub const ROAD: u8 = 38;
pub const BLAST: u8 = 39;
pub const BLAST_DARK: u8 = 40;
pub const SNOW: u8 = 41;
pub const RUST: u8 = 42;
/// A turned box drawn as a wreck inside it (`look/boulders.rs`).
pub const WRECK_CAR: u8 = 43;
pub const WRECK_VAN: u8 = 44;

/// The rock look (tools/maps/compile.mjs LOOKS).
const ROCK: u8 = 6;

/// A box longer than this, metres, on a terrain map has its grime follow
/// the ground along it ([`Block::run`]), where the ground under it moves
/// more than [`RUN_STEP_M`].
const RUN_MIN_M: f32 = 6.0;
const RUN_STEP_M: f32 = 0.3;

/// A mass's face is dressed with pieces while more than this share of it
/// shows past the boxes in front of it.
const MASS_OPEN: f32 = 0.05;

fn v3(v: sim::geom::V3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// A dressed look's material before its textures, its wet and its weather:
/// its tint, roughness and metal, and the dielectric F0 it claims
/// (`look/fresnel.rs`). `None` for a look drawn plain.
pub fn box_material(look: u8) -> Option<StandardMaterial> {
    let (set, _, tint) = dress(look)?;
    let (roughness, metallic) = match (look, set) {
        (3 | 4, _) => (0.7, 0.3),
        (0 | 1 | 5 | 7 | 11, _) => (0.62, 0.85),
        (FLOOR, _) => (0.75, 0.55),
        // The ship's sets carry their own roughness and metal.
        (12..=15 | RIB | HUSK..=FONT, _) => (1.0, 0.7),
        (DECK, _) => (1.0, 0.3),
        (GOLD, _) => (1.0, 0.9),
        // Bare plate is metal through and through; paint over it mostly
        // is not; tread plate is worn dull.
        (STEEL, _) => (0.62, 0.85),
        (STEEL_RED | STEEL_BLUE, _) => (0.7, 0.3),
        (TREAD, _) => (0.75, 0.8),
        (STONE | STONE_DARK, _) => (0.88, 0.0),
        (_, "metal") => (0.55, 0.35),
        _ => (0.9, 0.0),
    };
    Some(StandardMaterial {
        base_color: tint,
        perceptual_roughness: roughness,
        metallic,
        reflectance: fresnel::DIELECTRIC,
        ..default()
    })
}

/// What a look is, as it weathers (`look/weathering.rs`).
fn surface(look: u8) -> Surface {
    match look {
        0 | 1 | 3 | 4 | 5 | 7 | 11 | PYLON | STEEL | STEEL_RED | STEEL_BLUE | TREAD => Surface::Plate,
        ROCK => Surface::Rock,
        FLOOR => Surface::Floor,
        12..=15 | DECK | GOLD | RIB | HUSK..=FONT => Surface::Hull,
        STONE | STONE_DARK | BRICK | MARBLE | BLAST | BLAST_DARK | ROAD => Surface::Stone,
        SNOW => Surface::Floor,
        _ => Surface::Concrete,
    }
}

/// The key a map's rock relief is drawn with: one field per map, so
/// neighbouring rock boxes' faces carry on as one rock.
fn rock_key(id: u8) -> u32 {
    (id as u32 + 1).wrapping_mul(0x2545_F491) ^ 0x0c1f_f5ed
}

/// A box's own key (a mass's layout), from where it stands.
fn box_key(mid: Vec3, id: u8) -> u32 {
    let q = |v: f32| (v * 16.0).round() as i32 as u32;
    q(mid.x).wrapping_mul(73_856_093)
        ^ q(mid.y).wrapping_mul(19_349_663)
        ^ q(mid.z).wrapping_mul(83_492_791)
        ^ rock_key(id)
}

/// Faces as the meshes below are built: positions, normals, texture
/// coordinates (in metres; divided by the tile at the end), triangles.
#[derive(Default)]
pub(crate) struct Faces {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

impl Faces {
    /// A flat quad, wound to face `n`, its texture from `uv` of each corner.
    fn quad(&mut self, p: [Vec3; 4], n: Vec3, uv: impl Fn(Vec3) -> [f32; 2]) {
        let base = self.pos.len() as u32;
        let order: [usize; 4] = if (p[1] - p[0]).cross(p[2] - p[0]).dot(n) >= 0.0 {
            [0, 1, 2, 3]
        } else {
            [0, 3, 2, 1]
        };
        for &k in &order {
            self.pos.push(p[k].to_array());
            self.nor.push(n.to_array());
            self.uv.push(uv(p[k]));
        }
        self.idx
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A quad wound to face `n` with each corner's texture coordinates
    /// given (metres, divided by `tile` here).
    pub(crate) fn quad_uv(&mut self, p: [Vec3; 4], n: Vec3, uv: [[f32; 2]; 4], tile: f32) {
        let base = self.pos.len() as u32;
        let order: [usize; 4] = if (p[1] - p[0]).cross(p[2] - p[0]).dot(n) >= 0.0 {
            [0, 1, 2, 3]
        } else {
            [0, 3, 2, 1]
        };
        for &k in &order {
            self.pos.push(p[k].to_array());
            self.nor.push(n.to_array());
            self.uv.push([uv[k][0] / tile, uv[k][1] / tile]);
        }
        self.idx
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A triangle wound to face `n`, its texture laid flat by its plane.
    pub(crate) fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, n: Vec3, tile: f32) {
        let base = self.pos.len() as u32;
        let order: [Vec3; 3] = if (b - a).cross(c - a).dot(n) >= 0.0 {
            [a, b, c]
        } else {
            [a, c, b]
        };
        let an = n.abs();
        for p in order {
            self.pos.push(p.to_array());
            self.nor.push(n.to_array());
            let uv = if an.y > an.x && an.y > an.z {
                [p.x, p.z]
            } else if an.x > an.z {
                [p.z, -p.y]
            } else {
                [p.x, -p.y]
            };
            self.uv.push([uv[0] / tile, uv[1] / tile]);
        }
        self.idx.extend_from_slice(&[base, base + 1, base + 2]);
    }

    pub(crate) fn mesh(self, tile: f32) -> Mesh {
        let uv: Vec<[f32; 2]> = self.uv.iter().map(|u| [u[0] / tile, u[1] / tile]).collect();
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
            .with_inserted_indices(Indices::U32(self.idx));
        let _ = m.generate_tangents();
        m
    }
}

/// How many facets a round solid of radius `r` is drawn with: one every
/// 25 cm or so of its rim. A mark lies on these, not on the true circle
/// (`look/fx/surface.rs`).
pub fn round_sides(r: f32) -> usize {
    ((core::f32::consts::TAU * r / 0.25).ceil() as usize).clamp(24, 128)
}

/// A round solid: its side smooth (a facet every 25 cm or so of its
/// rim: [`round_sides`]), its top a disc; about its own middle, y from 0
/// to `h`.
fn round_mesh(r: f32, h: f32, tile: f32) -> Mesh {
    let n = round_sides(r);
    let mut f = Faces::default();
    let ring = |i: usize| {
        let a = i as f32 / n as f32 * core::f32::consts::TAU;
        (a.cos(), a.sin(), a * r)
    };
    for i in 0..=n {
        let (c, s, u) = ring(i);
        for y in [0.0, h] {
            f.pos.push([c * r, y, s * r]);
            f.nor.push([c, 0.0, s]);
            f.uv.push([u, -y]);
        }
    }
    for i in 0..n as u32 {
        let (a, b) = (2 * i, 2 * i + 2);
        // Outward: bottom, top, next top; bottom, next top, next bottom.
        f.idx.extend_from_slice(&[a, a + 1, b + 1, a, b + 1, b]);
    }
    // The top, a fan.
    let mid = f.pos.len() as u32;
    f.pos.push([0.0, h, 0.0]);
    f.nor.push([0.0, 1.0, 0.0]);
    f.uv.push([0.0, 0.0]);
    for i in 0..=n {
        let (c, s, _) = ring(i);
        f.pos.push([c * r, h, s * r]);
        f.nor.push([0.0, 1.0, 0.0]);
        f.uv.push([c * r, s * r]);
    }
    for i in 0..n as u32 {
        f.idx.extend_from_slice(&[mid, mid + 2 + i, mid + 1 + i]);
    }
    f.mesh(tile)
}

/// A ramp: its slope, its sides and its ends, in world coordinates.
fn ramp_mesh(s: &sim::solid::Solid, tile: f32) -> Option<Mesh> {
    let sim::solid::Shape::Ramp { x0, z0, x1, z1, .. } = s.shape else {
        return None;
    };
    let top = |x: f32, z: f32| Vec3::new(x, s.top_at(x, z), z);
    let bot = |x: f32, z: f32| Vec3::new(x, s.y0, z);
    let mut f = Faces::default();
    let (a, b, c, d) = (top(x0, z0), top(x1, z0), top(x1, z1), top(x0, z1));
    let n = (c - a).cross(b - a).normalize_or(Vec3::Y);
    let n = if n.y < 0.0 { -n } else { n };
    f.quad([a, b, c, d], n, |p| [p.x, p.z]);
    f.quad([bot(x0, z0), bot(x1, z0), bot(x1, z1), bot(x0, z1)], Vec3::NEG_Y, |p| {
        [p.x, p.z]
    });
    // Each side from the bottom to the slope over it.
    let sides = [
        ((x0, z0), (x1, z0), Vec3::NEG_Z),
        ((x1, z0), (x1, z1), Vec3::X),
        ((x1, z1), (x0, z1), Vec3::Z),
        ((x0, z1), (x0, z0), Vec3::NEG_X),
    ];
    for ((ax, az), (bx, bz), n) in sides {
        let (ta, tb) = (top(ax, az), top(bx, bz));
        if ta.y - s.y0 < 1e-3 && tb.y - s.y0 < 1e-3 {
            continue;
        }
        let along = |p: Vec3| if n.x != 0.0 { [p.z, -p.y] } else { [p.x, -p.y] };
        f.quad([bot(ax, az), bot(bx, bz), tb, ta], n, along);
    }
    Some(f.mesh(tile))
}

/// A turned box's mesh, its texture laid in world metres by each face's
/// way in the world (as a box's is), so the segments of a curved wall run
/// on as one surface instead of each starting its own.
fn turned_mesh(size: Vec3, mid: Vec3, rot: Quat, tile: f32) -> Mesh {
    let mut m = Mesh::from(Cuboid::new(size.x, size.y, size.z));
    let pos: Vec<[f32; 3]> = match m.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(bevy::mesh::VertexAttributeValues::Float32x3(v)) => v.clone(),
        _ => Vec::new(),
    };
    let nor: Vec<[f32; 3]> = match m.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(bevy::mesh::VertexAttributeValues::Float32x3(v)) => v.clone(),
        _ => Vec::new(),
    };
    let uv: Vec<[f32; 2]> = pos
        .iter()
        .zip(&nor)
        .map(|(p, n)| {
            let w = rot * Vec3::from_array(*p) + mid;
            let wn = rot * Vec3::from_array(*n);
            let a = wn.abs();
            let (u, v) = if a.y > a.x && a.y > a.z {
                (w.x, w.z)
            } else {
                // Round the wall: along it by its heading, so a curve's
                // faces carry on from one to the next.
                let along = if a.x > a.z {
                    w.z * wn.x.signum()
                } else {
                    -w.x * wn.z.signum()
                };
                (along, -w.y)
            };
            [u / tile, v / tile]
        })
        .collect();
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    let _ = m.generate_tangents();
    m
}

/// A box still in its plain colour (a mass waits for its pieces first).
type Undressed = (
    Without<Dressed>,
    Without<Mass>,
    Without<crate::look::boulders::Boulder>,
    Without<crate::look::boulders::Pylon>,
);

/// The level's ground and boxes in their textures, once those are ready:
/// the ground (and its rock skin) in the splat material, every textured
/// box weathered (`look/weathering.rs`). The grass and the clutter dress
/// themselves (`look/grass.rs`, `look/clutter.rs`).
#[allow(clippy::too_many_arguments)]
pub fn dress_level(
    mut commands: Commands,
    tier: Res<crate::look::quality::Tier>,
    tex: Res<crate::look::textures::Textures>,
    mut ground_mats: ResMut<Assets<crate::look::ground::GroundMaterial>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut level_mats: ResMut<Assets<weathering::LevelMaterial>>,
    ground: Query<(Entity, &GroundMesh), Without<Dressed>>,
    boxes: Query<(Entity, &BoxLook, Option<&Block>), Undressed>,
    dressed: Query<&MeshMaterial3d<crate::look::ground::GroundMaterial>, (With<GroundMesh>, With<Dressed>)>,
    mood: Res<crate::look::mood::MoodNow>,
) {
    // On the lite frame (`quality::Tier::lite`: a world drawn under the
    // window's resolution) the ground draws no wall planes or rock face;
    // a rung moved after it was dressed reaches it here.
    let lite = tier.lite();
    if tier.is_changed() {
        for m in &dressed {
            if ground_mats
                .get(&m.0)
                .is_some_and(|g| crate::look::ground::is_lite(g) != lite)
            {
                if let Some(g) = ground_mats.get_mut(&m.0) {
                    crate::look::ground::set_lite(g, lite);
                }
            }
        }
    }
    // The rain's wet on everything it falls on (`look/rain.rs`); none
    // under a roof.
    let wet = if mood.roofed { 0.0 } else { mood.mood.wet };
    let dusty = mood.mood.dust > 0.0;
    let Some((albedo, normal)) = tex.ground.clone() else {
        return;
    };
    // The ground and the rock skin over it in the splat material: the sea's
    // height for its wet band, the deck's shadows and the rain's wet, and
    // on the lite frame no wall planes or rock face.
    for (e, g) in ground.iter() {
        let m = ground_mats.add(crate::look::ground::material(
            albedo.clone(),
            normal.clone(),
            g.sea,
            lite,
            g.isle,
            mood.mood.snow,
            crate::look::cloud_shadow::CloudShadowParams::off(wet),
        ));
        commands
            .entity(e)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert((MeshMaterial3d(m), Dressed));
    }
    for (e, look, block) in boxes.iter() {
        let (Some(base), Some((set, _, _))) = (box_material(look.0), dress(look.0)) else {
            commands.entity(e).insert(Dressed);
            continue;
        };
        let Some((albedo, normal)) = tex.set(set) else {
            continue;
        };
        // With the photographed roughness and occlusion, the numbers scale
        // the photograph's (its roughness is about 0.6 on average).
        let orm = tex.orm(set);
        let scale = if orm.is_some() { ROUGH_SCALE } else { 1.0 };
        // A set that glows (the hull's light seams, the windows): in its
        // look's colour.
        let glow = tex.emissive(set);
        let (tint, rough) =
            crate::look::rain::wetted(base.base_color, (base.perceptual_roughness * scale).min(1.0), wet);
        let m = StandardMaterial {
            base_color: tint,
            base_color_texture: Some(albedo),
            normal_map_texture: Some(normal),
            perceptual_roughness: rough,
            metallic_roughness_texture: orm.clone(),
            occlusion_texture: orm,
            emissive: if glow.is_some() {
                let (c, k) = vein(look.0);
                LinearRgba::from(c) * k
            } else {
                LinearRgba::BLACK
            },
            emissive_texture: glow,
            // The snow's set lies over more than a tile of its UVs.
            uv_transform: if look.0 == SNOW {
                bevy::math::Affine2::from_scale(Vec2::splat(1.0 / crate::look::built::SNOW_LAID))
            } else {
                bevy::math::Affine2::IDENTITY
            },
            ..base
        };
        let mut ec = commands.entity(e);
        // Snow takes no grime or seams, nor a soaked road dust.
        match block.filter(|_| look.0 != SNOW && !(look.0 == ROAD && wet > 0.0)) {
            Some(b) => {
                let w = weathering::material(m, surface(look.0), b, dusty, mood.roofed);
                ec.remove::<MeshMaterial3d<StandardMaterial>>()
                    .insert(MeshMaterial3d(level_mats.add(w)));
            }
            None => {
                ec.insert(MeshMaterial3d(mats.add(m)));
            }
        }
        ec.insert(Dressed);
    }
}

/// How bright a light strip is, over the display's white (HDR: past 1 it
/// blooms on the desktop), and a launcher's glow beside it.
const STRIP_GLOW: f32 = 4.0;
/// How bright the hull's light seams glow, over the display's white.
const VEIN_GLOW: f32 = 3.0;
/// And the windows' honeycomb.
const GLASS_GLOW: f32 = 0.95;
/// And the light under the deck, through its grates.
const GRATE_GLOW: f32 = 0.6;

/// A glowing set's light by look: the hull's seams magenta, faint on the
/// dark hull, red's and blue's on a side's, the windows gold, the deck's
/// grates violet.
fn vein(look: u8) -> (Color, f32) {
    match look {
        13 | RIB => (Color::srgb(0.6, 0.3, 0.7), VEIN_GLOW * 0.5),
        14 => (Color::srgb(1.0, 0.32, 0.3), VEIN_GLOW),
        15 => (Color::srgb(0.3, 0.55, 1.0), VEIN_GLOW),
        GOLD => (Color::srgb(1.0, 0.74, 0.35), GLASS_GLOW),
        DECK => (Color::srgb(0.55, 0.35, 1.0), GRATE_GLOW),
        _ => (Color::srgb(1.0, 0.4, 0.9), VEIN_GLOW),
    }
}

/// What a font's flame throws on what stands round it: its colour, lumens
/// and reach, m.
const FONT_LIGHT: [f32; 3] = [0.25, 0.55, 1.0];
const FONT_LUMENS: f32 = 90000.0;
const FONT_RANGE: f32 = 11.0;

/// How far a map's lamp reaches, m; its fixture's radius and glow.
const LAMP_RANGE: f32 = 22.0;
const LAMP_R: f32 = 0.7;
const LAMP_FIXTURE_GLOW: f32 = 8.0;
/// A lamp this high hangs just under a vault: it throws its light down as a
/// cone (half angles, radians) and leaves the dark overhead dark. No shadow:
/// every rung lights a fighter the same.
const LAMP_DOWN_FROM: f32 = 13.0;
const LAMP_CONE: (f32, f32) = (0.6, 1.05);
/// Any lower lamp lights a room with a low ceiling, a metre or two over a
/// head: its fixture is this small and this bright.
const LAMP_ROOM_R: f32 = 0.34;
const LAMP_ROOM_GLOW: f32 = 3.0;
const LAUNCHER_GLOW: f32 = 2.5;
/// A lift's beam: how much of its colour it adds to what is behind it.
const LIFT_BEAM_ALPHA: f32 = 0.08;
/// The beam's radius as a share of the lift's narrower side: a column of
/// light up the middle, not a glass wall over the whole pad.
const LIFT_BEAM_R: f32 = 0.22;
/// The beam is shells of light one inside another, each this share of its
/// radius: the more of them a line of sight crosses the brighter, so it has
/// a core and no edge. Each adds this share of the beam's colour, fading
/// to nothing by its top as this power of what is left of its height and in
/// from nothing over this share of it at its foot.
const LIFT_BEAM_SHELLS: [f32; 4] = [1.0, 0.72, 0.46, 0.22];
const LIFT_BEAM_SHELL: f32 = 0.5;
const LIFT_BEAM_FADE: f32 = 1.3;
const LIFT_BEAM_FOOT: f32 = 0.04;
const LIFT_BEAM_SIDES: usize = 20;
const LIFT_BEAM_RINGS: usize = 8;

/// A lift's beam, `r` wide and `height` tall about its middle: open shells
/// (`LIFT_BEAM_SHELLS`), their fade in their vertices' alpha, for a paint
/// that adds.
fn lift_beam(r: f32, height: f32) -> Mesh {
    let (mut positions, mut normals, mut colors, mut indices) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for share in LIFT_BEAM_SHELLS {
        let base = positions.len() as u32;
        for i in 0..=LIFT_BEAM_RINGS {
            let t = i as f32 / LIFT_BEAM_RINGS as f32;
            let alpha = LIFT_BEAM_SHELL * (1.0 - t).powf(LIFT_BEAM_FADE) * (t / LIFT_BEAM_FOOT).min(1.0);
            for j in 0..LIFT_BEAM_SIDES {
                let (sin, cos) = (j as f32 / LIFT_BEAM_SIDES as f32 * core::f32::consts::TAU).sin_cos();
                positions.push([r * share * cos, (t - 0.5) * height, r * share * sin]);
                normals.push([cos, 0.0, sin]);
                colors.push([1.0, 1.0, 1.0, alpha]);
            }
        }
        let n = LIFT_BEAM_SIDES as u32;
        for i in 0..LIFT_BEAM_RINGS as u32 {
            for j in 0..n {
                let (a, b) = (base + i * n + j, base + i * n + (j + 1) % n);
                indices.extend([a, b, b + n, a, b + n, a + n]);
            }
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32(indices))
}

/// A light strip's colour (looks `glow`, `red_glow`, `blue_glow`,
/// `magenta_glow`, `cyan_glow`): drawn unshadowed and emissive, the same
/// in the sun and in a hangar.
fn strip(i: u8) -> Option<Color> {
    Some(match i {
        8 => Color::srgb(0.62, 0.88, 1.0),
        9 => Color::srgb(1.0, 0.28, 0.34),
        10 => Color::srgb(0.3, 0.55, 1.0),
        20 => Color::srgb(1.0, 0.35, 0.85),
        21 => Color::srgb(0.3, 0.95, 1.0),
        _ => return None,
    })
}

/// A strip's material.
pub(crate) fn strip_material(c: Color, roughness: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: c,
        emissive: LinearRgba::from(c) * STRIP_GLOW,
        perceptual_roughness: roughness,
        ..default()
    }
}

/// A box's look (tools/maps/compile.mjs LOOKS): its colour and roughness.
fn look(i: u8) -> (Color, f32, f32) {
    match i {
        7 => (Color::srgb(0.36, 0.39, 0.44), 0.45, 0.55),         // hull
        11 => (Color::srgb(0.66, 0.68, 0.71), 0.45, 0.55),        // plate
        12 | VAULT => (Color::srgb(0.45, 0.35, 0.65), 0.35, 0.7), // the ship's hull
        RIB => (Color::srgb(0.24, 0.17, 0.37), 0.3, 0.8),         // its ribs, plain and dark
        13 => (Color::srgb(0.25, 0.2, 0.38), 0.4, 0.7),
        14 => (Color::srgb(0.6, 0.25, 0.4), 0.35, 0.7),
        15 => (Color::srgb(0.25, 0.35, 0.65), 0.35, 0.7),
        DECK => (Color::srgb(0.1, 0.1, 0.13), 0.6, 0.5),
        GOLD => (Color::srgb(0.9, 0.7, 0.3), 0.3, 0.3),
        8..=10 | 20 | 21 => (strip(i).unwrap_or(Color::WHITE), 0.3, 0.0),
        0 => (Color::srgb(0.54, 0.56, 0.59), 0.9, 0.0), // wall
        1 => (Color::srgb(0.62, 0.64, 0.67), 0.6, 0.2), // block
        2 => (Color::srgb(0.79, 0.64, 0.15), 0.5, 0.4), // trim
        3 => (Color::srgb(0.54, 0.33, 0.31), 0.6, 0.2), // red
        4 => (Color::srgb(0.31, 0.39, 0.54), 0.6, 0.2), // blue
        5 => (Color::srgb(0.29, 0.31, 0.34), 0.7, 0.2), // dark
        PYLON | STEEL => (Color::srgb(0.42, 0.44, 0.47), 0.55, 0.8),
        STEEL_RED => (Color::srgb(0.6, 0.24, 0.2), 0.6, 0.3),
        STEEL_BLUE => (Color::srgb(0.22, 0.32, 0.6), 0.6, 0.3),
        TREAD => (Color::srgb(0.34, 0.35, 0.37), 0.7, 0.8),
        STONE => (Color::srgb(0.6, 0.6, 0.61), 0.88, 0.0),
        STONE_DARK => (Color::srgb(0.38, 0.38, 0.39), 0.88, 0.0),
        _ => (Color::srgb(0.61, 0.33, 0.21), 0.95, 0.0), // rock
    }
}

/// The ground is rock from this slope (rise over run), fully by the next:
/// the scree banks turn to stone up their middle; from tan 0.9 the ground
/// shader's per-pixel cliff veto (look::ground::CLIFF_TAN_LO/HI) owns the
/// edge, so it wanders with its noise instead of stepping per vertex. The
/// rock skin grows on the same weight (`look/cliffs.rs`).
const ROCK_SLOPE: (f32, f32) = (0.45, 0.9);
/// A creek bed is gravel: ground under the floor (below `CREEK_Y`, fully
/// half a metre lower) that also lies in a trough, under both of its
/// neighbours `CREEK_STENCIL_M` off along an axis by `CREEK_BASIN`'s first
/// metres (fully by its second). A beach sloping to the sea is under the
/// floor too, but one of its neighbours is always lower, so it stays sand
/// (a mean of the two would read the beach's knee as a bed).
const CREEK_Y: f32 = -0.15;
const CREEK_STENCIL_M: f32 = 12.0;
const CREEK_BASIN: (f32, f32) = (0.6, 1.2);

/// The ground at vertex (i, j): its slope (rise/run, by central
/// differences) and its splat weights (sand, grass, gravel, rock): rock
/// where it is steep, gravel down in the creek's trough, grass up on the
/// hills in patches, sand the rest.
/// What kind of land a terrain is, for what grows on it and what lies
/// round it: a canyon (Dry Fork: grass up its hills, gravel in its creek,
/// land beyond its rim, a sea at its mouth), or an island over a void (a map
/// with a `void`: bare sandstone and its rock, cloud all round and under).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Land {
    Canyon,
    Isle,
}

impl Land {
    pub fn of(def: &sim::map::Def) -> Land {
        if def.void > f32::NEG_INFINITY {
            Land::Isle
        } else {
            Land::Canyon
        }
    }
}

/// Where an island's drawn ground stops and its keel hangs from, m: under the
/// lip, so the rim overhangs what is under it, all of that drawn inside the
/// column the sim collides.
const ISLE_CUT_Y: f32 = -3.0;

/// Over this height an island's ground is dune, m (its floor is 0).
const ISLE_DUNE_Y: f32 = 0.05;

pub(crate) fn ground_point(t: &TerrainDef, i: usize, j: usize, land: Land) -> (f32, [f32; 4]) {
    let (nx, nz) = (t.nx as usize, t.nz as usize);
    let h = |i: usize, j: usize| t.heights[j * nx + i] as f32 / HEIGHT_Q;
    let dx = (h((i + 1).min(nx - 1), j) - h(i.saturating_sub(1), j)) / (2.0 * t.cell);
    let dz = (h(i, (j + 1).min(nz - 1)) - h(i, j.saturating_sub(1))) / (2.0 * t.cell);
    let slope = (dx * dx + dz * dz).sqrt();
    let y = h(i, j);
    let s = ((CREEK_STENCIL_M / t.cell).round() as usize).max(1);
    let along_x = h((i + s).min(nx - 1), j).min(h(i.saturating_sub(s), j)) - y;
    let along_z = h(i, (j + s).min(nz - 1)).min(h(i, j.saturating_sub(s))) - y;
    let trough = ((along_x.max(along_z) - CREEK_BASIN.0) / (CREEK_BASIN.1 - CREEK_BASIN.0)).clamp(0.0, 1.0);
    if land == Land::Isle {
        // Sandstone: sand where it lies, rock where it stands. A dune over
        // the island's floor is sand however steep.
        let slope = if y > ISLE_DUNE_Y {
            slope.min(ROCK_SLOPE.0)
        } else {
            slope
        };
        return (slope, splat_in(slope, y, 0.0, 0.0));
    }
    let cover = crate::look::grass::cover(t.x0 + i as f32 * t.cell, t.z0 + j as f32 * t.cell, y);
    (slope, splat_in(slope, y, trough, cover))
}

/// What the ground of the map in play is drawn as at (x, z), for what a step
/// on it sounds like: [`ground_point`]'s rule from the sim's heights.
pub(crate) fn splat_at(map: &sim::map::Map, x: f32, z: f32) -> [f32; 4] {
    let y = map.ground(x, z);
    let n = map.ground_normal(x, z);
    let slope = (n.x * n.x + n.z * n.z).sqrt() / n.y.max(0.01);
    if map.void > f32::NEG_INFINITY {
        return splat_in(slope, y, 0.0, 0.0);
    }
    let s = CREEK_STENCIL_M;
    let along_x = map.ground(x + s, z).min(map.ground(x - s, z)) - y;
    let along_z = map.ground(x, z + s).min(map.ground(x, z - s)) - y;
    let trough = ((along_x.max(along_z) - CREEK_BASIN.0) / (CREEK_BASIN.1 - CREEK_BASIN.0)).clamp(0.0, 1.0);
    splat_in(slope, y, trough, crate::look::grass::cover(x, z, y))
}

/// The splat weights (sand, grass, gravel, rock) of ground at height `y`
/// with slope `slope`, anywhere, which the ground clutter scatters by
/// (`look/clutter.rs`): [`ground_point`]'s rule, taking every low spot for
/// a creek bed and the grass unpatched.
pub(crate) fn splat(slope: f32, y: f32) -> [f32; 4] {
    splat_in(slope, y, 1.0, crate::look::grass::by_height(y))
}

/// The same on bare sandstone (an island's, [`Land::Isle`]): sand where it
/// lies, rock where it stands, nothing else.
pub(crate) fn splat_bare(slope: f32, y: f32) -> [f32; 4] {
    splat_in(slope, y, 0.0, 0.0)
}

/// The weights, given how much of a trough the spot lies in (0..1) and how
/// much grass its height and its patch allow (0..1).
fn splat_in(slope: f32, y: f32, trough: f32, cover: f32) -> [f32; 4] {
    let rock = ((slope - ROCK_SLOPE.0) / (ROCK_SLOPE.1 - ROCK_SLOPE.0)).clamp(0.0, 1.0);
    let gravel = ((CREEK_Y - y) * 2.0).clamp(0.0, 1.0) * trough * (1.0 - rock);
    let grass = cover * (1.0 - rock) * (1.0 - gravel);
    let sand = (1.0 - rock - gravel - grass).max(0.0);
    [sand, grass, gravel, rock]
}

/// The ground's vertex normals: each triangle's (the sim's split) weighted
/// by its angle at the vertex, as Bevy's `compute_smooth_normals` does;
/// here so the rock skin fades into exactly these.
pub(crate) fn ground_normals(t: &TerrainDef) -> Vec<Vec3> {
    let (nx, nz) = (t.nx as usize, t.nz as usize);
    let p = |k: usize| {
        Vec3::new(
            t.x0 + (k % nx) as f32 * t.cell,
            t.heights[k] as f32 / HEIGHT_Q,
            t.z0 + (k / nx) as f32 * t.cell,
        )
    };
    let mut n = vec![Vec3::ZERO; nx * nz];
    for j in 0..nz - 1 {
        for i in 0..nx - 1 {
            let a = j * nx + i;
            let (b, c) = (a + 1, a + nx);
            let d = c + 1;
            for tri in [[a, d, b], [a, c, d]] {
                let q = tri.map(p);
                let face = (q[1] - q[0]).cross(q[2] - q[0]).normalize_or_zero();
                for (v, o1, o2) in [(0, 1, 2), (1, 2, 0), (2, 0, 1)] {
                    n[tri[v]] += face * (q[o1] - q[v]).angle_between(q[o2] - q[v]);
                }
            }
        }
    }
    n.into_iter().map(|v| v.normalize_or(Vec3::Y)).collect()
}

/// The ground mesh: the sim's vertices and its split of each cell (low
/// corner to high corner), with its splat weights in the vertex colour
/// ([`ground_point`]).
fn terrain_mesh(t: &TerrainDef, land: Land) -> Mesh {
    let (nx, nz) = (t.nx as usize, t.nz as usize);
    let h = |i: usize, j: usize| t.heights[j * nx + i] as f32 / HEIGHT_Q;
    let mut pos = Vec::with_capacity(nx * nz);
    let mut col = Vec::with_capacity(nx * nz);
    for j in 0..nz {
        for i in 0..nx {
            pos.push([t.x0 + i as f32 * t.cell, h(i, j), t.z0 + j as f32 * t.cell]);
            col.push(ground_point(t, i, j, land).1);
        }
    }
    let mut idx = Vec::with_capacity((nx - 1) * (nz - 1) * 6);
    for j in 0..nz - 1 {
        for i in 0..nx - 1 {
            let a = (j * nx + i) as u32;
            let (b, c) = (a + 1, a + nx as u32);
            let d = c + 1;
            // Upper (x offset ≥ z offset): a, b, d. Lower: a, d, c. Up-facing.
            idx.extend_from_slice(&[a, d, b, a, c, d]);
        }
    }
    let nrm: Vec<[f32; 3]> = ground_normals(t).into_iter().map(|n| n.to_array()).collect();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

/// Spawns map `id`'s level; `old` is the last one's, taken down first.
#[allow(clippy::too_many_arguments)]
pub fn build(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    water_mats: &mut Assets<crate::look::water::WaterMaterial>,
    cloud_mats: &mut Assets<crate::look::cloud_sea::CloudSeaMaterial>,
    images: &mut Assets<Image>,
    cards: &mut crate::look::trees::TreeCards,
    old: &Query<Entity, With<LevelPart>>,
    id: u8,
) {
    for e in old.iter() {
        commands.entity(e).despawn();
    }
    let Some(def) = sim::maps::ALL.iter().find(|d| d.id == id) else {
        return;
    };
    let map = def.build();
    let land = Land::of(def);
    let (lo, hi) = (map.lo, map.hi);
    let (cx, cz) = ((lo.0 + hi.0) * 0.5, (lo.1 + hi.1) * 0.5);
    let half = (hi.0 - lo.0).max(hi.1 - lo.1) * 0.5 + 8.0;
    // The ground under a point, where a block's grime starts.
    let ground_at = |x: f32, z: f32| map.terrain.as_ref().and_then(|t| t.height(x, z)).unwrap_or(0.0);
    // Every box as the others' faces are cut by (`rock::covers`): a light
    // strip wins every shared plane, the floor none; a mass merged into
    // another is not drawn.
    let floor_size = 2.0 * half + 40.0;
    let mut all: Vec<rock::Other> = map
        .boxes
        .iter()
        .zip(&map.looks)
        .map(|(b, &l)| rock::Other {
            wins: strip(l).is_some(),
            // A hidden box (a lift's volume) is nobody's cover.
            gone: l == HIDDEN,
            ..rock::Other::new(v3(b.min), v3(b.max))
        })
        .collect();
    if def.terrain.is_none() {
        all.push(rock::Other::new(
            Vec3::new(cx - floor_size * 0.5, -0.2, cz - floor_size * 0.5),
            Vec3::new(cx + floor_size * 0.5, 0.0, cz + floor_size * 0.5),
        ));
    }
    let is_mass = |i: usize, o: &rock::Other| map.looks[i] == ROCK && rock::is_mass(o.min, o.max, lo, hi);
    // Two masses that are one box between them (a mirrored cliff across
    // the middle) are drawn as that box: their shared planes and their
    // layouts would otherwise be drawn twice over.
    for i in 0..map.boxes.len() {
        for j in i + 1..map.boxes.len() {
            let (a, b) = (all[i], all[j]);
            if a.gone || b.gone || !is_mass(i, &a) || !is_mass(j, &b) {
                continue;
            }
            if let Some((min, max)) = rock::union_box(a.min, a.max, b.min, b.max) {
                all[i].min = min;
                all[i].max = max;
                all[j].gone = true;
            }
        }
    }

    for (i, (b, &l)) in map.boxes.iter().zip(&map.looks).enumerate() {
        let (color, roughness, metallic) = look(l);
        let size = b.max - b.min;
        let tile = dress(l).map_or(2.0, |d| d.1);
        if l == HIDDEN {
            // Drawn by the built set, set back from these planes.
            if crate::look::built::of(id).is_some() {
                commands.spawn((LevelPart, rock::Bare(v3(b.min), v3(b.max))));
            }
            continue;
        }
        if let Some(c) = strip(l) {
            let mid = (b.min + b.max) * 0.5;
            commands.spawn((
                LevelPart,
                Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                MeshMaterial3d(mats.add(strip_material(c, roughness))),
                Transform::from_xyz(mid.x, mid.y, mid.z),
                bevy::light::NotShadowCaster,
            ));
            continue;
        }
        let me = all[i];
        if me.gone {
            continue;
        }
        let (min, max) = (me.min, me.max);
        let mid = (min + max) * 0.5;
        let mut block = Block {
            mid,
            half: (max - min) * 0.5,
            turn: (1.0, 0.0),
            shape: Shape::Box,
            ground: ground_at(mid.x, mid.z),
            run: None,
            hidden: rock::hidden_edges(min, max, i, &all),
            own_value: true,
        };
        // A long box on uneven ground: its grime follows the ground along
        // it, not the height under its middle (an end wall across a
        // canyon's floor and up its banks).
        if map.terrain.is_some() {
            let size = max - min;
            let along_x = size.x >= size.z;
            if size.x.max(size.z) > RUN_MIN_M {
                let g: [f32; RUN] = core::array::from_fn(|k| {
                    let t = k as f32 / (RUN - 1) as f32;
                    if along_x {
                        ground_at(min.x + size.x * t, mid.z)
                    } else {
                        ground_at(mid.x, min.z + size.z * t)
                    }
                });
                if g.iter().any(|h| (h - block.ground).abs() > RUN_STEP_M) {
                    block.run = Some((if along_x { Run::X } else { Run::Z }, g));
                }
            }
        }
        // Its faces, less what the other boxes hide or draw themselves.
        let cover = rock::covers(min, max, i, &all);
        // A rock box's faces faceted where anyone can see them; one out of
        // bounds and clear of its edge drawn as a mass (`look/rock.rs`).
        let out = max.x <= lo.0 || min.x >= hi.0 || max.z <= lo.1 || min.z >= hi.1;
        let seen = match (l == ROCK, out) {
            (false, _) => [false; 6],
            (true, false) => rock::SEEN_ALL,
            (true, true) => rock::seen_from(min, max, lo, hi),
        };
        let mut soup = rock::Soup::default();
        rock::faceted_box(&mut soup, min, max, mid, rock_key(id), seen, &cover);
        if soup.is_empty() {
            // Inside other boxes: nothing of it shows.
            continue;
        }
        let mass = (l == ROCK && is_mass(i, &me)).then(|| {
            // Its drawn edges are the rock's, not the box's: no wear on them.
            block.shape = Shape::Other;
            // Pieces only along a face something of which shows.
            let mut seen = seen;
            for (f, s) in seen.iter_mut().enumerate() {
                let (lu, lv) = rock::face_size(min, max, f);
                *s &= cover[f].open(lu, lv) > MASS_OPEN;
            }
            Mass {
                min,
                max,
                seen,
                key: box_key(mid, id),
                tile,
                cover: cover.clone(),
            }
        });
        let mut e = commands.spawn((
            LevelPart,
            BoxLook(l),
            block,
            Mesh3d(meshes.add(soup.mesh(tile))),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: roughness,
                metallic,
                reflectance: fresnel::DIELECTRIC,
                ..default()
            })),
            Transform::from_translation(mid),
        ));
        if let Some(m) = mass {
            e.insert(m);
        } else if l == ROCK && !out {
            // A rock a fighter can reach: a boulder inside its box, once
            // the models are in (`look/boulders.rs`).
            e.insert(crate::look::boulders::Boulder {
                min,
                max,
                key: box_key(mid, id),
                pale: land == Land::Isle,
            });
        }
    }
    // Rock out of reach: the sea's stacks and stones.
    crate::look::boulders::spawn_scenery(commands, id, land == Land::Isle);
    crate::look::built::spawn(commands, id);
    // The shade a hall's lamps and fill lay on its floors
    // (`look/floor_shade.rs`).
    {
        let built = crate::look::built::of(id).is_some();
        let fill = crate::look::floor_shade::fill_units(&map, id);
        // What is drawn as a floor: not a strip of light, not a box nothing
        // draws, not a prop standing in its solid.
        let floor_box = |i: usize| strip(map.looks[i]).is_none() && (map.looks[i] != HIDDEN || built);
        let floor_solid = |i: usize| {
            let l = map.solid_looks[i];
            !matches!(l, PALM | DEAD_TREE | PYLON | WRECK_CAR | WRECK_VAN | HUSK..=FONT) && (l != HIDDEN || built)
        };
        let began = bevy::platform::time::Instant::now();
        if let Some(m) = crate::look::floor_shade::mesh(
            &map,
            fill,
            (LAMP_RANGE, LAMP_CONE, LAMP_DOWN_FROM),
            map.light.floor != HIDDEN,
            floor_box,
            floor_solid,
        ) {
            info!(
                "oneshot: floor shade: {} triangles in {:.0} ms",
                m.indices().map_or(0, |i| i.len() / 3),
                began.elapsed().as_secs_f32() * 1000.0
            );
            commands.spawn((
                LevelPart,
                Mesh3d(meshes.add(m)),
                MeshMaterial3d(mats.add(crate::look::floor_shade::material())),
                crate::look::floor_shade::bundle(),
            ));
        }
    }
    // And trees along the rim over the walls, out of reach too, and the
    // land beyond it (`look/far.rs`).
    // An island has none: the cloud is its distance.
    if let Some(t) = def.terrain.as_ref().filter(|_| land == Land::Canyon) {
        crate::look::far::spawn(
            commands,
            meshes,
            mats,
            Vec2::new(t.x0, t.z0),
            Vec2::new(t.x0 + (t.nx - 1) as f32 * t.cell, t.z0 + (t.nz - 1) as f32 * t.cell),
            hi.0,
            def.light.sea,
            id as u32,
        );
        let ground = |x: f32, z: f32| crate::look::grass::ground(t, x, z);
        for (foot, height, kind, seed) in crate::look::trees::rim(ground, lo, hi, id as u32) {
            let cards = cards.get(images);
            for (mesh, mat, at) in
                crate::look::trees::tree(kind, seed, crate::look::trees::RIM_TRUNK_R, height, cards, meshes, mats)
            {
                commands.spawn((
                    LevelPart,
                    TreePart,
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform::from_translation(foot).mul_transform(at),
                ));
            }
        }
    }
    // The dressing (`def.dress`: drawn, never collided): the ship's pods,
    // trims, vaults and ribs, each by its look, over the sim's boxes.
    for &(b, l) in def.dress {
        if l == HIDDEN {
            continue;
        }
        let (min, max) = (Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5]));
        let (color, roughness, metallic) = look(l);
        let tile = dress(l).map_or(2.0, |d| d.1);
        let size = max - min;
        let mid = (min + max) * 0.5;
        if let Some(c) = strip(l) {
            commands.spawn((
                LevelPart,
                Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                MeshMaterial3d(mats.add(strip_material(c, roughness))),
                Transform::from_translation(mid),
                bevy::light::NotShadowCaster,
            ));
            continue;
        }
        if l == VAULT {
            let v = crate::look::vault::vault(min, max, HULL_TILE_M, HULL_TILE_M);
            for (mesh, look_id) in [(v.plates, 12), (v.ribs, 13)] {
                let (c, r, m) = look(look_id);
                commands.spawn((
                    LevelPart,
                    BoxLook(look_id),
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(mats.add(StandardMaterial {
                        base_color: c,
                        perceptual_roughness: r,
                        metallic: m,
                        reflectance: fresnel::DIELECTRIC,
                        ..default()
                    })),
                    Transform::default(),
                ));
            }
            let c = strip(21).unwrap_or(Color::WHITE);
            commands.spawn((
                LevelPart,
                Mesh3d(meshes.add(v.strips)),
                MeshMaterial3d(mats.add(strip_material(c, 0.3))),
                Transform::default(),
                bevy::light::NotShadowCaster,
            ));
            continue;
        }
        if l == RIB {
            commands.spawn((
                LevelPart,
                BoxLook(l),
                Mesh3d(meshes.add(crate::look::vault::rib(min, max, tile))),
                MeshMaterial3d(mats.add(StandardMaterial {
                    base_color: color,
                    perceptual_roughness: roughness,
                    metallic,
                    reflectance: fresnel::DIELECTRIC,
                    ..default()
                })),
                Transform::default(),
            ));
            continue;
        }
        // A plain dressing box (a pod, a plate): its faces, weathered as a
        // box is, nothing of rock on them, and no wear on an edge buried in
        // the boxes it sits on (it is none of them: no index of its own).
        let mut soup = rock::Soup::default();
        rock::faceted_box(&mut soup, min, max, mid, rock_key(id), [false; 6], &rock::bare());
        if soup.is_empty() {
            continue;
        }
        let block = Block {
            mid,
            half: size * 0.5,
            turn: (1.0, 0.0),
            shape: Shape::Box,
            ground: ground_at(mid.x, mid.z),
            run: None,
            hidden: rock::hidden_edges(min, max, usize::MAX, &all),
            own_value: true,
        };
        commands.spawn((
            LevelPart,
            BoxLook(l),
            block,
            Mesh3d(meshes.add(soup.mesh(tile))),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: roughness,
                metallic,
                reflectance: fresnel::DIELECTRIC,
                ..default()
            })),
            Transform::from_translation(mid),
        ));
    }
    let dress_solids = def
        .dress_solids
        .iter()
        .map(|&(k, n, l)| (sim::solid::Solid::from_def(k, n), l));
    let mut trim: Vec<(u8, Mesh)> = Vec::new();
    let dress_profiles = def
        .dress_profiles
        .iter()
        .map(|&(k, n, kn, l)| (sim::solid::Solid::profile(k, n, kn), l));
    for (sol, l) in map
        .solids
        .iter()
        .copied()
        .zip(map.solid_looks.iter().copied())
        .chain(dress_solids)
        .chain(dress_profiles)
    {
        let sol = &sol;
        if l == HIDDEN {
            if crate::look::built::of(id).is_some() {
                let b = sol.bounds();
                commands.spawn((LevelPart, rock::Bare(v3(b.min), v3(b.max))));
            }
            continue;
        }
        let kind = match l {
            PALM => Some(crate::look::trees::Kind::Palm),
            DEAD_TREE => Some(crate::look::trees::Kind::Dead),
            _ => None,
        };
        if let Some(kind) = kind {
            let sim::solid::Shape::Round { x, z, r } = sol.shape else {
                continue;
            };
            // Most of the canyon's trees are in leaf (`trees::in_leaf`).
            let kind = match kind {
                crate::look::trees::Kind::Dead if crate::look::trees::in_leaf(x, z) => crate::look::trees::Kind::Live,
                k => k,
            };
            // Each tree its own, from where it stands (a mirrored twin differs).
            let seed = (x * 31.0 + z * 7.0 + sol.y0) as i32 as u32;
            let cards = cards.get(images);
            for (mesh, mat, at) in crate::look::trees::tree(kind, seed, r, sol.y1 - sol.y0, cards, meshes, mats) {
                commands.spawn((
                    LevelPart,
                    TreePart,
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform::from_xyz(x, sol.y0, z).mul_transform(at),
                ));
            }
            continue;
        }
        let (color, roughness, metallic) = look(l);
        let tile = dress(l).map_or(2.0, |d| d.1);
        let glow = strip(l);
        // Plain trim on a curve, a strip of light or a dark rib, has no
        // photograph or wear of its own: one mesh a look, one draw, however
        // much a hall hangs of it.
        if (glow.is_some() || l == RIB)
            && matches!(
                sol.shape,
                sim::solid::Shape::Revolve { .. } | sim::solid::Shape::Sweep { .. }
            )
        {
            if let Some(m) = crate::look::curve::profile_mesh(sol, tile, false) {
                match trim.iter_mut().find(|t| t.0 == l) {
                    Some(t) => {
                        if t.1.merge(&m).is_err() {
                            warn!("a trim's mesh would not merge");
                        }
                    }
                    None => trim.push((l, m)),
                }
            }
            continue;
        }
        let mat = mats.add(match glow {
            Some(c) => strip_material(c, roughness),
            None => StandardMaterial {
                base_color: color,
                perceptual_roughness: roughness,
                metallic,
                reflectance: fresnel::DIELECTRIC,
                ..default()
            },
        });
        let hy = (sol.y1 - sol.y0) * 0.5;
        let bounds = sol.bounds();
        let block = match sol.shape {
            sim::solid::Shape::Round { x, z, r } => Block {
                mid: Vec3::new(x, sol.y0 + hy, z),
                half: Vec3::new(r, hy, r),
                turn: (1.0, 0.0),
                shape: Shape::Round,
                ground: ground_at(x, z),
                run: None,
                hidden: 0,
                own_value: false,
            },
            // No wear: an arc is turned segments overlapping at their
            // ends, and each one's end edges are inside the next.
            sim::solid::Shape::Turned { x, z, hx, hz, c, s } => Block {
                mid: Vec3::new(x, sol.y0 + hy, z),
                half: Vec3::new(hx, hy, hz),
                turn: (c, s),
                shape: Shape::Other,
                ground: ground_at(x, z),
                run: None,
                hidden: 0,
                own_value: false,
            },
            // No wear either: the edges a box's wear is laid along aren't
            // there on a curve.
            sim::solid::Shape::Ramp { .. } | sim::solid::Shape::Revolve { .. } | sim::solid::Shape::Sweep { .. } => {
                let (a, b) = (bounds.min, bounds.max);
                let mid = Vec3::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5, (a.z + b.z) * 0.5);
                Block {
                    mid,
                    half: Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z) * 0.5,
                    turn: (1.0, 0.0),
                    shape: Shape::Other,
                    ground: ground_at(mid.x, mid.z),
                    run: None,
                    hidden: 0,
                    own_value: false,
                }
            }
        };
        let (mesh, at) = match sol.shape {
            sim::solid::Shape::Round { x, z, r } => {
                (round_mesh(r, sol.y1 - sol.y0, tile), Transform::from_xyz(x, sol.y0, z))
            }
            sim::solid::Shape::Turned { x, z, hx, hz, c, s } => {
                let h = sol.y1 - sol.y0;
                let rot = Quat::from_rotation_y(-s.atan2(c));
                let mid = Vec3::new(x, (sol.y0 + sol.y1) * 0.5, z);
                (
                    turned_mesh(Vec3::new(2.0 * hx, h, 2.0 * hz), mid, rot, tile),
                    Transform::from_translation(mid).with_rotation(rot),
                )
            }
            sim::solid::Shape::Ramp { .. } => match ramp_mesh(sol, tile) {
                Some(m) => (m, Transform::default()),
                None => continue,
            },
            sim::solid::Shape::Revolve { .. } | sim::solid::Shape::Sweep { .. } => {
                // The windows' honeycomb lies flat over a dome, its cells
                // one size from rim to hub.
                match crate::look::curve::profile_mesh(sol, tile, l == GOLD) {
                    Some(m) => (m, Transform::default()),
                    None => continue,
                }
            }
        };
        let mut e = commands.spawn((LevelPart, Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat), at));
        if glow.is_some() {
            e.insert(bevy::light::NotShadowCaster);
        } else {
            e.insert((BoxLook(l), block));
        }
        // A base's buttress: a pylon inside its turned box, once the
        // models are in (`look/boulders.rs`).
        if let (PYLON, sim::solid::Shape::Turned { hx, hz, .. }) = (l, sol.shape) {
            e.insert(crate::look::boulders::Pylon {
                model: crate::look::boulders::PYLON_MODEL,
                at: crate::look::boulders::pylon_transform(at, hx, sol.y1 - sol.y0, hz),
                min: v3(bounds.min),
                max: v3(bounds.max),
            });
        }
        // A wreck: its model inside its turned box.
        if let (WRECK_CAR | WRECK_VAN, sim::solid::Shape::Turned { hx, hz, .. }) = (l, sol.shape) {
            e.insert(crate::look::boulders::Pylon {
                model: crate::look::boulders::wreck_model(l),
                at: crate::look::boulders::wreck_transform(at, hx, sol.y1 - sol.y0, hz),
                min: v3(bounds.min),
                max: v3(bounds.max),
            });
        }
        // A ship's prop: its model inside its round solid, its face to the
        // map's middle.
        if let (HUSK..=FONT, sim::solid::Shape::Round { x, z, r }) = (l, sol.shape) {
            e.insert(crate::look::boulders::Pylon {
                model: crate::look::boulders::prop_model(l),
                at: crate::look::boulders::round_prop_transform(l, x, sol.y0, sol.y1, z, r),
                min: v3(bounds.min),
                max: v3(bounds.max),
            });
            if l == FONT {
                commands.spawn((
                    LevelPart,
                    PointLight {
                        color: Color::linear_rgb(FONT_LIGHT[0], FONT_LIGHT[1], FONT_LIGHT[2]),
                        intensity: FONT_LUMENS,
                        range: FONT_RANGE,
                        radius: 0.2,
                        shadows_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(x, sol.y1 + 0.2, z),
                ));
            }
        }
    }
    for (l, mesh) in trim {
        let (color, roughness, metallic) = look(l);
        let glow = strip(l);
        let mut e = commands.spawn((
            LevelPart,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(mats.add(match glow {
                Some(c) => strip_material(c, roughness),
                None => StandardMaterial {
                    base_color: color,
                    perceptual_roughness: roughness,
                    metallic,
                    reflectance: fresnel::DIELECTRIC,
                    ..default()
                },
            })),
        ));
        if glow.is_some() {
            e.insert(bevy::light::NotShadowCaster);
        }
    }
    for &(p, c, lm) in map.light.lamps {
        let colour = Color::srgb(c[0], c[1], c[2]);
        if p[1] >= LAMP_DOWN_FROM {
            commands.spawn((
                LevelPart,
                SpotLight {
                    color: colour,
                    intensity: lm,
                    range: LAMP_RANGE,
                    radius: 0.15,
                    inner_angle: LAMP_CONE.0,
                    outer_angle: LAMP_CONE.1,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_xyz(p[0], p[1], p[2]).looking_to(Dir3::NEG_Y, Vec3::Z),
            ));
        } else {
            commands.spawn((
                LevelPart,
                PointLight {
                    color: colour,
                    intensity: lm,
                    range: LAMP_RANGE,
                    radius: 0.15,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_xyz(p[0], p[1], p[2]),
            ));
        }
        // A built set draws its own.
        if crate::look::built::of(id).is_some() {
            continue;
        }
        // Its fixture: a lit disc in a dark ring.
        let (fixture_r, fixture_glow) = if p[1] >= LAMP_DOWN_FROM {
            (LAMP_R, LAMP_FIXTURE_GLOW)
        } else {
            (LAMP_ROOM_R, LAMP_ROOM_GLOW)
        };
        commands.spawn((
            LevelPart,
            Mesh3d(meshes.add(Cylinder::new(fixture_r, 0.1))),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: colour,
                emissive: LinearRgba::from(colour) * fixture_glow,
                ..default()
            })),
            Transform::from_xyz(p[0], p[1], p[2]),
            bevy::light::NotShadowCaster,
        ));
        commands.spawn((
            LevelPart,
            Mesh3d(meshes.add(Torus::new(fixture_r, fixture_r + 0.12))),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: Color::srgb(0.12, 0.09, 0.18),
                perceptual_roughness: 0.4,
                metallic: 0.7,
                ..default()
            })),
            Transform::from_xyz(p[0], p[1], p[2]),
            bevy::light::NotShadowCaster,
        ));
    }
    match &def.terrain {
        Some(t) => {
            // The ground cover, from the map's seed: the grass off the pads
            // and the launchers, the clutter off those, the spawns, the
            // vehicles' bays and the walls (`look/grass.rs`,
            // `look/clutter.rs`). Neither casts a shadow (a rung without
            // them would light a fighter's feet they shade). An island's
            // bare sandstone grows dry tufts and sheds its own chips; one
            // under snow, neither.
            let seed = id as u32 * 7919 + 1;
            let clear = crate::look::grass::Clear::pads(&map);
            let isle = land == Land::Isle;
            let snowed = isle && crate::look::mood::of(id).snow;
            let grows = |v: Vec<_>| if snowed { Vec::new() } else { v };
            for (mesh, centre, layer) in grows(crate::look::grass::blocks(t, lo, hi, seed, &clear, def.light.sea, isle))
            {
                commands.spawn((
                    LevelPart,
                    GrassPart,
                    crate::look::grass::CoverBlock {
                        centre,
                        layer,
                        reach: crate::look::grass::BLOCK_REACH,
                    },
                    Mesh3d(meshes.add(mesh)),
                    Transform::default(),
                    Visibility::Hidden,
                    bevy::light::NotShadowCaster,
                ));
            }
            let clear = crate::look::clutter::clear(&map, t);
            for (mesh, centre, layer) in grows(crate::look::clutter::blocks(
                t,
                lo,
                hi,
                seed ^ 0xc1a7_7e75,
                &clear,
                isle,
            )) {
                commands.spawn((
                    LevelPart,
                    ClutterPart,
                    crate::look::grass::CoverBlock {
                        centre,
                        layer,
                        reach: crate::look::clutter::BLOCK_REACH,
                    },
                    Mesh3d(meshes.add(mesh)),
                    Transform::default(),
                    Visibility::Hidden,
                    bevy::light::NotShadowCaster,
                ));
            }
            // The sea's height: a canyon's shore is wet up from it, an
            // island's keel and stacks are lapped by the cloud at it
            // (`look/ground.rs`, the lap).
            let wet_from = map.light.sea;
            let soil = mats.add(StandardMaterial {
                base_color: Color::srgb(0.75, 0.62, 0.48),
                perceptual_roughness: 0.95,
                reflectance: fresnel::DIELECTRIC,
                ..default()
            });
            // An island's ground ends at its void: its keel hangs there
            // instead (`look/keel.rs`).
            let cut = |m: Mesh| {
                if isle {
                    crate::look::keel::clip_below(&m, ISLE_CUT_Y)
                } else {
                    m
                }
            };
            if isle {
                commands.spawn((
                    LevelPart,
                    GroundMesh { sea: wet_from, isle },
                    Mesh3d(meshes.add(crate::look::keel::mesh(t, ISLE_CUT_Y, rock_key(id)))),
                    MeshMaterial3d(soil.clone()),
                    Transform::default(),
                ));
            }
            commands.spawn((
                LevelPart,
                GroundMesh {
                    sea: wet_from,
                    isle: land == Land::Isle,
                },
                Mesh3d(meshes.add(cut(terrain_mesh(t, land)))),
                MeshMaterial3d(soil.clone()),
                Transform::default(),
            ));
            // The rock skin over its rock faces (`look/cliffs.rs`): no
            // shadow of its own, the ground under it casts it.
            let (skin, lift) = crate::look::cliffs::skin(t, lo, hi, rock_key(id), land);
            // An island's cliffs are hidden by the rock that hugs them
            // (`boulders::scenery`), and its coast is too tight for the skin's facets.
            if let Some(skin) = skin.filter(|_| land != Land::Isle) {
                commands.spawn((
                    LevelPart,
                    GroundMesh {
                        sea: wet_from,
                        isle: land == Land::Isle,
                    },
                    Mesh3d(meshes.add(cut(skin))),
                    MeshMaterial3d(soil),
                    Transform::default(),
                    bevy::light::NotShadowCaster,
                ));
            }
            commands.insert_resource(crate::look::cliffs::SkinLift(Some(Arc::new(lift))));
            // The sea, where the map has one, under the mood's sun; under an
            // island, cloud.
            if map.light.sea.is_finite() && land == Land::Isle {
                crate::look::cloud_sea::spawn(commands, meshes, cloud_mats, Vec2::new(cx, cz), map.light.sea);
            } else if map.light.sea.is_finite() {
                crate::look::water::spawn(
                    commands,
                    meshes,
                    water_mats,
                    images,
                    t,
                    map.light.sea,
                    crate::look::mood::of(id).to_sun(),
                );
            }
        }
        None => {
            // The floor as a slab whose top is y = 0, textured in world
            // metres like the boxes on it.
            let size = floor_size;
            let floor = if map.light.floor == 255 { FLOOR } else { map.light.floor };
            // A built set's own floor is its own to draw.
            if floor != HIDDEN {
                commands.spawn((
                    LevelPart,
                    BoxLook(floor),
                    // A floor: no edges anyone sees, and no grime (its ground
                    // is far under it), only its mottle and its dust.
                    Block {
                        mid: Vec3::new(cx, -0.1, cz),
                        half: Vec3::new(size * 0.5, 0.1, size * 0.5),
                        turn: (1.0, 0.0),
                        shape: Shape::Other,
                        ground: -1.0e3,
                        run: None,
                        hidden: 0,
                        own_value: false,
                    },
                    Mesh3d(meshes.add({
                        // Cut where the boxes stand on it.
                        let me = all.len() - 1;
                        let (min, max) = (all[me].min, all[me].max);
                        let mut soup = rock::Soup::default();
                        let cover = rock::covers(min, max, me, &all);
                        rock::faceted_box(&mut soup, min, max, Vec3::new(cx, -0.1, cz), 0, [false; 6], &cover);
                        soup.mesh(dress(floor).map_or(4.0, |d| d.1))
                    })),
                    MeshMaterial3d(mats.add(StandardMaterial {
                        base_color: Color::srgb(0.42, 0.43, 0.45),
                        perceptual_roughness: 0.95,
                        reflectance: fresnel::DIELECTRIC,
                        ..default()
                    })),
                    Transform::from_xyz(cx, -0.1, cz),
                ));
            }
            commands.insert_resource(crate::look::cliffs::SkinLift(None));
        }
    }
    let lift = crate::look::built::of(id).and_then(|s| s.lift);
    for l in &map.launchers {
        let (a, b) = (l.area.min, l.area.max);
        let (cx, cz) = ((a.x + b.x) * 0.5, (a.z + b.z) * 0.5);
        commands.spawn((
            LevelPart,
            Mesh3d(meshes.add(Cuboid::new(b.x - a.x, 0.08, b.z - a.z))),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: lift.map_or(Color::srgb(0.27, 0.82, 1.0), |s| s.pad),
                emissive: lift.map_or(LinearRgba::rgb(0.1, 0.56, 0.75), |s| s.glow) * LAUNCHER_GLOW,
                reflectance: fresnel::DIELECTRIC,
                ..default()
            })),
            Transform::from_xyz(cx, a.y + 0.04, cz),
        ));
        // A lift: its beam stands to the throw's apex, drawn additive and
        // unlit so it reads as light, not a wall: brightest up its middle
        // and at its foot, with no edge and no lid.
        let r = (b.x - a.x).min(b.z - a.z) * LIFT_BEAM_R;
        let apex = l.vel.y * l.vel.y / (2.0 * sim::consts::GRAVITY) + 1.0;
        commands.spawn((
            LevelPart,
            Mesh3d(meshes.add(lift_beam(r, apex))),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: lift.map_or(Color::srgba(0.35, 0.8, 1.0, LIFT_BEAM_ALPHA), |s| s.beam),
                alpha_mode: AlphaMode::Add,
                unlit: true,
                cull_mode: None,
                double_sided: true,
                ..default()
            })),
            Transform::from_xyz(cx, a.y + apex * 0.5, cz),
            bevy::light::NotShadowCaster,
        ));
        commands.spawn((
            LevelPart,
            Mesh3d(meshes.add(Torus::new(r, r + 0.1))),
            MeshMaterial3d(mats.add(strip_material(Color::srgb(0.3, 0.95, 1.0), 0.3))),
            Transform::from_xyz(cx, a.y + 0.06, cz),
            bevy::light::NotShadowCaster,
        ));
    }
    // The map's sun (look::rig: its shadows, its disk), where its mood puts
    // it, as strong as the map says.
    let mood = crate::look::mood::of(id);
    let ev = if map.light.ev100 > 0.0 {
        map.light.ev100
    } else {
        mood.ev100
    };
    // With the graphics tier's cascades from its first frame: spawned where
    // the tier can be read.
    let k = map.light.sun;
    commands.queue(move |world: &mut World| {
        let tier = world
            .get_resource::<crate::look::quality::Tier>()
            .copied()
            .unwrap_or_default();
        world.spawn((LevelPart, crate::look::rig::sun(k, &mood, &tier, ev)));
    });
}

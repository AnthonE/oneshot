//! The level, from the sim's own map: its boxes, its solids (round, turned
//! and ramps: `sim::solid`), its ground, its launchers and its light. What
//! you see is what you collide with and what stops bullets.
//! Rebuilt whole on each welcome (a cup's next heat may be another map).

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use sim::terrain::HEIGHT_Q;

/// Everything a level spawned, to take down with it.
#[derive(Component)]
pub struct LevelPart;

/// The ground, waiting for its textured material (look::ground).
#[derive(Component)]
pub struct GroundMesh;

/// A box by its look, waiting for its textured material.
#[derive(Component)]
pub struct BoxLook(pub u8);

/// Given its textured material.
#[derive(Component)]
pub struct Dressed;

/// A block of grass, waiting for its atlas.
#[derive(Component)]
pub struct GrassPart;

/// A material's roughness over the photograph's, which averages about 0.6.
const ROUGH_SCALE: f32 = 1.5;

/// The arena floor's look (not a map look: a flat map's ground).
const FLOOR: u8 = 255;

/// A box's texture set, the metres it repeats over, and its tint (the
/// photograph's colour times this).
fn dress(look: u8) -> Option<(&'static str, f32, Color)> {
    Some(match look {
        0 => ("concrete", 3.0, Color::srgb(0.9, 0.88, 0.84)),
        1 => ("metal", 2.0, Color::srgb(0.85, 0.87, 0.9)),
        3 => ("panel", 4.0, Color::srgb(0.78, 0.36, 0.32)),
        4 => ("panel", 4.0, Color::srgb(0.32, 0.45, 0.78)),
        5 => ("metal", 2.0, Color::srgb(0.45, 0.47, 0.5)),
        6 => ("rock", 2.5, Color::srgb(0.95, 0.58, 0.42)),
        7 => ("panel", 4.0, Color::srgb(0.42, 0.46, 0.53)),
        11 => ("panel", 4.0, Color::srgb(0.78, 0.8, 0.83)),
        FLOOR => ("panel", 6.0, Color::srgb(0.34, 0.36, 0.39)),
        12 => ("alien", 8.0, Color::srgb(0.95, 0.95, 1.0)),
        13 => ("alien", 8.0, Color::srgb(0.42, 0.4, 0.52)),
        14 => ("alien", 8.0, Color::srgb(1.0, 0.55, 0.52)),
        15 => ("alien", 8.0, Color::srgb(0.55, 0.68, 1.0)),
        _ => return None,
    })
}

/// Faces as the meshes below are built: positions, normals, texture
/// coordinates (in metres; divided by the tile at the end), triangles.
#[derive(Default)]
struct Faces {
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

    fn mesh(self, tile: f32) -> Mesh {
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

/// A round solid: its side smooth (a facet every 25 cm or so of its
/// rim), its top a disc; about its own middle, y from 0 to `h`.
fn round_mesh(r: f32, h: f32, tile: f32) -> Mesh {
    let n = ((core::f32::consts::TAU * r / 0.25).ceil() as usize).clamp(24, 128);
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

/// A box's mesh with its texture laid in world metres, face by face, so
/// neighbouring boxes line up and nothing stretches.
fn box_mesh(size: Vec3, mid: Vec3, tile: f32) -> Mesh {
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
            let w = Vec3::from_array(*p) + mid;
            let n = Vec3::from_array(*n).abs();
            let (u, v) = if n.y > n.x && n.y > n.z {
                (w.x, w.z)
            } else if n.x > n.z {
                (w.z, -w.y)
            } else {
                (w.x, -w.y)
            };
            [u / tile, v / tile]
        })
        .collect();
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    let _ = m.generate_tangents();
    m
}

/// The level's ground and boxes in their textures, once those are ready.
pub fn dress_level(
    mut commands: Commands,
    tex: Res<crate::look::textures::Textures>,
    mut ground_mats: ResMut<Assets<crate::look::ground::GroundMaterial>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    ground: Query<Entity, (With<GroundMesh>, Without<Dressed>)>,
    boxes: Query<(Entity, &BoxLook), Without<Dressed>>,
    grass: Query<Entity, (With<GrassPart>, Without<Dressed>)>,
) {
    let Some((albedo, normal)) = tex.ground.clone() else {
        return;
    };
    if let Some(atlas) = tex.grass.clone() {
        if !grass.is_empty() {
            let m = mats.add(StandardMaterial {
                base_color_texture: Some(atlas),
                alpha_mode: AlphaMode::Mask(0.5),
                double_sided: true,
                cull_mode: None,
                perceptual_roughness: 0.9,
                ..default()
            });
            for e in grass.iter() {
                commands.entity(e).insert((MeshMaterial3d(m.clone()), Dressed));
            }
        }
    }
    for e in ground.iter() {
        let m = ground_mats.add(crate::look::ground::material(albedo.clone(), normal.clone()));
        commands
            .entity(e)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert((MeshMaterial3d(m), Dressed));
    }
    for (e, look) in boxes.iter() {
        let Some((set, _, tint)) = dress(look.0) else {
            commands.entity(e).insert(Dressed);
            continue;
        };
        let Some((albedo, normal)) = tex.set(set) else {
            continue;
        };
        let (roughness, metallic) = match (look.0, set) {
            (3 | 4 | 7 | 11 | FLOOR, _) => (0.75, 0.55),
            (12..=15, _) => (1.0, 0.7),
            (_, "metal") => (0.55, 0.35),
            _ => (0.9, 0.0),
        };
        // With the photographed roughness and occlusion, the numbers scale
        // the photograph's (its roughness is about 0.6 on average).
        let orm = tex.orm(set);
        let scale = if orm.is_some() { ROUGH_SCALE } else { 1.0 };
        // A set that glows (the alien hull's veins): in its look's colour.
        let glow = tex.emissive(set);
        let m = mats.add(StandardMaterial {
            base_color: tint,
            base_color_texture: Some(albedo),
            normal_map_texture: Some(normal),
            perceptual_roughness: (roughness * scale).min(1.0),
            metallic,
            metallic_roughness_texture: orm.clone(),
            occlusion_texture: orm,
            emissive: if glow.is_some() {
                vein(look.0) * VEIN_GLOW
            } else {
                LinearRgba::BLACK
            },
            emissive_texture: glow,
            ..default()
        });
        commands.entity(e).insert((MeshMaterial3d(m), Dressed));
    }
}

/// How bright a light strip is, over the display's white (HDR: past 1 it
/// blooms on the desktop), and a launcher's glow beside it.
const STRIP_GLOW: f32 = 4.0;
/// How bright the alien hull's veins glow, over the display's white.
const VEIN_GLOW: f32 = 3.0;

/// The veins' colour by look: cyan, faint on the dark hull, the side's on
/// a side's.
fn vein(look: u8) -> LinearRgba {
    LinearRgba::from(match look {
        13 => Color::srgb(0.2, 0.45, 0.6),
        14 => Color::srgb(1.0, 0.32, 0.22),
        15 => Color::srgb(0.28, 0.55, 1.0),
        _ => Color::srgb(0.35, 0.95, 1.0),
    })
}

/// How far a map's lamp reaches, m.
const LAMP_RANGE: f32 = 30.0;
const LAUNCHER_GLOW: f32 = 2.5;

/// A light strip's colour (looks `glow`, `red_glow`, `blue_glow`): drawn
/// unshadowed and emissive, the same in the sun and in a hangar.
fn strip(i: u8) -> Option<Color> {
    Some(match i {
        8 => Color::srgb(0.62, 0.88, 1.0),
        9 => Color::srgb(1.0, 0.36, 0.26),
        10 => Color::srgb(0.3, 0.55, 1.0),
        _ => return None,
    })
}

/// A box's look (tools/maps/compile.mjs LOOKS): its colour and roughness.
fn look(i: u8) -> (Color, f32, f32) {
    match i {
        7 => (Color::srgb(0.36, 0.39, 0.44), 0.45, 0.55),    // hull
        11 => (Color::srgb(0.66, 0.68, 0.71), 0.45, 0.55),   // plate
        12..=15 => (Color::srgb(0.45, 0.4, 0.6), 0.35, 0.7), // alien
        8..=10 => (strip(i).unwrap_or(Color::WHITE), 0.3, 0.0),
        0 => (Color::srgb(0.54, 0.56, 0.59), 0.9, 0.0),  // wall
        1 => (Color::srgb(0.62, 0.64, 0.67), 0.6, 0.2),  // block
        2 => (Color::srgb(0.79, 0.64, 0.15), 0.5, 0.4),  // trim
        3 => (Color::srgb(0.54, 0.33, 0.31), 0.6, 0.2),  // red
        4 => (Color::srgb(0.31, 0.39, 0.54), 0.6, 0.2),  // blue
        5 => (Color::srgb(0.29, 0.31, 0.34), 0.7, 0.2),  // dark
        _ => (Color::srgb(0.61, 0.33, 0.21), 0.95, 0.0), // rock
    }
}

/// The ground mesh: the sim's vertices and its split of each cell (low
/// corner to high corner), with its splat weights in the vertex colour
/// (sand, grass, gravel, rock): rock where it is steep, gravel down in the
/// creek, grass up on the hills, sand the rest.
fn terrain_mesh(t: &sim::terrain::TerrainDef) -> Mesh {
    let (nx, nz) = (t.nx as usize, t.nz as usize);
    let h = |i: usize, j: usize| t.heights[j * nx + i] as f32 / HEIGHT_Q;
    let mut pos = Vec::with_capacity(nx * nz);
    let mut col = Vec::with_capacity(nx * nz);
    for j in 0..nz {
        for i in 0..nx {
            pos.push([t.x0 + i as f32 * t.cell, h(i, j), t.z0 + j as f32 * t.cell]);
            let dx = (h((i + 1).min(nx - 1), j) - h(i.saturating_sub(1), j)) / (2.0 * t.cell);
            let dz = (h(i, (j + 1).min(nz - 1)) - h(i, j.saturating_sub(1))) / (2.0 * t.cell);
            let slope = (dx * dx + dz * dz).sqrt();
            let y = h(i, j);
            let rock = ((slope - 0.35) / 0.4).clamp(0.0, 1.0);
            let gravel = ((-y - 0.15) * 2.0).clamp(0.0, 1.0) * (1.0 - rock);
            let grass = ((y - 1.0) / 1.5).clamp(0.0, 1.0) * (1.0 - rock) * (1.0 - gravel);
            let sand = (1.0 - rock - gravel - grass).max(0.0);
            col.push([sand, grass, gravel, rock]);
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
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx));
    m.compute_smooth_normals();
    m
}

/// Spawns map `id`'s level; `old` is the last one's, taken down first.
pub fn build(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
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
    let (lo, hi) = (map.lo, map.hi);
    let (cx, cz) = ((lo.0 + hi.0) * 0.5, (lo.1 + hi.1) * 0.5);
    let half = (hi.0 - lo.0).max(hi.1 - lo.1) * 0.5 + 8.0;

    for (b, &l) in map.boxes.iter().zip(&map.looks) {
        let (color, roughness, metallic) = look(l);
        let size = b.max - b.min;
        let mid = (b.min + b.max) * 0.5;
        let tile = dress(l).map_or(2.0, |d| d.1);
        if let Some(c) = strip(l) {
            commands.spawn((
                LevelPart,
                Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
                MeshMaterial3d(mats.add(StandardMaterial {
                    base_color: c,
                    emissive: LinearRgba::from(c) * STRIP_GLOW,
                    perceptual_roughness: roughness,
                    ..default()
                })),
                Transform::from_xyz(mid.x, mid.y, mid.z),
                bevy::light::NotShadowCaster,
            ));
            continue;
        }
        commands.spawn((
            LevelPart,
            BoxLook(l),
            Mesh3d(meshes.add(box_mesh(
                Vec3::new(size.x, size.y, size.z),
                Vec3::new(mid.x, mid.y, mid.z),
                tile,
            ))),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: roughness,
                metallic,
                ..default()
            })),
            Transform::from_xyz(mid.x, mid.y, mid.z),
        ));
    }
    for (sol, &l) in map.solids.iter().zip(&map.solid_looks) {
        let (color, roughness, metallic) = look(l);
        let tile = dress(l).map_or(2.0, |d| d.1);
        let glow = strip(l);
        let mat = mats.add(match glow {
            Some(c) => StandardMaterial {
                base_color: c,
                emissive: LinearRgba::from(c) * STRIP_GLOW,
                perceptual_roughness: roughness,
                ..default()
            },
            None => StandardMaterial {
                base_color: color,
                perceptual_roughness: roughness,
                metallic,
                ..default()
            },
        });
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
        };
        let mut e = commands.spawn((LevelPart, Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat), at));
        if glow.is_some() {
            e.insert(bevy::light::NotShadowCaster);
        } else {
            e.insert(BoxLook(l));
        }
    }
    for &(p, c, lm) in map.light.lamps {
        commands.spawn((
            LevelPart,
            PointLight {
                color: Color::srgb(c[0], c[1], c[2]),
                intensity: lm,
                range: LAMP_RANGE,
                radius: 0.15,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_xyz(p[0], p[1], p[2]),
        ));
    }
    match &def.terrain {
        Some(t) => {
            for (mesh, centre) in crate::look::grass::blocks(t, lo, hi, id as u32 * 7919 + 1) {
                commands.spawn((
                    LevelPart,
                    GrassPart,
                    crate::look::grass::GrassBlock { centre },
                    Mesh3d(meshes.add(mesh)),
                    Transform::default(),
                    Visibility::Hidden,
                    bevy::light::NotShadowCaster,
                ));
            }
            commands.spawn((
                LevelPart,
                GroundMesh,
                Mesh3d(meshes.add(terrain_mesh(t))),
                MeshMaterial3d(mats.add(StandardMaterial {
                    base_color: Color::srgb(0.75, 0.62, 0.48),
                    perceptual_roughness: 0.95,
                    ..default()
                })),
                Transform::default(),
            ));
        }
        None => {
            // The floor as a slab whose top is y = 0, textured in world
            // metres like the boxes on it.
            let size = 2.0 * half + 40.0;
            let floor = if map.light.floor == 255 { FLOOR } else { map.light.floor };
            commands.spawn((
                LevelPart,
                BoxLook(floor),
                Mesh3d(meshes.add(box_mesh(
                    Vec3::new(size, 0.2, size),
                    Vec3::new(cx, -0.1, cz),
                    dress(floor).map_or(4.0, |d| d.1),
                ))),
                MeshMaterial3d(mats.add(StandardMaterial {
                    base_color: Color::srgb(0.42, 0.43, 0.45),
                    perceptual_roughness: 0.95,
                    ..default()
                })),
                Transform::from_xyz(cx, -0.1, cz),
            ));
        }
    }
    for l in &map.launchers {
        let (a, b) = (l.area.min, l.area.max);
        commands.spawn((
            LevelPart,
            Mesh3d(meshes.add(Cuboid::new(b.x - a.x, 0.08, b.z - a.z))),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: Color::srgb(0.27, 0.82, 1.0),
                emissive: LinearRgba::rgb(0.1, 0.56, 0.75) * LAUNCHER_GLOW,
                ..default()
            })),
            Transform::from_xyz((a.x + b.x) * 0.5, a.y + 0.04, (a.z + b.z) * 0.5),
        ));
    }
    for p in &map.pads {
        commands.spawn((
            LevelPart,
            Mesh3d(meshes.add(Cylinder::new(0.8, 0.06))),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: Color::srgb(0.79, 0.64, 0.15),
                metallic: 0.4,
                ..default()
            })),
            Transform::from_xyz(p.pos.x, p.pos.y + 0.03, p.pos.z),
        ));
    }
    // The sun at noon (look::rig: its shadows, its disk), as strong as the
    // map says.
    commands.spawn((LevelPart, crate::look::rig::sun(map.light.sun)));
}

//! The level, from the sim's own map: its boxes, its ground and its
//! launchers. What you see is what you collide with and what stops bullets.
//! Rebuilt whole on each welcome (a cup's next heat may be another map).

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use sim::terrain::HEIGHT_Q;

/// Everything a level spawned, to take down with it.
#[derive(Component)]
pub struct LevelPart;

/// A box's look (tools/maps/compile.mjs LOOKS): its colour and roughness.
fn look(i: u8) -> (Color, f32, f32) {
    match i {
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
/// corner to high corner), coloured by height and slope.
fn terrain_mesh(t: &sim::terrain::TerrainDef) -> Mesh {
    let (nx, nz) = (t.nx as usize, t.nz as usize);
    let h = |i: usize, j: usize| t.heights[j * nx + i] as f32 / HEIGHT_Q;
    let sand = Vec3::new(0.77, 0.6, 0.42);
    let grass = Vec3::new(0.43, 0.48, 0.23);
    let rock = Vec3::new(0.61, 0.33, 0.21);
    let mut pos = Vec::with_capacity(nx * nz);
    let mut col = Vec::with_capacity(nx * nz);
    for j in 0..nz {
        for i in 0..nx {
            pos.push([t.x0 + i as f32 * t.cell, h(i, j), t.z0 + j as f32 * t.cell]);
            let dx = (h((i + 1).min(nx - 1), j) - h(i.saturating_sub(1), j)) / (2.0 * t.cell);
            let dz = (h(i, (j + 1).min(nz - 1)) - h(i, j.saturating_sub(1))) / (2.0 * t.cell);
            let steep = ((dx * dx + dz * dz).sqrt() / 0.8).min(1.0);
            let c = sand.lerp(grass, (h(i, j) * 0.6).clamp(0.0, 1.0)).lerp(rock, steep);
            col.push([c.x, c.y, c.z, 1.0]);
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
        commands.spawn((
            LevelPart,
            Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            MeshMaterial3d(mats.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: roughness,
                metallic,
                ..default()
            })),
            Transform::from_xyz(mid.x, mid.y, mid.z),
        ));
    }
    match &def.terrain {
        Some(t) => {
            commands.spawn((
                LevelPart,
                Mesh3d(meshes.add(terrain_mesh(t))),
                MeshMaterial3d(mats.add(StandardMaterial {
                    perceptual_roughness: 0.95,
                    ..default()
                })),
                Transform::default(),
            ));
        }
        None => {
            let size = 2.0 * half + 40.0;
            commands.spawn((
                LevelPart,
                Mesh3d(meshes.add(Plane3d::default().mesh().size(size, size))),
                MeshMaterial3d(mats.add(StandardMaterial {
                    base_color: Color::srgb(0.42, 0.43, 0.45),
                    perceptual_roughness: 0.95,
                    ..default()
                })),
                Transform::from_xyz(cx, 0.0, cz),
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
                emissive: LinearRgba::rgb(0.1, 0.56, 0.75),
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
    // The sun, its shadows sized to the map.
    commands.spawn((
        LevelPart,
        DirectionalLight {
            illuminance: 11_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(cx - 30.0, 50.0, cz + 20.0).looking_at(Vec3::new(cx, 0.0, cz), Vec3::Y),
    ));
}

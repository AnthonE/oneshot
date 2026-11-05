//! The land beyond a canyon: higher ground standing back from its rim,
//! mesa over mesa out to a couple of kilometres, so there is something over
//! the walls but sky and a place the canyon is cut into.
//!
//! Scenery, like the sea's stacks (`boulders.rs`): nothing here is in the
//! sim, nobody reaches it, and it begins past the terrain's own edge, which
//! is already out of bounds. One mesh a map, a few thousand triangles: a
//! fan of rings round the terrain's rectangle, each ring further out and
//! coarser, its height a function of where it is ([`height`]): level with
//! the rim where it leaves the terrain, climbing in terraces as it goes,
//! cut by valleys, down under the water toward the sea. Coloured in its
//! vertices (rock on what is steep, scrub on what lies flat, paler with
//! distance), lit by the sun, casting and taking no shadow: the cascades
//! end long before it.
//!
//! From the floor the walls hide all of it but what stands high enough:
//! a wall's crest is 25° up from the far side of the floor, so a mesa
//! 400 m back shows only what it has over 200 m. That is why it climbs
//! ([`RISE_M`]).

use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::grass::hash01;
use crate::level::LevelPart;

/// Spokes round the terrain, and how far past its edge each ring stands,
/// metres: close together where the rim is near, wide where it is far.
const SPOKES: usize = 288;
const RINGS_M: [f32; 12] = [
    -6.0, 20.0, 50.0, 90.0, 140.0, 210.0, 300.0, 420.0, 580.0, 800.0, 1150.0, 1700.0,
];
/// The height the land leaves the terrain's edge at, under the rim so the
/// seam is behind it, and how much it climbs by [`RISE_OVER_M`] out.
const EDGE_Y: f32 = 46.0;
const RISE_M: f32 = 330.0;
const RISE_OVER_M: (f32, f32) = (30.0, 620.0);
/// The mesas: how far across, the share of the climb each terrace takes,
/// and a ridged relief on top, its height and how far across.
const MESA_M: f32 = 340.0;
const TERRACES: f32 = 4.0;
const RIDGE_H: f32 = 26.0;
const RIDGE_M: f32 = 90.0;
/// Toward the sea (a map with one): the land sinks under it from x this
/// far short of the bound, over this much more, as the canyon's own walls
/// do (Dry Fork's are down by x 168): it leaves the terrain's edge where
/// that edge is the seabed, and stood there as a wall across the water
/// when it sank later.
const SEA_FROM_M: f32 = -12.0;
const SEA_OVER_M: f32 = 32.0;
const SEA_UNDER_M: f32 = 26.0;
/// Colours, linear: the walls' red rock, a paler bed in it, the scrub on
/// the tops, and the air's far blue, which a point is `HAZE` of the way to
/// by `HAZE_M` (the atmosphere stops thickening at the camera's far plane).
const ROCK: [f32; 3] = [0.30, 0.15, 0.08];
const ROCK_PALE: [f32; 3] = [0.40, 0.26, 0.16];
const SCRUB: [f32; 3] = [0.17, 0.19, 0.08];
const AIR: [f32; 3] = [0.50, 0.62, 0.80];
const HAZE: f32 = 0.62;
const HAZE_M: f32 = 1500.0;

/// Smooth value noise in 0..1 at (x, z), one lattice cell a unit.
fn noise(x: f32, z: f32, salt: u32) -> f32 {
    let (ix, iz) = (x.floor(), z.floor());
    let (fx, fz) = (x - ix, z - iz);
    let (ux, uz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let h = |dx: i32, dz: i32| hash01((ix as i32 + dx) as u32 ^ salt, (iz as i32 + dz) as u32);
    let a = h(0, 0) + (h(1, 0) - h(0, 0)) * ux;
    let b = h(0, 1) + (h(1, 1) - h(0, 1)) * ux;
    a + (b - a) * uz
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The far land's height at (x, z), `out` metres past the terrain's edge,
/// on a map whose bound toward the sea is at x `sea_x` (the water at
/// `sea`; not finite: no sea).
pub fn height(x: f32, z: f32, out: f32, sea_x: f32, sea: f32, seed: u32) -> f32 {
    let climb = smooth(RISE_OVER_M.0, RISE_OVER_M.1, out);
    // Mesas: two octaves, cut into terraces with a short scarp between.
    let n = 0.65 * noise(x / MESA_M, z / MESA_M, seed)
        + 0.35 * noise(x / (MESA_M * 0.41) + 9.0, z / (MESA_M * 0.41) + 4.0, seed ^ 0x5bd1);
    let t = n * TERRACES;
    let step = t.floor() + smooth(0.62, 0.86, t - t.floor());
    let mesa = (step / TERRACES).clamp(0.0, 1.0);
    // Ridges: sharp along their crests, on the high ground more.
    let r = 1.0 - (2.0 * noise(x / RIDGE_M + 3.0, z / RIDGE_M + 7.0, seed ^ 0x9e37) - 1.0).abs();
    let mut h = EDGE_Y + RISE_M * climb * (0.3 + 0.7 * mesa) + RIDGE_H * r * r * climb;
    if sea.is_finite() {
        let wet = smooth(sea_x + SEA_FROM_M, sea_x + SEA_FROM_M + SEA_OVER_M, x);
        h += (sea - SEA_UNDER_M - h) * wet;
    }
    h
}

/// The mesh round a terrain whose rectangle is `lo..hi` (its own edge, past
/// the playable bound).
pub fn mesh(lo: Vec2, hi: Vec2, sea_x: f32, sea: f32, seed: u32) -> Mesh {
    let mid = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    let rings = RINGS_M.len();
    let mut pos: Vec<Vec3> = Vec::with_capacity(SPOKES * rings);
    let mut outs = Vec::with_capacity(SPOKES * rings);
    for s in 0..SPOKES {
        let a = s as f32 / SPOKES as f32 * core::f32::consts::TAU;
        let dir = Vec2::new(a.cos(), a.sin());
        // Where this spoke leaves the rectangle.
        let edge = (half.x / dir.x.abs().max(1e-4)).min(half.y / dir.y.abs().max(1e-4));
        for &out in &RINGS_M {
            let p = mid + dir * (edge + out);
            pos.push(Vec3::new(p.x, height(p.x, p.y, out.max(0.0), sea_x, sea, seed), p.y));
            outs.push(out.max(0.0));
        }
    }
    let at = |s: usize, r: usize| (s % SPOKES) * rings + r;
    // Each vertex's normal from its neighbours along and across the spokes.
    let mut nor = vec![Vec3::Y; pos.len()];
    for s in 0..SPOKES {
        for r in 0..rings {
            let along = pos[at(s, (r + 1).min(rings - 1))] - pos[at(s, r.saturating_sub(1))];
            let across = pos[at(s + 1, r)] - pos[at(s + SPOKES - 1, r)];
            nor[at(s, r)] = along.cross(across).normalize_or(Vec3::Y);
            if nor[at(s, r)].y < 0.0 {
                nor[at(s, r)] = -nor[at(s, r)];
            }
        }
    }
    let col: Vec<[f32; 4]> = (0..pos.len())
        .map(|i| {
            let (p, n) = (pos[i], nor[i]);
            // Scrub on what lies flat, rock on what stands, a pale bed
            // every sixty metres up.
            let flat = smooth(0.80, 0.94, n.y);
            let bed = smooth(0.55, 0.9, noise(p.y / 60.0, (p.x + p.z) / 900.0, 0x0bed));
            let mut c = [0.0f32; 3];
            for k in 0..3 {
                let rock = ROCK[k] + (ROCK_PALE[k] - ROCK[k]) * bed;
                let ground = rock + (SCRUB[k] - rock) * flat;
                let haze = HAZE * smooth(0.0, HAZE_M, outs[i]);
                c[k] = ground + (AIR[k] - ground) * haze;
            }
            [c[0], c[1], c[2], 1.0]
        })
        .collect();
    let mut idx = Vec::with_capacity(SPOKES * (rings - 1) * 6);
    for s in 0..SPOKES {
        for r in 0..rings - 1 {
            let (a, b, c, d) = (
                at(s, r) as u32,
                at(s + 1, r) as u32,
                at(s, r + 1) as u32,
                at(s + 1, r + 1) as u32,
            );
            // Up-facing: the spokes turn from +x toward +z.
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            pos.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            nor.iter().map(|n| n.to_array()).collect::<Vec<_>>(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

/// The far land of a map with a terrain, spawned with its level.
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    lo: Vec2,
    hi: Vec2,
    sea_x: f32,
    sea: f32,
    seed: u32,
) {
    commands.spawn((
        LevelPart,
        Mesh3d(meshes.add(mesh(lo, hi, sea_x, sea, seed))),
        MeshMaterial3d(mats.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 1.0,
            reflectance: 0.2,
            cull_mode: None,
            ..default()
        })),
        Transform::default(),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The land leaves the terrain under its rim, climbs as it goes, and is
    /// under the water toward the sea.
    #[test]
    fn it_climbs_away_from_the_rim_and_sinks_toward_the_sea() {
        let (sea_x, sea) = (150.0, -6.0);
        // At the edge, inland: the rim's level.
        assert!((height(-300.0, 0.0, 0.0, sea_x, sea, 3) - EDGE_Y).abs() < 1.0);
        // Far out, inland: high, somewhere.
        let far = (0..40)
            .map(|i| height(-900.0 - 7.0 * i as f32, 120.0 * (i % 5) as f32, 800.0, sea_x, sea, 3))
            .fold(f32::MIN, f32::max);
        assert!(far > 250.0, "the far land tops out at {far}");
        // Out to sea, and at the terrain's own edge off the beach: under it.
        assert!(height(sea_x + 400.0, 0.0, 300.0, sea_x, sea, 3) < sea);
        assert!(height(sea_x + 54.0, 20.0, 0.0, sea_x, sea, 3) < sea);
        // No sea, no sinking.
        assert!(height(900.0, 0.0, 800.0, sea_x, f32::NEG_INFINITY, 3) > EDGE_Y);
    }

    #[test]
    fn the_mesh_rings_the_terrain() {
        let m = mesh(Vec2::new(-200.0, -108.0), Vec2::new(210.0, 108.0), 150.0, -6.0, 3);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("no positions");
        };
        assert_eq!(p.len(), SPOKES * RINGS_M.len());
        // Nothing of it inside the terrain's own rectangle but the lap.
        for v in p {
            let inside = v[0] > -190.0 && v[0] < 200.0 && v[2] > -98.0 && v[2] < 98.0;
            assert!(!inside, "a far vertex at {v:?} is over the terrain");
        }
    }
}

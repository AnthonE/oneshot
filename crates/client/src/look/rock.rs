//! Rock: a box drawn as weathered stone. Each face is a grid of quads and
//! every vertex is pushed into the box by layered value noise (never out,
//! so the stone drawn stays inside what the sim collides with), the edges
//! and corners are rounded further, and horizontal strata darken the vertex
//! colour. A 3 m box reads as a boulder, a 16 m wall as a cliff face.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// How deep the weathering cuts: a share of the box's second-shortest side
/// (a wall is thin but tall, and its crags are sized to its height), capped
/// in metres; a boulder (all sides short) at least this share of its size.
const DEPTH_SHARE: f32 = 0.12;
const DEPTH_MAX_M: f32 = 4.0;
const BOULDER_DEPTH: f32 = 0.15;
/// The rounding of edges and corners: how far in from an edge it reaches
/// as a share of the shortest side (capped, m), and how deep it cuts at the
/// edge as a share of that reach.
const ROUND_REACH: f32 = 0.45;
const ROUND_REACH_MAX_M: f32 = 2.0;
const ROUND_DEPTH: f32 = 0.7;
/// A seam is also cut back into the rock, a ledge, this share of the
/// weathering's depth.
const LEDGE: f32 = 0.7;
/// The grid's step: a share of the second-shortest side, clamped, m.
const STEP_SHARE: f32 = 0.08;
const STEP_MIN_M: f32 = 0.4;
const STEP_MAX_M: f32 = 1.5;
/// The noise's octaves: metres each repeats over, and its weight (the
/// longest folds a cliff into buttresses).
const OCTAVES: [(f32, f32); 4] = [(28.0, 0.42), (7.0, 0.3), (2.4, 0.18), (0.9, 0.1)];
/// The strata: metres a band repeats over (varying along the rock between
/// these shares of it), how far the bands wander up and down, how dark a
/// band's seam is against its face, and how narrow the seam is (a power).
const STRATA_M: f32 = 3.2;
const STRATA_VARY: (f32, f32) = (0.7, 2.0);
const STRATA_WANDER: f32 = 1.2;
const STRATA_DARK: f32 = 0.7;
const SEAM_POWER: f32 = 2.5;
/// How much darker the deepest cut is than the crag beside it: grime.
const GRIME: f32 = 0.25;
/// A box thinner than this many cuts is a wall at the map's edge: only its
/// face toward the middle is cut, its back stays flat and is seen from
/// nowhere.
const WALL_CUTS: f32 = 2.2;
/// A wall's foot, where fighters walk, is cut only this share of the depth
/// (the sim stops them at the box, so the stone they touch must be near
/// it); the cut grows to full between these shares of the wall's height.
const FOOT_CUT: f32 = 0.15;
const FOOT_FROM: f32 = 0.3;
const FOOT_TO: f32 = 0.6;

fn hash3(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ (z as u32).wrapping_mul(0xcb1a_b31f)
        ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0
}

/// Smooth value noise in 0..1 at `p`, on a 1 m lattice.
fn vnoise(p: Vec3, seed: u32) -> f32 {
    let f = p.floor();
    let t = p - f;
    let s = t * t * (3.0 - 2.0 * t);
    let (x, y, z) = (f.x as i32, f.y as i32, f.z as i32);
    let c = |dx: i32, dy: i32, dz: i32| hash3(x + dx, y + dy, z + dz, seed);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(c(0, 0, 0), c(1, 0, 0), s.x);
    let x10 = lerp(c(0, 1, 0), c(1, 1, 0), s.x);
    let x01 = lerp(c(0, 0, 1), c(1, 0, 1), s.x);
    let x11 = lerp(c(0, 1, 1), c(1, 1, 1), s.x);
    lerp(lerp(x00, x10, s.y), lerp(x01, x11, s.y), s.z)
}

/// Layered noise in 0..1 at `p` (metres).
pub fn fbm(p: Vec3, seed: u32) -> f32 {
    OCTAVES
        .iter()
        .enumerate()
        .map(|(i, &(m, w))| w * vnoise(p / m, seed.wrapping_add(i as u32 * 101)))
        .sum()
}

/// The box from `min` to `max` as stone, its cuts facing `toward` (the
/// map's middle) when it is a thin wall. Texture coordinates in metres over
/// `tile`, laid flat by each face's plane.
pub fn rock(min: Vec3, max: Vec3, toward: Vec3, seed: u32, tile: f32) -> Mesh {
    let size = max - min;
    let mid = (min + max) * 0.5;
    let mut sides = [size.x, size.y, size.z];
    sides.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    let (thin, second) = (sides[0], sides[1]);
    let depth = (DEPTH_SHARE * second).min(DEPTH_MAX_M).max(BOULDER_DEPTH * thin);
    let reach = (ROUND_REACH * thin).min(ROUND_REACH_MAX_M);
    // Which way a point on the box's surface is cut: into the box, and on an
    // edge or a corner between its faces, so the faces stay joined there.
    let eps = 1e-3 * thin.max(0.01);
    let dir = |p: Vec3| -> Vec3 {
        let mut d = Vec3::ZERO;
        for k in 0..3 {
            if (p[k] - min[k]).abs() < eps {
                d[k] -= 1.0;
            }
            if (p[k] - max[k]).abs() < eps {
                d[k] += 1.0;
            }
        }
        d.normalize_or_zero()
    };
    let toward_dir = (toward - mid).normalize_or_zero();
    let step = (STEP_SHARE * second).clamp(STEP_MIN_M, STEP_MAX_M);
    let wall = thin < WALL_CUTS * depth;
    let mut pos: Vec<Vec3> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    for axis in 0..3 {
        let (a, b) = match axis {
            0 => (1, 2),
            1 => (2, 0),
            _ => (0, 1),
        };
        let (la, lb) = (size[a], size[b]);
        let (na, nb) = (
            (la / step).ceil().max(1.0) as usize,
            (lb / step).ceil().max(1.0) as usize,
        );
        for sign in [-1.0f32, 1.0] {
            let mut n = Vec3::ZERO;
            n[axis] = sign;
            let base = pos.len() as u32;
            for j in 0..=nb {
                for i in 0..=na {
                    let (u, v) = (i as f32 / na as f32, j as f32 / nb as f32);
                    let mut p = mid;
                    p[axis] = mid[axis] + sign * size[axis] * 0.5;
                    p[a] = min[a] + u * la;
                    p[b] = min[b] + v * lb;
                    let ea = (p[a] - min[a]).min(max[a] - p[a]);
                    let eb = (p[b] - min[b]).min(max[b] - p[b]);
                    let round = 1.0 - (ea.min(eb) / reach).min(1.0);
                    let noise = fbm(p, seed);
                    let wander = STRATA_WANDER * vnoise(Vec3::new(p.x, 0.0, p.z) / 9.0, seed ^ 77);
                    let vary = vnoise(Vec3::new(p.x, 0.0, p.z) / 50.0, seed ^ 31);
                    let period = STRATA_M * (STRATA_VARY.0 + (STRATA_VARY.1 - STRATA_VARY.0) * vary);
                    let band = 0.5 + 0.5 * ((p.y / period + wander) * core::f32::consts::TAU).sin();
                    // The seam: narrow at the band's floor, the face between seams wide.
                    let seam = (1.0 - band).powf(SEAM_POWER);
                    let d = dir(p);
                    let facing = if wall {
                        let rise = ((p.y - min.y) / size.y.max(1e-3) - FOOT_FROM) / (FOOT_TO - FOOT_FROM);
                        let rise = rise.clamp(0.0, 1.0);
                        let foot = FOOT_CUT + (1.0 - FOOT_CUT) * rise * rise * (3.0 - 2.0 * rise);
                        toward_dir.dot(d).max(0.0) * foot
                    } else {
                        1.0
                    };
                    let cut = facing * (depth * noise + LEDGE * depth * seam + ROUND_DEPTH * reach * round * round);
                    pos.push(p - d * cut);
                    let planar = match axis {
                        0 => [p.z, -p.y],
                        1 => [p.x, p.z],
                        _ => [p.x, -p.y],
                    };
                    uv.push([planar[0] / tile, planar[1] / tile]);
                    let dark = STRATA_DARK + (1.0 - STRATA_DARK) * (1.0 - seam);
                    let m = dark * (1.0 - GRIME * (cut / (depth + 1e-3)).min(1.0));
                    col.push([m, m * (0.97 + 0.03 * band), m * (0.92 + 0.08 * band), 1.0]);
                }
            }
            let w = (na + 1) as u32;
            for j in 0..nb as u32 {
                for i in 0..na as u32 {
                    let i0 = base + j * w + i;
                    let (i1, i2, i3) = (i0 + 1, i0 + w, i0 + w + 1);
                    // The grid's u runs along `a` and v along `b`, and a × b
                    // is +axis: wound to face +axis, flipped for the other side.
                    if sign > 0.0 {
                        idx.extend_from_slice(&[i0, i1, i3, i0, i3, i2]);
                    } else {
                        idx.extend_from_slice(&[i0, i3, i1, i0, i2, i3]);
                    }
                }
            }
        }
    }
    // Smooth normals over each face (the faces are separate islands, so a
    // box's edge stays an edge).
    let mut nor = vec![Vec3::ZERO; pos.len()];
    for t in idx.chunks(3) {
        let (p0, p1, p2) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let n = (p1 - p0).cross(p2 - p0);
        for &k in t {
            nor[k as usize] += n;
        }
    }
    let nor: Vec<[f32; 3]> = nor.iter().map(|n| n.normalize_or(Vec3::Y).to_array()).collect();
    let pos: Vec<[f32; 3]> = pos.iter().map(|p| p.to_array()).collect();
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx));
    let _ = m.generate_tangents();
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stone_stays_inside_its_box_and_is_cut_into() {
        let (min, max) = (Vec3::new(-1.5, -1.5, -1.5), Vec3::new(1.5, 1.6, 1.5));
        let m = rock(min, max, Vec3::ZERO, 3, 2.5);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("no positions");
        };
        let mut deepest = 0.0f32;
        for p in pos {
            let p = Vec3::from_array(*p);
            assert!(
                p.cmpge(min - 1e-4).all() && p.cmple(max + 1e-4).all(),
                "{p} is outside the box"
            );
            let inside = (p - min).min(max - p).min_element();
            deepest = deepest.max(inside);
        }
        assert!(deepest > 0.2, "nothing was cut in: {deepest}");
    }

    #[test]
    fn a_wall_keeps_its_back_flat() {
        // A thin wall at z = 30 with the map's middle at the origin: its
        // -z face is cut, its +z face is the plane it was given.
        let (min, max) = (Vec3::new(-20.0, 0.0, 30.0), Vec3::new(20.0, 16.0, 31.0));
        let m = rock(min, max, Vec3::ZERO, 1, 2.5);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("no positions");
        };
        let back = pos.iter().filter(|p| (p[2] - 31.0).abs() < 1e-4).count();
        let cut = pos.iter().filter(|p| p[2] > 30.05 && p[2] < 30.95).count();
        assert!(back > 100, "the back is not flat: {back}");
        assert!(cut > 100, "the front is not cut: {cut}");
    }
}

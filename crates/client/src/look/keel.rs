//! An island's keel (`level::Land::Isle`): the rock it hangs from, drawn
//! under it in place of the heightfield's column. The sim's terrain drops
//! straight down from the coast to its floor far under the void, which
//! from off the edge reads as a pillar standing in the cloud. Under the
//! void (`sim::map::Map::void`), where nobody is alive, the ground and its
//! rock skin are cut off ([`clip_below`]) and the keel hangs there instead:
//! rings under the island's own outline, each further down drawn in
//! further and lobed, to a point, and fingers of rock hanging off its
//! shoulders.
//!
//! Drawing only, and inside what collides (the column it replaces is the
//! sim's): only rounds fired down past the void could tell, and they stop
//! on the column where nothing is drawn. Drawn in the ground's material,
//! its weights all rock, so it is the island's own stone.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues as V};
use bevy::prelude::*;
use sim::terrain::{Terrain, TerrainDef};

/// The outline is sampled on this many spokes from the island's middle.
const SPOKES: usize = 160;
/// Its rings: metres under the cut, the share of the island's own reach
/// each keeps, and how far its edge wanders in and out (a share of it).
const RINGS: [(f32, f32, f32); 10] = [
    (0.0, 1.0, 0.0),
    (2.5, 0.96, 0.03),
    (6.0, 0.88, 0.07),
    (10.0, 0.78, 0.11),
    (15.0, 0.67, 0.15),
    (21.0, 0.56, 0.18),
    (29.0, 0.45, 0.2),
    (39.0, 0.34, 0.22),
    (51.0, 0.22, 0.22),
    (64.0, 0.1, 0.2),
];
/// Its point, metres under the cut.
const TIP_M: f32 = 78.0;
/// The fingers: how many, and their length and their base's radius,
/// metres (each its own, between).
const FINGERS: usize = 11;
const FINGER_LEN_M: (f32, f32) = (9.0, 26.0);
const FINGER_R_M: (f32, f32) = (1.2, 3.4);
const FINGER_SIDES: usize = 10;

fn hash(a: u32, b: u32) -> f32 {
    let mut h = a.wrapping_mul(0x8da6_b343) ^ b.wrapping_mul(0xd816_3841);
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb_352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    (h >> 8) as f32 / 16_777_216.0
}

/// A wander round the keel, `-1..1`, smooth and periodic in `a` (radians):
/// a few sines of whole turns, their phases from `seed` and drifting with
/// `depth`, so the lobes twist as they go down.
fn wander(a: f32, depth: f32, seed: u32) -> f32 {
    let mut s = 0.0;
    let mut w = 0.0;
    for (i, m) in [2.0f32, 3.0, 5.0, 8.0, 13.0].into_iter().enumerate() {
        let amp = 1.0 / (1.0 + i as f32 * 0.6);
        let phase = hash(seed, i as u32) * core::f32::consts::TAU + depth * 0.04 * (hash(seed, 10 + i as u32) - 0.5);
        s += (m * a + phase).sin() * amp;
        w += amp;
    }
    s / w
}

/// The island's middle and its reach from there along each spoke: where the
/// ground first falls under `top`, found to a centimetre.
fn outline(t: &Terrain, cells: (Vec2, Vec2), top: f32) -> (Vec2, Vec<f32>) {
    let (lo, hi) = cells;
    // The middle: the mean of what stands over the cut, on a metre's grid.
    let (mut sum, mut n) = (Vec2::ZERO, 0.0);
    let mut z = lo.y;
    while z <= hi.y {
        let mut x = lo.x;
        while x <= hi.x {
            if t.height(x, z).is_some_and(|h| h >= top) {
                sum += Vec2::new(x, z);
                n += 1.0;
            }
            x += 1.0;
        }
        z += 1.0;
    }
    let mid = if n > 0.0 { sum / n } else { (lo + hi) * 0.5 };
    let far = (hi - lo).length();
    let above = |r: f32, d: Vec2| {
        let p = mid + d * r;
        t.height(p.x, p.y).is_some_and(|h| h >= top)
    };
    let reach = (0..SPOKES)
        .map(|k| {
            let a = k as f32 / SPOKES as f32 * core::f32::consts::TAU;
            let d = Vec2::new(a.cos(), a.sin());
            let mut r = 0.0;
            while r < far && above(r + 0.25, d) {
                r += 0.25;
            }
            let (mut a0, mut a1) = (r, r + 0.25);
            for _ in 0..8 {
                let m = (a0 + a1) * 0.5;
                if above(m, d) {
                    a0 = m;
                } else {
                    a1 = m;
                }
            }
            a0
        })
        .collect();
    (mid, reach)
}

/// The keel under terrain `t`, hung from `top` (the map's void), keyed by
/// `seed`. Its vertex colour is the ground's splat, all rock.
pub fn mesh(t: &TerrainDef, top: f32, seed: u32) -> Mesh {
    let terrain = Terrain::build(t);
    let lo = Vec2::new(t.x0, t.z0);
    let hi = lo + Vec2::new((t.nx - 1) as f32, (t.nz - 1) as f32) * t.cell;
    let (mid, reach) = outline(&terrain, (lo, hi), top);
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    // The rings, then the point.
    for (k, &(down, keep, lobe)) in RINGS.iter().enumerate() {
        for (i, &r) in reach.iter().enumerate() {
            let a = i as f32 / SPOKES as f32 * core::f32::consts::TAU;
            let rr = r * keep * (1.0 + lobe * wander(a, down, seed));
            // The first ring is the outline itself, a hair inside so the
            // cut ground covers the seam.
            let rr = if k == 0 { r - 0.05 } else { rr };
            pos.push([mid.x + a.cos() * rr, top - down, mid.y + a.sin() * rr]);
        }
    }
    let tip = pos.len() as u32;
    pos.push([mid.x, top - TIP_M, mid.y]);
    let at = |k: usize, i: usize| (k * SPOKES + i % SPOKES) as u32;
    for k in 0..RINGS.len() - 1 {
        for i in 0..SPOKES {
            tris.push([at(k, i), at(k + 1, i), at(k, i + 1)]);
            tris.push([at(k, i + 1), at(k + 1, i), at(k + 1, i + 1)]);
        }
    }
    let last = RINGS.len() - 1;
    for i in 0..SPOKES {
        tris.push([at(last, i), tip, at(last, i + 1)]);
    }
    // Every face out from the keel's axis.
    orient(&pos, &mut tris, |y| Vec3::new(mid.x, y, mid.y));
    // The fingers: off the keel's shoulders, under its second and third
    // rings, each a cone to a point.
    for f in 0..FINGERS as u32 {
        let i = (hash(seed ^ 0xf1, f) * SPOKES as f32) as usize % SPOKES;
        let a = i as f32 / SPOKES as f32 * core::f32::consts::TAU;
        let ring = 2 + (hash(seed ^ 0xf2, f) * 2.0) as usize;
        let (down, keep, _) = RINGS[ring];
        // A finger is narrow beside its island: within a sixth of the
        // keel's reach there, so it hangs inside the outline.
        let r_keel = reach[i] * keep;
        let r_base = (FINGER_R_M.0 + (FINGER_R_M.1 - FINGER_R_M.0) * hash(seed ^ 0xf3, f)).min(r_keel / 6.0);
        let len = FINGER_LEN_M.0 + (FINGER_LEN_M.1 - FINGER_LEN_M.0) * hash(seed ^ 0xf4, f);
        let r = r_keel - r_base * 1.3;
        let c = Vec3::new(mid.x + a.cos() * r, top - down + 0.5, mid.y + a.sin() * r);
        let base = pos.len() as u32;
        for s in 0..FINGER_SIDES {
            let b = s as f32 / FINGER_SIDES as f32 * core::f32::consts::TAU;
            let w = 1.0 + 0.25 * (hash(seed ^ 0xf5, f * 64 + s as u32) - 0.5);
            pos.push((c + Vec3::new(b.cos(), 0.0, b.sin()) * r_base * w).to_array());
        }
        let point = pos.len() as u32;
        let end = c + Vec3::new(0.0, -len, 0.0);
        pos.push(end.to_array());
        let mut cone: Vec<[u32; 3]> = (0..FINGER_SIDES as u32)
            .map(|s| [base + s, point, base + (s + 1) % FINGER_SIDES as u32])
            .collect();
        // Out from its own axis, base to point.
        orient(&pos, &mut cone, |y| c.lerp(end, ((c.y - y) / len).clamp(0.0, 1.0)));
        tris.extend(cone);
    }
    let n = pos.len();
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32, 0.0, 0.0, 1.0]; n])
        .with_inserted_indices(Indices::U32(tris.into_iter().flatten().collect()));
    m.compute_smooth_normals();
    m
}

/// Turns each triangle to face away from the axis, which `axis` gives at
/// a height.
fn orient(pos: &[[f32; 3]], tris: &mut [[u32; 3]], axis: impl Fn(f32) -> Vec3) {
    for t in tris.iter_mut() {
        let [a, b, c] = t.map(|i| Vec3::from_array(pos[i as usize]));
        let n = (b - a).cross(c - a);
        let centre = (a + b + c) / 3.0;
        if n.dot(centre - axis(centre.y)) < 0.0 {
            t.swap(1, 2);
        }
    }
}

/// `mesh` with everything under height `y` cut away: each triangle kept,
/// dropped or cut where it crosses, every attribute carried to the cut
/// along its edge.
pub fn clip_below(mesh: &Mesh, y: f32) -> Mesh {
    let Some(V::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return mesh.clone();
    };
    let idx: Vec<u32> = match mesh.indices() {
        Some(Indices::U32(i)) => i.clone(),
        Some(Indices::U16(i)) => i.iter().map(|&v| v as u32).collect(),
        None => (0..pos.len() as u32).collect(),
    };
    // Each new vertex: an old one, or a point between two (a, b, t).
    let mut made: Vec<(u32, u32, f32)> = Vec::new();
    let mut cut: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
    let mut kept: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut keep = |v: u32, made: &mut Vec<(u32, u32, f32)>| {
        *kept.entry(v).or_insert_with(|| {
            made.push((v, v, 0.0));
            made.len() as u32 - 1
        })
    };
    let mut between = |a: u32, b: u32, made: &mut Vec<(u32, u32, f32)>| {
        let key = (a.min(b), a.max(b));
        *cut.entry(key).or_insert_with(|| {
            let (ya, yb) = (pos[key.0 as usize][1], pos[key.1 as usize][1]);
            made.push((key.0, key.1, (y - ya) / (yb - ya)));
            made.len() as u32 - 1
        })
    };
    let mut out: Vec<u32> = Vec::new();
    for t in idx.chunks_exact(3) {
        let t = [t[0], t[1], t[2]];
        let up = t.map(|v| pos[v as usize][1] >= y);
        match up.iter().filter(|&&u| u).count() {
            0 => {}
            3 => {
                for v in t {
                    let k = keep(v, &mut made);
                    out.push(k);
                }
            }
            n => {
                // Rotate so the lone one (in, for one; out, for two) is first,
                // keeping the winding.
                let lone = (0..3).find(|&i| up[i] == (n == 1)).unwrap_or(0);
                let (a, b, c) = (t[lone], t[(lone + 1) % 3], t[(lone + 2) % 3]);
                let ab = between(a, b, &mut made);
                let ac = between(a, c, &mut made);
                if n == 1 {
                    let ka = keep(a, &mut made);
                    out.extend_from_slice(&[ka, ab, ac]);
                } else {
                    let (kb, kc) = (keep(b, &mut made), keep(c, &mut made));
                    out.extend_from_slice(&[ab, kb, kc, ab, kc, ac]);
                }
            }
        }
    }
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    for (attr, values) in mesh.attributes() {
        let lerp =
            |a: &[f32], b: &[f32], t: f32| -> Vec<f32> { a.iter().zip(b).map(|(x, y)| x + (y - x) * t).collect() };
        let carried = match values {
            V::Float32x2(v) => V::Float32x2(
                made.iter()
                    .map(|&(a, b, t)| {
                        let l = lerp(&v[a as usize], &v[b as usize], t);
                        [l[0], l[1]]
                    })
                    .collect(),
            ),
            V::Float32x3(v) => V::Float32x3(
                made.iter()
                    .map(|&(a, b, t)| {
                        let l = lerp(&v[a as usize], &v[b as usize], t);
                        [l[0], l[1], l[2]]
                    })
                    .collect(),
            ),
            V::Float32x4(v) => V::Float32x4(
                made.iter()
                    .map(|&(a, b, t)| {
                        let l = lerp(&v[a as usize], &v[b as usize], t);
                        [l[0], l[1], l[2], l[3]]
                    })
                    .collect(),
            ),
            _ => continue,
        };
        m.insert_attribute(*attr, carried);
    }
    // A carried normal is no longer of unit length.
    if let Some(V::Float32x3(n)) = m.attribute_mut(Mesh::ATTRIBUTE_NORMAL) {
        for v in n.iter_mut() {
            *v = Vec3::from_array(*v).normalize_or(Vec3::Y).to_array();
        }
    }
    m.insert_indices(Indices::U32(out));
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 12 × 12 m: a 4 m square standing at 0 in the middle, 40 m down
    /// round it.
    fn island() -> TerrainDef {
        let n = 13usize;
        let heights: Vec<i16> = (0..n * n)
            .map(|k| {
                let (i, j) = ((k % n) as i32 - 6, (k / n) as i32 - 6);
                if i.abs() <= 2 && j.abs() <= 2 {
                    0
                } else {
                    (-40.0 * sim::terrain::HEIGHT_Q) as i16
                }
            })
            .collect();
        TerrainDef {
            cell: 1.0,
            x0: -6.0,
            z0: -6.0,
            nx: n as u16,
            nz: n as u16,
            heights: Box::leak(heights.into_boxed_slice()),
        }
    }

    fn positions(m: &Mesh) -> &Vec<[f32; 3]> {
        let Some(V::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("positions");
        };
        p
    }

    /// The keel hangs from the cut, inside the island's outline at every
    /// depth, down to its point.
    #[test]
    fn the_keel_hangs_inside_the_outline_from_the_cut() {
        let m = mesh(&island(), -10.0, 7);
        let p = positions(&m);
        let top = p.iter().map(|v| v[1]).fold(f32::MIN, f32::max);
        let bottom = p.iter().map(|v| v[1]).fold(f32::MAX, f32::min);
        assert!((-10.01..=-9.4).contains(&top), "{top}");
        assert!((bottom - (-10.0 - TIP_M)).abs() < 1e-3, "{bottom}");
        // The square reaches 2 m from its middle (the cut falls a little
        // past it, where the cliff's cell crosses -10).
        for v in p {
            assert!(v[0].abs() <= 3.2 && v[2].abs() <= 3.2, "{v:?}");
        }
    }

    /// A clip keeps what stands over the cut, drops what is under it, and
    /// ends the cut faces at the cut, every attribute carried.
    #[test]
    fn a_clip_cuts_at_the_height() {
        let m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_POSITION,
                vec![[0.0, 0.0, 0.0], [1.0, -20.0, 0.0], [0.0, -20.0, 1.0], [5.0, -30.0, 5.0]],
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32, 0.0, 0.0, 1.0]; 4])
            .with_inserted_indices(Indices::U32(vec![0, 1, 2, 1, 3, 2]));
        let c = clip_below(&m, -10.0);
        let p = positions(&c);
        assert!(p.iter().all(|v| v[1] >= -10.0 - 1e-4));
        assert_eq!(c.indices().unwrap().len(), 3);
        assert!(p
            .iter()
            .any(|v| (v[1] + 10.0).abs() < 1e-4 && (v[0] - 0.5).abs() < 1e-4));
        let Some(V::Float32x4(col)) = c.attribute(Mesh::ATTRIBUTE_COLOR) else {
            panic!("colour");
        };
        assert_eq!(col.len(), p.len());
    }
}

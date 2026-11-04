//! Grass: tufts of crossed cards cut from a photographed atlas, scattered
//! over the ground from the map's seed (thick on the grassy hills, a dry
//! tuft here and there on the sand, none on rock or in the creek), merged
//! a mesh per 16 m block, and drawn only near the eye. Cosmetic: never in
//! the sim, never in the way of a shot.
//!
//! Built as Gates' `render/clutter.rs` builds a tuft (`card_dry`; MIT,
//! NOTICE): three cards a tuft at a hashed yaw, each from a random cell of
//! the 2 × 2 atlas, height jittered, roots sunk a little, straw where it's
//! dry. Gates' wind (`foliage.wgsl`) is still to come.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use sim::terrain::{TerrainDef, HEIGHT_Q};

/// Gates' tuft: 0.4 m tall, three cards, twice as wide as tall, sunk 4%.
pub const TUFT_H: f32 = 0.40;
const CARDS_PER_TUFT: u32 = 3;
const CARD_ASPECT: f32 = 2.0;
const CARD_SINK: f32 = 0.04;
/// The atlas's cells.
const COLS: u32 = 2;
const ROWS: u32 = 2;
/// The scatter's cell, and a block's side.
const STEP_M: f32 = 0.7;
pub const BLOCK_M: f32 = 16.0;
/// Past this a block isn't drawn (Gates fades grass out by 46 m).
pub const DRAW_M: f32 = 46.0;
/// What a dry tuft's photograph is multiplied by, and a green one's.
const STRAW: [f32; 3] = [1.35, 1.12, 0.62];
const GREEN: [f32; 3] = [0.95, 1.0, 0.85];

#[derive(Component)]
pub struct GrassBlock {
    pub centre: Vec2,
}

fn hash01(seed: u32, k: u32) -> f32 {
    let mut h = seed.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 13;
    (h >> 8) as f32 / 16_777_216.0
}

/// The ground's height at (x, z), as the sim's triangles have it.
pub fn ground(t: &TerrainDef, x: f32, z: f32) -> Option<f32> {
    let (nx, nz) = (t.nx as usize, t.nz as usize);
    let u = (x - t.x0) / t.cell;
    let v = (z - t.z0) / t.cell;
    if u < 0.0 || v < 0.0 || u > (nx - 1) as f32 || v > (nz - 1) as f32 {
        return None;
    }
    let (i, j) = ((u as usize).min(nx - 2), (v as usize).min(nz - 2));
    let (fu, fv) = (u - i as f32, v - j as f32);
    let h = |a: usize, b: usize| t.heights[b * nx + a] as f32 / HEIGHT_Q;
    let (h00, h10, h01, h11) = (h(i, j), h(i + 1, j), h(i, j + 1), h(i + 1, j + 1));
    Some(if fu >= fv {
        h00 + (h10 - h00) * fu + (h11 - h10) * fv
    } else {
        h00 + (h11 - h01) * fu + (h01 - h00) * fv
    })
}

/// The grass is patchy: the rock's noise at this scale (patches a few
/// metres across) bares the ground this far, in the splat and the tufts
/// alike.
const PATCH_SCALE: f32 = 0.5;
const PATCH_BARE: f32 = 0.75;

/// How much of the grass at (x, z) is left by its patchiness, 0..1.
pub fn patch(x: f32, z: f32) -> f32 {
    1.0 - PATCH_BARE + PATCH_BARE * super::rock::fbm(Vec3::new(x, 0.0, z) * PATCH_SCALE, 5)
}

/// How much grass grows at (x, z): (green, dry), each 0..1. The splat's own
/// rule (level.rs): grass up the hills, none on steep rock or in the creek,
/// in patches.
fn growth(t: &TerrainDef, x: f32, z: f32) -> Option<(f32, f32)> {
    let y = ground(t, x, z)?;
    let dx = ground(t, x + 0.5, z)? - ground(t, x - 0.5, z)?;
    let dz = ground(t, x, z + 0.5)? - ground(t, x, z - 0.5)?;
    let slope = (dx * dx + dz * dz).sqrt();
    if slope > 0.55 || y < -0.12 {
        return None;
    }
    let green = ((y - 1.0) / 1.5).clamp(0.0, 1.0) * patch(x, z);
    Some((green, 0.07))
}

#[derive(Default)]
struct Soup {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    col: Vec<[f32; 4]>,
    idx: Vec<u32>,
}

impl Soup {
    fn tuft(&mut self, at: Vec3, yaw: f32, seed: u32, h: f32, tint: [f32; 3]) {
        let root = at - Vec3::Y * h * CARD_SINK;
        let (du, dv) = (1.0 / COLS as f32, 1.0 / ROWS as f32);
        for i in 0..CARDS_PER_TUFT {
            let a = yaw + i as f32 * core::f32::consts::PI / CARDS_PER_TUFT as f32;
            let side = Vec3::new(a.sin(), 0.0, a.cos());
            let hj = h * (0.8 + 0.4 * hash01(seed, i + 5));
            let half_w = hj * CARD_ASPECT * 0.5;
            let cell = (hash01(seed, i + 91) * (COLS * ROWS) as f32) as u32 % (COLS * ROWS);
            let (cu, cv) = ((cell % COLS) as f32 * du, (cell / COLS) as f32 * dv);
            let (b0, b1) = (root - side * half_w, root + side * half_w);
            let (t0, t1) = (b0 + Vec3::Y * hj, b1 + Vec3::Y * hj);
            let v = 0.86 + 0.28 * hash01(seed, i + 13);
            let base = self.pos.len() as u32;
            // Roots at the cell's bottom edge (v grows downward).
            for (p, uv) in [
                (b0, [cu, cv + dv]),
                (t0, [cu, cv]),
                (b1, [cu + du, cv + dv]),
                (t1, [cu + du, cv]),
            ] {
                self.pos.push(p.to_array());
                // Lit as the ground under it is: straight up.
                self.nor.push([0.0, 1.0, 0.0]);
                self.uv.push(uv);
                self.col.push([v * tint[0], v * tint[1], v * tint[2], 1.0]);
            }
            self.idx
                .extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
        }
    }

    fn mesh(self) -> Option<Mesh> {
        (!self.idx.is_empty()).then(|| {
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uv)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
                .with_inserted_indices(Indices::U32(self.idx))
        })
    }
}

/// The blocks of grass for a map's terrain, each a mesh and its centre.
pub fn blocks(t: &TerrainDef, lo: (f32, f32), hi: (f32, f32), seed: u32) -> Vec<(Mesh, Vec2)> {
    let mut out = Vec::new();
    let mut bz = lo.1;
    while bz < hi.1 {
        let mut bx = lo.0;
        while bx < hi.0 {
            let mut soup = Soup::default();
            let n = (BLOCK_M / STEP_M) as u32;
            for j in 0..n {
                for i in 0..n {
                    let cell = ((bx.to_bits() ^ bz.to_bits().rotate_left(16)) ^ (j * 977 + i)).wrapping_add(seed);
                    let x = bx + (i as f32 + hash01(cell, 1)) * STEP_M;
                    let z = bz + (j as f32 + hash01(cell, 2)) * STEP_M;
                    if x > hi.0 || z > hi.1 {
                        continue;
                    }
                    let Some((green, dry)) = growth(t, x, z) else {
                        continue;
                    };
                    let r = hash01(cell, 3);
                    let (tint, scale) = if r < green * 0.9 {
                        (GREEN, 1.0)
                    } else if r < green * 0.9 + dry {
                        (STRAW, 0.8)
                    } else {
                        continue;
                    };
                    let Some(y) = ground(t, x, z) else { continue };
                    let h = TUFT_H * scale * (0.75 + 0.5 * hash01(cell, 4));
                    soup.tuft(
                        Vec3::new(x, y, z),
                        hash01(cell, 5) * core::f32::consts::TAU,
                        cell,
                        h,
                        tint,
                    );
                }
            }
            if let Some(m) = soup.mesh() {
                out.push((m, Vec2::new(bx + BLOCK_M * 0.5, bz + BLOCK_M * 0.5)));
            }
            bx += BLOCK_M;
        }
        bz += BLOCK_M;
    }
    out
}

/// Only the blocks near the eye are drawn.
pub fn cull(eye: Query<&GlobalTransform, With<Camera3d>>, mut blocks: Query<(&GrassBlock, &mut Visibility)>) {
    let Ok(e) = eye.single() else {
        return;
    };
    let p = e.translation().xz();
    let reach = DRAW_M + BLOCK_M * 0.71;
    for (b, mut v) in blocks.iter_mut() {
        let want = if b.centre.distance(p) < reach {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *v != want {
            *v = want;
        }
    }
}

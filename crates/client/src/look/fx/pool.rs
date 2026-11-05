//! A fixed pool of particles and the one mesh that draws it.
//!
//! The pool itself is the shared `fx` crate's (`crates/fx`, vendored from
//! Gates by `tools/fx-sync.sh`, Bevy-free); this is its Bevy half: the mesh
//! it writes into, a camera from a transform, and `write`, which copies a
//! pool into the mesh's attributes in place.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

pub use ::fx::pool::{Cam, Orient, Particle, Pool, Quad, Verts, MIN_PX, STREAK_MAX_M, STREAK_MIN_M};

/// The camera a pool's quads face, from the eye's transform.
pub fn cam(gt: &GlobalTransform) -> Cam {
    Cam {
        pos: gt.translation().into(),
        right: Vec3::from(gt.right()).into(),
        up: Vec3::from(gt.up()).into(),
    }
}

/// Writing a pool into the Bevy mesh [`pool_mesh`] made.
pub trait MeshWrite {
    /// Copy the pool into `mesh`, in place, relative to `anchor` — or do
    /// nothing when it is empty and the mesh already is. Returns whether it
    /// wrote. No allocation: the attributes are the ones [`pool_mesh`] made.
    fn write(&mut self, mesh: &mut Mesh, cam: &Cam, anchor: Vec3) -> bool;
}

impl MeshWrite for Pool {
    fn write(&mut self, mesh: &mut Mesh, cam: &Cam, anchor: Vec3) -> bool {
        if !self.needs_write() {
            return false;
        }
        untrim(mesh, self.cap());
        let (mut pos, mut uv, mut col) = (None, None, None);
        for (attr, values) in mesh.attributes_mut() {
            let id = attr.id;
            match values {
                VertexAttributeValues::Float32x3(v) if id == Mesh::ATTRIBUTE_POSITION.id => {
                    pos = Some(v);
                }
                VertexAttributeValues::Float32x2(v) if id == Mesh::ATTRIBUTE_UV_0.id => {
                    uv = Some(v);
                }
                VertexAttributeValues::Float32x4(v) if id == Mesh::ATTRIBUTE_COLOR.id => {
                    col = Some(v);
                }
                _ => {}
            }
        }
        let (Some(pos), Some(uv), Some(col)) = (pos, uv, col) else {
            return false;
        };
        let wrote = self.fill(
            Verts {
                pos,
                uv,
                col,
                uv2: None,
                nrm: None,
                aux: None,
            },
            cam,
            anchor.into(),
        );
        trim(mesh, self.cap());
        wrote
    }
}

/// A trimmed pool mesh's length moves in steps of this many quads.
const TRIM_QUADS: usize = 64;

/// Before a fill: every attribute and the indices back out to `cap` quads,
/// as the pool writes them. In place: trimming kept the capacity. The tail
/// comes back collapsed, its normals the plain pool's (it never writes
/// them; a flipbook pool writes its own).
pub fn untrim(mesh: &mut Mesh, cap: usize) {
    let n = cap * 4;
    for (attr, values) in mesh.attributes_mut() {
        match values {
            VertexAttributeValues::Float32x3(v) => {
                let rest = if attr.id == Mesh::ATTRIBUTE_NORMAL.id {
                    ::fx::pool::NORMAL
                } else {
                    [0.0; 3]
                };
                v.resize(n, rest);
            }
            VertexAttributeValues::Float32x2(v) => v.resize(n, [0.0; 2]),
            VertexAttributeValues::Float32x4(v) => v.resize(n, [0.0; 4]),
            _ => {}
        }
    }
    if let Some(Indices::U32(idx)) = mesh.indices_mut() {
        for q in (idx.len() / 6) as u32..cap as u32 {
            let b = q * 4;
            idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
    }
}

/// After a fill: the mesh cut after the last quad in use (rounded up), so
/// what Bevy clones, lays out and uploads each frame is what is alive, not
/// the whole cap. A free slot's quad is all four corners at zero; the live
/// sort to the front of a sorted pool, and take the lowest free slots of
/// the other.
pub fn trim(mesh: &mut Mesh, cap: usize) {
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return;
    };
    let used = pos
        .chunks_exact(4)
        .rposition(|q| q.iter().any(|c| *c != [0.0; 3]))
        .map_or(0, |i| i + 1);
    let quads = (used.div_ceil(TRIM_QUADS).max(1) * TRIM_QUADS).min(cap);
    let n = quads * 4;
    for (_, values) in mesh.attributes_mut() {
        match values {
            VertexAttributeValues::Float32x3(v) => v.truncate(n),
            VertexAttributeValues::Float32x2(v) => v.truncate(n),
            VertexAttributeValues::Float32x4(v) => v.truncate(n),
            _ => {}
        }
    }
    if let Some(Indices::U32(idx)) = mesh.indices_mut() {
        idx.truncate(quads * 6);
    }
}

/// An empty mesh for a pool of `cap` quads, with its fixed indices. Normals
/// point up: the lit pool is shaded like the ground under it.
pub fn pool_mesh(cap: usize) -> Mesh {
    let n = cap * 4;
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![::fx::pool::NORMAL; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; n]);
    mesh.insert_indices(Indices::U32(::fx::pool::indices(cap)));
    mesh
}

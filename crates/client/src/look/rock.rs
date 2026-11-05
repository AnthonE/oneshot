//! Rock: what a rock-look box is drawn as, and the relief the terrain's
//! rock skin is shaded from (`cliffs.rs`). After Gates'
//! `crates/client/src/render/boulders.rs`, `render/cliff_skin.rs` and
//! `sim_core::face`'s facet noise (MIT, NOTICE).
//!
//! **What is drawn is what collides** (`level.rs`), so a rock box is drawn
//! two ways, by whether anyone can reach it:
//!
//! - **Faceted faces**, on every rock box: each face a lattice whose points
//!   lie exactly on the box's face, shaded from a relief the face does not
//!   have — fractured planes, bedding steps, recesses darker and proud
//!   facets lighter ([`relief`], the lattice's normals and its value in
//!   `UV_1`). The playable faces stay flat and where the box's are, to the
//!   float: a round stops where it is drawn to, a body stands where it
//!   looks to. Its silhouette is the box's.
//! - **A cliff mass**, for a rock box out of bounds and clear of the
//!   playable rectangle's edge (Dry Fork's cliffs beyond its walls): the
//!   box drawn as a core and Gates' rock pieces ([`PIECE_GLB`], a sculpted
//!   outcrop each, Meshy, commissioned and owned) stretched into boxes
//!   along its faces and its top, every one of them inside the collision
//!   box ([`mass_layout`]), so the skyline over the walls breaks like a
//!   cliff's. Nobody stands there; a round that reaches it stops on the box,
//!   at most [`MASS_INSET_M`] and a piece's own hollow behind the drawn
//!   rock, and leaves no mark there (`fx/marks.rs`, `Marks::bare`). Only
//!   the faces the playable area can see are dressed ([`Seen`]); the rest
//!   are the box's own planes.
//!
//! Until the pieces load, or if they cannot, a mass is its faceted box.
//! Every rung of the ladder draws the same rock: nothing here can hide or
//! show a fighter, and the mass is ~300 triangles a piece.

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

/// The rock pieces, each standing on its base at the origin, at its own
/// proportions: Gates' formation pieces remeshed low (~300 triangles), out
/// of `tools/assets.mjs`. Geometry only; drawn in the box's rock.
pub const PIECE_GLB: [&str; PIECES] = [
    "models/rock/block_a_far.glb",
    "models/rock/block_b_far.glb",
    "models/rock/block_c_far.glb",
    "models/rock/block_d_far.glb",
    "models/rock/block_e_far.glb",
    "models/rock/block_f_far.glb",
];
pub const PIECES: usize = 6;

/// A face's lattice pitch, metres: under the finest facet's, so its planes
/// resolve as planes.
pub const FACE_STEP_M: f32 = 0.75;
/// The most segments a face's side is cut into: a 140 m canyon wall is
/// shaded in metre-and-a-half cells rather than ten thousand.
pub const FACE_MAX_SEGS: usize = 96;
/// How deep the relief a face is shaded from would cut, metres (it is not
/// cut: the face is flat).
pub const FACE_DEPTH_M: f32 = 3.0;
/// How much brighter what stands proud is than what is recessed, as a
/// value multiplier, either way.
pub const VALUE_RELIEF: f32 = 0.25;
/// Each large facet's own shade, either way.
pub const FACET_SHADE: f32 = 0.1;
/// How much darker a facet is per unit its normal turns down from the face's.
pub const SKY_DARK: f32 = 0.5;
/// Two triangles meeting at a lattice point are shaded as one surface while
/// their normals are within this cosine (~26°); past it the crease shows.
pub const CREASE: f32 = 0.9;
/// The same for a piece's faces (~32°): smoothed across every angle the
/// pieces read as dough.
pub const PIECE_CREASE: f32 = 0.85;
/// The value multiplier's swing between a piece's crack and its ridge.
pub const PIECE_HOLLOW_DARK: f32 = 0.3;
/// Each piece's own value, either way.
pub const PIECE_VALUE: f32 = 0.08;

/// The buttress-sized facets' lattice, metres, the large ones' and the
/// small ones'.
const FACET_BROAD_M: f32 = 15.0;
pub const FACET_M: f32 = 6.5;
const FACET_FINE_M: f32 = 2.4;
/// Thickness of one stratum, metres; how far one may stand out from the
/// next, as a share of the push; how much of a stratum its step takes.
const STRATUM_M: f32 = 4.5;
const STRATUM_STEP: f32 = 0.42;
const STEP_BAND: f32 = 0.35;

/// A mass out of bounds is dressed with pieces along the top of its seen
/// faces, this much of its height at most, metres, and this share…
pub const MASS_BAND_M: f32 = 12.0;
pub const MASS_BAND: f32 = 0.6;
/// …its core set back from a seen face by up to this, metres…
pub const MASS_INSET_M: f32 = 2.0;
/// …and standing to this share of the band under its top, which the
/// pieces break.
pub const MASS_CAP: f32 = 0.3;
/// A column of pieces along a face, metres, and the least height a layer
/// of them is cut into.
pub const MASS_COL_M: f32 = 8.0;
pub const MASS_LAYER_MIN_M: f32 = 5.0;
/// How far a piece reaches back into the core, metres, and overlaps its
/// neighbours.
const MASS_DEEP_M: f32 = 2.5;
const MASS_LAP_M: f32 = 0.6;
/// A mass is a rock box at least this wide both ways and this tall.
pub const MASS_MIN_SIDE_M: f32 = 4.0;
pub const MASS_MIN_H_M: f32 = 6.0;

/// Integer hash to [0, 1).
pub fn h01(a: u32, b: u32) -> f32 {
    let mut x = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x >> 8) as f32 * (1.0 / 16_777_216.0)
}

fn corner(i: [i32; 3], key: u32) -> f32 {
    h01(
        (i[0] as u32).wrapping_mul(73_856_093)
            ^ (i[1] as u32).wrapping_mul(19_349_663)
            ^ (i[2] as u32).wrapping_mul(83_492_791),
        key,
    )
}

/// Piecewise-linear value noise in [0, 1]: each unit cube is split into
/// the six tetrahedra round its main diagonal and the hashed corners are
/// interpolated linearly inside each, so the field is a mosaic of planes
/// meeting at creases — fractured rock rather than rolling ground. Read
/// through a fixed rotation so no crease runs along a world axis.
pub fn facet_noise(x: f32, y: f32, z: f32, key: u32) -> f32 {
    let p = [
        0.788 * x - 0.497 * y + 0.364 * z,
        0.326 * x + 0.837 * y + 0.440 * z,
        -0.523 * x - 0.229 * y + 0.821 * z,
    ];
    let base = [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32];
    let f = [p[0] - base[0] as f32, p[1] - base[1] as f32, p[2] - base[2] as f32];
    // The axes in falling order of their fraction: the walk from the cube's
    // low corner to its high one through the tetrahedron holding `f`.
    let mut ax = [(f[0], 0usize), (f[1], 1), (f[2], 2)];
    if ax[0].0 < ax[1].0 {
        ax.swap(0, 1);
    }
    if ax[1].0 < ax[2].0 {
        ax.swap(1, 2);
    }
    if ax[0].0 < ax[1].0 {
        ax.swap(0, 1);
    }
    let mut c = base;
    let mut v = corner(c, key) * (1.0 - ax[0].0);
    c[ax[0].1] += 1;
    v += corner(c, key) * (ax[0].0 - ax[1].0);
    c[ax[1].1] += 1;
    v += corner(c, key) * (ax[1].0 - ax[2].0);
    c[ax[2].1] += 1;
    v + corner(c, key) * ax[2].0
}

/// How far rock stands out at `p`, in (0, 1); and how far it stands out
/// from the buttress it is part of, roughly −1..1, which reads as proud or
/// recessed. Broad buttresses, large and small facets, and strata whose
/// boundaries are steps: a shelf where the upper bed stands back, a riser
/// where it juts.
pub fn relief(p: Vec3, key: u32) -> (f32, f32) {
    let at = |m: f32, k: u32| facet_noise(p.x / m, p.y / m, p.z / m, k);
    let broad = at(FACET_BROAD_M, key ^ 0xb0ad);
    let big = at(FACET_M, key ^ 0xfac3);
    let fine = at(FACET_FINE_M, key ^ 0x5ca1);
    let lift = (at(23.0, key ^ 0xbed5) - 0.5) * STRATUM_M * 1.6;
    let b = (p.y + lift) / STRATUM_M;
    let bed = b.floor();
    let f = b - bed;
    let below = stratum(bed - 1.0, 1.0, p.x, p.z, key);
    let s = (f / STEP_BAND).min(1.0);
    let strata = below + (stratum(bed, f, p.x, p.z, key) - below) * s * s * (3.0 - 2.0 * s);
    let local = 0.42 * (big - 0.5) * 2.0 + 0.16 * (fine - 0.5) * 2.0 + strata;
    let t = 0.35 * (broad - 0.5) * 2.0 + local;
    // Into (0, 1) without a clamp: a clamped stretch would be flat again.
    (0.5 + t / (1.0 + 2.0 * t.abs()), local)
}

/// Bed `bed`'s stand-out at height `f` (0..1) up it: its own, drifting
/// along the face so a shelf comes and goes, and how its face leans.
fn stratum(bed: f32, f: f32, x: f32, z: f32, key: u32) -> f32 {
    let bed_key = key ^ (bed as i32 as u32).wrapping_mul(0x2545_F491);
    let drift = facet_noise(x / 11.0, 0.0, z / 11.0, bed_key);
    let stand = (h01(bed_key, 1) * 0.6 + drift * 0.4 - 0.5) * 2.0 * STRATUM_STEP;
    let lean = (h01(bed_key, 2) - 0.4) * 0.4;
    stand + lean * (f - 0.5)
}

/// What one lattice point's relief shades it: the value multiplier, before
/// its normal's own darkening.
pub fn relief_value(p: Vec3, local: f32, key: u32) -> f32 {
    let facet = facet_noise(p.x / FACET_M, p.y / FACET_M, p.z / FACET_M, key ^ 0x7a1e);
    1.0 + VALUE_RELIEF * local.clamp(-1.0, 1.0) + FACET_SHADE * (facet * 2.0 - 1.0)
}

/// A lattice meshed: each vertex its lattice point and its normal, and the
/// triangles over them.
#[derive(Default, Debug)]
pub struct Creased {
    pub verts: Vec<(usize, Vec3)>,
    pub tris: Vec<[u32; 3]>,
}

/// Quad `(j, k)` of an `nu`-wide lattice as its two triangles: `a d b` and
/// `a c d` over `a (j, k)`, `b (j, k + 1)`, `c (j + 1, k)`, `d (j + 1, k + 1)`
/// — split from its low corner to its high one, as the ground's cells are.
/// Wound to face `v × u` (u along `k`, v along `j`).
pub fn quad_tris(nu: usize, j: usize, k: usize) -> [[usize; 3]; 2] {
    let a = j * nu + k;
    let (b, c) = (a + 1, a + nu);
    let d = c + 1;
    [[a, d, b], [a, c, d]]
}

/// Mesh a lattice `nu` points wide and `nv` deep, the quads `keep` says,
/// with its normals from `virt` (the surface it is shaded as, point by
/// point): smooth across a facet and sharp across a crease. A corner takes
/// the triangles round its point that face within [`CREASE`] of its own,
/// and the vertex is shared by every corner that took the same ones.
pub fn crease_lattice(nu: usize, nv: usize, virt: &[Vec3], keep: impl Fn(usize, usize) -> bool) -> Creased {
    let mut out = Creased::default();
    if nu < 2 || nv < 2 || virt.len() < nu * nv {
        return out;
    }
    let (qu, qv) = (nu - 1, nv - 1);
    let mut face = vec![Vec3::ZERO; qu * qv * 2];
    for j in 0..qv {
        for k in 0..qu {
            for (t, c) in quad_tris(nu, j, k).into_iter().enumerate() {
                let (p0, p1, p2) = (virt[c[0]], virt[c[1]], virt[c[2]]);
                face[(j * qu + k) * 2 + t] = (p1 - p0).cross(p2 - p0);
            }
        }
    }
    // The six triangles round point (j, k): it is corner a of quad (j, k)
    // (both), b of (j, k − 1) (the first), c of (j − 1, k) (the second) and
    // d of (j − 1, k − 1) (both).
    let around = |j: usize, k: usize| -> [Option<usize>; 6] {
        let f = |qj: usize, qk: usize, t: usize| (qj * qu + qk) * 2 + t;
        [
            (j < qv && k < qu).then(|| f(j, k, 0)),
            (j < qv && k < qu).then(|| f(j, k, 1)),
            (j < qv && k > 0).then(|| f(j, k - 1, 0)),
            (j > 0 && k < qu).then(|| f(j - 1, k, 1)),
            (j > 0 && k > 0).then(|| f(j - 1, k - 1, 0)),
            (j > 0 && k > 0).then(|| f(j - 1, k - 1, 1)),
        ]
    };
    let mut made: Vec<[(u8, u32); 6]> = vec![[(0, u32::MAX); 6]; nu * nv];
    for j in 0..qv {
        for k in 0..qu {
            if !keep(j, k) {
                continue;
            }
            for (t, c) in quad_tris(nu, j, k).into_iter().enumerate() {
                let own = (j * qu + k) * 2 + t;
                let nf = face[own].normalize_or_zero();
                let mut tri = [0u32; 3];
                for (m, &i) in c.iter().enumerate() {
                    let mut set = 0u8;
                    let mut sum = Vec3::ZERO;
                    for (slot, f) in around(i / nu, i % nu).into_iter().enumerate() {
                        if let Some(f) = f {
                            if f == own || face[f].normalize_or_zero().dot(nf) > CREASE {
                                set |= 1 << slot;
                                sum += face[f];
                            }
                        }
                    }
                    let found = made[i].iter().find(|e| e.1 != u32::MAX && e.0 == set).map(|e| e.1);
                    tri[m] = match found {
                        Some(v) => v,
                        None => {
                            let v = out.verts.len() as u32;
                            out.verts.push((i, sum.normalize_or(nf)));
                            if let Some(free) = made[i].iter_mut().find(|e| e.1 == u32::MAX) {
                                *free = (set, v);
                            }
                            v
                        }
                    };
                }
                out.tris.push(tri);
            }
        }
    }
    out
}

/// A rock mesh being built for the level's material: positions, normals,
/// texture coordinates in metres (divided by the set's tile at the end) and
/// the value multiplier the weathering reads off `UV_1`.
#[derive(Default)]
pub struct Soup {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    val: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

impl Soup {
    pub fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }

    pub fn positions(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.pos.iter().map(|p| Vec3::from_array(*p))
    }

    pub fn vertex(&mut self, p: Vec3, n: Vec3, uv: [f32; 2], value: f32) -> u32 {
        self.pos.push(p.to_array());
        self.nrm.push(n.to_array());
        self.uv.push(uv);
        self.val.push([value, 0.0]);
        (self.pos.len() - 1) as u32
    }

    pub fn face(&mut self, a: u32, b: u32, c: u32) {
        self.idx.extend_from_slice(&[a, b, c]);
    }

    /// The soup as a mesh, its texture repeating every `tile` metres.
    pub fn mesh(self, tile: f32) -> Mesh {
        let uv: Vec<[f32; 2]> = self.uv.iter().map(|u| [u[0] / tile, u[1] / tile]).collect();
        let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, self.val)
            .with_inserted_indices(Indices::U32(self.idx));
        let _ = m.generate_tangents();
        m
    }
}

/// Which of a box's faces the playable area can see: +x, −x, +y, −y, +z,
/// −z. A face nobody can see is drawn as the box's own plane, unshaded.
pub type Seen = [bool; 6];

/// Every face but the bottom.
pub const SEEN_ALL: Seen = [true, true, true, false, true, true];

/// The faces of a box out of bounds the playable rectangle `lo..hi` lies
/// in front of: a face is seen when some of the rectangle is past its plane.
/// Its top is above anyone's eye, and its bottom on the ground.
pub fn seen_from(min: Vec3, max: Vec3, lo: (f32, f32), hi: (f32, f32)) -> Seen {
    [hi.0 > max.x, lo.0 < min.x, false, false, hi.1 > max.z, lo.1 < min.z]
}

/// A face of a box: its outward normal, its u and v axes (`v × u` is the
/// normal, as [`quad_tris`] winds) and the axis indices of each.
fn face_frame(f: usize) -> (Vec3, Vec3, Vec3, usize, usize, usize) {
    match f {
        0 => (Vec3::X, Vec3::Z, Vec3::Y, 0, 2, 1),
        1 => (Vec3::NEG_X, Vec3::Y, Vec3::Z, 0, 1, 2),
        2 => (Vec3::Y, Vec3::X, Vec3::Z, 1, 0, 2),
        3 => (Vec3::NEG_Y, Vec3::Z, Vec3::X, 1, 2, 0),
        4 => (Vec3::Z, Vec3::Y, Vec3::X, 2, 1, 0),
        _ => (Vec3::NEG_Z, Vec3::X, Vec3::Y, 2, 0, 1),
    }
}

/// A face's texture coordinates in metres, as `level.rs` lays a box's: the
/// tops in x and z, the sides along and down.
fn face_uv(p: Vec3, n: Vec3) -> [f32; 2] {
    let a = n.abs();
    if a.y > a.x && a.y > a.z {
        [p.x, p.z]
    } else if a.x > a.z {
        [p.z, -p.y]
    } else {
        [p.x, -p.y]
    }
}

/// One box among the level's, as [`covers`] reads them: its extent,
/// whether it wins every tie on a shared plane (a light strip, which is
/// never cut itself), and whether it is drawn at all (one merged into
/// another is not).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Other {
    pub min: Vec3,
    pub max: Vec3,
    pub wins: bool,
    pub gone: bool,
}

impl Other {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Other {
            min,
            max,
            wins: false,
            gone: false,
        }
    }
}

/// The parts of one face of a box another box hides (inside it, or back to
/// back with it) or draws itself (the same plane, facing the same way: the
/// box listed first draws it), as rectangles in the face's own metres along
/// its u and v from its low corner. Drawn twice, a shared plane flickers
/// between two blocks' values; drawn inside another box, it is overdraw.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cover {
    rects: Vec<[f32; 4]>,
}

impl Cover {
    /// Whether the point (u, v) of the face is not drawn.
    pub fn hides(&self, u: f32, v: f32) -> bool {
        self.rects.iter().any(|r| u > r[0] && u < r[2] && v > r[1] && v < r[3])
    }

    /// The face's cuts along u (`axis` 0) or v (1) inside `0..len`.
    fn cuts(&self, len: f32, axis: usize) -> impl Iterator<Item = f32> + '_ {
        self.rects
            .iter()
            .flat_map(move |r| [r[axis], r[axis + 2]])
            .filter(move |&c| c > 0.0 && c < len)
    }

    /// The share of a face `lu` by `lv` still drawn.
    pub fn open(&self, lu: f32, lv: f32) -> f32 {
        if lu <= 0.0 || lv <= 0.0 {
            return 0.0;
        }
        let us = merged([0.0, lu].into_iter().chain(self.cuts(lu, 0)));
        let vs = merged([0.0, lv].into_iter().chain(self.cuts(lv, 1)));
        let mut open = 0.0;
        for v in vs.windows(2) {
            for u in us.windows(2) {
                if !self.hides((u[0] + u[1]) * 0.5, (v[0] + v[1]) * 0.5) {
                    open += (u[1] - u[0]) * (v[1] - v[0]);
                }
            }
        }
        open / (lu * lv)
    }
}

/// Sorted, with the ones within a tenth of a millimetre of the last kept
/// dropped.
fn merged(it: impl Iterator<Item = f32>) -> Vec<f32> {
    let mut v: Vec<f32> = it.collect();
    v.sort_by(f32::total_cmp);
    v.dedup_by(|b, a| (*b - *a).abs() < 1e-4);
    v
}

/// How far a face cut where another box stands on it runs on under that
/// box, metres ([`covers`]).
pub const TUCK_M: f32 = 0.01;

/// What covers each face of box `me` of `all` (its extent `min..max`,
/// which a merge may have grown past its entry's).
pub fn covers(min: Vec3, max: Vec3, me: usize, all: &[Other]) -> [Cover; 6] {
    const EPS: f32 = 1e-4;
    let mut out: [Cover; 6] = Default::default();
    for (f, cover) in out.iter_mut().enumerate() {
        let (n, _, _, an, au, av) = face_frame(f);
        let up = n[an] > 0.0;
        let c = if up { max[an] } else { min[an] };
        for (j, o) in all.iter().enumerate() {
            if j == me || o.gone {
                continue;
            }
            let (b0, b1) = (o.min[an], o.max[an]);
            // It fills the air just outside the face, or has a face of its
            // own on the same plane facing the same way and is drawn first.
            let outside = if up {
                b0 <= c + EPS && b1 > c + EPS
            } else {
                b0 < c - EPS && b1 >= c - EPS
            };
            let twin = if up {
                (b1 - c).abs() <= EPS && b0 < c - EPS
            } else {
                (b0 - c).abs() <= EPS && b1 > c + EPS
            };
            if !(outside || (twin && (o.wins || j < me))) {
                continue;
            }
            let mut r = [
                o.min[au].max(min[au]) - min[au],
                o.min[av].max(min[av]) - min[av],
                o.max[au].min(max[au]) - min[au],
                o.max[av].min(max[av]) - min[av],
            ];
            if outside {
                // Tucked under the box that stands on it: the face runs on
                // a little inside that box, so where its edge meets the
                // box's foot (two meshes, two sets of vertices) there is
                // overlap, never a crack to the sky.
                let (lu, lv) = (max[au] - min[au], max[av] - min[av]);
                if r[0] > EPS {
                    r[0] += TUCK_M;
                }
                if r[1] > EPS {
                    r[1] += TUCK_M;
                }
                if r[2] < lu - EPS {
                    r[2] -= TUCK_M;
                }
                if r[3] < lv - EPS {
                    r[3] -= TUCK_M;
                }
            }
            if r[2] - r[0] > EPS && r[3] - r[1] > EPS {
                cover.rects.push(r);
            }
        }
    }
    out
}

/// The edges of box `me` of `all` (its extent `min..max`) that another
/// box hides or carries on, as bits (`weathering.wgsl`'s `edge_distance`):
/// 0..3 the top's at +x, −x, +z, −z; 4..7 the uprights at (+x, +z),
/// (+x, −z), (−x, +z), (−x, −z). An edge is hidden where, just past it on
/// either of its faces (and just inside the other), there is another box:
/// a slab on a wall's top, a parapet on a slab's rim, an end wall against a
/// side wall's end, a flush box going on with the face. Wear along those
/// would draw a worn band where the wall has no edge. Sampled at a quarter,
/// a half and three quarters along; hidden where two of the three are.
pub fn hidden_edges(min: Vec3, max: Vec3, me: usize, all: &[Other]) -> u8 {
    const D: f32 = 0.02;
    const EPS: f32 = 1e-4;
    let inside = |q: Vec3| {
        all.iter()
            .enumerate()
            .any(|(j, o)| j != me && !o.gone && (0..3).all(|k| q[k] >= o.min[k] - EPS && q[k] <= o.max[k] + EPS))
    };
    let (x, z) = (Vec3::X, Vec3::Z);
    // Each edge: a point on it at `t` of its run, and its two faces' normals.
    let top = |t: f32, n: Vec3| -> (Vec3, Vec3, Vec3) {
        let y = max.y;
        let p = if n.x != 0.0 {
            Vec3::new(if n.x > 0.0 { max.x } else { min.x }, y, min.z + (max.z - min.z) * t)
        } else {
            Vec3::new(min.x + (max.x - min.x) * t, y, if n.z > 0.0 { max.z } else { min.z })
        };
        (p, n, Vec3::Y)
    };
    let upright = |t: f32, a: Vec3, b: Vec3| -> (Vec3, Vec3, Vec3) {
        let px = if a.x > 0.0 { max.x } else { min.x };
        let pz = if b.z > 0.0 { max.z } else { min.z };
        (Vec3::new(px, min.y + (max.y - min.y) * t, pz), a, b)
    };
    let edges: [&dyn Fn(f32) -> (Vec3, Vec3, Vec3); 8] = [
        &|t| top(t, x),
        &|t| top(t, -x),
        &|t| top(t, z),
        &|t| top(t, -z),
        &|t| upright(t, x, z),
        &|t| upright(t, x, -z),
        &|t| upright(t, -x, z),
        &|t| upright(t, -x, -z),
    ];
    let mut mask = 0u8;
    for (bit, edge) in edges.iter().enumerate() {
        let hid = [0.25, 0.5, 0.75]
            .iter()
            .filter(|&&t| {
                let (p, a, b) = edge(t);
                inside(p + (a - b) * D) || inside(p + (b - a) * D)
            })
            .count();
        if hid >= 2 {
            mask |= 1 << bit;
        }
    }
    mask
}

/// The box two boxes make together, when they make one: the same extent
/// on two axes, and overlapping or touching on the third.
pub fn union_box(a0: Vec3, a1: Vec3, b0: Vec3, b1: Vec3) -> Option<(Vec3, Vec3)> {
    let same = |k: usize| (a0[k] - b0[k]).abs() < 1e-4 && (a1[k] - b1[k]).abs() < 1e-4;
    (0..3)
        .any(|k| (0..3).filter(|&m| m != k).all(same) && a0[k] <= b1[k] + 1e-4 && b0[k] <= a1[k] + 1e-4)
        .then(|| (a0.min(b0), a1.max(b1)))
}

/// Face `f`'s sides along its u and v.
pub fn face_size(min: Vec3, max: Vec3, f: usize) -> (f32, f32) {
    let (_, _, _, _, au, av) = face_frame(f);
    let s = max - min;
    (s[au], s[av])
}

/// Nothing covers any face.
pub fn bare() -> [Cover; 6] {
    Default::default()
}

/// An axis-aligned box `min..max` into `soup`, its positions less
/// `origin`: every face flat and exactly the box's, but where `cover` says
/// another box hides or draws it; each seen face a lattice shaded from
/// [`relief`] (keyed by `key`), the others flat, cut only where a cover
/// starts or ends. Laid in world metres, as every box is, so the cuts and
/// neighbouring boxes carry on as one surface.
pub fn faceted_box(soup: &mut Soup, min: Vec3, max: Vec3, origin: Vec3, key: u32, seen: Seen, cover: &[Cover; 6]) {
    let size = max - min;
    // One set of cuts per axis for every face: two faces meeting at an
    // edge then share its every vertex, and no T-junction opens a crack
    // along it. The relief's steps on an axis any seen face runs along.
    let cuts: [Vec<f32>; 3] = core::array::from_fn(|a| {
        let stepped = (0..6).any(|f| {
            let (_, _, _, _, au, av) = face_frame(f);
            seen[f] && (au == a || av == a)
        });
        let l = size[a];
        let s = if stepped {
            ((l / FACE_STEP_M).ceil() as usize).clamp(1, FACE_MAX_SEGS)
        } else {
            1
        };
        let steps = (0..=s).map(move |k| if k == s { l } else { l * k as f32 / s as f32 });
        let covered = (0..6).flat_map(|f| {
            let (_, _, _, _, au, av) = face_frame(f);
            let along = if au == a {
                Some(0)
            } else if av == a {
                Some(1)
            } else {
                None
            };
            along.into_iter().flat_map(move |k| cover[f].cuts(l, k))
        });
        merged(steps.chain(covered))
    });
    for (f, &shown) in seen.iter().enumerate() {
        let (n, u, v, an, au, av) = face_frame(f);
        let (lu, lv) = (size[au], size[av]);
        if lu <= 0.0 || lv <= 0.0 {
            continue;
        }
        let cv = &cover[f];
        // The face's low corner: on its plane, the other two at their min.
        let mut corner = min;
        if n[an] > 0.0 {
            corner[an] = max[an];
        }
        let (us, vs) = (&cuts[au], &cuts[av]);
        let (nu, nv) = (us.len(), vs.len());
        let mut drawn = Vec::with_capacity(nu * nv);
        let mut virt = Vec::with_capacity(nu * nv);
        let mut value = Vec::with_capacity(nu * nv);
        for &dv in vs {
            for &du in us {
                let p = corner + u * du + v * dv;
                drawn.push(p);
                if shown {
                    let (push, local) = relief(p, key);
                    virt.push(p + n * (FACE_DEPTH_M * (push - 1.0)));
                    value.push(relief_value(p, local, key));
                } else {
                    virt.push(p);
                    value.push(1.0);
                }
            }
        }
        let keep = |j: usize, k: usize| !cv.hides((us[k] + us[k + 1]) * 0.5, (vs[j] + vs[j + 1]) * 0.5);
        let c = crease_lattice(nu, nv, &virt, keep);
        let base = soup.pos.len() as u32;
        for &(i, nn) in &c.verts {
            let dark = 1.0 - SKY_DARK * (n.y - nn.y).max(0.0);
            soup.vertex(drawn[i] - origin, nn, face_uv(drawn[i], n), value[i] * dark);
        }
        for t in &c.tris {
            soup.face(base + t[0], base + t[1], base + t[2]);
        }
    }
}

/// One rock piece's geometry, read once off its loaded mesh.
#[derive(Clone, Debug)]
pub struct Piece {
    /// Positions in its own box: x and z −1..1 across its footprint, y 0..1
    /// from its base to its top.
    pos: Vec<Vec3>,
    idx: Vec<u32>,
    /// Its own proportions, width, height and depth, at any scale.
    size: Vec3,
    /// How hollow the surface is at each vertex: + in a crack, − on a
    /// ridge, about ±1 at the most.
    hollow: Vec<f32>,
    /// The triangles round each vertex: vertex `v`'s are
    /// `ring[ring_at[v]..ring_at[v + 1]]`.
    ring: Vec<u32>,
    ring_at: Vec<u32>,
}

impl Piece {
    /// `None` for a mesh with no positions or no triangles.
    pub fn read(mesh: &Mesh) -> Option<Piece> {
        let Some(VertexAttributeValues::Float32x3(raw)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            return None;
        };
        let raw: Vec<Vec3> = raw.iter().map(|p| Vec3::from_array(*p)).collect();
        let idx: Vec<u32> = match mesh.indices() {
            Some(i) => i.iter().map(|k| k as u32).collect(),
            None => (0..raw.len() as u32).collect(),
        };
        Piece::from_raw(raw, idx)
    }

    /// From positions and triangles, in any units.
    pub fn from_raw(raw: Vec<Vec3>, idx: Vec<u32>) -> Option<Piece> {
        if idx.len() < 3 || raw.is_empty() || idx.iter().any(|&k| k as usize >= raw.len()) {
            return None;
        }
        let idx: Vec<u32> = idx[..idx.len() / 3 * 3].to_vec();
        let (lo, hi) = raw
            .iter()
            .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let size = (hi - lo).max(Vec3::splat(1e-4));
        // A crack is a vertex its neighbours stand out in front of: the
        // mean of its neighbours off its tangent plane, over its edges.
        let mut nrm = vec![Vec3::ZERO; raw.len()];
        let mut mean = vec![Vec3::ZERO; raw.len()];
        let mut edge = vec![0.0f32; raw.len()];
        let mut deg = vec![0.0f32; raw.len()];
        for t in idx.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|k| k as usize);
            let n = (raw[b] - raw[a]).cross(raw[c] - raw[a]);
            for (v, o1, o2) in [(a, b, c), (b, c, a), (c, a, b)] {
                nrm[v] += n;
                mean[v] += raw[o1] + raw[o2];
                edge[v] += raw[o1].distance(raw[v]) + raw[o2].distance(raw[v]);
                deg[v] += 2.0;
            }
        }
        let hollow = (0..raw.len())
            .map(|v| {
                if deg[v] == 0.0 {
                    return 0.0;
                }
                let off = mean[v] / deg[v] - raw[v];
                let len = (edge[v] / deg[v]).max(1e-6);
                (off.dot(nrm[v].normalize_or_zero()) / len * 4.0).clamp(-1.0, 1.0)
            })
            .collect();
        let mid = (lo + hi) * 0.5;
        let pos: Vec<Vec3> = raw
            .iter()
            .map(|p| {
                Vec3::new(
                    ((p.x - mid.x) / (0.5 * size.x)).clamp(-1.0, 1.0),
                    ((p.y - lo.y) / size.y).clamp(0.0, 1.0),
                    ((p.z - mid.z) / (0.5 * size.z)).clamp(-1.0, 1.0),
                )
            })
            .collect();
        let mut ring_at = vec![0u32; pos.len() + 1];
        for &v in &idx {
            ring_at[v as usize + 1] += 1;
        }
        for v in 0..pos.len() {
            ring_at[v + 1] += ring_at[v];
        }
        let mut ring = vec![0u32; idx.len()];
        let mut fill = ring_at.clone();
        for (k, &v) in idx.iter().enumerate() {
            ring[fill[v as usize] as usize] = (k / 3) as u32;
            fill[v as usize] += 1;
        }
        Some(Piece {
            pos,
            idx,
            size,
            hollow,
            ring,
            ring_at,
        })
    }
}

/// Which of `pieces` a box of `size` wears, and whether turned a quarter:
/// the one it stretches least, in plan first, with a hash so neighbours of
/// one shape differ.
pub fn pick_piece(size: Vec3, key: u32, pieces: &[Piece]) -> (usize, bool) {
    let mut best = (f32::INFINITY, 0, false);
    for (k, p) in pieces.iter().enumerate() {
        for quarter in [false, true] {
            let (pw, pd) = if quarter {
                (p.size.z, p.size.x)
            } else {
                (p.size.x, p.size.z)
            };
            let (sx, sz, sy) = (size.x / pw, size.z / pd, size.y.max(0.3) / p.size.y);
            let score =
                (sx / sz).ln().abs() + 0.5 * (sy / (sx * sz).sqrt()).ln().abs() + 0.35 * h01(key ^ 0x9e3c, k as u32);
            if score < best.0 {
                best = (score, k, quarter);
            }
        }
    }
    (best.1, best.2)
}

/// One piece stretched into the box `lo..hi` into `soup`, its positions
/// less `origin`: its base on the box's floor, its top on its top, turned
/// as [`pick_piece`] says and a half more by hash. Never past the box: its
/// positions are its own box's, mapped.
pub fn piece_into(soup: &mut Soup, lo: Vec3, hi: Vec3, origin: Vec3, key: u32, pieces: &[Piece]) {
    if pieces.is_empty() {
        return;
    }
    let size = hi - lo;
    let (k, quarter) = pick_piece(size, key, pieces);
    let p = &pieces[k];
    let half = h01(key, 0x4a1f) < 0.5;
    let mid = (lo + hi) * 0.5;
    let world: Vec<Vec3> = p
        .pos
        .iter()
        .map(|u| {
            // Turns only, never a mirror: a mirror would wind it inside out.
            let (mut x, mut z) = if quarter { (-u.z, u.x) } else { (u.x, u.z) };
            if half {
                (x, z) = (-x, -z);
            }
            Vec3::new(
                (mid.x + x * 0.5 * size.x).clamp(lo.x, hi.x),
                (lo.y + u.y * size.y).clamp(lo.y, hi.y),
                (mid.z + z * 0.5 * size.z).clamp(lo.z, hi.z),
            )
        })
        .collect();
    let face: Vec<Vec3> = p
        .idx
        .chunks_exact(3)
        .map(|t| {
            let [a, b, c] = [t[0], t[1], t[2]].map(|k| k as usize);
            (world[b] - world[a]).cross(world[c] - world[a])
        })
        .collect();
    // Each corner of each face takes the normal of the faces round its
    // vertex within the crease of its own.
    let mut corner_n = vec![Vec3::ZERO; p.idx.len()];
    let mut groups: Vec<(Vec3, Vec3)> = Vec::new();
    for v in 0..p.pos.len() {
        let ring = &p.ring[p.ring_at[v] as usize..p.ring_at[v + 1] as usize];
        groups.clear();
        for &t in ring {
            let f = face[t as usize];
            let fu = f.normalize_or_zero();
            match groups.iter_mut().find(|g| g.0.dot(fu) >= PIECE_CREASE) {
                Some(g) => g.1 += f,
                None => groups.push((fu, f)),
            }
        }
        for &t in ring {
            let t = t as usize;
            let fu = face[t].normalize_or_zero();
            let n = groups
                .iter()
                .find(|g| g.0.dot(fu) >= PIECE_CREASE)
                .map_or(fu, |g| g.1.normalize_or(fu));
            for c in 0..3 {
                if p.idx[3 * t + c] as usize == v {
                    corner_n[3 * t + c] = n;
                }
            }
        }
    }
    let own = 1.0 + (h01(key, 7) * 2.0 - 1.0) * PIECE_VALUE;
    // Each triangle its own three vertices, laid on the plane its face is
    // nearest, so no triangle's photograph is smeared across two planes.
    for (t, f) in face.iter().enumerate() {
        let fu = f.normalize_or(Vec3::Y);
        let base = soup.pos.len() as u32;
        for c in 0..3 {
            let v = p.idx[3 * t + c] as usize;
            let w = world[v];
            let value = own * (1.0 - PIECE_HOLLOW_DARK * p.hollow[v]);
            soup.vertex(w - origin, corner_n[3 * t + c], face_uv(w, fu), value);
        }
        soup.face(base, base + 1, base + 2);
    }
}

/// One box of a mass's drawing: a core block, or a piece's box and its key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Part {
    Core { min: Vec3, max: Vec3, seen: Seen },
    Piece { min: Vec3, max: Vec3, key: u32 },
}

/// A rock box out of bounds as a core and pieces, every one inside
/// `min..max`: the core whole below the band at its top and set back from
/// each seen face above it, and columns of pieces along each seen face's
/// band reaching from inside the core to at most the face, their tops a
/// broken line under the box's.
pub fn mass_layout(min: Vec3, max: Vec3, key: u32, seen: Seen) -> Vec<Part> {
    let size = max - min;
    let band = (size.y * MASS_BAND).min(MASS_BAND_M);
    let y_band = max.y - band;
    let inset = (size.x.min(size.z) * 0.2).clamp(0.6, MASS_INSET_M);
    let core_top = max.y - band * MASS_CAP;
    let back = |s: bool| if s { inset } else { 0.0 };
    let mut parts = vec![
        Part::Core {
            min,
            max: Vec3::new(max.x, y_band, max.z),
            seen: [seen[0], seen[1], false, false, seen[4], seen[5]],
        },
        Part::Core {
            min: Vec3::new(min.x + back(seen[1]), y_band, min.z + back(seen[5])),
            max: Vec3::new(max.x - back(seen[0]), core_top, max.z - back(seen[4])),
            seen: [seen[0], seen[1], false, false, seen[4], seen[5]],
        },
    ];
    let layers = if band >= 2.0 * MASS_LAYER_MIN_M { 2 } else { 1 };
    for (f, (axis, along, out)) in [(0usize, 2usize, 1.0f32), (0, 2, -1.0), (2, 0, 1.0), (2, 0, -1.0)]
        .into_iter()
        .enumerate()
    {
        let side = [0, 1, 4, 5][f];
        if !seen[side] {
            continue;
        }
        let plane = if out > 0.0 { max[axis] } else { min[axis] };
        let (a, b) = (min[along], max[along]);
        let cols = ((b - a) / MASS_COL_M).ceil().max(1.0) as usize;
        let cut = |i: usize| {
            if i == 0 {
                a
            } else if i == cols {
                b
            } else {
                a + (b - a) * (i as f32 + (h01(key ^ side as u32, i as u32) - 0.5) * 0.5) / cols as f32
            }
        };
        let split = y_band + band * (0.4 + 0.2 * h01(key ^ 0x1a7e, side as u32));
        for i in 0..cols {
            for l in 0..layers {
                let k =
                    key ^ (side as u32).wrapping_mul(0x9E37_79B9) ^ (i as u32 * 8 + l as u32).wrapping_mul(0x85EB_CA6B);
                let (y0, y1) = match (layers, l) {
                    (2, 0) => (y_band, split),
                    (2, _) => (split, max.y),
                    _ => (y_band, max.y),
                };
                // The top layer's tops break the skyline under the box's.
                let top = if y1 >= max.y {
                    max.y - band * 0.25 * h01(k, 3)
                } else {
                    y1 + MASS_LAP_M
                };
                let front = plane - out * inset * 0.45 * h01(k, 5);
                let rear = plane - out * (inset + MASS_DEEP_M);
                let mut lo = Vec3::ZERO;
                let mut hi = Vec3::ZERO;
                lo[axis] = front.min(rear);
                hi[axis] = front.max(rear);
                lo[along] = cut(i) - MASS_LAP_M;
                hi[along] = cut(i + 1) + MASS_LAP_M;
                lo.y = y0 - MASS_LAP_M;
                hi.y = top;
                let lo = lo.clamp(min, max);
                let hi = hi.clamp(min, max);
                if (hi - lo).min_element() > 0.3 {
                    parts.push(Part::Piece {
                        min: lo,
                        max: hi,
                        key: k,
                    });
                }
            }
        }
    }
    parts
}

/// A mass into `soup`: its layout's core faceted and its pieces, or with
/// no pieces the faceted box alone. `cover` is the box's own (its lower
/// core's sides are its sides).
#[allow(clippy::too_many_arguments)]
pub fn mass(
    soup: &mut Soup,
    min: Vec3,
    max: Vec3,
    origin: Vec3,
    key: u32,
    seen: Seen,
    cover: &[Cover; 6],
    pieces: Option<&[Piece]>,
) {
    let Some(pieces) = pieces.filter(|p| !p.is_empty()) else {
        faceted_box(soup, min, max, origin, key, seen, cover);
        return;
    };
    let sides = [
        cover[0].clone(),
        cover[1].clone(),
        Cover::default(),
        Cover::default(),
        cover[4].clone(),
        cover[5].clone(),
    ];
    for (k, part) in mass_layout(min, max, key, seen).into_iter().enumerate() {
        match part {
            // The lower core is the box's own sides up to the band.
            Part::Core { min, max, seen } if k == 0 => faceted_box(soup, min, max, origin, key, seen, &sides),
            Part::Core { min, max, seen } => faceted_box(soup, min, max, origin, key, seen, &bare()),
            Part::Piece { min, max, key } => piece_into(soup, min, max, origin, key, pieces),
        }
    }
}

/// Whether a box is a mass: rock, wide and tall enough, and clear of the
/// playable rectangle `lo..hi` on some side (not touching it, so none of
/// its faces is one a fighter can stand against).
pub fn is_mass(min: Vec3, max: Vec3, lo: (f32, f32), hi: (f32, f32)) -> bool {
    let size = max - min;
    let clear = max.x < lo.0 || min.x > hi.0 || max.z < lo.1 || min.z > hi.1;
    clear && size.x.min(size.z) >= MASS_MIN_SIDE_M && size.y >= MASS_MIN_H_M
}

/// A rock box out of bounds, waiting for the pieces to draw it with.
#[derive(Component, Clone, Debug)]
pub struct Mass {
    pub min: Vec3,
    pub max: Vec3,
    pub seen: Seen,
    pub key: u32,
    /// The rock set's tile, metres.
    pub tile: f32,
    /// What covers its faces ([`covers`]).
    pub cover: [Cover; 6],
}

/// A drawn mass's collision box, where a round's mark is not left: it may
/// stop behind the drawn rock (`fx/marks.rs`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Bare(pub Vec3, pub Vec3);

/// The rock pieces: loading, then read once every one has landed.
#[derive(Resource, Default)]
pub struct RockPieces {
    handles: Vec<Handle<Mesh>>,
    ready: Option<Arc<Vec<Piece>>>,
    failed: bool,
}

pub fn load(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(RockPieces {
        handles: PIECE_GLB
            .iter()
            .map(|path| server.load(GltfAssetLabel::Primitive { mesh: 0, primitive: 0 }.from_asset(*path)))
            .collect(),
        ready: None,
        failed: false,
    });
}

impl RockPieces {
    /// The pieces: `Some(None)` to draw the faceted box instead, `None`
    /// while a file is still loading.
    fn get(&mut self, server: &AssetServer, meshes: &Assets<Mesh>) -> Option<Option<Arc<Vec<Piece>>>> {
        if self.ready.is_none() && !self.failed {
            if self
                .handles
                .iter()
                .any(|h| matches!(server.get_load_state(h), Some(bevy::asset::LoadState::Failed(_))))
            {
                warn!("rock pieces failed to load: the masses are drawn as their boxes");
                self.failed = true;
            } else if self.handles.iter().all(|h| meshes.contains(h)) {
                let read: Option<Vec<Piece>> = self
                    .handles
                    .iter()
                    .map(|h| meshes.get(h).and_then(Piece::read))
                    .collect();
                match read {
                    Some(p) if !p.is_empty() => self.ready = Some(Arc::new(p)),
                    _ => {
                        warn!("a rock piece has no triangles: the masses are drawn as their boxes");
                        self.failed = true;
                    }
                }
            } else {
                return None;
            }
        }
        Some(self.ready.clone())
    }
}

/// Draw each mass once the pieces are in (or known missing): its mesh in
/// place of its box's, then dressed like any box (`level::dress_level`).
pub fn dress_masses(
    mut commands: Commands,
    pieces: Option<ResMut<RockPieces>>,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    masses: Query<(Entity, &Mass, &Transform)>,
) {
    if masses.is_empty() {
        return;
    }
    let Some(mut pieces) = pieces else {
        return;
    };
    let Some(set) = pieces.get(&server, &meshes) else {
        return;
    };
    for (e, m, at) in &masses {
        let mut soup = Soup::default();
        mass(
            &mut soup,
            m.min,
            m.max,
            at.translation,
            m.key,
            m.seen,
            &m.cover,
            set.as_deref().map(|v| v.as_slice()),
        );
        commands
            .entity(e)
            .remove::<Mass>()
            .insert((Mesh3d(meshes.add(soup.mesh(m.tile))), Bare(m.min, m.max)));
    }
}

/// Keep the marks off the masses' boxes this level has (`Marks::bare`).
pub fn bare_marks(
    added: Query<(), Added<Bare>>,
    mut gone: RemovedComponents<Bare>,
    all: Query<&Bare>,
    fx: Option<ResMut<super::fx::Fx>>,
) {
    let changed = !added.is_empty() || gone.read().count() > 0;
    let Some(mut fx) = fx else {
        return;
    };
    if changed {
        fx.marks.bare = all.iter().map(|b| (b.0, b.1)).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit-ish rock: a box with its top corners pulled in, so it has
    /// cracks and ridges both, standing on its base.
    fn rock() -> Piece {
        let mut pos = Vec::new();
        for &(x, y, z) in &[
            (-2.0, 0.0, -2.0),
            (2.0, 0.0, -2.0),
            (2.0, 0.0, 2.0),
            (-2.0, 0.0, 2.0),
            (-1.4, 3.0, -1.6),
            (1.7, 2.6, -1.3),
            (1.5, 3.1, 1.8),
            (-1.6, 2.8, 1.4),
        ] {
            pos.push(Vec3::new(x, y, z));
        }
        // Outward-wound (counter-clockwise from outside).
        let idx = vec![
            0, 1, 2, 0, 2, 3, // bottom
            4, 6, 5, 4, 7, 6, // top
            0, 5, 1, 0, 4, 5, // −z
            1, 6, 2, 1, 5, 6, // +x
            2, 7, 3, 2, 6, 7, // +z
            3, 4, 0, 3, 7, 4, // −x
        ];
        Piece::from_raw(pos, idx).unwrap()
    }

    fn inside(p: Vec3, min: Vec3, max: Vec3) -> bool {
        p.cmpge(min - Vec3::splat(1e-3)).all() && p.cmple(max + Vec3::splat(1e-3)).all()
    }

    /// A rock box's faces are drawn exactly where the box's are: every
    /// vertex on one of its six planes and inside it, every normal facing
    /// out of the face it lies on.
    #[test]
    fn a_faceted_box_is_its_box() {
        let (min, max) = (Vec3::new(-25.5, -1.5, -5.0), Vec3::new(-22.5, 1.6, -3.0));
        let mid = (min + max) * 0.5;
        let mut soup = Soup::default();
        faceted_box(&mut soup, min, max, mid, 77, SEEN_ALL, &bare());
        assert!(!soup.is_empty());
        for (p, n) in soup.positions().zip(soup.nrm.iter().map(|n| Vec3::from_array(*n))) {
            let w = p + mid;
            assert!(inside(w, min, max), "{w} outside the box");
            let on = [
                ((w.x - max.x).abs() < 1e-4, Vec3::X),
                ((w.x - min.x).abs() < 1e-4, Vec3::NEG_X),
                ((w.y - max.y).abs() < 1e-4, Vec3::Y),
                ((w.y - min.y).abs() < 1e-4, Vec3::NEG_Y),
                ((w.z - max.z).abs() < 1e-4, Vec3::Z),
                ((w.z - min.z).abs() < 1e-4, Vec3::NEG_Z),
            ];
            assert!(on.iter().any(|o| o.0), "{w} on no face");
            assert!(
                on.iter().any(|(at, f)| *at && n.dot(*f) > 0.3),
                "{w}'s normal {n} faces none of its faces"
            );
            assert!((n.length() - 1.0).abs() < 1e-3);
        }
        // The relief shades it: not one normal per face.
        let distinct = soup
            .nrm
            .iter()
            .filter(|n| Vec3::from_array(**n).dot(Vec3::X) > 0.3)
            .map(|n| (n[1] * 100.0).round() as i32)
            .collect::<std::collections::BTreeSet<_>>();
        assert!(distinct.len() > 3, "the +x face is shaded flat: {distinct:?}");
    }

    /// How many of `soup`'s triangles (less `origin`) pass through `p`.
    fn hits(soup: &Soup, origin: Vec3, p: Vec3) -> usize {
        soup.idx
            .chunks_exact(3)
            .filter(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|k| Vec3::from_array(soup.pos[k as usize]) + origin);
                let n = (b - a).cross(c - a);
                if n.length() < 1e-9 || (p - a).dot(n.normalize()).abs() > 1e-4 {
                    return false;
                }
                let inside = |x: Vec3, y: Vec3| (y - x).cross(p - x).dot(n) >= -1e-9;
                inside(a, b) && inside(b, c) && inside(c, a)
            })
            .count()
    }

    /// Two walls meeting at a corner, overlapping there: every plane they
    /// share is drawn once, and nothing of one is drawn inside the other.
    #[test]
    fn a_shared_plane_is_drawn_once() {
        let a = Other::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 4.0, 1.0));
        let b = Other::new(Vec3::new(9.0, 0.0, 0.0), Vec3::new(10.0, 4.0, 10.0));
        let all = [a, b];
        for seen in [SEEN_ALL, [false; 6]] {
            let mut sa = Soup::default();
            let mut sb = Soup::default();
            faceted_box(
                &mut sa,
                a.min,
                a.max,
                Vec3::ZERO,
                5,
                seen,
                &covers(a.min, a.max, 0, &all),
            );
            faceted_box(
                &mut sb,
                b.min,
                b.max,
                Vec3::ZERO,
                5,
                seen,
                &covers(b.min, b.max, 1, &all),
            );
            let both = |p: Vec3| hits(&sa, Vec3::ZERO, p) + hits(&sb, Vec3::ZERO, p);
            for p in [
                Vec3::new(10.0, 2.1, 0.45),
                Vec3::new(9.45, 4.0, 0.55),
                Vec3::new(9.55, 2.1, 0.0),
                Vec3::new(9.5, 0.0, 0.6),
                Vec3::new(5.3, 2.1, 1.0),
                Vec3::new(9.55, 2.1, 10.0),
            ] {
                assert_eq!(both(p), 1, "{p} drawn {} times", both(p));
            }
            // Inside the other: not drawn at all.
            assert_eq!(both(Vec3::new(9.55, 2.1, 1.0)), 0);
            assert_eq!(both(Vec3::new(9.0, 2.1, 0.45)), 0);
        }
        // The shared corner's faces are cut out of the second; a box
        // inside another is all hidden.
        let c = covers(b.min, b.max, 1, &all);
        assert!(c[2].open(1.0, 10.0) < 0.95 && c[0].open(10.0, 4.0) < 1.0);
        let inner = Other::new(Vec3::new(1.0, 1.0, 0.2), Vec3::new(2.0, 2.0, 0.8));
        let c = covers(inner.min, inner.max, 1, &[a, inner]);
        let size = inner.max - inner.min;
        for (f, c) in c.iter().enumerate() {
            let (_, _, _, _, au, av) = face_frame(f);
            assert_eq!(
                c.open(size[au], size[av]),
                0.0,
                "face {f} of a box inside another is drawn"
            );
        }
    }

    /// Every triangle winds to face its own normal's side.
    #[test]
    fn a_faceted_box_faces_out() {
        let (min, max) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(9.0, 6.0, 4.0));
        let mut soup = Soup::default();
        faceted_box(&mut soup, min, max, Vec3::ZERO, 3, SEEN_ALL, &bare());
        for t in soup.idx.chunks_exact(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|k| Vec3::from_array(soup.pos[k as usize]));
            let geo = (b - a).cross(c - a);
            let n = Vec3::from_array(soup.nrm[t[0] as usize]);
            assert!(geo.dot(n) > 0.0, "a triangle at {a} faces in");
        }
    }

    /// A mass is drawn inside its box, all of it: the core, and every
    /// piece's every vertex; and it has pieces on the faces that are seen.
    #[test]
    fn a_mass_stands_inside_its_box() {
        let pieces = vec![rock(), rock()];
        for (min, max) in [
            (Vec3::new(-76.0, 0.0, -44.0), Vec3::new(-40.0, 26.0, -31.0)),
            (Vec3::new(-90.0, 0.0, -31.0), Vec3::new(-71.0, 24.0, 31.0)),
            (Vec3::new(-40.0, 0.0, -40.0), Vec3::new(-12.0, 21.0, -31.0)),
        ] {
            let seen = seen_from(min, max, (-70.0, -30.0), (70.0, 30.0));
            assert!(is_mass(min, max, (-70.0, -30.0), (70.0, 30.0)));
            let parts = mass_layout(min, max, 11, seen);
            assert!(parts.iter().filter(|p| matches!(p, Part::Piece { .. })).count() >= 2);
            for p in &parts {
                let (lo, hi) = match *p {
                    Part::Core { min, max, .. } | Part::Piece { min, max, .. } => (min, max),
                };
                assert!(inside(lo, min, max) && inside(hi, min, max), "{p:?} past {min}..{max}");
            }
            let origin = (min + max) * 0.5;
            let mut soup = Soup::default();
            mass(&mut soup, min, max, origin, 11, seen, &bare(), Some(&pieces));
            for p in soup.positions() {
                assert!(inside(p + origin, min, max), "{} outside {min}..{max}", p + origin);
            }
        }
    }

    /// Only a box clear of the playable rectangle is a mass: a wall on the
    /// bound (its face one a fighter stands against) is not, nor one inside.
    #[test]
    fn only_what_nobody_reaches_is_a_mass() {
        let (lo, hi) = ((-70.0, -30.0), (70.0, 30.0));
        assert!(!is_mass(
            Vec3::new(-71.0, 0.0, -31.0),
            Vec3::new(71.0, 16.0, -30.0),
            lo,
            hi
        ));
        assert!(!is_mass(
            Vec3::new(-25.0, -1.5, -5.0),
            Vec3::new(-22.0, 1.6, -3.0),
            lo,
            hi
        ));
        assert!(!is_mass(
            Vec3::new(-71.0, 0.0, -30.0),
            Vec3::new(-62.0, 16.0, 30.0),
            lo,
            hi
        ));
        assert!(is_mass(
            Vec3::new(-12.0, 0.0, -46.0),
            Vec3::new(14.0, 31.0, -31.0),
            lo,
            hi
        ));
        // Dry Fork's mirrored middle cliff is one box; an L is not.
        assert_eq!(
            union_box(
                Vec3::new(-12.0, 0.0, -46.0),
                Vec3::new(14.0, 31.0, -31.0),
                Vec3::new(-14.0, 0.0, -46.0),
                Vec3::new(12.0, 31.0, -31.0)
            ),
            Some((Vec3::new(-14.0, 0.0, -46.0), Vec3::new(14.0, 31.0, -31.0)))
        );
        assert_eq!(
            union_box(
                Vec3::ZERO,
                Vec3::new(10.0, 4.0, 1.0),
                Vec3::new(9.0, 0.0, 0.0),
                Vec3::new(10.0, 4.0, 10.0)
            ),
            None
        );
        // Its seen faces: the one toward the canyon and the ends toward it.
        let s = seen_from(Vec3::new(-76.0, 0.0, -44.0), Vec3::new(-40.0, 26.0, -31.0), lo, hi);
        assert_eq!(s, [true, false, false, false, true, false]);
    }

    /// The relief is in (0, 1) and varies; the facet noise is in [0, 1].
    #[test]
    fn the_relief_is_bounded_and_broken() {
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for i in 0..2000 {
            let p = Vec3::new(i as f32 * 0.37 - 300.0, (i % 53) as f32 * 0.5, (i % 17) as f32 * 1.3);
            let (push, local) = relief(p, 9);
            assert!(push > 0.0 && push < 1.0 && local.is_finite());
            let f = facet_noise(p.x, p.y, p.z, 4);
            assert!((0.0..=1.0).contains(&f));
            lo = lo.min(push);
            hi = hi.max(push);
        }
        assert!(hi - lo > 0.3, "the relief is flat: {lo}..{hi}");
    }

    /// A slab on a wall hides the wall's top edges; an end wall against a
    /// side wall's end hides that end's uprights; a lone box hides none.
    #[test]
    fn edges_another_box_covers_wear_nothing() {
        let wall = (Vec3::new(-10.0, 0.0, 6.5), Vec3::new(10.0, 4.0, 7.0));
        let slab = Other::new(Vec3::new(-10.0, 4.0, -7.0), Vec3::new(10.0, 4.5, 7.0));
        let end = Other::new(Vec3::new(10.0, 0.0, -7.0), Vec3::new(10.5, 4.0, 7.0));
        let all = [Other::new(wall.0, wall.1), slab, end];
        let m = hidden_edges(wall.0, wall.1, 0, &all);
        assert_eq!(m & 0b1111, 0b1111, "the slab hides every top edge: {m:08b}");
        assert_eq!(
            m & 0b0011_0000,
            0b0011_0000,
            "the end wall hides the +x uprights: {m:08b}"
        );
        assert_eq!(m & 0b1100_0000, 0, "the -x uprights are open: {m:08b}");
        let lone = [Other::new(wall.0, wall.1)];
        assert_eq!(hidden_edges(wall.0, wall.1, 0, &lone), 0);
    }
}

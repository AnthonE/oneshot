//! The rock skin: a faceted shell over the terrain's rock faces. After
//! Gates' `crates/client/src/render/cliff_skin.rs` and `render/cliffs.rs`
//! (MIT, NOTICE).
//!
//! A bank of the heightfield is one smooth sheet with a photograph on it:
//! from the canyon floor, Dry Fork's walls read as plaster. Gates covers
//! every face too steep to climb with a shell of broken planes, strata,
//! shelves and risers, which its sim stands bodies on. **Ours is the sim's
//! ground, drawn**: the shell lies on the heightfield the sim stands a body
//! on and stops a round with, between [`SKIN_SINK_M`] under it and
//! [`SKIN_OUT_M`] over it, never further, so nothing passes through drawn
//! rock and nothing stops in front of it. Its relief is in its shading
//! instead: the lattice's normals are taken from a surface pushed out along
//! the fall line by up to [`VIRTUAL_OUT_M`] (`rock::relief`, Gates' facets
//! and strata), its value darker in what that surface recesses and where it
//! turns from the sky, so the planes, the beds and their steps are lit as
//! rock while the silhouette is the sim's.
//!
//! **Where it grows.** Dry Fork has no face too steep to walk (its banks
//! are 41° at the steepest; the sim's limit is 46°, `consts::WALK_NY`), so
//! the skin grows where the ground is rock instead — the splat's own rule
//! (`level.rs`: rock from 19° and whole by 37°) — from [`SKIN_ROCK_LO`] of
//! its rock to whole at [`SKIN_ROCK_HI`]. A face too steep to climb is rock
//! by that rule too, on any map that has one. Where it fades it sinks under
//! the drawn ground (which is exactly the sim's), and it shades as the
//! ground until it stands out of it ([`shade_weight`]), so where the two
//! meet they are one surface, one colour and one normal.
//!
//! Drawn on every rung, in the ground's own material (`ground.wgsl` reads
//! the value off `UV_1`), so it binds what the ground binds: WebGL2 draws
//! it as the desktop does. It casts no shadow of its own: the ground under
//! it, a few centimetres down, casts the same one. A mark left on it is
//! lifted onto it (`Lift`, `fx/marks.rs`).

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use sim::terrain::{TerrainDef, HEIGHT_Q};

use super::rock;

/// The share of the ground's rock weight where the skin starts…
pub const SKIN_ROCK_LO: f32 = 0.4;
/// …and where it is whole.
pub const SKIN_ROCK_HI: f32 = 0.9;
/// The most the shell stands over the sim's ground, metres…
pub const SKIN_OUT_M: f32 = 0.04;
/// …and the most it sinks under it, where it fades out.
pub const SKIN_SINK_M: f32 = 0.015;
/// The share of [`SKIN_OUT_M`] the shell keeps where its relief is lowest.
const SKIN_FLOOR: f32 = 0.25;
/// The lattice's pitch, metres: a quarter of Dry Fork's cells, so a
/// stratum's step resolves as a step.
pub const SKIN_STEP_M: f32 = 0.5;
/// How far the surface the shell is shaded from stands out along the fall
/// line at most, metres, times the face's slope (rise/run, at most
/// [`LEVER_MAX`]).
pub const VIRTUAL_OUT_M: f32 = 3.5;
const LEVER_MAX: f32 = 1.5;
/// How far past the playable rectangle the skin is drawn, metres: beyond it
/// the ground lies under the walls.
const MARGIN_M: f32 = 2.0;

/// The drawn shell over the sim's ground, on the skin's lattice: where a
/// mark on the ground is laid (`fx/marks.rs`).
#[derive(Debug, Default)]
pub struct Lift {
    x0: f32,
    z0: f32,
    step: f32,
    nu: usize,
    nv: usize,
    /// The sim's ground at each point, and how far the shell stands off it.
    ground: Vec<f32>,
    lift: Vec<f32>,
}

impl Lift {
    /// The sim's ground at (x, z) and how far the drawn shell stands over
    /// it there (under 0 where the ground shows), each as the lattice's
    /// triangles carry it; `None` off the lattice.
    pub fn at(&self, x: f32, z: f32) -> Option<(f32, f32)> {
        if self.nu < 2 || self.nv < 2 {
            return None;
        }
        let (fx, fz) = ((x - self.x0) / self.step, (z - self.z0) / self.step);
        if !(0.0..=(self.nu - 1) as f32).contains(&fx) || !(0.0..=(self.nv - 1) as f32).contains(&fz) {
            return None;
        }
        let k = (fx.floor() as usize).min(self.nu - 2);
        let j = (fz.floor() as usize).min(self.nv - 2);
        let (fu, fv) = (fx - k as f32, fz - j as f32);
        let a = j * self.nu + k;
        let (b, c) = (a + 1, a + self.nu);
        let d = c + 1;
        let w = |v: &[f32]| {
            if fu >= fv {
                v[a] * (1.0 - fu) + v[b] * (fu - fv) + v[d] * fv
            } else {
                v[a] * (1.0 - fv) + v[c] * (fv - fu) + v[d] * fu
            }
        };
        Some((w(&self.ground), w(&self.lift)))
    }
}

/// This level's [`Lift`], when its map has a terrain.
#[derive(Resource, Default, Clone)]
pub struct SkinLift(pub Option<Arc<Lift>>);

/// How much of the skin's own look a lattice point at height `l` over the
/// sim's ground wears: none at or under it, all of it once it stands
/// [`SKIN_FLOOR`] of [`SKIN_OUT_M`] out, smooth between.
pub fn shade_weight(l: f32) -> f32 {
    let t = (l / (SKIN_OUT_M * SKIN_FLOOR)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How fully the skin grows at a point whose ground is `rock` rock.
pub fn fade(rock: f32) -> f32 {
    let t = ((rock - SKIN_ROCK_LO) / (SKIN_ROCK_HI - SKIN_ROCK_LO)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Barycentric weights of a point at (fu, fv) in a cell, over its corners
/// (0,0), (1,0), (0,1), (1,1): the cell's two triangles split from its low
/// corner to its high one, as the sim's are.
fn weights(fu: f32, fv: f32) -> [f32; 4] {
    if fu >= fv {
        [1.0 - fu, fu - fv, 0.0, fv]
    } else {
        [1.0 - fv, 0.0, fv - fu, fu]
    }
}

/// The skin over terrain `t`'s rock inside `lo..hi` (and [`MARGIN_M`]
/// past it), keyed by `key`: its mesh for the ground's material (`None`
/// where the map has no rock) and its [`Lift`].
pub fn skin(t: &TerrainDef, lo: (f32, f32), hi: (f32, f32), key: u32) -> (Option<Mesh>, Lift) {
    let (nx, nz) = (t.nx as usize, t.nz as usize);
    if nx < 2 || nz < 2 || t.heights.len() < nx * nz {
        return (None, Lift::default());
    }
    let sub = ((t.cell / SKIN_STEP_M).round() as usize).max(1);
    let step = t.cell / sub as f32;
    let (nu, nv) = ((nx - 1) * sub + 1, (nz - 1) * sub + 1);
    let h = |k: usize| t.heights[k] as f32 / HEIGHT_Q;
    // The ground's own, per vertex: its slope and splat, its normal.
    let ground: Vec<(f32, [f32; 4])> = (0..nx * nz)
        .map(|k| crate::level::ground_point(t, k % nx, k / nx))
        .collect();
    let normals = crate::level::ground_normals(t);
    let n = nu * nv;
    let (mut drawn, mut virt) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let (mut sim_y, mut lift) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let mut shades = Vec::with_capacity(n);
    let (mut fades, mut cols, mut ngs, mut vals) = (
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
    );
    for b in 0..nv {
        let j = (b / sub).min(nz - 2);
        let fv = (b - j * sub) as f32 / sub as f32;
        for a in 0..nu {
            let i = (a / sub).min(nx - 2);
            let fu = (a - i * sub) as f32 / sub as f32;
            let k00 = j * nx + i;
            let corners = [k00, k00 + 1, k00 + nx, k00 + nx + 1];
            let w = weights(fu, fv);
            let mix = |f: &dyn Fn(usize) -> f32| corners.iter().zip(w).map(|(&c, w)| f(c) * w).sum::<f32>();
            let y = mix(&h);
            let rock_w = mix(&|c| ground[c].1[3]);
            let fd = fade(rock_w);
            let lever = mix(&|c| ground[c].0).min(LEVER_MAX);
            let mut col = [0.0f32; 4];
            for (&c, w) in corners.iter().zip(w) {
                for (o, g) in col.iter_mut().zip(ground[c].1) {
                    *o += g * w;
                }
            }
            let ng = corners
                .iter()
                .zip(w)
                .fold(Vec3::ZERO, |s, (&c, w)| s + normals[c] * w)
                .normalize_or(Vec3::Y);
            let (x, z) = (t.x0 + a as f32 * step, t.z0 + b as f32 * step);
            let p = Vec3::new(x, y, z);
            let (push, local) = rock::relief(p, key);
            let l = SKIN_OUT_M * fd * (SKIN_FLOOR + (1.0 - SKIN_FLOOR) * push) - SKIN_SINK_M * (1.0 - fd);
            drawn.push(Vec3::new(x, y + l, z));
            virt.push(Vec3::new(x, y + fd * VIRTUAL_OUT_M * push * lever, z));
            sim_y.push(y);
            lift.push(l);
            fades.push(fd);
            // How much the skin shades as itself: nothing where it comes up
            // through the ground (`l` crosses zero at a fade of a quarter
            // to three fifths, by the relief), so the line where the two
            // meet has one colour, one value and one normal; whole once it
            // stands its floor's height out.
            let sw = shade_weight(l);
            // The rock's identity takes over as the skin grows.
            let keep = 1.0 - sw;
            cols.push([col[0] * keep, col[1] * keep, col[2] * keep, col[3] * keep + sw]);
            ngs.push(ng);
            vals.push(1.0 + (rock::relief_value(p, local, key) - 1.0) * sw);
            shades.push(sw);
        }
    }
    let (xa, xb) = (lo.0 - MARGIN_M, hi.0 + MARGIN_M);
    let (za, zb) = (lo.1 - MARGIN_M, hi.1 + MARGIN_M);
    let keep = |j: usize, k: usize| {
        let (x, z) = (t.x0 + k as f32 * step, t.z0 + j as f32 * step);
        let inside = x + step >= xa && x <= xb && z + step >= za && z <= zb;
        inside
            && [j * nu + k, j * nu + k + 1, (j + 1) * nu + k, (j + 1) * nu + k + 1]
                .iter()
                .any(|&i| fades[i] > 0.0)
    };
    let creased = rock::crease_lattice(nu, nv, &virt, keep);
    let lift_out = Lift {
        x0: t.x0,
        z0: t.z0,
        step,
        nu,
        nv,
        ground: sim_y,
        lift,
    };
    if creased.tris.is_empty() {
        return (None, lift_out);
    }
    let m = creased.verts.len();
    let (mut pos, mut nrm, mut col, mut uv0, mut uv1) = (
        Vec::with_capacity(m),
        Vec::with_capacity(m),
        Vec::with_capacity(m),
        Vec::with_capacity(m),
        Vec::with_capacity(m),
    );
    for &(i, vn) in &creased.verts {
        let (fd, sw) = (fades[i], shades[i]);
        // The ground's own normal where the skin comes up through it, the
        // relief's where it is whole; darker where the relief turns from
        // the sky.
        let n = ngs[i].lerp(vn, sw).normalize_or(ngs[i]);
        let value = vals[i] * (1.0 - sw * rock::SKY_DARK * (ngs[i].y - vn.y).max(0.0));
        pos.push(drawn[i].to_array());
        nrm.push(n.to_array());
        col.push(cols[i]);
        uv0.push([drawn[i].x, drawn[i].z]);
        uv1.push([value, fd]);
    }
    let idx: Vec<u32> = creased.tris.iter().flatten().copied().collect();
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        // The ground samples in world x and z and reads no `UV_0`, but a
        // mesh with `UV_1` must carry it for `bevy_pbr`'s fragment.
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv0)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uv1)
        .with_inserted_indices(Indices::U32(idx));
    (Some(mesh), lift_out)
}

/// Hand this level's lift to the marks when it changes.
pub fn lift_marks(lift: Res<SkinLift>, fx: Option<ResMut<super::fx::Fx>>) {
    if !lift.is_changed() {
        return;
    }
    if let Some(mut fx) = fx {
        fx.marks.lift = lift.0.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues as V;

    fn canyon() -> (&'static sim::maps::Def, &'static TerrainDef) {
        let d = sim::maps::ALL.iter().copied().find(|d| d.id == 3).expect("Dry Fork");
        (d, d.terrain.as_ref().expect("Dry Fork's terrain"))
    }

    fn f3(m: &Mesh, a: bevy::mesh::MeshVertexAttribute) -> Vec<Vec3> {
        match m.attribute(a) {
            Some(V::Float32x3(v)) => v.iter().map(|p| Vec3::from_array(*p)).collect(),
            _ => Vec::new(),
        }
    }

    /// **The skin drawn is the sim's ground**: every vertex, and the middle
    /// of every triangle (so no triangle bends across a crease of the
    /// sim's), stands within the skin's band of where the sim stands a body
    /// and stops a round.
    #[test]
    fn the_skin_lies_on_the_ground_the_sim_stands_on() {
        let (d, t) = canyon();
        let sim_t = sim::terrain::Terrain::build(t);
        let (mesh, lift) = skin(t, d.lo, d.hi, 7);
        let mesh = mesh.expect("Dry Fork's banks carry a skin");
        let pos = f3(&mesh, Mesh::ATTRIBUTE_POSITION);
        let band = |p: Vec3| {
            let g = sim_t.height(p.x, p.z).expect("over the terrain");
            let off = p.y - g;
            assert!(
                (-SKIN_SINK_M - 2e-3..=SKIN_OUT_M + 2e-3).contains(&off),
                "at {p}: {off:+.3} m off the sim's ground"
            );
            // The marks' lift is the drawn shell.
            let (lg, l) = lift.at(p.x, p.z).expect("on the lattice");
            assert!(
                (lg - g).abs() < 2e-3 && (lg + l - p.y).abs() < 2e-3,
                "at {p}: lift {lg}+{l}"
            );
        };
        for &p in &pos {
            band(p);
        }
        let Some(idx) = mesh.indices() else {
            panic!("no triangles");
        };
        let idx: Vec<usize> = idx.iter().collect();
        assert!(idx.len() / 3 > 5000, "only {} triangles of skin", idx.len() / 3);
        let mut up = 0;
        for t in idx.chunks_exact(3) {
            let [a, b, c] = [pos[t[0]], pos[t[1]], pos[t[2]]];
            band((a + b + c) / 3.0);
            up += ((b - a).cross(c - a).y > 0.0) as usize;
        }
        assert_eq!(up, idx.len() / 3, "a skin triangle faces down");
        for n in f3(&mesh, Mesh::ATTRIBUTE_NORMAL) {
            assert!((n.length() - 1.0).abs() < 1e-3 && n.y > 0.0, "normal {n}");
        }
    }

    /// The skin is shaded as broken rock: its normals spread well past the
    /// smooth ground's on the same bank.
    #[test]
    fn the_skin_is_shaded_broken() {
        let (d, t) = canyon();
        let (mesh, _) = skin(t, d.lo, d.hi, 7);
        let mesh = mesh.unwrap();
        let nrm = f3(&mesh, Mesh::ATTRIBUTE_NORMAL);
        let Some(V::Float32x2(uv1)) = mesh.attribute(Mesh::ATTRIBUTE_UV_1) else {
            panic!("no UV_1");
        };
        // The south bank alone (it faces north), so the spread is the
        // facets', not the two banks' facing.
        let pos = f3(&mesh, Mesh::ATTRIBUTE_POSITION);
        let whole: Vec<Vec3> = nrm
            .iter()
            .zip(uv1)
            .zip(&pos)
            .filter(|((_, u), p)| u[1] > 0.95 && p.z < 0.0)
            .map(|((n, _), _)| *n)
            .collect();
        assert!(whole.len() > 500, "only {} whole skin vertices", whole.len());
        let mean = whole.iter().copied().sum::<Vec3>().normalize();
        let spread = whole.iter().map(|n| 1.0 - n.dot(mean)).sum::<f32>() / whole.len() as f32;
        assert!(spread > 0.006, "the skin's normals barely move: {spread}");
        let vals: Vec<f32> = uv1.iter().filter(|u| u[1] > 0.95).map(|u| u[0]).collect();
        let (lo, hi) = vals
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        assert!(hi - lo > 0.15 && lo > 0.3 && hi < 1.6, "value {lo}..{hi}");
    }

    /// Where the skin is still under the ground it shades as the ground,
    /// so the line where it comes up through it has no step of value.
    #[test]
    fn the_skin_meets_the_ground_without_a_seam() {
        let (d, t) = canyon();
        let (mesh, lift) = skin(t, d.lo, d.hi, 7);
        let mesh = mesh.unwrap();
        let pos = f3(&mesh, Mesh::ATTRIBUTE_POSITION);
        let Some(V::Float32x2(uv1)) = mesh.attribute(Mesh::ATTRIBUTE_UV_1) else {
            panic!("no UV_1");
        };
        let mut under = 0;
        for (p, u) in pos.iter().zip(uv1) {
            let (_, l) = lift.at(p.x, p.z).expect("on the lattice");
            if l < -1e-4 {
                under += 1;
                assert!((u[0] - 1.0).abs() < 1e-4, "at {p}: value {} under the ground", u[0]);
            }
        }
        assert!(shade_weight(0.0) == 0.0 && shade_weight(SKIN_OUT_M * SKIN_FLOOR) == 1.0);
        assert!(under > 100, "only {under} vertices where the skin fades under");
    }

    /// No rock, no skin: a flat terrain draws none.
    #[test]
    fn flat_ground_has_no_skin() {
        static FLAT: [i16; 25] = [64; 25];
        let t = TerrainDef {
            cell: 2.0,
            x0: -4.0,
            z0: -4.0,
            nx: 5,
            nz: 5,
            heights: &FLAT,
        };
        let (mesh, lift) = skin(&t, (-4.0, -4.0), (4.0, 4.0), 1);
        assert!(mesh.is_none());
        let (g, l) = lift.at(0.3, -1.1).unwrap();
        assert!((g - 0.5).abs() < 1e-5 && l < 0.0);
    }
}

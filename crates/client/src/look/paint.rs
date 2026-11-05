//! Triple T, spray-painted. The wood is left as it grew; what tells a side
//! and a fighter apart is sprayed on it: a headband in the side's colour
//! round the top of the trunk, the emblem (`emblem.rs`) on its back, and a
//! skin's trim as a stripe round the waist. Spray, not decals: ragged
//! edges, overspray speckled past them, runs dripping off their lower
//! edges, and the bark's cracks left bare where the paint never reached.
//!
//! The paint is a mask over the model's own UVs (its atlas maps every
//! texel of the body once), baked here on the CPU when the first body is
//! dressed: every texel a triangle covers is found where it lies on the
//! body at rest, and painted by where that is. Four coverages, one per
//! channel: r the headband, g the emblem's glyph, b its field (where the
//! glyph is not), a the trim. Their colours are uniforms, so a mask serves
//! both sides and every colour of a shape: one per glyph and field worn.
//!
//! `StandardMaterial` plus a fragment stage (`body_paint.wgsl`) that lays
//! the paint over the wood before lighting. One texture beside the PBR's
//! own, so WebGL2 draws it as the desktop does. Cosmetic as before
//! (DESIGN.md §15): only the headband glows a little, in the side's colour,
//! as the whole body used to, so a fighter still reads in shade.

use std::collections::BTreeMap;

use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

use super::emblem;
use super::textures::hash2;

pub type BodyMaterial = ExtendedMaterial<StandardMaterial, Paint>;

pub const SHADER: &str = "shaders/body_paint.wgsl";

/// Texels a side of the mask. The wood's own map is 1024; paint is flat
/// colour, so half that carries it (about 3 mm a texel on the body).
pub const SIDE: usize = 512;

/// The uniform; each lane is named in `body_paint.wgsl`.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct PaintParams {
    /// Each coat's linear colour; w 1 sprayed, 0 not.
    pub team: Vec4,
    pub glyph: Vec4,
    pub field: Vec4,
    pub trim: Vec4,
    /// rgb: what the headband and the trim glow at full cover, nits.
    pub glow: Vec4,
    pub sheen: Vec4,
    /// x, y: the bark's albedo (luma, before the lift) where paint starts
    /// to take and where it covers; z the lift; w the paint's roughness.
    pub bite: Vec4,
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct Paint {
    #[uniform(100)]
    pub params: PaintParams,
    #[texture(101)]
    #[sampler(102)]
    pub mask: Handle<Image>,
}

impl MaterialExtension for Paint {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

/// Bark darker than this (linear luma, unlifted) takes no paint: the
/// cracks. The paint covers from `BITE_FULL` up.
const BITE_START: f32 = 0.012;
const BITE_FULL: f32 = 0.062;
/// Spray paint dries satin.
const PAINT_ROUGH: f32 = 0.55;

// Where things are on the body at rest, in the mesh's own frame (metres:
// x across, y forward, z down, the feet at 0 and the crown at -1.8).

/// The trunk: an oval round (0, `TRUNK_Y`) of half-widths `TRUNK_R`,
/// from `TRUNK_FOOT` up; a texel inside it (by `TRUNK_SLACK`, for the bark
/// standing proud) is the trunk's, not an arm's.
const TRUNK_Y: f32 = -0.08;
const TRUNK_R: [f32; 2] = [0.2, 0.18];
const TRUNK_SLACK: f32 = 1.35;
const TRUNK_FOOT: f32 = 0.72;
/// The headband: its middle and half its width, metres up; above the
/// eyes (their tops at 1.59).
const BAND_AT: f32 = 1.70;
const BAND_HALF: f32 = 0.047;
/// The trim's stripe round the waist.
const TRIM_AT: f32 = 0.96;
const TRIM_HALF: f32 = 0.016;
/// The emblem on the back: its middle, metres up, and its side.
const EMBLEM_AT: f32 = 1.24;
const EMBLEM_SIDE: f32 = 0.34;
/// A texel faces back (and takes the emblem) past this.
const EMBLEM_FACING: f32 = -0.25;
/// The emblem's shapes are drawn this many texels a side, then sprayed.
const EMBLEM_PX: usize = 160;

/// The unrolled trunk the rings are sprayed on: a texel's side, metres,
/// and the heights it runs between.
const WRAP_TEXEL: f32 = 0.0024;
const WRAP_TOP: f32 = 1.80;
const WRAP_FOOT: f32 = 0.80;
/// The trunk's girth, for the wrap's width (its mean radius).
const WRAP_RADIUS: f32 = 0.19;

/// How a can sprays: the soft edge and the reach of the overspray, metres;
/// how much of that reach is speckled, and how even the cover is.
const EDGE_M: f32 = 0.004;
const OVERSPRAY_M: f32 = 0.02;
const SPECKLE: f32 = 0.45;
const UNEVEN: f32 = 0.12;
/// The runs: one maybe every `DRIP_EVERY_M` along an edge, `DRIP_SHARE`
/// of those running; how long (metres, the shortest and the longest) and
/// how wide.
const DRIP_EVERY_M: f32 = 0.01;
const DRIP_SHARE: f32 = 0.24;
const DRIP_WIDTH_M: f32 = 0.0025;

/// One texel of the mask a triangle covers, where it is on the body at
/// rest, and the rings' cover there.
struct Texel {
    i: u32,
    p: Vec3,
    n: Vec3,
    band: f32,
    trim: f32,
}

/// The model's skin, unrolled onto the mask: every texel a triangle
/// covers.
pub struct Surface {
    texels: Vec<Texel>,
}

impl Surface {
    /// From the model's meshes at rest (positions, normals, UVs, indices).
    /// None if one has none of them on the CPU.
    pub fn new(meshes: &[&Mesh]) -> Option<Self> {
        let mut texels = unroll(meshes)?;
        let band = Raster::wrap(BAND_AT, BAND_HALF, 1).spray(Drips::new(0.012, 0.045), 11);
        let trim = Raster::wrap(TRIM_AT, TRIM_HALF, 2).spray(Drips::new(0.015, 0.06), 12);
        for t in &mut texels {
            if let Some((x, y)) = on_wrap(t.p) {
                t.band = band.at(x, y);
                t.trim = trim.at(x, y);
            }
        }
        Some(Self { texels })
    }

    /// The mask for `emblem`'s shapes (its colours are the material's), as
    /// straight RGBA8 rows, `SIDE`² texels.
    pub fn bake(&self, emblem: proto::Emblem) -> Vec<u8> {
        let shapes = emblem::layers(emblem, EMBLEM_PX as u32);
        let painted = shapes.iter().any(|&[g, f]| g > 0.0 || f > 0.0);
        let (glyph, field) = if painted {
            let side = EMBLEM_SIDE / EMBLEM_PX as f32;
            let g = Raster::flat(shapes.iter().map(|&[g, _]| g).collect(), side);
            // The field under the glyph is left bare: the glyph's colour
            // goes on the wood, not over the field's.
            let f = Raster::flat(shapes.iter().map(|&[g, f]| (f - g).max(0.0)).collect(), side);
            let drips = Drips::new(0.015, 0.06);
            let g = g.spray(drips, 21);
            let mut f = f.spray(drips, 22);
            for (f, g) in f.v.iter_mut().zip(&g.v) {
                *f = (*f - g).max(0.0);
            }
            (Some(g), Some(f))
        } else {
            (None, None)
        };
        let mut cover = vec![[0.0f32; 4]; SIDE * SIDE];
        let mut known = vec![false; SIDE * SIDE];
        for t in &self.texels {
            let mut c = [t.band, 0.0, 0.0, t.trim];
            if let (Some(g), Some(f), Some((u, v))) = (&glyph, &field, on_back(t.p, t.n)) {
                let (x, y) = (u * EMBLEM_PX as f32, v * EMBLEM_PX as f32);
                c[1] = g.at(x, y);
                c[2] = f.at(x, y);
            }
            cover[t.i as usize] = c;
            known[t.i as usize] = true;
        }
        bleed(&mut cover, &mut known, SIDE, 4);
        cover
            .iter()
            .flat_map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
            .collect()
    }
}

/// Every texel of the mask a triangle of `meshes` covers, where it lies on
/// the body at rest. None if a mesh has no positions, normals, UVs or
/// indices on the CPU.
fn unroll(meshes: &[&Mesh]) -> Option<Vec<Texel>> {
    let n = SIDE;
    let mut seen = vec![false; n * n];
    let mut texels = Vec::new();
    for m in meshes {
        let Some(VertexAttributeValues::Float32x3(pos)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else {
            return None;
        };
        let Some(VertexAttributeValues::Float32x3(nor)) = m.attribute(Mesh::ATTRIBUTE_NORMAL) else {
            return None;
        };
        let Some(VertexAttributeValues::Float32x2(uv)) = m.attribute(Mesh::ATTRIBUTE_UV_0) else {
            return None;
        };
        let idx: Vec<usize> = match m.indices()? {
            Indices::U16(v) => v.iter().map(|&i| i as usize).collect(),
            Indices::U32(v) => v.iter().map(|&i| i as usize).collect(),
        };
        for t in idx.chunks_exact(3) {
            let q = [t[0], t[1], t[2]].map(|k| Vec2::from(uv[k]) * n as f32);
            let d = (q[1].y - q[2].y) * (q[0].x - q[2].x) + (q[2].x - q[1].x) * (q[0].y - q[2].y);
            if d.abs() < 1e-12 {
                continue;
            }
            let lo = q[0].min(q[1]).min(q[2]).floor().max(Vec2::ZERO);
            let hi = q[0].max(q[1]).max(q[2]).ceil().min(Vec2::splat(n as f32));
            for y in lo.y as usize..hi.y as usize {
                for x in lo.x as usize..hi.x as usize {
                    let c = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((q[1].y - q[2].y) * (c.x - q[2].x) + (q[2].x - q[1].x) * (c.y - q[2].y)) / d;
                    let w1 = ((q[2].y - q[0].y) * (c.x - q[2].x) + (q[0].x - q[2].x) * (c.y - q[2].y)) / d;
                    let w2 = 1.0 - w0 - w1;
                    let i = y * n + x;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 || seen[i] {
                        continue;
                    }
                    seen[i] = true;
                    let at =
                        |a: &[[f32; 3]]| Vec3::from(a[t[0]]) * w0 + Vec3::from(a[t[1]]) * w1 + Vec3::from(a[t[2]]) * w2;
                    texels.push(Texel {
                        i: i as u32,
                        p: at(pos),
                        n: at(nor).normalize_or_zero(),
                        band: 0.0,
                        trim: 0.0,
                    });
                }
            }
        }
    }
    (!texels.is_empty()).then_some(texels)
}

// Our own arms in view (`hands.rs`): the arms mesh alone, a glove sprayed
// over each hand and up past the wrist, and a band in the side's colour
// round each forearm. Joints at rest in the mesh's frame (x across, the
// right arm toward -x; y forward; z down), metres: from the skin's bind.
const ARM_ELBOW: Vec3 = Vec3::new(0.329, -0.087, -1.054);
const ARM_WRIST: Vec3 = Vec3::new(0.502, -0.03, -0.903);
/// How far up the forearm from the wrist the glove's cuff reaches, and how
/// much its edge wanders round the arm, metres.
const GLOVE_CUFF_M: f32 = 0.045;
const GLOVE_RAG_M: f32 = 0.008;
/// The band: its middle as a share of the way from the elbow to the wrist,
/// and half its width, metres.
const ARM_BAND_AT: f32 = 0.42;
const ARM_BAND_HALF: f32 = 0.022;

/// The view arms' mask, as straight RGBA8 rows, `SIDE`² texels: r the
/// band (the side's colour, the material's `team`), a the glove (its
/// `trim`). None if the mesh is not on the CPU.
pub fn arms_mask(meshes: &[&Mesh]) -> Option<Vec<u8>> {
    let texels = unroll(meshes)?;
    let mut cover = vec![[0.0f32; 4]; SIDE * SIDE];
    let mut known = vec![false; SIDE * SIDE];
    for t in &texels {
        let side = if t.p.x < 0.0 { -1.0 } else { 1.0 };
        let m = |v: Vec3| Vec3::new(v.x * side, v.y, v.z);
        let (e, w) = (m(ARM_ELBOW), m(ARM_WRIST));
        let along = (w - e).normalize();
        let len = (w - e).length();
        // Round the forearm, for the edges' wander.
        let r = t.p - e - along * (t.p - e).dot(along);
        let a = r.z.atan2(r.y);
        let rag = |seed: u32| {
            let ph = hash2(seed as usize, 0, 7) * core::f32::consts::TAU;
            GLOVE_RAG_M * (0.6 * (3.0 * a + ph).sin() + 0.4 * (7.0 * a + 2.0 * ph).sin())
        };
        let spray = |d: f32, i: u32, seed: u32| {
            let soft = ((d / EDGE_M) * 0.5 + 0.5).clamp(0.0, 1.0);
            let near = (1.0 - (-d / OVERSPRAY_M)).clamp(0.0, 1.0);
            let speck = if d < 0.0 && hash2(i as usize, 2, seed) < near * SPECKLE * 0.5 {
                0.4 + 0.6 * hash2(i as usize, 3, seed)
            } else {
                0.0
            };
            (soft * (1.0 - UNEVEN + UNEVEN * hash2(i as usize, 1, seed))).max(speck)
        };
        // Past the cuff toward the fingers: the glove.
        let glove = spray((t.p - w).dot(along) + GLOVE_CUFF_M + rag(31), t.i, 31);
        let u = (t.p - e).dot(along) / len;
        let band = spray(ARM_BAND_HALF - (u - ARM_BAND_AT).abs() * len + rag(32) * 0.5, t.i, 32);
        cover[t.i as usize] = [band * (1.0 - glove), 0.0, 0.0, glove];
        known[t.i as usize] = true;
    }
    bleed(&mut cover, &mut known, SIDE, 4);
    Some(
        cover
            .iter()
            .flat_map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
            .collect(),
    )
}

/// Where a point at rest lies on the unrolled trunk, in its texels; none
/// off the trunk or its rings' heights.
fn on_wrap(p: Vec3) -> Option<(f32, f32)> {
    let h = -p.z;
    if !on_trunk(p) || !(WRAP_FOOT..WRAP_TOP).contains(&h) {
        return None;
    }
    let w = wrap_width() as f32;
    let a = p.x.atan2(p.y);
    Some(((a / core::f32::consts::TAU + 0.5) * w, (WRAP_TOP - h) / WRAP_TEXEL))
}

fn on_trunk(p: Vec3) -> bool {
    let (x, y) = (p.x / TRUNK_R[0], (p.y - TRUNK_Y) / TRUNK_R[1]);
    -p.z > TRUNK_FOOT && x * x + y * y < TRUNK_SLACK
}

/// Where a point at rest lies on the emblem, 0..1 across and down as seen
/// from behind; none off the back.
fn on_back(p: Vec3, n: Vec3) -> Option<(f32, f32)> {
    if !on_trunk(p) || n.y > EMBLEM_FACING {
        return None;
    }
    // Seen from behind, the body's left (+x) is on the left.
    let u = 0.5 - p.x / EMBLEM_SIDE;
    let v = (EMBLEM_AT + EMBLEM_SIDE * 0.5 + p.z) / EMBLEM_SIDE;
    ((0.0..1.0).contains(&u) && (0.0..1.0).contains(&v)).then_some((u, v))
}

fn wrap_width() -> usize {
    (core::f32::consts::TAU * WRAP_RADIUS / WRAP_TEXEL) as usize
}

/// Texels nothing covers take their covered neighbours' mean, `passes`
/// texels out, so a texel sampled across an island's edge finds the
/// island's paint, not bare.
fn bleed(cover: &mut [[f32; 4]], known: &mut [bool], n: usize, passes: usize) {
    for _ in 0..passes {
        let was = known.to_vec();
        let prev = cover.to_vec();
        for y in 0..n {
            for x in 0..n {
                let i = y * n + x;
                if was[i] {
                    continue;
                }
                let (mut sum, mut k) = ([0.0f32; 4], 0.0);
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= n as i32 || ny >= n as i32 {
                        continue;
                    }
                    let j = ny as usize * n + nx as usize;
                    if was[j] {
                        for c in 0..4 {
                            sum[c] += prev[j][c];
                        }
                        k += 1.0;
                    }
                }
                if k > 0.0 {
                    cover[i] = sum.map(|s| s / k);
                    known[i] = true;
                }
            }
        }
    }
}

/// How a coat runs: the shortest and the longest run, metres.
#[derive(Clone, Copy)]
struct Drips {
    short: f32,
    long: f32,
}

impl Drips {
    const fn new(short: f32, long: f32) -> Self {
        Self { short, long }
    }
}

/// A coat's cover on a grid of square texels `side` metres, rows top
/// down; `wrap` joins its left edge to its right (round the trunk).
struct Raster {
    w: usize,
    h: usize,
    side: f32,
    wrap: bool,
    v: Vec<f32>,
}

impl Raster {
    /// A square grid of `v`.
    fn flat(v: Vec<f32>, side: f32) -> Self {
        let w = (v.len() as f32).sqrt() as usize;
        Self {
            w,
            h: w,
            side,
            wrap: false,
            v,
        }
    }

    /// A ring round the unrolled trunk at `at` metres up, `half` wide each
    /// way, as a hand sprays it: wavering up and down round the trunk, and
    /// in width.
    fn wrap(at: f32, half: f32, seed: u32) -> Self {
        use core::f32::consts::TAU;
        let (w, h) = (wrap_width(), ((WRAP_TOP - WRAP_FOOT) / WRAP_TEXEL) as usize);
        let ph: [f32; 4] = core::array::from_fn(|k| hash2(k, 0, seed) * TAU);
        let mut v = vec![0.0; w * h];
        for x in 0..w {
            let a = (x as f32 + 0.5) / w as f32 * TAU - TAU * 0.5;
            let mid =
                at + 0.007 * (a + ph[0]).sin() + 0.004 * (2.0 * a + ph[1]).sin() + 0.002 * (5.0 * a + ph[2]).sin();
            let hw = half * (1.0 + 0.08 * (3.0 * a + ph[3]).sin());
            for y in 0..h {
                let up = WRAP_TOP - (y as f32 + 0.5) * WRAP_TEXEL;
                if (up - mid).abs() < hw {
                    v[y * w + x] = 1.0;
                }
            }
        }
        Self {
            w,
            h,
            side: WRAP_TEXEL,
            wrap: true,
            v,
        }
    }

    /// The cover at texel coordinates (x, y), bilinear; none off the grid
    /// (round it, if it wraps).
    fn at(&self, x: f32, y: f32) -> f32 {
        let (x, y) = (x - 0.5, y - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let get = |xi: i32, yi: i32| -> f32 {
            if yi < 0 || yi >= self.h as i32 {
                return 0.0;
            }
            let xi = if self.wrap {
                xi.rem_euclid(self.w as i32)
            } else if xi < 0 || xi >= self.w as i32 {
                return 0.0;
            } else {
                xi
            };
            self.v[yi as usize * self.w + xi as usize]
        };
        let (x0, y0) = (x0 as i32, y0 as i32);
        let top = get(x0, y0) * (1.0 - fx) + get(x0 + 1, y0) * fx;
        let bot = get(x0, y0 + 1) * (1.0 - fx) + get(x0 + 1, y0 + 1) * fx;
        top * (1.0 - fy) + bot * fy
    }

    /// A box blur `r` texels each way, across then down.
    fn blur(&self, r: usize) -> Vec<f32> {
        let (w, h) = (self.w as i32, self.h as i32);
        let r = r as i32;
        let k = (2 * r + 1) as f32;
        let mut across = vec![0.0; self.v.len()];
        for y in 0..h {
            for x in 0..w {
                let mut s = 0.0;
                for d in -r..=r {
                    let xi = x + d;
                    let xi = if self.wrap {
                        xi.rem_euclid(w)
                    } else if xi < 0 || xi >= w {
                        continue;
                    } else {
                        xi
                    };
                    s += self.v[(y * w + xi) as usize];
                }
                across[(y * w + x) as usize] = s / k;
            }
        }
        let mut out = vec![0.0; self.v.len()];
        for y in 0..h {
            for x in 0..w {
                let mut s = 0.0;
                for d in -r..=r {
                    let yi = y + d;
                    if (0..h).contains(&yi) {
                        s += across[(yi * w + x) as usize];
                    }
                }
                out[(y * w + x) as usize] = s / k;
            }
        }
        out
    }

    fn with(&self, v: Vec<f32>) -> Self {
        Self {
            w: self.w,
            h: self.h,
            side: self.side,
            wrap: self.wrap,
            v,
        }
    }

    fn texels(&self, m: f32) -> usize {
        ((m / self.side).round() as usize).max(1)
    }

    /// This shape as a can sprays it: a soft edge, an uneven cover,
    /// overspray speckled past the edge, and runs off its lower edges.
    fn spray(&self, drips: Drips, seed: u32) -> Self {
        let soft = self.with(self.blur(self.texels(EDGE_M))).blur(1);
        let halo = self.blur(self.texels(OVERSPRAY_M));
        let mut out: Vec<f32> = soft
            .iter()
            .enumerate()
            .map(|(i, &s)| s * (1.0 - UNEVEN + UNEVEN * hash2(i, 1, seed)))
            .collect();
        for (i, o) in out.iter_mut().enumerate() {
            let near = halo[i];
            if near > 0.02 && hash2(i, 2, seed) < near * SPECKLE {
                *o = o.max(0.4 + 0.6 * hash2(i, 3, seed));
            }
        }
        let soft = self.with(soft);
        let every = self.texels(DRIP_EVERY_M).max(2);
        let mut b = 0;
        while b < self.w {
            let k = b / every;
            if hash2(k, 4, seed) < DRIP_SHARE {
                let cx = b + (hash2(k, 5, seed) * every as f32) as usize;
                let wid = (DRIP_WIDTH_M / self.side).max(1.0) * (0.7 + 0.6 * hash2(k, 6, seed));
                let len = (drips.short + (drips.long - drips.short) * hash2(k, 7, seed).powf(1.5)) / self.side;
                soft.run(&mut out, cx.min(self.w - 1), wid, len);
            }
            b += every;
        }
        self.with(out)
    }

    /// A run down column `cx` from every lower edge it crosses: `wid`
    /// texels wide, narrowing, `len` long, a bead where it stops.
    fn run(&self, out: &mut [f32], cx: usize, wid: f32, len: f32) {
        let bead = (wid * 2.0) as usize;
        for y in 1..self.h {
            if !(self.v[(y - 1) * self.w + cx] > 0.5 && self.v[y * self.w + cx] <= 0.5) {
                continue;
            }
            let l = len as usize;
            for k in 0..l + bead {
                let yy = y + k;
                if yy >= self.h {
                    break;
                }
                let r = if k < l {
                    wid * (1.0 - 0.45 * k as f32 / len.max(1.0))
                } else {
                    let t = (k - l) as f32 / (wid * 2.0);
                    wid * 1.3 * (1.0 - t * t).max(0.0).sqrt()
                };
                let reach = r as i32 + 1;
                for dx in -reach..=reach {
                    let xi = cx as i32 + dx;
                    let xi = if self.wrap {
                        xi.rem_euclid(self.w as i32)
                    } else if xi < 0 || xi >= self.w as i32 {
                        continue;
                    } else {
                        xi
                    };
                    let c = (r + 0.5 - dx.abs() as f32).clamp(0.0, 1.0) * 0.95;
                    let o = &mut out[yy * self.w + xi as usize];
                    *o = o.max(c);
                }
            }
        }
    }
}

/// The unrolled body and every mask baked on it, by the emblem's shapes
/// (its glyph and field; colours are the material's).
#[derive(Resource, Default)]
pub struct Paints {
    surface: Option<Surface>,
    /// The model had no mesh data on the CPU: never painted.
    failed: bool,
    masks: BTreeMap<(u8, u8), Handle<Image>>,
}

impl Paints {
    /// The mask for `emblem`, unrolling the body from `meshes` the first
    /// time. A bare one if the model's meshes are not on the CPU.
    pub fn mask(&mut self, emblem: proto::Emblem, meshes: &[&Mesh], images: &mut Assets<Image>) -> Handle<Image> {
        if self.surface.is_none() && !self.failed {
            self.surface = Surface::new(meshes);
            if self.surface.is_none() {
                error!("paint: models/triple-t.glb's meshes have no positions, normals or UVs on the CPU: unpainted");
                self.failed = true;
            }
        }
        let (g, f, ..) = proto::emblem_parts(proto::emblem_valid(emblem));
        let key = if self.failed { (0, 0) } else { (g, f) };
        let surface = self.surface.as_ref();
        self.masks
            .entry(key)
            .or_insert_with(|| {
                let data = match surface {
                    Some(s) => s.bake(proto::emblem(g, f, 0, 0)),
                    None => vec![0; SIDE * SIDE * 4],
                };
                images.add(super::textures::baked_image(data, SIDE, false, false))
            })
            .clone()
    }
}

/// A palette colour, linear.
fn palette(k: u8) -> Vec4 {
    let [r, g, b] = emblem::PALETTE[k as usize & 15];
    let c = Color::srgb_u8(r, g, b).to_linear();
    Vec4::new(c.red, c.green, c.blue, 1.0)
}

/// What a body's paint is: the side's colour (`team`, linear), its emblem's
/// colours, its trim, and how bright the headband and the trim glow (nits
/// a unit of colour), over wood lifted by `lift`.
pub fn params(
    team: LinearRgba,
    emblem: proto::Emblem,
    trim: Option<LinearRgba>,
    glow: f32,
    sheen: f32,
    lift: f32,
) -> PaintParams {
    let (g, _, gc, fc) = proto::emblem_parts(proto::emblem_valid(emblem));
    let worn = if g == 0 { Vec4::ZERO } else { Vec4::W };
    let rgb = |c: LinearRgba| Vec3::new(c.red, c.green, c.blue);
    let trim_rgb = trim.map_or(Vec3::ZERO, rgb);
    PaintParams {
        team: rgb(team).extend(1.0),
        glyph: palette(gc) * worn.w,
        field: palette(fc) * worn.w,
        trim: trim_rgb.extend(if trim.is_some() { 1.0 } else { 0.0 }),
        glow: (rgb(team) * glow).extend(0.0),
        sheen: (trim_rgb * sheen).extend(0.0),
        bite: Vec4::new(BITE_START, BITE_FULL, lift, PAINT_ROUGH),
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Paints>()
        .add_plugins(MaterialPlugin::<BodyMaterial>::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ring_wraps_round_the_trunk_and_runs_down_from_it() {
        let r = Raster::wrap(BAND_AT, BAND_HALF, 1);
        let s = r.spray(Drips::new(0.012, 0.045), 11);
        // Its middle is covered all the way round, seam and all.
        let mid = (WRAP_TOP - BAND_AT) / WRAP_TEXEL;
        for x in [0.0, 1.0, s.w as f32 * 0.5, s.w as f32 - 0.5] {
            assert!(s.at(x, mid) > 0.6, "{x}: {}", s.at(x, mid));
        }
        // Well above it, bare; below it, some runs.
        let above = (WRAP_TOP - BAND_AT - 0.09) / WRAP_TEXEL;
        assert!((0..s.w).all(|x| s.at(x as f32 + 0.5, above) == 0.0));
        let below = (WRAP_TOP - BAND_AT + BAND_HALF + 0.022) / WRAP_TEXEL;
        let runs = (0..s.w).filter(|&x| s.at(x as f32 + 0.5, below) > 0.5).count();
        assert!(runs > 0 && runs < s.w / 4, "{runs}");
    }

    #[test]
    fn the_emblem_is_seen_unmirrored_from_behind() {
        // The body's right (-x) is on the right seen from behind; the top
        // of the emblem is up.
        let n = Vec3::NEG_Y;
        let at = |x: f32, up: f32| on_back(Vec3::new(x, -0.2, -up), n);
        let (u, _) = at(-0.1, EMBLEM_AT).unwrap();
        assert!(u > 0.5);
        let (_, v) = at(0.0, EMBLEM_AT + 0.1).unwrap();
        assert!(v < 0.5);
        // Not on the front, nor an arm.
        assert!(on_back(Vec3::new(0.0, 0.05, -EMBLEM_AT), Vec3::Y).is_none());
        assert!(at(0.4, EMBLEM_AT).is_none());
    }
}

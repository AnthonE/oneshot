//! Marks the world keeps: bullet holes and blast scorches.
//!
//! After Gates' `crates/client/src/render/decal.rs` (MIT, © 2026 MoreRight
//! DAO): every mark is a quad in ONE mesh, drawn with ONE lit, blended
//! material over a generated atlas (value × alpha), with a second atlas of
//! each mark's relief as a normal map, so a hole is a pit the sun lights one
//! side of. One draw on the desktop and in the browser alike. A fixed pool,
//! drop-oldest; the mesh is rewritten only when a mark is placed or a fade
//! step moves. Our surfaces are flat (boxes, the heightfield), so Gates'
//! conforming to meshes is left out. The holes' and the scorch's texels are
//! Gates' `texel` cells, unchanged.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::ImageSampler;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::atlas::{fbm, hash01, smooth, vnoise};
use super::table::{Matter, Weapon};

/// Seconds a mark stays, then how long it fades.
pub const LIFE_S: f32 = 30.0;
pub const FADE_S: f32 = 4.0;
/// Lifted this far off its surface along the normal.
const LIFT_M: f32 = 0.008;
/// Fade steps: the mesh is rewritten when one moves, not every frame.
const ALPHA_STEPS: f32 = 32.0;

const ATLAS_COLS: u32 = 4;
const ATLAS_ROWS: u32 = 2;
const CELL_TEX: u32 = 128;

/// What a mark is: a pair of variant cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    HoleSoil,
    HoleStone,
    HoleMetal,
    Scorch,
}

impl Kind {
    fn first_cell(self) -> u32 {
        match self {
            Kind::HoleSoil => 0,
            Kind::HoleStone => 2,
            Kind::HoleMetal => 4,
            Kind::Scorch => 6,
        }
    }
}

/// Which mark a blow leaves and how wide it is, metres: none on a body or
/// a hull (they move).
pub fn kind(weapon: Weapon, matter: Matter) -> Option<(Kind, f32)> {
    let k = match weapon {
        Weapon::Blast => 1.0,
        Weapon::Sniper => 1.5,
        Weapon::Pellet => 0.7,
        Weapon::Melee => return None,
        Weapon::Bullet => 1.0,
    };
    Some(match (weapon, matter) {
        (_, Matter::Shield | Matter::Flesh | Matter::Hull) => return None,
        (Weapon::Blast, _) => (Kind::Scorch, 2.6),
        (_, Matter::Concrete) => (Kind::HoleStone, 0.15 * k),
        (_, Matter::Metal) => (Kind::HoleMetal, 0.12 * k),
        (_, Matter::Dirt) => (Kind::HoleSoil, 0.2 * k),
    })
}

/// A mark's colour on its matter; the atlas carries the value.
fn tint(kind: Kind, matter: Matter) -> [f32; 3] {
    let c = match (kind, matter) {
        (Kind::Scorch, _) => Color::srgb(0.17, 0.15, 0.13),
        (_, Matter::Metal) => Color::srgb(0.84, 0.84, 0.86),
        (_, Matter::Dirt) => Color::srgb(0.36, 0.27, 0.19),
        _ => Color::srgb(0.80, 0.78, 0.74),
    };
    c.to_linear().to_f32_array_no_alpha()
}

/// A mark's four corners, their uvs, and its colour.
type Quad = ([[f32; 3]; 4], [[f32; 2]; 4], [f32; 4]);

#[derive(Clone, Copy, Default)]
struct Mark {
    /// Seconds left through life and fade; zero is a free slot.
    left: f32,
    /// The fade step last written.
    step: i32,
    pos: Vec3,
    normal: Vec3,
    /// The atlas's +u on the surface.
    tangent: Vec3,
    size: f32,
    cell: u32,
    tint: [f32; 3],
}

pub struct Marks {
    marks: Vec<Mark>,
    next: usize,
    rng: u32,
    dirty: bool,
    mesh: Handle<Mesh>,
}

impl Marks {
    pub fn new(cap: usize) -> Marks {
        Marks {
            marks: vec![Mark::default(); cap],
            next: 0,
            rng: 0x2545_f491,
            dirty: false,
            mesh: Handle::default(),
        }
    }

    fn roll(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1 << 24) as f32
    }

    /// Leave a mark of `kind`, `size` metres, at `at` on a surface facing
    /// `normal`, turned and sized a little at random. Drop-oldest: the
    /// oldest is the faintest.
    pub fn place(&mut self, kind: Kind, size: f32, at: Vec3, normal: Vec3, matter: Matter) {
        let n = normal.normalize_or(Vec3::Y);
        let i = self.marks.iter().position(|m| m.left <= 0.0).unwrap_or_else(|| {
            let i = self.next;
            self.next = (self.next + 1) % self.marks.len();
            i
        });
        // Any direction on the surface, turned at random.
        let seed = if n.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
        let t0 = seed.cross(n).normalize();
        let b0 = n.cross(t0);
        let a = self.roll() * core::f32::consts::TAU;
        let tangent = t0 * a.cos() + b0 * a.sin();
        let cell = kind.first_cell() + (self.roll() < 0.5) as u32;
        let size = size * (0.85 + 0.3 * self.roll());
        self.marks[i] = Mark {
            left: LIFE_S + FADE_S,
            step: -1,
            pos: at,
            normal: n,
            tangent,
            size,
            cell,
            tint: tint(kind, matter),
        };
        self.dirty = true;
    }

    /// Forget every mark (a new map).
    pub fn clear(&mut self) {
        for m in &mut self.marks {
            m.left = 0.0;
        }
        self.dirty = true;
    }

    /// The mesh the marks are drawn in.
    pub fn mesh(&self) -> &Handle<Mesh> {
        &self.mesh
    }

    /// Age every mark; whether the mesh must be rewritten.
    pub fn fade(&mut self, dt: f32) -> bool {
        let mut dirty = std::mem::take(&mut self.dirty);
        for m in &mut self.marks {
            if m.left <= 0.0 {
                continue;
            }
            m.left = (m.left - dt).max(0.0);
            let s = step(m.left);
            if s != m.step {
                m.step = s;
                dirty = true;
            }
        }
        dirty
    }

    /// Mark `i`'s four corners, their uvs, its colour; `None` for a free slot.
    fn quad(&self, i: usize) -> Option<Quad> {
        let m = &self.marks[i];
        if m.left <= 0.0 {
            return None;
        }
        let alpha = (m.step as f32 / ALPHA_STEPS).clamp(0.0, 1.0);
        let bit = m.normal.cross(m.tangent);
        let c = m.pos + m.normal * LIFT_M;
        let (u, v) = (m.tangent * m.size * 0.5, bit * m.size * 0.5);
        let (cu, cv) = ((m.cell % ATLAS_COLS) as f32, (m.cell / ATLAS_COLS) as f32);
        let inset = 0.5 / CELL_TEX as f32;
        let at = |s: f32, t: f32| {
            [
                (cu + inset + s * (1.0 - 2.0 * inset)) / ATLAS_COLS as f32,
                (cv + inset + t * (1.0 - 2.0 * inset)) / ATLAS_ROWS as f32,
            ]
        };
        Some((
            [
                (c - u - v).to_array(),
                (c + u - v).to_array(),
                (c + u + v).to_array(),
                (c - u + v).to_array(),
            ],
            [at(0.0, 1.0), at(1.0, 1.0), at(1.0, 0.0), at(0.0, 0.0)],
            [m.tint[0], m.tint[1], m.tint[2], alpha],
        ))
    }

    /// Copy the marks into `mesh`, in place, an attribute at a time.
    pub fn write(&self, mesh: &mut Mesh) {
        let n = self.marks.len();
        if let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
            for i in 0..n {
                let q = self.quad(i).map_or([[0.0; 3]; 4], |q| q.0);
                pos[i * 4..i * 4 + 4].copy_from_slice(&q);
            }
        }
        if let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
            for i in 0..n {
                if let Some(q) = self.quad(i) {
                    uv[i * 4..i * 4 + 4].copy_from_slice(&q.1);
                }
            }
        }
        if let Some(VertexAttributeValues::Float32x4(col)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
            for i in 0..n {
                let c = self.quad(i).map_or([0.0; 4], |q| q.2);
                col[i * 4..i * 4 + 4].copy_from_slice(&[c; 4]);
            }
        }
        if let Some(VertexAttributeValues::Float32x3(nor)) = mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL) {
            for (i, m) in self.marks.iter().enumerate() {
                nor[i * 4..i * 4 + 4].copy_from_slice(&[m.normal.to_array(); 4]);
            }
        }
        if let Some(VertexAttributeValues::Float32x4(tan)) = mesh.attribute_mut(Mesh::ATTRIBUTE_TANGENT) {
            for (i, m) in self.marks.iter().enumerate() {
                tan[i * 4..i * 4 + 4].copy_from_slice(&[[m.tangent.x, m.tangent.y, m.tangent.z, 1.0]; 4]);
            }
        }
    }
}

fn step(left: f32) -> i32 {
    ((left / FADE_S).min(1.0) * ALPHA_STEPS).ceil() as i32
}

/// The mark mesh, its material over the two atlases, always drawn
/// (collapsed quads until something is marked).
pub fn setup(
    commands: &mut Commands,
    marks: &mut Marks,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    let n = marks.marks.len();
    let mut idx = Vec::with_capacity(n * 6);
    for q in 0..n as u32 {
        let b = q * 4;
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; n * 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n * 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; n * 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; n * 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0f32, 0.0, 0.0, 1.0]; n * 4]);
    mesh.insert_indices(Indices::U32(idx));
    marks.mesh = meshes.add(mesh);
    let (albedo, relief) = atlas_images();
    let mat = mats.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(images.add(albedo)),
        normal_map_texture: Some(images.add(relief)),
        perceptual_roughness: 0.9,
        alpha_mode: AlphaMode::Blend,
        depth_bias: 8.0,
        ..default()
    });
    commands.spawn((
        Mesh3d(marks.mesh.clone()),
        MeshMaterial3d(mat),
        Transform::IDENTITY,
        Visibility::Visible,
        NoFrustumCulling,
        NotShadowCaster,
    ));
}

// ─── The atlas (Gates' texels) ──────────────────────────────────────────────

fn around(a: f32, k: f32, seed: u32) -> f32 {
    vnoise(a.cos() * k + 7.0, a.sin() * k + 7.0, seed)
}

/// Radial rays: coverage of `n` jittered spokes whose length is rolled per
/// spoke, tapering to their tips.
fn spokes(u: f32, v: f32, n: f32, len: f32, width: f32, seed: u32) -> f32 {
    let r = (u * u + v * v).sqrt();
    let a = v.atan2(u);
    let t = (a / core::f32::consts::TAU + 0.5) * n;
    let id = t.floor();
    let f = t - id;
    let jit = hash01(id as u32, seed);
    let l = len * (0.45 + 0.55 * hash01(id as u32, seed ^ 0x33));
    if r >= l {
        return 0.0;
    }
    let w = width * (1.0 - r / l);
    let centre = 0.3 + 0.4 * jit;
    smooth(w, w * 0.4, (f - centre).abs() * r.max(0.05) * 6.0)
}

/// Value, alpha and relief (a height in cell widths) of `cell` at
/// `(u, v) ∈ [-1, 1]²`.
fn texel(cell: u32, u: f32, v: f32) -> (f32, f32, f32) {
    let seed = 0x5EED_0000 ^ cell.wrapping_mul(0x0101_0101);
    let r = (u * u + v * v).sqrt();
    let a = v.atan2(u);
    let n = fbm(u * 5.0, v * 5.0, seed);
    let (value, alpha, height) = match cell / 2 {
        // A soil crater: a dark pit, the slope lighter, clods thrown out.
        0 => {
            let rn = r / (0.6 + 0.3 * (around(a, 1.6, seed) - 0.5));
            let pit = smooth(0.4, 0.0, rn);
            let body = 1.0 - smooth(0.5, 0.95, rn);
            let clods = smooth(0.62, 0.74, fbm(u * 9.0 + 3.0, v * 9.0, seed ^ 7))
                * smooth(0.6, 0.85, rn)
                * (1.0 - smooth(0.95, 1.35, rn));
            let rim = smooth(0.3, 0.5, rn) * (1.0 - smooth(0.5, 0.85, rn));
            (
                (0.7 - 0.55 * pit + 0.25 * (n - 0.5)).clamp(0.05, 1.0),
                (body * (0.7 + 0.3 * n)).max(clods * 0.9),
                -0.06 * (1.0 - smooth(0.0, 0.5, rn)) + 0.012 * rim + 0.015 * clods,
            )
        }
        // Stone: a dark pit in a pale chipped crater, cracks running out.
        1 => {
            let pit = smooth(0.14, 0.07, r / (0.8 + 0.4 * around(a, 2.5, seed)));
            let chip = 1.0 - smooth(0.26, 0.42, r / (0.75 + 0.5 * around(a, 2.0, seed ^ 2)));
            let cracks = spokes(u, v, 7.0, 0.95, 0.12, seed ^ 5);
            let dust = (1.0 - smooth(0.3, 0.75, r)) * 0.3;
            let base = 0.9 - 0.2 * n;
            let value = if pit > 0.5 {
                0.08
            } else if cracks > chip {
                0.22
            } else {
                base
            };
            (
                value,
                pit.max(chip * 0.95).max(cracks * 0.9).max(dust),
                -0.07 * pit - 0.025 * chip * (1.0 - pit) - 0.006 * cracks,
            )
        }
        // Metal: a dark centre, a bright ring of bare metal, a scuffed halo.
        2 => {
            let hole = smooth(0.1, 0.06, r);
            let rim = 1.0 - smooth(0.17, 0.24, r / (0.9 + 0.2 * around(a, 3.0, seed)));
            let scratch = vnoise(a * 9.0, r * 2.0, seed ^ 4);
            let halo = (1.0 - smooth(0.24, 0.58, r)) * (0.35 + 0.65 * scratch);
            let value = if hole > 0.5 { 0.05 } else { 0.8 + 0.2 * rim };
            (
                value,
                hole.max(rim).max(halo * 0.75),
                -0.06 * hole + 0.012 * rim * (1.0 - hole) - 0.02 * (1.0 - smooth(0.0, 0.5, r)),
            )
        }
        // Scorch: soot, darkest at the centre, ragged at the edge.
        3 => {
            let rn = r / (0.82 + 0.3 * (around(a, 2.0, seed) - 0.5));
            let body = 1.0 - smooth(0.35, 1.0, rn);
            (0.25 + 0.5 * rn.min(1.0) * n, body * (0.6 + 0.4 * n), 0.0)
        }
        _ => (0.0, 0.0, 0.0),
    };
    // Nothing reaches the cell's edge, so no mip bleeds a neighbour in.
    let pad = 1.0 - smooth(0.88, 0.97, u.abs().max(v.abs()));
    (value.clamp(0.0, 1.0), (alpha * pad).clamp(0.0, 1.0), height * pad)
}

/// Both atlases: value (sRGB, grey in RGB, coverage in A) and relief (a
/// linear tangent-space normal), each with its mip chain.
fn atlas_images() -> (Image, Image) {
    let (w, h) = (ATLAS_COLS * CELL_TEX, ATLAS_ROWS * CELL_TEX);
    let mut albedo = vec![0u8; (w * h * 4) as usize];
    let mut relief = vec![0u8; (w * h * 4) as usize];
    for px in relief.chunks_exact_mut(4) {
        px.copy_from_slice(&[128, 128, 255, 255]);
    }
    let n = CELL_TEX as usize;
    let mut heights = vec![0.0f32; n * n];
    for cell in 0..ATLAS_COLS * ATLAS_ROWS {
        let (cx, cy) = (cell % ATLAS_COLS, cell / ATLAS_COLS);
        for y in 0..CELL_TEX {
            for x in 0..CELL_TEX {
                let u = (x as f32 + 0.5) / CELL_TEX as f32 * 2.0 - 1.0;
                let v = (y as f32 + 0.5) / CELL_TEX as f32 * 2.0 - 1.0;
                let (val, a, ht) = texel(cell, u, v);
                heights[y as usize * n + x as usize] = ht;
                let i = (((cy * CELL_TEX + y) * w + cx * CELL_TEX + x) * 4) as usize;
                let g = (val * 255.0 + 0.5) as u8;
                albedo[i..i + 4].copy_from_slice(&[g, g, g, (a * 255.0 + 0.5) as u8]);
            }
        }
        // Slope per cell width: a central difference spans two texels.
        let k = CELL_TEX as f32 * 0.5;
        for y in 0..n {
            for x in 0..n {
                let at = |xx: usize, yy: usize| heights[yy.min(n - 1) * n + xx.min(n - 1)];
                let sx = (at(x + 1, y) - at(x.saturating_sub(1), y)) * k;
                let sy = (at(x, y + 1) - at(x, y.saturating_sub(1))) * k;
                let nv = Vec3::new(-sx, sy, 1.0).normalize();
                let i = (((cy as usize * n + y) * w as usize) + cx as usize * n + x) * 4;
                relief[i] = ((nv.x * 0.5 + 0.5) * 255.0 + 0.5) as u8;
                relief[i + 1] = ((nv.y * 0.5 + 0.5) * 255.0 + 0.5) as u8;
                relief[i + 2] = ((nv.z * 0.5 + 0.5) * 255.0 + 0.5) as u8;
            }
        }
    }
    let image = |data: Vec<u8>, format: TextureFormat, normal: bool| {
        let mut img = Image::new(
            Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            format,
            RenderAssetUsages::RENDER_WORLD,
        );
        crate::look::textures::mip_in_place(&mut img, normal);
        img.sampler = ImageSampler::linear();
        img
    };
    (
        image(albedo, TextureFormat::Rgba8UnormSrgb, false),
        image(relief, TextureFormat::Rgba8Unorm, true),
    )
}

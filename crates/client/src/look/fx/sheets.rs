//! The baked flipbook sheets and the material that draws them.
//!
//! `tools/flipbooks/bake.py` runs a small fluid simulation for each strip
//! (smoke, dust, fire, plasma, a muzzle's flash, an explosion) and bakes
//! every frame lit from six sides, so the shader (`shaders/fx_sheet.wgsl`)
//! lights a puff from wherever the sun stands and fades it against the
//! wall behind it. Two pools draw them: `cloud` (16 × 16 cells of 128 on a
//! 2048² sheet: smoke, dust, fire, plasma, flashes, wisps) and `boom`
//! (8 × 8 of 256: the explosions). Each pool is one mesh, premultiplied:
//! a texel with coverage is smoke lit like the world, a texel with heat
//! adds light through a ramp (`Particle::tone`), and the two mix in one
//! sprite, so a fireball turns to soot in place.
//!
//! The cut of each sheet is [`CLOUD`] and [`BOOM`] here and `SHEETS` in
//! the baker; change both.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;

pub use ::fx::pool::Sheet;
use ::fx::pool::{Cam, Pool, Verts};

pub const SHADER: &str = "shaders/fx_sheet.wgsl";

/// The cloud sheet: 16 × 16 cells of 128 texels.
pub const CLOUD: Sheet = Sheet {
    cols: 16,
    rows: 16,
    tex: 128,
};
/// The explosions' sheet: 8 × 8 cells of 256.
pub const BOOM: Sheet = Sheet {
    cols: 8,
    rows: 8,
    tex: 256,
};

/// A strip on a sheet: its first cell, its variants (each the next
/// flipbook along) and the frames in each.
#[derive(Clone, Copy, Debug)]
pub struct Strip {
    pub cell: u8,
    pub variants: u8,
    pub frames: u8,
}

/// The cloud sheet's strips, as the baker lays them.
pub const SMOKE: Strip = Strip {
    cell: 0,
    variants: 4,
    frames: 16,
};
pub const DUST: Strip = Strip {
    cell: 64,
    variants: 4,
    frames: 16,
};
pub const FIRE: Strip = Strip {
    cell: 128,
    variants: 2,
    frames: 16,
};
pub const PLASMA: Strip = Strip {
    cell: 160,
    variants: 2,
    frames: 16,
};
pub const FLASH: Strip = Strip {
    cell: 192,
    variants: 6,
    frames: 4,
};
pub const BLOOM: Strip = Strip {
    cell: 216,
    variants: 2,
    frames: 4,
};
pub const WISP: Strip = Strip {
    cell: 224,
    variants: 2,
    frames: 16,
};
/// The boom sheet's one strip.
pub const EXPLOSION: Strip = Strip {
    cell: 0,
    variants: 2,
    frames: 32,
};

/// The shader's emissive ramps a sheet's heat goes through
/// (`Particle::tone`): powder and fuel, ion blue, the cannon's green, the
/// SHARD GUN's pink. A tone between two blends them.
pub const TONE_FIRE: f32 = 0.0;
pub const TONE_PLASMA: f32 = 1.0;
pub const TONE_GREEN: f32 = 2.0;
pub const TONE_PINK: f32 = 3.0;

/// How far in front of a wall a puff is faded to nothing against it,
/// metres: a sprite cutting through the floor shows no line.
pub const SOFT_M: f32 = 0.6;
/// From what share of its life a puff erodes (its thin parts going
/// first) rather than fading evenly.
pub const ERODE_FROM: f32 = 0.55;

/// One sheet pair's material.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
#[bind_group_data(SheetKey)]
pub struct SheetMaterial {
    /// x: the soft fade distance, metres; y: the erosion start, 0–1 of a
    /// life; z, w: spare.
    #[uniform(0)]
    pub params: Vec4,
    /// Light from the right, left and top; coverage.
    #[texture(1)]
    #[sampler(2)]
    pub a: Handle<Image>,
    /// Light from below, the front and behind; heat.
    #[texture(3)]
    #[sampler(4)]
    pub b: Handle<Image>,
    /// Whether the sun's shadow falls on the smoke (a shadow map read per
    /// texel: the desktop's and WebGPU's rungs).
    pub shadowed: bool,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct SheetKey {
    shadowed: bool,
}

impl From<&SheetMaterial> for SheetKey {
    fn from(m: &SheetMaterial) -> Self {
        SheetKey { shadowed: m.shadowed }
    }
}

impl Material for SheetMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    /// Premultiplied: `rgb + dst × (1 − a)`, so coverage covers and heat
    /// adds, in one sprite.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    /// Transparent, so never in a prepass; off, so no pipeline is built
    /// for one.
    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if key.bind_group_data.shadowed {
            descriptor.vertex.shader_defs.push("FX_SHADOWS".into());
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("FX_SHADOWS".into());
            }
        }
        Ok(())
    }
}

/// A sheet's sampler: linear, clamped (a cell never wraps), filtered
/// between its mips once [`mip_sheets`] has built them.
fn sampler() -> ImageSamplerDescriptor {
    ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    }
}

/// Both sheets of `name` (`fx/<name>_a.png`, `fx/<name>_b.png`), linear:
/// they hold light and coverage, not colour.
pub fn load(assets: &AssetServer, name: &str) -> (Handle<Image>, Handle<Image>) {
    let linear = |s: &mut ImageLoaderSettings| {
        s.is_srgb = false;
        s.sampler = ImageSampler::Descriptor(sampler());
    };
    (
        assets.load_with_settings(format!("fx/{name}_a.png"), linear),
        assets.load_with_settings(format!("fx/{name}_b.png"), linear),
    )
}

/// A sheet's material, at `shadowed`.
pub fn material(a: Handle<Image>, b: Handle<Image>, shadowed: bool) -> SheetMaterial {
    SheetMaterial {
        params: Vec4::new(SOFT_M, ERODE_FROM, 0.0, 0.0),
        a,
        b,
        shadowed,
    }
}

/// Each sheet's mip chain, once it has loaded (as `textures::mip_models`
/// gives the models theirs): a 2048² sheet with one level shimmers on a
/// puff far away and costs bandwidth it needn't.
pub fn mip_sheets(
    mut events: MessageReader<AssetEvent<Image>>,
    mut images: ResMut<Assets<Image>>,
    assets: Res<AssetServer>,
) {
    for ev in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = ev else {
            continue;
        };
        if !assets.get_path(*id).is_some_and(|p| p.path().starts_with("fx")) {
            continue;
        }
        let Some(img) = images.get_mut(*id) else {
            continue;
        };
        crate::look::textures::mip_in_place(img, false);
        img.sampler = ImageSampler::Descriptor(sampler());
    }
}

/// An empty mesh for a flipbook pool of `cap` quads: a position, the
/// sprite's right axis (as its normal), both frames' uvs, the aux stream
/// (as its tangent: blend, age, tone, glow) and the tint.
pub fn sheet_mesh(cap: usize) -> Mesh {
    let n = cap * 4;
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[1.0f32, 0.0, 0.0]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.0f32; 2]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[0.0f32; 4]; n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; n]);
    mesh.insert_indices(Indices::U32(::fx::pool::indices(cap)));
    mesh
}

/// Copy a flipbook pool into the mesh [`sheet_mesh`] made, in place,
/// relative to `anchor`. Returns whether it wrote.
pub fn write(pool: &mut Pool, mesh: &mut Mesh, cam: &Cam, anchor: Vec3) -> bool {
    if !pool.needs_write() {
        return false;
    }
    let (mut pos, mut nrm, mut uv, mut uv2, mut aux, mut col) = (None, None, None, None, None, None);
    for (attr, values) in mesh.attributes_mut() {
        let id = attr.id;
        match values {
            VertexAttributeValues::Float32x3(v) => {
                if id == Mesh::ATTRIBUTE_POSITION.id {
                    pos = Some(v);
                } else if id == Mesh::ATTRIBUTE_NORMAL.id {
                    nrm = Some(v);
                }
            }
            VertexAttributeValues::Float32x2(v) => {
                if id == Mesh::ATTRIBUTE_UV_0.id {
                    uv = Some(v);
                } else if id == Mesh::ATTRIBUTE_UV_1.id {
                    uv2 = Some(v);
                }
            }
            VertexAttributeValues::Float32x4(v) => {
                if id == Mesh::ATTRIBUTE_TANGENT.id {
                    aux = Some(v);
                } else if id == Mesh::ATTRIBUTE_COLOR.id {
                    col = Some(v);
                }
            }
            _ => {}
        }
    }
    let (Some(pos), Some(nrm), Some(uv), Some(uv2), Some(aux), Some(col)) = (pos, nrm, uv, uv2, aux, col) else {
        return false;
    };
    pool.fill(
        Verts {
            pos,
            uv,
            col,
            uv2: Some(uv2),
            nrm: Some(nrm),
            aux: Some(aux),
        },
        cam,
        anchor.into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The strips tile each sheet exactly, in the baker's order.
    #[test]
    fn the_strips_fill_their_sheets() {
        let mut cell = 0u32;
        for s in [SMOKE, DUST, FIRE, PLASMA, FLASH, BLOOM, WISP] {
            assert_eq!(s.cell as u32, cell);
            cell += s.variants as u32 * s.frames as u32;
        }
        assert_eq!(cell, CLOUD.cols * CLOUD.rows);
        assert_eq!(
            EXPLOSION.variants as u32 * EXPLOSION.frames as u32,
            BOOM.cols * BOOM.rows
        );
    }
}

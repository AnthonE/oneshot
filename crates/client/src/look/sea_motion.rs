//! What moves the seas every frame, the water's (`water.rs`: its waves'
//! phases, its ripples' drift) and the cloud's (`cloud_sea.rs`: its drift),
//! in one tiny texture both materials bind once and read with
//! `textureLoad`, written in place each frame as the foliage's state is
//! (`foliage.rs`, `write_textures`). Written into the materials instead,
//! each frame would build a sea's bind group again and specialise it anew.
//!
//! Texel 0: the water's four phases, cycles. Texel 1: xy the ripples'
//! drift (tiles), zw the cloud's (m), each already wrapped.

use bevy::asset::{uuid_handle, RenderAssetUsages};
use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{Extent3d, TexelCopyBufferLayout, TextureDimension, TextureFormat};
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy::render::{Render, RenderApp, RenderSystems};

/// The texture, made as the app is built: a sea's material names it.
pub const MOTION: Handle<Image> = uuid_handle!("6d0c2a7e-91f3-4b58-a6e2-5c8d4f1b3e07");

const TEXELS: usize = 2;
const FLOATS: usize = TEXELS * 4;

/// This frame's texels, handed to the render world.
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct SeaMotion {
    /// The water's waves' phases, cycles.
    pub phase: [f32; 4],
    /// The ripples' drift, tiles.
    pub ripples: Vec2,
    /// The cloud's drift, m.
    pub cloud: Vec2,
}

pub fn plugin(app: &mut App) {
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .insert(
            MOTION.id(),
            Image::new_fill(
                Extent3d {
                    width: TEXELS as u32,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                &[0u8; 16],
                TextureFormat::Rgba32Float,
                RenderAssetUsages::RENDER_WORLD,
            ),
        )
        .expect("the seas' motion could not be registered");
    app.init_resource::<SeaMotion>();
    app.add_plugins(ExtractResourcePlugin::<SeaMotion>::default());
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app.add_systems(Render, write.in_set(RenderSystems::PrepareResources));
    }
}

/// Into the texture the materials already bind: a new `Image` would be a
/// new texture, and every sea's bind group would go on reading the old.
fn write(motion: Option<Res<SeaMotion>>, images: Res<RenderAssets<GpuImage>>, queue: Res<RenderQueue>) {
    let (Some(m), Some(img)) = (motion, images.get(&MOTION)) else {
        return;
    };
    let px: [f32; FLOATS] = [
        m.phase[0],
        m.phase[1],
        m.phase[2],
        m.phase[3],
        m.ripples.x,
        m.ripples.y,
        m.cloud.x,
        m.cloud.y,
    ];
    let mut bytes = [0u8; FLOATS * 4];
    for (b, f) in bytes.chunks_exact_mut(4).zip(px) {
        b.copy_from_slice(&f.to_le_bytes());
    }
    queue.write_texture(
        img.texture.as_image_copy(),
        &bytes,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some((FLOATS * 4) as u32),
            rows_per_image: None,
        },
        Extent3d {
            width: TEXELS as u32,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
}

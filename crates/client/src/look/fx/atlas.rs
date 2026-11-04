//! The particle sprites: one 256² sheet generated at startup (white, the
//! shape in the alpha), so every sprite takes its particle's colour.
//!
//! The shapes are the shared `fx` crate's (vendored from Gates, MIT, © 2026
//! MoreRight DAO); this is our layout of them ([`CELLS`]): Gates' droplet and
//! leaf cells are ours here, an electric arc (a shield taking a round) and a
//! crystal shard (the SHARD GUN's).

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use ::fx::atlas::Shape;
pub use ::fx::atlas::{fbm, hash01, smooth, vnoise, SPRITE_COLS, SPRITE_ROWS, SPRITE_TEX};

/// The sprites, by cell.
pub const GLOW: u8 = 0;
pub const STREAK: u8 = 1;
/// Three star flashes, cells `STAR..STAR + 3`.
pub const STAR: u8 = 2;
/// Three puffs, cells `PUFF..PUFF + 3`.
pub const PUFF: u8 = 5;
/// Two smoke wisps.
pub const SMOKE: u8 = 8;
/// A jagged bolt of lightning along the sprite's length.
pub const ARC: u8 = 10;
pub const RING: u8 = 11;
pub const SPECK: u8 = 12;
/// A crystal sliver with a bright spine.
pub const SHARD: u8 = 13;
pub const SPLINTER: u8 = 14;
/// A flame tongue, tip up: drawn unrolled, or the fire leans.
pub const FLAME: u8 = 15;

/// Our sheet: Gates' with its droplet and leaf cells swapped for an arc and
/// a shard (the shapes are the shared `fx` crate's).
pub const CELLS: [Shape; ::fx::atlas::CELLS] = [
    Shape::Glow,
    Shape::Streak,
    Shape::Star(0),
    Shape::Star(1),
    Shape::Star(2),
    Shape::Puff,
    Shape::Puff,
    Shape::Puff,
    Shape::Smoke,
    Shape::Smoke,
    Shape::Arc,
    Shape::Ring,
    Shape::Speck,
    Shape::Shard,
    Shape::Splinter,
    Shape::Flame,
];

/// The sheet's level 0: RGBA8 sRGB.
pub fn sprite_pixels() -> (Vec<u8>, u32, u32) {
    ::fx::atlas::sheet(&CELLS)
}

/// The sheet with its mip chain.
pub fn sprite_image() -> Image {
    let (level0, w, h) = sprite_pixels();
    let mut img = Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        level0,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    crate::look::textures::mip_in_place(&mut img, false);
    img.sampler = ImageSampler::linear();
    img
}

/// The uv rectangle of `cell` (min, max), inset half a texel.
pub fn cell_uv(cell: u8) -> (Vec2, Vec2) {
    let (lo, hi) = ::fx::atlas::cell_uv(cell);
    (Vec2::from(lo), Vec2::from(hi))
}

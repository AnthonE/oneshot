//! The texture sets the level is dressed in (CC0, from Gates:
//! `tools/assets_client.mjs`), with their mip chains built here on the CPU
//! as Gates' `render/mipmap.rs` does: Bevy uploads a JPG with one level,
//! and a tiled ground with one level shimmers. Albedo is filtered in linear
//! light (sRGB-correct); normals are averaged and renormalised.
//!
//! The ground's four sets go into two texture arrays (albedo, normal), as
//! Gates' `stack_ground` packs them: WebGL2 allows 16 textures a draw, and
//! a splat of four sets in separate textures beside the PBR's own would
//! pass it.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};

/// The ground's sets in splat order, and how many metres each repeats
/// over (Gates' `GROUND_TILE_M`: each drawn at its photographed size).
pub const GROUND: [&str; 4] = ["sand", "grass", "gravel", "rock"];
pub const GROUND_TILE_M: [f32; 4] = [2.14, 2.0, 1.6, 1.8];
/// The boxes' sets.
pub const BOXES: [&str; 3] = ["concrete", "metal", "rock"];

#[derive(Resource, Default)]
pub struct Textures {
    loading: Vec<(&'static str, Handle<Image>, Handle<Image>)>,
    /// The boxes' sets, mipped, by name.
    pub sets: Vec<(&'static str, Handle<Image>, Handle<Image>)>,
    /// The ground's arrays, once built.
    pub ground: Option<(Handle<Image>, Handle<Image>)>,
    /// The grass cards' atlas (a cutout), once mipped.
    pub grass: Option<Handle<Image>>,
    grass_loading: Option<Handle<Image>>,
}

impl Textures {
    pub fn ready(&self) -> bool {
        self.ground.is_some()
    }

    pub fn set(&self, name: &str) -> Option<(Handle<Image>, Handle<Image>)> {
        self.sets
            .iter()
            .find(|s| s.0 == name)
            .map(|s| (s.1.clone(), s.2.clone()))
    }
}

fn repeat() -> ImageSamplerDescriptor {
    ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: if cfg!(target_arch = "wasm32") { 4 } else { 16 },
        ..default()
    }
}

/// Starts every set loading.
pub fn load(mut t: ResMut<Textures>, assets: Res<AssetServer>) {
    let mut names: Vec<&'static str> = GROUND.to_vec();
    for b in BOXES {
        if !names.contains(&b) {
            names.push(b);
        }
    }
    for n in names {
        let albedo = assets.load_with_settings(format!("textures/{n}_albedo.jpg"), |s: &mut ImageLoaderSettings| {
            s.is_srgb = true;
            s.sampler = ImageSampler::Descriptor(repeat());
        });
        let normal = assets.load_with_settings(format!("textures/{n}_normal.jpg"), |s: &mut ImageLoaderSettings| {
            s.is_srgb = false;
            s.sampler = ImageSampler::Descriptor(repeat());
        });
        t.loading.push((n, albedo, normal));
    }
    t.grass_loading = Some(assets.load_with_settings(
        "textures/grass_card_albedo.png",
        |s: &mut ImageLoaderSettings| {
            s.is_srgb = true;
            s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::ClampToEdge,
                address_mode_v: ImageAddressMode::ClampToEdge,
                ..repeat()
            });
        },
    ));
}

/// Once every set has loaded: their mips, and the ground's arrays.
pub fn prepare(mut t: ResMut<Textures>, mut images: ResMut<Assets<Image>>) {
    if t.ready() || t.loading.is_empty() {
        return;
    }
    let grass_in = t.grass_loading.as_ref().is_none_or(|g| images.contains(g));
    if !grass_in
        || !t
            .loading
            .iter()
            .all(|(_, a, n)| images.contains(a) && images.contains(n))
    {
        return;
    }
    if let Some(g) = t.grass_loading.take() {
        if let Some(img) = images.get_mut(&g) {
            mip_cutout_in_place(img);
        }
        t.grass = Some(g);
    }
    let loading = std::mem::take(&mut t.loading);
    for (_, a, n) in &loading {
        for (h, normal) in [(a, false), (n, true)] {
            if let Some(img) = images.get_mut(h) {
                mip_in_place(img, normal);
            }
        }
    }
    let layers = |normal: bool| -> Vec<Image> {
        GROUND
            .iter()
            .filter_map(|g| loading.iter().find(|s| s.0 == *g))
            .filter_map(|s| images.get(if normal { &s.2 } else { &s.1 }).cloned())
            .collect()
    };
    let (alb, nor) = (layers(false), layers(true));
    if let (Some(a), Some(n)) = (array(&alb), array(&nor)) {
        t.ground = Some((images.add(a), images.add(n)));
    }
    t.sets = loading;
}

fn srgb_to_linear(v: u8) -> f32 {
    super::fill::srgb_to_linear(v as f32 / 255.0)
}

fn linear_to_srgb(v: f32) -> u8 {
    (super::fill::linear_to_srgb(v.clamp(0.0, 1.0)) * 255.0).round() as u8
}

/// One level down, box-filtered: in linear light for colour, renormalised
/// for a normal map.
fn halve(src: &[u8], w: usize, h: usize, srgb: bool, normal: bool) -> Vec<u8> {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        for x in 0..nw {
            let mut acc = [0f32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (sx, sy) = ((2 * x + dx).min(w - 1), (2 * y + dy).min(h - 1));
                let i = (sy * w + sx) * 4;
                for c in 0..4 {
                    let v = src[i + c];
                    acc[c] += if srgb && c < 3 {
                        srgb_to_linear(v)
                    } else {
                        v as f32 / 255.0
                    };
                }
            }
            let o = (y * nw + x) * 4;
            if normal {
                let v = Vec3::new(
                    acc[0] / 4.0 * 2.0 - 1.0,
                    acc[1] / 4.0 * 2.0 - 1.0,
                    acc[2] / 4.0 * 2.0 - 1.0,
                )
                .normalize_or(Vec3::Z);
                for (c, n) in [v.x, v.y, v.z].into_iter().enumerate() {
                    out[o + c] = ((n * 0.5 + 0.5) * 255.0).round() as u8;
                }
                out[o + 3] = 255;
            } else {
                for c in 0..4 {
                    let m = acc[c] / 4.0;
                    out[o + c] = if srgb && c < 3 {
                        linear_to_srgb(m)
                    } else {
                        (m * 255.0).round() as u8
                    };
                }
            }
        }
    }
    out
}

/// The full chain from an RGBA8 level 0: the levels one after another.
fn chain(level0: &[u8], w: usize, h: usize, srgb: bool, normal: bool) -> (Vec<u8>, u32) {
    let mut out = level0.to_vec();
    let (mut cur, mut cw, mut ch, mut n) = (level0.to_vec(), w, h, 1u32);
    while cw > 1 || ch > 1 {
        cur = halve(&cur, cw, ch, srgb, normal);
        cw = (cw / 2).max(1);
        ch = (ch / 2).max(1);
        out.extend_from_slice(&cur);
        n += 1;
    }
    (out, n)
}

fn rgba8(img: &Image) -> Option<(Vec<u8>, usize, usize, bool)> {
    let size = img.texture_descriptor.size;
    let srgb = match img.texture_descriptor.format {
        TextureFormat::Rgba8UnormSrgb => true,
        TextureFormat::Rgba8Unorm => false,
        _ => return None,
    };
    let (w, h) = (size.width as usize, size.height as usize);
    let data = img.data.as_ref()?;
    (data.len() >= w * h * 4).then(|| (data[..w * h * 4].to_vec(), w, h, srgb))
}

/// The models' maps (`models/*.glb`, a JPEG level each): their chains as
/// the level's get theirs, filtered between levels. A linear map (normal,
/// roughness) is averaged as data.
pub fn mip_models(
    mut events: MessageReader<AssetEvent<Image>>,
    mut images: ResMut<Assets<Image>>,
    assets: Res<AssetServer>,
) {
    for ev in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = ev else {
            continue;
        };
        if !assets.get_path(*id).is_some_and(|p| p.path().starts_with("models")) {
            continue;
        }
        let Some(img) = images.get_mut(*id) else {
            continue;
        };
        mip_in_place(img, false);
        let mut d = match &img.sampler {
            ImageSampler::Descriptor(d) => d.clone(),
            ImageSampler::Default => ImageSamplerDescriptor::linear(),
        };
        d.mipmap_filter = ImageFilterMode::Linear;
        d.min_filter = ImageFilterMode::Linear;
        d.mag_filter = ImageFilterMode::Linear;
        img.sampler = ImageSampler::Descriptor(d);
    }
}

/// Gives a loaded RGBA8 image its whole mip chain.
fn mip_in_place(img: &mut Image, normal: bool) {
    if img.texture_descriptor.mip_level_count > 1 {
        return;
    }
    let Some((data, w, h, srgb)) = rgba8(img) else {
        return;
    };
    let (all, n) = chain(&data, w, h, srgb, normal);
    img.data = Some(all);
    img.texture_descriptor.mip_level_count = n;
}

/// The fraction of texels a cutout keeps at the 0.5 test.
fn coverage(level: &[u8]) -> f32 {
    let n = level.len() / 4;
    level.chunks(4).filter(|p| p[3] >= 128).count() as f32 / n.max(1) as f32
}

/// A cutout's chain, each level's alpha scaled so it keeps level 0's
/// coverage at the 0.5 test (Gates' `mipmap.rs`): averaged alone, a tuft
/// thins to nothing a few levels down.
fn mip_cutout_in_place(img: &mut Image) {
    if img.texture_descriptor.mip_level_count > 1 {
        return;
    }
    let Some((data, w, h, srgb)) = rgba8(img) else {
        return;
    };
    let want = coverage(&data);
    let mut out = data.clone();
    let (mut cur, mut cw, mut ch, mut n) = (data, w, h, 1u32);
    while cw > 1 || ch > 1 {
        cur = halve(&cur, cw, ch, srgb, false);
        cw = (cw / 2).max(1);
        ch = (ch / 2).max(1);
        // The scale that brings coverage back, by bisection.
        let (mut lo, mut hi) = (0.25f32, 8.0f32);
        for _ in 0..16 {
            let k = (lo + hi) * 0.5;
            let c = cur.chunks(4).filter(|p| (p[3] as f32 * k) >= 128.0).count() as f32 / (cw * ch) as f32;
            if c < want {
                lo = k;
            } else {
                hi = k;
            }
        }
        let k = (lo + hi) * 0.5;
        for p in cur.chunks_mut(4) {
            p[3] = (p[3] as f32 * k).min(255.0) as u8;
        }
        out.extend_from_slice(&cur);
        n += 1;
    }
    img.data = Some(out);
    img.texture_descriptor.mip_level_count = n;
}

/// Same-sized RGBA8 images (each already mipped) as one 2D array.
fn array(layers: &[Image]) -> Option<Image> {
    let first = layers.first()?;
    let d = &first.texture_descriptor;
    if layers
        .iter()
        .any(|l| l.texture_descriptor.size != d.size || l.texture_descriptor.format != d.format)
    {
        return None;
    }
    let mut data = Vec::new();
    for l in layers {
        data.extend_from_slice(l.data.as_ref()?);
    }
    let mut img = Image::new(
        Extent3d {
            width: d.size.width,
            height: d.size.height,
            depth_or_array_layers: layers.len() as u32,
        },
        TextureDimension::D2,
        data,
        d.format,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = d.mip_level_count;
    img.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });
    img.sampler = ImageSampler::Descriptor(repeat());
    Some(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_runs_to_one_texel_and_keeps_its_light() {
        // A 4 × 4 of half-and-half black and white averages to linear mid
        // grey, not sRGB 128.
        let mut l0 = vec![0u8; 4 * 4 * 4];
        for (i, px) in l0.chunks_mut(4).enumerate() {
            let v = if i % 2 == 0 { 255 } else { 0 };
            px.copy_from_slice(&[v, v, v, 255]);
        }
        let (all, n) = chain(&l0, 4, 4, true, false);
        assert_eq!(n, 3);
        assert_eq!(all.len(), (16 + 4 + 1) * 4);
        let last = &all[all.len() - 4..];
        assert!((last[0] as i32 - 188).abs() <= 1, "linear 0.5 is sRGB 188: {last:?}");
    }
}

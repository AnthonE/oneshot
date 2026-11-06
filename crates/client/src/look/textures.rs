//! The texture sets the level is dressed in (CC0, from Gates:
//! `tools/assets.mjs`), with their mip chains built here on the CPU
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
/// over (Gates' `GROUND_TILE_M`): the soils at the size they were
/// photographed (`tools/ground.sh`), the rock at five times its own, so a
/// bed of its strata is a metre thick and reads from the canyon floor
/// (a centimetre a texel, still finer than anyone stands to a cliff);
/// at its own 1.8 m a 45 m wall was one flat value, and at 4 m a striped
/// cloth.
pub const GROUND: [&str; 4] = ["sand", "grass", "gravel", "rock"];
pub const GROUND_TILE_M: [f32; 4] = [2.48, 2.51, 2.0, 9.6];
/// The boxes' sets: a base's metal and dressed stone the last three
/// (`tools/ground.sh`).
pub const BOXES: [&str; 6] = ["concrete", "metal", "rock", "steel", "tread", "stone"];

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
    /// The boxes' roughness and occlusion photographs, loading.
    orm_loading: Vec<(&'static str, Handle<Image>, Handle<Image>)>,
    /// Each box set's occlusion, roughness and metal in one map, glTF's
    /// packing (R occlusion, G roughness, B metal, full: the material's
    /// numbers scale it), as Bevy's two slots read it.
    pub orm: Vec<(&'static str, Handle<Image>)>,
    /// Sets that glow in places: the glow's mask, by set.
    pub emissive: Vec<(&'static str, Handle<Image>)>,
}

impl Textures {
    pub fn ready(&self) -> bool {
        self.ground.is_some()
    }

    /// A set's glow mask, when it glows.
    pub fn emissive(&self, name: &str) -> Option<Handle<Image>> {
        self.emissive.iter().find(|s| s.0 == name).map(|s| s.1.clone())
    }

    /// A box set's packed occlusion-roughness-metal map, when it has one.
    pub fn orm(&self, name: &str) -> Option<Handle<Image>> {
        self.orm.iter().find(|s| s.0 == name).map(|s| s.1.clone())
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
        // Gates' 4 for WebGL2's integrated GPUs; the desktop's 16 wherever
        // the desktop's frame is drawn (the WebGPU module too).
        anisotropy_clamp: if cfg!(webgl2) { 4 } else { 16 },
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
    for n in BOXES {
        let linear = |s: &mut ImageLoaderSettings| {
            s.is_srgb = false;
            s.sampler = ImageSampler::Descriptor(repeat());
        };
        let rough = assets.load_with_settings(format!("textures/{n}_rough.jpg"), linear);
        let ao = assets.load_with_settings(format!("textures/{n}_ao.jpg"), linear);
        t.orm_loading.push((n, rough, ao));
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
    let orm_in = t
        .orm_loading
        .iter()
        .all(|(_, r, a)| images.contains(r) && images.contains(a));
    if !grass_in
        || !orm_in
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
    for (n, r, a) in std::mem::take(&mut t.orm_loading) {
        let packed = match (images.get(&r).and_then(channel0), images.get(&a).and_then(channel0)) {
            (Some(r), Some(a)) => pack_orm(&r, &a),
            _ => None,
        };
        if let Some(img) = packed {
            t.orm.push((n, images.add(img)));
        }
        images.remove(&r);
        images.remove(&a);
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
    // The plating: drawn here, not photographed.
    let (albedo, normal, orm) = panel_images(PANEL_PX);
    let (a, n, o) = (images.add(albedo), images.add(normal), images.add(orm));
    t.sets.push(("panel", a, n));
    t.orm.push(("panel", o));
    // The ship's surfaces (`ship.rs`): the hull's plates, the deck's
    // grating, the windows' honeycomb.
    for (name, [albedo, normal, orm, glow], glows) in [
        ("alien", super::ship::hull_images(super::ship::HULL_PX), true),
        ("deck", super::ship::deck_images(PANEL_PX), false),
        ("gold", super::ship::gold_images(PANEL_PX), true),
    ] {
        let (a, n, o) = (images.add(albedo), images.add(normal), images.add(orm));
        t.sets.push((name, a, n));
        t.orm.push((name, o));
        if glows {
            t.emissive.push((name, images.add(glow)));
        }
    }
}

/// What one texel of a baked set is, besides its height: its colour
/// (linear), occlusion, roughness, metal and glow.
pub(crate) struct Texel {
    pub albedo: [f32; 3],
    pub ao: f32,
    pub rough: f32,
    pub metal: f32,
    pub glow: f32,
}

/// A procedural set over a seamless n × n tile: `height` (0..1, for the
/// normal map, its slope scaled by `k`) and `texel` (given the height)
/// baked into albedo (sRGB), normal, occlusion-roughness-metal and the glow
/// mask (sRGB), each with its mip chain.
pub(crate) fn bake(
    n: usize,
    k: f32,
    height: &dyn Fn(usize, usize) -> f32,
    texel: &dyn Fn(usize, usize, f32) -> Texel,
) -> [Image; 4] {
    let hs: Vec<f32> = (0..n * n).map(|i| height(i % n, i / n)).collect();
    let h = |x: isize, y: isize| hs[(y.rem_euclid(n as isize) as usize) * n + x.rem_euclid(n as isize) as usize];
    let (mut albedo, mut normal, mut orm, mut glow) = (
        vec![0u8; n * n * 4],
        vec![0u8; n * n * 4],
        vec![0u8; n * n * 4],
        vec![0u8; n * n * 4],
    );
    for y in 0..n {
        for x in 0..n {
            let i = (y * n + x) * 4;
            let hv = hs[y * n + x];
            let (xi, yi) = (x as isize, y as isize);
            let nv = Vec3::new(
                (h(xi - 1, yi) - h(xi + 1, yi)) * k,
                (h(xi, yi - 1) - h(xi, yi + 1)) * k,
                1.0,
            )
            .normalize();
            for (c, v) in [nv.x, nv.y, nv.z].into_iter().enumerate() {
                normal[i + c] = ((v * 0.5 + 0.5) * 255.0).round() as u8;
            }
            normal[i + 3] = 255;
            let t = texel(x, y, hv);
            for (c, v) in t.albedo.into_iter().enumerate() {
                albedo[i + c] = linear_to_srgb(v);
            }
            albedo[i + 3] = 255;
            orm[i] = (t.ao.clamp(0.0, 1.0) * 255.0).round() as u8;
            orm[i + 1] = (t.rough.clamp(0.0, 1.0) * 255.0).round() as u8;
            orm[i + 2] = (t.metal.clamp(0.0, 1.0) * 255.0).round() as u8;
            orm[i + 3] = 255;
            let g = linear_to_srgb(t.glow);
            glow[i..i + 3].copy_from_slice(&[g, g, g]);
            glow[i + 3] = 255;
        }
    }
    [
        baked_image(albedo, n, true, false),
        baked_image(normal, n, false, true),
        baked_image(orm, n, false, false),
        baked_image(glow, n, true, false),
    ]
}

/// A square RGBA8 level 0 as a repeating, mipped image.
pub(crate) fn baked_image(data: Vec<u8>, n: usize, srgb: bool, normal: bool) -> Image {
    let (all, levels) = chain(&data, n, n, srgb, normal);
    let mut img = mipped(
        Extent3d {
            width: n as u32,
            height: n as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        all,
        if srgb {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = levels;
    img.sampler = ImageSampler::Descriptor(repeat());
    img
}

/// A hash in 0..1 of a lattice point and a key.
pub(crate) fn hash2(x: usize, y: usize, k: u32) -> f32 {
    let mut h =
        (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ k.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0
}

/// The plating's texels a side; one tile is [`PANEL_PLATES`] plates square.
pub const PANEL_PX: usize = 512;
pub const PANEL_PLATES: usize = 2;

/// Hull plating, ours and procedural: plates with a groove between them, a
/// bevelled inset panel in each and a few bolts, the plates each a little
/// lighter or darker and faintly mottled. Near-white (the look's tint is
/// the colour). Albedo (sRGB), normal and occlusion-roughness-metal, each
/// with its mip chain.
pub fn panel_images(n: usize) -> (Image, Image, Image) {
    let plate = n / PANEL_PLATES;
    let px = |v: f32| v * plate as f32;
    // The height at a texel: 0 in a seam, up to 1 on a plate's face, the
    // inset's lip a step down.
    let height = |x: usize, y: usize| -> f32 {
        let (u, v) = ((x % plate) as f32, (y % plate) as f32);
        let p = plate as f32;
        let edge = u.min(v).min(p - 1.0 - u).min(p - 1.0 - v);
        let seam = ((edge - px(0.006)) / px(0.012)).clamp(0.0, 1.0);
        let inset = {
            let d = edge - px(0.12);
            // A groove a few texels wide at the inset's border.
            1.0 - 0.35 * (1.0 - (d.abs() / px(0.008)).min(1.0))
        };
        // Bolts in the plate's corners, just inside the seam.
        let b = px(0.055);
        let mut bolt = 0.0f32;
        for (cx, cy) in [(b, b), (p - b, b), (b, p - b), (p - b, p - b)] {
            let r = ((u - cx).powi(2) + (v - cy).powi(2)).sqrt();
            bolt = bolt.max((1.0 - r / px(0.018)).clamp(0.0, 1.0));
        }
        seam * inset + 0.25 * bolt
    };
    let noise = hash2;
    // Soft mottling: noise on a coarse lattice, bilinear.
    let mottle = |x: usize, y: usize| -> f32 {
        let cell = 32usize;
        let (gx, gy) = (x / cell, y / cell);
        let (fx, fy) = ((x % cell) as f32 / cell as f32, (y % cell) as f32 / cell as f32);
        let g = n / cell;
        let at = |i: usize, j: usize| noise(i % g, j % g, 7);
        let top = at(gx, gy) * (1.0 - fx) + at(gx + 1, gy) * fx;
        let bot = at(gx, gy + 1) * (1.0 - fx) + at(gx + 1, gy + 1) * fx;
        top * (1.0 - fy) + bot * fy
    };
    let hs: Vec<f32> = (0..n * n).map(|i| height(i % n, i / n)).collect();
    let h = |x: isize, y: isize| hs[(y.rem_euclid(n as isize) as usize) * n + x.rem_euclid(n as isize) as usize];
    let (mut albedo, mut normal, mut orm) = (vec![0u8; n * n * 4], vec![0u8; n * n * 4], vec![0u8; n * n * 4]);
    for y in 0..n {
        for x in 0..n {
            let i = (y * n + x) * 4;
            let hv = hs[y * n + x];
            let (xi, yi) = (x as isize, y as isize);
            // The groove is a couple of millimetres deep over a plate's
            // metre: steep enough to catch the light.
            let k = 3.0;
            let nx = (h(xi - 1, yi) - h(xi + 1, yi)) * k;
            let ny = (h(xi, yi - 1) - h(xi, yi + 1)) * k;
            let nv = Vec3::new(nx, ny, 1.0).normalize();
            for (c, v) in [nv.x, nv.y, nv.z].into_iter().enumerate() {
                normal[i + c] = ((v * 0.5 + 0.5) * 255.0).round() as u8;
            }
            normal[i + 3] = 255;
            let plate_shade = 0.9 + 0.1 * noise(x / plate, y / plate, 3);
            let grain = 0.94 + 0.06 * mottle(x, y) + 0.02 * (noise(x, y, 5) - 0.5);
            let seam_dark = 0.45 + 0.55 * hv.min(1.0);
            let lin = (0.82 * plate_shade * grain * seam_dark).clamp(0.0, 1.0);
            let sv = linear_to_srgb(lin);
            albedo[i..i + 3].copy_from_slice(&[sv, sv, sv]);
            albedo[i + 3] = 255;
            let ao = 0.55 + 0.45 * hv.min(1.0);
            let rough = 0.55 + 0.25 * (1.0 - hv.min(1.0)) + 0.1 * (mottle(x, y) - 0.5);
            orm[i] = (ao * 255.0).round() as u8;
            orm[i + 1] = (rough.clamp(0.0, 1.0) * 255.0).round() as u8;
            orm[i + 2] = 255;
            orm[i + 3] = 255;
        }
    }
    let image = |data: Vec<u8>, srgb: bool, normal: bool| {
        let (all, levels) = chain(&data, n, n, srgb, normal);
        let mut img = mipped(
            Extent3d {
                width: n as u32,
                height: n as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            all,
            if srgb {
                TextureFormat::Rgba8UnormSrgb
            } else {
                TextureFormat::Rgba8Unorm
            },
            RenderAssetUsages::RENDER_WORLD,
        );
        img.texture_descriptor.mip_level_count = levels;
        img.sampler = ImageSampler::Descriptor(repeat());
        img
    };
    (
        image(albedo, true, false),
        image(normal, false, true),
        image(orm, false, false),
    )
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

/// A loaded 8-bit image's first channel and its size: a greyscale map
/// whichever way the loader stored it (one, two or four bytes a texel).
fn channel0(img: &Image) -> Option<(Vec<u8>, usize, usize)> {
    let size = img.texture_descriptor.size;
    let (w, h) = (size.width as usize, size.height as usize);
    let data = img.data.as_ref()?;
    let bpp = match img.texture_descriptor.format {
        TextureFormat::R8Unorm => 1,
        TextureFormat::Rg8Unorm => 2,
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => 4,
        _ => return None,
    };
    (data.len() >= w * h * bpp).then(|| ((0..w * h).map(|i| data[i * bpp]).collect(), w, h))
}

/// Roughness and occlusion (same size) as one mipped linear RGBA8 map:
/// R occlusion, G roughness, B and A full.
fn pack_orm(rough: &(Vec<u8>, usize, usize), ao: &(Vec<u8>, usize, usize)) -> Option<Image> {
    let ((r, w, h), (a, aw, ah)) = (rough, ao);
    if (w, h) != (aw, ah) {
        return None;
    }
    let mut l0 = vec![255u8; w * h * 4];
    for (i, px) in l0.chunks_mut(4).enumerate() {
        px[0] = a[i];
        px[1] = r[i];
    }
    let (all, n) = chain(&l0, *w, *h, false, false);
    let mut img = mipped(
        Extent3d {
            width: *w as u32,
            height: *h as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        all,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = n;
    img.sampler = ImageSampler::Descriptor(repeat());
    Some(img)
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
pub(crate) fn mip_in_place(img: &mut Image, normal: bool) {
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
    let mut img = mipped(
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

/// An image whose data is its whole mip chain. `Image::new` checks the
/// data against the first level only (in a debug build, where a chain
/// panicked it): made from that, then given the chain.
fn mipped(
    size: Extent3d,
    dim: TextureDimension,
    data: Vec<u8>,
    format: TextureFormat,
    usage: RenderAssetUsages,
) -> Image {
    use bevy::image::TextureFormatPixelInfo;
    let texels = size.width as usize * size.height as usize * size.depth_or_array_layers as usize;
    let first = (texels * format.pixel_size().unwrap_or(4)).min(data.len());
    let mut img = Image::new(size, dim, data[..first].to_vec(), format, usage);
    img.data = Some(data);
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ships_surfaces_tile_and_glow_only_in_their_lights() {
        // Each set, and how much of it may glow: the hull's seams a
        // little, the windows' glass most of them, the deck nothing.
        for (name, [a, _, _, g], most) in [
            ("hull", crate::look::ship::hull_images(64), 0.25),
            ("deck", crate::look::ship::deck_images(64), 0.0),
            ("gold", crate::look::ship::gold_images(64), 0.9),
        ] {
            assert_eq!(a.texture_descriptor.mip_level_count, 7, "{name}");
            let g = g.data.unwrap();
            let lit = (0..64 * 64).filter(|i| g[i * 4] > 128).count() as f32 / (64.0 * 64.0);
            if most > 0.0 {
                assert!(lit > 0.0 && lit < most, "{name}: {lit} of it glows");
            } else {
                assert_eq!(lit, 0.0, "{name} glows");
            }
        }
    }

    #[test]
    fn the_plating_tiles_and_has_its_seams() {
        let (a, n, o) = panel_images(64);
        for img in [&a, &n, &o] {
            assert_eq!(img.texture_descriptor.mip_level_count, 7);
        }
        let a = a.data.unwrap();
        // A seam (the tile's edge) is darker than a plate's middle.
        let at = |x: usize, y: usize| a[(y * 64 + x) * 4];
        assert!(at(0, 16) < at(16, 16), "{} {}", at(0, 16), at(16, 16));
    }

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

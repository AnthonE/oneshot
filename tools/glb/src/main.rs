//! `cargo run --release --manifest-path tools/glb/Cargo.toml -- <in> <out>`:
//! copies every `.glb` in `<in>` to `<out>` with its KTX2/UASTC textures
//! re-embedded as JPEG (PNG where a map has alpha), so the Bevy client
//! loads them on every target with no transcoder (Bevy's is C++, which
//! wasm can't build). The textures then name their image as a plain
//! `source`, and `KHR_texture_basisu` leaves the document.
//!
//! Ported from AnthonE/Gates `crates/client/src/webassets.rs`
//! (MIT, © 2026 MoreRight DAO; NOTICE has the licence), whose header has
//! the reasoning: JPEG 4:4:4 at q90 (q95 for normal maps), PNG only where
//! some texel's alpha is below 255, every other buffer view byte for byte.

use std::io::Read;

/// The mip level of the packed chain the variant is cut at. Gates cuts at 1;
/// our models are few (Triple T at 1024², the guns at 512²) and the guns are
/// seen at arm's length, so we keep level 0.
pub const WEB_LEVEL: u32 = 0;

/// JPEG quality for an opaque map that is not a normal map.
pub const JPEG_QUALITY: u8 = 90;
/// JPEG quality for a normal map: its error bends the lighting.
pub const JPEG_QUALITY_NORMAL: u8 = 95;

/// Why a file could not be converted.
#[derive(Debug)]
pub enum Error {
    /// Not a GLB, or a GLB this reader does not understand.
    Malformed(&'static str),
    /// The document is not the JSON it claims to be.
    Json(serde_json::Error),
    /// A KTX2 image could not be decoded.
    Decode(String),
    /// The PNG encoder refused.
    Png(String),
    /// The JPEG encoder refused.
    Jpeg(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Malformed(why) => write!(f, "malformed GLB: {why}"),
            Error::Json(e) => write!(f, "GLB JSON: {e}"),
            Error::Decode(why) => write!(f, "KTX2 decode: {why}"),
            Error::Png(why) => write!(f, "PNG encode: {why}"),
            Error::Jpeg(why) => write!(f, "JPEG encode: {why}"),
        }
    }
}

const MAGIC: u32 = 0x4654_6C67; // "glTF"
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

fn u32_at(d: &[u8], at: usize) -> Result<u32, Error> {
    d.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Error::Malformed("truncated"))
}

/// Split a GLB into its JSON document and its binary chunk.
pub fn split(glb: &[u8]) -> Result<(serde_json::Value, Vec<u8>), Error> {
    if u32_at(glb, 0)? != MAGIC {
        return Err(Error::Malformed("not a GLB"));
    }
    if u32_at(glb, 4)? != 2 {
        return Err(Error::Malformed("not GLB version 2"));
    }
    let mut at = 12;
    let mut json = None;
    let mut bin = Vec::new();
    while at + 8 <= glb.len() {
        let len = u32_at(glb, at)? as usize;
        let kind = u32_at(glb, at + 4)?;
        let body = glb
            .get(at + 8..at + 8 + len)
            .ok_or(Error::Malformed("chunk runs past the end"))?;
        match kind {
            CHUNK_JSON => json = Some(serde_json::from_slice(body).map_err(Error::Json)?),
            CHUNK_BIN => bin = body.to_vec(),
            _ => {}
        }
        at += 8 + len;
    }
    Ok((json.ok_or(Error::Malformed("no JSON chunk"))?, bin))
}

/// Assemble a GLB from a document and a binary chunk.
pub fn join(json: &serde_json::Value, bin: &[u8]) -> Vec<u8> {
    let mut doc = serde_json::to_vec(json).expect("a Value serializes");
    while doc.len() % 4 != 0 {
        doc.push(b' ');
    }
    let mut body = bin.to_vec();
    while body.len() % 4 != 0 {
        body.push(0);
    }
    let total = 12 + 8 + doc.len() + 8 + body.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(doc.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
    out.extend_from_slice(&doc);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Rewrite a GLB so every `image/ktx2` image holds what `encode` makes of
/// its bytes instead: `(bytes, mime type)`, given the image's index and its
/// KTX2. Returns the new GLB and how many images were converted.
pub fn rewrite(
    glb: &[u8],
    mut encode: impl FnMut(usize, &[u8]) -> Result<(Vec<u8>, &'static str), Error>,
) -> Result<(Vec<u8>, usize), Error> {
    let (mut json, bin) = split(glb)?;
    // Which bufferView each KTX2 image owns.
    let mut image_views = Vec::new();
    if let Some(images) = json.get("images").and_then(|v| v.as_array()) {
        for (i, im) in images.iter().enumerate() {
            let ktx = im.get("mimeType").and_then(|m| m.as_str()) == Some("image/ktx2");
            if let (true, Some(view)) = (ktx, im.get("bufferView").and_then(|v| v.as_u64())) {
                image_views.push((i, view as usize));
            }
        }
    }
    let views = json
        .get("bufferViews")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut new_bin = Vec::with_capacity(bin.len());
    let mut new_views = Vec::with_capacity(views.len());
    let mut mimes = Vec::with_capacity(image_views.len());
    let mut converted = 0;
    for (vi, view) in views.iter().enumerate() {
        let offset = view.get("byteOffset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let length =
            view.get("byteLength")
                .and_then(|v| v.as_u64())
                .ok_or(Error::Malformed("bufferView without byteLength"))? as usize;
        let bytes = bin
            .get(offset..offset + length)
            .ok_or(Error::Malformed("bufferView runs past the binary chunk"))?;
        let replaced;
        let bytes: &[u8] = if let Some(&(image, _)) = image_views.iter().find(|(_, v)| *v == vi) {
            let (out, mime) = encode(image, bytes)?;
            replaced = out;
            mimes.push((image, mime));
            converted += 1;
            &replaced
        } else {
            bytes
        };
        while new_bin.len() % 4 != 0 {
            new_bin.push(0);
        }
        let mut nv = view.clone();
        nv["byteOffset"] = serde_json::Value::from(new_bin.len());
        nv["byteLength"] = serde_json::Value::from(bytes.len());
        new_bin.extend_from_slice(bytes);
        new_views.push(nv);
    }
    if let Some(images) = json.get_mut("images").and_then(|v| v.as_array_mut()) {
        for (i, mime) in &mimes {
            images[*i]["mimeType"] = serde_json::Value::from(*mime);
        }
    }
    // The chunk `join` writes is padded to four bytes; the buffer's declared
    // length is that chunk, exactly, rather than three bytes short of it.
    while new_bin.len() % 4 != 0 {
        new_bin.push(0);
    }
    json["bufferViews"] = serde_json::Value::Array(new_views);
    if let Some(buffers) = json.get_mut("buffers").and_then(|v| v.as_array_mut()) {
        if let Some(b0) = buffers.first_mut() {
            b0["byteLength"] = serde_json::Value::from(new_bin.len());
        }
    }
    Ok((join(&json, &new_bin), converted))
}

/// Decode one level of a KTX2/UASTC container to RGBA8: `(w, h, rgba)`.
///
/// The same route Gates' `tests/packed_maps.rs` takes and Bevy takes —
/// container → zstd → UASTC transcode — with the row pitch in PIXELS for the
/// uncompressed target, because `basis-universal 0.3.1`'s safe wrapper
/// computes it in blocks and segfaults (the packer entry in Gates'
/// `CLAUDE.md`; neither is in this repo).
pub fn decode_level(ktx: &[u8], level: u32) -> Result<(u32, u32, Vec<u8>), Error> {
    use basis_universal::{sys, DecodeFlags, TranscoderBlockFormat};
    let reader = ktx2::Reader::new(ktx).map_err(|e| Error::Decode(format!("{e:?}")))?;
    let h = reader.header();
    let want = level.min(h.level_count.saturating_sub(1));
    let lv = reader
        .levels()
        .nth(want as usize)
        .ok_or_else(|| Error::Decode(format!("no level {want}")))?;
    let raw: Vec<u8> = match h.supercompression_scheme {
        Some(ktx2::SupercompressionScheme::Zstandard) => {
            let mut cursor = std::io::Cursor::new(lv.data);
            let mut dec = ruzstd::decoding::StreamingDecoder::new(&mut cursor)
                .map_err(|e| Error::Decode(format!("zstd: {e}")))?;
            let mut out = Vec::new();
            dec.read_to_end(&mut out)
                .map_err(|e| Error::Decode(format!("zstd: {e}")))?;
            out
        }
        None => lv.data.to_vec(),
        other => return Err(Error::Decode(format!("supercompression {other:?}"))),
    };
    let (w, hg) = (
        (h.pixel_width >> want).max(1),
        (h.pixel_height >> want).max(1),
    );
    let (bx, by) = (w.div_ceil(4), hg.div_ceil(4));
    if raw.len() != (bx * by * 16) as usize {
        return Err(Error::Decode(format!(
            "level {want} is {} bytes, not {bx}x{by} UASTC blocks",
            raw.len()
        )));
    }
    basis_universal::transcoder_init();
    let t = unsafe { sys::low_level_uastc_transcoder_new() };
    let mut rgba = vec![0u8; (w * hg * 4) as usize];
    let ok = unsafe {
        sys::low_level_uastc_transcoder_transcode_slice(
            t,
            rgba.as_mut_ptr() as _,
            bx,
            by,
            raw.as_ptr(),
            raw.len() as u32,
            TranscoderBlockFormat::RGBA32.into(),
            4,
            false,
            true,
            w,
            hg,
            w,
            std::ptr::null_mut(),
            hg,
            0,
            3,
            DecodeFlags::HIGH_QUALITY.bits(),
        )
    };
    unsafe { sys::low_level_uastc_transcoder_delete(t) };
    if !ok {
        return Err(Error::Decode("UASTC transcode failed".into()));
    }
    Ok((w, hg, rgba))
}

/// RGBA8 to PNG bytes.
pub fn png(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| Error::Png(e.to_string()))?;
        writer
            .write_image_data(rgba)
            .map_err(|e| Error::Png(e.to_string()))?;
    }
    Ok(out)
}

/// RGBA8 with its alpha dropped to a 4:4:4 JPEG at `quality`.
pub fn jpeg(w: u32, h: u32, rgba: &[u8], quality: u8) -> Result<Vec<u8>, Error> {
    let (w, h) = (
        u16::try_from(w).map_err(|_| Error::Jpeg("wider than 65535".into()))?,
        u16::try_from(h).map_err(|_| Error::Jpeg("taller than 65535".into()))?,
    );
    let mut out = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut out, quality);
    enc.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    enc.set_optimized_huffman_tables(true);
    enc.encode(rgba, w, h, jpeg_encoder::ColorType::Rgba)
        .map_err(|e| Error::Jpeg(e.to_string()))?;
    Ok(out)
}

/// One decoded map as the browser gets it: PNG where any texel has alpha
/// below 255, JPEG otherwise (the module header has why), as `(bytes, mime)`.
pub fn encode_web(
    w: u32,
    h: u32,
    rgba: &[u8],
    normal: bool,
) -> Result<(Vec<u8>, &'static str), Error> {
    if rgba.chunks_exact(4).any(|t| t[3] != 255) {
        return Ok((png(w, h, rgba)?, "image/png"));
    }
    let q = if normal {
        JPEG_QUALITY_NORMAL
    } else {
        JPEG_QUALITY
    };
    Ok((jpeg(w, h, rgba, q)?, "image/jpeg"))
}

/// The images a GLB's materials use as normal maps, by image index —
/// through `KHR_texture_basisu` where the texture names its KTX2 there.
pub fn normal_images(json: &serde_json::Value) -> Vec<usize> {
    let textures = json.get("textures").and_then(|v| v.as_array());
    let image_of = |t: u64| -> Option<usize> {
        let tex = textures?.get(t as usize)?;
        tex.pointer("/extensions/KHR_texture_basisu/source")
            .or_else(|| tex.get("source"))
            .and_then(|v| v.as_u64())
            .map(|i| i as usize)
    };
    json.get("materials")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|m| m.pointer("/normalTexture/index").and_then(|v| v.as_u64()))
        .filter_map(image_of)
        .collect()
}

/// Convert one GLB's KTX2 images at [`WEB_LEVEL`], each to the format
/// [`encode_web`] picks for it.
pub fn convert(glb: &[u8]) -> Result<(Vec<u8>, usize), Error> {
    let normals = normal_images(&split(glb)?.0);
    rewrite(glb, |image, ktx| {
        let (w, h, rgba) = decode_level(ktx, WEB_LEVEL)?;
        encode_web(w, h, &rgba, normals.contains(&image))
    })
}

/// Point each texture at its converted image directly and drop
/// `KHR_texture_basisu`, which no longer describes anything in the file.
pub fn unbasis(json: &mut serde_json::Value) {
    if let Some(textures) = json.get_mut("textures").and_then(|v| v.as_array_mut()) {
        for t in textures {
            let src = t.pointer("/extensions/KHR_texture_basisu/source").cloned();
            if let Some(src) = src {
                t["source"] = src;
                if let Some(ext) = t.get_mut("extensions").and_then(|e| e.as_object_mut()) {
                    ext.remove("KHR_texture_basisu");
                    if ext.is_empty() {
                        t.as_object_mut().unwrap().remove("extensions");
                    }
                }
            }
        }
    }
    for key in ["extensionsUsed", "extensionsRequired"] {
        if let Some(list) = json.get_mut(key).and_then(|v| v.as_array_mut()) {
            list.retain(|e| e != "KHR_texture_basisu");
            if list.is_empty() {
                json.as_object_mut().unwrap().remove(key);
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(src), Some(dst)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: oneshot-glb <in dir> <out dir>");
        std::process::exit(2);
    };
    std::fs::create_dir_all(dst).unwrap();
    let mut paths: Vec<_> = std::fs::read_dir(src)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("glb"))
        .collect();
    paths.sort();
    for p in paths {
        let glb = std::fs::read(&p).unwrap();
        let (out, n) = convert(&glb).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        let (mut json, bin) = split(&out).unwrap();
        unbasis(&mut json);
        let out = join(&json, &bin);
        let to = std::path::Path::new(dst).join(p.file_name().unwrap());
        std::fs::write(&to, &out).unwrap();
        println!(
            "{}: {n} images, {} -> {} KB",
            to.display(),
            glb.len() / 1024,
            out.len() / 1024
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A GLB with two views: a "mesh" view and an image view marked KTX2.
    fn glb() -> Vec<u8> {
        let mesh: Vec<u8> = (0..37u8).collect();
        let fake_ktx: Vec<u8> = (100..110u8).collect();
        let mut bin = mesh.clone();
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let img_off = bin.len();
        bin.extend_from_slice(&fake_ktx);
        let json = serde_json::json!({
            "asset": {"version": "2.0"},
            "buffers": [{"byteLength": bin.len()}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": mesh.len(), "target": 34962},
                {"buffer": 0, "byteOffset": img_off, "byteLength": fake_ktx.len()}
            ],
            "accessors": [{"bufferView": 0, "byteOffset": 4, "componentType": 5126, "count": 2, "type": "VEC3"}],
            "images": [{"mimeType": "image/ktx2", "bufferView": 1}],
            "textures": [{"source": 0}]
        });
        join(&json, &bin)
    }

    #[test]
    fn the_image_view_is_replaced_and_every_other_view_is_byte_identical() {
        let src = glb();
        let (out, n) = rewrite(&src, |image, ktx| {
            assert_eq!(image, 0);
            assert_eq!(ktx, &(100..110u8).collect::<Vec<u8>>());
            Ok((vec![7u8; 23], "image/jpeg"))
        })
        .unwrap();
        assert_eq!(n, 1);
        let (json, bin) = split(&out).unwrap();
        assert_eq!(json["images"][0]["mimeType"], "image/jpeg");
        let views = json["bufferViews"].as_array().unwrap();
        let (o0, l0) = (
            views[0]["byteOffset"].as_u64().unwrap() as usize,
            views[0]["byteLength"].as_u64().unwrap() as usize,
        );
        assert_eq!(&bin[o0..o0 + l0], &(0..37u8).collect::<Vec<u8>>()[..]);
        assert_eq!(views[0]["target"], 34962, "a view's other fields travel");
        let (o1, l1) = (
            views[1]["byteOffset"].as_u64().unwrap() as usize,
            views[1]["byteLength"].as_u64().unwrap() as usize,
        );
        assert_eq!(&bin[o1..o1 + l1], &[7u8; 23][..]);
        assert_eq!(o1 % 4, 0, "views are 4-aligned");
        assert_eq!(
            json["buffers"][0]["byteLength"].as_u64().unwrap() as usize,
            bin.len()
        );
        assert_eq!(
            json["accessors"][0]["byteOffset"], 4,
            "accessors address their view relatively and do not move"
        );
        // A round trip through `split`/`join` is exact.
        assert_eq!(split(&join(&json, &bin)).unwrap().1, bin);
    }

    #[test]
    fn a_model_with_no_ktx2_passes_through() {
        let src = glb();
        let (mut json, bin) = split(&src).unwrap();
        json["images"][0]["mimeType"] = serde_json::Value::from("image/png");
        let plain = join(&json, &bin);
        let (out, n) = rewrite(&plain, |_, _| panic!("nothing to convert")).unwrap();
        assert_eq!(n, 0);
        assert_eq!(split(&out).unwrap().0["images"][0]["mimeType"], "image/png");
    }

    #[test]
    fn a_png_round_trips_its_pixels() {
        let rgba: Vec<u8> = (0..4 * 4 * 4).map(|i| (i * 7 % 256) as u8).collect();
        let bytes = png(4, 4, &rgba).unwrap();
        let dec = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = dec.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (4, 4));
        assert_eq!(&buf[..info.buffer_size()], &rgba[..]);
    }

    #[test]
    fn alpha_picks_png_and_an_opaque_map_is_a_jpeg() {
        let (w, h) = (16u32, 16u32);
        let mut rgba: Vec<u8> = (0..w * h)
            .flat_map(|i| [(i % w * 16) as u8, (i / w * 16) as u8, 128, 255])
            .collect();
        let (bytes, mime) = encode_web(w, h, &rgba, false).unwrap();
        assert_eq!(mime, "image/jpeg");
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
        rgba[7] = 254;
        assert_eq!(encode_web(w, h, &rgba, false).unwrap().1, "image/png");
    }

    #[test]
    fn normal_maps_are_found_through_the_basisu_extension() {
        let json = serde_json::json!({
            "materials": [
                {"normalTexture": {"index": 1}, "pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}},
                {"normalTexture": {"index": 2}}
            ],
            "textures": [
                {"extensions": {"KHR_texture_basisu": {"source": 0}}},
                {"extensions": {"KHR_texture_basisu": {"source": 3}}},
                {"source": 4}
            ]
        });
        assert_eq!(normal_images(&json), vec![3, 4]);
    }
}

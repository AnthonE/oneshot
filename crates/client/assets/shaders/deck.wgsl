// The cloud deck's geometry and fine detail on the GPU (after Gates'
// assets/shaders/deck.wgsl, MIT, NOTICE), shared by the deck (`cloud.wgsl`),
// the stars (`stars.wgsl`) and the moon (`moon.wgsl`), so a cloud hides the
// moon exactly where the sky draws that cloud. Texture reads stay in each
// shader: a WebGL2 build cannot pass a texture into a function.

// Where a ray meets the deck, noise units: `sky::TexelGeo::new`'s
// projection. `alt` is the deck's altitude over its noise scale; the ray's
// height is held at the horizon cutoff so the point stays finite under it.
fn deck_point(d: vec3<f32>, alt: f32, cutoff: f32) -> vec2<f32> {
    return d.xz * (alt / max(d.y, cutoff));
}

// How much of the sky a ray `y` above the horizon loses to the weather's
// fog, `0..=1`: `sky::SkyFog::band` short of its dust (`deck_haze` adds it).
// `air` is `SkyFog::lanes` — the ground fog and the rain haze, each straight
// up and level across the island.
fn deck_fog(y: f32, air: vec4<f32>) -> f32 {
    let s = max(y + 0.02, 1e-3);
    let tau = min(air.x / s, air.z) + min(air.y / s, air.w);
    return 1.0 - exp(-tau);
}

// `deck_fog` with the dust over arid ground in it: `sky::SkyFog::band`
// whole. `dust` is `SkyFog::dust_lanes` — x straight up through its layer,
// y level across the island. The layers' depths add, so their clear
// shares multiply.
fn deck_haze(y: f32, air: vec4<f32>, dust: vec4<f32>) -> f32 {
    let s = max(y + 0.02, 1e-3);
    return 1.0 - (1.0 - deck_fog(y, air)) * exp(-min(dust.x / s, dust.y));
}

// A veil of the fog over the whole sky (`Mood::veil`), on `band`: `veil` of it
// all the way up, and whole by the horizon so the far things are lost in it.
const VEIL_HORIZON_Y: f32 = 0.16;
// The veil thins around the sun, so its break shows through: how far off it
// (a power of the cosine) and by how much.
const VEIL_BREAK_POW: f32 = 5.0;
const VEIL_BREAK: f32 = 0.75;
// The veil's masses: their wavelengths on a plane `VEIL_ALT` over the eye (in
// units of that height, so they crowd together toward the horizon as a ceiling
// does) and how far they darken it at the thickest and lift it at the thinnest.
const VEIL_ALT: f32 = 1.0;
const VEIL_WAVES: vec4<f32> = vec4(1.5, 0.62, 0.27, 0.11);
const VEIL_DARK: f32 = 0.26;
const VEIL_LIFT: f32 = 0.92;
// How fast they drift, plane units a second, so slowly it is only seen by
// looking away and back.
const VEIL_DRIFT: vec2<f32> = vec2(0.0024, 0.0011);

fn deck_veil(band: f32, d: vec3<f32>, to_sun: vec3<f32>, veil: f32) -> f32 {
    if veil <= 0.0 {
        return band;
    }
    let toward = pow(max(dot(d, to_sun), 0.0), VEIL_BREAK_POW);
    let thin = veil * (1.0 - VEIL_BREAK * toward);
    return max(band, mix(1.0, thin, smoothstep(0.0, VEIL_HORIZON_Y, d.y)));
}

// What the veil's colour is multiplied by along `d`: broad soft masses of
// cloud with smaller ones in them, dark bellies and paler thin places, fading
// to one value as they crowd toward the horizon.
fn deck_veil_mass(d: vec3<f32>, time: f32) -> f32 {
    let q = deck_point(d, VEIL_ALT, 0.03) + VEIL_DRIFT * time;
    let w = vec2(deck_value(q * 1.3 + vec2(3.1, 7.7)), deck_value(q * 1.3 + vec2(11.9, 1.3))) - 0.5;
    let p = q + w * 0.55;
    // Billowed, its fold rounded so it leaves bellies and no contour lines.
    let x = 2.0 * deck_value(p / VEIL_WAVES.y + vec2(5.2, 2.9)) - 1.0;
    let fold = 1.1 - sqrt(x * x + 0.16);
    let n = deck_value(p / VEIL_WAVES.x) * 0.5 + fold * 0.32
        + deck_value(p / VEIL_WAVES.z + vec2(8.4, 6.1)) * 0.13 + deck_value(p / VEIL_WAVES.w + vec2(1.7, 9.3)) * 0.05;
    let m = mix(VEIL_DARK, VEIL_LIFT, smoothstep(0.3, 0.74, n));
    return mix(1.0, m, smoothstep(0.015, 0.14, d.y));
}

// The raymarched slab's shape (`cloud.wgsl`), shared with the stars and
// the moon so a cloud hides them where it is drawn rather than where its
// base's outline is. A cumulus seen from the side rises far above that
// outline, and stars read off the flat deck shone through its flanks.
//
// How far a column wanders from its base by the slab's ceiling, m: a 2D
// field read straight up is an extrusion, and its grain runs up a cloud's
// side as stripes; none at the base, whose outline the sun's cover reads.
const SLAB_WARP_M: f32 = 90.0;

fn slab_warp(p: vec3<f32>, h: f32) -> vec2<f32> {
    return vec2(sin(h * 7.0 + p.z * 0.0031), sin(h * 6.0 + p.x * 0.0027)) * (SLAB_WARP_M * h);
}

// The slab's density before erosion at height `h` (`0..1` through it) in a
// column `c` edges past the field's threshold, `wide` edges past it in the
// field blurred to `SLAB_WIDE_M` (`slab_wide_lod`). A flat base at the
// condensation level and a dome over it: the field is near linear across a
// cloud's edge, so a height linear in it builds cones, and its square root
// builds the rounded top of a hemisphere.
//
// **A cloud is as tall as it is broad.** The dome's height is read off the
// blurred field, which only a broad cloud's middle stands above: a puff a
// couple of hundred metres across stays a low scrap (`SLAB_EDGE_TOP` of the
// slab), and a cloud a kilometre across builds to `SLAB_DOME` of it. Read
// off the column's own value, as it was, every peak in the field stood a
// sixth of the slab tall at its very edge: each cloud had a wall for a
// side, and a small one was a pillar. Its skin is a little soft
// (`SLAB_SKIN` edges to full density), so the erosion has an edge to carve.
const SLAB_WIDE_M: f32 = 260.0;
const SLAB_EDGE_TOP: f32 = 0.06;
const SLAB_DOME: f32 = 0.8;
const SLAB_DOME_OVER: f32 = 0.45;
const SLAB_SKIN: f32 = 0.7;

// The field's mip whose texel is `SLAB_WIDE_M` across: `texels` a side,
// `uv_per_m` of its UV to a metre across the deck.
fn slab_wide_lod(texels: f32, uv_per_m: f32) -> f32 {
    return max(log2(SLAB_WIDE_M * uv_per_m * texels), 0.0);
}

fn slab_shape(h: f32, c: f32, wide: f32) -> f32 {
    if c <= 0.0 || h <= 0.0 || h >= 1.0 {
        return 0.0;
    }
    let top = SLAB_EDGE_TOP + SLAB_DOME * sqrt(clamp(wide * SLAB_DOME_OVER, 0.0, 1.0));
    let profile = smoothstep(0.0, 0.05, h) * (1.0 - smoothstep(top * 0.5, top, h));
    return smoothstep(0.0, SLAB_SKIN, c) * profile;
}

// Where a ray from the eye (at height `eye_y`) is at its `i`th of `n` even
// steps through the slab `slab` (x floor m, y ceiling m), across the deck
// with the eye at its origin; w is the step's length, m. `cutoff` holds a
// low ray's height up so the run stays finite. The run is at most 14 km, as
// the clouds' own (`cloud.wgsl`'s `MAX_RUN_M`).
fn slab_step(d: vec3<f32>, eye_y: f32, slab: vec4<f32>, cutoff: f32, i: i32, n: i32) -> vec4<f32> {
    let dy = max(d.y, cutoff);
    let t0 = max((slab.x - eye_y) / dy, 0.0);
    let t1 = min((slab.y - eye_y) / dy, t0 + 14000.0);
    let dt = max(t1 - t0, 0.0) / f32(n);
    let t = t0 + (f32(i) + 0.5) * dt;
    return vec4(d.x * t, eye_y + d.y * t, d.z * t, dt);
}

// How much of the deck stands in front, by the ray's height: none under
// the cutoff, all of it `fade` above.
fn deck_fade(y: f32, cutoff: f32, fade: f32) -> f32 {
    return clamp((y - cutoff) / fade, 0.0, 1.0);
}

fn deck_hash(p: vec2<i32>) -> f32 {
    var h = (bitcast<u32>(p.x) * 0x8da6b343u) ^ (bitcast<u32>(p.y) * 0xd8163841u);
    h = (h ^ (h >> 15u)) * 0x2c1b3c6du;
    h = (h ^ (h >> 12u)) * 0x297a2d39u;
    h = h ^ (h >> 15u);
    return f32(h >> 8u) / 16777216.0;
}

fn deck_value(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let s = f * f * (3.0 - 2.0 * f);
    let a = deck_hash(i);
    let b = deck_hash(i + vec2(1, 0));
    let c = deck_hash(i + vec2(0, 1));
    let d = deck_hash(i + vec2(1, 1));
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

// Two billowed octaves above the field's last (`sky::billow_tiled`'s
// cauliflower, continued past what the field's texels hold), centred on
// zero, at unit amplitude. The fold is rounded: a sharp one cuts thin cloud
// into a web of dark cracks. `foot` is a pixel's span on the deck, noise
// units: an octave a pixel spans fades out rather than shimmering.
fn deck_detail(q: vec2<f32>, foot: f32) -> f32 {
    var p = q * 16.0;
    var wave = 1.0 / 16.0;
    var amp = 1.0;
    var sum = 0.0;
    for (var o = 0; o < 2; o += 1) {
        let keep = clamp(wave / max(foot, 1e-6) - 1.0, 0.0, 1.0);
        let x = 2.0 * deck_value(p) - 1.0;
        sum += amp * keep * (sqrt(x * x + 0.09) - 0.55);
        // Twice the frequency and turned, so the lattices do not line up.
        p = vec2(p.x * 1.6 - p.y * 1.2, p.x * 1.2 + p.y * 1.6) + vec2(17.3, -9.1);
        wave *= 0.5;
        amp *= 0.5;
    }
    return sum;
}

// The deck's drift now, noise units: where it was when it was last taken
// (`look/mood.rs`'s `Anchor`), run on at its speed since. A material is
// written only when the run wanders off the server's clock, not a frame.
// `drift.xy` the offset, `flow.xy` noise units a second, `flow.z` the
// `globals.time` it was taken at.
fn deck_drift(drift: vec2<f32>, flow: vec4<f32>, time: f32) -> vec2<f32> {
    return drift + flow.xy * (time - flow.z);
}

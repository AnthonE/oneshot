// The sea of cloud under an island over a void (`look/cloud_sea.rs`):
// `bevy_pbr`'s own fragment over a flat sheet at the map's sea level, its
// colour and its normal taken from billows of noise in the world's x and z,
// drifting, so the sun lights their tops and greys their hollows, and the
// air fades it with the distance as it fades everything.
//
// Uniforms and arithmetic only (the noise is `truck_wear.wgsl`'s), so it
// binds and runs the same on WebGL2.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

struct Sea {
    // A billow's top and the hollow between, linear.
    top: vec4<f32>,
    hollow: vec4<f32>,
    // x the big billows' size, m; y how steeply they stand (the normal's
    // tilt at full); z 0 the sea, above that a thin sheet of mist over it; w 0
    // (the drift is `sea_motion`'s).
    shape: vec4<f32>,
    // xy the sheet's middle (world x, z); z how far out from it the sheet
    // begins to go to the fog's colour, w where it has gone as far as it
    // goes, m.
    fade: vec4<f32>,
    // xyz the colour it goes to (the weather fog's, `rig::fog_for`, in the
    // lit frame's units); w how far toward it the rim gets.
    far: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> sea: Sea;
// What moves every frame (`sea_motion.rs`), written in place so the
// material never changes: texel 1's zw is the drift, m, already wrapped.
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var sea_motion: texture_2d<f32>;

// How thick a sheet of mist is at its thickest.
const MIST_ALPHA: f32 = 0.82;

fn hash2(c: vec2<i32>) -> f32 {
    var h = (bitcast<u32>(c.x) * 0x8da6b343u) ^ (bitcast<u32>(c.y) * 0xd8163841u);
    h = (h ^ (h >> 16u)) * 0x7feb352du;
    h = (h ^ (h >> 15u)) * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return f32(h >> 8u) * (1.0 / 16777216.0);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash2(i), hash2(i + vec2<i32>(1, 0)), u.x);
    let b = mix(hash2(i + vec2<i32>(0, 1)), hash2(i + vec2<i32>(1, 1)), u.x);
    return mix(a, b, u.y);
}

// How much of a noise at `freq` cycles/m a pixel `px` metres across can
// carry: whole at six pixels a cycle, gone at two (far off it is flat,
// as a sea of cloud is from far off).
fn band(px: f32, freq: f32) -> f32 {
    return clamp(1.5 - px * freq * 3.0, 0.0, 1.0);
}

// Fractal noise, `octaves` of value noise each finer and fainter, every one
// fading to its mean where the pixel can't hold it. 0..1, about 0.5.
fn fbm(p0: vec2<f32>, freq0: f32, px: f32, octaves: i32) -> f32 {
    var freq = freq0;
    var amp = 0.5;
    var sum = 0.0;
    var norm = 0.0;
    var p = p0;
    for (var k = 0; k < octaves; k++) {
        sum += mix(0.5, vnoise(p * freq), band(px, freq)) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.13;
        p = vec2<f32>(p.x * 0.8 - p.y * 0.6, p.x * 0.6 + p.y * 0.8) + vec2<f32>(37.1, 11.7);
    }
    return sum / norm;
}

// Banks of mist: fractal noise read through itself, so the masses smear and
// curl into long soft shapes as a sea of stratus does and no lattice shows.
// Rounded over its middle (a sheet of fog has few hard creases).
fn billows(p: vec2<f32>, px: f32) -> f32 {
    let freq = 1.0 / sea.shape.x;
    let warp = vec2<f32>(
        fbm(p + vec2<f32>(13.1, 71.7), freq * 0.5, px, 3),
        fbm(p + vec2<f32>(57.3, 9.2), freq * 0.5, px, 3)
    ) - 0.5;
    // Drawn out a little across the wind, as a stratus sheet lies. A stronger
    // warp, under perspective, reads as a fan of streaks.
    let q = (p + warp * sea.shape.x * 0.3) * vec2<f32>(0.8, 1.0);
    let bank = fbm(q, freq, px, 6);
    return smoothstep(0.24, 0.76, bank);
}

// Light and shade over the sea in broad fields, three billows across: the
// deck's shadows on it, which no shadow map reaches so far down.
fn fields(p: vec2<f32>, px: f32) -> f32 {
    let freq = 1.0 / (sea.shape.x * 3.0);
    return mix(0.5, vnoise(p * freq + vec2<f32>(101.0, 47.0)), band(px, freq));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let wp = in.world_position.xyz;
    // A pixel's footprint: the mean of its two sides, so a grazing look
    // (long one way, short the other) keeps the big billows far out.
    let px = sqrt(length(dpdx(wp)) * length(dpdy(wp)));
    // A sheet of mist drifts faster the higher it lies, its own pattern.
    let layer = sea.shape.z;
    let p = wp.xz * (1.0 + layer * 0.17) + textureLoad(sea_motion, vec2<i32>(1, 0), 0).zw * (1.0 + layer * 0.6) + vec2<f32>(211.0, 97.0) * layer;
    // The height of the billows here and a step aside, for their slope:
    // the step a pixel's width at least, so a far pixel's normal is flat.
    let e = max(px, sea.shape.x * 0.02);
    let h = billows(p, px);
    let hx = billows(p + vec2<f32>(e, 0.0), px);
    let hz = billows(p + vec2<f32>(0.0, e), px);
    let k = sea.shape.y * sea.shape.x * 0.018 / e;
    // Tilted off the sheet's own normal (the bank leans in).
    let up = normalize(in.world_normal);
    let n = normalize(up + vec3<f32>(-(hx - h) * k, 0.0, -(hz - h) * k));
    pbr_input.N = n;
    pbr_input.world_normal = n;
    // Fog has no shine.
    pbr_input.material.perceptual_roughness = 1.0;
    pbr_input.material.reflectance = vec3<f32>(0.0);

    // Tops pale, folds grey, and broad fields of light and shade over
    // all of it: the light does the rest.
    let shade = 0.88 + 0.24 * fields(p, px);
    let c = mix(sea.hollow.rgb, sea.top.rgb, h) * shade;
    pbr_input.material.base_color = vec4<f32>(c, 1.0);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    // The bank goes to the fog's colour as it climbs, so its rim is lost
    // against the sky's own haze and never reads as an edge.
    let out_m = distance(wp.xz, sea.fade.xy);
    let far = smoothstep(sea.fade.z, sea.fade.w, out_m) * sea.far.w;
    out.color = vec4<f32>(mix(out.color.rgb, sea.far.rgb, far), out.color.a);
    // The mist's sheets: only where a bank runs high, thin, and gone by the
    // time they rise into the far bank.
    if layer > 0.5 {
        let thin = 1.0 - smoothstep(sea.fade.z * 0.5, sea.fade.z, out_m);
        out.color.a = smoothstep(0.46, 0.86, h) * MIST_ALPHA * thin;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

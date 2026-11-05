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
    // tilt at full); zw the drift, m, already wrapped.
    shape: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> sea: Sea;

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

// Billows: rounded, so the noise is folded to puff up (1 − |2n − 1|
// squared is a ridge; its square root a dome), four octaves, each
// fainter and finer, each fading out where the pixel can't hold it.
fn billows(p: vec2<f32>, px: f32) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var freq = 1.0 / sea.shape.x;
    var q = p;
    for (var k = 0; k < 4; k++) {
        let n = vnoise(q * freq);
        let dome = sqrt(clamp(1.0 - abs(2.0 * n - 1.0), 0.0, 1.0));
        sum += mix(0.6, dome, band(px, freq)) * amp;
        amp *= 0.5;
        freq *= 2.13;
        q += vec2<f32>(37.1, 11.7);
    }
    return sum / 0.9375;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let wp = in.world_position.xyz;
    let px = max(length(dpdx(wp)), length(dpdy(wp)));
    let p = wp.xz + sea.shape.zw;
    // The height of the billows here and a step aside, for their slope:
    // the step a pixel's width at least, so a far pixel's normal is flat.
    let e = max(px, sea.shape.x * 0.02);
    let h = billows(p, px);
    let hx = billows(p + vec2<f32>(e, 0.0), px);
    let hz = billows(p + vec2<f32>(0.0, e), px);
    let k = sea.shape.y * sea.shape.x * 0.08 / e;
    let n = normalize(vec3<f32>(-(hx - h) * k, 1.0, -(hz - h) * k));
    pbr_input.N = n;
    pbr_input.world_normal = n;

    // Tops pale, hollows grey: the light does the rest.
    let c = mix(sea.hollow.rgb, sea.top.rgb, smoothstep(0.25, 0.85, h));
    pbr_input.material.base_color = vec4<f32>(c, 1.0);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

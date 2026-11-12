// A snowed valley's boulder (`look/snow_stone.rs`): `bevy_pbr`'s own fragment
// over its paint, with snow lying on what faces the sky. Its flanks stay dark:
// a boulder is cover against white, and only its crown is capped.
//
// A uniform and arithmetic, so it binds and runs the same on WebGL2.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

struct SnowStone {
    // x how far a facet turns up where the cap starts, y where it is whole
    // (its normal's y); z how much of a cap this stone takes (0 a made
    // thing); w the cap's roughness.
    cap: vec4<f32>,
    // rgb the cap's colour; w 0.
    snow: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> stone: SnowStone;

// The cap's edge is torn at two sizes, m: a hand's and a finger's. Straight,
// it was a contour line round the boulder.
const TORN_M: vec2<f32> = vec2<f32>(0.6, 0.17);
const TORN: vec2<f32> = vec2<f32>(0.22, 0.1);

fn hash3(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yxz + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn value3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(mix(hash3(i), hash3(i + vec3<f32>(1.0, 0.0, 0.0)), u.x),
            mix(hash3(i + vec3<f32>(0.0, 1.0, 0.0)), hash3(i + vec3<f32>(1.0, 1.0, 0.0)), u.x), u.y),
        mix(mix(hash3(i + vec3<f32>(0.0, 0.0, 1.0)), hash3(i + vec3<f32>(1.0, 0.0, 1.0)), u.x),
            mix(hash3(i + vec3<f32>(0.0, 1.0, 1.0)), hash3(i + vec3<f32>(1.0, 1.0, 1.0)), u.x), u.y),
        u.z);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let p = in.world_position.xyz;
    let wn = normalize(in.world_normal);
    // Each noise goes to its mean when a pixel is its own size.
    let px = max(length(dpdx(p)), length(dpdy(p)));
    let torn = (value3(p / TORN_M.x) - 0.5) * 2.0 * TORN.x * (1.0 - smoothstep(0.3, 1.2, px / TORN_M.x))
        + (value3(p / TORN_M.y + vec3<f32>(7.3, 1.9, 4.1)) - 0.5) * 2.0 * TORN.y * (1.0 - smoothstep(0.3, 1.2, px / TORN_M.y));
    let cap = smoothstep(stone.cap.x, stone.cap.y, wn.y + torn) * stone.cap.z;
    let c = mix(pbr_input.material.base_color.rgb, stone.snow.rgb, cap);
    pbr_input.material.base_color = vec4<f32>(c, pbr_input.material.base_color.a);
    pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, stone.cap.w, cap);
    // Snow lies smooth over the stone's own grain.
    pbr_input.N = normalize(mix(pbr_input.N, wn, cap));

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

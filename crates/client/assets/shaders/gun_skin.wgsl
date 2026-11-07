// A skin on a gun (`look/gun_skin.rs`): `bevy_pbr`'s own fragment with the
// gun's paint recoloured before lighting. Saturated paint takes the coat's
// colour and the rest (steel, polymer, bone) its dark; each keeps its own
// light and dark against that part's middle, so wear, grime and panel lines
// stay where they were painted. What the gun's glow map lights is left alone.
//
// The gun in our own view wears it too, coated or not (`look/gun_glow.rs`):
// its glow is turned up and down as a whole, and a gauge puts the glow out
// where its rounds are spent, by where a texel is in the model's own frame.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

struct Coat {
    // rgb: the paint's linear colour at its middle; w its metal.
    paint: vec4<f32>,
    // rgb: the rest's linear colour at its middle; w the paint's roughness.
    rest: vec4<f32>,
    // x, y: the paint's and the rest's middle (linear luma); z, w: how far
    // from grey a texel starts to count as paint and where it does.
    mid: vec4<f32>,
    // The model's frame from the world's.
    local: mat4x4<f32>,
    // x: the glow's gain; y: how much of the coat it wears (0: its own
    // paint); z: what of the glow a gauge leaves where it is out; w: the
    // gauge's lit share, 0..=1.
    glow: vec4<f32>,
    // A run's end lit longest (xyz), and its edge's width, a share (w).
    stays: vec4<f32>,
    // A run's end out first (xyz); w how many cells (the first out last).
    goes: vec4<f32>,
    // Boxes in the model's frame, low and high corners in turn: the run's
    // region (empty for none), then each cell's.
    boxes: array<vec4<f32>, 10>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> coat: Coat;

// How far over its middle a texel's tone may go.
const TONE_MAX: f32 = 2.5;
// The glow map's level where a texel is all light.
const LIT_FULL: f32 = 0.1;
// Cells a gauge has at most.
const CELLS: i32 = 4;

fn inside(p: vec3<f32>, k: i32) -> bool {
    return all(p >= coat.boxes[2 * k].xyz) && all(p <= coat.boxes[2 * k + 1].xyz);
}

// How lit the gauge leaves a point of the model, 0..=1; 1 off it.
fn gauge(p: vec3<f32>) -> f32 {
    let share = coat.glow.w;
    if inside(p, 0) {
        let d = coat.goes.xyz - coat.stays.xyz;
        let t = clamp(dot(p - coat.stays.xyz, d) / max(dot(d, d), 1e-8), 0.0, 1.0);
        let edge = max(coat.stays.w, 1e-3);
        return clamp((share * (1.0 + edge) - t) / edge, 0.0, 1.0);
    }
    let n = i32(coat.goes.w + 0.5);
    for (var k = 0; k < CELLS; k++) {
        if k < n && inside(p, k + 1) {
            return clamp(share * f32(n) - f32(k), 0.0, 1.0);
        }
    }
    return 1.0;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    let c = pbr_input.material.base_color.rgb;
    // How far from grey it was painted, near enough on sRGB's curve: a share
    // of its brightness instead would make paint of a dark texel's JPEG noise.
    let g = sqrt(max(c, vec3<f32>(0.0)));
    let chroma = max(g.r, max(g.g, g.b)) - min(g.r, min(g.g, g.b));
    let glow = pbr_input.material.emissive.rgb;
    let lit = smoothstep(0.0, LIT_FULL, max(glow.r, max(glow.g, glow.b)));
    let paint = smoothstep(coat.mid.z, coat.mid.w, chroma) * (1.0 - lit) * coat.glow.y;
    let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    let coated = mix(
        coat.rest.rgb * min(luma / coat.mid.y, TONE_MAX),
        coat.paint.rgb * min(luma / coat.mid.x, TONE_MAX),
        paint,
    );
    pbr_input.material.base_color = vec4<f32>(mix(c, mix(coated, c, lit), coat.glow.y), pbr_input.material.base_color.a);
    pbr_input.material.metallic = mix(pbr_input.material.metallic, coat.paint.w, paint);
    pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, coat.rest.w, paint);

    let p = (coat.local * vec4<f32>(in.world_position.xyz, 1.0)).xyz;
    let gain = coat.glow.x * mix(coat.glow.z, 1.0, gauge(p));
    pbr_input.material.emissive = vec4<f32>(glow * gain, pbr_input.material.emissive.a);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

// Triple T's spray paint (`look/paint.rs`): `bevy_pbr`'s own fragment with
// the paint laid over the wood before lighting. The mask is four coverages
// over the model's UVs (r the headband, g the emblem's glyph, b its field,
// a the trim); their colours are the uniform's. Paint takes on the bark
// and not in its cracks, dries satin, and the headband and trim glow a
// little in their colours.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_functions,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

struct Paint {
    // Each coat's linear colour; w 1 sprayed, 0 not.
    team: vec4<f32>,
    glyph: vec4<f32>,
    field: vec4<f32>,
    trim: vec4<f32>,
    // rgb: the headband's and the trim's glow at full cover, nits.
    glow: vec4<f32>,
    sheen: vec4<f32>,
    // x, y: the bark's luma (before the lift) where paint starts to take and
    // where it covers; z the lift; w the paint's roughness.
    bite: vec4<f32>,
    // rgb: the light the shield's shell casts on it at a flare of 1, nits:
    // gold, and an overshield's green.
    flare_gold: vec4<f32>,
    flare_over: vec4<f32>,
}

// `shield::LEVEL_MAX`: what the tag's 16 bits of level run to.
const LEVEL_MAX: f32 = 4.0;

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> paint: Paint;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var mask_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var mask_smp: sampler;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef VERTEX_UVS_A
    let m = textureSample(mask_tex, mask_smp, in.uv);
    var c = pbr_input.material.base_color.rgb;
    let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)) / max(paint.bite.z, 1e-4);
    let soak = smoothstep(paint.bite.x, paint.bite.y, luma);
    let f = m.b * soak * paint.field.w;
    let g = m.g * soak * paint.glyph.w;
    let t = m.r * soak * paint.team.w;
    let s = m.a * soak * paint.trim.w;
    c = mix(c, paint.field.rgb, f);
    c = mix(c, paint.glyph.rgb, g);
    c = mix(c, paint.team.rgb, t);
    c = mix(c, paint.trim.rgb, s);
    let cover = max(max(f, g), max(t, s));
    pbr_input.material.base_color = vec4<f32>(c, pbr_input.material.base_color.a);
    pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, paint.bite.w, cover);
    pbr_input.material.emissive = vec4<f32>(
        pbr_input.material.emissive.rgb + paint.glow.rgb * t + paint.sheen.rgb * s,
        pbr_input.material.emissive.a,
    );
#endif

    // The shell's light on it, as this mesh's tag says (`shield::Flare`):
    // none while camouflage hides it.
    let tag = mesh_functions::get_tag(in.instance_index);
    if (tag & (1u << 18u)) == 0u {
        let level = f32(tag & 0xffffu) / 65535.0 * LEVEL_MAX;
        let flare = select(paint.flare_gold.rgb, paint.flare_over.rgb, ((tag >> 16u) & 3u) == 1u);
        pbr_input.material.emissive = vec4<f32>(
            pbr_input.material.emissive.rgb + flare * level,
            pbr_input.material.emissive.a,
        );
    }

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

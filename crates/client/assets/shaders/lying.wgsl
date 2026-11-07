// A gun lying for the taking (`look/lying.rs`): `bevy_pbr`'s own fragment,
// the gun as it came, with a faint glow over it, strongest at its edges so
// its outline reads off the floor. Its own glow map (sights, counters) stays.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

struct Glow {
    // rgb: the glow's linear colour at the edge; w: the share of it that
    // lies over the whole gun.
    color: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> glow: Glow;

// How tight to the edge the rim is.
const RIM_POW: f32 = 2.5;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    let facing = clamp(dot(pbr_input.N, pbr_input.V), 0.0, 1.0);
    let rim = pow(1.0 - facing, RIM_POW);
    out.color = vec4<f32>(out.color.rgb + glow.color.rgb * (glow.color.w + rim), out.color.a);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

// An island's scenery stone (`look/isle_stone.rs`): `bevy_pbr`'s own fragment
// over its paint, darker the further it hangs under the rim, and going to the
// cloud's colour as it comes down to the sea of cloud, so nothing stands on the
// sheet with a hard line.
//
// A uniform and arithmetic, so it binds and runs the same on WebGL2.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

struct IsleStone {
    // x where its shade starts, y where it is whole (heights, m); z how much of
    // its colour is left there; w 0.
    shade: vec4<f32>,
    // xyz the cloud's colour (the mood's fog, in the frame's lit units); w the
    // height it starts going to it, m. The sea's sheet is `LAP_M` under that.
    lap: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> stone: IsleStone;

// `isle_stone::LAP_M`.
const LAP_M: f32 = 24.0;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let y = in.world_position.y;
    let deep = clamp((stone.shade.x - y) / (stone.shade.x - stone.shade.y), 0.0, 1.0);
    let c = pbr_input.material.base_color.rgb * mix(1.0, stone.shade.z, deep);
    pbr_input.material.base_color = vec4<f32>(c, pbr_input.material.base_color.a);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    // Squared, so most of the way it is the stone's own and only near the sheet
    // does it go, all the way by the sheet itself.
    let lap = clamp((stone.lap.w - y) / LAP_M, 0.0, 1.0);
    out.color = vec4<f32>(mix(out.color.rgb, stone.lap.rgb, lap * lap), out.color.a);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

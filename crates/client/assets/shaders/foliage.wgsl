// Foliage, main pass (`look/foliage.rs`): `StandardMaterial` with a vertex
// stage that moves the grass and the ground clutter (wind, trails, a
// blast's shock, the distance fade) and a fragment stage that is
// `bevy_pbr`'s own plus one cut (cards seen edge-on fade out) and the base
// colour read through `alpha_cut`, as the prepass reads it. After Gates'
// assets/shaders/foliage.wgsl (MIT, NOTICE). The prepass twin is
// `foliage_prepass.wgsl`; both call `foliage_common`.
//
// The fragment is a hand copy of Bevy 0.18.1's `pbr.wgsl` without
// `apply_decals` and the OIT, forward-decal, deferred and meshlet branches:
// a Bevy upgrade re-checks it.
#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    view_transformations::position_world_to_clip,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::{STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT, STANDARD_MATERIAL_FLAGS_UNLIT_BIT},
    pbr_bindings,
}
#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither
#endif
#import "shaders/foliage_common.wgsl"::{displace, grazing_keep}
#import "shaders/alpha_cut.wgsl"::{cut_read, cut_uv}

@vertex
fn vertex(in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    let wp0 = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(in.position, 1.0)).xyz;
#ifdef VERTEX_UVS_B
    // Baked in world space (`grass.rs`, `clutter.rs`): UV1 carries the
    // height above the piece's root (negative: a rigid piece and how deep
    // it sinks) and its own random number, over whole steps of how dry a
    // card is (`grass::DRY_STEPS`), which the fragment reads.
    let h = in.uv_b.x;
    let rand = fract(in.uv_b.y);
#else
    // Nothing to say how it stands: it stands still.
    let h = 0.0;
    let rand = 0.0;
#endif
    let wp = wp0 + displace(wp0, wp0, h, rand);

    out.world_position = vec4<f32>(wp, 1.0);
    out.position = position_world_to_clip(wp);
#ifdef VERTEX_NORMALS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(in.normal, in.instance_index);
#endif
#ifdef VERTEX_UVS_A
    out.uv = in.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = in.uv_b;
#endif
#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local,
        in.tangent,
        in.instance_index
    );
#endif
#ifdef VERTEX_COLORS
    out.color = in.color;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = in.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        in.instance_index, world_from_local[3]);
#endif
    return out;
}

@fragment
fn fragment(vertex_output: VertexOutput) -> FragmentOutput {
    var in = vertex_output;
    // First, while every invocation is still live: both take derivatives.
    let keep = grazing_keep(in.world_position.xyz);
#ifdef VERTEX_UVS_A
#ifdef STANDARD_MATERIAL_BASE_COLOR_UV_B
    let uv = cut_uv(in.uv_b);
#else
    let uv = cut_uv(in.uv);
#endif
#else
    let uv = vec2<f32>(0.0);
#endif
    let duv = vec4(dpdx(uv), dpdy(uv));

    // Every face lit as the front: a card's normal is pulled toward its
    // tuft's volume (`grass::BLADE_TIP_BLEND`), which is the same normal
    // seen from either side, so flipping it for the back face would light
    // the far side of a tuft from inside.
    var pbr_input = pbr_input_from_standard_material(in, true);
    // The base colour again, read the way the prepass cuts by (`alpha_cut`).
#ifdef VERTEX_UVS_A
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT) != 0u {
        var c = pbr_bindings::material.base_color;
#ifdef VERTEX_COLORS
        c *= in.color;
#endif
        var tex = cut_read(uv, duv);
#ifdef VERTEX_UVS_B
        // A dried card: the photograph goes to its own grey by the whole
        // steps on UV1.y (`grass::DRY_STEPS`), and the vertex colour tints
        // that grey straw; a green photograph tinted straw keeps its purple
        // seed heads, pink.
        let dry = floor(in.uv_b.y) / 15.0;
        tex = vec4<f32>(mix(tex.rgb, vec3<f32>(dot(tex.rgb, vec3<f32>(0.2126, 0.7152, 0.0722))), dry), tex.a);
#endif
        pbr_input.material.base_color = c * tex;
    }
#endif

#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif

    // An opaque material ignores alpha in `alpha_discard`, so the clutter
    // that shares this shader is never cut.
    pbr_input.material.base_color.a *= keep;
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

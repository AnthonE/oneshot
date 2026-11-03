// The ground (look/ground.rs): four texture sets, blended by the splat
// weights in the vertex colour (sand, grass, gravel, rock), each tiled in
// world x and z at its own size, then Bevy's PBR lighting. After Gates'
// shaders/ground_splat.wgsl (MIT, NOTICE).

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var albedo_tex: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var albedo_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var normal_tex: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var normal_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var<uniform> tile_m: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var<uniform> tint: array<vec4<f32>, 4>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    var w = vec4<f32>(1.0, 0.0, 0.0, 0.0);
#ifdef VERTEX_COLORS
    w = in.color;
#endif
    w = w / max(w.x + w.y + w.z + w.w, 1e-4);

    // Every set sampled every time: a sample under a branch breaks the
    // derivatives mip selection needs.
    let p = in.world_position.xz;
    var c = vec3<f32>(0.0);
    var n = vec3<f32>(0.0);
    for (var i = 0; i < 4; i += 1) {
        let uv = p / tile_m[i];
        c += textureSample(albedo_tex, albedo_smp, uv, i).rgb * tint[i].rgb * w[i];
        n += (textureSample(normal_tex, normal_smp, uv, i).rgb * 2.0 - 1.0) * w[i];
    }

    // The map's normal, laid on the ground: u runs along world x, v along
    // world z, around the mesh's own normal.
    let N = normalize(pbr_input.world_normal);
    let T = normalize(vec3<f32>(1.0, 0.0, 0.0) - N * N.x);
    let B = cross(N, T);
    let nt = normalize(vec3<f32>(n.xy, max(n.z, 0.05)));
    pbr_input.N = normalize(T * nt.x - B * nt.y + N * nt.z);

    pbr_input.material.base_color = vec4<f32>(c, 1.0);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

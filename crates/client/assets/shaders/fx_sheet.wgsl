// A flipbook pool's sprites (look/fx/sheets.rs): two baked sheets, the
// frame and the next crossfaded, lit six ways in the sprite's own frame by
// the sun and the ambient, shadowed where the sun is (FX_SHADOWS), faded
// against whatever stands behind (DEPTH_PREPASS), eroded as it ages, and
// its heat added through a ramp. Premultiplied out.
//
// Vertex streams, as the pool writes them: the sprite's right axis as the
// normal, the next frame's uvs as uv_b, [blend, age, tone, glow] as the
// tangent, the tint as the colour.

#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    view_transformations::{position_world_to_clip, depth_ndc_to_view_z},
    mesh_view_bindings::{view, lights},
}
#ifdef DEPTH_PREPASS
#import bevy_pbr::prepass_utils::prepass_depth
#endif
#ifdef FX_SHADOWS
#import bevy_pbr::shadows::fetch_directional_shadow
#endif
#ifdef DISTANCE_FOG
#import bevy_pbr::mesh_view_bindings::fog
#import bevy_pbr::pbr_functions::apply_fog
#endif

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var sheet_a: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var sheet_a_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var sheet_b: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var sheet_b_sampler: sampler;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
#ifdef VERTEX_NORMALS
    // The sprite's right axis, already in the world (the mesh only moves).
    out.world_normal = vertex.normal;
#endif
#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
#ifdef VERTEX_TANGENTS
    out.world_tangent = vertex.tangent;
#endif
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif
    return out;
}

// What heat `h` (0–1) glows, HDR: powder and fuel (tone 0), ion blue (1),
// the cannon's green (2), the SHARD GUN's pink (3); between two, a blend.
fn ramp(h: f32, tone: f32) -> vec3<f32> {
    let fire = vec3<f32>(smoothstep(0.06, 0.3, h), smoothstep(0.12, 0.7, h) * 0.85, smoothstep(0.5, 1.0, h) * 0.7);
    let blue = vec3<f32>(smoothstep(0.55, 1.0, h) * 0.9, smoothstep(0.25, 0.8, h) * 0.9, smoothstep(0.0, 0.3, h));
    let green = vec3<f32>(smoothstep(0.5, 1.0, h) * 0.7, smoothstep(0.0, 0.3, h), smoothstep(0.3, 0.9, h) * 0.5);
    let pink = vec3<f32>(smoothstep(0.0, 0.3, h), smoothstep(0.45, 1.0, h) * 0.7, smoothstep(0.1, 0.6, h) * 0.85);
    let t = clamp(tone, 0.0, 3.0);
    var c = mix(fire, blue, clamp(t, 0.0, 1.0));
    c = mix(c, green, clamp(t - 1.0, 0.0, 1.0));
    c = mix(c, pink, clamp(t - 2.0, 0.0, 1.0));
    // Brighter than linear in the heat: a core burns white, an edge
    // smoulders, and what is barely warm gives nothing to the soot.
    return c * (0.12 + 3.9 * h * h);
}

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    var aux = vec4<f32>(0.0, 0.0, 0.0, 1.0);
#ifdef VERTEX_TANGENTS
    aux = in.world_tangent;
#endif
    var tint = vec4<f32>(1.0);
#ifdef VERTEX_COLORS
    tint = in.color;
#endif
    var uv = vec2<f32>(0.0);
    var uv_b = vec2<f32>(0.0);
#ifdef VERTEX_UVS_A
    uv = in.uv;
#endif
#ifdef VERTEX_UVS_B
    uv_b = in.uv_b;
#endif
    let blend = aux.x;
    let age = aux.y;
    let a = mix(textureSample(sheet_a, sheet_a_sampler, uv), textureSample(sheet_a, sheet_a_sampler, uv_b), blend);
    let b = mix(textureSample(sheet_b, sheet_b_sampler, uv), textureSample(sheet_b, sheet_b_sampler, uv_b), blend);

    // Coverage: the sheet's, eroded from its thin parts as the puff ages
    // past params.y of its life, and the recipe's own fade.
    let erode = clamp((age - params.y) / max(1.0 - params.y, 1e-3), 0.0, 1.0) * 1.3;
    let cover = a.a * smoothstep(erode - 0.35, erode + 0.02, a.a);
    var alpha = cover * tint.a;

    // Against the wall behind: nothing in the last params.x metres before it.
    var soft = 1.0;
#ifdef DEPTH_PREPASS
    let scene = prepass_depth(in.position, 0u);
    let dz = depth_ndc_to_view_z(in.position.z) - depth_ndc_to_view_z(scene);
    soft = clamp(dz / max(params.x, 1e-3), 0.0, 1.0);
#endif

    // Six-way lighting in the sprite's frame: right, up and toward the eye.
    let right = normalize(in.world_normal);
    let to_eye = normalize(view.world_position.xyz - in.world_position.xyz);
    let up = normalize(cross(to_eye, right));
    var sun = vec3<f32>(0.0);
    var to_sun = vec3<f32>(0.0, 1.0, 0.0);
    if (lights.n_directional_lights > 0u) {
        to_sun = lights.directional_lights[0].direction_to_light;
        sun = lights.directional_lights[0].color.rgb * view.exposure;
    }
    let l = vec3<f32>(dot(to_sun, right), dot(to_sun, up), dot(to_sun, to_eye));
    let lit = a.r * max(l.x, 0.0) + a.g * max(-l.x, 0.0)
        + a.b * max(l.y, 0.0) + b.r * max(-l.y, 0.0)
        + b.g * max(l.z, 0.0) + b.b * max(-l.z, 0.0);
    var shadow = 1.0;
#ifdef FX_SHADOWS
    if (lights.n_directional_lights > 0u) {
        let view_z = depth_ndc_to_view_z(in.position.z);
        shadow = fetch_directional_shadow(0u, in.world_position, to_sun, view_z);
    }
#endif
    let ambient = lights.ambient_color.rgb * view.exposure;
    let open = (a.r + a.g + a.b + b.r + b.g + b.b) / 6.0;
    var smoke = tint.rgb * (sun * lit * shadow + ambient * (0.35 + 0.65 * open));

    // Heat: the ramp, as bright as the recipe asks, faded with the recipe.
    let heat = b.a;
    var glow = ramp(heat, aux.z) * (heat * aux.w * tint.a);

#ifdef DISTANCE_FOG
    // The air between: what it keeps of the light and what it adds, as
    // Bevy's fog does for any surface, on the straight colour.
    let black = apply_fog(fog, vec4<f32>(0.0, 0.0, 0.0, 1.0), in.world_position.xyz, view.world_position.xyz).rgb;
    let white = apply_fog(fog, vec4<f32>(1.0, 1.0, 1.0, 1.0), in.world_position.xyz, view.world_position.xyz).rgb;
    let through = clamp(white - black, vec3<f32>(0.0), vec3<f32>(1.0));
    smoke = smoke * through + black;
    glow = glow * through;
#endif

    alpha = alpha * soft;
    out.color = vec4<f32>(smoke * alpha + glow * soft, alpha);
    return out;
}

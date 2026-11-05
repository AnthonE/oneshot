// The shield's shell (look/shield.rs): a fighter's skinned body again,
// pushed out along its normals, drawn additive in gold: a fresnel rim, a
// crawl of hexagon edges, a band of light sweeping up, all scaled by the
// flare level. The vertex half is Bevy's mesh vertex shader (skinning, no
// morph targets) plus the push.

#import bevy_pbr::{
    mesh_functions,
    skinning,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, globals},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<uniform> params: vec4<f32>;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
#ifdef SKINNED
    let world_from_local = skinning::skin_model(vertex.joint_indices, vertex.joint_weights, vertex.instance_index);
#else
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
#endif
#ifdef VERTEX_NORMALS
#ifdef SKINNED
    out.world_normal = skinning::skin_normals(world_from_local, vertex.normal);
#else
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
#endif
#endif
#ifdef VERTEX_POSITIONS
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
#ifdef VERTEX_NORMALS
    out.world_position = vec4<f32>(out.world_position.xyz + normalize(out.world_normal) * params.x, 1.0);
#endif
    out.position = position_world_to_clip(out.world_position.xyz);
#endif
#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, vertex.tangent, vertex.instance_index);
#endif
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

fn fmod2(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return a - b * floor(a / b);
}

// Distance from `p` to the edge of its hexagon (0 on the edge).
fn hex_edge(p: vec2<f32>) -> f32 {
    let r = vec2<f32>(1.0, 1.7320508);
    let h = r * 0.5;
    let a = fmod2(p, r) - h;
    let b = fmod2(p - h, r) - h;
    var g = b;
    if dot(a, a) < dot(b, b) {
        g = a;
    }
    let q = abs(g);
    let d = max(dot(q, normalize(r)), q.x);
    return 0.5 - d;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var out: FragmentOutput;
    let level = color.a;
    let n = normalize(in.world_normal);
    let v = normalize(view.world_position.xyz - in.world_position.xyz);
    let rim = pow(1.0 - abs(dot(n, v)), 2.2);
    let t = globals.time;
    // Hexagons over the body's own uv, the lattice drifting.
    var uv = in.world_position.xz + in.world_position.yy;
#ifdef VERTEX_UVS_A
    uv = in.uv;
#endif
    let e = hex_edge(uv * params.y + vec2<f32>(t * 0.6, t * 0.35));
    let lines = smoothstep(0.07, 0.0, e);
    // A band of light sweeping up the body, and a flicker across it.
    let band = smoothstep(0.35, 0.0, abs(fract(in.world_position.y * 0.6 - t * 1.4) - 0.5));
    let flicker = 0.75 + 0.25 * sin(t * 47.0 + in.world_position.y * 9.0);
    // params.z takes the fill away (the arms in view: rim and lines only).
    let fill = 1.0 - params.z;
    let k = level * flicker * (rim * 1.5 + lines * (0.1 + rim) * 0.9 + (band * 0.15 + 0.02) * fill);
    // Alpha 0: AlphaMode::Add blends as premultiplied alpha (src + (1 - a)
    // dst), so only a = 0 adds the shell over what is behind it.
    out.color = vec4<f32>(color.rgb * k, 0.0);
    return out;
}

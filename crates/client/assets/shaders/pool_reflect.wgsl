// The architecture over still water, upside down under it (`look/pool.rs`):
// `bevy_pbr`'s own unlit fragment, this vertex stage shaking it a little, the
// further under the sheet the more, as the sheet's ripples would.
//
// A uniform and arithmetic, so it binds and runs the same on WebGL2.

#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput},
    mesh_functions,
    mesh_view_bindings::globals,
    view_transformations::position_world_to_clip,
}

struct PoolReflect {
    // x how far it shakes, m, y over how many metres, zw unused.
    shake: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> pool: PoolReflect;

@vertex
fn vertex(in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    var wp = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(in.position, 1.0)).xyz;
    let deep = clamp(-wp.y / pool.shake.y, 0.0, 1.0);
    let t = globals.time;
    wp.x += sin(wp.z * 1.3 + t * 0.9) * pool.shake.x * deep;
    wp.z += cos(wp.x * 1.1 - t * 0.7) * pool.shake.x * deep;
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
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, in.tangent, in.instance_index);
#endif
#ifdef VERTEX_COLORS
    out.color = in.color;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = in.instance_index;
#endif
    return out;
}

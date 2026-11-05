// A blast's shock shell (look/fx/shell.rs): a sphere's rim, bright where
// it is seen edge on, fading and thinning as it grows. Additive.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> color: vec4<f32>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var out: FragmentOutput;
    let age = color.a;
    let n = normalize(in.world_normal);
    let v = normalize(view.world_position.xyz - in.world_position.xyz);
    // Clamped: a hair over 1 would make pow NaN, and a NaN adds black.
    let facing = clamp(abs(dot(n, v)), 0.0, 1.0);
    // The rim sharpens as the shell thins: a band at birth, a line near
    // the end.
    let sharp = 3.0 + 7.0 * age;
    let rim = pow(max(1.0 - facing, 0.0), sharp);
    // The far side of the sphere adds too (it is a shell of air): half.
    let side = select(0.5, 1.0, is_front);
    let fade = (1.0 - age) * (1.0 - age);
    out.color = vec4<f32>(color.rgb * rim * fade * side, 1.0);
    return out;
}

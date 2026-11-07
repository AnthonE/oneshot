// Still water under a roof, its sheet (`look/pool.rs`): `bevy_pbr`'s own fragment
// over a slow ripple, letting the reflection under it through, less at a glance.
//
// A uniform and arithmetic, so it binds and runs the same on WebGL2.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::{globals, view},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}

struct PoolWater {
    // x through looking down, y at a glance, z the ripple's size, m, w its bend.
    sheet: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> pool: PoolWater;

// The slope of three slow waves going different ways, at (x, z).
fn slope(p: vec2<f32>, t: f32) -> vec2<f32> {
    let a = dot(p, vec2<f32>(0.83, 0.56)) * 1.0 + t * 0.31;
    let b = dot(p, vec2<f32>(-0.42, 0.91)) * 1.7 - t * 0.43;
    let c = dot(p, vec2<f32>(0.21, -0.98)) * 3.1 + t * 0.71;
    return vec2<f32>(0.83, 0.56) * cos(a)
        + vec2<f32>(-0.42, 0.91) * cos(b) * 0.6
        + vec2<f32>(0.21, -0.98) * cos(c) * 0.3;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    // A ripple of its own, or (a wet road) the photograph's.
    var n = pbr_input.N;
    if pool.sheet.w > 0.0 {
        let p = in.world_position.xz / pool.sheet.z;
        let s = slope(p, globals.time) * pool.sheet.w;
        n = normalize(vec3<f32>(-s.x, 1.0, -s.y));
        pbr_input.N = n;
        pbr_input.world_normal = n;
    }

    var out: FragmentOutput;
    let lit = apply_pbr_lighting(pbr_input);
    let to_eye = normalize(view.world_position.xyz - in.world_position.xyz);
    // How much is the sheet and how much what lies under it.
    let glance = 1.0 - clamp(dot(n, to_eye), 0.0, 1.0);
    let through = mix(pool.sheet.x, pool.sheet.y, glance * glance);
    out.color = vec4<f32>(lit.rgb, 1.0 - through);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

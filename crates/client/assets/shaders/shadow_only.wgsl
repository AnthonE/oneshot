// Your own body (`look/own_shadow.rs`, after Gates'
// crates/client/src/render/own_shadow.rs, MIT): drawn by no camera, cast
// in the sun's shadow in full. The material has no prepass, so it is in no
// depth, normal or motion buffer; here, in every camera's own pass, every
// fragment is discarded. The shadow pass draws depth off the vertex stage
// (Bevy's prepass shader, skinned like any body) and never runs this.
#import bevy_pbr::forward_io::VertexOutput

// Never read but by the discard: WebGL2 wants a binding of 16 bytes.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> shadow_only: vec4<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if shadow_only.w >= 0.0 {
        discard;
    }
    return shadow_only;
}

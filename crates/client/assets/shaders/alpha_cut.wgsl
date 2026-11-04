// The base colour an alpha-masked card is cut by, shared by the main pass
// and the depth prepass of the foliage (`look/foliage.rs`), so the two
// passes cut the same pixels. After Gates' assets/shaders/alpha_cut.wgsl
// (MIT, NOTICE).
//
// **Gradients from the top of the fragment stage, never implicit ones.** With
// a depth prepass the main pass draws only where the prepass wrote depth, so a
// card the prepass kept and the main pass cut leaves depth with no colour on
// it: the clear colour shows through, unfogged (dark speckles over the
// grass). Bevy's own reads take their mip level from implicit derivatives,
// and the two passes did not always agree on it. The caller takes
// `dpdx`/`dpdy` of the UV first thing, while every invocation is live, and
// both passes read through here.
#import bevy_pbr::{mesh_view_bindings::view, pbr_bindings}

// The base colour's UV through the material's transform.
fn cut_uv(uv: vec2<f32>) -> vec2<f32> {
    return (pbr_bindings::material.uv_transform * vec3(uv, 1.0)).xy;
}

// The base colour texture at `uv`, with `duv` = (dpdx, dpdy) of `uv`. The
// camera's mip bias (TAA's) scales the gradients as `textureSampleBias`
// would shift the level.
fn cut_read(uv: vec2<f32>, duv: vec4<f32>) -> vec4<f32> {
    let k = exp2(view.mip_bias);
    return textureSampleGrad(
        pbr_bindings::base_color_texture,
        pbr_bindings::base_color_sampler,
        uv,
        duv.xy * k,
        duv.zw * k,
    );
}

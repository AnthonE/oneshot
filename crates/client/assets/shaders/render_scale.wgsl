// The world image, drawn under the UI at the window's size (render_scale.rs,
// the RENDER SCALE row). After Gates' assets/shaders/render_scale.wgsl (MIT).
#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var frame: texture_2d<f32>;
@group(1) @binding(1) var frame_sampler: sampler;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    // The atmosphere leaves the sky's alpha at zero; its RGB is already the
    // finished sky, so the presentation must not blend by it.
    return vec4(textureSample(frame, frame_sampler, in.uv).rgb, 1.0);
}

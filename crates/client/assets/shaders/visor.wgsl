// The HUD on the inside of a visor (visor.rs): the HUD's own picture,
// premultiplied as its camera drew it over a clear image, bent as a dome of
// glass bends it (the middle swells, the corners stay), a fringe of its
// colours toward the rim, a halo of its own light round everything it
// draws, faint lines across it, and the whole of it lagging the head away
// from the middle.
#import bevy_ui::ui_vertex_output::UiVertexOutput

struct Visor {
    // x the bend, y the split, z the glow, w its reach in px.
    look: vec4<f32>,
    // xy the lag (uv), z the width over the height, w the lines.
    lag: vec4<f32>,
    // xy one px in uv.
    px: vec4<f32>,
}

@group(1) @binding(0) var hud: texture_2d<f32>;
@group(1) @binding(1) var hud_sampler: sampler;
@group(1) @binding(2) var<uniform> v: Visor;

// Where on the HUD's picture this point of the screen shows, its colours
// split by `k` (red out, blue in).
fn bent(uv: vec2<f32>, k: f32) -> vec2<f32> {
    let aspect = vec2<f32>(v.lag.z, 1.0);
    let p = (uv - 0.5) * aspect;
    let r2 = dot(p, p);
    // The corners' r², so they stay where they are.
    let rc2 = dot(aspect * 0.5, aspect * 0.5);
    let s = (1.0 + v.look.x * r2) / (1.0 + v.look.x * rc2) * (1.0 + k * r2);
    // The lag, none in the middle, all of it past a third of the way out.
    let away = smoothstep(0.1, 0.33, sqrt(r2));
    return p * s / aspect + 0.5 - v.lag.xy * away;
}

fn at(uv: vec2<f32>) -> vec4<f32> {
    let inside = all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
    return select(vec4<f32>(0.0), textureSampleLevel(hud, hud_sampler, uv, 0.0), inside);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let split = v.look.y;
    let mid = at(bent(in.uv, 0.0));
    let red = at(bent(in.uv, split));
    let blue = at(bent(in.uv, -split));
    var c = vec4<f32>(red.r, mid.g, blue.b, max(mid.a, max(red.a, blue.a)));

    // The halo: two rings of taps round the point, their light added under
    // what is drawn here.
    if (v.look.z > 0.0) {
        let uv = bent(in.uv, 0.0);
        let r = v.look.w * v.px.xy;
        var sum = vec4<f32>(0.0);
        for (var i = 0; i < 8; i++) {
            let a = f32(i) * 0.7853982;
            let d = vec2<f32>(cos(a), sin(a));
            sum += at(uv + d * r) * 0.6;
            sum += at(uv + d * r * 0.45) + at(uv + vec2<f32>(-d.y, d.x) * r * 0.75) * 0.8;
        }
        let glow = sum / (8.0 * 2.4) * v.look.z;
        c = c + glow * (1.0 - c.a);
    }

    // The lines across it, only where it draws.
    let row = in.uv.y / v.px.y;
    c = vec4<f32>(c.rgb * (1.0 - v.lag.w * (0.5 + 0.5 * cos(row * 2.0943951))), c.a);

    // Out of premultiplied, for the UI's blend.
    let a = clamp(c.a, 0.0, 1.0);
    return vec4<f32>(c.rgb / max(a, 1e-4), a);
}

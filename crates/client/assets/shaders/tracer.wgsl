// Tracers (look/fx/tracers.rs): a round's line of light, a capsule in
// screen space. Every corner of a line's quad carries both its ends, so all
// four project the same segment; the quad is spread round it in pixels, and
// each pixel is shaded by its distance to the segment. Its half-width is the
// world's, held between a floor and a ceiling in pixels: under the floor the
// line dims instead of thinning, so far off it is a faint hair, not a dotted
// shimmer, and near the eye it is a few pixels, not a fat carrot. Across, a
// hot core in a softer glow of the round's colour; along, dark at the tail
// and brightest at the head, where a hot dot sits.
//
// A wake (a heavy round's vapour) takes the same capsule, wider and
// unclamped near the eye, covering rather than adding, broken up along its
// length more as it ages.
//
// Premultiplied: a line writes alpha 0, so it only adds.

#import bevy_pbr::{
    mesh_functions,
    mesh_view_bindings::{view, globals},
}

struct Knobs {
    // x: every line's brightness, times.
    k: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> knobs: Knobs;

// The tail's half-width, a share of the head's.
const TAIL_W: f32 = 0.55;
// A line's half-width in pixels at 1080p: never under (it dims instead),
// never over its own most (`tracers::Tracer::px`, in the glow's alpha).
const MIN_HW_PX: f32 = 0.9;
// A wake's: smoke near the eye is as wide as it looks.
const WAKE_MAX_HW_PX: f32 = 90.0;
// The quad round the segment, in half-widths, and a pixel and a half more.
const SPAN: f32 = 3.6;
const WAKE_SPAN: f32 = 2.2;
// The core: a gaussian this share of the half-width (and never under a
// third of a pixel); the glow: this many half-widths, this bright.
const CORE: f32 = 0.45;
const CORE_MIN_PX: f32 = 0.35;
const HALO: f32 = 2.2;
const HALO_K: f32 = 0.8;
// How the light falls from the head back to the tail.
const TAIL_POW: f32 = 1.6;
// The head's dot, in half-widths.
const HEAD_R: f32 = 1.4;
// A wake: how much of it a breath of noise takes away as it ages, the
// noise's scale along it (per metre), how fast it drifts, and how far it
// wanders off its line as it ages, in half-widths.
const WAKE_BREAK: f32 = 0.85;
const WAKE_NOISE_M: f32 = 2.2;
const WAKE_DRIFT: f32 = 0.35;
const WAKE_WAVE: f32 = 1.4;
const WAKE_WAVE_M: f32 = 0.25;

struct VertexIn {
    @builtin(instance_index) instance_index: u32,
    // The tail and the head, relative to the mesh.
    @location(0) tail: vec3<f32>,
    @location(1) head: vec3<f32>,
    // Side (-1, 1), end (0 tail, 1 head), half-width at the head (m), the
    // metres it takes off its start to come up to full.
    @location(2) corner: vec4<f32>,
    // Metres off its start at the tail and at the head, the head's dot, a
    // wake's age (negative: a line).
    @location(3) along: vec4<f32>,
    // rgb the core, a how bright (0: nothing).
    @location(4) core: vec4<f32>,
    // rgb the glow, a a line's widest (half, pixels at 1080p) or a wake's
    // seed.
    @location(5) edge: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    // Both ends in pixels off the view's middle, y up: tail xy, head zw.
    @location(0) ends: vec4<f32>,
    // Half-widths in pixels at the tail and the head, and how much of the
    // floor each fills (under 1: dimmed).
    @location(1) widths: vec4<f32>,
    @location(2) core: vec4<f32>,
    @location(3) edge: vec4<f32>,
    // x 0 at the tail to 1 at the head, y metres off its start (both along
    // the world, not the screen), z the head's dot, w a wake's age.
    @location(4) along: vec4<f32>,
    // x the fade-in's metres.
    @location(5) misc: vec4<f32>,
}

fn pixels(c: vec4<f32>, half: vec2<f32>) -> vec2<f32> {
    return c.xy / c.w * half;
}

@vertex
fn vertex(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    let tw = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(in.tail, 1.0));
    let hw = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(in.head, 1.0));
    let ct = view.clip_from_world * tw;
    let ch = view.clip_from_world * hw;
    if in.core.a <= 0.0 || ct.w <= 0.0 || ch.w <= 0.0 {
        out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        return out;
    }
    let half = view.viewport.zw * 0.5;
    let st = pixels(ct, half);
    let sh = pixels(ch, half);
    let wake = in.along.w >= 0.0;
    // Pixels a metre covers at a depth of one, and the frame against 1080p.
    let k = view.clip_from_view[1][1] * half.y;
    let res = view.viewport.w / 1080.0;
    let lo = MIN_HW_PX * res;
    let hi = max(select(in.edge.a, WAKE_MAX_HW_PX, wake) * res, lo);
    let raw_h = in.corner.z * k / ch.w;
    let raw_t = in.corner.z * select(TAIL_W, 1.0, wake) * k / ct.w;
    let w_t = clamp(raw_t, lo, hi);
    let w_h = clamp(raw_h, lo, hi);

    var d = sh - st;
    let l = length(d);
    d = select(vec2<f32>(1.0, 0.0), d / max(l, 1e-4), l > 1e-3);
    let n = vec2<f32>(-d.y, d.x);
    let at_head = in.corner.y > 0.5;
    let me = select(st, sh, at_head);
    let c = select(ct, ch, at_head);
    let span = select(w_t, w_h, at_head) * select(SPAN, WAKE_SPAN, wake) + 1.5;
    let p = me + n * in.corner.x * span + d * select(-1.0, 1.0, at_head) * span;
    out.clip = vec4<f32>(p / half * c.w, c.z, c.w);

    out.ends = vec4<f32>(st, sh);
    out.widths = vec4<f32>(w_t, w_h, min(raw_t / lo, 1.0), min(raw_h / lo, 1.0));
    out.core = in.core;
    out.edge = in.edge;
    out.along = vec4<f32>(in.corner.y, mix(in.along.x, in.along.y, in.corner.y), in.along.z, in.along.w);
    out.misc = vec4<f32>(in.corner.w, 0.0, 0.0, 0.0);
    return out;
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y,
    );
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let half = view.viewport.zw * 0.5;
    let q = vec2<f32>(in.clip.x - view.viewport.x - half.x, half.y - (in.clip.y - view.viewport.y));
    let a = in.ends.xy;
    let b = in.ends.zw;
    let ba = b - a;
    let qa = q - a;
    let h = clamp(dot(qa, ba) / max(dot(ba, ba), 1e-4), 0.0, 1.0);
    var r = length(qa - ba * h);
    let w = mix(in.widths.x, in.widths.y, h);
    let floor_fill = mix(in.widths.z, in.widths.w, h);
    let fade_in = smoothstep(0.0, max(in.misc.x, 1e-3), in.along.y);
    let bright = in.core.a * floor_fill * fade_in * knobs.k.x;

    if in.along.w >= 0.0 {
        // A wake: soft across, wandering off its line and torn along it as
        // it ages, drifting.
        let t = in.along.w;
        let side = dot(qa, vec2<f32>(-ba.y, ba.x)) / max(length(ba), 1e-4);
        let wave = (vnoise(vec2<f32>(in.along.y * WAKE_WAVE_M + in.edge.a, globals.time * WAKE_DRIFT * 0.5)) - 0.5) * 2.0;
        let lb = max(length(ba), 1e-4);
        let on = dot(qa, ba) / lb;
        let past = max(max(-on, on - lb), 0.0);
        r = length(vec2<f32>(side - wave * WAKE_WAVE * w * sqrt(t), past));
        let across = r / w;
        let body = exp(-across * across * 1.3);
        let m = vnoise(vec2<f32>(in.along.y * WAKE_NOISE_M + in.edge.a, globals.time * WAKE_DRIFT + across * 0.6));
        let torn = smoothstep(0.15 + t * WAKE_BREAK, 0.45 + t * WAKE_BREAK, m);
        let cover = body * torn * bright;
        return vec4<f32>(in.core.rgb * cover, cover);
    }

    let core = exp(-pow(r / max(CORE * w, CORE_MIN_PX), 2.0));
    let halo = exp(-pow(r / (HALO * w), 2.0)) * HALO_K;
    let lit = pow(clamp(in.along.x, 0.0, 1.0), TAIL_POW);
    let dot_r = length(q - b) / (HEAD_R * in.widths.y + 0.5);
    let head = exp(-dot_r * dot_r) * in.along.z;
    let rgb = in.core.rgb * (core * lit + head) + in.edge.rgb * halo * lit;
    return vec4<f32>(rgb * bright, 0.0);
}

// The sword's blade (look/blade.rs): hard light, no surface of its own. Mostly
// see-through; what you read is its drawing: a double line round its edges and
// a spine, scanlines crawling to its tip, foil colours that turn as the view
// does, and columns of data hanging at depths inside it, which slide against
// each other as it moves because they are looked at through the blade. Now and
// then a slice of it jumps sideways, as a projection does. When it is new it
// draws itself out of the hilt. WebGL2 runs it: no compute, no textures.
//
// Round it, an aura: a strip down its middle line (`blade.rs`) turned to face
// the eye, so its glow has no edge from any side, and the pixels it sheds.
//
// Behind a blade that moves fast, ghosts: the blade again where it just was,
// only its colours and its edges, fading.

#import bevy_pbr::{
    mesh_functions,
    mesh_view_bindings::{view, globals},
    view_transformations::position_world_to_clip,
}

struct Blade {
    // rgb: its edges' and spine's light, linear; w: how bright overall.
    ice: vec4<f32>,
    // rgb: its inner line's; w: how much of its body the foil's colours are.
    inner: vec4<f32>,
    // x: 0 the blade itself, 1 an aura, 2 a ghost; y: how long a ghost lasts.
    mode: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> blade: Blade;

const TAU: f32 = 6.2831853;
// `globals.time` wraps here.
const CLOCK_WRAP_S: f32 = 3600.0;
// How long it takes to draw itself out, and how bright its front is as it does.
const DRAW_S: f32 = 0.34;
const DRAW_FRONT: f32 = 5.0;
// Its body: dark glass. How much of what is behind it it hides, how bright it is.
const BODY_HIDES: f32 = 0.42;
const BODY_GAIN: f32 = 0.13;
// Scanlines the length of it, and how many pass a point in a second.
const LINES: f32 = 190.0;
const LINES_PER_S: f32 = 7.0;
// Bright bands running to the tip: a blade's lengths in a second, and how many at once.
const BAND_RATE: f32 = 0.62;
const BAND_FREQ: f32 = 1.3;
// The data: columns a metre, rows a metre, how many are lit, the first layer's depth, metres.
const COLUMNS_M: f32 = 230.0;
const ROWS_M: f32 = 76.0;
const DATA_OFF: f32 = 0.78;
const DEPTH_M: f32 = 0.012;
// Slices jump in bursts: how many along it, how often a burst may start and
// how many of those do, how often the dice roll in one, how many jump, how far.
const SLICES: f32 = 21.0;
const BURST_PER_S: f32 = 3.7;
const BURST_SHARE: f32 = 0.11;
const SLICE_PER_S: f32 = 19.0;
const SLICE_SHARE: f32 = 0.22;
const SLICE_M: f32 = 0.035;
// Rays of the projection, fanning from the hilt: how many across it, how fast they shift.
const RAYS: f32 = 11.0;
const RAYS_PER_S: f32 = 0.35;
// An aura: how bright; its pixels, cells a metre, how many are lit, how bright.
const AURA_GAIN: f32 = 0.62;
const MOTE_M: f32 = 185.0;
const MOTE_OFF: f32 = 0.955;
const MOTE_GAIN: f32 = 2.6;
// A ghost: how bright its body and its edges are.
const GHOST_BODY: f32 = 0.055;
const GHOST_EDGE: f32 = 0.42;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) eye: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) born: f32,
    // An aura: how far off its middle line, metres.
    @location(5) off_m: f32,
};

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

@vertex
fn vertex(v: Vertex) -> Out {
    var out: Out;
    let i = v.instance_index;
    let eye = (mesh_functions::get_local_from_world(i) * vec4<f32>(view.world_position, 1.0)).xyz;
    var p = v.position;
    var uv = v.uv;
    if blade.mode.x > 0.5 && blade.mode.x < 1.5 {
        // Its strip turned about the blade's length to face the eye; its
        // half-width came in `uv.x`.
        let across = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), eye - p) + vec3<f32>(1e-5, 0.0, 0.0));
        p += across * v.uv.x;
        uv.x = sign(v.uv.x);
        out.off_m = v.uv.x;
    } else if blade.mode.x < 0.5 {
        let band = floor(v.uv.y * SLICES);
        let slot = floor(globals.time * SLICE_PER_S);
        let burst = step(1.0 - BURST_SHARE, hash(vec2<f32>(floor(globals.time * BURST_PER_S), 5.0)));
        let jumps = burst * step(1.0 - SLICE_SHARE, hash(vec2<f32>(band, slot)));
        p.z += jumps * (hash(vec2<f32>(band + 7.0, slot)) - 0.5) * SLICE_M * v.uv.y;
    }
    let world = mesh_functions::mesh_position_local_to_world(mesh_functions::get_world_from_local(i), vec4<f32>(p, 1.0));
    out.clip = position_world_to_clip(world.xyz);
    out.local = p;
    out.normal = v.normal;
    out.eye = eye;
    out.uv = uv;
    out.born = bitcast<f32>(mesh_functions::get_tag(i));
    return out;
}

// A line `half` wide at x = 0, never thinner than a pixel.
fn stroke(x: f32, half: f32) -> f32 {
    let w = max(half, fwidth(x));
    return 1.0 - smoothstep(w * 0.5, w * 1.5, abs(x));
}

// How much a pattern with `cells` this many to a pixel is still seen.
fn resolved(per_pixel: f32) -> f32 {
    return 1.0 - smoothstep(0.25, 0.7, per_pixel);
}

// One layer of data: dashes in columns, each column drifting at its own speed.
fn data(p: vec2<f32>, t: f32) -> f32 {
    let x = p.x * COLUMNS_M;
    let col = floor(x);
    let speed = mix(0.7, 3.4, hash(vec2<f32>(col, 3.0)));
    let y = p.y * ROWS_M - t * speed;
    let row = floor(y);
    let on = step(DATA_OFF, hash(vec2<f32>(col, row)));
    let fx = fract(x);
    let fy = fract(y);
    return on * smoothstep(0.08, 0.2, fx) * (1.0 - smoothstep(0.8, 0.92, fx))
        * smoothstep(0.04, 0.12, fy) * (1.0 - smoothstep(0.88, 0.96, fy));
}

// The foil: thin-film colours, cyan through periwinkle to magenta and back for
// a turn of `phase`, always with its blue in it: never a sticker's whole rainbow.
fn foil(phase: f32) -> vec3<f32> {
    return vec3<f32>(
        0.45 + 0.55 * cos(TAU * (phase + 0.55)),
        0.5 + 0.5 * cos(TAU * (phase + 0.05)),
        0.85 + 0.15 * cos(TAU * (phase + 0.75)),
    );
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let t = globals.time;
    let u = in.uv.x;
    let v = in.uv.y;

    var age = t - in.born;
    if age < 0.0 {
        age += CLOCK_WRAP_S;
    }
    // It draws itself out of the hilt.
    var drawn = 1.0;
    if blade.mode.x < 1.5 {
        drawn = clamp(age / DRAW_S, 0.0, 1.0);
    }
    let reach = 1.0 - (1.0 - drawn) * (1.0 - drawn);
    if v > reach * 1.02 {
        discard;
    }
    let front = (1.0 - drawn) * smoothstep(reach - 0.09, reach, v);

    let ray = normalize(in.eye - in.local);
    let n = normalize(in.normal);
    let facing = abs(dot(n, ray));
    let film = foil(facing * 1.5 + v * 1.25 + u * 0.3 - t * 0.09);
    let waking = mix(1.0, 0.6 + 0.4 * sin(t * 83.0), 1.0 - drawn);

    // An aura: only glow, gone at its edge, and the pixels it sheds, drifting
    // to the tip each column at its own speed. It is what the blade has where a
    // rung has no bloom.
    if blade.mode.x > 0.5 && blade.mode.x < 1.5 {
        let off = abs(u);
        let ends = smoothstep(0.0, 0.1, v);
        let glow = mix(blade.ice.rgb, film, 0.5) * pow(1.0 - off, 2.6) * (1.0 - v * 0.45) * ends;
        let beat = 0.88 + 0.12 * sin(v * 21.0 - t * 6.0);
        let x = in.off_m * MOTE_M;
        let col = floor(x);
        let y = in.local.y * MOTE_M - t * mix(3.0, 11.0, hash(vec2<f32>(col, 9.0)));
        let row = floor(y);
        let lit = step(MOTE_OFF, hash(vec2<f32>(col, row)));
        let spark = lit * step(0.3, fract(x)) * step(0.3, fract(y)) * resolved(fwidth(x));
        let twinkle = 0.5 + 0.5 * sin(t * 9.0 + hash(vec2<f32>(row, col)) * TAU);
        let motes = foil(hash(vec2<f32>(col, row + 3.0)) * 0.6 + v) * spark * twinkle * (1.0 - off) * (1.0 - off) * ends * MOTE_GAIN;
        return vec4<f32>((glow * AURA_GAIN * beat + motes) * waking * blade.ice.w, 0.0);
    }

    // A ghost: its colours and edges, gone in a moment.
    if blade.mode.x > 1.5 {
        let left = 1.0 - clamp(age / blade.mode.y, 0.0, 1.0);
        let edge = smoothstep(0.78, 0.98, abs(u));
        let c = film * GHOST_BODY + mix(film, blade.ice.rgb, 0.4) * edge * GHOST_EDGE;
        return vec4<f32>(c * left * left * blade.ice.w, 0.0);
    }

    let tint = mix(blade.ice.rgb, film, blade.inner.w);

    // Scanlines, the data and its bands, gone evenly where they would moire.
    let lines = v * LINES;
    let line_seen = resolved(fwidth(lines));
    let wave = 0.5 + 0.5 * cos(TAU * (lines - t * LINES_PER_S));
    let scan = mix(0.5, wave * wave * wave, line_seen);
    let band_at = fract(v * BAND_FREQ - t * BAND_RATE);
    let band = pow(band_at, 22.0) * (1.0 - smoothstep(0.985, 1.0, band_at));
    // What a foil does as it turns: wide bright bars sliding across it.
    let sheen = pow(0.5 + 0.5 * cos(TAU * (v * 1.9 + u * 0.55 + facing * 2.1 - t * 0.17)), 7.0);

    // The projection's rays, from the hilt along its length, gone by its point.
    let ray_x = u * RAYS + t * RAYS_PER_S;
    let ray_i = floor(ray_x);
    let ray_f = fract(ray_x);
    let ray_n = mix(hash(vec2<f32>(ray_i, 1.0)), hash(vec2<f32>(ray_i + 1.0, 1.0)), ray_f * ray_f * (3.0 - 2.0 * ray_f));
    let rays = smoothstep(0.55, 0.95, ray_n) * pow(1.0 - v, 1.4);

    // Hanging inside the blade: looked at through it, so it slides as the view
    // turns. A little of each layer's colour is its own depth's.
    let deep = max(abs(ray.x), 0.22);
    let data_seen = resolved(fwidth(in.local.z) * COLUMNS_M);
    var hung = vec3<f32>(0.0);
    var hung_a = 0.0;
    for (var k = 0; k < 3; k++) {
        // 1, 3 and 6 of them.
        let d = DEPTH_M * f32((k + 1) * (k + 2)) * 0.5;
        let q = in.local - ray * d / deep;
        let dashes = data(vec2<f32>(q.z, q.y), t + f32(k) * 13.7) * data_seen;
        let c = mix(blade.ice.rgb, foil(f32(k) * 0.22 + v * 0.8 + facing - t * 0.05), 0.6);
        let fade = 1.0 - f32(k) * 0.3;
        hung += c * dashes * fade;
        hung_a += dashes * fade;
    }

    // Its drawing: a hot line round its edges with light falling off inward, a
    // fine second line inside that, and a spine.
    let e = abs(u);
    let outer = smoothstep(0.9, 0.995, e);
    let edge_light = pow(e, 5.0);
    let inner_line = stroke(e - 0.74, 0.01);
    let spine = stroke(u, 0.012);
    let root = 1.0 - smoothstep(0.0, 0.09, v);

    // Its edges keep, and the rest thins out toward its point: it frays into scanlines.
    let tip = smoothstep(0.8, 1.0, v);
    let held = mix(1.0, wave, tip * line_seen) * (1.0 - tip * tip * 0.5);
    let inside = 1.0 - outer;

    var c = tint * BODY_GAIN * (0.3 + scan * 1.6) * inside;
    c += film * sheen * 0.42 * inside;
    c += film * rays * 0.5 * inside;
    c += hung * (0.32 + band * 1.5 + sheen * 0.55) * inside;
    c += film * band * 1.2 * inside;
    c += film * edge_light * 0.75;
    c += blade.inner.rgb * inner_line * (1.1 + band * 2.0);
    c += blade.ice.rgb * (outer * 3.2 + spine * 1.6 + root * 2.2 + pow(1.0 - facing, 3.0) * 0.45);
    c += vec3<f32>(1.0) * front * DRAW_FRONT;

    let shown = held * waking;
    let a = clamp(BODY_HIDES + outer * 0.5 + spine * 0.3 + inner_line * 0.2 + hung_a * 0.08, 0.0, 1.0);
    // Light, with what is behind it held back so it reads on a bright sky.
    return vec4<f32>(c * blade.ice.w * shown, a * shown);
}

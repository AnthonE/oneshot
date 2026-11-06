// A lunge in stop-motion (look/cels.rs), in three modes, all light added to what
// is behind and all on its 24 steps a second, nothing eased between them.
//
// A frame: a flat pane of light hung across the path, a hard outline with a
// hairline inside it and a film's sprocket holes down its long sides.
// A pose: a body again, frozen where it was a step ago, flat foil with a bright rim.
// Lines: over the whole view, from the lunger's own eye: hard streaks at its edge.
//
// A frame and a pose fade by whole steps, breaking up into scanlines as they go.
// WebGL2 runs it: no compute, no textures.

#import bevy_pbr::{
    mesh_functions,
    skinning,
    forward_io::Vertex,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::{view, globals},
}

struct Cel {
    // x: 0 a frame, 1 a pose, 2 lines; y: how long it lasts, seconds; z, w: a
    // frame's width and height, metres.
    knobs: vec4<f32>,
    // rgb: its outline's light, linear; w: how much a frame is filled.
    ice: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> cel: Cel;

const TAU: f32 = 6.2831853;
const STEPS_PER_S: f32 = 24.0;
// `globals.time` wraps here.
const CLOCK_WRAP_S: f32 = 3600.0;
// A frame's outline, its hairline's inset and half-width, metres.
const OUTLINE_M: f32 = 0.022;
const INSET_M: f32 = 0.05;
const HAIR_M: f32 = 0.004;
// Its sprocket holes: their band's distance in from the edge and width, how
// far apart along it, how long each, metres.
const HOLE_IN_M: f32 = 0.075;
const HOLE_W_M: f32 = 0.03;
const HOLE_EVERY_M: f32 = 0.17;
const HOLE_L_M: f32 = 0.09;
// Scanlines a metre, on a frame and a pose.
const SCAN_M: f32 = 34.0;
// How bright a pose's body and its rim are.
const POSE_BODY: f32 = 0.26;
const POSE_RIM: f32 = 1.5;
// Lines: how many round the view, what share are on, where the nearest begins.
const LINE_BINS: f32 = 170.0;
const LINE_ON: f32 = 0.34;
const LINE_FROM: f32 = 0.62;
const LINE_GAIN: f32 = 1.05;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tag: f32,
};

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

// The blade's colours: cyan through periwinkle to magenta for a turn of `phase`.
fn foil(phase: f32) -> vec3<f32> {
    return vec3<f32>(
        0.45 + 0.55 * cos(TAU * (phase + 0.55)),
        0.5 + 0.5 * cos(TAU * (phase + 0.05)),
        0.85 + 0.15 * cos(TAU * (phase + 0.75)),
    );
}

@vertex
fn vertex(v: Vertex) -> Out {
    var out: Out;
    let i = v.instance_index;
    out.uv = v.uv;
    // Lines cover the view, however the camera stands.
    if cel.knobs.x > 1.5 {
        out.clip = vec4<f32>(v.uv.x * 2.0 - 1.0, 1.0 - v.uv.y * 2.0, 0.999, 1.0);
        out.tag = f32(mesh_functions::get_tag(i)) / 255.0;
        return out;
    }
#ifdef SKINNED
    let world_from_local = skinning::skin_model(v.joint_indices, v.joint_weights, i);
    out.normal = skinning::skin_normals(world_from_local, v.normal);
#else
    let world_from_local = mesh_functions::get_world_from_local(i);
    out.normal = mesh_functions::mesh_normal_local_to_world(v.normal, i);
#endif
    let world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    out.world = world.xyz;
    out.clip = position_world_to_clip(world.xyz);
    out.tag = bitcast<f32>(mesh_functions::get_tag(i));
    return out;
}

// The rectangle of `half` about the origin: 1 inside, with a pixel's edge.
fn inside(p: vec2<f32>, half: vec2<f32>) -> f32 {
    let d = half - abs(p);
    let w = max(fwidth(p.x), fwidth(p.y));
    return smoothstep(0.0, w, min(d.x, d.y));
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let t = globals.time;

    if cel.knobs.x > 1.5 {
        let p = (in.uv - 0.5) * vec2<f32>(view.viewport.z / view.viewport.w, 1.0) * 2.0;
        let r = length(p);
        let a = atan2(p.y, p.x) / TAU + 0.5;
        let step_now = floor(t * STEPS_PER_S);
        let bin = floor(a * LINE_BINS);
        let on = step(1.0 - LINE_ON, hash(vec2<f32>(bin, step_now)));
        let begins = LINE_FROM + hash(vec2<f32>(bin + 3.0, step_now)) * 0.65;
        let across = 1.0 - abs(fract(a * LINE_BINS) - 0.5) * 2.0;
        let line = smoothstep(0.5, 0.9, across) * smoothstep(begins, begins + 0.5, r);
        let c = mix(cel.ice.rgb, foil(hash(vec2<f32>(bin, 7.0)) * 0.7 + t * 0.2), 0.45) * line * on;
        return vec4<f32>(c * in.tag * LINE_GAIN, 0.0);
    }

    var age = t - in.tag;
    if age < 0.0 {
        age += CLOCK_WRAP_S;
    }
    let stepped = floor(age * STEPS_PER_S) / STEPS_PER_S;
    let spent = clamp(stepped / cel.knobs.y, 0.0, 1.0);
    if spent >= 1.0 {
        discard;
    }
    let left = 1.0 - spent;
    let hue = foil(in.tag * STEPS_PER_S * 0.137);
    // It breaks up by whole scanlines as it goes, a different few each step.
    let line = floor(in.world.y * SCAN_M + in.uv.x * 3.0);
    let whole = step(spent * spent, hash(vec2<f32>(line, floor(age * STEPS_PER_S))));

    if cel.knobs.x > 0.5 {
        let to_eye = normalize(view.world_position - in.world);
        let rim = pow(1.0 - abs(dot(normalize(in.normal), to_eye)), 1.7);
        let scan = 0.55 + 0.45 * cos(TAU * in.world.y * SCAN_M);
        let c = hue * POSE_BODY * scan + mix(hue, cel.ice.rgb, 0.45) * rim * POSE_RIM;
        return vec4<f32>(c * left * left * whole, 0.0);
    }

    let size = cel.knobs.zw;
    let p = (in.uv - 0.5) * size;
    let half_size = size * 0.5;
    let d = half_size - abs(p);
    let near_edge = min(d.x, d.y);
    let w = max(fwidth(p.x), fwidth(p.y));
    let outline = 1.0 - smoothstep(OUTLINE_M, OUTLINE_M + w, near_edge);
    let hair = 1.0 - smoothstep(HAIR_M, HAIR_M + w, abs(near_edge - INSET_M));
    let along = (fract(p.y / HOLE_EVERY_M + 0.5) - 0.5) * HOLE_EVERY_M;
    let band = vec2<f32>(d.x - HOLE_IN_M - HOLE_W_M * 0.5, along);
    let holes = inside(band, vec2<f32>(HOLE_W_M * 0.5, HOLE_L_M * 0.5));
    let scan = 0.5 + 0.5 * cos(TAU * p.y * SCAN_M);
    let c = cel.ice.rgb * outline * 2.2
        + hue * hair * 1.1
        + hue * holes * 0.9
        + hue * cel.ice.w * (0.5 + scan);
    return vec4<f32>(c * left * left * whole, 0.0);
}

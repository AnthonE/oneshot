// Rain (`look/rain.rs`, after Gates' assets/shaders/rain.wgsl, MIT): one
// mesh of streak quads the vertex stage keeps in a box around the eye. Every
// drop is a random home in the unit box plus how far the rain has fallen at
// its own speed, wrapped into the box around the camera — so the rain
// follows the player for free and never ends at a seam. Drops whose rank is
// over the rain's intensity collapse to nothing, which is how one mesh draws
// a drizzle and a downpour.
//
// How far it has fallen is the fall times `globals.time`, worked out here:
// a map's rain never changes while it is played, so the material is written
// once a map and not once a frame. Gates' snow is left out.
#import bevy_pbr::{
    mesh_view_bindings::{view, globals},
    view_transformations::position_world_to_clip,
}

struct Rain {
    // xyz: fall velocity, m/s (wind included). w: streak length, m.
    fall: vec4<f32>,
    // xyz: the box's half extents, m. w: intensity 0..1.
    box_: vec4<f32>,
    // rgb: the streak's colour, already lit and exposed. w: the radius
    // round the eye put out under a roof (`rain::SHELTER_CLEAR_M`), 0 out.
    look: vec4<f32>,
    // x: a streak's width, m. y: its opacity at its middle. z: 1 a round
    // flake (snow), 0 a streak. w: the shelter's radius (read on the CPU).
    shape: vec4<f32>,
    // The roof the eye stands under, a box in the plan (x0, z0, x1, z1):
    // nothing falls inside it. Empty in the open.
    roof: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> rain: Rain;

struct VertexIn {
    // The drop's home in the unit box.
    @location(0) position: vec3<f32>,
    // The quad corner: x across (-1..1), y along (0 head .. 1 tail).
    @location(2) uv: vec2<f32>,
    // x: rank 0..1 (collapses above the intensity). y: the drop's own phase.
    @location(5) color: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) fade: f32,
    // The drop's own phase: which flakes stand grey against the sky.
    @location(2) phase: f32,
}

// `rain::SPEED_STEPS`: a drop's speed is a whole number of these parts of
// the travel, so the travel's wrap moves it by whole boxes.
const SPEED_STEPS: f32 = 20.0;

@vertex
fn vertex(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    let cam = view.world_position;
    let half = rain.box_.xyz;
    let size = half * 2.0;
    // How far the rain has fallen, wrapped where no drop can see it: every
    // drop's speed is a whole number of twentieths of the travel, so a wrap
    // of twenty boxes moves each one by whole boxes.
    let wrap = size * SPEED_STEPS;
    let travel = rain.fall.xyz * globals.time;
    let drops = travel - wrap * floor(travel / wrap);
    // Each drop's speed varies a little around the fall, so a sheet of rain
    // does not fall in step: 17 to 23 twentieths of the travel.
    let speed = floor(SPEED_STEPS * (0.85 + 0.3 * in.color.y) + 0.5) / SPEED_STEPS;
    let p = in.position * size + drops * speed;
    let rel = p - cam;
    let wrapped = rel - size * floor(rel / size + 0.5);
    let head = cam + wrapped;

    let dir = normalize(rain.fall.xyz);
    let tail = head - dir * rain.fall.w;
    let along = mix(head, tail, in.uv.y);
    let to_cam = normalize(cam - along);
    var side = cross(dir, to_cam);
    let sl = length(side);
    let width = rain.shape.x;
    side = select(vec3(width * 0.5, 0.0, 0.0), side / max(sl, 1e-4) * width * 0.5, sl > 1e-4);

    // Alive: under the intensity, and faded toward the box's walls so the
    // wrap is never seen.
    let alive = select(0.0, 1.0, in.color.x < rain.box_.w);
    let edge = max(max(abs(wrapped.x) / half.x, abs(wrapped.z) / half.z), abs(wrapped.y) / half.y);
    // Under a roof, none near the eye (Gates' shelter): a metre's fade.
    let clear = rain.look.w;
    var open = select(1.0, smoothstep(clear, clear + 1.0, length(wrapped.xz)), clear > 0.0);
    // And none anywhere under that roof: a metre's fade in from its edge.
    let in_roof = min(min(head.x - rain.roof.x, rain.roof.z - head.x), min(head.z - rain.roof.y, rain.roof.w - head.z));
    open = open * (1.0 - smoothstep(0.0, 1.0, in_roof));
    let fade = alive * open * (1.0 - smoothstep(0.7, 1.0, edge));

    var world = along + side * in.uv.x;
    if fade <= 0.0 {
        world = head;
    }
    out.clip = position_world_to_clip(world);
    out.uv = in.uv;
    out.fade = fade;
    out.phase = in.color.y;
    return out;
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let across = 1.0 - abs(in.uv.x);
    var a = in.fade * across * (1.0 - in.uv.y * 0.75) * rain.shape.y;
    if rain.shape.z > 0.5 {
        // A flake: soft and round, brightest at its middle.
        // A flake: soft and round with a firm core. Against an overcast
        // half of them read grey, darker than the sky, as snow does; the
        // rest white.
        let r = length(vec2(in.uv.x, in.uv.y * 2.0 - 1.0));
        a = in.fade * (1.0 - smoothstep(0.45, 1.0, r)) * rain.shape.y;
        let v = select(1.12, 0.55, in.phase < 0.5);
        return vec4(rain.look.rgb * v, a);
    }
    return vec4(rain.look.rgb, a);
}

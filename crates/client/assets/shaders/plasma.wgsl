// The sticky (look/fx/plasma.rs): a ball of plasma, no model. A sphere, its
// silhouette boiling, flowing noise bent by more of itself with dark gaps in it,
// thin white-hot filaments where that noise crosses its middle and a heart that
// burns through, thinning to nothing at its edge (gas has no surface), and a
// corona round it, all HDR for the bloom. The mesh's tag is its heat, 0 to 255:
// how far its fuse has run. It flows faster, burns whiter and strobes as it goes.
// Additive; WebGL2 runs it: no compute, no textures.

#import bevy_pbr::{
    mesh_functions,
    mesh_view_bindings::{view, globals},
    view_transformations::position_world_to_clip,
}

// x: how bright, y: how far its surface boils, as a share of its radius, z: 1
// for its corona, the larger shell of glow round it.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> knobs: vec4<f32>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) local: vec3<f32>,
    @location(3) heat: f32,
};

fn hash(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3<f32>(127.1, 311.7, 74.7))) * 43758.5453);
}

// Value noise, 0 to 1.
fn noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(hash(i), hash(i + vec3<f32>(1.0, 0.0, 0.0)), u.x),
                mix(hash(i + vec3<f32>(0.0, 1.0, 0.0)), hash(i + vec3<f32>(1.0, 1.0, 0.0)), u.x), u.y);
    let b = mix(mix(hash(i + vec3<f32>(0.0, 0.0, 1.0)), hash(i + vec3<f32>(1.0, 0.0, 1.0)), u.x),
                mix(hash(i + vec3<f32>(0.0, 1.0, 1.0)), hash(i + vec3<f32>(1.0, 1.0, 1.0)), u.x), u.y);
    return mix(a, b, u.z);
}

fn fbm(p: vec3<f32>) -> f32 {
    return noise(p) * 0.55 + noise(p * 2.03 + 11.7) * 0.3 + noise(p * 4.1 + 5.3) * 0.15;
}

// Its clock, faster as its fuse runs.
fn flow(heat: f32) -> f32 {
    return globals.time * mix(1.0, 2.6, heat);
}

@vertex
fn vertex(v: Vertex) -> Out {
    var out: Out;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let heat = f32(mesh_functions::get_tag(v.instance_index)) / 255.0;
    let t = flow(heat);
    let boil = noise(v.position * 2.4 + vec3<f32>(0.0, t * 1.9, t * 1.3)) - 0.5;
    // The ball's surface boils; its corona stays round, or its edge shows.
    let p = v.position * (1.0 + (1.0 - knobs.z) * knobs.y * boil * (1.0 + heat));
    let world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(p, 1.0));
    out.world = world.xyz;
    out.clip = position_world_to_clip(world.xyz);
    out.normal = mesh_functions::mesh_normal_local_to_world(v.normal, v.instance_index);
    out.local = v.position;
    out.heat = heat;
    return out;
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let to_eye = normalize(view.world_position - in.world);
    let facing = clamp(dot(normalize(in.normal), to_eye), 0.0, 1.0);
    let t = flow(in.heat);

    // Its corona: only glow, thickest looking through the middle of the shell
    // and gone at its edge, so it has no edge. Its veins of light drift in it.
    if knobs.z > 0.5 {
        let drift = 0.72 + 0.56 * noise(in.local * 3.2 + vec3<f32>(t * 0.9, -t * 0.7, t * 0.5));
        let thick = pow(facing, 2.6) * drift;
        let glow = vec3<f32>(0.05, 0.3, 1.25) + vec3<f32>(0.3, 0.45, 0.6) * in.heat * in.heat;
        return vec4<f32>(glow * thick * knobs.x * mix(1.0, 2.2, in.heat * in.heat), 0.0);
    }

    // The flow: noise bent by more of itself, so it curls.
    let q = in.local * 2.2;
    let bend = vec3<f32>(
        noise(q + vec3<f32>(0.0, 0.0, t * 0.8)),
        noise(q + vec3<f32>(5.2, 1.3, -t * 0.7)),
        noise(q + vec3<f32>(1.7, 9.2, t * 0.6)),
    );
    let field = fbm(q * 1.3 + bend * 2.6 + vec3<f32>(0.0, t * 1.1, 0.0));
    // Clouds of it with dark gaps between.
    let body = smoothstep(0.26, 0.8, field);

    // Filaments, where the field crosses its middle, and finer ones off a
    // second field: thin and white-hot.
    let vein = 1.0 - abs(field * 2.0 - 1.0);
    let fine = 1.0 - abs(fbm(q * 3.1 - bend * 3.4 + vec3<f32>(t * 0.9, 0.0, 3.0)) * 2.0 - 1.0);
    let filament = pow(clamp(vein, 0.0, 1.0), 48.0) + pow(clamp(fine, 0.0, 1.0), 70.0) * 0.8;
    let core = pow(facing, 2.2);
    // It has no surface: it thins out toward its edge, gas, not glass.
    let limb = smoothstep(0.0, 0.62, facing);

    // Saturated all the way up but for the filaments: the tone curve bleaches
    // anything brighter to white, and white reads as a light bulb.
    let deep = vec3<f32>(0.006, 0.035, 0.42);
    let ion = vec3<f32>(0.03, 0.26, 1.15);
    let hot = vec3<f32>(0.85, 1.25, 2.1);

    var c = deep * (0.8 + body * 2.4);
    c += ion * (0.3 + body * 1.1) * core;
    c += hot * filament * (1.6 + core * 3.2);
    // A heart that burns through it.
    c += hot * pow(facing, 6.0) * 0.75;
    // White-hot from the middle as the fuse runs out.
    c = mix(c, hot * (0.5 + body), in.heat * in.heat * core * 0.8);
    c *= limb;

    // It beats, faster and harder toward the end.
    let beat = 1.0 + (0.12 + in.heat * in.heat * 0.6) * sin(globals.time * mix(11.0, 52.0, in.heat));
    let gain = knobs.x * mix(1.0, 2.6, in.heat * in.heat) * beat;
    // No alpha: added to what is behind it (`AlphaMode::Add`).
    return vec4<f32>(c * gain, 0.0);
}

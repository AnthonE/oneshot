// What every foliage surface does (`look/foliage.rs`): the grass cards and
// the ground clutter (`look/clutter.rs`: dry plants that sway, stones and
// fallen twigs that only sink with the distance). After Gates'
// assets/shaders/foliage_common.wgsl (MIT, NOTICE), trimmed to baked
// world-space meshes. Imported by `foliage.wgsl` (main pass) and
// `foliage_prepass.wgsl` (depth, normal and motion prepass), so both passes
// move a vertex by the SAME function. A mismatch is not cosmetic: with a
// depth prepass the main pass tests against the prepass's depth, and a card
// the two place differently draws as holes.
//
// No storage buffer, no compute: two textures read with `textureLoad` and
// `textureSampleLevel` in the vertex stage, which WebGL2 does as well as
// WebGPU and Vulkan, so all three targets move the grass the same way.

#import bevy_pbr::mesh_view_bindings::view

struct FoliageParams {
    // x: bend amplitude, metres at the reference height
    // y: reference height, metres (a tuft, a dry plant)
    // z: sway angular frequency, rad/s
    // w: flutter amplitude, metres at the reference height
    sway: vec4<f32>,
    // x: where the distance fade starts, a share of the cover's distance
    //    (state texel 3: the GRASS DISTANCE row)
    // y: where the last piece has sunk, a share of it (0 = no fade)
    // z: how hard cards seen edge-on are hidden (0 = never)
    fade: vec4<f32>,
    // x: trample strength (0 = ignores trails)
    misc: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> foliage: FoliageParams;
// 4×1 texels written every frame from the CPU (`foliage::update`):
// [0] = (wind dir x, wind dir z, strength, time),
// [1] = (trail window centre x, z, window edge m, the last frame's seconds),
// [2] = (blast x, z, push, seconds since),
// [3] = (the cover's distance m, 0, 0, 0).
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var foliage_state: texture_2d<f32>;
// Trails of flattened grass, world-anchored and wrapping (`foliage::Trample`).
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var trample_map: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var trample_sampler: sampler;

// How far into the distance fade a piece rooted at `root` is, 0..1: each
// piece (its own `rand`) at its own distance between the fade's start and
// end, so the cover thins rather than ending on a line. None in a shadow
// cascade, whose "eye" is the sun.
fn fade_at(root: vec3<f32>, rand: f32) -> f32 {
    let ring = textureLoad(foliage_state, vec2<i32>(3, 0), 0).x;
    let ortho = view.clip_from_view[3].w == 1.0;
    if foliage.fade.y <= 0.0 || ring <= 0.0 || ortho {
        return 0.0;
    }
    let a = foliage.fade.x * ring;
    let span = max((foliage.fade.y - foliage.fade.x) * ring, 0.01);
    let start = a + span * 0.5 * rand;
    let d = distance(root, view.world_position);
    return clamp((d - start) / (span * 0.5), 0.0, 1.0);
}

// The world-space offset for a vertex `h` metres above its piece's root.
// `root` is the vertex's own position before any offset; `anchor` is where
// its sway phase is read; `rand` is per card or per piece, so neighbours do
// not move in lockstep. A NEGATIVE `h` is a rigid piece (a stone, a fallen
// twig): it never sways, and sinks `-h` metres with the distance fade.
fn displace(root: vec3<f32>, anchor: vec3<f32>, h: f32, rand: f32) -> vec3<f32> {
    return displace_at(root, anchor, h, rand, 0.0);
}

// Where `displace` put this vertex a frame ago, for the motion vectors TAA
// reprojects by: a sway with no motion of its own smeared and shimmered.
// The wind is this frame's; only the clock is turned back.
fn displace_prev(root: vec3<f32>, anchor: vec3<f32>, h: f32, rand: f32) -> vec3<f32> {
    let back = textureLoad(foliage_state, vec2<i32>(1, 0), 0).w;
    return displace_at(root, anchor, h, rand, back);
}

// `displace`, `back` seconds ago.
fn displace_at(root: vec3<f32>, anchor: vec3<f32>, h: f32, rand: f32, back: f32) -> vec3<f32> {
    if h < 0.0 {
        return vec3<f32>(0.0, h * fade_at(root, rand), 0.0);
    }
    if h <= 0.0 {
        return vec3<f32>(0.0);
    }
    let s0 = textureLoad(foliage_state, vec2<i32>(0, 0), 0);
    let dir = s0.xy;
    let strength = s0.z;
    let t = s0.w - back;
    let perp = vec2<f32>(-dir.y, dir.x);
    let along = dot(anchor.xz, dir);

    // Gust bands travelling downwind, wavering across it.
    let gust = 0.55 + 0.45 * sin(along * 0.11 - t * 0.9 + sin(dot(anchor.xz, perp) * 0.05 + t * 0.13) * 1.5);
    let k = clamp(h / max(foliage.sway.y, 0.01), 0.0, 2.5);
    let w = foliage.sway.z;
    let osc = sin(t * w + rand * 6.2831 + along * 0.35);
    // A cantilever: the tip travels with the square of its height.
    let bend = foliage.sway.x * k * k * strength * (0.75 * gust + 0.35 * osc);
    // Flutter: fast and small, a different phase every few centimetres.
    let fphase = t * w * 3.7 + dot(root, vec3<f32>(2.1, 1.7, 2.9)) + rand * 6.2831;
    let flutter = foliage.sway.w * k * (0.3 + strength) * sin(fphase);

    var off = vec3<f32>(dir.x * bend + perp.x * flutter, flutter * 0.4, dir.y * bend + perp.y * flutter);

    // A blast's shock (`foliage::update`, texel 2): a front running out at
    // 60 m/s that flattens what it passes and lets it spring back, pushing
    // away from the blast on the same cantilever as the wind.
    var push = 0.0;
    let s2 = textureLoad(foliage_state, vec2<i32>(2, 0), 0);
    if s2.z > 0.0 {
        let rel = anchor.xz - s2.xy;
        let d = length(rel);
        let since = s2.w - back - d / 60.0;
        if since > 0.0 && d < 45.0 {
            let fall = 1.0 - smoothstep(5.0, 45.0, d);
            let p = foliage.sway.x * k * k * s2.z * fall * exp(-since / 0.45) * cos(since * 9.0);
            let away = rel / max(d, 0.5);
            off += vec3<f32>(away.x * p, 0.0, away.y * p);
            push += abs(p);
        }
    }

    // Bending a stem shortens it: drop the tip so it swings on an arc.
    let swing = abs(bend) + push;
    off.y -= min(swing * swing / max(2.0 * h, 0.05), 0.5 * h);

    // Trails: flattened where somebody walked.
    if foliage.misc.x > 0.0 {
        let s1 = textureLoad(foliage_state, vec2<i32>(1, 0), 0);
        let rel = root.xz - s1.xy;
        let reach = s1.z * 0.5;
        let edge = clamp((reach - max(abs(rel.x), abs(rel.y))) * 0.5, 0.0, 1.0);
        let tr = textureSampleLevel(trample_map, trample_sampler, root.xz / max(s1.z, 1.0), 0.0).r
            * edge * foliage.misc.x;
        let a = rand * 6.2831;
        off = off * (1.0 - tr) + vec3<f32>(cos(a) * 0.7 * h, -0.75 * h, sin(a) * 0.7 * h) * tr;
    }

    // Distance fade: each piece sinks to its root at its own distance.
    let f = fade_at(root, rand);
    off = off * (1.0 - f) - vec3<f32>(0.0, h * f, 0.0);
    return off;
}

// How much of a card survives being seen edge-on: 1 face-on, 0 along its
// plane. Called at the TOP of the fragment stage (derivatives), and by both
// passes, so the prepass and the main pass cut the same pixels.
fn grazing_keep(world_position: vec3<f32>) -> f32 {
    let n = cross(dpdx(world_position), dpdy(world_position));
    let ortho = view.clip_from_view[3].w == 1.0;
    let v = view.world_position - world_position;
    let c = abs(dot(n, v)) / max(length(n) * length(v), 1e-8);
    let g = foliage.fade.z;
    let keep = smoothstep(0.06 * g, 0.26 * g, c);
    // Shadow cascades are orthographic: a card casts its shadow at any angle.
    if g < 0.01 || ortho {
        return 1.0;
    }
    return keep;
}

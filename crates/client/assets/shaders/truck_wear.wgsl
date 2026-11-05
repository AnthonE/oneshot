// A truck's wear (`look/truck.rs`): `bevy_pbr`'s own fragment with its steel
// and its trim roughed up before lighting, by how hurt the hull is (its
// health is sent with it, so everyone sees the same truck) and by where it
// was hit. In the truck's own frame, so it all rides with it: scratches
// along it, then dull scuffed patches, dirt thrown up its sills, soot, and
// at the last char with embers in it; a hole and a bare ring where a round
// met it, a scorch where a blast did.
//
// Uniforms and arithmetic only, as the level's weathering is
// (`weathering.wgsl`, whose noise this is), so it binds and runs the same on
// WebGL2.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

struct Wear {
    // The world into the truck's own frame: +X forward, +Y up from the
    // ground, +Z its right, metres.
    to_local: mat4x4<f32>,
    // x how hurt it is (0 whole, 1 nothing left), y how burnt, z its own
    // seed (no two trucks one pattern).
    state: vec4<f32>,
    // Where it was hit, in its own frame; w the mark's reach, m: a round's
    // positive, a blast's negative, 0 none.
    marks: array<vec4<f32>, 16>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> truck: Wear;

// What dirt, soot and char are, linear.
const DIRT: vec3<f32> = vec3<f32>(0.16, 0.115, 0.075);
const SOOT: vec3<f32> = vec3<f32>(0.012, 0.011, 0.010);
const EMBER: vec3<f32> = vec3<f32>(2.2, 0.55, 0.06);

fn hash3(c: vec3<i32>) -> f32 {
    // WGSL will not mix `^` with `*` unparenthesised.
    var h = (bitcast<u32>(c.x) * 0x8da6b343u) ^ (bitcast<u32>(c.y) * 0xd8163841u) ^ (bitcast<u32>(c.z) * 0xcb1ab31fu);
    h = (h ^ (h >> 16u)) * 0x7feb352du;
    h = (h ^ (h >> 15u)) * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return f32(h >> 8u) * (1.0 / 16777216.0);
}

fn vnoise(p: vec3<f32>) -> f32 {
    let i = vec3<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash3(i), hash3(i + vec3<i32>(1, 0, 0)), u.x);
    let b = mix(hash3(i + vec3<i32>(0, 1, 0)), hash3(i + vec3<i32>(1, 1, 0)), u.x);
    let c = mix(hash3(i + vec3<i32>(0, 0, 1)), hash3(i + vec3<i32>(1, 0, 1)), u.x);
    let d = mix(hash3(i + vec3<i32>(0, 1, 1)), hash3(i + vec3<i32>(1, 1, 1)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z);
}

// How much of a noise at `freq` cycles/m a pixel `px` metres across can
// carry: whole at six pixels a cycle, gone at two.
fn band(px: f32, freq: f32) -> f32 {
    return clamp(1.5 - px * freq * 3.0, 0.0, 1.0);
}

fn vnoise_aa(p: vec3<f32>, freq: f32, px: f32) -> f32 {
    return mix(0.5, vnoise(p * freq), band(px, freq));
}

fn fbm(p: vec3<f32>, freq: f32, px: f32) -> f32 {
    return (vnoise_aa(p, freq, px) * 0.5
        + vnoise_aa(p + vec3<f32>(5.1), freq * 2.03, px) * 0.25
        + vnoise_aa(p + vec3<f32>(9.7), freq * 4.07, px) * 0.125) / 0.875;
}

// A face's own two axes, m, by the way it faces: along the truck and up
// its sides, along and across its top, across and up its nose and tail.
fn face_uv(p: vec3<f32>, n: vec3<f32>) -> vec2<f32> {
    let a = abs(n);
    if a.y >= a.x && a.y >= a.z {
        return p.xz;
    }
    if a.z >= a.x {
        return p.xy;
    }
    return p.zy;
}

// One family of scratches: straight and parallel, turned `angle` off the
// face's first axis, `rows` of them a metre apart across their way, each
// row cut in runs `run` metres long of which `amount` (0..1) are there,
// each `half` metres to its edge. Thinner than a pixel, a scratch is drawn
// a pixel wide and that much fainter, so it thins out with distance and
// doesn't sparkle.
fn scratch_family(uv: vec2<f32>, angle: f32, rows: f32, run: f32, amount: f32, half: f32, salt: i32, px: f32) -> f32 {
    let cs = vec2<f32>(cos(angle), sin(angle));
    let along = (uv.x * cs.x + uv.y * cs.y) / run;
    let b = (uv.y * cs.x - uv.x * cs.y) * rows;
    let row = i32(floor(b));
    let slid = along + hash3(vec3<i32>(row, salt, 7)) * 13.0;
    let seg = i32(floor(slid));
    let there = step(1.0 - amount, hash3(vec3<i32>(row, seg, salt + 3)));
    let centre = 0.2 + 0.6 * hash3(vec3<i32>(row, seg, salt + 5));
    let across = abs(fract(b) - centre) / rows;
    let f = fract(slid);
    let ends = smoothstep(0.0, 0.12, f) * (1.0 - smoothstep(0.75, 1.0, f));
    let wide = max(half, px * 0.7);
    return there * ends * (1.0 - smoothstep(wide, wide * 1.8, across)) * (half / wide);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    // The pixel's footprint, metres, before any branch, as derivatives need.
    let wp = in.world_position.xyz;
    let px = max(length(dpdx(wp)), length(dpdy(wp)));
    let p = (truck.to_local * vec4<f32>(wp, 1.0)).xyz;
    // The face's own normal, in the truck's frame.
    var ng = normalize((truck.to_local * vec4<f32>(in.world_normal, 0.0)).xyz);
    if !is_front {
        ng = -ng;
    }
    let hurt = truck.state.x;
    let burnt = truck.state.y;
    // Each truck its own pattern.
    let q = p + vec3<f32>(truck.state.z * 17.3, truck.state.z * 5.7, truck.state.z * 29.1);

    var c = pbr_input.material.base_color.rgb;
    var rough = pbr_input.material.perceptual_roughness;
    var metal = pbr_input.material.metallic;
    var glow = vec3<f32>(0.0);

    // 1 · Scratches from the first knocks on: hairlines mostly along the
    // truck, as it brushes past things, then longer ones off that line,
    // then gouges that read from across a yard. Bare and bright on steel,
    // pale on trim; rougher either way.
    let uv = face_uv(q, ng);
    let few = smoothstep(0.03, 0.5, hurt);
    let more = smoothstep(0.2, 0.8, hurt);
    let fine = scratch_family(uv, 0.04, 150.0, 0.3, 0.5 * few, 0.0007, 11, px)
        + scratch_family(uv, -0.1, 110.0, 0.45, 0.4 * few, 0.0008, 23, px);
    let cross = scratch_family(uv, 0.35, 60.0, 0.5, 0.3 * more, 0.0012, 37, px)
        + scratch_family(uv, -0.6, 45.0, 0.4, 0.25 * more, 0.0012, 41, px);
    let gouge = scratch_family(uv, 0.02, 14.0, 1.1, 0.35 * more, 0.004, 53, px)
        + scratch_family(uv, 0.2, 9.0, 0.8, 0.25 * more, 0.005, 67, px);
    let scratch = clamp(fine + cross + gouge, 0.0, 1.0);
    c = mix(c, c * 1.7 + vec3<f32>(0.06), scratch * 0.8);
    rough = mix(rough, min(rough + 0.25, 1.0), scratch);

    // 2 · Scuffs: patches rubbed dull, where the brushing no longer shows
    // a light.
    let scuffed = smoothstep(0.12, 0.7, hurt);
    let scuff = smoothstep(0.62 - 0.3 * scuffed, 0.78 - 0.3 * scuffed, fbm(q + vec3<f32>(41.0), 1.3, px)) * scuffed;
    c = mix(c, c * 0.72, scuff);
    rough = mix(rough, 0.85, scuff * 0.8);
    metal = mix(metal, metal * 0.55, scuff);

    // 3 · Dirt up the sills and behind the wheels, in a ragged band that
    // climbs as it is knocked about; none on what faces down.
    let dirty = smoothstep(0.05, 0.6, hurt);
    let reach = (0.35 + 0.75 * dirty) * (0.55 + 0.9 * vnoise_aa(q + vec3<f32>(23.0), 1.1, px));
    let dirt = (1.0 - smoothstep(0.3, 0.3 + reach, p.y)) * dirty * smoothstep(-0.6, -0.1, ng.y);
    let grain = 0.75 + 0.5 * vnoise_aa(q, 11.0, px);
    c = mix(c, DIRT * grain, clamp(dirt, 0.0, 1.0) * 0.85);
    rough = mix(rough, 1.0, dirt * 0.9);
    metal = mix(metal, 0.0, dirt * 0.9);

    // 4 · Soot: blotches that spread from half its health gone.
    let sooty = smoothstep(0.4, 1.0, hurt);
    let soot = smoothstep(0.7 - 0.45 * sooty, 0.9 - 0.45 * sooty, fbm(q + vec3<f32>(67.0), 0.9, px)) * sooty;
    c = mix(c, SOOT * 4.0, soot * 0.9);
    rough = mix(rough, 0.95, soot);
    metal = mix(metal, 0.0, soot * 0.8);

    // 5 · Where it was hit.
    for (var i = 0; i < 16; i++) {
        let m = truck.marks[i];
        if m.w == 0.0 {
            continue;
        }
        let d = p - m.xyz;
        // Across the face, and how far off it: a mark lies on the skin
        // nearest where the round met the hull, not on what is behind it.
        let off = dot(d, ng);
        let r = length(d - ng * off);
        let near = 1.0 - smoothstep(0.25, 0.4, abs(off));
        if m.w > 0.0 {
            // A round: a dark hole, a ring of bare steel round it, and a
            // ragged halo of cracked finish out to its reach.
            let ragged = 0.75 + 0.5 * vnoise(q * 37.0);
            let hole = (1.0 - smoothstep(0.011, 0.018, r)) * near;
            let ring = smoothstep(0.014, 0.02, r) * (1.0 - smoothstep(0.026 * ragged, 0.045 * ragged, r)) * near;
            let halo = (1.0 - smoothstep(0.3 * m.w, m.w * ragged, r)) * near * band(px, 30.0);
            c = mix(c, c * 0.6, halo * 0.5);
            rough = mix(rough, 0.9, halo * 0.5);
            c = mix(c, vec3<f32>(0.62, 0.63, 0.65), ring);
            rough = mix(rough, 0.3, ring);
            c = mix(c, vec3<f32>(0.004), hole);
            rough = mix(rough, 1.0, hole);
            metal = mix(metal, 0.0, hole);
        } else {
            // A blast: a scorch, black at its heart, ragged at its rim.
            let reach = -m.w * (0.7 + 0.6 * vnoise_aa(q + vec3<f32>(f32(i) * 3.1), 4.0, px));
            let scorch = (1.0 - smoothstep(0.25 * reach, reach, r)) * near;
            c = mix(c, SOOT * 2.0, scorch * 0.92);
            rough = mix(rough, 1.0, scorch);
            metal = mix(metal, 0.0, scorch * 0.9);
        }
    }

    // 6 · Burning: char spreads over what is left, embers in its cracks.
    let char = clamp(burnt * (0.35 + 1.1 * fbm(q + vec3<f32>(89.0), 1.6, px)), 0.0, 1.0);
    c = mix(c, SOOT, char * 0.95);
    rough = mix(rough, 1.0, char);
    metal = mix(metal, 0.0, char);
    // Embers: fine cracks still alight, in patches, and from too far to
    // see a crack, the patch's dull glow.
    let alight = smoothstep(0.55, 0.8, vnoise(q * 1.9 + vec3<f32>(3.0))) * char * smoothstep(0.3, 0.9, burnt);
    let crack = (1.0 - smoothstep(0.004, 0.02, abs(vnoise(q * 14.0) - 0.5))) * band(px, 28.0);
    glow = EMBER * alight * (0.5 * crack + 0.03);

    pbr_input.material.base_color = vec4<f32>(c, pbr_input.material.base_color.a);
    pbr_input.material.perceptual_roughness = rough;
    pbr_input.material.metallic = metal;
    pbr_input.material.emissive = vec4<f32>(pbr_input.material.emissive.rgb + glow, pbr_input.material.emissive.a);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

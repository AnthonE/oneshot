// A skin on a gun (`look/gun_skin.rs`): `bevy_pbr`'s own fragment with the
// gun's paint recoloured before lighting. Saturated paint takes the coat's
// colour and the rest (steel, polymer, bone) its dark; each keeps its own
// light and dark against that part's middle, so wear, grime and panel lines
// stay where they were painted. What the gun's glow map lights is left alone.
//
// Over Bevy's lighting each gun reflects a sky photograph (`env_tex`, cut by
// tools/gun_env.py), a level for each roughness: the scene's own fill is two
// tones, which leaves metal looking painted.
//
// The gun in our own view wears it too, coated or not (`look/gun_glow.rs`):
// its glow is turned up and down as a whole, and a gauge puts the glow out
// where its rounds are spent, by where a texel is in the model's own frame.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_functions::get_local_from_world,
    mesh_view_bindings::globals,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

struct Coat {
    // rgb: the paint's linear colour at its middle; w its metal.
    paint: vec4<f32>,
    // rgb: the rest's linear colour at its middle; w the paint's roughness.
    rest: vec4<f32>,
    // x, y: the paint's and the rest's middle (linear luma); z, w: how far
    // from grey a texel starts to count as paint and where it does.
    mid: vec4<f32>,
    // The model's frame from the world's.
    local: mat4x4<f32>,
    // x: the glow's gain; y: how much of the coat it wears (0: its own
    // paint); z: what of the glow a gauge leaves where it is out; w: the
    // gauge's lit share, 0..=1.
    glow: vec4<f32>,
    // A run's end lit longest (xyz), and its edge's width, a share (w).
    stays: vec4<f32>,
    // A run's end out first (xyz); w how many cells (the first out last).
    goes: vec4<f32>,
    // Boxes in the model's frame, low and high corners in turn: the run's
    // region (empty for none), then each cell's.
    boxes: array<vec4<f32>, 10>,
    // The skin's pattern, drawn over the coat in the mesh's own frame so it
    // stays on the gun: x which (0 none, 1 molten, 2 frost, 3 veins,
    // 4 gilded), y its glow's strength, z its cells a metre, w its speed.
    look: vec4<f32>,
    // rgb: the pattern's hot colour, linear; w how much of the gun's steel
    // it takes (its paint it takes whole).
    hot: vec4<f32>,
    // x: the sky photograph's brightness in a reflection; y its last level.
    env: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> coat: Coat;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var env_tex: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var env_smp: sampler;

// How much of the sky photograph's own colour a reflection keeps.
const SKY_CHROMA: f32 = 0.3;

// What a reflection keeps of the light it meets, by roughness and the view's
// angle: Karis' fit of the split-sum's table (2014), scale and bias on F0.
fn env_brdf(rough: f32, nv: f32) -> vec2<f32> {
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = rough * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * nv)) * r.x + r.y;
    return vec2<f32>(-1.04, 1.04) * a004 + r.zw;
}

// How far over its middle a texel's tone may go.
const TONE_MAX: f32 = 2.5;
// The glow map's level where a texel is all light.
const LIT_FULL: f32 = 0.1;
// Cells a gauge has at most.
const CELLS: i32 = 4;

fn inside(p: vec3<f32>, k: i32) -> bool {
    return all(p >= coat.boxes[2 * k].xyz) && all(p <= coat.boxes[2 * k + 1].xyz);
}

// How lit the gauge leaves a point of the model, 0..=1; 1 off it.
fn gauge(p: vec3<f32>) -> f32 {
    let share = coat.glow.w;
    if inside(p, 0) {
        let d = coat.goes.xyz - coat.stays.xyz;
        let t = clamp(dot(p - coat.stays.xyz, d) / max(dot(d, d), 1e-8), 0.0, 1.0);
        let edge = max(coat.stays.w, 1e-3);
        return clamp((share * (1.0 + edge) - t) / edge, 0.0, 1.0);
    }
    let n = i32(coat.goes.w + 0.5);
    for (var k = 0; k < CELLS; k++) {
        if k < n && inside(p, k + 1) {
            return clamp(share * f32(n) - f32(k), 0.0, 1.0);
        }
    }
    return 1.0;
}

fn hash3(p: vec3<f32>) -> vec3<f32> {
    let q = vec3<f32>(
        dot(p, vec3<f32>(127.1, 311.7, 74.7)),
        dot(p, vec3<f32>(269.5, 183.3, 246.1)),
        dot(p, vec3<f32>(113.5, 271.9, 124.6)),
    );
    return fract(sin(q) * 43758.5453);
}

fn hash1(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3<f32>(12.9898, 78.233, 37.719))) * 43758.5453);
}

fn noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(hash1(i), hash1(i + vec3(1.0, 0.0, 0.0)), u.x),
                mix(hash1(i + vec3(0.0, 1.0, 0.0)), hash1(i + vec3(1.0, 1.0, 0.0)), u.x), u.y);
    let b = mix(mix(hash1(i + vec3(0.0, 0.0, 1.0)), hash1(i + vec3(1.0, 0.0, 1.0)), u.x),
                mix(hash1(i + vec3(0.0, 1.0, 1.0)), hash1(i + vec3(1.0, 1.0, 1.0)), u.x), u.y);
    return mix(a, b, u.z);
}

fn fbm(p: vec3<f32>) -> f32 {
    return 0.5 * noise(p) + 0.3 * noise(p * 2.03 + 17.0) + 0.2 * noise(p * 4.11 + 41.0);
}

// Cells: x the distance to the nearest point, y to the edge between it and
// the next (near enough), z the nearest cell's own number, 0..1.
fn cells(p: vec3<f32>) -> vec3<f32> {
    let i = floor(p);
    let f = fract(p);
    var d1 = 8.0;
    var d2 = 8.0;
    var id = 0.0;
    for (var z = -1; z <= 1; z++) {
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                let o = vec3<f32>(f32(x), f32(y), f32(z));
                let h = hash3(i + o);
                let r = o + h - f;
                let d = dot(r, r);
                if d < d1 {
                    d2 = d1;
                    d1 = d;
                    id = h.x;
                } else if d < d2 {
                    d2 = d;
                }
            }
        }
    }
    return vec3<f32>(sqrt(d1), sqrt(d2) - sqrt(d1), id);
}

struct Look {
    // The surface's colour where the pattern takes it, and how much it does.
    base: vec3<f32>,
    cover: f32,
    glow: vec3<f32>,
    metal: f32,
    rough: f32,
}

// A skin's pattern at `p` (the mesh's frame in cells), `t` its own time,
// over a texel `tone` as light or dark as it was painted against its part's
// middle (its wear, grime and panel lines, which the pattern keeps).
fn pattern(kind: i32, p: vec3<f32>, t: f32, tone: f32) -> Look {
    var o: Look;
    o.cover = coat.hot.w;
    let hot = coat.hot.rgb;
    if kind == 1 {
        // Molten: charred rock, cracked, the cracks running with lava that
        // breathes and crawls.
        let warp = fbm(p * 0.5 + vec3<f32>(0.0, t * 0.15, 0.0));
        let c = cells(p + warp * 0.9);
        let crack = 1.0 - smoothstep(0.015, 0.11, c.y);
        let pulse = 0.55 + 0.45 * sin(t * 2.2 + c.z * 6.283);
        let flow = 0.5 + fbm(p * 2.0 - vec3<f32>(0.0, t * 0.6, t * 0.3));
        let heat = crack * pulse * flow;
        let ember = 0.25 * smoothstep(0.55, 0.9, fbm(p * 1.3 + t * 0.2));
        o.base = mix(vec3<f32>(0.018, 0.014, 0.012), vec3<f32>(0.05, 0.02, 0.01), c.z) * tone;
        o.glow = mix(hot * vec3<f32>(1.0, 0.25, 0.08), hot, clamp(heat, 0.0, 1.0)) * (heat + ember);
        o.metal = 0.0;
        o.rough = 0.85;
    } else if kind == 2 {
        // Frost: ice in facets, each its own shade, their seams lit and a
        // shimmer passing over them, glints coming and going.
        let c = cells(p);
        let edge = 1.0 - smoothstep(0.0, 0.07, c.y);
        let shimmer = 0.5 + 0.5 * sin(t * 1.6 + dot(p, vec3<f32>(0.9, 0.35, 0.6)) * 2.0);
        let glint = step(0.993, hash1(floor(p * 5.0) + floor(t * 3.0)));
        o.base = mix(vec3<f32>(0.6, 0.78, 0.92), vec3<f32>(0.92, 0.98, 1.0), c.z) * tone;
        o.glow = hot * (0.06 + edge * (0.25 + 0.75 * shimmer) + glint * 4.0);
        o.metal = 0.2;
        o.rough = 0.12;
    } else if kind == 3 {
        // Veins: dark moss, a net of living veins through it, warped as
        // roots grow, finer ones between, and pulses running along them.
        let warp = fbm(p * 0.4) * 1.8;
        let big = cells(p * 0.6 + warp);
        let fine = cells(p * 1.7 + warp * 1.4 + 9.0);
        let width = 0.03 + 0.05 * fbm(p * 1.1 + 5.0);
        let vein = 1.0 - smoothstep(0.0, width, big.y);
        let thin = (1.0 - smoothstep(0.0, 0.035, fine.y)) * 0.35;
        let pulse = 0.5 + 0.5 * sin(t * 3.0 - (big.x + warp) * 7.0);
        o.base = mix(vec3<f32>(0.01, 0.025, 0.012), vec3<f32>(0.035, 0.07, 0.03), fbm(p * 2.0)) * tone;
        o.glow = hot * (max(vein, thin) * (0.15 + 1.0 * pulse * pulse) + vein * 0.1);
        o.metal = 0.1;
        o.rough = 0.55;
    } else {
        // Gilded: gold, engraved with scrolls (the contours of a noise),
        // a shine sweeping down it and sparks where the engraving catches.
        let n = fbm(p * 0.6);
        let d = abs(fract(n * 12.0) - 0.5);
        let scroll = 1.0 - smoothstep(0.02, 0.045, d);
        let along = fract(dot(p, vec3<f32>(0.55, 0.2, 0.35)) * 0.12 - t * 0.35);
        let band = smoothstep(0.0, 0.015, along) * (1.0 - smoothstep(0.015, 0.04, along));
        let spark = step(0.99, hash1(floor(p * 4.0) + floor(t * 5.0))) * scroll;
        let gold = vec3<f32>(0.9, 0.5, 0.07);
        o.base = mix(gold, vec3<f32>(0.12, 0.06, 0.015), scroll) * min(tone, 1.2);
        o.glow = hot * (band * 1.2 + spark * 3.0) + gold * 0.08 * (1.0 - scroll);
        o.metal = mix(0.75, 0.2, scroll);
        o.rough = mix(0.25, 0.7, scroll);
    }
    return o;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    let c = pbr_input.material.base_color.rgb;
    // How far from grey it was painted, near enough on sRGB's curve: a share
    // of its brightness instead would make paint of a dark texel's JPEG noise.
    let g = sqrt(max(c, vec3<f32>(0.0)));
    let chroma = max(g.r, max(g.g, g.b)) - min(g.r, min(g.g, g.b));
    let glow = pbr_input.material.emissive.rgb;
    let lit = smoothstep(0.0, LIT_FULL, max(glow.r, max(glow.g, glow.b)));
    let paint = smoothstep(coat.mid.z, coat.mid.w, chroma) * (1.0 - lit) * coat.glow.y;
    let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    let coated = mix(
        coat.rest.rgb * min(luma / coat.mid.y, TONE_MAX),
        coat.paint.rgb * min(luma / coat.mid.x, TONE_MAX),
        paint,
    );
    pbr_input.material.base_color = vec4<f32>(mix(c, mix(coated, c, lit), coat.glow.y), pbr_input.material.base_color.a);
    pbr_input.material.metallic = mix(pbr_input.material.metallic, coat.paint.w, paint);
    pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, coat.rest.w, paint);

    let p = (coat.local * vec4<f32>(in.world_position.xyz, 1.0)).xyz;
    let gain = coat.glow.x * mix(coat.glow.z, 1.0, gauge(p));
    var emissive = glow * gain;

    let kind = i32(coat.look.x + 0.5);
    if kind > 0 {
        let at = (get_local_from_world(in.instance_index) * vec4<f32>(in.world_position.xyz, 1.0)).xyz;
        let tone = clamp(luma / mix(coat.mid.y, coat.mid.x, paint), 0.3, 2.2);
        let l = pattern(kind, at * coat.look.z, globals.time * coat.look.w, tone);
        // All of the paint, and of the steel as much as the skin takes: the
        // rest stays the gun's own metal, under the coat's dark.
        let take = mix(l.cover * coat.glow.y, 1.0, paint) * (1.0 - lit);
        let col = pbr_input.material.base_color;
        let own_rough = pbr_input.material.perceptual_roughness;
        pbr_input.material.base_color = vec4<f32>(mix(col.rgb, l.base, take), col.a);
        pbr_input.material.metallic = mix(pbr_input.material.metallic, l.metal, take);
        // The pattern's roughness, but the gun's own scuffs and polish in it.
        let rough = clamp(l.rough + (own_rough - 0.5) * 0.6, 0.04, 1.0);
        pbr_input.material.perceptual_roughness = mix(own_rough, rough, take);
        emissive += l.glow * coat.look.y * take;
    }
    pbr_input.material.emissive = vec4<f32>(emissive, pbr_input.material.emissive.a);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
        // The sky in it: sharp where the gun is polished, a sheen where it
        // is rough, coloured by the metal, white off the paint's clear coat.
        let n = pbr_input.N;
        let v = pbr_input.V;
        let nv = max(dot(n, v), 1e-4);
        let r = reflect(-v, n);
        let m = pbr_input.material;
        let photo = textureSampleLevel(env_tex, env_smp, vec3<f32>(r.x, r.y, -r.z), m.perceptual_roughness * coat.env.y).rgb;
        // Its shapes and light, little of its colour: one photograph's blue
        // sky would tint a gun under every map's own.
        let sky = mix(vec3<f32>(dot(photo, vec3<f32>(0.2126, 0.7152, 0.0722))), photo, SKY_CHROMA);
        let f0 = mix(vec3<f32>(0.04), m.base_color.rgb, m.metallic);
        let ab = env_brdf(m.perceptual_roughness, nv);
        let shade = pbr_input.diffuse_occlusion * pbr_input.specular_occlusion;
        out.color = vec4<f32>(out.color.rgb + sky * (f0 * ab.x + ab.y) * shade * coat.env.x, out.color.a);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

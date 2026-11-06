// Weathering on the level's boxes (`look/weathering.rs`): `bevy_pbr`'s own
// fragment with the base colour stained before lighting, after Gates'
// shaders/weathering.wgsl (MIT, NOTICE). A world-space mottle and each
// block's own value, grime toward the ground, rain streaks down the sides,
// wear along the block's edges and dust settled on its tops. A mesh that
// carries `UV_1` (the rock's facets, `look/rock.rs`) has its value
// multiplier in `uv_b.x`.
//
// Uniforms and arithmetic only, so it binds and runs the same on WebGL2.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    mesh_view_bindings::view,
}
#import "shaders/shaft_march.wgsl"::{shaft_lost}

struct Weather {
    // x amplitude, y fine frequency /m, z broad frequency /m, w the block's
    // own value (a multiplier).
    mottle: vec4<f32>,
    // x strength, y frequency across the face /m.
    streak: vec4<f32>,
    // rgb what a streak multiplies by.
    streak_tint: vec4<f32>,
    // x strength, y reach up from the ground m, z the ground's height m,
    // w 0 that height everywhere, 1 or 2 `ground_run` along local x or z.
    grime: vec4<f32>,
    // rgb what grime multiplies by.
    grime_tint: vec4<f32>,
    // x dust on the tops, y edge wear, z the wear's width m, w how much
    // lighter what is worn is (a multiplier).
    wear: vec4<f32>,
    // rgb settled dust's linear albedo, w how much rougher worn is.
    dust_tint: vec4<f32>,
    // xyz the block's middle, w its shape: 0 no edges, 1 a box, 2 an
    // upright cylinder.
    centre: vec4<f32>,
    // xyz half extents (a cylinder: x its radius, y half its height); w the
    // edges another box hides, as bits (`edge_distance`).
    half: vec4<f32>,
    // x, y: cos and sin of its turn about y (local +x is (c, 0, s)); z 1:
    // a plaza's round slab, its top carved as a seal (`weathering::seal`).
    turn: vec4<f32>,
    // The ground under a long block at eight points end to end along the
    // axis `grime.w` names (`weathering::Block::run`).
    ground_run: array<vec4<f32>, 2>,
    // WebGL2's light shafts (`shaft_march.wgsl`): the map's box, its mist
    // and what a shaded ray takes back. `shaft_hi.w` 0: off.
    shaft_lo: vec4<f32>,
    shaft_hi: vec4<f32>,
    shaft_tint: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> weather: Weather;

// A plaza's seal: its rings as shares of its radius, how many spokes, a
// groove's half-width m, and how much darker a groove and the rim's band
// are and how much paler the hub.
const SEAL_RINGS: vec4<f32> = vec4<f32>(0.14, 0.36, 0.6, 0.86);
const SEAL_SPOKES: f32 = 12.0;
const SEAL_GROOVE_M: f32 = 0.05;
const SEAL_GROOVE_DARK: f32 = 0.5;
const SEAL_RIM_DARK: f32 = 0.84;
const SEAL_HUB_PALE: f32 = 1.12;

// 1 on a groove of the seal at `at` metres (across `x` metres), fading
// over the pixel's footprint.
fn seal_line(x: f32, at: f32, aa: f32) -> f32 {
    return 1.0 - smoothstep(SEAL_GROOVE_M, SEAL_GROOVE_M + aa, abs(x - at));
}

// An integer hash: world coordinates run to hundreds of metres, where a
// `fract(sin(·))` hash has lost its low bits.
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
// carry: whole at six pixels a cycle, gone at two. Past that a term is
// speckle that crawls as the camera moves.
fn band(px: f32, freq: f32) -> f32 {
    return clamp(1.5 - px * freq * 3.0, 0.0, 1.0);
}

// Value noise at `freq`, faded to its mean where the pixel cannot carry it.
fn vnoise_aa(p: vec3<f32>, freq: f32, px: f32) -> f32 {
    return mix(0.5, vnoise(p * freq), band(px, freq));
}

fn fbm(p: vec3<f32>, freq: f32, px: f32) -> f32 {
    return (vnoise_aa(p, freq, px) * 0.5
        + vnoise_aa(p + vec3<f32>(5.1), freq * 2.03, px) * 0.25
        + vnoise_aa(p + vec3<f32>(9.7), freq * 4.07, px) * 0.125) / 0.875;
}

// A world point in the block's own frame (its middle at 0, unturned).
fn local(p: vec3<f32>) -> vec3<f32> {
    let d = p - weather.centre.xyz;
    let c = weather.turn.x;
    let s = weather.turn.y;
    return vec3<f32>(d.x * c + d.z * s, d.y, d.z * c - d.x * s);
}

// Edge `bit`'s distance `e`, unless another box hides that edge.
fn edge_if(d: f32, mask: u32, bit: u32, e: f32) -> f32 {
    return select(min(d, e), d, ((mask >> bit) & 1u) != 0u);
}

// How far a point on the block's surface is from its nearest edge, m: for
// each edge the further of its two faces' planes (the point lies on one).
// The bottom's edges are left out, where a block meets what it stands on,
// and so are the ones another box hides or carries on (`half.w`'s bits,
// `rock::hidden_edges`: 0..3 the top's at +x, -x, +z, -z; 4..7 the
// uprights at (+x,+z), (+x,-z), (-x,+z), (-x,-z)).
fn edge_distance(p: vec3<f32>) -> f32 {
    let shape = weather.centre.w;
    let l = local(p);
    let h = weather.half.xyz;
    let top = abs(h.y - l.y);
    if shape > 1.5 {
        // A cylinder's one edge is its top's rim.
        return max(abs(h.x - length(l.xz)), top);
    }
    if shape > 0.5 {
        let mask = u32(weather.half.w + 0.5);
        let xp = abs(h.x - l.x);
        let xn = abs(h.x + l.x);
        let zp = abs(h.z - l.z);
        let zn = abs(h.z + l.z);
        var d = 1.0e4;
        d = edge_if(d, mask, 0u, max(xp, top));
        d = edge_if(d, mask, 1u, max(xn, top));
        d = edge_if(d, mask, 2u, max(zp, top));
        d = edge_if(d, mask, 3u, max(zn, top));
        d = edge_if(d, mask, 4u, max(xp, zp));
        d = edge_if(d, mask, 5u, max(xp, zn));
        d = edge_if(d, mask, 6u, max(xn, zp));
        d = edge_if(d, mask, 7u, max(xn, zn));
        return d;
    }
    return 1.0e4;
}

// Where the grime starts under `p`: the ground's height under the block's
// middle, or along a long block its run of samples, end to end.
fn ground_under(p: vec3<f32>) -> f32 {
    let axis = weather.grime.w;
    if axis < 0.5 {
        return weather.grime.z;
    }
    let l = local(p);
    let along = select(l.x, l.z, axis > 1.5);
    let h = select(weather.half.x, weather.half.z, axis > 1.5);
    let t = clamp((along + h) / max(2.0 * h, 1.0e-4), 0.0, 1.0) * 7.0;
    let i = min(u32(t), 6u);
    let f = t - f32(i);
    let a = weather.ground_run[i / 4u][i % 4u];
    let b = weather.ground_run[(i + 1u) / 4u][(i + 1u) % 4u];
    return mix(a, b, f);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    let p = in.world_position.xyz;
    // The pixel's footprint, metres, before any branch, as derivatives need.
    let px = max(length(dpdx(p)), length(dpdy(p)));
    // The face's own normal: the masks ask which way the block faces, not
    // which way its map bumps.
    var ng = normalize(in.world_normal);
    if !is_front {
        ng = -ng;
    }
    var c = pbr_input.material.base_color.rgb;
    var rough = pbr_input.material.perceptual_roughness;

#ifdef VERTEX_UVS_B
    // The rock's facets: recessed darker, proud lighter (`look/rock.rs`).
    c *= in.uv_b.x;
#endif

    // 1 · No two blocks one value, and no wall one value across.
    c *= weather.mottle.w;
    let fine = fbm(p, weather.mottle.y, px) - 0.5;
    let broad = vnoise_aa(p + vec3<f32>(31.0), weather.mottle.z, px) - 0.5;
    c *= max(1.0 + weather.mottle.x * (1.6 * fine + 1.2 * broad), 0.0);

    // 1b · A plaza's seal: on its top, grooves cut in rings and in spokes
    // between the hub's ring and the third, the band outside the last ring
    // darker, the hub paler. Value and roughness only: the floor stays the
    // flat top the sim walks on.
    if weather.turn.z > 0.5 && ng.y > 0.9 {
        let d = p.xz - weather.centre.xz;
        let rm = length(d);
        let big_r = weather.half.x;
        let aa = max(px, 0.004);
        var groove = max(
            max(seal_line(rm, SEAL_RINGS.x * big_r, aa), seal_line(rm, SEAL_RINGS.y * big_r, aa)),
            max(seal_line(rm, SEAL_RINGS.z * big_r, aa), seal_line(rm, SEAL_RINGS.w * big_r, aa)),
        );
        let sector = atan2(d.y, d.x) * SEAL_SPOKES / 6.2831853;
        let off = abs(fract(sector + 0.5) - 0.5) * 6.2831853 / SEAL_SPOKES * rm;
        let spans = step(SEAL_RINGS.x * big_r, rm) * step(rm, SEAL_RINGS.z * big_r);
        groove = max(groove, seal_line(off, 0.0, aa) * spans);
        c *= mix(1.0, SEAL_GROOVE_DARK, groove);
        rough = mix(rough, 1.0, groove);
        c *= mix(1.0, SEAL_RIM_DARK, smoothstep(SEAL_RINGS.w * big_r, SEAL_RINGS.w * big_r + aa, rm));
        c *= mix(1.0, SEAL_HUB_PALE, 1.0 - smoothstep(SEAL_RINGS.x * big_r - aa, SEAL_RINGS.x * big_r, rm));
    }

    // 2 · Grime toward the ground, in a ragged band: splash and soil.
    let above = p.y - ground_under(p);
    let ragged = weather.grime.y * (0.6 + 0.8 * vnoise_aa(p + vec3<f32>(47.0), 0.8, px));
    let grime = (1.0 - smoothstep(0.0, ragged, above)) * weather.grime.x;
    c = mix(c, c * weather.grime_tint.rgb, clamp(grime, 0.0, 1.0));

    // 3 · Streaks, on the sides only: noise long down the wall and narrow
    // across it, its across-coordinate the face's own horizontal.
    let side = 1.0 - smoothstep(0.35, 0.7, abs(ng.y));
    let across = p.x * -ng.z + p.z * ng.x;
    let f = weather.streak.y;
    let s1 = mix(0.5, vnoise(vec3<f32>(across * f, p.y * 0.16, across * 0.11)), band(px, f));
    let s2 = mix(0.5, vnoise(vec3<f32>(across * f * 2.7 + 3.0, p.y * 0.35, 1.7)), band(px, f * 2.7));
    // A faded streak field sits at its mean, under the threshold: a far
    // wall keeps its value and loses the stripes rather than greying.
    let streak = smoothstep(0.38, 0.78, s1 * 0.7 + s2 * 0.3) * side * weather.streak.x;
    c = mix(c, c * weather.streak_tint.rgb, streak);

    // 4 · Wear along the edges: chipped concrete and rock, paint worn to
    // the metal. Ragged, and gone where the band is thinner than a pixel.
    let width = max(weather.wear.z, 0.005);
    let chips = vnoise_aa(p, 0.9 / width, px);
    let reach = width * (0.3 + 1.4 * chips);
    var wear = (1.0 - smoothstep(0.0, reach, edge_distance(p))) * weather.wear.y;
    wear *= clamp(width / max(px, 1.0e-4) - 0.5, 0.0, 1.0);
    c *= mix(1.0, weather.wear.w, wear);
    rough = clamp(rough + weather.dust_tint.w * wear, 0.05, 1.0);

    // 5 · Dust on what faces up, in patches. Flat tops only: a chamfer a
    // pixel wide at range would sparkle.
    let up = smoothstep(0.82, 0.96, ng.y);
    let patches = smoothstep(0.42, 0.66, fbm(p + vec3<f32>(13.0), 0.6, px));
    let dust = clamp(up * patches * weather.wear.x, 0.0, 1.0);
    let grain = 0.75 + 0.5 * vnoise_aa(p, 9.0, px);
    c = mix(c, weather.dust_tint.rgb * grain, dust);
    rough = mix(rough, 1.0, dust * 0.8);

    pbr_input.material.base_color = vec4<f32>(c, pbr_input.material.base_color.a);
    pbr_input.material.perceptual_roughness = rough;

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    // WebGL2's shafts: the sun's share of the mist taken back where this
    // face's ray crossed shade.
    if weather.shaft_hi.w > 0.0 {
        let to = pbr_input.world_position.xyz - view.world_position;
        let len = length(to);
        let lost = shaft_lost(view.world_position, to / max(len, 1e-4), len, weather.shaft_lo, weather.shaft_hi, weather.shaft_tint, in.position.xy);
        out.color = vec4(max(out.color.rgb - lost, vec3(0.0)), out.color.a);
    }
    return out;
}

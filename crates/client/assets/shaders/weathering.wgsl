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
    // x strength, y frequency across the face /m; z 1: what settles on its
    // tops is snow (a snowed valley's open tops), not dust.
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
    // a plaza's round slab, its top carved as a seal (`weathering::seal`);
    // w 1: cut stone, in courses with joints (`Surface::Stone`); 2: poured
    // concrete, in its formwork's panels (a snowed valley's).
    turn: vec4<f32>,
    // The ground under a long block at eight points end to end along the
    // axis `grime.w` names (`weathering::Block::run`).
    ground_run: array<vec4<f32>, 2>,
    // WebGL2's light shafts (`shaft_march.wgsl`): the map's box, its mist
    // and what a shaded ray takes back. `shaft_hi.w` 0: off.
    shaft_lo: vec4<f32>,
    shaft_hi: vec4<f32>,
    shaft_tint: vec4<f32>,
    // The hall this block stands in or bounds (a snowed valley's hangar):
    // its box, lo and hi; `hall_hi.w` 0: none.
    hall_lo: vec4<f32>,
    hall_hi: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> weather: Weather;

// A plaza's dial: its rings as shares of its radius, its hours and their
// quarter marks round the rim, a groove's half width (m) and how dark, the
// bronze let into its scale, the figure of the sun's year on its south
// meridian (half width, half length, how far south of the middle, m) and the
// band outside the last ring.
const SEAL_RINGS: vec4<f32> = vec4<f32>(0.11, 0.42, 0.8, 0.93);
const SEAL_HOURS: f32 = 24.0;
const SEAL_TICKS: f32 = 96.0;
const SEAL_GROOVE_M: f32 = 0.028;
const SEAL_GROOVE_DARK: f32 = 0.42;
const SEAL_BRONZE: vec3<f32> = vec3<f32>(0.26, 0.17, 0.085);
const SEAL_YEAR: vec3<f32> = vec3<f32>(0.5, 1.15, 2.05);
const SEAL_RIM_DARK: f32 = 0.8;
const SEAL_HUB_PALE: f32 = 1.1;

// Cut stone's courses: a course's height and a block's length, m, the
// blocks of each course staggered half a block on the last; a joint's
// half-width m and how dark it is; how far a block's own value strays.
const COURSE_H: f32 = 0.6;
const COURSE_LEN: f32 = 1.3;
const JOINT_W: f32 = 0.014;
const JOINT_DARK: f32 = 0.58;
const COURSE_VALUE: f32 = 0.09;

// Poured concrete: its formwork's panels (height and length m, one over
// the next, unstaggered), the lip between two pours (half-width m), how far
// a panel's own value strays, and its tie holes: four a panel, their radius
// m and how dark. A floor is poured in square bays two panels long, with no
// holes.
const POUR: vec2<f32> = vec2<f32>(1.2, 2.4);
const POUR_JOINT_W: f32 = 0.022;
const POUR_VALUE: f32 = 0.11;
const TIE_R: f32 = 0.035;
const TIE_DARK: f32 = 0.4;
// Its stains hang from its top edge, strongest in its first `HANG_M.x`
// metres and gone by `.y` more where they run longest (at 6 m every one
// ran a wall's whole height, and the wall was corrugated sheet).
const HANG_M: vec2<f32> = vec2<f32>(0.6, 2.6);
// In a hall the sky's fill is this much of itself (the lamps are its
// light), fading in over `HALL_EDGE_M` inside its box; what faces down
// there (its roof's underside, which the cube's earth half would paint
// with another map's sunlit sand) takes `HALL_SOFFIT` of that again.
const HALL_FILL: f32 = 0.38;
const HALL_SOFFIT: f32 = 0.35;
// A soffit's beams, as shading: this wide along each bay's joint, this dark.
const SOFFIT_RIB_M: f32 = 0.5;
const SOFFIT_RIB_DARK: f32 = 0.55;
// A hall's poured floor is driven on: a lane down the hall's shorter way
// (from whole `LANE_M.x` off its middle to none at `.y`, its edge
// wandering), `LANE_DARK` and worn to `LANE_ROUGH` so a lamp lies in it,
// two tyres' tracks in it `LANE_TRACK_M` off its middle; the dust is kept
// to its margins.
const LANE_M: vec2<f32> = vec2<f32>(4.5, 6.5);
const LANE_DARK: f32 = 0.88;
const HALL_FLOOR_LIFT: f32 = 1.5;
const LANE_ROUGH: f32 = 0.3;
const LANE_TRACK_M: f32 = 1.1;
const LANE_TRACK_ROUGH: f32 = 0.5;
// Snow blown against a poured wall's foot outdoors: plastered up it from
// the ground, to a ragged line `PLASTER_M` up (lowest, highest), in
// `SNOW_RGB`; the wall is damp and dark over it (the grime). And on a top
// it lies in windrows down the wind (`WINDROW`: how much longer than wide,
// and the noise's cut, hard-edged), none within `LIP_BARE_M` of a free
// edge, where the wind takes it: a deck's lip and its joints show.
const PLASTER_M: vec2<f32> = vec2<f32>(0.3, 1.6);
const SNOW_RGB: vec3<f32> = vec3<f32>(0.8, 0.81, 0.83);
const WINDROW: vec2<f32> = vec2<f32>(3.0, 0.46);
const LIP_BARE_M: vec2<f32> = vec2<f32>(0.08, 0.45);
const HALL_EDGE_M: f32 = 0.4;

// Poured concrete at `p`: the joint (1 in it), the panel's own value and
// a tie hole (1 in it), in the block's own frame as `courses`.
fn pours(p: vec3<f32>, ng: vec3<f32>, aa: f32) -> vec3<f32> {
    let l = local(p) + weather.half.xyz;
    let c = weather.turn.x;
    let s = weather.turn.y;
    let nl = vec3<f32>(ng.x * c + ng.z * s, ng.y, ng.z * c - ng.x * s);
    var u = l.x;
    var v = l.y;
    var size = vec2<f32>(POUR.y, POUR.x);
    var face = 0.0;
    var holes = 1.0;
    if abs(nl.y) > 0.7 {
        v = l.z;
        size = vec2<f32>(POUR.y * 2.0);
        face = 3.0;
        holes = 0.0;
    } else if abs(nl.x) > abs(nl.z) {
        u = l.z;
        face = select(1.0, 2.0, nl.x < 0.0);
    } else {
        face = select(4.0, 5.0, nl.z < 0.0);
    }
    let q = vec2<f32>(u, v) / size;
    let g = (fract(q) - 0.5) * size;
    let edge = min(size.x * 0.5 - abs(g.x), size.y * 0.5 - abs(g.y));
    var joint = 1.0 - smoothstep(POUR_JOINT_W, POUR_JOINT_W + aa, edge);
    joint *= clamp(POUR_JOINT_W / max(aa, 1.0e-4) * 1.5 - 0.3, 0.0, 1.0);
    // A hole a quarter of the way in from each corner.
    let h = length(abs(g) - size * 0.25);
    var hole = (1.0 - smoothstep(TIE_R, TIE_R + aa, h)) * holes;
    hole *= clamp(TIE_R / max(aa, 1.0e-4) * 1.5 - 0.3, 0.0, 1.0);
    let cell = vec2<i32>(floor(q));
    let value = 1.0 + POUR_VALUE * (hash3(vec3<i32>(cell.x + 17, cell.y * 3 + 5, i32(face) * 11)) * 2.0 - 1.0);
    return vec3<f32>(joint, value, hole);
}

// How far inside its hall `p` is (0 outside, 1 `HALL_EDGE_M` in), read a
// hand off the face so a wall's inner face is inside.
fn in_hall(p: vec3<f32>, ng: vec3<f32>) -> f32 {
    if weather.hall_hi.w < 0.5 {
        return 0.0;
    }
    let q = p + ng * 0.15;
    let d = min(q - weather.hall_lo.xyz, weather.hall_hi.xyz - q);
    return smoothstep(0.0, HALL_EDGE_M, min(d.x, d.z)) * step(0.0, d.y);
}

// Cut stone at `p`: the joint (1 in it) and the block's own value, read
// in the block's own frame so the courses run level along every face and
// a top is flagged in squares. `aa` is the pixel's size, m.
fn courses(p: vec3<f32>, ng: vec3<f32>, aa: f32) -> vec2<f32> {
    let l = local(p) + weather.half.xyz;
    let c = weather.turn.x;
    let s = weather.turn.y;
    let nl = vec3<f32>(ng.x * c + ng.z * s, ng.y, ng.z * c - ng.x * s);
    var u = l.x;
    var v = l.y;
    var h = COURSE_H;
    var face = 0.0;
    if abs(nl.y) > 0.7 {
        // A top: square flags.
        u = l.x;
        v = l.z;
        h = COURSE_LEN;
        face = 3.0;
    } else if abs(nl.x) > abs(nl.z) {
        u = l.z;
        face = select(1.0, 2.0, nl.x < 0.0);
    } else {
        face = select(4.0, 5.0, nl.z < 0.0);
    }
    let row = floor(v / h);
    let shift = select(0.0, COURSE_LEN * 0.5, (row - 2.0 * floor(row * 0.5)) > 0.5);
    let col = floor((u + shift) / COURSE_LEN);
    let dv = abs(fract(v / h) - 0.5) * h;
    let du = abs(fract((u + shift) / COURSE_LEN) - 0.5) * COURSE_LEN;
    let edge = min(h * 0.5 - dv, COURSE_LEN * 0.5 - du);
    var joint = 1.0 - smoothstep(JOINT_W, JOINT_W + aa, edge);
    // Gone where it is thinner than a pixel: a far wall keeps its courses'
    // values and loses the lines.
    joint *= clamp(JOINT_W / max(aa, 1.0e-4) - 0.3, 0.0, 1.0);
    let value = 1.0 + COURSE_VALUE * (hash3(vec3<i32>(i32(col) + 17, i32(row) * 3 + 5, i32(face) * 11)) * 2.0 - 1.0);
    return vec2<f32>(joint, value);
}

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

    // 1a · Cut stone: courses of blocks, each its own value, a dark joint
    // between them. Shading only: the faces stay the sim's planes. A seal's
    // top keeps its own carving.
    if weather.turn.w > 1.5 {
        let ps = pours(p, ng, max(px, 0.002));
        c *= ps.y;
        c *= mix(1.0, JOINT_DARK, ps.x);
        c *= mix(1.0, TIE_DARK, ps.z);
        rough = mix(rough, 1.0, max(ps.x, ps.z) * 0.5);
    } else if weather.turn.w > 0.5 && !(weather.turn.z > 0.5 && ng.y > 0.9) {
        let cs = courses(p, ng, max(px, 0.002));
        c *= cs.y;
        c *= mix(1.0, JOINT_DARK, cs.x);
        rough = mix(rough, 1.0, cs.x * 0.5);
    }

    // 1b · A plaza's dial: on its top, an hour line each 15 degrees from
    // the hub's ring out to the scale, a scale of quarter marks between the
    // last two rings with bronze let into it, the meridian north and south in
    // bronze and the sun's year in a figure of eight on it. Value and
    // roughness only: the floor stays the flat top the sim walks on.
    if weather.turn.z > 0.5 && ng.y > 0.9 {
        let d = p.xz - weather.centre.xz;
        let rm = length(d);
        let big_r = weather.half.x;
        let aa = max(px, 0.004);
        let r0 = SEAL_RINGS.x * big_r;
        let r1 = SEAL_RINGS.y * big_r;
        let r2 = SEAL_RINGS.z * big_r;
        let r3 = SEAL_RINGS.w * big_r;
        var groove = max(max(seal_line(rm, r0, aa), seal_line(rm, r1, aa)), seal_line(rm, r2, aa));
        // How far off the nearest of `n` lines round the dial, m.
        let turn = atan2(d.y, d.x) / 6.2831853;
        let hour = abs(fract(turn * SEAL_HOURS + 0.5) - 0.5) / SEAL_HOURS * 6.2831853 * rm;
        groove = max(groove, seal_line(hour, 0.0, aa) * step(r0, rm) * step(rm, r2));
        // Bronze: the scale's rings, its marks and the meridian.
        var inlay = max(seal_line(rm, r3, aa * 1.5), seal_line(rm, r2 + 0.035, aa));
        let tick = abs(fract(turn * SEAL_TICKS + 0.5) - 0.5) / SEAL_TICKS * 6.2831853 * rm;
        let reach = mix(r2 + (r3 - r2) * 0.45, r3, step(abs(fract(turn * SEAL_HOURS + 0.5) - 0.5), 0.5 / (SEAL_TICKS / SEAL_HOURS)));
        inlay = max(inlay, seal_line(tick, 0.0, aa) * step(r2, rm) * step(rm, reach));
        inlay = max(inlay, seal_line(d.x, 0.0, aa) * step(r0, rm) * step(rm, r3));
        // The year: a lemniscate about the south meridian (x/a)^2 = (u^2)(1-u^2).
        let u = (d.y + SEAL_YEAR.z) / SEAL_YEAR.y;
        if abs(u) < 1.05 {
            let xa = d.x / SEAL_YEAR.x;
            let f = xa * xa - u * u * (1.0 - u * u);
            let g = length(vec2<f32>(2.0 * xa / SEAL_YEAR.x, (-2.0 * u + 4.0 * u * u * u) / SEAL_YEAR.y));
            inlay = max(inlay, 1.0 - smoothstep(SEAL_GROOVE_M, SEAL_GROOVE_M + aa, abs(f) / max(g, 0.05)));
        }
        c *= mix(1.0, SEAL_GROOVE_DARK, groove);
        rough = mix(rough, 1.0, groove);
        c = mix(c, SEAL_BRONZE, inlay);
        rough = mix(rough, 0.45, inlay);
        c *= mix(1.0, SEAL_RIM_DARK, smoothstep(r3, r3 + aa, rm));
        c *= mix(1.0, SEAL_HUB_PALE, 1.0 - smoothstep(r0 - aa, r0, rm));
    }

    // 2 · Grime toward the ground, in a ragged band: splash and soil.
    let above = p.y - ground_under(p);
    let ragged = weather.grime.y * (0.6 + 0.8 * vnoise_aa(p + vec3<f32>(47.0), 0.8, px));
    // No splash or snow lies against a wall under a roof.
    let hall = in_hall(p, ng);
    // Nor against a side with no floor at its foot (a parapet's outer face on a deck's edge):
    // the block's own frame, a bit a side (`weathering::Block::foot`).
    let side_n = vec2<f32>(ng.x * weather.turn.x + ng.z * weather.turn.y, ng.z * weather.turn.x - ng.x * weather.turn.y);
    let side_bit = select(select(8u, 4u, side_n.y > 0.0), select(2u, 1u, side_n.x > 0.0), abs(side_n.x) > abs(side_n.y));
    let floored = f32((u32(weather.streak.w + 0.5) & side_bit) != 0u);
    let grime = (1.0 - smoothstep(0.0, ragged, above)) * weather.grime.x * (1.0 - hall) * floored;
    c = mix(c, c * weather.grime_tint.rgb, clamp(grime, 0.0, 1.0));
    // 2a · Snow against a poured wall's foot, outdoors, on its sides.
    if weather.turn.w > 1.5 && hall < 0.5 {
        // Torn, not a wave: a broad noise and a fine one. None on a block
        // whose foot is under the ground here (it stands in a slope).
        let tear = 0.7 * vnoise_aa(p + vec3<f32>(7.0), 0.45, px) + 0.3 * vnoise_aa(p + vec3<f32>(3.0), 2.6, px);
        let reach = mix(PLASTER_M.x, PLASTER_M.y, tear * tear);
        let plaster = (1.0 - smoothstep(reach * 0.8, reach, above)) * smoothstep(-0.5, -0.1, above)
            * (1.0 - smoothstep(0.35, 0.7, abs(ng.y))) * floored;
        c = mix(c, SNOW_RGB * (0.9 + 0.2 * vnoise_aa(p, 6.0, px)), plaster);
        rough = mix(rough, 1.0, plaster);
    }

    // 3 · Streaks, on the sides only: noise long down the wall and narrow
    // across it, its across-coordinate the face's own horizontal.
    let side = 1.0 - smoothstep(0.35, 0.7, abs(ng.y));
    let across = p.x * -ng.z + p.z * ng.x;
    let f = weather.streak.y;
    let s1 = mix(0.5, vnoise(vec3<f32>(across * f, p.y * 0.16, across * 0.11)), band(px, f));
    let s2 = mix(0.5, vnoise(vec3<f32>(across * f * 2.7 + 3.0, p.y * 0.35, 1.7)), band(px, f * 2.7));
    // A faded streak field sits at its mean, under the threshold: a far
    // wall keeps its value and loses the stripes rather than greying.
    var streak = smoothstep(0.38, 0.78, s1 * 0.7 + s2 * 0.3) * side * weather.streak.x;
    // No rain runs down a wall under a roof; a poured wall's stains hang
    // from its top edge, each its own length.
    streak *= 1.0 - hall;
    if weather.turn.w > 1.5 {
        let hang = weather.centre.y + weather.half.y - p.y;
        streak *= 1.6 * (1.0 - smoothstep(HANG_M.x, HANG_M.x + HANG_M.y * s1, hang));
    }
    c = mix(c, c * weather.streak_tint.rgb, clamp(streak, 0.0, 1.0));

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
    var patches = smoothstep(0.42, 0.66, fbm(p + vec3<f32>(13.0), 0.6, px));
    if weather.streak.z > 0.5 {
        // Snow: windrows, and the wind keeps a top's free edges bare.
        let rows = fbm(vec3<f32>(p.x / WINDROW.x, p.y, p.z) + vec3<f32>(13.0), 0.6, px);
        // The lip is the top's own border in the plan, whatever stands on it (a parapet covers the
        // bare strip under it, and at a gap in one the deck has a kerb), on the sides its floor
        // ends at (`weathering::Block::foot`'s high bits).
        let lp = local(p);
        let carried = u32(weather.streak.w + 0.5);
        var border = 1.0e4;
        if weather.centre.w > 1.5 {
            border = weather.half.x - length(lp.xz);
        } else {
            border = min(border, select(weather.half.x - lp.x, 1.0e4, (carried & 16u) != 0u));
            border = min(border, select(weather.half.x + lp.x, 1.0e4, (carried & 32u) != 0u));
            border = min(border, select(weather.half.z - lp.z, 1.0e4, (carried & 64u) != 0u));
            border = min(border, select(weather.half.z + lp.z, 1.0e4, (carried & 128u) != 0u));
        }
        let cut = 0.012 + px * 0.15;
        patches = smoothstep(WINDROW.y - cut, WINDROW.y + cut, rows) * smoothstep(LIP_BARE_M.x, LIP_BARE_M.y, border);
    }
    // A hall's floor: its lane, and the dust at its margins.
    var lane = 0.0;
    if hall > 0.0 && ng.y > 0.9 && weather.turn.w > 1.5 {
        let mid = (weather.hall_lo.xyz + weather.hall_hi.xyz) * 0.5;
        let ext = weather.hall_hi.xyz - weather.hall_lo.xyz;
        let off = abs(select(p.z - mid.z, p.x - mid.x, ext.x > ext.z));
        let wander = (vnoise_aa(p, 0.35, px) - 0.5) * 2.0;
        lane = (1.0 - smoothstep(LANE_M.x, LANE_M.y, off + wander)) * hall;
        // A tyre's width, and it comes and goes with the wear.
        let track = (1.0 - smoothstep(0.12, 0.26, abs(off - LANE_TRACK_M))) * lane
            * smoothstep(0.35, 0.6, vnoise_aa(p + vec3<f32>(5.0), 0.3, px));
        // A hall's floor is its palest broad surface: lifted, its lane told by gloss more than by dark.
        c *= HALL_FLOOR_LIFT * mix(1.0, LANE_DARK, max(lane * 0.6, track));
        // Worn in patches, and a tyre's rubber is dull across the sheen.
        let worn = mix(LANE_ROUGH, LANE_ROUGH + 0.12, vnoise_aa(p + vec3<f32>(21.0), 0.5, px));
        rough = mix(mix(rough, worn, lane), LANE_TRACK_ROUGH, track);
    }
    let dust = clamp(up * patches * weather.wear.x * (1.0 - 0.4 * hall) * (1.0 - lane), 0.0, 1.0);
    let grain = 0.75 + 0.5 * vnoise_aa(p, 9.0, px);
    c = mix(c, weather.dust_tint.rgb * grain, dust);
    rough = mix(rough, 1.0, dust * 0.8);

    pbr_input.material.base_color = vec4<f32>(c, pbr_input.material.base_color.a);
    pbr_input.material.perceptual_roughness = rough;
    // A hall is lit by its lamps: the sky's fill does not come through its roof.
    let fill = mix(1.0, HALL_FILL * mix(1.0, HALL_SOFFIT, clamp(-ng.y, 0.0, 1.0)), hall);
    // A hall's soffit is in bays: a beam's dark band along each pour's joint.
    if hall > 0.0 && ng.y < -0.9 && weather.turn.w > 1.5 {
        let l = local(p) + weather.half.xyz;
        let bay = POUR.y * 2.0;
        // How far off the nearest joint, either way.
        let off = (0.5 - max(abs(fract(l.x / bay) - 0.5), abs(fract(l.z / bay) - 0.5))) * bay;
        let rib = (1.0 - smoothstep(SOFFIT_RIB_M * 0.5, SOFFIT_RIB_M, off)) * hall;
        pbr_input.material.base_color = vec4<f32>(pbr_input.material.base_color.rgb * mix(1.0, SOFFIT_RIB_DARK, rib), pbr_input.material.base_color.a);
    }
    pbr_input.diffuse_occlusion *= fill;
    // Its mirrored sky by half as much again: a hall's steel shows no sky.
    pbr_input.specular_occlusion *= mix(1.0, HALL_FILL * 0.5, hall);

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

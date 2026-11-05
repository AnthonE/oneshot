// The sea (look/water.rs): the swell in the vertex stage; per pixel a body
// graded as a volume by the water column under it, the shore wash, Bevy's
// lighting on the body alone, then the sky through Schlick's Fresnel and
// the sun's GGX glint, blended premultiplied. After Gates'
// shaders/water.wgsl (MIT, © 2026 MoreRight DAO; NOTICE): its browser path
// (the swell's normal per vertex, the column from the vertex, no depth
// prepass), the one that runs on WebGL2: one uniform struct, constant
// indices into the wave table, every angle reduced in cycles before sin,
// every derivative at the top of the fragment, textureSampleLevel under
// branches. The numbers are water.rs's; the argument for each term is there.

#import bevy_pbr::{
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_functions,
    mesh_view_bindings::{view, lights},
    mesh_view_types,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    lighting,
    shadows,
    view_transformations::position_world_to_clip,
}

// Field order and names mirror `water::WaterParams` exactly.
struct Water {
    // xyz `EXTINCT`, w `ALPHA_MAX`.
    optics: vec4<f32>,
    // xyz `SCATTER_ALBEDO`, w `WATER_IOR`.
    scatter: vec4<f32>,
    // xyz `FOAM_BODY`, w `FOAM_ALPHA`.
    foam: vec4<f32>,
    // xyz the clear sky's zenith, cd/m²; w the analytic sky's share (1 on
    // the desktop, 0 in a browser).
    zenith: vec4<f32>,
    // xyz the clear sky's horizon, cd/m²; w `sky::AIR_FLOOR`.
    horizon: vec4<f32>,
    // xyz toward the sun; w cd/m² per texel of the sky cube.
    sun: vec4<f32>,
    // x F0, y `GLINT_A2_NEAR`, z `GLINT_A2_FAR`, w `GLINT_FAR_M`.
    glint: vec4<f32>,
    // x `EDGE_M`, y `FOAM_SURGE_FLOOR`, z `HORIZON_NDOTV`, w the swell's
    // hidden slope variance.
    shore: vec4<f32>,
    // A wave a slot: xy its direction, z one over its length (cycles a
    // metre), w its amplitude.
    waves: array<vec4<f32>, 4>,
    // The waves' phases, cycles.
    phase: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> water: Water;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var sky_cube: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var sky_sampler: sampler;

// Deep by fiat (`water::DEEP_M`): saturates every curve here.
const DEEP_M: f32 = 64.0;
const TAU: f32 = 6.28318530718;

// ---------------------------------------------------------------------------
// The vertex stage: the swell.
// ---------------------------------------------------------------------------

// An angle given in cycles, reduced to [-π, π) before any trig: WGSL
// promises `sin` only near zero, and WebGL2 promises nothing.
fn reduced(cycles: f32) -> f32 {
    return TAU * (fract(cycles + 0.5) - 0.5);
}

// The crest-sharpened sine, `2u² − ¾`, and its derivative.
fn wave_shape(theta: f32) -> vec2<f32> {
    let u = 0.5 + 0.5 * sin(theta);
    return vec2<f32>(2.0 * u * u - 0.75, 2.0 * u * cos(theta));
}

// One wave of the swell at `local`: its height and gradient, `(h, gx, gz)`.
// `w` is its `waves` slot, `share` the vertex's band weight times shoaling.
fn swell_term(w: vec4<f32>, phase: f32, share: f32, local: vec2<f32>) -> vec3<f32> {
    let s = wave_shape(reduced(dot(w.xy, local) * w.z + phase));
    let a = w.w * share;
    return vec3<f32>(a * s.x, (a * s.y * TAU * w.z) * w.xy);
}

// The position is the vertex's world xz flat at the sea level (the
// transform is the identity); `color` carries each wave's share; `uv_b`
// arrives as `[wash, water column]` and passes through.
@vertex
fn vertex(in: Vertex) -> VertexOutput {
    let local = in.position.xz;
    var share = vec4<f32>(1.0);
#ifdef VERTEX_COLORS
    share = in.color;
#endif
    // Unrolled: constant indices only, which WebGL2's GLSL takes without a
    // second thought.
    let sw = swell_term(water.waves[0], water.phase.x, share.x, local)
        + swell_term(water.waves[1], water.phase.y, share.y, local)
        + swell_term(water.waves[2], water.phase.z, share.z, local)
        + swell_term(water.waves[3], water.phase.w, share.w, local);

    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    let p = vec3<f32>(in.position.x, in.position.y + sw.x, in.position.z);
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(p, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        normalize(vec3<f32>(-sw.y, 1.0, -sw.z)),
        in.instance_index,
    );
#ifdef VERTEX_UVS_A
    out.uv = in.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = in.uv_b;
#endif
#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local,
        in.tangent,
        in.instance_index,
    );
#endif
#ifdef VERTEX_COLORS
    // The shares are spent: Bevy must not multiply them into a base colour.
    out.color = vec4<f32>(1.0);
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = in.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        in.instance_index,
        world_from_local[3],
    );
#endif
    return out;
}

// ---------------------------------------------------------------------------
// The fragment stage's helpers.
// ---------------------------------------------------------------------------

fn hash2(p: vec2<i32>) -> f32 {
    let q = bitcast<vec2<u32>>(p);
    var h = q.x * 374761393u + q.y * 668265263u;
    h = (h ^ (h >> 13u)) * 1274126177u;
    h = h ^ (h >> 16u);
    return f32(h & 0xffffffu) / 16777216.0;
}

// Value noise in world metres over `1 / scale`, in [0, 1].
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let c = vec2<i32>(i);
    let a = hash2(c);
    let b = hash2(c + vec2<i32>(1, 0));
    let d = hash2(c + vec2<i32>(0, 1));
    let e = hash2(c + vec2<i32>(1, 1));
    return mix(mix(a, b, u.x), mix(d, e, u.x), u.y);
}

// The clear sky's air-mass weight toward elevation `y`: 0 at the zenith, 1
// at the horizon (`sky::backdrop_at`'s curve).
fn air_weight(y: f32) -> f32 {
    let e = clamp(y, 0.0, 1.0);
    let f = water.horizon.w;
    let m0 = 1.0 / (1.0 + f);
    return clamp((1.0 / (e + f) - m0) / (1.0 / f - m0), 0.0, 1.0);
}

// Sky radiance toward `r` (unit, r.y >= 0), cd/m²: the analytic clear sky
// (its share `zenith.w`) plus the cube, looked up toward `rd`, the same
// reflection off the wave's own normal without the ripples: the cube has
// no mips, so a ripple-scattered ray would read its texels as glitter.
// Sampled the way the `Skybox` samples it (z flipped); explicit level, as
// this may run under a branch.
fn sky_radiance(r: vec3<f32>, rd: vec3<f32>) -> vec3<f32> {
    let c = mix(water.zenith.rgb, water.horizon.rgb, air_weight(r.y)) * water.zenith.w;
    let deck = textureSampleLevel(sky_cube, sky_sampler, rd * vec3<f32>(1.0, 1.0, -1.0), 0.0).rgb;
    return c + deck * water.sun.w;
}

// The sun (and any other directional light) off the surface: GGX at
// water's F0, shadowed, with specular anti-aliasing from the normal's
// variance across the pixel (Kaplanyan & Hoffman 2016) and the slope the
// pixel cannot resolve (`extra`) widening the lobe.
fn sun_glint(N: vec3<f32>, V: vec3<f32>, wp: vec3<f32>, world_normal: vec3<f32>, flags: u32, n_var: f32, extra: f32) -> vec3<f32> {
    let dist = length(view.world_position.xyz - wp);
    var a2 = mix(water.glint.y, water.glint.z, smoothstep(0.0, water.glint.w, dist)) + extra;
    a2 = min(a2 + min(2.0 * n_var, 0.18), 1.0);
    let roughness = sqrt(a2);
    let perceptual = sqrt(roughness);
    let NdotV = max(dot(N, V), 1e-4);

    var input: lighting::LightingInput;
    input.layers[lighting::LAYER_BASE].N = N;
    input.layers[lighting::LAYER_BASE].R = reflect(-V, N);
    input.layers[lighting::LAYER_BASE].NdotV = NdotV;
    input.layers[lighting::LAYER_BASE].perceptual_roughness = perceptual;
    input.layers[lighting::LAYER_BASE].roughness = roughness;
    input.P = wp;
    input.V = V;
    input.diffuse_color = vec3<f32>(0.0);
    input.F0_ = vec3<f32>(water.glint.x);
    input.F_ab = lighting::F_AB(perceptual, NdotV);
#ifdef STANDARD_MATERIAL_CLEARCOAT
    input.clearcoat_strength = 0.0;
    input.layers[1].N = N;
    input.layers[1].R = reflect(-V, N);
    input.layers[1].NdotV = NdotV;
    input.layers[1].perceptual_roughness = 1.0;
    input.layers[1].roughness = 1.0;
#endif
#ifdef STANDARD_MATERIAL_ANISOTROPY
    input.anisotropy = 0.0;
    input.Ta = vec3<f32>(1.0, 0.0, 0.0);
    input.Ba = vec3<f32>(0.0, 0.0, 1.0);
#endif

    let view_z = dot(vec4<f32>(
        view.view_from_world[0].z,
        view.view_from_world[1].z,
        view.view_from_world[2].z,
        view.view_from_world[3].z
    ), vec4<f32>(wp, 1.0));

    var total = vec3<f32>(0.0);
    let n = lights.n_directional_lights;
    for (var i: u32 = 0u; i < n; i = i + 1u) {
        var shadow = 1.0;
        if ((flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u
                && (lights.directional_lights[i].flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u) {
            shadow = shadows::fetch_directional_shadow(i, vec4<f32>(wp, 1.0), world_normal, view_z);
        }
        total += lighting::directional_light(i, &input, false) * shadow;
    }
    return total * view.exposure;
}

// ---------------------------------------------------------------------------
// The fragment stage.
// ---------------------------------------------------------------------------

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    let wp = in.world_position.xyz;
    let V0 = normalize(view.world_position.xyz - wp);
    var vin = in;
#ifdef VERTEX_COLORS
    vin.color = vec4<f32>(1.0);
#endif
    // The swell's normal from the vertex. A face turned past the eye at
    // the horizon reflects the sea under it, which a mirror sky cannot
    // draw: it is bent back to graze, never past the flat sea's own.
    let nw = normalize(in.world_normal);
    let graze = min(water.shore.z, max(dot(nw, V0), 0.0));
    vin.world_normal = normalize(nw + V0 * max(0.0, graze - dot(nw, V0)));

    // Bevy's inputs, the ripple normal map applied through its own tangent
    // path.
    var pbr_input = pbr_input_from_standard_material(vin, is_front);
    let N = pbr_input.N;
    let V = pbr_input.V;
    // Every derivative before any branch: naga does not hold the fragment
    // stage to uniform control flow, so a `dpdx` under an `if` is a smear,
    // not an error.
    let dndx = dpdx(N);
    let dndy = dpdy(N);
    let n_var = 0.5 * (dot(dndx, dndx) + dot(dndy, dndy));
    let dist = length(view.world_position.xyz - wp);

    // 1 · The water behind this pixel, vertical metres: the column the
    // vertex read off the terrain. And the wash the vertex carries,
    // breathing with the longest wave's own phase: a moving edge cannot be
    // a hard edge.
#ifdef VERTEX_UVS_B
    let dz = max(in.uv_b.y, 0.0);
    let s0 = sin(reduced(dot(water.waves[0].xy, wp.xz) * water.waves[0].z + water.phase.x));
    let surge = water.shore.y + (1.0 - water.shore.y) * (0.5 + 0.5 * s0);
    var foam = clamp(in.uv_b.x, 0.0, 1.0) * surge;
#else
    let dz = DEEP_M;
    var foam = 0.0;
#endif

    // 2 · The body, over the refracted path (Snell bends the ray toward
    // the vertical on the way in, so the shallows stay clear from a beach
    // eye): `S·(1 − T)` and `α = 1 − mean(T)` with `T = e^{−σ·path}`.
    let sigma = water.optics.xyz;
    let albedo = water.scatter.xyz;
    let ior = water.scatter.w;
    let cos_i = clamp(abs(V.y), 0.0, 1.0);
    let sin2_t = (1.0 - cos_i * cos_i) / (ior * ior);
    let path = dz / sqrt(max(1.0 - sin2_t, 1e-4));
    let T = exp(-sigma * path);
    let alpha_max = water.optics.w;
    let a_body = clamp((1.0 - (T.x + T.y + T.z) / 3.0) * alpha_max, 0.0, alpha_max);
    var body = vec4<f32>(albedo * (1.0 - T), a_body);

    // 3 · Foam. The vertex's wash is a band along the depth contours, and
    // a band of one even white reads as a drawn streak: break it into
    // patches in world space. The fine octave settles to its mean before
    // it is finer than a pixel. Half Gates' coverage: a shelf a metre
    // deep is all wash band, and at his strength it read as milk.
    let settle = smoothstep(25.0, 90.0, dist);
    if foam > 0.0 {
        let fine = mix(vnoise(wp.xz * 0.9 + vec2<f32>(5.0, 11.0)), 0.5, settle);
        let clump = vnoise(wp.xz * 0.3) * 0.6 + fine * 0.4;
        foam = clamp(foam * (0.12 + 0.65 * clump), 0.0, 1.0);
    }
    // The contact line itself is a polygon intersection: everything fades
    // in from it, or it is drawn as a line.
    let edge = smoothstep(0.0, water.shore.x, dz);
    foam = foam * smoothstep(0.0, 0.04, dz);
    let fa = water.foam.w;
    body = vec4<f32>(
        body.rgb + (water.foam.rgb * fa - body.rgb) * foam,
        body.a + (fa - body.a) * foam
    );

    // 4 · Bevy lights the body: sun, shadows, the hemisphere fill. No
    // specular from it: the sky and the sun are drawn below, and its
    // environment specular is the fill. SSAO at this pixel is the seabed's.
    pbr_input.material.base_color = body;
    pbr_input.material.reflectance = vec3<f32>(0.0);
    pbr_input.diffuse_occlusion = vec3<f32>(1.0);
    var color = apply_pbr_lighting(pbr_input);

    // 5 · The reflection. Where the ripple normal turns faster than the
    // pixels can follow, a mirror reads it as speckle; a real pixel there
    // averages many facets, so reflect off their mean, the wave's own
    // normal, by how fast N is changing across the pixel.
    let calm = 0.8 * smoothstep(0.002, 0.02, n_var);
    let Nr = normalize(mix(N, pbr_input.world_normal, calm));
    let NdotV = clamp(dot(Nr, V), 0.0, 1.0);
    let F0 = water.glint.x;
    let smooth_share = edge * (1.0 - foam);
    let F = (F0 + (1.0 - F0) * pow(1.0 - NdotV, 5.0)) * smooth_share;
    var R = reflect(-V, Nr);
    // A ray reflected downward would meet the next wave: read the horizon.
    R = normalize(vec3<f32>(R.x, max(R.y, 0.0) + 1e-3, R.z));
    var Rd = reflect(-V, pbr_input.world_normal);
    Rd = normalize(vec3<f32>(Rd.x, max(Rd.y, 0.0) + 1e-3, Rd.z));
    let sky = sky_radiance(R, Rd) * view.exposure;
    // The swell's slope the far pixel cannot resolve, as roughness.
    let hid = water.shore.w * smoothstep(0.0, water.glint.w, dist);
    let glint = sun_glint(N, V, wp, pbr_input.world_normal, pbr_input.flags, n_var, hid) * smooth_share;
    // 6 · Premultiplied: the sky replaces what Fresnel takes from below,
    // and attenuates the background as well as adding to it.
    color = vec4<f32>(
        color.rgb * (1.0 - F) + sky * F + glint,
        color.a + (1.0 - color.a) * F
    );

    // Fog and the rest of Bevy's post-lighting work act on the straight
    // colour; the premultiply is ours (`Premultiplied` passes it through).
    let a = max(color.a, 1e-4);
    let post = main_pass_post_lighting_processing(pbr_input, vec4<f32>(color.rgb / a, color.a));
    var out: FragmentOutput;
    out.color = vec4<f32>(post.rgb * a, post.a);
    return out;
}

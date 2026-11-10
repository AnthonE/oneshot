// A Veil craft's shell (`look/craft.rs`): `bevy_pbr`'s own fragment with a
// beetle's wing case over it. Its paint turns from violet toward blue where
// it is seen edge on, and over the game's lighting it reflects the sky
// photograph the guns do (`gun_skin.rs`'s cube), as a clear coat does: little
// face on, nearly all of it at the edge. A craft's skin is smooth and has no
// map (`tools/models/veil.py`): its colours are its mesh's (COLOR_0, the
// paint in the shade each vertex stands in), and their alpha is that shade
// alone, which is what dims a reflection down in a seam.
//
// And it has been flown: a mottle in its tone and gloss and hairline scuffs
// running fore and aft, laid by where each pixel lies on the craft (its two
// UV sets carry that, metres: along and across it, and up it), each faded
// out before a pixel is too big to hold it and all of it by a few metres
// from the eye, so a far craft is clean paint and a near one has grain.
//
// Uniforms, one cube and arithmetic only, so it binds and runs the same on
// WebGL2.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

struct Shell {
    // rgb: what its paint turns toward edge on, linear; a: how far (0 none).
    edge: vec4<f32>,
    // x: how bright the sky's photograph is in it, on screen (its mean is 1);
    // y: the photograph's blur, in its levels (0 a mirror); z: how much of it
    // shows face on (a clear coat's is 0.04); w: how much of the sky's own
    // colour the reflection keeps.
    sky: vec4<f32>,
    // x: how much grain it has (0 none: glass).
    grain: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> shell: Shell;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var env_tex: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var env_smp: sampler;

// How sharply the paint turns at the edge (a power of how edge on it is).
const TURN: f32 = 3.0;
// The mottle: cycles a metre (patches a hand across, with broad ones four
// times the size under them), how far the noise is stretched (a sum of
// noises keeps near its middle), how far it moves the paint's tone either
// way and its roughness either way. Little of that: a coat this glossy
// shows a highlight where it is smooth and none where it is rough, so more
// cut a highlight into islands, which is banding and not wear.
const MOTTLE_FREQ: f32 = 7.0;
const MOTTLE_GAIN: f32 = 2.2;
const MOTTLE_TONE: f32 = 0.1;
const MOTTLE_ROUGH: f32 = 0.05;
// The scratches: cycles a metre across the craft, how many times longer
// than wide they run along it, the noise's level one shows from (about a
// fifth of the skin has one), how much rougher and paler they are, and
// how much of the coat's reflection they take. Hairlines: a pixel or two
// at arm's length, gone by three metres.
const SCUFF_FREQ: f32 = 110.0;
const SCUFF_LONG: f32 = 30.0;
const SCUFF_FROM: f32 = 0.64;
const SCUFF_ROUGH: f32 = 0.25;
const SCUFF_PALE: f32 = 0.3;
const SCUFF_DULL: f32 = 0.7;
// All its grain is whole to here and gone by here, metres from the eye.
const GRAIN_NEAR: f32 = 3.5;
const GRAIN_FAR: f32 = 8.0;

fn hash3(c: vec3<i32>) -> f32 {
    // WGSL will not mix `^` with `*` unparenthesised.
    var h = (bitcast<u32>(c.x) * 0x8da6b343u) ^ (bitcast<u32>(c.y) * 0xd8163841u) ^ (bitcast<u32>(c.z) * 0xcb1ab31fu);
    h = (h ^ (h >> 16u)) * 0x7feb352du;
    h = (h ^ (h >> 15u)) * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return f32(h >> 8u) * (1.0 / 16777216.0);
}

// Value noise over a lattice, in 0..1 (the truck's and the level's).
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

// How much of a noise at `freq` cycles a metre a pixel `px` metres across
// can carry: whole at six pixels a cycle, gone at two.
fn band(px: f32, freq: f32) -> f32 {
    return clamp(1.5 - px * freq * 3.0, 0.0, 1.0);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef VERTEX_COLORS
    let shade = clamp(in.color.a, 0.0, 1.0);
#else
    let shade = 1.0;
#endif
    // Its grain, by where the pixel lies on the craft (a mesh without the
    // two UV sets that say so has none).
    var mottle = 0.0;
    var scuff = 0.0;
#ifdef VERTEX_UVS_A
#ifdef VERTEX_UVS_B
    let wp = in.world_position.xyz;
    let px = max(length(dpdx(wp)), length(dpdy(wp)));
    let nearby = (1.0 - smoothstep(GRAIN_NEAR, GRAIN_FAR, distance(view.world_position.xyz, wp))) * shell.grain.x;
    let at = vec3<f32>(in.uv.x, in.uv.y, in.uv_b.x);
    mottle = clamp((0.25 * vnoise(at * MOTTLE_FREQ * 0.25 + vec3<f32>(2.7, 8.1, 5.3)) + 0.5 * vnoise(at * MOTTLE_FREQ)
        + 0.25 * vnoise(at * MOTTLE_FREQ * 2.7 + vec3<f32>(7.3, 1.9, 4.1)) - 0.5) * MOTTLE_GAIN, -1.0, 1.0)
        * band(px, MOTTLE_FREQ) * nearby;
    let streak = vnoise(vec3<f32>(at.x * SCUFF_FREQ / SCUFF_LONG, at.y * SCUFF_FREQ, at.z * SCUFF_FREQ));
    scuff = smoothstep(SCUFF_FROM, SCUFF_FROM + 0.1, streak) * band(px, SCUFF_FREQ) * nearby;
    let grained = pbr_input.material.base_color.rgb * (1.0 + MOTTLE_TONE * mottle) * (1.0 + SCUFF_PALE * scuff);
    pbr_input.material.base_color = vec4<f32>(grained, pbr_input.material.base_color.a);
    pbr_input.material.perceptual_roughness = clamp(
        pbr_input.material.perceptual_roughness + MOTTLE_ROUGH * mottle + SCUFF_ROUGH * scuff, 0.045, 1.0);
#endif
#endif
    let n = pbr_input.N;
    let v = pbr_input.V;
    let nv = clamp(dot(n, v), 0.0, 1.0);
    // Edge on, the paint is another colour, as a wing case's is; in the same
    // shade.
    let turn = pow(1.0 - nv, TURN) * shell.edge.a;
    let paint = pbr_input.material.base_color.rgb;
    pbr_input.material.base_color = vec4<f32>(mix(paint, shell.edge.rgb * shade, turn), pbr_input.material.base_color.a);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
        // The sky in its coat.
        let r = reflect(-v, n);
        let photo = textureSampleLevel(env_tex, env_smp, vec3<f32>(r.x, r.y, -r.z), shell.sky.y).rgb;
        let grey = vec3<f32>(dot(photo, vec3<f32>(0.2126, 0.7152, 0.0722)));
        let sky = mix(grey, photo, shell.sky.w);
        let fresnel = shell.sky.z + (1.0 - shell.sky.z) * pow(1.0 - nv, 5.0);
        let seen = pbr_input.diffuse_occlusion * pbr_input.specular_occlusion * shade * shade * (1.0 - SCUFF_DULL * scuff) * (1.0 - 0.35 * max(mottle, 0.0));
        out.color = vec4<f32>(out.color.rgb + sky * fresnel * shell.sky.x * seen, out.color.a);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

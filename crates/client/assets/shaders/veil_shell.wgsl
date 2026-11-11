// A Veil craft's shell (`look/craft.rs`): `bevy_pbr`'s own fragment with an
// alloy's coat over it. A shell is metal with its colour in it, dark face on:
// what it shows is what it reflects, so over the game's lighting it reflects
// the sky photograph the guns do (`gun_skin.rs`'s cube), tinted by the alloy
// and blurred by its roughness, with a thin clear lacquer's white sheen over
// that, and its colour turns from violet toward blue where it is seen edge
// on. A craft's skin has no map (`tools/models/veil.py`): its colours are its
// mesh's (COLOR_0, the paint in the shade each vertex stands in), and their
// alpha is that shade alone, which is what dims a reflection down in a seam.
//
// And it has been flown, which is what makes it a thing and not a drawing of
// one. Laid by where each pixel lies on the craft (its two UV sets carry
// that, metres: along and across it, and up it):
//   - a mottle in its tone and gloss, and hairline scuffs fore and aft;
//   - grime run down its steep faces in streaks;
//   - the coat chipped through to bare alloy along its worn edges (how worn
//     a corner is rides in the second UV set), ragged pixel by pixel up
//     close and a faint paler edge from afar;
//   - dust: settled in what is shaded (a seam, a louvre), on what faces the
//     sky, and thrown up its belly; matte, so it is where the sheen is not.
// Each is faded out before a pixel is too big to hold it.
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
    // y: the photograph's blur at full roughness, in its levels; z: how much
    // of a clear lacquer's sheen lies over it (0 none: bare iron); w: how
    // much of the sky's own colour the reflection keeps.
    sky: vec4<f32>,
    // x: how much grain it has (0 none: glass); y: how much dust it takes;
    // z: how far its worn edges chip to bare alloy (0: it has no coat).
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
const MOTTLE_TONE: f32 = 0.12;
const MOTTLE_ROUGH: f32 = 0.07;
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
// All its grain is whole to here and gone by here, metres from the eye
// (past a flier's own chase camera: its back is what its pilot sees).
const GRAIN_NEAR: f32 = 6.0;
const GRAIN_FAR: f32 = 15.0;
// The streaks of grime down a steep face: cycles a metre along the craft,
// how many times longer they run down it, the noise's level one shows from,
// how much darker and rougher it is.
const RUN_FREQ: f32 = 16.0;
const RUN_LONG: f32 = 9.0;
const RUN_FROM: f32 = 0.6;
const RUN_DARK: f32 = 0.28;
const RUN_ROUGH: f32 = 0.14;
// The chips: the bare alloy under the coat (linear) and its roughness; the
// noise's cycles a metre; how worn a corner must be for the coat to be gone
// from half of it, how ragged its edge is (the noise's share of that), and
// how much of a worn edge is bare from too far to see a chip.
const BARE: vec3<f32> = vec3<f32>(0.33, 0.34, 0.38);
const BARE_ROUGH: f32 = 0.42;
const CHIP_FREQ: f32 = 26.0;
const CHIP_FROM: f32 = 0.6;
const CHIP_RAG: f32 = 0.55;
const CHIP_FAR: f32 = 0.08;
// The dust (linear), and its roughness; how much of it lies in full shade,
// on what faces straight up, and at the belly; the height over the ground,
// metres, the thrown dust is whole to and gone by; its patches' cycles a
// metre; and how much of the paint a full coat of it hides.
const DUST: vec3<f32> = vec3<f32>(0.115, 0.095, 0.075);
const DUST_ROUGH: f32 = 0.92;
const DUST_SHADE: f32 = 1.25;
const DUST_LIE: f32 = 0.16;
const DUST_THROWN: f32 = 0.3;
const DUST_LOW: vec2<f32> = vec2<f32>(0.3, 0.8);
const DUST_FREQ: f32 = 2.6;
const DUST_COVER: f32 = 0.6;
// The lacquer's own blur, in the photograph's levels: sharper than the
// alloy under it.
const LACQUER_LEVEL: f32 = 0.8;
// What it reflects of the map's own fill seen edge on (face on it has all
// of it: that is its colour in shade), and of the white sheen every surface
// has seen edge on: a share each (`gun_skin.wgsl`'s reason: seen along its
// length, as a craft's back is from its own saddle, all of that sheen is a
// pale craft).
const FILL_GRAZE: f32 = 0.35;
const SHEEN: f32 = 0.15;

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

// What a reflection keeps of the light it meets, by roughness and the view's
// angle: Karis' fit of the split-sum's table (2014), scale and bias on F0
// (`gun_skin.wgsl`'s).
fn env_brdf(rough: f32, nv: f32) -> vec2<f32> {
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = rough * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * nv)) * r.x + r.y;
    return vec2<f32>(-1.04, 1.04) * a004 + r.zw;
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
    var c = pbr_input.material.base_color.rgb;
    var rough = pbr_input.material.perceptual_roughness;
    var metal = pbr_input.material.metallic;
    // How much of its sheen is gone (a scratch, dust), and of its coat
    // (a chip, dust): bare alloy and dust do not turn at the edge.
    var dull = 0.0;
    var bared = 0.0;
    // Its wear, by where the pixel lies on the craft (a mesh without the
    // two UV sets that say so has none).
#ifdef VERTEX_UVS_A
#ifdef VERTEX_UVS_B
    let wp = in.world_position.xyz;
    let px = max(length(dpdx(wp)), length(dpdy(wp)));
    let nearby = (1.0 - smoothstep(GRAIN_NEAR, GRAIN_FAR, distance(view.world_position.xyz, wp))) * shell.grain.x;
    let at = vec3<f32>(in.uv.x, in.uv.y, in.uv_b.x);
    let mottle = clamp((0.25 * vnoise(at * MOTTLE_FREQ * 0.25 + vec3<f32>(2.7, 8.1, 5.3)) + 0.5 * vnoise(at * MOTTLE_FREQ)
        + 0.25 * vnoise(at * MOTTLE_FREQ * 2.7 + vec3<f32>(7.3, 1.9, 4.1)) - 0.5) * MOTTLE_GAIN, -1.0, 1.0)
        * band(px, MOTTLE_FREQ) * nearby;
    let scratch = vnoise(vec3<f32>(at.x * SCUFF_FREQ / SCUFF_LONG, at.y * SCUFF_FREQ, at.z * SCUFF_FREQ));
    let scuff = smoothstep(SCUFF_FROM, SCUFF_FROM + 0.1, scratch) * band(px, SCUFF_FREQ) * nearby;
    // Grime run down what is steep.
    let steep = 1.0 - smoothstep(0.45, 0.85, abs(pbr_input.N.y));
    let run = vnoise(vec3<f32>(at.x * RUN_FREQ, at.y * RUN_FREQ, at.z * RUN_FREQ / RUN_LONG));
    let streak = smoothstep(RUN_FROM, RUN_FROM + 0.25, run) * steep * band(px, RUN_FREQ) * shell.grain.x;
    c = c * (1.0 + MOTTLE_TONE * mottle) * (1.0 + SCUFF_PALE * scuff) * (1.0 - RUN_DARK * streak);
    rough = rough + MOTTLE_ROUGH * mottle + SCUFF_ROUGH * scuff + RUN_ROUGH * streak;
    // The coat chipped off its worn edges.
    let worn = clamp(1.0 - in.uv_b.y, 0.0, 1.0) * shell.grain.z;
    let rag = 0.6 * vnoise(at * CHIP_FREQ) + 0.4 * vnoise(at * CHIP_FREQ * 3.3 + vec3<f32>(3.1, 7.7, 1.3));
    let chip = mix(smoothstep(0.5, 1.0, worn) * CHIP_FAR,
        smoothstep(CHIP_FROM, CHIP_FROM + 0.06, worn + (rag - 0.5) * CHIP_RAG), band(px, CHIP_FREQ));
    c = mix(c, BARE * shade, chip);
    metal = mix(metal, 1.0, chip);
    rough = mix(rough, BARE_ROUGH, chip);
    // Dust over all of that.
    let lie = clamp(pbr_input.N.y, 0.0, 1.0);
    let low = 1.0 - smoothstep(DUST_LOW.x, DUST_LOW.y, at.z);
    let patches = 0.55 * vnoise(at * DUST_FREQ + vec3<f32>(4.2, 9.1, 6.6))
        + 0.3 * mix(0.5, vnoise(at * DUST_FREQ * 4.3), band(px, DUST_FREQ * 4.3))
        + 0.15 * mix(0.5, vnoise(at * DUST_FREQ * 17.0), band(px, DUST_FREQ * 17.0));
    let dust = clamp(((1.0 - shade) * DUST_SHADE + lie * lie * DUST_LIE + low * DUST_THROWN) * (0.4 + 1.2 * patches) - 0.1,
        0.0, 1.0) * shell.grain.y;
    c = mix(c, DUST * shade, dust * DUST_COVER);
    rough = mix(rough, DUST_ROUGH, dust);
    metal = mix(metal, 0.0, dust * DUST_COVER);
    dull = clamp(max(dust, SCUFF_DULL * scuff + 0.35 * max(mottle, 0.0)), 0.0, 1.0);
    bared = clamp(chip + dust, 0.0, 1.0);
#endif
#endif
    let n = pbr_input.N;
    let v = pbr_input.V;
    let nv = clamp(dot(n, v), 0.0, 1.0);
    // Edge on, the alloy is another colour, as a wing case's is; in the same
    // shade.
    let turn = pow(1.0 - nv, TURN) * shell.edge.a * (1.0 - bared);
    c = mix(c, shell.edge.rgb * shade, turn);
    rough = clamp(rough, 0.045, 1.0);
    pbr_input.material.base_color = vec4<f32>(c, pbr_input.material.base_color.a);
    pbr_input.material.perceptual_roughness = rough;
    pbr_input.material.metallic = clamp(metal, 0.0, 1.0);

    let own_spec = pbr_input.specular_occlusion;
    pbr_input.specular_occlusion = own_spec * mix(FILL_GRAZE, 1.0, nv);
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
        // The sky in it: the alloy's own colour of it, as blurred as the
        // alloy is rough, and the lacquer's white sheen over that, little
        // face on and nearly all of it at the edge.
        let r = reflect(-v, n);
        let dir = vec3<f32>(r.x, r.y, -r.z);
        let photo = textureSampleLevel(env_tex, env_smp, dir, rough * shell.sky.y).rgb;
        let sharp = textureSampleLevel(env_tex, env_smp, dir, LACQUER_LEVEL).rgb;
        let sky = mix(vec3<f32>(dot(photo, vec3<f32>(0.2126, 0.7152, 0.0722))), photo, shell.sky.w);
        let sheen = mix(vec3<f32>(dot(sharp, vec3<f32>(0.2126, 0.7152, 0.0722))), sharp, shell.sky.w);
        let f0 = mix(vec3<f32>(0.04), c, pbr_input.material.metallic);
        let ab = env_brdf(rough, nv);
        let lacquer = (0.04 + 0.96 * pow(1.0 - nv, 5.0)) * shell.sky.z * (1.0 - dull);
        let seen = pbr_input.diffuse_occlusion * own_spec * shade * shade;
        out.color = vec4<f32>(out.color.rgb + (sky * (f0 * ab.x + ab.y * SHEEN) + sheen * lacquer) * shell.sky.x * seen, out.color.a);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

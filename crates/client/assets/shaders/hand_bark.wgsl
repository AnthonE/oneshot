// Our own hands in view (`look/bark.rs`): `bevy_pbr`'s own fragment with
// Triple T's bark laid over it. His map is one atlas for all of him, a
// millimetre a texel on a hand: bark across a room, and a smear on the
// nearest thing on the screen, where a pixel is a quarter of that. So up
// close his bark is a photograph of bark (`tools/bark.py`'s cut), a sixth of
// a millimetre a texel, laid by where a pixel lies in the hand's own frame:
// along each of its three axes, each laying taking the faces that look its
// way, the furrows running the forearm's length. Its normals lean the
// light, its occlusion shades its furrows, and the atlas stays under it as
// his own light and dark, so it is the arm everyone else sees.
//
// One uniform and two maps, so it binds and runs the same on WebGL2.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_functions::{get_local_from_world, get_world_from_local},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
}

struct Bark {
    // x: tiles of the photograph a metre of the hand; y: how much of the
    // atlas' own light and dark the bark keeps; z: the atlas' middle (linear
    // luma), which that is read against; w: how sharply one laying gives way
    // to the next.
    lay: vec4<f32>,
    // x: the bark's roughness on a ridge, y: in a furrow; z: how much of its
    // occlusion shades the light it stands in.
    finish: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> bark: Bark;
// The photograph's colour.
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var colour_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var bark_smp: sampler;
// Its normal's x and y (OpenGL's green), and its occlusion where the z was.
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var relief_tex: texture_2d<f32>;

// How far the atlas' tone may take the bark down and up.
const TONE_MIN: f32 = 0.5;
const TONE_MAX: f32 = 1.5;

// A laying's relief: its normal in the map's own frame (x along u, y along
// v, z out), and its occlusion.
fn relief(uv: vec2<f32>) -> vec4<f32> {
    let t = textureSample(relief_tex, bark_smp, uv);
    // The map's green is up its picture, which is down its v.
    let xy = vec2<f32>(t.r * 2.0 - 1.0, 1.0 - t.g * 2.0);
    return vec4<f32>(xy, sqrt(max(1.0 - dot(xy, xy), 0.0)), t.b);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    // Where the pixel lies on the hand, and which way the hand faces there,
    // in the hand's own frame (the forearm lies along y).
    let from_local = get_world_from_local(in.instance_index);
    let to_local = get_local_from_world(in.instance_index);
    let p = (to_local * vec4<f32>(in.world_position.xyz, 1.0)).xyz * bark.lay.x;
    let n = normalize(transpose(mat3x3<f32>(from_local[0].xyz, from_local[1].xyz, from_local[2].xyz)) * pbr_input.N);
    var w = pow(abs(n), vec3<f32>(bark.lay.w));
    w = w / (w.x + w.y + w.z);

    // The three layings: across x (its u along z), across y, across z; the
    // photograph's own up along y in the two that have it.
    let cx = textureSample(colour_tex, bark_smp, p.zy).rgb;
    let cy = textureSample(colour_tex, bark_smp, p.xz).rgb;
    let cz = textureSample(colour_tex, bark_smp, p.xy).rgb;
    let rx = relief(p.zy);
    let ry = relief(p.xz);
    let rz = relief(p.xy);

    // His own light and dark over the photograph's colour.
    let atlas = pbr_input.material.base_color.rgb;
    let tone = clamp(dot(atlas, vec3<f32>(0.2126, 0.7152, 0.0722)) / max(bark.lay.z, 1e-4), TONE_MIN, TONE_MAX);
    let colour = (cx * w.x + cy * w.y + cz * w.z) * mix(1.0, tone, bark.lay.y);
    pbr_input.material.base_color = vec4<f32>(colour, pbr_input.material.base_color.a);

    let open = rx.w * w.x + ry.w * w.y + rz.w * w.z;
    pbr_input.material.perceptual_roughness = mix(bark.finish.y, bark.finish.x, open);
    let shade = mix(1.0, open, bark.finish.z);
    pbr_input.diffuse_occlusion = pbr_input.diffuse_occlusion * shade;
    pbr_input.specular_occlusion = pbr_input.specular_occlusion * shade;

    // Each laying's normal leans the hand's own (the "whiteout" blend: its
    // slope added to the face's, its height along the face's own way), then
    // the three together, back out to the world.
    let nx = vec3<f32>(rx.xy + n.zy, rx.z * n.x);
    let ny = vec3<f32>(ry.xy + n.xz, ry.z * n.y);
    let nz = vec3<f32>(rz.xy + n.xy, rz.z * n.z);
    let leant = normalize(nx.zyx * w.x + ny.xzy * w.y + nz.xyz * w.z);
    pbr_input.N = normalize(transpose(mat3x3<f32>(to_local[0].xyz, to_local[1].xyz, to_local[2].xyz)) * leant);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

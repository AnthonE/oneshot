// A Veil craft's shell (`look/craft.rs`): `bevy_pbr`'s own fragment with a
// beetle's wing case over it. Its paint turns from violet toward blue where
// it is seen edge on, and over the game's lighting it reflects the sky
// photograph the guns do (`gun_skin.rs`'s cube), as a clear coat does: little
// face on, nearly all of it at the edge. A craft's skin is smooth and has no
// map (`tools/models/veil.py`): its colours are its mesh's (COLOR_0, the
// paint in the shade each vertex stands in), and their alpha is that shade
// alone, which is what dims a reflection down in a seam.
//
// Uniforms, one cube and arithmetic only, so it binds and runs the same on
// WebGL2.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
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
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> shell: Shell;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var env_tex: texture_cube<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var env_smp: sampler;

// How sharply the paint turns at the edge (a power of how edge on it is).
const TURN: f32 = 3.0;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef VERTEX_COLORS
    let shade = clamp(in.color.a, 0.0, 1.0);
#else
    let shade = 1.0;
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
        let seen = pbr_input.diffuse_occlusion * pbr_input.specular_occlusion * shade * shade;
        out.color = vec4<f32>(out.color.rgb + sky * fresnel * shell.sky.x * seen, out.color.a);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

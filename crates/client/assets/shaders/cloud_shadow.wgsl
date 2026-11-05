// The cloud deck's shadows on the ground, and the rain's wet on it
// (`look/cloud_shadow.rs`), for the ground's fragment (`ground.wgsl`). After
// Gates' assets/shaders/cloud_shadow.wgsl (MIT, NOTICE).
//
// The rig dims the one sun by the deck over its disk as seen from the map's
// middle (`CloudShadowParams::at`), the same for every player, so this is
// relative to that point: the sun's direct term at a point once more, times
// how much more or less of the sun reaches the point than the middle.
// Nothing where the two agree, so the ground under a fighter or a box stays
// in their light; a slope a hundred metres off goes dark and light as the
// deck's shadows cross it. Bevy's own `directional_light` and shadow fetch,
// so the term is the sun's exactly (its colour through the atmosphere, its
// specular, its shadows: a slope the shadow map darkens is not darkened
// twice).
//
// **The field is a window in the uniform, on every target**: 16 × 16 texels
// of the deck's coverage at the shadows' softness (`sky::SoftField`), bytes
// packed four to a word, the very bytes the rig reads its sun's cover from.
// Not a texture (the ground's pipeline under the atmosphere already binds
// the sixteen sampled textures a WebGPU floor allows) and not a storage
// buffer (WebGL2 has none): one path for the three targets. The window is
// moved with the drift, a write every minute or so.

#import bevy_pbr::{
    mesh_view_bindings::{view, globals, lights},
    mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
    pbr_types::PbrInput,
    pbr_functions::{calculate_diffuse_color, calculate_F0},
    lighting,
    lighting::LAYER_BASE,
    shadows::fetch_directional_shadow,
}

// Field for field `cloud_shadow::CloudShadowParams`.
struct CloudShadow {
    // x 1 where the shadows are drawn, 0 off; y unused; z the field
    // threshold; w the edge.
    cover: vec4<f32>,
    // xy the field's offset at `sun.w`, noise units; zw how fast it moves,
    // noise units a second.
    drift: vec4<f32>,
    // xyz toward the sun; w the `globals.time` the drift was taken at.
    sun: vec4<f32>,
    // x the deck's altitude, m; y noise units a metre; z one over the
    // field's period, noise units; w the field's texels a side.
    deck: vec4<f32>,
    // x the horizon cutoff; y the fade above it; z the share of the sun a
    // full deck takes; w how far a texel's centre sits from its noise
    // coordinate, texels.
    fade: vec4<f32>,
    // xyz the point the sun's light is set by (the map's middle); w unused.
    at: vec4<f32>,
    // xy the window's first column and row, field texels; zw unused.
    window: vec4<f32>,
    // x how wet the ground is; y the soak's value keep; z its chroma
    // stretch; w the darkest a soak may take a colour to, luma.
    wet: vec4<f32>,
    // The window, a row of 16 texels to an entry, four to a word.
    texels: array<vec4<u32>, 16>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(106) var<uniform> cloud: CloudShadow;

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

// Texel `i` of the field, wrapping, `0..=1`, out of the window.
fn field_texel(i: vec2<i32>) -> f32 {
    let n = i32(cloud.deck.w);
    let o = vec2<i32>(cloud.window.xy);
    // Unsigned before the wrap: under WebGL2 a `%` of a negative is
    // undefined. A thousand fields over is past any drift an hour makes.
    let k = vec2<u32>(i - o + vec2<i32>(n * 1024)) % vec2<u32>(u32(n));
    let l = min(k, vec2<u32>(15u));
    let word = cloud.texels[l.y][l.x / 4u];
    return f32((word >> ((l.x % 4u) * 8u)) & 0xffu) / 255.0;
}

// The field at `q`, noise units, bilinear by hand, as `SoftField::at` reads
// it: texel `i` of level 0 is noise coordinate `i · period / N`; a texel of
// level L is the box over 2^L of those, so its centre sits `fade.w` of its
// own texel on.
fn field_at(q: vec2<f32>) -> f32 {
    let u = q * cloud.deck.z * cloud.deck.w - vec2(cloud.fade.w);
    let i = vec2<i32>(floor(u));
    let f = u - floor(u);
    let a = field_texel(i);
    let b = field_texel(i + vec2(1, 0));
    let c = field_texel(i + vec2(0, 1));
    let d = field_texel(i + vec2(1, 1));
    let top = a + (b - a) * f.x;
    let bot = c + (d - c) * f.x;
    return top + (bot - top) * f.y;
}

// How much of the sun reaches `p` (world, m), as the rig counts it at the
// map's middle: `1 −` the shade's share of the deck over the sun. The deck
// rides the middle, so `p` reads it at its offset from there along the ray
// to the sun, and the middle reads what `SkyField::cover_at` reads.
fn sun_through(p: vec3<f32>, l: vec3<f32>, drift: vec2<f32>, fade: f32) -> f32 {
    let up = max(cloud.deck.x - (p.y - cloud.at.y), 0.0);
    let q = (p.xz - cloud.at.xz + l.xz * (up / max(l.y, cloud.fade.x))) * cloud.deck.y + drift;
    let cover = clamp((field_at(q) - cloud.cover.z) / cloud.cover.w, 0.0, 1.0) * fade;
    return 1.0 - cloud.fade.z * cover;
}

// The light to add to `apply_pbr_lighting`'s for the deck's shadows, before
// exposure: zero where the point and the middle see the sun alike.
fn cloud_shadow(in: PbrInput) -> vec3<f32> {
    if cloud.cover.x <= 0.0 {
        return vec3(0.0);
    }
    // Which directional light is the sun: the one pointing where the rig's
    // sun is.
    let count = lights.n_directional_lights;
    var sun = count;
    var best = 0.95;
    for (var i = 0u; i < count; i += 1u) {
        let c = dot(lights.directional_lights[i].direction_to_light, cloud.sun.xyz);
        if c > best {
            best = c;
            sun = i;
        }
    }
    if sun == count {
        return vec3(0.0);
    }
    let l = lights.directional_lights[sun].direction_to_light;
    let fade = clamp((l.y - cloud.fade.x) / cloud.fade.y, 0.0, 1.0);
    if fade <= 0.0 {
        return vec3(0.0);
    }
    let drift = cloud.drift.xy + cloud.drift.zw * (globals.time - cloud.sun.w);
    let here = sun_through(in.world_position.xyz, l, drift, fade);
    let there = sun_through(cloud.at.xyz, l, drift, fade);
    // Under a step of the field's eight bits: nothing anyone sees, and the
    // second look at the sun below is not free.
    if abs(here - there) < 1.0 / 512.0 {
        return vec3(0.0);
    }

    // The sun on this point as `apply_pbr_lighting` lights it.
    let base = in.material.base_color.rgb;
    let metallic = in.material.metallic;
    let perceptual_roughness = in.material.perceptual_roughness;
    let n_dot_v = max(dot(in.N, in.V), 0.0001);
    var li: lighting::LightingInput;
    li.layers[LAYER_BASE].NdotV = n_dot_v;
    li.layers[LAYER_BASE].N = in.N;
    li.layers[LAYER_BASE].R = reflect(-in.V, in.N);
    li.layers[LAYER_BASE].perceptual_roughness = perceptual_roughness;
    li.layers[LAYER_BASE].roughness = lighting::perceptualRoughnessToRoughness(perceptual_roughness);
    li.P = in.world_position.xyz;
    li.V = in.V;
    li.diffuse_color = calculate_diffuse_color(
        base,
        metallic,
        in.material.specular_transmission,
        in.material.diffuse_transmission,
    );
    li.F0_ = calculate_F0(base, metallic, in.material.reflectance);
    li.F_ab = lighting::F_AB(perceptual_roughness, n_dot_v);
    var shadow = 1.0;
    if (in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u
            && (lights.directional_lights[sun].flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
        let view_z = dot(vec4<f32>(
            view.view_from_world[0].z,
            view.view_from_world[1].z,
            view.view_from_world[2].z,
            view.view_from_world[3].z
        ), in.world_position);
        shadow = fetch_directional_shadow(sun, in.world_position, in.world_normal, view_z);
    }
    // The rig lit this point as it lit the middle; this is the difference.
    return lighting::directional_light(sun, &li, true) * shadow * (here / there - 1.0);
}

// How wet the ground is at a surface facing `n`: the map's wet, on what
// faces the sky.
fn ground_wet(n: vec3<f32>) -> f32 {
    return cloud.wet.x * clamp(n.y, 0.0, 1.0);
}

// Gates' `terrain_mesh::wetted`: darker, and more saturated about its own
// luma, never taken under the band's dark end.
fn wetted(c: vec3<f32>, wet: f32) -> vec3<f32> {
    if wet <= 0.0 {
        return c;
    }
    let luma = dot(c, LUMA);
    var value = 1.0;
    if luma > cloud.wet.w {
        value = max(1.0 - wet * (1.0 - cloud.wet.y), cloud.wet.w / luma);
    }
    let chroma = 1.0 + wet * cloud.wet.z;
    return max((vec3(luma) + (c - vec3(luma)) * chroma) * value, vec3(0.0));
}

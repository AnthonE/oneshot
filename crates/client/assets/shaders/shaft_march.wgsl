// Light shafts where Bevy's volumetric pass can't run (WebGL2): a short
// march of the sun's shadow map along the view ray inside the map's box,
// after `look/shafts.rs` (its `WebShafts`). The flat fog lays the whole
// mist on, lit everywhere; this takes back the sun's share of it wherever
// the ray crossed shade, so the walls and the pillars between the eye and a
// low sun cut the air into beams. It only ever takes light away: the mist's
// extinction is the flat fog's, the same on every rung.
//
// Shared by the boxes (`weathering.wgsl`) and the sky behind them
// (`cloud.wgsl`'s BACKDROP). One hardware-compared tap a step, as Bevy's
// own volumetric pass takes.

#import bevy_pbr::mesh_view_bindings::{view, lights}
#import bevy_pbr::shadows::{get_cascade_index, world_to_directional_light_local}
#import bevy_pbr::shadow_sampling::sample_shadow_map_hardware

// `shafts::WEB_STEPS`.
const SHAFT_STEPS: u32 = 12u;

// The light the march takes back along a ray from `eye` toward `dir` (unit),
// out to `t_end` metres, in the frame's own (exposed) units. `lo`: the box's
// low corner, w the mist's extinction per metre; `hi`: its high corner, w 1
// where the march runs; `tint`: the flat fog's colour times the sun's share
// of it, w the droplets' asymmetry. `pix` dithers each pixel's steps.
fn shaft_lost(eye: vec3<f32>, dir: vec3<f32>, t_end: f32, lo: vec4<f32>, hi: vec4<f32>, tint: vec4<f32>, pix: vec2<f32>) -> vec3<f32> {
    if hi.w <= 0.0 || lights.n_directional_lights == 0u {
        return vec3(0.0);
    }
    // Where the ray is inside the box.
    let safe = select(dir, vec3(1e-6), abs(dir) < vec3(1e-6));
    let a = (lo.xyz - eye) / safe;
    let b = (hi.xyz - eye) / safe;
    let near = min(a, b);
    let far = max(a, b);
    let t0 = max(max(near.x, near.y), max(near.z, 0.0));
    let t1 = min(min(far.x, far.y), min(far.z, t_end));
    if t1 <= t0 {
        return vec3(0.0);
    }
    let light = &lights.directional_lights[0];
    let to_sun = (*light).direction_to_light.xyz;
    let bias = (*light).shadow_depth_bias * to_sun;
    let sigma = lo.w;
    let dt = (t1 - t0) / f32(SHAFT_STEPS);
    // Interleaved gradient noise: a pixel's steps start a different share
    // of a step in, so the stepping is grain, not bands.
    let jitter = fract(52.9829189 * fract(dot(pix, vec2(0.06711056, 0.00583715))));
    var dark = 0.0;
    for (var i = 0u; i < SHAFT_STEPS; i += 1u) {
        let t = t0 + (f32(i) + jitter) * dt;
        let p = eye + dir * t;
        let view_z = (view.view_from_world * vec4(p, 1.0)).z;
        let cascade = get_cascade_index(0u, view_z);
        var lit = 1.0;
        if cascade < (*light).num_cascades {
            let ll = world_to_directional_light_local(0u, cascade, vec4(p + bias, 1.0));
            if ll.w > 0.0 {
                lit = sample_shadow_map_hardware(ll.xy, ll.z, i32((*light).depth_texture_base_index + cascade));
            }
        }
        dark += (1.0 - lit) * exp(-sigma * t);
    }
    // Henyey–Greenstein against side-on, as the desktop's volume is
    // normalised (`shafts::volume`): strongest looking into the light.
    let g = tint.w;
    let mu = dot(dir, to_sun);
    let phase = pow((1.0 + g * g) / max(1.0 + g * g - 2.0 * g * mu, 1e-4), 1.5);
    return tint.rgb * phase * dark * sigma * dt;
}

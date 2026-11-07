// The cloud deck, per pixel (`look/deck.rs` `CloudMaterial`), on every
// target. After Gates' assets/shaders/cloud.wgsl and sky_deck.wgsl (MIT,
// NOTICE), folded into one shader so the two kinds of target draw the same
// clouds:
//
//   · Wherever the atmosphere draws (the desktop, the browser's WebGPU
//     module): one fullscreen triangle at the far plane in the OPAQUE
//     pass, so `AtmosphereNode::RenderSky` composites it as
//     `inscattering + T·cloud`, and the clouds keep the atmosphere's aerial
//     perspective while the sky behind them stays the atmosphere's.
//   · `BACKDROP`, under WebGL2 (no atmosphere): transparent, premultiplied
//     at alpha 1, the clear sky, the sun's disk, the clouds and the
//     weather's fog band composed together (Gates' `sky_deck.wgsl`).
//
// Two decks, one uniform, on both:
//   · `march.x == 0` — the flat deck: `sky::compose_range`'s arithmetic per
//     pixel, with `deck.wgsl`'s fine detail. Cheap; the CLOUDS row's LOW.
//   · `march.x > 0` — a slab of cloud from `slab.x` to `slab.y` metres,
//     raymarched: the deck's coverage field gives each column its cloud and
//     its height, a tiling 3D noise erodes the edges into cauliflower, and a
//     short march toward the light shades it (Beer–Lambert, a second softer
//     scattering order, a two-lobe phase for the silver lining).
// The drift runs on in here from where it was last taken (`deck_drift`),
// so the material is written when the sky's clock is re-anchored, not a
// frame.
#import bevy_pbr::mesh_view_bindings::{view, globals}
#import bevy_pbr::utils::coords_to_viewport_uv
#import "shaders/deck.wgsl"::{deck_point, deck_fade, deck_detail, deck_haze, deck_drift, deck_value, deck_veil, deck_veil_mass, slab_warp, slab_shape, slab_wide_lod}
#ifdef BACKDROP
#import "shaders/shaft_march.wgsl"::{shaft_lost}
#endif
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif

// Field for field `deck::CloudParams`. Colours linear, deck units (1.0 is
// one `sky::CLOUD_NITS`), already graded by the hour and the weather.
struct Cloud {
    // xyz a lit top; w the field threshold (1 − cover).
    top: vec4<f32>,
    // xyz a grey base; w the coverage edge's width.
    base: vec4<f32>,
    // xyz toward the light that shapes the slab (the sun, the moon at
    // night), y lifted so a march leaves the slab; w the sunlit term's
    // weight (`deck::SLAB_SUN`).
    light: vec4<f32>,
    // xy toward the sun, horizontal; zw the flat deck's light-march step.
    sun: vec4<f32>,
    // xyz toward the moon; w the halo's cosine.
    moon: vec4<f32>,
    // xyz the halo at its centre; w the moonlit scatter's cosine.
    halo: vec4<f32>,
    // xyz the moonlit scatter at the moon; w unused.
    scatter: vec4<f32>,
    // x altitude over noise scale; y the horizon cutoff; z the fade above
    // it; w the flat deck's fine detail amplitude.
    shape: vec4<f32>,
    // xy the field's offset, noise units; z one over its period; w half a
    // texel, uv.
    drift: vec4<f32>,
    // x `CORE_SHADE`; y `RIM_GLOW`; z `SIDE_VIEW_Y`; w cd/m² per deck unit.
    shade: vec4<f32>,
    // xyz the air's optical depth straight up, per channel; w 1 where the
    // atmosphere lays its transmittance on per channel, 0 through its mean.
    hue: vec4<f32>,
    // x the slab's floor, m; y its ceiling, m; z the field's metres per
    // noise unit; w metres per tile of the detail noise.
    slab: vec4<f32>,
    // x view steps (0: the flat deck); y light steps; z extinction per
    // metre at density 1; w 1 where TAA averages the jitter away.
    march: vec4<f32>,
    // x the field's texel, m; y the detail's texel, m; zw unused.
    texel: vec4<f32>,
    // x how fast a cumulus boils, m/s (the detail rising through the
    // slab, `deck::BOIL_MS`); y how dark the underside goes where it is
    // thickest (`Mood::deep`); zw unused. The detail rides the deck's drift.
    wind: vec4<f32>,
    // xyz the daylight added to the clear sky behind the deck
    // (`rig::SKY_LIFT`), on both skies; w unused.
    lift: vec4<f32>,
    // xy how fast the drift moves, noise units a second; z the
    // `globals.time` `drift.xy` was taken at; w unused.
    flow: vec4<f32>,
    // The backdrop under WebGL2 (`BACKDROP`), unused where the atmosphere
    // draws. xyz the clear zenith; w `sky::AIR_FLOOR`.
    zenith: vec4<f32>,
    // xyz the clear horizon; w the cosine of the sun disk's soft edge.
    horizon: vec4<f32>,
    // xyz the night's floor; w unused.
    night: vec4<f32>,
    // xyz a low sun's glow at its strongest; w the cosine of the sun
    // disk's radius.
    glow: vec4<f32>,
    // xyz the weather fog's colour; w the veil (`Mood::veil`).
    fog: vec4<f32>,
    // The weather's air over the sky (`sky::SkyFog::lanes`).
    air: vec4<f32>,
    // Its dust (`sky::SkyFog::dust_lanes`).
    dust: vec4<f32>,
    // xyz toward the sun, unit; w unused.
    disk: vec4<f32>,
    // xyz the sun disk's radiance, deck units; w unused.
    disk_rgb: vec4<f32>,
    // WebGL2's light shafts (`shaft_march.wgsl`), under BACKDROP: the map's
    // box, its mist and what a shaded ray takes back; `shaft_hi.w` 0: off.
    shaft_lo: vec4<f32>,
    shaft_hi: vec4<f32>,
    shaft_tint: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> cl: Cloud;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var field_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var field_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var detail_tex: texture_3d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var detail_samp: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
}

@vertex
fn vertex(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    // Depth 0 is infinitely far under reverse-Z: drawn only where nothing
    // is, and read by the atmosphere as sky.
    out.clip = vec4(in.position.xy, 0.0, 1.0);
    return out;
}

fn ray(position: vec2<f32>) -> vec3<f32> {
    let ndc = coords_to_viewport_uv(position, view.viewport) * vec2(2.0, -2.0) + vec2(-1.0, 1.0);
    let v = view.view_from_clip * vec4(ndc, 1.0, 1.0);
    return normalize((view.world_from_view * vec4(v.xyz / v.w, 0.0)).xyz);
}

const LUM: vec3<f32> = vec3(0.2126, 0.7152, 0.0722);
// `sky::HUE_MAX`.
const HUE_MAX: f32 = 3.0;
// The longest run a ray makes through the slab, m: the far deck is a
// band a few pixels tall, and past this it is the horizon fade's anyway.
const MAX_RUN_M: f32 = 14000.0;
// The light march's first step, m; each after it twice the last.
const LIGHT_FIRST_M: f32 = 40.0;

// `sky::deck_hue_for`: the hue of the atmosphere's transmittance at `y`,
// undone, so a cloud composited through it comes out grey and not tan.
fn deck_hue(y: f32) -> vec3<f32> {
    let yc = clamp(y, 0.0, 1.0);
    let deg = degrees(asin(yc));
    let m = 1.0 / (yc + 0.50572 * pow(deg + 6.07995, -1.6364));
    let t = max(exp(-cl.hue.xyz * m), vec3(1e-6));
    let least = min(t.r, min(t.g, t.b));
    let grey = min(dot(t, LUM), HUE_MAX * least);
    if cl.hue.w > 0.5 {
        return grey / t;
    }
    return vec3(grey / ((t.r + t.g + t.b) / 3.0));
}

// Henyey–Greenstein against isotropic: 1 for a sphere's average.
fn hg(g: f32, c: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / pow(1.0 + g2 - 2.0 * g * c, 1.5);
}

// Two lobes, a fifth of the light thrown hard forward — the bright rim on
// a cloud between the eye and the sun — and the rest nearly even. Over its
// value side-on, so a cloud lit from the side is the `top` it is given.
fn phase(c: f32) -> f32 {
    return (0.2 * hg(0.7, c) + 0.8 * hg(-0.1, c)) / 0.836;
}

// The second scattering order's: light that has already scattered once
// has forgotten most of where it came from.
fn phase_soft(c: f32) -> f32 {
    return (0.2 * hg(0.35, c) + 0.8 * hg(-0.05, c)) / 0.9426;
}

// Interleaved gradient noise: the ray's start, a fraction of a step, so the
// steps band into grain rather than into contour lines.
fn ign(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2(0.06711056, 0.00583715))));
}

// The field's offset this frame, noise units.
fn drift() -> vec2<f32> {
    return deck_drift(cl.drift.xy, cl.flow, globals.time);
}

fn field_at(xz: vec2<f32>, lod: f32) -> f32 {
    let q = xz / cl.slab.z + drift();
    return textureSampleLevel(field_tex, field_samp, q * cl.drift.z + cl.drift.w, lod).r;
}

// How thick the deck stands over `xz`, 0 thin to 1 thick: a heavy overcast's
// underside is mottled in broad soft masses with smaller ones in them. Its
// own smooth noise riding the deck's drift: the field's eight bits, stretched
// this far, come out in terraces.
fn deep_at(xz: vec2<f32>) -> f32 {
    let q = xz + drift() * cl.slab.z;
    let w = vec2(deck_value(q / 2300.0 + vec2(3.1, 7.7)), deck_value(q / 2300.0 + vec2(11.9, 1.3))) - 0.5;
    let p = q + w * 1500.0;
    let n = deck_value(p / 1900.0) * 0.5 + deck_value(p / 830.0 + vec2(5.2, 2.9)) * 0.3
        + deck_value(p / 340.0 + vec2(8.4, 6.1)) * 0.14 + deck_value(p / 140.0 + vec2(1.7, 9.3)) * 0.06;
    return smoothstep(0.26, 0.74, n);
}

// Cloud density at `p` (world, m), `0..1`. The deck's own field decides
// where a column carries cloud and — the further past the threshold, the
// taller — how high it builds, so the slab casts the silhouette the flat
// deck, the stars and the sun's cover read off the same field. `detail`:
// 0 the bare shape, 1 eroded by the detail noise, 2 and by its finer pass.
fn density(p: vec3<f32>, lod: f32, dlod: f32, detail: i32) -> f32 {
    let h = (p.y - cl.slab.x) / (cl.slab.y - cl.slab.x);
    if h <= 0.0 || h >= 1.0 {
        return 0.0;
    }
    let xz = p.xz + slab_warp(p, h);
    let c = (field_at(xz, lod + 0.5) - cl.top.w) / cl.base.w;
    if c <= 0.0 {
        return 0.0;
    }
    // How broad the cloud is here: the field blurred, for its height.
    let wide_lod = slab_wide_lod(f32(textureDimensions(field_tex).x), cl.drift.z / cl.slab.z);
    let wide = (field_at(xz, max(wide_lod, lod + 0.5)) - cl.top.w) / cl.base.w;
    let shape = slab_shape(h, c, wide);
    if shape <= 0.0 || detail == 0 {
        return shape;
    }
    // Erode the edges into lumps, harder toward the top, where a cumulus
    // boils; the core stays whole. The finer pass is the same noise at a
    // third the size and turned, so the two lattices never line up: the
    // small cauliflower on the big.
    // The detail rides the deck (the field is read at `xz / scale + drift`,
    // so the same offset in metres keeps a lump on its cloud) and rises.
    let dn = drift();
    let wind = vec3(dn.x * cl.slab.z, -globals.time * cl.wind.x, dn.y * cl.slab.z);
    let w = (p + wind) / cl.slab.w;
    var n = textureSampleLevel(detail_tex, detail_samp, w, dlod).r;
    if detail > 1 {
        let wf = vec3(w.z, w.x, w.y) * 3.1 + vec3(0.37, 0.11, 0.73);
        let nf = textureSampleLevel(detail_tex, detail_samp, wf, dlod + 1.63).r;
        n = n * 0.7 + nf * 0.3;
    }
    // The noise's middle is stretched to its ends first: read as it comes,
    // most of it sits near one value and the erosion only shrinks the
    // cloud evenly; stretched, it bites lumps out of the skin.
    let e = (1.0 - smoothstep(0.2, 0.8, n)) * (0.72 + 0.26 * h);
    return clamp((shape - e) / (1.0 - e), 0.0, 1.0);
}

// The slab, raymarched: premultiplied cloud radiance and coverage.
//
// **Across the deck the eye is at its origin, as the flat deck's is**:
// `deck_point` projects a ray from the eye, never from the eye's place on
// the island, and the sun's cover (`SkyComposer::cover_at`), the stars and
// the moon all read the deck that way. A march from the eye's world x and
// z would put every cloud a kilometre or two from where those put it, and
// the sun would go out in a gap. Only the eye's height is its own.
fn march(d: vec3<f32>, px: vec2<f32>) -> vec4<f32> {
    let eye = vec3(0.0, view.world_position.y, 0.0);
    let dy = max(d.y, cl.shape.y);
    let t0 = max((cl.slab.x - eye.y) / dy, 0.0);
    let t1 = min((cl.slab.y - eye.y) / dy, t0 + MAX_RUN_M);
    if t1 <= t0 {
        return vec4(0.0);
    }
    // Two rates. Coarse steps cross the empty sky reading only the
    // shape; on finding cloud the march steps back and walks it at a
    // quarter of the step, eroded and lit. Inside a cloud the step is what
    // decides how much each pixel's dithered start shows, and a coarse one
    // through a crisp edge shows as a weave across the whole cloud.
    let n = i32(cl.march.x);
    let dt = (t1 - t0) / f32(n);
    let dtf = dt * 0.25;
    var seed = px;
    if cl.march.w > 0.5 {
        seed += 5.588238 * f32(globals.frame_count % 64u);
    }
    // A pixel's angle, so a far sample reads a coarser mip and the deck
    // does not shimmer as it drifts.
    let pix = 2.0 / (view.clip_from_view[1][1] * view.viewport.w);
    let l = cl.light.xyz;
    let ct = dot(d, l);
    let ph = phase(ct);
    let ph2 = phase_soft(ct);
    let sigma = cl.march.z;
    let lsteps = i32(cl.march.y);

    var trans = 1.0;
    var lum = vec3(0.0);
    var t = t0 + ign(seed) * dt;
    // Fine steps left before an empty run sends the march back to coarse.
    var fine = 0;
    for (var i = 0; i < 4 * n; i += 1) {
        if t >= t1 {
            break;
        }
        let p = eye + d * t;
        let stride = select(dt, dtf, fine > 0);
        let foot = max(pix * t, 0.5 * stride * length(d.xz));
        let lod = max(log2(foot / cl.texel.x), 0.0);
        let dlod = max(log2(foot / cl.texel.y), 0.0);
        if fine == 0 {
            if density(p, lod, dlod, 0) > 0.0 {
                t = max(t - 0.75 * dt, t0);
                fine = 6;
            } else {
                t += dt;
            }
            continue;
        }
        let rho = density(p, lod, dlod, 2);
        if rho > 0.002 {
            fine = 6;
            // Toward the light: how much cloud it came through to get here.
            var od = 0.0;
            var s = LIGHT_FIRST_M;
            var along = 0.0;
            for (var k = 0; k < lsteps; k += 1) {
                let q = p + l * (along + 0.5 * s);
                od += density(q, lod + f32(k), dlod + f32(k), select(0, 1, k < 2)) * s;
                along += s;
                s *= 2.0;
            }
            let tau = od * sigma;
            let sun = cl.light.w * (ph * exp(-tau) + 0.35 * ph2 * exp(-0.25 * tau));
            // Sky light from above: a base sees less of the sky than a top.
            let hgt = clamp((p.y - cl.slab.x) / (cl.slab.y - cl.slab.x), 0.0, 1.0);
            var amb = mix(0.5, 1.0, hgt);
            if cl.wind.y > 0.0 {
                amb *= 1.0 - cl.wind.y * deep_at(p.xz);
            }
            let lit = cl.top.rgb * sun + cl.base.rgb * amb;
            let a = 1.0 - exp(-rho * sigma * dtf);
            lum += trans * a * lit;
            trans *= 1.0 - a;
            if trans < 0.02 {
                lum += trans * lit;
                trans = 0.0;
                break;
            }
        } else {
            fine -= 1;
        }
        t += dtf;
    }
    return vec4(lum, 1.0 - trans);
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let d = ray(in.clip.xy);

    // The flat deck's samples, before any branch: they need derivatives.
    let q = deck_point(d, cl.shape.x, cl.shape.y) + drift();
    let qs = q + cl.sun.zw;
    let uv = q * cl.drift.z + cl.drift.w;
    let gx = dpdx(uv);
    let gy = dpdy(uv);
    let foot = max(length(dpdx(q)), length(dpdy(q)));
    let f0 = textureSampleGrad(field_tex, field_samp, uv, gx, gy).r;
    let fs0 = textureSampleGrad(field_tex, field_samp, qs * cl.drift.z + cl.drift.w, gx, gy).r;

    // Behind the cloud. Where the atmosphere draws: nothing (it paints the
    // sky) but the moon's halo, which it does not draw, and the daylight
    // lift. The disk is `moon.wgsl`'s, added over this with its phase: a
    // round one here would show through a crescent's dark side.
    var sky = vec3(0.0);
    let cm = dot(d, cl.moon.xyz);
    if d.y > 0.0 && cm > cl.moon.w {
        let k = (cm - cl.moon.w) / (1.0 - cl.moon.w);
        sky += cl.halo.rgb * k * k;
    }
    let s = max((cm - cl.halo.w) / (1.0 - cl.halo.w), 0.0);
#ifdef BACKDROP
    // Under WebGL2 the clear sky is this shader's: zenith to horizon along
    // the air-mass curve, the night's floor, a low sun's glow, and the
    // sun's disk, which no atmosphere draws here.
    let af = cl.zenith.w;
    let am = 1.0 / (clamp(d.y, 0.0, 1.0) + af);
    let w_air = clamp((am - 1.0 / (1.0 + af)) / (1.0 / af - 1.0 / (1.0 + af)), 0.0, 1.0);
    sky += mix(cl.zenith.rgb, cl.horizon.rgb, w_air) + cl.night.rgb;
    if d.y > -0.1 {
        let h = d.xz / max(length(d.xz), 1e-6);
        let facing = max(dot(h, cl.sun.xy), 0.0);
        let low = max(1.0 - max(d.y, 0.0) / 0.4, 0.0);
        sky += cl.glow.rgb * facing * facing * facing * low * low;
    }
    let cd = dot(d, cl.disk.xyz);
    sky += cl.disk_rgb.rgb * smoothstep(cl.horizon.w, cl.glow.w, cd);
    // The daylight lift the atmosphere's sky gets below, with no
    // transmittance to undo: the gradient alone is the slate it was
    // measured against.
    sky += cl.lift.rgb * smoothstep(0.0, 0.05, d.y);
#else
    // Daylight behind the deck: more of the air's own blue than the
    // atmosphere's sky carries, laid on through its transmittance as a
    // cloud is (`deck_hue`), so it arrives the colour it was given.
    sky += cl.lift.rgb * deck_hue(d.y) * smoothstep(0.0, 0.05, d.y);
#endif

    let fade = deck_fade(d.y, cl.shape.y, cl.shape.z);
    var pre = vec3(0.0);
    var cov = 0.0;
    if fade > 0.0 {
        if cl.march.x < 0.5 {
            let thresh = cl.top.w;
            let edge = cl.base.w;
            let f = f0 + cl.shape.w * deck_detail(q, foot);
            let f_sun = fs0 + cl.shape.w * deck_detail(qs, foot);
            let c0 = clamp((f - thresh) / edge, 0.0, 1.0);
            let lit = clamp((f - f_sun) * 6.0 + 0.5, 0.0, 1.0);
            let thick = clamp((f - thresh) / (3.0 * edge), 0.0, 1.0);
            let side = clamp((cl.shade.z - d.y) / (cl.shade.z - cl.shape.y), 0.0, 1.0);
            let under = cl.base.rgb * (1.0 - cl.shade.x * thick) * (1.0 - cl.wind.y * thick)
                + (cl.top.rgb - cl.base.rgb) * cl.shade.y * (1.0 - c0);
            let w = max(lit * (1.0 - 0.5 * thick), side * side * (3.0 - 2.0 * side));
            let cloud = mix(under, cl.top.rgb, w) + cl.scatter.rgb * s * s * (1.0 - 0.6 * thick);
            cov = c0 * fade;
            pre = cloud * cov;
        } else {
            let m = march(d, in.clip.xy);
            cov = m.a * fade;
            pre = (m.rgb + cl.scatter.rgb * s * s * (1.0 - 0.6 * m.a) * m.a) * fade;
        }
#ifndef BACKDROP
        pre *= deck_hue(d.y);
#endif
    }
    var rgb = pre + sky * (1.0 - cov);
#ifdef BACKDROP
    // Fog lies between the eye and everything: as much of the sky as the
    // weather's air takes along this ray. Where the atmosphere draws it is
    // `sky_fog.wgsl`'s, over the atmosphere too.
    var fogged = cl.fog.rgb;
    if cl.fog.w > 0.0 {
        fogged *= deck_veil_mass(d, globals.time);
    }
    rgb = mix(rgb, fogged, deck_veil(deck_haze(d.y, cl.air, cl.dust), d, cl.disk.xyz, cl.fog.w));
    // The light shafts: the sun's share of the mist taken back where the
    // ray crossed the map's shade on its way out of the box, in the frame's
    // own units, as the boxes take it (`weathering.wgsl`).
    var out = rgb * cl.shade.w * view.exposure;
    if cl.shaft_hi.w > 0.0 {
        let lost = shaft_lost(view.world_position, d, 1.0e4, cl.shaft_lo, cl.shaft_hi, cl.shaft_tint, in.clip.xy);
        out = max(out - lost, vec3(0.0));
    }
#ifdef TONEMAP_IN_SHADER
    // A view with no HDR target (WebGL2's: no atmosphere, no bloom) is
    // tonemapped in each material's own shader, as Bevy's are, and the sky
    // is one: untonemapped, it stood beside a tonemapped world in another
    // grade, and a lit cumulus past 1 clipped to a flat white cut-out.
    return tone_mapping(vec4(out, 1.0), view.color_grading);
#else
    return vec4(out, 1.0);
#endif
#else
    return vec4(rgb * cl.shade.w * view.exposure, 1.0);
#endif
}

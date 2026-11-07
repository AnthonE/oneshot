// A power-up on its pad (look/powerup.rs), no model, in two parts.
//
// The orb: a sphere drawn as the ball it stands for, solved exactly along
// each pixel's ray (the mesh only says where to look), so it has a depth: a
// heart burning in its middle, light curling through it at two depths, and
// a shell of hard-light plates (the sphere cut nearest-first among `cells`:
// 12 pentagons and 80 hexagons) lit along their seams, the near side bright
// and the far side dim through it, a rim where it turns away. The
// overshield is a green star wound in its cage; camouflage a pale glass
// bubble, filmed at its rim, its plates coming and going and a wipe taking
// it down to almost nothing and back.
// A ring: a torus turned about the orb by the clock (each its own tilt and
// speed, by its tag), comets of light running round it, never thinner than
// a pixel (further off it is drawn wider and dimmer, so it never flickers).
// A halo: a larger sphere of glow round the orb, thickest through its middle
// and nothing at its edge, so it has none: the orb's size from across a map.
//
// All HDR for the bloom, added to what is behind. WebGL2 runs it: no
// compute, no textures.

#import bevy_pbr::{
    mesh_functions,
    mesh_view_bindings::{view, globals},
    view_transformations::position_world_to_clip,
}

// `powerup::CELLS`.
const CELLS: u32 = 92u;
const TAU: f32 = 6.2831853;

struct Powerup {
    // x: how bright; y: 0 the orb, 1 a ring, 2 a halo; z: 0 the overshield,
    // 1 camouflage; w: a ring's tube's radius, metres.
    knobs: vec4<f32>,
    deep: vec4<f32>,
    mid: vec4<f32>,
    hot: vec4<f32>,
    // The plates' middles, on the unit sphere.
    cells: array<vec4<f32>, 92>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> pu: Powerup;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Where it is in the orb's own frame (a ring's after its turn).
    @location(2) local: vec3<f32>,
    // The eye in the orb's own frame: it spins and swells with the pad.
    @location(3) eye: vec3<f32>,
    // A ring's vertex before its turn, which way round it is.
    @location(4) round: vec3<f32>,
    // How much of a ring's width is its own, of what it is drawn (1 near).
    @location(5) thin: f32,
};

// A pixel's width over its distance, about (1080p, a 90° view).
const PIXEL: f32 = 0.0008;

fn hash(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3<f32>(127.1, 311.7, 74.7))) * 43758.5453);
}

// Value noise, 0 to 1.
fn noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(hash(i), hash(i + vec3<f32>(1.0, 0.0, 0.0)), u.x),
                mix(hash(i + vec3<f32>(0.0, 1.0, 0.0)), hash(i + vec3<f32>(1.0, 1.0, 0.0)), u.x), u.y);
    let b = mix(mix(hash(i + vec3<f32>(0.0, 0.0, 1.0)), hash(i + vec3<f32>(1.0, 0.0, 1.0)), u.x),
                mix(hash(i + vec3<f32>(0.0, 1.0, 1.0)), hash(i + vec3<f32>(1.0, 1.0, 1.0)), u.x), u.y);
    return mix(a, b, u.z);
}

fn turn_y(a: f32) -> mat3x3<f32> {
    let c = cos(a);
    let s = sin(a);
    return mat3x3<f32>(vec3<f32>(c, 0.0, -s), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(s, 0.0, c));
}

fn turn_x(a: f32) -> mat3x3<f32> {
    let c = cos(a);
    let s = sin(a);
    return mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, c, s), vec3<f32>(0.0, -s, c));
}

// Ring `k`'s turn at `t`: tipped off level, its tip swinging round.
fn ring_turn(k: u32, t: f32) -> mat3x3<f32> {
    var tilt = 1.2;
    var swing = 0.9;
    var start = 0.0;
    if k == 1u {
        tilt = -0.95;
        swing = -0.6;
        start = 1.9;
    }
    return turn_y(t * swing + start) * turn_x(tilt);
}

@vertex
fn vertex(v: Vertex) -> Out {
    var out: Out;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    var p = v.position;
    var n = v.normal;
    let is_ring = abs(pu.knobs.y - 1.0) < 0.5;
    if is_ring {
        let r = ring_turn(mesh_functions::get_tag(v.instance_index), globals.time);
        p = r * p;
        n = r * n;
    }
    var world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(p, 1.0)).xyz;
    out.normal = mesh_functions::mesh_normal_local_to_world(n, v.instance_index);
    out.thin = 1.0;
    if is_ring {
        // Its tube out to a pixel across: along its normal, which points
        // away from the line through the tube.
        let extra = max(distance(view.world_position, world) * PIXEL * 0.5 - pu.knobs.w, 0.0);
        world += normalize(out.normal) * extra;
        out.thin = pu.knobs.w / (pu.knobs.w + extra);
    }
    out.world = world;
    out.clip = position_world_to_clip(world);
    out.local = p;
    // Its scale is even: the inverse is the transpose over its square.
    let m = mat3x3<f32>(world_from_local[0].xyz, world_from_local[1].xyz, world_from_local[2].xyz);
    out.eye = transpose(m) * (view.world_position - world_from_local[3].xyz) / dot(m[0], m[0]);
    out.round = v.position;
    return out;
}

// The plate `p` (on the unit sphere) is on: its middle, which it is, and
// how far `p` is inside its seam (radians, near enough).
struct Plate {
    mid: vec3<f32>,
    id: f32,
    seam: f32,
};

fn plate(p: vec3<f32>) -> Plate {
    var best = -2.0;
    var next = -2.0;
    var a = vec3<f32>(0.0, 1.0, 0.0);
    var b = a;
    var id = 0.0;
    for (var i = 0u; i < CELLS; i++) {
        let c = pu.cells[i].xyz;
        let k = dot(p, c);
        if k > best {
            next = best;
            b = a;
            best = k;
            a = c;
            id = f32(i);
        } else if k > next {
            next = k;
            b = c;
        }
    }
    var out: Plate;
    out.mid = a;
    out.id = id;
    out.seam = (best - next) / max(length(a - b), 1e-3);
    return out;
}

// A seam's line: 1 on it, gone `w` off it, its edge a pixel soft (`px`,
// the seam's change across a pixel). Far off, where a pixel spans plates,
// it is the even glow its lines would make (`SEAM_SHARE` of a plate's
// width, roughly), not a shimmer of them.
const SEAM_SHARE: f32 = 0.18;
fn line(seam: f32, px: f32, w: f32) -> f32 {
    let sharp = 1.0 - smoothstep(w, w + px * 1.5, seam);
    return mix(sharp, SEAM_SHARE * w / 0.02, smoothstep(0.025, 0.07, px));
}

// Light wound round the middle at `p`: thin bands spiralling in, bent by
// noise, turning against the shell.
fn streams(p: vec3<f32>, t: f32) -> f32 {
    let q = turn_y(-t * 1.7) * p;
    // Never atan2(0, 0): a NaN here would smear through the bloom.
    let x = select(q.x, 1e-5, abs(q.x) + abs(q.z) < 1e-5);
    let wob = noise(q * 2.6 + vec3<f32>(0.0, t * 0.7, 0.0));
    let u = atan2(q.z, x) * 3.0 + q.y * 4.5 + length(q.xz) * 5.0 - t * 2.4 + wob * 2.8;
    return pow(0.5 + 0.5 * sin(u), 26.0);
}

// Camouflage's wipe at `t`: where its edge stands (the orb's own up, its
// radius 1). Down over the orb, held gone, back up, held whole.
fn wipe_edge(t: f32) -> f32 {
    let c = fract(t / 4.6);
    let down = smoothstep(0.08, 0.42, c);
    let up = smoothstep(0.58, 0.92, c);
    return 1.25 - 2.5 * (down - up);
}

// How much of camouflage shows at height `y`: all under the wipe's edge, a
// trace over it.
fn shown(y: f32, edge: f32) -> f32 {
    return mix(0.16, 1.0, smoothstep(edge + 0.12, edge - 0.12, y));
}

fn ring(in: Out, t: f32) -> vec4<f32> {
    let to_eye = normalize(view.world_position - in.world);
    let facing = abs(dot(normalize(in.normal), to_eye));
    // Three comets round it, a bright head and a long tail.
    let u = atan2(in.round.z, in.round.x) / TAU * 3.0 + t * 0.55;
    let comet = pow(fract(u), 5.0);
    let tube = 0.35 + 0.65 * facing;
    var c = pu.mid.rgb * (0.4 + comet * 1.8) * tube + pu.hot.rgb * comet * comet * comet * facing;
    if pu.knobs.z > 0.5 {
        c *= shown(in.local.y, wipe_edge(t));
    }
    return vec4<f32>(c * pu.knobs.x * in.thin, 0.0);
}

fn halo(in: Out, t: f32) -> vec4<f32> {
    let d = normalize(in.local - in.eye);
    let b = dot(in.eye, d);
    // How near the ray passes its middle, squared, of its radius.
    let r2 = clamp(dot(in.eye, in.eye) - b * b, 0.0, 1.0);
    let glow = (exp(-r2 * 5.0) - exp(-5.0)) / (1.0 - exp(-5.0));
    var c = pu.mid.rgb * glow * (1.0 + 0.12 * sin(t * 3.4));
    if pu.knobs.z > 0.5 {
        // Paler than the overshield's, so a little more of it; and half of
        // it stays while camouflage wipes itself out: it is still there to
        // be found.
        c *= 1.4 * mix(0.5, 1.0, smoothstep(-1.25, 1.25, wipe_edge(t)));
    }
    return vec4<f32>(c * pu.knobs.x, 0.0);
}

@fragment
fn fragment(in: Out) -> @location(0) vec4<f32> {
    let t = globals.time;
    if pu.knobs.y > 1.5 {
        return halo(in, t);
    }
    if pu.knobs.y > 0.5 {
        return ring(in, t);
    }
    // The ray, in the orb's frame, where the unit sphere is the orb.
    let e = in.eye;
    let d = normalize(in.local - e);
    let b = dot(e, d);
    let h = clamp(b * b - dot(e, e) + 1.0, 0.0, 1.0);
    let facing = sqrt(h);
    let near = e + d * (-b - facing);
    let far = e + d * (-b + facing);
    // 0 at its rim, 1 through its middle; and how near the ray passes its
    // middle, squared (0 through it, 1 grazing it).
    let depth = facing;
    let r2 = 1.0 - h;
    let rim = pow(1.0 - facing, 3.0);

    let pn = plate(near);
    let pf = plate(far);
    let px_n = fwidth(pn.seam);
    let px_f = fwidth(pf.seam);
    let deep = pu.deep.rgb;
    let mid = pu.mid.rgb;
    let hot = pu.hot.rgb;
    var c = vec3<f32>(0.0);

    if pu.knobs.z < 0.5 {
        // The overshield: a heart in a cage.
        let heart = exp(-r2 * 9.0);
        let core = exp(-r2 * 30.0);
        let s = streams(mix(near, far, 0.3), t) + streams(mix(near, far, 0.7), t + 3.1) * 0.8;
        // Each plate lights as a wave runs over the cage, out of step with
        // its neighbours.
        let wave = turn_y(t * 0.4) * vec3<f32>(0.6, 0.8, 0.0);
        let lift = pow(0.5 + 0.5 * sin(t * 2.3 - dot(pn.mid, wave) * 4.0 + hash(vec3<f32>(pn.id)) * 2.0), 4.0);
        let seam_n = line(pn.seam, px_n, 0.016);
        let seam_f = line(pf.seam, px_f, 0.012);
        c += deep * (0.4 + depth * 1.6);
        c += mid * (heart * 0.5 + lift * 0.14 * (0.3 + rim));
        c += hot * (core * 1.4 + s * (0.25 + heart) * 0.9);
        c += mid * rim * 1.3 + hot * rim * rim * 0.4;
        c += hot * seam_n * (0.3 + lift * 0.8) + mid * seam_f * 0.22;
        c *= 1.0 + 0.1 * sin(t * 3.4);
    } else {
        // Camouflage: glass, filmed at its rim, its plates coming and going.
        let edge = wipe_edge(t);
        let on_n = smoothstep(0.2, 0.75, 0.5 + 0.5 * sin(t * 1.6 + hash(vec3<f32>(pn.id)) * TAU));
        let on_f = smoothstep(0.2, 0.75, 0.5 + 0.5 * sin(t * 1.6 + hash(vec3<f32>(pf.id)) * TAU));
        let seam_n = line(pn.seam, px_n, 0.012) * on_n;
        let seam_f = line(pf.seam, px_f, 0.01) * on_f;
        // A thin film's colours slide round its rim.
        let film = 0.5 + 0.5 * cos(TAU * (vec3<f32>(0.0, 0.33, 0.67) + facing * 1.6 + near.y * 0.4 + t * 0.12));
        let ripple = pow(0.5 + 0.5 * sin(sqrt(r2) * 20.0 - t * 4.0), 6.0) * (1.0 - r2);
        let heart = exp(-r2 * 9.0) * (0.75 + 0.25 * sin(t * 5.3));
        let wisp = streams(mix(near, far, 0.5), t * 0.6);
        let skin = shown(near.y, edge);
        let body = shown(mix(near, far, 0.5).y, edge);
        c += deep * (0.2 + depth * 0.5) * body;
        c += mid * (heart * 0.9 + ripple * 0.3 + depth * 0.3) * body + hot * wisp * depth * 0.4 * body;
        c += hot * exp(-r2 * 45.0) * 0.9 * body;
        c += (mid * 0.6 + film * length(mid) * 1.1) * rim * 2.4 * skin;
        c += hot * seam_n * 1.0 * skin + mid * seam_f * 0.4 * shown(far.y, edge);
        // The wipe's edge, a line of light across it.
        let off = (near.y - edge) / 0.035;
        let scan = exp(-off * off);
        c += hot * scan * 0.9;
    }
    // No alpha: added to what is behind it (`AlphaMode::Add`).
    return vec4<f32>(c * pu.knobs.x, 0.0);
}

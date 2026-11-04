// The ground (look/ground.rs): four texture sets, blended by the splat
// weights in the vertex colour (sand, grass, gravel, rock), each tiled in
// world x and z at its own size, then Bevy's PBR lighting. A steep face is
// decided per pixel from the normal's tilt (the cliff veto), read off fixed
// vertical planes so its photograph is not stretched down the fall line
// (the wall planes), and broken into blocks with cracks and streaks (the
// rock face). After Gates' shaders/ground_splat.wgsl (MIT, NOTICE): its
// hash, value noise, rock cells, wall planes, cliff veto and rock face,
// copied and trimmed; without the island's roads, blight, snow, caustics and
// height blend. Every number arrives in `GroundParams` (binding 104).

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}

// ⚠ This struct's field list and order must match `GroundParams` in
// look/ground.rs exactly: a uniform whose two sides disagree about layout
// binds fine and is garbage in every field after the first mismatch. A test
// in ground.rs scrapes both. What each field holds is written there.
struct GroundParams {
    tile_m: vec4<f32>,
    tint: array<vec4<f32>, 4>,
    cliff: vec4<f32>,
    wall: vec4<f32>,
    wall_planes: vec4<f32>,
    rock_a: vec4<f32>,
    rock_b: vec4<f32>,
    rock_c: vec4<f32>,
    rock_d: vec4<f32>,
    rock_e: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var albedo_tex: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var albedo_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var normal_tex: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var normal_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var<uniform> ground: GroundParams;

// A tangent-space normal as a surface gradient. Summing GRADIENTS is what
// makes a blend of normals mean anything: averaging the vectors pulls every
// mix toward the flat +z and quietly flattens the relief.
fn to_gradient(n: vec3<f32>) -> vec2<f32> {
    return n.xy / max(n.z, ground.wall.w);
}

fn unpack_normal(t: vec4<f32>) -> vec3<f32> {
    return normalize(t.xyz * 2.0 - 1.0);
}

// --- Hashes and noise ----------------------------------------------------------

// Chris Wellons' lowbias32: a full-avalanche integer hash in five ops.
fn lowbias32(x0: u32) -> u32 {
    var x = x0;
    x = x ^ (x >> 16u);
    x = x * 0x7feb352du;
    x = x ^ (x >> 15u);
    x = x * 0x846ca68bu;
    x = x ^ (x >> 16u);
    return x;
}

fn cell_key(c: vec3<i32>) -> u32 {
    return lowbias32(bitcast<u32>(c.x) * 73856093u
        ^ bitcast<u32>(c.y) * 19349663u
        ^ bitcast<u32>(c.z) * 83492791u);
}

fn unit_of(h: u32) -> f32 {
    return f32(h >> 8u) * (1.0 / 16777216.0);
}

// Three independent draws in [0, 1) off one cell key.
fn draw3(h: u32) -> vec3<f32> {
    let a = lowbias32(h ^ 0x9e3779b9u);
    let b = lowbias32(a ^ 0x85ebca6bu);
    let c = lowbias32(b ^ 0xc2b2ae35u);
    return vec3<f32>(unit_of(a), unit_of(b), unit_of(c));
}

// Value noise over a 3D lattice, quintic-faded, in [0, 1].
fn value3(p: vec3<f32>) -> f32 {
    let i = vec3<i32>(floor(p));
    let f = p - floor(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let n000 = unit_of(cell_key(i));
    let n100 = unit_of(cell_key(i + vec3<i32>(1, 0, 0)));
    let n010 = unit_of(cell_key(i + vec3<i32>(0, 1, 0)));
    let n110 = unit_of(cell_key(i + vec3<i32>(1, 1, 0)));
    let n001 = unit_of(cell_key(i + vec3<i32>(0, 0, 1)));
    let n101 = unit_of(cell_key(i + vec3<i32>(1, 0, 1)));
    let n011 = unit_of(cell_key(i + vec3<i32>(0, 1, 1)));
    let n111 = unit_of(cell_key(i + vec3<i32>(1, 1, 1)));
    let x00 = mix(n000, n100, u.x);
    let x10 = mix(n010, n110, u.x);
    let x01 = mix(n001, n101, u.x);
    let x11 = mix(n011, n111, u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z);
}

// --- The rock face: blocks, cracks and streaks ------------------------------

struct RockCell {
    // The nearest block's own key, for every per-block draw.
    key: u32,
    // The second-nearest block's, so the boundary between the two has a key.
    key2: u32,
    // The third-nearest's, for where three blocks meet.
    key3: u32,
    // Distance to the nearest block boundary, in cell units.
    edge: f32,
    // Distance to the boundary with the third-nearest block, in cell units.
    edge3: f32,
}

// Nearest-feature cells over 3D world space, so a steep face and a flat
// outcrop cut the same blocks and no frame has to be chosen on the surface.
fn rock_cell(p: vec3<f32>) -> RockCell {
    let base = vec3<i32>(floor(p));
    let f = p - floor(p);
    var d1 = 1e9;
    var d2 = 1e9;
    var d3 = 1e9;
    var f1 = vec3<f32>(0.0);
    var f2 = vec3<f32>(0.0);
    var f3 = vec3<f32>(0.0);
    var k1 = 0u;
    var k2 = 0u;
    var k3 = 0u;
    for (var k: i32 = -1; k <= 1; k = k + 1) {
        for (var j: i32 = -1; j <= 1; j = j + 1) {
            for (var i: i32 = -1; i <= 1; i = i + 1) {
                let o = vec3<i32>(i, j, k);
                let key = cell_key(base + o);
                let pt = vec3<f32>(o) + vec3<f32>(0.15) + draw3(key) * 0.7;
                let d = dot(pt - f, pt - f);
                if d < d1 {
                    d3 = d2;
                    f3 = f2;
                    k3 = k2;
                    d2 = d1;
                    f2 = f1;
                    k2 = k1;
                    d1 = d;
                    f1 = pt;
                    k1 = key;
                } else if d < d2 {
                    d3 = d2;
                    f3 = f2;
                    k3 = k2;
                    d2 = d;
                    f2 = pt;
                    k2 = key;
                } else if d < d3 {
                    d3 = d;
                    f3 = pt;
                    k3 = key;
                }
            }
        }
    }
    let n = normalize(f2 - f1);
    var out: RockCell;
    out.key = k1;
    out.key2 = k2;
    out.key3 = k3;
    out.edge = max(-dot(f - 0.5 * (f1 + f2), n), 0.0);
    out.edge3 = max(-dot(f - 0.5 * (f1 + f3), normalize(f3 - f1)), 0.0);
    return out;
}

// A per-block draw that turns over to its neighbours' across the seam
// instead of stepping at it: stepped, the eye read every boundary as an
// outline, a face as polygons pasted on. Each of the three nearest blocks
// is weighted by how near its boundary is, and every weight is 1 on that
// boundary, so the blend is continuous across it and the block's own draw
// holds only past `rock_e.x` cells in.
fn rock_draw(c: RockCell, salt: u32) -> vec3<f32> {
    let w2 = 1.0 - smoothstep(0.0, ground.rock_e.x, c.edge);
    let w3 = 1.0 - smoothstep(0.0, ground.rock_e.x, c.edge3);
    var sum = draw3(c.key ^ salt);
    if w2 > 0.0 {
        sum = sum + draw3(c.key2 ^ salt) * w2;
    }
    if w3 > 0.0 {
        sum = sum + draw3(c.key3 ^ salt) * w3;
    }
    return sum / (1.0 + w2 + w3);
}

// --- The wall planes ----------------------------------------------------------

// The vertical planes a steep face reads its sets off: four, at fixed
// bearings 45° apart (`wall_axis`), of which a pixel blends the two nearest
// its face, so no face is more than 22.5° off a plane and its photograph is
// stretched at most 1.08× across it. Each UV is LINEAR in the world
// position, so its gradients are exact and nothing turns with the surface.
// Everything is in metres; `wall_tap` applies the set's tile.
struct Wall {
    // Each plane's u axis (`wall_axis`), UV and gradients.
    axis_a: vec2<f32>,
    uv_a: vec2<f32>,
    dx_a: vec2<f32>,
    dy_a: vec2<f32>,
    axis_b: vec2<f32>,
    uv_b: vec2<f32>,
    dx_b: vec2<f32>,
    dy_b: vec2<f32>,
    // Plane b's share of the wall; plane a takes the rest.
    share_b: f32,
}

// The u axis of wall plane `k` (0..4): the horizontal across a face whose
// normal bears `k × 45°` from +X in the XZ plane. A constant per plane,
// which is what keeps every wall UV linear in `wp`.
fn wall_axis(k: i32) -> vec2<f32> {
    var axes = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(-0.70710678, 0.70710678),
        vec2<f32>(-1.0, 0.0),
        vec2<f32>(-0.70710678, -0.70710678),
    );
    return axes[k];
}

// One set's two maps, read off the wall planes.
struct WallTap {
    albedo: vec4<f32>,
    // The photograph's relief as a world-space offset, not yet laid in the
    // surface: each plane's tangent-space gradient on that plane's own axes.
    relief: vec3<f32>,
}

// One plane's two maps, weighted by its share. Explicit-gradient samples
// only: this runs under the non-uniform wall branch, where an implicit
// derivative is undefined. `tile` is 1 / the set's metres, the same factor
// as the UV's, or the mip is picked for the wrong density.
fn wall_plane(uv0: vec2<f32>, dx0: vec2<f32>, dy0: vec2<f32>, axis: vec2<f32>, layer: i32, tile: f32, k: f32) -> WallTap {
    let uv = uv0 * tile;
    let dx = dx0 * tile;
    let dy = dy0 * tile;
    var t: WallTap;
    t.albedo = k * textureSampleGrad(albedo_tex, albedo_smp, uv, layer, dx, dy);
    let gr = to_gradient(unpack_normal(textureSampleGrad(normal_tex, normal_smp, uv, layer, dx, dy)));
    // u runs along the plane's axis and the image's up is world up.
    t.relief = k * vec3<f32>(gr.x * axis.x, gr.y, gr.x * axis.y);
    return t;
}

fn wall_tap(wall: Wall, layer: i32, tile: f32) -> WallTap {
    var t: WallTap;
    if wall.share_b < 1.0 {
        t = wall_plane(wall.uv_a, wall.dx_a, wall.dy_a, wall.axis_a, layer, tile, 1.0 - wall.share_b);
    }
    if wall.share_b > 0.0 {
        let b = wall_plane(wall.uv_b, wall.dx_b, wall.dy_b, wall.axis_b, layer, tile, wall.share_b);
        t.albedo = t.albedo + b.albedo;
        t.relief = t.relief + b.relief;
    }
    return t;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // The derivatives first, at the body's own depth: undefined under
    // non-uniform control flow, and the wall branch below is non-uniform by
    // construction. A test in ground.rs holds them here.
    let wp = in.world_position.xyz;
    let dp_dx = dpdx(wp);
    let dp_dy = dpdy(wp);

    let wn = normalize(in.world_normal);
    let sin_tilt = length(wn.xz);
    let cos_tilt = abs(wn.y);
    // The projection follows the triangle the photograph is laid on where
    // that is not what the smoothed normal says: where the relief cuts a
    // step one vertex wide, the normal spreads it over two, and the riser
    // would read the top plane, smeared down the fall line. Where the two
    // agree the normal still decides, because it is continuous across a
    // triangle edge and the triangle's plane is not.
    var pn = wn;
    let gc = cross(dp_dx, dp_dy);
    if dot(gc, gc) > 0.0 {
        var gn = normalize(gc);
        if dot(gn, wn) < 0.0 {
            gn = -gn;
        }
        pn = normalize(mix(gn, wn, smoothstep(ground.wall_planes.y, ground.wall_planes.z, dot(gn, wn))));
    }
    let p_sin = length(pn.xz);
    // `pow(cos, k)` against `pow(sin, k)`: the planar and the wall stretch
    // are exact complements, so this crosses over at 45° by construction.
    let w_top = pow(abs(pn.y), ground.wall.y);
    let w_wall = pow(p_sin, ground.wall.y);
    var wall_mix = 0.0;
    if p_sin > ground.wall.x {
        wall_mix = w_wall / max(w_top + w_wall, 1e-6);
    }

    var w = vec4<f32>(1.0, 0.0, 0.0, 0.0);
#ifdef VERTEX_COLORS
    w = clamp(in.color, vec4<f32>(0.0), vec4<f32>(1.0));
#endif
    w = w / max(w.x + w.y + w.z + w.w, 1e-4);

    // --- The cliff, per pixel -------------------------------------------------
    //
    // The mesh's weights hold below; above `cliff.x` the pixel turns to rock
    // from the interpolated normal's own tilt, whatever its vertices said,
    // so the triangles' teeth never draw along the cliff's foot. A
    // world-space noise moves the threshold so the edge wanders like scree.
    // Nothing is sampled under this branch.
    let tan_tilt = sin_tilt / max(cos_tilt, 0.05);
    let reach = ground.cliff.z * 0.5;
    if tan_tilt > ground.cliff.x - reach {
        var cn = 0.0;
        if tan_tilt < ground.cliff.y + reach {
            let q = wp / ground.cliff.w;
            cn = value3(q) * 0.65 + value3(q * 2.7 + vec3<f32>(3.1, 7.7, 1.3)) * 0.35 - 0.5;
        }
        let cliff = smoothstep(ground.cliff.x, ground.cliff.y, tan_tilt + cn * ground.cliff.z);
        w = w * (1.0 - cliff) + vec4<f32>(0.0, 0.0, 0.0, cliff);
    }

    // --- The planar taps ---------------------------------------------------------
    //
    // Every set sampled every time, with implicit gradients: a sample under
    // a branch breaks the derivatives mip selection needs. The normals are
    // kept as gradients, to be summed.
    let uv0 = wp.xz / ground.tile_m.x;
    let uv1 = wp.xz / ground.tile_m.y;
    let uv2 = wp.xz / ground.tile_m.z;
    let uv3 = wp.xz / ground.tile_m.w;
    var a0 = textureSample(albedo_tex, albedo_smp, uv0, 0);
    var a1 = textureSample(albedo_tex, albedo_smp, uv1, 1);
    var a2 = textureSample(albedo_tex, albedo_smp, uv2, 2);
    var a3 = textureSample(albedo_tex, albedo_smp, uv3, 3);
    let g0 = to_gradient(unpack_normal(textureSample(normal_tex, normal_smp, uv0, 0)));
    let g1 = to_gradient(unpack_normal(textureSample(normal_tex, normal_smp, uv1, 1)));
    let g2 = to_gradient(unpack_normal(textureSample(normal_tex, normal_smp, uv2, 2)));
    let g3 = to_gradient(unpack_normal(textureSample(normal_tex, normal_smp, uv3, 3)));

    // --- The wall planes ----------------------------------------------------------
    //
    // Per set and only where it has weight: on a cliff the veto has left
    // rock alone, so a cliff pays one set's two maps on one plane, or two
    // where it turns between planes. Skipped whole below `WALL_ON`, which is
    // nearly every metre of the floor. The branch samples nothing itself:
    // `wall_tap` does, with explicit gradients, which is what makes it legal.
    var wm = vec4<f32>(0.0);
    var t0: WallTap;
    var t1: WallTap;
    var t2: WallTap;
    var t3: WallTap;
    if wall_mix > 0.0 {
        // The face's bearing, folded to half a turn (a plane reads both of
        // its sides) and counted in the 45° steps between planes: plane `ka`
        // and the next one round share it, over the middle
        // `WALL_PLANE_BLEND` of each step and nowhere else.
        var bearing = atan2(pn.z, pn.x);
        if bearing < 0.0 {
            bearing = bearing + 3.14159265;
        }
        let steps = bearing / 0.78539816;
        let ka = min(i32(steps), 3);
        let kb = (ka + 1) % 4;
        var wall: Wall;
        wall.share_b = clamp((steps - f32(ka) - 0.5) / ground.wall_planes.x + 0.5, 0.0, 1.0);
        wall.axis_a = wall_axis(ka);
        wall.uv_a = vec2<f32>(dot(wp.xz, wall.axis_a), -wp.y);
        wall.dx_a = vec2<f32>(dot(dp_dx.xz, wall.axis_a), -dp_dx.y);
        wall.dy_a = vec2<f32>(dot(dp_dy.xz, wall.axis_a), -dp_dy.y);
        wall.axis_b = wall_axis(kb);
        wall.uv_b = vec2<f32>(dot(wp.xz, wall.axis_b), -wp.y);
        wall.dx_b = vec2<f32>(dot(dp_dx.xz, wall.axis_b), -dp_dx.y);
        wall.dy_b = vec2<f32>(dot(dp_dy.xz, wall.axis_b), -dp_dy.y);
        if w.x > 0.0 {
            t0 = wall_tap(wall, 0, 1.0 / ground.tile_m.x);
            wm.x = wall_mix;
        }
        if w.y > 0.0 {
            t1 = wall_tap(wall, 1, 1.0 / ground.tile_m.y);
            wm.y = wall_mix;
        }
        if w.z > 0.0 {
            t2 = wall_tap(wall, 2, 1.0 / ground.tile_m.z);
            wm.z = wall_mix;
        }
        if w.w > 0.0 {
            t3 = wall_tap(wall, 3, 1.0 / ground.tile_m.w);
            wm.w = wall_mix;
        }
    }
    a0 = mix(a0, t0.albedo, wm.x);
    a1 = mix(a1, t1.albedo, wm.y);
    a2 = mix(a2, t2.albedo, wm.z);
    a3 = mix(a3, t3.albedo, wm.w);

    // --- The rock face ------------------------------------------------------------
    //
    // A smooth heightfield lights a scarp as one value, so the rock carries
    // its own structure: facets at two scales that each catch the sun at
    // their own angle, a few long cracks, weathering, and streaks down the
    // faces. Nothing here moves a weight; this decides what rock looks like.
    // Every pass fades out by the fragment's footprint, so a distant face
    // skips the blocks and does not sparkle.
    let fp_m = max(length(dp_dx), length(dp_dy));
    let big_px = fp_m / ground.rock_a.x;
    let fine_px = fp_m / ground.rock_a.y;
    let near_big = 1.0 - smoothstep(0.05, 0.25, big_px);
    let near_fine = 1.0 - smoothstep(0.05, 0.25, fine_px);
    var rock_shade = 1.0;
    var rock_tilt = vec3<f32>(0.0);
    if w.w > 0.002 {
        var shade = 1.0;
        var t_big = vec3<f32>(0.0);
        // Skipped where it would add nothing: every output of the block pass
        // is scaled by `near_big`, and a crack needs half a pixel of width,
        // which `ROCK_CRACK_W` keeps inside `near_big`.
        if near_big > 0.0 {
            // The block lattice is read through a warp so a boundary wanders
            // like a joint and is never a ruled polygon edge.
            let warp = vec3<f32>(value3(wp / 4.0), value3(wp / 4.0 + vec3<f32>(31.7, 11.3, 5.9)), value3(wp / 4.0 + vec3<f32>(7.1, 23.9, 17.3))) * 2.0 - 1.0;
            let big = rock_cell((wp + warp * ground.rock_b.w) / ground.rock_a.x);
            let d = rock_draw(big, 0u);
            shade = 1.0 + ground.rock_b.x * (2.0 * d.y - 1.0) * near_big;
            // Only some boundaries are cracks: a key per PAIR of blocks, so a
            // crack runs the length of the boundary it chose and stops.
            let pair = cell_key(vec3<i32>(bitcast<i32>(min(big.key, big.key2)), bitcast<i32>(max(big.key, big.key2)), 7));
            if unit_of(pair) < ground.rock_c.x {
                let crack_px = ground.rock_c.y / max(big_px, 1e-5);
                let on = smoothstep(0.5, 1.5, crack_px);
                shade = shade * (1.0 - ground.rock_c.z * on * (1.0 - smoothstep(0.0, ground.rock_c.y, big.edge)));
            }
            t_big = (rock_draw(big, 0x68e31da4u) * 2.0 - 1.0) * (ground.rock_a.z * near_big);
        }
        var t_fine = vec3<f32>(0.0);
        if near_fine > 0.0 {
            let fine = rock_cell(wp / ground.rock_a.y + vec3<f32>(17.3, 5.1, 9.7));
            t_fine = (rock_draw(fine, 0x1b873593u) * 2.0 - 1.0) * (ground.rock_a.w * near_fine);
            shade = shade * (1.0 + 0.5 * ground.rock_b.x * (2.0 * rock_draw(fine, 0u).y - 1.0) * near_fine);
        }
        // A facet leans on a face, where a scarp breaks into planes; on a
        // gentle slope the same lean is a slab tipped at random, and keeps
        // only `rock_e.y` of it.
        rock_tilt = (t_big + t_fine) * mix(ground.rock_e.y, 1.0, smoothstep(ground.rock_e.z, ground.rock_e.w, sin_tilt));
        // Weathering: broad patches, the scale a player reads from across
        // the canyon.
        let wx = value3(wp / ground.rock_b.y) * 0.65 + value3(wp / (ground.rock_b.y * 0.37)) * 0.35;
        shade = shade * (1.0 + ground.rock_b.z * (2.0 * wx - 1.0));
        // Water streaks down a face: a lattice stretched nine times taller
        // than it is wide, on faces too steep to walk.
        let face = smoothstep(ground.rock_d.z, ground.rock_d.w, sin_tilt);
        if face > 0.0 {
            let sw = ground.rock_d.x;
            let sh = ground.rock_d.y;
            let st = 0.65 * value3(vec3<f32>(wp.x / sw, wp.y / sh, wp.z / sw))
                + 0.35 * value3(vec3<f32>(wp.x / (sw * 0.44), wp.y / (sh * 0.36), wp.z / (sw * 0.44)));
            // Odd about 0.5, and the lattice's values are symmetric about
            // 0.5, so the face is lightened between streaks exactly as much
            // as it is darkened in them: the mean stays the photograph's.
            let streak = 2.0 * smoothstep(0.38, 0.62, st) - 1.0;
            shade = shade * (1.0 - ground.rock_c.w * face * streak);
        }
        rock_shade = shade;
    }

    // --- Colour and normal ---------------------------------------------------
    //
    // Each photograph times the map's own soil, the rock's by its blocks'
    // shade.
    let c = a0.rgb * ground.tint[0].rgb * w.x
        + a1.rgb * ground.tint[1].rgb * w.y
        + a2.rgb * ground.tint[2].rgb * w.z
        + a3.rgb * ground.tint[3].rgb * (w.w * rock_shade);

    // The relief, blended as gradients. The planar taps' share is laid in
    // the ground's own frame (u along world x, the image's up toward -z,
    // around the mesh's normal); each wall tap's is already a world offset
    // on its plane's axes, laid in the surface by removing its component
    // along the normal, so a steep face carries the photograph's relief and
    // not a planar tap's streak of it.
    let pg = g0 * (w.x * (1.0 - wm.x)) + g1 * (w.y * (1.0 - wm.y))
        + g2 * (w.z * (1.0 - wm.z)) + g3 * (w.w * (1.0 - wm.w));
    let tx = vec3<f32>(1.0, 0.0, 0.0) - wn * wn.x;
    let T = tx * inverseSqrt(max(dot(tx, tx), 1e-8));
    let B = cross(wn, T);
    let wall_relief = t0.relief * (w.x * wm.x) + t1.relief * (w.y * wm.y)
        + t2.relief * (w.z * wm.z) + t3.relief * (w.w * wm.w);
    let n0 = normalize(T * pg.x + B * pg.y + wn + wall_relief - dot(wall_relief, wn) * wn);
    // The block's facet, only where the surface is rock and only across the
    // surface: the tilt's component along the normal is removed, so a facet
    // leans and never lifts.
    let lean = (rock_tilt - dot(rock_tilt, n0) * n0) * w.w;
    pbr_input.N = normalize(n0 + lean);

    pbr_input.material.base_color = vec4<f32>(c, 1.0);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

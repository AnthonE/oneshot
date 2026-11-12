// The ground (look/ground.rs): four texture sets, blended by the splat
// weights in the vertex colour (sand, grass, gravel, rock), each tiled in
// world x and z at its own size, then Bevy's PBR lighting. A steep face is
// decided per pixel from the normal's tilt (the cliff veto), read off fixed
// vertical planes so its photograph is not stretched down the fall line
// (the wall planes), and broken into blocks with cracks and streaks (the
// rock face). After Gates' shaders/ground_splat.wgsl (MIT, NOTICE): its
// hash, value noise, rock cells, wall planes, cliff veto and rock face,
// copied and trimmed; without the island's roads, blight, snow, caustics and
// height blend. Every number arrives in `GroundParams` (binding 104, the
// ground's one uniform block), declared in `cloud_shadow.wgsl` with the
// rain's wet and the cloud deck's shadows it holds. The rock skin
// (look/cliffs.rs) carries its value multiplier in `UV_1.x`; the ground has
// no `UV_1`.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
}
#import "shaders/cloud_shadow.wgsl"::{ground, cloud_shadow, ground_wet, wetted}

// Wet ground's roughness (dry is the material's), and how far the wet
// line wanders (m) over its noise's wavelength (m).
const WET_ROUGHNESS: f32 = 0.35;
// What an island's stone goes to where its cloud laps it (`ground.sea.w`
// under zero, `ground::LAP_M`): the sea of cloud's own top
// (`cloud_sea::TOP`), linear.
const LAP_RGB: vec3<f32> = vec3<f32>(0.62, 0.63, 0.66);
// The rock's beds. Its photograph repeats every few metres, and a 45 m
// wall of one tile is a striped cloth; a cliff is beds of different stone,
// each a metre or three thick and its own tone, dipping and wandering along
// the wall. `BED_M` is a bed's thickness and `BED_RUN_M` how far one runs
// before it changes; the thick ones (`BED_THICK`) ride over them, some of
// them pale (`BED_PALE`, a cream caprock against the red). `BED_DIP_M` is
// how far a bed wanders off level over `BED_DIP_WAVE_M`, and the
// photograph is slid by the same so its laminations follow.
const BED_M: f32 = 1.9;
const BED_RUN_M: f32 = 70.0;
const BED_THICK: f32 = 3.6;
const BED_TONE: f32 = 0.2;
const BED_TONE_THICK: f32 = 0.16;
const BED_PALE: vec3<f32> = vec3<f32>(1.12, 1.16, 1.22);
const BED_DIP_M: f32 = 3.0;
const BED_DIP_WAVE_M: f32 = 27.0;
// The soil is never one value either: broad patches over the floor, this
// far either side of the photograph's mean, `SOIL_PATCH_M` across.
const SOIL_PATCH: f32 = 0.14;
const SOIL_PATCH_M: f32 = 19.0;
// An island's sand under the wind (`layers.x` over 3.5): broad pale crests and
// dark lees, this far under and over the photograph's mean, and fine ripple
// across the wind, `ISLE_RIPPLE_M` crest to crest, fading before a pixel can't
// hold it.
const ISLE_LEE: f32 = 0.66;
const ISLE_CREST: f32 = 1.14;
const ISLE_RIPPLE: f32 = 0.05;
const ISLE_RIPPLE_M: f32 = 0.5;
// How much of the skin's baked value the rock keeps: at full strength its
// recesses read as stains across the beds.
const SKIN_VALUE: f32 = 0.25;
const WET_WANDER_M: f32 = 0.25;
const WET_WANDER_WAVE_M: f32 = 3.0;
// A valley under snow (`ground.layers.z` set): its rock is a wall a hundred
// metres high read from across the valley, where a block's own patch of
// snow is a mottle of two colours and a line along a bed is a contour.
// Its beds are bands with hard tops (`LEDGE_DEEP`'s pitch), each its own
// tone (`BAND_TONE` either way), and snow lies on their ledges in shelves,
// two families (each `[pitch m, the deepest a shelf gets m, the share of
// stretches that hold one, a stretch's length m]`): a stretch of a bed
// holds a shelf or none, its depth rising and falling along it
// (`LEDGE_RUN_M`) and gone in places, its lower edge the ledge's clean lip
// (`LIP_AT` of the way up its bed), ragged above where it banks against
// the riser, the rock under the lip in shade (`LIP_SHADE` over
// `LIP_SHADE_M`). Where the face leans back (the normal's y from `LEAN.x`
// to `LEAN.y`: 59° to 44°) more stretches hold one (`LEAN_SHARE`) and
// deeper (`LEAN_DEEP` times). Too far to draw at its depth a shelf is a
// line a pixel deep and as much fainter, never a level lift. The beds dip
// `LEDGE_DIP_M` over `LEDGE_DIP_WAVE_M` (each family its own way) and waver
// `LEDGE_WAVER_M` every few metres.
// Snow falls down its couloirs: where the mesh says a face is hollow (its
// first lane under snow, `level::face_hollow`: from none at `COULOIR.x` to
// whole at `.y`, its edge ragged by `COULOIR_RAG`), on a face tall enough
// to feed one (its second lane, under a half: a massif's or a spur's face
// holds shelves and no more; a slot cut square into the mountain is a
// plane there, `level::cut`). It is a thread: thinner by `COULOIR_THIN`
// where its reach is bare (`GULLY_BARE` of its reaches, `GULLY_REACH_M`
// tall), swelling and pinching between (`COULOIR_SWELL`). At the wall's
// foot what slid lies in a bank whose top is the mesh's too (the second
// lane over a half: a hollow read three times as broad as the gully's):
// at `FAN_Y.x` (a height over the floor) under a buttress's nose, at
// `FAN_Y.y` under a couloir (from `BANK.x` to `.y` of that lane), so it
// stands highest under each couloir and sags between, wider than it is
// tall, its edge ragged (`BANK_EDGE_M`: how soft, how far it wanders) and
// streaked by what slid. The rock itself is paler on a buttress's nose and
// darker in the cleft beside it (`NOSE_TONE`), so the wall's relief reads
// where the light is flat on it.
// Noise through a threshold drew islands and ribbons that bent together at
// one height; a cut lowered along the foot drew a row of flames, on a
// noise and on the mesh's hollows alike; sheets on a face that leans drew
// ovals; lines on a lattice of their own, each with its cone, drew
// pinstripes and a row of teeth or of tents, and ran down buttress and
// couloir alike; a bank as high as the gully's own narrow hollow drew a
// tusk under each; reaches cut to nothing drew a column of lozenges.
// A low face has beds at its own scale: the thin family's are bands too
// there (`BAND_TONE`), and more of its stretches hold a shelf (`LOW_SHELF`,
// as a face that leans by that much).
// Snow on rock is the field's snow (`SNOW_ON_ROCK` of the way from its lee
// to its crest) and faces the sky more than its face does (`SNOW_UP`).
const LEDGE_DEEP: vec4<f32> = vec4<f32>(11.5, 2.4, 0.5, 90.0);
const LEDGE_THIN: vec4<f32> = vec4<f32>(3.3, 0.7, 0.3, 14.0);
const LEDGE_DIP_M: f32 = 6.0;
const LEDGE_DIP_WAVE_M: f32 = 95.0;
const LEDGE_WAVER_M: f32 = 0.8;
const LEDGE_RUN_M: f32 = 14.0;
const LIP_AT: f32 = 0.3;
const LIP_SHADE: f32 = 0.45;
const LIP_SHADE_M: f32 = 1.6;
const BAND_TONE: f32 = 0.3;
const LEAN: vec2<f32> = vec2<f32>(0.52, 0.72);
const LEAN_SHARE: f32 = 0.4;
const LEAN_DEEP: f32 = 1.5;
const COULOIR: vec2<f32> = vec2<f32>(0.68, 0.8);
const COULOIR_THIN: f32 = 0.09;
const NOSE_TONE: f32 = 0.22;
const COULOIR_RAG: f32 = 0.07;
const GULLY_REACH_M: f32 = 12.0;
const COULOIR_SWELL: f32 = 0.07;
const GULLY_BARE: f32 = 0.5;
const FAN_Y: vec2<f32> = vec2<f32>(11.0, 22.0);
const BANK: vec2<f32> = vec2<f32>(0.05, 0.9);
const BANK_EDGE_M: vec2<f32> = vec2<f32>(2.5, 1.6);
const LOW_SHELF: f32 = 0.45;
const SNOW_ON_ROCK: f32 = 0.6;
const SNOW_UP: f32 = 1.1;
// Its beds, read from across a valley: `VALLEY_BED` times the canyon's
// thickness and tone, and drawn under the blocks' own shade, not in its place.
const VALLEY_BED: vec2<f32> = vec2<f32>(3.2, 1.25);
// Its snow, the sand's lane: a valley's is laid by the wind, not trodden.
// The set's dark lines (boot prints, cracks) are lifted to `SNOW_FLOOR` of
// its mean, and the set fades to that mean and its relief to nothing as a
// pixel's footprint grows from `SNOW_NEAR_M.x` to `.y` (its tile is a weave
// from a height). Drifts `SNOW_DRIFT_M` long down the wind, a third as
// wide, carry the field: their lees `SNOW_LEE` (toward blue) and crests
// `SNOW_CREST` off the mean (an island's sand: `ISLE_LEE`).
const SNOW_FLOOR: f32 = 0.9;
const SNOW_MEAN: f32 = 0.49;
const SNOW_RELIEF: f32 = 0.8;
const SNOW_NEAR_M: vec2<f32> = vec2<f32>(0.025, 0.11);
const SNOW_DRIFT_M: f32 = 34.0;
const SNOW_LEE: vec3<f32> = vec3<f32>(0.8, 0.86, 0.97);
const SNOW_CREST: f32 = 1.06;
// Between a drift and the set's grain, wind ridges `SNOW_RIDGE_M` long and
// a third as wide, `SNOW_RIDGE` of the field's run (gone to their mean when
// a pixel is their size). And the field stands: its run tilts the normal
// (`SNOW_TILT`: over how far it is read, m, and how much it leans), so a
// drift's lee takes shade under the sun and not a tone alone.
const SNOW_RIDGE_M: f32 = 3.6;
const SNOW_RIDGE: f32 = 0.22;
const SNOW_TILT: vec2<f32> = vec2<f32>(0.5, 0.55);
// It lies thin where the mesh says (its third lane under snow,
// `grass::snow_thin`: crowns and shoulders, flanks, the ice's shore and
// its islets; the cover's stones and tufts stand by the same rule): there
// the ground shows through in windows with a ragged edge (a noise
// `SCOUR_M` across against the lane, `SCOUR_EDGE` soft), `SCOUR_THIN` of
// the way to `SCOUR_GROUND`, with its stones (`SCOUR_STONE`, of two sizes
// `SCOUR_STONE_M`). Just over the ice (`SCOUR_ICE_M`, m over the sheet,
// from all to none) what shows is the sheet's own rubble, `ICE_RUBBLE`.
// Colours in the snow's lane, before its tint.
const SCOUR_M: f32 = 6.0;
const SCOUR_LONG: f32 = 2.2;
const SCOUR_BEND_M: f32 = 3.0;
const SCOUR_EDGE: f32 = 0.06;
const SCOUR_THIN: f32 = 0.85;
const SCOUR_GROUND: vec3<f32> = vec3<f32>(0.115, 0.1, 0.082);
const SCOUR_GRAVEL_MEAN: f32 = 0.3;
const SCOUR_GRAIN: f32 = 0.7;
const SCOUR_FAR: vec2<f32> = vec2<f32>(0.2, 0.16);
const SCOUR_STONE: vec3<f32> = vec3<f32>(0.07, 0.07, 0.075);
const SCOUR_STONE_M: vec2<f32> = vec2<f32>(0.38, 1.3);
const SCOUR_ICE_M: vec2<f32> = vec2<f32>(0.1, 0.5);
const ICE_RUBBLE: vec3<f32> = vec3<f32>(0.24, 0.29, 0.36);
// And its ice (`ground.sea.w` under −1.5: the sea's lanes are the ice's
// level, how far over it is still ice and over how much more the snow takes
// over): a sheet paler than rock and darker than snow, smooth where the
// snow is rough, cracked into plates `ICE_PLATE_M` across, with snow blown
// over `ICE_DRIFT` of it in streaks `ICE_DRIFT_M` long.
const ICE_RGB: vec3<f32> = vec3<f32>(0.4, 0.48, 0.57);
const ICE_ROUGHNESS: f32 = 0.22;
const ICE_ROUGH_DUSTED: f32 = 0.6;
const ICE_PLATE_M: f32 = 9.0;
const ICE_CRACK: f32 = 0.03;
const ICE_CRACK_RGB: vec3<f32> = vec3<f32>(0.74, 0.82, 0.9);
const ICE_DRIFT: f32 = 0.3;
const ICE_DRIFT_M: f32 = 36.0;
// A dark line where the sheet meets the shore (a tide crack), this dark.
const ICE_RIM: f32 = 0.45;

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var albedo_tex: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var albedo_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var normal_tex: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var normal_smp: sampler;

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

// A valley's snow's run at `q` (in the plan): its drifts down the wind, a
// broader swell, and its wind ridges (`ridged`: how much of them a pixel
// this size still shows).
fn snow_run(q: vec3<f32>, ridged: f32) -> f32 {
    let far = 0.6 * value3(vec3<f32>(q.x / SNOW_DRIFT_M, 0.0, q.z / (SNOW_DRIFT_M * 0.33)) + vec3<f32>(6.1, 0.0, 2.9))
        + 0.4 * value3(q / 7.0 + vec3<f32>(3.3, 0.0, 8.1));
    let near = value3(vec3<f32>(q.x / SNOW_RIDGE_M, 0.0, q.z / (SNOW_RIDGE_M * 0.33)) + vec3<f32>(1.3, 0.0, 4.7)) - 0.5;
    return far + SNOW_RIDGE * ridged * near;
}

// One family of a valley's shelves (`f`: its pitch, the deepest a shelf
// gets, the share of stretches that hold one, a stretch's length; all m but
// the share) on a face that leans back by `lean`: the snow on it (x) and
// the shade under its lip (y). A bed is cut into stretches (a lattice in
// the plan, each bed its own); a stretch holds a shelf or none, by its own
// draw, of its own depth (most thin, a few deep), pinched out toward the
// stretch's ends and where its run fails.
fn ledge_snow(wp: vec3<f32>, by: f32, f: vec4<f32>, fp_m: f32, lean: f32) -> vec2<f32> {
    let bed = by / f.x - LIP_AT;
    let i = i32(floor(bed));
    // Each bed's lattice stands its own way off the next's.
    let off = draw3(cell_key(vec3<i32>(i, 17, 5))).xy;
    let q = vec2<f32>(wp.x, wp.z) / f.w + off;
    let cell = vec2<i32>(floor(q));
    let d = draw3(cell_key(vec3<i32>(cell.x, i, cell.y)));
    // A lip's shade belongs to the shelf over it: the bed above this one's.
    let up = fract(bed) * f.x;
    let under = up - f.x;
    var shade = 0.0;
    let sm = min(LIP_SHADE_M, (1.0 - LIP_AT) * f.x * 0.4);
    if under > -sm {
        let off1 = draw3(cell_key(vec3<i32>(i + 1, 17, 5))).xy;
        let q1 = vec2<f32>(wp.x, wp.z) / f.w + off1;
        let c1 = vec2<i32>(floor(q1));
        let d1 = draw3(cell_key(vec3<i32>(c1.x, i + 1, c1.y)));
        if d1.x < min(f.z + LEAN_SHARE * lean, 1.0) {
            let g1 = q1 - floor(q1);
            let ends1 = smoothstep(0.0, 0.06, min(min(g1.x, 1.0 - g1.x), min(g1.y, 1.0 - g1.y)));
            let run1 = value3(vec3<f32>(wp.x / LEDGE_RUN_M, f32(i + 1) * 3.7, wp.z / LEDGE_RUN_M));
            let depth1 = f.y * (0.15 + 0.85 * d1.y * d1.y) * ends1 * smoothstep(0.28, 0.6, run1);
            shade = LIP_SHADE * (1.0 - smoothstep(0.0, sm, -under)) * clamp(depth1 / 0.5, 0.0, 1.0)
                * (1.0 - smoothstep(0.5, 2.0, fp_m / sm));
        }
    }
    if d.x >= min(f.z + LEAN_SHARE * lean, 1.0) {
        return vec2<f32>(0.0, shade);
    }
    let g = q - floor(q);
    let ends = smoothstep(0.0, 0.06, min(min(g.x, 1.0 - g.x), min(g.y, 1.0 - g.y)));
    let run = value3(vec3<f32>(wp.x / LEDGE_RUN_M, f32(i) * 3.7, wp.z / LEDGE_RUN_M));
    let depth = f.y * (0.15 + 0.85 * d.y * d.y) * (1.0 + LEAN_DEEP * lean) * ends * smoothstep(0.28, 0.6, run);
    // Never thinner than a pixel, and as much fainter.
    let deep = max(depth, fp_m);
    // Clean along the lip, ragged where it banks against the riser.
    let rag = value3(vec3<f32>(wp.x / 1.3, f32(i) * 1.9, wp.z / 1.3));
    let top = deep * (0.65 + 0.7 * rag);
    let lip = max(0.05 * f.y, fp_m * 0.5);
    let line = smoothstep(0.0, lip, up) * (1.0 - smoothstep(top * 0.7, top, up));
    return vec2<f32>(line * depth / deep, shade);
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
    // The light tier (`wall.z`, a software GPU) keeps the splat, the veto
    // and the planar taps, and skips the wall planes and the rock face:
    // a cliff pixel there costs twelve of the floor's.
    let lite = ground.wall.z > 0.5;
    var wall_mix = 0.0;
    if p_sin > ground.wall.x {
        wall_mix = select(w_wall / max(w_top + w_wall, 1e-6), 0.0, lite);
    }

    var w = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    var face_hollow = 0.5;
    var face_tall = 0.0;
    var bank_hollow = 0.0;
    var snow_thin = 0.0;
#ifdef VERTEX_COLORS
    w = clamp(in.color, vec4<f32>(0.0), vec4<f32>(1.0));
    // A valley has two grounds, snow and rock, and the rock's lane says
    // which. The other three are no weights there (`level::ground_point`):
    // how far into a couloir a face is, how tall it is, how thin the snow.
    if ground.layers.z > 0.5 {
        face_hollow = w.x;
        // The second is two things (`level::ground_point`): under a half,
        // how tall the face is; over it, how far into a broad hollow.
        face_tall = smoothstep(0.1, 0.45, w.y);
        bank_hollow = clamp((w.y - 0.5) * 2.0, 0.0, 1.0);
        snow_thin = w.z;
        w = vec4<f32>(1.0 - w.w, 0.0, 0.0, w.w);
    }
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
    var a0 = textureSample(albedo_tex, albedo_smp, uv0, i32(ground.layers.y));
    var a1 = textureSample(albedo_tex, albedo_smp, uv1, 1);
    var a2 = textureSample(albedo_tex, albedo_smp, uv2, 2);
    let rock_layer = i32(ground.layers.x);
    var a3 = textureSample(albedo_tex, albedo_smp, uv3, rock_layer);
    let g0 = to_gradient(unpack_normal(textureSample(normal_tex, normal_smp, uv0, i32(ground.layers.y))));
    let g1 = to_gradient(unpack_normal(textureSample(normal_tex, normal_smp, uv1, 1)));
    let g2 = to_gradient(unpack_normal(textureSample(normal_tex, normal_smp, uv2, 2)));
    let g3 = to_gradient(unpack_normal(textureSample(normal_tex, normal_smp, uv3, rock_layer)));

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
        // The beds' dip (`BED_DIP_M`): smooth and tens of metres long, so
        // the gradients below still pick the photograph's mip.
        let dip = (value3(wp / BED_DIP_WAVE_M) - 0.5) * BED_DIP_M;
        wall.axis_a = wall_axis(ka);
        wall.uv_a = vec2<f32>(dot(wp.xz, wall.axis_a), -wp.y - dip);
        wall.dx_a = vec2<f32>(dot(dp_dx.xz, wall.axis_a), -dp_dx.y);
        wall.dy_a = vec2<f32>(dot(dp_dy.xz, wall.axis_a), -dp_dy.y);
        wall.axis_b = wall_axis(kb);
        wall.uv_b = vec2<f32>(dot(wp.xz, wall.axis_b), -wp.y - dip);
        wall.dx_b = vec2<f32>(dot(dp_dx.xz, wall.axis_b), -dp_dx.y);
        wall.dy_b = vec2<f32>(dot(dp_dy.xz, wall.axis_b), -dp_dy.y);
        if w.x > 0.0 {
            // The first lane's own layer: sand, or the drifts under snow
            // (read as sand, a snowed bank went orange where it steepened).
            t0 = wall_tap(wall, i32(ground.layers.y), 1.0 / ground.tile_m.x);
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
            t3 = wall_tap(wall, rock_layer, 1.0 / ground.tile_m.w);
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
    var rock_hue = vec3<f32>(1.0);
    var rock_tilt = vec3<f32>(0.0);
    if w.w > 0.002 && !lite {
        var shade = 1.0;
        // The beds: each its own tone, the thick ones over the thin, a few
        // of the thick ones pale.
        let by = wp.y + (value3(wp / BED_DIP_WAVE_M) - 0.5) * BED_DIP_M;
        let in_valley = ground.layers.z > 0.5;
        let bed_m = BED_M * select(1.0, VALLEY_BED.x, in_valley);
        let bed_k = select(1.0, VALLEY_BED.y, in_valley);
        let bed = value3(vec3<f32>(wp.x / BED_RUN_M, by / bed_m, wp.z / BED_RUN_M));
        let thick = value3(vec3<f32>(wp.x / (BED_RUN_M * 1.7) + 5.3, by / (bed_m * BED_THICK), wp.z / (BED_RUN_M * 1.7) + 2.9));
        shade = 1.0 + bed_k * (BED_TONE * (2.0 * smoothstep(0.3, 0.7, bed) - 1.0) + BED_TONE_THICK * (2.0 * smoothstep(0.35, 0.65, thick) - 1.0));
        rock_hue = mix(vec3<f32>(1.0), BED_PALE, smoothstep(0.62, 0.8, thick));
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
            // A valley's beds stay under its blocks' shade; the canyon's
            // blocks take their place (its look as it was judged).
            let blocks = 1.0 + ground.rock_b.x * (2.0 * d.y - 1.0) * near_big;
            if in_valley {
                shade = shade * blocks;
            } else {
                shade = blocks;
            }
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
    let soil = 1.0 + SOIL_PATCH * (2.0 * (0.65 * value3(vec3<f32>(wp.x, 0.0, wp.z) / SOIL_PATCH_M)
        + 0.35 * value3(vec3<f32>(wp.x, 0.0, wp.z) / (SOIL_PATCH_M * 0.31) + vec3<f32>(9.1, 0.0, 4.7))) - 1.0);
    let ripple_px = length(dp_dx.xz) + length(dp_dy.xz);
    var sand_wind = 1.0;
    if ground.layers.x > 3.5 {
        let q = vec3<f32>(wp.x, 0.0, wp.z);
        let mass = 0.62 * value3(q / 15.0 + vec3<f32>(3.3, 0.0, 8.1)) + 0.38 * value3(q / 6.0 + vec3<f32>(1.7, 0.0, 5.3));
        sand_wind = mix(ISLE_LEE, ISLE_CREST, smoothstep(0.34, 0.66, mass));
        let wander = 1.5 * value3(q / 2.4 + vec3<f32>(7.0, 0.0, 2.0));
        let keep = clamp(1.5 - ripple_px / ISLE_RIPPLE_M * 3.0, 0.0, 1.0);
        sand_wind *= 1.0 + ISLE_RIPPLE * keep * sin((wp.x * 0.8 + wp.z * 0.6 + wander) * 6.2831853 / ISLE_RIPPLE_M);
    }
    // A valley's snow is untrodden: nothing in it darker than `SNOW_FLOOR`
    // of its mean, the set gone to its mean with distance, and the field
    // carried by drifts down the wind: pale crests, lees toward blue.
    var snow0 = a0.rgb * sand_wind;
    var snow_rock = a0.rgb;
    var snow_near = 1.0;
    var snow_tilt = vec3<f32>(0.0);
    if ground.layers.z > 0.5 {
        snow_near = 1.0 - smoothstep(SNOW_NEAR_M.x, SNOW_NEAR_M.y, fp_m);
        let q = vec3<f32>(wp.x, 0.0, wp.z);
        let ridged = 1.0 - smoothstep(0.25, 1.0, fp_m / SNOW_RIDGE_M);
        let run = snow_run(q, ridged);
        let drift = smoothstep(0.3, 0.7, run);
        // The drifts stand: their backs tilt the snow, so a lee takes shade.
        snow_tilt = vec3<f32>(run - snow_run(q + vec3<f32>(SNOW_TILT.x, 0.0, 0.0), ridged), 0.0, run - snow_run(q + vec3<f32>(0.0, 0.0, SNOW_TILT.x), ridged)) * (SNOW_TILT.y / SNOW_TILT.x);
        let flat0 = mix(vec3<f32>(SNOW_MEAN), max(a0.rgb, vec3<f32>(SNOW_FLOOR * SNOW_MEAN)), snow_near);
        snow0 = flat0 * mix(SNOW_LEE, vec3<f32>(SNOW_CREST), drift);
        snow_rock = flat0 * mix(SNOW_LEE, vec3<f32>(SNOW_CREST), SNOW_ON_ROCK);
        // Thin where the mesh says: the ground and its stones through it, in windows.
        if snow_thin > 0.02 && w.x > 0.01 {
            // The windows lie down the wind, their noise read through a warp
            // (its lattice was drawn as squares) and with a fine octave on its edge.
            let bend = vec3<f32>(value3(q / 9.0 + vec3<f32>(4.4, 0.0, 1.2)), 0.0, value3(q / 9.0 + vec3<f32>(7.7, 0.0, 9.3))) * 2.0 - 1.0;
            let sq = vec3<f32>(q.x / SCOUR_LONG, 0.0, q.z) + bend * SCOUR_BEND_M;
            let lies = 0.5 * value3(sq / SCOUR_M + vec3<f32>(1.7, 0.0, 5.3)) + 0.32 * value3(sq / (SCOUR_M * 0.28) + vec3<f32>(8.9, 0.0, 2.1))
                + 0.18 * value3(q / 0.55 + vec3<f32>(2.2, 0.0, 6.6));
            // Open before the cover stands (`grass::OPEN`), so a tuft has ground at its foot.
            let scour = smoothstep(-SCOUR_EDGE, SCOUR_EDGE, snow_thin - (0.85 * lies - 0.05));
            // Stones of two sizes, each gone to its share of the window when under a pixel.
            let small = smoothstep(0.66, 0.72, value3(wp / SCOUR_STONE_M.x));
            let big = smoothstep(0.7, 0.76, value3(wp / SCOUR_STONE_M.y + vec3<f32>(3.1, 7.7, 1.3)));
            let stones = max(mix(SCOUR_FAR.x, small, 1.0 - smoothstep(0.3, 1.2, fp_m / SCOUR_STONE_M.x)),
                mix(SCOUR_FAR.y, big, 1.0 - smoothstep(0.3, 1.2, fp_m / SCOUR_STONE_M.y)));
            // The ground is the gravel's photograph (a valley weighs it nothing else), in the window's own colour.
            let gravel = SCOUR_GROUND * mix(vec3<f32>(1.0), a2.rgb / SCOUR_GRAVEL_MEAN, SCOUR_GRAIN * snow_near);
            let bare = mix(ICE_RUBBLE, gravel, smoothstep(SCOUR_ICE_M.x, SCOUR_ICE_M.y, in.world_position.y - ground.sea.x));
            snow0 = mix(mix(snow0, bare, SCOUR_THIN * scour), SCOUR_STONE, stones * scour);
        }
    }
    var c = (snow0 * ground.tint[0].rgb * w.x
        + a1.rgb * ground.tint[1].rgb * w.y
        + a2.rgb * ground.tint[2].rgb * w.z) * soil
        + a3.rgb * ground.tint[3].rgb * rock_hue * (w.w * rock_shade);
#ifdef VERTEX_UVS_B
    // The skin's facets: recessed darker, proud lighter.
    c *= mix(1.0, in.uv_b.x, SKIN_VALUE);
#endif

    // An island under snow (`layers.y` over 4.5): snow lying on its rock's
    // ledges, where a facet turns up, in patches. A valley's (`layers.z`):
    // on the beds' ledges and where the face leans back.
    var lie_up = 0.0;
    if ground.layers.y > 4.5 && w.w > 0.002 {
        var lie = 0.0;
        if ground.layers.z > 0.5 {
            let waver = (value3(wp / 5.0 + vec3<f32>(2.0, 9.0, 4.0)) - 0.5) * 2.0 * LEDGE_WAVER_M;
            let by = wp.y + (value3(wp / BED_DIP_WAVE_M) - 0.5) * BED_DIP_M + waver;
            // One dip for both families: strata lie parallel.
            let dip = value3(wp / LEDGE_DIP_WAVE_M + vec3<f32>(11.0, 3.0, 7.0)) - 0.5;
            let lean = smoothstep(LEAN.x, LEAN.y, wn.y);
            let by_deep = by + dip * LEDGE_DIP_M;
            let deep = ledge_snow(wp, by_deep, LEDGE_DEEP, fp_m, lean);
            let low = 1.0 - face_tall;
            let thin = ledge_snow(wp, by_deep + 1.7, LEDGE_THIN, fp_m, lean + LOW_SHELF * low);
            // The deep beds as bands: each its own tone, hard along its top (the lip its shelf lies on).
            let band = by_deep / LEDGE_DEEP.x - LIP_AT;
            let bi = i32(floor(band));
            let over = smoothstep(0.0, 2.0 * max(fp_m, 0.05) / LEDGE_DEEP.x, fract(band));
            let tone = 1.0 + BAND_TONE * (2.0 * mix(draw3(cell_key(vec3<i32>(bi - 1, 41, 3))).x, draw3(cell_key(vec3<i32>(bi, 41, 3))).x, over) - 1.0);
            // A low face's thin beds are bands too: beds at its own scale.
            let band2 = (by_deep + 1.7) / LEDGE_THIN.x - LIP_AT;
            let bj = i32(floor(band2));
            let over2 = smoothstep(0.0, 2.0 * max(fp_m, 0.05) / LEDGE_THIN.x, fract(band2));
            let tone2 = 1.0 + BAND_TONE * low * (2.0 * mix(draw3(cell_key(vec3<i32>(bj - 1, 43, 7))).x, draw3(cell_key(vec3<i32>(bj, 43, 7))).x, over2) - 1.0)
                * (1.0 - smoothstep(0.3, 1.2, fp_m / LEDGE_THIN.x));
            c = c * mix(1.0, tone * tone2 * (1.0 - max(deep.y, thin.y)), w.w);
            // A buttress's nose is paler than the cleft beside it: the wall's
            // relief by tone, where the light is flat on it.
            c = c * mix(1.0, 1.0 + NOSE_TONE * clamp((0.5 - face_hollow) * 2.0, -1.0, 1.0), w.w * face_tall);
            // Down its couloirs: a thread that thins where its reach is bare, never a row of lozenges.
            let rag = (0.65 * value3(wp / 2.3 + vec3<f32>(4.0, 1.0, 9.0)) + 0.35 * value3(wp / 0.8 + vec3<f32>(1.0, 5.0, 3.0)) - 0.5) * 2.0 * COULOIR_RAG;
            let reach = value3(vec3<f32>(wp.x / 19.0, wp.y / GULLY_REACH_M, wp.z / 19.0) + vec3<f32>(9.0, 4.0, 1.0));
            let fed = smoothstep(GULLY_BARE - 0.1, GULLY_BARE + 0.1, reach);
            let swell = (value3(vec3<f32>(wp.x / 31.0, wp.y / 7.0, wp.z / 31.0) + vec3<f32>(2.0, 6.0, 5.0)) - 0.5) * 2.0 * COULOIR_SWELL;
            let line = smoothstep(COULOIR.x, COULOIR.y, face_hollow + rag + swell - COULOIR_THIN * (1.0 - fed));
            // The bank at its foot: as high as the broad hollow over it is deep, wider than it is tall.
            let top = mix(FAN_Y.x, FAN_Y.y, smoothstep(BANK.x, BANK.y, bank_hollow)) + rag / COULOIR_RAG * BANK_EDGE_M.y;
            let slid = 1.0 - 0.25 * value3(vec3<f32>(wp.x / 1.6, wp.y / 15.0, wp.z / 1.6));
            let bank = (1.0 - smoothstep(top - BANK_EDGE_M.x, top, wp.y)) * slid;
            let gully = max(line, bank) * face_tall;
            lie = max(max(deep.x, thin.x), gully);
        } else {
            let up = wn.y + rock_tilt.y;
            lie = smoothstep(0.12, 0.32, up) * smoothstep(0.3, 0.7, value3(wp / 3.0));
        }
        lie_up = select(0.0, w.w * lie, ground.layers.z > 0.5);
        c = mix(c, snow_rock * ground.tint[0].rgb, w.w * lie * select(0.85, 0.94, ground.layers.z > 0.5));
    }

    // The swash: from under the water to `sea.y` above it the ground is
    // wet, darker and glossy, dry again `sea.z` higher, the line wandering
    // a little so it is not a contour. Only the band pays for its noise.
    let wet_top = ground.sea.x + ground.sea.y;
    var ice = 0.0;
    if in.world_position.y < wet_top + ground.sea.z + WET_WANDER_M {
        let wander = (value3(in.world_position.xyz / WET_WANDER_WAVE_M) - 0.5) * 2.0 * WET_WANDER_M;
        let wet = 1.0 - smoothstep(wet_top + wander, wet_top + wander + ground.sea.z, in.world_position.y);
        if ground.sea.w < -1.5 {
            // A valley's ice: the shore wanders a hand's height, never over the sheet itself.
            let top = wet_top + wander * 0.2;
            let q = vec3<f32>(wp.x, 0.0, wp.z);
            // Snow blown over it in streaks down the wind, the sheet bare between.
            let drift = 0.7 * value3(vec3<f32>(wp.x / ICE_DRIFT_M, 0.0, wp.z / (ICE_DRIFT_M * 0.16)) + vec3<f32>(2.7, 0.0, 6.1))
                + 0.3 * value3(vec3<f32>(wp.x / 9.0, 0.0, wp.z / 2.2) + vec3<f32>(8.3, 0.0, 1.9));
            let bare = 1.0 - smoothstep(1.0 - ICE_DRIFT - 0.1, 1.0 - ICE_DRIFT + 0.1, drift);
            let sheet_at = 1.0 - smoothstep(top, top + ground.sea.z, in.world_position.y);
            ice = sheet_at * bare;
            // The tide crack: a dark line where the sheet leaves the shore.
            let rim = sheet_at * (1.0 - sheet_at) * 4.0;
            c = c * (1.0 - ICE_RIM * rim);
            // Its plates' cracks, pale lines a pixel wide at least.
            let warp = vec3<f32>(value3(q / 5.0), 0.0, value3(q / 5.0 + vec3<f32>(13.1, 0.0, 7.7))) * 2.0 - 1.0;
            let plate = rock_cell((q + warp * 1.6) / ICE_PLATE_M);
            let crack_px = ICE_CRACK * ICE_PLATE_M / max(fp_m, 1e-5);
            let crack = (1.0 - smoothstep(0.0, ICE_CRACK, plate.edge)) * smoothstep(0.4, 1.2, crack_px);
            // Each plate its own tone, and the depth under it showing darker in patches.
            let dusted = rock_draw(plate, 0u).y;
            let tone = 0.8 + 0.42 * dusted;
            let sheet = mix(ICE_RGB * tone, ICE_CRACK_RGB, crack * 0.8);
            c = mix(c, sheet, ice);
            pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, mix(ICE_ROUGHNESS, ICE_ROUGH_DUSTED, dusted * dusted), ice * (1.0 - crack));
        } else if ground.sea.w < 0.0 {
            // An island's cloud: the stone lost in it from its foot up.
            c = mix(c, LAP_RGB, wet);
        } else {
            c = c * (1.0 - ground.sea.w * wet);
            pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, WET_ROUGHNESS, wet);
        }
    }

    // The relief, blended as gradients. The planar taps' share is laid in
    // the ground's own frame (u along world x, the image's up toward -z,
    // around the mesh's normal); each wall tap's is already a world offset
    // on its plane's axes, laid in the surface by removing its component
    // along the normal, so a steep face carries the photograph's relief and
    // not a planar tap's streak of it.
    let relief0 = select(1.0, SNOW_RELIEF * snow_near, ground.layers.z > 0.5);
    let pg = g0 * (w.x * (1.0 - wm.x) * relief0) + g1 * (w.y * (1.0 - wm.y))
        + g2 * (w.z * (1.0 - wm.z)) + g3 * (w.w * (1.0 - wm.w));
    let tx = vec3<f32>(1.0, 0.0, 0.0) - wn * wn.x;
    let T = tx * inverseSqrt(max(dot(tx, tx), 1e-8));
    let B = cross(wn, T);
    let wall_relief = t0.relief * (w.x * wm.x) + t1.relief * (w.y * wm.y)
        + t2.relief * (w.z * wm.z) + t3.relief * (w.w * wm.w);
    // Ice is smooth: none of the snow's relief on it.
    let n0 = normalize((T * pg.x + B * pg.y) * (1.0 - ice) + wn + wall_relief - dot(wall_relief, wn) * wn + snow_tilt * (w.x * (1.0 - ice)));
    // The block's facet, only where the surface is rock and only across the
    // surface: the tilt's component along the normal is removed, so a facet
    // leans and never lifts.
    let lean = (rock_tilt - dot(rock_tilt, n0) * n0) * w.w;
    pbr_input.N = normalize(n0 + lean + vec3<f32>(0.0, SNOW_UP * lie_up, 0.0));

    // Rain: darker and more saturated where the ground faces the sky, and
    // smoother (Gates' `WET_ROUGH`, the wet term's keep).
    let wet = ground_wet(wn);
    pbr_input.material.base_color = vec4<f32>(wetted(c, wet), 1.0);
    pbr_input.material.perceptual_roughness = pbr_input.material.perceptual_roughness * (1.0 - wet * 0.25);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = vec4(out.color.rgb + view.exposure * cloud_shadow(pbr_input), out.color.a);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

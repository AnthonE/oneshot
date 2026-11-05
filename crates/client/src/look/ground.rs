//! The ground: four photographed sets (sand, grass, the creek's gravel,
//! rock) blended by the splat weights the terrain mesh carries in its
//! vertex colour, each at its own photographed size, over Bevy's PBR. A
//! steep face is decided per pixel from the normal's tilt (the cliff veto),
//! read off fixed vertical planes so its photograph is not stretched down
//! the fall line (the wall planes), and broken into blocks with cracks and
//! streaks (the rock face). After Gates' `render/ground_splat.rs` and
//! `shaders/ground_splat.wgsl` (MIT, NOTICE): its extension of
//! `StandardMaterial`, its texture arrays, its sizes, its cliff, wall and
//! rock knobs; without the island's roads, blight, snow and caustics.
//! The rain's wet and the cloud deck's shadows come with it
//! (`cloud_shadow.rs`), and the rock skin over the terrain's rock faces
//! (`cliffs.rs`) wears it too, its value off `UV_1`.
//!
//! Every number the shader reads arrives in one uniform block
//! ([`GroundParams`], binding 104), the deck's and the rain's
//! (`cloud_shadow::CloudShadowParams`) its last field. Under WebGL2 a
//! fragment shader may read only twelve blocks on many devices (ANGLE on
//! D3D11 and on Metal), and Bevy's view, mesh and `StandardMaterial` take
//! eleven of them, so this one is all the material adds. The block is
//! declared in `cloud_shadow.wgsl`, which both shaders read it through.

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub type GroundMaterial = ExtendedMaterial<StandardMaterial, Ground>;

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct Ground {
    #[texture(100, dimension = "2d_array")]
    #[sampler(101)]
    pub albedo: Handle<Image>,
    #[texture(102, dimension = "2d_array")]
    #[sampler(103)]
    pub normal: Handle<Image>,
    /// Every number the shader reads, in one block.
    #[uniform(104)]
    pub params: GroundParams,
}

/// The shader's numbers: `GroundParams` in `shaders/cloud_shadow.wgsl`,
/// which `shaders/ground.wgsl` reads it through.
///
/// ⚠ The field list and order must match the WGSL struct exactly. A
/// uniform whose two sides disagree about layout binds fine and is garbage
/// in every field after the first mismatch, and nothing about that failure
/// looks like a layout bug. Every field is a `Vec4` (std140 pads anything
/// smaller to one anyway) but the last, the deck's own struct; `tests`
/// below scrapes both sides.
#[derive(Clone, Debug, ShaderType)]
pub struct GroundParams {
    /// Metres each set repeats over, in splat order.
    pub tile_m: Vec4,
    /// What each set's photograph is multiplied by: the map's own soil (a
    /// red-rock canyon's sand is not a beach's).
    pub tint: [Vec4; 4],
    /// x/y = rise over run where the per-pixel rock starts / is full, z =
    /// how far the noise moves that, w = the noise's wavelength (m).
    pub cliff: Vec4,
    /// x = `sin(tilt)` where the wall planes turn on, y = the exponent the
    /// two plane weights are raised to, z = 1 on the light tier (no wall
    /// planes, no rock face: a software GPU), w = the floor under a normal
    /// map's z before it becomes a gradient.
    pub wall: Vec4,
    /// x = the share of each 45° step between two wall planes over which
    /// both are read, y/z = the cosine between the triangle's plane and the
    /// normal below which the projection follows the triangle / above which
    /// it follows the normal, w = 0.
    pub wall_planes: Vec4,
    /// x = block size (m), y = fine block size (m), z = block tilt, w =
    /// fine tilt.
    pub rock_a: Vec4,
    /// x = per-block value spread, y = weathering wavelength (m), z = its
    /// strength, w = how far the block lattice is warped (m).
    pub rock_b: Vec4,
    /// x = share of block boundaries that are cracks, y = crack half-width
    /// (cells), z = crack darkening, w = streak strength.
    pub rock_c: Vec4,
    /// x/y = streak lattice width/height (m), z/w = `sin(tilt)` where
    /// streaks start / are full.
    pub rock_d: Vec4,
    /// x = how far into a block its own draws hold (cells), y = the share
    /// of the lean a gentle slope keeps, z/w = `sin(tilt)` where the rest of
    /// it starts / is whole.
    pub rock_e: Vec4,
    /// x = the sea's height (far below the map when there is none), y =
    /// how far above it the ground is wet, z = over how much further it
    /// dries, w = how dark wet ground is against dry.
    pub sea: Vec4,
    /// The deck's shadows and the rain's wet (`cloud_shadow::drive` keeps
    /// it): in the uniform, not a texture or a buffer, so the ground binds
    /// what it did on every target.
    pub cloud: super::cloud_shadow::CloudShadowParams,
}

/// Dry Fork's soils over their photographs (`tools/ground.sh`, which
/// prints each one's mean): the sand a shade paler and less orange than it
/// was shot, the turf greener, the creek's stones as they are, and the
/// bedded sandstone lifted to a sunlit red (its photograph is in shade).
/// An island's (`level::Land::Isle`): the sand lifted to a pale cream (the
/// photograph is a warm orange, linear mean 0.29 0.16 0.08) and its rock
/// to a tan sandstone (0.21 0.09 0.03, a canyon's red); the other two
/// lanes are never weighted there.
pub const ISLE_TINT: [Vec4; 4] = [
    Vec4::new(1.9, 2.6, 3.5, 1.0),
    Vec4::new(0.66, 1.2, 0.62, 1.0),
    Vec4::new(0.95, 0.92, 0.88, 1.0),
    Vec4::new(2.3, 3.2, 4.4, 1.0),
];

/// An island's rock face: sandstone the wind has rounded, not a canyon's
/// split wall. Blocks twice as big and leaning half as far (the lanes of
/// `rock_a`: cell, fine cell, tilt, fine tilt), less value between them and
/// broad soft weathering over a wandering lattice (`rock_b`: shade,
/// weathering's wavelength and depth, warp), few cracks, faint, and faint
/// streaks (`rock_c`: crack share, width, darkness, streaks). Against the
/// canyon's [`ROCK_CELL_M`] and the rest below.
pub const ISLE_ROCK_A: Vec4 = Vec4::new(14.0, 4.5, 0.28, 0.1);
pub const ISLE_ROCK_B: Vec4 = Vec4::new(0.05, 20.0, 0.12, 2.5);
pub const ISLE_ROCK_C: Vec4 = Vec4::new(0.06, 0.03, 0.15, 0.05);

pub const TINT: [Vec4; 4] = [
    Vec4::new(1.0, 1.1, 1.25, 1.0),
    Vec4::new(0.66, 1.2, 0.62, 1.0),
    Vec4::new(0.95, 0.92, 0.88, 1.0),
    Vec4::new(1.45, 1.6, 2.1, 1.0),
];

// --- The cliff, per pixel ---------------------------------------------------
//
// The mesh's weights decide the ground below these slopes (the scree, to a
// rise over run of about 0.7, keeps what level.rs wrote); above them the
// pixel is rock whatever its vertices said, from the tilt of the
// interpolated normal. Decided per vertex alone, a near-binary weight
// interpolated across 1–2 m triangles draws the triangles: a sand/rock edge
// of regular teeth one cell wide along the cliff's foot. A world-space noise
// moves the threshold so the edge wanders like scree instead.

/// Rise over run where the per-pixel rock starts (42°) and where the pixel
/// is all rock (51°). Gates' band (0.95–1.40) matched its sim's cliff; this
/// one starts where Dry Fork's compiled wall leaves its scree.
pub const CLIFF_TAN_LO: f32 = 0.9;
pub const CLIFF_TAN_HI: f32 = 1.25;
/// How far the noise moves that threshold, in rise over run, peak to peak:
/// half of it reaches below `CLIFF_TAN_LO`, so a slope of 0.7 is as low as
/// the edge wanders.
pub const CLIFF_NOISE: f32 = 0.4;
/// The noise's wavelength, metres; a second octave rides at 0.37 of it.
pub const CLIFF_NOISE_M: f32 = 6.0;

// --- The wall planes ----------------------------------------------------------
//
// The ground's UV is a planar XZ projection, so on a face of tilt θ the
// photograph is stretched by 1/cos θ down the fall line: 2.9× at 70°,
// unbounded at vertical. Above `WALL_ON` every set is instead read off
// vertical planes at four fixed bearings 45° apart, stretched 1/sin θ down
// the face and at most 1.08× across it, the two nearest blended. The planes
// are FIXED world axes, never a frame built from the face: a frame that
// turns with the surface, times a world coordinate, sweeps metres of
// photograph past every metre of cliff wherever the bearing changes, and
// draws fine lines along every contour (Gates, 2026-10-02).

/// Where the wall planes turn on, as `sin(tilt)`: 30°. The planar and the
/// wall stretch are exact complements that cross at 45°; gated here, where
/// the sharpened wall share is ~0.02%, so the taps never switch on at a
/// visible share along one contour.
pub const WALL_ON: f32 = 0.5;
/// The exponent the two plane weights are raised to. At 16 they share only
/// ~42°–48°; at 8 every hillside between 35° and 55° was cross-hatched by
/// two copies of one photograph at two stretches.
pub const WALL_SHARPNESS: f32 = 16.0;
/// The share of each 45° step between two wall planes over which a face
/// reads both: the middle 15°. Elsewhere one plane, within 15° of the
/// face's bearing, so two thirds of a wall pays for one plane's taps.
pub const WALL_PLANE_BLEND: f32 = 1.0 / 3.0;
/// The cosine between a triangle's own plane and the smoothed normal past
/// which the projection is chosen by the triangle (~20°), and within which
/// by the normal (~10°). Where the relief cuts a step one vertex wide the
/// smoothed normal spreads it over two, and the riser would read the top
/// plane, smeared down the fall line; inside 10° the normal still decides,
/// because it is continuous across a triangle edge and the plane is not.
pub const PROJECT_TRI_COS: f32 = 0.94;
pub const PROJECT_NORMAL_COS: f32 = 0.985;
/// The floor under a normal map's z before it becomes a gradient. The sets
/// are JPEGs whose blue channel dips at block edges, and `n.xy / 1e-4`
/// returns a gradient in the thousands; 0.2 caps one texel's slope at 5:1.
pub const NORMAL_Z_FLOOR: f32 = 0.2;

// --- The rock face --------------------------------------------------------------
//
// A smooth heightfield lights a scarp as one value, so the rock carries its
// own structure: Voronoi blocks at two scales that each catch the sun at
// their own angle, a few long cracks, weathering, and streaks down the
// faces. Nothing here moves a weight; this decides what rock looks like.
// Everything fades with the block's size on screen (the fragment's own
// footprint), so a distant face does not sparkle.

// Gates' numbers were tuned on grey granite under its own sky; Dry Fork's
// red sandstone walls are seen from 20–40 m under a hard sun, where Gates'
// soft blocks read as stains on a dune. These lean further, differ more in
// value, turn over at a narrower seam and weather less (2026-11-04, on a
// 40 m test ridge, lit and backlit).

/// Edge of one rock block, metres: a 34 m wall shows a handful.
pub const ROCK_CELL_M: f32 = 7.0;
/// Edge of one fine block inside the large ones, metres.
pub const ROCK_FINE_M: f32 = 2.2;
/// How far a block's facet leans, as a tangent-plane offset per unit
/// normal: each component of the draw is in ±1, so the lean tops out near
/// 40°. And the fine blocks', riding on it.
pub const ROCK_TILT: f32 = 0.6;
pub const ROCK_FINE_TILT: f32 = 0.25;
/// Peak per-block departure of the face's value from its mean.
pub const ROCK_SHADE: f32 = 0.09;
/// Wavelength of the weathering patches, metres, and their peak departure.
pub const ROCK_WEATHER_M: f32 = 14.0;
pub const ROCK_WEATHER: f32 = 0.08;
/// How far the block lattice is pushed about before it is read, metres, so
/// a boundary wanders like a joint and is never a ruled polygon edge.
pub const ROCK_WARP_M: f32 = 1.0;
/// Share of block boundaries that are cracks.
pub const ROCK_CRACK_SHARE: f32 = 0.22;
/// A crack's half-width in block units, and how dark its centre goes. At
/// most 0.125: a crack shows from half a pixel wide, so it is gone by a
/// block footprint of `2 × ROCK_CRACK_W` pixels, inside the 0.25 where the
/// shader skips the block pass.
pub const ROCK_CRACK_W: f32 = 0.03;
const _: () = assert!(ROCK_CRACK_W <= 0.125);
pub const ROCK_CRACK_DARK: f32 = 0.3;
/// Peak departure of the streaks: darker in a streak, lighter between.
pub const ROCK_STREAK: f32 = 0.12;
/// The streak lattice's cell, metres: narrow across a face and tall down it.
pub const ROCK_STREAK_W_M: f32 = 1.6;
pub const ROCK_STREAK_H_M: f32 = 14.0;
/// `sin(tilt)` where streaks begin (~37°) and are full (~58°): water runs
/// down a face, it does not stripe a slope a fighter walks up.
pub const ROCK_FACE_ON: f32 = 0.6;
pub const ROCK_FACE_FULL: f32 = 0.85;
/// How far into a block its own lean and value hold, in block units.
/// Nearer a boundary they turn over to the neighbour's, and on it the two
/// are even, so a block has no outline: stepped at the boundary, every face
/// read as polygons pasted on; at Gates' 0.25 the facets were soft blots.
pub const ROCK_SEAM: f32 = 0.1;
/// The share of the facets' lean a gentle slope keeps (a scarp breaks into
/// planes; a hill is not a field of slabs tipped at random), and
/// `sin(tilt)` where the rest grows in (~20°) and is whole (~44°).
pub const ROCK_FLAT_LEAN: f32 = 0.35;
pub const ROCK_LEAN_ON: f32 = 0.35;
pub const ROCK_LEAN_FULL: f32 = 0.7;

/// Where there is no sea: a height no ground reaches, so the wet band
/// never shows (never `NEG_INFINITY`: the shader subtracts it).
pub const NO_SEA: f32 = -1.0e5;
/// The swash: ground is wet from the water to this far above it, and dry
/// again `WET_FADE_M` higher; wet ground is darker by `WET_DARK`.
pub const WET_M: f32 = 0.5;
pub const WET_FADE_M: f32 = 0.6;
pub const WET_DARK: f32 = 0.42;

impl Default for GroundParams {
    /// Dry Fork's: its soils over the sets' sizes, and the numbers above.
    fn default() -> Self {
        Self {
            tile_m: Vec4::from_array(super::textures::GROUND_TILE_M),
            tint: TINT,
            cliff: Vec4::new(CLIFF_TAN_LO, CLIFF_TAN_HI, CLIFF_NOISE, CLIFF_NOISE_M),
            wall: Vec4::new(WALL_ON, WALL_SHARPNESS, 0.0, NORMAL_Z_FLOOR),
            wall_planes: Vec4::new(WALL_PLANE_BLEND, PROJECT_TRI_COS, PROJECT_NORMAL_COS, 0.0),
            rock_a: Vec4::new(ROCK_CELL_M, ROCK_FINE_M, ROCK_TILT, ROCK_FINE_TILT),
            rock_b: Vec4::new(ROCK_SHADE, ROCK_WEATHER_M, ROCK_WEATHER, ROCK_WARP_M),
            rock_c: Vec4::new(ROCK_CRACK_SHARE, ROCK_CRACK_W, ROCK_CRACK_DARK, ROCK_STREAK),
            rock_d: Vec4::new(ROCK_STREAK_W_M, ROCK_STREAK_H_M, ROCK_FACE_ON, ROCK_FACE_FULL),
            rock_e: Vec4::new(ROCK_SEAM, ROCK_FLAT_LEAN, ROCK_LEAN_ON, ROCK_LEAN_FULL),
            sea: Vec4::new(NO_SEA, WET_M, WET_FADE_M, WET_DARK),
            cloud: default(),
        }
    }
}

impl MaterialExtension for Ground {
    fn fragment_shader() -> ShaderRef {
        "shaders/ground.wgsl".into()
    }
}

/// The ground's material over its photographs, wet up from the sea at
/// `sea_y` (the map's `light.sea`; not finite means no sea); `lite` is the
/// lowest rung, which draws no wall planes or rock face; `isle` an island's
/// ground, in its own tints; `cloud` the deck's shadows and the rain's wet
/// as the map's mood has them.
pub fn material(
    albedo: Handle<Image>,
    normal: Handle<Image>,
    sea_y: f32,
    lite: bool,
    isle: bool,
    cloud: super::cloud_shadow::CloudShadowParams,
) -> GroundMaterial {
    let mut params = GroundParams::default();
    if isle {
        params.tint = ISLE_TINT;
        params.rock_a = ISLE_ROCK_A;
        params.rock_b = ISLE_ROCK_B;
        params.rock_c = ISLE_ROCK_C;
    }
    params.sea.x = if sea_y.is_finite() { sea_y } else { NO_SEA };
    params.wall.z = if lite { 1.0 } else { 0.0 };
    params.cloud = cloud;
    ExtendedMaterial {
        base: StandardMaterial {
            perceptual_roughness: 0.92,
            reflectance: super::fresnel::DIELECTRIC,
            ..default()
        },
        extension: Ground { albedo, normal, params },
    }
}

/// Two failures that compile, bind and draw, and show only as a garbled
/// ground or a smear on a cliff (Gates shipped both): the uniform's two
/// sides disagreeing about their fields, and a derivative or an implicit
/// sample under the wall branch. Scraped from the sources, as Gates'
/// `tests/ground_splat.rs` does.
#[cfg(test)]
mod tests {
    const WGSL: &str = include_str!("../../assets/shaders/ground.wgsl");
    /// Where the block is declared, beside the deck's struct it holds.
    const BLOCK: &str = include_str!("../../assets/shaders/cloud_shadow.wgsl");
    const RUST: &str = include_str!("ground.rs");

    /// The field names of the struct that `header` opens, in order.
    fn fields(src: &str, header: &str, strip: &str) -> Vec<String> {
        let body = src
            .split_once(header)
            .unwrap_or_else(|| panic!("no `{header}` in the source"))
            .1;
        let body = body.split_once("\n}").expect("unterminated struct").0;
        body.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("//"))
            .filter_map(|l| l.trim_start_matches(strip).trim().split(':').next())
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect()
    }

    #[test]
    fn the_uniform_declares_the_same_fields_on_both_sides() {
        let shader_fields = fields(BLOCK, "struct GroundParams {", "");
        let rust_fields = fields(RUST, "pub struct GroundParams {", "pub ");
        assert_eq!(
            shader_fields, rust_fields,
            "the ground uniform's two sides disagree about their fields: the shader is \
             reading bytes the CPU did not write there"
        );
        assert!(
            shader_fields.len() >= 5,
            "only {} fields scraped: the parse broke",
            shader_fields.len()
        );
        // One block the material adds, and only one (the module's doc).
        let both = format!("{WGSL}{BLOCK}");
        assert_eq!(both.matches("var<uniform>").count(), 1);
        assert!(BLOCK.contains("@binding(104) var<uniform> ground: GroundParams;"));
    }

    #[test]
    fn the_derivatives_are_taken_before_any_branch_and_the_wall_samples_by_gradient() {
        let code: String = WGSL
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let frag = code.split_once("fn fragment(").expect("no fragment entry point").1;

        // Every derivative in the fragment sits at the function body's own
        // depth: undefined under non-uniform control flow, and naga does not
        // reject one there.
        let mut depth = 0i32;
        let mut deriv_lines = 0;
        for line in frag.lines() {
            if line.contains("dpdx(") || line.contains("dpdy(") || line.contains("fwidth(") {
                deriv_lines += 1;
                assert_eq!(
                    depth,
                    1,
                    "a derivative under a branch in the fragment: `{}`",
                    line.trim()
                );
            }
            depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
        }
        assert_eq!(
            deriv_lines, 2,
            "the scrape found {deriv_lines} derivative lines, not the two"
        );

        // The wall branch samples nothing itself: `wall_tap` does, once per
        // set with weight, through `wall_plane`, with explicit gradients.
        let block = frag
            .split_once("if wall_mix > 0.0 {")
            .expect("no wall branch")
            .1
            .split_once("\n    }")
            .expect("unterminated wall branch")
            .0;
        assert!(
            !block.contains("textureSample"),
            "the wall branch samples a texture itself"
        );
        assert_eq!(
            block.matches("wall_tap(").count(),
            4,
            "the wall branch taps once per set"
        );
        let body = |name: &str| -> &str {
            code.split_once(&format!("fn {name}("))
                .unwrap_or_else(|| panic!("no `fn {name}`"))
                .1
                .split_once("\n}")
                .unwrap_or_else(|| panic!("unterminated `fn {name}`"))
                .0
        };
        let tap = body("wall_tap");
        assert!(!tap.contains("textureSample") && tap.matches("wall_plane(").count() == 2);
        let plane = body("wall_plane");
        assert_eq!(
            plane.matches("textureSampleGrad(").count(),
            2,
            "albedo and normal, by gradient"
        );
        assert!(
            !plane.contains("textureSample("),
            "an implicit sample under the wall branch"
        );
        // Each plane's axis is a constant of an integer plane index, and
        // its UV reads nothing per fragment but the world position: linear
        // in it but for the beds' dip, a smooth slide of the photograph
        // (`BED_DIP_M` over `BED_DIP_WAVE_M`, a slope under a fifth) that
        // the gradients below leave out, so the mip is the linear UV's.
        let axis = body("wall_axis");
        assert!(axis.contains("array<vec2<f32>, 4>") && !axis.contains("normal"));
        assert!(block.contains("let dip = (value3(wp / BED_DIP_WAVE_M) - 0.5) * BED_DIP_M;"));
        for side in ["a", "b"] {
            assert!(block.contains(&format!("wall.axis_{side} = wall_axis(k{side});")));
            assert!(block.contains(&format!(
                "wall.uv_{side} = vec2<f32>(dot(wp.xz, wall.axis_{side}), -wp.y - dip);"
            )));
            assert!(block.contains(&format!(
                "wall.dx_{side} = vec2<f32>(dot(dp_dx.xz, wall.axis_{side}), -dp_dx.y);"
            )));
            assert!(block.contains(&format!(
                "wall.dy_{side} = vec2<f32>(dot(dp_dy.xz, wall.axis_{side}), -dp_dy.y);"
            )));
        }
    }
}

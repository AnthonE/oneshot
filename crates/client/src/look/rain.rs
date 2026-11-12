//! Rain: streaks that follow the eye, on a mood that has a shower
//! (`mood::Mood::rain`), and the wet it leaves (`Mood::wet`: the ground's
//! shader, `cloud_shadow.wgsl`, and the boxes' materials, `level.rs`).
//! After Gates' `render/rain.rs` (MIT, NOTICE), without its snow.
//!
//! **One mesh, one draw, and no particles**: a fixed mesh of streak quads
//! whose vertex stage wraps each drop into a box around the camera by how
//! far the rain has fallen. How hard it rains is how many drops are alive,
//! so a drizzle and a storm are the same mesh with one number moved. The
//! fall runs on `globals.time` in the shader, so the material is written
//! once a map.
//!
//! **The same rain for everyone.** The streaks are something a fighter is
//! seen through, so every target and every rung draws the same count of
//! them, and a light shower's streaks are a few percent of the frame.

use bevy::asset::Asset;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;

use super::mood::{Mood, MoodNow};

pub const SHADER: &str = "shaders/rain.wgsl";

/// Drops in the mesh: the most a shower draws, on every target; and in the
/// far snow's, whose box is a hundred times the near one's.
pub const DROPS: u32 = 4_000;
pub const FAR_DROPS: u32 = 14_000;
/// Half the box the drops fall in around the eye, metres.
pub const BOX_HALF: Vec3 = Vec3::new(14.0, 9.0, 14.0);
/// How fast a drop falls, m/s: a raindrop's terminal speed.
pub const FALL_MS: f32 = 9.0;
/// The rain's glint with no sun: the light a streak gives back.
pub const GLINT: f32 = 0.12;
/// Each drop falls at its own whole number of `1 / SPEED_STEPS` of the
/// travel, so the travel wrapped at `SPEED_STEPS` box lengths moves every
/// drop by whole boxes (`rain.wgsl`).
pub const SPEED_STEPS: f32 = 20.0;
/// Under a roof the drops within this of the eye, across, are put out
/// (Gates' `SHELTER_CLEAR_M`): rain doesn't fall through Overpass's bridge
/// onto whoever stands under it.
pub const SHELTER_CLEAR_M: f32 = 5.0;
/// How far up something over the eye counts as a roof, metres.
pub const SHELTER_UP_M: f32 = 12.0;
/// How finely a roof's extent is found from under it, and how far out, m.
const ROOF_STEP_M: f32 = 2.5;
const ROOF_REACH_M: f32 = 60.0;

/// Snow (`Mood::snow`): how fast it comes down, m/s, how far the wind takes it, a
/// flake's length and width, m, how opaque at its middle, and how bright it's lit.
pub const SNOW_FALL_MS: f32 = 1.6;
pub const SNOW_DRIFT: f32 = 1.4;
pub const SNOW_STROKE_M: f32 = 0.04;
pub const SNOW_WIDTH_M: f32 = 0.034;
pub const SNOW_ALPHA: f32 = 0.75;
pub const SNOW_LIT: f32 = 0.72;
/// How many flakes a mood's `rain` lights, over a shower's drops: snow
/// fogs less than rain for what falls (`Mood::rain_sigma` reads `rain`), so
/// it falls thicker for the same air; and the box it falls in, nearer the
/// eye, where a flake is big enough to see.
pub const SNOW_COUNT: f32 = 2.5;
pub const SNOW_BOX_HALF: Vec3 = Vec3::new(10.0, 7.0, 10.0);
/// Under a roof, the snow the wind drives in under it stops this near.
pub const SNOW_SHELTER_CLEAR_M: f32 = 1.5;
/// The far snow: a second mesh of the same drops in a box out to the far
/// side of an arena, its flakes big enough to be a few pixels across there
/// and faint, so the whole frame has snow in it, not only the air round the
/// eye; under a roof none within this far.
pub const FAR_SNOW_BOX_HALF: Vec3 = Vec3::new(55.0, 22.0, 55.0);
pub const FAR_SNOW_WIDTH_M: f32 = 0.15;
pub const FAR_SNOW_ALPHA: f32 = 0.32;
pub const FAR_SNOW_CLEAR_M: f32 = 6.0;
/// A streak of rain: how wide, how opaque at its middle.
pub const RAIN_WIDTH_M: f32 = 0.012;
pub const RAIN_ALPHA: f32 = 0.4;
/// The uniform, laid out to match `Rain` in the shader. 80 bytes: WebGL2
/// wants uniforms of at least 16.
#[derive(Clone, Copy, Default, ShaderType, Debug, PartialEq)]
pub struct RainParams {
    /// xyz the fall velocity (m/s), w the streak length (m).
    pub fall: Vec4,
    /// xyz the box's half extents (m), w the intensity `0..1`.
    pub box_: Vec4,
    /// rgb the streak's lit colour; w the radius round the eye that is
    /// clear under a roof ([`SHELTER_CLEAR_M`]), 0 in the open.
    pub look: Vec4,
    /// x a streak's width (m), y its opacity at its middle, z 1 for a round
    /// flake (snow), 0 for a streak; w the shelter's clear radius when under one.
    pub shape: Vec4,
    /// The roof the eye stands under, as a box in the plan (x0, z0, x1, z1):
    /// nothing falls inside it. Empty (all zero) in the open. A hall is
    /// wider than any radius round the eye: snow fell in a hangar's far end.
    pub roof: Vec4,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct RainMaterial {
    #[uniform(0)]
    pub params: RainParams,
}

impl Material for RainMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // A streak is a camera-facing quad either way round.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// The rain's entity, and the far snow's.
#[derive(Component)]
pub struct Rain;

/// The handles `drive` writes: the rain (or the near snow), the far snow.
#[derive(Resource)]
pub struct RainHandle(pub Handle<RainMaterial>, pub Handle<RainMaterial>);

/// The far snow of a mood that snows: the near snow's fall, in its own box.
pub fn far_for(m: &Mood, roofed: bool) -> RainParams {
    let near = params_for(m, roofed);
    if !m.snow {
        return RainParams::default();
    }
    RainParams {
        fall: near.fall.truncate().extend(FAR_SNOW_WIDTH_M),
        box_: FAR_SNOW_BOX_HALF.extend(near.box_.w),
        look: near.look,
        shape: Vec4::new(FAR_SNOW_WIDTH_M, FAR_SNOW_ALPHA, 1.0, FAR_SNOW_CLEAR_M),
        roof: Vec4::ZERO,
    }
}

/// Deterministic per-drop randomness — a look, not a sim draw.
fn hash01(i: u32, k: u32) -> f32 {
    let mut h = (i as u64) << 32 | k as u64;
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^= h >> 33;
    (h >> 40) as f32 / 16_777_216.0
}

/// The drop mesh: `drops` quads, each a home in the unit box (POSITION),
/// its corners (UV_0) and its rank and phase (COLOR).
pub fn drop_mesh(drops: u32) -> Mesh {
    let n = drops as usize;
    let mut pos = Vec::with_capacity(n * 4);
    let mut uv = Vec::with_capacity(n * 4);
    let mut col = Vec::with_capacity(n * 4);
    let mut idx: Vec<u16> = Vec::with_capacity(n * 6);
    for i in 0..drops {
        let home = [hash01(i, 0), hash01(i, 1), hash01(i, 2)];
        // Rank is the drop's index spread evenly, so the alive share tracks
        // the intensity exactly rather than by luck.
        let rank = (i as f32 + 0.5) / drops as f32;
        let phase = hash01(i, 3);
        let base = (i * 4) as u16;
        for (x, y) in [(-1.0, 0.0), (1.0, 0.0), (1.0, 1.0), (-1.0, 1.0)] {
            pos.push(home);
            uv.push([x, y]);
            col.push([rank, phase, 0.0, 1.0]);
        }
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U16(idx));
    mesh
}

const _: () = assert!(DROPS * 4 <= u16::MAX as u32 + 1, "a drop's corners are u16 indices");
const _: () = assert!(FAR_DROPS * 4 <= u16::MAX as u32 + 1, "a drop's corners are u16 indices");

/// What a mood's rain looks like. None under a roof (an interior's mood
/// never rains).
pub fn params_for(m: &Mood, roofed: bool) -> RainParams {
    if m.snow {
        // The same mesh, slow, each flake a short bright stroke.
        return RainParams {
            fall: (m.wind().extend(0.0).xzy() * SNOW_DRIFT - Vec3::Y * SNOW_FALL_MS).extend(SNOW_STROKE_M),
            box_: SNOW_BOX_HALF.extend(if roofed { 0.0 } else { (m.rain * SNOW_COUNT).min(1.0) }),
            look: Vec4::new(SNOW_LIT, SNOW_LIT, SNOW_LIT * 1.04, 0.0),
            shape: Vec4::new(SNOW_WIDTH_M, SNOW_ALPHA, 1.0, SNOW_SHELTER_CLEAR_M),
            roof: Vec4::ZERO,
        };
    }
    let wind = m.wind() * (1.0 + 0.6 * m.wind_ms);
    let fall = Vec3::new(wind.x, -(FALL_MS + 2.0 * m.rain), wind.y);
    // Lit by the sky it falls from: bright grey by day, under a heavy one
    // greyer, opened with the exposure as the frame is.
    let lit = ((GLINT + 0.6 * m.sun * (1.0 - 0.45 * m.dark)) * m.exposure_gain()).min(0.8);
    RainParams {
        fall: fall.extend(0.45 + 0.3 * m.rain),
        box_: BOX_HALF.extend(if roofed { 0.0 } else { m.rain }),
        look: Vec4::new(0.55 * lit, 0.58 * lit, 0.62 * lit, 0.0),
        shape: Vec4::new(RAIN_WIDTH_M, RAIN_ALPHA, 0.0, SHELTER_CLEAR_M),
        roof: Vec4::ZERO,
    }
}

/// Build the rain, hidden until a mood rains.
pub fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<RainMaterial>>) {
    let [near, far] = [(); 2].map(|_| {
        materials.add(RainMaterial {
            params: RainParams::default(),
        })
    });
    for (m, drops) in [(&near, DROPS), (&far, FAR_DROPS)] {
        commands.spawn((
            Rain,
            Mesh3d(meshes.add(drop_mesh(drops))),
            MeshMaterial3d(m.clone()),
            Transform::default(),
            Visibility::Hidden,
            NoFrustumCulling,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
    commands.insert_resource(RainHandle(near, far));
}

/// The map's rain, when the map changes, and its shelter when the eye goes
/// under a roof or out from one (the map's own boxes, as the sim has them).
#[allow(clippy::too_many_arguments)]
pub fn drive(
    now: Res<MoodNow>,
    handle: Option<Res<RainHandle>>,
    mut materials: ResMut<Assets<RainMaterial>>,
    mut rain: Query<(&mut Visibility, &MeshMaterial3d<RainMaterial>), With<Rain>>,
    link: NonSend<crate::LinkRes>,
    eye: Query<&GlobalTransform, With<super::rig::EyeCam>>,
    mut sheltered: Local<bool>,
    mut roof: Local<Option<(Vec2, Vec2)>>,
) {
    let mut want = params_for(&now.mood, now.roofed);
    let mut far = far_for(&now.mood, now.roofed);
    let show = want.box_.w > 0.0;
    let map = &link.0.session().core.map;
    let at = eye.single().ok().map(|g| g.translation());
    let under = show && at.is_some_and(|p| covered(map, p.x, p.y, p.z));
    // The roof it stands under, found again when the eye leaves the last one's box.
    let mut moved = false;
    match (under, at) {
        (true, Some(p)) => {
            let inside = roof.is_some_and(|(lo, hi)| p.x > lo.x && p.x < hi.x && p.z > lo.y && p.z < hi.y);
            if !inside {
                *roof = Some(roof_over(map, p));
                moved = true;
            }
        }
        _ => {
            moved = roof.take().is_some();
        }
    }
    if !now.is_changed() && under == *sheltered && !moved {
        return;
    }
    *sheltered = under;
    want.look.w = if under { want.shape.w } else { 0.0 };
    far.look.w = if under { far.shape.w } else { 0.0 };
    let r = roof.map_or(Vec4::ZERO, |(lo, hi)| Vec4::new(lo.x, lo.y, hi.x, hi.y));
    want.roof = r;
    far.roof = r;
    let Some(handle) = handle else {
        return;
    };
    for (mut v, mat) in rain.iter_mut() {
        let on = if mat.0 == handle.1 {
            show && far.box_.w > 0.0
        } else {
            show
        };
        let vis = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *v != vis {
            *v = vis;
        }
    }
    for (h, p) in [(&handle.0, want), (&handle.1, far)] {
        if materials.get(h).is_some_and(|m| m.params != p) {
            if let Some(m) = materials.get_mut(h) {
                m.params = p;
            }
        }
    }
}

/// Whether (x, z) at height `y` is under cover: something built over it
/// within [`SHELTER_UP_M`].
fn covered(map: &sim::map::Map, x: f32, y: f32, z: f32) -> bool {
    map.ray(
        sim::geom::V3::new(x, y, z),
        sim::geom::V3::new(0.0, 1.0, 0.0),
        SHELTER_UP_M,
    ) < SHELTER_UP_M
}

/// The roof over `p`, as a box in the plan: round every place joined to
/// `p` under cover at its height, by steps of [`ROOF_STEP_M`] out to
/// [`ROOF_REACH_M`], never through a wall or into the ground (a door's far
/// side is open sky, and stops it).
fn roof_over(map: &sim::map::Map, p: Vec3) -> (Vec2, Vec2) {
    let n = (ROOF_REACH_M / ROOF_STEP_M) as i32;
    let side = (2 * n + 1) as usize;
    let mut seen = vec![false; side * side];
    seen[n as usize * side + n as usize] = true;
    let mut queue = vec![(0i32, 0i32)];
    let (mut lo, mut hi) = (Vec2::new(p.x, p.z), Vec2::new(p.x, p.z));
    while let Some((i, j)) = queue.pop() {
        let here = Vec2::new(p.x + i as f32 * ROOF_STEP_M, p.z + j as f32 * ROOF_STEP_M);
        lo = lo.min(here);
        hi = hi.max(here);
        for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (a, b) = (i + di, j + dj);
            if a.abs() > n || b.abs() > n {
                continue;
            }
            let k = (b + n) as usize * side + (a + n) as usize;
            if seen[k] {
                continue;
            }
            seen[k] = true;
            let (x, z) = (p.x + a as f32 * ROOF_STEP_M, p.z + b as f32 * ROOF_STEP_M);
            let open = map.ground(x, z) < p.y && !map.occupied(x, z, 0.1, p.y - 0.1, p.y + 0.1);
            if open && covered(map, x, p.y, z) {
                queue.push((a, b));
            }
        }
    }
    let m = Vec2::splat(ROOF_STEP_M * 0.5);
    (lo - m, hi + m)
}

/// What the rain's wet does to a box's material: darker, and more
/// saturated about its own luma (Gates' `wetted`, the ground's own), and
/// smoother. `wet` is the mood's.
pub fn wetted(c: Color, roughness: f32, wet: f32) -> (Color, f32) {
    if wet <= 0.0 {
        return (c, roughness);
    }
    let l = LinearRgba::from(c);
    let rgb = [l.red, l.green, l.blue];
    let luma = super::fill::luminance(rgb);
    let value = if luma > WET_FLOOR {
        (1.0 - wet * (1.0 - WET_VALUE)).max(WET_FLOOR / luma)
    } else {
        1.0
    };
    let chroma = 1.0 + wet * WET_SATURATION;
    let out: [f32; 3] = core::array::from_fn(|i| ((luma + (rgb[i] - luma) * chroma) * value).max(0.0));
    (
        Color::linear_rgba(out[0], out[1], out[2], l.alpha),
        roughness * (1.0 - wet * (1.0 - WET_ROUGH)),
    )
}

/// How much of a colour's value a full soak keeps (Gates' `WET_VALUE`).
pub const WET_VALUE: f32 = 0.55;
/// How much its chroma is stretched about its own luma.
pub const WET_SATURATION: f32 = 0.35;
/// The darkest a soak may take a colour to, luma.
pub const WET_FLOOR: f32 = 0.05;
/// How much of its roughness a soaked surface keeps.
pub const WET_ROUGH: f32 = 0.75;

#[cfg(test)]
mod tests {
    use super::*;

    /// Wet is darker and glossier, never black, and dry is nothing.
    #[test]
    fn wet_darkens_and_smooths_but_never_blackens() {
        let c = Color::linear_rgb(0.4, 0.38, 0.35);
        assert_eq!(wetted(c, 0.9, 0.0), (c, 0.9));
        let (w, r) = wetted(c, 0.9, 1.0);
        let l = |c: Color| {
            let l = LinearRgba::from(c);
            super::super::fill::luminance([l.red, l.green, l.blue])
        };
        assert!(l(w) < l(c) && l(w) >= WET_FLOOR - 1e-6 && r < 0.9);
        let dark = Color::linear_rgb(0.03, 0.03, 0.03);
        assert!((l(wetted(dark, 0.5, 1.0).0) - l(dark)).abs() < 1e-3);
    }

    /// A light shower: under half the drops, and none indoors.
    #[test]
    fn a_shower_is_light_and_stays_outside() {
        let p = params_for(&super::super::mood::SUN_SHOWER, false);
        assert!(p.box_.w > 0.0 && p.box_.w < 0.5);
        assert_eq!(params_for(&super::super::mood::SUN_SHOWER, true).box_.w, 0.0);
        assert_eq!(params_for(&super::super::mood::NOON, false).box_.w, 0.0);
    }
}

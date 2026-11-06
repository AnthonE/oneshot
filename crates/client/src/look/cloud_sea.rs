//! The sea of cloud under an island over a void (a map with a `void`,
//! `sim::map::Map::void`): where a canyon has water at its sea level, the
//! island has cloud, billowing and drifting, lit from above, faded by the
//! air with the distance (`cloud_sea.wgsl`).
//!
//! Drawing only, like the sea: the sim's void is far above where this is
//! drawn, so whoever falls is dead before they reach it, and nothing in it
//! hides or shows a fighter. One sheet of rings round the map's middle,
//! flat out to [`FLAT_M`], then curling up into a bank of cloud that stands
//! over the horizon inside the far plane (`rig::FAR_M`): a flat sheet cut
//! off by the far plane leaves a band under the horizon where the
//! atmosphere draws its warm ground, a hard line round the world. The bank
//! goes to the weather fog's colour as it climbs ([`FADE_M`] to
//! [`REACH_M`], by [`FAR_SHARE`] of the way: the colour the sky's own haze
//! band is, `rig::fog_for`), so its rim is lost against the horizon. The
//! shader draws the billows per pixel, flattening them where a pixel can't
//! hold them, so it is the same sheet on every rung and on WebGL2.

use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

const SHADER: &str = "shaders/cloud_sea.wgsl";

/// The sheet's rings, metres from the map's middle: flat to [`FLAT_M`],
/// then the bank, its rim at [`RIM_Y`] by [`REACH_M`], under the far plane
/// however the eye turns.
const RINGS_M: [f32; 14] = [
    0.0, 15.0, 30.0, 50.0, 75.0, 110.0, 150.0, 200.0, 260.0, 330.0, 400.0, 460.0, 515.0, 560.0,
];
pub const FLAT_M: f32 = 400.0;
pub const REACH_M: f32 = 560.0;
/// From here out the sheet goes to the fog's colour, [`FAR_SHARE`] of the
/// way by [`REACH_M`].
pub const FADE_M: f32 = 300.0;
pub const FAR_SHARE: f32 = 0.9;
/// The bank's rim: over the eye of anyone on the island (a strut's tip is
/// 9 m up), so from there it stands just over the horizon.
pub const RIM_Y: f32 = 20.0;
const SPOKES: usize = 96;
/// A billow's top and the hollow between, linear: a grey white under the
/// lit sand's value, and a cool blue grey deep in the folds, so a sea of
/// them reads as rolling masses and not a snowfield.
pub const TOP: [f32; 3] = [0.62, 0.63, 0.66];
pub const HOLLOW: [f32; 3] = [0.27, 0.3, 0.38];
/// The big billows' size, metres, and how steeply they stand.
pub const BILLOW_M: f32 = 100.0;
pub const RELIEF: f32 = 6.0;
/// How fast it drifts, m/s, along x and z: slower than the sky's deck.
pub const DRIFT: [f32; 2] = [0.9, 0.35];
/// The drift wraps at this, metres (a whole number of the biggest
/// billows, so the wrap is never seen; small, so f32 keeps its bits).
const WRAP_M: f32 = BILLOW_M * 64.0;

/// The uniform; each lane is named in `cloud_sea.wgsl`.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct SeaParams {
    pub top: Vec4,
    pub hollow: Vec4,
    pub shape: Vec4,
    pub fade: Vec4,
    pub far: Vec4,
}

/// The extension. Not bindless, as the level's isn't.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct Sea {
    #[uniform(100)]
    pub params: SeaParams,
    /// Its drift (`sea_motion.rs`), so the material is written only when
    /// the fog's colour changes.
    #[texture(101, sample_type = "float", filterable = false)]
    pub motion: Handle<Image>,
}

pub type CloudSeaMaterial = ExtendedMaterial<StandardMaterial, Sea>;

impl MaterialExtension for Sea {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    /// Forward only: this client never runs deferred.
    fn deferred_fragment_shader() -> ShaderRef {
        ShaderRef::Default
    }
}

/// The sheet, its middle and how far it has drifted.
#[derive(Component, Default)]
pub struct CloudSea {
    mid: Vec2,
}

pub fn plugin(app: &mut App) {
    app.add_plugins(MaterialPlugin::<CloudSeaMaterial>::default())
        .add_systems(Update, drift);
}

fn params(mid: Vec2, far: Vec3) -> SeaParams {
    SeaParams {
        top: Vec3::from_array(TOP).extend(1.0),
        hollow: Vec3::from_array(HOLLOW).extend(1.0),
        shape: Vec4::new(BILLOW_M, RELIEF, 0.0, 0.0),
        fade: Vec4::new(mid.x, mid.y, FADE_M, REACH_M),
        far: far.extend(FAR_SHARE),
    }
}

/// How far the sheet at `y` stands over it `r` metres out: nothing to
/// [`FLAT_M`], then up a smooth quarter-pipe to [`RIM_Y`].
pub fn lift(r: f32, y: f32) -> f32 {
    let t = ((r - FLAT_M) / (REACH_M - FLAT_M)).clamp(0.0, 1.0);
    (RIM_Y - y).max(0.0) * t * t
}

/// The sheet: rings round `mid` at `y` and their bank, facing up and in.
fn mesh(mid: Vec2, y: f32) -> Mesh {
    let mut pos = vec![[mid.x, y, mid.y]];
    let mut nrm = vec![[0.0, 1.0, 0.0]];
    for &r in &RINGS_M[1..] {
        // The bank's slope, for its normal: up and in toward the middle.
        let slope = (lift(r + 1.0, y) - lift(r - 1.0, y)) * 0.5;
        for k in 0..SPOKES {
            let a = k as f32 / SPOKES as f32 * core::f32::consts::TAU;
            let (s, c) = a.sin_cos();
            pos.push([mid.x + c * r, y + lift(r, y), mid.y + s * r]);
            nrm.push(Vec3::new(-c * slope, 1.0, -s * slope).normalize().to_array());
        }
    }
    let ring = |i: usize, k: usize| (1 + i * SPOKES + k % SPOKES) as u32;
    let mut idx = Vec::new();
    for k in 0..SPOKES {
        idx.extend_from_slice(&[0, ring(0, k + 1), ring(0, k)]);
    }
    for i in 0..RINGS_M.len() - 2 {
        for k in 0..SPOKES {
            let (a, b) = (ring(i, k), ring(i, k + 1));
            let (c, d) = (ring(i + 1, k), ring(i + 1, k + 1));
            idx.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_indices(Indices::U32(idx))
}

/// Spawns the sea of cloud at `y` (the map's `light.sea`) round `mid`.
pub fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<CloudSeaMaterial>,
    mid: Vec2,
    y: f32,
) {
    commands.spawn((
        crate::level::LevelPart,
        CloudSea { mid },
        Mesh3d(meshes.add(mesh(mid, y))),
        MeshMaterial3d(materials.add(CloudSeaMaterial {
            base: StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 1.0,
                reflectance: 0.0,
                ..default()
            },
            extension: Sea {
                params: params(mid, Vec3::splat(0.5)),
                motion: super::sea_motion::MOTION,
            },
        })),
        Transform::IDENTITY,
        // Its billows are the shader's: the sheet itself shades nothing,
        // and an island's shadow on it would make it a floor close under.
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// Once a frame: the drift on by [`DRIFT`], wrapped, and the colour the
/// far sheet goes to (the mood's fog, as the deck's haze band has it).
fn drift(
    time: Res<Time>,
    mood: Res<super::mood::MoodNow>,
    tier: Res<super::quality::Tier>,
    mut materials: ResMut<Assets<CloudSeaMaterial>>,
    mut motion: ResMut<super::sea_motion::SeaMotion>,
    seas: Query<(Ref<CloudSea>, &MeshMaterial3d<CloudSeaMaterial>)>,
) {
    if seas.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    motion.cloud = (motion.cloud + Vec2::from_array(DRIFT) * dt).rem_euclid(Vec2::splat(WRAP_M));
    // The fog's colour moves only with the mood: the material is written
    // then (or for a new sheet), not every frame.
    if !mood.is_changed() && !tier.is_changed() && !seas.iter().any(|(s, _)| s.is_added()) {
        return;
    }
    let far = super::rig::fog_for(&mood.mood, tier.atmosphere, 0.0).color.to_linear();
    let far = Vec3::new(far.red, far.green, far.blue);
    for (sea, handle) in &seas {
        let want = params(sea.mid, far);
        if materials.get(&handle.0).is_some_and(|m| m.extension.params != want) {
            if let Some(m) = materials.get_mut(&handle.0) {
                m.extension.params = want;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flat at its height out to the bank; the bank's rim stands over the
    /// height of any eye on the island and within the far plane from
    /// anywhere on it, so no band of the atmosphere's ground shows under
    /// the horizon.
    #[test]
    fn the_sheet_is_flat_then_banks_up_inside_the_far_plane() {
        let mid = Vec2::new(3.0, -2.0);
        let m = mesh(mid, -60.0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("positions");
        };
        for v in p {
            let r = Vec2::new(v[0], v[2]).distance(mid);
            if r <= FLAT_M {
                assert_eq!(v[1], -60.0);
            }
            assert!(r <= REACH_M + 0.01);
            assert!(v[1] <= RIM_Y + 1e-3);
        }
        assert!(p.iter().any(|v| (v[1] - RIM_Y).abs() < 1e-3));
        // An eye 30 m over the island's ground (0), up to 35 m off the
        // middle (the island's far edge): the rim is above it and nearer
        // than the far plane. The sheet is whole under the island and
        // only thins once it has begun to bank.
        const { assert!(RIM_Y >= 12.0) };
        const { assert!(REACH_M + 35.0 < crate::look::rig::FAR_M) };
        const { assert!(FADE_M < REACH_M && FADE_M > 100.0) };
    }
}

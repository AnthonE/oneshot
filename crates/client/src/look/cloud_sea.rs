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
//! atmosphere draws its warm ground, a hard line round the world. The shader
//! draws the billows per pixel, flattening them where a pixel can't hold
//! them, so it is the same sheet on every rung and on WebGL2.

use bevy::asset::RenderAssetUsages;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

const SHADER: &str = "shaders/cloud_sea.wgsl";

/// The sheet's rings, metres from the map's middle, and how far each stands
/// over the sea's level: flat to [`FLAT_M`], then the bank, its rim
/// [`BANK_M`] up by [`REACH_M`], under the far plane however the eye turns.
const RINGS_M: [f32; 14] = [
    0.0, 15.0, 30.0, 50.0, 75.0, 110.0, 150.0, 200.0, 260.0, 330.0, 400.0, 470.0, 530.0, 575.0,
];
pub const FLAT_M: f32 = 400.0;
pub const REACH_M: f32 = 575.0;
pub const BANK_M: f32 = 60.0;
const SPOKES: usize = 96;
/// A billow's top and the hollow between, linear: a grey white under the
/// lit sand's value, and a cool grey in the folds.
pub const TOP: [f32; 3] = [0.6, 0.61, 0.64];
pub const HOLLOW: [f32; 3] = [0.34, 0.36, 0.41];
/// The big billows' size, metres, and how steeply they stand.
pub const BILLOW_M: f32 = 70.0;
pub const RELIEF: f32 = 1.6;
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
}

/// The extension. Not bindless, as the level's isn't.
#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct Sea {
    #[uniform(100)]
    pub params: SeaParams,
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

/// The sheet and how far it has drifted.
#[derive(Component, Default)]
pub struct CloudSea {
    drift: Vec2,
}

pub fn plugin(app: &mut App) {
    app.add_plugins(MaterialPlugin::<CloudSeaMaterial>::default())
        .add_systems(Update, drift);
}

fn params(drift: Vec2) -> SeaParams {
    SeaParams {
        top: Vec3::from_array(TOP).extend(1.0),
        hollow: Vec3::from_array(HOLLOW).extend(1.0),
        shape: Vec4::new(BILLOW_M, RELIEF, drift.x, drift.y),
    }
}

/// How far the sheet stands over the sea's level `r` metres out: nothing
/// to [`FLAT_M`], then up a smooth quarter-pipe to [`BANK_M`].
pub fn lift(r: f32) -> f32 {
    let t = ((r - FLAT_M) / (REACH_M - FLAT_M)).clamp(0.0, 1.0);
    BANK_M * t * t
}

/// The sheet: rings round `mid` at `y` and their bank, facing up and in.
fn mesh(mid: Vec2, y: f32) -> Mesh {
    let mut pos = vec![[mid.x, y, mid.y]];
    let mut nrm = vec![[0.0, 1.0, 0.0]];
    for &r in &RINGS_M[1..] {
        // The bank's slope, for its normal: up and in toward the middle.
        let slope = (lift(r + 1.0) - lift(r - 1.0)) * 0.5;
        for k in 0..SPOKES {
            let a = k as f32 / SPOKES as f32 * core::f32::consts::TAU;
            let (s, c) = a.sin_cos();
            pos.push([mid.x + c * r, y + lift(r), mid.y + s * r]);
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
        CloudSea::default(),
        Mesh3d(meshes.add(mesh(mid, y))),
        MeshMaterial3d(materials.add(CloudSeaMaterial {
            base: StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 1.0,
                reflectance: 0.0,
                ..default()
            },
            extension: Sea {
                params: params(Vec2::ZERO),
            },
        })),
        Transform::IDENTITY,
        // Its billows are the shader's: the sheet itself shades nothing,
        // and an island's shadow on it would make it a floor close under.
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// Once a frame: the drift on by [`DRIFT`], wrapped.
fn drift(
    time: Res<Time>,
    mut materials: ResMut<Assets<CloudSeaMaterial>>,
    mut seas: Query<(&mut CloudSea, &MeshMaterial3d<CloudSeaMaterial>)>,
) {
    let dt = time.delta_secs();
    for (mut sea, handle) in &mut seas {
        sea.drift = (sea.drift + Vec2::from_array(DRIFT) * dt).rem_euclid(Vec2::splat(WRAP_M));
        if let Some(m) = materials.get_mut(&handle.0) {
            m.extension.params = params(sea.drift);
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
        let m = mesh(mid, -30.0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("positions");
        };
        for v in p {
            let r = Vec2::new(v[0], v[2]).distance(mid);
            if r <= FLAT_M {
                assert_eq!(v[1], -30.0);
            }
            assert!(r <= REACH_M + 0.01);
        }
        // An eye 30 m over the island's ground (0), up to 60 m off the
        // middle: the rim is above it and nearer than the far plane.
        assert!(-30.0 + BANK_M >= 30.0);
        assert!(REACH_M + 60.0 < crate::look::rig::FAR_M);
    }
}

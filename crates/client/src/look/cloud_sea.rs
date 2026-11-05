//! The sea of cloud under an island over a void (a map with a `void`,
//! `sim::map::Map::void`): where a canyon has water at its sea level, the
//! island has cloud, billowing and drifting, lit from above, faded by the
//! air with the distance (`cloud_sea.wgsl`).
//!
//! Drawing only, like the sea: the sim's void is far above where this is
//! drawn, so whoever falls is dead before they reach it, and nothing in it
//! hides or shows a fighter. One flat sheet out to the far plane; the
//! shader draws the billows per pixel, flattening them where a pixel can't
//! hold them, so it is the same sheet on every rung and on WebGL2.

use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

const SHADER: &str = "shaders/cloud_sea.wgsl";

/// How far the sheet reaches from the map's middle, metres: past the far
/// plane, so its edge is never seen.
pub const REACH_M: f32 = 6_000.0;
/// A billow's top and the hollow between, linear: the deck's white, and a
/// cool grey under it.
pub const TOP: [f32; 3] = [0.80, 0.81, 0.83];
pub const HOLLOW: [f32; 3] = [0.42, 0.44, 0.49];
/// The big billows' size, metres, and how steeply they stand.
pub const BILLOW_M: f32 = 70.0;
pub const RELIEF: f32 = 0.9;
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

/// The sheet: a square [`REACH_M`] out each way from `mid`, at `y`, facing
/// up, in a few cells (it is flat; the cells keep the far triangles from
/// being thousands of metres across).
fn mesh(mid: Vec2, y: f32) -> Mesh {
    const N: usize = 16;
    let mut pos = Vec::with_capacity((N + 1) * (N + 1));
    for j in 0..=N {
        for i in 0..=N {
            let u = i as f32 / N as f32 * 2.0 - 1.0;
            let v = j as f32 / N as f32 * 2.0 - 1.0;
            pos.push([mid.x + u * REACH_M, y, mid.y + v * REACH_M]);
        }
    }
    let mut idx = Vec::with_capacity(N * N * 6);
    for j in 0..N {
        for i in 0..N {
            let a = (j * (N + 1) + i) as u32;
            let (b, c) = (a + 1, a + (N + 1) as u32);
            let d = c + 1;
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    let n = pos.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
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
        // Its billows are the shader's: the sheet itself shades nothing.
        NotShadowCaster,
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

    /// The sheet reaches past the far plane all round, flat at its height.
    #[test]
    fn the_sheet_is_flat_and_reaches_the_horizon() {
        let m = mesh(Vec2::new(3.0, -2.0), -30.0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("positions");
        };
        assert!(p.iter().all(|v| v[1] == -30.0));
        let (lo, hi) = p
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v[0]), hi.max(v[0])));
        assert!(lo <= 3.0 - REACH_M + 1.0 && hi >= 3.0 + REACH_M - 1.0);
    }
}

//! The sword's blade. The hilt is an ordinary model; the blade in its file
//! (`tools/models/sword.py`) is only a shape, which [`light`] draws as hard light
//! (`assets/shaders/blade.wgsl`), so it reads the same on every rung and has
//! no maps. Drawing only: its reach is `sim::consts`' and nothing here knows it.
//!
//! Its tag is when it was lit, so each new one draws itself out of the hilt with
//! nothing running on it after. An icon's and a shadow's blade are not lit: the
//! one stays a silhouette and the other is taken away ([`is_blade`]).
//!
//! Swung, it leaves ghosts ([`trail`]): the blade again where it just was, in
//! whoever holds it's frame, so walking or turning leaves none.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::gltf::GltfMaterialName;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, MeshTag, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::Material;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::scene::SceneInstanceReady;
use bevy::shader::ShaderRef;

pub const SHADER: &str = "shaders/blade.wgsl";

/// The material the sword's file gives its blade.
const MATERIAL: &str = "blade";
/// Its edges' and spine's light, linear; and how bright it is overall.
const ICE: Vec4 = Vec4::new(0.42, 0.9, 1.15, 1.0);
/// Its inner line's light, and how much of its body is the foil's colours.
const INNER: Vec4 = Vec4::new(0.8, 0.14, 1.05, 0.72);
/// How far its aura reaches past its edges, metres.
const AURA_M: f32 = 0.06;
/// How much of that it loses by its point.
const AURA_TIP: f32 = 0.8;
/// The light it throws: lumens, how far, its colour.
const LUMENS: f32 = 42_000.0;
const LIGHT_M: f32 = 3.5;
const LIGHT_RGB: [f32; 3] = [0.42, 0.82, 1.0];
/// Where that is, up the model: well up the blade, or the fingers round the
/// hilt burn white.
const LIGHT_AT_M: f32 = 0.62;
/// How fast its tip must go in its holder's frame to leave ghosts, m/s; a ghost
/// for every so far it went, at most so many a frame, and how long they last.
const GHOST_SPEED_M_S: f32 = 5.5;
const GHOST_EVERY_M: f32 = 0.05;
const GHOST_MAX: u32 = 9;
const GHOST_S: f32 = 0.12;
/// Its tip, up the model.
const TIP_M: f32 = 1.07;

/// Field for field `Blade` in the shader.
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct BladeParams {
    pub ice: Vec4,
    pub inner: Vec4,
    pub mode: Vec4,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Debug)]
pub struct BladeMaterial {
    #[uniform(0)]
    pub params: BladeParams,
}

impl Material for BladeMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// The blade's material, its aura's and its ghosts', and the aura's strip once
/// the sword's file is in.
#[derive(Resource, Clone)]
pub struct Kit {
    blade: Handle<BladeMaterial>,
    aura: Handle<BladeMaterial>,
    ghost: Handle<BladeMaterial>,
    strip: Option<Handle<Mesh>>,
}

pub fn setup(mut commands: Commands, mut materials: ResMut<Assets<BladeMaterial>>) {
    let mut one = |mode: f32| {
        materials.add(BladeMaterial {
            params: BladeParams {
                ice: ICE,
                inner: INNER,
                mode: Vec4::new(mode, GHOST_S, 0.0, 0.0),
            },
        })
    };
    let kit = Kit {
        blade: one(0.0),
        aura: one(1.0),
        ghost: one(2.0),
        strip: None,
    };
    commands.insert_resource(kit);
}

/// A strip down the blade's middle line, each ring's two corners on it with its
/// half-width and [`AURA_M`] in `uv.x`, either way: the shader turns it to the eye.
fn strip(blade: &Mesh) -> Option<Mesh> {
    let VertexAttributeValues::Float32x3(at) = blade.attribute(Mesh::ATTRIBUTE_POSITION)? else {
        return None;
    };
    let VertexAttributeValues::Float32x2(uv) = blade.attribute(Mesh::ATTRIBUTE_UV_0)? else {
        return None;
    };
    // By distance along it: its height and its lowest and highest z.
    let mut rings: Vec<(f32, f32, f32, f32)> = Vec::new();
    for (p, t) in at.iter().zip(uv) {
        match rings.iter_mut().find(|r| (r.0 - t[1]).abs() < 1e-4) {
            Some(r) => {
                r.2 = r.2.min(p[2]);
                r.3 = r.3.max(p[2]);
            }
            None => rings.push((t[1], p[1], p[2], p[2])),
        }
    }
    rings.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut positions = Vec::with_capacity(rings.len() * 2);
    let mut uvs = Vec::with_capacity(rings.len() * 2);
    let mut indices = Vec::with_capacity(rings.len() * 6);
    for (i, &(v, y, lo, hi)) in rings.iter().enumerate() {
        let half = (hi - lo) * 0.5 + AURA_M * (1.0 - v * v * AURA_TIP);
        for side in [-half, half] {
            positions.push([0.0, y, (lo + hi) * 0.5]);
            uvs.push([side, v]);
        }
        if i + 1 < rings.len() {
            let a = i as u32 * 2;
            indices.extend([a, a + 1, a + 3, a, a + 3, a + 2]);
        }
    }
    let normals = vec![[0.0, 0.0, 1.0]; positions.len()];
    Some(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
            .with_inserted_indices(Indices::U32(indices)),
    )
}

/// Whether a mesh of a gun's scene is the sword's blade.
pub fn is_blade(name: Option<&GltfMaterialName>) -> bool {
    name.is_some_and(|n| n.0 == MATERIAL)
}

/// `arm`'s gun `scene` once it has spawned: the sword's blade lit, and any
/// other gun as it came.
pub fn light(commands: &mut Commands, scene: Entity, arm: u8) {
    if arm == sim::arms::SWORD {
        commands.entity(scene).observe(lit);
    }
}

/// A lit blade held by someone: the frame its ghosts stay in, and where it was
/// in that frame last frame.
#[derive(Component)]
pub struct Held {
    frame: Entity,
    last: Option<Transform>,
}

/// Gone at this time.
#[derive(Component)]
pub struct Ghost(f32);

/// The blade itself, lit: not its aura or its ghosts.
#[derive(Component)]
pub struct Lit;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn lit(
    ev: On<SceneInstanceReady>,
    children: Query<&Children>,
    blades: Query<(&Mesh3d, &GltfMaterialName)>,
    parents: Query<&ChildOf>,
    holders: Query<(), Or<(With<super::viewmodel::InView>, With<super::bodies::Body>)>>,
    kit: Option<ResMut<Kit>>,
    mut meshes: ResMut<Assets<Mesh>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let Some(mut kit) = kit else {
        return;
    };
    let born = MeshTag(time.elapsed_secs_wrapped().to_bits());
    for e in children.iter_descendants(ev.entity) {
        let Ok((mesh, name)) = blades.get(e) else {
            continue;
        };
        if !is_blade(Some(name)) {
            continue;
        }
        commands.entity(e).remove::<MeshMaterial3d<StandardMaterial>>().insert((
            Lit,
            MeshMaterial3d(kit.blade.clone()),
            born.clone(),
            NotShadowCaster,
            NotShadowReceiver,
        ));
        if let Some(frame) = parents.iter_ancestors(ev.entity).find(|a| holders.contains(*a)) {
            commands.entity(e).insert(Held { frame, last: None });
        }
        if kit.strip.is_none() {
            let made = meshes.get(&mesh.0).and_then(strip);
            kit.strip = made.map(|m| meshes.add(m));
        }
        if let Some(strip) = kit.strip.clone() {
            commands.spawn((
                Mesh3d(strip),
                MeshMaterial3d(kit.aura.clone()),
                born.clone(),
                Transform::IDENTITY,
                // Its box is a line: the shader spreads it.
                NoFrustumCulling,
                NotShadowCaster,
                NotShadowReceiver,
                ChildOf(e),
            ));
        }
        commands.spawn((
            PointLight {
                color: Color::linear_rgb(LIGHT_RGB[0], LIGHT_RGB[1], LIGHT_RGB[2]),
                intensity: LUMENS,
                range: LIGHT_M,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_xyz(0.0, LIGHT_AT_M, 0.0),
            ChildOf(ev.entity),
        ));
    }
}

/// Each frame: ghosts behind a blade going fast, and the old ones gone.
pub fn trail(
    time: Res<Time>,
    kit: Option<Res<Kit>>,
    mut blades: Query<(&GlobalTransform, &Mesh3d, &InheritedVisibility, &mut Held)>,
    frames: Query<&GlobalTransform>,
    ghosts: Query<(Entity, &Ghost)>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs();
    for (e, ghost) in &ghosts {
        if now >= ghost.0 {
            commands.entity(e).try_despawn();
        }
    }
    let Some(kit) = kit else {
        return;
    };
    let dt = time.delta_secs();
    let clock = time.elapsed_secs_wrapped();
    for (at, mesh, seen, mut held) in &mut blades {
        let Ok(frame) = frames.get(held.frame) else {
            continue;
        };
        let here = at.reparented_to(frame);
        let last = held.last.replace(here);
        let Some(last) = last.filter(|_| seen.get() && dt > 0.0) else {
            continue;
        };
        let tip = |t: &Transform| t.transform_point(Vec3::Y * TIP_M);
        let went = (tip(&here) - tip(&last)).length() / here.scale.y.abs().max(1e-3);
        if went / dt < GHOST_SPEED_M_S {
            continue;
        }
        let n = ((went / GHOST_EVERY_M).ceil() as u32).clamp(1, GHOST_MAX);
        for i in 0..n {
            let k = i as f32 / n as f32;
            commands.spawn((
                Mesh3d(mesh.0.clone()),
                MeshMaterial3d(kit.ghost.clone()),
                MeshTag((clock - dt * (1.0 - k)).to_bits()),
                Transform {
                    translation: last.translation.lerp(here.translation, k),
                    rotation: last.rotation.slerp(here.rotation, k),
                    scale: here.scale,
                },
                Ghost(now + GHOST_S),
                NotShadowCaster,
                NotShadowReceiver,
                ChildOf(held.frame),
            ));
        }
    }
}

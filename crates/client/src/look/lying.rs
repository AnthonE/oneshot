//! A gun or grenades lying for the taking (`items.rs`): the model as it came,
//! with a faint gold glow over it, strongest at its edges (`lying.wgsl`), so
//! a grey gun reads off a grey floor. Its own glow map is left alone: a
//! plain emissive would only scale that.

use std::collections::HashMap;

use bevy::gltf::GltfMaterialName;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::scene::SceneInstanceReady;
use bevy::shader::ShaderRef;

pub type LyingMaterial = ExtendedMaterial<StandardMaterial, Glow>;

pub const SHADER: &str = "shaders/lying.wgsl";

/// The glow at a lying gun's edge, linear: the pads' old gold; and the
/// share of it over the whole gun.
const RIM: Vec3 = Vec3::new(0.79 * 0.35, 0.64 * 0.35, 0.15 * 0.35);
const ALL_OVER: f32 = 0.15;

#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct GlowParams {
    pub color: Vec4,
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct Glow {
    #[uniform(100)]
    pub params: GlowParams,
}

impl MaterialExtension for Glow {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

/// Each model's material glowing, by its own: one however many lie about.
#[derive(Resource, Default)]
pub struct Glows(HashMap<AssetId<StandardMaterial>, Handle<LyingMaterial>>);

/// Makes `scene` (a gun's or a frag's) glow once it has spawned.
pub fn glow(commands: &mut Commands, scene: Entity) {
    commands.entity(scene).observe(swap);
}

/// The scene is in: its materials take the glow. A sword's blade is light
/// already.
fn swap(
    ev: On<SceneInstanceReady>,
    children: Query<&Children>,
    plain: Query<(&MeshMaterial3d<StandardMaterial>, Option<&GltfMaterialName>)>,
    own: Res<Assets<StandardMaterial>>,
    mut glows: ResMut<Glows>,
    mut mats: ResMut<Assets<LyingMaterial>>,
    mut commands: Commands,
) {
    for e in children.iter_descendants(ev.entity) {
        let Ok((m, name)) = plain.get(e) else {
            continue;
        };
        if super::blade::is_blade(name) {
            continue;
        }
        let Some(base) = own.get(&m.0) else {
            continue;
        };
        let handle = glows
            .0
            .entry(m.0.id())
            .or_insert_with(|| {
                mats.add(ExtendedMaterial {
                    base: base.clone(),
                    extension: Glow {
                        params: GlowParams {
                            color: RIM.extend(ALL_OVER),
                        },
                    },
                })
            })
            .clone();
        commands
            .entity(e)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(handle));
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Glows>()
        .add_plugins(MaterialPlugin::<LyingMaterial>::default());
}

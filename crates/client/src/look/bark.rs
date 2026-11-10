//! The bark of our own hands in view. Triple T is bark all over, and his map
//! is one atlas for the whole of him: a millimetre a texel on a hand, which
//! is bark across a room and a smear on the nearest thing on the screen
//! (`hands.rs`), where a pixel is a quarter of that. So the hands in view
//! wear a photograph of bark over it (Poly Haven's, CC0; `tools/bark.py`
//! cuts it small, fine and his colour): laid by where a pixel lies on the
//! hand (`hand_bark.wgsl`), its furrows running the forearm's length, its
//! normals leaning the light and its occlusion shading the furrows. The
//! atlas stays under it as his own light and dark.
//!
//! `StandardMaterial` plus a fragment stage: one uniform and two maps, so
//! WebGL2 draws what the desktop does, on every rung. A body across the
//! room keeps the atlas alone: none of this shows there.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageLoaderSettings, ImageSampler};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::scene::SceneInstanceReady;
use bevy::shader::ShaderRef;

pub type HandMaterial = ExtendedMaterial<StandardMaterial, Bark>;

pub const SHADER: &str = "shaders/hand_bark.wgsl";

/// The uniform; each lane is named in `hand_bark.wgsl`.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub struct BarkParams {
    pub lay: Vec4,
    pub finish: Vec4,
}

/// How much of a hand one tile of the photograph covers, metres as the hand
/// looks: half a metre of trunk on a hand's length, so its furrows are a few
/// millimetres across, as his own are, and a texel is a seventh of one.
pub const BARK_TILE_M: f32 = 0.14;
/// The atlas' middle on a hand (linear luma, as its file has it:
/// `tools/models/hand.py` bakes the body's lift in), and how much of the
/// atlas' own light and dark the bark keeps.
const ATLAS_MID: f32 = 0.156;
const ATLAS_TONE: f32 = 0.5;
/// How sharply one laying of the photograph gives way to the next round a
/// finger: higher, and less of the hand is two of them at once.
const LAY_SHARP: f32 = 5.0;
/// Bark is dry: its roughness on a ridge and down a furrow.
const ROUGH_RIDGE: f32 = 0.82;
const ROUGH_FURROW: f32 = 0.96;
/// How much of the photograph's occlusion shades the light a hand stands in.
const SHADE: f32 = 0.6;

pub fn params() -> BarkParams {
    BarkParams {
        lay: Vec4::new(1.0 / BARK_TILE_M, ATLAS_TONE, ATLAS_MID, LAY_SHARP),
        finish: Vec4::new(ROUGH_RIDGE, ROUGH_FURROW, SHADE, 0.0),
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct Bark {
    #[uniform(100)]
    pub params: BarkParams,
    #[texture(101)]
    #[sampler(102)]
    pub colour: Handle<Image>,
    #[texture(103)]
    pub relief: Handle<Image>,
}

impl MaterialExtension for Bark {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

/// The photograph's two maps, and each hand's material in them, by the
/// model's own.
#[derive(Resource)]
pub struct Kit {
    colour: Handle<Image>,
    relief: Handle<Image>,
    /// Their chains are built (`chains`).
    mipped: bool,
    worn: HashMap<AssetId<StandardMaterial>, Handle<HandMaterial>>,
}

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    let map = |file: &str, srgb: bool| {
        assets.load_with_settings(format!("textures/{file}.jpg"), move |s: &mut ImageLoaderSettings| {
            s.is_srgb = srgb;
            s.sampler = ImageSampler::Descriptor(super::textures::repeat());
        })
    };
    commands.insert_resource(Kit {
        colour: map("bark_albedo", true),
        relief: map("bark_normal", false),
        mipped: false,
        worn: HashMap::new(),
    });
}

/// Once both maps are in: their chains, as the level's sets get theirs, and
/// the GPU's from there as theirs are (nothing reads them back). The
/// relief's is averaged as data: its third lane is occlusion, not a z.
fn chains(mut kit: ResMut<Kit>, mut images: ResMut<Assets<Image>>) {
    if kit.mipped || !images.contains(&kit.colour) || !images.contains(&kit.relief) {
        return;
    }
    for h in [kit.colour.clone(), kit.relief.clone()] {
        if let Some(img) = images.get_mut(&h) {
            super::textures::mip_in_place(img, false);
            img.asset_usage = RenderAssetUsages::RENDER_WORLD;
        }
    }
    kit.mipped = true;
}

/// Puts the bark on a hand's `scene`, once it has spawned.
pub fn wear(commands: &mut Commands, scene: Entity) {
    commands.entity(scene).observe(dress);
}

fn dress(
    ev: On<SceneInstanceReady>,
    children: Query<&Children>,
    plain: Query<&MeshMaterial3d<StandardMaterial>>,
    own: Res<Assets<StandardMaterial>>,
    mut kit: ResMut<Kit>,
    mut mats: ResMut<Assets<HandMaterial>>,
    mut commands: Commands,
) {
    for e in children.iter_descendants(ev.entity) {
        let Ok(m) = plain.get(e) else {
            continue;
        };
        let Some(base) = own.get(&m.0) else {
            continue;
        };
        let (colour, relief) = (kit.colour.clone(), kit.relief.clone());
        let handle = kit
            .worn
            .entry(m.0.id())
            .or_insert_with(|| {
                mats.add(ExtendedMaterial {
                    base: base.clone(),
                    extension: Bark {
                        params: params(),
                        colour,
                        relief,
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
    app.add_plugins(MaterialPlugin::<HandMaterial>::default())
        .add_systems(PreStartup, load)
        .add_systems(Update, chains);
}

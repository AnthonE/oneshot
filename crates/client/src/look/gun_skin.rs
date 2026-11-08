//! A skin on a gun. The skin a wallet owns (`proto::SKINS`, checked by the
//! server) trims Triple T (`bodies.rs`) and coats his guns: the gun's own
//! paint recoloured as it is drawn (`gun_skin.wgsl`), its wear, panel lines
//! and light kept, so one coat fits all fifteen with no maps of its own.
//!
//! Over the coat each skin but the free one draws its own pattern, moving
//! and lit (molten cracks, frost, veins, gold), in the mesh's own frame so
//! it stays on the gun. Cosmetic as the trim is (DESIGN.md §15): a skin may
//! glow, which shows its wearer more, never less. A gun on a pad or on the
//! floor wears its own paint.
//!
//! The gun in our own view wears this material whatever its skin, each
//! instance its own, so its glow can live (`gun_glow.rs`); so does every gun
//! a fighter holds, for what it reflects.
//!
//! Guns are metal, and metal is what it reflects. The scene's fill is a
//! two-tone cube (`fill.rs`), which leaves a gun looking painted, so every
//! coat reflects a sky photograph too (`ENV`, cut by `tools/gun_env.py`):
//! sharp on a polished part, a sheen on a rough one, by the gun's own maps.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::gltf::GltfMaterialName;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use bevy::scene::SceneInstanceReady;
use bevy::shader::ShaderRef;
use sim::arms;

pub type GunMaterial = ExtendedMaterial<StandardMaterial, Coat>;

pub const SHADER: &str = "shaders/gun_skin.wgsl";

/// The uniform; each lane is named in `gun_skin.wgsl`.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub struct CoatParams {
    pub paint: Vec4,
    pub rest: Vec4,
    pub mid: Vec4,
    pub local: Mat4,
    pub glow: Vec4,
    pub stays: Vec4,
    pub goes: Vec4,
    pub boxes: [Vec4; 10],
    pub look: Vec4,
    pub hot: Vec4,
    pub env: Vec4,
}

/// How bright the sky photograph is in a reflection, on screen (its mean is
/// 1).
const ENV_GAIN: f32 = 0.55;

/// No coat, its glow as painted, no gauge.
pub const BARE: CoatParams = CoatParams {
    paint: Vec4::ZERO,
    rest: Vec4::ONE,
    mid: Vec4::ONE,
    local: Mat4::IDENTITY,
    glow: Vec4::new(1.0, 0.0, 1.0, 1.0),
    stays: Vec4::ZERO,
    goes: Vec4::ZERO,
    // The run's box empty, no cells.
    boxes: [
        Vec4::ONE,
        Vec4::ZERO,
        Vec4::ZERO,
        Vec4::ZERO,
        Vec4::ZERO,
        Vec4::ZERO,
        Vec4::ZERO,
        Vec4::ZERO,
        Vec4::ZERO,
        Vec4::ZERO,
    ],
    look: Vec4::ZERO,
    hot: Vec4::ZERO,
    env: Vec4::new(ENV_GAIN, (ENV_LEVELS - 1) as f32, 0.0, 0.0),
};

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct Coat {
    #[uniform(100)]
    pub params: CoatParams,
    #[texture(101, dimension = "cube")]
    #[sampler(102)]
    pub env: Handle<Image>,
}

/// The sky photograph every gun reflects: a cube of `ENV_FACE`² at its
/// sharpest, `ENV_LEVELS` levels, RGB9E5 (`tools/gun_env.py`).
const ENV: &[u8] = include_bytes!("../../assets/textures/gun_env.rgb9e5");
const ENV_FACE: u32 = 128;
const ENV_LEVELS: u32 = 6;

/// The cube, once.
#[derive(Resource)]
pub struct GunEnv(Handle<Image>);

fn env_image() -> Image {
    let mut img = Image::new_uninit(
        Extent3d {
            width: ENV_FACE,
            height: ENV_FACE,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        TextureFormat::Rgb9e5Ufloat,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = ENV_LEVELS;
    img.data = Some(ENV.to_vec());
    img.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    img
}

impl MaterialExtension for Coat {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

/// One skin's colours, sRGB: its paint's and the rest's; the paint's metal
/// and roughness; its pattern, if it has one.
struct Colours {
    paint: u32,
    rest: u32,
    metal: f32,
    rough: f32,
    pattern: Option<Pattern>,
}

/// A skin's pattern (`gun_skin.wgsl`'s `pattern`).
struct Pattern {
    /// Which: 1 molten, 2 frost, 3 veins, 4 gilded.
    kind: u8,
    /// Its glow's strength.
    glow: f32,
    /// Its cells a metre.
    cells: f32,
    /// Its time's speed.
    speed: f32,
    /// Its hot colour, sRGB.
    hot: u32,
    /// How much of the gun's steel it takes (its paint it takes whole).
    cover: f32,
}

const fn colours(paint: u32, rest: u32, metal: f32, rough: f32, pattern: Option<Pattern>) -> Option<Colours> {
    Some(Colours {
        paint,
        rest,
        metal,
        rough,
        pattern,
    })
}

const fn pattern(kind: u8, glow: f32, cells: f32, speed: f32, hot: u32, cover: f32) -> Option<Pattern> {
    Some(Pattern {
        kind,
        glow,
        cells,
        speed,
        hot,
        cover,
    })
}

/// By skin (`proto::SKINS` order); none for the standard look. Pill, the
/// free one, is a plain coat.
const COLOURS: [Option<Colours>; 6] = [
    None,
    colours(0xd9dcdf, 0x4a4e53, 0.1, 0.45, None),
    colours(
        0xb3300f,
        0x241c1a,
        0.15,
        0.5,
        pattern(1, 6.0, 14.0, 1.0, 0xffb040, 0.55),
    ),
    colours(0x9fd2ea, 0x39475a, 0.25, 0.4, pattern(2, 2.5, 16.0, 1.0, 0x6fe8ff, 0.5)),
    colours(0x3f9a4e, 0x1e2a20, 0.2, 0.5, pattern(3, 4.0, 9.0, 1.0, 0x5cff7a, 0.55)),
    colours(
        0xd9a531,
        0x5a4520,
        0.9,
        0.42,
        pattern(4, 3.0, 10.0, 1.0, 0xfff0c0, 0.65),
    ),
];

/// A gun line's paint, as the shader reads it: its paint's and the rest's
/// middle (linear luma), and how far from grey a texel becomes paint.
struct Line {
    paint_mid: f32,
    rest_mid: f32,
    chroma: [f32; 2],
}

/// The human guns' olive is a faint colour on grey metal.
const FORGE: Line = Line {
    paint_mid: 0.08,
    rest_mid: 0.08,
    chroma: [0.02, 0.06],
};
/// The ship's violet is most of the gun.
const VEIL: Line = Line {
    paint_mid: 0.05,
    rest_mid: 0.1,
    chroma: [0.066, 0.13],
};

fn line(arm: u8) -> &'static Line {
    if arm >= arms::SWORD {
        &VEIL
    } else {
        &FORGE
    }
}

fn linear(hex: u32, w: f32) -> Vec4 {
    let [_, r, g, b] = hex.to_be_bytes();
    let c = Color::srgb_u8(r, g, b).to_linear();
    Vec4::new(c.red, c.green, c.blue, w)
}

fn params(c: &Colours, l: &Line) -> CoatParams {
    let (look, hot) = c.pattern.as_ref().map_or((Vec4::ZERO, Vec4::ZERO), |p| {
        (
            Vec4::new(p.kind as f32, p.glow, p.cells, p.speed),
            linear(p.hot, p.cover),
        )
    });
    CoatParams {
        paint: linear(c.paint, c.metal),
        rest: linear(c.rest, c.rough),
        mid: Vec4::new(l.paint_mid, l.rest_mid, l.chroma[0], l.chroma[1]),
        glow: Vec4::new(1.0, 1.0, 1.0, 1.0),
        look,
        hot,
        ..BARE
    }
}

/// What a gun's scene wears, read when it has spawned, and for which hand
/// in our own view, if it is there.
#[derive(Component, Clone, Copy)]
pub struct Worn {
    skin: u8,
    arm: u8,
    view: Option<bool>,
}

/// Each coated material, by the gun's own and the skin.
#[derive(Resource, Default)]
pub struct Coats(HashMap<(AssetId<StandardMaterial>, u8), Handle<GunMaterial>>);

/// Puts skin `skin` on `arm`'s gun `scene`, once it has spawned. The standard
/// look is the gun's own paint, reflecting as the rest do.
pub fn wear(commands: &mut Commands, scene: Entity, skin: u8, arm: u8) {
    commands
        .entity(scene)
        .insert(Worn { skin, arm, view: None })
        .observe(coat);
}

/// The same for the gun in our own view, in the left hand or the right:
/// whatever its skin, its materials its own, its glow alive.
pub fn wear_in_view(commands: &mut Commands, scene: Entity, skin: u8, arm: u8, left: bool) {
    commands
        .entity(scene)
        .insert(Worn {
            skin,
            arm,
            view: Some(left),
        })
        .observe(coat);
}

#[allow(clippy::too_many_arguments)]
fn coat(
    ev: On<SceneInstanceReady>,
    worn: Query<&Worn>,
    children: Query<&Children>,
    plain: Query<(&MeshMaterial3d<StandardMaterial>, Option<&GltfMaterialName>)>,
    own: Res<Assets<StandardMaterial>>,
    mut coats: ResMut<Coats>,
    mut mats: ResMut<Assets<GunMaterial>>,
    env: Res<GunEnv>,
    mut commands: Commands,
) {
    let Ok(&Worn { skin, arm, view }) = worn.get(ev.entity) else {
        return;
    };
    let c = COLOURS.get(skin as usize).and_then(Option::as_ref);
    let coat = c.map_or(BARE, |c| params(c, line(arm)));
    // In view, each of the gun's own materials once, for it alone.
    let mut alive: Vec<(AssetId<StandardMaterial>, Handle<GunMaterial>)> = Vec::new();
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
        let make = |mats: &mut Assets<GunMaterial>| {
            mats.add(ExtendedMaterial {
                base: base.clone(),
                extension: Coat {
                    params: coat,
                    env: env.0.clone(),
                },
            })
        };
        let handle = if view.is_some() {
            match alive.iter().find(|(id, _)| *id == m.0.id()) {
                Some((_, h)) => h.clone(),
                None => {
                    let h = make(&mut mats);
                    alive.push((m.0.id(), h.clone()));
                    h
                }
            }
        } else {
            coats
                .0
                .entry((m.0.id(), skin))
                .or_insert_with(|| make(&mut mats))
                .clone()
        };
        commands
            .entity(e)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(handle));
    }
    if let Some(left) = view {
        commands.entity(ev.entity).insert(super::gun_glow::Alive::new(
            arm,
            left,
            coat,
            alive.into_iter().map(|(_, h)| h).collect(),
        ));
    }
}

fn load_env(mut images: ResMut<Assets<Image>>, mut commands: Commands) {
    commands.insert_resource(GunEnv(images.add(env_image())));
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Coats>()
        .add_systems(PreStartup, load_env)
        .add_plugins(MaterialPlugin::<GunMaterial>::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sky_photograph_is_its_cube() {
        let texels: u32 = (0..ENV_LEVELS).map(|l| (ENV_FACE >> l).pow(2)).sum();
        assert_eq!(ENV.len() as u32, texels * 6 * 4);
    }

    #[test]
    fn every_skin_but_the_standard_look_has_a_coat() {
        assert_eq!(COLOURS.len(), proto::SKINS.len());
        for (i, c) in COLOURS.iter().enumerate() {
            assert_eq!(c.is_none(), i == proto::SKIN_STANDARD as usize, "{}", proto::SKINS[i]);
        }
    }
}

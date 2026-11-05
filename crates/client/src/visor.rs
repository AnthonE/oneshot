//! The HUD on the inside of a visor. The HUD (`hud.rs`, under `HudRoot`)
//! is drawn by its own camera into an image the window's size, at the
//! window's own resolution, and that image is laid over the frame through
//! `visor.wgsl`, which makes it look lit on curved glass close to the eye:
//!
//!   · **Bent**: the middle swells a few percent toward the eye and the
//!     corners stay put, as a dome bends what is painted inside it.
//!   · **Glowing**: everything the HUD draws casts a soft halo of its own
//!     colour, as light on glass does (the HUD draws after the bloom, so
//!     nothing else could give it one).
//!   · **Split**: a faint fringe of red out and blue in toward the rim.
//!   · **Lagging**: the whole of it trails the head as the gun in view does
//!     ([`HudLag`], off `viewmodel.rs`'s sway and heave), a few pixels,
//!     and never the middle, where the crosshair is.
//!
//! The names over heads (`tags.rs`) are not on the visor: they mark where
//! a body is, so they stay where it is drawn. Menus and screens are their
//! own roots, drawn flat over everything. An image target and a UI
//! material are things WebGL2 draws as well as anything
//! (`look/render_scale.rs` is built the same way); the glow is the ladder's
//! (`look/quality.rs`): off on the lowest rung. Drawing only.

use bevy::camera::{visibility::RenderLayers, ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::ui::FocusPolicy;
use bevy::window::PrimaryWindow;

use crate::hud::HudRoot;
use crate::look::quality::{Quality, Tier};

/// How far the middle swells against the corners (a share of its size).
pub const BEND: f32 = 0.035;
/// The colour fringe at the corners, a share of the way to the middle.
pub const SPLIT: f32 = 0.0025;
/// The halo: how bright against what casts it, and how far it reaches, px.
pub const GLOW: f32 = 0.55;
pub const GLOW_PX: f32 = 7.0;
/// Faint lines across the HUD, as a projection's: how deep.
pub const LINES: f32 = 0.06;
/// The lag: px a radian of the gun's trail, px a metre of its heave, and
/// the most, px.
pub const LAG_PX_PER_RAD: f32 = 150.0;
pub const LAG_PX_PER_M: f32 = 450.0;
pub const LAG_MAX_PX: f32 = 16.0;

/// The trail the HUD takes this frame, as the gun's: radians right and up
/// it trails (`viewmodel.rs`), and metres it heaves.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub struct HudLag {
    pub trail: Vec2,
    pub heave: f32,
}

impl HudLag {
    /// The lag in px, right and down.
    pub fn px(&self) -> Vec2 {
        let v = Vec2::new(
            self.trail.x * LAG_PX_PER_RAD,
            -(self.trail.y * LAG_PX_PER_RAD + self.heave * LAG_PX_PER_M),
        );
        v.clamp_length_max(LAG_MAX_PX)
    }
}

#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct VisorParams {
    /// x the bend, y the split, z the glow, w its reach in px.
    pub look: Vec4,
    /// xy the lag (uv), z the image's width over its height, w the lines.
    pub lag: Vec4,
    /// xy one px in uv.
    pub px: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct VisorMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub hud: Handle<Image>,
    #[uniform(2)]
    pub params: VisorParams,
}

impl UiMaterial for VisorMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/visor.wgsl".into()
    }
}

/// The HUD's camera, its image and the node that lays it over the frame.
#[derive(Resource)]
pub struct Visor {
    cam: Entity,
    image: Handle<Image>,
    mat: Handle<VisorMaterial>,
    size: UVec2,
    scale: f32,
}

/// The names over heads: their own root, off the visor (`tags.rs`).
#[derive(Component)]
pub struct TagRoot;

pub fn plugin(app: &mut App) {
    app.add_plugins(UiMaterialPlugin::<VisorMaterial>::default())
        .init_resource::<HudLag>()
        .add_systems(PostUpdate, fit.before(bevy::camera::CameraUpdateSystems));
}

/// After the HUD is built: its camera and image, the node that shows it,
/// and the names' own root.
pub fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<VisorMaterial>>,
    root: Query<Entity, With<HudRoot>>,
) {
    let Ok(root) = root.single() else {
        return;
    };
    let mut image = Image::new_target_texture(1, 1, TextureFormat::Rgba8UnormSrgb, None);
    image.sampler = ImageSampler::linear();
    let image = images.add(image);
    let cam = commands
        .spawn((
            Camera2d,
            Camera {
                // Before every camera that shows it.
                order: -10,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::Image(ImageRenderTarget {
                handle: image.clone(),
                scale_factor: 1.0,
            }),
            RenderLayers::none(),
            Msaa::Off,
            Tonemapping::None,
            DebandDither::Disabled,
        ))
        .id();
    commands.entity(root).insert(UiTargetCamera(cam));
    let mat = mats.add(VisorMaterial {
        hud: image.clone(),
        params: VisorParams::default(),
    });
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        MaterialNode(mat.clone()),
        FocusPolicy::Pass,
    ));
    commands.spawn((
        TagRoot,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        FocusPolicy::Pass,
    ));
    commands.insert_resource(Visor {
        cam,
        image,
        mat,
        size: UVec2::ONE,
        scale: 1.0,
    });
}

/// Before the cameras update: the image the window's size, its scale the
/// window's, and this frame's look and lag.
pub fn fit(
    visor: Option<ResMut<Visor>>,
    lag: Res<HudLag>,
    tier: Res<Tier>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut targets: Query<&mut RenderTarget>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<VisorMaterial>>,
) {
    let (Some(mut visor), Ok(window)) = (visor, windows.single()) else {
        return;
    };
    let size = window.resolution.physical_size().max(UVec2::ONE);
    let scale = window.scale_factor();
    let resized = size != visor.size || scale != visor.scale;
    if resized {
        visor.size = size;
        visor.scale = scale;
        if let Some(image) = images.get_mut(&visor.image) {
            image.resize(Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            });
        }
        if let Ok(mut t) = targets.get_mut(visor.cam) {
            *t = RenderTarget::Image(ImageRenderTarget {
                handle: visor.image.clone(),
                scale_factor: scale,
            });
        }
    }
    let glow = if tier.preset == Some(Quality::Potato) {
        0.0
    } else {
        GLOW
    };
    let px = Vec2::ONE / size.as_vec2();
    let lag = lag.px() * scale * px;
    let params = VisorParams {
        look: Vec4::new(BEND, SPLIT, glow, GLOW_PX * scale),
        lag: Vec4::new(lag.x, lag.y, size.x as f32 / size.y as f32, LINES),
        px: Vec4::new(px.x, px.y, 0.0, 0.0),
    };
    // Touched on a resize too, so its bind group is built again against
    // the new texture (`render_scale.rs`).
    let same = mats.get(&visor.mat).is_some_and(|m| {
        let (a, b) = (m.params, params);
        a.look == b.look && a.lag == b.lag && a.px == b.px
    });
    if !same || resized {
        if let Some(m) = mats.get_mut(&visor.mat) {
            m.params = params;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lag is the gun's trail, turned to pixels, and never more than
    /// its most.
    #[test]
    fn the_lag_follows_the_trail_and_is_held_in() {
        assert_eq!(HudLag::default().px(), Vec2::ZERO);
        let right = HudLag {
            trail: Vec2::new(0.05, 0.0),
            heave: 0.0,
        };
        assert!(right.px().x > 0.0 && right.px().y == 0.0);
        let up = HudLag {
            trail: Vec2::ZERO,
            heave: 0.01,
        };
        assert!(up.px().y < 0.0, "a lift moves it up the screen");
        let wild = HudLag {
            trail: Vec2::new(5.0, 5.0),
            heave: 1.0,
        };
        assert!(wild.px().length() <= LAG_MAX_PX + 1e-4);
    }
}

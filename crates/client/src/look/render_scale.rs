//! Draw the world below the window's resolution, then the UI over it at
//! full size: the RENDER SCALE row. After Gates'
//! `crates/client/src/render/render_scale.rs` and its
//! `shaders/render_scale.wgsl` (MIT, NOTICE).
//!
//! The eye keeps its projection and its post stack; only its target
//! changes, to an image [`extent`] of the window. A presentation camera
//! draws no world and tonemaps nothing: its one UI node is that image,
//! already a finished frame, stretched over the window, and the HUD and the
//! screens draw over it at the window's own resolution, because the UI
//! goes with it (one default UI camera, never two). At 100% the eye draws
//! straight to the window again, with its own order, and the presentation
//! is gone. The same on all three targets: an image target and a UI
//! material are things WebGL2 draws as well as anything.

use bevy::camera::{visibility::RenderLayers, RenderTarget};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::ui::FocusPolicy;
use bevy::window::PrimaryWindow;

use super::quality::Tier;
use super::rig::EyeCam;

/// Percent of each dimension, not of the pixels: 50% draws a quarter of
/// them.
pub const RENDER_SCALE_MIN: u8 = 50;
pub const RENDER_SCALE_STEP: u8 = 5;
/// The window's own resolution: the eye draws straight to it.
pub const RENDER_SCALE_MAX: u8 = 100;

/// The world image's size for a `window` (physical pixels) at `percent`:
/// never zero on a side.
pub fn extent(window: UVec2, percent: u8) -> UVec2 {
    let percent = u64::from(percent.clamp(RENDER_SCALE_MIN, RENDER_SCALE_MAX));
    let dim = |n| ((u64::from(n) * percent / 100) as u32).max(1);
    UVec2::new(dim(window.x), dim(window.y))
}

/// One click of the row, on its steps and inside its ends.
pub fn step(percent: u8, delta: i32) -> u8 {
    let s = i32::from(RENDER_SCALE_STEP);
    let at = (i32::from(percent) + s / 2) / s * s;
    (at + delta.signum() * s).clamp(i32::from(RENDER_SCALE_MIN), i32::from(RENDER_SCALE_MAX)) as u8
}

/// A percent from a file, on the steps and inside the ends.
pub fn sanitize(percent: u8) -> u8 {
    step(percent, 0)
}

/// The world image as a finished, opaque frame: the atmosphere leaves the
/// sky's alpha at zero between the clouds, and UI blending must not turn
/// those lit pixels into holes in the presentation's clear.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct OpaqueFrame {
    #[texture(0)]
    #[sampler(1)]
    pub image: Handle<Image>,
}

impl UiMaterial for OpaqueFrame {
    fn fragment_shader() -> ShaderRef {
        "shaders/render_scale.wgsl".into()
    }
}

/// The presentation camera and its frame.
#[derive(Component)]
pub struct Presentation;

/// On the eye while it draws to an image: the image, the presentation's
/// two entities, and what to put back at 100%.
#[derive(Component)]
pub struct ScaledSurface {
    pub image: Handle<Image>,
    /// The backdrop's material: touched after a resize, so its bind group
    /// is built again against the new texture (Bevy rebuilds a resized
    /// image's texture, and a material's bind group only when the material
    /// itself changes).
    pub frame: Handle<OpaqueFrame>,
    pub camera: Entity,
    pub backdrop: Entity,
    original_target: RenderTarget,
    original_order: isize,
}

pub fn plugin(app: &mut App) {
    app.add_plugins(UiMaterialPlugin::<OpaqueFrame>::default())
        .add_systems(PostUpdate, apply.before(bevy::camera::CameraUpdateSystems));
}

/// Before the cameras update: a new image size, or a new camera, reaches
/// the projection and the UI's layout the same frame.
#[allow(clippy::type_complexity)]
pub fn apply(
    mut commands: Commands,
    tier: Res<Tier>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut eyes: Query<(Entity, &mut Camera, &mut RenderTarget, Option<&ScaledSurface>), With<EyeCam>>,
    mut images: ResMut<Assets<Image>>,
    mut frames: ResMut<Assets<OpaqueFrame>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let window_size = window.resolution.physical_size();
    if window_size.x == 0 || window_size.y == 0 {
        return;
    }
    let percent = tier.gfx.render_scale.clamp(RENDER_SCALE_MIN, RENDER_SCALE_MAX);
    for (eye, mut camera, mut target, surface) in &mut eyes {
        if percent == RENDER_SCALE_MAX {
            if let Some(surface) = surface {
                *target = surface.original_target.clone();
                camera.order = surface.original_order;
                commands.entity(surface.backdrop).despawn();
                commands.entity(surface.camera).despawn();
                images.remove(&surface.image);
                frames.remove(&surface.frame);
                commands.entity(eye).remove::<ScaledSurface>().insert(IsDefaultUiCamera);
            }
            continue;
        }
        let size = extent(window_size, percent);
        if let Some(surface) = surface {
            // Read first, so a settled frame doesn't mark the image changed
            // and upload it again.
            if images.get(&surface.image).is_some_and(|i| i.size() != size) {
                if let Some(image) = images.get_mut(&surface.image) {
                    image.resize(Extent3d {
                        width: size.x,
                        height: size.y,
                        depth_or_array_layers: 1,
                    });
                }
                // Without this the window shows the old texture, frozen,
                // while the eye draws into the new one.
                let _ = frames.get_mut(&surface.frame);
            }
            continue;
        }
        let mut image = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
        image.sampler = ImageSampler::linear();
        let image = images.add(image);
        let presentation = commands
            .spawn((
                Presentation,
                Camera2d,
                Camera {
                    order: camera.order + 1,
                    ..default()
                },
                target.clone(),
                IsDefaultUiCamera,
                RenderLayers::none(),
                Msaa::Off,
                Tonemapping::None,
                DebandDither::Disabled,
            ))
            .id();
        let frame = frames.add(OpaqueFrame { image: image.clone() });
        let backdrop = commands
            .spawn((
                Presentation,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                MaterialNode(frame.clone()),
                GlobalZIndex(i32::MIN),
                UiTargetCamera(presentation),
                FocusPolicy::Pass,
            ))
            .id();
        // The HUD and the screens go with the frame: one default UI
        // camera, never two (Bevy warns and picks either).
        commands
            .entity(eye)
            .remove::<IsDefaultUiCamera>()
            .insert(ScaledSurface {
                image: image.clone(),
                frame,
                camera: presentation,
                backdrop,
                original_target: target.clone(),
                original_order: camera.order,
            });
        *target = RenderTarget::Image(image.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_image_is_a_share_of_each_side_and_never_empty() {
        assert_eq!(extent(UVec2::new(1920, 1080), 100), UVec2::new(1920, 1080));
        assert_eq!(extent(UVec2::new(1920, 1080), 50), UVec2::new(960, 540));
        assert_eq!(extent(UVec2::new(1920, 1080), 70), UVec2::new(1344, 756));
        assert_eq!(extent(UVec2::new(1, 1), 50), UVec2::new(1, 1));
        assert_eq!(
            extent(UVec2::new(1000, 1000), 10),
            UVec2::new(500, 500),
            "held to the floor"
        );
    }

    #[test]
    fn the_row_steps_on_its_grid_and_stops_at_its_ends() {
        assert_eq!(step(100, 1), 100);
        assert_eq!(step(100, -1), 95);
        assert_eq!(step(50, -1), 50);
        assert_eq!(step(72, 1), 75);
        assert_eq!(sanitize(13), RENDER_SCALE_MIN);
        assert_eq!(sanitize(250), RENDER_SCALE_MAX);
        assert_eq!(sanitize(72), 70);
    }
}

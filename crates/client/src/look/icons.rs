//! Each gun's picture for the HUD, drawn from its own model (`guns.rs`):
//! at startup every gun is stood side-on, far below the world on a layer
//! of its own, in front of a camera of its own that sees it flat
//! (orthographic) and draws it in white on a clear image, once its scene
//! is in. Then the cameras stop and the models go; the images stay, and the
//! HUD tints them its ink (`hud.rs`). No files: the pictures are whatever
//! the models are, so a new model is a new icon. The same on every target
//! (an image target is something WebGL2 draws as well as anything).

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::image::ImageSampler;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::scene::SceneInstanceReady;
use sim::arms;

use super::guns::{Models, GUNS};

/// The layer the studio draws on, which no other camera sees.
pub const LAYER: usize = 29;
/// An icon's size, px, and how much of it the gun's length fills.
pub const ICON_W: u32 = 256;
pub const ICON_H: u32 = 96;
const FILL: f32 = 0.94;
/// Where the studio stands: far under every map, a gun every few metres.
const STUDIO: Vec3 = Vec3::new(0.0, -5000.0, 0.0);
const SPACING_M: f32 = 4.0;
/// Frames drawn with every pipeline built, after a gun's scene is in,
/// before its camera stops; and the most it waits for them.
const FRAMES: u8 = 3;
const STALL_FRAMES: u16 = 900;
/// The most a gun's camera waits for its scene, frames (two minutes at
/// 60): a model that never comes in (a failed load) gives up, its icon
/// blank, rather than keep a camera drawing for the run.
pub(super) const LOAD_STALL_FRAMES: u16 = 7200;

/// Each gun's icon, by arm id (none until it is drawn).
#[derive(Resource, Default)]
pub struct GunIcons {
    by_arm: Vec<Option<Handle<Image>>>,
}

impl GunIcons {
    pub fn get(&self, arm: u8) -> Option<&Handle<Image>> {
        self.by_arm.get(arm as usize).and_then(Option::as_ref)
    }
}

/// One gun in the studio: its camera and its model, how many frames it
/// has waited for the model, and how many its camera has drawn since the
/// model came in.
#[derive(Component)]
pub struct Sitter {
    arm: u8,
    cam: Entity,
    image: Handle<Image>,
    ready: bool,
    loading: u16,
    frames: u8,
    waited: u16,
}

pub fn setup(
    mut commands: Commands,
    models: Res<Models>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let white = mats.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    commands.insert_resource(IconInk(white));
    for (i, (arm, g)) in GUNS.iter().enumerate() {
        let Some(scene) = models.guns.get(*arm as usize).cloned().flatten() else {
            continue;
        };
        let mut image = Image::new_target_texture(ICON_W, ICON_H, TextureFormat::Rgba8UnormSrgb, None);
        image.sampler = ImageSampler::linear();
        let image = images.add(image);
        let at = STUDIO + Vec3::X * SPACING_M * i as f32;
        let len = g.height * g.scale;
        let cam = commands
            .spawn((
                Camera3d::default(),
                Camera {
                    order: -40 - i as isize,
                    clear_color: ClearColorConfig::Custom(Color::NONE),
                    ..default()
                },
                RenderTarget::Image(ImageRenderTarget {
                    handle: image.clone(),
                    scale_factor: 1.0,
                }),
                Projection::Orthographic(OrthographicProjection {
                    scaling_mode: ScalingMode::Fixed {
                        width: len / FILL,
                        height: len / FILL * ICON_H as f32 / ICON_W as f32,
                    },
                    ..OrthographicProjection::default_3d()
                }),
                RenderLayers::layer(LAYER),
                Msaa::Sample4,
                Tonemapping::None,
                DebandDither::Disabled,
                Transform::from_translation(at + Vec3::Z * 3.0).looking_at(at, Vec3::Y),
            ))
            .id();
        // Side-on, its barrel to the right, its top up; its middle on the
        // camera's axis, its grip low in the frame as the gun sits.
        let turn = Quat::from_mat3(&Mat3::from_cols(Vec3::Z, Vec3::X, Vec3::Y));
        let mid = Vec3::new(0.0, g.height * 0.5, 0.0) * g.scale;
        commands
            .spawn((
                Sitter {
                    arm: *arm,
                    cam,
                    image,
                    ready: false,
                    loading: 0,
                    frames: 0,
                    waited: 0,
                },
                SceneRoot(scene),
                Transform::from_translation(at - turn * mid)
                    .with_rotation(turn)
                    .with_scale(Vec3::splat(g.scale)),
            ))
            .observe(dress);
    }
    commands.insert_resource(GunIcons {
        by_arm: vec![None; arms::COUNT],
    });
}

/// The white every icon is drawn in.
#[derive(Resource)]
pub struct IconInk(Handle<StandardMaterial>);

/// A gun's scene is in: every mesh of it onto the studio's layer, in white.
fn dress(
    ev: On<SceneInstanceReady>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    ink: Res<IconInk>,
    mut sitters: Query<&mut Sitter>,
    mut commands: Commands,
) {
    for e in children.iter_descendants(ev.entity) {
        if meshes.contains(e) {
            commands.entity(e).remove::<MeshMaterial3d<StandardMaterial>>().insert((
                MeshMaterial3d(ink.0.clone()),
                RenderLayers::layer(LAYER),
                NotShadowCaster,
            ));
        }
    }
    if let Ok(mut s) = sitters.get_mut(ev.entity) {
        s.ready = true;
    }
}

/// Every frame until the studio is empty: a gun drawn a few frames with no
/// pipeline left compiling (a draw whose pipeline isn't built is skipped:
/// `pipelines.rs`) is done, its camera and model gone, its icon kept.
pub fn develop(
    mut sitters: Query<(Entity, &mut Sitter)>,
    backlog: Res<super::pipelines::PipelineBacklog>,
    mut icons: ResMut<GunIcons>,
    mut commands: Commands,
) {
    let built = backlog.pending() == 0;
    for (e, mut s) in &mut sitters {
        if !s.ready {
            s.loading = s.loading.saturating_add(1);
            if s.loading > LOAD_STALL_FRAMES {
                commands.entity(s.cam).despawn();
                commands.entity(e).despawn();
            }
            continue;
        }
        s.waited = s.waited.saturating_add(1);
        s.frames = if built { s.frames + 1 } else { 0 };
        if s.frames > FRAMES || s.waited > STALL_FRAMES {
            if let Some(slot) = icons.by_arm.get_mut(s.arm as usize) {
                *slot = Some(s.image.clone());
            }
            commands.entity(s.cam).despawn();
            commands.entity(e).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every gun fits its icon side to side, and the studio's guns stand
    /// apart, so no camera sees another's.
    #[test]
    fn every_gun_fits_its_icon_and_stands_alone() {
        for (_, g) in GUNS.iter() {
            let len = g.height * g.scale;
            assert!(len / FILL * ICON_H as f32 / ICON_W as f32 > 0.1, "{}", g.file);
            assert!(len < SPACING_M * 0.5, "{}", g.file);
        }
        assert!(super::super::guns::gun(arms::BR).is_some());
    }
}

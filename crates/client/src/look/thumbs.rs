//! Each piece of gear's picture for its card in the armory (`front.rs`),
//! drawn from its own model as the HUD's gun icons are (`icons.rs`): at
//! startup every piece is stood three-quarter on, far below the world on a
//! layer of its own, lit as Triple T is on the title (`portrait.rs`' key,
//! fill and rim, the world's exposure), in front of a camera of its own
//! drawing into an image, once its scene is in. Then the cameras stop and
//! the models go; the images stay. No files: a new model is a new picture,
//! and a new piece of gear is a line in [`SHOTS`].

use bevy::app::Propagate;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Exposure, ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::ImageSampler;
use bevy::light::{EnvironmentMapLight, NotShadowCaster};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::Msaa;
use bevy::scene::SceneInstanceReady;

use super::guns::Models;

/// The layer the gear's studio draws on, which no other camera sees.
pub const LAYER: usize = 28;
/// A picture's size, px: twice the card's picture (`front.rs`), so it is
/// sharp on a high-density screen.
pub const THUMB_W: u32 = 216;
pub const THUMB_H: u32 = 112;
/// Where the studio stands: far under every map, away from the guns'
/// (`icons.rs`), a piece every few metres.
const STUDIO: Vec3 = Vec3::new(0.0, -5000.0, -200.0);
const SPACING_M: f32 = 6.0;
/// The lens's vertical field of view, degrees.
const FOV_DEG: f32 = 20.0;
/// Frames drawn with every pipeline built, after a piece's scenes are in,
/// before its camera stops; and the most it waits for them.
const FRAMES: u8 = 3;
const STALL_FRAMES: u16 = 900;

/// The slots, as `front.rs`' `GEAR_ROWS` has them.
pub const HEAD: usize = 0;
pub const FEET: usize = 1;

/// A model in a piece's picture.
#[derive(Clone, Copy)]
enum Part {
    Shades,
    BootLeft,
    BootRight,
}

/// One piece's picture: its slot and id (`proto::HEADWEAR`,
/// `proto::FOOTWEAR`), its models where they stand (metres, the models'
/// own frame, turned about +Y, radians), the point the lens looks at,
/// the way it looks from and how far.
struct Shot {
    slot: usize,
    g: u8,
    parts: &'static [(Part, Vec3, f32)],
    aim: Vec3,
    from: Vec3,
    dist: f32,
}

/// The shades: three-quarter on from above, the lenses toward the lens
/// (the model's +Z). The boots: the pair side by side, toes out a little.
const SHOTS: [Shot; 2] = [
    Shot {
        slot: HEAD,
        g: 1,
        parts: &[(Part::Shades, Vec3::ZERO, 0.0)],
        aim: Vec3::new(0.0, 0.0, -0.17),
        from: Vec3::new(-0.62, 0.38, 1.0),
        dist: 1.45,
    },
    Shot {
        slot: FEET,
        g: 1,
        parts: &[
            (Part::BootLeft, Vec3::new(0.17, 0.0, 0.0), -0.2),
            (Part::BootRight, Vec3::new(-0.17, 0.0, 0.0), 0.2),
        ],
        aim: Vec3::new(0.0, 0.17, 0.0),
        from: Vec3::new(-0.62, 0.42, 1.0),
        dist: 2.1,
    },
];

/// Each piece's picture, by slot and id (none for no gear, or one with no
/// model yet). Handed out from the start: a card shows it as it is drawn.
#[derive(Resource, Default)]
pub struct GearPics {
    pics: Vec<(usize, u8, Handle<Image>)>,
}

impl GearPics {
    pub fn get(&self, slot: usize, g: u8) -> Option<Handle<Image>> {
        self.pics.iter().find(|(s, i, _)| *s == slot && *i == g).map(|(.., h)| h.clone())
    }
}

/// One piece in the studio: its camera, its image, and how many of its
/// models are in and how many frames its camera has drawn since.
#[derive(Component)]
pub struct Sitter {
    cam: Entity,
    parts: u8,
    ready: u8,
    frames: u8,
    waited: u16,
}

/// A model of a sitter's (its root).
#[derive(Component)]
struct PartOf(Entity);

pub fn setup(mut commands: Commands, models: Res<Models>, mut images: ResMut<Assets<Image>>) {
    let layer = RenderLayers::layer(LAYER);
    let fill = images.add(super::fill::cubemap(super::fill::FILL_FACE));
    let mut pics = GearPics::default();
    for (i, shot) in SHOTS.iter().enumerate() {
        let mut image = Image::new_target_texture(THUMB_W, THUMB_H, TextureFormat::Rgba8UnormSrgb, None);
        image.sampler = ImageSampler::linear();
        let image = images.add(image);
        let at = STUDIO + Vec3::X * SPACING_M * i as f32;
        let aim = at + shot.aim;
        let cam = commands
            .spawn((
                Camera3d::default(),
                Camera {
                    order: -80 - i as isize,
                    clear_color: ClearColorConfig::Custom(Color::NONE),
                    ..default()
                },
                RenderTarget::Image(ImageRenderTarget {
                    handle: image.clone(),
                    scale_factor: 1.0,
                }),
                Projection::Perspective(PerspectiveProjection {
                    fov: FOV_DEG.to_radians(),
                    near: 0.05,
                    far: SPACING_M,
                    ..default()
                }),
                Exposure {
                    ev100: super::rig::DAY_EV100,
                },
                Tonemapping::TonyMcMapface,
                EnvironmentMapLight {
                    diffuse_map: fill.clone(),
                    specular_map: fill.clone(),
                    intensity: super::fill::peak_lux(),
                    rotation: Quat::IDENTITY,
                    affects_lightmapped_mesh_diffuse: false,
                },
                Msaa::Sample4,
                Transform::from_translation(aim + shot.from.normalize() * shot.dist).looking_at(aim, Vec3::Y),
                layer.clone(),
            ))
            .id();
        let root = commands
            .spawn((
                Transform::from_translation(at),
                Visibility::default(),
                Propagate(layer.clone()),
            ))
            .id();
        commands.entity(root).insert(Sitter {
            cam,
            parts: shot.parts.len() as u8,
            ready: 0,
            frames: 0,
            waited: 0,
        });
        for &(part, off, yaw) in shot.parts {
            let scene = match part {
                Part::Shades => &models.shades,
                Part::BootLeft => &models.boot_left,
                Part::BootRight => &models.boot_right,
            };
            commands
                .spawn((
                    SceneRoot(scene.clone()),
                    Transform::from_translation(off).with_rotation(Quat::from_rotation_y(yaw)),
                    PartOf(root),
                    ChildOf(root),
                ))
                .observe(dress);
        }
        pics.pics.push((shot.slot, shot.g, image));
    }
    // The studio's lights, as the title's portrait has them.
    let k = 2f32.powf(super::rig::DAY_EV100 - super::portrait::GATES_EV100);
    for (lux, [r, g, b], from) in super::portrait::LIGHTS {
        commands.spawn((
            DirectionalLight {
                illuminance: lux * k,
                color: Color::srgb(r, g, b),
                shadows_enabled: false,
                ..default()
            },
            Transform::from_translation(STUDIO + from).looking_at(STUDIO, Vec3::Y),
            layer.clone(),
        ));
    }
    commands.insert_resource(pics);
}

/// A model's scene is in: it casts no shadow, and its sitter counts it.
fn dress(
    ev: On<SceneInstanceReady>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    parts: Query<&PartOf>,
    mut sitters: Query<&mut Sitter>,
    mut commands: Commands,
) {
    for e in children.iter_descendants(ev.entity) {
        if meshes.contains(e) {
            commands.entity(e).insert(NotShadowCaster);
        }
    }
    if let Ok(PartOf(root)) = parts.get(ev.entity) {
        if let Ok(mut s) = sitters.get_mut(*root) {
            s.ready += 1;
        }
    }
}

/// Every frame until the studio is empty: a piece drawn a few frames with
/// every model in and no pipeline left compiling is done, its camera and
/// models gone, its picture kept.
pub fn develop(
    mut sitters: Query<(Entity, &mut Sitter)>,
    backlog: Res<super::pipelines::PipelineBacklog>,
    mut commands: Commands,
) {
    let built = backlog.pending() == 0;
    for (e, mut s) in &mut sitters {
        if s.ready < s.parts {
            continue;
        }
        s.waited = s.waited.saturating_add(1);
        s.frames = if built { s.frames + 1 } else { 0 };
        if s.frames > FRAMES || s.waited > STALL_FRAMES {
            commands.entity(s.cam).despawn();
            commands.entity(e).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every piece of gear there is has its picture, and the studio's
    /// pieces stand apart, so no camera sees another's.
    #[test]
    fn every_piece_of_gear_has_a_picture() {
        for (slot, list) in [(HEAD, &proto::HEADWEAR[..]), (FEET, &proto::FOOTWEAR[..])] {
            for g in 1..list.len() as u8 {
                assert!(SHOTS.iter().any(|s| s.slot == slot && s.g == g), "{}", list[g as usize]);
            }
        }
        for s in &SHOTS {
            assert!(s.dist < SPACING_M * 0.5);
        }
    }
}

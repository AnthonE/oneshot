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

use super::guns::{Models, LEFT, RIGHT};

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

/// One piece's picture: its slot and id (`proto::HEADWEAR`,
/// `proto::FOOTWEAR`), the point the lens looks at (metres, the model's own
/// frame, for headwear the middle of the eyes), the way it looks from and
/// how far.
struct Shot {
    slot: usize,
    g: u8,
    aim: Vec3,
    from: Vec3,
    dist: f32,
}

/// Headwear, three-quarter on from above, its front toward the lens (the
/// model's +Z).
const fn head(g: u8, aim: Vec3, dist: f32) -> Shot {
    Shot {
        slot: HEAD,
        g,
        aim,
        from: Vec3::new(-0.62, 0.38, 1.0),
        dist,
    }
}

/// The boots: the pair side by side (`PAIR`), toes out a little.
const fn pair(g: u8) -> Shot {
    Shot {
        slot: FEET,
        g,
        aim: Vec3::new(0.0, 0.17, 0.0),
        from: Vec3::new(-0.62, 0.42, 1.0),
        dist: 1.45,
    }
}

/// A pair's boots in a picture: how far out each stands and how far it turns,
/// radians.
const PAIR: [(f32, f32); 2] = [(0.17, -0.2), (-0.17, 0.2)];

const SHOTS: [Shot; 19] = [
    head(1, Vec3::new(0.0, 0.0, -0.17), 1.1),
    head(2, Vec3::new(0.0, 0.0, -0.1), 1.1),
    head(3, Vec3::new(0.0, 0.0, -0.1), 1.1),
    head(4, Vec3::new(0.0, 0.0, -0.1), 1.1),
    head(5, Vec3::new(0.0, 0.0, -0.1), 1.2),
    head(6, Vec3::new(0.0, -0.15, -0.05), 1.3),
    head(7, Vec3::new(0.0, -0.1, -0.1), 1.5),
    head(8, Vec3::new(0.0, -0.1, -0.1), 1.5),
    head(9, Vec3::new(0.0, -0.1, -0.1), 1.8),
    head(10, Vec3::new(0.0, 0.3, -0.25), 1.5),
    head(11, Vec3::new(0.0, 0.3, -0.25), 1.6),
    head(12, Vec3::new(0.0, 0.3, -0.25), 2.0),
    head(13, Vec3::new(0.0, 0.3, -0.25), 1.4),
    pair(1),
    pair(2),
    pair(3),
    pair(4),
    pair(5),
    pair(6),
];

/// Each piece's picture, by slot and id (none for no gear, or one with no
/// model yet). Handed out from the start: a card shows it as it is drawn.
#[derive(Resource, Default)]
pub struct GearPics {
    pics: Vec<(usize, u8, Handle<Image>)>,
}

impl GearPics {
    pub fn get(&self, slot: usize, g: u8) -> Option<Handle<Image>> {
        self.pics
            .iter()
            .find(|(s, i, _)| *s == slot && *i == g)
            .map(|(.., h)| h.clone())
    }
}

/// One piece in the studio: its camera, its image, how many of its models
/// are in, how many frames it has waited for them (at most
/// `icons::LOAD_STALL_FRAMES`: a failed load gives up) and how many its
/// camera has drawn since.
#[derive(Component)]
pub struct Sitter {
    cam: Entity,
    parts: u8,
    ready: u8,
    loading: u16,
    frames: u8,
    waited: u16,
}

/// One of the studio's lights, gone with its last sitter.
#[derive(Component)]
pub struct StudioLight;

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
        // Its models where they stand (metres, the models' own frame, turned
        // about +Y, radians).
        let parts: Vec<(Option<Handle<Scene>>, Vec3, f32)> = match shot.slot {
            HEAD => vec![(models.head_scene(shot.g), Vec3::ZERO, 0.0)],
            _ => [LEFT, RIGHT]
                .into_iter()
                .map(|side| {
                    let (x, yaw) = PAIR[side];
                    (models.boot_scene(shot.g, side), Vec3::new(x, 0.0, 0.0), yaw)
                })
                .collect(),
        };
        commands.entity(root).insert(Sitter {
            cam,
            parts: parts.len() as u8,
            ready: 0,
            loading: 0,
            frames: 0,
            waited: 0,
        });
        for (scene, off, yaw) in parts {
            let Some(scene) = scene else { continue };
            commands
                .spawn((
                    SceneRoot(scene),
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
            StudioLight,
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
/// models gone, its picture kept. The lights go with the last of them.
pub fn develop(
    mut sitters: Query<(Entity, &mut Sitter)>,
    lights: Query<Entity, With<StudioLight>>,
    backlog: Res<super::pipelines::PipelineBacklog>,
    mut commands: Commands,
) {
    let built = backlog.pending() == 0;
    let mut left = 0;
    for (e, mut s) in &mut sitters {
        let done = if s.ready < s.parts {
            s.loading = s.loading.saturating_add(1);
            s.loading > super::icons::LOAD_STALL_FRAMES
        } else {
            s.waited = s.waited.saturating_add(1);
            s.frames = if built { s.frames + 1 } else { 0 };
            s.frames > FRAMES || s.waited > STALL_FRAMES
        };
        if done {
            commands.entity(s.cam).despawn();
            commands.entity(e).despawn();
        } else {
            left += 1;
        }
    }
    if left == 0 {
        for l in &lights {
            commands.entity(l).despawn();
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

    /// Every piece of gear there is has its file (`guns::load`, by its name).
    #[test]
    fn every_piece_of_gear_has_a_model() {
        let at = |stem: String| {
            let path = format!("{}/assets/models/{stem}.glb", env!("CARGO_MANIFEST_DIR"));
            assert!(std::path::Path::new(&path).exists(), "{path}");
        };
        proto::HEADWEAR[1..].iter().for_each(|name| at(name.to_string()));
        for name in &proto::FOOTWEAR[1..] {
            at(format!("{name}-left"));
            at(format!("{name}-right"));
        }
    }
}

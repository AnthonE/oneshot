//! Triple T on the title, beside the play panel, after Gates'
//! `crates/client/src/render/portrait.rs` (MIT, NOTICE): the body every
//! fighter wears (`bodies.rs`), in the skin picked in the panel, holding a
//! rifle and idling (a slow look round: his feet stay planted while his
//! torso turns, `anim.rs`), on his own render layer, filmed by his own
//! camera into an image the title shows ([`view`]).
//!
//! He stands far under every map, on [`LAYER`], which only his camera and
//! his lights are on (his scene's descendants inherit the layer through
//! Bevy's `Propagate`), so the world's camera never draws him and the
//! level's sun never lights him. His camera renders only while a
//! [`PortraitView`] node exists. Exposed as the world is, his lights are
//! Gates' key, fill and rim at the world's exposure, and his shade is the
//! world's sky fill: he looks on the title as he does in a match. The same
//! on the desktop, WebGPU and WebGL2: a second camera into an image is a
//! render pass any of them draws.
//!
//! In the armory he turns round to show what is being picked: his back
//! while the emblem is ([`Portrait::show_back`]), and a drag across him
//! turns him by hand ([`drag`]).
//!
//! **He decides nothing**: what he wears is the panel's pick, and a skin
//! the wallet doesn't own is still only what the server lets you wear.

use bevy::app::Propagate;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, Exposure, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::EnvironmentMapLight;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::Msaa;
use client_net::core::Drawn;
use sim::geom::V3;

use super::guns::Models;

/// The render layer only the portrait's camera, lights and body are on.
pub const LAYER: usize = 7;

/// The picture's smallest size, px: an upright figure, 2:3. It is drawn at
/// the size the title shows it ([`fit`]), from this up to [`H_MAX_PX`].
pub const W_PX: u32 = 400;
pub const H_PX: u32 = 600;
/// The tallest it is drawn, px: a 4K window's [`VIEW_VH`] fits.
pub const H_MAX_PX: u32 = 1260;
/// How tall the title shows him, a share of the window's height
/// (`front.rs`' stage), percent.
pub const VIEW_VH: f32 = 56.0;

/// Where he stands: well under every map, out of every world query's way.
const AT: Vec3 = Vec3::new(0.0, -400.0, 0.0);

/// A three-quarter turn toward the picture's left, radians: a figure
/// square to the lens reads as a mugshot.
const TURN: f32 = 0.42;
/// His look round: how far either way and how long a sweep takes, for the
/// yaw and the pitch (radians, seconds). Under `anim::IDLE_TWIST_MAX`, so his
/// feet stay where they are.
const SWAY_YAW: f32 = 0.3;
const SWAY_YAW_S: f32 = 9.0;
const SWAY_PITCH: f32 = 0.12;
const SWAY_PITCH_S: f32 = 6.5;
/// His turn to show his back (the emblem), radians: a little short of a
/// half turn, so the emblem reads three-quarter on, as his front does.
const BACK: f32 = core::f32::consts::PI - 2.0 * TURN;
/// How fast he turns to what is asked (1/s): about a second round, his feet
/// stepping under him (`anim.rs`).
const TURN_RATE: f32 = 3.5;
/// A drag across him turns him this far a pixel, radians.
const DRAG_PER_PX: f32 = 0.012;

/// Vertical field of view, degrees, and the distance it frames the whole
/// 1.8 m body from, and where the lens looks, metres above the feet.
const FOV_DEG: f32 = 26.0;
const DIST_M: f32 = 4.9;
const AIM_Y_M: f32 = 0.95;

/// His side's colour: the title's accent is blue.
const TEAM: u8 = 1;
/// What he holds.
const ARM: u8 = sim::arms::AR;

/// The exposure Gates' portrait lights were set under (Bevy's default),
/// and those lights, lux there: key high on the camera's left, a cool weak
/// fill from the right, a warm rim from behind that pulls him off a dark
/// title. Scaled to the world's exposure here.
const GATES_EV100: f32 = 9.7;
const LIGHTS: [(f32, [f32; 3], Vec3); 3] = [
    (3_400.0, [1.0, 1.0, 1.0], Vec3::new(-0.7, 1.0, 0.8)),
    (900.0, [0.80, 0.86, 1.0], Vec3::new(1.0, 0.3, 0.5)),
    (2_400.0, [1.0, 0.86, 0.66], Vec3::new(0.4, 0.7, -1.0)),
];

#[derive(Resource)]
pub struct Portrait {
    pub image: Handle<Image>,
    /// His body and the look it was made in.
    body: Option<(Entity, proto::Look)>,
    /// He turns to show his back (the armory's emblem).
    back: bool,
    /// How far a drag has turned him, and how far he is turned now, past
    /// his three-quarter (radians).
    dragged: f32,
    turned: f32,
}

impl Portrait {
    /// Turn to show his back (the emblem) or his front; either undoes a drag.
    pub fn show_back(&mut self, back: bool) {
        if self.back != back {
            self.back = back;
            self.dragged = 0.0;
        }
    }
}

#[derive(Component)]
pub struct PortraitCam;

#[derive(Component)]
pub struct PortraitBody;

/// A UI node showing the picture. While one exists the camera renders.
#[derive(Component)]
pub struct PortraitView;

/// The picture as a UI node, `height` tall at the picture's aspect.
pub fn view(portrait: &Portrait, height: Val) -> impl Bundle {
    (
        PortraitView,
        ImageNode::new(portrait.image.clone()),
        // A drag across him turns him ([`drag`]).
        Interaction::default(),
        Node {
            height,
            aspect_ratio: Some(W_PX as f32 / H_PX as f32),
            flex_shrink: 0.0,
            ..default()
        },
    )
}

/// The target, the camera and the lights (`Startup`). The body waits for
/// the models ([`stand`]).
pub fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = images.add(Image::new_target_texture(
        W_PX,
        H_PX,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    let fill = images.add(super::fill::cubemap(super::fill::FILL_FACE));
    let layer = RenderLayers::layer(LAYER);
    let aim = AT + Vec3::Y * AIM_Y_M;
    commands.spawn((
        PortraitCam,
        Camera3d::default(),
        Camera {
            // Before the world's camera, so the UI reads this frame's picture.
            order: -10,
            is_active: false,
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        RenderTarget::Image(image.clone().into()),
        Projection::Perspective(PerspectiveProjection {
            fov: FOV_DEG.to_radians(),
            near: 0.1,
            far: 20.0,
            ..default()
        }),
        Exposure {
            ev100: super::rig::DAY_EV100,
        },
        Tonemapping::TonyMcMapface,
        EnvironmentMapLight {
            diffuse_map: fill.clone(),
            specular_map: fill,
            intensity: super::fill::peak_lux(),
            rotation: Quat::IDENTITY,
            affects_lightmapped_mesh_diffuse: false,
        },
        AmbientLight {
            brightness: 0.0,
            ..default()
        },
        Msaa::Sample4,
        Transform::from_translation(aim + Vec3::Z * DIST_M).looking_at(aim, Vec3::Y),
        layer.clone(),
    ));
    let k = 2f32.powf(super::rig::DAY_EV100 - GATES_EV100);
    for (lux, [r, g, b], from) in LIGHTS {
        commands.spawn((
            DirectionalLight {
                illuminance: lux * k,
                color: Color::srgb(r, g, b),
                shadows_enabled: false,
                ..default()
            },
            Transform::from_translation(AT + from).looking_at(AT, Vec3::Y),
            layer.clone(),
        ));
    }
    commands.insert_resource(Portrait {
        image,
        body: None,
        back: false,
        dragged: 0.0,
        turned: 0.0,
    });
}

/// While the title shows him: his body in the panel's skin (made again
/// when the skin changes), standing, holding his rifle, looking round.
#[allow(clippy::too_many_arguments)]
pub fn stand(
    mut commands: Commands,
    mut portrait: ResMut<Portrait>,
    player: Res<crate::settings::Player>,
    models: Option<Res<Models>>,
    views: Query<(), With<PortraitView>>,
    time: Res<Time>,
    mut bodies: Query<&mut super::bodies::Body>,
    mut at: Query<&mut Transform>,
) {
    let Some(models) = models else {
        return;
    };
    if views.is_empty() {
        // Off the title: no body posed, animated and drawn into an image
        // nobody shows; made again when the title is back.
        if let Some((e, _)) = portrait.body.take() {
            commands.entity(e).despawn();
        }
        portrait.back = false;
        portrait.dragged = 0.0;
        portrait.turned = 0.0;
        return;
    }
    let look = player.look();
    let root = match portrait.body {
        Some((e, l)) if l == look => e,
        had => {
            if let Some((old, _)) = had {
                commands.entity(old).despawn();
            }
            let e = super::bodies::spawn(&mut commands, TEAM, look);
            commands
                .entity(e)
                .insert((PortraitBody, Propagate(RenderLayers::layer(LAYER))));
            portrait.body = Some((e, look));
            return;
        }
    };
    let want = if portrait.back { BACK } else { 0.0 } + portrait.dragged;
    let ease = 1.0 - (-TURN_RATE * time.delta_secs()).exp();
    portrait.turned += (want - portrait.turned) * ease;
    let t = time.elapsed_secs();
    let wave = |period: f32| (t * core::f32::consts::TAU / period).sin();
    let d = Drawn {
        alive: true,
        grounded: true,
        pos: V3::new(AT.x, AT.y, AT.z),
        // Facing the lens (+Z), turned toward the picture's left.
        yaw: core::f32::consts::FRAC_PI_2 + TURN + portrait.turned + SWAY_YAW * wave(SWAY_YAW_S),
        pitch: SWAY_PITCH * wave(SWAY_PITCH_S),
        arm: ARM,
        team: TEAM,
        ..Drawn::default()
    };
    super::bodies::pose(
        &mut commands,
        &models,
        &d,
        None,
        root,
        &mut bodies,
        &mut at,
        time.delta_secs(),
    );
}

/// A drag across the picture turns him: held down on him, the pointer's
/// sideways travel.
pub fn drag(
    views: Query<&Interaction, With<PortraitView>>,
    motion: Res<bevy::input::mouse::AccumulatedMouseMotion>,
    portrait: Option<ResMut<Portrait>>,
) {
    let Some(mut portrait) = portrait else {
        return;
    };
    if motion.delta.x != 0.0 && views.iter().any(|i| *i == Interaction::Pressed) {
        portrait.dragged += motion.delta.x * DRAG_PER_PX;
    }
}

/// The picture's pixels for a window `physical_h` tall: as many as it is
/// shown at, within [`H_PX`]..=[`H_MAX_PX`], at 2:3.
pub fn size_for(physical_h: u32) -> (u32, u32) {
    let h = ((physical_h as f32 * VIEW_VH / 100.0).round() as u32).clamp(H_PX, H_MAX_PX);
    // A multiple of three, so the 2:3 width is whole.
    let h = h - h % 3;
    (h * W_PX / H_PX, h)
}

/// The picture drawn at the size it is shown, so a tall window doesn't
/// show it stretched and soft: resized when the window's height moves.
pub fn fit(
    windows: Query<&Window, (With<bevy::window::PrimaryWindow>, Changed<Window>)>,
    portrait: Option<Res<Portrait>>,
    mut images: ResMut<Assets<Image>>,
) {
    let (Ok(w), Some(portrait)) = (windows.single(), portrait) else {
        return;
    };
    let (pw, ph) = size_for(w.physical_height());
    let Some(img) = images.get(&portrait.image) else {
        return;
    };
    if img.width() == pw && img.height() == ph {
        return;
    }
    if let Some(img) = images.get_mut(&portrait.image) {
        img.resize(bevy::render::render_resource::Extent3d {
            width: pw,
            height: ph,
            depth_or_array_layers: 1,
        });
    }
}

/// The camera on while something shows the picture.
pub fn activate(views: Query<(), With<PortraitView>>, mut cams: Query<&mut Camera, With<PortraitCam>>) {
    let on = !views.is_empty();
    for mut cam in &mut cams {
        if cam.is_active != on {
            cam.is_active = on;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drawn at the size shown, within its bounds, at its aspect.
    #[test]
    fn the_picture_is_drawn_at_the_size_shown() {
        assert_eq!(size_for(720), (W_PX, H_PX));
        let (w, h) = size_for(1440);
        assert_eq!(h, 804);
        assert_eq!(w * H_PX, h * W_PX);
        assert_eq!(size_for(4320).1, H_MAX_PX);
    }
}

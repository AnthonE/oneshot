//! The game client: Bevy on the desktop and in the browser
//! (docs/DESIGN.md §2, §16 milestone 12). **It draws, it never decides**:
//! everything it shows comes from client-net's session (the server's
//! snapshots and the prediction of our own fighter and vehicle), and all it
//! sends back is the controls.
//!
//! C1, the skeleton: the level from the sim's map, fighters and vehicles as
//! plain shapes, a first-person camera (a chase camera at the wheel), the
//! controls, and a text HUD. Gates' lighting, sky, terrain and grass come
//! next (docs/RIP.md §9).

pub mod level;
pub mod link;

use std::collections::BTreeMap;

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use sim::consts::{CROUCH_EYE, EYE, PITCH_MAX, VEH_H};
use sim::input::*;

use crate::link::Link;

/// The connection, on the app's own thread (a browser's isn't `Send`).
pub struct LinkRes(pub Box<dyn Link>);

/// Where we look, in radians: ours, never the server's (aim is drawn with
/// no round trip).
#[derive(Resource, Default)]
struct Look {
    yaw: f32,
    pitch: f32,
}

/// The welcome this level was built for.
#[derive(Resource, Default)]
struct Built(u32);

/// Drawn things by their slot.
#[derive(Resource, Default)]
struct Drawn {
    fighters: BTreeMap<u8, Entity>,
    vehicles: BTreeMap<u8, Entity>,
}

#[derive(Component)]
struct Hud;

#[derive(Component)]
struct Eye;

/// Radians per pixel of mouse.
const SENS: f32 = 0.0022;

pub fn app(link: Box<dyn Link>) -> App {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "oneshot".into(),
            canvas: Some("#game".into()),
            fit_canvas_to_parent: true,
            ..default()
        }),
        ..default()
    }))
    .insert_non_send_resource(LinkRes(link))
    .insert_resource(ClearColor(Color::srgb(0.62, 0.77, 0.91)))
    .init_resource::<Look>()
    .init_resource::<Built>()
    .init_resource::<Drawn>()
    .add_systems(Startup, setup)
    .add_systems(
        Update,
        (grab, pump, rebuild, draw_fighters, draw_vehicles, view, hud).chain(),
    );
    app
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Eye,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 78f32.to_radians(),
            near: 0.05,
            far: 600.0,
            ..default()
        }),
        DistanceFog {
            color: Color::srgb(0.62, 0.77, 0.91),
            falloff: FogFalloff::Linear {
                start: 90.0,
                end: 320.0,
            },
            ..default()
        },
        Transform::from_xyz(0.0, 2.0, 0.0),
    ));
    commands.spawn((
        Hud,
        Text::new("connecting..."),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            bottom: Val::Px(12.0),
            ..default()
        },
    ));
}

/// A click takes the mouse; Escape gives it back.
fn grab(
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let Ok(mut c) = cursor.single_mut() else {
        return;
    };
    if mouse.just_pressed(MouseButton::Left) {
        c.grab_mode = CursorGrabMode::Locked;
        c.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        c.grab_mode = CursorGrabMode::None;
        c.visible = true;
    }
}

/// The controls into an input, and the session's frame.
fn pump(
    mut link: NonSendMut<LinkRes>,
    mut look: ResMut<Look>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    cursor: Query<&CursorOptions, With<PrimaryWindow>>,
) {
    let grabbed = cursor.single().is_ok_and(|c| c.grab_mode != CursorGrabMode::None);
    if grabbed {
        look.yaw += motion.delta.x * SENS;
        let max = PITCH_MAX as f32 / 65536.0 * core::f32::consts::TAU;
        look.pitch = (look.pitch - motion.delta.y * SENS).clamp(-max, max);
    }
    let axis = |plus: KeyCode, minus: KeyCode| (keys.pressed(plus) as i32 - keys.pressed(minus) as i32) * 127;
    let mut buttons = 0;
    for (on, b) in [
        (grabbed && mouse.pressed(MouseButton::Left), FIRE),
        (mouse.pressed(MouseButton::Right), ZOOM),
        (keys.pressed(KeyCode::Space), JUMP),
        (
            keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::KeyC),
            CROUCH,
        ),
        (keys.pressed(KeyCode::KeyR), RELOAD),
        (keys.pressed(KeyCode::KeyQ), SWAP),
        (keys.pressed(KeyCode::KeyF), MELEE),
        (keys.pressed(KeyCode::KeyG), GRENADE),
        (keys.pressed(KeyCode::KeyE), USE),
    ] {
        if on {
            buttons |= b;
        }
    }
    let turn = core::f32::consts::TAU;
    let input = Input {
        buttons,
        yaw: ((look.yaw / turn).rem_euclid(1.0) * 65536.0) as u32 as u16,
        pitch: (look.pitch / turn * 65536.0) as i16,
        mx: axis(KeyCode::KeyD, KeyCode::KeyA) as i8,
        mz: axis(KeyCode::KeyW, KeyCode::KeyS) as i8,
    };
    link.0.update(input);
}

/// A new welcome: the map it names.
fn rebuild(
    link: NonSend<LinkRes>,
    mut built: ResMut<Built>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    old: Query<Entity, With<level::LevelPart>>,
) {
    let s = link.0.session();
    if !s.ready || s.welcomes == built.0 {
        return;
    }
    built.0 = s.welcomes;
    level::build(&mut commands, &mut meshes, &mut mats, &old, s.map_id);
}

fn team_color(team: u8) -> Color {
    if team & 1 == 0 {
        Color::srgb(0.85, 0.27, 0.23)
    } else {
        Color::srgb(0.23, 0.48, 0.85)
    }
}

/// Everyone else, where client-net interpolated them.
fn draw_fighters(
    link: NonSend<LinkRes>,
    mut drawn: ResMut<Drawn>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut at: Query<&mut Transform>,
) {
    let now = link.0.now_ms();
    let others = link.0.session().core.others(now);
    let keep: Vec<u8> = others.iter().map(|d| d.slot).collect();
    for d in &others {
        let y = if d.crouched { 0.65 } else { 0.9 };
        let t = Transform::from_xyz(d.pos.x, d.pos.y + y, d.pos.z)
            .with_rotation(Quat::from_rotation_y(-d.yaw))
            .with_scale(Vec3::new(1.0, if d.crouched { 0.72 } else { 1.0 }, 1.0));
        match drawn.fighters.get(&d.slot) {
            Some(&e) => {
                if let Ok(mut tr) = at.get_mut(e) {
                    *tr = t;
                }
            }
            None => {
                let e = commands
                    .spawn((
                        Mesh3d(meshes.add(Capsule3d::new(0.35, 1.1))),
                        MeshMaterial3d(mats.add(StandardMaterial {
                            base_color: team_color(d.team),
                            perceptual_roughness: 0.6,
                            ..default()
                        })),
                        t,
                    ))
                    .id();
                drawn.fighters.insert(d.slot, e);
            }
        }
    }
    drawn.fighters.retain(|slot, e| {
        let k = keep.contains(slot);
        if !k {
            commands.entity(*e).despawn();
        }
        k
    });
}

/// The vehicles: ours as predicted, the rest interpolated.
fn draw_vehicles(
    link: NonSend<LinkRes>,
    mut drawn: ResMut<Drawn>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut at: Query<&mut Transform>,
) {
    let now = link.0.now_ms();
    let core = &link.0.session().core;
    let mut list: Vec<(u8, Vec3, f32)> = core
        .vehicles(now)
        .iter()
        .map(|v| (v.slot, Vec3::new(v.pos.x, v.pos.y, v.pos.z), v.yaw))
        .collect();
    if let (Some((_, p, yaw)), Some((i, _))) = (core.own_vehicle(), core.own().0.riding()) {
        list.push((i as u8, Vec3::new(p.x, p.y, p.z), yaw));
    }
    for &(slot, p, yaw) in &list {
        let t = Transform::from_translation(p).with_rotation(Quat::from_rotation_y(-yaw));
        match drawn.vehicles.get(&slot) {
            Some(&e) => {
                if let Ok(mut tr) = at.get_mut(e) {
                    *tr = t;
                }
            }
            None => {
                let hull = mats.add(StandardMaterial {
                    base_color: Color::srgb(0.36, 0.42, 0.29),
                    perceptual_roughness: 0.6,
                    metallic: 0.3,
                    ..default()
                });
                let dark = mats.add(StandardMaterial {
                    base_color: Color::srgb(0.14, 0.15, 0.17),
                    perceptual_roughness: 0.8,
                    ..default()
                });
                let e = commands
                    .spawn((t, Visibility::default()))
                    .with_children(|c| {
                        c.spawn((
                            Mesh3d(meshes.add(Cuboid::new(4.4, 0.7, 2.2))),
                            MeshMaterial3d(hull.clone()),
                            Transform::from_xyz(0.0, 0.75, 0.0),
                        ));
                        c.spawn((
                            Mesh3d(meshes.add(Cuboid::new(0.12, 0.9, 2.2))),
                            MeshMaterial3d(dark.clone()),
                            Transform::from_xyz(-0.9, 1.55, 0.0),
                        ));
                        for (x, z) in [(1.5, 1.05), (1.5, -1.05), (-1.5, 1.05), (-1.5, -1.05)] {
                            c.spawn((
                                Mesh3d(meshes.add(Cylinder::new(0.42, 0.35))),
                                MeshMaterial3d(dark.clone()),
                                Transform::from_xyz(x, 0.42, z)
                                    .with_rotation(Quat::from_rotation_x(core::f32::consts::FRAC_PI_2)),
                            ));
                        }
                    })
                    .id();
                drawn.vehicles.insert(slot, e);
            }
        }
    }
    drawn.vehicles.retain(|slot, e| {
        let k = list.iter().any(|v| v.0 == *slot);
        if !k {
            commands.entity(*e).despawn();
        }
        k
    });
}

/// From our eye, or behind our vehicle at the wheel.
fn view(link: NonSend<LinkRes>, look: Res<Look>, mut cam: Query<&mut Transform, With<Eye>>) {
    let Ok(mut t) = cam.single_mut() else {
        return;
    };
    let (f, p) = link.0.session().core.own();
    let dir = Vec3::new(
        look.pitch.cos() * look.yaw.cos(),
        look.pitch.sin(),
        look.pitch.cos() * look.yaw.sin(),
    );
    let at = Vec3::new(p.x, p.y, p.z);
    if f.riding().is_some_and(|(_, seat)| seat == sim::vehicle::DRIVER) {
        let eye = at - dir * 7.5 + Vec3::Y * (VEH_H + 2.2);
        *t = Transform::from_translation(eye).looking_at(at + Vec3::Y * VEH_H, Vec3::Y);
    } else {
        let h = if f.body.crouched { CROUCH_EYE } else { EYE };
        let eye = at + Vec3::Y * h;
        *t = Transform::from_translation(eye).looking_at(eye + dir, Vec3::Y);
    }
}

fn hud(link: NonSend<LinkRes>, mut text: Query<&mut Text, With<Hud>>) {
    let Ok(mut t) = text.single_mut() else {
        return;
    };
    let s = link.0.session();
    if !s.ready {
        **t = "connecting...".into();
        return;
    }
    let (f, _) = s.core.own();
    let g = f.gun();
    let pt = sim::consts::PT;
    let name = sim::maps::ALL.iter().find(|d| d.id == s.map_id).map_or("", |d| d.name);
    let seat = match f.riding() {
        Some((_, 0)) => "  |  driving (E to get out)",
        Some(_) => "  |  riding",
        None => "",
    };
    **t = format!(
        "{name}  |  shield {}  health {}  |  {} {} / {}  |  K {} D {}{seat}",
        f.shield / pt,
        f.health / pt,
        sim::arms::arm(g.arm).name,
        g.mag,
        g.reserve,
        f.kills,
        f.deaths,
    );
}

#[cfg(target_arch = "wasm32")]
mod web_start {
    use wasm_bindgen::prelude::*;

    /// The page's entry: connect (as the three.js page does) and run.
    #[wasm_bindgen]
    pub async fn play(addr: String, hash: String, name: String, mode: u8) -> Result<(), JsValue> {
        let c = client_net::web::Client::connect(addr, hash, name, String::new(), mode, 0).await?;
        super::app(Box::new(super::link::Web(c))).run();
        Ok(())
    }
}

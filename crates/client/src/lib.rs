//! The game client: Bevy on the desktop and in the browser
//! (docs/DESIGN.md §2, §16 milestone 12). **It draws, it never decides**:
//! everything it shows comes from client-net's session (the server's
//! snapshots and the prediction of our own fighter and vehicle), and all it
//! sends back is the controls.
//!
//! The level from the sim's map in Gates' light, sky, ground and grass
//! (docs/RIP.md §9); Triple T bodies with guns in hand; the gun in view,
//! effects and sound; a first-person camera (a chase camera at the wheel,
//! the kill cam when dead); the three.js page's controls; the HUD. In a
//! browser the page (`crates/client/web/index.html`) has the menu and calls `play()`.

pub mod hud;
pub mod level;
pub mod link;
pub mod look;
pub mod pose;
pub mod sound;

use std::collections::BTreeMap;

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
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
pub struct Look {
    yaw: f32,
    pitch: f32,
    /// The zoom button is held.
    pub zoom: bool,
    /// The trigger counts: not still held from the click that took the
    /// mouse (that click only takes it, as on the page).
    armed: bool,
}

/// The welcome this level was built for.
#[derive(Resource, Default)]
struct Built(u32);

/// Drawn things by their slot (a fighter with the side and skin its body
/// was made in).
#[derive(Resource, Default)]
struct Drawn {
    fighters: BTreeMap<u8, (Entity, u8, u8)>,
    vehicles: BTreeMap<u8, Entity>,
}

#[derive(Component)]
pub struct Eye;

/// Test driving, inputs only (`?autoplay=1`, `--autoplay`): walk the map's
/// roam route until fog shows an enemy, then close in, aim and fire. After
/// the three.js page's autopilot; it sends what a player's hands could.
#[derive(Resource, Default)]
pub struct Autoplay {
    pub on: bool,
    leg: usize,
    frames: u64,
    /// Where we were at the last check, and when (ms): pushing ahead and
    /// not getting anywhere is stuck.
    was: (f32, f32),
    checked: f64,
    /// Walking on: the next roam point at this time (ms).
    next_leg: f64,
    /// Stuck chasing someone fog shows behind a wall: the route instead
    /// until this time (ms).
    detour: f64,
}

/// How often the autopilot checks it is getting somewhere, how far counts,
/// how long it walks the route when it isn't, and how long a leg may take.
const AUTO_CHECK_MS: f64 = 1500.0;
const AUTO_MOVED_M: f32 = 1.0;
const AUTO_DETOUR_MS: f64 = 4000.0;
const AUTO_LEG_MS: f64 = 20000.0;
/// Where the autopilot walks on a map with no roam points (Proving Grounds):
/// round its open lanes, as the three.js page's did.
const AUTO_LANES: [(f32, f32); 6] = [
    (-15.0, 8.0),
    (15.0, 8.0),
    (21.0, 0.0),
    (15.0, -8.0),
    (-15.0, -8.0),
    (-21.0, 0.0),
];

/// Stream messages that came before the app ran (the page waited for the
/// welcome): the HUD reads them first.
#[derive(Resource, Default)]
pub struct Early(pub Vec<proto::Msg>);

/// Radians per pixel of mouse; zoomed, this much of it.
const SENS: f32 = 0.0022;
const ZOOM_SENS: f32 = 0.45;

/// The keys, as the three.js page binds them (the three.js page's input.ts, after
/// Fireplace's arena). The wheel swaps too.
const KEYS: [(KeyCode, u16); 11] = [
    (KeyCode::Space, JUMP),
    (KeyCode::ControlLeft, CROUCH),
    (KeyCode::KeyC, CROUCH),
    (KeyCode::KeyQ, MELEE),
    (KeyCode::KeyV, MELEE),
    (KeyCode::KeyG, GRENADE),
    (KeyCode::KeyR, RELOAD),
    (KeyCode::KeyX, SWAP),
    (KeyCode::Digit1, SWAP),
    (KeyCode::Digit2, SWAP),
    (KeyCode::KeyE, USE),
];

/// How the app runs: `autoplay` drives it (tests); `lite` draws with no
/// shadows or SMAA, for a software GPU (the browser gates on SwiftShader;
/// the page also halves its canvas).
#[derive(Clone, Copy, Default, Resource)]
pub struct Opts {
    pub autoplay: bool,
    pub lite: bool,
    /// No server: one fighter posed for a picture (`pose::Pose`).
    pub pose: Option<pose::Pose>,
    /// Everyone else as a capsule in the side's colour, not Triple T: a
    /// software GPU can't skin a room of them (the gate, eight seats on).
    pub capsules: bool,
}

impl Opts {
    /// From a comma list, as the page and the command line give it.
    pub fn parse(s: &str) -> Opts {
        let has = |k: &str| s.split([',', ' ']).any(|w| w == k);
        Opts {
            autoplay: has("autoplay"),
            lite: has("lite"),
            pose: None,
            capsules: has("capsules"),
        }
    }
}

pub fn app(link: Box<dyn Link>, opts: Opts) -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "oneshot".into(),
                    canvas: Some("#game".into()),
                    fit_canvas_to_parent: true,
                    ..default()
                }),
                ..default()
            })
            // No `.meta` beside our assets: don't ask the server for one.
            .set(AssetPlugin {
                meta_check: bevy::asset::AssetMetaCheck::Never,
                ..default()
            }),
    )
    .insert_non_send_resource(LinkRes(link))
    .insert_resource(ClearColor(Color::srgb(0.62, 0.77, 0.91)))
    .insert_resource(Autoplay {
        on: opts.autoplay,
        ..default()
    })
    .init_resource::<Look>()
    .init_resource::<Built>()
    .init_resource::<Drawn>()
    .init_resource::<look::textures::Textures>()
    .init_resource::<hud::HudState>()
    .init_resource::<Early>()
    .add_plugins(sound::SoundPlugin)
    .add_plugins(MaterialPlugin::<look::ground::GroundMaterial>::default())
    .add_systems(
        Startup,
        (setup, look::textures::load, look::guns::load, look::effects::setup),
    )
    .add_systems(
        Update,
        (
            grab,
            pump,
            happened,
            rebuild,
            look::textures::prepare,
            look::textures::mip_models,
            level::dress_level,
            look::grass::cull,
            look::bodies::graph,
            draw_fighters,
            draw_vehicles,
            fly,
            look::effects::fade,
            view,
            draw_view,
            hud::draw,
        )
            .chain(),
    );
    #[cfg(target_arch = "wasm32")]
    app.add_systems(Update, expose.after(hud::draw));
    app.insert_resource(opts);
    if opts.lite {
        app.add_systems(Update, lite);
    }
    if let Some(p) = opts.pose {
        app.insert_resource(p).add_systems(Update, pose::aim.after(view));
    } else {
        app.add_systems(Startup, hud::setup);
    }
    app
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut media: ResMut<Assets<bevy::pbr::ScatteringMedium>>,
) {
    let eye = look::rig::camera(&mut commands, &mut images, &mut media, Eye);
    look::viewmodel::spawn(&mut commands, eye);
}

/// A click takes the mouse; Escape gives it back.
fn grab(
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut look: ResMut<Look>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let Ok(mut c) = cursor.single_mut() else {
        return;
    };
    if mouse.just_pressed(MouseButton::Left) && c.grab_mode == CursorGrabMode::None {
        c.grab_mode = CursorGrabMode::Locked;
        c.visible = false;
        look.armed = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        c.grab_mode = CursorGrabMode::None;
        c.visible = true;
    }
}

/// The controls into an input, and the session's frame. Only with the
/// mouse taken, as the page does; a press that came and went inside one
/// frame still counts (`just_pressed` without `pressed`).
#[allow(clippy::too_many_arguments)]
fn pump(
    mut link: NonSendMut<LinkRes>,
    mut look: ResMut<Look>,
    mut auto: ResMut<Autoplay>,
    mut st: ResMut<hud::HudState>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    cursor: Query<&CursorOptions, With<PrimaryWindow>>,
) {
    let grabbed = cursor.single().is_ok_and(|c| c.grab_mode != CursorGrabMode::None);
    let key = |k: KeyCode| grabbed && (keys.pressed(k) || keys.just_pressed(k));
    let button = |b: MouseButton| grabbed && (mouse.pressed(b) || mouse.just_pressed(b));
    look.zoom = button(MouseButton::Right) && !auto.on;
    if grabbed {
        let sens = SENS * if look.zoom { ZOOM_SENS } else { 1.0 };
        look.yaw += motion.delta.x * sens;
        let max = PITCH_MAX as f32 / 65536.0 * core::f32::consts::TAU;
        look.pitch = (look.pitch - motion.delta.y * sens).clamp(-max, max);
    }
    if keys.just_pressed(KeyCode::KeyN) {
        st.net = !st.net;
    }
    let axis =
        |plus: KeyCode, minus: KeyCode| (keys.pressed(plus) as i32 - keys.pressed(minus) as i32) * 127 * grabbed as i32;
    let mut buttons = KEYS.iter().filter(|(k, _)| key(*k)).fold(0, |b, (_, v)| b | v);
    if !mouse.pressed(MouseButton::Left) && !mouse.just_pressed(MouseButton::Left) {
        look.armed = true;
    }
    if look.armed && button(MouseButton::Left) {
        buttons |= FIRE;
    }
    if look.zoom {
        buttons |= ZOOM;
    }
    if grabbed && scroll.delta.y != 0.0 {
        buttons |= SWAP;
    }
    let turn = core::f32::consts::TAU;
    let (mut mx, mut mz) = (axis(KeyCode::KeyD, KeyCode::KeyA), axis(KeyCode::KeyW, KeyCode::KeyS));
    if auto.on {
        (buttons, mx, mz) = autopilot(&mut auto, &mut look, &link.0.session().core, link.0.now_ms());
    }
    let input = Input {
        buttons,
        yaw: ((look.yaw / turn).rem_euclid(1.0) * 65536.0) as u32 as u16,
        pitch: (look.pitch / turn * 65536.0) as i16,
        mx: mx as i8,
        mz: mz as i8,
    };
    link.0.update(input);
}

/// One frame of the autopilot: buttons and the two axes, and where to look.
fn autopilot(auto: &mut Autoplay, look: &mut Look, core: &client_net::core::NetCore, now: f64) -> (u16, i32, i32) {
    auto.frames += 1;
    let (f, me) = core.own();
    let mate = core.rules.teams.then_some(core.team);
    let best = core
        .others(now)
        .into_iter()
        .filter(|d| d.alive && Some(d.team & 1) != mate.map(|t| t & 1))
        .map(|d| ((d.pos.x - me.x).hypot(d.pos.z - me.z), d))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    // Pushing ahead (roaming, or closing on someone far) and not getting
    // anywhere: on a rock, or at a wall with whoever fog shows behind it.
    let pushing = best.is_none_or(|(dist, _)| dist > 8.0);
    let mut stuck = false;
    if now - auto.checked >= AUTO_CHECK_MS {
        stuck = pushing && f.alive && (me.x - auto.was.0).hypot(me.z - auto.was.1) < AUTO_MOVED_M;
        auto.was = (me.x, me.z);
        auto.checked = now;
    }
    if stuck && best.is_some() {
        auto.detour = now + AUTO_DETOUR_MS;
    }
    let best = best.filter(|_| now >= auto.detour);
    let Some((dist, d)) = best else {
        // Nobody in sight (or a detour): walk the route.
        let route: Vec<(f32, f32)> = if core.map.roam.is_empty() {
            AUTO_LANES.to_vec()
        } else {
            core.map.roam.iter().map(|p| (p.x, p.z)).collect()
        };
        let mut t = route[auto.leg % route.len()];
        if (t.0 - me.x).hypot(t.1 - me.z) < 2.0 || stuck || now >= auto.next_leg {
            auto.leg += 1;
            auto.next_leg = now + AUTO_LEG_MS;
            t = route[auto.leg % route.len()];
        }
        let t = Vec2::new(t.0, t.1);
        look.yaw = (t.y - me.z).atan2(t.x - me.x);
        look.pitch = 0.0;
        return (0, 0, 127);
    };
    let eye = if f.body.crouched { CROUCH_EYE } else { EYE };
    let (dx, dz) = (d.pos.x - me.x, d.pos.z - me.z);
    look.yaw = dz.atan2(dx);
    look.pitch = (d.pos.y + 1.3 - (me.y + eye)).atan2(dx.hypot(dz));
    let fire = if auto.frames & 1 == 1 { FIRE } else { 0 };
    let strafe = if (auto.frames >> 6) & 1 == 1 { 60 } else { -60 };
    (fire, strafe, if dist > 8.0 { 127 } else { 0 })
}

/// A new welcome: the map it names. (Bevy hands a system its world in
/// pieces; this one needs eight.)
#[allow(clippy::too_many_arguments)]
fn rebuild(
    link: NonSend<LinkRes>,
    mut built: ResMut<Built>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    old: Query<Entity, With<level::LevelPart>>,
    eye: Query<Entity, With<Eye>>,
) {
    let s = link.0.session();
    if !s.ready || s.welcomes == built.0 {
        return;
    }
    built.0 = s.welcomes;
    level::build(&mut commands, &mut meshes, &mut mats, &old, s.map_id);
    if let Ok(eye) = eye.single() {
        look::rig::sky_for(&mut commands, &mut images, eye, s.map_id as u64 + 1);
    }
}

/// Everyone else, where client-net interpolated them.
#[allow(clippy::too_many_arguments)]
fn draw_fighters(
    link: NonSend<LinkRes>,
    mut drawn: ResMut<Drawn>,
    mut commands: Commands,
    models: Res<look::guns::Models>,
    st: Res<hud::HudState>,
    posed: Option<Res<pose::Pose>>,
    opts: Res<Opts>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    graph: Option<Res<look::bodies::Graph>>,
    mut bodies: Query<&mut look::bodies::Body>,
    mut at: Query<&mut Transform>,
    mut players: Query<(&mut AnimationPlayer, Option<&mut AnimationTransitions>), Without<AnimationGraphHandle>>,
    mut playing: Query<(&mut AnimationPlayer, &mut AnimationTransitions), With<AnimationGraphHandle>>,
) {
    let now = link.0.now_ms();
    let core = &link.0.session().core;
    // Dead, and we saw who did it: the kill cam's bodies (ourselves among
    // them) instead of the living.
    let others = match (core.killcam(now), &posed) {
        (_, Some(p)) => p.2.fighter.then_some(p.0).into_iter().collect(),
        (Some(k), _) if !core.own().0.alive => k.bodies,
        _ => core.others(now),
    };
    let seats = look::bodies::seats(&core.vehicles(now));
    for d in &others {
        let skin = posed.as_ref().map_or_else(|| st.skin(d.slot), |p| p.1);
        let e = match drawn.fighters.get(&d.slot) {
            Some(&(e, team, s)) if team == d.team && s == skin => e,
            had => {
                if let Some(&(old, ..)) = had {
                    commands.entity(old).despawn();
                }
                let e = if opts.capsules {
                    look::bodies::spawn_capsule(&mut commands, &mut meshes, &mut mats, d.team)
                } else {
                    look::bodies::spawn(&mut commands, &models, &mut meshes, &mut mats, d.team, skin)
                };
                drawn.fighters.insert(d.slot, (e, d.team, skin));
                e
            }
        };
        if opts.capsules {
            look::bodies::pose_capsule(d, e, &mut at);
            continue;
        }
        look::bodies::pose(
            &mut commands,
            &models,
            graph.as_deref(),
            d,
            seats.get(&d.slot).map(|(v, k)| (v, *k)),
            e,
            &mut bodies,
            &mut at,
            &mut players,
            &mut playing,
        );
    }
    drawn.fighters.retain(|slot, (e, ..)| {
        let k = others.iter().any(|d| d.slot == *slot);
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

/// From our eye, or behind our vehicle at the wheel; dead, the kill cam.
fn view(link: NonSend<LinkRes>, look: Res<Look>, mut cam: Query<(&mut Transform, &mut Projection), With<Eye>>) {
    let Ok((mut t, mut proj)) = cam.single_mut() else {
        return;
    };
    let core = &link.0.session().core;
    let (f, p) = core.own();
    let dir = |yaw: f32, pitch: f32| Vec3::new(pitch.cos() * yaw.cos(), pitch.sin(), pitch.cos() * yaw.sin());
    let mut fov = look::rig::FOV_DEG;
    let at = Vec3::new(p.x, p.y, p.z);
    let kill = if f.alive { None } else { core.killcam(link.0.now_ms()) };
    if let Some(k) = kill {
        // The run-up to the kill, from the killer's eye.
        let eye = Vec3::new(k.eye.x, k.eye.y, k.eye.z);
        *t = Transform::from_translation(eye).looking_at(eye + dir(k.yaw, k.pitch), Vec3::Y);
    } else if f.riding().is_some_and(|(_, seat)| seat == sim::vehicle::DRIVER) {
        let eye = at - dir(look.yaw, look.pitch) * 7.5 + Vec3::Y * (VEH_H + 2.2);
        *t = Transform::from_translation(eye).looking_at(at + Vec3::Y * VEH_H, Vec3::Y);
    } else {
        let h = if f.body.crouched { CROUCH_EYE } else { EYE };
        let eye = at + Vec3::Y * h;
        *t = Transform::from_translation(eye).looking_at(eye + dir(look.yaw, look.pitch), Vec3::Y);
        let zoom = sim::arms::arm(f.gun().arm).zoom;
        if look.zoom && f.alive && zoom > 1 {
            fov /= zoom as f32;
        }
    }
    if let Projection::Perspective(pp) = &mut *proj {
        pp.fov = fov.to_radians();
    }
}

/// The gun in our view.
fn draw_view(
    link: NonSend<LinkRes>,
    look: Res<Look>,
    time: Res<Time>,
    models: Res<look::guns::Models>,
    mut commands: Commands,
    mut hold: Query<(Entity, &mut look::viewmodel::ViewGun, &mut Transform, &mut Visibility)>,
    mut flash: Query<&mut PointLight, With<look::viewmodel::Flash>>,
) {
    let (f, _) = link.0.session().core.own();
    let v = f.body.vel;
    let scoped = look.zoom && sim::arms::arm(f.gun().arm).zoom > 1;
    let own = look::viewmodel::Own {
        f,
        speed: if f.body.grounded {
            (v.x * v.x + v.z * v.z).sqrt()
        } else {
            0.0
        },
        shown: f.alive && f.riding().is_none() && !scoped,
    };
    look::viewmodel::update(&mut commands, &models, own, time.delta_secs(), &mut hold, &mut flash);
}

/// What the session queued for us this frame: events become effects and
/// HUD marks, stream messages names and the feed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn happened(
    mut link: NonSendMut<LinkRes>,
    mut heard: ResMut<sound::Heard>,
    look: Res<Look>,
    time: Res<Time>,
    fx: Res<look::effects::Fx>,
    mut st: ResMut<hud::HudState>,
    mut early: ResMut<Early>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let now_ms = link.0.now_ms();
    let s = link.0.session_mut();
    let events = s.core.drain_events();
    let mut msgs = core::mem::take(&mut early.0);
    msgs.extend(s.take_messages());
    let me = s.core.slot;
    look::effects::on_events(&mut commands, &fx, &mut mats, &events, me);
    hud::on_events(&mut st, s, now_ms, &events, &msgs, time.elapsed_secs(), look.yaw);
    heard.0 = events;
}

/// A software GPU's frame: no shadows, no SMAA, no grass.
fn lite(
    mut commands: Commands,
    mut suns: Query<&mut DirectionalLight>,
    aa: Query<Entity, With<bevy::anti_alias::smaa::Smaa>>,
    grass: Query<Entity, With<level::GrassPart>>,
) {
    for e in &grass {
        commands.entity(e).despawn();
    }
    for mut s in &mut suns {
        if s.shadows_enabled {
            s.shadows_enabled = false;
        }
    }
    for e in &aa {
        commands.entity(e).remove::<bevy::anti_alias::smaa::Smaa>();
    }
}

/// For the browser gate (`ci/e2e.mjs`): `window.__oneshot`, as the
/// three.js page has it.
#[cfg(target_arch = "wasm32")]
fn expose(link: NonSend<LinkRes>, st: Res<hud::HudState>, mut frames: Local<u32>) {
    use wasm_bindgen::JsValue;
    *frames += 1;
    let Some(w) = web_sys::window() else {
        return;
    };
    let s = link.0.session();
    let o = js_sys::Object::new();
    let set = |k: &str, v: JsValue| {
        let _ = js_sys::Reflect::set(&o, &JsValue::from_str(k), &v);
    };
    set("connected", s.ready.into());
    set("frames", (*frames).into());
    set("kills", st.kills.into());
    set("deaths", st.deaths.into());
    set("alive", s.core.own().0.alive.into());
    set("slot", s.core.slot.into());
    let (f, p) = s.core.own();
    set("pos", js_sys::Array::of3(&p.x.into(), &p.y.into(), &p.z.into()).into());
    set("health", f.health.into());
    set("killcam", s.core.killcam(link.0.now_ms()).is_some().into());
    // For the page's menu: what the stream last said, and whether it's over.
    set("status", st.status.as_str().into());
    set("cup", st.cup.as_str().into());
    set("bye", st.ended.into());
    set("ended", (st.ended || link.0.closed()).into());
    // The netcode's health, as the three.js page gave it: snapshots,
    // confirmed predictions, corrected ones.
    let n = s.core.stats;
    let stats = js_sys::Array::of3(&n.snapshots.into(), &n.confirmations.into(), &n.corrections.into());
    set("stats", stats.into());
    let _ = js_sys::Reflect::set(&w, &JsValue::from_str("__oneshot"), &o);
}

/// Rockets and frags in flight.
fn fly(
    link: NonSend<LinkRes>,
    models: Res<look::guns::Models>,
    mut commands: Commands,
    mut have: Query<(Entity, &look::effects::Flying, &mut Transform)>,
) {
    let proj = link.0.session().core.latest().proj();
    look::effects::projectiles(&mut commands, &models, proj, &mut have);
}

#[cfg(target_arch = "wasm32")]
mod web_start {
    use client_net::web::Client;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    async fn sleep(ms: i32) {
        let p = js_sys::Promise::new(&mut |done, _| {
            if let Some(w) = web_sys::window() {
                let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&done, ms);
            }
        });
        let _ = JsFuture::from(p).await;
    }

    /// The page's entry, as the three.js page plays: connect to `addr`
    /// (pinning `hash`, or none for a CA's certificate) as `name`, signed in
    /// with an Elo Pros `pass` (empty for a guest), asking for `mode` in
    /// `skin`; wait for the welcome, handing each line the stream gives
    /// (the queue, a cup, a refusal) to `status`; then run the game.
    ///
    /// A server on its way down refuses as full: `follow()` then resolves
    /// to the live server's `[addr, hash]` (the deploy's other slot,
    /// ops/README.md), or null if it is this one, and we go there once
    /// with the same pass. Resolves true once the game runs, false if the
    /// server said no (`status` has had why). `opts`: a comma list of
    /// `autoplay` and `lite` (see `Opts`).
    #[allow(clippy::too_many_arguments)]
    #[wasm_bindgen]
    pub async fn play(
        addr: String,
        hash: String,
        name: String,
        pass: String,
        mode: u8,
        skin: u8,
        opts: String,
        status: js_sys::Function,
        follow: js_sys::Function,
    ) -> Result<bool, JsValue> {
        let connect = |addr: String, hash: String| Client::connect(addr, hash, name.clone(), pass.clone(), mode, skin);
        let say = |line: &str| {
            let _ = status.call1(&JsValue::NULL, &JsValue::from_str(line));
        };
        let mut c = connect(addr, hash).await?;
        let mut moved = false;
        let mut early = Vec::new();
        'wait: loop {
            c.tick(sim::input::Input::default());
            for m in c.session_mut().take_messages() {
                if m == (proto::Msg::Refuse {
                    reason: proto::REFUSE_FULL,
                }) && !moved
                {
                    moved = true;
                    say("moving to the new server...");
                    let to = follow.call0(&JsValue::NULL)?.dyn_into::<js_sys::Promise>()?;
                    if let Some(a) = JsFuture::from(to).await?.dyn_ref::<js_sys::Array>() {
                        c.close();
                        c = connect(
                            a.get(0).as_string().unwrap_or_default(),
                            a.get(1).as_string().unwrap_or_default(),
                        )
                        .await?;
                        early.clear();
                        continue 'wait;
                    }
                }
                if let Some(line) = super::hud::status_line(&m) {
                    say(&line);
                }
                early.push(m);
            }
            if c.ready() {
                break;
            }
            if c.closed() {
                return Ok(false);
            }
            sleep(100).await;
        }
        let mut app = super::app(Box::new(super::link::Web(c)), super::Opts::parse(&opts));
        app.insert_resource(super::Early(early));
        app.run();
        Ok(true)
    }

    /// The page's `?pose`: no server, one fighter for a picture (`pose.rs`;
    /// `query` is the page's, as `?yaw=1.57&arm=3`).
    #[wasm_bindgen]
    pub fn pose(query: String, opts: String) {
        let pose = super::pose::Pose::parse(&query);
        let opts = super::Opts {
            pose: Some(pose),
            ..super::Opts::parse(&opts)
        };
        super::app(Box::new(super::link::Still::on(pose.2.map)), opts).run();
    }
}

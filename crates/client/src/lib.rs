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

pub mod hud;
pub mod level;
pub mod link;
pub mod look;
pub mod sound;

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
pub struct Look {
    yaw: f32,
    pitch: f32,
    /// The zoom button is held.
    pub zoom: bool,
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
struct Eye;

/// Test driving, inputs only (`?autoplay=1`, `--autoplay`): walk the map's
/// roam route until fog shows an enemy, then close in, aim and fire. After
/// the three.js page's autopilot; it sends what a player's hands could.
#[derive(Resource, Default)]
pub struct Autoplay {
    pub on: bool,
    leg: usize,
    frames: u64,
    /// Where we were at the last check: stuck on a rock, take the next leg.
    was: (f32, f32),
}

/// Radians per pixel of mouse.
const SENS: f32 = 0.0022;

/// How the app runs: `autoplay` drives it (tests); `lite` draws with no
/// shadows or SMAA, for a software GPU (the browser gates on SwiftShader;
/// the page also halves its canvas); `fxdemo` stages the effects in front
/// of the camera (drawing only, `look/fx/demo.rs`).
#[derive(Clone, Copy, Default, Resource)]
pub struct Opts {
    pub autoplay: bool,
    pub lite: bool,
    pub fxdemo: bool,
    /// `fxstage<N>`: the demo plays only its stage `N` (`demo.rs`).
    pub fxstage: Option<u32>,
}

impl Opts {
    /// From a comma list, as the page and the command line give it.
    pub fn parse(s: &str) -> Opts {
        let has = |k: &str| s.split([',', ' ']).any(|w| w == k);
        Opts {
            autoplay: has("autoplay"),
            lite: has("lite"),
            fxdemo: has("fxdemo") || s.contains("fxstage"),
            fxstage: s
                .split([',', ' '])
                .find_map(|w| w.strip_prefix("fxstage").and_then(|n| n.parse().ok())),
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
    .add_plugins(sound::SoundPlugin)
    .add_plugins(MaterialPlugin::<look::ground::GroundMaterial>::default())
    .add_plugins(MaterialPlugin::<look::shield::ShieldMaterial>::default())
    .add_systems(
        Startup,
        (
            setup,
            look::textures::load,
            look::guns::load,
            look::fx::setup,
            hud::setup,
        ),
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
            view,
            look::fx::draw,
            draw_view,
            hud::draw,
            hud::hurt_rim,
        )
            .chain(),
    );
    #[cfg(target_arch = "wasm32")]
    app.add_systems(Update, expose.after(hud::draw));
    app.insert_resource(opts);
    if opts.lite {
        app.add_systems(Update, lite);
    }
    if opts.fxdemo {
        app.add_systems(Update, look::fx::demo::run.after(happened).before(look::fx::draw));
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
#[allow(clippy::too_many_arguments)]
fn pump(
    mut link: NonSendMut<LinkRes>,
    mut look: ResMut<Look>,
    mut auto: ResMut<Autoplay>,
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
    look.zoom = grabbed && mouse.pressed(MouseButton::Right) && !auto.on;
    let axis = |plus: KeyCode, minus: KeyCode| (keys.pressed(plus) as i32 - keys.pressed(minus) as i32) * 127;
    let mut buttons = 0;
    for (on, b) in [
        (grabbed && mouse.pressed(MouseButton::Left), FIRE),
        (look.zoom, ZOOM),
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
    let Some((dist, d)) = best else {
        // Nobody in sight: walk the route until fog shows someone.
        let route = &core.map.roam;
        if route.is_empty() {
            return (0, 0, 0);
        }
        let mut t = route[auto.leg % route.len()];
        let check = auto.frames.is_multiple_of(90);
        let stuck = check && (me.x - auto.was.0).hypot(me.z - auto.was.1) < 1.0;
        if check {
            auto.was = (me.x, me.z);
        }
        if (t.x - me.x).hypot(t.z - me.z) < 2.0 || stuck || auto.frames.is_multiple_of(600) {
            auto.leg += 1;
            t = route[auto.leg % route.len()];
        }
        look.yaw = (t.z - me.z).atan2(t.x - me.x);
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
    graph: Option<Res<look::bodies::Graph>>,
    mut bodies: Query<&mut look::bodies::Body>,
    mut at: Query<&mut Transform>,
    mut players: Query<(&mut AnimationPlayer, Option<&mut AnimationTransitions>), Without<AnimationGraphHandle>>,
    mut playing: Query<(&mut AnimationPlayer, &mut AnimationTransitions), With<AnimationGraphHandle>>,
    mut fx: ResMut<look::fx::Fx>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut shield_mats: ResMut<Assets<look::shield::ShieldMaterial>>,
    mut shells: Query<&mut Visibility, With<look::shield::ShellPart>>,
    time: Res<Time>,
) {
    let now = link.0.now_ms();
    let core = &link.0.session().core;
    // Dead, and we saw who did it: the kill cam's bodies (ourselves among
    // them) instead of the living.
    let others = match core.killcam(now) {
        Some(k) if !core.own().0.alive => k.bodies,
        _ => core.others(now),
    };
    let seats = look::bodies::seats(&core.vehicles(now));
    look::fx::bodies::watch(&mut fx, &others, time.delta_secs(), time.elapsed_secs());
    let (f, p) = core.own();
    look::fx::bodies::watch_own(
        &mut fx,
        core.slot,
        Vec3::new(p.x, p.y, p.z),
        f.body.vel.y,
        f.body.grounded && f.alive && f.riding().is_none(),
    );
    let t = time.elapsed_secs();
    for d in &others {
        let e = *drawn
            .fighters
            .entry(d.slot)
            .or_insert_with(|| look::bodies::spawn(&mut commands, &models, d.team));
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
        look::bodies::flare(
            e,
            fx.was.flare(d.slot, t),
            &mut bodies,
            &mut mats,
            &mut shield_mats,
            &mut shells,
        );
    }
    drawn.fighters.retain(|slot, e| {
        let k = others.iter().any(|d| d.slot == *slot);
        if !k {
            commands.entity(*e).despawn();
        }
        k
    });
}

/// The vehicles: ours as predicted, the rest interpolated.
#[allow(clippy::too_many_arguments)]
fn draw_vehicles(
    link: NonSend<LinkRes>,
    mut drawn: ResMut<Drawn>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut at: Query<&mut Transform>,
    mut fx: ResMut<look::fx::Fx>,
    time: Res<Time>,
) {
    let now = link.0.now_ms();
    let core = &link.0.session().core;
    let seen = core.vehicles(now);
    let mut list: Vec<(u8, Vec3, f32)> = seen
        .iter()
        .map(|v| (v.slot, Vec3::new(v.pos.x, v.pos.y, v.pos.z), v.yaw))
        .collect();
    if let (Some((_, p, yaw)), Some((i, _))) = (core.own_vehicle(), core.own().0.riding()) {
        list.push((i as u8, Vec3::new(p.x, p.y, p.z), yaw));
    }
    for &(slot, p, yaw) in &list {
        let health = seen.iter().find(|v| v.slot == slot).map_or(255, |v| v.health);
        look::fx::hulls::roll(&mut fx, slot, p, yaw, health, time.delta_secs());
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
fn view(
    link: NonSend<LinkRes>,
    look: Res<Look>,
    mut fx: ResMut<look::fx::Fx>,
    time: Res<Time>,
    opts: Res<Opts>,
    mut cam: Query<(&mut Transform, &mut Projection), With<Eye>>,
) {
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
    // The effects demo stands its camera in one place (drawing only).
    if opts.fxdemo {
        *t = look::fx::demo::pose(&core.map);
    }
    // Shake and kick: the drawn camera only, never the aim we send.
    let o = fx.shake.step(time.delta_secs());
    look::shake::apply(&mut t, o);
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
    mut fx: ResMut<look::fx::Fx>,
    mut st: ResMut<hud::HudState>,
) {
    let now_ms = link.0.now_ms();
    let s = link.0.session_mut();
    let events = s.core.drain_events();
    let msgs = s.take_messages();
    {
        let others = s.core.others(now_ms);
        let vehicles = s.core.vehicles(now_ms);
        let scene = look::fx::Scene {
            map: &s.core.map,
            others: &others,
            vehicles: &vehicles,
            me: s.core.slot,
            t: time.elapsed_secs(),
        };
        look::fx::on_events(&mut fx, &scene, &events);
        // Our own gun, drawn the frame it fires, down where we look.
        let (f, p) = s.core.own();
        let h = if f.body.crouched { CROUCH_EYE } else { EYE };
        let eye = Vec3::new(p.x, p.y + h, p.z);
        let dir = Vec3::new(
            look.pitch.cos() * look.yaw.cos(),
            look.pitch.sin(),
            look.pitch.cos() * look.yaw.sin(),
        );
        let cam = Transform::from_translation(eye).looking_at(eye + dir, Vec3::Y).rotation;
        look::fx::shots::own(&mut fx, &scene, f, eye, cam);
    }
    hud::on_events(&mut st, s, now_ms, &events, &msgs, time.elapsed_secs(), look.yaw);
    heard.0 = events;
}

/// A software GPU's frame: no shadows, no SMAA.
fn lite(
    mut commands: Commands,
    mut suns: Query<&mut DirectionalLight>,
    aa: Query<Entity, With<bevy::anti_alias::smaa::Smaa>>,
) {
    for mut s in &mut suns {
        if s.shadows_enabled {
            s.shadows_enabled = false;
        }
    }
    for e in &aa {
        commands.entity(e).remove::<bevy::anti_alias::smaa::Smaa>();
    }
}

/// For the browser gate (`ci/client_e2e.mjs`): `window.__oneshot`, as the
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
    let _ = js_sys::Reflect::set(&w, &JsValue::from_str("__oneshot"), &o);
}

/// Rockets and frags in flight.
fn fly(
    link: NonSend<LinkRes>,
    models: Res<look::guns::Models>,
    time: Res<Time>,
    mut fx: ResMut<look::fx::Fx>,
    mut commands: Commands,
    mut have: Query<(Entity, &mut look::fx::flying::Flying, &mut Transform)>,
) {
    let proj = link.0.session().core.latest().proj();
    look::fx::flying::projectiles(&mut commands, &mut fx, &models, proj, time.elapsed_secs(), &mut have);
}

#[cfg(target_arch = "wasm32")]
mod web_start {
    use wasm_bindgen::prelude::*;

    /// The page's entry: connect (as the three.js page does) and run.
    #[wasm_bindgen]
    /// `opts`: a comma list of `autoplay`, `lite`, `fxdemo` and `fxstage<N>` (see `Opts`).
    pub async fn play(addr: String, hash: String, name: String, mode: u8, opts: String) -> Result<(), JsValue> {
        let c = client_net::web::Client::connect(addr, hash, name, String::new(), mode, 0).await?;
        super::app(Box::new(super::link::Web(c)), super::Opts::parse(&opts)).run();
        Ok(())
    }
}

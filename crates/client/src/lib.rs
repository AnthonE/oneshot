//! The game client: Bevy on the desktop and in the browser
//! (docs/DESIGN.md §2, §16 milestone 12). **It draws, it never decides**:
//! everything it shows comes from client-net's session (the server's
//! snapshots and the prediction of our own fighter and vehicle), and all it
//! sends back is the controls.
//!
//! The level from the sim's map in Gates' light, sky, ground and grass
//! (docs/RIP.md §9); Triple T bodies with guns in hand; the gun in view,
//! effects and sound; a first-person camera (a chase camera at the wheel,
//! the kill cam when dead); the three.js page's controls; the HUD; and the
//! game's own screens in Gates' style: the title (`front`), the Esc menu
//! (`pause`) and the settings (`options`), the same on the desktop and in a
//! browser, where the page (`crates/client/web/index.html`) only finds the
//! server and signs in, and calls `launch()`.

pub mod binds;
pub mod config;
pub mod display;
pub mod door;
pub mod front;
pub mod hud;
pub mod items;
pub mod level;
pub mod link;
pub mod loading;
pub mod look;
pub mod medals;
pub mod options;
pub mod pause;
pub mod pointer;
pub mod pose;
pub mod settings;
pub mod shot;
pub mod sound;
pub mod tags;
pub mod tally;
pub mod ui;

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

/// The link (by `front::LinkGen`) and welcome this level was built for.
#[derive(Resource, Default)]
pub(crate) struct Built(Option<(u32, u32)>);

impl Built {
    pub(crate) fn gen(&self) -> Option<u32> {
        self.0.map(|(g, _)| g)
    }
}

/// Drawn things by their slot (a fighter with the side and skin its body
/// was made in). Made once and kept: fog hiding someone hides their body or
/// hull, it doesn't throw it away, so their coming back costs nothing (a
/// skinned scene, its materials and shield shells, made mid-fight, was a
/// hitch).
#[derive(Resource, Default)]
struct Drawn {
    fighters: BTreeMap<u8, (Entity, u8, u8)>,
    vehicles: BTreeMap<u8, Entity>,
}

#[derive(Component)]
pub struct Eye;

/// What has the mouse: the title, the match, the Esc menu, or the settings.
#[derive(States, Default, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Screen {
    Title,
    #[default]
    Playing,
    Paused,
    Settings,
}

/// Where a hull's wheels are in its frame (+X forward), metres: drawn
/// there, and pressing the grass there (`look::foliage::Walkers::hull`).
const WHEELS: [(f32, f32); 4] = [(1.5, 1.05), (1.5, -1.05), (-1.5, 1.05), (-1.5, -1.05)];

/// What every hull is made of, made with the first.
struct HullParts {
    hull: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    body: Handle<Mesh>,
    screen: Handle<Mesh>,
    wheel: Handle<Mesh>,
}

/// A fighter's body or a vehicle's hull, shown or hidden by `Drawn`.
#[derive(Component)]
struct Kept;

/// The kept entities' visibility (never a shield shell's).
type KeptVis<'w, 's> = Query<'w, 's, &'static mut Visibility, (With<Kept>, Without<look::shield::ShellPart>)>;

/// What a body is made of, and the kept ones' visibility.
type BodyParts<'w, 's> = (
    ResMut<'w, Assets<Mesh>>,
    ResMut<'w, Assets<StandardMaterial>>,
    KeptVis<'w, 's>,
);

/// Shows or hides a kept entity, writing only on a change; whether it
/// changed.
fn show(vis: &mut KeptVis, e: Entity, on: bool) -> bool {
    let Ok(mut v) = vis.get_mut(e) else {
        return false;
    };
    let want = if on { Visibility::Inherited } else { Visibility::Hidden };
    let changed = *v != want;
    if changed {
        *v = want;
    }
    changed
}

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

/// Radians per pixel of mouse. Scoped, divided by the scope's zoom: the
/// view narrows by it, so the world moves under the crosshair at one speed
/// (Gates' `input.rs`).
const SENS: f32 = 0.0022;

/// The bound actions that are buttons (the player's keys, `binds.rs`).
/// The wheel swaps too.
const BUTTONS: [(binds::Action, u16); 8] = [
    (binds::Action::Jump, JUMP),
    (binds::Action::Crouch, CROUCH),
    (binds::Action::Melee, MELEE),
    (binds::Action::Grenade, GRENADE),
    (binds::Action::Reload, RELOAD),
    (binds::Action::Swap, SWAP),
    (binds::Action::Use, USE),
    (binds::Action::GrenSwitch, GREN_SWITCH),
];

/// How the app runs: `autoplay` drives it (tests); `lite` draws POTATO
/// for this run, keeping the player's own rung for the file (the browser
/// gates on SwiftShader; the page also halves its canvas); `fxdemo` stages
/// the effects in front of the camera (drawing only, `look/fx/demo.rs`).
#[derive(Clone, Copy, Default, Resource)]
pub struct Opts {
    pub autoplay: bool,
    pub lite: bool,
    /// `potato` (`?potato`, `--potato`): POTATO whatever was chosen, and
    /// kept (`look/potato.rs`).
    pub potato: bool,
    /// `quality=<name>` (`?quality=`, `--quality`): that rung for this run
    /// only, and no picking for the machine: what a picture of a rung asks.
    pub quality: Option<look::quality::Quality>,
    pub fxdemo: bool,
    /// `fxstage<N>`: the demo plays only its stage `N` (`demo.rs`).
    pub fxstage: Option<u32>,
    /// No server: one fighter posed for a picture (`pose::Pose`).
    pub pose: Option<pose::Pose>,
    /// Everyone else as a capsule in the side's colour, not Triple T: a
    /// software GPU can't skin a room of them (the gate, eight seats on).
    pub capsules: bool,
    /// Open the match on the Esc menu (`menu`) or the settings
    /// (`settings`): for pictures of them (`ci/client_shot.mjs ... menu=1`).
    /// On the title, `settings` opens the settings over it.
    pub open: Screen,
    /// `graphics`: the settings, on their GRAPHICS tab.
    pub graphics: bool,
    /// The title, or straight into a mode, or neither (a picture).
    pub front: front::Start,
    /// `page`: the page's own menu is the title (a browser's, `web/menu.js`):
    /// the backdrop behind it, and matches asked for by `knock()`.
    pub page: bool,
}

impl Opts {
    /// From a comma list, as the page and the command line give it.
    pub fn parse(s: &str) -> Opts {
        let has = |k: &str| s.split([',', ' ']).any(|w| w == k);
        Opts {
            autoplay: has("autoplay"),
            lite: has("lite"),
            potato: has("potato"),
            quality: s
                .split([',', ' '])
                .find_map(|w| w.strip_prefix("quality=").and_then(look::quality::Quality::from_name)),
            fxdemo: has("fxdemo") || s.contains("fxstage"),
            fxstage: s
                .split([',', ' '])
                .find_map(|w| w.strip_prefix("fxstage").and_then(|n| n.parse().ok())),
            pose: None,
            capsules: has("capsules"),
            page: has("page"),
            front: front::Start::Off,
            graphics: has("graphics"),
            open: if has("settings") || has("graphics") {
                Screen::Settings
            } else if has("menu") {
                Screen::Paused
            } else {
                Screen::Playing
            },
        }
    }
}

/// The game: the title (or straight into a mode, `opts.front`) on `door`.
pub fn game(door: door::Door, opts: Opts) -> App {
    let (link, first): (Box<dyn Link>, Screen) = match opts.front {
        // Nothing behind it: the match's level is the first built.
        front::Start::Join(_) | front::Start::Off => (Box::new(link::Still::idle()), Screen::Title),
        front::Start::Title if opts.open == Screen::Settings => (front::backdrop(), Screen::Settings),
        front::Start::Title => (front::backdrop(), Screen::Title),
    };
    let mut app = app_on(link, opts, first);
    front::install(&mut app, door, opts.front, opts.open);
    app.insert_resource(front::PageMenu(opts.page));
    app
}

/// The client on `link` with no title: a picture (`pose`), or the fx stage.
pub fn app(link: Box<dyn Link>, opts: Opts) -> App {
    app_on(link, opts, opts.open)
}

fn app_on(link: Box<dyn Link>, opts: Opts, first: Screen) -> App {
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
    // What the device can bind, read before `Startup` spawns the camera.
    .add_plugins(look::gpu::GpuPlugin)
    .insert_non_send_resource(LinkRes(link))
    // The shell's ground until a level is spawned (`loading::settle` then
    // gives it the sky's): the frames before the first UI draw are the
    // title's colour, not a flash of blue.
    .insert_resource(ClearColor(ui::BG))
    .insert_resource(Autoplay {
        on: opts.autoplay,
        ..default()
    })
    .init_resource::<Look>()
    .init_resource::<Built>()
    .init_resource::<Drawn>()
    .init_resource::<look::textures::Textures>()
    .init_resource::<hud::HudState>()
    .init_resource::<items::Items>()
    .init_resource::<Early>()
    .init_resource::<look::trees::TreeCards>()
    .init_resource::<look::viewmodel::ViewMuzzles>()
    .add_plugins(sound::SoundPlugin)
    .add_plugins(ui::plugin)
    .add_plugins(settings::plugin)
    // The graphics ladder after the settings it reads: its tier exists
    // before anything is spawned with it.
    .add_plugins(look::quality::plugin)
    .add_plugins(look::potato::plugin)
    .add_plugins(look::render_scale::plugin)
    .add_plugins(display::plugin)
    .insert_state(first)
    .add_plugins(pause::plugin)
    .add_plugins(options::plugin)
    .add_plugins(loading::plugin)
    .add_plugins(look::shot::plugin)
    .add_plugins(MaterialPlugin::<look::ground::GroundMaterial>::default())
    // The boxes' weathering (`look/weathering.rs`).
    .add_plugins(look::weathering::plugin)
    // The grass's and the clutter's wind, trails and fade (`look/foliage.rs`).
    .add_plugins(look::foliage::plugin)
    .add_plugins(MaterialPlugin::<look::shield::ShieldMaterial>::default())
    // Your own body, cast in the sun's shadow and drawn by no camera
    // (`look/own_shadow.rs`).
    .add_plugins(MaterialPlugin::<look::own_shadow::ShadowOnly>::default())
    // The title's Triple T is on a layer of his own, and every node his
    // scene spawns takes it before visibility is worked out
    // (`look/portrait.rs`).
    .add_plugins(bevy::app::HierarchyPropagatePlugin::<
        bevy::camera::visibility::RenderLayers,
    >::new(PostUpdate))
    .configure_sets(
        PostUpdate,
        bevy::app::PropagateSet::<bevy::camera::visibility::RenderLayers>::default()
            .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
    )
    // The sky (look/deck.rs and its layers) under each map's mood.
    .add_plugins(MaterialPlugin::<look::deck::CloudMaterial>::default())
    .add_plugins(MaterialPlugin::<look::deck::SkyFogMaterial>::default())
    .add_plugins(MaterialPlugin::<look::stars::StarMaterial>::default())
    .add_plugins(MaterialPlugin::<look::moon::MoonMaterial>::default())
    .add_plugins(MaterialPlugin::<look::rain::RainMaterial>::default())
    .add_plugins(MaterialPlugin::<look::rainbow::RainbowMaterial>::default())
    .init_resource::<look::mood::MoodNow>()
    .init_resource::<look::mood::SkyClock>()
    .init_resource::<look::mood::Drift>()
    .init_resource::<look::shafts::Shafts>()
    .init_resource::<look::cliffs::SkinLift>()
    // The sea (look/water.rs) and its waves.
    .add_plugins(MaterialPlugin::<look::water::WaterMaterial>::default())
    .add_systems(Update, look::water::animate.after(view))
    .add_systems(
        Startup,
        (
            setup,
            look::textures::load,
            look::rock::load,
            look::guns::load,
            look::own_shadow::setup,
            look::portrait::setup,
            look::fx::setup,
            look::prewarm::setup,
            (look::deck::setup, (look::stars::setup, look::moon::setup)).chain(),
            look::rain::setup,
            look::rainbow::setup,
            look::shafts::setup,
        ),
    )
    // The sky after the level is built for its map: the clock, the drift
    // off it, then everything drawn from the mood and the drift, each
    // written only when it moves.
    .add_systems(
        Update,
        (
            sky_clock,
            look::mood::drift,
            look::shafts::drive,
            (
                look::deck::drive,
                look::stars::drive,
                look::moon::drive,
                look::rain::drive,
                look::rainbow::drive,
                look::cloud_shadow::drive,
                look::rig::fog,
            ),
        )
            .chain()
            .after(rebuild),
    )
    .add_systems(
        Update,
        (
            look::prewarm::warm,
            look::prewarm::warm_level,
            look::prewarm::warm_models,
            look::prewarm::retire,
        )
            .chain(),
    )
    // The marks keep off what is drawn back from its box, and onto the
    // rock skin (`look/rock.rs`, `look/cliffs.rs`).
    .add_systems(
        Update,
        (look::rock::bare_marks, look::cliffs::lift_marks).after(level::dress_level),
    )
    .add_systems(PreUpdate, look::fx::retier.after(look::quality::resolve))
    .add_systems(
        Update,
        (
            grab,
            pump,
            happened,
            rebuild,
            look::textures::prepare,
            look::textures::mip_models,
            look::rock::dress_masses,
            (
                level::dress_level,
                look::grass::dress,
                look::clutter::dress,
                look::foliage::warm,
            )
                .chain(),
            look::grass::cull,
            look::anim::build,
            // What presses the grass is what is drawn (`look/foliage.rs`).
            (draw_fighters, draw_vehicles, look::foliage::update).chain(),
            items::draw,
            fly,
            view,
            look::fx::draw,
            draw_view,
            hud::draw,
            hud::hurt_rim,
        )
            .chain(),
    );
    app.add_systems(Update, (medals::row, medals::carnage).after(hud::draw));
    // Your shadow and the title's Triple T are bodies too, posed beside
    // everyone else's; then every body's player is driven; then, between
    // the animation and the transforms, every spine is turned
    // (`look/anim.rs`).
    app.add_systems(
        Update,
        (
            look::own_shadow::stream.after(draw_fighters),
            look::portrait::stand,
            look::portrait::activate,
        ),
    )
    .add_systems(
        Update,
        look::bodies::drive
            .after(look::own_shadow::stream)
            .after(look::portrait::stand)
            .after(draw_fighters),
    )
    .add_systems(
        PostUpdate,
        look::bodies::pose_spines
            .after(bevy::app::AnimationSystems)
            .before(bevy::transform::TransformSystems::Propagate),
    )
    // Names over heads, after the camera is placed (`tags.rs`).
    .add_systems(Update, tags::draw.after(view).after(hud::draw));
    #[cfg(target_arch = "wasm32")]
    app.add_systems(Update, expose.after(hud::draw))
        .add_systems(PostUpdate, pointer::follow);
    app.insert_resource(opts);
    if opts.graphics {
        app.world_mut().resource_mut::<options::Page>().cat = options::GRAPHICS;
    }
    if let Some(p) = opts.pose {
        app.insert_resource(p).add_systems(Update, pose::aim.after(view));
    } else {
        app.add_systems(Startup, (hud::setup, medals::setup))
            .add_systems(Update, (hud::prompt, hud::reticle_tint, hud::board).after(hud::draw));
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

/// The sky's clock (`look::mood::SkyClock`): the server's tick as this
/// client estimates it, so the deck drifts the same for everyone in a
/// match; the app's own clock where no server keeps one (the title).
fn sky_clock(link: NonSend<LinkRes>, time: Res<Time>, mut clock: ResMut<look::mood::SkyClock>) {
    let s = link.0.session();
    let tick = if s.ready {
        s.core.render_tick(link.0.now_ms())
    } else {
        0.0
    };
    clock.s = if tick > 0.0 {
        tick / f64::from(sim::consts::TICK_HZ)
    } else {
        time.elapsed_secs_f64()
    };
}

/// In a match: a click takes the mouse; Escape opens the menu (in a
/// browser the page keeps that Esc, and `pointer::follow` opens it).
fn grab(
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut look: ResMut<Look>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    screen: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
) {
    if *screen.get() != Screen::Playing {
        return;
    }
    let Ok(mut c) = cursor.single_mut() else {
        return;
    };
    if mouse.just_pressed(MouseButton::Left) && c.grab_mode == CursorGrabMode::None {
        c.grab_mode = CursorGrabMode::Locked;
        c.visible = false;
        look.armed = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        next.set(Screen::Paused);
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
    (binds, settings): (Res<binds::Binds>, Res<settings::Settings>),
) {
    // The mouse is ours only while held: on the web, the page's lock is the
    // truth (a lock asked for and not yet granted, or let go behind our
    // back, turns nothing and fires nothing).
    let grabbed =
        cursor.single().is_ok_and(|c| c.grab_mode != CursorGrabMode::None) && pointer::pointer_locked().unwrap_or(true);
    let held = |a: binds::Action| grabbed && (binds.pressed(a, &keys, &mouse) || binds.just_pressed(a, &keys, &mouse));
    let button = |b: MouseButton| grabbed && (mouse.pressed(b) || mouse.just_pressed(b));
    // A gun in each hand: the right button is the left hand's trigger,
    // not the scope.
    let (dual, scope, driving) = {
        let f = link.0.session().core.own().0;
        let driving = f.riding().is_some_and(|(_, s)| s == sim::vehicle::DRIVER);
        let scope = if f.alive {
            sim::arms::arm(f.gun().arm).zoom.max(1)
        } else {
            1
        };
        (f.dual(), scope, driving)
    };
    // No scope at the wheel: the chase camera doesn't narrow.
    look.zoom = button(MouseButton::Right) && !auto.on && !dual && !driving;
    if grabbed {
        let base = SENS * settings.sensitivity;
        let sens = if look.zoom { base / scope as f32 } else { base };
        let up = if settings.invert { -1.0 } else { 1.0 };
        look.yaw += motion.delta.x * sens;
        let max = PITCH_MAX as f32 / 65536.0 * core::f32::consts::TAU;
        look.pitch = (look.pitch - motion.delta.y * sens * up).clamp(-max, max);
    }
    if grabbed && binds.just_pressed(binds::Action::NetGraph, &keys, &mouse) {
        st.net = !st.net;
    }
    let on = |a: binds::Action| (grabbed && binds.pressed(a, &keys, &mouse)) as i32;
    let axis = |plus: binds::Action, minus: binds::Action| (on(plus) - on(minus)) * 127;
    let mut buttons = BUTTONS.iter().filter(|(a, _)| held(*a)).fold(0, |b, (_, v)| b | v);
    if !mouse.pressed(MouseButton::Left) && !mouse.just_pressed(MouseButton::Left) {
        look.armed = true;
    }
    if look.armed && button(MouseButton::Left) {
        buttons |= FIRE;
    }
    if look.zoom {
        buttons |= ZOOM;
    }
    if dual && look.armed && button(MouseButton::Right) {
        buttons |= FIRE_LEFT;
    }
    if grabbed && scroll.delta.y != 0.0 {
        buttons |= SWAP;
    }
    let turn = core::f32::consts::TAU;
    let (mut mx, mut mz) = (
        axis(binds::Action::Right, binds::Action::Left),
        axis(binds::Action::Forward, binds::Action::Back),
    );
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
    // Tap by tick, not by frame: down if the last tick had it up. Frames
    // outnumbering ticks would otherwise hold it down for good.
    let fire = if f.last_buttons & FIRE == 0 { FIRE } else { 0 };
    let strafe = if (auto.frames >> 6) & 1 == 1 { 60 } else { -60 };
    (fire, strafe, if dist > 8.0 { 127 } else { 0 })
}

/// A new welcome: the map it names. (Bevy hands a system its world in
/// pieces; this one needs eight.)
#[allow(clippy::too_many_arguments)]
fn rebuild(
    link: NonSend<LinkRes>,
    mut built: ResMut<Built>,
    gen: Option<Res<front::LinkGen>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut water_mats: ResMut<Assets<look::water::WaterMaterial>>,
    mut cards: ResMut<look::trees::TreeCards>,
    old: Query<Entity, With<level::LevelPart>>,
    mut eye: Query<(&mut bevy::camera::Exposure, &mut bevy::light::EnvironmentMapLight), With<Eye>>,
    mut shade: ResMut<look::quality::MapShade>,
    mut mood: ResMut<look::mood::MoodNow>,
) {
    let s = link.0.session();
    let now = Some((gen.map_or(0, |g| g.0), s.welcomes));
    if !s.ready || built.0 == now {
        return;
    }
    built.0 = now;
    // The map's mood first: the level's sun and its wet are the mood's.
    look::mood::set(&mut mood, s.map_id);
    level::build(
        &mut commands,
        &mut meshes,
        &mut mats,
        &mut water_mats,
        &mut images,
        &mut cards,
        &old,
        s.map_id,
    );
    if let Ok((mut exposure, mut env)) = eye.single_mut() {
        if let Some(d) = sim::maps::ALL.iter().find(|d| d.id == s.map_id) {
            look::rig::expose_for(&d.light, &mood.mood, &mut exposure, &mut env);
        }
    }
    // Every rung's shade reaches across this map (`look/quality.rs`).
    if let Some(d) = sim::maps::ALL.iter().find(|d| d.id == s.map_id) {
        shade.set_if_neq(look::quality::MapShade::of(d));
    }
}

/// The effects' share of drawing bodies: the shield's flare and shells.
type ShieldParts<'w, 's> = (
    ResMut<'w, look::fx::Fx>,
    ResMut<'w, Assets<look::shield::ShieldMaterial>>,
    Query<'w, 's, &'static mut Visibility, With<look::shield::ShellPart>>,
);

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
    (mut meshes, mut mats, mut vis): BodyParts,
    mut bodies: Query<&mut look::bodies::Body>,
    mut at: Query<&mut Transform>,
    (mut fx, mut shield_mats, mut shells): ShieldParts,
    (time, mut walkers): (Res<Time>, ResMut<look::foliage::Walkers>),
) {
    let now = link.0.now_ms();
    let core = &link.0.session().core;
    // Dead, and we saw who did it: the kill cam's bodies (ourselves among
    // them) instead of the living.
    let (others, live) = match (core.killcam(now), &posed) {
        (_, Some(p)) => (p.2.fighter.then_some(p.0).into_iter().collect(), false),
        (Some(k), _) if !core.own().0.alive => (k.bodies, false),
        _ => (core.others(now), true),
    };
    let seats = look::bodies::seats(&core.vehicles(now));
    look::fx::bodies::watch(&mut fx, &others, time.delta_secs(), time.elapsed_secs());
    let (f, p) = core.own();
    // The grass flattens under the feet this client draws on the ground,
    // ours first: never a kill cam's replay, never a seat.
    walkers.feet.clear();
    if live && f.alive && f.body.grounded && f.riding().is_none() {
        walkers.fighter(Vec3::new(p.x, p.y, p.z));
    }
    if live {
        for d in others
            .iter()
            .filter(|d| d.alive && d.grounded && !seats.contains_key(&d.slot))
        {
            walkers.fighter(Vec3::new(d.pos.x, d.pos.y, d.pos.z));
        }
    }
    look::fx::bodies::watch_own(
        &mut fx,
        core.slot,
        Vec3::new(p.x, p.y, p.z),
        f.body.vel.y,
        f.body.grounded && f.alive && f.riding().is_none(),
    );
    let t = time.elapsed_secs();
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
                commands.entity(e).insert(Kept);
                drawn.fighters.insert(d.slot, (e, d.team, skin));
                e
            }
        };
        let _ = show(&mut vis, e, true);
        if opts.capsules {
            look::bodies::pose_capsule(d, e, &mut at);
            continue;
        }
        look::bodies::pose(
            &mut commands,
            &models,
            d,
            seats.get(&d.slot).map(|(v, k)| (v.yaw, *k)),
            e,
            &mut bodies,
            &mut at,
            time.delta_secs(),
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
    for (slot, (e, ..)) in &drawn.fighters {
        if !others.iter().any(|d| d.slot == *slot) && show(&mut vis, *e, false) {
            // Shown again, it starts its clip afresh, not fading from the
            // pose it was hidden in (a kill cam's victim got up off the
            // floor).
            if let Ok(mut b) = bodies.get_mut(*e) {
                b.afresh();
            }
        }
    }
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
    mut vis: KeptVis,
    mut parts: Local<Option<HullParts>>,
    mut fx: ResMut<look::fx::Fx>,
    time: Res<Time>,
    mut walkers: ResMut<look::foliage::Walkers>,
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
        if let Some(&e) = drawn.vehicles.get(&slot) {
            let _ = show(&mut vis, e, true);
        }
        walkers.hull(p, yaw, &WHEELS);
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
                // One set of meshes and materials for every hull.
                let k = parts.get_or_insert_with(|| HullParts {
                    hull: mats.add(StandardMaterial {
                        base_color: Color::srgb(0.36, 0.42, 0.29),
                        perceptual_roughness: 0.6,
                        metallic: 0.3,
                        ..default()
                    }),
                    dark: mats.add(StandardMaterial {
                        base_color: Color::srgb(0.14, 0.15, 0.17),
                        perceptual_roughness: 0.8,
                        ..default()
                    }),
                    body: meshes.add(Cuboid::new(4.4, 0.7, 2.2)),
                    screen: meshes.add(Cuboid::new(0.12, 0.9, 2.2)),
                    wheel: meshes.add(Cylinder::new(0.42, 0.35)),
                });
                let (hull, dark) = (k.hull.clone(), k.dark.clone());
                let (body, screen, wheel) = (k.body.clone(), k.screen.clone(), k.wheel.clone());
                let e = commands
                    .spawn((t, Visibility::default(), Kept))
                    .with_children(|c| {
                        c.spawn((
                            Mesh3d(body),
                            MeshMaterial3d(hull.clone()),
                            Transform::from_xyz(0.0, 0.75, 0.0),
                        ));
                        c.spawn((
                            Mesh3d(screen),
                            MeshMaterial3d(dark.clone()),
                            Transform::from_xyz(-0.9, 1.55, 0.0),
                        ));
                        for (x, z) in WHEELS {
                            c.spawn((
                                Mesh3d(wheel.clone()),
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
    for (slot, e) in &drawn.vehicles {
        if !list.iter().any(|v| v.0 == *slot) {
            let _ = show(&mut vis, *e, false);
        }
    }
}

/// From our eye, or behind our vehicle at the wheel; dead, the kill cam.
pub(crate) fn view(
    link: NonSend<LinkRes>,
    look: Res<Look>,
    mut fx: ResMut<look::fx::Fx>,
    time: Res<Time>,
    opts: Res<Opts>,
    settings: Res<settings::Settings>,
    mut cam: Query<(&mut Transform, &mut Projection), With<Eye>>,
) {
    let Ok((mut t, mut proj)) = cam.single_mut() else {
        return;
    };
    let core = &link.0.session().core;
    let (f, p) = core.own();
    let dir = |yaw: f32, pitch: f32| Vec3::new(pitch.cos() * yaw.cos(), pitch.sin(), pitch.cos() * yaw.sin());
    let mut fov = settings.fov;
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
        *t = look::fx::demo::pose(&core.map, opts.fxstage);
    }
    // Shake and kick: the drawn camera only, never the aim we send.
    let o = fx.shake.step(time.delta_secs());
    look::shake::apply(&mut t, o);
    if let Projection::Perspective(pp) = &mut *proj {
        pp.fov = fov.to_radians();
    }
}

/// The gun in our view: hidden dead, at the wheel and behind a scope; a
/// passenger, who fights from the seat in first person, holds it.
#[allow(clippy::too_many_arguments)]
fn draw_view(
    link: NonSend<LinkRes>,
    look: Res<Look>,
    time: Res<Time>,
    models: Res<look::guns::Models>,
    mut commands: Commands,
    mut motion: Local<look::viewmodel::Motion>,
    mut muzzles: ResMut<look::viewmodel::ViewMuzzles>,
    mut hold: look::viewmodel::Hands,
    mut flash: look::viewmodel::FlashLight,
) {
    let (f, p) = link.0.session().core.own();
    let scoped = look.zoom && sim::arms::arm(f.gun().arm).zoom > 1;
    let seat = f.riding().map(|(_, seat)| seat);
    let own = look::viewmodel::Own {
        f,
        at: Vec3::new(p.x, p.y, p.z),
        yaw: look.yaw,
        pitch: look.pitch,
        riding: seat.is_some(),
        shown: f.alive && seat != Some(sim::vehicle::DRIVER) && !scoped,
    };
    look::viewmodel::update(
        &mut commands,
        &models,
        own,
        time.delta_secs(),
        &mut motion,
        &mut muzzles,
        &mut hold,
        &mut flash,
    );
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
    mut early: ResMut<Early>,
    muzzles: Res<look::viewmodel::ViewMuzzles>,
) {
    let now_ms = link.0.now_ms();
    let s = link.0.session_mut();
    let events = s.core.drain_events();
    let mut msgs = core::mem::take(&mut early.0);
    msgs.extend(s.take_messages());
    {
        let others = s.core.others(now_ms);
        let vehicles = s.core.vehicles(now_ms);
        let dir = Vec3::new(
            look.pitch.cos() * look.yaw.cos(),
            look.pitch.sin(),
            look.pitch.cos() * look.yaw.sin(),
        );
        let scene = look::fx::Scene {
            map: &s.core.map,
            others: &others,
            vehicles: &vehicles,
            me: s.core.slot,
            aim: dir,
            t: time.elapsed_secs(),
        };
        look::fx::on_events(&mut fx, &scene, &events);
        // Our own gun, drawn the frame it fires, down where we look.
        let (f, p) = s.core.own();
        let h = if f.body.crouched { CROUCH_EYE } else { EYE };
        let eye = Vec3::new(p.x, p.y + h, p.z);
        let cam = Transform::from_translation(eye).looking_at(eye + dir, Vec3::Y).rotation;
        look::fx::shots::own(&mut fx, &scene, f, eye, cam, muzzles.0);
    }
    hud::on_events(&mut st, s, now_ms, &events, &msgs, time.elapsed_secs(), look.yaw);
    heard.0 = events;
}

/// For the browser gate (`ci/e2e.mjs`): `window.__oneshot`, as the
/// three.js page has it, and where the title's controls are (`ui`: a tag's
/// centre and size in the page's pixels, for a click).
#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
fn expose(
    link: NonSend<LinkRes>,
    st: Res<hud::HudState>,
    in_match: Option<Res<front::InMatch>>,
    screen: Res<State<Screen>>,
    tags: Query<(
        &front::Tag,
        &ComputedNode,
        &bevy::ui::UiGlobalTransform,
        &InheritedVisibility,
    )>,
    windows: Query<&Window, With<PrimaryWindow>>,
    settle: Res<loading::Settle>,
    tier: Res<look::quality::Tier>,
    mood: Res<look::mood::MoodNow>,
    page: Option<Res<front::PageState>>,
    mut frames: Local<u32>,
) {
    use wasm_bindgen::JsValue;
    *frames += 1;
    let Some(w) = web_sys::window() else {
        return;
    };
    let s = link.0.session();
    let o = js_sys::Object::new();
    let set = |o: &js_sys::Object, k: &str, v: JsValue| {
        let _ = js_sys::Reflect::set(o, &JsValue::from_str(k), &v);
    };
    let matched = in_match.as_ref().is_some_and(|m| m.0);
    let live = link.0.live() && (in_match.is_none() || matched);
    set(&o, "connected", (live && s.ready).into());
    set(&o, "frames", (*frames).into());
    set(&o, "kills", st.kills.into());
    set(&o, "deaths", st.deaths.into());
    set(&o, "assists", st.assists.into());
    // Our medals this match, by name, and the carnage report's rows.
    let medals = js_sys::Object::new();
    for (&id, &n) in &st.earned {
        let _ = js_sys::Reflect::set(&medals, &JsValue::from_str(proto::medal::info(id).name), &n.into());
    }
    set(&o, "medals", medals.into());
    set(&o, "carnage", st.carnage.len().into());
    set(&o, "alive", s.core.own().0.alive.into());
    set(&o, "slot", s.core.slot.into());
    set(&o, "team", s.core.team.into());
    // Who sits where, by name: a party's page sees its members (ci/party.mjs).
    let names = js_sys::Array::new();
    for (_, n) in st.names() {
        names.push(&JsValue::from_str(n));
    }
    set(&o, "names", names.into());
    let (f, p) = s.core.own();
    set(
        &o,
        "pos",
        js_sys::Array::of3(&p.x.into(), &p.y.into(), &p.z.into()).into(),
    );
    set(&o, "health", f.health.into());
    set(&o, "killcam", s.core.killcam(link.0.now_ms()).is_some().into());
    let name = match screen.get() {
        Screen::Title => "title",
        Screen::Playing => "playing",
        Screen::Paused => "paused",
        Screen::Settings => "settings",
    };
    set(&o, "screen", name.into());
    // For the page's menu: in a match, a knock out, and the line it gave.
    set(&o, "in_match", matched.into());
    if let Some(p) = page {
        set(&o, "knocking", p.knocking.into());
        set(&o, "front_status", p.status.as_str().into());
        set(&o, "front_bad", p.bad.into());
    }
    // Which module this is and what it draws (`look/gpu.rs`).
    set(&o, "gfx", look::gpu::api().into());
    set(&o, "tier", tier.describe().into());
    // The map's sky (`look/mood.rs`).
    set(&o, "sky", mood.mood.name.into());
    set(
        &o,
        "quality",
        tier.preset.map_or("custom", look::quality::Quality::name).into(),
    );
    // The screen's level is there with nothing over it (`loading::Settle`):
    // what `ci/client_shot.mjs` waits for before its seconds.
    set(&o, "ready", settle.shown().into());
    set(&o, "stage", settle.now.stage().into());
    // What the stream last said, and whether it's over.
    set(&o, "status", st.status.as_str().into());
    set(&o, "cup", st.cup.as_str().into());
    set(&o, "bye", st.ended.into());
    set(&o, "ended", (st.ended || (live && link.0.closed())).into());
    // The netcode's health, as the three.js page gave it: snapshots,
    // confirmed predictions, corrected ones.
    let n = s.core.stats;
    let stats = js_sys::Array::of3(&n.snapshots.into(), &n.confirmations.into(), &n.corrections.into());
    set(&o, "stats", stats.into());
    let ui = js_sys::Object::new();
    let k = windows.single().map_or(1.0, |w| w.scale_factor());
    for (t, node, at, vis) in &tags {
        if !vis.get() {
            continue;
        }
        let (c, size) = (at.translation / k, node.size() / k);
        let r = [c.x, c.y, size.x, size.y].map(JsValue::from);
        set(&ui, t.0, js_sys::Array::of4(&r[0], &r[1], &r[2], &r[3]).into());
    }
    set(&o, "ui", ui.into());
    let _ = js_sys::Reflect::set(&w, &JsValue::from_str("__oneshot"), &o);
}

/// Rockets and frags in flight.
fn fly(
    link: NonSend<LinkRes>,
    models: Res<look::guns::Models>,
    time: Res<Time>,
    mut fx: ResMut<look::fx::Fx>,
    mut commands: Commands,
    mut have: Query<(Entity, &mut look::fx::flying::Flying, &mut Transform, &mut Visibility)>,
) {
    let core = &link.0.session().core;
    let others = core.others(link.0.now_ms());
    look::fx::flying::projectiles(
        &mut commands,
        &mut fx,
        &models,
        &core.map,
        core.latest().proj(),
        &others,
        time.elapsed_secs(),
        time.delta_secs(),
        &mut have,
    );
}

#[cfg(target_arch = "wasm32")]
mod web_start {
    use wasm_bindgen::prelude::*;

    /// The page's entry: the game, on the title (or straight into `mode`
    /// with `autoplay`, `fxdemo` or `fxstage` in `opts`, a comma list: see
    /// `Opts`). `addr` and `hash` are `server.json`'s game server (no hash
    /// with a CA's certificate: checked by the browser, never pinned);
    /// `follow()` resolves to the live server's `[addr, hash]` when this one
    /// is on its way down (or null); `signin()` opens Elo Pros' popup and
    /// resolves to `{pass, wallet}`, or is null with `off` saying why.
    /// `name`, if not empty, is the name for this run.
    #[allow(clippy::too_many_arguments)]
    #[wasm_bindgen]
    pub fn launch(
        addr: String,
        hash: String,
        opts: String,
        name: String,
        mode: u8,
        signin: JsValue,
        off: String,
        follow: js_sys::Function,
    ) {
        let mut o = super::Opts::parse(&opts);
        o.front = if o.autoplay || o.fxdemo {
            super::front::Start::Join(mode)
        } else {
            super::front::Start::Title
        };
        let door = super::door::Door::new(addr, hash, follow, signin, off);
        let mut app = super::game(door, o);
        super::look::gpu::hook_panics();
        if !name.is_empty() {
            app.world_mut().resource_mut::<super::settings::Player>().name = super::settings::Player::clean(&name);
        }
        app.run();
    }

    /// The page's menu asks for a match (`page` in `launch`'s opts): as
    /// `name`, with an Elo Pros pass or the server's ticket (or nothing, a
    /// guest), `mode` in `skin`, with the `party` it comes with (its code;
    /// 0 for none). How it goes is `__oneshot.knocking`, `.front_status`,
    /// `.in_match`.
    #[wasm_bindgen]
    pub fn knock(name: String, pass: String, mode: u8, skin: u8, party: u32) {
        let ask = super::door::Ask {
            name,
            pass,
            mode,
            skin,
            party,
        };
        super::front::PAGE.with(|p| p.borrow_mut().push(super::front::PageCmd::Knock(ask)));
    }

    /// Stop waiting for a match (the queue, a cup).
    #[wasm_bindgen]
    pub fn cancel() {
        super::front::PAGE.with(|p| p.borrow_mut().push(super::front::PageCmd::Cancel));
    }

    /// Leave the match, back to the page's menu.
    #[wasm_bindgen]
    pub fn leave() {
        super::front::PAGE.with(|p| p.borrow_mut().push(super::front::PageCmd::Leave));
    }

    /// The page's `?pose`: no server, one fighter for a picture (`pose.rs`;
    /// `query` is the page's, as `?yaw=1.57&arm=3`), or with `fx=<name>`
    /// one effect on the fx stage (`look/fx/stage.rs`, `tools/fx.sh`).
    #[wasm_bindgen]
    pub fn pose(query: String, opts: String) {
        let pose = super::pose::Pose::parse(&query);
        let opts = super::Opts {
            pose: Some(pose),
            ..super::Opts::parse(&opts)
        };
        let stage = super::look::fx::stage::Stage::parse(&query);
        let mut app = super::app(Box::new(super::link::Still::on(pose.2.map)), opts);
        super::look::gpu::hook_panics();
        // `fx=<name>`: the fx stage instead of a fighter (`look/fx/stage.rs`).
        if let Some(s) = stage {
            super::look::fx::stage::install(&mut app, s);
        }
        app.run();
    }
}

//! The game client: Bevy on the desktop and in the browser
//! (docs/DESIGN.md §2, §16 milestone 12). **It draws, it never decides**:
//! everything it shows comes from client-net's session (the server's
//! snapshots and the prediction of our own fighter and vehicle), and all it
//! sends back is the controls.
//!
//! The level from the sim's map in Gates' light, sky, ground and grass
//! (docs/RIP.md §9); Triple T bodies with guns in hand; the gun in view,
//! effects and sound; a first-person camera (a chase camera at the wheel,
//! the death cam over your own body when dead); the three.js page's
//! controls; the HUD; and the game's own screens in Gates' style: the
//! title (`front`), the Esc menu (`pause`) and the settings (`options`),
//! the same on the desktop and in a browser, where the page
//! (`crates/client/web/index.html`) only finds the server and signs in, and
//! calls `launch()`.

pub mod asset_fetch;
pub mod binds;
pub mod challenges;
pub mod clip;
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod crash;
pub mod display;
pub mod door;
pub mod film;
pub mod flags;
pub mod front;
pub mod hud;
pub mod items;
pub mod level;
pub mod link;
pub mod loading;
pub mod look;
pub mod medals;
pub mod options;
pub mod pad;
pub mod party;
pub mod pause;
pub mod plate;
pub mod pointer;
pub mod pose;
pub mod scope;
pub mod settings;
pub mod shot;
pub mod sound;
pub mod tags;
pub mod tally;
pub mod ui;
pub mod visor;

use std::collections::BTreeMap;

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use sim::consts::{PITCH_MAX, VEH_H};
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
    /// Scoped in (`level` > 0, on a gun that has a scope).
    pub zoom: bool,
    /// The scope's level: 0 off, 1, 2 (the snipers' second). A click steps
    /// it, a hold lets go of it on release; a hit, a reload, a melee, a
    /// throw or another gun takes it off, as Halo 2's.
    pub level: u8,
    /// When the right button last went down (ms, the link's clock).
    scope_down: f64,
    /// Our own fighter last frame, to see what knocks the scope off.
    scope_was: Option<ScopeWas>,
    /// The trigger counts: not still held from the click that took the
    /// mouse (that click only takes it, as on the page).
    armed: bool,
    /// The scope's magnification as drawn: eased toward
    /// [`Look::magnification`] at [`ZOOM_RATE`] (`view`). The view narrows
    /// by it and the mouse slows by it; 0 until the first frame (read as 1).
    mag: f32,
}

/// How fast a scope comes up and goes down: `1 − exp(−k·dt)` a frame
/// (Gates' `VIEWMODEL_RAISE_RATE`): halfway to a 2x in 50 ms.
const ZOOM_RATE: f32 = 14.0;

/// What [`Look::level`] watches on our own fighter from frame to frame.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ScopeWas {
    arm: u8,
    cur: u8,
    vitals: i32,
    reloading: bool,
    meleeing: bool,
    throwing: bool,
}

/// A press held longer than this is a hold: the scope goes on release.
const SCOPE_HOLD_MS: f64 = 300.0;

impl Look {
    /// The magnification drawn: 1 unscoped.
    pub fn magnification(&self, arm: &sim::arms::Arm) -> f32 {
        if self.zoom {
            arm.zoom_at(self.level) as f32
        } else {
            1.0
        }
    }

    /// The magnification as drawn, eased (`mag`): what the view, the
    /// mouse and everything shown behind a scope go by.
    pub fn drawn_mag(&self) -> f32 {
        self.mag.max(1.0)
    }

    /// Behind the scope, as drawn: the eased zoom halfway to `arm`'s
    /// first level. The gun goes and the scope's ring comes together.
    pub fn scoped(&self, arm: &sim::arms::Arm) -> bool {
        let first = arm.zoom_at(1) as f32;
        first > 1.0 && self.drawn_mag() >= 1.0 + (first - 1.0) * 0.5
    }

    /// One frame's ease toward what `f` is: scoped (`level`). The crouch
    /// is the sim's own, over `CROUCH_TICKS` (`Body::eye_h`).
    fn ease(&mut self, f: &sim::fighter::Fighter, dt: f32) {
        let want = if f.alive {
            self.magnification(sim::arms::arm(f.gun().arm))
        } else {
            1.0
        };
        let mag = self.drawn_mag();
        self.mag = mag + (want - mag) * (1.0 - (-ZOOM_RATE * dt).exp());
        // A hair from there is there: no tail of 1.0001 a scope never
        // leaves.
        if (self.mag - want).abs() < 0.005 {
            self.mag = want;
        }
    }

    /// Out of the scope at once, no ease (a film's cut).
    pub fn unscope(&mut self) {
        self.zoom = false;
        self.level = 0;
        self.mag = 1.0;
    }
}

/// The link (by `front::LinkGen`) and welcome this level was built for.
#[derive(Resource, Default)]
pub(crate) struct Built(Option<(u32, u32)>);

/// Drawn things by their slot (a fighter with the side and look its body
/// was made in). Made once and kept: fog hiding someone hides their body or
/// hull, it doesn't throw it away, so their coming back costs nothing (a
/// skinned scene, its materials and shield shells, made mid-fight, was a
/// hitch).
#[derive(Resource, Default)]
struct Drawn {
    fighters: BTreeMap<u8, (Entity, u8, proto::Look)>,
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

/// A fighter's body or a vehicle's hull, shown or hidden by `Drawn`.
#[derive(Component)]
struct Kept;

/// The kept entities' visibility (never a shield shell's).
type KeptVis<'w, 's> = Query<
    'w,
    's,
    &'static mut Visibility,
    (
        With<Kept>,
        Without<look::shield::ShellPart>,
        Without<look::bodies::BodyPart>,
    ),
>;

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
    /// Holds the scope the whole way (`--scope`): a picture of one.
    pub scope: bool,
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

/// Radians per pixel of mouse at sensitivity 1 (`settings::SENS_DEG`).
/// Scoped, divided by the scope's zoom: the view narrows by it, so the
/// world moves under the crosshair at one speed (Gates' `input.rs`).
const SENS: f32 = settings::SENS_DEG * core::f32::consts::PI / 180.0;

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
    /// `softgpu`: the page found its WebGPU adapter a software one, which
    /// wgpu's WebGPU backend doesn't say (`look/potato.rs`).
    pub softgpu: bool,
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
    /// A party a link asked for (`party=CODE`, `--party CODE`): joined.
    pub party: u32,
}

impl Opts {
    /// From a comma list, as the page and the command line give it.
    pub fn parse(s: &str) -> Opts {
        let has = |k: &str| s.split([',', ' ']).any(|w| w == k);
        Opts {
            autoplay: has("autoplay"),
            lite: has("lite"),
            potato: has("potato"),
            softgpu: has("softgpu"),
            quality: s
                .split([',', ' '])
                .find_map(|w| w.strip_prefix("quality=").and_then(look::quality::Quality::from_name)),
            fxdemo: has("fxdemo") || s.contains("fxstage"),
            fxstage: s
                .split([',', ' '])
                .find_map(|w| w.strip_prefix("fxstage").and_then(|n| n.parse().ok())),
            pose: None,
            capsules: has("capsules"),
            party: s
                .split([',', ' '])
                .find_map(|w| w.strip_prefix("party="))
                .and_then(proto::party::code_parse)
                .unwrap_or(0),
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
    party::install(&mut app, opts.party);
    app
}

/// Switch off `bevy_render`'s `present_frames` span, the one span Bevy opens
/// every frame without its `trace` feature (Gates' `client-web`). In a
/// browser the logger answers a span with `performance.mark` and
/// `performance.measure`: two calls into JS a frame, and two User Timing
/// entries a frame that nothing clears, ~430,000 an hour, held by the tab.
/// A layer not interested in a callsite disables it for every layer, so the
/// span is never built at all.
fn quiet_frame_span(_: &mut App) -> Option<bevy::log::BoxedLayer> {
    use bevy::log::tracing::{subscriber::Interest, Metadata, Subscriber};
    use bevy::log::tracing_subscriber::layer::{Context, Layer};

    struct Quiet;
    fn frame_span(m: &Metadata<'_>) -> bool {
        m.is_span() && m.name() == "present_frames"
    }
    impl<S: Subscriber> Layer<S> for Quiet {
        fn register_callsite(&self, m: &'static Metadata<'static>) -> Interest {
            if frame_span(m) {
                Interest::never()
            } else {
                Interest::always()
            }
        }
        fn enabled(&self, m: &Metadata<'_>, _: Context<'_, S>) -> bool {
            !frame_span(m)
        }
    }
    Some(Box::new(Quiet))
}

/// The client on `link` with no title: a picture (`pose`), or the fx stage.
pub fn app(link: Box<dyn Link>, opts: Opts) -> App {
    app_on(link, opts, opts.open)
}

fn app_on(link: Box<dyn Link>, opts: Opts, first: Screen) -> App {
    // The asset loader's threads get a deep stack (Gates' `bin/gates.rs`):
    // `bevy_gltf` blocks on a scope while a model's textures load, and the
    // waiting IO thread runs the next queued load inside it, one nest of
    // frames per model in flight; Gates' boot overflowed the default 2 MiB
    // on a busy 4-core box. Same thread count Bevy picks; `TaskPoolPlugin`
    // keeps a pool that already exists. A browser has one thread and no pool.
    #[cfg(not(target_arch = "wasm32"))]
    bevy::tasks::IoTaskPool::get_or_init(|| {
        bevy::tasks::TaskPoolBuilder::default()
            .num_threads((bevy::tasks::available_parallelism() / 4).clamp(1, 4))
            .stack_size(32 << 20)
            .thread_name("IO Task Pool".to_string())
            .build()
    });
    let mut app = App::new();
    // Before `AssetPlugin`, which keeps a source already registered: Bevy's
    // wasm reader panics on a download that breaks mid-body, this one
    // retries it (`asset_fetch.rs`).
    #[cfg(target_arch = "wasm32")]
    app.register_asset_source(bevy::asset::io::AssetSourceId::Default, asset_fetch::source("assets"));
    app.add_plugins(
        DefaultPlugins
            .set(bevy::log::LogPlugin {
                custom_layer: quiet_frame_span,
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "oneshot".into(),
                    // WM_CLASS on X11, app_id on Wayland: what a desktop entry matches.
                    name: Some("oneshot".into()),
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
    .init_resource::<flags::Flags>()
    .init_resource::<challenges::Challenges>()
    .init_resource::<look::rag::Blows>()
    .init_resource::<look::rag::OwnRag>()
    .init_resource::<Early>()
    .init_resource::<Now>()
    .init_resource::<Seen>()
    .add_systems(First, stamp)
    .init_resource::<look::trees::TreeCards>()
    .init_resource::<look::viewmodel::ViewMuzzles>()
    .init_resource::<look::hands::Grips>()
    .add_plugins(sound::SoundPlugin)
    .add_plugins(ui::plugin)
    .add_plugins(plate::plugin)
    .add_plugins(settings::plugin)
    // The graphics ladder after the settings it reads: its tier exists
    // before anything is spawned with it.
    .add_plugins(look::quality::plugin)
    .add_plugins(look::potato::plugin)
    .add_plugins(look::render_scale::plugin)
    .add_plugins(visor::plugin)
    .add_plugins(display::plugin)
    .insert_state(first)
    .add_plugins(pause::plugin)
    .add_plugins(options::plugin)
    .add_plugins(pad::plugin)
    .add_plugins(loading::plugin)
    .add_plugins(look::shot::plugin)
    // F9's clip of the last half minute, and `--record` (`clip.rs`).
    .add_plugins(clip::plugin)
    .add_plugins(MaterialPlugin::<look::ground::GroundMaterial>::default())
    // The boxes' weathering (`look/weathering.rs`).
    .add_plugins(look::weathering::plugin)
    // A truck's wear (`look/truck.rs`).
    .add_plugins(look::truck::plugin)
    .add_plugins(look::cloud_sea::plugin)
    // The grass's and the clutter's wind, trails and fade (`look/foliage.rs`).
    .add_plugins(look::foliage::plugin)
    // Triple T's spray paint: the side's headband, the emblem, the trim
    // (`look/paint.rs`).
    .add_plugins(look::paint::plugin)
    .add_plugins(MaterialPlugin::<look::shield::ShieldMaterial>::default())
    .add_plugins(MaterialPlugin::<look::fx::sheets::SheetMaterial>::default())
    .add_plugins(MaterialPlugin::<look::fx::shell::ShockMaterial>::default())
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
    .init_resource::<look::fx::ambient::Ambient>()
    .init_resource::<look::shafts::WebShafts>()
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
            look::boulders::load,
            look::guns::load,
            look::viewmodel::kit,
            look::counter::setup,
            look::icons::setup.after(look::guns::load),
            arms_setup
                .after(setup)
                .after(look::guns::load)
                .after(look::viewmodel::kit),
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
                look::shafts::paint_web,
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
            look::prewarm::bake_roster,
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
            (pump, see).chain(),
            happened,
            rebuild,
            look::textures::prepare,
            (look::textures::mip_models, look::fx::sheets::mip_sheets),
            (look::rock::dress_masses, look::boulders::dress),
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
            (draw_fighters, draw_vehicles, look::truck::drive, look::foliage::update).chain(),
            (items::draw, flags::draw),
            fly,
            view,
            // The gun in view before the effects: our own flash sits on
            // its muzzle as posed this frame (`fx/shots.rs`).
            (draw_view, look::counter::draw).chain(),
            look::viewmodel::shimmer,
            (look::fx::ambient::drive, look::fx::draw).chain(),
            // The HUD is hidden off a match (`front::hud_shown`): nothing
            // to draw into it there.
            hud::draw.run_if(in_match),
            (hud::gear, hud::gun_icons, hud::vitals, hud::corner, scope::draw).run_if(in_match),
            hud::hurt_rim.run_if(in_match),
        )
            .chain(),
    );
    // Not gated: the medal row and the carnage report are their own roots,
    // and clear themselves once the match is left.
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
            look::portrait::fit,
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
        (look::bodies::pose_spines, look::rag::step, look::bodies::pose_hands)
            .chain()
            .after(bevy::app::AnimationSystems)
            .before(bevy::transform::TransformSystems::Propagate),
    )
    // Our own arms in view, onto the guns as `draw_view` posed them.
    .add_systems(Update, (look::hands::build, look::icons::develop))
    .add_systems(
        PostUpdate,
        look::hands::pose
            .after(bevy::app::AnimationSystems)
            .before(bevy::transform::TransformSystems::Propagate),
    )
    // Names over heads, after the camera is placed (`tags.rs`).
    .add_systems(Update, tags::draw.after(view).after(hud::draw).run_if(in_match));
    #[cfg(target_arch = "wasm32")]
    app.add_systems(Update, expose.after(hud::draw))
        .add_systems(PostUpdate, pointer::follow);
    app.insert_resource(opts);
    if opts.graphics {
        app.world_mut().resource_mut::<options::Page>().cat = options::GRAPHICS;
    }
    if let Some(p) = opts.pose {
        app.insert_resource(p)
            .add_systems(Update, (pose::aim.after(view), pose::fall.before(draw_fighters)));
    } else {
        app.add_systems(
            Startup,
            (
                hud::setup,
                medals::setup,
                scope::setup.after(hud::setup),
                visor::setup.after(hud::setup),
            ),
        )
        .add_systems(
            Update,
            (hud::prompt, hud::reticle_tint, hud::board)
                .after(hud::draw)
                .run_if(in_match),
        );
    }
    if opts.fxdemo {
        app.add_systems(Update, look::fx::demo::run.after(happened).before(look::fx::draw));
    }
    app
}

/// In a match: what the HUD and the names over heads draw for.
fn in_match(m: Res<front::InMatch>) -> bool {
    m.0
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut media: ResMut<Assets<bevy::pbr::ScatteringMedium>>,
) {
    let eye = look::rig::camera(&mut commands, &mut images, &mut media, Eye);
    look::viewmodel::spawn(&mut commands, eye);
}

/// Our own arms in view, under the eye, once the models are asked for.
fn arms_setup(mut commands: Commands, models: Res<look::guns::Models>, eye: Query<Entity, With<Eye>>) {
    if let Ok(eye) = eye.single() {
        look::hands::spawn(&mut commands, &models, eye);
    }
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
    // Watching a match: we look where the player we follow looks (their
    // snapshots' aim, as a film does), and a click or Space follows the
    // next one. Nothing we press is sent.
    if link.0.session().watch && link.0.live() {
        let next = grabbed && (mouse.just_pressed(MouseButton::Left) || keys.just_pressed(KeyCode::Space));
        if next {
            link.0.session_mut().follow_next();
        }
        let (f, _) = link.0.session().core.own();
        let turn = core::f32::consts::TAU / 65536.0;
        look.yaw = f.yaw as f32 * turn;
        look.pitch = f.pitch as f32 * turn;
        look.zoom = f.alive && f.last_buttons & ZOOM != 0 && sim::arms::arm(f.gun().arm).zoom > 1;
        link.0.update(Input::default());
        return;
    }
    let held = |a: binds::Action| grabbed && (binds.pressed(a, &keys, &mouse) || binds.just_pressed(a, &keys, &mouse));
    let button = |b: MouseButton| grabbed && (mouse.pressed(b) || mouse.just_pressed(b));
    // A gun in each hand: the right button is the left hand's trigger,
    // not the scope.
    let now = link.0.now_ms();
    let (dual, arm, driving, was) = {
        let f = link.0.session().core.own().0;
        let driving = f.riding().is_some_and(|(_, s)| s == sim::vehicle::DRIVER);
        let was = ScopeWas {
            arm: f.gun().arm,
            cur: f.cur,
            vitals: f.shield + f.health,
            reloading: f.reload > 0,
            meleeing: f.melee_cool > 0,
            throwing: f.gren_cool > 0,
        };
        (f.dual(), sim::arms::arm(f.gun().arm), driving, f.alive.then_some(was))
    };
    // What takes the scope off (Halo 2's): any damage, a reload (a hot
    // gun's vent), a melee, a throw, another gun, dying; and there's none
    // at the wheel (the chase camera doesn't narrow) or with a gun in each
    // hand (the right button is the left hand's trigger).
    let knocked = match (look.scope_was, was) {
        (Some(a), Some(b)) => {
            b.vitals < a.vitals
                || a.arm != b.arm
                || a.cur != b.cur
                || (b.reloading && !a.reloading)
                || (b.meleeing && !a.meleeing)
                || (b.throwing && !a.throwing)
        }
        _ => true,
    };
    look.scope_was = was;
    let levels = arm.zoom_levels();
    if knocked || dual || driving || levels == 0 {
        look.level = 0;
    }
    if auto.on {
        look.level = if auto.scope && was.is_some() && !dual && !driving {
            levels.min(1)
        } else {
            0
        };
    } else if levels > 0 && !dual && !driving && was.is_some() {
        if grabbed && mouse.just_pressed(MouseButton::Right) {
            look.level = (look.level + 1) % (levels + 1);
            look.scope_down = now;
        } else if !mouse.pressed(MouseButton::Right) && look.scope_down > 0.0 {
            if now - look.scope_down > SCOPE_HOLD_MS {
                look.level = 0;
            }
            look.scope_down = 0.0;
        }
    }
    if !mouse.pressed(MouseButton::Right) && !mouse.just_pressed(MouseButton::Right) {
        look.scope_down = 0.0;
    }
    look.zoom = look.level > 0;
    if grabbed {
        // Slowed as the view narrows, with it (`view`): the world moves
        // under the crosshair at one speed while the scope comes up.
        let base = SENS * settings.sensitivity;
        let sens = base / look.drawn_mag();
        let up = if settings.invert { -1.0 } else { 1.0 };
        // Kept to a turn: an f32 summed without end loses the low bits a
        // slow hand moves it by.
        look.yaw = (look.yaw + motion.delta.x * sens).rem_euclid(core::f32::consts::TAU);
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
    let eye = f.body.eye_h();
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
/// pieces; this one needs fifteen.)
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
    mut cloud_mats: ResMut<Assets<look::cloud_sea::CloudSeaMaterial>>,
    mut cards: ResMut<look::trees::TreeCards>,
    old: Query<Entity, With<level::LevelPart>>,
    mut eye: Query<(&mut bevy::camera::Exposure, &mut bevy::light::EnvironmentMapLight), With<Eye>>,
    mut shade: ResMut<look::quality::MapShade>,
    mut mood: ResMut<look::mood::MoodNow>,
    fx: Option<ResMut<look::fx::Fx>>,
) {
    let s = link.0.session();
    let now = Some((gen.map_or(0, |g| g.0), s.welcomes));
    if !s.ready || built.0 == now {
        return;
    }
    built.0 = now;
    // The last level's marks and debris were fitted to its faces: a cup's
    // next heat on the same link may be another map.
    if let Some(mut fx) = fx {
        fx.marks.clear();
        fx.chips.clear();
    }
    // The map's mood first: the level's sun and its wet are the mood's.
    look::mood::set(&mut mood, s.map_id);
    level::build(
        &mut commands,
        &mut meshes,
        &mut mats,
        &mut water_mats,
        &mut cloud_mats,
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
    look::bodies::Parts<'w, 's>,
    ResMut<'w, Assets<look::paint::BodyMaterial>>,
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
    (mut fx, mut shield_mats, mut shells, mut body_mats): ShieldParts,
    (time, mut walkers, film, seen): (
        Res<Time>,
        ResMut<look::foliage::Walkers>,
        Option<Res<film::Film>>,
        Res<Seen>,
    ),
    mut scratch: Local<FighterScratch>,
) {
    let core = &link.0.session().core;
    // A film's camera off the player's eye sees the player's own body
    // (`film.rs`).
    let third = film.is_some_and(|f| f.third());
    let FighterScratch { others, hulls, shown } = &mut *scratch;
    others.clear();
    let live = match &posed {
        Some(p) => {
            others.extend(p.2.fighter.then_some(p.0));
            false
        }
        None => {
            others.extend_from_slice(&seen.others);
            true
        }
    };
    if third && link.0.session().ready {
        others.push(core.own_drawn());
    }
    hulls.clear();
    hulls.extend_from_slice(&seen.vehicles);
    // Our own seat, when a film draws us in it.
    if let (true, Some((_, at, yaw)), Some((i, seat))) = (third, core.own_vehicle(), core.own().0.riding()) {
        let mut seats = [0; 2];
        seats[seat.min(1)] = core.slot + 1;
        hulls.push(client_net::core::DrawnVehicle {
            slot: i as u8,
            pos: at,
            yaw,
            seats,
            ..client_net::core::DrawnVehicle::default()
        });
    }
    // The pose mode's truck, with its fighter in the seat it was given.
    if let Some((slot, (at, yaw), seat)) = posed.as_ref().and_then(|p| Some((p.0.slot, p.2.truck?, p.2.seat?))) {
        let mut seats = [0; 2];
        seats[seat] = slot + 1;
        hulls.push(client_net::core::DrawnVehicle {
            pos: sim::geom::V3::new(at.x, at.y, at.z),
            yaw,
            seats,
            ..client_net::core::DrawnVehicle::default()
        });
    }
    let seats = look::bodies::seats(hulls);
    look::fx::bodies::watch(&mut fx, others, time.delta_secs(), time.elapsed_secs());
    let (f, p) = core.own();
    // The grass flattens under the feet this client draws on the ground,
    // ours first: never a seat.
    walkers.feet.clear();
    if live && !third && f.alive && f.body.grounded && f.riding().is_none() {
        walkers.fighter(Vec3::new(p.x, p.y, p.z));
    }
    if live {
        for d in others
            .iter()
            .filter(|d| d.alive && d.grounded && seats.get(d.slot as usize).is_none_or(|s| s.is_none()))
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
        f.alive.then(|| core.own_drawn().shield),
        time.elapsed_secs(),
    );
    let t = time.elapsed_secs();
    let mut kept = |d: &client_net::core::Drawn, look: proto::Look| match drawn.fighters.get(&d.slot) {
        Some(&(e, team, l)) if team == d.team && l == look => e,
        had => {
            if let Some(&(old, ..)) = had {
                commands.entity(old).despawn();
            }
            let e = if opts.capsules {
                look::bodies::spawn_capsule(&mut commands, &mut meshes, &mut mats, d.team)
            } else {
                look::bodies::spawn(&mut commands, &models, d.team, look)
            };
            commands.entity(e).insert(Kept);
            drawn.fighters.insert(d.slot, (e, d.team, look));
            e
        }
    };
    // Our own body, posed every frame where we stand and hidden while we
    // are in it, so the frame we die it is shown and goes limp from the
    // pose we died in (`rag.rs`), flying as the blow throws it, for the
    // death cam (`view`) to watch until we respawn.
    let own = (live && !third && link.0.session().ready).then(|| {
        let d = core.own_drawn();
        (d, kept(&d, st.look(d.slot)))
    });
    shown.clear();
    shown.extend(
        others
            .iter()
            .map(|d| (*d, kept(d, posed.as_ref().map_or_else(|| st.look(d.slot), |p| p.1)))),
    );
    if let Some((d, e)) = own {
        let _ = show(&mut vis, e, !d.alive);
        shown.push((d, e));
    }
    for (d, e) in shown.iter() {
        let (d, e) = (d, *e);
        if Some(d.slot) != own.map(|(o, _)| o.slot) {
            let _ = show(&mut vis, e, true);
        }
        if opts.capsules {
            look::bodies::pose_capsule(d, e, &mut at);
            continue;
        }
        look::bodies::pose(
            &mut commands,
            &models,
            d,
            seats.get(d.slot as usize).copied().flatten().map(|(v, k)| (v.yaw, k)),
            e,
            &mut bodies,
            &mut at,
            time.delta_secs(),
        );
        let glow = look::bodies::Glow {
            hit: fx.was.flare(d.slot, t),
            over: d.over && d.alive,
            cloak: if d.alive { d.cloak } else { 0.0 },
        };
        look::bodies::flare(e, glow, t, &mut bodies, &mut body_mats, &mut shield_mats, &mut shells);
    }
    for (slot, (e, ..)) in &drawn.fighters {
        if !shown.iter().any(|(d, _)| d.slot == *slot) && show(&mut vis, *e, false) {
            // Shown again, it starts its clip afresh, not fading from the
            // pose it was hidden in (a corpse doesn't get up off the
            // floor).
            if let Ok(mut b) = bodies.get_mut(*e) {
                b.afresh();
            }
        }
    }
}

/// `draw_fighters`' lists, kept from frame to frame.
#[derive(Default)]
struct FighterScratch {
    others: Vec<client_net::core::Drawn>,
    hulls: Vec<client_net::core::DrawnVehicle>,
    shown: Vec<(client_net::core::Drawn, Entity)>,
}

/// How high over the ground a hull's origin may be and still have its
/// wheels on it (the sim's suspension is a few centimetres).
const WHEELS_DOWN_M: f32 = 0.3;

/// The vehicles: ours as predicted, the rest interpolated; each a truck
/// (`look/truck.rs`). The pose mode's stands where it was asked to.
#[allow(clippy::too_many_arguments)]
fn draw_vehicles(
    link: NonSend<LinkRes>,
    mut drawn: ResMut<Drawn>,
    mut commands: Commands,
    models: Res<look::guns::Models>,
    posed: Option<Res<pose::Pose>>,
    mut at: Query<&mut Transform>,
    mut hulls: Query<&mut look::truck::Hull>,
    mut vis: KeptVis,
    mut fx: ResMut<look::fx::Fx>,
    time: Res<Time>,
    mut walkers: ResMut<look::foliage::Walkers>,
    seen: Res<Seen>,
    mut list: Local<Vec<(u8, Vec3, f32)>>,
) {
    let core = &link.0.session().core;
    let seen = &seen.vehicles;
    list.clear();
    list.extend(
        seen.iter()
            .map(|v| (v.slot, Vec3::new(v.pos.x, v.pos.y, v.pos.z), v.yaw)),
    );
    if let (Some((_, p, yaw)), Some((i, _))) = (core.own_vehicle(), core.own().0.riding()) {
        list.push((i as u8, Vec3::new(p.x, p.y, p.z), yaw));
    }
    if let Some((p, yaw)) = posed.and_then(|p| p.2.truck_at(time.elapsed_secs())) {
        list.push((0, p + Vec3::Y * core.map.ground(p.x, p.z), yaw));
    }
    for &(slot, p, yaw) in list.iter() {
        if let Some(&e) = drawn.vehicles.get(&slot) {
            let _ = show(&mut vis, e, true);
        }
        let health = seen.iter().find(|v| v.slot == slot).map_or(255, |v| v.health);
        // Only a live hull on its wheels presses the grass: not one in the
        // air over a crest, nor a wreck.
        if health > 0 && p.y - core.map.ground(p.x, p.z) < WHEELS_DOWN_M {
            walkers.hull(p, yaw, &look::truck::WHEELS);
        }
        look::fx::hulls::roll(&mut fx, slot, p, yaw, health, time.delta_secs());
        let t = Transform::from_translation(p).with_rotation(Quat::from_rotation_y(-yaw));
        match drawn.vehicles.get(&slot) {
            Some(&e) => {
                // A parked hull stays put: its wheels and body don't go
                // round the transforms again.
                if let Ok(mut tr) = at.get_mut(e) {
                    tr.set_if_neq(t);
                }
                if let Ok(mut hull) = hulls.get_mut(e) {
                    if hull.health != health {
                        hull.health = health;
                    }
                }
            }
            None => {
                let hull = look::truck::Hull { slot, health };
                let e = commands.spawn((t, Visibility::default(), Kept, hull)).id();
                look::truck::spawn(&mut commands, &models, e);
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

/// The death cam (Halo 2's): how far back from our body it sits, metres,
/// how far over it it looks down (radians below where we look), how long
/// it takes to pull back out of the eye, seconds, and how near a wall or
/// the ground it may come, metres.
const DEATH_CAM_M: f32 = 3.4;
const DEATH_CAM_DOWN: f32 = 0.42;
const DEATH_CAM_POP_S: f32 = 0.55;
const DEATH_CAM_PAD_M: f32 = 0.35;
/// The death cam follows the body this fast, 1/s.
const DEATH_CAM_FOLLOW: f32 = 14.0;

/// Where the death cam started (the eye we died in), when, and what it
/// watches.
#[derive(Default)]
pub(crate) struct DeathCam {
    from: Option<(Transform, f32)>,
    at: Vec3,
}

/// From our eye, or behind our vehicle at the wheel; dead, the death cam:
/// out of the eye and back over our own body as it flies and falls, turned
/// about it by the mouse, until we respawn.
#[allow(clippy::too_many_arguments)]
pub(crate) fn view(
    link: NonSend<LinkRes>,
    mut look: ResMut<Look>,
    mut fx: ResMut<look::fx::Fx>,
    time: Res<Time>,
    opts: Res<Opts>,
    settings: Res<settings::Settings>,
    mut cam: Query<(&mut Transform, &mut Projection), With<Eye>>,
    mut demo_at: Local<Option<(u8, Transform)>>,
    (rag, mut death): (Res<look::rag::OwnRag>, Local<DeathCam>),
) {
    let Ok((mut t, mut proj)) = cam.single_mut() else {
        return;
    };
    let core = &link.0.session().core;
    let (f, p) = core.own();
    let dir = |yaw: f32, pitch: f32| Vec3::new(pitch.cos() * yaw.cos(), pitch.sin(), pitch.cos() * yaw.sin());
    let mut fov = look::rig::fov_tall(settings.fov);
    let at = Vec3::new(p.x, p.y, p.z);
    let dead = !f.alive && link.0.session().ready;
    if !dead {
        death.from = None;
    }
    // The scope, eased toward what it is.
    look.ease(f, time.delta_secs());
    if dead {
        let now = time.elapsed_secs();
        // The body: where the rag doll has it, else where we fell.
        let body = rag.0.unwrap_or(at + Vec3::Y * 0.9);
        if death.from.is_none() {
            death.from = Some((*t, now));
            death.at = body;
        }
        let (from, since) = death.from.unwrap_or((*t, now));
        let k = 1.0 - (-DEATH_CAM_FOLLOW * time.delta_secs()).exp();
        death.at = death.at.lerp(body, k);
        let aim = dir(look.yaw, (look.pitch - DEATH_CAM_DOWN).clamp(-1.3, 0.4));
        let v = |a: Vec3| sim::geom::V3::new(a.x, a.y, a.z);
        let room = core.map.ray(v(death.at), v(-aim), DEATH_CAM_M);
        let back = (room - DEATH_CAM_PAD_M).max(0.5);
        let eye = death.at - aim * back;
        let out = Transform::from_translation(eye).looking_at(death.at, Vec3::Y);
        let u = ((now - since) / DEATH_CAM_POP_S).clamp(0.0, 1.0);
        let u = u * u * (3.0 - 2.0 * u);
        *t = Transform {
            translation: from.translation.lerp(out.translation, u),
            rotation: from.rotation.slerp(out.rotation, u),
            ..*t
        };
    } else if f.riding().is_some_and(|(_, seat)| seat == sim::vehicle::DRIVER) {
        let eye = at - dir(look.yaw, look.pitch) * 7.5 + Vec3::Y * (VEH_H + 2.2);
        *t = Transform::from_translation(eye).looking_at(at + Vec3::Y * VEH_H, Vec3::Y);
    } else {
        // The sim's eye: a crouch already goes down over a moment there
        // (`sim::movement::Body::duck`), and a tuck in the air keeps it.
        let eye = at + Vec3::Y * f.body.eye_h();
        *t = Transform::from_translation(eye).looking_at(eye + dir(look.yaw, look.pitch), Vec3::Y);
        fov /= look.drawn_mag();
    }
    // The effects demo stands its camera in one place (drawing only).
    // Worked out once a map, not every frame.
    if opts.fxdemo {
        let id = link.0.session().map_id;
        if demo_at.is_none_or(|(m, _)| m != id) {
            *demo_at = Some((id, look::fx::demo::pose(&core.map, opts.fxstage)));
        }
        if let Some((_, at)) = *demo_at {
            *t = at;
        }
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
    film: Option<Res<film::Film>>,
    mut grips: ResMut<look::hands::Grips>,
    mut lag: ResMut<visor::HudLag>,
) {
    grips.team = link.0.session().core.team;
    let (f, p) = link.0.session().core.own();
    // The gun goes once the scope is halfway up.
    let scoped = look.scoped(sim::arms::arm(f.gun().arm));
    let seat = f.riding().map(|(_, seat)| seat);
    let own = look::viewmodel::Own {
        f,
        at: Vec3::new(p.x, p.y, p.z),
        yaw: look.yaw,
        pitch: look.pitch,
        riding: seat.is_some(),
        shown: f.alive && seat != Some(sim::vehicle::DRIVER) && !scoped && !film.is_some_and(|f| f.third()),
    };
    look::viewmodel::update(
        &mut commands,
        &models,
        &mut grips,
        own,
        time.delta_secs(),
        &mut motion,
        &mut muzzles,
        &mut hold,
        &mut flash,
    );
    let (trail, heave) = motion.trail();
    *lag = visor::HudLag { trail, heave };
}

/// What the session queued for us this frame: events become effects and
/// HUD marks, stream messages names and the feed.
/// The netcode's clock for this frame, read once: what is drawn at the
/// render tick and the events played on it agree to the frame (a round
/// goes the frame its blast plays).
#[derive(Resource, Default)]
pub(crate) struct Now(pub f64);

fn stamp(link: NonSend<LinkRes>, mut now: ResMut<Now>) {
    now.0 = link.0.now_ms();
}

/// Everyone else, the hulls and the rounds in flight as drawn this frame:
/// interpolated once, at [`Now`], once the frame's snapshots are in
/// (`pump`), for every system that draws, tags, aims at or hears them. Its
/// Vecs are kept and refilled: nothing is made again a frame.
#[derive(Resource, Default)]
pub struct Seen {
    pub others: Vec<client_net::core::Drawn>,
    pub vehicles: Vec<client_net::core::DrawnVehicle>,
    pub projectiles: Vec<client_net::core::DrawnProj>,
    /// Our aim is on an enemy in reach (`hud::on_target`): the red reticle,
    /// the scope's ring.
    pub on_target: bool,
}

fn see(link: NonSend<LinkRes>, now: Res<Now>, mut seen: ResMut<Seen>) {
    let s = link.0.session();
    let seen = &mut *seen;
    s.core.others_into(now.0, &mut seen.others);
    s.core.vehicles_into(now.0, &mut seen.vehicles);
    s.core.projectiles_into(now.0, &mut seen.projectiles);
    seen.on_target = s.ready && hud::on_target(&s.core, &seen.others);
}

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
    film: Option<Res<film::Film>>,
    (mut blows, now, seen, mut others): (
        ResMut<look::rag::Blows>,
        Res<Now>,
        Res<Seen>,
        Local<Vec<client_net::core::Drawn>>,
    ),
) {
    let now_ms = now.0;
    let s = link.0.session_mut();
    // Into the Vec the sounds read this frame (`sound::Heard`), kept.
    let heard = &mut heard.0;
    s.core.drain_events_into(now_ms, heard);
    let events = &*heard;
    let mut msgs = core::mem::take(&mut early.0);
    msgs.extend(s.take_messages());
    // A film's camera off the eye sees our shots as anyone's: from the
    // drawn body, not the gun in view (`film.rs`).
    let third = film.is_some_and(|f| f.third());
    {
        others.clear();
        others.extend_from_slice(&seen.others);
        if third {
            others.push(s.core.own_drawn());
        }
        let others = &*others;
        let vehicles = &seen.vehicles;
        let dir = Vec3::new(
            look.pitch.cos() * look.yaw.cos(),
            look.pitch.sin(),
            look.pitch.cos() * look.yaw.sin(),
        );
        fx.own_swing(s.core.own().0.melee_cool, dir);
        let scene = look::fx::Scene {
            map: &s.core.map,
            others,
            vehicles,
            me: if third { u8::MAX } else { s.core.slot },
            aim: dir,
            t: time.elapsed_secs(),
        };
        look::fx::on_events(&mut fx, &scene, events);
        let me = s.core.own().1;
        look::rag::on_events(
            &mut blows,
            events,
            others,
            (s.core.slot, Vec3::new(me.x, me.y, me.z)),
            time.elapsed_secs(),
        );
        // Our own gun, drawn the frame it fires, down where we look.
        let (f, p) = s.core.own();
        // From the eye as drawn, as the camera is.
        let eye = Vec3::new(p.x, p.y + f.body.eye_h(), p.z);
        let cam = Transform::from_translation(eye).looking_at(eye + dir, Vec3::Y).rotation;
        if third {
            look::fx::shots::own_unseen(&mut fx, f);
        } else {
            look::fx::shots::own(&mut fx, &scene, f, eye, cam, muzzles.0);
        }
    }
    hud::on_events(&mut st, s, &seen.others, events, &msgs, time.elapsed_secs(), look.yaw);
}

/// `expose` writes every frame for this many, then once in this long.
#[cfg(target_arch = "wasm32")]
const EXPOSE_EVERY_FRAME: u32 = 8;
#[cfg(target_arch = "wasm32")]
const EXPOSE_EVERY_S: f64 = 0.1;

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
    party: Option<Res<party::PartyView>>,
    ear: Res<sound::Ear>,
    real: Res<Time<Real>>,
    (mut frames, mut described, mut wrote): (Local<u32>, Local<Option<String>>, Local<f64>),
) {
    use wasm_bindgen::JsValue;
    *frames += 1;
    // A hundred-odd calls into JS and a fresh JS string for every key: the
    // page's gates poll it, so every frame for the splash, then a few times a
    // second, not every frame of a match.
    let now = real.elapsed_secs_f64();
    if *frames > EXPOSE_EVERY_FRAME && now - *wrote < EXPOSE_EVERY_S {
        return;
    }
    *wrote = now;
    // Written again only when the tier moves (a row, the frame-rate watch).
    if tier.is_changed() || described.is_none() {
        *described = Some(tier.describe());
    }
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
    // The mixer: clips decoded, still loading, and commands sent it; and
    // the lines about the match the announcer has said.
    let (decoded, loading, sent, said) = ear.stats();
    set(
        &o,
        "sound",
        js_sys::Array::of4(&decoded.into(), &loading.into(), &sent.into(), &said.into()).into(),
    );
    set(&o, "in_match", matched.into());
    // The party as this page sees it (ci/party.mjs).
    if let Some(p) = party {
        let po = js_sys::Object::new();
        set(&po, "connected", p.connected.into());
        set(&po, "code", p.code_text().into());
        set(&po, "n", (p.members().len() as u32).into());
        set(&po, "leads", p.leads().into());
        let names = js_sys::Array::new();
        for m in p.members() {
            names.push(&JsValue::from_str(m.name.as_str()));
        }
        set(&po, "names", names.into());
        let (heard, sent) = party::voice_counts();
        set(&po, "heard", heard.into());
        set(&po, "sent", sent.into());
        set(&o, "party", po.into());
    }
    // Which module this is and what it draws (`look/gpu.rs`).
    set(&o, "gfx", look::gpu::api().into());
    set(&o, "tier", described.as_deref().unwrap_or_default().into());
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
    // Why it closed, as the HUD says it (`Link::ended_why`).
    if live && link.0.closed() {
        set(&o, "reason", link.0.ended_why().unwrap_or_default().into());
    }
    // For a bug report (boot.js): which match (its salt's commitment,
    // which the server's review files carry), where and when in it, on
    // which wire and build.
    set(&o, "map", s.map_id.into());
    set(&o, "tick", s.core.tick().into());
    set(&o, "proto", proto::PROTO_VER.into());
    set(&o, "version", env!("CARGO_PKG_VERSION").into());
    if live && s.ready {
        use std::fmt::Write as _;
        let mut hex = String::with_capacity(64);
        for b in s.core.salt_commit {
            let _ = write!(hex, "{b:02x}");
        }
        set(&o, "match", hex.into());
    }
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
#[allow(clippy::too_many_arguments)]
fn fly(
    link: NonSend<LinkRes>,
    models: Res<look::guns::Models>,
    time: Res<Time>,
    seen: Res<Seen>,
    mut fx: ResMut<look::fx::Fx>,
    mut commands: Commands,
    mut have: Query<(Entity, &mut look::fx::flying::Flying, &mut Transform, &mut Visibility)>,
) {
    let core = &link.0.session().core;
    // On the render tick, with the bodies that fired them (`core.rs`).
    look::fx::flying::projectiles(
        &mut commands,
        &mut fx,
        &models,
        &core.map,
        &seen.projectiles,
        &seen.others,
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

    /// A clip drawn into `canvas` and handed to `sink` as it is drawn: the
    /// page's `oneshot --clip` (`web/clip.js`, `film/`). `tape` is what the
    /// clip key gave the page (`window.__oneshotClip`, `clip.rs`); `style`
    /// one of `clip::STYLES`, or a whole shot list as JSON (`at` from the
    /// end when negative). The canvas is the list's size in CSS pixels at a
    /// scale of one: the page shrinks its box for a preview, never the
    /// canvas.
    ///
    /// `sink` is an object the film calls as it goes:
    /// `shot(i, name, drawnW, drawnH, outW, outH, fps, still)`,
    /// `frame(i, n, rgba, w, h)` in order, `audio(i, pcm)` (interleaved
    /// stereo at 48 kHz, a frame's worth a call), `shotEnd(i, frames)`,
    /// `progress(f, i)`, then `done()` or `fail(why)`. Like `launch`, it
    /// doesn't return: a page holds one Bevy app, so a clip draws in a
    /// frame of its own.
    #[wasm_bindgen]
    pub fn film(tape: Vec<u8>, style: String, canvas: String, sink: JsValue, quality: String) -> Result<(), JsValue> {
        use super::{clip, film};
        let tape = client_net::tape::read(&tape).map_err(|e| JsValue::from_str(&format!("not a clip: {e}")))?;
        let json = if style.trim_start().starts_with('{') {
            style
        } else {
            clip::script(style.trim()).ok_or_else(|| {
                JsValue::from_str(&format!("no clip style {style:?}: one of {}", clip::STYLES.join(", ")))
            })?
        };
        let mut script = film::Script::parse(&json).map_err(|e| JsValue::from_str(&format!("shot list: {e}")))?;
        // The player's rung, from the page (`clip::take`): a film beside a
        // match draws no more than the match does.
        if let Some(q) = super::look::quality::Quality::from_name(quality.trim()) {
            script.quality = q.name().into();
        }
        script.anchor(tape.end_ms() / 1000.0);
        let mut app = film::app(script, tape, Some(canvas)).map_err(|e| JsValue::from_str(&e))?;
        app.insert_non_send_resource(film::encode::Sink(sink));
        super::look::gpu::hook_panics();
        app.run();
        Ok(())
    }

    /// The clip styles a page may offer (`clip::STYLES`).
    #[wasm_bindgen]
    pub fn clip_styles() -> Vec<String> {
        super::clip::STYLES.iter().map(|s| s.to_string()).collect()
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

//! The title screen, after Gates' front end (`render/menu.rs`, the shell in
//! `render/ui.rs`, and the page it was drawn from,
//! `client-web/web/index.html`): the wordmark top-left, a nav column whose
//! type is its own button, and beside it the play panel (who you are, which
//! mode, PLAY) and an open stage: Triple T in the skin picked
//! (`look/portrait.rs`), and what the picked mode is. Behind
//! it all one of our maps, live, under Gates' wash, with the camera drifting
//! (Gates plays footage; we have the level).
//!
//! **Connecting is a state of this screen** (`door.rs`), as in Gates: PLAY
//! knocks, the status line says what the stream says while it waits (the
//! queue, a cup, a refusal), and the welcome swaps the link in and the match
//! starts. A refusal or a dead server lands back here with the reason. When
//! the match ends, or the Esc menu leaves it, the link goes back to the
//! backdrop and the title comes back.
//!
//! Bevy still doesn't decide: this owns a name, a skin, a picked mode and a
//! line of text, and hands the session to the app the moment there is one.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};

use crate::door::{self, Ask, Door, News, SignIn};
use crate::party::{self, Do, Friend, Mic, PartyAsks, PartyView, TalkLight};
use crate::settings::{Player, NAME_MAX};
use crate::{hud, link, ui, LinkRes, Screen};
use proto::party as pp;

/// How the app starts: no title (a picture), the title, or straight into a
/// mode (the autopilot, the effects demo).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Start {
    #[default]
    Off,
    Title,
    Join(u8),
}

/// The map behind the title: Chrysalis, its hall under the lamps.
pub const BACKDROP: u8 = 6;
/// Where the title's camera stands on it, and what it looks at; it drifts
/// [`SWAY`] metres either side over [`DRIFT_S`] (the hall is 58 m across).
const SHOT_EYE: Vec3 = Vec3::new(-12.0, 10.0, -16.0);
const SHOT_LOOK: Vec3 = Vec3::new(6.0, 7.5, 6.0);
const SWAY: f32 = 2.5;
const DRIFT_S: f32 = 70.0;
/// The curtain over the backdrop until its level is there
/// (`loading::Settle`), then how long it holds and how fast it lifts; and
/// the guard: lifted this long after the level is built whatever is still
/// arriving (the title over it works throughout).
const CURTAIN_HOLD_S: f64 = 0.15;
const CURTAIN_FADE_S: f64 = 0.9;
const CURTAIN_GUARD_S: f64 = 10.0;
/// PLAY, pressed again this soon after it knocked, isn't a cancel.
const CANCEL_AFTER_S: f32 = 0.6;
/// The match over: this long on the HUD's banner, then the title.
const END_HOLD_S: f32 = 3.5;

/// The link the title sits on: the backdrop, ready, that nothing reaches.
pub fn backdrop() -> Box<dyn link::Link> {
    Box::new(link::Still::on(BACKDROP))
}

/// In a match (the link is a real one): the HUD shows, Settings goes back
/// to the Esc menu.
#[derive(Resource, Default)]
pub struct InMatch(pub bool);

/// Leave the match for the title, saying why (the Esc menu's Leave).
#[derive(Resource, Default)]
pub struct Leaving(pub Option<String>);

/// Bumped on every link swap, so the level is built for the new link even
/// when it has had as many welcomes as the last.
#[derive(Resource, Default)]
pub struct LinkGen(pub u32);

/// A control the browser gate finds by name (`expose`, `ci/e2e.mjs`).
#[derive(Component)]
pub struct Tag(pub &'static str);

/// One mode as the play panel offers it.
struct Mode {
    id: u8,
    name: &'static str,
    short: &'static str,
    about: &'static str,
    ranked: bool,
    tag: &'static str,
}

const MODES: [Mode; 9] = [
    Mode {
        id: proto::MODE_CASUAL,
        name: "VS THE BOT",
        short: "1V1 · A BOT",
        about: "A duel with a bot, unrated. Learn the maps and the guns.",
        ranked: false,
        tag: "mode0",
    },
    Mode {
        id: proto::MODE_FFA,
        name: "FREE FOR ALL",
        short: "8 PLAYERS",
        about: "Everyone for themselves. Bots fill the empty seats.",
        ranked: false,
        tag: "mode3",
    },
    Mode {
        id: proto::MODE_GUN_GAME,
        name: "GUN GAME",
        short: "A GUN A KILL",
        about: "A new gun with every kill, the power weapons first. A kill with the last one wins.",
        ranked: false,
        tag: "mode4",
    },
    Mode {
        id: proto::MODE_BIG_TEAM,
        name: "BIG TEAM",
        short: "8V8 · VEHICLES",
        about: "Team Slayer on terrain, with vehicles. Bots fill the empty seats.",
        ranked: false,
        tag: "mode6",
    },
    Mode {
        id: proto::MODE_CTF,
        name: "CAPTURE THE FLAG",
        short: "4V4 · FLAGS",
        about:
            "Take their flag and bring it to yours while yours is home. First to 3 captures. Bots fill the empty seats.",
        ranked: false,
        tag: "mode8",
    },
    Mode {
        id: proto::MODE_WATCH,
        name: "WATCH",
        short: "A LIVE MATCH",
        about: "Watch a live match through one of its players' eyes, a little behind. Click for the next player.",
        ranked: false,
        tag: "mode9",
    },
    Mode {
        id: proto::MODE_DUEL,
        name: "DUEL",
        short: "1V1 · RATED",
        about: "1v1 Slayer: first to 15 kills or 10 minutes. Rated.",
        ranked: true,
        tag: "mode1",
    },
    Mode {
        id: proto::MODE_DOUBLES,
        name: "DOUBLES",
        short: "2V2 · RATED",
        about: "2v2 Team Slayer: first to 30 or 12 minutes. Rated.",
        ranked: true,
        tag: "mode2",
    },
    Mode {
        id: proto::MODE_CUP,
        name: "CUP",
        short: "HEATS OF 8",
        about: "Free for all heats of eight, points for placing; the top seats play a final.",
        ranked: true,
        tag: "mode5",
    },
];

fn mode(id: u8) -> &'static Mode {
    MODES.iter().find(|m| m.id == id).unwrap_or(&MODES[0])
}

/// A mode's name as the play panel says it.
pub(crate) fn mode_name(id: u8) -> &'static str {
    mode(id).name
}

/// What the panel beside the nav shows: what to play, the party, friends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tab {
    #[default]
    Play,
    /// What you wear: the skin, the gear, the emblem.
    Armory,
    Party,
    Friends,
    Challenges,
}

/// The title's own state.
#[derive(Resource)]
pub(crate) struct Front {
    /// The panel beside the nav.
    tab: Tab,
    /// The party's state as last drawn (`PartyView::gen`).
    drawn_party: u32,
    /// The challenges as last drawn (`Challenges::gen`).
    drawn_challenges: u32,
    /// The mode PLAY asks for.
    pub(crate) mode: u8,
    /// The line over PLAY: what the stream said, or why we're back.
    status: String,
    bad: bool,
    /// The name field has the keys.
    editing: bool,
    dirty: bool,
    /// The sign-in as last drawn: drawn again when it moves.
    drawn: Option<SignIn>,
    /// When the match ended (s): the title a moment later.
    ended_at: Option<f32>,
    /// The screen a match opens on (`Opts::open`).
    open: Screen,
    /// Joining at once (`Start::Join`), not yet asked.
    join: Option<u8>,
    /// When PLAY last knocked (s).
    knocked_at: f32,
    /// How far the PLAY panel's rows are scrolled (px), kept over rebuilds.
    scroll: f32,
    /// The armory's page (`ARMORY_TABS`).
    armory: usize,
}

/// The title opens on a panel other than PLAY (`--tab`, for a picture of
/// it): armory (`armory:head`, `armory:emblem`: its page), party, friends
/// or challenges. False for another word.
pub fn open_tab(world: &mut World, name: &str) -> bool {
    // The armory's page too: `armory:emblem`.
    let (name, page) = name.split_once(':').unwrap_or((name, ""));
    let tab = match name {
        "armory" => {
            if !page.is_empty() {
                let Some(k) = ARMORY_TABS.iter().position(|t| t.eq_ignore_ascii_case(page)) else {
                    return false;
                };
                world.resource_mut::<Front>().armory = k;
            }
            Tab::Armory
        }
        "party" => Tab::Party,
        "friends" => Tab::Friends,
        "challenges" => Tab::Challenges,
        "play" => Tab::Play,
        _ => return false,
    };
    world.resource_mut::<Front>().tab = tab;
    true
}

impl Front {
    /// The line a knock was turned away with, while it shows.
    pub(crate) fn refused(&self) -> Option<&str> {
        self.bad.then_some(self.status.as_str())
    }
}

/// A connection on its way.
#[derive(Default)]
struct Knocking(Option<door::Knock>);

/// What the party asks of the door (`party.rs`): a match its leader
/// started, joined as one of it.
pub enum PageCmd {
    /// A match (name, pass or ticket, mode, skin, party).
    Knock(Ask),
}

thread_local! {
    /// The party's asks, taken each frame by `page_cmds`.
    pub static PAGE: std::cell::RefCell<Vec<PageCmd>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[derive(Component)]
struct TitleRoot;

/// The PLAY panel's rows, over PLAY: they scroll.
#[derive(Component)]
struct Rows;

/// Over the backdrop until its level is there (`loading::Settle`).
#[derive(Component)]
pub(crate) struct Curtain;

#[derive(Component)]
struct NameText;

/// What a click on the title hits.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum Hit {
    Options,
    Controls,
    Quit,
    Mode(u8),
    /// A skin, by its id.
    Skin(u8),
    /// A gear slot (`GEAR_ROWS`) and what goes in it.
    Gear(usize, u8),
    /// The armory's page (`ARMORY_TABS`).
    Armory(usize),
    /// The emblem's glyph, and the shape behind it.
    Glyph(u8),
    Field(u8),
    /// The emblem's glyph's colour (0) or its shape's (1), from the palette.
    EmblemColour(usize, u8),
    Name,
    SignIn,
    Go,
    Tab(Tab),
    /// The party: hand the lead to, remove, mute a member (by tag).
    Lead(u8),
    Kick(u8),
    Mute(u8),
    /// Who may join, a step either way.
    Privacy(i8),
    CopyLink,
    Leave,
    Alone,
    /// The party's own room: map (0), score to win (1) or clock (2), a
    /// step either way.
    Custom(u8, i8),
    Mic,
    TalkMode,
    /// Friends: join their party (its code), watch their match (the same
    /// code), invite them (their row).
    Join(u32),
    Watch(u32),
    Invite(u8),
    Friends,
    /// The invite on the title: join, or not now.
    InviteJoin,
    InviteDismiss,
}

/// The title on `app`, from `door`, starting as `start` says.
pub fn install(app: &mut App, door: Door, start: Start, open: Screen) {
    app.insert_non_send_resource(door)
        .insert_non_send_resource(Knocking::default())
        .insert_resource(Front {
            mode: match start {
                Start::Join(m) => m,
                _ => proto::MODE_FFA,
            },
            status: String::new(),
            bad: false,
            editing: false,
            dirty: true,
            drawn: None,
            ended_at: None,
            open,
            join: match start {
                Start::Join(m) => Some(m),
                _ => None,
            },
            knocked_at: 0.0,
            scroll: 0.0,
            armory: 0,
            tab: Tab::Play,
            drawn_party: u32::MAX,
            drawn_challenges: u32::MAX,
        })
        .init_resource::<EmblemPics>()
        .init_resource::<InMatch>()
        .init_resource::<Leaving>()
        .init_resource::<LinkGen>()
        .add_systems(OnEnter(Screen::Title), enter)
        .add_systems(OnExit(Screen::Title), exit)
        .add_systems(
            Update,
            (click, typing, scroll, rebuild, (gear_pics, peek), curtain)
                .chain()
                .run_if(in_state(Screen::Title)),
        )
        .add_systems(Update, (page_cmds, join, poll, ended, hud_shown).chain())
        .add_systems(Update, fly.after(crate::view));
}

fn enter(mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>, mut front: ResMut<Front>) {
    crate::pause::free(&mut cursor);
    front.dirty = true;
}

/// The wheel scrolls the PLAY panel's rows (as `options.rs`' pane); where
/// they are scrolled to (by the wheel, or the pad bringing its focus into
/// view: `pad.rs`) is kept for a rebuild.
fn scroll(
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut rows: Query<&mut ScrollPosition, With<Rows>>,
    mut front: ResMut<Front>,
) {
    use bevy::input::mouse::MouseScrollUnit;
    let dy: f32 = wheel
        .read()
        .map(|w| match w.unit {
            MouseScrollUnit::Line => w.y * 32.0,
            MouseScrollUnit::Pixel => w.y,
        })
        .sum();
    // Rows about to be built again (another tab) start from the scroll
    // that click set, not the old rows'.
    if front.dirty {
        return;
    }
    for mut r in &mut rows {
        if dy != 0.0 {
            r.y = (r.y - dy).max(0.0);
        }
        if r.y != front.scroll {
            front.scroll = r.y;
        }
    }
}

fn exit(mut commands: Commands, roots: Query<Entity, With<TitleRoot>>, mut front: ResMut<Front>) {
    for e in &roots {
        commands.entity(e).despawn();
    }
    front.editing = false;
}

/// The gutter either side of the shell.
const GUT: f32 = 40.0;

#[allow(clippy::too_many_arguments)]
fn build(
    commands: &mut Commands,
    front: &Front,
    player: &Player,
    door: &Door,
    knocking: bool,
    portrait: Option<&crate::look::portrait::Portrait>,
    party: &PartyView,
    challenges: &crate::challenges::Challenges,
    pics: (&mut EmblemPics, &mut Assets<Image>),
) {
    let signin = door.signin();
    commands
        .spawn((TitleRoot, ui::shell(), GlobalZIndex(5)))
        .with_children(|root| {
            root.spawn((
                Curtain,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(ui::BG),
                ZIndex(-2),
            ));
            root.spawn(ui::scrim());
            // The top: the wordmark, the account.
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::new(Val::Px(GUT), Val::Px(GUT), Val::Px(24.0), Val::Px(14.0)),
                ..default()
            })
            .with_children(|top| {
                top.spawn(ui::wordmark("ARENA SLAYER"));
                account(top, &signin);
            });
            // The body: the nav, the panel, the stage.
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                padding: UiRect::horizontal(Val::Px(GUT)),
                ..default()
            })
            .with_children(|body| {
                nav(body, door.quits(), front.tab, party);
                match front.tab {
                    Tab::Play => panel(body, front, player, &signin, knocking, party),
                    Tab::Armory => armory_panel(body, front, player, knocking, party, pics),
                    Tab::Party => with_play(body, front, party, knocking, |p| party_panel(p, party)),
                    Tab::Friends => with_play(body, front, party, knocking, |p| friends_panel(p, party, &signin)),
                    Tab::Challenges => with_play(body, front, party, knocking, |p| {
                        challenges_panel(p, challenges, &signin)
                    }),
                }
                let words = match front.tab {
                    Tab::Armory => armory_words(player, front.armory),
                    _ => {
                        let m = mode(front.mode);
                        (m.short.to_string(), m.name.to_string(), m.about.to_string())
                    }
                };
                stage(body, words, portrait);
            });
            if let Some((from, _)) = &party.invite {
                invite_card(root, from);
            }
            // The foot.
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(18.0),
                padding: UiRect::new(Val::Px(GUT), Val::Px(GUT), Val::Px(12.0), Val::Px(16.0)),
                ..default()
            })
            .with_children(|foot| {
                foot.spawn(ui::label(door.place(), 12.0, ui::FAINT));
                foot.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });
                let map = sim::maps::ALL.iter().find(|d| d.id == BACKDROP).map_or("", |d| d.name);
                foot.spawn(ui::label(
                    format!("Enter plays · {}", map.to_uppercase()),
                    12.0,
                    ui::FAINT,
                ));
            });
        });
}

/// The account corner: sign in, or who is signed in.
fn account(top: &mut ChildSpawnerCommands, signin: &SignIn) {
    top.spawn(Node {
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::FlexEnd,
        row_gap: Val::Px(4.0),
        max_width: Val::Px(360.0),
        ..default()
    })
    .with_children(|a| match signin {
        SignIn::Off(why) => {
            a.spawn((ui::label(why.clone(), 12.5, ui::FAINT), ui::TEXT_SHADOW));
        }
        SignIn::Ready => {
            a.spawn((ui::button(220.0, false), Hit::SignIn, Tag("signin")))
                .with_child(ui::button_label("SIGN IN WITH ELO PROS"));
        }
        SignIn::Asking => {
            a.spawn(ui::label("Signing in...", 13.0, ui::DIM));
        }
        SignIn::In(p) => {
            a.spawn(ui::strong("SIGNED IN", 15.0, ui::TITLE));
            match &p.wallet {
                Some(w) => a.spawn(ui::strong(door::short_wallet(w), 12.5, ui::ELO)),
                None => a.spawn(ui::label("Link a wallet to play ranked", 12.5, ui::DIM)),
            };
        }
        SignIn::Failed(e) => {
            a.spawn((ui::button(220.0, false), Hit::SignIn, Tag("signin")))
                .with_child(ui::button_label("SIGN IN WITH ELO PROS"));
            a.spawn(ui::label(e.clone(), 12.5, ui::BAD));
        }
    });
}

fn nav(body: &mut ChildSpawnerCommands, quits: bool, tab: Tab, party: &PartyView) {
    body.spawn(Node {
        width: Val::Px(230.0),
        flex_shrink: 0.0,
        flex_direction: FlexDirection::Column,
        padding: UiRect::new(Val::Px(15.0), Val::Px(0.0), Val::Px(4.0), Val::Px(0.0)),
        ..default()
    })
    .with_children(|n| {
        n.spawn((
            ui::nav_item("PLAY", tab == Tab::Play, 26.0),
            Hit::Tab(Tab::Play),
            Tag("nav-play"),
        ));
        n.spawn((
            ui::nav_item("ARMORY", tab == Tab::Armory, 26.0),
            Hit::Tab(Tab::Armory),
            Tag("nav-armory"),
        ));
        let size = party.size();
        let party_word = if size > 1 {
            format!("PARTY {size}")
        } else {
            "PARTY".into()
        };
        n.spawn((
            ui::nav_item(&party_word, tab == Tab::Party, 26.0),
            Hit::Tab(Tab::Party),
            Tag("nav-party"),
        ));
        n.spawn((
            ui::nav_item("FRIENDS", tab == Tab::Friends, 26.0),
            Hit::Tab(Tab::Friends),
            Tag("nav-friends"),
        ));
        n.spawn((
            ui::nav_item("CHALLENGES", tab == Tab::Challenges, 26.0),
            Hit::Tab(Tab::Challenges),
            Tag("nav-challenges"),
        ));
        n.spawn(Node {
            height: Val::Px(12.0),
            ..default()
        });
        n.spawn((ui::nav_item("OPTIONS", false, 26.0), Hit::Options, Tag("nav-options")));
        n.spawn((
            ui::nav_item("CONTROLS", false, 26.0),
            Hit::Controls,
            Tag("nav-controls"),
        ));
        if quits {
            n.spawn(Node {
                height: Val::Px(18.0),
                ..default()
            });
            n.spawn((ui::nav_item("QUIT", false, 17.0), Hit::Quit, Tag("nav-quit")));
        }
    });
}

fn panel(
    body: &mut ChildSpawnerCommands,
    front: &Front,
    player: &Player,
    signin: &SignIn,
    knocking: bool,
    party: &PartyView,
) {
    body.spawn(ui::panel(380.0)).with_children(|panel| {
        // The rows scroll under the wheel when the panel can't hold them;
        // PLAY stays at its foot.
        panel.spawn(rows(front.scroll)).with_children(|p| {
            p.spawn(ui::group("WHO"));
            // The name: a field that takes the keys when clicked.
            let edge = if front.editing { ui::ACCENT_HOVER } else { ui::RULE_HI };
            p.spawn((
                Button,
                Hit::Name,
                Tag("name"),
                Node {
                    height: Val::Px(32.0),
                    padding: UiRect::horizontal(Val::Px(10.0)),
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    flex_shrink: 0.0,
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(ui::WELL),
                BorderColor::all(edge),
            ))
            .with_child((
                NameText,
                ui::strong(
                    name_shown(player, front.editing, true),
                    15.0,
                    name_color(player, front.editing),
                ),
            ));
            // What you wear, at a glance: a click opens the armory.
            look_row(p, player);

            p.spawn(ui::group("CASUAL"));
            for m in MODES.iter().filter(|m| !m.ranked) {
                mode_row(p, m, front.mode == m.id, true);
            }
            p.spawn(ui::group("RANKED"));
            let open = signin.ranked();
            if !open {
                let why = match signin {
                    SignIn::Off(why) => why.clone(),
                    SignIn::In(_) => "Link a wallet to play ranked.".into(),
                    _ => "Sign in, a wallet linked, to play ranked.".into(),
                };
                p.spawn(ui::label(why, 12.0, ui::FAINT));
            }
            for m in MODES.iter().filter(|m| m.ranked) {
                mode_row(p, m, front.mode == m.id, open);
            }
        });
        play_foot(panel, front, party, knocking);
    });
}

/// A panel whose rows (`fill`) scroll over the status line and PLAY: so
/// PLAY is a click away whatever the panel shows.
fn with_play(
    body: &mut ChildSpawnerCommands,
    front: &Front,
    party: &PartyView,
    knocking: bool,
    fill: impl FnOnce(&mut ChildSpawnerCommands),
) {
    body.spawn(ui::panel(380.0)).with_children(|panel| {
        panel.spawn(rows(front.scroll)).with_children(|p| fill(p));
        play_foot(panel, front, party, knocking);
    });
}

/// A panel's rows, over its foot: they scroll under the wheel when the
/// panel can't hold them.
fn rows(scroll: f32) -> impl Bundle {
    (
        Rows,
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(7.0),
            flex_grow: 1.0,
            min_height: Val::Px(0.0),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        ScrollPosition(Vec2::new(0.0, scroll)),
    )
}

/// The foot of a panel: the status line and PLAY.
fn play_foot(p: &mut ChildSpawnerCommands, front: &Front, party: &PartyView, knocking: bool) {
    p.spawn((
        ui::label(front.status.clone(), 13.5, if front.bad { ui::BAD } else { ui::DIM }),
        Node {
            min_height: Val::Px(18.0),
            flex_shrink: 0.0,
            ..default()
        },
    ));
    let m = mode(front.mode);
    // In a party the leader's PLAY is everyone's.
    let size = party.size();
    let waiting = size > 1 && !party.leads();
    let ready = !knocking && !waiting;
    let ink = ui::go_ink(ready);
    let (word, sub) = if knocking {
        ("CANCEL".to_string(), "STOP LOOKING FOR A MATCH".to_string())
    } else if waiting {
        let who = party
            .leader()
            .map_or("THE LEADER".into(), |l| l.name.as_str().to_uppercase());
        ("WAITING".to_string(), format!("{who} PICKS THE GAME"))
    } else if size > 1 || party.alone {
        ("PLAY".to_string(), format!("PARTY OF {size} · {}", m.name))
    } else {
        ("PLAY".to_string(), format!("{} · {}", m.name, m.short))
    };
    p.spawn((ui::go(ready), Hit::Go, Tag("go"))).with_children(|g| {
        g.spawn(ui::strong(word, 28.0, ink));
        g.spawn(ui::strong(sub, 11.0, ink.with_alpha(0.72)));
    });
}

/// The PLAY panel's line for what you wear: the skin's colour, what is worn,
/// and the way into the armory.
fn look_row(p: &mut ChildSpawnerCommands, player: &Player) {
    let idle = ui::ROW_IDLE;
    p.spawn((
        Button,
        Hit::Tab(Tab::Armory),
        Tag("look"),
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            height: Val::Px(34.0),
            flex_shrink: 0.0,
            padding: UiRect::horizontal(Val::Px(10.0)),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(idle),
        BorderColor::all(ui::RULE),
        ui::Hover::new(idle, ui::ROW_HOVER),
    ))
    .with_children(|r| {
        r.spawn(swatch_bar(skin_colour(player.skin), 4.0, 18.0));
        r.spawn(grow())
            .with_child(ui::strong(look_words(player), 12.5, ui::TEXT_COLOR));
        r.spawn(ui::strong("ARMORY  >", 11.0, ui::ACCENT_HOVER));
    });
}

/// What is worn, in a few words: the skin, the gear, the emblem's glyph.
fn look_words(player: &Player) -> String {
    let mut w = vec![skin_word(player.skin)];
    for (k, (_, slot)) in GEAR_ROWS.iter().enumerate() {
        let g = proto::gear_valid(slot, player.gear(k));
        if g != proto::GEAR_NONE {
            w.push(slot[g as usize].to_uppercase());
        }
    }
    let (g, ..) = proto::emblem_parts(proto::emblem_valid(player.emblem));
    if g != 0 {
        w.push(proto::EMBLEM_GLYPHS[g as usize].to_uppercase());
    }
    w.join(" · ")
}

/// A skin's colour as the armory shows it: its trim, or the side's blue for
/// the standard look.
fn skin_colour(skin: u8) -> Color {
    crate::look::bodies::trim(skin).unwrap_or(ui::ACCENT)
}

/// A bar of solid colour, `w` by `h` px.
fn swatch_bar(c: Color, w: f32, h: f32) -> impl Bundle {
    (
        Node {
            width: Val::Px(w),
            height: Val::Px(h),
            flex_shrink: 0.0,
            border_radius: BorderRadius::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(c),
    )
}

/// The armory's inner width: the panel's, less its padding.
const ARMORY_W: f32 = 380.0 - 36.0;
const TILE_GAP: f32 = 6.0;
/// A card's width, three a row, the skin cards' colour block and the gear
/// cards' picture (`look/thumbs.rs`' at half its pixels).
const CARD_W: f32 = 110.0;
const CARD_BLOCK_H: f32 = 40.0;
const CARD_PIC_H: f32 = 56.0;
/// The emblem's own picture, px; a glyph's tile and its picture; a shape's.
const EMBLEM_BOX: f32 = 96.0;
const GLYPH_TILE: f32 = 45.0;
const GLYPH_PX: f32 = 32.0;
const FIELD_TILE: f32 = 44.0;
/// The palette's swatches.
const SWATCH: f32 = 16.0;
const SWATCH_GAP: f32 = 2.0;
/// The emblem pictures' pixels a side: the preview's and a tile's.
const EMBLEM_PX: u32 = 96;
const TILE_PX: u32 = 40;

/// The armory's pages: the skins, a page a gear slot (`GEAR_ROWS`), the
/// emblem.
const ARMORY_TABS: [&str; 4] = ["ARMOUR", "HEAD", "FEET", "EMBLEM"];
const ARMORY_EMBLEM: usize = 3;

/// A choice in the armory, `w` by `h` px: lit under the pointer, blue when
/// it's what you wear.
fn tile(w: f32, h: f32, picked: bool) -> impl Bundle {
    let (idle, over) = if picked {
        (ui::PICKED, Color::srgba(0.290, 0.580, 0.800, 0.42))
    } else {
        (ui::ROW_IDLE, ui::ROW_HOVER)
    };
    (
        Button,
        Node {
            width: Val::Px(w),
            height: Val::Px(h),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(3.0),
            flex_shrink: 0.0,
            overflow: Overflow::clip(),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(idle),
        BorderColor::all(if picked { ui::ACCENT_HOVER } else { ui::RULE }),
        ui::Hover::new(idle, over),
    )
}

/// A row of tiles, wrapping.
fn tiles() -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        column_gap: Val::Px(TILE_GAP),
        row_gap: Val::Px(TILE_GAP),
        flex_shrink: 0.0,
        ..default()
    }
}

/// What an item card shows over its name: a skin's colour, or a piece of
/// gear's picture (`look/thumbs.rs`; none for no gear).
enum Face {
    Colour(Color),
    Gear(usize, u8),
}

/// An item card: its face over its name, and under it where it stands for
/// this wallet (owned, locked, its rank line). One it can't wear is dimmed.
fn card(p: &mut ChildSpawnerCommands, item: Item, picked: bool, face: Face, wardrobe: Option<&proto::Wardrobe>) {
    let stand = item.stand(wardrobe);
    let (face_h, h) = match face {
        Face::Colour(_) => (CARD_BLOCK_H, CARD_BLOCK_H + 36.0),
        Face::Gear(..) => (CARD_PIC_H, CARD_PIC_H + 36.0),
    };
    let lit = if stand.dim { 0.4 } else { 1.0 };
    p.spawn((tile(CARD_W, h, picked), item.hit())).with_children(|c| {
        let at = Node {
            width: Val::Px(CARD_W - 2.0),
            height: Val::Px(face_h),
            position_type: PositionType::Absolute,
            top: Val::Px(0.0),
            left: Val::Px(0.0),
            ..default()
        };
        match face {
            Face::Colour(colour) => {
                c.spawn((at, BackgroundColor(colour.with_alpha(0.85 * lit))));
            }
            Face::Gear(_, proto::GEAR_NONE) => {
                c.spawn((at, BackgroundColor(ui::WELL)));
            }
            Face::Gear(slot, g) => {
                c.spawn((
                    at,
                    BackgroundColor(ui::WELL),
                    GearPic(slot, g, lit),
                    // Clear till its picture is put in (`gear_pics`).
                    ImageNode::default().with_color(Color::NONE),
                ));
            }
        }
        c.spawn(Node {
            height: Val::Px(face_h - 6.0),
            ..default()
        });
        let ink = if stand.dim { ui::DIM } else { ui::TITLE };
        c.spawn(ui::strong(item.name(), 13.0, ink));
        if !stand.word.is_empty() {
            c.spawn(ui::strong(stand.word, 9.5, stand.colour));
        }
    });
}

/// A gear card's picture, filled in from the studio's (`look/thumbs.rs`)
/// once the card is up.
#[derive(Component)]
struct GearPic(usize, u8, f32);

fn gear_pics(
    mut cards: Query<(&GearPic, &mut ImageNode), Added<GearPic>>,
    pics: Option<Res<crate::look::thumbs::GearPics>>,
) {
    let Some(pics) = pics else { return };
    for (&GearPic(slot, g, lit), mut node) in &mut cards {
        if let Some(h) = pics.get(slot, g) {
            node.image = h;
            node.color = Color::WHITE.with_alpha(lit);
        }
    }
}

/// Something the armory sells or gives: a skin, or a piece of gear in a
/// slot (`GEAR_ROWS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Skin(u8),
    Gear(usize, u8),
}

/// Where an item stands for this wallet: a word for its card, a line for
/// the box under the cards, their colour, and whether its card is dimmed.
struct Stand {
    word: String,
    line: String,
    colour: Color,
    dim: bool,
}

impl Item {
    /// The item a card is.
    fn of(hit: Hit) -> Option<Item> {
        match hit {
            Hit::Skin(i) => Some(Item::Skin(i)),
            Hit::Gear(k, g) => Some(Item::Gear(k, g)),
            _ => None,
        }
    }

    fn hit(self) -> Hit {
        match self {
            Item::Skin(i) => Hit::Skin(i),
            Item::Gear(k, g) => Hit::Gear(k, g),
        }
    }

    /// What is worn on armory page `page` (none on the emblem's).
    fn worn(player: &Player, page: usize) -> Option<Item> {
        match page {
            0 => Some(Item::Skin(player.skin)),
            k if k <= GEAR_ROWS.len() => Some(Item::Gear(
                k - 1,
                proto::gear_valid(GEAR_ROWS[k - 1].1, player.gear(k - 1)),
            )),
            _ => None,
        }
    }

    fn name(self) -> String {
        match self {
            Item::Skin(i) => skin_word(i),
            Item::Gear(k, g) => GEAR_ROWS[k].1.get(g as usize).copied().unwrap_or("none").to_uppercase(),
        }
    }

    fn blurb(self) -> &'static str {
        let (list, i): (&[&str], u8) = match self {
            Item::Skin(i) => (&SKIN_BLURBS, i),
            Item::Gear(0, g) => (&HEAD_BLURBS, g),
            Item::Gear(_, g) => (&FEET_BLURBS, g),
        };
        list.get(i as usize).copied().unwrap_or("")
    }

    /// Its rank lock (`proto::SKIN_FLOOR`); gear has none.
    fn line(self) -> u16 {
        match self {
            Item::Skin(i) => proto::SKIN_FLOOR.get(i as usize).copied().unwrap_or(0),
            Item::Gear(..) => 0,
        }
    }

    fn owned(self, w: &proto::Wardrobe) -> bool {
        match self {
            Item::Skin(i) => proto::Wardrobe::has(w.skins, i),
            Item::Gear(0, g) => proto::Wardrobe::has(w.headwear, g),
            Item::Gear(_, g) => proto::Wardrobe::has(w.footwear, g),
        }
    }

    /// Where it stands for `w`, the server's word on our wallet (none till
    /// it has said). The server checks again on every hello; this only
    /// says what it will find.
    fn stand(self, w: Option<&proto::Wardrobe>) -> Stand {
        let line = self.line();
        let at_line = |word: &str| match line {
            0 => word.to_string(),
            l if word.is_empty() => format!("{l}+"),
            l => format!("{word} · {l}+"),
        };
        let stand = |word: String, line: String, colour: Color, dim: bool| Stand {
            word,
            line,
            colour,
            dim,
        };
        let worn_from = match line {
            0 => "Worn if your wallet owns it.".to_string(),
            l => format!("Worn if your wallet owns it, from a {l} rating."),
        };
        if matches!(self, Item::Skin(0) | Item::Gear(_, 0)) {
            return stand(String::new(), "Everyone has it.".into(), ui::FAINT, false);
        }
        let Some(w) = w else {
            return stand(at_line(""), worn_from, ui::MARK, false);
        };
        if !w.wallet {
            return stand(
                at_line("WALLET"),
                format!("{worn_from} Sign in with one to wear it."),
                ui::FAINT,
                true,
            );
        }
        if !w.known {
            return stand(
                at_line(""),
                format!("{worn_from} Your wallet couldn't be read just now; it is when you play."),
                ui::MARK,
                false,
            );
        }
        match (self.owned(w), w.floor) {
            (false, _) => stand(
                at_line("LOCKED"),
                format!("{worn_from} Not in yours yet."),
                ui::FAINT,
                true,
            ),
            (true, _) if line == 0 => stand("OWNED".into(), "In your wallet: yours to wear.".into(), ui::GOOD, false),
            (true, Some(f)) if f >= line => stand(
                "OWNED".into(),
                format!("In your wallet, and your {f} rating is over its line."),
                ui::GOOD,
                false,
            ),
            (true, Some(f)) => stand(
                at_line("OWNED"),
                format!("In your wallet, worn from a {line} rating: yours is {f}. Till then, the standard look."),
                ui::MARK,
                true,
            ),
            (true, None) => stand(
                at_line("OWNED"),
                format!(
                    "In your wallet, worn from a {line} rating: you aren't rated yet. Till then, the standard look."
                ),
                ui::MARK,
                true,
            ),
        }
    }
}

/// What each item is, said on the box under the cards (`proto::SKINS`,
/// `proto::HEADWEAR`, `proto::FOOTWEAR` order).
const SKIN_BLURBS: [&str; 6] = [
    "The issue plate in your side's colour, nothing on it.",
    "A clean white stripe round the waist.",
    "Trim the orange of a fuse burning down.",
    "Ice-blue trim, for the ones who stay cold under fire.",
    "Green trim, grown on a long climb up the ladder.",
    "Gold trim. Only the top of the ladder wears it.",
];
const HEAD_BLURBS: [&str; 2] = [
    "Nothing on your head.",
    "Dark lenses on the bridge of the nose. They dim nothing you see.",
];
const FEET_BLURBS: [&str; 2] = [
    "Triple T's own feet.",
    "Heavy treaded boots. Your steps sound no louder in them.",
];

/// A line of the box under the cards: the item under the pointer, else
/// the one worn.
#[derive(Component, Clone, Copy)]
enum Peek {
    Name,
    Word,
    Blurb,
    Line,
}

fn peek_box(p: &mut ChildSpawnerCommands) {
    p.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(5.0),
            padding: UiRect::all(Val::Px(10.0)),
            margin: UiRect::top(Val::Px(4.0)),
            min_height: Val::Px(96.0),
            flex_shrink: 0.0,
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(ui::WELL),
        BorderColor::all(ui::RULE),
    ))
    .with_children(|b| {
        b.spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Baseline,
            ..default()
        })
        .with_children(|r| {
            r.spawn((ui::strong("", 16.0, ui::TITLE), Peek::Name));
            r.spawn((ui::strong("", 10.5, ui::FAINT), Peek::Word));
        });
        b.spawn((ui::label("", 12.0, ui::TEXT_COLOR), Peek::Blurb));
        b.spawn((ui::label("", 11.0, ui::FAINT), Peek::Line));
    });
}

/// The box under the cards says the item under the pointer, else the one
/// worn.
fn peek(
    cards: Query<(&Interaction, &Hit)>,
    front: Res<Front>,
    player: Res<Player>,
    party: Res<PartyView>,
    mut lines: Query<(&Peek, &mut Text, &mut TextColor)>,
) {
    if lines.is_empty() {
        return;
    }
    let over = cards
        .iter()
        .filter(|(i, _)| **i != Interaction::None)
        .find_map(|(_, h)| Item::of(*h));
    let Some(item) = over.or_else(|| Item::worn(&player, front.armory)) else {
        return;
    };
    let stand = item.stand(party.wardrobe.as_ref());
    for (part, mut t, mut c) in &mut lines {
        let (text, colour) = match part {
            Peek::Name => (item.name(), if stand.dim { ui::DIM } else { ui::TITLE }),
            Peek::Word => (stand.word.clone(), stand.colour),
            Peek::Blurb => (item.blurb().to_string(), ui::TEXT_COLOR),
            Peek::Line if stand.colour == ui::GOOD => (stand.line.clone(), ui::FAINT),
            Peek::Line => (stand.line.clone(), stand.colour),
        };
        if t.0 != text {
            t.0 = text;
        }
        c.set_if_neq(TextColor(colour));
    }
}

/// A page's heading: how many there are to wear, and of those how many
/// the wallet owns, once the server has said.
fn armory_count(label: &str, items: impl Iterator<Item = Item>, w: Option<&proto::Wardrobe>) -> String {
    let (mut n, mut owned) = (0, 0);
    for item in items.filter(|i| !matches!(i, Item::Skin(0) | Item::Gear(_, 0))) {
        n += 1;
        owned += usize::from(w.is_some_and(|w| item.owned(w)));
    }
    match w {
        Some(w) if w.wallet && w.known => format!("{label} · {owned} OF {n} OWNED"),
        _ => format!("{label} · {n}"),
    }
}

/// The armory's pages across its top: the picked one blocked out in the blue.
fn armory_tabs(p: &mut ChildSpawnerCommands, at: usize) {
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        column_gap: Val::Px(4.0),
        flex_shrink: 0.0,
        margin: UiRect::bottom(Val::Px(4.0)),
        ..default()
    })
    .with_children(|r| {
        let w = ((ARMORY_W - 3.0 * 4.0) / 4.0).floor();
        for (k, name) in ARMORY_TABS.iter().enumerate() {
            let picked = k == at;
            let (idle, over) = if picked {
                (ui::ACCENT, ui::ACCENT_HOVER)
            } else {
                (ui::ROW_IDLE, ui::ROW_HOVER)
            };
            r.spawn((
                Button,
                Hit::Armory(k),
                Node {
                    width: Val::Px(w),
                    height: Val::Px(28.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(idle),
                BorderColor::all(if picked { Color::NONE } else { ui::RULE }),
                ui::Hover::new(idle, over),
            ))
            .with_child(ui::strong(*name, 11.5, if picked { ui::TITLE } else { ui::DIM }));
        }
    });
}

/// ARMORY: what you wear, a page each (the skins, a gear slot, the emblem),
/// every choice a card you can see, with PLAY at the foot. Triple T beside
/// it wears what is picked, and turns round for the emblem
/// (`look/portrait.rs`).
fn armory_panel(
    body: &mut ChildSpawnerCommands,
    front: &Front,
    player: &Player,
    knocking: bool,
    party: &PartyView,
    (pics, images): (&mut EmblemPics, &mut Assets<Image>),
) {
    let wardrobe = party.wardrobe.as_ref();
    body.spawn(ui::panel(380.0)).with_children(|panel| {
        armory_tabs(panel, front.armory);
        panel.spawn(rows(front.scroll)).with_children(|p| match front.armory {
            0 => {
                let items = (0..proto::SKINS.len() as u8).map(Item::Skin);
                p.spawn(ui::group(&armory_count("ARMOUR", items.clone(), wardrobe)));
                p.spawn(tiles()).with_children(|t| {
                    for item in items {
                        let Item::Skin(i) = item else { continue };
                        card(t, item, player.skin == i, Face::Colour(skin_colour(i)), wardrobe);
                    }
                });
                peek_box(p);
            }
            k if k <= GEAR_ROWS.len() => {
                let (label, slot) = GEAR_ROWS[k - 1];
                let on = proto::gear_valid(slot, player.gear(k - 1));
                let items = (0..slot.len() as u8).map(|g| Item::Gear(k - 1, g));
                p.spawn(ui::group(&armory_count(label, items.clone(), wardrobe)));
                p.spawn(tiles()).with_children(|t| {
                    for g in 0..slot.len() as u8 {
                        card(t, Item::Gear(k - 1, g), on == g, Face::Gear(k - 1, g), wardrobe);
                    }
                });
                peek_box(p);
            }
            _ => emblem_page(p, player, pics, images),
        });
        play_foot(panel, front, party, knocking);
    });
}

/// The emblem's page: its picture, the shapes beside it with the glyph on
/// each, every glyph, then the two colours.
fn emblem_page(p: &mut ChildSpawnerCommands, player: &Player, pics: &mut EmblemPics, images: &mut Assets<Image>) {
    let (g, f, gc, fc) = proto::emblem_parts(proto::emblem_valid(player.emblem));
    p.spawn(ui::group("EMBLEM · ON YOUR BACK"));
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::FlexStart,
        column_gap: Val::Px(10.0),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|r| {
        // The picture.
        r.spawn((
            Node {
                width: Val::Px(EMBLEM_BOX),
                height: Val::Px(EMBLEM_BOX),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(ui::WELL),
            BorderColor::all(ui::RULE),
        ))
        .with_children(|b| {
            if g == 0 {
                b.spawn(ui::strong("NONE", 12.0, ui::FAINT));
            } else {
                b.spawn((
                    ImageNode::new(pics.pic(images, player.emblem, EMBLEM_PX)),
                    Node {
                        width: Val::Px(EMBLEM_BOX - 16.0),
                        height: Val::Px(EMBLEM_BOX - 16.0),
                        ..default()
                    },
                ));
            }
        });
        // The shapes, the glyph (a star while there is none) on each.
        let shown = if g == 0 { 1 } else { g };
        r.spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(4.0),
            row_gap: Val::Px(4.0),
            width: Val::Px(4.0 * FIELD_TILE + 3.0 * 4.0),
            ..default()
        })
        .with_children(|t| {
            for i in 0..proto::EMBLEM_FIELDS.len() as u8 {
                t.spawn((tile(FIELD_TILE, FIELD_TILE, f == i), Hit::Field(i)))
                    .with_children(|c| {
                        if i == 0 {
                            c.spawn(ui::strong("NONE", 9.0, ui::FAINT));
                        } else {
                            c.spawn((
                                ImageNode::new(pics.pic(images, proto::emblem(shown, i, gc, fc), TILE_PX)),
                                Node {
                                    width: Val::Px(FIELD_TILE - 10.0),
                                    height: Val::Px(FIELD_TILE - 10.0),
                                    ..default()
                                },
                            ));
                        }
                    });
            }
        });
    });
    // Every glyph, white, as an icon; a rank lock under the ones that have one.
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        column_gap: Val::Px(4.0),
        row_gap: Val::Px(4.0),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|t| {
        for i in 0..proto::EMBLEM_GLYPHS.len() as u8 {
            t.spawn((tile(GLYPH_TILE, GLYPH_TILE, g == i), Hit::Glyph(i)))
                .with_children(|c| {
                    if i == 0 {
                        c.spawn(ui::strong("NONE", 9.0, ui::FAINT));
                        return;
                    }
                    let line = proto::GLYPH_FLOOR[i as usize];
                    let px = if line > 0 { GLYPH_PX - 8.0 } else { GLYPH_PX };
                    c.spawn((
                        ImageNode::new(pics.pic(images, proto::emblem(i, 0, 0, 0), TILE_PX)),
                        Node {
                            width: Val::Px(px),
                            height: Val::Px(px),
                            ..default()
                        },
                    ));
                    if line > 0 {
                        c.spawn(ui::strong(format!("{line}+"), 8.5, ui::MARK));
                    }
                });
        }
    });
    // The colours.
    if g != 0 {
        let colours = [("GLYPH", gc, true), ("SHAPE", fc, f != 0)];
        for (k, (label, at, shown)) in colours.into_iter().enumerate() {
            if !shown {
                continue;
            }
            p.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(SWATCH_GAP),
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|r| {
                r.spawn(Node {
                    width: Val::Px(44.0 + TILE_GAP - SWATCH_GAP),
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_child(ui::strong(label, 11.0, ui::FAINT));
                for (i, [cr, cg, cb]) in crate::look::emblem::PALETTE.iter().enumerate() {
                    let c = Color::srgb_u8(*cr, *cg, *cb);
                    let picked = at as usize == i;
                    r.spawn((
                        Button,
                        Hit::EmblemColour(k, i as u8),
                        Node {
                            width: Val::Px(SWATCH),
                            height: Val::Px(SWATCH),
                            flex_shrink: 0.0,
                            border: UiRect::all(Val::Px(2.0)),
                            border_radius: BorderRadius::all(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(c),
                        BorderColor::all(if picked { ui::TITLE } else { ui::RULE }),
                        ui::Hover::new(c, c),
                    ));
                }
            });
        }
    }
}

/// The stage's words in the armory: the page's pick, and what you wear,
/// said as a person would.
fn armory_words(player: &Player, page: usize) -> (String, String, String) {
    let mut worn: Vec<String> = Vec::new();
    for (k, (_, slot)) in GEAR_ROWS.iter().enumerate() {
        let g = proto::gear_valid(slot, player.gear(k));
        if g != proto::GEAR_NONE {
            worn.push(slot[g as usize].to_string());
        }
    }
    let (g, f, gc, fc) = proto::emblem_parts(proto::emblem_valid(player.emblem));
    if g != 0 {
        let glyph = proto::EMBLEM_GLYPHS[g as usize];
        worn.push(if f == 0 {
            format!("a {} {glyph} on the back", colour_word(gc))
        } else {
            format!(
                "a {} {glyph} on a {} {} on the back",
                colour_word(gc),
                colour_word(fc),
                proto::EMBLEM_FIELDS[f as usize]
            )
        });
    }
    let mut about = match worn.len() {
        0 => "Nothing else on.".to_string(),
        _ => {
            let mut w = worn.join(", ");
            if let Some(c) = w.get(..1) {
                w = c.to_uppercase() + &w[1..];
            }
            w + "."
        }
    };
    about.push_str(" Drag him to turn him round.");
    let (g, ..) = proto::emblem_parts(proto::emblem_valid(player.emblem));
    let name = match page {
        0 => skin_word(player.skin),
        k if k <= GEAR_ROWS.len() => {
            let (label, slot) = GEAR_ROWS[k - 1];
            match proto::gear_valid(slot, player.gear(k - 1)) {
                proto::GEAR_NONE => format!("BARE {label}"),
                g => slot[g as usize].to_uppercase(),
            }
        }
        _ if g == 0 => "NO EMBLEM".into(),
        _ => proto::EMBLEM_GLYPHS[g as usize].to_uppercase(),
    };
    (
        format!("ARMORY · {}", ARMORY_TABS[page.min(ARMORY_EMBLEM)]),
        name,
        about,
    )
}

/// A palette colour's name (`look/emblem.rs`' order).
fn colour_word(i: u8) -> &'static str {
    const NAMES: [&str; 16] = [
        "white", "grey", "slate", "black", "red", "orange", "yellow", "lime", "green", "teal", "sky", "blue", "navy",
        "purple", "pink", "brown",
    ];
    NAMES[i as usize & 15]
}

/// The armory's emblem pictures, each an emblem drawn `size` px a side
/// (`look/emblem.rs`), kept while a build shows them.
#[derive(Resource, Default)]
struct EmblemPics {
    pics: std::collections::HashMap<(proto::Emblem, u32), Handle<Image>>,
    used: Vec<(proto::Emblem, u32)>,
}

impl EmblemPics {
    /// `code`'s picture, drawn if it isn't yet.
    fn pic(&mut self, images: &mut Assets<Image>, code: proto::Emblem, size: u32) -> Handle<Image> {
        use bevy::asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
        let key = (proto::emblem_valid(code), size);
        self.used.push(key);
        self.pics
            .entry(key)
            .or_insert_with(|| {
                images.add(Image::new(
                    Extent3d {
                        width: size,
                        height: size,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    crate::look::emblem::rgba(key.0, size),
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
                ))
            })
            .clone()
    }

    /// A build starts: nothing used yet.
    fn begin(&mut self) {
        self.used.clear();
    }

    /// A build is done: the pictures it didn't show are dropped.
    fn end(&mut self, images: &mut Assets<Image>) {
        let used = std::mem::take(&mut self.used);
        self.pics.retain(|k, h| {
            if used.contains(k) {
                return true;
            }
            images.remove(h.id());
            false
        });
    }
}

/// A small button in a row: a member's LEAD or MUTE, a friend's JOIN.
fn chip(width: f32) -> impl Bundle {
    (
        Button,
        Node {
            width: Val::Px(width),
            height: Val::Px(22.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(ui::ROW_IDLE),
        BorderColor::all(ui::RULE),
        ui::Hover::default(),
    )
}

fn chip_label(s: &str) -> impl Bundle {
    ui::strong(s.to_string(), 11.0, ui::TEXT_COLOR)
}

/// One line of a list: a light, a name, what they're doing, its buttons.
fn line_row() -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            height: Val::Px(32.0),
            flex_shrink: 0.0,
            padding: UiRect::horizontal(Val::Px(10.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(ui::ROW_IDLE),
        BorderColor::all(ui::RULE),
    )
}

fn grow() -> Node {
    Node {
        flex_grow: 1.0,
        min_width: Val::Px(0.0),
        overflow: Overflow::clip(),
        ..default()
    }
}

fn playing_words(p: u8) -> String {
    if p == pp::PLAYING_NONE {
        "IN THE MENU".into()
    } else {
        format!("PLAYING {}", proto::mode_name(p).to_uppercase())
    }
}

/// PARTY: who is in it, who leads, who may join, the link, and voice.
fn party_panel(body: &mut ChildSpawnerCommands, party: &PartyView) {
    body.spawn(rows(0.0)).with_children(|p| {
        let Some(info) = party.party.as_ref() else {
            p.spawn(ui::group("PARTY"));
            p.spawn(ui::label("Getting your party ready...", 13.0, ui::DIM));
            return;
        };
        p.spawn(ui::group(&format!("PARTY · {}", pp::code_text(info.code))));
        let lead = party.leads();
        for m in info.members() {
            let mine = m.tag == info.me;
            p.spawn(line_row()).with_children(|r| {
                r.spawn((
                    TalkLight(m.tag),
                    Node {
                        width: Val::Px(8.0),
                        height: Val::Px(8.0),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(ui::RULE),
                ));
                let name = if mine {
                    format!("{} (YOU)", m.name.as_str())
                } else {
                    m.name.as_str().to_string()
                };
                r.spawn(grow())
                    .with_child(ui::strong(name, 14.0, if m.away { ui::FAINT } else { ui::TITLE }));
                if m.tag == info.leader {
                    r.spawn(ui::strong("LEADS", 10.5, ui::MARK));
                }
                r.spawn(ui::label(
                    if m.away {
                        "ON THE WAY".into()
                    } else {
                        playing_words(m.playing)
                    },
                    10.5,
                    ui::FAINT,
                ));
                if !mine && party.mic != Mic::None {
                    let muted = party::muted(&party::voice_key(m));
                    r.spawn((chip(52.0), Hit::Mute(m.tag))).with_child(chip_label(if muted {
                        "UNMUTE"
                    } else {
                        "MUTE"
                    }));
                }
                if lead && !mine {
                    r.spawn((chip(44.0), Hit::Lead(m.tag))).with_child(chip_label("LEAD"));
                    r.spawn((chip(24.0), Hit::Kick(m.tag))).with_child(chip_label("X"));
                }
            });
        }
        if info.members().len() < pp::MAX_PARTY {
            p.spawn(ui::label(
                "Send your link to bring people in. Whoever leads picks the game.",
                12.0,
                ui::FAINT,
            ));
        }

        p.spawn(ui::group("WHO CAN JOIN"));
        p.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|r| {
            let word = match info.privacy {
                pp::PRIVACY_OPEN => "ANYONE WITH THE LINK",
                pp::PRIVACY_FRIENDS => "YOUR FRIENDS",
                _ => "ONLY WHO YOU INVITE",
            };
            if lead {
                r.spawn((ui::stepper(), Hit::Privacy(-1)))
                    .with_child(ui::strong("<", 14.0, ui::TEXT_COLOR));
            }
            r.spawn(Node {
                flex_grow: 1.0,
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_child(ui::strong(word, 13.0, ui::TEXT_COLOR));
            if lead {
                r.spawn((ui::stepper(), Hit::Privacy(1)))
                    .with_child(ui::strong(">", 14.0, ui::TEXT_COLOR));
            }
        });
        p.spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(8.0),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|r| {
            r.spawn((ui::button(168.0, true), Hit::CopyLink, Tag("party-copy")))
                .with_child(ui::button_label("COPY PARTY LINK"));
            if info.members().len() > 1 {
                r.spawn((ui::button(168.0, false), Hit::Leave, Tag("party-leave")))
                    .with_child(ui::button_label("LEAVE PARTY"));
            }
        });
        let alone = if party.alone { "ON" } else { "OFF" };
        p.spawn(line_row()).with_children(|r| {
            r.spawn(grow())
                .with_child(ui::strong("PARTY ALONE · BOTS FILL THE REST", 12.0, ui::TEXT_COLOR));
            r.spawn((chip(44.0), Hit::Alone, Tag("party-alone")))
                .with_child(chip_label(alone));
        });
        // Alone, the leader sets the room's own map, score and clock.
        if party.alone && lead {
            let c = party.custom;
            let map = c
                .map
                .and_then(|id| sim::maps::ALL.iter().find(|d| d.id == id))
                .map_or("THE SERVER'S PICK".to_string(), |d| d.name.to_uppercase());
            let score = match pp::SCORE_LIMITS[c.score as usize] {
                0 => "THE MODE'S".to_string(),
                i32::MAX => "NO LIMIT".to_string(),
                n => n.to_string(),
            };
            let time = match pp::TIME_LIMITS_MIN[c.time as usize] {
                0 => "THE MODE'S".to_string(),
                u32::MAX => "NO CLOCK".to_string(),
                m => format!("{m} MIN"),
            };
            for (k, what, word) in [(0u8, "MAP", map), (1, "TO WIN", score), (2, "CLOCK", time)] {
                p.spawn(line_row()).with_children(|r| {
                    r.spawn(Node {
                        width: Val::Px(64.0),
                        flex_shrink: 0.0,
                        ..default()
                    })
                    .with_child(ui::label(what, 11.0, ui::FAINT));
                    r.spawn((ui::stepper(), Hit::Custom(k, -1)))
                        .with_child(ui::strong("<", 14.0, ui::TEXT_COLOR));
                    r.spawn(Node {
                        flex_grow: 1.0,
                        justify_content: JustifyContent::Center,
                        ..default()
                    })
                    .with_child(ui::strong(word, 12.5, ui::TEXT_COLOR));
                    r.spawn((ui::stepper(), Hit::Custom(k, 1)))
                        .with_child(ui::strong(">", 14.0, ui::TEXT_COLOR));
                });
            }
        }

        p.spawn(ui::group("VOICE"));
        if party.mic == Mic::None {
            p.spawn(ui::label(
                if cfg!(target_arch = "wasm32") {
                    "This browser can't do voice."
                } else {
                    "Party voice is in the browser for now."
                },
                12.0,
                ui::FAINT,
            ));
        } else {
            p.spawn(line_row()).with_children(|r| {
                let mic = match party.mic {
                    Mic::Off => "MIC OFF",
                    Mic::On => "MIC ON",
                    _ => "TALKING",
                };
                r.spawn((chip(96.0), Hit::Mic, Tag("party-mic")))
                    .with_child(chip_label(mic));
                r.spawn(grow()).with_child(ui::label(
                    if party.open_mic {
                        "Open mic".to_string()
                    } else {
                        format!("Hold {} to talk", party.talk_key)
                    },
                    12.0,
                    ui::DIM,
                ));
                r.spawn((chip(96.0), Hit::TalkMode))
                    .with_child(chip_label(&if party.open_mic {
                        format!("HOLD {}", party.talk_key)
                    } else {
                        "OPEN MIC".to_string()
                    }));
            });
            if let Some(why) = party.mic_why {
                p.spawn(ui::label(why, 12.0, ui::BAD));
            }
            p.spawn(ui::label(
                "Your party hears you, here and in a match. Nobody else does.",
                11.5,
                ui::FAINT,
            ));
        }
        p.spawn(ui::spacer());
        if let Some(n) = &party.notice {
            p.spawn(ui::label(n.clone(), 12.5, ui::DIM));
        }
        if !party.connected {
            p.spawn(ui::label(
                "The party's line dropped: joining it again...",
                12.5,
                ui::BAD,
            ));
        }
    });
}

/// FRIENDS: your Elo Pros friends, where they are, and a way in or to ask.
/// Today's and this week's challenges, as Elo Pros has them
/// (`challenges.rs`), with our progress once we're signed in.
fn challenges_panel(body: &mut ChildSpawnerCommands, ch: &crate::challenges::Challenges, signin: &SignIn) {
    use crate::challenges::{left_words, progress_words};
    body.spawn(rows(0.0)).with_children(|p| {
        let Some(v) = &ch.view else {
            p.spawn(ui::group("CHALLENGES"));
            p.spawn(ui::label(
                if ch.failed {
                    "The challenges didn't come through just now."
                } else {
                    "Reading today's challenges..."
                },
                13.0,
                if ch.failed { ui::BAD } else { ui::DIM },
            ));
            return;
        };
        let now = crate::challenges::unix_now();
        for (title, per) in [("TODAY", &v.day), ("THIS WEEK", &v.week)] {
            p.spawn(ui::group(&format!("{title} · {} LEFT", left_words(per.ends_at, now))));
            if per.challenges.is_empty() {
                p.spawn(ui::label("None yet.", 12.5, ui::FAINT));
            }
            for c in &per.challenges {
                p.spawn(line_row()).with_children(|r| {
                    r.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        flex_grow: 1.0,
                        ..default()
                    })
                    .with_children(|t| {
                        t.spawn(ui::strong(c.name.to_uppercase(), 13.5, ui::TITLE));
                        if !c.desc.is_empty() {
                            t.spawn(ui::label(c.desc.clone(), 11.0, ui::FAINT));
                        }
                    });
                    let done = c.done == Some(true);
                    r.spawn(ui::strong(
                        progress_words(c),
                        12.0,
                        if done { ui::GOOD } else { ui::TEXT_COLOR },
                    ));
                    if c.cards > 0 {
                        let cards = if c.cards == 1 {
                            "1 CARD".to_string()
                        } else {
                            format!("{} CARDS", c.cards)
                        };
                        r.spawn(ui::label(cards, 10.5, ui::ACCENT));
                    }
                });
            }
        }
        p.spawn(ui::spacer());
        if !matches!(signin, SignIn::In(_)) {
            p.spawn(ui::label("Sign in to count your progress.", 12.5, ui::DIM));
        }
        if !v.rules.is_empty() {
            p.spawn(ui::label(v.rules.clone(), 10.5, ui::FAINT));
        }
    });
}

fn friends_panel(body: &mut ChildSpawnerCommands, party: &PartyView, signin: &SignIn) {
    body.spawn(rows(0.0)).with_children(|p| {
        let here = party.friends.iter().filter(|f| f.at >= pp::AT_HERE).count();
        p.spawn(ui::group(&if party.friends.is_empty() {
            "FRIENDS".to_string()
        } else {
            format!("FRIENDS · {here} HERE")
        }));
        if !matches!(signin, SignIn::In(_)) && party.friends.is_empty() {
            p.spawn(ui::label("Sign in to see your friends here.", 13.0, ui::DIM));
            if matches!(signin, SignIn::Ready | SignIn::Failed(_)) {
                p.spawn((ui::button(220.0, false), Hit::SignIn))
                    .with_child(ui::button_label("SIGN IN WITH ELO PROS"));
            }
            return;
        }
        if !party.reachable && party.friends_known {
            p.spawn(ui::label("Your friends didn't come through just now.", 12.5, ui::BAD));
        } else if party.friends_known && party.friends.is_empty() {
            p.spawn(ui::label("No friends yet. Add them on your profile.", 13.0, ui::DIM));
        } else if !party.friends_known {
            p.spawn(ui::label("Reading your friends...", 13.0, ui::DIM));
        }
        let mine: Vec<&str> = party.members().iter().map(|m| m.player.as_str()).collect();
        let order = |f: &Friend| match f.at {
            pp::AT_HERE => 0,
            pp::AT_MATCH => 1,
            pp::AT_ELSEWHERE => 2,
            _ => 3,
        };
        let mut rows: Vec<(usize, &Friend)> = party.friends.iter().enumerate().collect();
        rows.sort_by_key(|(_, f)| order(f));
        for (i, f) in rows.into_iter().take(12) {
            p.spawn(line_row()).with_children(|r| {
                let lit = match f.at {
                    pp::AT_HERE => ui::GOOD,
                    pp::AT_MATCH => ui::MARK,
                    pp::AT_ELSEWHERE => ui::ACCENT,
                    _ => ui::RULE,
                };
                r.spawn((
                    Node {
                        width: Val::Px(8.0),
                        height: Val::Px(8.0),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(lit),
                ));
                r.spawn(grow()).with_child(ui::strong(f.name.clone(), 14.0, ui::TITLE));
                let in_mine = mine.contains(&f.player.as_str());
                let where_ = if in_mine {
                    "IN YOUR PARTY".to_string()
                } else {
                    match f.at {
                        pp::AT_MATCH => playing_words(f.playing),
                        pp::AT_HERE if f.size > 1 => format!("PARTY OF {}", f.size),
                        pp::AT_HERE => "IN THE MENU".into(),
                        pp::AT_ELSEWHERE => "ONLINE".into(),
                        _ => "OFFLINE".into(),
                    }
                };
                r.spawn(ui::label(where_, 10.5, ui::FAINT));
                if f.party != 0 && !in_mine {
                    r.spawn((chip(44.0), Hit::Join(f.party))).with_child(chip_label("JOIN"));
                }
                if f.party != 0 && f.at == pp::AT_MATCH && !in_mine {
                    r.spawn((chip(56.0), Hit::Watch(f.party)))
                        .with_child(chip_label("WATCH"));
                }
                if f.at >= pp::AT_HERE && !in_mine {
                    r.spawn((chip(56.0), Hit::Invite(i as u8)))
                        .with_child(chip_label("INVITE"));
                }
            });
        }
        p.spawn(ui::spacer());
        if let Some(n) = &party.notice {
            p.spawn(ui::label(n.clone(), 12.5, ui::DIM));
        }
        p.spawn((ui::button(140.0, false), Hit::Friends))
            .with_child(ui::button_label("REFRESH"));
    });
}

/// An invite, over the title's corner: join, or not now.
fn invite_card(root: &mut ChildSpawnerCommands, from: &str) {
    root.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(GUT),
            top: Val::Px(84.0),
            width: Val::Px(320.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            padding: UiRect::all(Val::Px(14.0)),
            border: UiRect::new(Val::Px(3.0), Val::Px(1.0), Val::Px(1.0), Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(ui::PANE_BG),
        BorderColor {
            left: ui::MARK,
            ..BorderColor::all(ui::RULE_HI)
        },
        GlobalZIndex(7),
    ))
    .with_children(|c| {
        c.spawn(ui::strong(
            format!("{} ASKS YOU INTO THEIR PARTY", from.to_uppercase()),
            13.0,
            ui::TITLE,
        ));
        c.spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|r| {
            r.spawn((ui::button(120.0, true), Hit::InviteJoin, Tag("invite-join")))
                .with_child(ui::button_label("JOIN"));
            r.spawn((ui::button(120.0, false), Hit::InviteDismiss))
                .with_child(ui::button_label("NOT NOW"));
        });
    });
}

fn mode_row(p: &mut ChildSpawnerCommands, m: &Mode, picked: bool, open: bool) {
    let (idle, over) = if picked {
        (ui::PICKED, Color::srgba(0.290, 0.580, 0.800, 0.42))
    } else {
        (ui::ROW_IDLE, ui::ROW_HOVER)
    };
    let node = Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::SpaceBetween,
        height: Val::Px(30.0),
        flex_shrink: 0.0,
        padding: UiRect::horizontal(Val::Px(12.0)),
        border: UiRect::new(
            Val::Px(if picked { 3.0 } else { 1.0 }),
            Val::Px(1.0),
            Val::Px(1.0),
            Val::Px(1.0),
        ),
        ..default()
    };
    let edge = BorderColor {
        left: if picked { ui::ACCENT_HOVER } else { ui::RULE },
        ..BorderColor::all(ui::RULE)
    };
    let (name_c, short_c) = match (open, picked) {
        (false, _) => (ui::FAINT.with_alpha(0.55), ui::FAINT.with_alpha(0.45)),
        (true, true) => (ui::TITLE, ui::TEXT_COLOR),
        (true, false) => (ui::TEXT_COLOR, ui::FAINT),
    };
    let kids = (ui::strong(m.name, 15.0, name_c), ui::strong(m.short, 11.0, short_c));
    if open {
        p.spawn((
            Button,
            Hit::Mode(m.id),
            Tag(m.tag),
            node,
            BackgroundColor(idle),
            edge,
            ui::Hover::new(idle, over),
        ))
        .with_children(|r| {
            r.spawn(kids.0);
            r.spawn(kids.1);
        });
    } else {
        p.spawn((node, BackgroundColor(ui::ROW_IDLE.with_alpha(0.4)), edge))
            .with_children(|r| {
                r.spawn(kids.0);
                r.spawn(kids.1);
            });
    }
}

/// How tall Triple T stands on the stage: tall enough to read at 720p
/// without reaching the wordmark (his picture is drawn at that size).
const HERO_VH: f32 = crate::look::portrait::VIEW_VH;

/// The open stage: Triple T beside the panel, and the picked mode over the
/// scene.
fn stage(
    body: &mut ChildSpawnerCommands,
    (kicker, name, about): (String, String, String),
    portrait: Option<&crate::look::portrait::Portrait>,
) {
    body.spawn(Node {
        flex_grow: 1.0,
        min_width: Val::Px(0.0),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::FlexEnd,
        column_gap: Val::Px(20.0),
        padding: UiRect::new(Val::Px(24.0), Val::Px(0.0), Val::Px(0.0), Val::Px(8.0)),
        ..default()
    })
    .with_children(|stage| {
        if let Some(p) = portrait {
            stage.spawn(crate::look::portrait::view(p, Val::Vh(HERO_VH)));
        }
        stage
            .spawn(Node {
                flex_grow: 1.0,
                min_width: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                ..default()
            })
            .with_children(|s| {
                s.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(10.0),
                    max_width: Val::Px(640.0),
                    ..default()
                })
                .with_children(|hero| {
                    hero.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(10.0),
                        ..default()
                    })
                    .with_children(|k| {
                        k.spawn((
                            Node {
                                width: Val::Px(26.0),
                                height: Val::Px(3.0),
                                ..default()
                            },
                            BackgroundColor(ui::MARK),
                        ));
                        k.spawn((ui::strong(kicker, 12.0, ui::TITLE), ui::TEXT_SHADOW));
                    });
                    hero.spawn((
                        Text::new(name),
                        ui::display(44.0),
                        TextColor(ui::TITLE),
                        ui::TEXT_SHADOW,
                    ));
                    hero.spawn((ui::label(about, 16.0, ui::TEXT_COLOR.with_alpha(0.88)), ui::TEXT_SHADOW));
                });
            });
    });
}

/// A skin's name.
fn skin_word(i: u8) -> String {
    proto::SKINS
        .get(i as usize)
        .copied()
        .unwrap_or("standard")
        .to_uppercase()
}

/// The gear's rows in the armory: its label and its slot.
const GEAR_ROWS: [(&str, &[&str]); 2] = [("HEAD", &proto::HEADWEAR), ("FEET", &proto::FOOTWEAR)];

/// The name as the field shows it: what's typed, a caret while editing, a
/// prompt while empty.
fn name_shown(p: &Player, editing: bool, caret: bool) -> String {
    match (p.name.is_empty(), editing) {
        (true, false) => "TYPE A NAME".into(),
        (_, true) => format!("{}{}", p.name, if caret { "|" } else { " " }),
        (false, false) => p.name.clone(),
    }
}

fn name_color(p: &Player, editing: bool) -> Color {
    if p.name.is_empty() && !editing {
        ui::FAINT
    } else {
        ui::TITLE
    }
}

/// Clicks and keys on the title.
#[allow(clippy::too_many_arguments)]
fn click(
    hits: Query<(&Interaction, &Hit), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut front: ResMut<Front>,
    mut player: ResMut<Player>,
    mut door: NonSendMut<Door>,
    mut knocking: NonSendMut<Knocking>,
    mut page: ResMut<crate::options::Page>,
    mut next: ResMut<NextState<Screen>>,
    mut exit: MessageWriter<AppExit>,
    time: Res<Time>,
    mut party: ResMut<PartyView>,
    mut asks: ResMut<PartyAsks>,
) {
    let hit = hits.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, h)| *h);
    // A click anywhere but the field puts the keys back.
    if front.editing && mouse.just_pressed(MouseButton::Left) && hit != Some(Hit::Name) {
        front.editing = false;
        front.dirty = true;
    }
    let hit = hit.or_else(|| {
        if front.editing {
            if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Escape) {
                front.editing = false;
                front.dirty = true;
            }
            return None;
        }
        if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
            return Some(Hit::Go);
        }
        if keys.just_pressed(KeyCode::Escape) && knocking.0.is_some() {
            return Some(Hit::Go);
        }
        None
    });
    let Some(hit) = hit else {
        return;
    };
    match hit {
        Hit::Options => {
            page.cat = 0;
            next.set(Screen::Settings);
        }
        Hit::Controls => {
            page.cat = crate::options::KEYS;
            next.set(Screen::Settings);
        }
        Hit::Quit => {
            exit.write(AppExit::Success);
        }
        Hit::Mode(m) => {
            front.mode = m;
            front.dirty = true;
        }
        Hit::Skin(i) => {
            player.skin = i;
            front.dirty = true;
        }
        Hit::Gear(k, g) => {
            *player.gear_mut(k) = proto::gear_valid(GEAR_ROWS[k].1, g);
            front.dirty = true;
        }
        Hit::Armory(k) => {
            if front.armory != k {
                front.scroll = 0.0;
            }
            front.armory = k;
            front.dirty = true;
        }
        Hit::Glyph(g) => {
            let p = proto::emblem_parts(player.emblem);
            player.emblem = proto::emblem(g, p.1, p.2, p.3);
            front.dirty = true;
        }
        Hit::Field(f) => {
            let p = proto::emblem_parts(player.emblem);
            player.emblem = proto::emblem(p.0, f, p.2, p.3);
            front.dirty = true;
        }
        Hit::EmblemColour(k, c) => {
            let mut p = proto::emblem_parts(player.emblem);
            if k == 0 {
                p.2 = c;
            } else {
                p.3 = c;
            }
            player.emblem = proto::emblem(p.0, p.1, p.2, p.3);
            front.dirty = true;
        }
        Hit::Name => {
            front.editing = true;
            front.dirty = true;
        }
        Hit::SignIn => {
            door.sign_in();
            front.dirty = true;
        }
        Hit::Tab(t) => {
            if front.tab != t {
                front.scroll = 0.0;
            }
            front.tab = t;
            if t == Tab::Friends {
                asks.0.push(Do::Act(pp::ACT_FRIENDS, 0));
            }
            front.dirty = true;
        }
        Hit::Lead(tag) => asks.0.push(Do::Act(pp::ACT_LEAD, tag as u32)),
        Hit::Kick(tag) => asks.0.push(Do::Act(pp::ACT_KICK, tag as u32)),
        Hit::Mute(tag) => {
            if let Some(m) = party.members().iter().find(|m| m.tag == tag) {
                party::toggle_mute(&party::voice_key(m));
                front.dirty = true;
            }
        }
        Hit::Privacy(d) => {
            if let Some(p) = &party.party {
                let next = (p.privacy as i32 + d as i32).rem_euclid(3) as u32;
                asks.0.push(Do::Act(pp::ACT_PRIVACY, next));
            }
        }
        Hit::CopyLink => {
            let code = party.code_text();
            party.notice = Some(match party::copy_link(&code) {
                Some(link) => format!("Copied: {link}"),
                None => format!("Your party's code is {code}: oneshot --party {code}"),
            });
            front.dirty = true;
        }
        Hit::Leave => asks.0.push(Do::Act(pp::ACT_LEAVE, 0)),
        Hit::Alone => {
            party.alone = !party.alone;
            front.dirty = true;
        }
        Hit::Custom(k, d) => {
            let step = |i: u8, n: usize| (i as i32 + d as i32).rem_euclid(n as i32) as u8;
            let c = &mut party.custom;
            match k {
                0 => {
                    // None (the server's pick), then every map in order.
                    let ids: Vec<Option<u8>> = core::iter::once(None)
                        .chain(sim::maps::ALL.iter().map(|m| Some(m.id)))
                        .collect();
                    let at = ids.iter().position(|&m| m == c.map).unwrap_or(0);
                    c.map = ids[step(at as u8, ids.len()) as usize];
                }
                1 => c.score = step(c.score, pp::SCORE_LIMITS.len()),
                _ => c.time = step(c.time, pp::TIME_LIMITS_MIN.len()),
            }
            front.dirty = true;
        }
        Hit::Mic => party::mic_toggle(),
        Hit::TalkMode => party::set_open_mic(!party.open_mic),
        Hit::Join(code) => {
            asks.0.push(Do::Act(pp::ACT_JOIN, code));
            front.tab = Tab::Party;
            front.dirty = true;
        }
        Hit::Watch(code) => {
            if knocking.0.is_none() {
                front.mode = proto::MODE_WATCH;
                let ask = Ask {
                    name: player.sent_name(),
                    pass: party::match_pass(&party, &door),
                    mode: proto::MODE_WATCH,
                    look: player.look(),
                    party: code,
                };
                knocking.0 = Some(door.knock(ask));
                front.knocked_at = time.elapsed_secs();
                front.status.clear();
                front.bad = false;
                front.dirty = true;
            }
        }
        Hit::Invite(i) => {
            if let Some(f) = party.friends.get(i as usize) {
                asks.0.push(Do::Invite(f.player.clone()));
                party.notice = Some(format!("Invited {}. They can join from their title.", f.name));
                front.dirty = true;
            }
        }
        Hit::Friends => asks.0.push(Do::Act(pp::ACT_FRIENDS, 0)),
        Hit::InviteJoin => {
            if let Some((_, code)) = party.invite.take() {
                asks.0.push(Do::Act(pp::ACT_JOIN, code));
                front.tab = Tab::Party;
            }
            front.dirty = true;
        }
        Hit::InviteDismiss => {
            party.invite = None;
            front.dirty = true;
        }
        Hit::Go => {
            let now = time.elapsed_secs();
            if knocking.0.is_some() {
                // A double click on PLAY isn't a change of mind.
                if now - front.knocked_at < CANCEL_AFTER_S {
                    return;
                }
                knocking.0 = None;
                front.status = "Cancelled".into();
                front.bad = false;
            } else {
                let m = mode(front.mode);
                let signin = door.signin();
                if m.ranked && !signin.ranked() {
                    front.status = "Ranked needs a sign-in with a wallet".into();
                    front.bad = true;
                } else if m.id != proto::MODE_WATCH && party.connected && (party.size() > 1 || party.alone) {
                    // A party's game: its leader starts it, and the server
                    // tells each of us to go (`party.rs`).
                    if party.leads() {
                        let alone = if party.alone {
                            pp::START_PRIVATE | party.custom.to_arg()
                        } else {
                            0
                        };
                        asks.0.push(Do::Act(pp::ACT_START, m.id as u32 | alone));
                        front.status = "Starting for the party...".into();
                        front.bad = false;
                    } else {
                        let who = party
                            .leader()
                            .map_or("The leader".into(), |l| l.name.as_str().to_string());
                        front.status = format!("{who} picks the game");
                        front.bad = false;
                    }
                } else {
                    // Our party's code goes with us (friends watch by it); a
                    // watcher's own is no match to watch, so none.
                    let code = party.party.as_ref().map_or(0, |p| p.code);
                    let ask = Ask {
                        name: player.sent_name(),
                        pass: party::match_pass(&party, &door),
                        mode: m.id,
                        look: player.look(),
                        party: if m.id == proto::MODE_WATCH { 0 } else { code },
                    };
                    knocking.0 = Some(door.knock(ask));
                    front.knocked_at = now;
                    front.status.clear();
                    front.bad = false;
                }
            }
            front.dirty = true;
        }
    }
}

/// The name field, while it has the keys: letters in, Backspace out, the
/// caret blinking.
fn typing(
    mut typed: MessageReader<KeyboardInput>,
    front: Res<Front>,
    mut player: ResMut<Player>,
    time: Res<Time>,
    mut shown: Query<(&mut Text, &mut TextColor), With<NameText>>,
) {
    if !front.editing {
        typed.clear();
        return;
    }
    let mut name = player.name.clone();
    for k in typed.read() {
        if k.state != ButtonState::Pressed {
            continue;
        }
        match &k.logical_key {
            Key::Backspace => {
                name.pop();
            }
            Key::Character(s) => {
                name.push_str(s);
                name = Player::clean(&name);
            }
            Key::Space => name.push(' '),
            _ => {}
        }
    }
    name.truncate(NAME_MAX);
    if name != player.name {
        player.name = name;
    }
    let caret = ((time.elapsed_secs() * 2.0) as u32).is_multiple_of(2);
    for (mut t, mut c) in &mut shown {
        let want = name_shown(&player, front.editing, caret);
        if t.0 != want {
            t.0 = want;
        }
        c.set_if_neq(TextColor(name_color(&player, front.editing)));
    }
}

/// How often the title looks at the sign-in for a change, seconds.
const SIGNIN_LOOK_S: f64 = 0.1;

/// Built again when anything on it moved, or the sign-in did.
#[allow(clippy::too_many_arguments)]
fn rebuild(
    mut commands: Commands,
    mut front: ResMut<Front>,
    player: Res<Player>,
    door: NonSend<Door>,
    knocking: NonSend<Knocking>,
    roots: Query<Entity, With<TitleRoot>>,
    mut portrait: Option<ResMut<crate::look::portrait::Portrait>>,
    party: Res<PartyView>,
    (mut challenges, time, mut looked): (ResMut<crate::challenges::Challenges>, Res<Time>, Local<f64>),
    (mut pics, mut images): (ResMut<EmblemPics>, ResMut<Assets<Image>>),
) {
    // The sign-in looked at a few times a second, or on a change here: each
    // look clones it (a string made, on the desktop).
    let now = time.elapsed_secs_f64();
    let due = front.dirty
        || front.drawn.is_none()
        || front.drawn_party != party.gen
        || front.drawn_challenges != challenges.gen
        || now - *looked >= SIGNIN_LOOK_S
        || now < *looked;
    if !due {
        challenges.poll();
        return;
    }
    *looked = now;
    let signin = door.signin();
    // Challenges: asked for while the title is up, for whoever is signed in.
    let wallet = match &signin {
        SignIn::In(p) => p.wallet.clone(),
        _ => None,
    };
    challenges.want(wallet, now);
    challenges.poll();
    if !front.dirty
        && front.drawn.as_ref() == Some(&signin)
        && front.drawn_party == party.gen
        && front.drawn_challenges == challenges.gen
    {
        return;
    }
    front.dirty = false;
    // Triple T shows his back while the emblem is being picked.
    if let Some(p) = portrait.as_deref_mut() {
        let (g, ..) = proto::emblem_parts(player.emblem);
        p.show_back(front.tab == Tab::Armory && front.armory == ARMORY_EMBLEM && g != 0);
    }
    front.drawn = Some(signin);
    front.drawn_party = party.gen;
    front.drawn_challenges = challenges.gen;
    for e in &roots {
        commands.entity(e).despawn();
    }
    pics.begin();
    build(
        &mut commands,
        &front,
        &player,
        &door,
        knocking.0.is_some(),
        portrait.as_deref(),
        &party,
        &challenges,
        (&mut pics, &mut images),
    );
    pics.end(&mut images);
}

/// The curtain over the backdrop: up until its level is there (built,
/// dressed, its models warm and its pipelines compiled: `loading::Settle`),
/// then lifted.
fn curtain(
    settle: Res<crate::loading::Settle>,
    gen: Res<LinkGen>,
    time: Res<Time<Real>>,
    mut since: Local<Option<(u32, f64)>>,
    mut q: Query<&mut BackgroundColor, With<Curtain>>,
) {
    let now = time.elapsed_secs_f64();
    let ours = settle.gen() == Some(gen.0);
    let guard = settle.now.built && settle.age(now) >= CURTAIN_GUARD_S;
    if !ours || !(settle.ready() || guard) {
        *since = None;
        return;
    }
    let at = match *since {
        Some((g, at)) if g == gen.0 => at,
        _ => {
            if !settle.ready() {
                warn!(
                    "oneshot: the title's curtain lifted at its guard, at {}",
                    settle.now.stage()
                );
            }
            *since = Some((gen.0, now));
            now
        }
    };
    let u = ((now - at - CURTAIN_HOLD_S) / CURTAIN_FADE_S).clamp(0.0, 1.0) as f32;
    let a = 1.0 - u * u * (3.0 - 2.0 * u);
    for mut bg in &mut q {
        let want = if a <= 0.0 { Color::NONE } else { ui::BG.with_alpha(a) };
        if bg.0 != want && bg.0 != Color::NONE {
            bg.0 = want;
        }
    }
}

/// What the party asked: its leader started a match, so we knock with it.
fn page_cmds(
    mut front: ResMut<Front>,
    mut door: NonSendMut<Door>,
    mut knocking: NonSendMut<Knocking>,
    in_match: Res<InMatch>,
    time: Res<Time>,
) {
    let cmds: Vec<PageCmd> = PAGE.with(|p| std::mem::take(&mut *p.borrow_mut()));
    for c in cmds {
        match c {
            PageCmd::Knock(ask) if !in_match.0 => {
                front.mode = ask.mode;
                front.status.clear();
                front.bad = false;
                front.dirty = true;
                // The knock it replaces is this one's, cancel guard and all.
                front.knocked_at = time.elapsed_secs();
                knocking.0 = Some(door.knock(ask));
            }
            PageCmd::Knock(_) => {}
        }
    }
}

/// `Start::Join`: knock at once.
fn join(
    mut front: ResMut<Front>,
    player: Res<Player>,
    mut door: NonSendMut<Door>,
    mut knocking: NonSendMut<Knocking>,
    time: Res<Time>,
) {
    let Some(m) = front.join.take() else {
        return;
    };
    front.knocked_at = time.elapsed_secs();
    knocking.0 = Some(door.knock(Ask {
        name: player.sent_name(),
        pass: String::new(),
        mode: m,
        look: player.look(),
        party: 0,
    }));
    front.dirty = true;
}

/// What the knock heard: a line, the welcome (the match starts), or no.
#[allow(clippy::too_many_arguments)]
fn poll(
    mut knocking: NonSendMut<Knocking>,
    mut link: NonSendMut<LinkRes>,
    mut front: ResMut<Front>,
    mut in_match: ResMut<InMatch>,
    mut gen: ResMut<LinkGen>,
    mut early: ResMut<crate::Early>,
    mut st: ResMut<hud::HudState>,
    mut fx: ResMut<crate::look::fx::Fx>,
    mut next: ResMut<NextState<Screen>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut look: ResMut<crate::Look>,
    mut ear: ResMut<crate::sound::Ear>,
) {
    let Some(k) = knocking.0.as_mut() else {
        return;
    };
    for news in k.poll() {
        front.dirty = true;
        match news {
            News::Line(l) => {
                if l == crate::door::MATCHED {
                    ear.ping("match_found_0");
                }
                front.status = l;
                front.bad = false;
            }
            News::Failed(e) => {
                // Never a progress line in red: the refusal, or that it closed.
                front.status = if e.is_empty() {
                    "The server closed the connection".into()
                } else {
                    e
                };
                front.bad = true;
                knocking.0 = None;
                return;
            }
            News::In(l, msgs) => {
                link.0 = l;
                early.0 = msgs;
                in_match.0 = true;
                gen.0 += 1;
                *st = st.fresh();
                fx.marks.clear();
                front.status.clear();
                front.bad = false;
                front.ended_at = None;
                knocking.0 = None;
                // The mouse, taken for the match. Not in a browser, which
                // wants a click of its own for that (`grab`).
                if front.open == Screen::Playing && cfg!(not(target_arch = "wasm32")) {
                    crate::pause::lock_cursor(&mut cursor, &mut look);
                }
                next.set(front.open);
                return;
            }
        }
    }
}

/// The match over (the server said goodbye, or hung up), or left: the
/// title, saying why.
#[allow(clippy::too_many_arguments)]
fn ended(
    mut link: NonSendMut<LinkRes>,
    mut in_match: ResMut<InMatch>,
    mut leaving: ResMut<Leaving>,
    mut gen: ResMut<LinkGen>,
    mut early: ResMut<crate::Early>,
    mut st: ResMut<hud::HudState>,
    mut fx: ResMut<crate::look::fx::Fx>,
    mut front: ResMut<Front>,
    time: Res<Time>,
    mut next: ResMut<NextState<Screen>>,
) {
    if !in_match.0 {
        leaving.0 = None;
        return;
    }
    let now = time.elapsed_secs();
    let over = st.ended || link.0.closed();
    let why = if let Some(why) = leaving.0.take() {
        why
    } else if over {
        let at = *front.ended_at.get_or_insert(now);
        if now - at < END_HOLD_S {
            return;
        }
        let mut gone = String::new();
        hud::gone_line(&mut gone, st.ended, &*link.0);
        [gone.as_str(), st.cup.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" · ")
    } else {
        return;
    };
    if !link.0.closed() {
        link.0.leave();
    }
    link.0 = backdrop();
    in_match.0 = false;
    gen.0 += 1;
    early.0.clear();
    *st = st.fresh();
    fx.marks.clear();
    front.status = why;
    front.bad = false;
    front.ended_at = None;
    front.dirty = true;
    next.set(Screen::Title);
}

/// The HUD only in a match, and not under a menu.
#[allow(clippy::type_complexity)]
fn hud_shown(
    in_match: Res<InMatch>,
    screen: Res<State<Screen>>,
    mut q: Query<
        &mut Visibility,
        Or<(
            With<hud::HudRoot>,
            With<crate::visor::TagRoot>,
            With<crate::scores::ScoresRoot>,
        )>,
    >,
) {
    let on = in_match.0 && *screen.get() == Screen::Playing;
    let want = if on { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut q {
        v.set_if_neq(want);
    }
}

/// Off a match, the camera drifts over the backdrop: Gates' 70 s push,
/// eased, one way and back.
fn fly(in_match: Res<InMatch>, time: Res<Time>, mut cam: Query<&mut Transform, With<crate::Eye>>) {
    if in_match.0 {
        return;
    }
    let Ok(mut t) = cam.single_mut() else {
        return;
    };
    let u = (time.elapsed_secs() % (2.0 * DRIFT_S)) / DRIFT_S;
    let u = if u > 1.0 { 2.0 - u } else { u };
    let e = u * u * (3.0 - 2.0 * u) * 2.0 - 1.0;
    let fwd = (SHOT_LOOK - SHOT_EYE).normalize();
    let side = fwd.cross(Vec3::Y).normalize();
    let eye = SHOT_EYE + side * (SWAY * e) + Vec3::Y * (0.4 * e);
    *t = Transform::from_translation(eye).looking_at(SHOT_LOOK + side * (0.3 * SWAY * e), Vec3::Y);
}

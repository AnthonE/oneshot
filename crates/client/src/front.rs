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
use crate::settings::{Player, NAME_MAX};
use crate::{hud, link, ui, LinkRes, Screen};

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
/// [`SWAY`] metres either side over [`DRIFT_S`] (the hall is 26 m across).
const SHOT_EYE: Vec3 = Vec3::new(-10.0, 6.5, -10.0);
const SHOT_LOOK: Vec3 = Vec3::new(8.0, 2.0, 5.0);
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

const MODES: [Mode; 7] = [
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
        id: proto::MODE_DUEL,
        name: "DUEL",
        short: "1V1 · RATED",
        about: "1v1 Slayer: first to 15 kills or 10 minutes. Rated on Elo Pros.",
        ranked: true,
        tag: "mode1",
    },
    Mode {
        id: proto::MODE_DOUBLES,
        name: "DOUBLES",
        short: "2V2 · RATED",
        about: "2v2 Team Slayer: first to 30 or 12 minutes. Rated on Elo Pros.",
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

/// The title's own state.
#[derive(Resource)]
pub(crate) struct Front {
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

#[derive(Component)]
struct TitleRoot;

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
    Skin(i32),
    Name,
    SignIn,
    Go,
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
        })
        .init_resource::<InMatch>()
        .init_resource::<Leaving>()
        .init_resource::<LinkGen>()
        .add_systems(OnEnter(Screen::Title), enter)
        .add_systems(OnExit(Screen::Title), exit)
        .add_systems(
            Update,
            (click, typing, rebuild, curtain)
                .chain()
                .run_if(in_state(Screen::Title)),
        )
        .add_systems(Update, (join, poll, ended, hud_shown).chain())
        .add_systems(Update, fly.after(crate::view));
}

fn enter(mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>, mut front: ResMut<Front>) {
    crate::pause::free(&mut cursor);
    front.dirty = true;
}

fn exit(mut commands: Commands, roots: Query<Entity, With<TitleRoot>>, mut front: ResMut<Front>) {
    for e in &roots {
        commands.entity(e).despawn();
    }
    front.editing = false;
}

/// The gutter either side of the shell.
const GUT: f32 = 40.0;

fn build(
    commands: &mut Commands,
    front: &Front,
    player: &Player,
    door: &Door,
    knocking: bool,
    portrait: Option<&crate::look::portrait::Portrait>,
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
                top.spawn(ui::wordmark("ARENA SLAYER · RANKED ON ELO PROS"));
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
                nav(body, door.quits());
                panel(body, front, player, &signin, knocking);
                stage(body, front.mode, portrait);
            });
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
            a.spawn(ui::label("Signing in on Elo Pros...", 13.0, ui::DIM));
        }
        SignIn::In(p) => {
            a.spawn(ui::strong("SIGNED IN", 15.0, ui::TITLE));
            match &p.wallet {
                Some(w) => a.spawn(ui::strong(door::short_wallet(w), 12.5, ui::ELO)),
                None => a.spawn(ui::label("Link a wallet on Elo Pros to play ranked", 12.5, ui::DIM)),
            };
        }
        SignIn::Failed(e) => {
            a.spawn((ui::button(220.0, false), Hit::SignIn, Tag("signin")))
                .with_child(ui::button_label("SIGN IN WITH ELO PROS"));
            a.spawn(ui::label(e.clone(), 12.5, ui::BAD));
        }
    });
}

fn nav(body: &mut ChildSpawnerCommands, quits: bool) {
    body.spawn(Node {
        width: Val::Px(230.0),
        flex_shrink: 0.0,
        flex_direction: FlexDirection::Column,
        padding: UiRect::new(Val::Px(15.0), Val::Px(0.0), Val::Px(4.0), Val::Px(0.0)),
        ..default()
    })
    .with_children(|n| {
        n.spawn((ui::nav_item("PLAY", true, 26.0), Tag("nav-play")));
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

fn panel(body: &mut ChildSpawnerCommands, front: &Front, player: &Player, signin: &SignIn, knocking: bool) {
    body.spawn(ui::panel(380.0)).with_children(|p| {
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
        // The skin: worn if the wallet owns it.
        p.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|r| {
            r.spawn(ui::strong("SKIN", 12.0, ui::FAINT));
            r.spawn(ui::label("if your wallet owns it", 11.0, ui::FAINT));
            r.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            r.spawn((ui::stepper(), Hit::Skin(-1)))
                .with_child(ui::strong("<", 14.0, ui::TEXT_COLOR));
            r.spawn(Node {
                width: Val::Px(96.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_child(ui::strong(skin_name(player.skin), 14.0, ui::TEXT_COLOR));
            r.spawn((ui::stepper(), Hit::Skin(1)))
                .with_child(ui::strong(">", 14.0, ui::TEXT_COLOR));
        });

        p.spawn(ui::group("CASUAL"));
        for m in MODES.iter().filter(|m| !m.ranked) {
            mode_row(p, m, front.mode == m.id, true);
        }
        p.spawn(ui::group("RANKED"));
        let open = signin.ranked();
        if !open {
            let why = match signin {
                SignIn::Off(why) => why.clone(),
                SignIn::In(_) => "Link a wallet on Elo Pros to play ranked.".into(),
                _ => "Sign in with Elo Pros, a wallet linked, to play ranked.".into(),
            };
            p.spawn(ui::label(why, 12.0, ui::FAINT));
        }
        for m in MODES.iter().filter(|m| m.ranked) {
            mode_row(p, m, front.mode == m.id, open);
        }

        p.spawn(ui::spacer());
        p.spawn((
            ui::label(front.status.clone(), 13.5, if front.bad { ui::BAD } else { ui::DIM }),
            Node {
                min_height: Val::Px(18.0),
                flex_shrink: 0.0,
                ..default()
            },
        ));
        let m = mode(front.mode);
        let ready = !knocking;
        let ink = ui::go_ink(ready);
        let (word, sub) = if knocking {
            ("CANCEL".to_string(), "STOP LOOKING FOR A MATCH".to_string())
        } else {
            ("PLAY".to_string(), format!("{} · {}", m.name, m.short))
        };
        p.spawn((ui::go(ready), Hit::Go, Tag("go"))).with_children(|g| {
            g.spawn(ui::strong(word, 28.0, ink));
            g.spawn(ui::strong(sub, 11.0, ink.with_alpha(0.72)));
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
fn stage(body: &mut ChildSpawnerCommands, id: u8, portrait: Option<&crate::look::portrait::Portrait>) {
    let m = mode(id);
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
                        k.spawn((ui::strong(m.short, 12.0, ui::TITLE), ui::TEXT_SHADOW));
                    });
                    hero.spawn((
                        Text::new(m.name),
                        ui::display(44.0),
                        TextColor(ui::TITLE),
                        ui::TEXT_SHADOW,
                    ));
                    hero.spawn((
                        ui::label(m.about, 16.0, ui::TEXT_COLOR.with_alpha(0.88)),
                        ui::TEXT_SHADOW,
                    ));
                });
            });
    });
}

fn skin_name(i: u8) -> String {
    proto::SKINS
        .get(i as usize)
        .copied()
        .unwrap_or("standard")
        .to_uppercase()
}

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
        Hit::Skin(d) => {
            let n = proto::SKINS.len() as i32;
            player.skin = (player.skin as i32 + d).rem_euclid(n) as u8;
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
                } else {
                    let ask = Ask {
                        name: player.sent_name(),
                        pass: signin.pass(),
                        mode: m.id,
                        skin: player.skin,
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

/// Built again when anything on it moved, or the sign-in did.
fn rebuild(
    mut commands: Commands,
    mut front: ResMut<Front>,
    player: Res<Player>,
    door: NonSend<Door>,
    knocking: NonSend<Knocking>,
    roots: Query<Entity, With<TitleRoot>>,
    portrait: Option<Res<crate::look::portrait::Portrait>>,
) {
    let signin = door.signin();
    if !front.dirty && front.drawn.as_ref() == Some(&signin) {
        return;
    }
    front.dirty = false;
    front.drawn = Some(signin);
    for e in &roots {
        commands.entity(e).despawn();
    }
    build(
        &mut commands,
        &front,
        &player,
        &door,
        knocking.0.is_some(),
        portrait.as_deref(),
    );
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

/// `Start::Join`: knock at once.
fn join(mut front: ResMut<Front>, player: Res<Player>, mut door: NonSendMut<Door>, mut knocking: NonSendMut<Knocking>) {
    let Some(m) = front.join.take() else {
        return;
    };
    knocking.0 = Some(door.knock(Ask {
        name: player.sent_name(),
        pass: String::new(),
        mode: m,
        skin: player.skin,
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
) {
    let Some(k) = knocking.0.as_mut() else {
        return;
    };
    for news in k.poll() {
        front.dirty = true;
        match news {
            News::Line(l) => {
                front.status = l;
                front.bad = false;
            }
            News::Failed(e) => {
                if !e.is_empty() || front.status.is_empty() {
                    front.status = if e.is_empty() {
                        "The server closed the connection".into()
                    } else {
                        e
                    };
                }
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
        [if st.ended { "Match over" } else { "Disconnected" }, st.cup.as_str()]
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
fn hud_shown(in_match: Res<InMatch>, screen: Res<State<Screen>>, mut q: Query<&mut Visibility, With<hud::HudRoot>>) {
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

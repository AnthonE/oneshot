//! The HUD, laid out as the arena shooters' is, in our own blue: the
//! grenades top left and the kill feed under them; the gun top right (its
//! spare rounds and name in a box, the one on our back beside it, the
//! magazine's rounds or the battery's heat under it); the shield top
//! middle, long and thin, health a thin line beneath; the clock, the map
//! and the two scores
//! that matter bottom right. A crosshair, a hit marker on our hits (red on
//! a kill), an arc toward whoever hurt us, and a line in the middle for the
//! countdown, the result and who killed us (then the respawn's count, low
//! in the middle until the scoreboard comes up); the screen's rim (red as
//! health goes, gold as the shield takes a round, a crackle of blue as it
//! breaks); before the welcome, the queue's line. `N` shows the net graph.
//! The scope is `scope.rs`'s.
//! Medals and the carnage report are `medals.rs`'s to draw; their lines in
//! the feed are here. Everything here is read from the session; nothing is
//! decided.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use bevy::prelude::*;
use proto::medal as md;
use proto::{EventState, Msg};
use sim::consts::{HEALTH_MAX, SHIELD_MAX, TICK_HZ};
use sim::events::Kind;

use client_net::session::{refuse_why, Session};

use crate::plate;

const FEED_S: f32 = 5.0;
const FEED_LINES: usize = 5;
/// A feed line's last this-long, it fades out, as a medal does.
const FEED_FADE_S: f32 = 0.8;
/// The feed's lines (the watch line, the flags, the feed's own) and the
/// parts of one: a line is up to three, the middle (how) dim between two
/// names. A part ends at a tab (`feed_parts`).
const FEED_ROWS: usize = FEED_LINES + 2;
const FEED_PARTS: usize = 3;
/// The USE prompt: how far down the screen, a share, and its key's cap.
const PROMPT_TOP_PCT: f32 = 58.0;
const KEY_BG: Color = Color::srgba(0.02, 0.05, 0.1, 0.45);
const KEY_EDGE: Color = Color::srgba(0.74, 0.88, 1.0, 0.55);
const HIT_S: f32 = 0.22;
const DAMAGE_S: f32 = 0.6;
/// The red rim when we're hit (after Gates' `hurt_flash.rs`): its alpha at
/// the blow, how long it fades, and where the red starts (a share of the
/// far-corner radius). Faint, at the edge, gone in half a second.
const RIM_ALPHA: f32 = 0.38;
const RIM_S: f32 = 0.5;
const RIM_CLEAR_PCT: f32 = 52.0;
/// How far out from the middle the damage arc sits, px.
const DAMAGE_R: f32 = 90.0;
/// A round on the shield: a faint gold rim, as long as the red's.
const RIM_SHIELD_ALPHA: f32 = 0.09;
/// The shield breaking: a crackle of blue-white round the edge, how long,
/// how far in it reaches (the share of the far-corner radius it starts
/// at) and how fast it flickers, Hz.
const POP_ALPHA: f32 = 0.55;
const POP_S: f32 = 0.45;
const POP_CLEAR_PCT: f32 = 38.0;
const POP_FLICKER_HZ: f32 = 22.0;
/// Dead this long, the scoreboard comes up until the respawn (Halo 2's),
/// seconds: long enough to watch yourself fly.
pub const DEAD_BOARD_S: f32 = 1.5;

/// The HUD's blue: its type, a box's fill (lit for what's picked, dim for
/// what isn't) and edge, a round in the magazine and one spent.
pub const INK: Color = Color::srgba(0.74, 0.88, 1.0, 0.96);
const INK_DIM: Color = Color::srgba(0.74, 0.88, 1.0, 0.7);
/// A plate's tint (`plate.rs`), lit and dim.
const PLATE: Color = Color::srgba(0.3, 0.6, 1.0, 1.0);
const PLATE_DIM: Color = Color::srgba(0.3, 0.5, 0.86, 0.85);
const EDGE_DIM: Color = Color::srgba(0.4, 0.6, 0.85, 0.45);
pub const PIP: Color = Color::srgba(0.6, 0.84, 1.0, 0.96);
const PIP_SPENT: Color = Color::srgba(0.45, 0.65, 0.9, 0.22);
const SHIELD: Color = Color::srgba(0.38, 0.7, 1.0, 0.95);
const SHIELD_LOW: Color = Color::srgba(1.0, 0.32, 0.22, 0.95);
/// The overshield, over the shield's bar: its share of the way to three
/// times full, green as its shell is drawn.
const OVER: Color = Color::srgba(0.35, 1.0, 0.5, 0.95);
/// Camouflage's seconds left, pale as its shimmer.
const CAMO_INK: Color = Color::srgba(0.7, 0.85, 1.0, 0.9);
const HEALTH: Color = Color::srgba(0.95, 0.36, 0.3, 0.9);
const HEAT: Color = Color::srgba(1.0, 0.62, 0.22, 0.95);
const RED_SIDE: Color = Color::srgba(0.92, 0.24, 0.2, 0.9);
const BLUE_SIDE: Color = Color::srgba(0.24, 0.46, 0.98, 0.9);
const OTHER: Color = Color::srgba(0.62, 0.66, 0.72, 0.85);
/// Off the screen's edges, px.
const EDGE_PX: f32 = 22.0;
/// The shield's width and height, how far down from the top it sits, and
/// the score bars' width.
const SHIELD_W: f32 = 460.0;
const SHIELD_H: f32 = 24.0;
const SHIELD_TOP_PX: f32 = 16.0;
const SCORE_W: f32 = 150.0;
/// A gun's picture in its box, px wide, and its height for its width.
const ICON_W: f32 = 112.0;
const ICON_ASPECT: f32 = crate::look::icons::ICON_H as f32 / crate::look::icons::ICON_W as f32;
/// Under this share the shield is low: its frame pulses red.
const SHIELD_LOW_PCT: f32 = 25.0;
/// Rounds a magazine may show; a bigger one (a battery) is a number.
const PIPS: usize = 60;
/// The shield plate's inset to its level: its glow and edge, and a gap.
const SHIELD_PAD: f32 = 8.0;

/// Names by slot (from `Msg::Seat`), the feed, and the markers' clocks.
#[derive(Resource, Default)]
pub struct HudState {
    pub(crate) names: BTreeMap<u8, String>,
    /// Each seat's skin and emblem, as the server checked them.
    looks: BTreeMap<u8, proto::Look>,
    /// Each seat's side (`Msg::Seat`).
    teams: BTreeMap<u8, u8>,
    feed: Vec<(String, f32)>,
    /// Kills and deaths by seat, as the feed told them (`tally.rs`): the
    /// scoreboard's.
    pub tally: crate::tally::Tally,
    /// Where each seat last fired from, as we were told it (whole, or
    /// heard and rounded): the hurt arc's way to a shooter fog hides.
    shot_from: BTreeMap<u8, sim::geom::V3>,
    hit_at: f32,
    hit_kill: bool,
    hurt_at: f32,
    /// The last hit on us landed on the shield (gold), not health (red).
    hurt_shield: bool,
    /// When our shield last broke, and its level last frame.
    pop_at: f32,
    last_shield: i32,
    arc_at: f32,
    hurt_angle: f32,
    /// How we last died, as a line ("You were sniped by ...").
    pub death_line: String,
    /// The scoreboard is held up.
    pub board: bool,
    /// Our kills, deaths and assists as the stream told them (the page's
    /// stats).
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    /// Our medals on screen: each id and when it came.
    pub medals: Vec<(u8, f32)>,
    /// Bumped when `medals` gains one.
    pub medals_gen: u32,
    /// Our medals for the announcer to say, oldest first (`sound.rs` takes
    /// them).
    pub announce: Vec<u8>,
    /// Lines about the match the feed found for the announcer (a betrayal),
    /// oldest first (`sound.rs` takes them too).
    pub say: Vec<&'static str>,
    /// Every medal of ours this match, by id: the end screen's.
    pub earned: BTreeMap<u8, u16>,
    /// Each seat's spree rung as the stream last told it, and the one it
    /// had when it last died: what a Killjoy ended.
    rung: BTreeMap<u8, u8>,
    spree_ended: BTreeMap<u8, u8>,
    /// The carnage report, a row a seat, once the match is over.
    pub carnage: BTreeMap<u8, Msg>,
    /// The welcomes seen: a new one is a new match (a cup's next heat).
    welcomes: u32,
    /// The last line the stream gave for the menu: the queue, a cup's
    /// standing, a refusal.
    pub status: String,
    /// In a cup: where we stand, as the server last said.
    pub cup: String,
    /// The server said goodbye: the match (or the cup) is over.
    pub ended: bool,
    /// The net graph is up.
    pub net: bool,
}

/// Medals on screen at once, and how long each stays.
pub const MEDALS_SHOWN: usize = 6;
pub const MEDAL_S: f32 = 4.0;
/// Medals the announcer may have waiting.
const ANNOUNCE_MAX: usize = 8;

impl HudState {
    /// A new match's: nothing carried over but the net graph.
    pub fn fresh(&self) -> HudState {
        HudState {
            net: self.net,
            ..default()
        }
    }

    pub fn look(&self, slot: u8) -> proto::Look {
        self.looks.get(&slot).copied().unwrap_or_default()
    }

    /// Every seat's name, by slot (bots' too).
    pub fn names(&self) -> impl Iterator<Item = (&u8, &String)> {
        self.names.iter()
    }

    pub fn name(&self, slot: u8) -> String {
        self.names
            .get(&slot)
            .cloned()
            .unwrap_or_else(|| format!("P{}", slot + 1))
    }

    /// [`HudState::name`] to write, with nothing made.
    pub fn shown(&self, slot: u8) -> Shown<'_> {
        match self.names.get(&slot) {
            Some(n) => Shown::Name(n),
            None => Shown::Seat(slot),
        }
    }
}

/// A seat's name as [`HudState::name`] spells it, for `write!`.
pub enum Shown<'a> {
    Name(&'a str),
    Seat(u8),
}

impl std::fmt::Display for Shown<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Shown::Name(n) => f.write_str(n),
            Shown::Seat(slot) => write!(f, "P{}", slot + 1),
        }
    }
}

/// Every HUD node hangs off this: hidden off a match (`front::hud_shown`).
#[derive(Component)]
pub struct HudRoot;

#[derive(Component)]
pub struct ShieldBar;
/// The overshield's fill, over the shield's.
#[derive(Component)]
pub struct OverBar;
/// Over the shield: camouflage's seconds left, while it is on.
#[derive(Component)]
pub struct CamoText;
/// The shield's box: its edge pulses red when the shield is low.
#[derive(Component)]
pub struct ShieldFrame;
#[derive(Component)]
pub struct HurtRim;
#[derive(Component)]
pub struct HealthBar;
/// Health's track: shown only once health has been taken.
#[derive(Component)]
pub struct HealthLine;
/// A grenade's box (0 frags, 1 stickies), and its count.
#[derive(Component)]
pub struct GrenBox(pub u8);
#[derive(Component)]
pub struct GrenCount(pub u8);
/// A hand's gun (0 the right, 1 the left when dual), and its parts: the
/// big number, the name, a round in the magazine, the heat bar and its
/// fill, the line under it (reloading, venting).
#[derive(Component)]
pub struct GunPanel(pub u8);
#[derive(Component)]
pub struct GunCount(pub u8);
#[derive(Component)]
pub struct GunName(pub u8);
/// A hand's gun as a picture (`look/icons.rs`), where its name was.
#[derive(Component)]
pub struct GunIcon(pub u8);
#[derive(Component)]
pub struct Pip(pub u8, pub usize);
#[derive(Component)]
pub struct HeatBox(pub u8);
#[derive(Component)]
pub struct HeatFill(pub u8);
#[derive(Component)]
pub struct GunNote(pub u8);
/// The gun on our back, named on a dim plate beside the one in hand.
#[derive(Component)]
pub struct BackGun;
#[derive(Component)]
pub struct BackPlate;
/// Driving or riding: how to get out.
#[derive(Component)]
pub struct RideText;
/// Bottom right: the clock, the map (and gun game's rung), and the two
/// score rows: their number, swatch, bar and fill, and our marker.
#[derive(Component)]
pub struct ClockText;
#[derive(Component)]
pub struct ModeText;
#[derive(Component)]
pub struct ScoreRow(pub u8);
#[derive(Component)]
pub struct ScoreNum(pub u8);
#[derive(Component)]
pub struct ScoreSwatch(pub u8);
#[derive(Component)]
pub struct ScoreFill(pub u8);
#[derive(Component)]
pub struct ScoreMine(pub u8);
#[derive(Component)]
pub struct FeedText;
/// One part of one of the feed's lines (`FEED_ROWS` × `FEED_PARTS`).
#[derive(Component)]
pub struct FeedPart(pub usize);
#[derive(Component)]
pub struct CenterText;
/// "Respawn in N", low in the middle while we are dead.
#[derive(Component)]
pub struct RespawnText;
#[derive(Component)]
pub struct NetText;
/// The scoreboard's panel, and its columns: names, kills, deaths, score.
#[derive(Component)]
pub struct Board;
#[derive(Component)]
pub struct BoardColumn(pub u8);
/// What USE would take here (`items::offer`): the row, the key on its
/// cap, the verb, and the gun.
#[derive(Component)]
pub struct PromptRow;
#[derive(Component)]
pub struct PromptKey;
#[derive(Component)]
pub struct PromptVerb;
#[derive(Component)]
pub struct PromptText;
#[derive(Component)]
pub struct Reticle;
/// One of the crosshair's four ticks: white, red on an enemy in reach.
#[derive(Component)]
pub struct ReticleStroke;
#[derive(Component)]
pub struct HitMark;
#[derive(Component)]
pub struct HurtArc;
#[derive(Component)]
pub struct HitStroke;

/// A HUD line in `font`: the display face for numbers and banners, the
/// text face for the feed and the net graph (`ui.rs`).
fn text(font: TextFont) -> impl Bundle {
    tinted(font, Color::srgba(1.0, 1.0, 1.0, 0.92))
}

/// A thin dark shadow under the HUD's type: legible on a bright sky
/// without doubling it.
pub const SHADOW: TextShadow = TextShadow {
    offset: Vec2::new(1.0, 1.5),
    color: Color::srgba(0.0, 0.02, 0.06, 0.55),
};

/// A HUD line in `font` and `color`.
fn tinted(font: TextFont, color: Color) -> impl Bundle {
    (Text::new(""), font, TextColor(color), SHADOW)
}

/// A box in the HUD's blue: lit or dim, its corners cut by `cut`.
fn boxed(pad_x: f32, pad_y: f32) -> Node {
    Node {
        padding: UiRect::axes(Val::Px(pad_x), Val::Px(pad_y)),
        align_items: AlignItems::Center,
        ..default()
    }
}

/// A grenade's mark: a frag's round shell, a sticky's glowing ball.
fn gren_icon(kind: u8) -> impl Bundle {
    let (c, glow) = if kind == 0 {
        (Color::srgba(0.72, 0.86, 1.0, 0.95), Color::NONE)
    } else {
        (Color::srgba(0.75, 0.95, 1.0, 1.0), Color::srgba(0.4, 0.75, 1.0, 0.8))
    };
    (
        Node {
            width: Val::Px(12.0),
            height: Val::Px(if kind == 0 { 14.0 } else { 12.0 }),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(c),
        BoxShadow::new(glow, Val::ZERO, Val::ZERO, Val::Px(1.0), Val::Px(5.0)),
    )
}

/// A hand's gun: a box with the spare rounds and the name, then the
/// magazine's rounds (or a battery's heat), then a note.
fn gun_panel(c: &mut ChildSpawnerCommands, hand: u8, right: bool) {
    let align = if right {
        AlignItems::FlexEnd
    } else {
        AlignItems::FlexStart
    };
    c.spawn((
        GunPanel(hand),
        Node {
            flex_direction: FlexDirection::Column,
            align_items: align,
            row_gap: Val::Px(5.0),
            ..default()
        },
    ))
    .with_children(|c| {
        let mut node = boxed(18.0, 6.0);
        node.column_gap = Val::Px(12.0);
        node.min_height = Val::Px(plate::BOX_MIN + 6.0);
        let plate = if right { plate::BOX_R } else { plate::BOX };
        c.spawn(node).with_children(|c| {
            c.spawn(plate::behind(plate, PLATE));
            c.spawn((GunCount(hand), tinted(crate::ui::display(26.0), INK)));
            c.spawn((
                Node {
                    width: Val::Px(2.0),
                    height: Val::Px(24.0),
                    ..default()
                },
                UiTransform::from_rotation(Rot2::degrees(-20.0)),
                BackgroundColor(EDGE_DIM),
            ));
            c.spawn((GunName(hand), tinted(crate::ui::display(15.0), INK)));
            c.spawn((
                GunIcon(hand),
                Node {
                    width: Val::Px(ICON_W),
                    height: Val::Px(ICON_W * ICON_ASPECT),
                    display: Display::None,
                    ..default()
                },
                ImageNode::default().with_color(INK),
            ));
        });
        // The magazine: a round each, wrapping, from the box's side.
        c.spawn(Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            justify_content: if right {
                JustifyContent::FlexEnd
            } else {
                JustifyContent::FlexStart
            },
            max_width: Val::Px(240.0),
            column_gap: Val::Px(2.0),
            row_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|c| {
            for i in 0..PIPS {
                c.spawn((
                    Pip(hand, i),
                    Node {
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(PIP),
                ));
            }
        });
        c.spawn((
            HeatBox(hand),
            Node {
                width: Val::Px(160.0),
                height: Val::Px(plate::BAR_MIN),
                padding: UiRect::all(Val::Px(8.0)),
                display: Display::None,
                ..default()
            },
            children![
                plate::behind(plate::BAR, PLATE_DIM),
                (
                    HeatFill(hand),
                    Node {
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    plate::image(plate::FILL, HEAT),
                )
            ],
        ));
        c.spawn((GunNote(hand), tinted(crate::ui::semibold(14.0), INK)));
    });
}

/// A score row: the number, a swatch of the side, a bar to the limit, and
/// our marker.
fn score_row(c: &mut ChildSpawnerCommands, i: u8) {
    c.spawn((
        ScoreRow(i),
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            ..default()
        },
    ))
    .with_children(|c| {
        c.spawn((
            Node {
                min_width: Val::Px(34.0),
                justify_content: JustifyContent::FlexEnd,
                ..default()
            },
            children![(
                ScoreNum(i),
                tinted(crate::ui::display(20.0), INK),
                TextLayout::new_with_justify(Justify::Right),
            )],
        ));
        c.spawn((
            ScoreSwatch(i),
            Node {
                width: Val::Px(14.0),
                height: Val::Px(16.0),
                ..default()
            },
            plate::image(plate::FILL, OTHER),
        ));
        c.spawn((
            Node {
                width: Val::Px(SCORE_W),
                height: Val::Px(plate::BAR_MIN),
                padding: UiRect::all(Val::Px(7.0)),
                ..default()
            },
            children![
                plate::behind(plate::BAR, PLATE_DIM),
                (
                    ScoreFill(i),
                    Node {
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    plate::image(plate::FILL, OTHER),
                )
            ],
        ));
        // Ours: a notch pointing at the row.
        c.spawn((
            ScoreMine(i),
            Node {
                width: Val::Px(8.0),
                height: Val::Px(8.0),
                ..default()
            },
            UiTransform::from_rotation(Rot2::degrees(45.0)),
            BackgroundColor(INK),
            Visibility::Hidden,
        ));
    });
}

pub fn setup(mut commands: Commands) {
    let abs = |left: Option<f32>, right: Option<f32>, top: Option<f32>, bottom: Option<f32>| Node {
        position_type: PositionType::Absolute,
        left: left.map_or(Val::Auto, Val::Px),
        right: right.map_or(Val::Auto, Val::Px),
        top: top.map_or(Val::Auto, Val::Px),
        bottom: bottom.map_or(Val::Auto, Val::Px),
        ..default()
    };
    // One root for all of it, so the title can hide it.
    let root = commands
        .spawn((
            HudRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
        ))
        .id();
    // Under the HUD, over the world.
    commands.spawn((
        ChildOf(root),
        HurtRim,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundGradient(vec![Gradient::Radial(RadialGradient::new(
            UiPosition::CENTER,
            RadialGradientShape::FarthestCorner,
            vec![
                ColorStop::new(Color::NONE, Val::Percent(RIM_CLEAR_PCT)),
                ColorStop::new(rim(0.0), Val::Percent(100.0)),
            ],
        ))]),
        ZIndex(-1),
        Visibility::Hidden,
    ));
    // Top left: the grenades, the left hand's gun when there are two, and
    // the feed.
    commands
        .spawn((
            ChildOf(root),
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexStart,
                row_gap: Val::Px(10.0),
                ..abs(Some(EDGE_PX), None, Some(18.0), None)
            },
        ))
        .with_children(|c| {
            c.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|c| {
                for kind in [0u8, 1] {
                    let mut node = boxed(16.0, 5.0);
                    node.column_gap = Val::Px(7.0);
                    node.min_height = Val::Px(plate::BOX_MIN);
                    c.spawn(node).with_children(|c| {
                        c.spawn((GrenBox(kind), plate::behind(plate::BOX, PLATE)));
                        c.spawn((GrenCount(kind), tinted(crate::ui::display(22.0), INK)));
                        c.spawn((Text::new("\u{d7}"), crate::ui::semibold(16.0), TextColor(INK), SHADOW));
                        c.spawn(gren_icon(kind));
                    });
                }
            });
            gun_panel(c, 1, false);
            // The feed: a pool of parts, each line a name, how (small and
            // dim) and a name, written and faded by `feed`.
            c.spawn((FeedText, tinted(crate::ui::semibold(15.0), INK)))
                .with_children(|t| {
                    for i in 0..FEED_ROWS * FEED_PARTS {
                        let font = if i % FEED_PARTS == 1 {
                            crate::ui::semibold(12.0)
                        } else {
                            crate::ui::semibold(15.0)
                        };
                        t.spawn((FeedPart(i), TextSpan::new(""), font, TextColor(INK)));
                    }
                });
        });
    // Top right: the gun in hand, the one on our back beside it; under it,
    // how to get out of a ride.
    commands
        .spawn((
            ChildOf(root),
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: Val::Px(6.0),
                ..abs(None, Some(EDGE_PX), Some(18.0), None)
            },
        ))
        .with_children(|c| {
            c.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::FlexStart,
                column_gap: Val::Px(10.0),
                ..default()
            })
            .with_children(|c| {
                c.spawn((
                    BackPlate,
                    Node {
                        margin: UiRect::top(Val::Px(8.0)),
                        min_height: Val::Px(plate::BAR_MIN),
                        ..boxed(12.0, 4.0)
                    },
                    children![
                        plate::behind(plate::BAR, PLATE_DIM),
                        (BackGun, tinted(crate::ui::display(13.0), INK_DIM))
                    ],
                ));
                gun_panel(c, 0, true);
            });
            c.spawn((RideText, tinted(crate::ui::semibold(15.0), INK)));
        });
    // Top middle: the shield, long and thin in its box, cut into tenths and
    // draining from both ends in to the middle; health a thin line under it.
    commands
        .spawn((
            ChildOf(root),
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-SHIELD_W * 0.5)),
                width: Val::Px(SHIELD_W),
                ..abs(None, None, Some(SHIELD_TOP_PX), None)
            },
        ))
        .with_children(|c| {
            c.spawn((
                CamoText,
                Visibility::Hidden,
                tinted(crate::ui::semibold(14.0), CAMO_INK),
            ));
            c.spawn(Node {
                width: Val::Px(SHIELD_W),
                height: Val::Px(SHIELD_H),
                padding: UiRect::all(Val::Px(SHIELD_PAD)),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_children(|c| {
                c.spawn((ShieldFrame, plate::behind(plate::BAR, PLATE)));
                c.spawn((
                    ShieldBar,
                    Node {
                        height: Val::Percent(100.0),
                        width: Val::Percent(100.0),
                        ..default()
                    },
                    plate::image(plate::FILL, SHIELD),
                ));
                c.spawn((
                    OverBar,
                    Node {
                        width: Val::Px(0.0),
                        ..abs(Some(SHIELD_PAD), None, Some(SHIELD_PAD), Some(SHIELD_PAD))
                    },
                    plate::image(plate::FILL, OVER),
                ));
                // The tenths, cut across the level.
                let inner = SHIELD_W - 2.0 * SHIELD_PAD;
                for i in 1..10 {
                    c.spawn((
                        Node {
                            width: Val::Px(2.0),
                            ..abs(
                                Some(SHIELD_PAD + i as f32 * inner / 10.0 - 1.0),
                                None,
                                Some(SHIELD_PAD),
                                Some(SHIELD_PAD),
                            )
                        },
                        UiTransform::from_rotation(Rot2::degrees(-20.0)),
                        BackgroundColor(Color::srgba(0.02, 0.06, 0.14, 0.6)),
                    ));
                }
            });
            c.spawn((
                HealthLine,
                Visibility::Hidden,
                Node {
                    width: Val::Px(SHIELD_W * 0.6),
                    height: Val::Px(4.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
                children![(
                    HealthBar,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(HEALTH),
                )],
            ));
        });
    // Bottom right: the clock, the map, the scores.
    commands
        .spawn((
            ChildOf(root),
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: Val::Px(4.0),
                ..abs(None, Some(EDGE_PX), None, Some(18.0))
            },
        ))
        .with_children(|c| {
            c.spawn((ClockText, tinted(crate::ui::display(22.0), INK)));
            c.spawn((
                ModeText,
                tinted(crate::ui::semibold(16.0), INK),
                TextLayout::new_with_justify(Justify::Right),
            ));
            score_row(c, 0);
            score_row(c, 1);
        });
    commands.spawn((
        ChildOf(root),
        NetText,
        text(crate::ui::text(14.0)),
        abs(Some(EDGE_PX), None, None, Some(72.0)),
    ));
    // The middle: a full-width row, its text centred.
    commands.spawn((
        ChildOf(root),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            top: Val::Percent(30.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(CenterText, text(crate::ui::display(26.0)))],
    ));
    commands.spawn((
        ChildOf(root),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            bottom: Val::Percent(16.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(RespawnText, tinted(crate::ui::display(20.0), INK))],
    ));
    // The scoreboard: a panel in the middle, four columns.
    commands
        .spawn((
            Board,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(37.0),
                width: Val::Px(520.0),
                margin: UiRect::left(Val::Px(-260.0)),
                padding: UiRect::axes(Val::Px(18.0), Val::Px(14.0)),
                column_gap: Val::Px(24.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            // The game's own panel (`ui.rs`), as the Esc menu's.
            BackgroundColor(crate::ui::PANEL_BG),
            BorderColor::all(crate::ui::RULE),
            Visibility::Hidden,
        ))
        .with_children(|c| {
            for (i, grow) in [(0u8, 1.0), (1, 0.0), (2, 0.0), (3, 0.0)] {
                c.spawn((
                    Node {
                        flex_grow: grow,
                        min_width: Val::Px(56.0),
                        ..default()
                    },
                    children![(
                        BoardColumn(i),
                        tinted(crate::ui::semibold(16.0), crate::ui::TEXT_COLOR),
                        TextLayout::new_with_justify(if i == 0 { Justify::Left } else { Justify::Right }),
                    )],
                ));
            }
        });
    // Under the crosshair: what USE would take, small: the key on a cap,
    // then the verb, dim, and the gun.
    commands.spawn((
        ChildOf(root),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            top: Val::Percent(PROMPT_TOP_PCT),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            PromptRow,
            Node {
                align_items: AlignItems::Center,
                column_gap: Val::Px(7.0),
                ..default()
            },
            Visibility::Hidden,
            children![
                (
                    Node {
                        min_width: Val::Px(20.0),
                        padding: UiRect::axes(Val::Px(6.0), Val::Px(1.0)),
                        justify_content: JustifyContent::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(KEY_BG),
                    BorderColor::all(KEY_EDGE),
                    children![(PromptKey, tinted(crate::ui::bold(12.0), INK))],
                ),
                (PromptVerb, tinted(crate::ui::semibold(13.0), INK_DIM)),
                (PromptText, tinted(crate::ui::bold(13.0), INK)),
            ],
        )],
    ));
    // The crosshair, the hit marker and the damage arc, all centred on the
    // screen's middle.
    let centred = |w: f32, h: f32| Node {
        position_type: PositionType::Absolute,
        left: Val::Percent(50.0),
        top: Val::Percent(50.0),
        width: Val::Px(w),
        height: Val::Px(h),
        margin: UiRect {
            left: Val::Px(-w / 2.0),
            top: Val::Px(-h / 2.0),
            ..default()
        },
        ..default()
    };
    commands
        .spawn((ChildOf(root), Reticle, centred(22.0, 22.0)))
        .with_children(|c| {
            let white = BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.85));
            for (l, t, w, h) in [
                (0.0, 10.0, 7.0, 2.0),
                (15.0, 10.0, 7.0, 2.0),
                (10.0, 0.0, 2.0, 7.0),
                (10.0, 15.0, 2.0, 7.0),
            ] {
                c.spawn((
                    ReticleStroke,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(l),
                        top: Val::Px(t),
                        width: Val::Px(w),
                        height: Val::Px(h),
                        ..default()
                    },
                    white,
                ));
            }
        });
    commands.spawn((
        ChildOf(root),
        HitMark,
        centred(26.0, 26.0),
        UiTransform::from_rotation(Rot2::degrees(45.0)),
        Visibility::Hidden,
        children![
            (
                HitStroke,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(12.0),
                    width: Val::Px(26.0),
                    height: Val::Px(2.0),
                    ..default()
                },
                BackgroundColor(Color::WHITE),
            ),
            (
                HitStroke,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(12.0),
                    top: Val::Px(0.0),
                    width: Val::Px(2.0),
                    height: Val::Px(26.0),
                    ..default()
                },
                BackgroundColor(Color::WHITE),
            ),
        ],
    ));
    commands.spawn((
        ChildOf(root),
        HurtArc,
        centred(56.0, 6.0),
        BackgroundColor(Color::srgba(1.0, 0.2, 0.15, 0.0)),
    ));
}

fn ordinal(n: u8) -> String {
    let th = match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{n}{th}")
}

/// What a stream message says to someone waiting: the queue, a cup's
/// standing, a refusal. The page's menu shows it until the welcome.
pub fn status_line(m: &Msg) -> Option<String> {
    Some(match *m {
        Msg::Queued { mode, waiting_s } => {
            let what = match mode {
                proto::MODE_CUP => "Waiting for the cup to fill",
                proto::MODE_DOUBLES => "Finding a 2v2",
                _ => "Finding an opponent",
            };
            format!("{what}... {}", mmss(waiting_s as f32))
        }
        Msg::Cup {
            round,
            rounds,
            entrants,
            place,
            points,
            done,
        } => {
            if done {
                format!("Cup over: {} of {entrants}", ordinal(place))
            } else {
                let stage = if round > rounds {
                    "the final".to_string()
                } else {
                    format!("round {round} of {rounds}")
                };
                format!("Cup, {stage}: {} of {entrants} on {points} points", ordinal(place))
            }
        }
        Msg::Refuse { reason } => refuse_why(reason).to_string(),
        _ => return None,
    })
}

/// A medal from the stream. Ours: on screen, counted, and for the
/// announcer if it says it. Anyone's that the feed carries (sprees, the
/// big multi-kills, a spree ended): a line in it.
fn on_medal(st: &mut HudState, me: u8, slot: u8, medal: u8, victim: u8, now: f32) {
    let info = md::info(medal);
    if info.family == md::Family::Spree {
        st.rung.insert(slot, medal);
    }
    if slot == me {
        st.medals.push((medal, now));
        if st.medals.len() > MEDALS_SHOWN {
            st.medals.remove(0);
        }
        st.medals_gen = st.medals_gen.wrapping_add(1);
        *st.earned.entry(medal).or_default() += 1;
        if medal == md::ASSIST {
            st.assists += 1;
        }
        if info.voiced {
            st.announce.push(medal);
            if st.announce.len() > ANNOUNCE_MAX {
                st.announce.remove(0);
            }
        }
    }
    if let Some(line) = feed_line(st, me, slot, medal, victim) {
        st.feed.push((line, now));
    }
}

/// What the feed says of `slot`'s `medal`, if anything: the rungs everyone
/// hears of, and a spree's end.
fn feed_line(st: &HudState, me: u8, slot: u8, medal: u8, victim: u8) -> Option<String> {
    let who = if slot == me { "You".to_string() } else { st.name(slot) };
    let name = md::info(medal).name;
    Some(match md::info(medal).family {
        md::Family::Spree if slot == me => return None,
        md::Family::Spree => format!("{who} is on a {name}"),
        md::Family::Multi if medal >= md::OVERKILL && slot != me => format!("{who}: {name}"),
        _ => match medal {
            md::KILLJOY => {
                let spree = st.spree_ended.get(&victim).map_or("spree", |&r| md::info(r).name);
                let whose = if victim == me {
                    "your".to_string()
                } else {
                    format!("{}'s", st.name(victim))
                };
                format!("{who} ended {whose} {spree}")
            }
            md::EXTERMINATION | md::PERFECTION => format!("{who}: {name}"),
            _ => return None,
        },
    })
}

/// A side's name, as the flags are called.
fn side_name(t: u8) -> &'static str {
    if t & 1 == 0 {
        "RED"
    } else {
        "BLUE"
    }
}

/// What the feed says of a flag event (`sim::events::Kind::Flag`).
fn flag_line(st: &HudState, part: u8, who: u8, flag: u8) -> String {
    let f = side_name(flag);
    match part {
        0 => format!("{} took the {f} flag", st.name(who)),
        1 => format!("{f} flag dropped"),
        2 if who == 255 => format!("{f} flag returned"),
        2 => format!("{} returned the {f} flag", st.name(who)),
        _ => format!("{} captured the {f} flag", st.name(who)),
    }
}

/// Capture the Flag: where each flag is, one line, over the feed, onto
/// `out`.
fn flags_line(out: &mut String, st: &HudState, s: &proto::Snapshot) {
    for (t, f) in s.flags().iter().enumerate() {
        if t > 0 {
            out.push_str("    ");
        }
        let _ = write!(out, "{} FLAG ", side_name(t as u8));
        let _ = match f.state {
            sim::world::FLAG_HELD => write!(out, "TAKEN by {}", st.shown(f.carrier)),
            sim::world::FLAG_DOWN => write!(out, "DROPPED, home in {}s", f.down_s),
            _ => write!(out, "HOME"),
        };
    }
}

/// This frame's events and stream messages: the markers, names, the feed.
pub fn on_events(
    st: &mut HudState,
    s: &Session,
    others: &[client_net::core::Drawn],
    events: &[EventState],
    msgs: &[Msg],
    now: f32,
    yaw: f32,
) {
    let me = s.core.slot;
    // A new match (a cup's next heat): its medals and report start over.
    if s.welcomes != st.welcomes {
        st.welcomes = s.welcomes;
        st.medals.clear();
        st.announce.clear();
        st.say.clear();
        st.earned.clear();
        st.rung.clear();
        st.spree_ended.clear();
        st.carnage.clear();
        st.assists = 0;
        st.medals_gen = st.medals_gen.wrapping_add(1);
    }
    for e in events {
        if e.kind == Kind::Shot as u8 || e.kind == Kind::Throw as u8 {
            st.shot_from.insert(e.a, e.pos);
        }
        if e.kind == Kind::Hit as u8 {
            if e.a == me && e.b != me {
                st.hit_at = now;
                st.hit_kill = false;
            }
            if e.b == me && e.a != me {
                // Where it came from, relative to where we look: where the
                // shooter is drawn, else where we last heard them fire (a
                // hit's own place is ours). Neither: the rim alone.
                let (_, p) = s.core.own();
                let from = others.iter().find(|d| d.slot == e.a).map(|d| d.pos);
                if let Some(from) = from.or_else(|| st.shot_from.get(&e.a).copied()) {
                    st.hurt_angle = (from.z - p.z).atan2(from.x - p.x) - yaw;
                    st.arc_at = now;
                }
                st.hurt_at = now;
                st.hurt_shield = s.core.own().0.shield > 0;
            }
        } else if e.kind == Kind::Kill as u8 && e.a == me && e.b != me {
            st.hit_at = now;
            st.hit_kill = true;
        } else if e.kind == Kind::Flag as u8 {
            st.feed.push((flag_line(st, e.part, e.a, e.b), now));
        }
    }
    // Our shield breaking.
    let f = s.core.own().0;
    if f.alive && f.shield == 0 && st.last_shield > 0 {
        st.pop_at = now;
    }
    st.last_shield = if f.alive { f.shield } else { 0 };
    for m in msgs {
        if let Some(line) = status_line(m) {
            if matches!(m, Msg::Cup { .. }) {
                st.cup = line.clone();
            }
            st.status = line;
        }
        match m {
            Msg::Seat {
                slot,
                name,
                skin,
                emblem,
                headwear,
                footwear,
                team,
                ..
            } => {
                st.names.insert(*slot, name.as_str().to_string());
                st.looks.insert(
                    *slot,
                    proto::Look {
                        skin: *skin,
                        emblem: *emblem,
                        headwear: *headwear,
                        footwear: *footwear,
                    },
                );
                st.teams.insert(*slot, *team);
            }
            Msg::Kill {
                killer,
                victim,
                cause,
                head,
            } => {
                let how = crate::tally::how(*cause);
                let head = if *head { " \u{b7} HEAD" } else { "" };
                // Name, how, name: the parts `feed` draws (a tab ends one).
                let line = if killer == victim {
                    format!("{} died\t{how}{head}", st.name(*victim))
                } else {
                    format!("{}\t{how}{head}\t{}", st.name(*killer), st.name(*victim))
                };
                st.feed.push((line, now));
                let side = |q: &u8| st.teams.get(q).map(|t| t & 1);
                let foe = !s.core.rules.teams || side(killer) != side(victim);
                st.tally.kill(*killer, *victim, foe);
                // A teammate killed by a teammate: the announcer tells both.
                if !foe && killer != victim {
                    if *killer == me {
                        st.say.push("betrayal");
                    } else if *victim == me {
                        st.say.push("betrayed");
                    }
                }
                if *killer == me && *victim != me {
                    st.kills += 1;
                }
                if *victim == me {
                    st.deaths += 1;
                    st.death_line = death_line(st, s.core.rules.teams, me, *killer, *cause);
                }
                if let Some(r) = st.rung.remove(victim) {
                    st.spree_ended.insert(*victim, r);
                }
            }
            &Msg::Medal { slot, medal, victim } => on_medal(st, me, slot, medal, victim, now),
            &Msg::Carnage { slot, .. } => {
                st.carnage.insert(slot, *m);
            }
            Msg::Bye => st.ended = true,
            _ => {}
        }
    }
    st.feed.retain(|(_, t)| now - t < FEED_S);
    while st.feed.len() > FEED_LINES {
        st.feed.remove(0);
    }
    st.medals.retain(|&(_, t)| now - t < MEDAL_S);
}

/// Dead long enough for the scoreboard ([`DEAD_BOARD_S`]), on the
/// respawn's own clock.
fn boarded(f: &sim::fighter::Fighter) -> bool {
    let gone = sim::consts::ticks(sim::consts::RESPAWN_MS).saturating_sub(u32::from(f.respawn));
    !f.alive && gone as f32 >= DEAD_BOARD_S * TICK_HZ as f32
}

/// What the screen says when we die, after Halo 2's.
fn death_line(st: &HudState, teams: bool, me: u8, killer: u8, cause: u16) -> String {
    if killer == me {
        return if cause == sim::events::Cause::Fall as u16 {
            "You fell to your death".into()
        } else {
            "You killed yourself".into()
        };
    }
    let side = |q: u8| st.teams.get(&q).map(|t| t & 1);
    if teams && side(killer) == side(me) {
        return format!("You were betrayed by {}", st.name(killer));
    }
    format!("You were {} by {}", crate::tally::verb(cause), st.name(killer))
}

fn mmss(s: f32) -> String {
    let s = s.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// `draw`'s lines, written into the same Strings every frame.
#[derive(Default)]
pub struct DrawLines {
    center: String,
    net: String,
    respawn: String,
    gone: String,
}

/// Why the match is gone for us, into `out` (cleared): over, or
/// disconnected and why, if the link says (`Link::ended_why`); empty while
/// it's on.
pub(crate) fn gone_line(out: &mut String, ended: bool, link: &dyn crate::link::Link) {
    out.clear();
    if ended {
        out.push_str("Match over");
    } else if link.closed() {
        out.push_str("Disconnected");
        if let Some(why) = link.ended_why() {
            out.push_str(": ");
            out.push_str(&why);
        }
    }
}

/// The lines, the crosshair, the hit marker and the hurt arc, from the
/// session.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn draw(
    link: NonSend<crate::LinkRes>,
    look: Res<crate::Look>,
    time: Res<Time>,
    st: Res<HudState>,
    mut arc: Query<(&mut Node, &mut UiTransform, &mut BackgroundColor), With<HurtArc>>,
    mut texts: Query<(&mut Text, Option<&CenterText>, Option<&NetText>, Option<&RespawnText>)>,
    mut reticle: Query<&mut Visibility, With<Reticle>>,
    mut hitmark: Query<&mut Visibility, (With<HitMark>, Without<Reticle>)>,
    mut strokes: Query<&mut BackgroundColor, (With<HitStroke>, Without<HurtArc>)>,
    mut lines: Local<DrawLines>,
) {
    let s = link.0.session();
    let now = time.elapsed_secs();
    let core = &s.core;
    let (f, _) = core.own();
    let DrawLines {
        center,
        net,
        respawn,
        gone,
    } = &mut *lines;
    gone_line(gone, st.ended, &*link.0);
    let gone = (!gone.is_empty()).then_some(gone.as_str());
    if !s.ready {
        center.clear();
        match gone {
            Some(g) => {
                let _ = write!(center, "{g}\n{}", st.status);
            }
            None if st.status.is_empty() => center.push_str("connecting..."),
            None => center.push_str(&st.status),
        }
        for (mut t, ce, _, _) in texts.iter_mut() {
            if ce.is_some() && t.0 != *center {
                t.0.clone_from(center);
            }
        }
        return;
    }
    // Dead: who did it, then (with the scoreboard up) the count.
    let boarded = boarded(f);
    let back = (f.respawn as f32 / TICK_HZ as f32).ceil() as u32;
    let snap = core.latest();
    let clock = snap.clock as f32 / TICK_HZ as f32;
    center.clear();
    match snap.phase {
        0 => {
            let _ = write!(center, "Starting in {}", clock.ceil() as u32);
        }
        2 => {
            let w = snap.winner;
            let _ = if w == 255 {
                write!(center, "Draw")
            } else if core.rules.teams {
                if w & 1 == core.team & 1 {
                    write!(center, "Your side wins")
                } else {
                    write!(center, "Your side lost")
                }
            } else if w == core.slot {
                write!(center, "You win")
            } else {
                write!(center, "{} wins", st.shown(w))
            };
            match core.verified {
                Some(true) => center.push_str("  -  match verified"),
                Some(false) => center.push_str("  -  verification FAILED"),
                None => {}
            }
            if !st.cup.is_empty() {
                center.push('\n');
                center.push_str(&st.cup);
            }
            if let Some(g) = gone {
                center.push('\n');
                center.push_str(g);
            }
        }
        _ if gone.is_some() => center.push_str(gone.unwrap_or_default()),
        _ if !f.alive && boarded => {
            let _ = write!(center, "Respawn in {back}");
        }
        _ if !f.alive => center.push_str(&st.death_line),
        _ => {}
    }
    net.clear();
    if st.net {
        let n = core.stats;
        let _ = write!(
            net,
            "snapshots {}\nconfirmed {}  corrected {}\ndepth {}  rate {:.2}\ninterp {:.1} ticks  dropped {}",
            n.snapshots, n.confirmations, n.corrections, n.depth, n.rate, n.delay_ticks, s.dropped
        );
    }
    respawn.clear();
    if snap.phase == 1 && gone.is_none() && !f.alive && !boarded {
        let _ = write!(respawn, "Respawn in {back}");
    }
    for (mut t, ce, ne, re) in texts.iter_mut() {
        let v = if re.is_some() {
            &*respawn
        } else if ce.is_some() {
            &*center
        } else if ne.is_some() {
            &*net
        } else {
            continue;
        };
        if t.0 != *v {
            t.0.clone_from(v);
        }
    }
    let arm = sim::arms::arm(f.gun().arm);
    let scoped = look.scoped(arm);
    if let Ok(mut v) = reticle.single_mut() {
        v.set_if_neq(if scoped || !f.alive {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
    let hit_u = (now - st.hit_at) / HIT_S;
    if let Ok(mut v) = hitmark.single_mut() {
        v.set_if_neq(if (0.0..1.0).contains(&hit_u) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let c = if st.hit_kill {
        Color::srgb(1.0, 0.25, 0.2)
    } else {
        Color::WHITE
    };
    for mut bg in strokes.iter_mut() {
        bg.set_if_neq(BackgroundColor(c));
    }
    // Moved only when a hit turns it: a margin is the layout's, and writing
    // one lays the HUD out again.
    if let Ok((mut n, mut tr, mut bg)) = arc.single_mut() {
        let u = (now - st.arc_at) / DAMAGE_S;
        let a = st.hurt_angle;
        let margin = UiRect {
            left: Val::Px(DAMAGE_R * a.sin() - 28.0),
            top: Val::Px(-DAMAGE_R * a.cos() - 3.0),
            ..default()
        };
        if n.margin != margin {
            n.margin = margin;
        }
        let turn = Rot2::radians(a);
        if tr.rotation != turn {
            tr.rotation = turn;
        }
        let alpha = if (0.0..1.0).contains(&u) { 0.85 * (1.0 - u) } else { 0.0 };
        bg.set_if_neq(BackgroundColor(Color::srgba(1.0, 0.2, 0.15, alpha)));
    }
}

/// The feed, top left: the watch line, the flags, then the feed's lines,
/// each faded out over its last [`FEED_FADE_S`]. A kill is the killer,
/// how (small and dim) and the victim, no brackets.
pub fn feed(
    link: NonSend<crate::LinkRes>,
    time: Res<Time>,
    st: Res<HudState>,
    mut parts: Query<(&mut TextSpan, &mut TextColor, &FeedPart)>,
    (mut rows, mut v): (Local<Vec<(String, f32)>>, Local<String>),
) {
    let s = link.0.session();
    if !s.ready {
        return;
    }
    let now = time.elapsed_secs();
    let core = &s.core;
    let snap = core.latest();
    rows.resize_with(FEED_ROWS, Default::default);
    let mut n = 0;
    let mut put = |line: &mut dyn FnMut(&mut String), a: f32| {
        if n < FEED_ROWS {
            rows[n].0.clear();
            line(&mut rows[n].0);
            rows[n].1 = a;
            n += 1;
        }
    };
    if s.watch && core.slot < sim::consts::MAX_FIGHTERS as u8 {
        put(
            &mut |o| {
                let _ = write!(o, "WATCHING {}\tCLICK for the next player", st.shown(core.slot));
            },
            1.0,
        );
    }
    if snap.n_flags > 0 {
        put(&mut |o| flags_line(o, &st, snap), 1.0);
    }
    for (l, at) in &st.feed {
        let a = ((FEED_S - (now - at)) / FEED_FADE_S).clamp(0.0, 1.0);
        put(&mut |o| o.push_str(l), a);
    }
    for r in rows.iter_mut().skip(n) {
        r.0.clear();
    }
    for (mut span, mut ink, p) in &mut parts {
        let (row, part) = (p.0 / FEED_PARTS, p.0 % FEED_PARTS);
        let (line, a) = (&rows[row].0, rows[row].1);
        let pieces = line.split('\t').count();
        v.clear();
        if let Some(piece) = line.split('\t').nth(part) {
            if part > 0 {
                v.push_str("  ");
            }
            v.push_str(piece);
        }
        // The line's last part ends it.
        if !line.is_empty() && part + 1 == pieces.min(FEED_PARTS) {
            v.push('\n');
        }
        if span.0 != *v {
            span.0.clone_from(&v);
        }
        let c = if part == 1 { INK_DIM } else { INK };
        ink.set_if_neq(TextColor(c.with_alpha(c.alpha() * a)));
    }
}

fn set_text(t: &mut Text, v: String) {
    if t.0 != v {
        t.0 = v;
    }
}

fn set_display(n: &mut Node, on: bool) {
    let want = if on { Display::Flex } else { Display::None };
    if n.display != want {
        n.display = want;
    }
}

fn set_width(n: &mut Node, w: Val) {
    if n.width != w {
        n.width = w;
    }
}

/// An image's tint, without telling the layout: a sliced plate's change
/// drops its measure and lays the HUD out again, and its colour is read
/// when it is drawn whatever.
fn tint(img: &mut Mut<ImageNode>, c: Color) {
    if img.color != c {
        img.bypass_change_detection().color = c;
    }
}

/// A round's size by the magazine's: a few big ones, many small.
fn pip_size(mag: u16) -> (f32, f32) {
    match mag {
        0..=8 => (7.0, 18.0),
        9..=18 => (5.0, 14.0),
        _ => (3.0, 10.0),
    }
}

/// What a hand's box shows: its number, the gun's name, the rounds to draw
/// (and how many are left), the heat (a hot gun's, 0 to 1) and its note.
#[derive(Clone, Copy, PartialEq)]
struct GunLook {
    count: u32,
    name: &'static str,
    pips: Option<(u16, u16)>,
    heat: Option<(f32, bool)>,
    note: &'static str,
}

fn gun_look(g: &sim::fighter::Gun, reloading: bool) -> GunLook {
    let arm = sim::arms::arm(g.arm);
    let venting = reloading && arm.hot();
    // A battery (a hot gun, the sword) is its charge, as a share.
    let battery = arm.hot() || arm.mag as usize > PIPS;
    let count = if battery {
        (g.mag as u32 * 100).div_ceil(arm.mag.max(1) as u32)
    } else {
        g.reserve as u32
    };
    GunLook {
        count,
        name: arm.name,
        pips: (!battery).then_some((arm.mag, g.mag)),
        heat: arm
            .hot()
            .then(|| ((g.heat as f32 / sim::arms::HEAT_FULL as f32).clamp(0.0, 1.0), venting)),
        note: if venting {
            "VENTING"
        } else if reloading {
            "RELOADING"
        } else if !battery && g.mag == 0 && g.reserve == 0 {
            "NO AMMO"
        } else {
            ""
        },
    }
}

/// Each hand's gun as its picture, once it is drawn; its name until then.
#[allow(clippy::type_complexity)]
pub fn gun_icons(
    link: NonSend<crate::LinkRes>,
    icons: Option<Res<crate::look::icons::GunIcons>>,
    mut pics: Query<(&GunIcon, &mut Node, &mut ImageNode), Without<GunName>>,
    mut names: Query<(&GunName, &mut Node), Without<GunIcon>>,
) {
    let s = link.0.session();
    let (true, Some(icons)) = (s.ready, icons) else {
        return;
    };
    let f = s.core.own().0;
    let arm = |h: u8| if h == 0 { f.gun().arm } else { f.left.arm };
    let mut shown = [false; 2];
    for (h, mut n, mut img) in &mut pics {
        let pic = icons.get(arm(h.0));
        shown[h.0 as usize & 1] = pic.is_some();
        if let Some(pic) = pic {
            if img.image != *pic {
                img.image = pic.clone();
            }
        }
        set_display(&mut n, pic.is_some());
    }
    for (h, mut n) in &mut names {
        set_display(&mut n, !shown[h.0 as usize & 1]);
    }
}

/// What `gear`'s lines say: written again only when it changes.
#[derive(Clone, Copy, PartialEq)]
pub struct GearText {
    gren: [u8; 2],
    hands: [Option<(u32, &'static str, &'static str)>; 2],
    back: &'static str,
    /// The keys to press and what for, over the gear: getting out of a
    /// ride, and Capture the Flag's take and drop.
    prompt: [Option<(&'static str, &'static str)>; 2],
}

/// The grenades, the guns in hand and on our back, and a ride's way out.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn gear(
    link: NonSend<crate::LinkRes>,
    binds: Res<crate::binds::Binds>,
    mut boxes: Query<(&GrenBox, &mut ImageNode)>,
    mut texts: Query<(
        &mut Text,
        Option<&GrenCount>,
        Option<&GunCount>,
        Option<&GunName>,
        Option<&GunNote>,
        Option<&BackGun>,
        Option<&RideText>,
    )>,
    mut panels: Query<(&GunPanel, &mut Node), (Without<Pip>, Without<HeatBox>, Without<HeatFill>)>,
    mut pips: Query<(&Pip, &mut Node, &mut BackgroundColor), (Without<GrenBox>, Without<HeatBox>, Without<HeatFill>)>,
    mut heat_box: Query<(&HeatBox, &mut Node), (Without<HeatFill>, Without<GunPanel>)>,
    mut back_plate: Query<
        &mut Node,
        (
            With<BackPlate>,
            Without<HeatBox>,
            Without<HeatFill>,
            Without<GunPanel>,
            Without<Pip>,
        ),
    >,
    mut heat: Query<(&HeatFill, &mut Node, &mut ImageNode), (Without<GrenBox>, Without<Pip>, Without<GunPanel>)>,
    mut said: Local<Option<GearText>>,
) {
    let s = link.0.session();
    if !s.ready {
        return;
    }
    let f = s.core.own().0;
    let reloading = f.reload > 0;
    let hands = [
        Some(gun_look(f.gun(), reloading)),
        f.dual().then(|| gun_look(&f.left, f.left_reload > 0)),
    ];
    for (b, mut img) in boxes.iter_mut() {
        tint(&mut img, if f.gren_kind == b.0 { PLATE } else { PLATE_DIM });
    }
    for (p, mut n) in panels.iter_mut() {
        set_display(&mut n, hands[p.0 as usize & 1].is_some());
    }
    let back = f.guns[1 - f.cur as usize];
    let carried = back.arm != 0 && !f.dual();
    for mut n in back_plate.iter_mut() {
        set_display(&mut n, carried);
    }
    let out = binds.key(crate::binds::Action::Use);
    let swap = binds.key(crate::binds::Action::Swap);
    // The enemy's flag at our feet, on the floor or at its base.
    let at = s.core.own().1;
    let flag_here = !f.flag
        && f.alive
        && f.riding().is_none()
        && s.core
            .latest()
            .flags()
            .get((s.core.team as usize & 1) ^ 1)
            .is_some_and(|fl| {
                let d = fl.pos - at;
                fl.state != sim::world::FLAG_HELD
                    && d.flat().len() < sim::consts::FLAG_REACH
                    && d.y.abs() < sim::consts::FLAG_RISE
            });
    let text = GearText {
        gren: [f.grenades, f.stickies],
        hands: hands.map(|h| h.map(|g| (g.count, g.name, g.note))),
        back: if carried { sim::arms::arm(back.arm).name } else { "" },
        prompt: [
            if f.riding().is_some() {
                Some((out, "get out"))
            } else if flag_here {
                Some((out, "take the flag"))
            } else {
                None
            },
            f.flag.then_some((swap, "drop the flag")),
        ],
    };
    let texts_due = *said != Some(text);
    *said = Some(text);
    for (mut t, gc, count, name, note, bk, ride) in texts.iter_mut().filter(|_| texts_due) {
        let hand = |h: u8| hands[h as usize & 1].as_ref();
        let v = if let Some(g) = gc {
            let n = if g.0 == 0 { f.grenades } else { f.stickies };
            n.to_string()
        } else if let Some(c) = count {
            hand(c.0).map_or_else(String::new, |g| g.count.to_string())
        } else if let Some(n) = name {
            hand(n.0).map_or("", |g| g.name).to_string()
        } else if let Some(n) = note {
            hand(n.0).map_or("", |g| g.note).to_string()
        } else if bk.is_some() {
            if carried {
                sim::arms::arm(back.arm).name.to_string()
            } else {
                String::new()
            }
        } else if ride.is_some() {
            let mut line = String::new();
            for (key, what) in text.prompt.iter().flatten() {
                if !line.is_empty() {
                    line.push_str("    ");
                }
                let _ = write!(line, "{key}  {what}");
            }
            line
        } else {
            continue;
        };
        set_text(&mut t, v);
    }
    for (p, mut n, mut bg) in pips.iter_mut() {
        let Some((size, left)) = hands[p.0 as usize & 1].as_ref().and_then(|g| g.pips) else {
            set_display(&mut n, false);
            continue;
        };
        set_display(&mut n, p.1 < size as usize);
        let (w, h) = pip_size(size);
        if n.width != Val::Px(w) {
            n.width = Val::Px(w);
            n.height = Val::Px(h);
            n.border_radius = BorderRadius {
                top_left: Val::Px(w / 2.0),
                top_right: Val::Px(w / 2.0),
                ..default()
            };
        }
        // Spent from the far end: the rounds left sit by the box's side.
        let full = if p.0 == 0 {
            p.1 >= size.saturating_sub(left) as usize
        } else {
            p.1 < left as usize
        };
        let c = if full { PIP } else { PIP_SPENT };
        if bg.0 != c {
            bg.0 = c;
        }
    }
    for (h, mut n) in heat_box.iter_mut() {
        set_display(
            &mut n,
            hands[h.0 as usize & 1].as_ref().is_some_and(|g| g.heat.is_some()),
        );
    }
    for (h, mut n, mut img) in heat.iter_mut() {
        let Some((u, venting)) = hands[h.0 as usize & 1].as_ref().and_then(|g| g.heat) else {
            continue;
        };
        set_width(&mut n, Val::Percent(u * 100.0));
        tint(&mut img, if venting { SHIELD_LOW } else { HEAT });
    }
}

/// The shield and health: the shield's frame pulses red while it is low or
/// gone; an overshield's green over the shield's bar, and camouflage's
/// seconds over the box.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn vitals(
    link: NonSend<crate::LinkRes>,
    time: Res<Time>,
    mut shield: Query<(&mut Node, &mut ImageNode), (With<ShieldBar>, Without<ShieldFrame>)>,
    mut frame: Query<&mut ImageNode, With<ShieldFrame>>,
    mut health: Query<&mut Node, (With<HealthBar>, Without<ShieldBar>)>,
    mut line: Query<&mut Visibility, (With<HealthLine>, Without<CamoText>)>,
    mut over: Query<&mut Node, (With<OverBar>, Without<ShieldBar>, Without<HealthBar>)>,
    mut camo: Query<(&mut Text, &mut Visibility), With<CamoText>>,
    mut camo_secs: Local<u32>,
) {
    let s = link.0.session();
    if !s.ready {
        return;
    }
    let f = s.core.own().0;
    if let Ok(mut n) = over.single_mut() {
        let k = (f.shield - SHIELD_MAX) as f32 / (sim::consts::OVERSHIELD_MAX - SHIELD_MAX) as f32;
        let k = if f.alive { k.clamp(0.0, 1.0) } else { 0.0 };
        set_width(&mut n, Val::Px(k * (SHIELD_W - 2.0 * SHIELD_PAD)));
    }
    if let Ok((mut t, mut v)) = camo.single_mut() {
        let on = f.alive && f.camo > 0;
        v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
        let secs = (f.camo as u32).div_ceil(sim::consts::TICK_HZ);
        if on && *camo_secs != secs {
            *camo_secs = secs;
            t.0.clear();
            let _ = write!(t.0, "CAMO {secs}");
        }
    }
    let sh = (f.shield as f32 / SHIELD_MAX as f32 * 100.0).clamp(0.0, 100.0);
    let low = f.alive && sh < SHIELD_LOW_PCT;
    if let Ok((mut n, mut img)) = shield.single_mut() {
        set_width(&mut n, Val::Percent(sh));
        tint(&mut img, if low { SHIELD_LOW } else { SHIELD });
    }
    if let Ok(mut b) = frame.single_mut() {
        let c = if low {
            let u = 0.5 + 0.5 * (time.elapsed_secs() * 9.0).sin();
            PLATE.mix(&SHIELD_LOW, u)
        } else {
            PLATE
        };
        tint(&mut b, c);
    }
    if let Ok(mut v) = line.single_mut() {
        let want = if f.alive && f.health < HEALTH_MAX {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *v != want {
            *v = want;
        }
    }
    if let Ok(mut n) = health.single_mut() {
        set_width(
            &mut n,
            Val::Percent((f.health as f32 / HEALTH_MAX as f32 * 100.0).clamp(0.0, 100.0)),
        );
    }
}

/// What `corner`'s lines say: written again only when it changes.
#[derive(Clone, Copy, PartialEq)]
pub struct CornerText {
    clock: Option<u32>,
    map: u8,
    rung: Option<(i32, i32)>,
    rows: [Option<i32>; 2],
}

/// Bottom right: the clock while the match runs, the map (and in gun game
/// the rung and the gun next), and two score rows. On sides: ours and
/// theirs, the leader on top. Alone: the leader on top, and us under it
/// (or whoever's second, when the leader is us).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn corner(
    link: NonSend<crate::LinkRes>,
    st: Res<HudState>,
    mut texts: Query<(&mut Text, Option<&ClockText>, Option<&ModeText>, Option<&ScoreNum>)>,
    mut rows: Query<(&ScoreRow, &mut Node), Without<ScoreFill>>,
    mut swatches: Query<(&ScoreSwatch, &mut ImageNode), Without<ScoreFill>>,
    mut fills: Query<(&ScoreFill, &mut Node, &mut ImageNode), (Without<ScoreRow>, Without<ScoreSwatch>)>,
    mut mine: Query<(&ScoreMine, &mut Visibility)>,
    mut said: Local<Option<CornerText>>,
) {
    let s = link.0.session();
    if !s.ready {
        return;
    }
    let core = &s.core;
    let snap = core.latest();
    let limit = core.rules.score_limit.max(1);
    let scores = &snap.score[..snap.n_score as usize];
    let score = |i: u8| scores.get(i as usize).copied().unwrap_or(0);
    let clock = snap.clock as f32 / TICK_HZ as f32;
    // (score, colour, ours) a row: at most two.
    let mut rows_at = [(0, Color::NONE, false); 2];
    let mut n_shown = 0;
    let mut show = |r: (i32, Color, bool)| {
        rows_at[n_shown] = r;
        n_shown += 1;
    };
    if core.rules.teams {
        let us = core.team & 1;
        let side = |t: u8| if t == 0 { RED_SIDE } else { BLUE_SIDE };
        show((score(us), side(us), true));
        show((score(1 - us), side(1 - us), false));
    } else {
        // The leader, and us under it, or whoever's second when the leader
        // is us (or we have no score yet): ranked as the board ranks.
        let key = |i: u8| (-score(i), i != core.slot, i);
        let seats = || (0..scores.len() as u8).filter(|&i| i == core.slot || st.names.contains_key(&i));
        if let Some(top) = seats().min_by_key(|&i| key(i)) {
            let we = seats().any(|i| i == core.slot);
            let next = if top == core.slot || !we {
                seats().filter(|&i| i != top).min_by_key(|&i| key(i))
            } else {
                Some(core.slot)
            };
            for i in [Some(top), next].into_iter().flatten() {
                let me = i == core.slot;
                show((score(i), if me { SHIELD } else { OTHER }, me));
            }
        }
    }
    // The leader on top.
    let shown = &mut rows_at[..n_shown];
    shown.sort_by_key(|&(v, _, me)| (-v, !me));
    let shown = &*shown;
    let text = CornerText {
        clock: (snap.phase == 1).then(|| clock.max(0.0) as u32),
        map: s.map_id,
        rung: core.rules.gun_game.then(|| (score(core.slot), limit)),
        rows: [0, 1].map(|k| shown.get(k).map(|r| r.0)),
    };
    if *said != Some(text) {
        *said = Some(text);
        let map = sim::maps::ALL.iter().find(|d| d.id == s.map_id).map_or("", |d| d.name);
        let mut mode = map.to_string();
        if core.rules.gun_game {
            let mine = score(core.slot);
            let next = if mine < limit - 1 {
                format!("next {}", sim::arms::arm(sim::world::Rules::rung(mine + 1)).name)
            } else {
                "last gun".into()
            };
            mode = format!("Rung {} of {limit}  -  {next}\n{mode}", mine.min(limit - 1) + 1);
        }
        for (mut t, ck, md, num) in texts.iter_mut() {
            let v = if ck.is_some() {
                if snap.phase == 1 && snap.clock == 0 && snap.n_flags > 0 {
                    "SUDDEN DEATH".to_string()
                } else if snap.phase == 1 {
                    mmss(clock)
                } else {
                    String::new()
                }
            } else if md.is_some() {
                mode.clone()
            } else if let Some(n) = num {
                shown.get(n.0 as usize).map_or_else(String::new, |r| r.0.to_string())
            } else {
                continue;
            };
            set_text(&mut t, v);
        }
    }
    for (r, mut n) in rows.iter_mut() {
        set_display(&mut n, (r.0 as usize) < shown.len());
    }
    for (r, mut img) in swatches.iter_mut() {
        if let Some(&(_, c, _)) = shown.get(r.0 as usize) {
            tint(&mut img, c);
        }
    }
    for (r, mut n, mut img) in fills.iter_mut() {
        if let Some(&(v, c, _)) = shown.get(r.0 as usize) {
            set_width(
                &mut n,
                Val::Percent((v as f32 / limit as f32 * 100.0).clamp(0.0, 100.0)),
            );
            tint(&mut img, c);
        }
    }
    for (r, mut v) in mine.iter_mut() {
        let want = if shown.get(r.0 as usize).is_some_and(|x| x.2) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *v != want {
            *v = want;
        }
    }
}

/// Whether our aim is on an enemy the gun in hand reaches, as drawn: the
/// red reticle. From our eye along our aim, against the others' bodies
/// where they are drawn, the map in between; a gun's reach is its
/// `aim_range`, its magnetism widening the bodies with distance (as
/// `World::magnet` bends a near miss); the sword's, its lunge's.
pub fn on_target(core: &client_net::core::NetCore, others: &[client_net::core::Drawn]) -> bool {
    use sim::consts::{HEAD_R, HEAD_Y, HEAD_Y_CROUCH, HIT_R, HIT_TOP, HIT_TOP_CROUCH, LUNGE_PAD, LUNGE_RANGE};
    let f = core.own().0;
    if !f.alive || f.riding().is_some() {
        return false;
    }
    let a = sim::arms::arm(f.gun().arm);
    let sword = a.kind == sim::arms::Kind::Sword;
    let reach = if sword { LUNGE_RANGE } else { a.aim_range };
    let eye = f.eye();
    let dir = sim::geom::aim_dir(f.yaw, f.pitch);
    let wall = core.map.ray(eye, dir, reach);
    let mate = core.rules.teams.then_some(core.team & 1);
    others.iter().filter(|d| d.alive && Some(d.team & 1) != mate).any(|d| {
        let pad = if sword {
            LUNGE_PAD
        } else {
            (d.pos - eye).len() * a.magnet
        };
        let (hy, top) = if d.crouched {
            (HEAD_Y_CROUCH, HIT_TOP_CROUCH)
        } else {
            (HEAD_Y, HIT_TOP)
        };
        sim::geom::ray_cylinder(eye, dir, d.pos, HIT_R + pad, top, wall).is_some()
            || sim::geom::ray_sphere(eye, dir, d.pos + sim::geom::V3::new(0.0, hy, 0.0), HEAD_R + pad, wall).is_some()
    })
}

/// The crosshair red while it is on an enemy in reach ([`on_target`]).
pub fn reticle_tint(seen: Res<crate::Seen>, mut q: Query<&mut BackgroundColor, With<ReticleStroke>>) {
    let red = seen.on_target;
    let c = if red {
        Color::srgba(1.0, 0.18, 0.12, 0.95)
    } else {
        Color::srgba(1.0, 1.0, 1.0, 0.85)
    };
    for mut bg in q.iter_mut() {
        if bg.0 != c {
            bg.0 = c;
        }
    }
}

/// The scoreboard: held up (its key), while we are dead (after
/// [`DEAD_BOARD_S`]), and once the match is over until the carnage report
/// comes.
/// Every seat by side (teams first, each side's score) then by score:
/// name, kills, deaths, score.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn board(
    link: NonSend<crate::LinkRes>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<crate::binds::Binds>,
    mut st: ResMut<HudState>,
    mut panel: Query<&mut Visibility, With<Board>>,
    mut cols: Query<(&mut Text, &BoardColumn)>,
    (mut out, mut seats): (Local<[String; 4]>, Local<Vec<u8>>),
) {
    let s = link.0.session();
    let over = s.ready && s.core.latest().phase == 2;
    let held = binds.pressed(crate::binds::Action::Scores, &keys, &mouse);
    if st.board != held {
        st.board = held;
    }
    // Once the match is over the carnage report (`medals.rs`) is the
    // board, as soon as its rows come.
    // Dead a moment, it comes up on its own until the respawn (Halo 2's).
    let dead = s.ready && !over && boarded(s.core.own().0);
    let up = s.ready && (st.board || dead || (over && st.carnage.is_empty()));
    if let Ok(mut v) = panel.single_mut() {
        let want = if up { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    if !up {
        return;
    }
    let core = &s.core;
    let snap = core.latest();
    let score = |i: usize| snap.score.get(i).copied().unwrap_or(0);
    let side = |q: u8| st.teams.get(&q).copied().unwrap_or(0) & 1;
    let kd = |q: u8| st.tally.kd.get(&q).copied().unwrap_or((0, 0));
    // Each column written row by row into the same Strings every frame.
    let heads = ["", "K", "D", "SCORE"];
    for (o, h) in out.iter_mut().zip(heads) {
        o.clear();
        o.push_str(h);
        o.push('\n');
    }
    let [name, kills, deaths, total] = &mut *out;
    seats.clear();
    if core.rules.teams {
        for t in [core.team & 1, 1 - (core.team & 1)] {
            name.push_str(if t == 0 { "RED\n" } else { "BLUE\n" });
            kills.push('\n');
            deaths.push('\n');
            let _ = writeln!(total, "{}", score(t as usize));
            seats.clear();
            seats.extend(st.names.keys().copied().filter(|&q| side(q) == t));
            seats.sort_by_key(|&q| (std::cmp::Reverse(kd(q).0), kd(q).1, q));
            for &q in seats.iter() {
                let (k, d) = kd(q);
                let _ = writeln!(name, "  {}", st.shown(q));
                let _ = writeln!(kills, "{k}");
                let _ = writeln!(deaths, "{d}");
                total.push('\n');
            }
        }
    } else {
        seats.extend(st.names.keys().copied());
        seats.sort_by_key(|&q| (std::cmp::Reverse(score(q as usize)), q));
        for &q in seats.iter() {
            let (k, d) = kd(q);
            let me = if q == core.slot { " <" } else { "" };
            let _ = writeln!(name, "{}{me}", st.shown(q));
            let _ = writeln!(kills, "{k}");
            let _ = writeln!(deaths, "{d}");
            let _ = writeln!(total, "{}", score(q as usize));
        }
    }
    for (mut t, c) in cols.iter_mut() {
        let v = &out[c.0 as usize];
        if t.0 != *v {
            t.0.clone_from(v);
        }
    }
}

/// The prompt under the crosshair: the USE key on its cap and what it
/// would take, the verb dim and the gun lit. Hidden with nothing to take.
#[allow(clippy::type_complexity)]
pub fn prompt(
    link: NonSend<crate::LinkRes>,
    binds: Res<crate::binds::Binds>,
    seen: Res<crate::Seen>,
    mut row: Query<&mut Visibility, With<PromptRow>>,
    mut q: Query<
        (&mut Text, &mut Node, Option<&PromptKey>, Option<&PromptVerb>),
        Or<(With<PromptKey>, With<PromptVerb>, With<PromptText>)>,
    >,
    mut said: Local<Option<(&'static str, crate::items::Offer)>>,
) {
    let s = link.0.session();
    let key = binds.key(crate::binds::Action::Use);
    let offer = s
        .ready
        .then(|| crate::items::offer(&s.core, &seen.vehicles))
        .flatten()
        .map(|what| (key, what));
    // Written when it changes, not a fresh line a frame.
    if *said == offer {
        return;
    }
    *said = offer;
    if let Ok(mut v) = row.single_mut() {
        v.set_if_neq(if offer.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let Some((key, what)) = offer else {
        return;
    };
    for (mut t, mut n, k, verb) in q.iter_mut() {
        let v = if k.is_some() {
            key.to_string()
        } else if verb.is_some() {
            what.verb.to_uppercase()
        } else {
            what.gun.to_string()
        };
        // A ride has no gun: no gap after its verb.
        set_display(&mut n, !v.is_empty());
        set_text(&mut t, v);
    }
}

fn rim(alpha: f32) -> Color {
    Color::srgba(0.55, 0.02, 0.02, alpha)
}

/// The rim: raised by a hit on us (`HudState::hurt_at`), red on health
/// and a faint gold on the shield, fading out; a flicker of blue-white as
/// the shield breaks.
pub fn hurt_rim(
    time: Res<Time>,
    st: Res<HudState>,
    mut q: Query<(&mut BackgroundGradient, &mut Visibility), With<HurtRim>>,
) {
    let now = time.elapsed_secs();
    let Ok((mut bg, mut vis)) = q.single_mut() else {
        return;
    };
    let u = (now - st.hurt_at) / RIM_S;
    let hurt = if st.hurt_at > 0.0 && (0.0..1.0).contains(&u) {
        let (peak, c) = if st.hurt_shield {
            (RIM_SHIELD_ALPHA, Vec3::new(1.0, 0.82, 0.35))
        } else {
            (RIM_ALPHA, Vec3::new(0.55, 0.02, 0.02))
        };
        (peak * (1.0 - u), c)
    } else {
        (0.0, Vec3::ZERO)
    };
    let p = (now - st.pop_at) / POP_S;
    let pop = if st.pop_at > 0.0 && (0.0..1.0).contains(&p) {
        let flicker = 0.7 + 0.3 * (now * POP_FLICKER_HZ * core::f32::consts::TAU).sin();
        POP_ALPHA * (1.0 - p) * (1.0 - p) * flicker
    } else {
        0.0
    };
    let (alpha, c, clear) = if pop > hurt.0 {
        (pop, Vec3::new(0.6, 0.85, 1.0), POP_CLEAR_PCT)
    } else {
        (hurt.0, hurt.1, RIM_CLEAR_PCT)
    };
    let want = if alpha > 0.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *vis != want {
        *vis = want;
    }
    if alpha > 0.0 {
        if let Some(Gradient::Radial(r)) = bg.0.first_mut() {
            if let Some(stop) = r.stops.first_mut() {
                stop.point = Val::Percent(clear);
            }
            if let Some(stop) = r.stops.last_mut() {
                stop.color = Color::srgba(c.x, c.y, c.z, alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_feed_tells_of_sprees_and_their_end() {
        let mut st = HudState::default();
        st.names.insert(1, "ana".into());
        st.names.insert(2, "bo".into());
        on_medal(&mut st, 0, 1, md::KILLING_SPREE, 3, 0.0);
        assert_eq!(st.feed.last().unwrap().0, "ana is on a Killing Spree");
        // Ana dies to Bo: the kill puts her rung aside, then Bo's Killjoy.
        if let Some(r) = st.rung.remove(&1) {
            st.spree_ended.insert(1, r);
        }
        on_medal(&mut st, 0, 2, md::KILLJOY, 1, 0.0);
        assert_eq!(st.feed.last().unwrap().0, "bo ended ana's Killing Spree");
        // Ours: on screen, counted, and said; a headshot only drawn.
        let lines = st.feed.len();
        on_medal(&mut st, 0, 0, md::DOUBLE_KILL, 1, 1.0);
        on_medal(&mut st, 0, 0, md::HEADSHOT, 1, 1.0);
        assert_eq!(st.medals, vec![(md::DOUBLE_KILL, 1.0), (md::HEADSHOT, 1.0)]);
        assert_eq!(st.announce, vec![md::DOUBLE_KILL]);
        assert_eq!(st.feed.len(), lines, "our own rungs aren't fed");
        on_medal(&mut st, 0, 0, md::ASSIST, 1, 1.0);
        assert_eq!((st.assists, st.earned[&md::ASSIST]), (1, 1));
    }

    #[test]
    fn the_menu_reads_the_queue_and_the_cup() {
        let q = Msg::Queued {
            mode: proto::MODE_DOUBLES,
            waiting_s: 75,
        };
        assert_eq!(status_line(&q).unwrap(), "Finding a 2v2... 1:15");
        let cup = |round, place, done| Msg::Cup {
            round,
            rounds: 3,
            entrants: 16,
            place,
            points: 6,
            done,
        };
        assert_eq!(
            status_line(&cup(2, 1, false)).unwrap(),
            "Cup, round 2 of 3: 1st of 16 on 6 points"
        );
        assert_eq!(
            status_line(&cup(4, 12, false)).unwrap(),
            "Cup, the final: 12th of 16 on 6 points"
        );
        assert_eq!(status_line(&cup(4, 3, true)).unwrap(), "Cup over: 3rd of 16");
        assert!(status_line(&Msg::Bye).is_none());
    }
}

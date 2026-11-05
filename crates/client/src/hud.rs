//! The HUD, laid out as the arena shooters' is, in our own blue: the
//! grenades top left and the kill feed under them; the gun top right (its
//! spare rounds and name in a box, the one on our back beside it, the
//! magazine's rounds or the battery's heat under it); the shield bottom
//! left, health a thin line beneath; the clock, the map and the two scores
//! that matter bottom right. A crosshair, a hit marker on our hits (red on
//! a kill), an arc toward whoever hurt us, and a line in the middle for the
//! countdown, the result, the kill cam and the respawn; before the welcome,
//! the queue's line. `N` shows the net graph. The scope is `scope.rs`'s.
//! Medals and the carnage report are `medals.rs`'s to draw; their lines in
//! the feed are here. Everything here is read from the session; nothing is
//! decided.

use std::collections::BTreeMap;

use bevy::prelude::*;
use proto::medal as md;
use proto::{EventState, Msg};
use sim::consts::{HEALTH_MAX, SHIELD_MAX, TICK_HZ};
use sim::events::Kind;

use client_net::session::{refuse_why, Session};

use crate::plate;

const FEED_S: f32 = 5.0;
const FEED_LINES: usize = 5;
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
const HEALTH: Color = Color::srgba(0.95, 0.36, 0.3, 0.9);
const HEAT: Color = Color::srgba(1.0, 0.62, 0.22, 0.95);
const RED_SIDE: Color = Color::srgba(0.92, 0.24, 0.2, 0.9);
const BLUE_SIDE: Color = Color::srgba(0.24, 0.46, 0.98, 0.9);
const OTHER: Color = Color::srgba(0.62, 0.66, 0.72, 0.85);
/// Off the screen's edges, px.
const EDGE_PX: f32 = 22.0;
/// The shield's width, and the score bars'.
const SHIELD_W: f32 = 290.0;
const SCORE_W: f32 = 150.0;
/// Under this share the shield is low: its frame pulses red.
const SHIELD_LOW_PCT: f32 = 25.0;
/// Rounds a magazine may show; a bigger one (a battery) is a number.
const PIPS: usize = 48;
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
    arc_at: f32,
    hurt_angle: f32,
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
}

/// Every HUD node hangs off this: hidden off a match (`front::hud_shown`).
#[derive(Component)]
pub struct HudRoot;

#[derive(Component)]
pub struct ShieldBar;
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
#[derive(Component)]
pub struct CenterText;
#[derive(Component)]
pub struct NetText;
/// The scoreboard's panel, and its columns: names, kills, deaths, score.
#[derive(Component)]
pub struct Board;
#[derive(Component)]
pub struct BoardColumn(pub u8);
/// What USE would take here (`items::offer`).
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
            c.spawn((FeedText, tinted(crate::ui::semibold(17.0), INK)));
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
    // Bottom left: the shield in its box, cut into tenths; health a thin
    // line under it.
    commands
        .spawn((
            ChildOf(root),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..abs(Some(EDGE_PX), None, None, Some(20.0))
            },
        ))
        .with_children(|c| {
            c.spawn(Node {
                width: Val::Px(SHIELD_W),
                height: Val::Px(30.0),
                padding: UiRect::all(Val::Px(SHIELD_PAD)),
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
                    height: Val::Px(5.0),
                    margin: UiRect::left(Val::Px(2.0)),
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
        children![(CenterText, text(crate::ui::display(32.0)))],
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
                padding: UiRect::all(Val::Px(16.0)),
                column_gap: Val::Px(24.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.78)),
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
                        text(crate::ui::semibold(18.0)),
                        TextLayout::new_with_justify(if i == 0 { Justify::Left } else { Justify::Right }),
                    )],
                ));
            }
        });
    // Under the crosshair: what USE would take.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            top: Val::Percent(60.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(PromptText, text(crate::ui::semibold(20.0)))],
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

/// This frame's events and stream messages: the markers, names, the feed.
pub fn on_events(st: &mut HudState, s: &Session, now_ms: f64, events: &[EventState], msgs: &[Msg], now: f32, yaw: f32) {
    let me = s.core.slot;
    // A new match (a cup's next heat): its medals and report start over.
    if s.welcomes != st.welcomes {
        st.welcomes = s.welcomes;
        st.medals.clear();
        st.announce.clear();
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
                let from = s.core.others(now_ms).into_iter().find(|d| d.slot == e.a).map(|d| d.pos);
                if let Some(from) = from.or_else(|| st.shot_from.get(&e.a).copied()) {
                    st.hurt_angle = (from.z - p.z).atan2(from.x - p.x) - yaw;
                    st.arc_at = now;
                }
                st.hurt_at = now;
            }
        } else if e.kind == Kind::Kill as u8 && e.a == me && e.b != me {
            st.hit_at = now;
            st.hit_kill = true;
        }
    }
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
                team,
                ..
            } => {
                st.names.insert(*slot, name.as_str().to_string());
                st.looks.insert(
                    *slot,
                    proto::Look {
                        skin: *skin,
                        emblem: *emblem,
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
                let how = if *head { format!("{how}, HEAD") } else { how.to_string() };
                let line = if killer == victim {
                    format!("{} died  [{how}]", st.name(*victim))
                } else {
                    format!("{}  [{how}]  {}", st.name(*killer), st.name(*victim))
                };
                st.feed.push((line, now));
                let side = |q: &u8| st.teams.get(q).map(|t| t & 1);
                let foe = !s.core.rules.teams || side(killer) != side(victim);
                st.tally.kill(*killer, *victim, foe);
                if *killer == me && *victim != me {
                    st.kills += 1;
                }
                if *victim == me {
                    st.deaths += 1;
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

fn mmss(s: f32) -> String {
    let s = s.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
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
    mut texts: Query<(&mut Text, Option<&FeedText>, Option<&CenterText>, Option<&NetText>)>,
    mut reticle: Query<&mut Visibility, With<Reticle>>,
    mut hitmark: Query<&mut Visibility, (With<HitMark>, Without<Reticle>)>,
    mut strokes: Query<&mut BackgroundColor, (With<HitStroke>, Without<HurtArc>)>,
) {
    let s = link.0.session();
    let now = time.elapsed_secs();
    let core = &s.core;
    let (f, _) = core.own();
    let gone = if st.ended {
        Some("Match over")
    } else if link.0.closed() {
        Some("Disconnected")
    } else {
        None
    };
    if !s.ready {
        let line = gone.map_or_else(
            || {
                if st.status.is_empty() {
                    "connecting...".to_string()
                } else {
                    st.status.clone()
                }
            },
            |g| format!("{g}\n{}", st.status),
        );
        for (mut t, _, ce, _) in texts.iter_mut() {
            if ce.is_some() && t.0 != line {
                t.0 = line.clone();
            }
        }
        return;
    }
    let killcam = if f.alive { None } else { core.killcam(link.0.now_ms()) };
    let snap = core.latest();
    let clock = snap.clock as f32 / TICK_HZ as f32;
    let center = match snap.phase {
        0 => format!("Starting in {}", clock.ceil() as u32),
        2 => {
            let w = snap.winner;
            let mut c = if w == 255 {
                "Draw".to_string()
            } else if core.rules.teams {
                if w & 1 == core.team & 1 {
                    "Your side wins".into()
                } else {
                    "Your side lost".into()
                }
            } else if w == core.slot {
                "You win".into()
            } else {
                format!("{} wins", st.name(w))
            };
            match core.verified {
                Some(true) => c.push_str("  -  match verified"),
                Some(false) => c.push_str("  -  verification FAILED"),
                None => {}
            }
            if !st.cup.is_empty() {
                c = format!("{c}\n{}", st.cup);
            }
            if let Some(g) = gone {
                c = format!("{c}\n{g}");
            }
            c
        }
        _ if gone.is_some() => gone.unwrap_or_default().to_string(),
        _ if !f.alive => {
            let back = (f.respawn as f32 / TICK_HZ as f32).ceil() as u32;
            match &killcam {
                Some(k) => format!("Killed by {}  -  respawning in {back}", st.name(k.killer)),
                None => format!("Respawning in {back}"),
            }
        }
        _ => String::new(),
    };
    let feed: String = st.feed.iter().map(|(l, _)| format!("{l}\n")).collect();
    let net = if st.net {
        let n = core.stats;
        format!(
            "snapshots {}\nconfirmed {}  corrected {}\ndepth {}  rate {:.2}\ninterp {:.1} ticks  dropped {}",
            n.snapshots, n.confirmations, n.corrections, n.depth, n.rate, n.delay_ticks, s.dropped
        )
    } else {
        String::new()
    };
    for (mut t, fe, ce, ne) in texts.iter_mut() {
        let v = if fe.is_some() {
            &feed
        } else if ce.is_some() {
            &center
        } else if ne.is_some() {
            &net
        } else {
            continue;
        };
        if t.0 != *v {
            t.0.clone_from(v);
        }
    }
    let arm = sim::arms::arm(f.gun().arm);
    let scoped = look.zoom && arm.zoom > 1;
    if let Ok(mut v) = reticle.single_mut() {
        *v = if scoped || !f.alive {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    let hit_u = (now - st.hit_at) / HIT_S;
    if let Ok(mut v) = hitmark.single_mut() {
        *v = if (0.0..1.0).contains(&hit_u) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    let c = if st.hit_kill {
        Color::srgb(1.0, 0.25, 0.2)
    } else {
        Color::WHITE
    };
    for mut bg in strokes.iter_mut() {
        bg.0 = c;
    }
    if let Ok((mut n, mut tr, mut bg)) = arc.single_mut() {
        let u = (now - st.arc_at) / DAMAGE_S;
        let a = st.hurt_angle;
        n.margin = UiRect {
            left: Val::Px(DAMAGE_R * a.sin() - 28.0),
            top: Val::Px(-DAMAGE_R * a.cos() - 3.0),
            ..default()
        };
        tr.rotation = Rot2::radians(a);
        let alpha = if (0.0..1.0).contains(&u) { 0.85 * (1.0 - u) } else { 0.0 };
        bg.0 = Color::srgba(1.0, 0.2, 0.15, alpha);
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
struct GunLook {
    count: String,
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
        format!("{}", (g.mag as u32 * 100).div_ceil(arm.mag.max(1) as u32))
    } else {
        g.reserve.to_string()
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
        let c = if f.gren_kind == b.0 { PLATE } else { PLATE_DIM };
        if img.color != c {
            img.color = c;
        }
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
    for (mut t, gc, count, name, note, bk, ride) in texts.iter_mut() {
        let hand = |h: u8| hands[h as usize & 1].as_ref();
        let v = if let Some(g) = gc {
            let n = if g.0 == 0 { f.grenades } else { f.stickies };
            n.to_string()
        } else if let Some(c) = count {
            hand(c.0).map_or_else(String::new, |g| g.count.clone())
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
            f.riding().map_or_else(String::new, |_| format!("{out}  get out"))
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
        n.width = Val::Percent(u * 100.0);
        let c = if venting { SHIELD_LOW } else { HEAT };
        if img.color != c {
            img.color = c;
        }
    }
}

/// The shield and health: the shield's frame pulses red while it is low or
/// gone.
#[allow(clippy::type_complexity)]
pub fn vitals(
    link: NonSend<crate::LinkRes>,
    time: Res<Time>,
    mut shield: Query<(&mut Node, &mut ImageNode), (With<ShieldBar>, Without<ShieldFrame>)>,
    mut frame: Query<&mut ImageNode, With<ShieldFrame>>,
    mut health: Query<&mut Node, (With<HealthBar>, Without<ShieldBar>)>,
    mut line: Query<&mut Visibility, With<HealthLine>>,
) {
    let s = link.0.session();
    if !s.ready {
        return;
    }
    let f = s.core.own().0;
    let sh = (f.shield as f32 / SHIELD_MAX as f32 * 100.0).clamp(0.0, 100.0);
    let low = f.alive && sh < SHIELD_LOW_PCT;
    if let Ok((mut n, mut img)) = shield.single_mut() {
        n.width = Val::Percent(sh);
        img.color = if low { SHIELD_LOW } else { SHIELD };
    }
    if let Ok(mut b) = frame.single_mut() {
        let c = if low {
            let u = 0.5 + 0.5 * (time.elapsed_secs() * 9.0).sin();
            PLATE.mix(&SHIELD_LOW, u)
        } else {
            PLATE
        };
        b.color = c;
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
        n.width = Val::Percent((f.health as f32 / HEALTH_MAX as f32 * 100.0).clamp(0.0, 100.0));
    }
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
    let map = sim::maps::ALL.iter().find(|d| d.id == s.map_id).map_or("", |d| d.name);
    let clock = snap.clock as f32 / TICK_HZ as f32;
    // (score, colour, ours) a row.
    let mut shown: Vec<(i32, Color, bool)> = Vec::new();
    if core.rules.teams {
        let us = core.team & 1;
        let side = |t: u8| if t == 0 { RED_SIDE } else { BLUE_SIDE };
        shown.push((score(us), side(us), true));
        shown.push((score(1 - us), side(1 - us), false));
    } else {
        let mut all: Vec<(i32, u8)> = (0..scores.len() as u8)
            .filter(|&i| i == core.slot || st.names.contains_key(&i))
            .map(|i| (score(i), i))
            .collect();
        all.sort_by_key(|&(v, i)| (-v, i != core.slot, i));
        let at = all.iter().position(|&(_, i)| i == core.slot).unwrap_or(0);
        let pick = if at == 0 { [0, 1] } else { [0, at] };
        for k in pick {
            if let Some(&(v, i)) = all.get(k) {
                let me = i == core.slot;
                shown.push((v, if me { SHIELD } else { OTHER }, me));
            }
        }
    }
    // The leader on top.
    shown.sort_by_key(|&(v, _, me)| (-v, !me));
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
            if snap.phase == 1 {
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
    for (r, mut n) in rows.iter_mut() {
        set_display(&mut n, (r.0 as usize) < shown.len());
    }
    for (r, mut img) in swatches.iter_mut() {
        if let Some(&(_, c, _)) = shown.get(r.0 as usize) {
            img.color = c;
        }
    }
    for (r, mut n, mut img) in fills.iter_mut() {
        if let Some(&(v, c, _)) = shown.get(r.0 as usize) {
            n.width = Val::Percent((v as f32 / limit as f32 * 100.0).clamp(0.0, 100.0));
            img.color = c;
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
pub fn on_target(core: &client_net::core::NetCore, now_ms: f64) -> bool {
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
    core.others(now_ms)
        .iter()
        .filter(|d| d.alive && Some(d.team & 1) != mate)
        .any(|d| {
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
                || sim::geom::ray_sphere(eye, dir, d.pos + sim::geom::V3::new(0.0, hy, 0.0), HEAD_R + pad, wall)
                    .is_some()
        })
}

/// The crosshair red while it is on an enemy in reach ([`on_target`]).
pub fn reticle_tint(link: NonSend<crate::LinkRes>, mut q: Query<&mut BackgroundColor, With<ReticleStroke>>) {
    let s = link.0.session();
    let red = s.ready && on_target(&s.core, link.0.now_ms());
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

/// The scoreboard: held up (its key), and once the match is over until the
/// carnage report comes.
/// Every seat by side (teams first, each side's score) then by score:
/// name, kills, deaths, score.
#[allow(clippy::type_complexity)]
pub fn board(
    link: NonSend<crate::LinkRes>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<crate::binds::Binds>,
    mut st: ResMut<HudState>,
    mut panel: Query<&mut Visibility, With<Board>>,
    mut cols: Query<(&mut Text, &BoardColumn)>,
) {
    let s = link.0.session();
    let over = s.ready && s.core.latest().phase == 2;
    st.board = binds.pressed(crate::binds::Action::Scores, &keys, &mouse);
    // Once the match is over the carnage report (`medals.rs`) is the
    // board, as soon as its rows come.
    let up = s.ready && (st.board || (over && st.carnage.is_empty()));
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
    let mut rows: Vec<[String; 4]> = Vec::new();
    let mut seats: Vec<u8> = st.names.keys().copied().collect();
    let side = |q: u8| st.teams.get(&q).copied().unwrap_or(0) & 1;
    let kd = |q: u8| st.tally.kd.get(&q).copied().unwrap_or((0, 0));
    if core.rules.teams {
        for t in [core.team & 1, 1 - (core.team & 1)] {
            let name = if t == 0 { "RED" } else { "BLUE" };
            rows.push([name.into(), String::new(), String::new(), score(t as usize).to_string()]);
            let mut mine: Vec<u8> = seats.iter().copied().filter(|&q| side(q) == t).collect();
            mine.sort_by_key(|&q| (std::cmp::Reverse(kd(q).0), kd(q).1, q));
            for q in mine {
                let (k, d) = kd(q);
                rows.push([format!("  {}", st.name(q)), k.to_string(), d.to_string(), String::new()]);
            }
        }
    } else {
        seats.sort_by_key(|&q| (std::cmp::Reverse(score(q as usize)), q));
        for q in seats {
            let (k, d) = kd(q);
            let me = if q == core.slot { " <" } else { "" };
            rows.push([
                format!("{}{me}", st.name(q)),
                k.to_string(),
                d.to_string(),
                score(q as usize).to_string(),
            ]);
        }
    }
    let heads = ["", "K", "D", "SCORE"];
    for (mut t, c) in cols.iter_mut() {
        let i = c.0 as usize;
        let mut v = format!("{}\n", heads[i]);
        for r in &rows {
            v.push_str(&r[i]);
            v.push('\n');
        }
        if t.0 != v {
            t.0 = v;
        }
    }
}

/// The prompt under the crosshair: the USE key and what it would take.
pub fn prompt(
    link: NonSend<crate::LinkRes>,
    binds: Res<crate::binds::Binds>,
    mut q: Query<&mut Text, With<PromptText>>,
) {
    let s = link.0.session();
    let line = if s.ready {
        crate::items::offer(&s.core, link.0.now_ms())
            .map(|what| format!("{}  {what}", binds.key(crate::binds::Action::Use)))
    } else {
        None
    }
    .unwrap_or_default();
    for mut t in q.iter_mut() {
        if t.0 != line {
            t.0 = line.clone();
        }
    }
}

fn rim(alpha: f32) -> Color {
    Color::srgba(0.55, 0.02, 0.02, alpha)
}

/// The red rim: raised by a hit on us (`HudState::hurt_at`), fading out.
pub fn hurt_rim(
    time: Res<Time>,
    st: Res<HudState>,
    mut q: Query<(&mut BackgroundGradient, &mut Visibility), With<HurtRim>>,
) {
    let Ok((mut bg, mut vis)) = q.single_mut() else {
        return;
    };
    let u = (time.elapsed_secs() - st.hurt_at) / RIM_S;
    let alpha = if st.hurt_at > 0.0 && (0.0..1.0).contains(&u) {
        RIM_ALPHA * (1.0 - u)
    } else {
        0.0
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
            if let Some(stop) = r.stops.last_mut() {
                stop.color = rim(alpha);
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

//! The death screen and the scoreboard.
//!
//! - **The death card**: the moment we die, a band across the top of the
//!   screen lands (a little big, settling): how we died ("SNIPED BY"),
//!   the killer's emblem and name in their side's colour, the gun's
//!   picture and name (and the head, if it was), and the respawn's count.
//!   Under it the screen's edge darkens.
//! - **The scoreboard**: a row a player, after the arena shooters' (the
//!   rank, the emblem, the name, the numbers), each row tinted its side's
//!   colour, ours lit with a gold bar at its left; a side's own row heads
//!   its players with the side's score. Held up on its key, up on its own
//!   while we are dead (after [`hud::DEAD_BOARD_S`]), and once the match is
//!   over until the carnage report comes.
//!
//! Read from `HudState` and the session; nothing is decided here.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use sim::consts::{MAX_FIGHTERS, TICK_HZ};

use crate::hud::{self, HudState, INK};
use crate::ui;

/// Where the card and the board sit, a share of the screen down.
const CARD_TOP_PCT: f32 = 9.0;
const BOARD_TOP_PCT: f32 = 30.0;
/// The card: its emblem, the gun's picture (wide as the HUD's are), the
/// killer's name; how long it takes to land and from how big; the band's
/// darkest.
const CARD_EMBLEM_PX: f32 = 36.0;
const CARD_GUN_W: f32 = 84.0;
const CARD_NAME_PX: f32 = 34.0;
const LAND_S: f32 = 0.22;
const LAND_SCALE: f32 = 1.12;
const BAND_A: f32 = 0.82;
/// The screen's edge while dead: how dark, how fast, where it starts (a
/// share of the far-corner radius).
const SHADE_A: f32 = 0.7;
const SHADE_S: f32 = 0.45;
const SHADE_CLEAR_PCT: f32 = 30.0;
/// The board: its width, a row's height, the emblem in a row, the
/// columns (rank, emblem, name grows, kills, deaths, score).
const BOARD_W: f32 = 560.0;
const ROW_H: f32 = 22.0;
const ROW_EMBLEM_PX: f32 = 18.0;
const COLS: [f32; 6] = [30.0, 28.0, 0.0, 48.0, 48.0, 70.0];
/// Rows: every seat, and a side's own row for each of two.
const ROWS: usize = MAX_FIGHTERS + 2;
/// An emblem's texture, texels a side.
const EMBLEM_TEX: u32 = 64;
/// A seat's row: its side's colour at this alpha (ours at the second);
/// a side's row at the third.
const ROW_A: f32 = 0.2;
const MINE_A: f32 = 0.46;
const SIDE_A: f32 = 0.55;
/// Without sides: the rows alternate these, ours is the accent.
const ROW_EVEN: Color = Color::srgba(0.169, 0.153, 0.137, 0.62);
const ROW_ODD: Color = Color::srgba(0.118, 0.106, 0.094, 0.62);
/// Whoever killed us, without sides.
const FOE: Color = Color::srgb(1.0, 0.55, 0.48);

#[derive(Component)]
pub struct ScoresRoot;
#[derive(Component)]
pub struct DeathShade;
#[derive(Component)]
pub struct Card;
#[derive(Component)]
pub struct CardBand;
/// The card's lines: how (small), the killer, the gun's words, the count.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum CardText {
    Verb,
    Name,
    How,
    Respawn,
}
#[derive(Component)]
pub struct CardEmblem;
#[derive(Component)]
pub struct CardGun;
/// The card's two hairlines, in the killer's side's colour.
#[derive(Component)]
pub struct CardRule;

#[derive(Component)]
pub struct Board;
/// A row of the board, and its emblem.
#[derive(Component)]
pub struct Row(usize);
#[derive(Component)]
pub struct RowEmblem(usize);
/// A row's text cell: the row, the column (0 rank, 2 name, 3 kills, 4
/// deaths, 5 score).
#[derive(Component)]
pub struct Cell(usize, usize);

fn colour(team: Option<u8>) -> Color {
    match team {
        Some(t) => crate::look::bodies::team_color(t),
        None => FOE,
    }
}

/// A hairline that fades out at both ends.
fn rule(c: Color) -> BackgroundGradient {
    BackgroundGradient(vec![LinearGradient::to_right(vec![
        ColorStop::new(Color::NONE, Val::Percent(0.0)),
        ColorStop::new(c, Val::Percent(50.0)),
        ColorStop::new(Color::NONE, Val::Percent(100.0)),
    ])
    .into()])
}

/// The card's band: dark across the middle, clear at the sides.
fn band(a: f32) -> BackgroundGradient {
    let ink = |k: f32| Color::srgba(0.03, 0.03, 0.035, BAND_A * k * a);
    BackgroundGradient(vec![LinearGradient::to_right(vec![
        ColorStop::new(ink(0.0), Val::Percent(0.0)),
        ColorStop::new(ink(1.0), Val::Percent(24.0)),
        ColorStop::new(ink(1.0), Val::Percent(76.0)),
        ColorStop::new(ink(0.0), Val::Percent(100.0)),
    ])
    .into()])
}

fn shade(a: f32) -> BackgroundGradient {
    BackgroundGradient(vec![RadialGradient::new(
        UiPosition::CENTER,
        RadialGradientShape::FarthestCorner,
        vec![
            ColorStop::new(Color::NONE, Val::Percent(SHADE_CLEAR_PCT)),
            ColorStop::new(Color::srgba(0.04, 0.01, 0.01, SHADE_A * a), Val::Percent(100.0)),
        ],
    )
    .into()])
}

fn cell(row: usize, col: usize, font: TextFont, justify: Justify) -> impl Bundle {
    let w = COLS[col];
    (
        Node {
            width: if w > 0.0 { Val::Px(w) } else { Val::Auto },
            flex_grow: if w > 0.0 { 0.0 } else { 1.0 },
            justify_content: match justify {
                Justify::Right => JustifyContent::FlexEnd,
                Justify::Center => JustifyContent::Center,
                _ => JustifyContent::FlexStart,
            },
            ..default()
        },
        children![(
            Cell(row, col),
            Text::new(""),
            font,
            TextColor(ui::TEXT_COLOR),
            hud::SHADOW
        )],
    )
}

pub fn setup(mut commands: Commands) {
    // One root, hidden off a match as the HUD's is (`front::hud_shown`).
    let root = commands
        .spawn((
            ScoresRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            bevy::ui::FocusPolicy::Pass,
        ))
        .id();
    // The shade under the HUD (the visor's image is at 0), over the world.
    commands.spawn((
        ChildOf(root),
        DeathShade,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        shade(0.0),
        GlobalZIndex(-1),
        Visibility::Hidden,
    ));
    // The card, over the HUD.
    commands.spawn((
        ChildOf(root),
        Card,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            top: Val::Percent(CARD_TOP_PCT),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        },
        UiTransform::default(),
        GlobalZIndex(1),
        Visibility::Hidden,
        children![
            (
                CardRule,
                Node {
                    width: Val::Percent(56.0),
                    height: Val::Px(1.0),
                    ..default()
                },
                rule(Color::NONE),
            ),
            (
                CardBand,
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(4.0),
                    padding: UiRect::axes(Val::Px(0.0), Val::Px(10.0)),
                    ..default()
                },
                band(0.0),
                children![
                    (
                        CardText::Verb,
                        Text::new(""),
                        ui::semibold(13.0),
                        TextColor(ui::DIM),
                        hud::SHADOW
                    ),
                    (
                        Node {
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(12.0),
                            ..default()
                        },
                        children![
                            (
                                CardEmblem,
                                Node {
                                    width: Val::Px(CARD_EMBLEM_PX),
                                    height: Val::Px(CARD_EMBLEM_PX),
                                    ..default()
                                },
                                ImageNode::default(),
                            ),
                            (
                                CardText::Name,
                                Text::new(""),
                                ui::display(CARD_NAME_PX),
                                TextColor(FOE),
                                hud::SHADOW
                            ),
                        ],
                    ),
                    (
                        Node {
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(8.0),
                            ..default()
                        },
                        children![
                            (
                                CardGun,
                                Node {
                                    width: Val::Px(CARD_GUN_W),
                                    height: Val::Px(
                                        CARD_GUN_W * crate::look::icons::ICON_H as f32
                                            / crate::look::icons::ICON_W as f32
                                    ),
                                    ..default()
                                },
                                ImageNode::default(),
                            ),
                            (
                                CardText::How,
                                Text::new(""),
                                ui::semibold(14.0),
                                TextColor(ui::DIM),
                                hud::SHADOW
                            ),
                        ],
                    ),
                ],
            ),
            (
                CardRule,
                Node {
                    width: Val::Percent(56.0),
                    height: Val::Px(1.0),
                    ..default()
                },
                rule(Color::NONE),
            ),
            (
                CardText::Respawn,
                Node {
                    margin: UiRect::top(Val::Px(6.0)),
                    ..default()
                },
                Text::new(""),
                ui::display(18.0),
                TextColor(INK),
                hud::SHADOW,
            ),
        ],
    ));
    // The board, over the HUD.
    commands
        .spawn((
            ChildOf(root),
            Board,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(BOARD_TOP_PCT),
                width: Val::Px(BOARD_W),
                margin: UiRect::left(Val::Px(-BOARD_W * 0.5)),
                padding: UiRect::all(Val::Px(6.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(1.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(ui::PANEL_BG),
            BorderColor::all(ui::RULE),
            GlobalZIndex(1),
            Visibility::Hidden,
        ))
        .with_children(|b| {
            // The heads.
            b.spawn(Node {
                height: Val::Px(20.0),
                align_items: AlignItems::Center,
                padding: UiRect::horizontal(Val::Px(8.0)),
                ..default()
            })
            .with_children(|r| {
                for (col, (head, j)) in [
                    ("#", Justify::Center),
                    ("", Justify::Left),
                    ("PLAYER", Justify::Left),
                    ("K", Justify::Right),
                    ("D", Justify::Right),
                    ("SCORE", Justify::Right),
                ]
                .into_iter()
                .enumerate()
                {
                    let w = COLS[col];
                    r.spawn((
                        Node {
                            width: if w > 0.0 { Val::Px(w) } else { Val::Auto },
                            flex_grow: if w > 0.0 { 0.0 } else { 1.0 },
                            justify_content: if j == Justify::Right {
                                JustifyContent::FlexEnd
                            } else if j == Justify::Center {
                                JustifyContent::Center
                            } else {
                                JustifyContent::FlexStart
                            },
                            ..default()
                        },
                        children![(Text::new(head), ui::semibold(11.0), TextColor(ui::FAINT))],
                    ));
                }
            });
            for i in 0..ROWS {
                b.spawn((
                    Row(i),
                    Node {
                        height: Val::Px(ROW_H),
                        align_items: AlignItems::Center,
                        padding: UiRect::horizontal(Val::Px(6.0)),
                        border: UiRect::left(Val::Px(3.0)),
                        border_radius: BorderRadius::all(Val::Px(2.0)),
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(ROW_EVEN),
                    BorderColor::all(Color::NONE),
                ))
                .with_children(|r| {
                    r.spawn(cell(i, 0, ui::semibold(13.0), Justify::Center));
                    r.spawn((
                        Node {
                            width: Val::Px(COLS[1]),
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        children![(
                            RowEmblem(i),
                            Node {
                                width: Val::Px(ROW_EMBLEM_PX),
                                height: Val::Px(ROW_EMBLEM_PX),
                                ..default()
                            },
                            ImageNode::default(),
                        )],
                    ));
                    r.spawn(cell(i, 2, ui::semibold(14.0), Justify::Left));
                    r.spawn(cell(i, 3, ui::semibold(14.0), Justify::Right));
                    r.spawn(cell(i, 4, ui::semibold(14.0), Justify::Right));
                    r.spawn(cell(i, 5, ui::display(16.0), Justify::Right));
                });
            }
        });
}

/// Each emblem's picture, made once.
#[derive(Default)]
pub struct Emblems(BTreeMap<proto::Emblem, Handle<Image>>);

impl Emblems {
    fn get(&mut self, images: &mut Assets<Image>, code: proto::Emblem) -> Option<Handle<Image>> {
        let code = proto::emblem_valid(code);
        if code == proto::EMBLEM_NONE {
            return None;
        }
        let h = self.0.entry(code).or_insert_with(|| {
            images.add(Image::new(
                Extent3d {
                    width: EMBLEM_TEX,
                    height: EMBLEM_TEX,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                crate::look::emblem::rgba(code, EMBLEM_TEX),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            ))
        });
        Some(h.clone())
    }
}

fn set_image(img: &mut ImageNode, node: &mut Node, h: Option<Handle<Image>>) {
    let shown = if h.is_some() { Display::Flex } else { Display::None };
    if node.display != shown {
        node.display = shown;
    }
    if let Some(h) = h {
        if img.image != h {
            img.image = h;
        }
    }
}

fn set_text(t: &mut Text, v: &str) {
    if t.0 != v {
        t.0.clear();
        t.0.push_str(v);
    }
}

/// What the card says of a death: the small line over the name, the
/// headline (the killer, or what we did to ourselves), its side, and the
/// gun's words.
pub fn card_words(st: &HudState, teams: bool, me: u8, d: hud::Death) -> (String, String, Option<u8>, String) {
    let how = crate::tally::how(d.cause);
    let mut what = how.to_string();
    if d.head {
        what.push_str("  \u{b7}  HEADSHOT");
    }
    if d.killer == me {
        let fell = d.cause == sim::events::Cause::Fall as u16;
        let line = if fell { "FELL TO YOUR DEATH" } else { "SUICIDE" };
        return (
            "YOU DIED".into(),
            line.into(),
            None,
            if fell { String::new() } else { what },
        );
    }
    let side = st.side(d.killer);
    let verb = if teams && side.is_some() && side == st.side(me) {
        "BETRAYED BY".to_string()
    } else {
        format!("{} BY", crate::tally::verb(d.cause).to_uppercase())
    };
    let name = if st.death_by.is_empty() {
        st.name(d.killer)
    } else {
        st.death_by.clone()
    };
    (verb, name, side.filter(|_| teams), what)
}

/// The shade and the card while we are dead.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn card(
    link: NonSend<crate::LinkRes>,
    time: Res<Time>,
    st: Res<HudState>,
    icons: Option<Res<crate::look::icons::GunIcons>>,
    mut images: ResMut<Assets<Image>>,
    mut emblems: Local<Emblems>,
    mut since: Local<Option<f32>>,
    (mut shown, mut banded): (Local<Option<hud::Death>>, Local<f32>),
    mut shade_q: Query<
        (&mut Visibility, &mut BackgroundGradient),
        (With<DeathShade>, Without<Card>, Without<CardBand>),
    >,
    mut card_q: Query<(&mut Visibility, &mut UiTransform), (With<Card>, Without<DeathShade>)>,
    mut band_q: Query<&mut BackgroundGradient, (With<CardBand>, Without<DeathShade>, Without<CardRule>)>,
    mut rules: Query<&mut BackgroundGradient, (With<CardRule>, Without<DeathShade>, Without<CardBand>)>,
    mut texts: Query<(&CardText, &mut Text, &mut TextColor)>,
    mut emblem: Query<(&mut ImageNode, &mut Node), (With<CardEmblem>, Without<CardGun>)>,
    mut gun: Query<(&mut ImageNode, &mut Node), (With<CardGun>, Without<CardEmblem>)>,
) {
    let s = link.0.session();
    let core = &s.core;
    let f = core.own().0;
    let dead = s.ready && core.phase() == sim::world::Phase::Playing && !f.alive && !st.ended && !link.0.closed();
    let now = time.elapsed_secs();
    let death = st.death.filter(|_| dead);
    match (dead, *since) {
        (true, None) => *since = Some(now),
        (false, Some(_)) => *since = None,
        _ => {}
    }
    let age = since.map_or(0.0, |t| now - t);
    let vis = |on: bool| if on { Visibility::Inherited } else { Visibility::Hidden };
    if let Ok((mut v, mut g)) = shade_q.single_mut() {
        v.set_if_neq(vis(dead));
        if dead && age < SHADE_S + 0.1 {
            *g = shade((age / SHADE_S).min(1.0));
        }
    }
    let Ok((mut v, mut xf)) = card_q.single_mut() else {
        return;
    };
    v.set_if_neq(vis(death.is_some()));
    let Some(d) = death else {
        *shown = None;
        *banded = -1.0;
        return;
    };
    // Landing: big, settling; the band and the type coming up with it.
    let u = (age / LAND_S).min(1.0);
    let ease = 1.0 - (1.0 - u) * (1.0 - u);
    let scale = Vec2::splat(LAND_SCALE - (LAND_SCALE - 1.0) * ease);
    if xf.scale != scale {
        xf.scale = scale;
    }
    // Written until it is whole: a long frame can jump the landing.
    if *banded != ease {
        *banded = ease;
        if let Ok(mut g) = band_q.single_mut() {
            *g = band(ease);
        }
    }
    let (verb, name, side, how) = card_words(&st, core.rules.teams, core.slot, d);
    if *shown != Some(d) {
        *shown = Some(d);
        let ink = if d.killer == core.slot { ui::TITLE } else { colour(side) };
        for mut g in &mut rules {
            *g = rule(ink.with_alpha(0.8));
        }
    }
    // Every frame: a gun's picture (and a seat's look) can come after the
    // death does.
    let em = (d.killer != core.slot)
        .then(|| emblems.get(&mut images, st.look(d.killer).emblem))
        .flatten();
    if let Ok((mut img, mut n)) = emblem.single_mut() {
        set_image(&mut img, &mut n, em);
    }
    let pic = (d.cause < sim::arms::COUNT as u16)
        .then(|| icons.as_ref().and_then(|i| i.get(d.cause as u8)).cloned())
        .flatten();
    if let Ok((mut img, mut n)) = gun.single_mut() {
        let ink = hud::INK.with_alpha(hud::INK.alpha() * ease);
        if img.color != ink {
            img.color = ink;
        }
        set_image(&mut img, &mut n, pic);
    }
    let back = (f.respawn as f32 / TICK_HZ as f32).ceil() as u32;
    let mut respawn = String::new();
    let _ = write!(respawn, "RESPAWN IN {back}");
    for (k, mut t, mut c) in &mut texts {
        let (v, ink) = match k {
            CardText::Verb => (verb.as_str(), ui::DIM),
            CardText::Name => (
                name.as_str(),
                if d.killer == core.slot { ui::TITLE } else { colour(side) },
            ),
            CardText::How => (how.as_str(), ui::DIM),
            CardText::Respawn => (respawn.as_str(), INK),
        };
        set_text(&mut t, v);
        c.set_if_neq(TextColor(ink.with_alpha(ink.alpha() * ease)));
    }
}

/// One of the board's lines.
#[derive(Clone, Copy)]
pub enum Line {
    Side {
        t: u8,
        score: i32,
    },
    Seat {
        slot: u8,
        rank: usize,
        team: Option<u8>,
        score: Option<i32>,
    },
}

/// The board: up while held, while dead, and once the match is over until
/// the carnage report; a row a seat (and a side's own row on sides).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn board(
    link: NonSend<crate::LinkRes>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    binds: Res<crate::binds::Binds>,
    mut st: ResMut<HudState>,
    mut images: ResMut<Assets<Image>>,
    (mut emblems, mut lines, mut buf): (Local<Emblems>, Local<Vec<Line>>, Local<String>),
    mut panel: Query<&mut Visibility, With<Board>>,
    mut rows: Query<(&Row, &mut Node, &mut BackgroundColor, &mut BorderColor)>,
    mut cells: Query<(&Cell, &mut Text, &mut TextColor)>,
    mut marks: Query<(&RowEmblem, &mut ImageNode, &mut Node), Without<Row>>,
) {
    let s = link.0.session();
    let over = s.ready && s.core.phase() == sim::world::Phase::Over;
    let held = binds.pressed(
        crate::binds::Action::Scores,
        &crate::binds::Controls::new(&keys, &mouse, 0.0),
    );
    if st.board != held {
        st.board = held;
    }
    let dead = s.ready && !over && hud::boarded(s.core.own().0);
    let up = s.ready && (st.board || dead || (over && st.carnage.is_empty()));
    if let Ok(mut v) = panel.single_mut() {
        v.set_if_neq(if up { Visibility::Inherited } else { Visibility::Hidden });
    }
    if !up {
        return;
    }
    let core = &s.core;
    let snap = core.latest();
    let score = |i: usize| snap.score.get(i).copied().unwrap_or(0);
    let kd = |q: u8| st.tally.kd.get(&q).copied().unwrap_or((0, 0));
    lines.clear();
    let mut seats: Vec<u8> = st.names().map(|(&q, _)| q).collect();
    if core.rules.teams {
        let mine = core.team & 1;
        for t in [mine, 1 - mine] {
            lines.push(Line::Side {
                t,
                score: score(t as usize),
            });
            let mut side: Vec<u8> = seats
                .iter()
                .copied()
                .filter(|&q| st.side(q).unwrap_or(0) == t)
                .collect();
            side.sort_by_key(|&q| (std::cmp::Reverse(kd(q).0), kd(q).1, q));
            let mut rank = 0;
            for (i, &q) in side.iter().enumerate() {
                if i == 0 || kd(side[i - 1]) != kd(q) {
                    rank = i + 1;
                }
                lines.push(Line::Seat {
                    slot: q,
                    rank,
                    team: Some(t),
                    score: None,
                });
            }
        }
    } else {
        seats.sort_by_key(|&q| (std::cmp::Reverse(score(q as usize)), q));
        let mut rank = 0;
        for (i, &q) in seats.iter().enumerate() {
            if i == 0 || score(seats[i - 1] as usize) != score(q as usize) {
                rank = i + 1;
            }
            lines.push(Line::Seat {
                slot: q,
                rank,
                team: None,
                score: Some(score(q as usize)),
            });
        }
    }
    let me = core.slot;
    for (r, mut n, mut bg, mut edge) in &mut rows {
        let line = lines.get(r.0).copied();
        let on = if line.is_some() { Display::Flex } else { Display::None };
        if n.display != on {
            n.display = on;
        }
        let (c, e) = match line {
            None => continue,
            Some(Line::Side { t, .. }) => (crate::look::bodies::team_color(t).with_alpha(SIDE_A), Color::NONE),
            Some(Line::Seat { slot, team, .. }) => {
                let mine = slot == me;
                let c = match team {
                    Some(t) => crate::look::bodies::team_color(t).with_alpha(if mine { MINE_A } else { ROW_A }),
                    None if mine => ui::PICKED,
                    None if r.0 % 2 == 0 => ROW_EVEN,
                    None => ROW_ODD,
                };
                (c, if mine { ui::MARK } else { Color::NONE })
            }
        };
        bg.set_if_neq(BackgroundColor(c));
        let e = BorderColor::all(e);
        if *edge != e {
            *edge = e;
        }
    }
    for (c, mut t, mut ink) in &mut cells {
        buf.clear();
        let mut col = ui::TEXT_COLOR;
        match lines.get(c.0).copied() {
            None => {}
            Some(Line::Side { t: side, score }) => {
                col = ui::TITLE;
                let _ = match c.1 {
                    2 => write!(buf, "{} TEAM", if side == 0 { "RED" } else { "BLUE" }),
                    5 => write!(buf, "{score}"),
                    _ => Ok(()),
                };
            }
            Some(Line::Seat { slot, rank, score, .. }) => {
                if slot == me {
                    col = ui::TITLE;
                }
                let (k, d) = kd(slot);
                let _ = match c.1 {
                    0 => {
                        col = ui::DIM;
                        write!(buf, "{rank}")
                    }
                    2 => write!(buf, "{}", st.shown(slot)),
                    3 => write!(buf, "{k}"),
                    4 => write!(buf, "{d}"),
                    5 => score.map_or(Ok(()), |v| write!(buf, "{v}")),
                    _ => Ok(()),
                };
            }
        }
        set_text(&mut t, &buf);
        ink.set_if_neq(TextColor(col));
    }
    for (m, mut img, mut n) in &mut marks {
        let h = match lines.get(m.0) {
            Some(Line::Seat { slot, .. }) => emblems.get(&mut images, st.look(*slot).emblem),
            _ => None,
        };
        set_image(&mut img, &mut n, h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_names_the_killer_and_how() {
        let mut st = HudState::default();
        let d = |killer, cause: u16, head| hud::Death { killer, cause, head };
        let sniper = sim::arms::SNIPER as u16;
        let (verb, name, side, how) = card_words(&st, false, 0, d(3, sniper, true));
        assert_eq!((verb.as_str(), name.as_str(), side), ("SNIPED BY", "P4", None));
        assert!(how.ends_with("HEADSHOT"), "{how}");
        let fall = sim::events::Cause::Fall as u16;
        let (verb, name, _, how) = card_words(&st, false, 0, d(0, fall, false));
        assert_eq!(
            (verb.as_str(), name.as_str(), how.as_str()),
            ("YOU DIED", "FELL TO YOUR DEATH", "")
        );
        st.set_seat_side(0, 1);
        st.set_seat_side(3, 1);
        let (verb, _, side, _) = card_words(&st, true, 0, d(3, sniper, false));
        assert_eq!((verb.as_str(), side), ("BETRAYED BY", Some(1)));
    }
}

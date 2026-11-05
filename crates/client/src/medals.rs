//! Medals on screen, and the carnage report. Our latest medals sit in a row
//! low on the left, each popping in and fading out, coloured by its kind:
//! the multi-kill rungs amber growing to red, sprees violet, the rare kills
//! gold, the rest ice, assists grey. Once the match is over, everyone's
//! report: kills, deaths, assists, KDA (kills + assists / 3 − deaths),
//! accuracy, damage dealt, best spree and their medals; ours spelled out
//! below. Read from `HudState`; nothing is decided here.

use bevy::prelude::*;
use proto::medal as md;
use proto::Msg;

use crate::hud::{HudState, MEDAL_S};
use crate::ui;

/// A new medal comes in this much bigger and settles in this long.
const POP_SCALE: f32 = 1.45;
const POP_S: f32 = 0.18;
/// The last this-long of a medal's time on screen, it fades.
const FADE_S: f32 = 0.6;
/// Just over the shield; over the net graph too while it is up.
const ROW_BOTTOM: f32 = 74.0;
const ROW_BOTTOM_NET: f32 = 160.0;

#[derive(Component)]
pub struct MedalRow;

#[derive(Component)]
pub struct Badge {
    at: f32,
    colour: Color,
}

#[derive(Component)]
pub struct CarnagePanel;

/// A medal's colour: what kind it is, and for a multi-kill how far up.
pub fn colour(id: u8) -> Color {
    let gold = ui::GOLD;
    match md::info(id).family {
        md::Family::Multi => {
            let rung = md::MULTI.iter().position(|&m| m == id).unwrap_or(0) as f32;
            let u = rung / (md::MULTI.len() - 1) as f32;
            Color::srgb(1.0, 0.78 - 0.52 * u, 0.22 + 0.16 * u)
        }
        md::Family::Spree => Color::srgb(0.78, 0.55, 1.0),
        md::Family::Assist => Color::srgb(0.75, 0.78, 0.82),
        md::Family::Style => match id {
            md::PERFECT | md::PERFECTION | md::EXTERMINATION | md::KILLJOY | md::GAME_WINNER => gold,
            _ => Color::srgb(0.55, 0.86, 1.0),
        },
    }
}

pub fn setup(mut commands: Commands) {
    commands.spawn((
        MedalRow,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(22.0),
            bottom: Val::Px(ROW_BOTTOM),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        },
    ));
}

fn badge(id: u8, at: f32) -> impl Bundle {
    let c = colour(id);
    (
        Badge { at, colour: c },
        Node {
            padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
            border: UiRect::all(Val::Px(2.0)),
            border_radius: BorderRadius::all(Val::Px(3.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.04, 0.06, 0.08, 0.78)),
        BorderColor::all(c),
        UiTransform::default(),
        children![(
            Text::new(md::info(id).name.to_uppercase()),
            ui::display(17.0),
            TextColor(c),
            TextShadow::default(),
        )],
    )
}

/// The row: rebuilt when a medal comes or goes, each badge's pop and fade
/// every frame.
#[allow(clippy::type_complexity)]
pub fn row(
    mut commands: Commands,
    time: Res<Time>,
    st: Res<HudState>,
    mut row: Query<(Entity, &mut Node), With<MedalRow>>,
    mut badges: Query<(
        &Badge,
        &mut UiTransform,
        &mut BackgroundColor,
        &mut BorderColor,
        &Children,
    )>,
    mut texts: Query<&mut TextColor>,
    mut built: Local<(u32, usize)>,
) {
    let Ok((r, mut node)) = row.single_mut() else {
        return;
    };
    let bottom = Val::Px(if st.net { ROW_BOTTOM_NET } else { ROW_BOTTOM });
    if node.bottom != bottom {
        node.bottom = bottom;
    }
    let now = time.elapsed_secs();
    if *built != (st.medals_gen, st.medals.len()) {
        *built = (st.medals_gen, st.medals.len());
        commands.entity(r).despawn_related::<Children>();
        commands.entity(r).with_children(|c| {
            for &(id, at) in &st.medals {
                c.spawn(badge(id, at));
            }
        });
        return;
    }
    for (b, mut xf, mut bg, mut border, kids) in &mut badges {
        let age = now - b.at;
        let scale = if age < POP_S {
            POP_SCALE - (POP_SCALE - 1.0) * age / POP_S
        } else {
            1.0
        };
        if xf.scale.x != scale {
            xf.scale = Vec2::splat(scale);
        }
        let a = ((MEDAL_S - age) / FADE_S).clamp(0.0, 1.0);
        bg.set_if_neq(BackgroundColor(Color::srgba(0.04, 0.06, 0.08, 0.78 * a)));
        border.set_if_neq(BorderColor::all(b.colour.with_alpha(a)));
        for k in kids {
            if let Ok(mut t) = texts.get_mut(*k) {
                t.set_if_neq(TextColor(b.colour.with_alpha(a)));
            }
        }
    }
}

/// One seat's line of the report.
struct Line {
    slot: u8,
    kills: u16,
    deaths: u16,
    assists: u16,
    shots: u16,
    hits: u16,
    damage: u16,
    best_spree: u16,
    medals: Vec<(u8, u8)>,
}

fn line(m: &Msg) -> Option<Line> {
    let &Msg::Carnage {
        slot,
        kills,
        deaths,
        assists,
        shots,
        hits,
        damage,
        best_spree,
        medals,
        ..
    } = m
    else {
        return None;
    };
    // The rare first (what the announcer would say), then the most.
    let mut ms: Vec<(u8, u8)> = (1..md::COUNT as u8)
        .filter(|&id| medals[id as usize] > 0)
        .map(|id| (id, medals[id as usize]))
        .collect();
    ms.sort_by_key(|&(id, n)| (!md::info(id).voiced, std::cmp::Reverse(n), id));
    Some(Line {
        slot,
        kills,
        deaths,
        assists,
        shots,
        hits,
        damage,
        best_spree,
        medals: ms,
    })
}

fn listed(ms: &[(u8, u8)], n: usize) -> String {
    let each: Vec<String> = ms
        .iter()
        .take(n)
        .map(|&(id, k)| {
            let name = md::info(id).name;
            if k > 1 {
                format!("{name} x{k}")
            } else {
                name.to_string()
            }
        })
        .collect();
    let more = ms.len().saturating_sub(n);
    let mut s = each.join(" · ");
    if more > 0 {
        s.push_str(&format!(" +{more}"));
    }
    s
}

/// Widths of the report's columns, px: the name, the numbers, the medals.
const COLS: [(&str, f32); 9] = [
    ("PLAYER", 170.0),
    ("K", 40.0),
    ("D", 40.0),
    ("A", 40.0),
    ("KDA", 56.0),
    ("ACC", 56.0),
    ("DMG", 60.0),
    ("SPREE", 60.0),
    ("MEDALS", 250.0),
];

fn cell(s: String, w: f32, font: TextFont, c: Color) -> impl Bundle {
    (
        Node {
            width: Val::Px(w),
            ..default()
        },
        children![(Text::new(s), font, TextColor(c))],
    )
}

/// The report, once the match is over and the rows have come: rebuilt when
/// a row comes, gone when the next match starts.
pub fn carnage(
    mut commands: Commands,
    link: NonSend<crate::LinkRes>,
    st: Res<HudState>,
    panel: Query<Entity, With<CarnagePanel>>,
    mut shown: Local<usize>,
) {
    let s = link.0.session();
    let over = s.ready && s.core.latest().phase == 2;
    let n = if over { st.carnage.len() } else { 0 };
    if n == *shown {
        return;
    }
    *shown = n;
    for e in &panel {
        commands.entity(e).despawn();
    }
    if n == 0 {
        return;
    }
    let me = s.core.slot;
    let teams = s.core.rules.teams;
    let mut lines: Vec<Line> = st.carnage.values().filter_map(line).collect();
    lines.sort_by_key(|l| {
        let side = if teams { l.slot & 1 } else { 0 };
        (side, std::cmp::Reverse(l.kills), l.deaths, l.slot)
    });
    let mine = lines
        .iter()
        .find(|l| l.slot == me)
        .map(|l| listed(&l.medals, usize::MAX));
    commands
        .spawn((
            CarnagePanel,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                bottom: Val::Percent(12.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.0),
                    padding: UiRect::all(Val::Px(14.0)),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(ui::SHADE),
            ))
            .with_children(|p| {
                p.spawn((Text::new("CARNAGE REPORT"), ui::display(20.0), TextColor(ui::TITLE)));
                p.spawn(Node::default()).with_children(|r| {
                    for (h, w) in COLS {
                        r.spawn(cell(h.to_string(), w, ui::semibold(12.0), ui::DIM));
                    }
                });
                for l in &lines {
                    let c = if l.slot == me {
                        ui::ACCENT
                    } else if teams {
                        crate::look::bodies::team_color(l.slot & 1)
                    } else {
                        ui::TEXT_COLOR
                    };
                    let kda = l.kills as f32 + l.assists as f32 / 3.0 - l.deaths as f32;
                    let acc = if l.shots == 0 {
                        "-".to_string()
                    } else {
                        format!("{}%", l.hits as u32 * 100 / l.shots as u32)
                    };
                    let medal_count: u32 = l.medals.iter().map(|&(_, k)| k as u32).sum();
                    let medals = if l.medals.is_empty() {
                        String::new()
                    } else {
                        format!("{medal_count}: {}", listed(&l.medals, 2))
                    };
                    let vals = [
                        st.name(l.slot),
                        l.kills.to_string(),
                        l.deaths.to_string(),
                        l.assists.to_string(),
                        format!("{kda:.1}"),
                        acc,
                        l.damage.to_string(),
                        l.best_spree.to_string(),
                        medals,
                    ];
                    p.spawn(Node::default()).with_children(|r| {
                        for (v, (_, w)) in vals.into_iter().zip(COLS) {
                            r.spawn(cell(v, w, ui::semibold(14.0), c));
                        }
                    });
                }
                if let Some(m) = mine.filter(|m| !m.is_empty()) {
                    p.spawn((
                        Node {
                            max_width: Val::Px(COLS.iter().map(|c| c.1).sum()),
                            margin: UiRect::top(Val::Px(6.0)),
                            ..default()
                        },
                        children![(
                            Text::new(format!("Your medals: {m}")),
                            ui::text(14.0),
                            TextColor(ui::TEXT_COLOR)
                        )],
                    ));
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_line_lists_the_rare_medals_first() {
        let mut medals = [0u8; md::COUNT];
        medals[md::HEADSHOT as usize] = 9;
        medals[md::DOUBLE_KILL as usize] = 2;
        medals[md::PERFECT as usize] = 1;
        let m = Msg::Carnage {
            slot: 1,
            kills: 12,
            deaths: 3,
            assists: 4,
            betrayals: 0,
            shots: 200,
            hits: 120,
            heads: 30,
            damage: 2400,
            taken: 900,
            best_spree: 7,
            best_multi: 2,
            medals,
        };
        let l = line(&m).unwrap();
        assert_eq!(listed(&l.medals, 2), "Double Kill x2 · Perfect +1");
        assert_eq!(listed(&l.medals, 9), "Double Kill x2 · Perfect · Headshot x9");
    }
}

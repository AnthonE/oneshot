//! The scope, over a zoomed gun's view (the snipers' 5x and 10x, the
//! rifles' 2x):
//! a rounded window in the middle, the world dimmed around it, a black
//! frame; a ladder of ticks up each side, the stadia lines across, a ring
//! on the aim point (red over an enemy in reach, as the crosshair is) and a
//! post below it; the rounds in the magazine stacked inside on the left and
//! the magnification in the corner. Ours, in the HUD's blue. Drawn while
//! the scope is held on a gun that has one; it decides nothing.

use bevy::prelude::*;

use crate::hud;

/// The window: its share of the screen off each side, its frame and its
/// corners, px, and the dim around it.
const SIDE_PCT: f32 = 27.0;
const TOP_PCT: f32 = 23.0;
const FRAME_PX: f32 = 6.0;
/// At most ~3.4 frames, or the dim leaves the corners' outsides bare.
const CORNER_PX: f32 = 20.0;
const DIM: Color = Color::srgba(0.0, 0.0, 0.0, 0.58);
const FRAME: Color = Color::srgba(0.01, 0.015, 0.02, 0.96);
/// The etching: ticks, stadia and post.
const ETCH: Color = Color::srgba(0.02, 0.03, 0.04, 0.82);
const RING: Color = Color::srgba(0.78, 0.9, 1.0, 0.92);
const RING_HOT: Color = Color::srgba(1.0, 0.2, 0.14, 0.95);
const TICKS: usize = 23;
/// Rounds drawn inside: the magazine, up to this many.
pub const ROUNDS: usize = 8;

#[derive(Component)]
pub struct ScopeRoot;
#[derive(Component)]
pub struct ScopeRing;
#[derive(Component)]
pub struct ScopeRound(pub usize);
#[derive(Component)]
pub struct ScopeZoom;

fn abs(left: Val, right: Val, top: Val, bottom: Val) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left,
        right,
        top,
        bottom,
        ..default()
    }
}

fn ladder(c: &mut ChildSpawnerCommands, left: bool) {
    let (l, r) = if left {
        (Val::Px(8.0), Val::Auto)
    } else {
        (Val::Auto, Val::Px(8.0))
    };
    c.spawn(Node {
        flex_direction: FlexDirection::Column,
        justify_content: JustifyContent::SpaceBetween,
        align_items: if left {
            AlignItems::FlexStart
        } else {
            AlignItems::FlexEnd
        },
        ..abs(l, r, Val::Percent(5.0), Val::Percent(5.0))
    })
    .with_children(|c| {
        for i in 0..TICKS {
            // Every fourth a long one; the middle the longest.
            let w = if i == TICKS / 2 {
                22.0
            } else if i % 4 == TICKS / 2 % 4 {
                14.0
            } else {
                8.0
            };
            c.spawn((
                Node {
                    width: Val::Px(w),
                    height: Val::Px(2.0),
                    ..default()
                },
                BackgroundColor(ETCH),
            ));
        }
    });
}

/// Under the HUD's root, hidden.
pub fn setup(mut commands: Commands, root: Query<Entity, With<hud::HudRoot>>) {
    let Ok(root) = root.single() else {
        return;
    };
    let pct = Val::Percent;
    commands
        .spawn((
            ChildOf(root),
            ScopeRoot,
            Node {
                width: pct(100.0),
                height: pct(100.0),
                ..abs(Val::Px(0.0), Val::Auto, Val::Px(0.0), Val::Auto)
            },
            // Under the rest of the HUD.
            ZIndex(-1),
            Visibility::Hidden,
        ))
        .with_children(|c| {
            // The window: its frame's outline, as wide as any screen, is
            // the dim around it, rounded where the frame is.
            c.spawn((
                Node {
                    border: UiRect::all(Val::Px(FRAME_PX)),
                    border_radius: BorderRadius::all(Val::Px(CORNER_PX)),
                    ..abs(pct(SIDE_PCT), pct(SIDE_PCT), pct(TOP_PCT), pct(TOP_PCT))
                },
                BorderColor::all(FRAME),
                Outline::new(Val::Px(4096.0), Val::ZERO, DIM),
            ))
            .with_children(|w| {
                ladder(w, true);
                ladder(w, false);
                // The stadia: a heavy bar in from each side, then a hair
                // line to near the ring.
                for left in [true, false] {
                    let side = |v: f32| if left { (pct(v), Val::Auto) } else { (Val::Auto, pct(v)) };
                    let (l, r) = side(0.0);
                    w.spawn((
                        Node {
                            width: pct(17.0),
                            height: Val::Px(5.0),
                            margin: UiRect::top(Val::Px(-2.5)),
                            ..abs(l, r, pct(50.0), Val::Auto)
                        },
                        BackgroundColor(ETCH),
                    ));
                    let (l, r) = side(17.0);
                    w.spawn((
                        Node {
                            width: pct(29.0),
                            height: Val::Px(1.5),
                            margin: UiRect::top(Val::Px(-0.75)),
                            ..abs(l, r, pct(50.0), Val::Auto)
                        },
                        BackgroundColor(ETCH),
                    ));
                }
                // The post: from under the ring to the bottom, a bar across
                // it a little way down.
                w.spawn((
                    Node {
                        width: Val::Px(2.0),
                        margin: UiRect {
                            left: Val::Px(-1.0),
                            top: Val::Px(14.0),
                            ..default()
                        },
                        ..abs(pct(50.0), Val::Auto, pct(50.0), Val::Px(0.0))
                    },
                    BackgroundColor(ETCH),
                ));
                w.spawn((
                    Node {
                        width: Val::Px(36.0),
                        height: Val::Px(3.0),
                        margin: UiRect {
                            left: Val::Px(-18.0),
                            top: Val::Px(30.0),
                            ..default()
                        },
                        ..abs(pct(50.0), Val::Auto, pct(50.0), Val::Auto)
                    },
                    BackgroundColor(ETCH),
                ));
                // The ring on the aim point.
                w.spawn((
                    ScopeRing,
                    Node {
                        width: Val::Px(14.0),
                        height: Val::Px(14.0),
                        margin: UiRect {
                            left: Val::Px(-7.0),
                            top: Val::Px(-7.0),
                            ..default()
                        },
                        border: UiRect::all(Val::Px(1.5)),
                        border_radius: BorderRadius::MAX,
                        ..abs(pct(50.0), Val::Auto, pct(50.0), Val::Auto)
                    },
                    BorderColor::all(RING),
                ));
                // The rounds left, stacked low on the left, tips inward.
                w.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(7.0),
                    ..abs(Val::Px(40.0), Val::Auto, pct(60.0), Val::Auto)
                })
                .with_children(|c| {
                    for i in 0..ROUNDS {
                        c.spawn((
                            ScopeRound(i),
                            Node {
                                width: Val::Px(52.0),
                                height: Val::Px(9.0),
                                border_radius: BorderRadius {
                                    top_left: Val::Px(1.0),
                                    bottom_left: Val::Px(1.0),
                                    top_right: Val::Px(9.0),
                                    bottom_right: Val::Px(9.0),
                                },
                                display: Display::None,
                                ..default()
                            },
                            BackgroundColor(hud::PIP),
                        ));
                    }
                });
                w.spawn((
                    ScopeZoom,
                    Text::new(""),
                    crate::ui::display(24.0),
                    TextColor(hud::INK),
                    hud::SHADOW,
                    abs(Val::Auto, Val::Px(18.0), Val::Auto, Val::Px(12.0)),
                ));
            });
        });
}

/// Up while the scope is held on a gun that has one, alive and on foot;
/// its rounds, its zoom, and its ring red over an enemy in reach.
#[allow(clippy::type_complexity)]
pub fn draw(
    link: NonSend<crate::LinkRes>,
    look: Res<crate::Look>,
    seen: Res<crate::Seen>,
    mut root: Query<&mut Visibility, With<ScopeRoot>>,
    mut ring: Query<&mut BorderColor, With<ScopeRing>>,
    mut rounds: Query<(&mut Node, &ScopeRound)>,
    mut zoom: Query<&mut Text, With<ScopeZoom>>,
) {
    let s = link.0.session();
    let f = s.core.own().0;
    let arm = sim::arms::arm(f.gun().arm);
    let up = s.ready && f.alive && look.magnification(arm) > 1.0 && f.riding().is_none();
    if let Ok(mut v) = root.single_mut() {
        let want = if up { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
    if !up {
        return;
    }
    let red = seen.on_target;
    if let Ok(mut b) = ring.single_mut() {
        let c = if red { RING_HOT } else { RING };
        if b.top != c {
            *b = BorderColor::all(c);
        }
    }
    // A battery's charge is a number, not rounds.
    let shown = if arm.hot() || arm.mag as usize > ROUNDS {
        0
    } else {
        f.gun().mag as usize
    };
    for (mut n, r) in rounds.iter_mut() {
        let want = if r.0 < shown { Display::Flex } else { Display::None };
        if n.display != want {
            n.display = want;
        }
    }
    if let Ok(mut t) = zoom.single_mut() {
        let v = format!("{}x", arm.zoom_at(look.level));
        if t.0 != v {
            t.0 = v;
        }
    }
}

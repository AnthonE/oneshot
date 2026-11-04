//! The settings screen, after Gates' `client/src/render/settings.rs` (MIT):
//! a category rail down the left with the picked one blocked out in the
//! blue, a pane of rows on the right, each a label and its control, and a
//! foot with BACK, RESET TAB and where it's all kept. A number is a `-`
//! value `+` stepper, a toggle a button that says ON or OFF; a key cell
//! captures the next key pressed (Esc cancels, a right click clears it), and
//! a key taken from another action says where from. Everything saves as it
//! changes (`settings.rs`). Over a match the match shows through, so a wider
//! view is seen as it's chosen. BACK or Esc: the Esc menu, or the title.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::binds::{normalize, Action, Bind, Binds, MOUSE, SPECS};
use crate::settings::{Disk, Knob, Settings};
use crate::{ui, Screen};

#[derive(Component)]
struct OptionsRoot;

/// The part that scrolls.
#[derive(Component)]
struct Pane;

#[derive(Component, Clone, Copy)]
struct Adjust {
    knob: Knob,
    delta: i32,
}

#[derive(Component, Clone, Copy)]
struct BindCell {
    action: Action,
    slot: usize,
}

#[derive(Component, Clone, Copy)]
struct Category(usize);

#[derive(Component)]
struct Back;

#[derive(Component)]
struct ResetTab;

/// The rail.
pub const CATEGORIES: [&str; 3] = ["MOUSE AND VIEW", "SOUND", "KEYS"];
/// The rail's KEYS (the title's CONTROLS opens on it).
pub const KEYS: usize = 2;

/// The screen's own state: the picked category, the cell waiting for a
/// key, the line under the pane, and where the pane was scrolled to (kept
/// across a rebuild).
#[derive(Resource, Default)]
pub struct Page {
    pub cat: usize,
    capturing: Option<(Action, usize)>,
    note: String,
    scroll: f32,
    dirty: bool,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Page>()
        .add_systems(
            OnEnter(Screen::Settings),
            (crate::pause::free_cursor, enter, build).chain(),
        )
        .add_systems(OnExit(Screen::Settings), ui::despawn_all::<OptionsRoot>)
        .add_systems(
            Update,
            (capture, click, keys, scroll, rebuild)
                .chain()
                .run_if(in_state(Screen::Settings)),
        );
}

fn enter(mut st: ResMut<Page>) {
    st.capturing = None;
    st.note.clear();
    st.scroll = 0.0;
    st.dirty = false;
}

/// Where BACK goes: the Esc menu in a match (or a picture), else the title.
fn back(in_match: Option<&crate::front::InMatch>) -> Screen {
    match in_match {
        Some(m) if !m.0 => Screen::Title,
        _ => Screen::Paused,
    }
}

fn build(
    mut commands: Commands,
    settings: Res<Settings>,
    binds: Res<Binds>,
    st: Res<Page>,
    disk: Option<Res<Disk>>,
    in_match: Option<Res<crate::front::InMatch>>,
) {
    let to = back(in_match.as_deref());
    spawn(&mut commands, &settings, &binds, &st, disk.as_deref(), to);
}

/// The frame every row shares: label left, control right.
fn frame() -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::SpaceBetween,
        column_gap: Val::Px(20.0),
        padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
        flex_shrink: 0.0,
        ..default()
    }
}

fn spawn(commands: &mut Commands, settings: &Settings, binds: &Binds, st: &Page, disk: Option<&Disk>, to: Screen) {
    let over_match = to == Screen::Paused;
    let (root_bg, panel_bg) = if over_match {
        (Color::srgba(0.055, 0.051, 0.043, 0.55), ui::PANE_BG)
    } else {
        (Color::NONE, ui::PANEL_BG)
    };
    let saved = disk.map_or("settings are not being saved this run".into(), Disk::describe);
    let back_to = if over_match { "the match" } else { "the title" };
    commands
        .spawn((OptionsRoot, ui::shell(), BackgroundColor(root_bg), GlobalZIndex(10)))
        .with_children(|root| {
            if !over_match {
                root.spawn(ui::scrim());
            }
            root.spawn((
                ui::strong("SETTINGS", 30.0, ui::TITLE),
                ui::TEXT_SHADOW,
                Node {
                    margin: UiRect::new(Val::Px(34.0), Val::Px(0.0), Val::Px(26.0), Val::Px(16.0)),
                    ..default()
                },
            ));
            // The rail and the pane, side by side, both full height.
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: Val::Px(14.0),
                padding: UiRect::new(Val::Px(34.0), Val::Px(34.0), Val::Px(0.0), Val::Px(12.0)),
                ..default()
            })
            .with_children(|body| {
                body.spawn((
                    Node {
                        width: Val::Px(250.0),
                        flex_shrink: 0.0,
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        padding: UiRect::all(Val::Px(8.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(panel_bg),
                    BorderColor::all(ui::RULE),
                ))
                .with_children(|rail| {
                    for (i, name) in CATEGORIES.iter().enumerate() {
                        let on = i == st.cat;
                        let (idle, over) = if on {
                            (ui::ACCENT, ui::ACCENT_HOVER)
                        } else {
                            (ui::ROW_IDLE, ui::ROW_HOVER)
                        };
                        rail.spawn((
                            Button,
                            Category(i),
                            ui::Hover::new(idle, over),
                            Node {
                                width: Val::Percent(100.0),
                                padding: UiRect::axes(Val::Px(14.0), Val::Px(11.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(idle),
                            BorderColor::all(if on { ui::ACCENT } else { ui::RULE }),
                            children![ui::strong(*name, 16.0, if on { ui::TEXT_COLOR } else { ui::DIM })],
                        ));
                    }
                });
                body.spawn((
                    Pane,
                    Node {
                        flex_grow: 1.0,
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(3.0),
                        padding: UiRect::all(Val::Px(14.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    BackgroundColor(panel_bg),
                    BorderColor::all(ui::RULE),
                    ScrollPosition(Vec2::new(0.0, st.scroll)),
                ))
                .with_children(|pane| match st.cat {
                    0 => {
                        number(pane, settings, Knob::Sensitivity, "MOUSE SENSITIVITY", "times");
                        toggle(pane, settings, Knob::Invert, "INVERT LOOK");
                        number(pane, settings, Knob::Fov, "FIELD OF VIEW", "unscoped");
                        note(
                            pane,
                            "Scoped, the mouse turns slower by the zoom, so the world moves under the crosshair at one speed.",
                        );
                    }
                    1 => {
                        number(pane, settings, Knob::Master, "VOLUME", "everything");
                        number(pane, settings, Knob::Effects, "GAME SOUNDS", "shots, steps, hits");
                    }
                    _ => {
                        for s in &SPECS {
                            key_row(pane, binds, st, s.action, s.name);
                        }
                        note(pane, "Fire, scope and the wheel's swap stay on the mouse.");
                    }
                });
            });
            // The foot.
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(18.0),
                margin: UiRect::new(Val::Px(34.0), Val::Px(34.0), Val::Px(0.0), Val::Px(16.0)),
                ..default()
            })
            .with_children(|foot| {
                foot.spawn((ui::button(120.0, true), Back))
                    .with_child(ui::strong("BACK", 14.0, ui::TITLE));
                foot.spawn((ui::button(150.0, false), ResetTab))
                    .with_child(ui::strong("RESET TAB", 14.0, ui::TEXT_COLOR));
                foot.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.0),
                    ..default()
                })
                .with_children(|col| {
                    let hint = if st.capturing.is_some() {
                        "Press the key or mouse button to use · Esc cancels".to_string()
                    } else if !st.note.is_empty() {
                        st.note.clone()
                    } else if st.cat == KEYS {
                        "Click a key to change it · right click clears it".to_string()
                    } else {
                        String::new()
                    };
                    if !hint.is_empty() {
                        let c = if st.note.is_empty() { ui::DIM } else { ui::TEXT_COLOR };
                        col.spawn(ui::label(hint, 13.0, c));
                    }
                    col.spawn(ui::label(
                        format!("BACK or Esc returns to {back_to} · {saved}"),
                        12.0,
                        ui::FAINT,
                    ));
                });
            });
        });
}

fn note(p: &mut ChildSpawnerCommands, text: &str) {
    p.spawn((
        Node {
            padding: UiRect::axes(Val::Px(8.0), Val::Px(10.0)),
            max_width: Val::Px(760.0),
            flex_shrink: 0.0,
            ..default()
        },
        children![ui::label(text, 14.0, ui::DIM)],
    ));
}

fn number(p: &mut ChildSpawnerCommands, s: &Settings, knob: Knob, name: &str, unit: &str) {
    p.spawn((frame(), BackgroundColor(ui::ROW_IDLE))).with_children(|r| {
        r.spawn(ui::strong(name, 15.0, ui::TEXT_COLOR));
        r.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn(ui::label(unit, 12.0, ui::FAINT));
            c.spawn((ui::stepper(), Adjust { knob, delta: -1 }))
                .with_child(ui::strong("-", 16.0, ui::TEXT_COLOR));
            c.spawn((
                Node {
                    width: Val::Px(84.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                children![ui::strong(s.value(knob), 15.0, ui::TEXT_COLOR)],
            ));
            c.spawn((ui::stepper(), Adjust { knob, delta: 1 }))
                .with_child(ui::strong("+", 16.0, ui::TEXT_COLOR));
        });
    });
}

fn toggle(p: &mut ChildSpawnerCommands, s: &Settings, knob: Knob, name: &str) {
    p.spawn((frame(), BackgroundColor(ui::ROW_IDLE))).with_children(|r| {
        r.spawn(ui::strong(name, 15.0, ui::TEXT_COLOR));
        r.spawn((
            Button,
            Adjust { knob, delta: 0 },
            ui::Hover::default(),
            BackgroundColor(ui::ROW_IDLE),
            BorderColor::all(ui::RULE),
            Node {
                width: Val::Px(92.0),
                height: Val::Px(26.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            children![ui::strong(s.value(knob), 14.0, ui::TEXT_COLOR)],
        ));
    });
}

fn key_row(p: &mut ChildSpawnerCommands, binds: &Binds, st: &Page, action: Action, name: &str) {
    p.spawn((frame(), BackgroundColor(ui::ROW_IDLE))).with_children(|r| {
        r.spawn(ui::strong(name.to_uppercase(), 15.0, ui::TEXT_COLOR));
        r.spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|c| {
            for (slot, b) in binds.slots(action).into_iter().enumerate() {
                let waiting = st.capturing == Some((action, slot));
                let (text, colour) = match (waiting, b) {
                    (true, _) => ("PRESS A KEY", ui::TITLE),
                    (false, Some(b)) => (b.label(), ui::TEXT_COLOR),
                    (false, None) => ("-", ui::FAINT),
                };
                let (idle, over) = if waiting {
                    (ui::ACCENT, ui::ACCENT_HOVER)
                } else {
                    (ui::ROW_IDLE, ui::ROW_HOVER)
                };
                c.spawn((
                    Button,
                    BindCell { action, slot },
                    ui::Hover::new(idle, over),
                    Node {
                        width: Val::Px(132.0),
                        height: Val::Px(26.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(idle),
                    BorderColor::all(if waiting { ui::ACCENT } else { ui::RULE }),
                    children![ui::strong(text, 14.0, colour)],
                ));
            }
        });
    });
}

/// A key cell waiting: the next key or side button pressed is its bind
/// (taken from wherever it was); Esc cancels; a key that can't be bound
/// says so. Runs first, so the key it takes does nothing else this frame.
fn capture(
    mut st: ResMut<Page>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut binds: ResMut<Binds>,
) {
    let Some((action, slot)) = st.capturing else {
        return;
    };
    if keys.just_pressed(KeyCode::Escape) {
        keys.clear_just_pressed(KeyCode::Escape);
        st.capturing = None;
        st.note = "Kept as it was".into();
        st.dirty = true;
        return;
    }
    let pressed = keys
        .get_just_pressed()
        .next()
        .map(|k| Bind::Key(normalize(*k)))
        .or_else(|| {
            MOUSE
                .iter()
                .find(|(b, ..)| mouse.just_pressed(*b))
                .map(|(b, ..)| Bind::Mouse(*b))
        });
    let Some(bind) = pressed else {
        return;
    };
    if let Bind::Key(k) = bind {
        keys.clear_just_pressed(k);
    }
    st.capturing = None;
    st.dirty = true;
    if !bind.bindable() {
        st.note = format!("{} can't be bound", bind.label());
        return;
    }
    st.note = match binds.set(action, slot, Some(bind)) {
        Some(from) => {
            let gone = if binds.slots(from).iter().all(Option::is_none) {
                format!("; {} has no key now", from.spec().name)
            } else {
                String::new()
            };
            format!("{} moved here from {}{gone}", bind.label(), from.spec().name)
        }
        None => format!("{} is {}", action.spec().name, bind.label()),
    };
}

/// Clicks: the rail picks a category, a stepper or toggle turns its knob,
/// a key cell waits for a key, a right click on one clears it, RESET TAB
/// puts the category's defaults back, BACK goes back.
#[allow(clippy::too_many_arguments)]
fn click(
    cats: Query<(&Interaction, &Category), Changed<Interaction>>,
    adjust: Query<(&Interaction, &Adjust), Changed<Interaction>>,
    cells: Query<(&Interaction, &BindCell)>,
    backs: Query<&Interaction, (Changed<Interaction>, With<Back>)>,
    resets: Query<&Interaction, (Changed<Interaction>, With<ResetTab>)>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut settings: ResMut<Settings>,
    mut binds: ResMut<Binds>,
    mut st: ResMut<Page>,
    mut next: ResMut<NextState<Screen>>,
    in_match: Option<Res<crate::front::InMatch>>,
) {
    if st.capturing.is_none() {
        for (i, c) in &cats {
            if *i == Interaction::Pressed && c.0 != st.cat {
                st.cat = c.0;
                st.scroll = 0.0;
                st.note.clear();
                st.dirty = true;
            }
        }
        if resets.iter().any(|i| *i == Interaction::Pressed) {
            let d = Settings::default();
            match st.cat {
                0 => {
                    settings.sensitivity = d.sensitivity;
                    settings.invert = d.invert;
                    settings.fov = d.fov;
                }
                1 => {
                    settings.master = d.master;
                    settings.effects = d.effects;
                }
                _ => *binds = Binds::default(),
            }
            st.note = format!("{} is back to its defaults", CATEGORIES[st.cat]);
            st.dirty = true;
        }
    }
    for (i, a) in &adjust {
        if *i == Interaction::Pressed {
            settings.adjust(a.knob, a.delta);
            st.note.clear();
            st.dirty = true;
        }
    }
    for (i, c) in &cells {
        if *i == Interaction::Pressed && mouse.just_pressed(MouseButton::Left) && st.capturing.is_none() {
            st.capturing = Some((c.action, c.slot));
            st.note.clear();
            st.dirty = true;
        } else if *i == Interaction::Hovered && mouse.just_pressed(MouseButton::Right) && st.capturing.is_none() {
            binds.set(c.action, c.slot, None);
            st.note = format!("{}: key {} cleared", c.action.spec().name, c.slot + 1);
            st.dirty = true;
        }
    }
    if backs.iter().any(|i| *i == Interaction::Pressed) && st.capturing.is_none() {
        next.set(back(in_match.as_deref()));
    }
}

/// Esc goes back (when no cell is waiting: then it cancels, `capture`).
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    st: Res<Page>,
    mut next: ResMut<NextState<Screen>>,
    in_match: Option<Res<crate::front::InMatch>>,
) {
    if st.capturing.is_none() && keys.just_pressed(KeyCode::Escape) {
        next.set(back(in_match.as_deref()));
    }
}

/// The wheel scrolls the list.
fn scroll(
    mut wheel: MessageReader<MouseWheel>,
    mut pane: Query<&mut ScrollPosition, With<Pane>>,
    mut st: ResMut<Page>,
) {
    let dy: f32 = wheel
        .read()
        .map(|w| match w.unit {
            MouseScrollUnit::Line => w.y * 32.0,
            MouseScrollUnit::Pixel => w.y,
        })
        .sum();
    if dy == 0.0 {
        return;
    }
    for mut p in &mut pane {
        p.y = (p.y - dy).max(0.0);
        st.scroll = p.y;
    }
}

/// Built again when anything on it changed (Gates' way: simpler than
/// patching, and a screen this size builds in no time).
#[allow(clippy::too_many_arguments)]
fn rebuild(
    mut commands: Commands,
    mut st: ResMut<Page>,
    roots: Query<Entity, With<OptionsRoot>>,
    settings: Res<Settings>,
    binds: Res<Binds>,
    disk: Option<Res<Disk>>,
    in_match: Option<Res<crate::front::InMatch>>,
) {
    if !st.dirty {
        return;
    }
    st.dirty = false;
    for e in &roots {
        commands.entity(e).despawn();
    }
    spawn(
        &mut commands,
        &settings,
        &binds,
        &st,
        disk.as_deref(),
        back(in_match.as_deref()),
    );
}

//! The settings screen, after Gates' `client/src/render/settings.rs` (MIT),
//! trimmed to one page: the mouse, the view, the sound, and every key.
//! A number is a `-` value `+` stepper, a toggle a button that says ON or
//! OFF; a key cell captures the next key pressed (Esc cancels, a right
//! click clears it), and a key taken from another action says where from.
//! Everything saves as it changes (`settings.rs`). The match shows through,
//! so a wider view is seen as it's chosen. BACK or Esc: the Esc menu.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::binds::{normalize, Action, Bind, Binds, MOUSE, SPECS};
use crate::settings::{Knob, Settings};
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

#[derive(Component)]
struct Back;

/// The screen's own state: the cell waiting for a key, the line under the
/// list, and where the pane was scrolled to (kept across a rebuild).
#[derive(Resource, Default)]
struct Page {
    capturing: Option<(Action, usize)>,
    note: String,
    scroll: f32,
    dirty: bool,
}

const NUMBERS: [(Knob, &str); 4] = [
    (Knob::Sensitivity, "Mouse sensitivity"),
    (Knob::Fov, "Field of view"),
    (Knob::Master, "Volume"),
    (Knob::Effects, "Game sounds"),
];

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

fn row() -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::SpaceBetween,
        padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
        ..default()
    }
}

fn group(p: &mut ChildSpawnerCommands, name: &str) {
    p.spawn((
        Text::new(name),
        ui::display(20.0),
        TextColor(ui::ACCENT),
        Node {
            margin: UiRect::new(Val::Px(14.0), Val::Px(0.0), Val::Px(14.0), Val::Px(4.0)),
            ..default()
        },
    ));
}

fn build(mut commands: Commands, settings: Res<Settings>, binds: Res<Binds>, st: Res<Page>) {
    spawn(&mut commands, &settings, &binds, &st);
}

fn spawn(commands: &mut Commands, settings: &Settings, binds: &Binds, st: &Page) {
    commands
        .spawn((
            OptionsRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            // Lighter than the Esc menu's: the view shows through.
            BackgroundColor(Color::srgba(0.043, 0.059, 0.078, 0.45)),
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(620.0),
                    max_width: Val::Percent(96.0),
                    max_height: Val::Percent(94.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(18.0)),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(ui::PANEL),
            ))
            .with_children(|panel| {
                panel.spawn((Text::new("SETTINGS"), ui::display(34.0), TextColor(ui::TITLE)));
                panel
                    .spawn((
                        Pane,
                        Node {
                            flex_direction: FlexDirection::Column,
                            overflow: Overflow::scroll_y(),
                            flex_shrink: 1.0,
                            margin: UiRect::vertical(Val::Px(8.0)),
                            ..default()
                        },
                        ScrollPosition(Vec2::new(0.0, st.scroll)),
                    ))
                    .with_children(|pane| {
                        group(pane, "MOUSE AND VIEW");
                        for (knob, name) in &NUMBERS[..2] {
                            number(pane, settings, *knob, name);
                        }
                        toggle(pane, settings, Knob::Invert, "Invert look");
                        group(pane, "SOUND");
                        for (knob, name) in &NUMBERS[2..] {
                            number(pane, settings, *knob, name);
                        }
                        group(pane, "KEYS");
                        for s in &SPECS {
                            key_row(pane, binds, st, s.action, s.name);
                        }
                        pane.spawn((
                            ui::label("Fire, scope and the wheel's swap stay on the mouse.", 13.0, ui::DIM),
                            Node {
                                margin: UiRect::new(Val::Px(14.0), Val::Px(0.0), Val::Px(6.0), Val::Px(0.0)),
                                ..default()
                            },
                        ));
                    });
                let hint = if st.capturing.is_some() {
                    "Press a key or mouse button · Esc cancels".to_string()
                } else if !st.note.is_empty() {
                    st.note.clone()
                } else {
                    "Click a key to change it · right click clears it · saved as you go".to_string()
                };
                panel.spawn(ui::label(
                    hint,
                    14.0,
                    if st.note.is_empty() { ui::DIM } else { ui::ACCENT },
                ));
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(14.0),
                        margin: UiRect::top(Val::Px(12.0)),
                        ..default()
                    })
                    .with_children(|f| {
                        f.spawn((ui::button(140.0, true), Back))
                            .with_child(ui::button_label("BACK", true));
                        f.spawn(ui::label("or Esc", 13.0, ui::DIM));
                    });
            });
        });
}

fn number(p: &mut ChildSpawnerCommands, s: &Settings, knob: Knob, name: &str) {
    p.spawn(row()).with_children(|r| {
        r.spawn(ui::label(name, 16.0, ui::TEXT_COLOR));
        r.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn((ui::stepper(), Adjust { knob, delta: -1 }))
                .with_child(ui::button_label("-", false));
            c.spawn((
                Node {
                    width: Val::Px(70.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                children![(Text::new(s.value(knob)), ui::bold(16.0), TextColor(ui::TITLE))],
            ));
            c.spawn((ui::stepper(), Adjust { knob, delta: 1 }))
                .with_child(ui::button_label("+", false));
        });
    });
}

fn toggle(p: &mut ChildSpawnerCommands, s: &Settings, knob: Knob, name: &str) {
    p.spawn(row()).with_children(|r| {
        r.spawn(ui::label(name, 16.0, ui::TEXT_COLOR));
        r.spawn((ui::button(150.0, false), Adjust { knob, delta: 0 }))
            .with_child(ui::button_label(s.value(knob), false));
    });
}

fn key_row(p: &mut ChildSpawnerCommands, binds: &Binds, st: &Page, action: Action, name: &str) {
    p.spawn(row()).with_children(|r| {
        r.spawn(ui::label(name, 16.0, ui::TEXT_COLOR));
        r.spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|c| {
            for (slot, b) in binds.slots(action).into_iter().enumerate() {
                let waiting = st.capturing == Some((action, slot));
                let text = if waiting {
                    "PRESS A KEY"
                } else {
                    b.map_or("—", Bind::label)
                };
                c.spawn((ui::button(130.0, waiting), BindCell { action, slot }))
                    .with_child(ui::button_label(text, waiting));
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

/// Clicks: a stepper or toggle turns its knob, a key cell waits for a
/// key, a right click on one clears it, BACK goes back.
#[allow(clippy::too_many_arguments)]
fn click(
    adjust: Query<(&Interaction, &Adjust), Changed<Interaction>>,
    cells: Query<(&Interaction, &BindCell)>,
    back: Query<&Interaction, (Changed<Interaction>, With<Back>)>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut settings: ResMut<Settings>,
    mut binds: ResMut<Binds>,
    mut st: ResMut<Page>,
    mut next: ResMut<NextState<Screen>>,
) {
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
    if back.iter().any(|i| *i == Interaction::Pressed) && st.capturing.is_none() {
        next.set(Screen::Paused);
    }
}

/// Esc goes back (when no cell is waiting: then it cancels, `capture`).
fn keys(keys: Res<ButtonInput<KeyCode>>, st: Res<Page>, mut next: ResMut<NextState<Screen>>) {
    if st.capturing.is_none() && keys.just_pressed(KeyCode::Escape) {
        next.set(Screen::Paused);
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
fn rebuild(
    mut commands: Commands,
    mut st: ResMut<Page>,
    roots: Query<Entity, With<OptionsRoot>>,
    settings: Res<Settings>,
    binds: Res<Binds>,
) {
    if !st.dirty {
        return;
    }
    st.dirty = false;
    for e in &roots {
        commands.entity(e).despawn();
    }
    spawn(&mut commands, &settings, &binds, &st);
}

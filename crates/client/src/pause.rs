//! The Esc menu, after Gates' `client/src/render/pause.rs` (MIT): Resume,
//! Settings, Leave. The match goes on underneath (it's everyone's): the
//! cursor is free, so the controls send nothing (`pump`'s grab gate) while
//! the session keeps running. Esc resumes; so does a number or a click.

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::ui;
use crate::{LinkRes, Screen};

/// The menu's nodes.
#[derive(Component)]
struct PauseRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum Verb {
    Resume,
    Settings,
    Leave,
}

const VERBS: [(Verb, &str); 3] = [
    (Verb::Resume, "Resume"),
    (Verb::Settings, "Settings"),
    (Verb::Leave, "Leave the match"),
];

/// We left: the page goes back to its menu (`expose`), the desktop closes.
#[derive(Resource, Default)]
pub struct Left(pub bool);

pub fn plugin(app: &mut App) {
    app.init_resource::<Left>()
        .add_systems(OnEnter(Screen::Paused), (free_cursor, setup))
        .add_systems(OnExit(Screen::Paused), ui::despawn_all::<PauseRoot>)
        .add_systems(Update, choose.run_if(in_state(Screen::Paused)));
}

/// The pointer, let go: the menu's.
pub fn free_cursor(mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    if let Ok(mut c) = cursor.single_mut() {
        c.grab_mode = CursorGrabMode::None;
        c.visible = true;
    }
}

/// The pointer, taken back for the match, and the click that did it not a
/// shot (`Look::armed`).
pub fn lock_cursor(cursor: &mut Query<&mut CursorOptions, With<PrimaryWindow>>, look: &mut crate::Look) {
    if let Ok(mut c) = cursor.single_mut() {
        c.grab_mode = CursorGrabMode::Locked;
        c.visible = false;
    }
    look.armed = false;
}

fn setup(mut commands: Commands) {
    commands
        .spawn((
            PauseRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(12.0),
                ..default()
            },
            BackgroundColor(ui::SHADE),
            GlobalZIndex(10),
        ))
        .with_children(|p| {
            p.spawn((Text::new("ONESHOT"), ui::display(56.0), TextColor(ui::TITLE)));
            p.spawn((
                ui::label("The match goes on while you're here.", 15.0, ui::DIM),
                Node {
                    margin: UiRect::bottom(Val::Px(14.0)),
                    ..default()
                },
            ));
            for (i, (verb, name)) in VERBS.iter().enumerate() {
                p.spawn((ui::button(280.0, i == 0), *verb))
                    .with_child(ui::button_label(format!("{}   {name}", i + 1), i == 0));
            }
            p.spawn((
                ui::label("Esc resumes · click a row or press its number", 13.0, ui::DIM),
                Node {
                    margin: UiRect::top(Val::Px(10.0)),
                    ..default()
                },
            ));
        });
}

/// A click, a number or Esc picks a verb; one per frame.
#[allow(clippy::too_many_arguments)]
fn choose(
    buttons: Query<(&Interaction, &Verb), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<Screen>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut look: ResMut<crate::Look>,
    mut link: NonSendMut<LinkRes>,
    mut left: ResMut<Left>,
    mut exit: MessageWriter<AppExit>,
) {
    let clicked = buttons
        .iter()
        .find(|(i, _)| **i == Interaction::Pressed)
        .map(|(_, v)| *v);
    let digit = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3]
        .iter()
        .position(|k| keys.just_pressed(*k))
        .map(|i| VERBS[i].0);
    let esc = keys.just_pressed(KeyCode::Escape).then_some(Verb::Resume);
    let Some(verb) = clicked.or(digit).or(esc) else {
        return;
    };
    match verb {
        Verb::Resume => {
            lock_cursor(&mut cursor, &mut look);
            next.set(Screen::Playing);
        }
        Verb::Settings => next.set(Screen::Settings),
        Verb::Leave => {
            link.0.leave();
            left.0 = true;
            // The page goes back to its menu when it sees `left`; the
            // desktop's window closes.
            #[cfg(not(target_arch = "wasm32"))]
            exit.write(AppExit::Success);
            let _ = &mut exit;
        }
    }
}

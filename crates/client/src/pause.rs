//! The Esc menu, after Gates' `client/src/render/pause.rs` (MIT): the
//! wordmark over a dim wash, and numbered rows, each a verb over a line
//! saying what it does. The match goes on underneath (it's everyone's): the
//! cursor is free, so the controls send nothing (`pump`'s grab gate) while
//! the session keeps running. Esc resumes; so does a number or a click.
//!
//! A browser's has REPORT A BUG too: the page's form (`web/boot.js`, a text
//! box Bevy hasn't), sent to the server's `/bugs`. The desktop's crashes
//! report themselves (`crash.rs`).

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::ui;
use crate::Screen;

/// The menu's nodes.
#[derive(Component)]
struct PauseRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum Verb {
    Resume,
    Settings,
    Leave,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Report,
}

/// Last, so LEAVE is 3 on both.
const VERBS: &[(Verb, &str, &str)] = &[
    (Verb::Resume, "RESUME", "Back into the match"),
    (
        Verb::Settings,
        "SETTINGS",
        "The mouse, the view, the sound and your keys",
    ),
    (Verb::Leave, "LEAVE THE MATCH", "Back to the title screen"),
    #[cfg(target_arch = "wasm32")]
    (
        Verb::Report,
        "REPORT A BUG",
        "Tell us what went wrong; this match's details go with it",
    ),
];

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Paused), (free_cursor, setup))
        .add_systems(OnExit(Screen::Paused), ui::despawn_all::<PauseRoot>)
        .add_systems(Update, choose.run_if(in_state(Screen::Paused)));
}

/// The pointer, let go: the menu's.
pub fn free_cursor(mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    free(&mut cursor);
}

pub fn free(cursor: &mut Query<&mut CursorOptions, With<PrimaryWindow>>) {
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
        .spawn((PauseRoot, ui::screen(Color::srgba(0.055, 0.047, 0.039, 0.86))))
        .with_children(|p| {
            p.spawn(ui::wordmark_at(
                "THE MATCH GOES ON WHILE YOU'RE HERE",
                AlignItems::Center,
            ));
            p.spawn(Node {
                height: Val::Px(18.0),
                ..default()
            });
            for (i, (verb, name, detail)) in VERBS.iter().enumerate() {
                p.spawn((ui::row(Val::Px(460.0), false), *verb)).with_children(|b| {
                    b.spawn(ui::strong(format!("{}   {name}", i + 1), 20.0, ui::TEXT_COLOR));
                    b.spawn(ui::label(*detail, 13.0, ui::DIM));
                });
            }
            p.spawn((
                ui::label("Esc resumes · click a row or press its number", 13.0, ui::FAINT),
                Node {
                    margin: UiRect::top(Val::Px(12.0)),
                    ..default()
                },
            ));
        });
}

/// A click, a number or Esc picks a verb; one per frame.
fn choose(
    buttons: Query<(&Interaction, &Verb), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<Screen>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut look: ResMut<crate::Look>,
    (leaving, in_match): (
        Option<ResMut<crate::front::Leaving>>,
        Option<Res<crate::front::InMatch>>,
    ),
    mut exit: MessageWriter<AppExit>,
) {
    // The match ended this frame (`front::ended`): the title it is.
    if crate::front::InMatch::just_left(&in_match) {
        return;
    }
    let clicked = buttons
        .iter()
        .find(|(i, _)| **i == Interaction::Pressed)
        .map(|(_, v)| *v);
    let digit = ui::digit_pressed(&keys, VERBS.len()).map(|i| VERBS[i].0);
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
        // The page's form, over the menu (the cursor is already free).
        Verb::Report =>
        {
            #[cfg(target_arch = "wasm32")]
            crate::look::gpu::tell_page("__oneshotReport", "", "")
        }
        // Back to the title (`front::ended`); with no title (a picture),
        // the window closes.
        Verb::Leave => match leaving {
            Some(mut l) => l.0 = Some("You left the match".into()),
            None => {
                exit.write(AppExit::Success);
            }
        },
    }
}

//! A pad on the game's screens (the title, the Esc menu, the settings):
//! the stick or the D-pad moves a focus between the buttons on screen, A
//! presses the one focused, B and Start are Esc (in a match only Start, so
//! B never opens the menu by accident). Every screen's clicks work as they
//! are: the focused button is Hovered, and a press is a Pressed for one
//! frame, right after the UI's own focus pass (`UiSystems::Focus`).
//!
//! A screen built again under the focus (they rebuild when anything on them
//! moves) keeps it on the button nearest where it was. The mouse takes over
//! the moment it moves.
//!
//! Gamepads come from gilrs: always in a browser (its Gamepad API), on the
//! desktop with the `pad` feature (libudev on Linux).

use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::prelude::*;
use bevy::ui::{ComputedNode, UiGlobalTransform, UiSystems};
use bevy::window::CursorMoved;

use crate::Screen;

/// A stick past this is a push.
const STICK: f32 = 0.55;
/// A held direction moves again after this, then this often.
const REPEAT_FIRST_S: f32 = 0.35;
const REPEAT_S: f32 = 0.12;

#[derive(Resource, Default)]
struct Pad {
    /// The pad has the focus (the mouse hasn't moved since).
    on: bool,
    focus: Option<Entity>,
    /// Where the focus was, for a screen rebuilt under it.
    at: Vec2,
    /// The button pressed last frame, let go this one.
    pressed: Option<Entity>,
    held: Vec2,
    next_move: f32,
    /// Esc held down for B or Start.
    esc: bool,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Pad>()
        .add_systems(PreUpdate, nav.after(UiSystems::Focus));
}

/// The direction a stick or D-pad says, snapped to the nearer axis, in the
/// screen's sense (down is +y).
fn pushed(g: &Gamepad) -> Vec2 {
    let d = g.dpad();
    let v = if d != Vec2::ZERO {
        d
    } else if g.left_stick().length() > STICK {
        g.left_stick()
    } else {
        return Vec2::ZERO;
    };
    if v.x.abs() > v.y.abs() {
        Vec2::new(v.x.signum(), 0.0)
    } else {
        Vec2::new(0.0, -v.y.signum())
    }
}

/// The button from `from` the way `dir` points: the nearest along it, with
/// whatever is off to the side counted dearer.
fn step(cands: &[(Entity, Vec2)], from: Vec2, dir: Vec2) -> Option<Entity> {
    cands
        .iter()
        .filter_map(|&(e, p)| {
            let d = p - from;
            let along = d.dot(dir);
            if along <= 2.0 {
                return None;
            }
            let across = (d - dir * along).length();
            (across <= along * 2.5).then_some((along + across * 2.0, e))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|x| x.1)
}

fn nearest(cands: &[(Entity, Vec2)], to: Vec2) -> Option<Entity> {
    cands
        .iter()
        .min_by(|a, b| a.1.distance_squared(to).total_cmp(&b.1.distance_squared(to)))
        .map(|x| x.0)
}

#[allow(clippy::too_many_arguments)]
fn nav(
    pads: Query<&Gamepad>,
    mut moved: MessageReader<CursorMoved>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    screen: Res<State<Screen>>,
    time: Res<Time<Real>>,
    mut pad: ResMut<Pad>,
    mut buttons: Query<
        (
            Entity,
            &mut Interaction,
            &ComputedNode,
            &UiGlobalTransform,
            &InheritedVisibility,
        ),
        With<Button>,
    >,
) {
    // The press of the last frame, let go (the UI keeps a Pressed until the
    // mouse is released, which no pad does).
    if let Some(e) = pad.pressed.take() {
        if let Ok((_, mut i, ..)) = buttons.get_mut(e) {
            i.set_if_neq(Interaction::Hovered);
        }
    }
    if moved.read().count() > 0 || mouse.get_just_pressed().next().is_some() {
        pad.on = false;
    }
    let menu = *screen.get() != Screen::Playing;
    let (mut dir, mut press, mut back, mut back_held, mut any) = (Vec2::ZERO, false, false, false, false);
    for g in &pads {
        let d = pushed(g);
        if d != Vec2::ZERO {
            dir = d;
            any = true;
        }
        press |= g.just_pressed(GamepadButton::South);
        back |= g.just_pressed(GamepadButton::Start) || (menu && g.just_pressed(GamepadButton::East));
        back_held |= g.pressed(GamepadButton::Start) || (menu && g.pressed(GamepadButton::East));
        any |= g.get_just_pressed().next().is_some();
    }
    // B and Start are Esc, held while they are.
    if back && !pad.esc {
        keys.press(KeyCode::Escape);
        pad.esc = true;
    } else if pad.esc && !back_held {
        keys.release(KeyCode::Escape);
        pad.esc = false;
    }
    if !menu {
        pad.focus = None;
        return;
    }
    if any {
        pad.on = true;
    }
    if !pad.on {
        return;
    }
    let cands: Vec<(Entity, Vec2)> = buttons
        .iter()
        .filter(|(_, _, n, _, v)| v.get() && n.size().x > 0.0 && n.size().y > 0.0)
        .map(|(e, _, _, t, _)| (e, t.translation))
        .collect();
    let Some(mut focus) = pad
        .focus
        .filter(|f| cands.iter().any(|(e, _)| e == f))
        .or_else(|| nearest(&cands, pad.at))
    else {
        pad.focus = None;
        return;
    };
    let now = time.elapsed_secs();
    if dir == Vec2::ZERO {
        pad.held = Vec2::ZERO;
    } else if dir != pad.held || now >= pad.next_move {
        let from = cands.iter().find(|(e, _)| *e == focus).map_or(pad.at, |c| c.1);
        if let Some(next) = step(&cands, from, dir) {
            focus = next;
        }
        pad.next_move = now + if dir != pad.held { REPEAT_FIRST_S } else { REPEAT_S };
        pad.held = dir;
    }
    if let Some(&(_, at)) = cands.iter().find(|(e, _)| *e == focus) {
        pad.at = at;
    }
    if pad.focus != Some(focus) {
        if let Some(old) = pad.focus {
            if let Ok((_, mut i, ..)) = buttons.get_mut(old) {
                i.set_if_neq(Interaction::None);
            }
        }
    }
    pad.focus = Some(focus);
    if let Ok((_, mut i, ..)) = buttons.get_mut(focus) {
        if press {
            *i = Interaction::Pressed;
            pad.pressed = Some(focus);
        } else {
            i.set_if_neq(Interaction::Hovered);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_takes_the_nearest_that_way() {
        let e = |i: u32| Entity::from_raw_u32(i).unwrap();
        // A column of three, and one to the right of the middle.
        let cands = [
            (e(1), Vec2::new(100.0, 100.0)),
            (e(2), Vec2::new(100.0, 140.0)),
            (e(3), Vec2::new(100.0, 180.0)),
            (e(4), Vec2::new(400.0, 150.0)),
        ];
        let at = Vec2::new(100.0, 140.0);
        assert_eq!(step(&cands, at, Vec2::new(0.0, 1.0)), Some(e(3)));
        assert_eq!(step(&cands, at, Vec2::new(0.0, -1.0)), Some(e(1)));
        assert_eq!(step(&cands, at, Vec2::new(1.0, 0.0)), Some(e(4)));
        assert_eq!(step(&cands, at, Vec2::new(-1.0, 0.0)), None);
        assert_eq!(nearest(&cands, Vec2::new(390.0, 160.0)), Some(e(4)));
    }
}

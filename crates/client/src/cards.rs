//! Siege's cards on the HUD (docs/DESIGN.md §18, phase 1): the two hands,
//! each its card's name, the key that plays it and its cool; and an offer
//! open, its two cards, the keys that take them and the time left. Read from
//! the predicted fighter (its own record, the only one a card rides); what a
//! card does is the sim's (`sim::card`).

use bevy::prelude::*;
use sim::card;
use sim::consts::TICK_HZ;

use crate::binds::Action;
use crate::hud::{HudRoot, INK, SHADOW};

/// How far up from the bottom the cards sit, over the prompt's row.
const BOTTOM_PX: f32 = 96.0;
/// An offer's line: brighter than the hands', it asks for a choice.
const OFFER: Color = Color::srgba(0.98, 0.86, 0.45, 0.98);

/// A line of the cards' HUD: 0 and 1 the hands, 2 the offer.
#[derive(Component)]
pub struct CardLine(u8);

pub fn setup(mut commands: Commands, root: Query<Entity, With<HudRoot>>) {
    let Ok(root) = root.single() else {
        return;
    };
    let line = |i: u8, size: f32, color: Color| {
        (
            CardLine(i),
            Text::new(""),
            crate::ui::bold(size),
            TextColor(color),
            SHADOW,
        )
    };
    commands.spawn((
        ChildOf(root),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            bottom: Val::Px(BOTTOM_PX),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(4.0),
            ..default()
        },
        children![
            line(2, 15.0, OFFER),
            (
                Node {
                    column_gap: Val::Px(22.0),
                    ..default()
                },
                children![line(0, 13.0, INK), line(1, 13.0, INK)],
            ),
        ],
    ));
}

/// Whole seconds left of `ticks`, rounded up.
fn secs(ticks: u16) -> u32 {
    (ticks as u32).div_ceil(TICK_HZ)
}

/// The cards' lines as they are now: each hand's, and the offer's.
fn lines(f: &sim::fighter::Fighter, key: impl Fn(Action) -> &'static str) -> [String; 3] {
    let hand = |k: usize| {
        let c = f.cards[k];
        if c == card::NONE {
            return String::new();
        }
        let a = if k == 0 { Action::CardA } else { Action::CardB };
        match f.card_cool[k] {
            0 => format!("[{}] {}", key(a), card::name(c)),
            t => format!("[{}] {} {}s", key(a), card::name(c), secs(t)),
        }
    };
    let offer = if f.offer_left > 0 {
        format!(
            "TAKE A CARD   [{}] {}   [{}] {}   {}s",
            key(Action::PickA),
            card::name(f.offer[0]),
            key(Action::PickB),
            card::name(f.offer[1]),
            secs(f.offer_left)
        )
    } else {
        String::new()
    };
    [hand(0), hand(1), offer]
}

pub fn draw(
    link: NonSend<crate::LinkRes>,
    (binds, touch): (Res<crate::binds::Binds>, Res<crate::touch::Touch>),
    mut q: Query<(&mut Text, &CardLine)>,
    mut said: Local<[String; 3]>,
) {
    let s = link.0.session();
    let now = if s.ready {
        lines(s.core.own().0, |a| crate::touch::key(&binds, &touch, a))
    } else {
        Default::default()
    };
    // Written when it changes, not a fresh line a frame.
    if *said == now {
        return;
    }
    for (mut t, l) in q.iter_mut() {
        let v = &now[l.0 as usize];
        if t.0 != *v {
            t.0 = v.clone();
        }
    }
    *said = now;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hands_and_an_offer_read_as_they_are() {
        let mut f = sim::fighter::Fighter::default();
        let key = |a: Action| match a {
            Action::CardA => "X",
            Action::CardB => "3",
            Action::PickA => "4",
            _ => "5",
        };
        assert_eq!(lines(&f, key), [String::new(), String::new(), String::new()]);
        f.cards = [card::GRAPPLE, card::BIRD];
        f.card_cool = [0, (TICK_HZ * 40 + 1) as u16];
        f.offer = [card::REWIND, card::GRAPPLE];
        f.offer_left = (TICK_HZ * 9) as u16;
        assert_eq!(
            lines(&f, key),
            [
                "[X] GRAPPLE".to_string(),
                "[3] FROST BIRD 41s".to_string(),
                "TAKE A CARD   [4] REWIND   [5] GRAPPLE   9s".to_string()
            ]
        );
    }
}

//! Siege's cards (docs/DESIGN.md §18, phase 1): verbs a fighter picks up in
//! the match, two at most, offered as it plays and never owed. A card that
//! reads only its fighter and the map (the grapple) is the fighter's own
//! tick (`Fighter::step_local`), predicted like walking; one that touches
//! the world (the rewind, the frost bird) is the world's (`World::play`).
//! An enemy's cards are never sent; an offer is told only to its fighter
//! (it rides the fighter's own record).

use crate::consts::*;

pub const NONE: u8 = 0;
pub const GRAPPLE: u8 = 1;
pub const REWIND: u8 = 2;
pub const BIRD: u8 = 3;
/// Kinds, none among them.
pub const KINDS: u8 = 4;

/// The deck an offer is dealt from, each card with how often it comes (of
/// their sum): the frost bird, an ultimate, rarer.
pub const DECK: [(u8, u32); 3] = [(GRAPPLE, 4), (REWIND, 4), (BIRD, 2)];

/// How long a card waits after it is played.
pub fn cool_ms(card: u8) -> u32 {
    match card {
        GRAPPLE => GRAPPLE_COOL_MS,
        REWIND => REWIND_COOL_MS,
        BIRD => BIRD_COOL_MS,
        _ => 0,
    }
}

/// Its name, as the HUD and the offer show it.
pub fn name(card: u8) -> &'static str {
    match card {
        GRAPPLE => "GRAPPLE",
        REWIND => "REWIND",
        BIRD => "FROST BIRD",
        _ => "",
    }
}

/// The card `roll` (any u32) deals from [`DECK`], leaving out `not`.
pub fn deal(roll: u32, not: [u8; 3]) -> u8 {
    let total: u32 = DECK.iter().filter(|d| !not.contains(&d.0)).map(|d| d.1).sum();
    if total == 0 {
        return NONE;
    }
    let mut r = roll % total;
    for &(c, w) in DECK.iter().filter(|d| !not.contains(&d.0)) {
        if r < w {
            return c;
        }
        r -= w;
    }
    NONE
}

//! Which casual modes are open, by how many are online: with few players,
//! one playlist (free for all) where they meet each other, not five where
//! each fights bots alone. A mode opens at its count in [`OPENS_AT`]; one
//! closed stays playable while a public room of it has a person in it (a
//! friend can still follow, and a dip in the count never strands a room).
//! A closed mode asked for is played as [`HUB`]. A party's private room,
//! the bot duel, watching and ranked are never closed.

use proto::{MODE_BIG_TEAM, MODE_CASUAL, MODE_CTF, MODE_FFA, MODE_GUN_GAME, MODE_SIEGE};

/// Where a closed mode's players go.
pub const HUB: u8 = MODE_FFA;

/// Each gated mode and the players online it opens at (the title's and
/// the matches' together): low while the game is young, so a mode opens
/// once a few could meet in it.
pub const OPENS_AT: [(u8, usize); 4] = [(MODE_GUN_GAME, 4), (MODE_CTF, 6), (MODE_BIG_TEAM, 8), (MODE_SIEGE, 8)];

/// What `--all-modes` opens: every mode, whatever the count.
pub const ALL: u16 = u16::MAX;

/// The modes open with `online` players, and those in `live` (a bit a mode:
/// a public room of it with a person in it). Any mode not gated is open.
pub fn open(online: usize, live: u16) -> u16 {
    let mut m = ALL;
    for (mode, at) in OPENS_AT {
        if online < at && live & bit(mode) == 0 {
            m &= !bit(mode);
        }
    }
    m
}

pub fn bit(mode: u8) -> u16 {
    1u16.checked_shl(mode as u32).unwrap_or(0)
}

/// The mode a casual hello for `mode` plays, with `open` open.
pub fn route(mode: u8, open: u16, private: bool) -> u8 {
    if private || mode == MODE_CASUAL || open & bit(mode) != 0 {
        mode
    } else {
        HUB
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn few_online_play_one_playlist() {
        let m = open(3, 0);
        assert_eq!(route(MODE_FFA, m, false), MODE_FFA);
        assert_eq!(route(MODE_CASUAL, m, false), MODE_CASUAL, "the bot duel is practice");
        for mode in [MODE_GUN_GAME, MODE_CTF, MODE_BIG_TEAM, MODE_SIEGE] {
            assert_eq!(route(mode, m, false), HUB);
            assert_eq!(route(mode, m, true), mode, "a private party plays what it asked");
        }
        assert_eq!(route(proto::MODE_DUEL, m, false), proto::MODE_DUEL);
    }

    #[test]
    fn modes_open_as_players_come_and_a_live_room_stays_open() {
        assert_ne!(open(4, 0) & bit(MODE_GUN_GAME), 0);
        assert_eq!(open(4, 0) & bit(MODE_CTF), 0);
        assert_ne!(open(6, 0) & bit(MODE_CTF), 0);
        assert_eq!(open(7, 0) & bit(MODE_BIG_TEAM), 0);
        assert_ne!(open(8, 0) & bit(MODE_BIG_TEAM), 0);
        // Big Team running with people in it: still open with two online.
        let m = open(2, bit(MODE_BIG_TEAM));
        assert_eq!(route(MODE_BIG_TEAM, m, false), MODE_BIG_TEAM);
        assert_eq!(route(MODE_CTF, m, false), HUB);
    }
}

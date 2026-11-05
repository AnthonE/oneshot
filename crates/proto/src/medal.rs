//! Medals: what a kill was worth past the score, as the stream names them
//! ([`crate::Msg::Medal`]). The server decides them from the world's events
//! (`room/src/medals.rs`); the client draws and voices them.
//!
//! Classic arena rules, so competitive players find what they know: a
//! multi-kill's kills each within four seconds of the last (Halo 2, 3 and
//! Reach), a spree rung every five kills in a life, a Perfect in the gun's
//! fewest shots (the battle rifle's 4, the magnum's 5). The announcer says
//! the rare ones; the common ones (a headshot, an assist) are only drawn.
//! Names are generic shooter words or ours, never another game's coinages:
//! the multi-kill rungs past Overkill are our own.

/// What a medal is, for the HUD's colour and the announcer's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Kills in quick succession.
    Multi,
    /// Kills without dying.
    Spree,
    /// How or when a kill was made.
    Style,
    /// Damage on someone a teammate (or anyone) then killed.
    Assist,
}

#[derive(Clone, Copy, Debug)]
pub struct Info {
    pub name: &'static str,
    pub family: Family,
    /// The announcer says it. Frequent, quiet medals (a headshot, an
    /// assist) are drawn, never said.
    pub voiced: bool,
    /// Sent to everyone, not just its earner: only what the kill feed
    /// already tells (who killed whom, with what, and when), so it gives
    /// nobody's place away.
    pub public: bool,
}

const fn m(name: &'static str, family: Family, voiced: bool, public: bool) -> Info {
    Info {
        name,
        family,
        voiced,
        public,
    }
}

// The multi-kill ladder: the 2nd to the 10th kill of a chain. Past the
// 10th, every kill is the last rung again.
pub const DOUBLE_KILL: u8 = 1;
pub const TRIPLE_KILL: u8 = 2;
pub const OVERKILL: u8 = 3;
pub const KILLSTORM: u8 = 4;
pub const KILLQUAKE: u8 = 5;
pub const KILLCANO: u8 = 6;
pub const KILLOSSAL: u8 = 7;
pub const KILLMAGEDDON: u8 = 8;
pub const KILLENNIUM: u8 = 9;
/// The rungs, by the chain's length less two.
pub const MULTI: [u8; 9] = [
    DOUBLE_KILL,
    TRIPLE_KILL,
    OVERKILL,
    KILLSTORM,
    KILLQUAKE,
    KILLCANO,
    KILLOSSAL,
    KILLMAGEDDON,
    KILLENNIUM,
];

// The spree ladder: 5, 10, … 40 kills in a life.
pub const KILLING_SPREE: u8 = 10;
pub const KILLING_FRENZY: u8 = 11;
pub const RUNNING_RIOT: u8 = 12;
pub const RAMPAGE: u8 = 13;
pub const UNTOUCHABLE: u8 = 14;
pub const INVINCIBLE: u8 = 15;
pub const INCONCEIVABLE: u8 = 16;
pub const UNSTOPPABLE: u8 = 17;
/// Kills between rungs of the spree ladder.
pub const SPREE_STEP: u16 = 5;
/// The rungs, at `SPREE_STEP`, `2 × SPREE_STEP`, ….
pub const SPREE: [u8; 8] = [
    KILLING_SPREE,
    KILLING_FRENZY,
    RUNNING_RIOT,
    RAMPAGE,
    UNTOUCHABLE,
    INVINCIBLE,
    INCONCEIVABLE,
    UNSTOPPABLE,
];

/// The killing round hit the head (a gun that kills on one there).
pub const HEADSHOT: u8 = 18;
/// A sniper or beam rifle's headshot kill.
pub const SNIPER_KILL: u8 = 19;
/// A sniper or beam rifle kill without its scope up.
pub const NO_SCOPE: u8 = 20;
/// A melee kill from the front.
pub const BEAT_DOWN: u8 = 21;
/// A melee kill from behind.
pub const ASSASSINATION: u8 = 22;
/// Run down by your vehicle.
pub const SPLATTER: u8 = 23;
/// A sticky grenade stuck to them.
pub const STICK: u8 = 24;
/// A frag's blast, or a sticky's that wasn't stuck to them.
pub const GRENADE_KILL: u8 = 25;
/// A kill made after you died (a grenade, a rocket in the air).
pub const FROM_THE_GRAVE: u8 = 26;
/// Ended someone's spree.
pub const KILLJOY: u8 = 27;
/// Killed the one who last killed you.
pub const REVENGE: u8 = 28;
/// Killed an enemy who had just killed a teammate.
pub const AVENGER: u8 = 29;
/// The match's first kill.
pub const FIRST_STRIKE: u8 = 30;
/// A kill with the magazine's last round.
pub const LAST_SHOT: u8 = 31;
/// A precision kill in the gun's fewest shots, none missed, from full, all
/// yours.
pub const PERFECT: u8 = 32;
/// Killed an enemy who hurt you first, after your shields went down.
pub const REVERSAL: u8 = 33;
/// Killed an enemy who was hurting a teammate.
pub const PROTECTOR: u8 = 34;
/// You drove; your passenger made the kill.
pub const WHEELMAN: u8 = 35;
/// You did real damage to them; someone else finished them.
pub const ASSIST: u8 = 36;
/// Killed the whole enemy side, four or more, each of them down at once.
pub const EXTERMINATION: u8 = 37;
/// An enemy was hurting you; a teammate killed them.
pub const DISTRACTION: u8 = 38;
/// A grenade kill from far off.
pub const HAIL_MARY: u8 = 39;
/// A headshot kill on someone your grenade softened.
pub const NADE_SHOT: u8 = 40;
/// The kill that won the match.
pub const GAME_WINNER: u8 = 41;
/// Won it with at least [`PERFECTION_KILLS`] kills and no death.
pub const PERFECTION: u8 = 42;
/// A No Scope after turning a full circle: the killer's yaw went round at
/// least once, one way, in the last moment before it (`room::medals`).
pub const NO_SCOPE_360: u8 = 43;

/// Every id, `NONE` (0) first.
pub const COUNT: usize = 44;
pub const NONE: u8 = 0;
/// A Perfection's kills: a duel's whole score.
pub const PERFECTION_KILLS: u16 = 15;

use Family::*;

// Voiced or silent after the classic games' announcer: every rung, and the
// rare kills; never the frequent ones.
pub static MEDALS: [Info; COUNT] = [
    m("", Style, false, false),
    m("Double Kill", Multi, true, true),
    m("Triple Kill", Multi, true, true),
    m("Overkill", Multi, true, true),
    m("Killstorm", Multi, true, true),
    m("Killquake", Multi, true, true),
    m("Killcano", Multi, true, true),
    m("Killossal", Multi, true, true),
    m("Killmageddon", Multi, true, true),
    m("Killennium", Multi, true, true),
    m("Killing Spree", Spree, true, true),
    m("Killing Frenzy", Spree, true, true),
    m("Running Riot", Spree, true, true),
    m("Rampage", Spree, true, true),
    m("Untouchable", Spree, true, true),
    m("Invincible", Spree, true, true),
    m("Inconceivable", Spree, true, true),
    m("Unstoppable", Spree, true, true),
    m("Headshot", Style, false, false),
    m("Sniper Kill", Style, false, false),
    m("No Scope", Style, true, false),
    m("Beat Down", Style, false, true),
    m("Assassination", Style, true, true),
    m("Splatter", Style, false, true),
    m("Stick", Style, false, false),
    m("Grenade Kill", Style, false, true),
    m("Kill from the Grave", Style, true, true),
    m("Killjoy", Style, true, true),
    m("Revenge", Style, true, true),
    m("Avenger", Style, true, true),
    m("First Strike", Style, true, true),
    m("Last Shot", Style, true, false),
    m("Perfect", Style, true, false),
    m("Reversal", Style, true, false),
    m("Protector", Style, false, false),
    m("Wheelman", Style, false, false),
    m("Assist", Assist, false, false),
    m("Extermination", Style, true, true),
    m("Distraction", Style, true, false),
    m("Hail Mary", Style, true, false),
    m("Nade Shot", Style, false, false),
    m("Game Winner", Style, true, true),
    m("Perfection", Style, true, true),
    m("360 No Scope", Style, false, false),
];

/// Medal `id`'s entry; `NONE`'s for an id past the table.
pub fn info(id: u8) -> &'static Info {
    MEDALS.get(id as usize).unwrap_or(&MEDALS[0])
}

/// The multi-kill rung for a chain of `n` kills (none under two).
pub fn multi(n: u8) -> Option<u8> {
    (n >= 2).then(|| MULTI[(n as usize - 2).min(MULTI.len() - 1)])
}

/// The spree rung reached at `n` kills without dying, if `n` is one.
pub fn spree(n: u16) -> Option<u8> {
    (n > 0 && n.is_multiple_of(SPREE_STEP))
        .then(|| SPREE.get(n as usize / SPREE_STEP as usize - 1).copied())
        .flatten()
}

/// The spree rung a fighter on `n` kills without dying stands on.
pub fn spree_rung(n: u16) -> Option<u8> {
    let k = (n / SPREE_STEP) as usize;
    (k > 0).then(|| SPREE[(k - 1).min(SPREE.len() - 1)])
}

/// A medal id a clip of the announcer's is filed under:
/// `sound/voice/<slug>.ogg`.
pub fn slug(id: u8) -> String {
    info(id)
        .name
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('_'),
            c if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ladders_read_in_order() {
        assert_eq!(multi(1), None);
        assert_eq!(multi(2), Some(DOUBLE_KILL));
        assert_eq!(multi(4), Some(OVERKILL));
        assert_eq!(multi(10), Some(KILLENNIUM));
        assert_eq!(multi(30), Some(KILLENNIUM));
        assert_eq!(spree(4), None);
        assert_eq!(spree(5), Some(KILLING_SPREE));
        assert_eq!(spree(7), None);
        assert_eq!(spree(40), Some(UNSTOPPABLE));
        assert_eq!(spree(45), None);
        assert_eq!(spree_rung(7), Some(KILLING_SPREE));
        assert_eq!(spree_rung(99), Some(UNSTOPPABLE));
        assert_eq!(slug(FROM_THE_GRAVE), "kill_from_the_grave");
        assert_eq!(slug(NO_SCOPE_360), "360_no_scope");
        for (i, m) in MEDALS.iter().enumerate().skip(1) {
            assert!(!m.name.is_empty(), "medal {i} has no name");
        }
    }
}

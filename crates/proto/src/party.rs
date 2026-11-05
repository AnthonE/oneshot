//! The menu's connection: a party, its friends and its voice.
//!
//! A page opens a second session beside any match, with
//! [`crate::MODE_LOBBY`], and keeps it while it plays: that is where its
//! party lives (who is in it, who leads, who may join), where its friends'
//! presence comes in, and where the party talks. When the leader starts a
//! game the server says [`crate::Msg::Go`] to every member, and each opens
//! a match session carrying the party's code, so the server seats them
//! together.

use crate::bits::{BitReader, BitWriter, WireError};
use crate::Name;

type R<T> = Result<T, WireError>;

/// Players one party holds: a Big Team side.
pub const MAX_PARTY: usize = 8;
/// A player id as Elo Pros writes it: `0x` and 40 hex (a wallet), or
/// `acct_` and 20 hex (a login with no wallet).
pub const MAX_PLAYER_ID: usize = 42;
/// Friends one session is told of: the first this many Elo Pros lists.
pub const MAX_FRIENDS: usize = 100;
/// A party's code: this many bits, never zero.
pub const CODE_BITS: u32 = 30;
/// Bits a member's tag takes: its voice frames carry it.
pub const TAG_BITS: u32 = 4;

/// Who may join a party: anyone with its code, the leader's friends (and
/// anyone invited), or only those the leader invites.
pub const PRIVACY_OPEN: u8 = 0;
pub const PRIVACY_FRIENDS: u8 = 1;
pub const PRIVACY_INVITE: u8 = 2;

/// [`crate::Msg::PartyAct`]'s acts.
/// Join the party whose code is `arg` (leaving the one you are in).
pub const ACT_JOIN: u8 = 1;
/// Leave for a party of your own.
pub const ACT_LEAVE: u8 = 2;
/// The leader starts mode `arg & 0xff`; `arg & 0x100`: the party alone,
/// bots in the other seats.
pub const ACT_START: u8 = 3;
/// The leader removes the member tagged `arg`.
pub const ACT_KICK: u8 = 4;
/// The leader sets who may join (`PRIVACY_*`).
pub const ACT_PRIVACY: u8 = 5;
/// The leader hands the lead to the member tagged `arg`.
pub const ACT_LEAD: u8 = 6;
/// What I am playing now (a mode, or [`PLAYING_NONE`]): what my party and
/// my friends see.
pub const ACT_PLAYING: u8 = 7;
/// Read my friends from Elo Pros again now.
pub const ACT_FRIENDS: u8 = 8;
pub const START_PRIVATE: u32 = 0x100;

/// A private match's own settings, in [`ACT_START`]'s arg above the mode
/// and [`START_PRIVATE`] (a public room plays its mode as it is): the map
/// (its id + 1 in bits 16..24; 0, the server picks), the score to win (an
/// index into [`SCORE_LIMITS`] in 24..28) and the clock (an index into
/// [`TIME_LIMITS_MIN`] in 28..32). Index 0 is the mode's own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Custom {
    pub map: Option<u8>,
    pub score: u8,
    pub time: u8,
}

/// Scores to win a private match may take (0: the mode's own; the last: no
/// limit, the clock decides).
pub const SCORE_LIMITS: [i32; 10] = [0, 1, 3, 5, 10, 15, 25, 50, 100, i32::MAX];
/// Clocks in minutes (0: the mode's own; the last: none).
pub const TIME_LIMITS_MIN: [u32; 8] = [0, 5, 10, 15, 20, 30, 60, u32::MAX];

impl Custom {
    pub fn from_arg(arg: u32) -> Custom {
        let map = (arg >> 16) as u8;
        Custom {
            map: map.checked_sub(1),
            score: ((arg >> 24) & 0xf) as u8 % SCORE_LIMITS.len() as u8,
            time: (arg >> 28) as u8 % TIME_LIMITS_MIN.len() as u8,
        }
    }

    pub fn to_arg(&self) -> u32 {
        let map = self.map.map_or(0, |m| m as u32 + 1) & 0xff;
        map << 16 | (self.score as u32 & 0xf) << 24 | (self.time as u32 & 0xf) << 28
    }

    /// Whether anything differs from the mode's own.
    pub fn any(&self) -> bool {
        *self != Custom::default()
    }

    /// The score to win, if not the mode's own.
    pub fn score_limit(&self) -> Option<i32> {
        (self.score > 0).then(|| SCORE_LIMITS[self.score as usize])
    }

    /// The clock in ms (0: none), if not the mode's own.
    pub fn time_limit_ms(&self) -> Option<u32> {
        (self.time > 0).then(|| match TIME_LIMITS_MIN[self.time as usize] {
            u32::MAX => 0,
            m => m * 60_000,
        })
    }
}

/// In no match: in the menu.
pub const PLAYING_NONE: u8 = 15;

/// Where a friend is ([`crate::Msg::Friend::at`]).
pub const AT_OFFLINE: u8 = 0;
/// On Elo Pros, not here.
pub const AT_ELSEWHERE: u8 = 1;
/// Here, in the menu.
pub const AT_HERE: u8 = 2;
/// Here, in a match.
pub const AT_MATCH: u8 = 3;
const AT_BITS: u32 = 2;

/// Bits a mode or [`PLAYING_NONE`] takes.
const PLAY_BITS: u32 = 4;

/// An Elo Pros player id, or none for a guest: `[0-9a-z_]` only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerId {
    pub len: u8,
    pub bytes: [u8; MAX_PLAYER_ID],
}

impl PlayerId {
    pub const NONE: PlayerId = PlayerId {
        len: 0,
        bytes: [0; MAX_PLAYER_ID],
    };

    fn ok(b: u8) -> bool {
        b.is_ascii_digit() || b.is_ascii_lowercase() || b == b'_'
    }

    /// None if it is too long or holds anything but `[0-9a-z_]`.
    pub fn new(s: &str) -> Option<PlayerId> {
        let b = s.as_bytes();
        if b.len() > MAX_PLAYER_ID || !b.iter().all(|&c| Self::ok(c)) {
            return None;
        }
        let mut p = PlayerId::NONE;
        p.bytes[..b.len()].copy_from_slice(b);
        p.len = b.len() as u8;
        Some(p)
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) fn pack(&self, w: &mut BitWriter) -> R<()> {
        w.write(self.len as u32, 6)?;
        for &b in &self.bytes[..self.len as usize] {
            w.write(b as u32, 7)?;
        }
        Ok(())
    }

    pub(crate) fn unpack(r: &mut BitReader) -> R<PlayerId> {
        let len = r.read(6)? as usize;
        if len > MAX_PLAYER_ID {
            return Err(WireError::Cap);
        }
        let mut p = PlayerId::NONE;
        for b in p.bytes[..len].iter_mut() {
            *b = r.read(7)? as u8;
            if !Self::ok(*b) {
                return Err(WireError::Malformed);
            }
        }
        p.len = len as u8;
        Ok(p)
    }
}

/// One member as the party sees them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Member {
    /// Theirs while they are in this party: their voice frames carry it.
    pub tag: u8,
    pub name: Name,
    /// Their Elo Pros id; none for a guest.
    pub player: PlayerId,
    /// Gone for a moment (a page reloading after a match): their place is
    /// kept a while.
    pub away: bool,
    /// What they are in: a mode, or [`PLAYING_NONE`].
    pub playing: u8,
}

impl Member {
    pub const EMPTY: Member = Member {
        tag: 0,
        name: Name {
            len: 0,
            bytes: [0; crate::MAX_NAME],
        },
        player: PlayerId::NONE,
        away: false,
        playing: PLAYING_NONE,
    };

    /// Signed in with a wallet: what ranked needs.
    pub fn wallet(&self) -> bool {
        self.player.as_str().starts_with("0x")
    }
}

/// A party as one of its members is told it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Party {
    /// Its code: what a link to it carries.
    pub code: u32,
    pub privacy: u8,
    /// The leader's tag, and the one being told's.
    pub leader: u8,
    pub me: u8,
    pub n: u8,
    pub members: [Member; MAX_PARTY],
}

impl Party {
    pub fn members(&self) -> &[Member] {
        &self.members[..(self.n as usize).min(MAX_PARTY)]
    }

    pub(crate) fn pack(&self, w: &mut BitWriter) -> R<()> {
        if self.n as usize > MAX_PARTY || self.code == 0 || self.code >> CODE_BITS != 0 {
            return Err(WireError::Range);
        }
        w.write(self.code, CODE_BITS)?;
        w.write(self.privacy as u32, 2)?;
        w.write(self.leader as u32, TAG_BITS)?;
        w.write(self.me as u32, TAG_BITS)?;
        w.write(self.n as u32, 4)?;
        for m in self.members() {
            w.write(m.tag as u32, TAG_BITS)?;
            crate::w_name(w, &m.name)?;
            m.player.pack(w)?;
            w.write_bit(m.away)?;
            w.write(m.playing as u32, PLAY_BITS)?;
        }
        Ok(())
    }

    pub(crate) fn unpack(r: &mut BitReader) -> R<Party> {
        let mut p = Party {
            code: r.read(CODE_BITS)?,
            privacy: r.read(2)? as u8,
            leader: r.read(TAG_BITS)? as u8,
            me: r.read(TAG_BITS)? as u8,
            n: r.read(4)? as u8,
            members: [Member::EMPTY; MAX_PARTY],
        };
        if p.code == 0 || p.privacy > PRIVACY_INVITE {
            return Err(WireError::Malformed);
        }
        if p.n as usize > MAX_PARTY {
            return Err(WireError::Cap);
        }
        for m in p.members[..p.n as usize].iter_mut() {
            *m = Member {
                tag: r.read(TAG_BITS)? as u8,
                name: crate::r_name(r)?,
                player: PlayerId::unpack(r)?,
                away: r.read_bit()?,
                playing: r.read(PLAY_BITS)? as u8,
            };
        }
        Ok(p)
    }
}

/// A party code as people read it: six of these 32, five bits each (no 0,
/// O, 1 or I to misread).
const CODE_ALPHABET: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

pub fn code_text(code: u32) -> String {
    (0..6)
        .rev()
        .map(|i| CODE_ALPHABET[((code >> (5 * i)) & 31) as usize] as char)
        .collect()
}

/// The code `s` names (case and spaces ignored, O read as 0 is refused).
pub fn code_parse(s: &str) -> Option<u32> {
    let mut code = 0u32;
    let mut n = 0;
    for c in s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'-') {
        let c = c.to_ascii_uppercase();
        let v = CODE_ALPHABET.iter().position(|&a| a == c)? as u32;
        code = code << 5 | v;
        n += 1;
        if n > 6 {
            return None;
        }
    }
    (n == 6 && code != 0).then_some(code)
}

pub(crate) fn w_at(w: &mut BitWriter, at: u8) -> R<()> {
    if at > AT_MATCH {
        return Err(WireError::Range);
    }
    w.write(at as u32, AT_BITS)
}

pub(crate) fn r_at(r: &mut BitReader) -> R<u8> {
    Ok(r.read(AT_BITS)? as u8)
}

pub(crate) fn w_play(w: &mut BitWriter, p: u8) -> R<()> {
    w.write(p as u32, PLAY_BITS)
}

pub(crate) fn r_play(r: &mut BitReader) -> R<u8> {
    Ok(r.read(PLAY_BITS)? as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_reads_back_and_never_needs_a_lookalike() {
        for code in [1, 0x2aaa_aaaa, (1 << CODE_BITS) - 1, 123_456] {
            let t = code_text(code);
            assert_eq!(t.len(), 6);
            assert_eq!(code_parse(&t), Some(code), "{t}");
            assert_eq!(code_parse(&t.to_lowercase()), Some(code));
        }
        assert_eq!(code_parse("ABC"), None);
        assert_eq!(code_parse("ABCDEFG"), None);
        assert_eq!(code_parse("ABCDE0"), None, "0 isn't a letter of a code");
        assert_eq!(code_parse("222222"), None, "zero is no code");
    }
}

#[cfg(test)]
mod custom_tests {
    use super::*;

    #[test]
    fn a_private_matchs_settings_ride_the_start_and_keep_the_mode() {
        let c = Custom {
            map: Some(5),
            score: 2,
            time: 7,
        };
        let arg = crate::MODE_CTF as u32 | START_PRIVATE | c.to_arg();
        assert_eq!(arg & 0xff, crate::MODE_CTF as u32);
        assert_ne!(arg & START_PRIVATE, 0);
        assert_eq!(Custom::from_arg(arg), c);
        assert_eq!(c.score_limit(), Some(3));
        assert_eq!(c.time_limit_ms(), Some(0), "no clock");
        assert_eq!(Custom::from_arg(START_PRIVATE), Custom::default());
        assert!(!Custom::default().any());
        // Out-of-range indexes never index past the tables.
        let wild = Custom::from_arg(0xffff_0000);
        assert!((wild.score as usize) < SCORE_LIMITS.len() && (wild.time as usize) < TIME_LIMITS_MIN.len());
    }
}

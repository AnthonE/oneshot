//! Parties: who plays together, who leads, who may join, and who hears
//! whom (proto's `party`, docs/DESIGN.md §11).
//!
//! Every lobby session is in a party, of one if nobody else: the Halo 2
//! lobby, where you are always in your own until you join someone's. A
//! party's code is its link. Who may join is the leader's call: anyone with
//! the code, the leader's friends (read from Elo Pros, `friends.rs`), or
//! only those the leader invites. A member whose page goes away (it reloads
//! after every match) keeps their place for [`GRACE`], so the party is
//! still there when they are back; past that they are dropped and, if they
//! led, the lead passes to whoever joined first.
//!
//! When the leader starts, every member present is told [`Msg::Go`] and
//! opens a match session carrying the code; the sim seats them together
//! (`lib.rs`). Voice goes to the talker's party and nobody else.
//!
//! No network and no clock here: the net layer calls in with what
//! happened and sends what each call hands back.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use proto::party::{self as pp, Member, PlayerId, MAX_PARTY, PLAYING_NONE};
use proto::{Msg, Name};

use crate::auth::Who;

/// How long a member who went away keeps their place.
pub const GRACE: Duration = Duration::from_secs(90);
/// Tags a party hands out (the wire's `TAG_BITS`).
const TAGS: u8 = 16;
/// Invites one member sends a minute, and lobby sessions one server keeps.
pub const INVITES_PER_MIN: usize = 10;
pub const MAX_LOBBIES: usize = 4096;

/// What each call hands back: stream messages for sessions.
pub type Outbox = Vec<(u64, Msg)>;

#[derive(Clone, Debug)]
struct Seat {
    id: String,
    name: Name,
    tag: u8,
    /// None while away; since when.
    session: Option<u64>,
    away_since: Option<Instant>,
    playing: u8,
    /// Shown to the party: an Elo Pros id, or none for a guest.
    player: PlayerId,
}

#[derive(Clone, Debug)]
struct PartyState {
    privacy: u8,
    /// The leader's id (a seat's).
    leader: String,
    seats: Vec<Seat>,
    invited: BTreeSet<String>,
    kicked: BTreeSet<String>,
    /// What the leader last started, and whether the party plays alone.
    started: Option<(u8, bool)>,
}

#[derive(Clone, Debug)]
struct Lobbyist {
    who: Who,
    name: Name,
    party: u32,
    /// Their friends' ids, as Elo Pros last told us.
    friends: BTreeSet<String>,
    invites: Vec<Instant>,
}

/// What a member's match hello is checked against: their party, if they
/// are in it, its size and how it plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seating {
    pub code: u32,
    pub size: u8,
    pub private: bool,
}

#[derive(Default)]
pub struct Parties {
    parties: BTreeMap<u32, PartyState>,
    sessions: BTreeMap<u64, Lobbyist>,
}

/// Whether a party of `n` fits `mode`: ranked Duel and the bot are one
/// player's; Doubles two at most, with wallets; the rest a side's worth.
pub fn fits(mode: u8, n: usize, wallets: usize) -> bool {
    match mode {
        proto::MODE_CASUAL | proto::MODE_DUEL => n == 1,
        proto::MODE_DOUBLES => n <= 2 && wallets == n,
        proto::MODE_CUP => wallets == n,
        proto::MODE_FFA | proto::MODE_GUN_GAME => n <= 8,
        proto::MODE_BIG_TEAM => n <= MAX_PARTY,
        _ => false,
    }
}

fn random_code() -> u32 {
    let mut b = [0u8; 4];
    let _ = getrandom::fill(&mut b);
    u32::from_le_bytes(b) & ((1 << pp::CODE_BITS) - 1)
}

fn player_of(who: &Who) -> PlayerId {
    if who.signed() {
        PlayerId::new(&who.id).unwrap_or(PlayerId::NONE)
    } else {
        PlayerId::NONE
    }
}

impl Parties {
    fn new_code(&self) -> u32 {
        loop {
            let c = random_code();
            if c != 0 && !self.parties.contains_key(&c) {
                return c;
            }
        }
    }

    pub fn lobbies(&self) -> usize {
        self.sessions.len()
    }

    /// A lobby session opened, for `who`, whose friends (Elo Pros says)
    /// are `friends`. It takes back a place it held in a party (a page back
    /// from a match), else joins `want` if it may, else gets a party of its
    /// own. A `want` no party here has is made here, the first back leading
    /// it: a deploy moves every page to the new slot with its code. A
    /// session already open for the same player is let go (another tab
    /// took over): it is in the second list.
    pub fn connect(
        &mut self,
        session: u64,
        who: Who,
        name: Name,
        want: u32,
        friends: Vec<String>,
        now: Instant,
    ) -> (Outbox, Vec<u64>) {
        let mut out = Outbox::new();
        let replaced: Vec<u64> = self
            .sessions
            .iter()
            .filter(|(_, l)| l.who.id == who.id)
            .map(|(&s, _)| s)
            .collect();
        for &s in &replaced {
            out.extend(self.disconnect(s, now));
        }
        self.sessions.insert(
            session,
            Lobbyist {
                who: who.clone(),
                name,
                party: 0,
                friends: friends.into_iter().collect(),
                invites: Vec::new(),
            },
        );
        // A place kept for them: in the party they ask for, or any.
        let held = self
            .parties
            .iter()
            .filter(|(c, p)| (want == 0 || **c == want) && p.seats.iter().any(|s| s.id == who.id))
            .map(|(&c, _)| c)
            .next();
        if let Some(code) = held {
            let p = self.parties.get_mut(&code).unwrap();
            if let Some(seat) = p.seats.iter_mut().find(|s| s.id == who.id) {
                seat.session = Some(session);
                seat.away_since = None;
                seat.name = name;
                seat.player = player_of(&who);
                // Back from a match: in the menu until it says otherwise.
                seat.playing = PLAYING_NONE;
            }
            self.sessions.get_mut(&session).unwrap().party = code;
            out.extend(self.tell(code));
            return (out, replaced);
        }
        if want != 0 && want >> pp::CODE_BITS == 0 && !self.parties.contains_key(&want) {
            out.extend(self.found(session, want));
            return (out, replaced);
        }
        if want != 0 {
            match self.may_join(session, want) {
                Ok(()) => {
                    out.extend(self.seat(session, want));
                    return (out, replaced);
                }
                Err(why) => out.push((session, Msg::Refuse { reason: why })),
            }
        }
        out.extend(self.alone(session));
        (out, replaced)
    }

    /// Their friends' ids, as Elo Pros has them now.
    pub fn set_friends(&mut self, session: u64, ids: impl IntoIterator<Item = String>) {
        if let Some(l) = self.sessions.get_mut(&session) {
            l.friends = ids.into_iter().collect();
        }
    }

    /// The session signed in: it is this player now, in the same seat.
    pub fn sign_in(&mut self, session: u64, who: Who) -> Outbox {
        let Some(l) = self.sessions.get_mut(&session) else {
            return Outbox::new();
        };
        let old = std::mem::replace(&mut l.who, who.clone());
        let code = l.party;
        if let Some(p) = self.parties.get_mut(&code) {
            if p.leader == old.id {
                p.leader = who.id.clone();
            }
            if let Some(seat) = p.seats.iter_mut().find(|s| s.id == old.id) {
                seat.id = who.id.clone();
                seat.player = player_of(&who);
            }
        }
        self.tell(code)
    }

    /// The page went away: its place is kept for [`GRACE`].
    pub fn disconnect(&mut self, session: u64, now: Instant) -> Outbox {
        let Some(l) = self.sessions.remove(&session) else {
            return Outbox::new();
        };
        let Some(p) = self.parties.get_mut(&l.party) else {
            return Outbox::new();
        };
        if let Some(seat) = p.seats.iter_mut().find(|s| s.session == Some(session)) {
            seat.session = None;
            seat.away_since = Some(now);
        }
        // A party of one, gone: nobody to keep it for but them.
        self.tell(l.party)
    }

    /// Drops those away past [`GRACE`]; a party nobody is left in ends.
    pub fn sweep(&mut self, now: Instant) -> Outbox {
        let mut changed = Vec::new();
        for (&code, p) in self.parties.iter_mut() {
            let before = p.seats.len();
            p.seats
                .retain(|s| s.away_since.is_none_or(|t| now.duration_since(t) < GRACE));
            if p.seats.len() != before {
                changed.push(code);
            }
        }
        let mut out = Outbox::new();
        for code in changed {
            out.extend(self.settle(code));
        }
        out
    }

    /// After a seat went: the lead passes on if it left, and a party with
    /// nobody in it ends.
    fn settle(&mut self, code: u32) -> Outbox {
        let Some(p) = self.parties.get_mut(&code) else {
            return Outbox::new();
        };
        if p.seats.is_empty() {
            self.parties.remove(&code);
            return Outbox::new();
        }
        if !p.seats.iter().any(|s| s.id == p.leader) {
            let next = p.seats.iter().find(|s| s.session.is_some()).unwrap_or(&p.seats[0]);
            p.leader = next.id.clone();
        }
        self.tell(code)
    }

    fn may_join(&self, session: u64, code: u32) -> Result<(), u8> {
        let l = self.sessions.get(&session).ok_or(proto::REFUSE_NO_PARTY)?;
        let p = self.parties.get(&code).ok_or(proto::REFUSE_NO_PARTY)?;
        let id = &l.who.id;
        if p.seats.iter().any(|s| &s.id == id) {
            return Ok(());
        }
        if p.kicked.contains(id) {
            return Err(proto::REFUSE_KICKED);
        }
        if p.seats.len() >= MAX_PARTY {
            return Err(proto::REFUSE_PARTY_FULL);
        }
        let invited = p.invited.contains(id);
        match p.privacy {
            pp::PRIVACY_FRIENDS if !invited && !(l.who.signed() && l.friends.contains(&p.leader)) => {
                Err(proto::REFUSE_FRIENDS_ONLY)
            }
            pp::PRIVACY_INVITE if !invited => Err(proto::REFUSE_INVITE_ONLY),
            _ => Ok(()),
        }
    }

    /// Takes `session` out of its party (the rest are told).
    fn unseat(&mut self, session: u64) -> Outbox {
        let Some(l) = self.sessions.get_mut(&session) else {
            return Outbox::new();
        };
        let code = std::mem::take(&mut l.party);
        if let Some(p) = self.parties.get_mut(&code) {
            p.seats.retain(|s| s.session != Some(session));
        }
        self.settle(code)
    }

    /// Seats `session` in party `code` (checked already).
    fn seat(&mut self, session: u64, code: u32) -> Outbox {
        let mut out = self.unseat(session);
        let l = &self.sessions[&session];
        let (who, name) = (l.who.clone(), l.name);
        let Some(p) = self.parties.get_mut(&code) else {
            return out;
        };
        if !p.seats.iter().any(|s| s.id == who.id) {
            let tag = (0..TAGS).find(|t| p.seats.iter().all(|s| s.tag != *t)).unwrap_or(0);
            p.seats.push(Seat {
                id: who.id.clone(),
                name,
                tag,
                session: Some(session),
                away_since: None,
                playing: PLAYING_NONE,
                player: player_of(&who),
            });
        }
        if let Some(l) = self.sessions.get_mut(&session) {
            l.party = code;
        }
        out.extend(self.tell(code));
        out
    }

    /// A fresh party of one for `session`.
    fn alone(&mut self, session: u64) -> Outbox {
        let code = self.new_code();
        self.found(session, code)
    }

    /// A party `code`, led by `session`.
    fn found(&mut self, session: u64, code: u32) -> Outbox {
        let leader = self.sessions[&session].who.id.clone();
        self.parties.insert(
            code,
            PartyState {
                // As Elo Pros' lobbies: anyone with the link, until the
                // leader says otherwise.
                privacy: pp::PRIVACY_OPEN,
                leader,
                seats: Vec::new(),
                invited: BTreeSet::new(),
                kicked: BTreeSet::new(),
                started: None,
            },
        );
        self.seat(session, code)
    }

    /// Every member present told the party as it is now.
    fn tell(&self, code: u32) -> Outbox {
        let Some(p) = self.parties.get(&code) else {
            return Outbox::new();
        };
        let leader = p.seats.iter().find(|s| s.id == p.leader).map_or(0, |s| s.tag);
        let mut view = pp::Party {
            code,
            privacy: p.privacy,
            leader,
            me: 0,
            n: p.seats.len().min(MAX_PARTY) as u8,
            members: [Member::EMPTY; MAX_PARTY],
        };
        for (m, s) in view.members.iter_mut().zip(&p.seats) {
            *m = Member {
                tag: s.tag,
                name: s.name,
                player: s.player,
                away: s.session.is_none(),
                playing: s.playing,
            };
        }
        p.seats
            .iter()
            .filter_map(|s| s.session.map(|id| (id, s.tag)))
            .map(|(id, tag)| (id, Msg::Party(pp::Party { me: tag, ..view })))
            .collect()
    }

    fn leading(&self, session: u64) -> Result<(u32, &PartyState), u8> {
        let l = self.sessions.get(&session).ok_or(proto::REFUSE_NO_PARTY)?;
        let p = self.parties.get(&l.party).ok_or(proto::REFUSE_NO_PARTY)?;
        if p.leader != l.who.id {
            return Err(proto::REFUSE_NOT_LEADER);
        }
        Ok((l.party, p))
    }

    /// A party act from `session`. A refusal goes back to it alone.
    pub fn act(&mut self, session: u64, act: u8, arg: u32) -> Outbox {
        match self.try_act(session, act, arg) {
            Ok(out) => out,
            Err(reason) => vec![(session, Msg::Refuse { reason })],
        }
    }

    fn try_act(&mut self, session: u64, act: u8, arg: u32) -> Result<Outbox, u8> {
        match act {
            pp::ACT_JOIN => {
                self.may_join(session, arg)?;
                Ok(self.seat(session, arg))
            }
            pp::ACT_LEAVE => {
                let alone = self
                    .sessions
                    .get(&session)
                    .and_then(|l| self.parties.get(&l.party))
                    .is_some_and(|p| p.seats.len() == 1);
                if alone {
                    return Ok(Outbox::new());
                }
                let mut out = self.unseat(session);
                out.extend(self.alone(session));
                Ok(out)
            }
            pp::ACT_START => {
                let (code, p) = self.leading(session)?;
                let mode = (arg & 0xff) as u8;
                let private = arg & pp::START_PRIVATE != 0;
                let here: Vec<&Seat> = p.seats.iter().filter(|s| s.session.is_some()).collect();
                let wallets = here.iter().filter(|s| s.player.as_str().starts_with("0x")).count();
                if !fits(mode, here.len(), wallets) {
                    return Err(proto::REFUSE_PARTY_MODE);
                }
                let out = here
                    .iter()
                    .filter_map(|s| s.session)
                    .map(|id| (id, Msg::Go { mode, private }))
                    .collect();
                self.parties.get_mut(&code).unwrap().started = Some((mode, private));
                Ok(out)
            }
            pp::ACT_KICK => {
                let (code, p) = self.leading(session)?;
                let seat = p
                    .seats
                    .iter()
                    .find(|s| s.tag as u32 == arg && s.id != p.leader)
                    .cloned()
                    .ok_or(proto::REFUSE_NO_PARTY)?;
                let p = self.parties.get_mut(&code).unwrap();
                p.kicked.insert(seat.id.clone());
                p.invited.remove(&seat.id);
                let mut out = Outbox::new();
                if let Some(s) = seat.session {
                    out.push((
                        s,
                        Msg::Refuse {
                            reason: proto::REFUSE_KICKED,
                        },
                    ));
                    out.extend(self.unseat(s));
                    out.extend(self.alone(s));
                } else {
                    self.parties.get_mut(&code).unwrap().seats.retain(|s| s.id != seat.id);
                    out.extend(self.settle(code));
                }
                Ok(out)
            }
            pp::ACT_PRIVACY => {
                let (code, _) = self.leading(session)?;
                if arg > pp::PRIVACY_INVITE as u32 {
                    return Err(proto::REFUSE_PARTY_MODE);
                }
                self.parties.get_mut(&code).unwrap().privacy = arg as u8;
                Ok(self.tell(code))
            }
            pp::ACT_LEAD => {
                let (code, p) = self.leading(session)?;
                let to = p
                    .seats
                    .iter()
                    .find(|s| s.tag as u32 == arg)
                    .map(|s| s.id.clone())
                    .ok_or(proto::REFUSE_NO_PARTY)?;
                self.parties.get_mut(&code).unwrap().leader = to;
                Ok(self.tell(code))
            }
            pp::ACT_PLAYING => {
                let code = self.sessions.get(&session).map_or(0, |l| l.party);
                let playing = if arg < PLAYING_NONE as u32 {
                    arg as u8
                } else {
                    PLAYING_NONE
                };
                let p = self.parties.get_mut(&code).ok_or(proto::REFUSE_NO_PARTY)?;
                if let Some(s) = p.seats.iter_mut().find(|s| s.session == Some(session)) {
                    if s.playing == playing {
                        return Ok(Outbox::new());
                    }
                    s.playing = playing;
                }
                Ok(self.tell(code))
            }
            _ => Err(proto::REFUSE_PARTY_MODE),
        }
    }

    /// `session` asks `player` into its party: a friend of theirs (Elo
    /// Pros says), let in past who-may-join and told if they're here.
    pub fn invite(&mut self, session: u64, player: &str, now: Instant) -> Outbox {
        let refuse = |reason| vec![(session, Msg::Refuse { reason })];
        let Some(l) = self.sessions.get_mut(&session) else {
            return Outbox::new();
        };
        if !l.who.signed() || !l.friends.contains(player) {
            return refuse(proto::REFUSE_NOT_FRIEND);
        }
        l.invites.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
        if l.invites.len() >= INVITES_PER_MIN {
            return Outbox::new();
        }
        l.invites.push(now);
        let (code, from) = (l.party, l.name);
        let Some(p) = self.parties.get_mut(&code) else {
            return Outbox::new();
        };
        p.invited.insert(player.to_string());
        p.kicked.remove(player);
        self.sessions
            .iter()
            .filter(|(_, l)| l.who.id == player && l.party != code)
            .map(|(&s, _)| (s, Msg::Invited { from, code }))
            .collect()
    }

    /// Where `session`'s friends are, as rows for its page: here (in the
    /// menu or a match, and the party code if it may join), else what Elo
    /// Pros said (`elsewhere`: online there).
    pub fn friend_rows(&self, session: u64, rows: &[crate::elo::FriendRow]) -> Vec<Msg> {
        let Some(me) = self.sessions.get(&session) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (i, f) in rows.iter().take(pp::MAX_FRIENDS).enumerate() {
            let here = self.sessions.values().find(|l| l.who.id == f.id);
            let (at, party, size, playing) = match here {
                Some(l) => {
                    let p = self.parties.get(&l.party);
                    let seat = p.and_then(|p| p.seats.iter().find(|s| s.id == l.who.id));
                    let playing = seat.map_or(PLAYING_NONE, |s| s.playing);
                    let size = p.map_or(1, |p| p.seats.len()) as u8;
                    // Joinable by me: theirs isn't mine, has room, and lets me.
                    let open = p.is_some_and(|p| {
                        l.party != me.party
                            && p.seats.len() < MAX_PARTY
                            && !p.kicked.contains(&me.who.id)
                            && match p.privacy {
                                pp::PRIVACY_OPEN => true,
                                pp::PRIVACY_FRIENDS => me.friends.contains(&p.leader) || p.invited.contains(&me.who.id),
                                _ => p.invited.contains(&me.who.id),
                            }
                    });
                    let at = if playing == PLAYING_NONE {
                        pp::AT_HERE
                    } else {
                        pp::AT_MATCH
                    };
                    (at, if open { l.party } else { 0 }, size, playing)
                }
                None if f.online => (pp::AT_ELSEWHERE, 0, 0, PLAYING_NONE),
                None => (pp::AT_OFFLINE, 0, 0, PLAYING_NONE),
            };
            out.push(Msg::Friend {
                i: i as u8,
                name: Name::new(&f.name),
                player: PlayerId::new(&f.id).unwrap_or(PlayerId::NONE),
                at,
                party,
                size: size.min(15),
                playing,
            });
        }
        out
    }

    /// The tag `session` talks under and the sessions that hear it: the
    /// rest of its party, present.
    pub fn voice(&self, session: u64) -> Option<(u8, Vec<u64>)> {
        let l = self.sessions.get(&session)?;
        let p = self.parties.get(&l.party)?;
        let tag = p.seats.iter().find(|s| s.session == Some(session))?.tag;
        let to = p
            .seats
            .iter()
            .filter_map(|s| s.session)
            .filter(|&s| s != session)
            .collect();
        Some((tag, to))
    }

    /// For a match hello naming `code` from `id`: their party, if they
    /// are in it and its leader started `mode`.
    pub fn seating(&self, code: u32, id: &str, mode: u8) -> Option<Seating> {
        let p = self.parties.get(&code)?;
        if !p.seats.iter().any(|s| s.id == id) {
            return None;
        }
        let (m, private) = p.started?;
        (m == mode).then_some(Seating {
            code,
            // Those away are on their way (a page reloading into the match).
            size: p.seats.len().max(1) as u8,
            private,
        })
    }

    /// Who `session` is.
    pub fn who(&self, session: u64) -> Option<&Who> {
        self.sessions.get(&session).map(|l| &l.who)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn who(n: u8) -> Who {
        let id = format!("0x{:040x}", n);
        Who {
            id: id.clone(),
            account: Some(format!("acct_{:020x}", n)),
            wallet: Some(id),
        }
    }

    fn party_of(out: &Outbox, s: u64) -> Option<pp::Party> {
        out.iter().rev().find_map(|(to, m)| match m {
            Msg::Party(p) if *to == s => Some(*p),
            _ => None,
        })
    }

    fn refused(out: &Outbox, s: u64) -> Option<u8> {
        out.iter().find_map(|(to, m)| match m {
            Msg::Refuse { reason } if *to == s => Some(*reason),
            _ => None,
        })
    }

    #[test]
    fn everyone_starts_alone_and_a_code_brings_a_friend_in() {
        let t = Instant::now();
        let mut ps = Parties::default();
        let (out, _) = ps.connect(1, who(1), Name::new("lead"), 0, vec![], t);
        let mine = party_of(&out, 1).unwrap();
        assert_eq!((mine.n, mine.leader, mine.me), (1, 0, 0));
        assert_eq!(mine.privacy, pp::PRIVACY_OPEN, "a new party takes anyone with its link");
        ps.act(1, pp::ACT_PRIVACY, pp::PRIVACY_FRIENDS as u32);

        // A stranger is turned away from a friends-only party; a friend
        // (Elo Pros says) gets in.
        let (out, _) = ps.connect(2, who(2), Name::new("stranger"), mine.code, vec![], t);
        assert_eq!(refused(&out, 2), Some(proto::REFUSE_FRIENDS_ONLY));
        assert_ne!(party_of(&out, 2).unwrap().code, mine.code, "a party of their own");
        ps.set_friends(2, [who(1).id]);
        let out = ps.act(2, pp::ACT_JOIN, mine.code);
        let p = party_of(&out, 1).unwrap();
        assert_eq!(p.n, 2);
        assert_eq!(party_of(&out, 2).unwrap().me, p.members[1].tag);

        // Open: anyone with the code; invite only: only the invited.
        ps.act(1, pp::ACT_PRIVACY, pp::PRIVACY_OPEN as u32);
        let (out, _) = ps.connect(3, who(3), Name::new("open"), mine.code, vec![], t);
        assert_eq!(party_of(&out, 3).unwrap().code, mine.code);
        ps.act(1, pp::ACT_PRIVACY, pp::PRIVACY_INVITE as u32);
        let (out, _) = ps.connect(4, who(4), Name::new("late"), mine.code, vec![], t);
        assert_eq!(refused(&out, 4), Some(proto::REFUSE_INVITE_ONLY));
        // Only a friend can be invited, and the invite reaches them.
        assert_eq!(refused(&ps.invite(1, &who(4).id, t), 1), Some(proto::REFUSE_NOT_FRIEND));
        ps.set_friends(1, [who(2).id, who(4).id]);
        let out = ps.invite(1, &who(4).id, t);
        assert!(matches!(out[..], [(4, Msg::Invited { code, .. })] if code == mine.code));
        let out = ps.act(4, pp::ACT_JOIN, mine.code);
        assert_eq!(party_of(&out, 1).unwrap().n, 4);
        // Non-leaders can't run it.
        assert_eq!(
            refused(&ps.act(2, pp::ACT_PRIVACY, 0), 2),
            Some(proto::REFUSE_NOT_LEADER)
        );
    }

    #[test]
    fn a_page_back_from_a_match_takes_its_place_and_the_lead_waits_for_it() {
        let t = Instant::now();
        let mut ps = Parties::default();
        let (out, _) = ps.connect(1, who(1), Name::new("lead"), 0, vec![], t);
        let code = party_of(&out, 1).unwrap().code;
        ps.act(1, pp::ACT_PRIVACY, pp::PRIVACY_INVITE as u32);
        ps.set_friends(1, [who(2).id]);
        ps.invite(1, &who(2).id, t);
        ps.connect(2, who(2), Name::new("two"), code, vec![], t);
        // The leader's page reloads: away, still leading.
        let out = ps.disconnect(1, t);
        let p = party_of(&out, 2).unwrap();
        assert!(p.members[0].away);
        assert_eq!(p.leader, p.members[0].tag);
        assert!(ps.sweep(t + GRACE / 2).is_empty());
        // Back, on a new session, with no code at all: the same seat.
        let (out, _) = ps.connect(9, who(1), Name::new("lead"), 0, vec![], t + GRACE / 2);
        let p = party_of(&out, 9).unwrap();
        assert_eq!((p.code, p.n, p.me, p.leader), (code, 2, 0, 0));
        assert!(!p.members[0].away);
        // Gone for good: the lead passes on.
        ps.disconnect(9, t + GRACE);
        let out = ps.sweep(t + 3 * GRACE);
        let p = party_of(&out, 2).unwrap();
        assert_eq!((p.n, p.leader), (1, p.members[0].tag));
    }

    #[test]
    fn the_leader_starts_for_everyone_present_if_the_party_fits() {
        let t = Instant::now();
        let mut ps = Parties::default();
        let (out, _) = ps.connect(1, who(1), Name::new("lead"), 0, vec![], t);
        let code = party_of(&out, 1).unwrap().code;
        ps.act(1, pp::ACT_PRIVACY, pp::PRIVACY_OPEN as u32);
        let guest = Who::guest();
        ps.connect(2, guest.clone(), Name::new("guest"), code, vec![], t);
        assert_eq!(
            refused(&ps.act(1, pp::ACT_START, proto::MODE_DUEL as u32), 1),
            Some(proto::REFUSE_PARTY_MODE),
            "a duel is one player's"
        );
        assert_eq!(
            refused(&ps.act(1, pp::ACT_START, proto::MODE_DOUBLES as u32), 1),
            Some(proto::REFUSE_PARTY_MODE),
            "ranked needs everyone's wallet"
        );
        let out = ps.act(1, pp::ACT_START, proto::MODE_BIG_TEAM as u32 | pp::START_PRIVATE);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|(_, m)| *m
            == Msg::Go {
                mode: proto::MODE_BIG_TEAM,
                private: true
            }));
        assert_eq!(
            ps.seating(code, &guest.id, proto::MODE_BIG_TEAM),
            Some(Seating {
                code,
                size: 2,
                private: true
            })
        );
        assert_eq!(
            ps.seating(code, &guest.id, proto::MODE_FFA),
            None,
            "another mode: alone"
        );
        assert_eq!(ps.seating(code, "0xnobody", proto::MODE_BIG_TEAM), None);
        // Voice: each hears the other.
        assert_eq!(ps.voice(1).unwrap().1, vec![2]);
        assert_eq!(ps.voice(2).unwrap().1, vec![1]);
    }

    #[test]
    fn a_kick_keeps_them_out_and_a_second_tab_takes_over() {
        let t = Instant::now();
        let mut ps = Parties::default();
        let (out, _) = ps.connect(1, who(1), Name::new("lead"), 0, vec![], t);
        let code = party_of(&out, 1).unwrap().code;
        ps.act(1, pp::ACT_PRIVACY, pp::PRIVACY_OPEN as u32);
        let (out, _) = ps.connect(2, who(2), Name::new("two"), code, vec![], t);
        let tag = party_of(&out, 2).unwrap().me;
        let out = ps.act(1, pp::ACT_KICK, tag as u32);
        assert_eq!(refused(&out, 2), Some(proto::REFUSE_KICKED));
        assert_eq!(party_of(&out, 1).unwrap().n, 1);
        assert_eq!(refused(&ps.act(2, pp::ACT_JOIN, code), 2), Some(proto::REFUSE_KICKED));
        // The leader opens a second tab: the first is let go, the seat moves.
        let (out, replaced) = ps.connect(5, who(1), Name::new("lead"), 0, vec![], t);
        assert_eq!(replaced, vec![1]);
        assert_eq!(party_of(&out, 5).unwrap().code, code);
    }
}

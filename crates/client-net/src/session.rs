//! A connection's game side, whatever carries it (docs/DESIGN.md §2): the
//! hello, the welcome that starts the netcode (and refuses a map this build
//! doesn't know), the salt checked at the reveal, snapshots in and inputs
//! out. The browser's WebTransport (`web.rs`) and the native wtransport
//! (`native.rs`) only move bytes for it; neither decides anything here.

use proto::{decode_snapshot, encode_input, Msg, Name, Pass, MAX_DATAGRAM, PROTO_VER};
use sim::input::Input;
use sim::map::Map;

use crate::clip::Clipper;
use crate::core::NetCore;

/// Where a session's datagrams go. Sending never waits: a full queue drops
/// the datagram (the next carries every unacked input again).
pub trait Wire {
    /// Whether it went (false: dropped).
    fn send_datagram(&mut self, payload: &[u8]) -> bool;
}

/// Why the server said no, as a player reads it (`Msg::Refuse`).
pub fn refuse_why(reason: u8) -> &'static str {
    match reason {
        proto::REFUSE_VERSION => "this page is out of date: reload it",
        proto::REFUSE_PASS => "Elo Pros' sign-in didn't check out: sign in again",
        proto::REFUSE_NEEDS_WALLET => "ranked needs a wallet linked to your Elo Pros account",
        proto::REFUSE_BUSY => "that wallet is already queued or playing",
        proto::REFUSE_BANNED => "that wallet is banned from ranked after a review: casual is open",
        proto::REFUSE_FULL => "the server is full or restarting: try again in a minute",
        proto::REFUSE_NO_PARTY => "that party has broken up",
        proto::REFUSE_PARTY_FULL => "that party is full",
        proto::REFUSE_FRIENDS_ONLY => "that party is for the leader's friends",
        proto::REFUSE_INVITE_ONLY => "that party is invite only: ask its leader to invite you",
        proto::REFUSE_KICKED => "the party's leader removed you",
        proto::REFUSE_NOT_LEADER => "only the party's leader can do that",
        proto::REFUSE_PARTY_MODE => "the party doesn't fit that game: too many, or ranked without everyone's wallet",
        proto::REFUSE_NOT_FRIEND => "you can invite your Elo Pros friends once you're signed in",
        _ => "the server is full",
    }
}

pub struct Session {
    pub core: NetCore,
    /// The welcome has come: a room, a seat, a clock.
    pub ready: bool,
    /// The map the welcome named.
    pub map_id: u8,
    /// Welcomes so far: a cup's entrant gets one a heat.
    pub welcomes: u32,
    /// Datagrams the wire dropped.
    pub dropped: u32,
    /// Stream messages for the app, until it takes them.
    out: Vec<Msg>,
    /// A tape's session (`Session::replay`): its own fighter is drawn off
    /// the snapshots like everyone else's (`NetCore::watch`) and nothing
    /// it makes is sent.
    pub watch: bool,
    /// The last minute it was sent, for the clip key (`crate::clip`). On
    /// for every live session; a replay keeps none.
    clip: Option<Box<Clipper>>,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Session {
        Session {
            // A stand-in until the welcome names the seat.
            core: NetCore::new(0, 0, 0, sim::world::Rules::duel(), Map::proving_grounds()),
            ready: false,
            map_id: 0,
            welcomes: 0,
            dropped: 0,
            out: Vec::new(),
            watch: false,
            clip: Some(Box::default()),
        }
    }

    /// A session with no server behind it, for a tape (`tape::Replay`
    /// feeds it): it watches, and keeps no clip of its own.
    pub fn replay() -> Session {
        Session {
            watch: true,
            clip: None,
            ..Session::new()
        }
    }

    /// Keep at least `secs` seconds for the tape, not the clip key's minute
    /// (a recorder's session).
    pub fn clip_keep_s(&mut self, secs: f64) {
        if let Some(c) = self.clip.as_mut() {
            c.keep_s(secs);
        }
    }

    /// No clip key's memory (a test's or a tool's session).
    pub fn no_clips(&mut self) {
        self.clip = None;
    }

    /// A tape of at least the last `want_s` seconds (and the lead a film
    /// needs), or as much as is held; `None` with clips off or before the
    /// first snapshot (`clip::Clipper::tape`).
    pub fn clip_tape(&self, want_s: f64) -> Option<Vec<u8>> {
        self.clip.as_ref()?.tape(want_s)
    }

    /// Seconds a clip can reach back over now; 0 with clips off.
    pub fn clip_held_s(&self) -> f64 {
        self.clip.as_ref().map_or(0.0, |c| c.held_s())
    }

    /// The first message on the stream: who, what to play, and the party
    /// it comes with (its code; 0 for none).
    pub fn hello(name: &str, pass: Pass, mode: u8, look: proto::Look, party: u32) -> Msg {
        Msg::Hello {
            proto: PROTO_VER,
            name: Name::new(name),
            pass,
            mode,
            skin: look.skin,
            emblem: look.emblem,
            party,
        }
    }

    /// A stream message: a welcome starts the netcode, a reveal is checked
    /// against the welcome's commitment, the rest wait for the app.
    pub fn on_msg(&mut self, m: Msg) {
        if let Some(c) = self.clip.as_mut() {
            match m {
                Msg::Welcome { .. } => c.on_welcome(m),
                _ => c.on_msg(&m),
            }
        }
        match m {
            Msg::Welcome {
                slot,
                team,
                tick,
                rules,
                salt_commit,
                map,
                ..
            } => {
                // A map this build doesn't know: it is out of date.
                let Some(m) = sim::map::by_id(map) else {
                    self.out.push(Msg::Refuse {
                        reason: proto::REFUSE_VERSION,
                    });
                    return;
                };
                self.map_id = map;
                self.welcomes += 1;
                self.core = NetCore::new(slot, team, tick, rules, m);
                self.core.watch = self.watch;
                self.core.salt_commit = salt_commit;
                self.ready = true;
            }
            Msg::Reveal { salt } => {
                self.core.on_reveal(salt);
                self.out.push(m);
            }
            m => self.out.push(m),
        }
    }

    /// A datagram that arrived at `at_ms` (the app's clock).
    pub fn on_datagram(&mut self, at_ms: f64, d: &[u8]) {
        if !self.ready || proto::datagram_kind(d) != Some(proto::DG_SNAPSHOT) {
            return;
        }
        if let Ok(s) = decode_snapshot(d) {
            let had = self.core.stats.snapshots;
            self.core.on_snapshot(at_ms, &s);
            // Only what was drawn goes on the clip: an old one (late, or a
            // last heat's) is nothing a replay would show either.
            if self.core.stats.snapshots != had {
                if let Some(c) = self.clip.as_mut() {
                    c.on_snapshot(at_ms, s.tick, d);
                }
            }
        }
    }

    /// Once a frame: advances the clock with the controls as they are and
    /// sends what is due.
    pub fn update(&mut self, now_ms: f64, input: Input, wire: &mut impl Wire) {
        if let Some(c) = self.clip.as_mut() {
            c.tick(now_ms);
        }
        if !self.ready {
            return;
        }
        let mut buf = [0u8; MAX_DATAGRAM];
        for m in self.core.update(now_ms, input) {
            let Ok(n) = encode_input(&m, &mut buf) else { continue };
            if !wire.send_datagram(&buf[..n]) {
                self.dropped += 1;
            }
        }
    }

    /// Stream messages since the last call.
    pub fn take_messages(&mut self) -> Vec<Msg> {
        core::mem::take(&mut self.out)
    }
}

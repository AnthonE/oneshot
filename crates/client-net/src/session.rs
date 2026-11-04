//! A connection's game side, whatever carries it (docs/DESIGN.md §2): the
//! hello, the welcome that starts the netcode (and refuses a map this build
//! doesn't know), the salt checked at the reveal, snapshots in and inputs
//! out. The browser's WebTransport (`web.rs`) and the native wtransport
//! (`native.rs`) only move bytes for it; neither decides anything here.

use proto::{decode_snapshot, encode_input, Msg, Name, Pass, MAX_DATAGRAM, PROTO_VER};
use sim::input::Input;
use sim::map::Map;

use crate::core::NetCore;

/// Where a session's datagrams go. Sending never waits: a full queue drops
/// the datagram (the next carries every unacked input again).
pub trait Wire {
    /// Whether it went (false: dropped).
    fn send_datagram(&mut self, payload: &[u8]) -> bool;
}

/// The QUIC close code the server cuts a session with when it fell behind
/// on the stream (`server/src/outlet.rs` `CLOSE_BEHIND`).
pub const CLOSE_BEHIND: u32 = 1;

/// Why the connection ended, as a player reads it, from the close's code
/// and reason. Only the desktop gets them: wtransport closes with a plain
/// QUIC close, and a browser reads a code only from WebTransport's own
/// close (code 0 and no reason here, so the page says what it can).
pub fn close_why(code: u32, reason: &str) -> String {
    match (code, reason) {
        (CLOSE_BEHIND, _) => "the game fell too far behind (a slow connection, or the game was asleep)".into(),
        (_, r) if !r.is_empty() && r != "bye" => r.to_string(),
        _ => "the connection was lost".into(),
    }
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
        }
    }

    /// The first message on the stream: who, and what to play.
    pub fn hello(name: &str, pass: Pass, mode: u8, skin: u8) -> Msg {
        Msg::Hello {
            proto: PROTO_VER,
            name: Name::new(name),
            pass,
            mode,
            skin,
        }
    }

    /// A stream message: a welcome starts the netcode, a reveal is checked
    /// against the welcome's commitment, the rest wait for the app.
    pub fn on_msg(&mut self, m: Msg) {
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
            self.core.on_snapshot(at_ms, &s);
        }
    }

    /// Once a frame: advances the clock with the controls as they are and
    /// sends what is due.
    pub fn update(&mut self, now_ms: f64, input: Input, wire: &mut impl Wire) {
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

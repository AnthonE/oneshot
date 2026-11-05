//! The clip key's memory: the last minute of a live match as this client
//! was sent it, kept so a player can turn it into a video (the client's
//! `clip.rs` and `film.rs`) without anything leaving their machine. After
//! Gates' `client/src/clip.rs` (MIT).
//!
//! A clip is an ordinary [`tape`](crate::tape), drawn the way a trailer's
//! is. What makes it more than a recording is that it starts mid-stream:
//!
//!   · **The welcome is re-keyed.** A session's netcode starts at a welcome
//!     and takes as its first snapshot only one near the welcome's tick
//!     (`core::FIRST_WITHIN`). A clip cut ten minutes into a match opens on
//!     the welcome with its tick moved to the segment's first snapshot, so
//!     the replay takes it. Snapshots themselves are whole (nothing on the
//!     wire is a delta against an earlier one), so a segment needs no
//!     keyframes the way Gates' does: its first snapshot is one.
//!   · **What the stream built is state.** Who sits where (their names,
//!     sides and skins), the phase and a cup's standing come once, as they
//!     change. They are folded as segments age out and played in a burst at
//!     the clip's first instant, ahead of the snapshots, so the replay knows
//!     every seat its first frame shows.
//!
//! Snapshots are kept for [`KEEP_SEGS`] segments of [`SEG_MS`] each, the
//! oldest dropped as a new one opens. A new welcome (a cup's next heat) is
//! another room: the memory starts again.
//!
//! The tape replays as a watcher of the player (`tape`'s header): their own
//! fighter is drawn off the snapshots like anyone else, since a replay has
//! no input to predict it from.

use std::collections::VecDeque;

use proto::Msg;
use sim::consts::MAX_FIGHTERS;

use crate::tape::{self, Lane};

/// One segment of snapshots, ms.
pub const SEG_MS: f64 = 15_000.0;
/// Segments kept: a clip may reach back between `(KEEP_SEGS − 1) · SEG_MS`
/// and `KEEP_SEGS · SEG_MS`.
pub const KEEP_SEGS: usize = 5;
/// Tape a clip carries ahead of what it asks for: the film's boot plays two
/// seconds and each shot settles for a second and a half before it rolls.
pub const LEAD_S: f64 = 4.0;
/// Seconds of play the longest clip style draws: what the clip key asks
/// for.
pub const CLIP_S: f64 = 30.5;
/// Stream messages kept inside the window. Far over what a minute holds (a
/// kill feed, a seat changing); past it the rest are dropped until the
/// window moves on.
pub const MSG_CAP: usize = 4096;

/// A run of snapshots, and the tick its first one carries.
struct Seg {
    /// The session clock it opened at.
    start_ms: f64,
    start_tick: u32,
    /// Each snapshot: when, and where in `bytes`.
    dgrams: Vec<(f64, u32, u16)>,
    bytes: Vec<u8>,
}

/// What the stream built before the window: the last word on each seat,
/// the phase and the cup.
#[derive(Clone, Copy)]
struct State {
    seats: [Option<Msg>; MAX_FIGHTERS],
    phase: Option<Msg>,
    cup: Option<Msg>,
}

impl State {
    const EMPTY: State = State {
        seats: [None; MAX_FIGHTERS],
        phase: None,
        cup: None,
    };

    fn fold(&mut self, m: &Msg) {
        match *m {
            Msg::Seat { slot, .. } => {
                if let Some(s) = self.seats.get_mut(slot as usize) {
                    *s = Some(*m);
                }
            }
            Msg::Phase { .. } => self.phase = Some(*m),
            Msg::Cup { .. } => self.cup = Some(*m),
            _ => {}
        }
    }

    fn all(&self) -> impl Iterator<Item = &Msg> {
        self.seats
            .iter()
            .flatten()
            .chain(self.phase.iter())
            .chain(self.cup.iter())
    }
}

pub struct Clipper {
    welcome: Option<Msg>,
    /// The session clock: the newest of a frame's and an arrival's, so
    /// every entry is stamped in order.
    clock_ms: f64,
    /// Stream messages since the oldest segment began.
    msgs: Vec<(f64, Msg)>,
    /// What came before them.
    state: State,
    segs: VecDeque<Seg>,
    /// Segments kept ([`KEEP_SEGS`] unless a recorder asks for more).
    keep: usize,
}

impl Default for Clipper {
    fn default() -> Self {
        Clipper::new()
    }
}

impl Clipper {
    pub fn new() -> Clipper {
        Clipper {
            welcome: None,
            clock_ms: 0.0,
            msgs: Vec::new(),
            state: State::EMPTY,
            segs: VecDeque::with_capacity(KEEP_SEGS + 1),
            keep: KEEP_SEGS,
        }
    }

    /// Keep at least `secs` seconds rather than the clip key's minute: a
    /// recorder's tape (the client's `--record`, `ci/film.sh record`).
    pub fn keep_s(&mut self, secs: f64) {
        self.keep = KEEP_SEGS.max((secs * 1000.0 / SEG_MS).ceil() as usize + 1);
    }

    /// A welcome: a room of its own, so the memory starts here.
    pub fn on_welcome(&mut self, m: Msg) {
        self.welcome = Some(m);
        self.msgs.clear();
        self.state = State::EMPTY;
        self.segs.clear();
    }

    /// Any other stream message, as the session takes it in.
    pub fn on_msg(&mut self, m: &Msg) {
        if self.welcome.is_none() {
            return;
        }
        if self.msgs.len() >= MSG_CAP {
            // The state still counts; the feed past the cap doesn't.
            if matches!(m, Msg::Seat { .. } | Msg::Phase { .. } | Msg::Cup { .. }) {
                self.state.fold(m);
            }
            return;
        }
        self.msgs.push((self.clock_ms, *m));
    }

    /// A snapshot the session drew (the newest yet), its tick and its bytes,
    /// taken in at `at_ms`.
    pub fn on_snapshot(&mut self, at_ms: f64, tick: u32, bytes: &[u8]) {
        if self.welcome.is_none() || bytes.len() > u16::MAX as usize {
            return;
        }
        self.clock_ms = self.clock_ms.max(at_ms);
        let due = self.segs.back().is_none_or(|s| self.clock_ms - s.start_ms >= SEG_MS);
        if due {
            // The segment that goes lends the new one its buffers, grown to
            // a segment's size already.
            let (mut dgrams, mut held) = (Vec::new(), Vec::new());
            if self.segs.len() >= self.keep {
                if let Some(old) = self.segs.pop_front() {
                    (dgrams, held) = (old.dgrams, old.bytes);
                    dgrams.clear();
                    held.clear();
                }
            }
            self.segs.push_back(Seg {
                start_ms: self.clock_ms,
                start_tick: tick,
                dgrams,
                bytes: held,
            });
            while self.segs.len() > self.keep {
                self.segs.pop_front();
            }
            // Messages from before the oldest segment are state now.
            let from = self.segs.front().map_or(0.0, |s| s.start_ms);
            let cut = self.msgs.partition_point(|(t, _)| *t < from);
            for (_, m) in self.msgs.drain(..cut) {
                self.state.fold(&m);
            }
        }
        if let Some(seg) = self.segs.back_mut() {
            seg.dgrams
                .push((self.clock_ms, seg.bytes.len() as u32, bytes.len() as u16));
            seg.bytes.extend_from_slice(bytes);
        }
    }

    /// The session's frame at `now_ms`.
    pub fn tick(&mut self, now_ms: f64) {
        self.clock_ms = self.clock_ms.max(now_ms);
    }

    /// Seconds of play a clip can reach back over now.
    pub fn held_s(&self) -> f64 {
        self.segs
            .front()
            .map_or(0.0, |s| ((self.clock_ms - s.start_ms) / 1000.0).max(0.0))
    }

    /// A tape of at least the last `want_s` seconds and [`LEAD_S`] more, or
    /// as much as is held: the newest segment that reaches back that far
    /// and everything after it, the clock restarted at its start. Opens on
    /// the welcome (its tick the segment's) and the state, at 0. `None`
    /// before the first snapshot of a room.
    pub fn tape(&self, want_s: f64) -> Option<Vec<u8>> {
        let welcome = self.welcome?;
        let back_to = self.clock_ms - (want_s + LEAD_S) * 1000.0;
        let from = self.segs.iter().rposition(|s| s.start_ms <= back_to).unwrap_or(0);
        let first = self.segs.get(from)?;
        let t0 = first.start_ms;
        let welcome = match welcome {
            Msg::Welcome {
                slot,
                team,
                tick_hz,
                map,
                rules,
                salt_commit,
                ..
            } => Msg::Welcome {
                slot,
                team,
                tick: first.start_tick,
                tick_hz,
                map,
                rules,
                salt_commit,
            },
            _ => return None,
        };
        let size: usize = self.segs.iter().skip(from).map(|s| s.bytes.len()).sum();
        let mut out = Vec::with_capacity(size + 64 * 1024);
        tape::write_header(&mut out);
        tape::write_entry(&mut out, 0.0, Lane::Msg, &tape::msg_bytes(&welcome)?);
        let cut = self.msgs.partition_point(|(t, _)| *t < t0);
        let mut state = self.state;
        for (_, m) in &self.msgs[..cut] {
            state.fold(m);
        }
        for m in state.all() {
            tape::write_entry(&mut out, 0.0, Lane::Msg, &tape::msg_bytes(m)?);
        }
        // Then both lanes in clock order. A transport takes in the stream
        // before the datagrams each frame, so a message wins a tie.
        let mut dgrams = self
            .segs
            .iter()
            .skip(from)
            .flat_map(|s| s.dgrams.iter().map(move |d| (d, &s.bytes)));
        let mut msgs = self.msgs[cut..].iter();
        let (mut d, mut m) = (dgrams.next(), msgs.next());
        loop {
            let take_msg = match (d, m) {
                (None, None) => break,
                (Some(_), None) => false,
                (None, Some(_)) => true,
                (Some(((td, _, _), _)), Some((tm, _))) => tm <= td,
            };
            if take_msg {
                let (t, msg) = m?;
                tape::write_entry(&mut out, t - t0, Lane::Msg, &tape::msg_bytes(msg)?);
                m = msgs.next();
            } else {
                let (&(t, at, len), bytes) = d?;
                let at = at as usize;
                tape::write_entry(&mut out, t - t0, Lane::Datagram, &bytes[at..at + len as usize]);
                d = dgrams.next();
            }
        }
        Some(out)
    }
}

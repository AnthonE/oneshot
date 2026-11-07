//! Watching a match (after Gates' spectator seats): while a room has
//! watchers it keeps a line of what every seat would be sent, each built by
//! [`crate::build`] from that seat's own view and fog, and hands each frame
//! out only once it is [`Line::delay`] old. A watcher follows a seat and
//! may switch to another at once, so it sees exactly what that player saw,
//! late enough that nobody in the match can use it. Nothing here reads the
//! watcher; it sends nothing that moves the world.
//!
//! A frame is built every [`WATCH_EVERY`] ticks (half the players' rate:
//! interpolation fills the rest) and kept encoded.

use std::collections::VecDeque;

use proto::{encode_snapshot, Msg, MAX_DATAGRAM};
use sim::consts::MAX_FIGHTERS;
use sim::events::Event;
use sim::world::World;

use crate::fog::{Fog, Plain};
use crate::{build, EventQueue};

/// Ticks between frames on the line.
pub const WATCH_EVERY: u32 = 4;
/// How late a casual match is shown, and a ranked one (or a cup's heat).
pub const DELAY_CASUAL_MS: u32 = 10_000;
pub const DELAY_RANKED_MS: u32 = 30_000;

/// Every seat's snapshot at one tick, encoded; None for an empty seat.
pub struct Frame {
    pub tick: u32,
    pub snaps: [Option<Box<[u8]>>; MAX_FIGHTERS],
}

/// What a room keeps for its watchers.
pub struct Line {
    /// Ticks a frame or a message waits before it goes out.
    pub delay: u32,
    fogs: [Fog; MAX_FIGHTERS],
    queues: [EventQueue; MAX_FIGHTERS],
    /// The world's events since the last frame.
    pending: Vec<Event>,
    frames: VecDeque<Frame>,
    /// Stream messages, to everyone (None) or one seat's watchers, each
    /// with the tick it may go out.
    msgs: VecDeque<(u32, Option<u8>, Msg)>,
    /// The tick the line began: nothing older is on it.
    from: u32,
    /// Each seat's `Msg::Seat` as the line has shown it so far (who sits
    /// there at the shown tick, not now): what a new watcher is told.
    names: [Option<Msg>; MAX_FIGHTERS],
}

impl Line {
    /// A line from `from` (the room's tick now) on.
    pub fn new(delay: u32, from: u32) -> Line {
        Line {
            delay,
            from,
            names: [None; MAX_FIGHTERS],
            fogs: Default::default(),
            queues: Default::default(),
            pending: Vec::new(),
            frames: VecDeque::new(),
            msgs: VecDeque::new(),
        }
    }

    /// After the world's tick: its events and the room's messages join the
    /// line, and on a frame tick every seated seat's view is built,
    /// sharing the room's plain sight tests of this tick (`plain`, of `w`),
    /// each seat's fog looking `leads[s]` seconds ahead (`fog::lead_s` at
    /// [`WATCH_EVERY`]).
    pub fn observe(&mut self, w: &World, out: &[(Option<u8>, Msg)], plain: &mut Plain, leads: &[f32; MAX_FIGHTERS]) {
        let tick = w.tick;
        self.pending.extend_from_slice(w.events.as_slice());
        self.observe_msgs(tick, out);
        if !tick.is_multiple_of(WATCH_EVERY) {
            return;
        }
        let mut frame = Frame {
            tick,
            snaps: Default::default(),
        };
        let mut buf = [0u8; 2 * MAX_DATAGRAM];
        for (s, &lead) in leads.iter().enumerate() {
            if !w.fighters[s].seated {
                continue;
            }
            let f = &mut self.fogs[s];
            // Frames every WATCH_EVERY ticks: fog takes its turns (each pair's
            // full look) and its hold at that rate, not the players'.
            f.update_with(w, s, lead, tick, plain, WATCH_EVERY);
            let q = &mut self.queues[s];
            let snap = build(
                w,
                s,
                (0, 0),
                &self.pending,
                q,
                |x| f.shows(x, tick),
                |i| f.shows_vehicle(i, tick),
            );
            // A watcher never acks: what went in this frame is delivered.
            q.list.clear();
            if let Ok(n) = encode_snapshot(&snap, &mut buf) {
                frame.snaps[s] = Some(buf[..n].into());
            }
        }
        self.pending.clear();
        self.frames.push_back(frame);
    }

    /// Messages made at `tick`, to go out with that tick's frame.
    pub fn observe_msgs(&mut self, tick: u32, out: &[(Option<u8>, Msg)]) {
        for (to, m) in out {
            self.msgs.push_back((tick.wrapping_add(self.delay), *to, *m));
        }
    }

    /// Frames and messages old enough to show at `tick`, oldest first.
    pub fn due(&mut self, tick: u32) -> (Vec<Frame>, Vec<(Option<u8>, Msg)>) {
        let mut frames = Vec::new();
        while self
            .frames
            .front()
            .is_some_and(|f| f.tick.wrapping_add(self.delay) <= tick)
        {
            frames.extend(self.frames.pop_front());
        }
        let mut msgs = Vec::new();
        while self.msgs.front().is_some_and(|m| m.0 <= tick) {
            if let Some((_, to, m)) = self.msgs.pop_front() {
                if let (None, Msg::Seat { slot, .. }) = (to, m) {
                    if let Some(n) = self.names.get_mut(slot as usize) {
                        *n = Some(m);
                    }
                }
                msgs.push((to, m));
            }
        }
        (frames, msgs)
    }

    /// Seat `q`'s name as shown now, or `now` (what it is in the room) if
    /// the line hasn't shown one yet; a line's first watcher sets them all.
    pub fn shown_name(&mut self, q: usize, now: Msg) -> Msg {
        *self.names[q].get_or_insert(now)
    }

    /// Seat `q` has someone new in it: no seat's fog holds where it was.
    pub fn forget(&mut self, q: usize) {
        for f in &mut self.fogs {
            f.forget(q);
        }
    }

    /// The tick a watcher arriving now is shown first: `delay` behind, or
    /// the line's first frame if it is younger than that (a match watched
    /// for the first time well after it began).
    pub fn shown_tick(&self, tick: u32) -> u32 {
        tick.saturating_sub(self.delay).max(self.from)
    }
}

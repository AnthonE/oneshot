//! A match as the server runs it, with no transport in it: the world, the
//! seats (people and bots), each client's input buffer, the rewind depth
//! the server chooses for it, and the snapshots it is sent
//! (docs/DESIGN.md §4, §7, §10). `crates/server` puts this behind
//! WebTransport; the netcode tests drive it directly.

use proto::{EventState, InputMsg, Msg, Name, PadState, ProjState, Remote, Snapshot};
use proto::{MAX_REMOTES, MAX_SNAP_EVENTS, MAX_SNAP_PADS, MAX_SNAP_PROJ};
use sim::bot::Bot;
use sim::consts::{ticks, MAX_FIGHTERS, MAX_PADS, SHIELD_MAX, TICK_HZ};
use sim::events::{Event, Kind as Ev};
use sim::input::Input;
use sim::map::Map;
use sim::world::{Phase, ProjKind, Rules, World};

/// Input frames a client may have waiting; past this the oldest go, so a
/// client can never run the server's clock for it.
pub const MAX_DEPTH: u32 = 8;
/// Frames held by seq.
const BUF: usize = 32;
/// The most a shot is rewound: 160 ms (docs/DESIGN.md §7).
pub const MAX_FAVOUR_MS: u32 = 160;
/// The playout a client may claim, in ticks.
pub const PLAYOUT_MIN: u32 = 2;
pub const PLAYOUT_MAX: u32 = 12;
/// Snapshots go out every this many ticks.
pub const SNAP_EVERY: u32 = 2;
/// Snapshot send times kept, for RTT.
const SENT: usize = 64;

/// One client's netcode state on the server (after Gates' `ClientNetState`).
#[derive(Clone)]
pub struct ClientNet {
    frames: [Option<(u32, Input)>; BUF],
    /// The next seq to run; set by the first frame that arrives.
    cursor: Option<u32>,
    newest_seq: Option<u32>,
    last: Input,
    /// The seq the last tick ran (a real frame or a repeat in its place).
    pub last_seq: u32,
    /// The newest snapshot tick this client has acked, as the server saw.
    pub acked: Option<u32>,
    pub playout: u8,
    sent: [(u32, f64); SENT],
    pub rtt_ms: f64,
    /// Frames that came too late or too early, and repeats used.
    pub late: u32,
    pub repeats: u32,
    /// Events since the last snapshot to this client.
    pending: Vec<Event>,
}

impl Default for ClientNet {
    fn default() -> Self {
        ClientNet {
            frames: [None; BUF],
            cursor: None,
            newest_seq: None,
            last: Input::default(),
            last_seq: 0,
            acked: None,
            playout: PLAYOUT_MIN as u8,
            sent: [(u32::MAX, 0.0); SENT],
            rtt_ms: 0.0,
            late: 0,
            repeats: 0,
            pending: Vec::new(),
        }
    }
}

impl ClientNet {
    /// An input datagram arrived at `now_ms`; `last_sent` is the newest
    /// snapshot tick this client has been sent.
    pub fn on_input(&mut self, m: &InputMsg, now_ms: f64, last_sent: Option<u32>) {
        // Acks: only ever forward, and never past what was sent. A client
        // that claims an older ack changes nothing.
        if let Some(sent) = last_sent {
            let ack = m.ack_tick;
            if ack <= sent && self.acked.is_none_or(|a| ack > a) {
                self.acked = Some(ack);
                let (t, at) = self.sent[ack as usize % SENT];
                if t == ack {
                    let rtt = now_ms - at;
                    self.rtt_ms = if self.rtt_ms == 0.0 {
                        rtt
                    } else {
                        self.rtt_ms + (rtt - self.rtt_ms) / 8.0
                    };
                }
            }
        }
        self.playout = m.playout;
        for (i, f) in m.frames().iter().enumerate() {
            let seq = m.first_seq.wrapping_add(i as u32);
            let cursor = *self.cursor.get_or_insert(seq);
            let ahead = seq.wrapping_sub(cursor);
            if ahead >= BUF as u32 {
                // Already run, or absurdly far ahead.
                if ahead < 0x8000_0000 {
                    self.late += 1;
                }
                continue;
            }
            self.frames[seq as usize % BUF] = Some((seq, f.sanitized()));
            if self.newest_seq.is_none_or(|n| seq.wrapping_sub(n) < 0x8000_0000) {
                self.newest_seq = Some(seq);
            }
        }
    }

    /// The frame for this tick: the one at the cursor, or the last one
    /// repeated with its taps cleared if it hasn't come.
    pub fn take_frame(&mut self) -> Input {
        let Some(mut cursor) = self.cursor else {
            return Input::default();
        };
        // Too much waiting: drop the oldest. Stepping is the server's.
        if let Some(n) = self.newest_seq {
            let depth = n.wrapping_sub(cursor);
            if depth < 0x8000_0000 && depth > MAX_DEPTH {
                cursor = n - MAX_DEPTH;
            }
        }
        let slot = &mut self.frames[cursor as usize % BUF];
        let inp = match slot.take() {
            Some((s, f)) if s == cursor => f,
            _ => {
                self.repeats += 1;
                self.last.repeated()
            }
        };
        self.last = inp;
        self.last_seq = cursor;
        self.cursor = Some(cursor.wrapping_add(1));
        inp
    }

    /// Frames waiting past the one last run.
    pub fn depth(&self) -> i8 {
        match self.newest_seq {
            Some(n) => (n.wrapping_sub(self.last_seq) as i32).clamp(-127, 127) as i8,
            None => 0,
        }
    }

    fn sent_snapshot(&mut self, tick: u32, now_ms: f64) {
        self.sent[tick as usize % SENT] = (tick, now_ms);
    }

    /// How far back this client's hitscan is checked at `tick`, in 1/256
    /// tick: how old its newest acked snapshot is plus how far behind that
    /// it draws others, both as the server measures, capped.
    pub fn rewind_q8(&self, tick: u32) -> u32 {
        let Some(acked) = self.acked else {
            return 0;
        };
        let age = tick.saturating_sub(acked);
        let play = (self.playout as u32).clamp(PLAYOUT_MIN, PLAYOUT_MAX);
        (age + play).min(ticks(MAX_FAVOUR_MS)) << 8
    }
}

/// What sits in a seat.
#[derive(Clone)]
pub enum Seat {
    Empty,
    Bot(Bot),
    Person(Box<ClientNet>, Name),
}

/// Stream messages to send: to one seat, or to everyone.
pub type Outbox = Vec<(Option<u8>, Msg)>;

pub struct Room {
    pub world: World,
    pub seats: [Seat; MAX_FIGHTERS],
    /// Seats the rules use: 2 for a duel, 4 for doubles.
    pub size: usize,
    seed: u64,
    phase: Phase,
}

impl Room {
    /// A room with bots in every seat until people come.
    pub fn new(map: Map, rules: Rules, seed: u64, salt: (u64, u64)) -> Room {
        let size = if rules.teams { 4 } else { 2 };
        let mut r = Room {
            world: World::new(map, rules, seed, salt),
            seats: core::array::from_fn(|_| Seat::Empty),
            size,
            seed,
            phase: Phase::Countdown,
        };
        for s in 0..size {
            r.seat_bot(s);
        }
        r
    }

    fn team_of(&self, s: usize) -> u8 {
        (s & 1) as u8
    }

    fn seat_bot(&mut self, s: usize) {
        self.seats[s] = Seat::Bot(Bot::new(self.seed, s, 0.5));
        let t = self.team_of(s);
        self.world.seat(s, t);
    }

    /// A person takes the first seat a bot holds. Returns the seat and the
    /// stream messages: a welcome for them, the seats for everyone.
    pub fn join(&mut self, name: Name) -> Option<(u8, Outbox)> {
        let s = (0..self.size).find(|&s| matches!(self.seats[s], Seat::Bot(_) | Seat::Empty))?;
        self.seats[s] = Seat::Person(Box::default(), name);
        let team = self.team_of(s);
        self.world.seat(s, team);
        let mut out: Outbox = vec![(
            Some(s as u8),
            Msg::Welcome {
                slot: s as u8,
                team,
                tick: self.world.tick,
                tick_hz: TICK_HZ as u16,
                map: 0,
                rules: self.world.rules,
            },
        )];
        for q in 0..self.size {
            out.push((Some(s as u8), self.seat_msg(q)));
        }
        out.push((None, self.seat_msg(s)));
        Some((s as u8, out))
    }

    /// A person left: a bot takes the seat.
    pub fn leave(&mut self, s: u8) -> Outbox {
        self.seat_bot(s as usize);
        vec![(None, self.seat_msg(s as usize))]
    }

    fn seat_msg(&self, s: usize) -> Msg {
        let (bot, name) = match &self.seats[s] {
            Seat::Person(_, n) => (false, *n),
            _ => (true, Name::new("BOT")),
        };
        Msg::Seat {
            slot: s as u8,
            team: self.team_of(s),
            bot,
            name,
        }
    }

    pub fn client(&mut self, s: u8) -> Option<&mut ClientNet> {
        match self.seats.get_mut(s as usize)? {
            Seat::Person(c, _) => Some(c),
            _ => None,
        }
    }

    /// An input datagram from seat `s`.
    pub fn on_input(&mut self, s: u8, m: &InputMsg, now_ms: f64) {
        let last_sent = self.last_snapshot_tick();
        if let Some(c) = self.client(s) {
            c.on_input(m, now_ms, last_sent);
        }
    }

    /// The newest tick snapshots went out on.
    fn last_snapshot_tick(&self) -> Option<u32> {
        let t = self.world.tick;
        (t >= SNAP_EVERY).then(|| t - t % SNAP_EVERY)
    }

    /// One tick. Returns the snapshots to send (on snapshot ticks) and the
    /// stream messages.
    pub fn step(&mut self, now_ms: f64) -> (Vec<(u8, Snapshot)>, Outbox) {
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        let mut rewind = [0u32; MAX_FIGHTERS];
        let next = self.world.tick.wrapping_add(1);
        for s in 0..MAX_FIGHTERS {
            match &mut self.seats[s] {
                Seat::Bot(b) => inputs[s] = b.think(&self.world, s),
                Seat::Person(c, _) => {
                    inputs[s] = c.take_frame();
                    rewind[s] = c.rewind_q8(next);
                }
                Seat::Empty => {}
            }
        }
        self.world.step(&inputs, &rewind);

        let mut out: Outbox = Vec::new();
        for e in self.world.events.as_slice() {
            if e.kind == Ev::Kill {
                out.push((
                    None,
                    Msg::Kill {
                        killer: e.a,
                        victim: e.b,
                        cause: e.c,
                    },
                ));
            }
        }
        if self.world.phase != self.phase {
            self.phase = self.world.phase;
            out.push((
                None,
                Msg::Phase {
                    phase: self.phase as u8,
                    winner: self.world.winner,
                },
            ));
        }
        let events = self.world.events.as_slice();
        for seat in self.seats.iter_mut() {
            if let Seat::Person(c, _) = seat {
                c.pending.extend_from_slice(events);
            }
        }

        let mut snaps = Vec::new();
        if self.world.tick.is_multiple_of(SNAP_EVERY) {
            for s in 0..MAX_FIGHTERS {
                let tick = self.world.tick;
                if let Seat::Person(c, _) = &mut self.seats[s] {
                    let snap = build(&self.world, s, c.last_seq, c.depth(), &c.pending, |_| true);
                    c.pending.clear();
                    c.sent_snapshot(tick, now_ms);
                    snaps.push((s as u8, snap));
                }
            }
        }
        (snaps, out)
    }
}

/// The snapshot seat `s` is sent: its own fighter exactly, the others it
/// may see coarsely. `visible(q)` is fog of war's to answer (milestone 3);
/// everything else here is what any client may know.
pub fn build(
    w: &World,
    s: usize,
    last_seq: u32,
    depth: i8,
    events: &[Event],
    visible: impl Fn(usize) -> bool,
) -> Snapshot {
    let mut snap = Snapshot {
        tick: w.tick,
        last_seq,
        depth,
        own: w.fighters[s],
        phase: w.phase as u8,
        clock: w.clock,
        winner: w.winner,
        score: w.score,
        ..Snapshot::default()
    };
    for (q, f) in w.fighters.iter().enumerate() {
        if q == s || !f.seated || !visible(q) || snap.n_remote as usize >= MAX_REMOTES {
            continue;
        }
        snap.remotes[snap.n_remote as usize] = Remote {
            slot: q as u8,
            team: f.team,
            alive: f.alive,
            crouched: f.body.crouched,
            grounded: f.body.grounded,
            reloading: f.reload > 0,
            pos: f.body.pos,
            vel: f.body.vel,
            yaw: f.yaw,
            pitch: f.pitch,
            arm: f.gun().arm,
            shield: (f.shield.max(0) as i64 * 255 / SHIELD_MAX as i64) as u8,
        };
        snap.n_remote += 1;
    }
    for p in w.proj.iter().filter(|p| p.active) {
        if snap.n_proj as usize >= MAX_SNAP_PROJ {
            break;
        }
        snap.proj[snap.n_proj as usize] = ProjState {
            kind: (p.kind == ProjKind::Frag) as u8,
            owner: p.owner,
            pos: p.pos,
            vel: p.vel,
        };
        snap.n_proj += 1;
    }
    for e in events {
        if snap.n_events as usize >= MAX_SNAP_EVENTS {
            break;
        }
        // Hits on others are theirs to know, not this client's.
        if e.kind == Ev::Hit && e.b as usize != s && e.a as usize != s {
            continue;
        }
        snap.events[snap.n_events as usize] = EventState {
            kind: e.kind as u8,
            a: e.a,
            b: e.b,
            part: e.part,
            c: e.c,
            pos: e.pos,
            end: e.end,
        };
        snap.n_events += 1;
    }
    let n = w.map.pads.len().min(MAX_PADS).min(MAX_SNAP_PADS);
    for i in 0..n {
        snap.pads[i] = PadState {
            empty_s: (w.pads[i].empty_for.div_ceil(TICK_HZ)).min(u16::MAX as u32) as u16,
        };
    }
    snap.n_pads = n as u8;
    snap
}

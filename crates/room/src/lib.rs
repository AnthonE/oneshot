//! A match as the server runs it, with no transport in it: the world, the
//! seats (people and bots), each client's input buffer, the rewind depth
//! the server chooses for it, and the snapshots it is sent
//! (docs/DESIGN.md §4, §7, §10). `crates/server` puts this behind
//! WebTransport; the netcode tests drive it directly.

pub mod fog;
pub mod replay;
pub mod watch;

use proto::{EventState, InputMsg, Msg, Name, PadState, ProjState, Remote, Snapshot};
use proto::{MAX_REMOTES, MAX_SNAP_EVENTS, MAX_SNAP_PADS, MAX_SNAP_PROJ};
use sim::bot::Bot;
use sim::consts::{ticks, MAX_FIGHTERS, MAX_PADS, SHIELD_MAX, TICK_HZ};
use sim::events::{Event, Kind as Ev};
use sim::geom::V3;
use sim::input::Input;
use sim::map::Map;
use sim::world::{Phase, ProjKind, Rules, World};

/// Input frames a client may have waiting; past this the oldest go, so a
/// client can never bank inputs. (It can never step faster: one runs a tick.)
pub const MAX_DEPTH: u32 = 16;
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
    /// The fewest frames waiting behind the one run, over the ticks since
    /// the last snapshot (−1: a tick had none). What the client's clock
    /// steers on: a page that draws slowly sends in bursts, and it is the
    /// low point between bursts that starves the server.
    low: Option<i32>,
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
            low: None,
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
            if ahead >= 0x8000_0000 {
                // Already run.
                self.late += 1;
                continue;
            }
            if ahead >= BUF as u32 {
                // The client's clock jumped ahead (it stalled and skipped):
                // follow it. Still one frame a tick; what it skipped is lost.
                self.cursor = Some(seq);
                self.frames = [None; BUF];
                self.newest_seq = None;
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
        let (inp, spare) = match slot.take() {
            Some((s, f)) if s == cursor => {
                let n = self.newest_seq.unwrap_or(cursor);
                (f, n.wrapping_sub(cursor) as i32)
            }
            _ => {
                self.repeats += 1;
                // How far behind the client is: frames it hasn't sent yet
                // count against it, so its clock can jump rather than crawl.
                let behind = self.newest_seq.map_or(-1, |n| (n.wrapping_sub(cursor) as i32).min(-1));
                (self.last.repeated(), behind)
            }
        };
        self.low = Some(self.low.map_or(spare, |l| l.min(spare)));
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

    /// The low point of [`ClientNet::depth`] since the last call, for the
    /// snapshot.
    pub fn take_low(&mut self) -> i8 {
        let d = self.low.take().unwrap_or_else(|| self.depth() as i32);
        d.clamp(-127, 127) as i8
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
    /// Bots stand still and never fire: for tests that need a target.
    pub passive_bots: bool,
    seed: u64,
    phase: Phase,
    /// The match's secret, kept here only to publish its commitment at the
    /// start and the salt itself at the end (docs/DESIGN.md §9).
    salt: (u64, u64),
    fogs: [fog::Fog; MAX_FIGHTERS],
    /// Every call that moves the world, for the replay.
    rec: replay::Recorder,
    /// A ranked room: people only, no bots, nobody joins after the start.
    pub ranked: bool,
    /// The aim review's numbers, for every seat.
    watch: watch::Watch,
}

impl Room {
    /// A room with bots in every seat until people come.
    pub fn new(map: Map, rules: Rules, seed: u64, salt: (u64, u64)) -> Room {
        let mut r = Room::empty(map, rules, seed, salt);
        for s in 0..r.size {
            r.seat_bot(s);
        }
        r
    }

    /// A ranked room: its seats are filled with [`Room::seat_person`], and
    /// one whose person leaves stays empty (its fighter stands still).
    pub fn ranked(map: Map, rules: Rules, seed: u64, salt: (u64, u64)) -> Room {
        Room {
            ranked: true,
            ..Room::empty(map, rules, seed, salt)
        }
    }

    fn empty(map: Map, rules: Rules, seed: u64, salt: (u64, u64)) -> Room {
        let size = if rules.teams { 4 } else { 2 };
        Room {
            rec: replay::Recorder::new(seed, salt, &rules, 0),
            world: World::new(map, rules, seed, salt),
            seats: core::array::from_fn(|_| Seat::Empty),
            size,
            passive_bots: false,
            seed,
            phase: Phase::Countdown,
            salt,
            fogs: Default::default(),
            ranked: false,
            watch: watch::Watch::default(),
        }
    }

    /// The replay so far (the whole match once it is over).
    pub fn replay(&self) -> Vec<u8> {
        self.rec.finish(&self.world)
    }

    /// The aim review's numbers so far.
    pub fn watch(&self) -> &watch::Watch {
        &self.watch
    }

    /// The commitment to this match's salt.
    pub fn salt_commit(&self) -> [u8; 32] {
        proto::salt_commit([self.salt.0, self.salt.1])
    }

    fn world_seat(&mut self, s: usize, team: u8) {
        self.rec.seat(s, team);
        self.world.seat(s, team);
    }

    fn team_of(&self, s: usize) -> u8 {
        (s & 1) as u8
    }

    fn seat_bot(&mut self, s: usize) {
        self.seats[s] = Seat::Bot(Bot::new(self.seed, s, 0.5));
        let t = self.team_of(s);
        self.world_seat(s, t);
    }

    /// A person takes the first seat a bot holds. Returns the seat and the
    /// stream messages: a welcome for them, the seats for everyone.
    pub fn join(&mut self, name: Name) -> Option<(u8, Outbox)> {
        if self.ranked {
            return None;
        }
        let s = (0..self.size).find(|&s| matches!(self.seats[s], Seat::Bot(_) | Seat::Empty))?;
        Some((s as u8, self.seat_person(s, name)))
    }

    /// A person in seat `s`: the welcome for them, the seats for everyone.
    pub fn seat_person(&mut self, s: usize, name: Name) -> Outbox {
        self.seats[s] = Seat::Person(Box::default(), name);
        self.fogs[s] = fog::Fog::default();
        let team = self.team_of(s);
        self.world_seat(s, team);
        let mut out: Outbox = vec![(
            Some(s as u8),
            Msg::Welcome {
                slot: s as u8,
                team,
                tick: self.world.tick,
                tick_hz: TICK_HZ as u16,
                map: 0,
                rules: self.world.rules,
                salt_commit: self.salt_commit(),
            },
        )];
        for q in 0..self.size {
            out.push((Some(s as u8), self.seat_msg(q)));
        }
        out.push((None, self.seat_msg(s)));
        out
    }

    /// A person left: a bot takes the seat, or in a ranked room nobody does
    /// and the fighter stands where it was.
    pub fn leave(&mut self, s: u8) -> Outbox {
        if self.ranked {
            self.seats[s as usize] = Seat::Empty;
        } else {
            self.seat_bot(s as usize);
        }
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
                Seat::Bot(b) => {
                    if !self.passive_bots {
                        inputs[s] = b.think(&self.world, s);
                    }
                }
                Seat::Person(c, _) => {
                    inputs[s] = c.take_frame();
                    rewind[s] = c.rewind_q8(next);
                }
                Seat::Empty => {}
            }
        }
        self.rec.step(&inputs, &rewind);
        self.world.step(&inputs, &rewind);
        let (fogs, tick) = (&self.fogs, self.world.tick);
        self.watch.observe(&self.world, |v, t| fogs[v].shows(t, tick));

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
            if self.phase == Phase::Over {
                // The match is decided: the salt can be checked now.
                out.push((
                    None,
                    Msg::Reveal {
                        salt: [self.salt.0, self.salt.1],
                    },
                ));
            }
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
                    let lead = fog::lead_s(c.rtt_ms, c.playout as u32);
                    let f = &mut self.fogs[s];
                    f.update(&self.world, s, lead, tick);
                    let low = c.take_low();
                    let snap = build(&self.world, s, c.last_seq, low, &c.pending, |q| f.shows(q, tick));
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
    // What this client may know: itself, its side, and whom fog shows it.
    let me = &w.fighters[s];
    let eye = me.eye();
    let known = |q: usize| q == s || (w.rules.teams && w.fighters[q].team == me.team) || visible(q);
    for p in w.proj.iter().filter(|p| p.active) {
        if snap.n_proj as usize >= MAX_SNAP_PROJ {
            break;
        }
        // A projectile flies in the open: sent if it is ours or our side's,
        // or in clear line of sight.
        if !known(p.owner as usize) && !w.map.clear(eye, p.pos) {
            continue;
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
        let Some(e) = filter_event(w, s, eye, e, &known) else {
            continue;
        };
        snap.events[snap.n_events as usize] = e;
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

/// A shot heard is placed only to this grid, with no direction.
pub const HEARD_GRID_M: f32 = 2.0;
/// How far gunfire and blasts carry.
pub const HEAR_SHOT_M: f32 = 60.0;
/// How far a grenade's throw carries.
pub const HEAR_THROW_M: f32 = 30.0;
/// `part` on a Shot or Throw that was heard, not seen.
pub const HEARD: u8 = 1;

fn coarse(p: V3) -> V3 {
    let g = HEARD_GRID_M;
    V3::new((p.x / g).round() * g, (p.y / g).round() * g, (p.z / g).round() * g)
}

/// What client `s` may be told of an event: all of it, a heard version
/// (rounded place, no direction), or nothing (docs/DESIGN.md §8).
fn filter_event(w: &World, s: usize, eye: V3, e: &Event, known: &impl Fn(usize) -> bool) -> Option<EventState> {
    let a = e.a as usize;
    let full = EventState {
        kind: e.kind as u8,
        a: e.a,
        b: e.b,
        part: e.part,
        c: e.c,
        pos: e.pos,
        end: e.end,
    };
    let heard = |range: f32| {
        ((e.pos - eye).len() <= range).then(|| {
            let at = coarse(e.pos);
            EventState {
                part: HEARD,
                pos: at,
                end: at,
                ..full
            }
        })
    };
    match e.kind {
        // Hits are the two fighters' business; where the victim stood is
        // told only if this client may know it.
        Ev::Hit => {
            if a != s && e.b as usize != s {
                return None;
            }
            let pos = if known(e.b as usize) { e.pos } else { V3::ZERO };
            Some(EventState { pos, ..full })
        }
        Ev::Shot if known(a) => Some(EventState { part: 0, ..full }),
        Ev::Shot => heard(HEAR_SHOT_M),
        Ev::Throw if known(a) => Some(full),
        Ev::Throw => heard(HEAR_THROW_M),
        Ev::Blast if known(a) || w.map.clear(eye, e.pos) => Some(full),
        Ev::Blast => heard(HEAR_SHOT_M),
        Ev::Kill => {
            let pos = if known(e.b as usize) { e.pos } else { V3::ZERO };
            Some(EventState { pos, ..full })
        }
        // Where someone spawned, picked up or swung: only if seen.
        Ev::Spawn | Ev::Pickup | Ev::Melee => known(a).then_some(full),
        Ev::None => None,
    }
}

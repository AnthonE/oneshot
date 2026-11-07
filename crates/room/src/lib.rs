//! A match as the server runs it, with no transport in it: the world, the
//! seats (people and bots), each client's input buffer, the rewind depth
//! the server chooses for it, and the snapshots it is sent
//! (docs/DESIGN.md §4, §7, §10). `crates/server` puts this behind
//! WebTransport; the netcode tests drive it directly.

pub mod fog;
pub mod medals;
pub mod replay;
pub mod spectate;
pub mod watch;

use std::collections::VecDeque;

use proto::MAX_SNAP_VEHICLES;
use proto::{
    encode_snapshot, DropState, VehState, MAX_DATAGRAM, MAX_REMOTES, MAX_SNAP_DROPS, MAX_SNAP_EVENTS, MAX_SNAP_PADS,
    MAX_SNAP_PROJ,
};
use proto::{EventState, InputMsg, Msg, Name, PadState, ProjState, Remote, Snapshot};
use sim::bot::Bot;
use sim::consts::{ticks, MAX_FIGHTERS, MAX_PADS, TICK_HZ};
use sim::events::{Event, Kind as Ev};
use sim::geom::V3;
use sim::input::{Input, FIRE, FIRE_LEFT, JUMP, ZOOM};
use sim::map::Map;
use sim::world::{Phase, Rules, World};

/// Input frames a client may have waiting; past this the oldest go, so a
/// client can never bank inputs. (It can never step faster: one runs a tick.)
pub const MAX_DEPTH: u32 = 16;
/// Frames held by seq.
const BUF: usize = 32;
/// A frame that hasn't come runs as the last one that did, held as it was,
/// for this long: past jitter, and past the gap between a slow page's
/// bursts once [`MAX_DEPTH`] has run dry (a page at 3 frames a second
/// still plays as it pressed), so only frames that stopped coming let go.
pub const LOST_HOLD_MS: u32 = 250;
/// Then the triggers, the scope and jump let go and the stick comes back
/// to still over this long: a player whose frames stopped (a hidden tab, a
/// cut cable) stands where they were rather than running and firing until
/// the session times out.
pub const LOST_STOP_MS: u32 = 150;
/// The most a shot is rewound: 160 ms (docs/DESIGN.md §7).
pub const MAX_FAVOUR_MS: u32 = 160;
/// The playout a client may claim, in ticks.
pub const PLAYOUT_MIN: u32 = 2;
pub const PLAYOUT_MAX: u32 = 12;
/// Snapshots go out every this many ticks.
pub const SNAP_EVERY: u32 = 2;
/// Snapshot send times kept, for RTT: 4 s of ticks, so a high ping's acks
/// still find theirs.
const SENT: usize = 512;
/// A seat's first input frame is taken only this soon after the tick it
/// was seated at (its client counts from the welcome's tick, a level's load
/// and all): frames from a match it was in before (a cup's last heat, still
/// on their way) are another clock's.
pub const FIRST_INPUT_WITHIN: u32 = ticks(30_000);

/// The frame run for the `n`th tick in a row (from 1) whose frame hasn't
/// come, `last` being the last that did (docs/DESIGN.md §4): held as it
/// was for [`LOST_HOLD_MS`], then the triggers, the scope and jump let go
/// and the stick comes back to still over [`LOST_STOP_MS`]. Taps stay as
/// they were ([`Input::repeated`]), and so does a crouch, a pose and not
/// an act; the view stays where it was.
pub fn lost_frame(last: Input, n: u32) -> Input {
    let hold = ticks(LOST_HOLD_MS);
    let held = last.repeated();
    if n <= hold {
        return held;
    }
    let stop = ticks(LOST_STOP_MS) as i32;
    let left = (stop - (n - hold).min(stop as u32) as i32).max(0);
    let ease = |v: i8| (v as i32 * left / stop) as i8;
    Input {
        buttons: held.buttons & !(FIRE | FIRE_LEFT | ZOOM | JUMP),
        mx: ease(held.mx),
        mz: ease(held.mz),
        ..held
    }
}

/// What a client had been shown when it made a frame: its newest snapshot
/// (as the server accepted its ack) and how far behind that it drew
/// everyone else. Taken from the datagram a frame first came in, so a
/// frame run later is rewound to its own moment, not to the newest ack's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct View {
    acked: Option<u32>,
    playout: u8,
}

/// One client's netcode state on the server (after Gates' `ClientNetState`).
#[derive(Clone)]
pub struct ClientNet {
    frames: [Option<(u32, Input, View)>; BUF],
    /// The next seq to run; set by the first frame that arrives.
    cursor: Option<u32>,
    newest_seq: Option<u32>,
    /// The last frame that came, and the ticks run in a row since without
    /// one.
    last: Input,
    dry: u32,
    /// The seq the last tick ran (a real frame or a repeat in its place).
    pub last_seq: u32,
    /// The newest snapshot tick this client has acked, as the server saw.
    pub acked: Option<u32>,
    /// The tick it was seated at: it was sent no snapshot of that tick or
    /// before, so an ack of one (a first `ack_tick: 0`) is refused rather
    /// than taken as a view that old.
    pub joined: u32,
    pub playout: u8,
    /// The view of the frame last run (a repeat's: the newest).
    ran: View,
    sent: [(u32, f64); SENT],
    pub rtt_ms: f64,
    /// Frames that came after their tick had run, and ticks run on a
    /// repeat ([`lost_frame`]) because a frame hadn't come.
    pub late: u32,
    pub repeats: u32,
    /// The fewest frames waiting behind the one run, over the ticks since
    /// the last snapshot (−1: a tick had none). What the client's clock
    /// steers on: a page that draws slowly sends in bursts, and it is the
    /// low point between bursts that starves the server.
    low: Option<i32>,
    /// Events since the last snapshot to this client.
    pending: Vec<Event>,
    /// What this client was told of them, until it acks a snapshot that
    /// carried it.
    pub queue: EventQueue,
    /// What it was sent, cut into records: the bases of its deltas.
    bases: Box<proto::delta::Ring>,
    /// Snapshots since each other seat was last updated in one, while it
    /// stays in view; [`UNSEEN`] while it isn't (`update_every`); and what
    /// that update said, repeated while it is held.
    since: [u8; MAX_FIGHTERS],
    updates: [Remote; MAX_FIGHTERS],
}

/// A seat not in this client's last snapshot: updated the moment it is.
pub const UNSEEN: u8 = u8::MAX;

/// A client's events as it may be told them (filtered by `room::build`),
/// each sent in every snapshot from the first that carries it until the
/// client acks one of those (docs/DESIGN.md §10). Oldest first; one is
/// skipped only when it is dropped (counted in `dropped`) or fog no longer
/// lets it be told before it was first sent. The client keeps an event by
/// its id and drops repeats.
#[derive(Clone, Debug, Default)]
pub struct EventQueue {
    /// Each with the tick of the first snapshot that carried it: those are
    /// given oldest first, so the acked ones are always at the front.
    list: VecDeque<(Option<u32>, EventState)>,
    next_id: u16,
    /// Events dropped: past [`MAX_QUEUED_EVENTS`] (a client stopped
    /// acking), or one the wire can't carry (which would otherwise wait at
    /// the front of every snapshot, never sent).
    pub dropped: u32,
}

/// Events a client may have waiting: past this the oldest go.
pub const MAX_QUEUED_EVENTS: usize = 256;

impl EventQueue {
    fn push(&mut self, mut e: EventState) {
        let mut b = [0u8; 32];
        if proto::Pack::pack(&e, &mut proto::bits::BitWriter::new(&mut b)).is_err() {
            self.dropped += 1;
            return;
        }
        e.id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if self.list.len() >= MAX_QUEUED_EVENTS {
            self.list.pop_front();
            self.dropped += 1;
        }
        self.list.push_back((None, e));
    }

    /// The client has every snapshot up to `tick`: what those carried is
    /// home.
    pub fn ack(&mut self, tick: u32) {
        while self
            .list
            .front()
            .is_some_and(|(since, _)| since.is_some_and(|t| t <= tick))
        {
            self.list.pop_front();
        }
        debug_assert!(self.list.iter().all(|(since, _)| since.is_none_or(|t| t > tick)));
    }

    /// Waiting, oldest first.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

impl Default for ClientNet {
    fn default() -> Self {
        ClientNet {
            frames: [None; BUF],
            cursor: None,
            newest_seq: None,
            last: Input::default(),
            dry: 0,
            last_seq: 0,
            acked: None,
            joined: 0,
            playout: PLAYOUT_MIN as u8,
            ran: View::default(),
            sent: [(u32::MAX, 0.0); SENT],
            rtt_ms: 0.0,
            late: 0,
            repeats: 0,
            low: None,
            pending: Vec::new(),
            queue: EventQueue::default(),
            bases: Box::default(),
            since: [UNSEEN; MAX_FIGHTERS],
            updates: [Remote::default(); MAX_FIGHTERS],
        }
    }
}

impl ClientNet {
    /// An input datagram arrived at `now_ms`; `last_sent` is the newest
    /// snapshot tick this client has been sent.
    pub fn on_input(&mut self, m: &InputMsg, now_ms: f64, last_sent: Option<u32>) {
        // Acks: only ever forward, never past what was sent, and never of a
        // tick from before it was seated. A client that claims an older ack
        // changes nothing.
        if let Some(sent) = last_sent {
            let ack = m.ack_tick;
            if ack <= sent && ack > self.joined && self.acked.is_none_or(|a| ack > a) {
                self.acked = Some(ack);
                self.queue.ack(ack);
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
        let view = View {
            acked: self.acked,
            playout: m.playout,
        };
        for (i, f) in m.frames().iter().enumerate() {
            let seq = m.first_seq.wrapping_add(i as u32);
            if self.cursor.is_none() && seq.wrapping_sub(self.joined) > FIRST_INPUT_WITHIN {
                self.late += 1;
                continue;
            }
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
            // Sent again (every datagram carries what isn't acked): it
            // keeps the view it first came with.
            let slot = &mut self.frames[seq as usize % BUF];
            if !slot.is_some_and(|(s, ..)| s == seq) {
                *slot = Some((seq, f.sanitized(), view));
            }
            if self.newest_seq.is_none_or(|n| seq.wrapping_sub(n) < 0x8000_0000) {
                self.newest_seq = Some(seq);
            }
        }
    }

    /// The frame for this tick: the one at the cursor, or if it hasn't
    /// come, the last that did winding down ([`lost_frame`]).
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
            Some((s, f, view)) if s == cursor => {
                let n = self.newest_seq.unwrap_or(cursor);
                self.ran = view;
                self.last = f;
                self.dry = 0;
                (f, n.wrapping_sub(cursor) as i32)
            }
            _ => {
                self.repeats += 1;
                self.dry = self.dry.saturating_add(1);
                // How far behind the client is: frames it hasn't sent yet
                // count against it, so its clock can jump rather than crawl.
                let behind = self.newest_seq.map_or(-1, |n| (n.wrapping_sub(cursor) as i32).min(-1));
                self.ran = View {
                    acked: self.acked,
                    playout: self.playout,
                };
                (lost_frame(self.last, self.dry), behind)
            }
        };
        self.low = Some(self.low.map_or(spare, |l| l.min(spare)));
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

    /// How far back the frame last taken ([`Self::take_frame`]) has this
    /// client's hitscan checked at `tick`, in 1/256 tick: how old the
    /// newest snapshot it had acked when it sent that frame is, plus how
    /// far behind that it drew others then, capped. The newest ack instead
    /// would rewind a frame that waited in the buffer to a later moment
    /// than its shooter saw (a frame or two of ticks: a strafing target
    /// ten centimetres on).
    pub fn rewind_q8(&self, tick: u32) -> u32 {
        let Some(acked) = self.ran.acked else {
            return 0;
        };
        let age = tick.saturating_sub(acked);
        let play = (self.ran.playout as u32).clamp(PLAYOUT_MIN, PLAYOUT_MAX);
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
    /// Seats the rules use: 2 for a duel, 4 for doubles, 8 for a free for
    /// all, 16 for Big Team.
    pub size: usize,
    /// Bots stand still and never fire: for tests that need a target.
    pub passive_bots: bool,
    seed: u64,
    phase: Phase,
    /// The tick play began (the countdown over), once it has.
    pub began: Option<u32>,
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
    /// People who left before the end: their seat, name and aim's numbers
    /// as they left (the seat's own start again for whoever is next).
    pub departed: Vec<(u8, Name, watch::Stats)>,
    /// Medals and the carnage report, for every seat.
    medals: medals::Medals,
    /// The skin and emblem each person wears, checked by the server against
    /// their wallet before they were seated. Cosmetic: never reaches the world.
    looks: [proto::Look; MAX_FIGHTERS],
    /// What its watchers are shown, late (`spectate`); None with nobody
    /// watching.
    pub spectate: Option<Box<spectate::Line>>,
    /// Ticks between snapshots: [`SNAP_EVERY`] unless the server gives a
    /// mode its own rate ([`Room::set_snap_every`]).
    snap_every: u32,
    /// Snapshots go as deltas against what each client acked, when that
    /// is smaller (`proto::delta`); off, every one goes whole.
    pub deltas: bool,
    /// What each seat's snapshot work made this tick, before it is sent in
    /// seat order (`SeatOut`).
    outs: Box<[SeatOut; MAX_FIGHTERS]>,
    /// On a big map, others far from a client are updated every few
    /// snapshots (`update_every`); off, or on an arena, every snapshot.
    pub far_rates: bool,
    /// Whether this is a big map (`sim::map::Def::big`).
    big: bool,
    /// People seated from which seats' snapshots are worked on side by side
    /// ([`PARALLEL_FROM`]); fewer, one after another. The bytes are the
    /// same either way.
    pub parallel_from: usize,
    /// Events dropped from the queues of people who have since left.
    dropped_gone: u64,
}

/// One seat's snapshot as its own thread made it: the snapshot, its bytes
/// on the wire (whole or a delta), and its records' scratch.
struct SeatOut {
    snap: Snapshot,
    wire: [u8; MAX_DATAGRAM],
    delta: [u8; MAX_DATAGRAM],
    keyed: proto::delta::Keyed,
    /// Which of `wire` and `delta` goes, and how many bytes; None: no
    /// snapshot this tick.
    sent: Option<(bool, usize)>,
}

impl Default for SeatOut {
    fn default() -> Self {
        SeatOut {
            snap: Snapshot::default(),
            wire: [0; MAX_DATAGRAM],
            delta: [0; MAX_DATAGRAM],
            keyed: proto::delta::Keyed::default(),
            sent: None,
        }
    }
}

impl SeatOut {
    fn bytes(&self) -> Option<&[u8]> {
        self.sent
            .map(|(d, n)| if d { &self.delta[..n] } else { &self.wire[..n] })
    }
}

/// People seated before a snapshot's seats are worked on side by side.
/// Measured on 8v8 Dry Fork: a seat's fog and snapshot is about 15 µs,
/// about what waking the pool's threads costs, and the server already
/// steps rooms side by side, so 16 seats gain nothing; a bigger room does.
pub const PARALLEL_FROM: usize = 24;

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
        let size = (rules.seats as usize).clamp(1, MAX_FIGHTERS);
        let map_id = map.id;
        Room {
            rec: replay::Recorder::new(seed, salt, &rules, map.id),
            world: World::new(map, rules, seed, salt),
            seats: core::array::from_fn(|_| Seat::Empty),
            size,
            passive_bots: false,
            seed,
            phase: Phase::Countdown,
            began: None,
            salt,
            fogs: Default::default(),
            ranked: false,
            watch: watch::Watch::default(),
            departed: Vec::new(),
            medals: medals::Medals::default(),
            looks: [proto::Look::default(); MAX_FIGHTERS],
            spectate: None,
            snap_every: SNAP_EVERY,
            deltas: true,
            outs: Box::new(core::array::from_fn(|_| SeatOut::default())),
            parallel_from: PARALLEL_FROM,
            far_rates: true,
            big: sim::maps::ALL.iter().any(|d| d.id == map_id && d.big()),
            dropped_gone: 0,
        }
    }

    /// Events lost since the match began: past the sim's cap in a tick
    /// (`sim::events::Events::dropped`), and from people's queues
    /// ([`EventQueue::dropped`]), those who left included.
    pub fn events_dropped(&self) -> u64 {
        let queues: u64 = self
            .seats
            .iter()
            .map(|x| match x {
                Seat::Person(c, _) => c.queue.dropped as u64,
                _ => 0,
            })
            .sum();
        self.world.events.dropped as u64 + queues + self.dropped_gone
    }

    /// Snapshots every `n` ticks from now on (2 to 8), on the ticks that are
    /// multiples of it: a mode's own rate. What the world does is the same
    /// at any rate; only what is sent. A delta's bases are kept at any of
    /// them (`proto::delta::Ring` has a slot for every two ticks).
    pub fn set_snap_every(&mut self, n: u32) {
        self.snap_every = n.clamp(2, 8);
    }

    /// Ticks between snapshots.
    pub fn snap_every(&self) -> u32 {
        self.snap_every
    }

    /// Someone watches: the line starts (if it hasn't), a ranked match
    /// shown later than a casual one. Returns what the watcher is sent
    /// first: a welcome to the shown tick, as `follow`'s seat, and every
    /// seat's name.
    pub fn spectate_welcome(&mut self, follow: u8) -> Vec<Msg> {
        let ms = if self.ranked {
            spectate::DELAY_RANKED_MS
        } else {
            spectate::DELAY_CASUAL_MS
        };
        let line = self
            .spectate
            .get_or_insert_with(|| Box::new(spectate::Line::new(ticks(ms), self.world.tick)));
        let tick = line.shown_tick(self.world.tick);
        let mut out = vec![Msg::Welcome {
            slot: follow,
            team: self.team_of(follow as usize),
            tick,
            tick_hz: TICK_HZ as u16,
            map: self.world.map.id,
            rules: self.world.rules,
            salt_commit: self.salt_commit(),
        }];
        // Who sat where at the shown tick (a seat taken since is told when
        // the line gets to it).
        for q in 0..self.size {
            let now = self.seat_msg(q);
            let shown = self.spectate.as_mut().map_or(now, |l| l.shown_name(q, now));
            out.push(shown);
        }
        out
    }

    /// The seat a new watcher follows: the first person, else the first
    /// seated.
    pub fn spectate_first(&self) -> u8 {
        let person = (0..self.size).find(|&s| matches!(self.seats[s], Seat::Person(..)));
        person
            .or_else(|| (0..self.size).find(|&s| self.world.fighters[s].seated))
            .unwrap_or(0) as u8
    }

    /// The seat after `s` a watcher may follow (seated), wrapping.
    pub fn spectate_next(&self, s: u8) -> u8 {
        (1..=self.size)
            .map(|k| (s as usize + k) % self.size)
            .find(|&q| self.world.fighters[q].seated)
            .unwrap_or(s as usize) as u8
    }

    /// The replay so far (the whole match once it is over).
    pub fn replay(&self) -> Vec<u8> {
        self.rec.finish(&self.world)
    }

    /// The aim review's numbers so far.
    pub fn watch(&self) -> &watch::Watch {
        &self.watch
    }

    /// Medals and the carnage report so far.
    pub fn medals(&self) -> &medals::Medals {
        &self.medals
    }

    /// Seat `s`'s carnage report as the stream gives it: the medals' tally
    /// and the aim review's rounds.
    pub fn carnage(&self, s: usize) -> Msg {
        let t = self.medals.tally(s);
        let c16 = |x: u32| x.min(u16::MAX as u32) as u16;
        let mut medals = [0u8; proto::medal::COUNT];
        for (m, &n) in medals.iter_mut().zip(&t.medals) {
            *m = n.min(u8::MAX as u16) as u8;
        }
        Msg::Carnage {
            slot: s as u8,
            kills: t.kills,
            deaths: t.deaths,
            assists: t.assists,
            betrayals: t.betrayals,
            shots: c16(t.shots),
            hits: c16(t.hits),
            heads: c16(t.heads),
            damage: c16(t.damage),
            taken: c16(t.taken),
            best_spree: t.best_spree,
            best_multi: t.best_multi,
            medals,
        }
    }

    /// The commitment to this match's salt.
    pub fn salt_commit(&self) -> [u8; 32] {
        proto::salt_commit([self.salt.0, self.salt.1])
    }

    fn world_seat(&mut self, s: usize, team: u8) {
        self.rec.seat(s, team);
        self.world.seat(s, team);
        self.medals.reset(s);
        self.watch.reset(s);
        // Someone new, at a spawn: nobody's fog holds the last one's place.
        for f in &mut self.fogs {
            f.forget(s);
        }
        if let Some(l) = self.spectate.as_mut() {
            l.forget(s);
        }
    }

    fn team_of(&self, s: usize) -> u8 {
        (s & 1) as u8
    }

    fn seat_bot(&mut self, s: usize) {
        self.seats[s] = Seat::Bot(Bot::new(self.seed, s, 0.5));
        self.looks[s] = proto::Look::default();
        let t = self.team_of(s);
        self.world_seat(s, t);
    }

    /// A person takes the first seat a bot holds, wearing `look` (one the
    /// server checked). Returns the seat and the stream messages: a welcome
    /// for them, the seats for everyone.
    pub fn join(&mut self, name: Name, look: proto::Look) -> Option<(u8, Outbox)> {
        self.join_team(name, look, None)
    }

    /// [`Room::join`], on `team`'s side while it has a seat (a party sits
    /// together; the sides are the seats' parity).
    pub fn join_team(&mut self, name: Name, look: proto::Look, team: Option<u8>) -> Option<(u8, Outbox)> {
        if self.ranked {
            return None;
        }
        let s = (0..self.size)
            .find(|&s| self.open(s) && team.is_none_or(|t| self.team_of(s) == t))
            .or_else(|| (0..self.size).find(|&s| self.open(s)))?;
        Some((s as u8, self.seat_person(s, name, look)))
    }

    /// Whether a person could take seat `s` (a bot holds it, or nobody).
    fn open(&self, s: usize) -> bool {
        matches!(self.seats[s], Seat::Bot(_) | Seat::Empty)
    }

    /// Seats a person could take, on `team`'s side or anywhere.
    pub fn open_seats(&self, team: Option<u8>) -> usize {
        (0..self.size)
            .filter(|&s| self.open(s) && team.is_none_or(|t| self.team_of(s) == t))
            .count()
    }

    /// A person in seat `s`: the welcome for them, the seats for everyone.
    pub fn seat_person(&mut self, s: usize, name: Name, look: proto::Look) -> Outbox {
        self.seats[s] = Seat::Person(
            Box::new(ClientNet {
                joined: self.world.tick,
                ..ClientNet::default()
            }),
            name,
        );
        self.looks[s] = proto::Look {
            skin: if (look.skin as usize) < proto::SKINS.len() {
                look.skin
            } else {
                proto::SKIN_STANDARD
            },
            emblem: proto::emblem_valid(look.emblem),
            headwear: proto::gear_valid(&proto::HEADWEAR, look.headwear),
            footwear: proto::gear_valid(&proto::FOOTWEAR, look.footwear),
        };
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
                map: self.world.map.id,
                rules: self.world.rules,
                salt_commit: self.salt_commit(),
            },
        )];
        for q in 0..self.size {
            out.push((Some(s as u8), self.seat_msg(q)));
        }
        out.push((None, self.seat_msg(s)));
        self.note(&out);
        out
    }

    /// Messages made outside the tick (a seat taken or left) join the
    /// watchers' line too.
    fn note(&mut self, out: &Outbox) {
        if let Some(l) = self.spectate.as_mut() {
            let now: Outbox = out
                .iter()
                .filter(|(_, m)| !matches!(m, Msg::Welcome { .. }))
                .copied()
                .collect();
            l.observe_msgs(self.world.tick, &now);
        }
    }

    /// A person left: a bot takes the seat, or in a ranked room nobody does
    /// and the fighter stands where it was.
    pub fn leave(&mut self, s: u8) -> Outbox {
        if let Seat::Person(c, name) = &self.seats[s as usize] {
            self.dropped_gone += c.queue.dropped as u64;
            // Their aim's numbers stay for the review, under their name.
            self.departed.push((s, *name, self.watch.stats[s as usize].clone()));
        }
        if self.ranked {
            self.seats[s as usize] = Seat::Empty;
            self.looks[s as usize] = proto::Look::default();
        } else {
            self.seat_bot(s as usize);
        }
        let out = vec![(None, self.seat_msg(s as usize))];
        self.note(&out);
        out
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
            skin: self.looks[s].skin,
            emblem: self.looks[s].emblem,
            headwear: self.looks[s].headwear,
            footwear: self.looks[s].footwear,
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
        let n = self.snap_every;
        (t >= n).then(|| t - t % n)
    }

    /// One tick. Returns the snapshots to send (on snapshot ticks) and the
    /// stream messages.
    pub fn step(&mut self, now_ms: f64) -> (Vec<(u8, Snapshot)>, Outbox) {
        let (mut snaps, mut out) = (Vec::new(), Vec::new());
        self.step_into(now_ms, &mut out, |s, snap, _| snaps.push((s, *snap)));
        (snaps, out)
    }

    /// [`Room::step`] for the server: the stream messages go onto `out`
    /// (cleared first), and each snapshot is handed to `send` with its
    /// seat and its bytes on the wire (empty if it can't fit a datagram),
    /// encoded once.
    pub fn step_into(&mut self, now_ms: f64, out: &mut Outbox, mut send: impl FnMut(u8, &Snapshot, &[u8])) {
        out.clear();
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
        let mut plain = fog::Plain::default();
        self.watch
            .observe_with(&self.world, &mut plain, |v, t| fogs[v].shows(t, tick));

        for e in self.world.events.as_slice() {
            if e.kind == Ev::Kill {
                out.push((
                    None,
                    Msg::Kill {
                        killer: e.a,
                        victim: e.b,
                        cause: e.c,
                        head: e.part == 2,
                    },
                ));
            }
        }
        // After the feed's lines: a medal to its earner, or to everyone if
        // it says no more than the feed.
        for a in self.medals.observe(&self.world) {
            let to = (!proto::medal::info(a.medal).public).then_some(a.slot);
            out.push((
                to,
                Msg::Medal {
                    slot: a.slot,
                    medal: a.medal,
                    victim: a.victim,
                },
            ));
        }
        if self.world.phase != self.phase {
            self.phase = self.world.phase;
            if self.phase == Phase::Playing {
                self.began = Some(self.world.tick);
            }
            out.push((
                None,
                Msg::Phase {
                    phase: self.phase as u8,
                    winner: self.world.winner,
                },
            ));
            if self.phase == Phase::Over {
                // The carnage report: everyone's, now nothing in it can
                // help anyone.
                for q in 0..self.size {
                    if self.world.fighters[q].seated {
                        out.push((None, self.carnage(q)));
                    }
                }
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

        if self.world.tick.is_multiple_of(self.snap_every) {
            // Each person's fog, snapshot and delta: one seat's state each,
            // the world read-only, so seats are worked on side by side and
            // the snapshots are the same as one after another. Sent after,
            // in seat order.
            let Room {
                world,
                seats,
                fogs,
                outs,
                snap_every,
                deltas,
                parallel_from,
                far_rates,
                big,
                ..
            } = self;
            let rated = *far_rates && *big;
            let (world, plain, snap_every, deltas) = (&*world, &plain, *snap_every, *deltas);
            let work = |(s, ((seat, f), o)): (usize, ((&mut Seat, &mut fog::Fog), &mut SeatOut))| {
                o.sent = None;
                if let Seat::Person(c, _) = seat {
                    seat_snapshot(world, s, c, f, o, plain, (snap_every, rated), deltas, now_ms);
                }
            };
            let people = seats.iter().filter(|x| matches!(x, Seat::Person(..))).count();
            if people >= *parallel_from {
                use rayon::prelude::*;
                seats
                    .par_iter_mut()
                    .zip(fogs.par_iter_mut())
                    .zip(outs.par_iter_mut())
                    .enumerate()
                    .for_each(work);
            } else {
                seats
                    .iter_mut()
                    .zip(fogs.iter_mut())
                    .zip(outs.iter_mut())
                    .enumerate()
                    .for_each(work);
            }
            for (s, o) in self.outs.iter().enumerate() {
                if let Some(b) = o.bytes() {
                    send(s as u8, &o.snap, b);
                }
            }
        }
        if let Some(l) = self.spectate.as_mut() {
            // Each seat's fog for its watchers looks as far ahead as the
            // seat's own would at the line's rate.
            let leads = core::array::from_fn(|s| match &self.seats[s] {
                Seat::Person(c, _) => fog::lead_s(c.rtt_ms, c.playout as u32, spectate::WATCH_EVERY),
                _ => fog::lead_s(0.0, PLAYOUT_MIN, spectate::WATCH_EVERY),
            });
            l.observe(&self.world, out, &mut plain, &leads);
        }
    }
}

/// Seat `s`'s snapshot on a snapshot tick, on whichever thread: its fog
/// looks again, its snapshot is built and encoded, and, against the newest
/// one it acked if both ends still keep it and that is smaller, made a
/// delta; it then joins the seat's bases. Into `o`.
#[allow(clippy::too_many_arguments)]
fn seat_snapshot(
    world: &World,
    s: usize,
    c: &mut ClientNet,
    f: &mut fog::Fog,
    o: &mut SeatOut,
    plain: &fog::Plain,
    (snap_every, rated): (u32, bool),
    deltas: bool,
    now_ms: f64,
) {
    let tick = world.tick;
    let lead = fog::lead_s(c.rtt_ms, c.playout as u32, snap_every);
    f.update_with(world, s, lead, tick, plain, snap_every);
    let low = c.take_low();
    let (snap, n) = build_encoded(
        world,
        s,
        (c.last_seq, low),
        &c.pending,
        &mut c.queue,
        |q| f.shows(q, tick),
        |i| f.shows_vehicle(i, tick),
        rated.then_some((&mut c.since, &mut c.updates)),
        &mut o.wire,
    );
    c.pending.clear();
    c.sent_snapshot(tick, now_ms);
    o.snap = snap;
    o.sent = Some((false, n));
    if n > 0 && deltas && o.keyed.fill(&snap).is_ok() {
        let base = c
            .acked
            .filter(|&a| tick.wrapping_sub(a) < proto::delta::DELTA_WINDOW)
            .and_then(|a| c.bases.get(a));
        if let Some(m) = base.and_then(|b| proto::delta::encode_delta(&o.keyed, b, &mut o.delta).ok()) {
            if m < n {
                o.sent = Some((true, m));
            }
        }
        core::mem::swap(c.bases.slot(tick), &mut o.keyed);
    }
}

/// The snapshot seat `s` is sent: its own fighter exactly, the others it
/// may see coarsely. `(last_seq, depth)` are the client's last input seq
/// run and the low point of its input buffer (`ClientNet::take_low`);
/// `visible(q)` and `vehicle_visible(i)` are fog of war's to answer;
/// everything else here is what any client may know.
pub fn build(
    w: &World,
    s: usize,
    (last_seq, depth): (u32, i8),
    events: &[Event],
    queue: &mut EventQueue,
    visible: impl Fn(usize) -> bool,
    vehicle_visible: impl Fn(usize) -> bool,
) -> Snapshot {
    let mut wire = [0u8; MAX_DATAGRAM];
    build_encoded(
        w,
        s,
        (last_seq, depth),
        events,
        queue,
        visible,
        vehicle_visible,
        None,
        &mut wire,
    )
    .0
}

/// [`build`], and the snapshot's bytes in `wire`: how many (0 if even with
/// no events it is over a datagram, which `tests/budget.rs` rules out).
#[allow(clippy::too_many_arguments)]
fn build_encoded(
    w: &World,
    s: usize,
    (last_seq, depth): (u32, i8),
    events: &[Event],
    queue: &mut EventQueue,
    visible: impl Fn(usize) -> bool,
    vehicle_visible: impl Fn(usize) -> bool,
    mut rates: Option<Rates>,
    wire: &mut [u8; MAX_DATAGRAM],
) -> (Snapshot, usize) {
    let mut snap = Snapshot {
        tick: w.tick,
        last_seq,
        depth,
        own: w.fighters[s],
        phase: w.phase as u8,
        clock: w.clock,
        winner: w.winner,
        score: w.score,
        n_score: if w.rules.teams { 2 } else { w.rules.seats },
        ..Snapshot::default()
    };
    // What this client may know: itself, its side, and whom fog shows it.
    let me = &w.fighters[s];
    let eye = me.eye();
    // Nobody (an event's `a` past the seats: `proto::NOBODY`) is never known.
    let known =
        |q: usize| q < MAX_FIGHTERS && (q == s || (w.rules.teams && w.fighters[q].team == me.team) || visible(q));
    // Every kind, each by its own rule; no wildcard, so a new kind must
    // say what this client may see of it.
    for k in sim::thing::Kind::ALL {
        match k {
            sim::thing::Kind::Fighter => add_fighters(&mut snap, w, s, &visible, rates.as_mut()),
            sim::thing::Kind::Projectile => add_projectiles(&mut snap, w, s, eye, &known, &vehicle_visible),
            sim::thing::Kind::Pad => add_pads(&mut snap, w),
            sim::thing::Kind::Vehicle => add_vehicles(&mut snap, w, s, &known, &vehicle_visible),
            sim::thing::Kind::Drop => add_drops(&mut snap, w, eye),
            sim::thing::Kind::Flag => add_flags(&mut snap, w),
        }
    }
    // What this client may be told of the new events joins its queue; the
    // queue goes out oldest first, as many as fit.
    for e in events {
        if let Some(e) = filter_event(w, s, eye, e, &known) {
            queue.push(e);
        }
    }
    // What still waits for its first snapshot (a firefight's backlog) is
    // told as fog has it now: who went out of sight while it waited is
    // only heard, or not told at all.
    queue.list.retain_mut(|(since, e)| since.is_some() || refog(e, &known));
    for (_, e) in queue.list.iter().take(MAX_SNAP_EVENTS) {
        snap.events[snap.n_events as usize] = *e;
        snap.n_events += 1;
    }
    // Over a datagram, the snapshot would be dropped whole: the newest
    // events wait for the next instead. It only grows with its events, so
    // the most that fit are found in a few encodes, not one per event.
    let n = match encode_snapshot(&snap, wire) {
        Ok(n) => n,
        Err(_) => {
            let fits = |snap: &mut Snapshot, k: u8, wire: &mut [u8]| {
                snap.n_events = k;
                encode_snapshot(snap, wire).is_ok()
            };
            // `lo` fits (or is 0), `hi` doesn't.
            let (mut lo, mut hi) = (0u8, snap.n_events);
            while hi - lo > 1 {
                let mid = lo + (hi - lo) / 2;
                if fits(&mut snap, mid, wire) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            snap.n_events = lo;
            encode_snapshot(&snap, wire).unwrap_or(0)
        }
    };
    for (since, _) in queue.list.iter_mut().take(snap.n_events as usize) {
        since.get_or_insert(w.tick);
    }
    (snap, n)
}

/// The others fog shows this client, coarsely.
/// With `rates` (a big map's), each is updated every [`update_every`]
/// snapshots, `rates` counting the snapshots since and keeping that update;
/// between, it is held (`Snapshot::held`): its last update, repeated, as
/// long as nothing but where it is and faces has changed.
fn add_fighters(
    snap: &mut Snapshot,
    w: &World,
    s: usize,
    visible: &impl Fn(usize) -> bool,
    mut rates: Option<&mut Rates>,
) {
    let me = &w.fighters[s];
    for (q, f) in w.fighters.iter().enumerate() {
        let shown = q != s && f.seated && visible(q);
        if !shown || snap.n_remote as usize >= MAX_REMOTES {
            if let Some((since, _)) = rates.as_deref_mut() {
                since[q] = UNSEEN;
            }
            continue;
        }
        let mut r = Remote::of(q as u8, f);
        if let Some((since, last)) = rates.as_deref_mut() {
            // Only where it is and faces is ever held: a death, a cloak, a
            // shield, a gun, a crouch or a flag goes out the moment it is.
            let moved_only = Remote {
                pos: r.pos,
                vel: r.vel,
                yaw: r.yaw,
                pitch: r.pitch,
                ..last[q]
            } == r;
            if since[q] == UNSEEN || !moved_only || since[q].saturating_add(1) >= update_every(me, f) {
                since[q] = 0;
                last[q] = r;
            } else {
                since[q] += 1;
                r = last[q];
                snap.held |= 1 << q;
            }
        }
        snap.remotes[snap.n_remote as usize] = r;
        snap.n_remote += 1;
    }
}

/// A client's counts of snapshots since each seat's last update, and those
/// updates (`add_fighters`).
type Rates<'a> = (&'a mut [u8; MAX_FIGHTERS], &'a mut [Remote; MAX_FIGHTERS]);

/// Past this, another fighter is updated every other snapshot on a big map.
pub const FAR_M: f32 = 60.0;
/// Past this, every fourth.
pub const FARTHER_M: f32 = 120.0;
/// Firing within this of the line to a viewer: shooting at it.
pub const AT_ME_DEG: f32 = 15.0;

/// How many snapshots apart `them` is updated for `me` on a big map: every
/// one near, riding, or firing toward `me`; every other past [`FAR_M`];
/// every fourth past [`FARTHER_M`].
pub fn update_every(me: &sim::fighter::Fighter, them: &sim::fighter::Fighter) -> u8 {
    let to_me = me.body.pos + V3::new(0.0, 1.1, 0.0) - them.eye();
    let far = to_me.len();
    if far <= FAR_M || them.riding().is_some() {
        return 1;
    }
    let firing = them.last_buttons & FIRE != 0;
    let aim = sim::geom::aim_dir(them.yaw, them.pitch);
    // cos 15°, written out: trig is the sim's to do (`sim::fmath`).
    const COS_AT_ME: f32 = 0.965_925_8;
    if firing && aim.dot(to_me) >= COS_AT_ME * far {
        return 1;
    }
    if far <= FARTHER_M {
        2
    } else {
        4
    }
}

/// A projectile flies in the open: sent if its owner is known to this
/// client, or it is in clear line of sight. A sticky stuck to someone, or
/// to a vehicle, goes where they go: sent only while they would be
/// ([`stuck_shown`]), so it never tells where a hidden enemy is.
fn add_projectiles(
    snap: &mut Snapshot,
    w: &World,
    s: usize,
    eye: V3,
    known: &impl Fn(usize) -> bool,
    vehicle_visible: &impl Fn(usize) -> bool,
) {
    // Everything this client may see, then the most relevant to it first
    // when there are more than a snapshot holds (as Halo: Reach ranked by
    // relevance a client at a time): on the stack, ties by slot.
    let mut seen = [(0.0f32, 0usize); sim::consts::MAX_PROJ];
    let mut n = 0;
    for (i, p) in w.proj.live() {
        if !known(p.owner as usize) && !w.map.clear(eye, p.pos) {
            continue;
        }
        if !stuck_shown(w, s, p, known, vehicle_visible) {
            continue;
        }
        seen[n] = (proj_rank(p, s, eye), i);
        n += 1;
    }
    let seen = &mut seen[..n];
    if n > MAX_SNAP_PROJ {
        seen.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    }
    for &(_, i) in seen.iter().take(MAX_SNAP_PROJ) {
        let p = &w.proj[i];
        snap.proj[snap.n_proj as usize] = ProjState {
            kind: p.kind as u8,
            owner: p.owner,
            pos: p.pos,
            vel: p.vel,
        };
        snap.n_proj += 1;
    }
}

/// How far a round closing on the eye counts, against its distance: one
/// coming at you is worth seeing before one as near going away.
pub const PROJ_CLOSING: f32 = 0.5;

/// A round's place in client `s`'s queue, lowest first: its own rounds and
/// any about it (homing on it, stuck to it) before anything, then the rest
/// by distance from its eye, a round closing on it as [`PROJ_CLOSING`] as
/// far.
fn proj_rank(p: &sim::world::Projectile, s: usize, eye: V3) -> f32 {
    if p.owner as usize == s || p.target as usize == s {
        return -1.0;
    }
    let to_eye = eye - p.pos;
    let far = to_eye.len();
    // A landed round's `vel` is where it sits on what it struck.
    if !p.landed && p.vel.dot(to_eye) > 0.0 {
        far * PROJ_CLOSING
    } else {
        far
    }
}

/// Whether what a projectile is stuck to may be shown to client `s`:
/// always for one not stuck to anything; for a sticky on a fighter, only
/// if it is known; on a vehicle, only if that vehicle is sent
/// ([`vehicle_shown`]).
fn stuck_shown(
    w: &World,
    s: usize,
    p: &sim::world::Projectile,
    known: &impl Fn(usize) -> bool,
    vehicle_visible: &impl Fn(usize) -> bool,
) -> bool {
    if !(p.kind == sim::world::ProjKind::Sticky && p.landed && p.target != sim::world::NO_TARGET) {
        return true;
    }
    let t = p.target as usize;
    if t < MAX_FIGHTERS {
        known(t)
    } else {
        vehicle_shown(w, s, t - MAX_FIGHTERS, known, vehicle_visible)
    }
}

/// Whether vehicle `i` goes to client `s`: the one it rides, one someone
/// it knows rides, or one fog shows it.
fn vehicle_shown(
    w: &World,
    s: usize,
    i: usize,
    known: &impl Fn(usize) -> bool,
    visible: &impl Fn(usize) -> bool,
) -> bool {
    let Some(v) = w.vehicles.get(i).filter(|v| v.active) else {
        return false;
    };
    let mine = w.fighters[s].riding().is_some_and(|(r, _)| r == i);
    mine || v.seats.iter().any(|&x| x != 0 && known(x as usize - 1)) || visible(i)
}

/// How far a gun on the floor is named to a client that sees it.
pub const SEE_DROP_M: f32 = 50.0;
/// A drop is seen if a line from the eye clears the map to this far over
/// where it lies.
pub const DROP_LOOK_Y: f32 = 0.3;

/// The guns on the floor this client sees, nearest first: in clear line of
/// its eye and within [`SEE_DROP_M`]. Never by who left it, so a fresh one
/// (where someone fell) never tells where a hidden enemy died.
fn add_drops(snap: &mut Snapshot, w: &World, eye: V3) {
    let mut seen = [(f32::INFINITY, 0usize); sim::consts::MAX_DROPS];
    let mut n = 0;
    for (i, d) in w.drops.live() {
        let far = (d.pos - eye).len();
        if far <= SEE_DROP_M && w.map.clear(eye, d.pos + V3::new(0.0, DROP_LOOK_Y, 0.0)) {
            seen[n] = (far, i);
            n += 1;
        }
    }
    let seen = &mut seen[..n];
    seen.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for &(_, i) in seen.iter().take(MAX_SNAP_DROPS) {
        let d = &w.drops[i];
        snap.drops[snap.n_drops as usize] = DropState {
            slot: i as u8,
            arm: d.gun.arm,
            pos: d.pos,
        };
        snap.n_drops += 1;
    }
}

/// The vehicle this client rides in, exactly; the others fog shows it
/// (`visible`: [`fog::Fog::shows_vehicle`]), or that someone it knows
/// rides in; and a seat's rider only when it may know them.
fn add_vehicles(
    snap: &mut Snapshot,
    w: &World,
    s: usize,
    known: &impl Fn(usize) -> bool,
    visible: &impl Fn(usize) -> bool,
) {
    let me = &w.fighters[s];
    let mine = me.riding().map(|(i, _)| i);
    if let Some(i) = mine {
        snap.own_vehicle = Some(w.vehicles[i]);
    }
    for (i, v) in w.vehicles.live() {
        if snap.n_vehicles as usize >= MAX_SNAP_VEHICLES {
            break;
        }
        if !vehicle_shown(w, s, i, known, visible) {
            continue;
        }
        let seat = |x: u8| if x != 0 && known(x as usize - 1) { x } else { 0 };
        snap.vehicles[snap.n_vehicles as usize] = VehState {
            slot: i as u8,
            pos: v.pos,
            vel: v.vel,
            yaw: v.yaw,
            health: (v.health.max(0) as i64 * 255 / sim::consts::VEH_HEALTH as i64) as u8,
            seats: [seat(v.seats[0]), seat(v.seats[1])],
        };
        snap.n_vehicles += 1;
    }
}

/// Capture the Flag's flags: at home or on the floor, where (public, as
/// Halo's waypoints are); carried, by whom, never where.
fn add_flags(snap: &mut Snapshot, w: &World) {
    if !w.rules.ctf {
        return;
    }
    for (t, f) in w.flags.iter().enumerate() {
        let held = f.state == sim::world::FLAG_HELD;
        snap.flags[t] = proto::FlagState {
            state: f.state,
            carrier: if held { f.carrier } else { 0 },
            pos: if held { V3::ZERO } else { f.pos },
            // Its clock, or its defenders' return if that is sooner.
            down_s: f.home_in.div_ceil(TICK_HZ).min(63) as u8,
        };
    }
    snap.n_flags = 2;
}

/// Pad timers are public: they say when a gun is back, not who is near.
fn add_pads(snap: &mut Snapshot, w: &World) {
    let n = w.map.pads.len().min(MAX_PADS).min(MAX_SNAP_PADS);
    for i in 0..n {
        snap.pads[i] = PadState {
            empty_s: (w.pads[i].empty_for.div_ceil(TICK_HZ)).min(proto::PAD_S_MAX as u32) as u16,
        };
    }
    snap.n_pads = n as u8;
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

/// An event filtered when it happened, again for a snapshot later: `false`
/// if it may no longer be told at all. Only ever tells less.
fn refog(e: &mut EventState, known: &impl Fn(usize) -> bool) -> bool {
    use sim::events::Kind as Ev;
    let a = e.a as usize;
    match e.kind {
        k if (k == Ev::Shot as u8 || k == Ev::Throw as u8) && e.part != HEARD && !known(a) => {
            let at = coarse(e.pos);
            e.part = HEARD;
            e.pos = at;
            e.end = at;
            true
        }
        k if k == Ev::Hit as u8 || k == Ev::Kill as u8 => {
            if !known(e.b as usize) {
                e.pos = V3::ZERO;
            }
            true
        }
        k if k == Ev::Spawn as u8 || k == Ev::Pickup as u8 || k == Ev::Melee as u8 || k == Ev::Ride as u8 => known(a),
        _ => true,
    }
}

/// What client `s` may be told of an event: all of it, a heard version
/// (rounded place, no direction), or nothing (docs/DESIGN.md §8).
fn filter_event(w: &World, s: usize, eye: V3, e: &Event, known: &impl Fn(usize) -> bool) -> Option<EventState> {
    let a = e.a as usize;
    let full = EventState {
        // The client's queue numbers it.
        id: 0,
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
        // Where someone spawned, picked up, swung, or got in or out: only if
        // seen.
        Ev::Spawn | Ev::Pickup | Ev::Melee | Ev::Ride => known(a).then_some(full),
        // A hull hit: the hitter's business and its riders'.
        Ev::VehicleHit => {
            let rides = w.fighters[s].riding().is_some_and(|(i, _)| i == e.part as usize);
            (a == s || rides).then_some(full)
        }
        // A wreck seen (its blast is sent as a Blast, heard or seen).
        Ev::Wreck => (known(a) || w.map.clear(eye, e.pos)).then_some(full),
        // A flag taken, dropped, returned or scored is announced to
        // everyone; where is a base or the flag's own public place.
        Ev::Flag => Some(full),
        Ev::None => None,
    }
}

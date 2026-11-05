//! A match as the server runs it, with no transport in it: the world, the
//! seats (people and bots), each client's input buffer, the rewind depth
//! the server chooses for it, and the snapshots it is sent
//! (docs/DESIGN.md §4, §7, §10). `crates/server` puts this behind
//! WebTransport; the netcode tests drive it directly.

pub mod fog;
pub mod medals;
pub mod replay;
pub mod watch;

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
use sim::input::Input;
use sim::map::Map;
use sim::world::{Phase, Rules, World};

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
    /// What this client was told of them, until it acks a snapshot that
    /// carried it.
    pub queue: EventQueue,
}

/// A client's events as it may be told them (filtered by `room::build`),
/// each sent in every snapshot from the first that carries it until the
/// client acks one of those (docs/DESIGN.md §10). Oldest first, never one
/// skipped: so the client keeps an event by its id and drops repeats.
#[derive(Clone, Debug, Default)]
pub struct EventQueue {
    list: Vec<(Option<u32>, EventState)>,
    next_id: u16,
    /// Events dropped because a client stopped acking.
    pub dropped: u32,
}

/// Events a client may have waiting: past this the oldest go.
pub const MAX_QUEUED_EVENTS: usize = 256;

impl EventQueue {
    fn push(&mut self, mut e: EventState) {
        e.id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if self.list.len() >= MAX_QUEUED_EVENTS {
            self.list.remove(0);
            self.dropped += 1;
        }
        self.list.push((None, e));
    }

    /// The client has every snapshot up to `tick`: what those carried is
    /// home.
    pub fn ack(&mut self, tick: u32) {
        self.list.retain(|(since, _)| since.is_none_or(|t| t > tick));
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
            last_seq: 0,
            acked: None,
            playout: PLAYOUT_MIN as u8,
            sent: [(u32::MAX, 0.0); SENT],
            rtt_ms: 0.0,
            late: 0,
            repeats: 0,
            low: None,
            pending: Vec::new(),
            queue: EventQueue::default(),
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
    /// Seats the rules use: 2 for a duel, 4 for doubles, 8 for a free for
    /// all.
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
    /// Medals and the carnage report, for every seat.
    medals: medals::Medals,
    /// The skin and emblem each person wears, checked by the server against
    /// their wallet before they were seated. Cosmetic: never reaches the world.
    looks: [proto::Look; MAX_FIGHTERS],
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
        let size = (rules.seats as usize).clamp(1, MAX_FIGHTERS);
        Room {
            rec: replay::Recorder::new(seed, salt, &rules, map.id),
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
            medals: medals::Medals::default(),
            looks: [proto::Look::default(); MAX_FIGHTERS],
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
        self.seats[s] = Seat::Person(Box::default(), name);
        self.looks[s] = proto::Look {
            skin: if (look.skin as usize) < proto::SKINS.len() {
                look.skin
            } else {
                proto::SKIN_STANDARD
            },
            emblem: proto::emblem_valid(look.emblem),
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
        out
    }

    /// A person left: a bot takes the seat, or in a ranked room nobody does
    /// and the fighter stands where it was.
    pub fn leave(&mut self, s: u8) -> Outbox {
        if self.ranked {
            self.seats[s as usize] = Seat::Empty;
            self.looks[s as usize] = proto::Look::default();
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
            skin: self.looks[s].skin,
            emblem: self.looks[s].emblem,
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

        let mut snaps = Vec::new();
        if self.world.tick.is_multiple_of(SNAP_EVERY) {
            for s in 0..MAX_FIGHTERS {
                let tick = self.world.tick;
                if let Seat::Person(c, _) = &mut self.seats[s] {
                    let lead = fog::lead_s(c.rtt_ms, c.playout as u32);
                    let f = &mut self.fogs[s];
                    f.update(&self.world, s, lead, tick);
                    let low = c.take_low();
                    let snap = build(
                        &self.world,
                        s,
                        (c.last_seq, low),
                        &c.pending,
                        &mut c.queue,
                        |q| f.shows(q, tick),
                        |i| f.shows_vehicle(i, tick),
                    );
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
/// may see coarsely. `clock` is the client's last input seq run and its
/// input depth; `visible(q)` and `vehicle_visible(i)` are fog of war's to
/// answer; everything else here is what any client may know.
pub fn build(
    w: &World,
    s: usize,
    (last_seq, depth): (u32, i8),
    events: &[Event],
    queue: &mut EventQueue,
    visible: impl Fn(usize) -> bool,
    vehicle_visible: impl Fn(usize) -> bool,
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
        n_score: if w.rules.teams { 2 } else { w.rules.seats },
        ..Snapshot::default()
    };
    // What this client may know: itself, its side, and whom fog shows it.
    let me = &w.fighters[s];
    let eye = me.eye();
    let known = |q: usize| q == s || (w.rules.teams && w.fighters[q].team == me.team) || visible(q);
    // Every kind, each by its own rule; no wildcard, so a new kind must
    // say what this client may see of it.
    for k in sim::thing::Kind::ALL {
        match k {
            sim::thing::Kind::Fighter => add_fighters(&mut snap, w, s, &visible),
            sim::thing::Kind::Projectile => add_projectiles(&mut snap, w, eye, &known),
            sim::thing::Kind::Pad => add_pads(&mut snap, w),
            sim::thing::Kind::Vehicle => add_vehicles(&mut snap, w, s, &known, &vehicle_visible),
            sim::thing::Kind::Drop => add_drops(&mut snap, w, eye),
        }
    }
    // What this client may be told of the new events joins its queue; the
    // queue goes out oldest first, as many as fit.
    for e in events {
        if let Some(e) = filter_event(w, s, eye, e, &known) {
            queue.push(e);
        }
    }
    for (_, e) in queue.list.iter().take(MAX_SNAP_EVENTS) {
        snap.events[snap.n_events as usize] = *e;
        snap.n_events += 1;
    }
    // Over a datagram, the snapshot would be dropped whole: the newest
    // events wait for the next instead.
    let mut scratch = [0u8; 2 * MAX_DATAGRAM];
    while snap.n_events > 0 && encode_snapshot(&snap, &mut scratch).map_or(true, |n| n > MAX_DATAGRAM) {
        snap.n_events -= 1;
    }
    for (since, _) in queue.list.iter_mut().take(snap.n_events as usize) {
        since.get_or_insert(w.tick);
    }
    snap
}

/// The others fog shows this client, coarsely.
fn add_fighters(snap: &mut Snapshot, w: &World, s: usize, visible: &impl Fn(usize) -> bool) {
    for (q, f) in w.fighters.iter().enumerate() {
        if q == s || !f.seated || !visible(q) || snap.n_remote as usize >= MAX_REMOTES {
            continue;
        }
        snap.remotes[snap.n_remote as usize] = Remote::of(q as u8, f);
        snap.n_remote += 1;
    }
}

/// A projectile flies in the open: sent if its owner is known to this
/// client, or it is in clear line of sight.
fn add_projectiles(snap: &mut Snapshot, w: &World, eye: V3, known: &impl Fn(usize) -> bool) {
    for (_, p) in w.proj.live() {
        if snap.n_proj as usize >= MAX_SNAP_PROJ {
            break;
        }
        if !known(p.owner as usize) && !w.map.clear(eye, p.pos) {
            continue;
        }
        snap.proj[snap.n_proj as usize] = ProjState {
            kind: p.kind as u8,
            owner: p.owner,
            pos: p.pos,
            vel: p.vel,
        };
        snap.n_proj += 1;
    }
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
        let ridden_by_known = v.seats.iter().any(|&x| x != 0 && known(x as usize - 1));
        if mine != Some(i) && !ridden_by_known && !visible(i) {
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
        Ev::None => None,
    }
}

//! The browser's netcode, with no transport in it (docs/DESIGN.md §5–6), so
//! it is tested natively and driven the same way by the page and by the
//! server's tests:
//!
//! - **clock:** the client runs ahead of the server by about its one-way
//!   latency plus a buffer, so its inputs arrive just before the server
//!   needs them. It steers on the input depth each snapshot reports, within
//!   ±5% (Gates' `client-core/src/clock.rs`).
//! - **prediction:** its own fighter, stepped with `sim`'s `step_local` the
//!   moment an input is made. Each snapshot carries the server's exact own
//!   fighter after the last input it ran: equal to the prediction there is
//!   a confirmation; different, the client adopts it and replays the rest
//!   (Gates' `client-core/src/predict.rs`).
//! - **interpolation:** everyone else, drawn between two snapshots a little
//!   behind the newest (Gates' `client-core/src/interp.rs`).

use proto::{EventState, InputMsg, Remote, Snapshot, MAX_INPUT_FRAMES, MAX_REMOTES};
use sim::consts::{MAX_FIGHTERS, TICK_HZ};
use sim::fighter::Fighter;
use sim::geom::{aim_dir, V3};
use sim::input::Input;
use sim::map::Map;
use sim::world::{Phase, Rules};

/// Inputs and predictions kept, by seq.
pub const RING: usize = 64;
/// Snapshots kept for interpolation.
pub const INTERP_FRAMES: usize = 8;
/// Input frames the clock aims to have waiting at the server at its low
/// point between our datagrams: one spare, never none.
pub const TARGET_DEPTH: f64 = 1.0;
/// Ticks between snapshots (and between input datagrams).
pub const SEND_EVERY: u32 = 2;
/// Ticks owed past which the clock skips rather than makes them all.
pub const SKIP_AFTER: u32 = TICK_HZ / 2;
/// A correction this large snaps instead of easing.
pub const SNAP_M: f32 = 4.0;
/// How fast a correction eases out, in ms.
pub const EASE_MS: f64 = 100.0;

const TICK_MS: f64 = 1000.0 / TICK_HZ as f64;

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub snapshots: u32,
    pub confirmations: u32,
    pub corrections: u32,
    pub resyncs: u32,
    /// The server's input buffer for us, as last reported.
    pub depth: i8,
    pub rate: f64,
    pub delay_ticks: f64,
}

#[derive(Clone, Copy)]
struct Frame {
    tick: u32,
    n: u8,
    remotes: [Remote; MAX_REMOTES],
}

/// A remote fighter as drawn this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drawn {
    pub slot: u8,
    pub team: u8,
    pub alive: bool,
    pub crouched: bool,
    pub grounded: bool,
    pub reloading: bool,
    pub pos: V3,
    pub vel: V3,
    /// Radians.
    pub yaw: f32,
    pub pitch: f32,
    pub arm: u8,
    pub shield: u8,
}

pub struct NetCore {
    pub slot: u8,
    pub team: u8,
    pub rules: Rules,
    pub map: Map,
    pub stats: Stats,

    // The clock, in fractional client ticks; seq is its whole part.
    t: f64,
    last_ms: Option<f64>,
    rate: f64,
    /// The next seq to make.
    seq: u32,
    inputs: [Input; RING],
    /// The predicted fighter after each seq.
    after: [Fighter; RING],
    /// The newest seq the server says it ran.
    acked_seq: u32,

    pred: Fighter,
    /// Before the newest predicted tick, for drawing between ticks.
    prev_pos: V3,
    /// A correction still easing out, added to the drawn position.
    offset: V3,

    newest: Option<u32>,
    ack_bits: u32,
    latest: Snapshot,
    frames: [Frame; INTERP_FRAMES],
    n_frames: usize,
    /// Server tick minus local ticks, smoothed: when snapshots arrive.
    clock_offset: Option<f64>,
    jitter: f64,
    delay: f64,
    events: Vec<EventState>,
    /// The welcome's commitment to the match's salt, and whether the salt
    /// revealed at the end matched it (None until then).
    pub salt_commit: [u8; 32],
    pub verified: Option<bool>,
}

impl NetCore {
    /// From the server's welcome. `tick` is the server's tick then.
    pub fn new(slot: u8, team: u8, tick: u32, rules: Rules, map: Map) -> NetCore {
        let seq = tick + 8;
        NetCore {
            slot,
            team,
            rules,
            map,
            stats: Stats {
                rate: 1.0,
                ..Stats::default()
            },
            t: seq as f64,
            last_ms: None,
            rate: 1.0,
            seq,
            inputs: [Input::default(); RING],
            after: [Fighter::default(); RING],
            acked_seq: seq.wrapping_sub(1),
            pred: Fighter::default(),
            prev_pos: V3::ZERO,
            offset: V3::ZERO,
            newest: None,
            ack_bits: 0,
            latest: Snapshot::default(),
            frames: [Frame {
                tick: 0,
                n: 0,
                remotes: [Remote::default(); MAX_REMOTES],
            }; INTERP_FRAMES],
            n_frames: 0,
            clock_offset: None,
            jitter: 0.0,
            delay: 4.0,
            events: Vec::new(),
            salt_commit: [0; 32],
            verified: None,
        }
    }

    /// The match is over and its salt revealed: check it against the
    /// welcome's commitment. Every spread in the match was drawn from it, so
    /// a replay with it reproduces every shot.
    pub fn on_reveal(&mut self, salt: [u64; 2]) -> bool {
        let ok = proto::salt_commit(salt) == self.salt_commit;
        self.verified = Some(ok);
        ok
    }

    /// Advances the clock to `now_ms`, making one input per client tick
    /// from `input` (the controls as they are now) and predicting it.
    /// Returns the input datagrams to send.
    pub fn update(&mut self, now_ms: f64, input: Input) -> Vec<InputMsg> {
        let input = input.sanitized();
        // The clock keeps real time through a stall (a tab in the
        // background, a long first frame): time dropped here would leave the
        // client behind the server, starving it for as long as the ±5% takes
        // to win the ticks back.
        let dt = self.last_ms.map_or(0.0, |l| (now_ms - l).max(0.0));
        self.last_ms = Some(now_ms);
        self.t += dt / TICK_MS * self.rate;
        // Owed more than half a second of ticks (a stall, not a slow
        // frame): skip to the newest. The server runs repeats for the ones
        // skipped. Short of that, every owed tick is made and sent: a
        // datagram goes every SEND_EVERY ticks inside the loop below, each
        // carrying the last MAX_INPUT_FRAMES.
        let owed = self.t.floor() - self.seq as f64;
        if owed > SKIP_AFTER as f64 {
            let to = (self.t.floor() as u32).wrapping_sub(SEND_EVERY);
            while self.seq != to {
                self.inputs[self.seq as usize % RING] = input.repeated();
                self.after[self.seq as usize % RING] = self.pred;
                self.seq = self.seq.wrapping_add(1);
            }
            self.stats.resyncs += 1;
        }
        let dt = dt.min(250.0);
        let mut out = Vec::new();
        while (self.seq as f64) < self.t.floor() {
            let s = self.seq;
            self.inputs[s as usize % RING] = input;
            self.prev_pos = self.pred.body.pos;
            let fired = self.pred.step_in(self.phase(), &input, &self.map).fired;
            self.after[s as usize % RING] = self.pred;
            if fired {
                self.own_shot();
            }
            self.seq = s.wrapping_add(1);
            if self.seq.is_multiple_of(SEND_EVERY) {
                out.push(self.input_msg());
            }
        }
        // Ease a correction out.
        let k = (-dt / EASE_MS).exp() as f32;
        self.offset = self.offset * k;
        out
    }

    /// Every input the server hasn't run yet, up to the cap: lost datagrams
    /// cost nothing as long as the next one gets there.
    fn input_msg(&self) -> InputMsg {
        let last = self.seq.wrapping_sub(1);
        let unacked = last.wrapping_sub(self.acked_seq).min(MAX_INPUT_FRAMES as u32);
        let first = last.wrapping_sub(unacked.max(1) - 1);
        let mut m = InputMsg {
            ack_tick: self.newest.unwrap_or(0),
            ack_bits: self.ack_bits,
            playout: self.delay.round().clamp(0.0, 255.0) as u8,
            first_seq: first,
            n: 0,
            ..InputMsg::default()
        };
        let mut s = first;
        while s != self.seq && (m.n as usize) < MAX_INPUT_FRAMES {
            m.frames[m.n as usize] = self.inputs[s as usize % RING];
            m.n += 1;
            s = s.wrapping_add(1);
        }
        m
    }

    fn own_shot(&mut self) {
        let f = &self.pred;
        let eye = f.eye();
        let dir = aim_dir(f.yaw, f.pitch);
        let range = sim::arms::arm(f.gun().arm).range.max(1.0);
        let t = self.map.ray(eye, dir, range);
        self.events.push(EventState {
            kind: sim::events::Kind::Shot as u8,
            a: self.slot,
            c: f.gun().arm as u16,
            pos: eye,
            end: eye + dir * t,
            ..EventState::default()
        });
    }

    /// A snapshot arrived at `now_ms`.
    pub fn on_snapshot(&mut self, now_ms: f64, s: &Snapshot) {
        // Acks: the newest tick, and which of the 32 before it we have.
        match self.newest {
            Some(n) if s.tick <= n => {
                let back = n - s.tick;
                if (1..=32).contains(&back) {
                    self.ack_bits |= 1 << (back - 1);
                }
                return; // Older than what we have: nothing new to draw.
            }
            Some(n) => {
                let fwd = s.tick - n;
                self.ack_bits = if fwd > 32 {
                    0
                } else {
                    (self.ack_bits << fwd) | (1 << (fwd - 1))
                };
            }
            None => {}
        }
        self.newest = Some(s.tick);
        self.stats.snapshots += 1;
        self.stats.depth = s.depth;

        // When snapshots arrive against the local clock: jitter and delay.
        let local = now_ms / TICK_MS;
        let off = s.tick as f64 - local;
        match self.clock_offset {
            None => self.clock_offset = Some(off),
            Some(o) => {
                // Late arrivals pull the estimate down at once; early ones
                // raise it slowly, so it tracks the earliest path.
                let dev = (off - o).abs();
                self.jitter += (dev - self.jitter) / 16.0;
                self.clock_offset = Some(if off < o { off } else { o + (off - o) * 0.05 });
            }
        }
        self.delay = (SEND_EVERY as f64 * 2.0 + self.jitter * 2.0).clamp(2.0, 12.0);
        self.stats.delay_ticks = self.delay;

        // The clock: keep the low point of our inputs waiting there at about
        // TARGET_DEPTH: below it the server guesses, above it we add delay.
        let d = s.depth as f64;
        if !(-8.0..=16.0).contains(&d) {
            self.t += TARGET_DEPTH - d;
            if self.t < self.seq as f64 {
                // Never take back a seq already made.
                self.t = self.seq as f64;
            }
            self.stats.resyncs += 1;
            self.rate = 1.0;
        } else if d < TARGET_DEPTH {
            self.rate = 1.05;
        } else if d > TARGET_DEPTH + 1.5 {
            self.rate = 0.95;
        } else {
            self.rate = 1.0;
        }
        self.stats.rate = self.rate;

        self.reconcile(s);

        // Others, for interpolation.
        let f = Frame {
            tick: s.tick,
            n: s.n_remote,
            remotes: s.remotes,
        };
        if self.n_frames < INTERP_FRAMES {
            self.frames[self.n_frames] = f;
            self.n_frames += 1;
        } else {
            self.frames.rotate_left(1);
            self.frames[INTERP_FRAMES - 1] = f;
        }
        for e in s.events() {
            let own_shot = e.kind == sim::events::Kind::Shot as u8 && e.a == self.slot;
            if !own_shot {
                self.events.push(*e);
            }
        }
        self.latest = *s;
    }

    fn reconcile(&mut self, s: &Snapshot) {
        let l = s.last_seq;
        let auth = s.own;
        let phase = phase_of(s.phase);
        let alive_changed = auth.alive != self.pred.alive;
        // Seqs the server can't have run yet are ours to keep predicting;
        // one it has run but we never made (the clock jumped) is a reset.
        let ahead = self.seq.wrapping_sub(l.wrapping_add(1));
        let known = ahead < RING as u32 && l.wrapping_sub(self.acked_seq) < 0x8000_0000;
        if known && !alive_changed && same_local(&self.after[l as usize % RING], &auth) {
            self.stats.confirmations += 1;
        } else {
            if !alive_changed {
                self.stats.corrections += 1;
            }
            let before = self.pred.body.pos;
            let mut f = auth;
            if ahead < RING as u32 {
                let mut q = l.wrapping_add(1);
                while q != self.seq {
                    f.step_in(phase, &self.inputs[q as usize % RING], &self.map);
                    self.after[q as usize % RING] = f;
                    q = q.wrapping_add(1);
                }
            }
            let jump = before - f.body.pos;
            self.offset = if alive_changed || (self.offset + jump).len() > SNAP_M {
                V3::ZERO
            } else {
                self.offset + jump
            };
            self.prev_pos = f.body.pos;
            self.pred = f;
        }
        if l.wrapping_sub(self.acked_seq) < 0x8000_0000 {
            self.acked_seq = l;
        }
        // What the world owns comes from the server whatever happened.
        let p = &mut self.pred;
        p.seated = auth.seated;
        p.team = auth.team;
        p.alive = auth.alive;
        p.shield = auth.shield;
        p.health = auth.health;
        p.respawn = auth.respawn;
        p.kills = auth.kills;
        p.deaths = auth.deaths;
    }

    /// Our own fighter as predicted, its body drawn at `alpha` of the way
    /// through the current tick, with any correction still easing out.
    pub fn own(&self) -> (&Fighter, V3) {
        let a = (self.t - (self.seq as f64 - 1.0)).clamp(0.0, 1.0) as f32;
        let p = self.prev_pos + (self.pred.body.pos - self.prev_pos) * a + self.offset;
        (&self.pred, p)
    }

    /// The match's phase as of the newest snapshot.
    pub fn phase(&self) -> Phase {
        if self.newest.is_none() {
            return Phase::Countdown;
        }
        phase_of(self.latest.phase)
    }

    /// The newest snapshot (score, clock, pads, projectiles).
    pub fn latest(&self) -> &Snapshot {
        &self.latest
    }

    /// Events since the last call: everyone else's from snapshots, our own
    /// shots the moment they were predicted.
    pub fn drain_events(&mut self) -> Vec<EventState> {
        core::mem::take(&mut self.events)
    }

    /// The server tick everyone else is drawn at, at `now_ms`.
    pub fn render_tick(&self, now_ms: f64) -> f64 {
        self.clock_offset.map_or(0.0, |o| now_ms / TICK_MS + o - self.delay)
    }

    /// Everyone else as drawn at `now_ms`: between the two snapshots around
    /// the render tick, or extrapolated at most two intervals past the
    /// newest, then held.
    pub fn others(&self, now_ms: f64) -> Vec<Drawn> {
        let mut out = Vec::new();
        if self.n_frames == 0 {
            return out;
        }
        let rt = self.render_tick(now_ms);
        let frames = &self.frames[..self.n_frames];
        let mut ai = 0;
        for (i, f) in frames.iter().enumerate() {
            if f.tick as f64 <= rt {
                ai = i;
            }
        }
        let a = &frames[ai];
        let b = frames.get(ai + 1).filter(|b| b.tick as f64 > rt);
        for ra in &a.remotes[..a.n as usize] {
            let mut d = drawn(ra);
            let rb = b.and_then(|b| b.remotes[..b.n as usize].iter().find(|r| r.slot == ra.slot));
            match (b, rb) {
                (Some(b), Some(rb)) if ra.alive == rb.alive => {
                    let u = ((rt - a.tick as f64) / (b.tick - a.tick) as f64).clamp(0.0, 1.0) as f32;
                    d.pos = ra.pos + (rb.pos - ra.pos) * u;
                    d.yaw = lerp_angle(d.yaw, yaw_rad(rb.yaw), u);
                    d.pitch = d.pitch + (pitch_rad(rb.pitch) - d.pitch) * u;
                }
                _ => {
                    let ahead = ((rt - a.tick as f64).max(0.0)).min((SEND_EVERY * 2) as f64) as f32;
                    if ra.alive {
                        d.pos = ra.pos + ra.vel * (ahead / TICK_HZ as f32);
                    }
                }
            }
            out.push(d);
        }
        out
    }
}

/// The fields `step_local` owns: what prediction must get exactly right.
pub fn same_local(a: &Fighter, b: &Fighter) -> bool {
    a.body == b.body
        && a.yaw == b.yaw
        && a.pitch == b.pitch
        && a.guns == b.guns
        && a.cur == b.cur
        && a.refire == b.refire
        && a.reload == b.reload
        && a.burst_left == b.burst_left
        && a.burst_gap == b.burst_gap
        && a.melee_cool == b.melee_cool
        && a.gren_cool == b.gren_cool
        && a.grenades == b.grenades
        && a.shots == b.shots
        && a.last_buttons == b.last_buttons
}

fn phase_of(p: u8) -> Phase {
    match p {
        0 => Phase::Countdown,
        1 => Phase::Playing,
        _ => Phase::Over,
    }
}

fn yaw_rad(y: u16) -> f32 {
    y as f32 * (core::f32::consts::TAU / 65536.0)
}

fn pitch_rad(p: i16) -> f32 {
    p as f32 * (core::f32::consts::TAU / 65536.0)
}

fn lerp_angle(a: f32, b: f32, u: f32) -> f32 {
    let mut d = b - a;
    let tau = core::f32::consts::TAU;
    while d > core::f32::consts::PI {
        d -= tau;
    }
    while d < -core::f32::consts::PI {
        d += tau;
    }
    a + d * u
}

fn drawn(r: &Remote) -> Drawn {
    Drawn {
        slot: r.slot,
        team: r.team,
        alive: r.alive,
        crouched: r.crouched,
        grounded: r.grounded,
        reloading: r.reloading,
        pos: r.pos,
        vel: r.vel,
        yaw: yaw_rad(r.yaw),
        pitch: pitch_rad(r.pitch),
        arm: r.arm,
        shield: r.shield,
    }
}

/// Scores by seat, for the HUD.
pub fn scores(s: &Snapshot) -> [i32; MAX_FIGHTERS] {
    s.score
}

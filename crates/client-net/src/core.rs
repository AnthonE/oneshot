//! The browser's netcode, with no transport in it (docs/DESIGN.md §5–6), so
//! it is tested natively and driven the same way by the page and by the
//! server's tests:
//!
//! - **clock:** the client runs ahead of the server by about its one-way
//!   latency plus a buffer, so its inputs arrive just before the server
//!   needs them. It steers on the input depth each snapshot reports, within
//!   ±5% (Gates' `client-core/src/clock.rs`).
//! - **prediction:** its own fighter, stepped with `sim`'s `step_local` the
//!   moment an input is made, and the vehicle it rides ([`step_own`]): at
//!   the wheel on its own input, behind someone on its driver's stick as
//!   last sent (`Snapshot::drive`), kept on. Each snapshot carries the
//!   server's exact own fighter (and vehicle) after the last input it ran:
//!   equal to the prediction there is a confirmation; different, the client
//!   adopts it and replays the rest (Gates' `client-core/src/predict.rs`).
//!   A passenger's hull is re-run from the server's on every snapshot, on
//!   the stick that came with it; the jump eases out as a correction does.
//! - **interpolation:** everyone else, drawn between two snapshots a little
//!   behind the newest (Gates' `client-core/src/interp.rs`).
//! - **rounds in flight:** on the same render tick as everyone else, ours
//!   from our predicted muzzle gliding onto it, and the cosmetic events
//!   held for that tick too ([`crate::proj`]).

use crate::proj::{self, Seed, GLIDE, HOLD_MAX_MS, HOST_ME, MAX_LAUNCHES, NO_HOST, OWN_AHEAD_MS};
use proto::{
    EventState, InputMsg, ProjState, Remote, Snapshot, VehState, MAX_INPUT_FRAMES, MAX_REMOTES, MAX_SNAP_PROJ,
    MAX_SNAP_VEHICLES,
};
use sim::consts::{ticks, MAX_FIGHTERS, MAX_VEHICLES, SHARE_FULL, TARGETS, TICK_HZ};
use sim::contact::{Man, Present, Scene, Sticks};
use sim::fighter::Fighter;
use sim::geom::{aim_dir, V3};
use sim::input::Input;
use sim::map::Map;
use sim::vehicle::{Vehicle, DRIVER};
use sim::world::{Phase, Rules};
use std::cell::Cell;
use std::collections::VecDeque;

/// One tick of what this client predicts, in the server's order
/// (`World::play`): our fighter's own tick (`step_in`), then the contact
/// step over `sc` (`sim::contact`): the others in `walkers` walk on their
/// inputs, each hull drives on its stick (none: parked), hulls crash,
/// everyone riding sits, hulls run down whoever they meet. Nothing the
/// server would deal is dealt here. `own` is our seat in the scene.
#[allow(clippy::too_many_arguments)]
pub fn step_near(
    f: &mut Fighter,
    own: usize,
    sc: &mut Scene,
    phase: Phase,
    inputs: &[Input; MAX_FIGHTERS],
    sticks: &Sticks,
    walkers: u16,
    map: &Map,
) -> sim::fighter::Local {
    let l = f.step_in(phase, &inputs[own], map);
    sc.men[own] = Man::of(f);
    if phase == Phase::Playing {
        sc.walk(inputs, walkers & !(1 << own), map);
        sc.drive(sticks, map);
        let _ = sc.crash(map);
        sc.seat();
        sc.run_down(&mut Present);
        if sim::consts::SOLID_HULLS {
            sc.push_out(map);
        }
    }
    f.body = sc.men[own].body;
    l
}

/// The hulls in our bubble (`VehState::near`), as predicted to the newest
/// seq, each on the stick it was last sent with (None: parked); and who we
/// know rides them, seated, for their sides (a teammate's hull never runs
/// us down).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Near {
    /// Which slots are in, a bit each.
    pub mask: u8,
    pub hulls: [Vehicle; MAX_VEHICLES],
    pub sticks: Sticks,
    /// Who we know rides them, seated; and who is on foot by them (in
    /// `walkers`, a bit a seat), each walking on the input it last ran.
    pub men: [Man; MAX_FIGHTERS],
    pub walkers: u16,
    pub inputs: [Input; MAX_FIGHTERS],
}

/// What of the hulls in our bubble, and those on foot by them, a seq left:
/// what they are drawn from between our past and our present.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct NearAt {
    mask: u8,
    hulls: [Vehicle; MAX_VEHICLES],
    walkers: u16,
    feet: [V3; MAX_FIGHTERS],
}

impl NearAt {
    fn of(n: &Near) -> NearAt {
        NearAt {
            mask: n.mask,
            hulls: n.hulls,
            walkers: n.walkers,
            feet: n.men.map(|m| m.body.pos),
        }
    }
}

/// [`step_near`] for all this client predicts: our fighter (seat `own`),
/// the hull it rides (at the wheel on our input, behind someone on
/// `stick`), and the hulls in our bubble on theirs: so a crash into one, or
/// being shoved by one on foot, is predicted rather than corrected.
#[allow(clippy::too_many_arguments)]
pub fn step_bubble(
    f: &mut Fighter,
    own: usize,
    v: &mut Option<Vehicle>,
    near: &mut Near,
    phase: Phase,
    inp: &Input,
    stick: Option<(i8, i8)>,
    spare_mates: bool,
    map: &Map,
) -> sim::fighter::Local {
    let mut sc = Scene {
        men: near.men,
        spare_mates,
        ..Scene::default()
    };
    for (i, h) in sc.hulls.iter_mut().enumerate() {
        if near.mask & (1 << i) != 0 {
            *h = near.hulls[i];
        }
    }
    let mut sticks = near.sticks;
    let ridden = match (f.riding(), *v) {
        (Some((i, seat)), Some(h)) => {
            sc.hulls[i] = h;
            sticks[i] = if seat == DRIVER {
                f.active().then_some((inp.mx, inp.mz))
            } else {
                stick
            };
            Some(i)
        }
        _ => None,
    };
    let mut inputs = near.inputs;
    inputs[own] = *inp;
    let body = f.body;
    let l = step_near(f, own, &mut sc, phase, &inputs, &sticks, near.walkers, map);
    if let (Some(i), Some(h)) = (ridden, v.as_mut()) {
        *h = sc.hulls[i];
    } else if f.riding().is_some() {
        // Riding with no hull given: where it was, never a blank's seat.
        f.body = body;
    }
    for i in 0..MAX_VEHICLES {
        if near.mask & (1 << i) != 0 && ridden != Some(i) {
            near.hulls[i] = sc.hulls[i];
        }
    }
    for (q, (r, m)) in near.men.iter_mut().zip(&sc.men).enumerate() {
        if r.ride != 0 || near.walkers & (1 << q) != 0 {
            r.body = m.body;
        }
    }
    l
}

/// [`step_near`] for our fighter and the hull it rides alone: at the wheel
/// the hull drives on our own input; behind someone, on its driver's
/// `stick` (`mx`, `mz`, all of an input a hull reads), and with none it is
/// parked, as the server steps a hull nobody drives.
pub fn step_own(
    f: &mut Fighter,
    v: &mut Option<Vehicle>,
    phase: Phase,
    inp: &Input,
    stick: Option<(i8, i8)>,
    map: &Map,
) -> sim::fighter::Local {
    let mut sc = Scene::default();
    let mut sticks: Sticks = [None; MAX_VEHICLES];
    let ridden = match (f.riding(), *v) {
        (Some((i, seat)), Some(h)) => {
            sc.hulls[i] = h;
            sticks[i] = if seat == DRIVER {
                f.active().then_some((inp.mx, inp.mz))
            } else {
                stick
            };
            Some(i)
        }
        _ => None,
    };
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0] = *inp;
    let body = f.body;
    let l = step_near(f, 0, &mut sc, phase, &inputs, &sticks, 0, map);
    if let (Some(i), Some(h)) = (ridden, v.as_mut()) {
        *h = sc.hulls[i];
    } else if f.riding().is_some() {
        // Riding with no hull given: where it was, never a blank's seat.
        f.body = body;
    }
    l
}

/// Inputs and predictions kept, by seq.
pub const RING: usize = 64;
/// Past this many ticks to re-run, the hulls in our bubble are dropped from
/// the guess (a stick held that long is no guess): ours alone is re-run.
pub const MAX_RERUN: u32 = 40;
/// A hull in our bubble whose re-run moves it eases there: a metre or more
/// over [`EASE_FAR_MS`], a quarter or less over this; past these, it snaps.
pub const MEMBER_EASE_NEAR_MS: f64 = 200.0;
pub const MEMBER_SNAP_M: f32 = 2.0;
pub const MEMBER_SNAP_TURN: f32 = 0.5;
/// Snapshots kept for interpolation.
pub const INTERP_FRAMES: usize = 8;
/// Input frames the clock aims to have waiting at the server at its low
/// point between our datagrams: one spare, never none.
pub const TARGET_DEPTH: f64 = 1.0;
/// How much of the run-up to a death the kill cam shows, in ms.
pub const KILLCAM_MS: u32 = 2500;
/// Snapshots kept for it: the window and a little more.
pub const KILLCAM_FRAMES: usize = (KILLCAM_MS as usize * TICK_HZ as usize / 1000) / SEND_EVERY as usize + 8;
/// Ticks between snapshots, and the fewest new input frames a datagram
/// goes for (`NetCore::update`).
pub const SEND_EVERY: u32 = 2;
/// Ticks owed past which the clock skips rather than makes them all.
pub const SKIP_AFTER: u32 = TICK_HZ / 2;
/// A correction this large snaps instead of easing; for a vehicle's
/// heading, this many radians.
pub const SNAP_M: f32 = 4.0;
pub const SNAP_TURN: f32 = 1.0;
/// How fast a correction eases out, in ms: a big one (a metre or more)
/// quickly, a small one (a quarter metre or less) slowly enough not to be
/// seen, blended between (Gates' `Predictor::decay_error`; DESIGN §5).
pub const EASE_FAR_MS: f64 = 100.0;
pub const EASE_NEAR_MS: f64 = 325.0;
const EASE_FAR_M: f32 = 1.0;
const EASE_NEAR_M: f32 = 0.25;
/// A correction left smaller than this is dropped.
pub const DEAD_ZONE_M: f32 = 0.01;
/// How far everyone else may be drawn past the newest snapshot, on their
/// velocity, before they are held (Gates' `EXTRAP_MAX_TICKS`, 133 ms).
pub const EXTRAPOLATE_MS: u32 = 125;
/// Two snapshots further apart than this aren't drawn between: whoever
/// moved that far unseen is shown where they were, then where they are.
pub const LERP_SPAN_MS: u32 = 250;
/// The server's clock as estimated from arrivals: each snapshot moves the
/// estimate a tenth of the way to what it says; what is drawn follows the
/// estimate at 90 to 110% of real time while within two ticks of it, and
/// faster the further off, up to half again at [`CLOCK_FAR_TICKS`] more (a
/// latency step: caught up in a fraction of a second, never back).
/// Snapshots this far early re-take it at once (it only moves everyone on);
/// late ones never do, so a stall's burst can't pull everyone back.
const CLOCK_GAIN: f64 = 0.1;
const CLOCK_SLEW: f64 = 0.1;
const CLOCK_SLEW_FAR: f64 = 0.5;
const CLOCK_FAR_TICKS: f64 = 10.0;
const CLOCK_SNAP_MS: u32 = 150;
/// One snapshot's say in the jitter, at most, in ticks: a stall's burst
/// shouldn't hold the delay at its cap for seconds.
const JITTER_SAMPLE_MAX: f64 = 8.0;
/// How fast the playout delay follows its target, ticks a second: up
/// briskly when the link gets worse, down gently (Gates slews at 0.5 a
/// 30 Hz tick a second).
const DELAY_UP_PER_S: f64 = 4.0;
const DELAY_DOWN_PER_S: f64 = 1.0;

/// A number drawn from that follows its target at a bounded rate of real
/// time, worked out at the moment it's read: however snapshots bunch up,
/// no frame moves it faster than the rate.
#[derive(Clone, Copy, Debug)]
struct Slew {
    from: f64,
    target: f64,
    /// When `from` held, local ms.
    at_ms: f64,
    /// Most it moves per tick of real time, up and down.
    up: f64,
    down: f64,
}

impl Slew {
    fn new(v: f64, up: f64, down: f64) -> Slew {
        Slew {
            from: v,
            target: v,
            at_ms: 0.0,
            up,
            down,
        }
    }

    fn at(&self, now_ms: f64) -> f64 {
        let dt = ((now_ms - self.at_ms) / TICK_MS).max(0.0);
        self.from + (self.target - self.from).clamp(-self.down * dt, self.up * dt)
    }

    fn aim(&mut self, now_ms: f64, target: f64) {
        self.from = self.at(now_ms);
        self.at_ms = now_ms;
        self.target = target;
    }

    /// As [`Self::aim`], at new rates.
    fn aim_at(&mut self, now_ms: f64, target: f64, up: f64, down: f64) {
        self.aim(now_ms, target);
        self.up = up;
        self.down = down;
    }

    fn set(&mut self, now_ms: f64, v: f64) {
        self.from = v;
        self.target = v;
        self.at_ms = now_ms;
    }
}
/// How far past the welcome's tick the first snapshot may be, in ms. A
/// snapshot from before the welcome (a cup's last heat, still sending as
/// the next one seats us) is far ahead of a new room's clock; taken
/// first, it would make every snapshot of the new room look old.
pub const FIRST_WITHIN: u32 = 2000;

const TICK_MS: f64 = 1000.0 / TICK_HZ as f64;

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub snapshots: u32,
    pub confirmations: u32,
    pub corrections: u32,
    /// Riding behind someone, snapshots whose hull wasn't the one predicted
    /// on its driver's stick kept on (the driver moved it): re-run, the
    /// jump eased. Not a correction: nothing of ours was wrong.
    pub hull_corrections: u32,
    /// Snapshots with hulls in our bubble, and the ticks re-run in all.
    pub near_snaps: u32,
    pub rerun_ticks: u64,
    pub resyncs: u32,
    /// The server's input buffer for us, as last reported.
    pub depth: i8,
    pub rate: f64,
    pub delay_ticks: f64,
    /// Ticks between the server's snapshots, as they come: the least gap
    /// seen (a mode may have its own rate); 0 before two have.
    pub snap_every: u32,
}

/// One snapshot as interpolation keeps it: everyone else, the vehicles,
/// and the rounds in flight with the ids and hosts matched to them.
#[derive(Clone, Copy)]
pub(crate) struct Frame {
    pub(crate) tick: u32,
    n: u8,
    remotes: [Remote; MAX_REMOTES],
    /// Which of `remotes` are held, not updated (`Snapshot::held`): drawn
    /// from their own updates either side.
    held: u16,
    nv: u8,
    vehicles: [VehState; MAX_SNAP_VEHICLES],
    pub(crate) np: u8,
    pub(crate) proj: [ProjState; MAX_SNAP_PROJ],
    /// Each round's id (`proj::assign_ids`), and who it is stuck to
    /// (`proj::detect_hosts`).
    pub(crate) pid: [u16; MAX_SNAP_PROJ],
    pub(crate) host: [u8; MAX_SNAP_PROJ],
    /// The round is new in this frame: never drawn before it.
    pub(crate) fresh: [bool; MAX_SNAP_PROJ],
    /// Our own fighter and its vehicle, exactly: what a watcher draws
    /// between ([`NetCore::watch`]).
    own: Fighter,
    own_v: Option<Vehicle>,
}

impl Frame {
    fn empty() -> Frame {
        Frame {
            tick: 0,
            n: 0,
            remotes: [Remote::default(); MAX_REMOTES],
            held: 0,
            nv: 0,
            vehicles: [VehState::default(); MAX_SNAP_VEHICLES],
            np: 0,
            proj: [ProjState::default(); MAX_SNAP_PROJ],
            pid: [0; MAX_SNAP_PROJ],
            host: [NO_HOST; MAX_SNAP_PROJ],
            fresh: [false; MAX_SNAP_PROJ],
            own: Fighter::default(),
            own_v: None,
        }
    }
}

/// A round in flight as drawn this frame. `id` holds from frame to frame
/// while it flies (ours keeps the one it launched with).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DrawnProj {
    pub id: u16,
    /// `ProjState::kind`.
    pub kind: u8,
    pub owner: u8,
    pub pos: V3,
    pub vel: V3,
}

impl DrawnProj {
    /// As the wire has it, for what draws from a `ProjState`.
    pub fn state(&self) -> ProjState {
        ProjState {
            kind: self.kind,
            owner: self.owner,
            pos: self.pos,
            vel: self.vel,
        }
    }
}

/// One of our own rounds from the tick we fired it, before and while the
/// server's copy of it is in the frames.
#[derive(Clone, Copy, Debug)]
struct Launch {
    /// The seq it was fired on, and when, local ms.
    q: u32,
    at_ms: f64,
    seed: Seed,
    /// Server tick minus seq when fired: seq `q` runs on tick `q + k`.
    k: f64,
    /// Its drawn id; once the server's round is bound to it, that round's
    /// id in the frames too.
    id: u16,
    bound: bool,
    /// Unbound, the tick of the first snapshot that ran `q`: it goes when
    /// the render tick gets there (its blast, or a mispredicted shot).
    ack_tick: Option<u32>,
    /// Unbound and acked, the tick it was drawn at then: its end is known,
    /// so it is held there till it goes, never flown on through a body.
    held_at: Option<f64>,
    /// Ticks taken off its drawn tick: a clock resync that jumped our seqs
    /// on moved our body, not the round already in the air.
    skip: f64,
    /// What is added to where it is drawn: the drawn body's correction at
    /// the launch, then whatever keeps a snapshot from moving it, easing
    /// out (`ease`) while it flies.
    off: V3,
}

/// An event held for the render tick to reach the snapshot it came in.
#[derive(Clone, Copy, Debug)]
struct Held {
    tick: u32,
    at_ms: f64,
    e: EventState,
    /// What in our bubble the event is of, as a target (a hull it hit or
    /// wrecked, the one its maker rides, or its maker on foot), and where
    /// the snapshot had it: the event plays when its drawn tick gets
    /// there, moved with it.
    member: Option<(u8, V3)>,
}

/// One snapshot as the kill cam keeps it: everyone this client was shown,
/// and itself.
#[derive(Clone, Copy)]
struct CamFrame {
    tick: u32,
    n: u8,
    remotes: [Remote; MAX_REMOTES],
    me: Remote,
}

/// A death the kill cam can show: when, who, and when we heard.
#[derive(Clone, Copy, Debug)]
struct Death {
    tick: u32,
    killer: u8,
    at_ms: f64,
}

/// The kill cam at one moment: the killer's eye and aim, and every body as
/// it stood then, ourselves among them.
#[derive(Clone, Debug)]
pub struct KillCam {
    pub killer: u8,
    pub eye: V3,
    /// Radians.
    pub yaw: f32,
    pub pitch: f32,
    /// How far through the run-up, 0..1 (1: the kill, held).
    pub progress: f32,
    pub bodies: Vec<Drawn>,
}

/// A vehicle as drawn this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DrawnVehicle {
    pub slot: u8,
    pub pos: V3,
    /// Radians.
    pub yaw: f32,
    pub health: u8,
    pub seats: [u8; 2],
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
    /// An overshield on, and how hidden its camouflage keeps it (0..1).
    pub over: bool,
    pub cloak: f32,
}

pub struct NetCore {
    pub slot: u8,
    pub team: u8,
    pub rules: Rules,
    pub map: Map,
    pub stats: Stats,
    /// Watching, not playing: a tape's session (`tape::Replay`). With no
    /// input to predict from, our own fighter is drawn between the
    /// snapshots at the render tick, as everyone else is, and the clock is
    /// the snapshots' alone; nothing is made to send.
    pub watch: bool,

    // The clock, in fractional client ticks; seq is its whole part.
    t: f64,
    last_ms: Option<f64>,
    rate: f64,
    /// The next seq to make.
    seq: u32,
    inputs: [Input; RING],
    /// Buttons seen down since the last tick was made, ORed: a press that
    /// begins and ends between two ticks still reaches one, and only one
    /// (Gates' `sticky_buttons`).
    sticky: u16,
    /// The predicted fighter after each seq, and the vehicle it rode.
    after: [Fighter; RING],
    after_v: [Option<Vehicle>; RING],
    /// Frames made since the last input datagram.
    unsent: u32,
    /// The newest seq the server says it ran.
    acked_seq: u32,
    /// The server's tick at the welcome: the first snapshot must be near
    /// it ([`FIRST_WITHIN`]).
    welcome_tick: u32,

    pred: Fighter,
    pred_v: Option<Vehicle>,
    /// Riding behind someone, its driver's stick as the newest snapshot
    /// said: what the hull is predicted on until the next says otherwise.
    stick: Option<(i8, i8)>,
    /// The hulls in our bubble, as predicted to the newest seq; and after
    /// each seq, which were in and where (drawn from, between our past and
    /// our present, by their shares).
    near: Near,
    near_after: Box<[NearAt; RING]>,
    /// Each hull's share of our rewind as the two newest snapshots said (15
    /// for one not in our bubble), and when the newest came: drawn sliding
    /// from the one to the other over a snapshot's time.
    share: [(u8, u8); TARGETS],
    share_at: f64,
    /// A hull in our bubble's correction still easing out: its middle, and
    /// its heading in radians.
    moff: [V3; MAX_VEHICLES],
    mturn: [f32; MAX_VEHICLES],
    /// One on foot in it: its correction still easing out.
    boff: [V3; MAX_FIGHTERS],
    /// Teammates never run each other down (sides, friendly fire off).
    spare_mates: bool,
    /// Before the newest predicted tick, for drawing between ticks (the
    /// body, and the vehicle's middle and heading).
    prev_pos: V3,
    prev_v: V3,
    prev_vyaw: u16,
    /// A correction still easing out, added to the drawn position; and the
    /// driven vehicle's, its middle and its heading (radians).
    offset: V3,
    voffset: V3,
    vyaw_off: f32,

    newest: Option<u32>,
    latest: Snapshot,
    frames: [Frame; INTERP_FRAMES],
    n_frames: usize,
    /// Server tick minus local ticks, smoothed (when snapshots arrive):
    /// the estimate is the target, what's drawn slews to it. None until
    /// the first snapshot.
    clock: Option<Slew>,
    jitter: f64,
    /// The playout delay in ticks, slewed to its target.
    delay: Slew,
    /// Events to hand on at once (hits, kills, our own), and those held
    /// for the render tick (everyone else's shots, blasts, ...).
    events: Vec<EventState>,
    held: VecDeque<Held>,
    /// The newest event id kept: an event comes again until acked.
    event_id: Option<u16>,
    /// The next round id to give, and our own rounds since they left the gun.
    next_pid: u16,
    launches: Vec<Launch>,
    /// Each launch's flight as last asked for, by its id (`coast_memo`).
    coasts: [Cell<Option<proj::Coasted>>; MAX_LAUNCHES],
    /// Each bound launch's run-on past the server's newest state of it,
    /// by its id: drawn ahead every frame, flown on a tick or two a frame,
    /// not from that state each time (`proj::sample`).
    run_ons: [Cell<Option<proj::Coasted>>; MAX_LAUNCHES],
    /// Ticks a forward clock resync added that the next `update` makes on
    /// top of the time passed: our rounds already flying don't take them.
    resync_ahead: f64,
    /// After a forward resync, the seq the server must have from us before
    /// its depth says anything new: until then each snapshot still reports
    /// the shortfall the jump already made up, and taking it again would
    /// jump again (a stalled page's queue of them ran the clock minutes
    /// ahead).
    steer_from: Option<u32>,
    /// Forward resync since the last `update`, in ticks (all of it, and the
    /// part `resync_ahead` took): a stall's shortfall was our own silence,
    /// which the update's real time already makes up.
    jumped: (f64, f64),
    /// The last few seconds as this client was sent them, for the kill cam.
    /// Nothing in it is more than the snapshots already said: fog shaped
    /// it on the server.
    cam: Vec<CamFrame>,
    cam_at: usize,
    death: Option<Death>,
    /// The welcome's commitment to the match's salt, and whether the salt
    /// revealed at the end matched it (None until then).
    pub salt_commit: [u8; 32],
    pub verified: Option<bool>,
}

impl NetCore {
    /// The tick we've predicted up to (a report's "when").
    pub fn tick(&self) -> u32 {
        self.seq
    }

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
            watch: false,
            t: seq as f64,
            last_ms: None,
            rate: 1.0,
            seq,
            inputs: [Input::default(); RING],
            sticky: 0,
            after: [Fighter::default(); RING],
            after_v: [None; RING],
            unsent: 0,
            acked_seq: seq.wrapping_sub(1),
            welcome_tick: tick,
            pred: Fighter::default(),
            pred_v: None,
            stick: None,
            near: Near::default(),
            near_after: Box::new([NearAt::default(); RING]),
            share: [(SHARE_FULL, SHARE_FULL); TARGETS],
            share_at: 0.0,
            moff: [V3::ZERO; MAX_VEHICLES],
            mturn: [0.0; MAX_VEHICLES],
            boff: [V3::ZERO; MAX_FIGHTERS],
            spare_mates: rules.teams && !rules.friendly_fire,
            prev_pos: V3::ZERO,
            prev_v: V3::ZERO,
            prev_vyaw: 0,
            offset: V3::ZERO,
            voffset: V3::ZERO,
            vyaw_off: 0.0,
            newest: None,
            latest: Snapshot::default(),
            frames: [Frame::empty(); INTERP_FRAMES],
            n_frames: 0,
            clock: None,
            jitter: 0.0,
            delay: Slew::new(
                SEND_EVERY as f64 * 2.0,
                DELAY_UP_PER_S / TICK_HZ as f64,
                DELAY_DOWN_PER_S / TICK_HZ as f64,
            ),
            events: Vec::new(),
            held: VecDeque::new(),
            event_id: None,
            next_pid: 0,
            launches: Vec::with_capacity(MAX_LAUNCHES),
            coasts: Default::default(),
            run_ons: Default::default(),
            resync_ahead: 0.0,
            steer_from: None,
            jumped: (0.0, 0.0),
            cam: Vec::with_capacity(KILLCAM_FRAMES),
            cam_at: 0,
            death: None,
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
        let mut out = Vec::new();
        self.update_each(now_ms, input, |m| out.push(*m));
        out
    }

    /// [`NetCore::update`], handing each input datagram to `send` as it is
    /// made rather than in a Vec (the frame's path allocates nothing).
    pub fn update_each(&mut self, now_ms: f64, input: Input, mut send: impl FnMut(&InputMsg)) {
        if self.watch {
            self.last_ms = Some(now_ms);
            self.watched(now_ms);
            return;
        }
        let input = input.sanitized();
        self.sticky |= input.buttons;
        // The clock keeps real time through a stall (a tab in the
        // background, a long first frame): time dropped here would leave the
        // client behind the server, starving it for as long as the ±5% takes
        // to win the ticks back.
        let dt = self.last_ms.map_or(0.0, |l| (now_ms - l).max(0.0));
        self.last_ms = Some(now_ms);
        self.t += dt / TICK_MS * self.rate;
        // Owed more than half a second of ticks (a stall, not a slow
        // frame): skip to the newest. The server runs repeats for the ones
        // skipped. Short of that, every owed tick is made and sent, each
        // datagram carrying the last MAX_INPUT_FRAMES: one at the end of
        // the page's frame once SEND_EVERY are new, and one inside the loop
        // whenever a datagram's worth piles up. The server rewinds a
        // frame's shots to the view of the datagram it first came in, so
        // at 64 Hz or slower every frame leaves with the page frame that
        // made it (sent in the loop on even seqs, a frame's last often
        // waited for the next, two ticks on at 60 Hz); faster, one may wait
        // a page frame, a tick at most.
        let (jumped, jumped_unseen) = core::mem::take(&mut self.jumped);
        if dt / TICK_MS > SKIP_AFTER as f64 {
            // A stall: what the snapshots said meanwhile was the server
            // running dry on our silence, not our clock. Real time makes
            // that up; a resync's jump on top would put us ahead.
            self.t = (self.t - jumped).max(self.seq as f64);
            self.resync_ahead = (self.resync_ahead - jumped_unseen).max(0.0);
            self.steer_from = Some(self.t.floor() as u32);
        }
        let owed = self.t.floor() - self.seq as f64;
        if owed > SKIP_AFTER as f64 {
            let to = (self.t.floor() as u32).wrapping_sub(SEND_EVERY);
            while self.seq != to {
                self.inputs[self.seq as usize % RING] = input.repeated();
                self.after[self.seq as usize % RING] = self.pred;
                self.after_v[self.seq as usize % RING] = self.pred_v;
                self.seq = self.seq.wrapping_add(1);
            }
            self.stats.resyncs += 1;
        }
        let dt = dt.min(250.0);
        // A resync's jump moves our drawn body on, but not our rounds in the
        // air: their drawn tick keeps going from where it was.
        let skip = core::mem::take(&mut self.resync_ahead);
        for l in self.launches.iter_mut() {
            l.skip += skip;
        }
        while (self.seq as f64) < self.t.floor() {
            let s = self.seq;
            // The first tick made takes every button seen since the last.
            let input = Input {
                buttons: input.buttons | core::mem::take(&mut self.sticky),
                ..input
            };
            self.inputs[s as usize % RING] = input;
            self.prev_pos = self.pred.body.pos;
            let was = self.pred_v;
            let phase = self.phase();
            let local = step_bubble(
                &mut self.pred,
                self.slot as usize,
                &mut self.pred_v,
                &mut self.near,
                phase,
                &input,
                self.stick,
                self.spare_mates,
                &self.map,
            );
            // A vehicle just got into starts where it is, not at the origin.
            if let Some(v) = was.or(self.pred_v) {
                self.prev_v = v.pos;
                self.prev_vyaw = v.yaw;
            }
            self.after[s as usize % RING] = self.pred;
            self.after_v[s as usize % RING] = self.pred_v;
            self.near_after[s as usize % RING] = NearAt::of(&self.near);
            if local.fired {
                self.own_shot();
            }
            if local.mounted {
                self.own_turret_shot();
            }
            self.launch(s, now_ms, &local);
            self.seq = s.wrapping_add(1);
            self.unsent += 1;
            if self.unsent >= MAX_INPUT_FRAMES as u32 {
                send(&self.input_msg(now_ms));
                self.unsent = 0;
            }
        }
        if self.unsent >= SEND_EVERY {
            send(&self.input_msg(now_ms));
            self.unsent = 0;
        }
        // Ease corrections out. Riding, the body is drawn on the hull: its
        // own offset is the hull's, and must not linger for getting out.
        self.offset = if self.pred.riding().is_some() {
            V3::ZERO
        } else {
            ease(self.offset, dt)
        };
        self.voffset = ease(self.voffset, dt);
        for (o, t) in self.moff.iter_mut().zip(self.mturn.iter_mut()) {
            *o = ease_by(*o, dt, MEMBER_EASE_NEAR_MS);
            *t = ease_by(V3::new(*t * 5.0, 0.0, 0.0), dt, MEMBER_EASE_NEAR_MS).x * 0.2;
        }
        for o in self.boff.iter_mut() {
            *o = ease_by(*o, dt, MEMBER_EASE_NEAR_MS);
        }
        // A heading's correction: a fifth of a radian as a metre is.
        let turn = ease(V3::new(self.vyaw_off * 5.0, 0.0, 0.0), dt).x * 0.2;
        self.vyaw_off = turn;
        // Our rounds' corrections ease out as the body's do, but for one
        // whose end has come: it stays where it is drawn till its blast.
        let frames = &self.frames[..self.n_frames];
        for l in self.launches.iter_mut() {
            let ended = if l.bound {
                proj::ended(frames, l.id)
            } else {
                l.ack_tick.is_some()
            };
            if !ended {
                l.off = ease(l.off, dt);
            }
        }
        self.free_launches(now_ms);
    }

    /// What predicted tick `q` fired, as rounds of ours to draw from the
    /// muzzle on: only once the server's clock is known (a snapshot came),
    /// and the newest refused once [`MAX_LAUNCHES`] fly.
    fn launch(&mut self, q: u32, now_ms: f64, l: &sim::fighter::Local) {
        // A watcher predicts nothing: what is watched is drawn as sent.
        if self.newest.is_none() || self.watch {
            return;
        }
        let f = &self.pred;
        let seeds = [
            l.fired.then(|| proj::seed_fire(f, false, l.charged)).flatten(),
            l.fired_left.then(|| proj::seed_fire(f, true, l.charged_left)).flatten(),
            l.throw.then(|| proj::seed_throw(f, l.sticky)),
        ];
        // The drawn body's correction: the round leaves the drawn muzzle.
        // Riding, the hull's, turn and all, where our seat is on it.
        let off = match (self.pred.riding(), self.pred_v) {
            (Some((_, seat)), Some(v)) => {
                let eased = Vehicle {
                    pos: v.pos + self.voffset,
                    yaw: v.yaw.wrapping_add(turn_of(self.vyaw_off)),
                    ..v
                };
                eased.mount(seat) - v.mount(seat)
            }
            _ => self.offset,
        };
        let k = self.latest.tick.wrapping_sub(self.latest.last_seq) as i32 as f64;
        for seed in seeds.into_iter().flatten() {
            if self.launches.len() >= MAX_LAUNCHES {
                return;
            }
            let id = self.next_pid;
            self.next_pid = self.next_pid.wrapping_add(1);
            self.launches.push(Launch {
                q,
                at_ms: now_ms,
                seed,
                k,
                id,
                bound: false,
                ack_tick: None,
                held_at: None,
                skip: 0.0,
                off,
            });
        }
    }

    /// The server tick our drawn body is on at `now_ms`, as launch `l`
    /// reckons ticks (`own()` draws between the predictions after seqs
    /// `seq - 2` and `seq - 1`).
    fn drawn_tick(&self, l: &Launch) -> f64 {
        self.seq.wrapping_sub(2) as f64 + self.alpha() as f64 + l.k
    }

    /// The tick launch `l` is drawn at, at `now_ms`, and how far that is
    /// past the render tick: our drawn body's tick on the frame it fired,
    /// gliding onto the render tick at [`GLIDE`] of real time; never more
    /// than [`OWN_AHEAD_MS`] past the newest snapshot as the clock has it
    /// now (moving on every frame, not by a snapshot's step).
    fn launch_tick(&self, l: &Launch, now_ms: f64) -> (f64, f64) {
        let rt = self.render_tick(now_ms);
        let lead = self.drawn_tick(l) - l.skip - rt;
        let o = (lead - GLIDE * (now_ms - l.at_ms) / TICK_MS).max(0.0);
        let cap = rt + self.delay.at(now_ms) + ticks(OWN_AHEAD_MS) as f64;
        let t = (rt + o).min(cap).max(rt);
        (t, o)
    }

    /// Launch `l` before (or without) the server's copy: flown from its
    /// seed to tick `t`.
    fn stand_in(&self, l: &Launch, t: f64) -> (V3, V3) {
        // The seed is a tick before the sim's first state, made on tick
        // `q + k`.
        let age = t - (l.q as f64 + l.k - 1.0);
        let (p, v, _) = proj::coast_on(
            self.coast_memo(l),
            l.seed.kind,
            l.seed.pos,
            l.seed.vel,
            age.max(0.0),
            &self.map,
        );
        (p, v)
    }

    /// Where launch `l`'s flight was last left (`proj::coast_on`).
    fn coast_memo(&self, l: &Launch) -> &Cell<Option<proj::Coasted>> {
        &self.coasts[l.id as usize % MAX_LAUNCHES]
    }

    /// Launch `l` as drawn at `now_ms`, or None once it is gone.
    fn launch_drawn(&self, l: &Launch, now_ms: f64) -> Option<DrawnProj> {
        let rt = self.render_tick(now_ms);
        let (t, _) = self.launch_tick(l, now_ms);
        let (pos, vel) = if l.bound {
            let frames = &self.frames[..self.n_frames];
            let (first, _) = proj::first_seen(frames, l.id)?;
            if t < first as f64 {
                // Not yet as far as the server's first state of it.
                self.stand_in(l, t)
            } else {
                let run_on = &self.run_ons[l.id as usize % MAX_LAUNCHES];
                let s = proj::sample(frames, l.id, (t, rt, ticks(OWN_AHEAD_MS) as f64), &self.map, run_on)?;
                self.placed(&s, now_ms)
            }
        } else {
            if l.ack_tick.is_some_and(|a| rt >= a as f64) {
                return None;
            }
            if l.ack_tick.is_none() && now_ms - l.at_ms >= 1000.0 {
                return None;
            }
            self.stand_in(l, l.held_at.map_or(t, |h| t.min(h)))
        };
        Some(DrawnProj {
            id: l.id,
            kind: l.seed.kind,
            owner: self.slot,
            pos: pos + l.off,
            vel,
        })
    }

    /// Drops the launches done with: gone, or drawn from here on exactly as
    /// anyone's round is.
    fn free_launches(&mut self, now_ms: f64) {
        let mut i = 0;
        while i < self.launches.len() {
            let l = self.launches[i];
            let keep = match self.launch_drawn(&l, now_ms) {
                None => false,
                Some(d) if l.bound => {
                    let (_, o) = self.launch_tick(&l, now_ms);
                    let plain = self.plain(l.id, now_ms);
                    !(o == 0.0 && l.off == V3::ZERO && plain.is_some_and(|p| p.pos == d.pos))
                }
                Some(_) => true,
            };
            if keep {
                i += 1;
            } else {
                self.launches.remove(i);
            }
        }
    }

    /// Round `pid` as anyone's is drawn: on the render tick.
    fn plain(&self, pid: u16, now_ms: f64) -> Option<DrawnProj> {
        let frames = &self.frames[..self.n_frames];
        let rt = self.render_tick(now_ms);
        let s = proj::sample(
            frames,
            pid,
            (rt, rt, ticks(EXTRAPOLATE_MS) as f64),
            &self.map,
            &Cell::new(None),
        )?;
        let (pos, vel) = self.placed(&s, now_ms);
        Some(DrawnProj {
            id: pid,
            kind: s.kind,
            owner: s.owner,
            pos,
            vel,
        })
    }

    /// Where a sampled round is drawn: a stuck one on its host as drawn
    /// (or where it was, if its host isn't), never run on by its offset.
    fn placed(&self, s: &proj::Sample, now_ms: f64) -> (V3, V3) {
        if s.host == NO_HOST {
            // Flying onto someone it sticks to: onto them as drawn, not as
            // the server had them (we are drawn a round trip ahead of it).
            if let Some((h, at, u)) = s.onto {
                if let Some((drawn, _)) = self.host_drawn(h, now_ms) {
                    return (s.pos + (drawn - at) * u, s.vel);
                }
            }
            return (s.pos, s.vel);
        }
        match self.host_drawn(s.host, now_ms) {
            Some((at, vel)) => (at + s.vel, vel),
            None => (s.pos, V3::ZERO),
        }
    }

    /// Host `h` (`proj::detect_hosts`) as drawn at `now_ms`: where, and
    /// how fast.
    fn host_drawn(&self, h: u8, now_ms: f64) -> Option<(V3, V3)> {
        if h == HOST_ME {
            let (f, p) = self.own();
            return Some((p, f.body.vel));
        }
        if (h as usize) < MAX_FIGHTERS {
            return self.other_at(h, now_ms).map(|d| (d.pos, d.vel));
        }
        let slot = h - MAX_FIGHTERS as u8;
        if self.pred.riding().map(|(i, _)| i as u8) == Some(slot) {
            return self.own_vehicle().map(|(v, p, _)| (p, v.vel));
        }
        self.vehicle_at(slot, now_ms).map(|v| (v.pos, V3::ZERO))
    }

    /// Our own rounds still drawn from their launch, not yet on the render
    /// tick as anyone's are (or gone at their ack).
    pub fn launches(&self) -> usize {
        self.launches.len()
    }

    /// Every round in flight as drawn at `now_ms`. Anyone's (and ours once
    /// it has glided onto it) on the render tick, from the frames its
    /// shooter is drawn from; ours from the muzzle we are drawn with
    /// (`own()`) the frame it fired, its clock run at `1 - GLIDE` until it
    /// is on the render tick. A stuck sticky rides its host as drawn.
    pub fn projectiles(&self, now_ms: f64) -> Vec<DrawnProj> {
        let mut out = Vec::new();
        self.projectiles_into(now_ms, &mut out);
        out
    }

    /// [`NetCore::projectiles`] into `out` (cleared first): a Vec kept from
    /// frame to frame allocates nothing.
    pub fn projectiles_into(&self, now_ms: f64, out: &mut Vec<DrawnProj>) {
        out.clear();
        let frames = &self.frames[..self.n_frames];
        let mut seen = [0u16; INTERP_FRAMES * MAX_SNAP_PROJ];
        let mut n_seen = 0;
        for f in frames {
            for &pid in &f.pid[..f.np as usize] {
                if seen[..n_seen].contains(&pid) {
                    continue;
                }
                seen[n_seen] = pid;
                n_seen += 1;
                if self.launches.iter().any(|l| l.bound && l.id == pid) {
                    continue;
                }
                if let Some(d) = self.plain(pid, now_ms) {
                    out.push(d);
                }
            }
        }
        for l in &self.launches {
            if let Some(d) = self.launch_drawn(l, now_ms) {
                out.push(d);
            }
        }
    }

    /// Our unbound launches meet the server's rounds in the newest frame:
    /// each the nearest of ours of its kind, new since it was fired, where
    /// we flew it (to its cone). Tried on the first snapshot that ran it,
    /// however late (the ones before it lost), and on any within
    /// `2 * SEND_EVERY` seqs of it (the server fired a tick or two late).
    /// One that never turns up is held where it is drawn and goes at the
    /// render tick of the snapshot that ran it.
    fn bind(&mut self, now_ms: f64, s: &Snapshot) {
        let ks = s.tick.wrapping_sub(s.last_seq) as i32 as f64;
        let ni = self.n_frames - 1;
        for li in 0..self.launches.len() {
            let l = self.launches[li];
            if l.bound {
                continue;
            }
            let after = s.last_seq.wrapping_sub(l.q);
            if after >= 0x8000_0000 {
                continue; // not run yet
            }
            if l.ack_tick.is_none() || after <= 2 * SEND_EVERY {
                let age = after as f64 + 1.0;
                let (at, _, _) =
                    proj::coast_on(self.coast_memo(&l), l.seed.kind, l.seed.pos, l.seed.vel, age, &self.map);
                let flown = (at - l.seed.pos).len();
                let reach = proj::BIND_M + l.seed.cone * flown;
                let born = l.q as f64 + ks;
                let frames = &self.frames[..self.n_frames];
                let f = &frames[ni];
                let mut best: Option<(f32, u16)> = None;
                for i in 0..f.np as usize {
                    let p = &f.proj[i];
                    let pid = f.pid[i];
                    if p.owner != self.slot || p.kind != l.seed.kind {
                        continue;
                    }
                    if self.launches.iter().any(|m| m.bound && m.id == pid) {
                        continue;
                    }
                    if proj::first_seen(frames, pid).is_none_or(|(t, fresh)| !fresh || (t as f64) < born) {
                        continue;
                    }
                    let d = (p.pos - at).len();
                    if d <= reach && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, pid));
                    }
                }
                if let Some((_, pid)) = best {
                    // The server's round takes our id, in every frame.
                    for f in self.frames[..self.n_frames].iter_mut() {
                        for p in f.pid[..f.np as usize].iter_mut() {
                            if *p == pid {
                                *p = l.id;
                            }
                        }
                    }
                    self.launches[li].bound = true;
                    continue;
                }
            }
            if l.ack_tick.is_none() {
                let (t, _) = self.launch_tick(&l, now_ms);
                let l = &mut self.launches[li];
                l.ack_tick = Some(s.tick);
                l.held_at = Some(t);
            }
        }
    }

    /// Every input the server hasn't run yet, up to the cap: lost datagrams
    /// cost nothing as long as the next one gets there.
    fn input_msg(&self, now_ms: f64) -> InputMsg {
        let last = self.seq.wrapping_sub(1);
        let unacked = last.wrapping_sub(self.acked_seq).min(MAX_INPUT_FRAMES as u32);
        let first = last.wrapping_sub(unacked.max(1) - 1);
        let mut m = InputMsg {
            ack_tick: self.newest.unwrap_or(0),
            playout: self.playout(now_ms),
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

    /// A round of the turret we gun, as predicted: from its muzzle on the
    /// hull as drawn (where its barrels are on screen), to where our aim
    /// meets the map.
    fn own_turret_shot(&mut self) {
        let (Some(v), Some((a, cause))) = (
            self.drawn_hull(),
            self.pred_v.and_then(|v| sim::vehicle::turret(v.turret)),
        ) else {
            return;
        };
        let f = &self.pred;
        let eye = f.eye();
        let dir = aim_dir(f.yaw, f.pitch);
        let t = self.map.ray(eye, dir, a.range);
        self.events.push(EventState {
            kind: sim::events::Kind::Shot as u8,
            a: self.slot,
            c: cause,
            pos: v.muzzle(dir),
            end: eye + dir * t,
            ..EventState::default()
        });
    }

    /// A snapshot arrived at `now_ms`.
    pub fn on_snapshot(&mut self, now_ms: f64, s: &Snapshot) {
        match self.newest {
            // Older than what we have: nothing new to draw.
            Some(n) if s.tick <= n => return,
            Some(n) => {
                let fwd = s.tick - n;
                if self.stats.snap_every == 0 || fwd < self.stats.snap_every {
                    self.stats.snap_every = fwd;
                }
            }
            None => {
                // Half a second before the welcome (sent with it) to two
                // seconds after; anything else is another room's.
                let from = self.welcome_tick.wrapping_sub(ticks(500));
                if s.tick.wrapping_sub(from) > ticks(500 + FIRST_WITHIN) {
                    return;
                }
            }
        }
        // Where our rounds are drawn before this snapshot: after it, they
        // carry what it would move them by (the clock, the binding, the
        // server's own path) as a correction to ease out, never a pop.
        let mut drawn = [None; MAX_LAUNCHES];
        let n_drawn = self.launches.len();
        for (d, l) in drawn.iter_mut().zip(&self.launches) {
            *d = self.launch_drawn(l, now_ms).map(|d| d.pos);
        }
        // The hull we rode in before this snapshot says otherwise (a wreck).
        let rode = self.pred.riding();
        self.newest = Some(s.tick);
        self.stats.snapshots += 1;
        self.stats.depth = s.depth;

        // When snapshots arrive against the local clock: the server's clock,
        // the jitter about it (RFC 3550's gain), and the playout delay. The
        // render tick they make moves on with real time, never back: a
        // late snapshot slows it a little, it doesn't pull it back (Gates'
        // `server_est` and slewed playout).
        let local = now_ms / TICK_MS;
        let off = s.tick as f64 - local;
        match self.clock.as_mut() {
            None => {
                self.clock = Some({
                    let mut c = Slew::new(off, CLOCK_SLEW, CLOCK_SLEW);
                    c.set(now_ms, off);
                    c
                });
                self.delay.set(now_ms, self.delay.target);
            }
            Some(c) => {
                let err = off - c.target;
                if err > ticks(CLOCK_SNAP_MS) as f64 {
                    c.set(now_ms, off);
                } else {
                    self.jitter += (err.abs().min(JITTER_SAMPLE_MAX) - self.jitter) / 16.0;
                    let target = c.target + err * CLOCK_GAIN;
                    // The further off what's drawn is, the faster it follows.
                    let gap = (target - c.at(now_ms)).abs();
                    let rate = (CLOCK_SLEW + (CLOCK_SLEW_FAR - CLOCK_SLEW) * (gap - 2.0) / CLOCK_FAR_TICKS)
                        .clamp(CLOCK_SLEW, CLOCK_SLEW_FAR);
                    c.aim_at(now_ms, target, rate, rate);
                }
                // Two snapshots' gap and twice the jitter: a mode whose
                // snapshots are further apart is drawn as far behind again.
                let gap = match self.stats.snap_every {
                    0 => SEND_EVERY,
                    n => n.min(8),
                };
                let target = (gap as f64 * 2.0 + self.jitter * 2.0).clamp(2.0, 12.0);
                self.delay.aim(now_ms, target);
            }
        }
        self.stats.delay_ticks = self.delay.at(now_ms);

        // The clock: keep the low point of our inputs waiting there at about
        // TARGET_DEPTH: below it the server guesses, above it we add delay.
        // A watcher sends no inputs: the depth was the player's, not ours.
        let d = s.depth as f64;
        // The newest seq of ours the server had (its last run and the depth
        // past it): short of `steer_from`, this snapshot is from before our
        // last jump reached it.
        let seen = s.last_seq.wrapping_add(s.depth as i32 as u32);
        if self.steer_from.is_some_and(|f| seen.wrapping_sub(f) < 0x8000_0000) {
            self.steer_from = None;
        }
        if self.watch {
            self.rate = 1.0;
        } else if d < -8.0 && self.steer_from.is_some() {
            // Made up already: wait for the server to see it.
            self.rate = 1.0;
        } else if !(-8.0..=16.0).contains(&d) {
            let drawn = self.own().1;
            let drawn_v = self.own_vehicle();
            let (t0, a0) = (self.t, self.alpha());
            self.t += TARGET_DEPTH - d;
            if self.t < self.seq as f64 {
                // Never take back a seq already made.
                self.t = self.seq as f64;
            }
            // Forward, the next update makes the ticks jumped all at once:
            // all but what alpha shows already (the fold below takes that).
            let unseen = (self.t - t0) - (self.alpha() - a0) as f64;
            if unseen > 0.0 {
                self.resync_ahead += unseen;
            }
            if self.t > t0 {
                self.steer_from = Some(self.t.floor() as u32);
                self.jumped.0 += self.t - t0;
                self.jumped.1 += unseen.max(0.0);
            }
            // The clock went back through the tick being drawn: what was on
            // screen eases on from where it was, as a correction does,
            // rather than step back.
            self.offset = self.offset + (drawn - self.own().1);
            if let (Some((_, p0, y0)), Some((_, p1, y1))) = (drawn_v, self.own_vehicle()) {
                self.voffset = self.voffset + (p0 - p1);
                self.vyaw_off += y0 - y1;
            }
            self.stats.resyncs += 1;
            self.rate = 1.0;
        } else {
            self.rate = clock_rate(d);
        }
        self.stats.rate = self.rate;

        // Where each hull in our bubble is drawn before this snapshot: after
        // it (re-run on what it says), a move is eased out, never a pop.
        let mut before = [None; MAX_VEHICLES];
        for (i, b) in before.iter_mut().enumerate() {
            *b = self.member_drawn(i as u8, now_ms);
        }
        let mut before_feet = [None; MAX_FIGHTERS];
        for (q, b) in before_feet.iter_mut().enumerate() {
            *b = self.body_drawn(q as u8, now_ms);
        }
        if !self.watch {
            self.reconcile(s);
        }
        // Each hull's share slides on from where it is drawn now.
        for (t, sh) in self.share.iter_mut().enumerate() {
            let drawn = share_between(*sh, self.share_at, now_ms, self.stats.snap_every);
            let to = if t < MAX_FIGHTERS {
                s.remotes()
                    .iter()
                    .find(|r| r.slot as usize == t)
                    .and_then(|r| r.near)
                    .map(|n| n.share)
            } else {
                let i = t - MAX_FIGHTERS;
                s.vehicles()
                    .iter()
                    .find(|v| v.slot as usize == i)
                    .and_then(|v| v.near)
                    .map(|n| n.share)
            };
            *sh = (drawn.round() as u8, to.unwrap_or(SHARE_FULL));
        }
        self.share_at = now_ms;

        // Others, for interpolation; the rounds in flight with their ids.
        let mut f = Frame {
            tick: s.tick,
            n: s.n_remote,
            remotes: s.remotes,
            held: s.held,
            nv: s.n_vehicles,
            vehicles: s.vehicles,
            np: s.n_proj.min(MAX_SNAP_PROJ as u8),
            proj: s.proj,
            own: s.own,
            own_v: s.own_vehicle,
            ..Frame::empty()
        };
        proj::detect_hosts(&mut f, s);
        {
            let n = self.n_frames;
            let prev = n.checked_sub(1).map(|i| &self.frames[i]);
            let prev2 = n.checked_sub(2).map(|i| &self.frames[i]);
            proj::assign_ids(prev, prev2, &mut f, &mut self.next_pid);
        }
        if self.n_frames < INTERP_FRAMES {
            self.frames[self.n_frames] = f;
            self.n_frames += 1;
        } else {
            self.frames.rotate_left(1);
            self.frames[INTERP_FRAMES - 1] = f;
        }
        let cf = CamFrame {
            tick: s.tick,
            n: s.n_remote,
            remotes: s.remotes,
            me: Remote::of(self.slot, &s.own),
        };
        if self.cam.len() < KILLCAM_FRAMES {
            self.cam.push(cf);
        } else {
            self.cam[self.cam_at] = cf;
            self.cam_at = (self.cam_at + 1) % KILLCAM_FRAMES;
        }
        self.bind(now_ms, s);
        for (i, b) in before.iter().enumerate() {
            let Some(b) = b else {
                continue;
            };
            let Some(n) = self.member_drawn(i as u8, now_ms) else {
                // Out of our bubble at once (wrecked, out of sight, us dead):
                // eased back to the render tick, never a snap across.
                let n = (self.moff[i], self.mturn[i]);
                self.moff[i] = V3::ZERO;
                self.mturn[i] = 0.0;
                if let Some(now) = self.vehicle_at(i as u8, now_ms) {
                    let jump = b.pos - now.pos;
                    let turn = (b.yaw - now.yaw + core::f32::consts::PI).rem_euclid(core::f32::consts::TAU)
                        - core::f32::consts::PI;
                    if jump.len() <= SNAP_M {
                        (self.moff[i], self.mturn[i]) = (jump, turn);
                    }
                } else {
                    (self.moff[i], self.mturn[i]) = n;
                }
                continue;
            };
            let (jump, turn) = (b.pos - n.pos, b.yaw - n.yaw);
            let turn = (turn + core::f32::consts::PI).rem_euclid(core::f32::consts::TAU) - core::f32::consts::PI;
            if (self.moff[i] + jump).len() > MEMBER_SNAP_M || (self.mturn[i] + turn).abs() > MEMBER_SNAP_TURN {
                self.moff[i] = V3::ZERO;
                self.mturn[i] = 0.0;
            } else {
                self.moff[i] = self.moff[i] + jump;
                self.mturn[i] += turn;
            }
        }
        for (q, b) in before_feet.iter().enumerate() {
            let (Some(b), Some(n)) = (b, self.body_drawn(q as u8, now_ms)) else {
                continue;
            };
            let off = self.boff[q] + (*b - n);
            self.boff[q] = if off.len() > MEMBER_SNAP_M { V3::ZERO } else { off };
        }
        // The hull we ride is drawn in the present; any other on the render
        // tick, its wreck with it.
        let ours = |i: Option<(usize, usize)>| i.map(|(i, _)| i as u8);
        let rode = [ours(rode), ours(s.own.riding())];
        for e in s.events() {
            // Seen already (sent again until a snapshot with it is acked).
            if self
                .event_id
                .is_some_and(|n| e.id.wrapping_sub(n) == 0 || e.id.wrapping_sub(n) >= 0x8000)
            {
                continue;
            }
            self.event_id = Some(e.id);
            use sim::events::Kind;
            // Our own shots were drawn when predicted; a watcher predicts
            // none, so it takes the server's.
            let own_shot = e.kind == Kind::Shot as u8 && e.a == self.slot && !self.watch;
            // What is told (hits, kills) and what is ours plays at once;
            // what is seen in the world plays on the render tick, with the
            // rounds and bodies it belongs to. Our blasts too: they end a
            // round drawn there; and our wrecks but of the hull we ride.
            let wreck = e.kind == Kind::Wreck as u8;
            let at_once = e.kind == Kind::Hit as u8
                || e.kind == Kind::Kill as u8
                || e.kind == Kind::VehicleHit as u8
                || (wreck && rode.contains(&Some(e.part)))
                || (e.a == self.slot && e.kind != Kind::Blast as u8 && !wreck);
            // One riding with us is drawn on the hull we draw, ahead of the
            // render tick: its shots play at once, from where it is drawn
            // (where the round met stays the server's).
            let co_shot = (e.kind == Kind::Shot as u8 && !self.watch)
                .then(|| self.co_rider(e.a))
                .flatten()
                .zip(self.drawn_hull())
                .zip(s.remotes().iter().find(|r| r.slot == e.a));
            if own_shot {
                // Played the tick it was predicted.
            } else if let Some(((seat, hull), r)) = co_shot {
                self.events.push(EventState {
                    pos: e.pos + (hull.mount(seat) - r.pos),
                    ..*e
                });
            } else if at_once {
                self.events.push(*e);
            } else {
                self.held.push_back(Held {
                    tick: s.tick,
                    at_ms: now_ms,
                    e: *e,
                    member: member_of(s, e),
                });
            }
            if e.kind == sim::events::Kind::Kill as u8 && e.b == self.slot {
                self.death = (e.a != self.slot).then_some(Death {
                    tick: s.tick,
                    killer: e.a,
                    at_ms: now_ms,
                });
            }
        }
        if s.own.alive {
            self.death = None;
        }
        self.latest = *s;
        for (i, &was) in drawn[..n_drawn].iter().enumerate() {
            let l = self.launches[i];
            if let (Some(p0), Some(p1)) = (was, self.launch_drawn(&l, now_ms).map(|d| d.pos)) {
                let off = l.off + (p0 - p1);
                self.launches[i].off = if off.len() > SNAP_M { V3::ZERO } else { off };
            }
        }
    }

    /// A watcher follows seat `slot` from here: its snapshots' own fighter
    /// is that seat's, and so are their events, numbered by that seat's own
    /// count (the last seat's are no measure of what is new).
    pub fn follow(&mut self, slot: u8, team: u8) {
        self.slot = slot;
        self.team = team;
        self.event_id = None;
    }

    fn reconcile(&mut self, s: &Snapshot) {
        let l = s.last_seq;
        let auth = s.own;
        let auth_v = s.own_vehicle;
        let phase = phase_of(s.phase);
        let alive_changed = auth.alive != self.pred.alive;
        // Behind someone, the hull is predicted on its driver's stick, which
        // the driver may have moved since: our fighter is checked on its
        // own (in the server's seat), and the hull is re-run from the
        // server's every time, on the stick that came with it.
        let passenger = auth.riding().is_some_and(|(_, seat)| seat != DRIVER);
        self.stick = if passenger { s.drive } else { None };
        let same = |f: &Fighter, v: &Option<Vehicle>| {
            if passenger {
                same_local(&Fighter { body: auth.body, ..*f }, &auth)
            } else {
                same_local(f, &auth) && *v == auth_v
            }
        };
        // Seqs the server can't have run yet are ours to keep predicting;
        // one it has run but we never made (the clock jumped) is a reset.
        let ahead = self.seq.wrapping_sub(l.wrapping_add(1));
        let known = ahead < RING as u32 && l.wrapping_sub(self.acked_seq) < 0x8000_0000;
        let at = l as usize % RING;
        let confirmed = known && !alive_changed && same(&self.after[at], &self.after_v[at]);
        if confirmed {
            self.stats.confirmations += 1;
        } else if !alive_changed {
            self.stats.corrections += 1;
        }
        if passenger && known && matches!(self.after_v[at], Some(p) if Some(p) != auth_v) {
            self.stats.hull_corrections += 1;
        }
        // The hulls in our bubble as the server had them, to re-run with
        // ours: none past what a re-run guesses well across.
        let mut near = Near::default();
        {
            for v in s.vehicles() {
                let (Some(n), i) = (v.near, v.slot as usize) else {
                    continue;
                };
                // Past what a re-run guesses well across, only one parked
                // (still, nobody driving): that guess holds however long.
                let parked = n.stick.is_none() && v.vel.flat().len() < 0.5;
                if ahead > MAX_RERUN && !parked {
                    continue;
                }
                let Some(def) = self.map.vehicles.get(i) else {
                    continue;
                };
                near.mask |= 1 << i;
                near.hulls[i] = Vehicle {
                    active: true,
                    pos: v.pos,
                    vel: v.vel,
                    yaw: v.yaw,
                    grounded: n.grounded,
                    health: sim::consts::VEH_HEALTH,
                    seats: v.seats,
                    respawn: 0,
                    home: v.slot,
                    turret: def.turret,
                };
                near.sticks[i] = n.stick;
                for (seat, &x) in v.seats.iter().enumerate() {
                    let q = x as usize;
                    if q == 0 || q > MAX_FIGHTERS || q - 1 == self.slot as usize {
                        continue;
                    }
                    // A rider we know: one sent, or one of ours.
                    let team = s
                        .remotes()
                        .iter()
                        .find(|r| r.slot as usize == q - 1)
                        .map_or(self.team, |r| r.team);
                    near.men[q - 1] = Man {
                        body: near.hulls[i].seat_body(seat),
                        ride: Fighter::ride_of(i, seat),
                        live: true,
                        flag: false,
                        team,
                    };
                }
            }
        }
        // Those on foot by them, each on the input it last ran.
        if ahead <= MAX_RERUN {
            for r in s.remotes() {
                let (Some(n), q) = (r.near, r.slot as usize) else {
                    continue;
                };
                if !r.alive || q >= MAX_FIGHTERS || q == self.slot as usize {
                    continue;
                }
                near.walkers |= 1 << q;
                near.men[q] = Man {
                    body: sim::movement::Body {
                        pos: r.pos,
                        vel: r.vel,
                        grounded: r.grounded,
                        crouched: r.crouched,
                        duck: n.duck,
                    },
                    ride: 0,
                    live: true,
                    flag: r.flag,
                    team: r.team,
                };
                let held = |b: bool, k: u16| if b { k } else { 0 };
                near.inputs[q] = Input {
                    buttons: held(n.jump, sim::input::JUMP) | held(n.crouch, sim::input::CROUCH),
                    yaw: r.yaw,
                    pitch: r.pitch,
                    mx: n.mx,
                    mz: n.mz,
                };
            }
        }
        if near.mask != 0 || near.walkers != 0 {
            self.stats.near_snaps += 1;
        } else {
            self.near = Near::default();
        }
        if near.mask != 0 || near.walkers != 0 {
            self.near_after[at] = NearAt::of(&near);
        }
        if !confirmed || passenger || near.mask != 0 || near.walkers != 0 {
            self.stats.rerun_ticks += ahead.min(RING as u32) as u64;
            let before = self.pred.body.pos;
            let before_v = self.pred_v;
            let mut f = auth;
            let mut v = auth_v;
            if ahead < RING as u32 {
                let mut q = l.wrapping_add(1);
                while q != self.seq {
                    step_bubble(
                        &mut f,
                        self.slot as usize,
                        &mut v,
                        &mut near,
                        phase,
                        &self.inputs[q as usize % RING],
                        self.stick,
                        self.spare_mates,
                        &self.map,
                    );
                    // Our rounds the server hasn't yet leave from where the
                    // re-run has us on their seq, not the guess that launched
                    // them (a hull predicted on a stick its driver has since
                    // moved): seeded a metre off, one would never be bound
                    // to the server's. The move eases out (`on_snapshot`).
                    let moved = f.eye() - self.after[q as usize % RING].eye();
                    if moved != V3::ZERO {
                        for l in self.launches.iter_mut().filter(|l| !l.bound && l.q == q) {
                            l.seed.pos = l.seed.pos + moved;
                        }
                    }
                    self.after[q as usize % RING] = f;
                    self.after_v[q as usize % RING] = v;
                    self.near_after[q as usize % RING] = NearAt::of(&near);
                    q = q.wrapping_add(1);
                }
            }
            self.pred_v = v;
            self.near = near;
            // The tick being drawn through moves with the correction, so
            // the step between ticks stays the one already on screen.
            // The vehicle's correction eases out with the body's, so the hull
            // glides with the camera rather than snap under it.
            match (before_v, v) {
                (Some(b), Some(n)) => {
                    let jump = b.pos - n.pos;
                    let turn = b.yaw.wrapping_sub(n.yaw);
                    self.prev_v = self.prev_v - jump;
                    self.prev_vyaw = self.prev_vyaw.wrapping_sub(turn);
                    let turn_rad = turn as i16 as f32 * (core::f32::consts::TAU / 65536.0);
                    if alive_changed
                        || (self.voffset + jump).len() > SNAP_M
                        || (self.vyaw_off + turn_rad).abs() > SNAP_TURN
                    {
                        self.voffset = V3::ZERO;
                        self.vyaw_off = 0.0;
                        self.prev_v = n.pos;
                        self.prev_vyaw = n.yaw;
                    } else {
                        self.voffset = self.voffset + jump;
                        self.vyaw_off += turn_rad;
                    }
                }
                (_, Some(n)) => {
                    self.prev_v = n.pos;
                    self.prev_vyaw = n.yaw;
                    self.voffset = V3::ZERO;
                    self.vyaw_off = 0.0;
                }
                _ => {}
            }
            let jump = before - f.body.pos;
            if alive_changed || (self.offset + jump).len() > SNAP_M {
                self.offset = V3::ZERO;
                self.prev_pos = f.body.pos;
            } else {
                self.offset = self.offset + jump;
                self.prev_pos = self.prev_pos - jump;
            }
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
        p.charging = auth.charging;
        p.camo = auth.camo;
        p.flare = auth.flare;
        p.respawn = auth.respawn;
        p.kills = auth.kills;
        p.deaths = auth.deaths;
    }

    /// How far the clock is through the tick after the newest predicted
    /// one, 0 to 1: what is drawn is that far from the tick before the
    /// newest to the newest, one tick behind the clock, so every frame
    /// moves by its own share of time (Gates' `ClientClock::alpha`).
    pub fn alpha(&self) -> f32 {
        let a = (self.t - self.seq as f64) as f32;
        if a.is_nan() {
            0.0
        } else {
            a.clamp(0.0, 1.0)
        }
    }

    /// Our own fighter as predicted, its body drawn at [`Self::alpha`]
    /// between the last two ticks, with any correction still easing out.
    /// Riding, it is drawn in its seat on the drawn hull, so the two never
    /// part (one correction, one decision to snap or ease).
    pub fn own(&self) -> (&Fighter, V3) {
        if let (Some((_, seat)), Some(drawn)) = (self.pred.riding(), self.drawn_hull()) {
            return (&self.pred, drawn.mount(seat));
        }
        let a = self.alpha();
        let p = self.prev_pos + (self.pred.body.pos - self.prev_pos) * a + self.offset;
        (&self.pred, p)
    }

    /// The vehicle we ride in, as predicted (at the wheel on our input,
    /// behind someone on its driver's stick), drawn at `alpha` through the
    /// tick: its middle and its heading in radians.
    pub fn own_vehicle(&self) -> Option<(Vehicle, V3, f32)> {
        let v = self.pred_v?;
        self.pred.riding()?;
        let a = self.alpha();
        let p = self.prev_v + (v.pos - self.prev_v) * a + self.voffset;
        // The short way round between the two headings.
        let turn = v.yaw.wrapping_sub(self.prev_vyaw) as i16 as f32;
        let yaw = yaw_rad(self.prev_vyaw) + turn * a * (core::f32::consts::TAU / 65536.0) + self.vyaw_off;
        Some((v, p, yaw))
    }

    /// The hull we ride as [`Self::own_vehicle`] draws it, as a vehicle.
    fn drawn_hull(&self) -> Option<Vehicle> {
        let (v, p, yaw) = self.own_vehicle()?;
        let turn = (yaw as f64 * (65536.0 / core::f64::consts::TAU)).round();
        Some(Vehicle {
            pos: p,
            yaw: turn.rem_euclid(65536.0) as u16,
            ..v
        })
    }

    /// Our own fighter as a body anyone would draw: where [`Self::own`]
    /// puts it, turned the way it looks (a film's camera off the eye).
    pub fn own_drawn(&self) -> Drawn {
        let (f, p) = self.own();
        Drawn {
            pos: p,
            ..drawn(&Remote::of(self.slot, f))
        }
    }

    /// A watcher's own fighter at the render tick: the snapshots' exact
    /// fighter before it, its body, aim and vehicle drawn on toward the one
    /// after (or on its velocity past the newest, then held), the way
    /// [`Self::others`] draws everyone else.
    fn watched(&mut self, now_ms: f64) {
        if self.n_frames == 0 {
            return;
        }
        let rt = self.render_tick(now_ms);
        let frames = &self.frames[..self.n_frames];
        let ai = frames.iter().rposition(|f| f.tick as f64 <= rt).unwrap_or(0);
        let a = &frames[ai];
        let b = frames.get(ai + 1).filter(|b| b.tick as f64 > rt);
        let mut f = a.own;
        let mut v = a.own_v;
        match b {
            Some(b)
                if b.own.alive == a.own.alive && b.own.ride == a.own.ride && b.tick - a.tick <= ticks(LERP_SPAN_MS) =>
            {
                let u = ((rt - a.tick as f64) / (b.tick - a.tick) as f64).clamp(0.0, 1.0) as f32;
                f.body.pos = a.own.body.pos + (b.own.body.pos - a.own.body.pos) * u;
                f.yaw = turn_between(a.own.yaw, b.own.yaw, u);
                f.pitch = (a.own.pitch as f32 + (b.own.pitch as f32 - a.own.pitch as f32) * u) as i16;
                if let (Some(va), Some(vb), Some(v)) = (a.own_v, b.own_v, v.as_mut()) {
                    v.pos = va.pos + (vb.pos - va.pos) * u;
                    v.yaw = turn_between(va.yaw, vb.yaw, u);
                }
            }
            _ => {
                let k = ahead(rt, a.tick) / TICK_HZ as f32;
                if f.alive {
                    f.body.pos = f.body.pos + f.body.vel * k;
                }
                if let Some(v) = v.as_mut() {
                    v.pos = v.pos + v.vel * k;
                }
            }
        }
        if let (Some((_, seat)), Some(v)) = (f.riding(), v.as_ref()) {
            f.body = v.seat_body(seat);
        }
        self.pred = f;
        self.pred_v = v;
        self.prev_pos = f.body.pos;
        if let Some(v) = v {
            self.prev_v = v.pos;
            self.prev_vyaw = v.yaw;
        }
        self.offset = V3::ZERO;
        self.voffset = V3::ZERO;
        self.vyaw_off = 0.0;
        // Nothing between ticks to draw through: what is drawn is the
        // render tick's own.
        self.t = self.seq as f64;
    }

    /// The vehicles we were sent (but the one we ride in), between the two
    /// snapshots around the render tick.
    pub fn vehicles(&self, now_ms: f64) -> Vec<DrawnVehicle> {
        let mut out = Vec::new();
        self.vehicles_into(now_ms, &mut out);
        out
    }

    /// [`NetCore::vehicles`] into `out` (cleared first): a Vec kept from
    /// frame to frame allocates nothing.
    pub fn vehicles_into(&self, now_ms: f64, out: &mut Vec<DrawnVehicle>) {
        out.clear();
        if self.n_frames == 0 {
            return;
        }
        let mine = self.pred.riding().map(|(i, _)| i as u8);
        let rt = self.render_tick(now_ms);
        let (ai, bi) = self.around(rt);
        let (a, b) = (&self.frames[ai], bi.map(|i| &self.frames[i]));
        for va in &a.vehicles[..a.nv as usize] {
            if Some(va.slot) != mine {
                let d = self.member_drawn(va.slot, now_ms).unwrap_or_else(|| {
                    // Just out of our bubble, sliding back still.
                    let mut d = vehicle_between(va, a.tick, b, rt);
                    d.pos = d.pos + self.moff[va.slot as usize];
                    d.yaw += self.mturn[va.slot as usize];
                    d
                });
                out.push(d);
            }
        }
        // In our bubble since the render tick: drawn already, at its share.
        let newest = &self.frames[self.n_frames - 1];
        for vn in &newest.vehicles[..newest.nv as usize] {
            let drawn_already = a.vehicles[..a.nv as usize].iter().any(|v| v.slot == vn.slot);
            if !drawn_already && Some(vn.slot) != mine {
                out.extend(self.member_drawn(vn.slot, now_ms));
            }
        }
    }

    /// The tick vehicle `slot` is drawn on at `now_ms`, in the server's
    /// ticks: the render tick, or for one in our bubble its share of the way
    /// from there to our present.
    pub fn vehicle_tick(&self, slot: u8, now_ms: f64) -> f64 {
        self.member_tick(MAX_FIGHTERS + slot as usize, now_ms)
            .unwrap_or_else(|| self.render_tick(now_ms))
    }

    /// The tick our own body is drawn on at `now_ms` (our present), in the
    /// server's ticks.
    pub fn present_tick(&self) -> f64 {
        let k = self.latest.tick.wrapping_sub(self.latest.last_seq) as i32 as f64;
        self.seq.wrapping_sub(2) as f64 + self.alpha() as f64 + k
    }

    /// The tick hull `i` is drawn on: its share of the way from our present
    /// back to the render tick (DESIGN §6; `room::bubble`). None for one
    /// drawn on the render tick, as everything outside our bubble is.
    fn member_tick(&self, t: usize, now_ms: f64) -> Option<f64> {
        let sh = *self.share.get(t)?;
        if sh == (SHARE_FULL, SHARE_FULL) || self.watch || self.n_frames == 0 {
            return None;
        }
        let s = share_between(sh, self.share_at, now_ms, self.stats.snap_every);
        let (p, r) = (self.present_tick(), self.render_tick(now_ms));
        Some(p - (s / SHARE_FULL as f64) * (p - r).max(0.0))
    }

    /// Hull `i` in our bubble as drawn at `now_ms`, at its tick: between the
    /// snapshots around it if they have it there, else between the ticks
    /// we predicted it at, with any correction still easing out. None for
    /// one not in our bubble.
    fn member_drawn(&self, slot: u8, now_ms: f64) -> Option<DrawnVehicle> {
        let i = slot as usize;
        let t = self.member_tick(MAX_FIGHTERS + i, now_ms)?;
        let newest = self.frames[self.n_frames - 1];
        let mut d = if t <= newest.tick as f64 {
            let (ai, bi) = self.around(t);
            let (a, b) = (&self.frames[ai], bi.map(|k| &self.frames[k]));
            let va = a.vehicles[..a.nv as usize].iter().find(|v| v.slot == slot)?;
            vehicle_between(va, a.tick, b, t)
        } else {
            let k = self.latest.tick.wrapping_sub(self.latest.last_seq) as i32 as f64;
            let newest_seq = self.seq.wrapping_sub(1) as f64;
            let q = (t - k).min(newest_seq).max(self.latest.last_seq as f64);
            let (q0, u) = (q.floor() as u32, (q - q.floor()) as f32);
            let q1 = if (q0 as f64) < newest_seq {
                q0.wrapping_add(1)
            } else {
                q0
            };
            let (n0, n1) = (self.near_after[q0 as usize % RING], self.near_after[q1 as usize % RING]);
            if n0.mask & (1 << i) == 0 || n1.mask & (1 << i) == 0 {
                return None;
            }
            let (a, b) = (n0.hulls[i], n1.hulls[i]);
            let vn = newest.vehicles[..newest.nv as usize].iter().find(|v| v.slot == slot)?;
            DrawnVehicle {
                slot,
                pos: a.pos + (b.pos - a.pos) * u,
                yaw: lerp_angle(yaw_rad(a.yaw), yaw_rad(b.yaw), u),
                health: vn.health,
                seats: vn.seats,
            }
        };
        d.pos = d.pos + self.moff[i];
        d.yaw += self.mturn[i];
        Some(d)
    }

    /// Seat `slot` on foot in our bubble, its feet as drawn at `now_ms` at
    /// its tick (as [`Self::member_drawn`] draws a hull). None for one
    /// drawn on the render tick.
    fn body_drawn(&self, slot: u8, now_ms: f64) -> Option<V3> {
        let q = slot as usize;
        let t = self.member_tick(q, now_ms)?;
        let newest = self.frames[self.n_frames - 1];
        let p = if t <= newest.tick as f64 {
            let (ai, _) = self.around(t);
            self.remote_at(slot, ai, t)?.pos
        } else {
            let k = self.latest.tick.wrapping_sub(self.latest.last_seq) as i32 as f64;
            let newest_seq = self.seq.wrapping_sub(1) as f64;
            let at = (t - k).min(newest_seq).max(self.latest.last_seq as f64);
            let (q0, u) = (at.floor() as u32, (at - at.floor()) as f32);
            let q1 = if (q0 as f64) < newest_seq {
                q0.wrapping_add(1)
            } else {
                q0
            };
            let (n0, n1) = (self.near_after[q0 as usize % RING], self.near_after[q1 as usize % RING]);
            if n0.walkers & (1 << q) == 0 || n1.walkers & (1 << q) == 0 {
                return None;
            }
            n0.feet[q] + (n1.feet[q] - n0.feet[q]) * u
        };
        Some(p + self.boff[q])
    }

    /// The hull in our bubble `slot` rides, and its seat, as the newest
    /// snapshot has it.
    fn member_rider(&self, slot: u8) -> Option<(u8, usize)> {
        let newest = self.frames.get(self.n_frames.checked_sub(1)?)?;
        newest.vehicles[..newest.nv as usize].iter().find_map(|v| {
            let seat = v.seats.iter().position(|&x| x == slot + 1)?;
            let drawn_on = self
                .share
                .get(MAX_FIGHTERS + v.slot as usize)
                .is_some_and(|&sh| sh != (SHARE_FULL, SHARE_FULL));
            (v.near.is_some() && drawn_on).then_some((v.slot, seat))
        })
    }

    /// One vehicle of [`NetCore::vehicles`], by its slot.
    fn vehicle_at(&self, slot: u8, now_ms: f64) -> Option<DrawnVehicle> {
        if self.n_frames == 0 || self.pred.riding().map(|(i, _)| i as u8) == Some(slot) {
            return None;
        }
        if let Some(d) = self.member_drawn(slot, now_ms) {
            return Some(d);
        }
        let rt = self.render_tick(now_ms);
        let (ai, bi) = self.around(rt);
        let (a, b) = (&self.frames[ai], bi.map(|i| &self.frames[i]));
        let va = a.vehicles[..a.nv as usize].iter().find(|v| v.slot == slot)?;
        let mut d = vehicle_between(va, a.tick, b, rt);
        d.pos = d.pos + self.moff[slot as usize];
        d.yaw += self.mturn[slot as usize];
        Some(d)
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

    /// Events due at `now_ms`: everyone else's (shots, throws, blasts,
    /// spawns, ...) once the render tick reaches the snapshot they came in,
    /// so a flash or a blast plays with the body or round it belongs to,
    /// or once held [`HOLD_MAX_MS`]; then hits, kills and our own, which
    /// play the moment they arrive (our shots the moment predicted).
    pub fn drain_events(&mut self, now_ms: f64) -> Vec<EventState> {
        let mut out = Vec::new();
        self.drain_events_into(now_ms, &mut out);
        out
    }

    /// [`NetCore::drain_events`] into `out` (cleared first).
    pub fn drain_events_into(&mut self, now_ms: f64, out: &mut Vec<EventState>) {
        out.clear();
        let rt = self.render_tick(now_ms);
        // A hull in our bubble is drawn ahead of the render tick: what it
        // does plays when its own drawn tick gets there, where it is drawn.
        let mut k = 0;
        while k < self.held.len() {
            let h = self.held[k];
            let drawn = h.member.and_then(|(t, at)| {
                let tick = self.member_tick(t as usize, now_ms)?;
                (h.tick as f64 <= tick).then(|| {
                    let now = if (t as usize) < MAX_FIGHTERS {
                        self.body_drawn(t, now_ms)
                    } else {
                        self.member_drawn(t - MAX_FIGHTERS as u8, now_ms).map(|d| d.pos)
                    };
                    now.map(|p| p - at)
                })
            });
            match drawn {
                Some(shift) => {
                    out.push(EventState {
                        pos: h.e.pos + shift.unwrap_or(V3::ZERO),
                        ..h.e
                    });
                    self.held.remove(k);
                }
                None => k += 1,
            }
        }
        // Snapshots only come newer, so they are due in order.
        while let Some(h) = self.held.front() {
            if h.tick as f64 <= rt || now_ms - h.at_ms >= HOLD_MAX_MS {
                out.push(h.e);
                self.held.pop_front();
            } else {
                break;
            }
        }
        out.append(&mut self.events);
    }

    /// How far behind the newest snapshot everyone else is drawn at
    /// `now_ms`, in ticks: what the server adds to the snapshot's age to
    /// rewind a shot to what we saw (DESIGN §7). The delay and the clock
    /// both slew, so it is measured, not either's target.
    fn playout(&self, now_ms: f64) -> u8 {
        match self.newest {
            Some(n) if self.clock.is_some() => (n as f64 - self.render_tick(now_ms)).round().clamp(0.0, 255.0) as u8,
            _ => self.delay.target.round() as u8,
        }
    }

    /// The server tick everyone else is drawn at, at `now_ms`.
    pub fn render_tick(&self, now_ms: f64) -> f64 {
        self.clock
            .map_or(0.0, |c| now_ms / TICK_MS + c.at(now_ms) - self.delay.at(now_ms))
    }

    /// Whether anyone is drawn past the newest snapshot at `now_ms`: the
    /// render tick has outrun what arrived (late or lost snapshots), so
    /// they are extrapolated or held. The impaired-network gate counts it.
    pub fn extrapolating(&self, now_ms: f64) -> bool {
        if self.n_frames == 0 {
            return false;
        }
        let newest = &self.frames[self.n_frames - 1];
        newest.n > 0 && self.render_tick(now_ms) > newest.tick as f64
    }

    /// The two kept frames around render tick `rt`: the last at or before
    /// it (the first kept, if none is), and the next if it is after it.
    /// Everyone else and the vehicles are drawn between them. There must be
    /// a frame.
    fn around(&self, rt: f64) -> (usize, Option<usize>) {
        let frames = &self.frames[..self.n_frames];
        let ai = frames.iter().rposition(|f| f.tick as f64 <= rt).unwrap_or(0);
        let bi = Some(ai + 1).filter(|&i| i < frames.len() && frames[i].tick as f64 > rt);
        (ai, bi)
    }

    /// Everyone else as drawn at `now_ms`: between the two snapshots around
    /// the render tick, or extrapolated at most [`EXTRAPOLATE_MS`] past the
    /// newest, then held.
    pub fn others(&self, now_ms: f64) -> Vec<Drawn> {
        let mut out = Vec::new();
        self.others_into(now_ms, &mut out);
        out
    }

    /// [`NetCore::others`] into `out` (cleared first): a Vec kept from
    /// frame to frame allocates nothing.
    pub fn others_into(&self, now_ms: f64, out: &mut Vec<Drawn>) {
        out.clear();
        if self.n_frames == 0 {
            return;
        }
        let rt = self.render_tick(now_ms);
        let (ai, _) = self.around(rt);
        let a = &self.frames[ai];
        out.extend(
            a.remotes[..a.n as usize]
                .iter()
                .filter_map(|r| self.other_between(r.slot, ai, rt, now_ms)),
        );
        // Riding a hull in our bubble since the render tick (it came in
        // unseen, on our present): drawn on it already.
        let newest = &self.frames[self.n_frames - 1];
        for r in &newest.remotes[..newest.n as usize] {
            let fresh = !a.remotes[..a.n as usize].iter().any(|x| x.slot == r.slot);
            if fresh && self.member_rider(r.slot).is_some() {
                out.extend(self.other_between(r.slot, ai, rt, now_ms));
            }
        }
    }

    /// The seat `slot` has in the hull we ride (as the server sent it,
    /// seats and all), if it rides with us.
    fn co_rider(&self, slot: u8) -> Option<usize> {
        self.pred.riding()?;
        let v = self.pred_v?;
        (slot != self.slot).then_some(())?;
        v.seats.iter().position(|&x| x == slot + 1)
    }

    /// One fighter of [`NetCore::others`], by its slot.
    fn other_at(&self, slot: u8, now_ms: f64) -> Option<Drawn> {
        if self.n_frames == 0 {
            return None;
        }
        let rt = self.render_tick(now_ms);
        let (ai, _) = self.around(rt);
        let a = &self.frames[ai];
        let shown = a.remotes[..a.n as usize].iter().any(|r| r.slot == slot);
        shown.then(|| self.other_between(slot, ai, rt, now_ms)).flatten()
    }

    /// Fighter `slot` at render tick `rt` (frame `ai` the one at or before
    /// it, where it is in view): from its newest update up to that frame
    /// toward its next one after, or run on. A far one on a big map is
    /// updated every few snapshots and held between (`Snapshot::held`), so
    /// its updates, not the frames, are what it is drawn between.
    fn other_between(&self, slot: u8, ai: usize, rt: f64, now_ms: f64) -> Option<Drawn> {
        let frames = &self.frames[..self.n_frames];
        // Riding with us: in its seat on the hull as we draw it, never a
        // round trip behind it; its aim the newest we were sent. A watcher
        // draws the hull and its riders alike on the render tick.
        let newest = || {
            frames
                .iter()
                .rev()
                .find_map(|f| f.remotes[..f.n as usize].iter().find(|r| r.slot == slot))
        };
        // On foot in our bubble: at its share of the way to our present.
        if let Some(p) = self.body_drawn(slot, now_ms) {
            return Some(Drawn {
                pos: p,
                ..drawn(newest()?)
            });
        }
        // Riding a hull in our bubble: in its seat on the hull as drawn.
        if let Some((i, seat)) = self.member_rider(slot) {
            if let Some(h) = self.member_drawn(i, now_ms) {
                let turn = (h.yaw as f64 * (65536.0 / core::f64::consts::TAU)).round();
                let hull = Vehicle {
                    pos: h.pos,
                    yaw: turn.rem_euclid(65536.0) as u16,
                    turret: self
                        .map
                        .vehicles
                        .get(i as usize)
                        .map_or(sim::vehicle::NO_TURRET, |d| d.turret),
                    ..Vehicle::default()
                };
                return Some(Drawn {
                    pos: hull.mount(seat),
                    ..drawn(newest()?)
                });
            }
        }
        // It rode a hull in our bubble at the render tick, and has got out
        // or died since: where the newest snapshot has it (dead where it
        // fell), not alive in a seat a round trip back.
        let full = (SHARE_FULL, SHARE_FULL);
        let rode_then = frames[ai].vehicles[..frames[ai].nv as usize]
            .iter()
            .any(|v| v.seats.contains(&(slot + 1)) && self.share[MAX_FIGHTERS + v.slot as usize] != full);
        if rode_then && !self.watch {
            return newest().map(drawn);
        }
        if let (Some(seat), Some(hull), false) = (self.co_rider(slot), self.drawn_hull(), self.watch) {
            return Some(Drawn {
                pos: hull.mount(seat),
                vel: hull.vel,
                ..drawn(newest()?)
            });
        }
        // Out of our hull (or dead in it) since the render tick, which still
        // has it seated a round trip back: where it got out (or fell), as
        // the newest snapshot has it, till the render tick gets there.
        let seats = |v: Option<Vehicle>| v.is_some_and(|v| v.seats.iter().any(|&x| x == slot + 1));
        let (seated_then, seated_now) = (seats(frames[ai].own_v), seats(frames[self.n_frames - 1].own_v));
        if seated_then && !seated_now && !self.watch && frames[ai].own.riding().is_some() {
            return newest().map(drawn);
        }
        self.remote_at(slot, ai, rt)
    }

    /// Fighter `slot` between the snapshots around tick `rt` (frame `ai`
    /// the one at or before it, where it is in view): from its newest
    /// update up to that frame toward its next one after, or run on.
    fn remote_at(&self, slot: u8, ai: usize, rt: f64) -> Option<Drawn> {
        let frames = &self.frames[..self.n_frames];
        // Its updates: a held one repeats an update, never is one.
        let find = |f: &Frame| {
            let r = f.remotes[..f.n as usize].iter().find(|r| r.slot == slot).copied();
            r.filter(|_| f.held & (1 << slot) == 0)
        };
        // Its update at or before the frame, through frames that held it.
        let mut from = None;
        for f in frames[..=ai].iter().rev() {
            if let Some(r) = find(f) {
                from = Some((r, f.tick));
                break;
            }
            if f.held & (1 << slot) == 0 {
                break;
            }
        }
        let (ra, a_tick) = from?;
        let next = frames[ai + 1..].iter().find_map(|f| find(f).map(|r| (r, f.tick)));
        Some(remote_between(&ra, a_tick, next, rt))
    }
}

/// Remote `ra`, updated at tick `a_tick`, drawn at render tick `rt`:
/// toward its next update `next` (the remote and its tick) if there is one,
/// or run on from its velocity.
fn remote_between(ra: &Remote, a_tick: u32, next: Option<(Remote, u32)>, rt: f64) -> Drawn {
    let mut d = drawn(ra);
    match next {
        Some((rb, b_tick)) if ra.alive == rb.alive => {
            // A gap too long to draw across: as if the next hadn't
            // come (run on, then held) until it is due, then there.
            if b_tick - a_tick <= ticks(LERP_SPAN_MS) {
                let u = ((rt - a_tick as f64) / (b_tick - a_tick) as f64).clamp(0.0, 1.0) as f32;
                d.pos = ra.pos + (rb.pos - ra.pos) * u;
                d.yaw = lerp_angle(d.yaw, yaw_rad(rb.yaw), u);
                d.pitch = d.pitch + (pitch_rad(rb.pitch) - d.pitch) * u;
            } else if ra.alive {
                d.pos = ra.pos + ra.vel * (ahead(rt, a_tick) / TICK_HZ as f32);
            }
        }
        _ => {
            if ra.alive {
                d.pos = ra.pos + ra.vel * (ahead(rt, a_tick) / TICK_HZ as f32);
            }
        }
    }
    d
}

/// Vehicle `va` of the frame at tick `a_tick`, drawn at render tick `rt`,
/// as [`remote_between`] draws a fighter.
fn vehicle_between(va: &VehState, a_tick: u32, b: Option<&Frame>, rt: f64) -> DrawnVehicle {
    let mut d = DrawnVehicle {
        slot: va.slot,
        pos: va.pos,
        yaw: yaw_rad(va.yaw),
        health: va.health,
        seats: va.seats,
    };
    let vb = b.and_then(|b| b.vehicles[..b.nv as usize].iter().find(|v| v.slot == va.slot));
    match (b, vb) {
        (Some(b), Some(vb)) if b.tick - a_tick <= ticks(LERP_SPAN_MS) => {
            let u = ((rt - a_tick as f64) / (b.tick - a_tick) as f64).clamp(0.0, 1.0) as f32;
            d.pos = va.pos + (vb.pos - va.pos) * u;
            d.yaw = lerp_angle(d.yaw, yaw_rad(vb.yaw), u);
        }
        _ => d.pos = va.pos + va.vel * (ahead(rt, a_tick) / TICK_HZ as f32),
    }
    d
}

impl NetCore {
    /// The kill cam at `now_ms`, while we are dead and the killer was in
    /// what we were sent: the run-up to the kill from the killer's eye, at
    /// real speed from when we heard of it, then held on the kill. None if
    /// we are alive, killed ourselves, or never saw who did it.
    pub fn killcam(&self, now_ms: f64) -> Option<KillCam> {
        let d = self.death?;
        let n = self.cam.len();
        let frames = (0..n).map(|k| &self.cam[(self.cam_at + k) % n]);
        let from = d.tick.saturating_sub(sim::consts::ticks(KILLCAM_MS));
        let has = |f: &CamFrame| f.remotes[..f.n as usize].iter().any(|r| r.slot == d.killer && r.alive);
        let first = frames
            .clone()
            .find(|f| f.tick >= from && f.tick <= d.tick && has(f))?
            .tick;
        let last = frames.clone().rfind(|f| f.tick <= d.tick && has(f))?.tick;
        let span = (d.tick - first).max(1) as f64;
        let rt = (first as f64 + (now_ms - d.at_ms) / TICK_MS).min(d.tick as f64);
        // The two kept frames around the playback tick.
        let mut a: Option<&CamFrame> = None;
        let mut b: Option<&CamFrame> = None;
        for f in frames {
            if (f.tick as f64) <= rt {
                a = Some(f);
            } else if b.is_none() {
                b = Some(f);
            }
        }
        let a = a?;
        let u = b.map_or(0.0, |b| {
            ((rt - a.tick as f64) / (b.tick - a.tick) as f64).clamp(0.0, 1.0) as f32
        });
        let mut bodies = Vec::with_capacity(a.n as usize + 1);
        fn all(f: &CamFrame) -> impl Iterator<Item = &Remote> {
            f.remotes[..f.n as usize].iter().chain(core::iter::once(&f.me))
        }
        for x in all(a) {
            let mut d = drawn(x);
            if let Some(y) = b.and_then(|b| all(b).find(|y| y.slot == x.slot && y.alive == x.alive)) {
                d.pos = x.pos + (y.pos - x.pos) * u;
                d.yaw = lerp_angle(d.yaw, yaw_rad(y.yaw), u);
                d.pitch = d.pitch + (pitch_rad(y.pitch) - d.pitch) * u;
            }
            bodies.push(d);
        }
        // The killer as last seen at or before the playback tick: past its
        // last sighting it stays where it was.
        let k = match bodies.iter().find(|x| x.slot == d.killer && x.alive) {
            Some(k) => *k,
            None => {
                let at = self
                    .cam
                    .iter()
                    .filter(|f| f.tick <= last.min(rt as u32) && has(f))
                    .max_by_key(|f| f.tick)?;
                drawn(at.remotes[..at.n as usize].iter().find(|r| r.slot == d.killer)?)
            }
        };
        bodies.retain(|x| x.slot != d.killer);
        let eye_h = if k.crouched {
            sim::consts::CROUCH_EYE
        } else {
            sim::consts::EYE
        };
        Some(KillCam {
            killer: d.killer,
            eye: k.pos + V3::new(0.0, eye_h, 0.0),
            yaw: k.yaw,
            pitch: k.pitch,
            progress: ((rt - first as f64) / span).clamp(0.0, 1.0) as f32,
            bodies,
        })
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
        && a.charge == b.charge
        && a.left == b.left
        && a.left_refire == b.left_refire
        && a.left_reload == b.left_reload
        && a.left_charge == b.left_charge
        && a.stickies == b.stickies
        && a.gren_kind == b.gren_kind
        && a.gren_cool == b.gren_cool
        && a.grenades == b.grenades
        && a.shots == b.shots
        && a.last_buttons == b.last_buttons
        && a.ride == b.ride
        && a.mount == b.mount
}

/// What is left of a correction `dt` ms on: eased out over
/// [`EASE_FAR_MS`] when a metre or more, [`EASE_NEAR_MS`] when a quarter
/// metre or less, blended between; dropped under [`DEAD_ZONE_M`].
fn ease(e: V3, dt: f64) -> V3 {
    ease_by(e, dt, EASE_NEAR_MS)
}

/// [`ease`], a small correction out over `near_ms`.
fn ease_by(e: V3, dt: f64, near_ms: f64) -> V3 {
    let size = e.len();
    if size < DEAD_ZONE_M {
        return V3::ZERO;
    }
    let u = ((size - EASE_NEAR_M) / (EASE_FAR_M - EASE_NEAR_M)).clamp(0.0, 1.0) as f64;
    let tau = near_ms + (EASE_FAR_MS - near_ms) * u;
    e * (-dt / tau).exp() as f32
}

/// The clock's rate for a low point of `d` inputs waiting at the server.
/// Short of the band (TARGET_DEPTH to one over it) the server is guessing
/// our inputs, which costs corrections: 5% fast. Past it we only add
/// latency: slower in proportion to how far, 1% to 5% (Gates'
/// `ClientClock` gain), so the clock eases down into the band rather than
/// swing across it and run our own movement 5% slow on the way.
fn clock_rate(d: f64) -> f64 {
    const KP: f64 = 0.015;
    if d < TARGET_DEPTH {
        1.05
    } else if d > TARGET_DEPTH + 1.5 {
        1.0 - (KP * (d - TARGET_DEPTH - 1.0)).clamp(0.01, 0.05)
    } else {
        1.0
    }
}

/// How far past snapshot tick `a` to extrapolate at render tick `rt`, in
/// ticks: none before it, at most [`EXTRAPOLATE_MS`].
fn ahead(rt: f64, a: u32) -> f32 {
    (rt - a as f64).clamp(0.0, ticks(EXTRAPOLATE_MS) as f64) as f32
}

fn phase_of(p: u8) -> Phase {
    match p {
        0 => Phase::Countdown,
        1 => Phase::Playing,
        _ => Phase::Over,
    }
}

/// Radians as a turn in 1/65536, the short way.
fn turn_of(rad: f32) -> u16 {
    (rad * (65536.0 / core::f32::consts::TAU)).round() as i32 as u16
}

/// A share drawn sliding from `(from, to)` over a snapshot's time since
/// `at_ms`.
fn share_between((from, to): (u8, u8), at_ms: f64, now_ms: f64, snap_every: u32) -> f64 {
    let snap_ms = snap_every.max(1) as f64 * TICK_MS;
    let u = ((now_ms - at_ms) / snap_ms).clamp(0.0, 1.0);
    from as f64 + (to as f64 - from as f64) * u
}

/// What a held event is of in our bubble: the hull it hit or wrecked, or
/// the one its maker rides; and where the snapshot had that hull.
fn member_of(s: &Snapshot, e: &EventState) -> Option<(u8, V3)> {
    use sim::events::Kind;
    let hull = |i: u8| {
        let v = s.vehicles().iter().find(|v| v.slot == i && v.near.is_some())?;
        Some((MAX_FIGHTERS as u8 + i, v.pos))
    };
    if e.kind == Kind::VehicleHit as u8 || e.kind == Kind::Wreck as u8 {
        return hull(e.part);
    }
    // Only what happens at its maker (a blast, a flag's goings, stay where
    // they are in the world), and never nobody's.
    let at_maker = [Kind::Shot, Kind::Melee, Kind::Throw, Kind::Ride, Kind::Pickup];
    if !at_maker.iter().any(|&k| e.kind == k as u8) || e.a as usize >= MAX_FIGHTERS {
        return None;
    }
    if let Some(r) = s.remotes().iter().find(|r| r.slot == e.a && r.near.is_some()) {
        return Some((r.slot, r.pos));
    }
    let v = s
        .vehicles()
        .iter()
        .find(|v| v.near.is_some() && v.seats.contains(&(e.a + 1)))?;
    Some((MAX_FIGHTERS as u8 + v.slot, v.pos))
}

fn yaw_rad(y: u16) -> f32 {
    y as f32 * (core::f32::consts::TAU / 65536.0)
}

fn pitch_rad(p: i16) -> f32 {
    p as f32 * (core::f32::consts::TAU / 65536.0)
}

/// A heading `u` of the short way from `a` to `b`, in the wire's turns.
fn turn_between(a: u16, b: u16, u: f32) -> u16 {
    let d = b.wrapping_sub(a) as i16 as f32;
    a.wrapping_add((d * u).round() as i32 as u16)
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
        over: r.over,
        cloak: (r.cloak as f32 / proto::CLOAK_STEPS).min(1.0),
    }
}

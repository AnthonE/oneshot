//! Aim windows: what the detector reads (docs/ANTICHEAT.md §6). One window
//! is one seat's aim at one enemy around the tick that enemy came into
//! plain sight, a row a tick: the seat's own turn, how far its aim is from
//! the enemy, what fog and plain sight said, and what the trigger did.
//! Everything in a row is what the server knows; nothing is the client's
//! word beyond its inputs.
//!
//! `tools/aim` cuts them from replays and bot matches, each beside its
//! cheated twin (`crate::assist`), and `tools/aim/train.py` learns from
//! them. The rows' order is [`FEATURES`]; a change to it is a change to
//! every dataset, so it is named in the dataset's `info.json`.

use sim::events::Kind as Ev;
use sim::input::{Input, FIRE, ZOOM};
use sim::world::World;

use crate::assist::{aim_point, angles_to, wrap_deg as wrap};

/// Ticks before the enemy came into plain sight, by default.
pub const PRE: usize = 32;
/// Ticks in a window by default: 1 s at 128 Hz. A dataset names its own
/// in its `info.json`.
pub const LEN: usize = 128;
/// Angles are kept within this many degrees: past it, only "far off".
const CLIP_DEG: f32 = 90.0;
/// A body's half width, metres, for `on`.
const HALF_M: f32 = 0.3;

/// What a row holds, in order.
pub const FEATURES: [&str; 16] = [
    // The seat's own turn this tick, degrees.
    "turn_yaw",
    "turn_pitch",
    // From its aim to the enemy's head, degrees, signed.
    "err_yaw",
    "err_pitch",
    // How fast the enemy crosses the seat's view: its bearing's change
    // this tick, degrees (what the aim must match to track).
    "bearing_yaw",
    "bearing_pitch",
    // The enemy in plain sight; fog sent it.
    "plain",
    "shown",
    // The trigger held; a round left the gun; a hit on the enemy; to the
    // head.
    "fire",
    "shot",
    "hit",
    "head",
    // Metres to the enemy, over 32.
    "dist",
    "zoom",
    // Both standing (neither dead nor waiting to spawn).
    "alive",
    // The aim on the enemy's body: within its half width at that range of
    // the chest, the middle or the head. When a trigger is pulled against
    // when this came on is a person's click or a triggerbot's.
    "on",
];
pub const F: usize = FEATURES.len();

/// One window being written.
#[derive(Clone, Debug)]
pub struct Tape {
    pub viewer: usize,
    pub target: usize,
    pub rows: Vec<[f32; F]>,
    len: usize,
    /// Last row's aim and bearing to the enemy, degrees.
    prev: Option<(f32, f32, f32, f32)>,
}

impl Tape {
    /// A window of `len` ticks.
    pub fn new(viewer: usize, target: usize, len: usize) -> Tape {
        Tape {
            viewer,
            target,
            rows: Vec::with_capacity(len),
            len,
            prev: None,
        }
    }

    pub fn full(&self) -> bool {
        self.rows.len() >= self.len
    }

    /// A row for the tick `w` just stepped: `plain` and `shown` are the
    /// enemy in plain sight and sent by fog, `inp` the viewer's input
    /// that tick.
    pub fn record(&mut self, w: &World, plain: bool, shown: bool, inp: &Input) {
        if self.full() {
            return;
        }
        let (v, t) = (self.viewer, self.target);
        let (me, them) = (&w.fighters[v], &w.fighters[t]);
        let yaw = me.yaw as f32 * (360.0 / 65536.0);
        let pitch = me.pitch as f32 * (360.0 / 65536.0);
        let (by, bp) = angles_to(me.eye(), aim_point(them, 1.0));
        let (ty, tp, bdy, bdp) = match self.prev {
            Some((py, pp, pby, pbp)) => (wrap(yaw - py), pitch - pp, wrap(by - pby), bp - pbp),
            None => (0.0, 0.0, 0.0, 0.0),
        };
        self.prev = Some((yaw, pitch, by, bp));
        let (mut shot, mut hit, mut head) = (0.0, 0.0, 0.0);
        for e in w.events.as_slice() {
            match e.kind {
                Ev::Shot if e.a as usize == v => shot = 1.0,
                Ev::Hit if e.a as usize == v && e.b as usize == t => {
                    hit = 1.0;
                    if e.part == 2 || e.part == 3 {
                        head = 1.0;
                    }
                }
                _ => {}
            }
        }
        let c = |d: f32| d.clamp(-CLIP_DEG, CLIP_DEG);
        let b = |x: bool| if x { 1.0 } else { 0.0 };
        self.rows.push([
            c(ty),
            c(tp),
            c(wrap(by - yaw)),
            c(bp - pitch),
            c(bdy),
            c(bdp),
            b(plain),
            b(shown),
            b(inp.buttons & FIRE != 0),
            shot,
            hit,
            head,
            (them.body.pos - me.body.pos).len() / 32.0,
            b(inp.buttons & ZOOM != 0),
            b(me.active() && them.active()),
            b(on_body(me, them, yaw, pitch)),
        ]);
    }
}

/// Whether `me`'s aim (`yaw`, `pitch`, degrees) is on `them`'s body.
fn on_body(me: &sim::fighter::Fighter, them: &sim::fighter::Fighter, yaw: f32, pitch: f32) -> bool {
    let eye = me.eye();
    let d = (them.body.pos - eye).len().max(1.0);
    let half = (sim::fmath::atan2(HALF_M, d) * (180.0 / core::f32::consts::PI)).max(0.5);
    [0.0, 0.5, 1.0].iter().any(|&bone| {
        let (y, p) = angles_to(eye, aim_point(them, bone));
        wrap(y - yaw).hypot(p - pitch) < half
    })
}

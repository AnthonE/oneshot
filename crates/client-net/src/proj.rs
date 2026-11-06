//! Rounds in flight as drawn (docs/DESIGN.md §6): pure functions, no state
//! of their own, tested natively.
//!
//! A round the server sent is drawn on the render tick, from the same
//! snapshots as its shooter, so it neither leads nor lags him; past the
//! newest snapshot it runs on as the sim flies it ([`coast`]), stopped at
//! the map, then held. Ours starts at our predicted muzzle and glides onto
//! the render tick (`core::NetCore::projectiles`). The wire carries no id
//! for a round, so ids are matched from snapshot to snapshot here
//! ([`assign_ids`]), and a sticky stuck to someone rides them as drawn
//! ([`detect_hosts`]). The client never decides: no body tests, no blasts.

use std::cell::Cell;

use crate::core::{Frame, LERP_SPAN_MS};
use proto::{ProjState, Snapshot, MAX_SNAP_PROJ};
use sim::arms::{arm, Kind, CANNON, LAUNCHER, ROCKET};
use sim::consts::{ticks, DT, GRAVITY, GREN_SPEED, MAX_FIGHTERS, TICK_HZ};
use sim::fighter::Fighter;
use sim::geom::{aim_dir, V3};
use sim::map::Map;
use sim::movement::quant;
use sim::world::ProjKind;

/// Two snapshots' rounds are one if the older, run on its velocity, lands
/// within this of the newer, plus this much speed's worth of the gap.
pub const PROJ_MATCH_M: f32 = 0.5;
pub const PROJ_MATCH_V: f32 = 10.0;
/// A sticky slower than this, `vel` from someone's feet (or a hull's
/// middle) to within [`STUCK_M`] of it, is stuck to them: on the wire a
/// stuck sticky's `vel` is its offset from its host.
pub const STUCK_V: f32 = 4.0;
pub const STUCK_M: f32 = 0.1;
/// How fast our own round's lead on the render tick is given up: this
/// share of real time.
pub const GLIDE: f64 = 0.2;
/// Our own round is drawn at most this far past the newest snapshot (as
/// the clock reckons it at the moment): longer than the round trips we
/// play at, so it leaves the muzzle at once; past it, it waits there.
pub const OWN_AHEAD_MS: u32 = 500;
/// Our own rounds tracked from launch at once; the newest past it is
/// refused (drawn when the server sends it, as anyone's).
pub const MAX_LAUNCHES: usize = 16;
/// Our launch is the server's round within this of where we flew it, plus
/// its cone's worth of the distance flown.
pub const BIND_M: f32 = 1.0;
/// An event is held for the render tick at most this long, in ms.
pub const HOLD_MAX_MS: f64 = 250.0;
/// A round stuck to nobody; stuck to us.
pub const NO_HOST: u8 = 255;
pub const HOST_ME: u8 = 254;

/// `ProjState::kind`, as `sim::world::ProjKind` numbers them.
pub const K_ROCKET: u8 = ProjKind::Rocket as u8;
pub const K_FRAG: u8 = ProjKind::Frag as u8;
pub const K_BOLT: u8 = ProjKind::Bolt as u8;
pub const K_CHARGE: u8 = ProjKind::Charge as u8;
pub const K_NEEDLE: u8 = ProjKind::Needle as u8;
pub const K_LOB: u8 = ProjKind::Lob as u8;
pub const K_CANNON: u8 = ProjKind::Cannon as u8;
pub const K_STICKY: u8 = ProjKind::Sticky as u8;

/// Most whole ticks [`coast`] runs, whatever it is asked.
const COAST_MAX: f64 = 1024.0;

/// One tick of the sim's free flight (`World::projectiles`), stopped at
/// the map: where it is, how fast, and whether the map stopped it.
fn step(kind: u8, pos: V3, vel: V3, map: &Map) -> (V3, V3, bool) {
    // A sticky stuck to the map: still.
    if kind == K_STICKY && vel == V3::ZERO {
        return (pos, vel, true);
    }
    let mut v = vel;
    let mut ramp = None;
    match kind {
        // Under gravity: `bounce`; a sticky's own flight.
        K_FRAG | K_STICKY => v.y -= GRAVITY * DT,
        // `bounce` under the launcher's share of it, never ramped.
        K_LOB => {
            let g = GRAVITY * arm(LAUNCHER).gravity;
            v.y -= g * DT;
        }
        // `fly`: its gravity, then the ramp to its top speed.
        K_ROCKET | K_CANNON => {
            let a = arm(if kind == K_ROCKET { ROCKET } else { CANNON });
            if a.gravity > 0.0 {
                v.y -= GRAVITY * a.gravity * DT;
            }
            ramp = Some(a);
        }
        // Bolts and shards fly straight; homing needs a target we don't
        // have.
        _ => {}
    }
    let speed = v.len();
    if speed.is_nan() || speed <= 0.0 {
        return (pos, v, true);
    }
    let d = v * (1.0 / speed);
    let reach = speed * DT;
    let t = map.ray(pos, d, reach);
    if t < reach {
        return (pos + d * (t - 0.05).max(0.0), v, true);
    }
    let p = pos + v * DT;
    if let Some(a) = ramp {
        if a.speed_max > 0.0 {
            let k = (a.speed_max - a.speed) / a.ramp;
            if k * (a.speed_max - speed) > 0.0 {
                let to = speed + k * reach;
                let to = if k > 0.0 {
                    to.min(a.speed_max)
                } else {
                    to.max(a.speed_max)
                };
                v = d * to;
            }
        }
    }
    (quant3(p), quant3(v), false)
}

fn quant3(v: V3) -> V3 {
    V3::new(quant(v.x), quant(v.y), quant(v.z))
}

/// A round of `kind` at `pos` and `vel`, `ticks` on as the sim flies it in
/// open air: whole ticks exactly (bit for bit, `coast_steps_like_the_sim`),
/// the fraction between the last two. The map stops it 5 cm short, and it
/// is held there (the flag says so). No bounce and no bodies: what it
/// meets is the server's to say.
pub fn coast(kind: u8, pos: V3, vel: V3, ticks: f64, map: &Map) -> (V3, V3, bool) {
    coast_on(&Cell::new(None), kind, pos, vel, ticks, map)
}

/// Where [`coast_on`] last left a round: what it was flown from, how many
/// whole ticks, and how it stood then (stopped by the map, or not).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coasted {
    kind: u8,
    from: (V3, V3),
    whole: u32,
    at: (V3, V3),
    stopped: bool,
}

/// [`coast`], going on from where `memo` left this same round when that is
/// no later than `ticks` (the same steps, so the same bits), and leaving
/// it there: a round drawn every frame is flown a tick or two a frame,
/// not from its seed each time.
pub fn coast_on(memo: &Cell<Option<Coasted>>, kind: u8, pos: V3, vel: V3, ticks: f64, map: &Map) -> (V3, V3, bool) {
    let t = if ticks.is_finite() {
        ticks.clamp(0.0, COAST_MAX)
    } else {
        0.0
    };
    let whole = t.floor() as u32;
    let frac = (t - t.floor()) as f32;
    let this = |m: &Coasted| m.kind == kind && m.from == (pos, vel);
    let last = memo.get().filter(this);
    let (mut done, mut p, mut v) = (0, pos, vel);
    if let Some(m) = last.filter(|m| m.whole <= whole) {
        if m.stopped {
            return (m.at.0, m.at.1, true);
        }
        (done, p, v) = (m.whole, m.at.0, m.at.1);
    }
    // Only ever moved on: an earlier ask flies from the seed and leaves it.
    let keep = |done: u32, at: (V3, V3), stopped: bool| {
        if last.is_none_or(|m| done >= m.whole) {
            memo.set(Some(Coasted {
                kind,
                from: (pos, vel),
                whole: done,
                at,
                stopped,
            }));
        }
    };
    while done < whole {
        let (np, nv, clipped) = step(kind, p, v, map);
        done += 1;
        if clipped {
            keep(done, (np, nv), true);
            return (np, nv, true);
        }
        p = np;
        v = nv;
    }
    keep(whole, (p, v), false);
    if frac > 0.0 {
        let (np, nv, _) = step(kind, p, v, map);
        return (p + (np - p) * frac, v + (nv - v) * frac, false);
    }
    (p, v, false)
}

/// What `World::fire` launches for gun `id`: the round's kind and speed,
/// or None for a gun that fires no round.
pub fn launch_kind(id: u8, charged: bool) -> Option<(u8, f32)> {
    let a = arm(id);
    match a.kind {
        Kind::Rocket => Some((K_ROCKET, a.speed)),
        Kind::Cannon => Some((K_CANNON, a.speed)),
        Kind::Needle => Some((K_NEEDLE, a.speed)),
        Kind::Lob => Some((K_LOB, a.speed)),
        Kind::Bolt if charged => Some((K_CHARGE, a.charge_speed)),
        Kind::Bolt => Some((K_BOLT, a.speed)),
        _ => None,
    }
}

/// A round as it leaves the gun, a tick before the sim's first state of
/// it: `coast(seed, 1)` is that state (to the cone).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seed {
    pub kind: u8,
    pub pos: V3,
    pub vel: V3,
    /// The cone it was drawn from, radians: where the server's may be.
    pub cone: f32,
}

/// `f` (as stepped the tick it fired) fires the gun in its right hand or
/// its `left`, as `World::fire` launches it, down the middle of its cone.
pub fn seed_fire(f: &Fighter, left: bool, charged: bool) -> Option<Seed> {
    let id = if left { f.left.arm } else { f.gun().arm };
    let (kind, speed) = launch_kind(id, charged)?;
    let a = arm(id);
    let dir = aim_dir(f.yaw, f.pitch);
    Some(Seed {
        kind,
        pos: f.eye() + dir * 0.4,
        vel: dir * speed,
        cone: if f.dual() { a.dual_spread } else { a.spread },
    })
}

/// `f` throws, as `World::throw` does.
pub fn seed_throw(f: &Fighter, sticky: bool) -> Seed {
    let dir = aim_dir(f.yaw, f.pitch);
    Seed {
        kind: if sticky { K_STICKY } else { K_FRAG },
        pos: f.eye() + dir * 0.4,
        vel: dir * GREN_SPEED + V3::new(0.0, 2.0, 0.0),
        cone: 0.0,
    }
}

/// Which of `new`'s rounds were `prev`'s: the same kind and owner, where
/// the older one's velocity takes it, the nearest pairs first (ties by
/// index, so the same snapshots always give the same ids). Those left try
/// the rounds `prev2` had and `prev` missed (one lost or fogged frame);
/// the rest are new rounds, numbered from `next_pid`. Snapshots further
/// apart than `LERP_SPAN_MS` share nothing.
pub(crate) fn assign_ids(prev: Option<&Frame>, prev2: Option<&Frame>, new: &mut Frame, next_pid: &mut u16) {
    let n = new.np as usize;
    let mut got: [Option<u16>; MAX_SNAP_PROJ] = [None; MAX_SNAP_PROJ];
    let span = ticks(LERP_SPAN_MS);
    if let Some(p) = prev.filter(|p| new.tick.wrapping_sub(p.tick) <= span) {
        let all = [true; MAX_SNAP_PROJ];
        pair(p, &all, new, &mut got);
    }
    if let (Some(p), Some(p2)) = (prev, prev2) {
        if new.tick.wrapping_sub(p2.tick) <= span {
            let mut gone = [false; MAX_SNAP_PROJ];
            for (i, g) in gone.iter_mut().enumerate().take(p2.np as usize) {
                let id = p2.pid[i];
                *g = !p.pid[..p.np as usize].contains(&id) && !got.contains(&Some(id));
            }
            pair(p2, &gone, new, &mut got);
        }
    }
    for (i, g) in got.iter().enumerate().take(n) {
        match *g {
            Some(id) => {
                new.pid[i] = id;
                new.fresh[i] = false;
            }
            None => {
                new.pid[i] = *next_pid;
                new.fresh[i] = true;
                *next_pid = next_pid.wrapping_add(1);
            }
        }
    }
}

/// Pairs `from`'s rounds (those `eligible`) with `new`'s not yet `got`.
fn pair(from: &Frame, eligible: &[bool; MAX_SNAP_PROJ], new: &Frame, got: &mut [Option<u16>; MAX_SNAP_PROJ]) {
    let dt = new.tick.wrapping_sub(from.tick) as f32 / TICK_HZ as f32;
    // Every pair of at most MAX_SNAP_PROJ rounds a side: on the stack.
    let mut cand = [(0.0f32, 0usize, 0usize); MAX_SNAP_PROJ * MAX_SNAP_PROJ];
    let mut n = 0;
    for (qi, q) in from.proj.iter().enumerate().take(from.np as usize) {
        if !eligible[qi] {
            continue;
        }
        for (pi, p) in new.proj.iter().enumerate().take(new.np as usize) {
            if got[pi].is_some() {
                continue;
            }
            if p.kind != q.kind || p.owner != q.owner {
                continue;
            }
            let stuck_q = from.host[qi] != NO_HOST;
            let stuck_p = new.host[pi] != NO_HOST;
            let d = if stuck_q && stuck_p {
                // Stuck to the same one, at the same place on them.
                if from.host[qi] != new.host[pi] {
                    continue;
                }
                let d = (q.vel - p.vel).len();
                if d >= STUCK_M {
                    continue;
                }
                d
            } else if stuck_p {
                // Just stuck: from where it flew.
                let d = (q.pos + q.vel * dt - p.pos).len();
                if d >= PROJ_MATCH_M + (q.vel.len() + PROJ_MATCH_V) * dt {
                    continue;
                }
                d
            } else {
                // A stuck one's `vel` is no velocity: dropped from its host,
                // it is where it was, give or take the host's step.
                let v = if stuck_q { V3::ZERO } else { q.vel };
                let d = (q.pos + v * dt - p.pos).len();
                if d >= PROJ_MATCH_M + (v.len() + PROJ_MATCH_V) * dt {
                    continue;
                }
                d
            };
            cand[n] = (d, qi, pi);
            n += 1;
        }
    }
    // A total order (no two pairs share both rounds), so unstable is the
    // same as stable.
    let cand = &mut cand[..n];
    cand.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut used = [false; MAX_SNAP_PROJ];
    for &mut (_, qi, pi) in cand {
        if used[qi] || got[pi].is_some() {
            continue;
        }
        used[qi] = true;
        got[pi] = Some(from.pid[qi]);
    }
}

/// Which of `frame`'s stickies are stuck, and to whom: slow, and `vel` on
/// from someone the snapshot has lands on it. A remote names its seat, a
/// vehicle `MAX_FIGHTERS` + its slot, we [`HOST_ME`]; the rest
/// [`NO_HOST`].
pub(crate) fn detect_hosts(frame: &mut Frame, s: &Snapshot) {
    for i in 0..frame.np as usize {
        frame.host[i] = host_of(&frame.proj[i], s);
    }
}

fn host_of(p: &ProjState, s: &Snapshot) -> u8 {
    if p.kind != K_STICKY || p.vel.len() >= STUCK_V {
        return NO_HOST;
    }
    let on = |h: V3| (h + p.vel - p.pos).len() < STUCK_M;
    if let Some(r) = s.remotes().iter().find(|r| on(r.pos)) {
        return r.slot;
    }
    if let Some(v) = s.vehicles().iter().find(|v| on(v.pos)) {
        return MAX_FIGHTERS as u8 + v.slot;
    }
    // The hull we ride in (the snapshot may carry it only as ours).
    if let (Some((i, _)), Some(v)) = (s.own.riding(), s.own_vehicle) {
        if on(v.pos) {
            return MAX_FIGHTERS as u8 + i as u8;
        }
    }
    if s.own.alive && on(s.own.body.pos) {
        return HOST_ME;
    }
    NO_HOST
}

/// A round at one moment, as [`sample`] finds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sample {
    pub kind: u8,
    pub owner: u8,
    /// Where it is; for a stuck one, where it was in the frame before
    /// (drawn there if its host isn't).
    pub pos: V3,
    /// Its velocity; for a stuck one, its offset from its host.
    pub vel: V3,
    pub host: u8,
    /// Flying, between a frame and the next that has it stuck: that host,
    /// where the server had it then, and how far between the two `pos` is
    /// (it is drawn onto the host as drawn).
    pub onto: Option<(u8, V3, f32)>,
}

/// Where the frame holding round `pid` at index `i` has it.
fn find(f: &Frame, pid: u16) -> Option<usize> {
    f.pid[..f.np as usize].iter().position(|&p| p == pid)
}

/// The tick of the first frame that has round `pid`, and whether it was
/// new there (no earlier frame had it, kept or not).
pub(crate) fn first_seen(frames: &[Frame], pid: u16) -> Option<(u32, bool)> {
    frames.iter().find_map(|f| find(f, pid).map(|i| (f.tick, f.fresh[i])))
}

/// Whether the ring has a frame after the last that has `pid`: the
/// round's end (it went, or went out of sight).
pub(crate) fn ended(frames: &[Frame], pid: u16) -> bool {
    frames
        .iter()
        .rposition(|f| find(f, pid).is_some())
        .is_some_and(|li| li + 1 < frames.len())
}

/// Round `pid` at tick `t`, the one rule for every round: between the two
/// frames around it, or run on at most `cap` ticks past the last, then
/// held. None once the render tick `rt` reaches the frame it is missing
/// from (it went: its held blast plays then), and before the frame it was
/// first sent in. A run-on goes on from where `memo` left this round
/// ([`coast_on`]: the same steps, so the same bits).
pub(crate) fn sample(
    frames: &[Frame],
    pid: u16,
    (t, rt, cap): (f64, f64, f64),
    map: &Map,
    memo: &Cell<Option<Coasted>>,
) -> Option<Sample> {
    let li = frames.iter().rposition(|f| find(f, pid).is_some())?;
    let fi = frames.iter().position(|f| find(f, pid).is_some())?;
    let end = frames.get(li + 1).map(|f| f.tick as f64);
    if end.is_some_and(|e| rt >= e) {
        return None;
    }
    let first = &frames[fi];
    let mut t = t;
    if t < first.tick as f64 {
        // Born there: not before. Older than the frames kept: as first kept.
        let born = find(first, pid).is_some_and(|i| first.fresh[i]);
        if born {
            return None;
        }
        t = first.tick as f64;
    }
    if let Some(e) = end {
        t = t.min(e);
    }
    let ai = (fi..=li)
        .rev()
        .find(|&i| frames[i].tick as f64 <= t && find(&frames[i], pid).is_some())?;
    let a = &frames[ai];
    let ia = find(a, pid)?;
    let pa = a.proj[ia];
    let mut out = Sample {
        kind: pa.kind,
        owner: pa.owner,
        pos: pa.pos,
        vel: pa.vel,
        host: a.host[ia],
        onto: None,
    };
    if out.host != NO_HOST {
        return Some(out);
    }
    let b = (ai + 1..=li).find_map(|i| find(&frames[i], pid).map(|j| (&frames[i], j)));
    if let Some((b, ib)) = b {
        if b.tick.wrapping_sub(a.tick) <= ticks(LERP_SPAN_MS) {
            let pb = b.proj[ib];
            let u = ((t - a.tick as f64) / (b.tick - a.tick) as f64).clamp(0.0, 1.0) as f32;
            out.pos = pa.pos + (pb.pos - pa.pos) * u;
            out.vel = pa.vel + (pb.vel - pa.vel) * u;
            if b.host[ib] != NO_HOST {
                // A stuck one's `vel` is its offset from its host.
                out.vel = pa.vel;
                out.onto = Some((b.host[ib], pb.pos - pb.vel, u));
            }
            return Some(out);
        }
    }
    let (pos, vel, _) = coast_on(memo, pa.kind, pa.pos, pa.vel, (t - a.tick as f64).clamp(0.0, cap), map);
    out.pos = pos;
    out.vel = vel;
    Some(out)
}

//! Fog of war: what a client may know about where its enemies are
//! (docs/DESIGN.md §8). An enemy goes into a client's snapshot only when it
//! could be on that client's screen soon: a clear line from the viewer's
//! eye (now, or pushed ahead by its own speed, peeking a little to either
//! side, or jumping) to the enemy's head, chest, feet or edges (now, or
//! pushed ahead by its speed). The push is the client's latency plus a
//! snapshot plus its playout, so a peek never pops in late. Once seen, an
//! enemy stays sent for [`HOLD_TICKS`] after its last clear line.
//!
//! Rays against the sim's own map (`Map::clear`), every snapshot, for every
//! person and every enemy. At 2v2 that is a few thousand box tests a
//! snapshot; a baked visibility table comes when maps and rooms grow.

use sim::consts::{
    CAMO_BOT_HIDDEN, CAMO_BOT_M, HEAD_Y, HEAD_Y_CROUCH, HIT_R, MAX_DROPS, MAX_FIGHTERS, MAX_VEHICLES, TICK_HZ,
};
use sim::fighter::Fighter;
use sim::geom::V3;
use sim::map::Map;
use sim::movement::ducked;
use sim::vehicle::Vehicle;
use sim::world::World;

/// Ticks an enemy stays sent after its last clear line (94 ms).
pub const HOLD_TICKS: u32 = 12;
/// How far to either side a viewer may lean around a corner, in metres.
pub const PEEK_M: f32 = 0.4;
/// How much higher a viewer's eye may get in a jump.
pub const JUMP_M: f32 = 0.6;
/// The longest look ahead, in seconds.
pub const MAX_LEAD_S: f32 = 0.2;

/// How far ahead to look for this viewer: half its round trip, one
/// snapshot (`snap_every` ticks), and how far behind it draws others.
pub fn lead_s(rtt_ms: f64, playout_ticks: u32, snap_every: u32) -> f32 {
    let snap = snap_every as f64 / TICK_HZ as f64;
    let play = playout_ticks.clamp(crate::PLAYOUT_MIN, crate::PLAYOUT_MAX) as f64 / TICK_HZ as f64;
    ((rtt_ms / 2000.0 + snap + play) as f32).min(MAX_LEAD_S)
}

fn eye(f: &Fighter) -> V3 {
    f.body.eye()
}

/// The chest and the head over the feet, as far down as a crouch has gone
/// (`sim::movement::ducked`): where the hit volume is.
fn chest_h(f: &Fighter) -> f32 {
    ducked(f.body.duck, 1.1, 0.75)
}

fn head_h(f: &Fighter) -> f32 {
    ducked(f.body.duck, HEAD_Y, HEAD_Y_CROUCH)
}

/// Where `viewer`'s eye could be within `lead` seconds: now, pushed ahead
/// by its speed, leaning `peek` metres across `side` either way, or jumping.
pub fn eyes(viewer: &Fighter, side: V3, lead: f32, peek: f32) -> [V3; 7] {
    let e0 = eye(viewer);
    let e1 = e0 + viewer.body.vel * lead;
    [
        e0,
        e1,
        e0 + side * peek,
        e0 - side * peek,
        e1 + side * peek,
        e1 - side * peek,
        e0 + V3::new(0.0, JUMP_M, 0.0),
    ]
}

/// The points of a fighter that, in clear line, show it: head, chest, feet
/// and the chest's two edges across `side`, now and pushed ahead by its
/// speed over `lead` seconds. A new kind on the wire (a vehicle) names its
/// own points and goes through [`sees_points`].
pub fn fighter_points(target: &Fighter, side: V3, lead: f32) -> [V3; 10] {
    let head = head_h(target);
    let ch = chest_h(target);
    let edge = side * (HIT_R + 0.1);
    let p0 = target.body.pos;
    let p1 = p0 + target.body.vel * lead;
    let mut points = [V3::ZERO; 10];
    for (k, p) in [p0, p1].into_iter().enumerate() {
        let c = p + V3::new(0.0, ch, 0.0);
        points[k * 5] = p + V3::new(0.0, head, 0.0);
        points[k * 5 + 1] = c;
        points[k * 5 + 2] = p + V3::new(0.0, 0.15, 0.0);
        points[k * 5 + 3] = c + edge;
        points[k * 5 + 4] = c - edge;
    }
    points
}

/// Whether any eye has a clear line to any point.
pub fn sees_points(map: &Map, eyes: &[V3], points: &[V3]) -> bool {
    eyes.iter().any(|&e| points.iter().any(|&t| map.clear(e, t)))
}

/// Across the line from `viewer` to `target`, flat: the way a peek leans.
fn across(viewer: V3, target: V3) -> V3 {
    let d = (target - viewer).flat().norm();
    V3::new(-d.z, 0.0, d.x)
}

/// Whether `viewer` could see `target` within `lead` seconds, leaning up to
/// `peek` metres. The leak gate calls it with looser numbers than fog does.
pub fn sees(map: &Map, viewer: &Fighter, target: &Fighter, lead: f32, peek: f32) -> bool {
    let side = across(viewer.body.pos, target.body.pos);
    sees_points(
        map,
        &eyes(viewer, side, lead, peek),
        &fighter_points(target, side, lead),
    )
}

/// Whether the ground hides every line from any of `eyes` to any of
/// `points`, without casting them: along a line above all of them (from the
/// eyes' middle at their highest to the points' middle at their highest),
/// somewhere the ground stands higher than that line by more than it can
/// rise across the bundle's width. Then every line passes under the ground
/// there. Never true when one of them is clear; sometimes false when all
/// are hidden (the rays then decide).
pub fn ground_hides(map: &Map, eyes: &[V3], points: &[V3]) -> bool {
    let Some(g) = &map.terrain else {
        return false;
    };
    let mid = |ps: &[V3]| {
        let n = ps.len() as f32;
        let s = ps.iter().fold(V3::ZERO, |a, &p| a + p);
        V3::new(s.x / n, ps.iter().fold(f32::NEG_INFINITY, |a, p| a.max(p.y)), s.z / n)
    };
    let (a, b) = (mid(eyes), mid(points));
    let wide = |ps: &[V3], c: V3| ps.iter().fold(0.0f32, |w, p| w.max((*p - c).flat().len()));
    // How much higher the ground may be under any line than under the
    // reference line, plus a margin for rounding.
    let rise = wide(eyes, a).max(wide(points, b)) * g.steepest() + 0.05;
    let n = ((b - a).flat().len() * 0.25) as usize;
    (1..n.clamp(2, 64)).any(|k| {
        let s = k as f32 / n.clamp(2, 64) as f32;
        let p = a + (b - a) * s;
        g.height(p.x, p.z).is_some_and(|h| h - rise > p.y)
    })
}

/// [`sees`], as fog runs it: each eye against the target's points now and
/// its points ahead, a group at a time; a group the ground plainly hides
/// ([`ground_hides`]) casts no rays. Past [`FAR_M`] the leans and the
/// chest's edges are left out (at 40 m they turn a line by half a degree):
/// 18 rays, not 70. The plain test goes first (the aim review has cast it
/// this tick already, `Plain`): a pair in plain sight needs no more, and no
/// shortcut can hide what is in plain sight.
fn sees_full(
    map: &Map,
    viewer: &Fighter,
    target: &Fighter,
    lead: f32,
    far: bool,
    plain: impl FnOnce() -> bool,
) -> bool {
    if plain() {
        return true;
    }
    let side = across(viewer.body.pos, target.body.pos);
    let all_eyes = eyes(viewer, side, lead, PEEK_M);
    let all = fighter_points(target, side, lead);
    // Now, ahead, jumping; head, chest and feet.
    let (e, p): (&[V3], [&[V3]; 2]) = if far {
        (&[all_eyes[0], all_eyes[1], all_eyes[6]], [&all[0..3], &all[5..8]])
    } else {
        (&all_eyes, [&all[0..5], &all[5..10]])
    };
    e.iter().any(|eye| {
        p.iter().any(|group| {
            let one = core::slice::from_ref(eye);
            !ground_hides(map, one, group) && sees_points(map, one, group)
        })
    })
}

/// The plain test: is the target's chest or head in clear line from the
/// viewer's eye right now (a head over waist-high cover is in plain sight).
/// Fog must never hide what passes this.
pub fn plainly_sees(map: &Map, viewer: &Fighter, target: &Fighter) -> bool {
    let head = head_h(target);
    let e = eye(viewer);
    map.clear(e, target.body.pos + V3::new(0.0, chest_h(target), 0.0))
        || map.clear(e, target.body.pos + V3::new(0.0, head, 0.0))
}

/// Each ordered pair's [`plainly_sees`] for one tick, cast on first ask:
/// the aim review asks every tick and fog again on a snapshot tick, of the
/// same world. A fresh one each tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Plain {
    asked: [u16; MAX_FIGHTERS],
    seen: [u16; MAX_FIGHTERS],
}

const _: () = assert!(MAX_FIGHTERS <= u16::BITS as usize);

impl Plain {
    /// [`plainly_sees`] from seat `v` to seat `t` in `w`.
    pub fn sees(&mut self, w: &World, v: usize, t: usize) -> bool {
        let bit = 1u16 << t;
        if self.asked[v] & bit == 0 {
            self.asked[v] |= bit;
            if plainly_sees(&w.map, &w.fighters[v], &w.fighters[t]) {
                self.seen[v] |= bit;
            }
        }
        self.seen[v] & bit != 0
    }

    /// [`Plain::sees`] without keeping the answer: the cached one if this
    /// tick has cast it, else cast now. For the seats' fogs, each on its own
    /// thread, each asking only of its own row.
    pub fn look(&self, w: &World, v: usize, t: usize) -> bool {
        let bit = 1u16 << t;
        if self.asked[v] & bit != 0 {
            self.seen[v] & bit != 0
        } else {
            plainly_sees(&w.map, &w.fighters[v], &w.fighters[t])
        }
    }
}

/// Points of a hull a vehicle has, a time.
const VPOINTS: usize = 6;

/// The points of a hull that, in clear line, show it: its middle, the
/// middle of its top and its four top corners, now and pushed ahead by its
/// speed over `lead`. The first two now are the plain test's.
pub fn vehicle_points(v: &Vehicle, lead: f32) -> [V3; 2 * VPOINTS] {
    let (f, r) = v.axes();
    let s = v.spec();
    let mut out = [V3::ZERO; 2 * VPOINTS];
    for (k, at) in [v.pos, v.pos + v.vel * lead].into_iter().enumerate() {
        let top = at + V3::new(0.0, s.h * 0.8, 0.0);
        let p = &mut out[k * VPOINTS..(k + 1) * VPOINTS];
        p[0] = at + V3::new(0.0, s.h * 0.5, 0.0);
        p[1] = top;
        p[2] = top + f * s.half_l + r * s.half_w;
        p[3] = top - f * s.half_l - r * s.half_w;
        p[4] = top + f * s.half_l - r * s.half_w;
        p[5] = top - f * s.half_l + r * s.half_w;
    }
    out
}

/// Whether a hull is in clear line from `eye`, now or within `lead`.
pub fn sees_vehicle(map: &Map, eye: V3, v: &Vehicle, lead: f32) -> bool {
    vehicle_points(v, lead).iter().any(|&p| map.clear(eye, p))
}

/// [`sees_vehicle`], as fog runs it: the points now and the points ahead a
/// group at a time, a group the ground plainly hides casting no rays; past
/// [`FAR_M`], only the middles and two opposite corners. The plain test first.
fn sees_vehicle_full(map: &Map, eye: V3, v: &Vehicle, lead: f32, far: bool) -> bool {
    let p = vehicle_points(v, lead);
    let groups: [&[V3]; 2] = if far {
        [&p[0..4], &p[VPOINTS..VPOINTS + 4]]
    } else {
        [&p[0..VPOINTS], &p[VPOINTS..]]
    };
    let one = core::slice::from_ref(&eye);
    plainly_sees_vehicle(map, eye, v)
        || groups
            .iter()
            .any(|g| !ground_hides(map, one, g) && sees_points(map, one, g))
}

/// The plain test for a hull: its middle or the middle of its top in clear
/// line now (the first two of [`vehicle_points`]). Fog must never hide what
/// passes this.
pub fn plainly_sees_vehicle(map: &Map, eye: V3, v: &Vehicle) -> bool {
    vehicle_points(v, 0.0)[..2].iter().any(|&p| map.clear(eye, p))
}

/// One viewer's fog: until when each other seat, each vehicle and each
/// gun on the floor stays sent.
#[derive(Clone, Debug, Default)]
pub struct Fog {
    until: [u32; MAX_FIGHTERS],
    vuntil: [u32; MAX_VEHICLES],
    duntil: [u32; MAX_DROPS],
    /// Each fighter alive at the last look: one back from the dead is
    /// somewhere new, and the hold on where it was is no hold on that.
    alive: [bool; MAX_FIGHTERS],
    /// Each drop's ticks left at the last look: one with more is a new
    /// gun in the slot (where someone just fell), with no hold.
    dttl: [u32; MAX_DROPS],
}

/// How far a gun on the floor is named to a client that sees it.
pub const SEE_DROP_M: f32 = 50.0;
/// A drop is seen if a line from the eye clears the map to this far over
/// where it lies.
pub const DROP_LOOK_Y: f32 = 0.3;

/// Past this, a pair's full look comes every [`FAR_EVERY`] snapshots, not
/// every other: far away, a fighter crosses less of the screen in a
/// snapshot, and on a big map most pairs are far.
pub const FAR_M: f32 = 40.0;
pub const FAR_EVERY: usize = 4;

/// Whether `target`'s active camouflage hides it from `viewer`: cloaked
/// past [`CAMO_BOT_HIDDEN`] and farther than [`CAMO_BOT_M`], the rule the
/// bots make out a fighter by (`sim::bot`). Firing, swinging or taking a
/// hit flares it and it shows again.
pub fn camouflaged(viewer: &Fighter, target: &Fighter) -> bool {
    target.cloak() > CAMO_BOT_HIDDEN && (target.body.pos - viewer.body.pos).len() > CAMO_BOT_M
}

/// How many snapshots apart a pair's full looks are: 2, or [`FAR_EVERY`]
/// past [`FAR_M`].
pub fn every(viewer: &Fighter, target: &Fighter) -> usize {
    let d = (target.body.pos - viewer.body.pos).flat();
    if d.dot(d) > FAR_M * FAR_M {
        FAR_EVERY
    } else {
        2
    }
}

/// The look ahead of a full look `every` snapshots apart, `snap_every`
/// ticks each: `lead`, and the snapshots until the pair's next full look.
pub fn full_lead(lead: f32, every: usize, snap_every: u32) -> f32 {
    lead + (every - 1) as f32 * snap_every as f32 / TICK_HZ as f32
}

/// How long a pair stays sent after its last clear line, with snapshots
/// `snap_every` ticks apart: [`HOLD_TICKS`] at the arenas' rate, and as many
/// snapshots at a slower one, so it still outlasts the gap between full looks.
pub fn hold_ticks(snap_every: u32) -> u32 {
    HOLD_TICKS.max(HOLD_TICKS / crate::SNAP_EVERY * snap_every)
}

impl Fog {
    /// Looks again from `viewer` at `tick`, a snapshot's tick. Each enemy
    /// gets the full look (every eye, every point, [`full_lead`]) every
    /// other snapshot ([`every`]: every fourth past [`FAR_M`]), the room
    /// taking turns; on the snapshots between, the plain test, so nobody in
    /// plain sight is ever held back. The hold ([`HOLD_TICKS`]) outlasts
    /// the gap, so a pair in sight stays sent (docs/DESIGN.md §8).
    pub fn update(&mut self, w: &World, viewer: usize, lead: f32, tick: u32) {
        self.update_with(w, viewer, lead, tick, &Plain::default(), crate::SNAP_EVERY);
    }

    /// [`Fog::update`], sharing this tick's plain tests (`plain`, of `w`),
    /// for snapshots `snap_every` ticks apart.
    pub fn update_with(&mut self, w: &World, viewer: usize, lead: f32, tick: u32, plain: &Plain, snap_every: u32) {
        let me = &w.fighters[viewer];
        let snap_every = snap_every.max(1);
        let hold = hold_ticks(snap_every);
        let turn = (tick / snap_every) as usize + viewer;
        for (q, f) in w.fighters.iter().enumerate() {
            // A respawn: seen only if seen where it now is (never an unseen
            // spawn, however recently the body was in view).
            if f.alive && !self.alive[q] {
                self.until[q] = 0;
            }
            self.alive[q] = f.alive;
            if q == viewer || !f.seated {
                self.until[q] = 0;
            } else if w.rules.teams && f.team == me.team {
                self.until[q] = u32::MAX;
            } else if camouflaged(me, f) {
                // Active camouflage past its range hides from a client as
                // it does from a bot: not sent, rather than sent faint and
                // left to the client to draw so. The hold fades it out.
            } else {
                let k = every(me, f);
                let seen = if (turn + q).is_multiple_of(k) {
                    sees_full(&w.map, me, f, full_lead(lead, k, snap_every), k == FAR_EVERY, || {
                        plain.look(w, viewer, q)
                    })
                } else {
                    plain.look(w, viewer, q)
                };
                if seen {
                    self.until[q] = tick + hold;
                }
            }
        }
        // Vehicles the same way: the full look (from the eye, ahead by
        // lead and a snapshot) every other snapshot, the plain test between.
        let eye = me.eye();
        let mine = me.riding().map(|(i, _)| i);
        for (i, v) in w.vehicles.iter().enumerate() {
            if !v.active {
                self.vuntil[i] = 0;
            } else if mine == Some(i) {
                self.vuntil[i] = u32::MAX;
            } else {
                let far = (v.pos - me.body.pos).flat().len() > FAR_M;
                let k = if far { FAR_EVERY } else { 2 };
                let seen = if (turn + i).is_multiple_of(k) {
                    sees_vehicle_full(&w.map, eye, v, full_lead(lead, k, snap_every), far)
                } else {
                    plainly_sees_vehicle(&w.map, eye, v)
                };
                if seen {
                    self.vuntil[i] = tick + hold;
                } else if self.vuntil[i] == u32::MAX {
                    self.vuntil[i] = 0;
                }
            }
        }
        self.update_drops(w, eye, me, tick, turn, hold, snap_every);
    }

    /// Guns on the floor, from the eye, within [`SEE_DROP_M`]. One lying
    /// still is looked at every other snapshot (every fourth past
    /// [`FAR_M`]) and held like a fighter: a line to where it lies, once
    /// clear, tells nothing new for a hold, so one held is looked at again
    /// only as its hold runs down to the next look. One flying (out of a
    /// falling hand, or thrown by a blast) is looked at every snapshot and
    /// shown only while the line is clear. A new gun in a slot starts with
    /// no hold: never where a hidden enemy fell.
    #[allow(clippy::too_many_arguments)]
    fn update_drops(&mut self, w: &World, eye: V3, me: &Fighter, tick: u32, turn: usize, hold: u32, snap_every: u32) {
        let look_y = V3::new(0.0, DROP_LOOK_Y, 0.0);
        for i in 0..MAX_DROPS {
            let d = &w.drops[i];
            if !d.active {
                self.duntil[i] = 0;
                self.dttl[i] = 0;
                continue;
            }
            if d.ttl > self.dttl[i] {
                self.duntil[i] = 0;
            }
            self.dttl[i] = d.ttl;
            let to = d.pos - eye;
            if to.len() > SEE_DROP_M {
                self.duntil[i] = 0;
                continue;
            }
            if d.vel != V3::ZERO {
                self.duntil[i] = if w.map.clear(eye, d.pos + look_y) { tick } else { 0 };
                continue;
            }
            let far = (d.pos - me.body.pos).flat().len() > FAR_M;
            let k = if far { FAR_EVERY } else { 2 };
            // Held past the next look: nothing to ask yet.
            let held = self.duntil[i] != 0 && self.duntil[i] >= tick + k as u32 * snap_every;
            if !held && (turn + i).is_multiple_of(k) && w.map.clear(eye, d.pos + look_y) {
                self.duntil[i] = tick + hold;
            }
        }
    }

    /// Whether drop `i` may be sent at `tick`.
    pub fn shows_drop(&self, i: usize, tick: u32) -> bool {
        self.duntil[i] != 0 && self.duntil[i] >= tick
    }

    /// Whether vehicle `i` may be sent at `tick`.
    pub fn shows_vehicle(&self, i: usize, tick: u32) -> bool {
        self.vuntil[i] != 0 && self.vuntil[i] >= tick
    }

    /// Whether seat `q` may be sent at `tick`.
    pub fn shows(&self, q: usize, tick: u32) -> bool {
        self.until[q] != 0 && self.until[q] >= tick
    }

    /// Seat `q` has someone new in it, somewhere new: nothing held of it.
    pub fn forget(&mut self, q: usize) {
        self.until[q] = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::movement::Body;

    fn at(x: f32, z: f32) -> Fighter {
        Fighter {
            seated: true,
            alive: true,
            body: Body {
                pos: V3::new(x, 0.0, z),
                grounded: true,
                ..Body::default()
            },
            ..Fighter::default()
        }
    }

    #[test]
    fn the_platform_hides_what_is_behind_it() {
        let map = Map::proving_grounds();
        let me = at(-22.0, 0.0);
        let them = at(-6.0, 0.0);
        assert!(!sees(&map, &me, &them, 0.1, PEEK_M));
        // Down an open lane, they are seen.
        assert!(sees(&map, &at(-22.0, 10.0), &at(-6.0, 10.0), 0.1, PEEK_M));
    }

    #[test]
    fn a_lean_around_a_pillar_shows_them_before_the_chest_is_in_line() {
        let map = Map::proving_grounds();
        // The pillar at (−8, 12) stands between.
        let me = at(-8.0, 9.0);
        let them = at(-9.2, 15.0);
        assert!(!plainly_sees(&map, &me, &them));
        assert!(sees(&map, &me, &them, 0.0, PEEK_M));
    }

    #[test]
    fn a_runner_is_seen_before_it_clears_the_corner() {
        let map = Map::proving_grounds();
        let me = at(-8.0, 3.0);
        let mut them = at(-8.0, 15.0);
        assert!(!sees(&map, &me, &them, 0.1, 0.0));
        // Running out from behind the pillar at 7 m/s: 200 ms of lead
        // reaches past its edge.
        them.body.vel = V3::new(-7.0, 0.0, 0.0);
        assert!(sees(&map, &me, &them, 0.2, 0.0));
    }

    #[test]
    fn hold_and_teammates() {
        let rules = sim::world::Rules {
            teams: true,
            ..sim::world::Rules::doubles()
        };
        let mut w = World::new(Map::proving_grounds(), rules, 1, (2, 3));
        for s in 0..4 {
            w.seat(s, (s & 1) as u8);
        }
        w.fighters[0].body.pos = V3::new(-22.0, 0.0, 10.0);
        w.fighters[1].body.pos = V3::new(-6.0, 0.0, 10.0);
        w.fighters[2].body.pos = V3::new(22.0, 0.0, 20.0);
        w.fighters[3].body.pos = V3::new(-6.0, 0.0, -2.0);
        let mut fog = Fog::default();
        fog.update(&w, 0, 0.05, 100);
        assert!(fog.shows(1, 100), "in the open");
        assert!(fog.shows(2, 100), "a teammate, wherever");
        assert!(!fog.shows(3, 100), "behind the platform");
        assert!(!fog.shows(0, 100), "never yourself");
        // Seat 1 ducks behind the platform: held for HOLD_TICKS, then gone.
        w.fighters[1].body.pos = V3::new(-6.0, 0.0, -1.0);
        fog.update(&w, 0, 0.05, 101);
        assert!(fog.shows(1, 100 + HOLD_TICKS));
        assert!(!fog.shows(1, 101 + HOLD_TICKS));
    }
}

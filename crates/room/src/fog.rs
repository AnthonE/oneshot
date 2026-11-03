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

use sim::consts::{CROUCH_EYE, EYE, HEAD_Y, HEAD_Y_CROUCH, HIT_R, MAX_FIGHTERS, TICK_HZ};
use sim::fighter::Fighter;
use sim::geom::V3;
use sim::map::Map;
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
/// snapshot, and how far behind it draws others.
pub fn lead_s(rtt_ms: f64, playout_ticks: u32) -> f32 {
    let snap = crate::SNAP_EVERY as f64 / TICK_HZ as f64;
    let play = playout_ticks.clamp(crate::PLAYOUT_MIN, crate::PLAYOUT_MAX) as f64 / TICK_HZ as f64;
    ((rtt_ms / 2000.0 + snap + play) as f32).min(MAX_LEAD_S)
}

fn eye(f: &Fighter) -> V3 {
    let e = if f.body.crouched { CROUCH_EYE } else { EYE };
    f.body.pos + V3::new(0.0, e, 0.0)
}

fn chest_h(f: &Fighter) -> f32 {
    if f.body.crouched {
        0.75
    } else {
        1.1
    }
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
    let head = if target.body.crouched { HEAD_Y_CROUCH } else { HEAD_Y };
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

/// The plain test: is the target's chest or head in clear line from the
/// viewer's eye right now (a head over waist-high cover is in plain sight).
/// Fog must never hide what passes this.
pub fn plainly_sees(map: &Map, viewer: &Fighter, target: &Fighter) -> bool {
    let head = if target.body.crouched { HEAD_Y_CROUCH } else { HEAD_Y };
    let e = eye(viewer);
    map.clear(e, target.body.pos + V3::new(0.0, chest_h(target), 0.0))
        || map.clear(e, target.body.pos + V3::new(0.0, head, 0.0))
}

/// One viewer's fog: until when each other seat stays sent.
#[derive(Clone, Debug, Default)]
pub struct Fog {
    until: [u32; MAX_FIGHTERS],
}

impl Fog {
    /// Looks again from `viewer` at `tick`.
    pub fn update(&mut self, w: &World, viewer: usize, lead: f32, tick: u32) {
        let me = &w.fighters[viewer];
        for (q, f) in w.fighters.iter().enumerate() {
            if q == viewer || !f.seated {
                self.until[q] = 0;
            } else if w.rules.teams && f.team == me.team {
                self.until[q] = u32::MAX;
            } else if sees(&w.map, me, f, lead, PEEK_M) {
                self.until[q] = tick + HOLD_TICKS;
            }
        }
    }

    /// Whether seat `q` may be sent at `tick`.
    pub fn shows(&self, q: usize, tick: u32) -> bool {
        self.until[q] != 0 && self.until[q] >= tick
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

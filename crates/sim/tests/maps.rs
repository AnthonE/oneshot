//! Every map holds up: spawns stand clear on a floor, pads sit where a
//! fighter can take them, red and blue start alike, every spawn and pad can
//! be walked to from the first spawn, and bots fight on it. The compiler
//! (`tools/maps/compile.mjs`) checks a map's shape; this checks it plays.

use std::collections::{BTreeSet, VecDeque};

use sim::bot::Bot;
use sim::consts::*;
use sim::events::{Cause, Kind as Ev};
use sim::fmath::{fabs, round};
use sim::input::Input;
use sim::map::{by_id, Map, ANY_TEAM};
use sim::maps::ALL;
use sim::world::{Rules, World};

/// Whether a body of radius `r` standing at height `y` at (x, z) is in a
/// box (beyond touching its top).
fn blocked(m: &Map, x: f32, z: f32, y: f32, r: f32) -> bool {
    m.occupied(x, z, r, y, y + BODY_H)
}

#[test]
fn ids_are_the_maps_and_one_each() {
    let ids: BTreeSet<u8> = ALL.iter().map(|d| d.id).collect();
    assert_eq!(ids.len(), ALL.len());
    for d in ALL {
        let m = by_id(d.id).unwrap();
        assert_eq!(m.id, d.id);
        assert_eq!(m.looks.len(), m.boxes.len(), "{}", m.name);
        assert_eq!(m.solid_looks.len(), m.solids.len(), "{}", m.name);
    }
    assert_eq!(Map::proving_grounds().id, 0);
}

#[test]
fn spawns_stand_clear_and_pads_can_be_taken() {
    for d in ALL {
        let m = d.build();
        for (i, s) in m.spawns.iter().enumerate() {
            let p = s.pos;
            assert!(
                p.x - BODY_R >= m.lo.0 && p.x + BODY_R <= m.hi.0 && p.z - BODY_R >= m.lo.1 && p.z + BODY_R <= m.hi.1,
                "{} spawn {i}: out of bounds",
                m.name
            );
            assert_eq!(
                m.floor(p.x, p.z, BODY_R, p.y + STEP),
                p.y,
                "{} spawn {i}: not on its floor",
                m.name
            );
            assert!(!blocked(&m, p.x, p.z, p.y, BODY_R), "{} spawn {i}: in a box", m.name);
            assert!(!m.over_void(p.x, p.z, p.y), "{} spawn {i}: over the void", m.name);
        }
        assert!(m.pads.len() <= MAX_PADS, "{}", m.name);
        for (i, p) in m.siege.iter().flatten().enumerate() {
            assert_eq!(
                m.floor(p.x, p.z, BODY_R, p.y + STEP),
                p.y,
                "{} siege node {i}: not on a floor",
                m.name
            );
            assert!(
                !blocked(&m, p.x, p.z, p.y, BODY_R),
                "{} siege node {i}: in a box",
                m.name
            );
        }
        for (i, pad) in m.pads.iter().enumerate() {
            let p = pad.pos;
            assert_eq!(
                m.floor(p.x, p.z, BODY_R, p.y + STEP),
                p.y,
                "{} pad {i}: not on a floor",
                m.name
            );
            assert!(!blocked(&m, p.x, p.z, p.y, BODY_R), "{} pad {i}: in a box", m.name);
            assert!(!m.over_void(p.x, p.z, p.y), "{} pad {i}: over the void", m.name);
        }
        let side = |t: u8| m.spawns.iter().filter(|s| s.team == t || s.team == ANY_TEAM).count();
        assert!(
            side(0) >= 2 && side(0) == side(1),
            "{}: red {} blue {}",
            m.name,
            side(0),
            side(1)
        );
        assert!(m.spawns.len() >= 8, "{}: eight seats need eight spawns", m.name);
        // Big Team is Capture the Flag (`Rules::big_ctf`): its maps have flags.
        if d.plays(sim::map::ROT_BIG_TEAM) {
            assert!(m.flags.is_some(), "{}: a Big Team map with no flags", m.name);
        }
    }
}

/// Walkable places from `from` on a grid: a step to a neighbour climbs at
/// most STEP and needs the body's height clear; walking off a ledge drops
/// to the floor below. No jumps: everything must be reachable on foot.
fn walk(m: &Map, from: (f32, f32, f32)) -> BTreeSet<(i32, i32, i32)> {
    const G: f32 = 0.25;
    let key = |x: f32, z: f32, y: f32| (round(x / G) as i32, round(z / G) as i32, round(y * 64.0) as i32);
    let mut seen = BTreeSet::new();
    let mut q = VecDeque::new();
    let k0 = key(from.0, from.2, from.1);
    seen.insert(k0);
    q.push_back(k0);
    let r = BODY_R;
    while let Some((ix, iz, iy)) = q.pop_front() {
        let y = iy as f32 / 64.0;
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (ix + dx, iz + dz);
            let (x, z) = (nx as f32 * G, nz as f32 * G);
            if x - r < m.lo.0 || x + r > m.hi.0 || z - r < m.lo.1 || z + r > m.hi.1 {
                continue;
            }
            let ny = m.floor(x, z, r, y + STEP);
            if m.lost_floor(x, z, ny) || blocked(m, x, z, ny, r) {
                continue;
            }
            // Up ground steeper than movement climbs (movement::too_steep).
            let (px, pz) = (ix as f32 * G, iz as f32 * G);
            if m.terrain.is_some() && m.ground(x, z) > y + 0.01 && m.ground(x, z) - m.ground(px, pz) > G * MAX_CLIMB {
                continue;
            }
            let k = key(x, z, ny);
            if seen.insert(k) {
                q.push_back(k);
            }
        }
    }
    seen
}

#[test]
fn everything_can_be_walked_to() {
    for d in ALL {
        let m = d.build();
        let s0 = m.spawns[0].pos;
        let reach = walk(&m, (s0.x, s0.y, s0.z));
        let near = |x: f32, z: f32, y: f32| {
            reach.iter().any(|&(ix, iz, iy)| {
                fabs(ix as f32 * 0.25 - x) <= 0.25
                    && fabs(iz as f32 * 0.25 - z) <= 0.25
                    && fabs(iy as f32 / 64.0 - y) < 0.01
            })
        };
        for (i, s) in m.spawns.iter().enumerate() {
            assert!(
                near(s.pos.x, s.pos.z, s.pos.y),
                "{}: spawn {i} can't be walked to",
                m.name
            );
        }
        for (i, p) in m.pads.iter().enumerate() {
            assert!(
                near(p.pos.x, p.pos.z, p.pos.y),
                "{}: pad {i} can't be walked to",
                m.name
            );
        }
        for (i, p) in m.siege.iter().flatten().enumerate() {
            assert!(near(p.x, p.z, p.y), "{}: siege node {i} can't be walked to", m.name);
        }
        for (i, r) in m.roam.iter().enumerate() {
            assert!(
                reach
                    .iter()
                    .any(|&(ix, iz, _)| fabs(ix as f32 * 0.25 - r.x) <= 0.25 && fabs(iz as f32 * 0.25 - r.z) <= 0.25),
                "{}: roam point {i} can't be walked to",
                m.name
            );
        }
    }
}

/// Cloudbreak's tower climbs on foot, floor by floor, and its walks join on.
#[test]
fn cloudbreak_climbs_and_its_walks_join() {
    let m = by_id(7).unwrap();
    let s0 = m.spawns[0].pos;
    let reach = walk(&m, (s0.x, s0.y, s0.z));
    let stands = |x: f32, z: f32, y: f32| {
        reach.iter().any(|&(ix, iz, iy)| {
            fabs(ix as f32 * 0.25 - x) <= 0.25
                && fabs(iz as f32 * 0.25 - z) <= 0.25
                && fabs(iy as f32 / 64.0 - y) < 0.03
        })
    };
    for (what, x, z, y) in [
        ("the sighting room", 0.0, 18.9, 4.5),
        ("the shoulder", -4.8, 18.5, 4.5),
        ("the corner", -4.8, 24.8, 6.9),
        ("the back walk", 0.0, 24.8, 8.5),
        ("the gallery", 0.0, 19.0, 8.5),
        ("the north walk", 0.0, 26.2, -1.3),
        ("red's north walk", -9.9, 23.3, -1.3),
        ("blue's north walk", 9.9, 23.3, -1.3),
        ("red's south walk", -12.0, -19.7, -1.3),
        ("blue's south walk", 12.0, -19.7, -1.3),
    ] {
        assert!(stands(x, z, y), "{what} can't be walked to");
    }
}

/// Cistern's levels all join, up and back, and its lifts land on its bridges.
#[test]
fn cisterns_levels_join() {
    let m = by_id(2).unwrap();
    let s0 = m.spawns[0].pos;
    let reach = walk(&m, (s0.x, s0.y, s0.z));
    let stands = |x: f32, z: f32, y: f32| {
        reach.iter().any(|&(ix, iz, iy)| {
            fabs(ix as f32 * 0.25 - x) <= 0.25
                && fabs(iz as f32 * 0.25 - z) <= 0.25
                && fabs(iy as f32 / 64.0 - y) < 0.03
        })
    };
    for (what, x, z, y) in [
        ("the far quay", 19.0, 0.0, 1.8),
        ("the dais", 0.0, 0.0, 1.0),
        ("a stair's foot", -11.5, 22.0, 1.8),
        ("a landing", 0.0, 21.0, 5.2),
        ("the red arm", -11.0, 0.0, 5.2),
        ("the ring", 4.3, 0.0, 5.2),
    ] {
        assert!(stands(x, z, y), "{what} can't be walked to");
    }
}

/// Overpass' slip roads and its blocks' stairs all climb, both sides.
#[test]
fn overpass_climbs() {
    let m = by_id(1).unwrap();
    let s0 = m.spawns[0].pos;
    let reach = walk(&m, (s0.x, s0.y, s0.z));
    let stands = |x: f32, z: f32, y: f32| {
        reach.iter().any(|&(ix, iz, iy)| {
            fabs(ix as f32 * 0.25 - x) <= 0.25
                && fabs(iz as f32 * 0.25 - z) <= 0.25
                && fabs(iy as f32 / 64.0 - y) < 0.03
        })
    };
    for (what, x, z, y) in [
        ("the road", 0.0, 0.0, 0.0),
        ("the flyover", 0.0, 0.0, 5.0),
        ("red's roof", -27.0, 0.0, 3.5),
        ("blue's roof", 27.0, 0.0, 3.5),
        ("the north alley", 0.0, 19.5, 0.0),
    ] {
        assert!(stands(x, z, y), "{what} can't be walked to");
    }
}

/// Bulkhead's spawns don't look down each other's throats across the dock.
#[test]
fn bulkheads_spawns_are_mostly_out_of_each_others_sight() {
    let m = by_id(5).unwrap();
    let head = |p: sim::geom::V3| sim::geom::V3::new(p.x, p.y + EYE, p.z);
    let (mut seen, mut pairs) = (0, 0);
    for a in m.spawns.iter().filter(|s| s.team == 0) {
        for b in m.spawns.iter().filter(|s| s.team == 1) {
            pairs += 1;
            seen += usize::from(m.clear(head(a.pos), head(b.pos)));
        }
    }
    assert!(seen * 3 <= pairs, "{seen} of {pairs} pairs of spawns see each other");
}

/// Every rotation the server plays has a map in it (else it falls back to
/// any map that can play the mode), and a map out of the rotation is in
/// none of them.
#[test]
fn every_rotation_has_a_map() {
    use sim::map::*;
    for (rot, name) in [
        (ROT_CASUAL, "casual"),
        (ROT_DUEL, "duel"),
        (ROT_DOUBLES, "doubles"),
        (ROT_FFA, "ffa"),
        (ROT_GUN_GAME, "gun game"),
        (ROT_CUP, "cup"),
        (ROT_BIG_TEAM, "big team"),
        (ROT_CTF, "ctf"),
        (ROT_SIEGE, "siege"),
    ] {
        assert!(ALL.iter().any(|d| d.plays(rot)), "no map plays {name}");
        for d in ALL.iter().filter(|d| d.practice) {
            assert!(!d.plays(rot), "{} is out of the rotation", d.name);
        }
    }
}

#[test]
fn bots_fight_on_every_map() {
    for d in ALL {
        for rules in [Rules::doubles(), Rules::ffa()] {
            let rules = Rules {
                score_limit: 1000,
                time_limit_ms: 0,
                ..rules
            };
            let n = rules.seats as usize;
            let mut w = World::new(d.build(), rules, 3, (1, 2));
            let mut bots: Vec<Bot> = (0..n).map(|s| Bot::new(3, s, 0.6)).collect();
            for s in 0..n {
                w.seat(s, (s & 1) as u8);
            }
            let mut inputs = [Input::default(); MAX_FIGHTERS];
            // Off the edge with nobody's help (a map with a void).
            let mut own_falls = 0;
            for _ in 0..20_000 {
                for (s, b) in bots.iter_mut().enumerate() {
                    inputs[s] = b.think(&w, s);
                }
                w.step(&inputs, &[0; MAX_FIGHTERS]);
                own_falls += w
                    .events
                    .as_slice()
                    .iter()
                    .filter(|e| e.kind == Ev::Kill && e.c == Cause::Fall as u16 && e.a == e.b)
                    .count();
            }
            let kills: u16 = w.fighters.iter().map(|f| f.kills).sum();
            assert!(
                kills >= 5,
                "{} with {n} seats: only {kills} kills in 20000 ticks",
                d.name
            );
            assert_eq!(own_falls, 0, "{} with {n} seats: bots walked off the edge", d.name);
        }
    }
}

/// Big Team's sixteen seated at once on Dry Fork: nobody on top of anyone,
/// and each side spread over its spawns, never one heap at its base.
#[test]
fn a_full_big_team_spreads_over_its_spawns() {
    let mut w = World::new(by_id(3).unwrap(), Rules::big_team(), 1, (2, 3));
    for s in 0..MAX_FIGHTERS {
        w.seat(s, (s & 1) as u8);
    }
    for a in 0..MAX_FIGHTERS {
        for b in a + 1..MAX_FIGHTERS {
            let d = (w.fighters[a].body.pos - w.fighters[b].body.pos).len();
            assert!(d > 2.0, "{a} and {b} spawned {d} m apart");
        }
        let crowd = (0..MAX_FIGHTERS)
            .filter(|&b| b != a && w.fighters[b].team == w.fighters[a].team)
            .filter(|&b| (w.fighters[a].body.pos - w.fighters[b].body.pos).len() < SPAWN_CROWD_R)
            .count();
        assert!(
            crowd <= 2,
            "{a} spawned with {crowd} teammates within {SPAWN_CROWD_R} m"
        );
    }
}

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
        // A map kept to Big Team (Icefall: too far across for four to meet
        // in the time) is fought on by Big Team's sixteen.
        let played = if d.modes == sim::map::ROT_BIG_TEAM {
            vec![Rules::big_team()]
        } else {
            vec![Rules::doubles(), Rules::ffa()]
        };
        for rules in played {
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

/// Big Team's sixteen seated at once on its maps (Dry Fork, Icefall):
/// nobody on top of anyone, and each side spread over its spawns, never one
/// heap at its base.
#[test]
fn a_full_big_team_spreads_over_its_spawns() {
    for id in [3, 9] {
        let mut w = World::new(by_id(id).unwrap(), Rules::big_team(), 1, (2, 3));
        for s in 0..MAX_FIGHTERS {
            w.seat(s, (s & 1) as u8);
        }
        for a in 0..MAX_FIGHTERS {
            for b in a + 1..MAX_FIGHTERS {
                let d = (w.fighters[a].body.pos - w.fighters[b].body.pos).len();
                assert!(d > 2.0, "map {id}: {a} and {b} spawned {d} m apart");
            }
            let crowd = (0..MAX_FIGHTERS)
                .filter(|&b| b != a && w.fighters[b].team == w.fighters[a].team)
                .filter(|&b| (w.fighters[a].body.pos - w.fighters[b].body.pos).len() < SPAWN_CROWD_R)
                .count();
            assert!(
                crowd <= 2,
                "map {id}: {a} spawned with {crowd} teammates within {SPAWN_CROWD_R} m"
            );
        }
    }
}

/// Icefall's launchers throw where they are meant to, each side's alike:
/// the long throw from the main door onto the massif's perch, the north
/// door's to the lake's shore, the south door's to the first drumlin, and
/// the lift up through the parapet's gap onto the terrace. A throw that
/// clips the facade or a spur on the way lands somewhere else.
#[test]
fn icefalls_launchers_land_on_their_marks() {
    use sim::geom::V3;
    use sim::movement::{step, Body};
    let m = by_id(9).unwrap();
    // Red's marks (x, z, feet); blue's are their mirror images.
    let marks: [(f32, f32, f32); 4] = [
        (-84.0, 0.0, 25.9),
        (-106.0, 48.0, -1.6),
        (-118.0, -72.0, 0.3),
        (-211.0, 23.0, 12.5),
    ];
    assert_eq!(m.launchers.len(), 8);
    for (n, l) in m.launchers.iter().enumerate() {
        let mid = (l.area.min + l.area.max) * 0.5;
        let mut b = Body {
            pos: V3::new(mid.x, m.floor(mid.x, mid.z, BODY_R, l.area.max.y), mid.z),
            vel: V3::new(0.0, 0.0, 0.0),
            grounded: true,
            crouched: false,
            duck: 0,
        };
        step(&mut b, &Input::default(), &m);
        assert!(!b.grounded, "launcher {n} throws nobody");
        let mut t = 0;
        while !b.grounded && t < TICK_HZ * 8 {
            step(&mut b, &Input::default(), &m);
            t += 1;
        }
        let (x, z, y) = marks[n / 2];
        let x = if n % 2 == 1 { -x } else { x };
        let off = fabs(b.pos.x - x) + fabs(b.pos.z - z);
        assert!(
            off < 4.0 && fabs(b.pos.y - y) < 1.5,
            "launcher {n} lands at {:?}, {off} m off its mark ({x}, {z}, {y})",
            b.pos
        );
    }
}

/// Icefall's ring is a road: a truck leaves red's hangar by its main door,
/// goes out by the north gate, along the lake's shore, over the col, back
/// along the moraine and through the snow shed, and home by the south gate,
/// never stopped by a door, a rock or a bank. And a lap takes about as long
/// as the route is at its speed: nothing on it is a crawl.
#[test]
fn a_truck_drives_icefalls_ring() {
    let m = by_id(9).unwrap();
    let d = m.vehicles[0];
    let mut v = sim::vehicle::Vehicle {
        active: true,
        pos: d.pos,
        yaw: d.yaw,
        grounded: true,
        health: VEH_HEALTH,
        ..Default::default()
    };
    let way: [(f32, f32); 18] = [
        (-200.0, 0.0),
        (-176.0, 6.0),
        (-158.0, 52.0),
        (-112.0, 48.0),
        (-60.0, 52.0),
        (-28.0, 54.0),
        (-8.0, 44.0),
        (-12.0, 16.0),
        (-12.0, -16.0),
        (-6.0, -46.0),
        (40.0, -100.0),
        (30.0, -121.0),
        (-30.0, -121.0),
        (-60.0, -116.0),
        (-110.0, -100.0),
        (-158.0, -52.0),
        (-176.0, -6.0),
        (-200.0, 0.0),
    ];
    let mut ticks = 0u32;
    let mut length = 0.0;
    let mut from = (v.pos.x, v.pos.z);
    for (k, &(x, z)) in way.iter().enumerate() {
        length += ((x - from.0) * (x - from.0) + (z - from.1) * (z - from.1)).sqrt();
        from = (x, z);
        let mut t = 0;
        loop {
            let (dx, dz) = (x - v.pos.x, z - v.pos.z);
            if dx * dx + dz * dz < 36.0 {
                break;
            }
            let (f, _) = v.axes();
            // Which way the mark lies off the nose, and how far round.
            let cross = f.x * dz - f.z * dx;
            let dot = f.x * dx + f.z * dz;
            let far = (dx * dx + dz * dz).sqrt();
            let steer = (cross / far * 3.0).clamp(-1.0, 1.0);
            let inp = Input {
                mx: (steer * 127.0) as i8,
                // Slower into a mark behind it or hard round.
                mz: if dot < 0.0 || fabs(steer) > 0.9 { 50 } else { 127 },
                ..Input::default()
            };
            v.step_local(&inp, &m);
            t += 1;
            assert!(
                t < TICK_HZ * 25,
                "stuck on the way to mark {k} ({x}, {z}) at {:?}",
                v.pos
            );
        }
        ticks += t;
    }
    let secs = ticks as f32 / TICK_HZ as f32;
    assert!(secs < length / VEH_TOP * 1.6, "a lap of {length} m took {secs} s");
}

/// Icefall's mountains are what hold a flier in, never its bounds: all
/// along them the ground stands over the highest a flier climbs (its
/// ceiling over the terrace it starts from).
#[test]
fn icefalls_rim_stands_over_a_fliers_ceiling() {
    let m = by_id(9).unwrap();
    let pad = m
        .vehicles
        .iter()
        .filter(|v| v.kind == sim::vehicle::FLIER)
        .map(|v| v.pos.y)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(pad.is_finite(), "Icefall has fliers");
    let ceil = pad + FLY_CEIL + FLY_H;
    let mut x = m.lo.0;
    while x <= m.hi.0 {
        for z in [m.lo.1, m.hi.1] {
            assert!(m.ground(x, z) > ceil, "the rim at ({x}, {z}) is {} m", m.ground(x, z));
        }
        x += 2.0;
    }
    let mut z = m.lo.1;
    while z <= m.hi.1 {
        for x in [m.lo.0, m.hi.0] {
            assert!(m.ground(x, z) > ceil, "the rim at ({x}, {z}) is {} m", m.ground(x, z));
        }
        z += 2.0;
    }
}

/// A jump goes up and over, never sideways: a head stopped at a ceiling
/// (Dry Fork's bunker, under its roof), feet on a top the grid rounds into
/// (Overpass' van) or a top a float holds a hair high (Chrysalis' bridge)
/// each threw a body out of it sideways, half a metre to three.
#[test]
fn a_jump_is_never_thrown_sideways() {
    use sim::geom::V3;
    use sim::input::JUMP;
    use sim::movement::{step, Body};
    // (map, x, z, feet, yaw): a run at the wall, or a jump standing.
    let runs: [(u8, f32, f32, f32, Option<u16>); 4] = [
        (3, -98.0, 0.3, 0.125, Some(0)),
        (1, -16.4, 19.0, 2.3, None),
        (6, -12.0, -1.0, 7.0, None),
        (6, -12.0, 1.2, 7.0, None),
    ];
    for (id, x, z, y, yaw) in runs {
        let m = by_id(id).unwrap();
        let mut b = Body {
            pos: V3::new(x, m.floor(x, z, BODY_R, y + STEP), z),
            vel: V3::new(0.0, 0.0, 0.0),
            grounded: true,
            crouched: false,
            duck: 0,
        };
        // Over by tick 100, short of the launcher at its door.
        for t in 0..100u32 {
            let inp = Input {
                buttons: if t == 15 { JUMP } else { 0 },
                yaw: yaw.unwrap_or(0),
                pitch: 0,
                mx: 0,
                mz: if yaw.is_some() { 127 } else { 0 },
            };
            let (px, pz) = (b.pos.x, b.pos.z);
            step(&mut b, &inp, &m);
            let d = fabs(b.pos.x - px) + fabs(b.pos.z - pz);
            assert!(d < 0.1, "{}: thrown {d} m at tick {t} from ({px}, {pz})", m.name);
        }
        if yaw.is_some() {
            assert!(
                b.pos.x > -95.0,
                "{}: never cleared the low wall, at {:?}",
                m.name,
                b.pos
            );
        }
    }
}

/// Dry Fork's closed end is held by its cliff, never by the bound: a run up
/// the bank behind red's base, jumping all the way, stops against rock
/// short of it. The bound once stood 20 m out on the bank, a wall nobody
/// could see.
#[test]
fn dry_forks_closed_end_is_its_cliff() {
    use sim::geom::V3;
    use sim::input::JUMP;
    use sim::movement::{step, Body};
    let m = by_id(3).unwrap();
    for z in [-50.0, -30.0, -10.0, 0.0, 10.0, 30.0, 50.0] {
        let mut b = Body {
            pos: V3::new(-125.0, m.floor(-125.0, z, BODY_R, 40.0), z),
            vel: V3::new(0.0, 0.0, 0.0),
            grounded: true,
            crouched: false,
            duck: 0,
        };
        for t in 0..TICK_HZ * 30 {
            let inp = Input {
                buttons: if t % 2 == 0 { JUMP } else { 0 },
                yaw: 32768,
                pitch: 0,
                mx: 0,
                mz: 127,
            };
            step(&mut b, &inp, &m);
        }
        assert!(
            b.pos.x < -150.0 && b.pos.x > m.lo.0 + 2.0,
            "z {z}: stopped at {:?}, the bound at {}",
            b.pos,
            m.lo.0
        );
    }
}

//! The void: off the edge of an island a fighter falls to its death. Its
//! own walk off is its own death; thrown off by someone who hurt it, the
//! kill is theirs, however long the fall, and that credit lapses after a
//! few seconds on its feet. What it held goes down with it, and bots never
//! walk off on their own.

use sim::arms::{AR, MAGNUM};
use sim::bot::Bot;
use sim::consts::*;
use sim::events::{Cause, Event, Kind as Ev};
use sim::fmath::fabs;
use sim::geom::V3;
use sim::input::*;
use sim::map::{Def, LightDef};
use sim::terrain::{TerrainDef, HEIGHT_Q};
use sim::world::{Phase, Rules, World};

const NO_LAG: [u32; MAX_FIGHTERS] = [0; MAX_FIGHTERS];
/// The island: flat at 0 over |x|, |z| ≤ 10, its cliffs a metre wide down
/// to 40 m under, the void at 8 m under.
const HALF: f32 = 10.0;
const VOID: f32 = -8.0;

fn island() -> Def {
    let n = 41usize;
    let heights: Vec<i16> = (0..n * n)
        .map(|k| {
            let (x, z) = ((k % n) as f32 - 20.0, (k / n) as f32 - 20.0);
            if fabs(x) <= HALF && fabs(z) <= HALF {
                0
            } else {
                (-40.0 * HEIGHT_Q) as i16
            }
        })
        .collect();
    Def {
        id: 250,
        name: "island",
        practice: true,
        lo: (-18.0, -18.0),
        hi: (18.0, 18.0),
        void: VOID,
        boxes: &[],
        looks: &[],
        spawns: &[
            ([-6.0, 0.0, -6.0], 0, 0),
            ([6.0, 0.0, 6.0], 32768, 1),
            ([-6.0, 0.0, 6.0], 0, 0),
            ([6.0, 0.0, -6.0], 32768, 1),
        ],
        pads: &[],
        roam: &[[-7.0, -7.0], [7.0, 7.0], [-7.0, 7.0], [7.0, -7.0], [0.0, 0.0]],
        terrain: Some(TerrainDef {
            cell: 1.0,
            x0: -20.0,
            z0: -20.0,
            nx: n as u16,
            nz: n as u16,
            heights: Box::leak(heights.into_boxed_slice()),
        }),
        launchers: &[],
        vehicles: &[],
        solids: &[],
        solid_looks: &[],
        dress: &[],
        dress_solids: &[],
        profiles: &[],
        dress_profiles: &[],
        nav: false,
        light: LightDef::DAY,
        flags: &[],
    }
}

/// Two fighters on the island, everyone for themselves, countdown over:
/// 0 at `at`, 1 across the island.
fn two(at: V3) -> World {
    let rules = Rules {
        teams: false,
        score_limit: 100,
        time_limit_ms: 0,
        loadout: [AR, MAGNUM],
        ..Rules::duel()
    };
    let mut w = World::new(island().build(), rules, 1, (3, 4));
    w.seat(0, 0);
    w.seat(1, 1);
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &NO_LAG);
    }
    w.fighters[0].body.pos = at;
    w.fighters[1].body.pos = V3::new(8.0, 0.0, 8.0);
    w
}

/// Steps with fighter 0 under `inp` until it dies (at most 6 s): its death.
fn until_dead(w: &mut World, inp: Input) -> Option<Event> {
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0] = inp;
    for _ in 0..TICK_HZ * 6 {
        w.step(&inputs, &NO_LAG);
        if let Some(e) = w.events.as_slice().iter().find(|e| e.kind == Ev::Kill && e.b == 0) {
            return Some(*e);
        }
    }
    None
}

/// West, held forward.
const WEST: Input = Input {
    buttons: 0,
    yaw: 32768,
    pitch: 0,
    mx: 0,
    mz: 127,
};

/// A frag set down at `at` by fighter 1, fuse lit.
fn frag(w: &mut World, at: V3) {
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[1] = Input {
        buttons: GRENADE,
        pitch: -PITCH_MAX,
        ..Input::default()
    };
    w.step(&inputs, &NO_LAG);
    for i in 0..MAX_PROJ {
        if w.proj[i].active {
            w.proj[i].pos = at;
            w.proj[i].vel = V3::ZERO;
        }
    }
}

#[test]
fn walking_off_the_edge_is_ones_own_death_and_takes_the_guns_down() {
    let mut w = two(V3::new(-8.0, 0.0, 0.0));
    let e = until_dead(&mut w, WEST).expect("fell");
    assert_eq!((e.a, e.c), (0, Cause::Fall as u16));
    assert!(e.pos.y < VOID, "{:?}", e.pos);
    assert_eq!(w.score[0], -1);
    assert_eq!(w.fighters[0].deaths, 1);
    assert_eq!(w.drops.live().count(), 0, "the guns went down with it");
    // Back on the island.
    let idle = [Input::default(); MAX_FIGHTERS];
    for _ in 0..ticks(RESPAWN_MS) + 2 {
        w.step(&idle, &NO_LAG);
    }
    assert!(w.fighters[0].alive && w.fighters[0].body.pos.y == 0.0);
}

#[test]
fn thrown_off_by_a_blast_is_the_throwers_kill() {
    // A metre in from the west cliff, a frag three in from that.
    let mut w = two(V3::new(-9.0, 0.0, 0.0));
    frag(&mut w, V3::new(-6.0, 0.0, 0.0));
    let e = until_dead(&mut w, Input::default()).expect("thrown off");
    assert_eq!((e.a, e.b, e.c), (1, 0, Cause::Fall as u16));
    assert_eq!(w.score[1], 1);
    assert_eq!(w.fighters[1].kills, 1);
}

#[test]
fn the_credit_lapses_after_a_while_on_ones_feet() {
    // Hurt well inside the island, then a walk off: within the time, the
    // hurter's; after it on its feet, one's own.
    for (wait_ms, by) in [(FALL_CREDIT_MS - 1500, 1u8), (FALL_CREDIT_MS + 500, 0)] {
        let mut w = two(V3::new(-3.0, 0.0, 0.0));
        frag(&mut w, V3::new(0.0, 0.0, 0.0));
        let idle = [Input::default(); MAX_FIGHTERS];
        let mut hurt = false;
        for _ in 0..TICK_HZ * 3 {
            w.step(&idle, &NO_LAG);
            if w.events.as_slice().iter().any(|e| e.kind == Ev::Hit && e.b == 0) {
                hurt = true;
                break;
            }
        }
        for _ in 0..ticks(wait_ms) {
            w.step(&idle, &NO_LAG);
        }
        assert!(hurt && w.fighters[0].alive && w.fighters[0].body.pos.x > -HALF + 1.0);
        let e = until_dead(&mut w, WEST).expect("fell");
        assert_eq!((e.a, e.c), (by, Cause::Fall as u16), "after {wait_ms} ms");
    }
}

#[test]
fn rounds_and_frags_into_the_void_are_lost_never_going_off() {
    let mut w = two(V3::new(0.0, 0.0, 0.0));
    frag(&mut w, V3::new(-14.0, -2.0, 0.0));
    let idle = [Input::default(); MAX_FIGHTERS];
    for _ in 0..TICK_HZ * 4 {
        w.step(&idle, &NO_LAG);
        assert!(!w.events.as_slice().iter().any(|e| e.kind == Ev::Blast));
    }
    assert_eq!(w.proj.live().count(), 0);
}

#[test]
fn bots_never_walk_off_the_island_on_their_own() {
    for rules in [Rules::doubles(), Rules::ffa()] {
        let rules = Rules {
            score_limit: 1000,
            time_limit_ms: 0,
            ..rules
        };
        let n = rules.seats as usize;
        let mut w = World::new(island().build(), rules, 5, (1, 2));
        let mut bots: Vec<Bot> = (0..n).map(|s| Bot::new(5, s, 0.6)).collect();
        for s in 0..n {
            w.seat(s, (s & 1) as u8);
        }
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        let (mut own, mut thrown, mut kills) = (0, 0, 0);
        for _ in 0..20_000 {
            for (s, b) in bots.iter_mut().enumerate() {
                inputs[s] = b.think(&w, s);
            }
            w.step(&inputs, &[0; MAX_FIGHTERS]);
            for e in w.events.as_slice().iter().filter(|e| e.kind == Ev::Kill) {
                kills += 1;
                if e.c == Cause::Fall as u16 {
                    if e.a == e.b {
                        own += 1;
                    } else {
                        thrown += 1;
                    }
                }
            }
        }
        assert_eq!(own, 0, "{n} seats: {own} walked off ({thrown} thrown, {kills} kills)");
        assert!(kills >= 5, "{n} seats: {kills} kills");
    }
}

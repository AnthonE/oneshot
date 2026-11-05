//! Capture the Flag as a player meets it: taking the enemy's flag, not
//! firing while carrying it, scoring it at home only while your own flag is
//! there, dropping it on death, returning it by touch or on its clock; and
//! bots that play it on every map with flags.

use sim::bot::{bot_match_on, Bot};
use sim::consts::*;
use sim::events::Kind as Ev;
use sim::geom::V3;
use sim::input::*;
use sim::map::Map;
use sim::world::{Phase, Rules, World, FLAG_DOWN, FLAG_HELD, FLAG_HOME, NO_WINNER};

const NO_LAG: [u32; MAX_FIGHTERS] = [0; MAX_FIGHTERS];
const RED: V3 = V3::new(-30.0, 0.0, 0.0);
const BLUE: V3 = V3::new(30.0, 0.0, 0.0);

/// The open test map with flags at ±30 m, two a side, countdown over.
fn world() -> World {
    let mut map = Map::open();
    map.flags = Some([RED, BLUE]);
    let mut w = World::new(map, Rules::ctf(), 1, (5, 6));
    for s in 0..4 {
        w.seat(s, (s & 1) as u8);
    }
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &NO_LAG);
    }
    // Everyone out of the way of both flags.
    for s in 0..4 {
        w.fighters[s].body.pos = V3::new(0.0, 0.0, -40.0 + 5.0 * s as f32);
    }
    w
}

fn step(w: &mut World, inputs: &[Input; MAX_FIGHTERS]) -> Vec<(u8, u8, u8)> {
    w.step(inputs, &NO_LAG);
    w.events
        .as_slice()
        .iter()
        .filter(|e| e.kind == Ev::Flag)
        .map(|e| (e.part, e.a, e.b))
        .collect()
}

fn idle(w: &mut World) -> Vec<(u8, u8, u8)> {
    step(w, &[Input::default(); MAX_FIGHTERS])
}

#[test]
fn an_enemy_takes_the_flag_carries_it_and_scores_at_home() {
    let mut w = world();
    // Seat 1 (blue) walks onto red's flag.
    w.fighters[1].body.pos = RED;
    assert_eq!(idle(&mut w), vec![(0, 1, 0)], "blue takes red's flag");
    assert_eq!(w.flags[0].state, FLAG_HELD);
    assert!(w.fighters[1].flag);

    // Both hands on it: the trigger does nothing, nor does a grenade.
    let mag = w.fighters[1].gun().mag;
    let shots = w.fighters[1].shots;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[1] = Input {
        buttons: FIRE | GRENADE,
        ..Input::default()
    };
    for _ in 0..10 {
        step(&mut w, &inputs);
        inputs[1].buttons ^= FIRE | GRENADE;
    }
    assert_eq!((w.fighters[1].gun().mag, w.fighters[1].shots), (mag, shots));
    assert!(w.proj.live().next().is_none());

    // The flag rides with its carrier.
    w.fighters[1].body.pos = V3::new(5.0, 0.0, 5.0);
    idle(&mut w);
    assert!((w.flags[0].pos - V3::new(5.0, 0.0, 5.0)).len() < 0.01);

    // At blue's base, with blue's flag home: a capture, and red's flag goes
    // home.
    w.fighters[1].body.pos = BLUE;
    assert_eq!(idle(&mut w), vec![(3, 1, 0)]);
    assert_eq!(w.score[1], 1);
    assert_eq!(w.flags[0].state, FLAG_HOME);
    assert_eq!(w.flags[0].pos, RED);
    assert!(!w.fighters[1].flag);
}

#[test]
fn no_capture_while_your_own_flag_is_away() {
    let mut w = world();
    w.fighters[1].body.pos = RED;
    idle(&mut w);
    // Red takes blue's flag too.
    w.fighters[0].body.pos = BLUE;
    idle(&mut w);
    assert_eq!(w.flags[1].state, FLAG_HELD);
    // Blue's carrier home: blue's flag is in red's hands, so no score.
    w.fighters[1].body.pos = BLUE;
    w.fighters[0].body.pos = V3::new(0.0, 0.0, 30.0);
    idle(&mut w);
    assert_eq!(w.score, [0; MAX_FIGHTERS]);
    assert!(w.fighters[1].flag);
}

#[test]
fn a_dead_carrier_drops_it_a_defender_returns_it_and_the_clock_does_too() {
    let mut w = world();
    w.fighters[1].body.pos = RED;
    idle(&mut w);
    w.fighters[1].body.pos = V3::new(10.0, 0.0, 10.0);
    idle(&mut w);
    // Red kills the carrier with a melee from behind.
    w.fighters[0].body.pos = V3::new(8.5, 0.0, 10.0);
    w.fighters[0].yaw = 0;
    w.fighters[1].yaw = 0;
    let mut saw = Vec::new();
    for _ in 0..TICK_HZ {
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        if w.fighters[0].melee_cool == 0 {
            inputs[0].buttons = MELEE;
        }
        saw.extend(step(&mut w, &inputs));
        if !w.fighters[1].alive {
            break;
        }
    }
    assert!(!w.fighters[1].alive, "the carrier died");
    assert!(saw.contains(&(1, 1, 0)), "and dropped red's flag: {saw:?}");
    assert_eq!(w.flags[0].state, FLAG_DOWN);
    // Seat 0 (red) touches it: back home at once.
    w.fighters[0].body.pos = w.flags[0].pos;
    assert_eq!(idle(&mut w), vec![(2, 0, 0)]);
    assert_eq!(w.flags[0].state, FLAG_HOME);

    // Dropped again and left alone: home on its clock.
    w.fighters[3].body.pos = RED;
    idle(&mut w);
    w.fighters[3].body.pos = V3::new(-10.0, 0.0, 20.0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[3].buttons = USE;
    assert_eq!(step(&mut w, &inputs), vec![(1, 3, 0)], "USE lets go of it");
    // Not straight back up, even standing on it.
    for _ in 0..ticks(FLAG_REGRAB_MS) - 2 {
        assert_eq!(idle(&mut w), vec![]);
    }
    w.fighters[3].body.pos = V3::new(-10.0, 0.0, 30.0);
    let mut back = Vec::new();
    for _ in 0..ticks(FLAG_RETURN_MS - FLAG_REGRAB_MS) + 2 {
        back.extend(idle(&mut w));
    }
    assert_eq!(back, vec![(2, NO_WINNER, 0)]);
    assert_eq!(w.flags[0].pos, RED);
}

#[test]
fn kills_dont_score_and_three_captures_win() {
    let mut w = world();
    for n in 1..=3 {
        w.fighters[1].body.pos = RED;
        idle(&mut w);
        w.fighters[1].body.pos = BLUE;
        idle(&mut w);
        assert_eq!(w.score[1], n);
        w.fighters[1].body.pos = V3::new(0.0, 0.0, -35.0);
    }
    assert_eq!(w.phase, Phase::Over);
    assert_eq!(w.winner, 1);
}

#[test]
fn bots_go_for_flags_on_every_map_with_flags() {
    // Bots take flags on the arenas (CTF's playlist) within ten minutes;
    // carrying one home past defenders is rarer (a capture or two an hour)
    // and the capture itself is pinned above. On a Big Team map the flags
    // are too far apart for bots on foot, so there it only plays.
    for d in sim::maps::ALL.iter().filter(|d| d.flags.len() == 2) {
        let rules = Rules {
            score_limit: 1000,
            time_limit_ms: 0,
            ..Rules::ctf()
        };
        let mut w = World::new(d.build(), rules, 7, (1, 2));
        let n = if d.big() { MAX_FIGHTERS } else { rules.seats as usize };
        let mut bots = [Bot::new(7, 0, 0.5); MAX_FIGHTERS];
        for (s, b) in bots.iter_mut().enumerate().take(n) {
            w.seat(s, (s & 1) as u8);
            *b = Bot::new(7, s, 0.3 + 0.2 * (s % 4) as f32);
        }
        let mut taken = 0;
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        let minutes = if d.big() { 1 } else { 10 };
        for _ in 0..ticks(minutes * 60_000) {
            for s in 0..n {
                inputs[s] = bots[s].think(&w, s);
            }
            w.step(&inputs, &NO_LAG);
            taken += w
                .events
                .as_slice()
                .iter()
                .filter(|e| e.kind == Ev::Flag && e.part == 0)
                .count();
        }
        if !d.big() {
            assert!(taken > 0, "{}: bots never took a flag in {minutes} minutes", d.name);
        }
        // And the parity gate's run has a CTF match here.
        assert_ne!(bot_match_on(d.id, 3, 1, 600), 0);
    }
}

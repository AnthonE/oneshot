//! Siege as a player meets it (docs/DESIGN.md §18): an enemy standing in a
//! generator's ring overloads it, faster with more of them; a defender in
//! it holds it, defenders alone wind it back; a broken generator stays
//! broken and scores; the core's shield is up till every generator of its
//! side is broken, and breaking the core wins; kills score nobody; the
//! clock run out is sudden death with every core open; towers that fire
//! needles at enemies near, minions first; minion waves that walk to the
//! enemy's generator, count in its ring and die to a rifle; and bots that
//! play it on Dry Fork.

use sim::bot::{bot_match_on, Bot};
use sim::consts::*;
use sim::events::Cause;
use sim::events::Kind as Ev;
use sim::geom::V3;
use sim::input::*;
use sim::map::Map;
use sim::world::{Phase, Rules, World, NODE_CONTESTED, NODE_FALLING, NODE_RISING, NO_WINNER};

const NO_LAG: [u32; MAX_FIGHTERS] = [0; MAX_FIGHTERS];
/// Red's generators along z at x = -30, its core at -40; blue's mirrored.
const CORE: usize = SIEGE_GENS;
const BLUE_CORE: usize = 2 * SIEGE_GENS + 1;
const AWAY: V3 = V3::new(0.0, 0.0, -45.0);

fn nodes() -> [V3; SIEGE_NODES] {
    core::array::from_fn(|n| {
        let k = n % (SIEGE_GENS + 1);
        let x = if k == SIEGE_GENS { 40.0 } else { 30.0 };
        let z = if k == SIEGE_GENS { 0.0 } else { (k as f32 - 1.0) * 15.0 };
        V3::new(if n <= SIEGE_GENS { -x } else { x }, 0.0, z)
    })
}

/// The open test map with Siege's nodes, two a side, countdown over,
/// everyone away from every ring.
fn world_with(rules: Rules) -> World {
    let mut map = Map::open();
    map.siege = Some(nodes());
    let mut w = World::new(map, rules, 1, (5, 6));
    for s in 0..4 {
        w.seat(s, (s & 1) as u8);
    }
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &NO_LAG);
    }
    for s in 0..4 {
        w.fighters[s].body.pos = AWAY + V3::new(5.0 * s as f32, 0.0, 0.0);
    }
    w
}

/// [`world_with`], its towers and minion waves held off: the rings alone.
fn quiet_with(rules: Rules) -> World {
    let mut w = world_with(rules);
    w.wave_in = u32::MAX;
    w
}

fn world() -> World {
    quiet_with(Rules::siege())
}

/// One idle tick: Siege's events, as (part, a, b). The towers held off.
fn idle(w: &mut World) -> Vec<(u8, u8, u8)> {
    for n in w.nodes.iter_mut() {
        n.cool = u16::MAX;
    }
    w.step(&[Input::default(); MAX_FIGHTERS], &NO_LAG);
    w.events
        .as_slice()
        .iter()
        .filter(|e| e.kind == Ev::Node)
        .map(|e| (e.part, e.a, e.b))
        .collect()
}

/// Ticks until node `n` breaks (at most `limit`), and the events then.
fn until_broken(w: &mut World, n: usize, limit: u32) -> (u32, Vec<(u8, u8, u8)>) {
    for t in 1..=limit {
        let e = idle(w);
        if w.nodes[n].broken {
            return (t, e);
        }
    }
    panic!("node {n} still standing after {limit} ticks: {:?}", w.nodes[n]);
}

#[test]
fn an_enemy_in_the_ring_overloads_a_generator_faster_with_more() {
    let mut w = world();
    let at = nodes();
    // Blue's seat 1 alone at red's generator 0.
    w.fighters[1].body.pos = at[0] + V3::new(2.0, 0.0, 0.0);
    let full = ticks(GEN_OVERLOAD_MS);
    let (t, e) = until_broken(&mut w, 0, full + 2);
    assert!(t.abs_diff(full) <= 1, "one alone: {t} ticks of {full}");
    assert_eq!(e, vec![(0, 1, 0)], "broken by seat 1");
    assert_eq!(w.score[1], 1, "a generator scores");
    // Broken stays broken, whoever stands there.
    w.fighters[0].body.pos = at[0];
    for _ in 0..10 {
        idle(&mut w);
    }
    assert!(w.nodes[0].broken);

    // Two of them: twice the pace. Out of the ring, nothing.
    w.fighters[0].body.pos = AWAY;
    w.fighters[1].body.pos = at[1];
    w.fighters[3].body.pos = at[1] + V3::new(0.0, 0.0, 3.0);
    let (t, _) = until_broken(&mut w, 1, full);
    assert!(t.abs_diff(full.div_ceil(2)) <= 1, "two at it: {t} ticks");
    w.fighters[3].body.pos = at[2] + V3::new(NODE_REACH + 0.1, 0.0, 0.0);
    for _ in 0..10 {
        idle(&mut w);
    }
    assert_eq!(w.nodes[2].load, 0, "just outside the ring");
}

#[test]
fn a_defender_holds_it_and_defenders_alone_wind_it_back() {
    let mut w = world();
    let at = nodes();
    w.fighters[1].body.pos = at[0];
    for _ in 0..100 {
        idle(&mut w);
    }
    assert_eq!(w.nodes[0].load, 100);
    assert_eq!(w.nodes[0].held, NODE_RISING);
    // Red's seat 0 comes in: held.
    w.fighters[0].body.pos = at[0] + V3::new(1.0, 0.0, 0.0);
    for _ in 0..50 {
        idle(&mut w);
    }
    assert_eq!(w.nodes[0].load, 100);
    assert_eq!(w.nodes[0].held, NODE_CONTESTED);
    // Blue leaves: red winds it back, then it is quiet.
    w.fighters[1].body.pos = AWAY;
    for _ in 0..40 {
        idle(&mut w);
    }
    assert_eq!(w.nodes[0].load, 60);
    assert_eq!(w.nodes[0].held, NODE_FALLING);
    // Red leaves too: it stays where it was.
    w.fighters[0].body.pos = AWAY;
    for _ in 0..40 {
        idle(&mut w);
    }
    assert_eq!(w.nodes[0].load, 60);
    // The dead hold nothing.
    w.fighters[1].body.pos = at[0];
    w.fighters[0].body.pos = at[0];
    w.fighters[0].alive = false;
    w.fighters[0].respawn = 60_000;
    idle(&mut w);
    assert_eq!(w.nodes[0].held, NODE_RISING);
}

#[test]
fn the_core_opens_when_every_generator_is_down_and_breaking_it_wins() {
    let mut w = world();
    let at = nodes();
    // The core with its shield up: nothing.
    w.fighters[1].body.pos = at[CORE];
    for _ in 0..100 {
        idle(&mut w);
    }
    assert_eq!(w.nodes[CORE].load, 0, "shield up");
    let mut saw = Vec::new();
    for (g, &gen) in at.iter().enumerate().take(SIEGE_GENS) {
        w.fighters[1].body.pos = gen;
        let (_, e) = until_broken(&mut w, g, ticks(GEN_OVERLOAD_MS) + 2);
        saw.extend(e);
    }
    assert!(saw.contains(&(1, NO_WINNER, CORE as u8)), "the shield fell: {saw:?}");
    assert!(w.core_open(0) && !w.core_open(1));
    assert_eq!(w.phase, Phase::Playing, "generators never end it");
    w.fighters[1].body.pos = at[CORE];
    let (t, e) = until_broken(&mut w, CORE, ticks(CORE_OVERLOAD_MS) + 2);
    assert!(t.abs_diff(ticks(CORE_OVERLOAD_MS)) <= 1);
    assert_eq!(e, vec![(0, 1, CORE as u8)]);
    assert_eq!(w.phase, Phase::Over);
    assert_eq!(w.winner, 1);
    assert_eq!(w.score[1], SIEGE_GENS as i32 + 1);
}

#[test]
fn kills_score_nobody() {
    let mut w = world();
    w.fighters[0].body.pos = V3::new(0.0, 0.0, 0.0);
    w.fighters[1].body.pos = V3::new(3.0, 0.0, 0.0);
    w.fighters[0].yaw = 0;
    w.fighters[1].shield = 0;
    w.fighters[1].health = 1;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0].yaw = 0;
    inputs[0].buttons = FIRE;
    for _ in 0..TICK_HZ {
        w.step(&inputs, &NO_LAG);
    }
    assert!(w.fighters[0].kills > 0, "the kill counted for the fighter");
    assert_eq!(w.score[0], 0, "but not for the side");
}

#[test]
fn time_runs_out_and_it_is_sudden_death_with_every_core_open() {
    let rules = Rules {
        time_limit_ms: 2_000,
        ..Rules::siege()
    };
    let mut w = quiet_with(rules);
    let at = nodes();
    w.fighters[1].body.pos = at[0];
    let mut saw = Vec::new();
    for _ in 0..ticks(2_000) + 2 {
        saw.extend(idle(&mut w));
    }
    assert!(saw.contains(&(2, NO_WINNER, 0)), "sudden death: {saw:?}");
    assert_eq!(w.phase, Phase::Playing);
    assert!(w.core_open(0) && w.core_open(1));
    // Red goes straight for blue's core, generators standing.
    w.fighters[0].body.pos = at[BLUE_CORE];
    w.fighters[1].body.pos = AWAY;
    let (_, e) = until_broken(&mut w, BLUE_CORE, ticks(CORE_OVERLOAD_MS) + 2);
    assert_eq!(e, vec![(0, 0, BLUE_CORE as u8)]);
    assert_eq!(w.winner, 0);

    // Nobody settles it: over at the end of sudden death, the side that
    // broke more ahead. Blue loaded red's generator a little; nobody broke
    // one: a draw.
    let mut w = quiet_with(rules);
    for _ in 0..ticks(2_000) + ticks(SIEGE_SUDDEN_DEATH_MS) + 2 {
        idle(&mut w);
    }
    assert_eq!(w.phase, Phase::Over);
    assert_eq!(w.winner, NO_WINNER);
}

#[test]
fn a_tower_fires_needles_at_the_nearest_enemy_minions_first_and_kills_are_nobodys() {
    let mut w = quiet_with(Rules::siege());
    let at = nodes();
    // Blue's seat 1 by red's generator 0, in reach: needles, a hit each.
    w.fighters[1].body.pos = at[0] + V3::new(6.0, 0.0, 0.0);
    let mut shots = 0;
    for _ in 0..ticks(1_000) {
        w.step(&[Input::default(); MAX_FIGHTERS], &NO_LAG);
        shots += w
            .events
            .as_slice()
            .iter()
            .filter(|e| e.kind == Ev::Tower && e.a == 1 && e.part == 0)
            .count();
    }
    let every = ticks(TOWER_EVERY_MS) + 1;
    assert!(shots as u32 >= ticks(1_000) / every - 1, "{shots} needles in a second");
    let f = &w.fighters[1];
    assert!(f.shield + f.health < 2 * HEALTH_MAX, "it hurts");
    // Out of reach, nothing; red's own, nothing.
    w.fighters[1].body.pos = at[0] + V3::new(TOWER_RANGE + 2.0, 0.0, 0.0);
    w.fighters[0].body.pos = at[0] + V3::new(3.0, 0.0, 0.0);
    w.step(&[Input::default(); MAX_FIGHTERS], &NO_LAG);
    for _ in 0..ticks(1_000) {
        w.step(&[Input::default(); MAX_FIGHTERS], &NO_LAG);
        assert!(
            !w.events.as_slice().iter().any(|e| e.kind == Ev::Tower),
            "nobody in reach"
        );
    }
    // Left in it, they die: nobody's kill, nobody's point.
    w.fighters[1].body.pos = at[0] + V3::new(6.0, 0.0, 0.0);
    let mut killed = None;
    for _ in 0..ticks(30_000) {
        w.step(&[Input::default(); MAX_FIGHTERS], &NO_LAG);
        if let Some(e) = w.events.as_slice().iter().find(|e| e.kind == Ev::Kill) {
            killed = Some(*e);
            break;
        }
    }
    let e = killed.expect("the tower killed them");
    assert_eq!((e.a, e.b, e.c), (1, 1, Cause::Tower as u16));
    assert_eq!(w.score, [0; MAX_FIGHTERS]);
    assert_eq!(w.fighters[0].kills, 0);
}

#[test]
fn waves_come_out_walk_to_the_enemy_generator_count_in_its_ring_and_die_to_a_rifle() {
    let mut w = world_with(Rules::siege());
    let at = nodes();
    // Nobody in the way.
    for s in 0..4 {
        w.fighters[s].body.pos = V3::new(-45.0 + 30.0 * s as f32, 0.0, 45.0);
    }
    let mut out = 0;
    for _ in 0..ticks(MINION_FIRST_MS) + 2 {
        w.step(&[Input::default(); MAX_FIGHTERS], &NO_LAG);
        out += w
            .events
            .as_slice()
            .iter()
            .filter(|e| e.kind == Ev::Minion && e.part == 2)
            .count();
    }
    assert_eq!(out, 2 * MINION_WAVE, "a wave a side");
    assert!(w.minions.iter().filter(|m| m.alive).all(|m| m.hp == MINION_HP));
    // Even waves trade in the middle; with blue's cleared (a player's
    // work), red's walks on to blue's first generator and stands in it.
    for m in w.minions[MINION_SIDE..].iter_mut() {
        m.alive = false;
    }
    w.wave_in = u32::MAX;
    let blue_gen = SIEGE_GENS + 1;
    let mut seen_rising = false;
    for _ in 0..ticks(40_000) {
        w.step(&[Input::default(); MAX_FIGHTERS], &NO_LAG);
        seen_rising |= w.nodes[blue_gen].held == NODE_RISING;
        if w.nodes[blue_gen].broken {
            break;
        }
    }
    assert!(seen_rising, "red's minions stood in blue's ring");
    let red_near = w.minions[..MINION_SIDE]
        .iter()
        .any(|m| m.alive && (m.body.pos - at[blue_gen]).flat().len() < NODE_REACH);
    assert!(red_near || w.nodes[blue_gen].load > 0 || w.nodes[blue_gen].broken);

    // A minion takes two bursts of the rifle: seat 0 (red) at a blue one.
    let mut w = quiet_with(Rules::siege());
    w.wave_in = 1;
    w.step(&[Input::default(); MAX_FIGHTERS], &NO_LAG);
    w.wave_in = u32::MAX;
    let j = MINION_SIDE;
    assert!(w.minions[j].alive && w.minions[j].team == 1);
    let m = w.minions[j].body.pos;
    w.fighters[0].body.pos = m + V3::new(-8.0, 0.0, 0.0);
    w.fighters[0].yaw = 0;
    w.minions[j].body.pos = m;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0].yaw = 0;
    inputs[0].pitch = -1200;
    let mut dead = false;
    for t in 0..ticks(3_000) {
        // Held still, the trigger pulled and let go.
        w.minions[j].body.pos = m;
        inputs[0].buttons = if t % 40 < 2 { FIRE } else { 0 };
        w.step(&inputs, &NO_LAG);
        if w.events
            .as_slice()
            .iter()
            .any(|e| e.kind == Ev::Minion && e.part == 0 && e.a == 0)
        {
            dead = true;
            break;
        }
    }
    assert!(dead, "a rifle killed it: {:?}", w.minions[j]);
    assert_eq!(w.fighters[0].kills, 0, "a minion is no kill");
}

#[test]
fn bots_and_minions_push_on_dry_fork() {
    for d in sim::maps::ALL.iter().filter(|d| d.siege.len() == SIEGE_NODES) {
        let mut w = World::new(d.build(), Rules::siege(), 7, (1, 2));
        let mut bots = [Bot::new(7, 0, 0.5); MAX_FIGHTERS];
        for (s, b) in bots.iter_mut().enumerate() {
            w.seat(s, (s & 1) as u8);
            *b = Bot::new(7, s, SIEGE_BOT_SKILL);
        }
        let (mut needles, mut minions_shot, mut loaded) = (0, 0, false);
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        let minutes = 8;
        for _ in 0..ticks(minutes * 60_000) {
            for s in 0..MAX_FIGHTERS {
                inputs[s] = bots[s].think(&w, s);
            }
            w.step(&inputs, &NO_LAG);
            for e in w.events.as_slice() {
                needles += (e.kind == Ev::Tower) as u32;
                minions_shot += (e.kind == Ev::Minion && e.part <= 1 && e.a != NO_WINNER) as u32;
            }
            loaded |= w.nodes.iter().any(|n| n.load > 0 || n.broken);
        }
        assert!(loaded, "{}: nobody stood in a ring in {minutes} minutes", d.name);
        assert!(needles > 0, "{}: no tower fired", d.name);
        assert!(minions_shot > 0, "{}: nobody shot a minion", d.name);
        // And the parity gate's run has a Siege match here.
        assert_ne!(bot_match_on(d.id, 4, 1, 600), 0);
    }
}

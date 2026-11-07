//! The contact step alone (`sim::contact`, what a client re-runs) moves
//! bodies and hulls exactly as the world's whole tick does, but for what
//! it leaves to the world: blasts, wrecks, a lunge, a hit, getting in or
//! out, a death or a respawn.

use sim::bot::Bot;
use sim::consts::*;
use sim::contact::{Present, Scene, Sticks};
use sim::events::Kind as Ev;
use sim::input::Input;
use sim::world::{Rules, World};

#[test]
fn the_contact_step_alone_matches_the_world() {
    let map = sim::map::by_id(3).expect("Dry Fork");
    let rules = Rules {
        score_limit: 100_000,
        time_limit_ms: 0,
        ..Rules::big_team()
    };
    let n = rules.seats as usize;
    let mut w = World::new(map, rules, 9, (1, 2));
    let mut bots = [Bot::new(9, 0, 0.5); MAX_FIGHTERS];
    for (s, b) in bots.iter_mut().enumerate().take(n) {
        w.seat(s, (s & 1) as u8);
        *b = Bot::new(9, s, 0.3 + 0.2 * (s % 4) as f32);
    }
    let lag = [0u32; MAX_FIGHTERS];
    let spare = w.rules.teams && !w.rules.friendly_fire;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let (mut bodies, mut hulls, mut driven, mut met) = (0usize, 0usize, 0usize, 0usize);
    for _ in 0..40_000 {
        for s in 0..n {
            inputs[s] = bots[s].think(&w, s);
        }
        let playing = w.phase == sim::world::Phase::Playing;
        let before = w.fighters;
        let mut sc = Scene::take(&w.fighters, &w.vehicles, spare);
        let ran: [Input; MAX_FIGHTERS] = core::array::from_fn(|s| inputs[s].sanitized());
        // Each hull's stick as its driver's own tick will leave it.
        let sticks: Sticks = core::array::from_fn(|i| {
            let v = &w.vehicles[i];
            let d = v.driver().filter(|&d| v.active && w.fighters[d].active())?;
            Some((ran[d].mx, ran[d].mz))
        });
        sc.walk(&ran, u16::MAX, &w.map);
        sc.drive(&sticks, &w.map);
        let _ = sc.crash(&w.map);
        sc.seat();
        sc.run_down(&mut Present);
        if SOLID_HULLS {
            sc.push_out(&w.map);
        }
        w.step(&inputs, &lag);
        if !playing || w.phase != sim::world::Phase::Playing {
            continue;
        }
        let ev = w.events.as_slice();
        // What only the world does to a body or a hull this tick.
        if ev
            .iter()
            .any(|e| matches!(e.kind, Ev::Blast | Ev::Wreck | Ev::VehicleHit))
        {
            continue;
        }
        let named = |q: usize| ev.iter().any(|e| e.a as usize == q || e.b as usize == q);
        for (q, (b, a)) in before.iter().zip(&w.fighters).enumerate().take(n) {
            if named(q) || b.lunge != 0 || a.lunge != 0 || b.ride != a.ride || b.active() != a.active() {
                continue;
            }
            assert_eq!(sc.men[q].body, a.body, "tick {}: seat {q}'s body", w.tick);
            bodies += 1;
        }
        for (i, ((h, v), st)) in sc.hulls.iter().zip(w.vehicles.iter()).zip(&sticks).enumerate() {
            if !v.active || !h.active {
                continue;
            }
            assert_eq!(
                (h.pos, h.vel, h.yaw, h.grounded),
                (v.pos, v.vel, v.yaw, v.grounded),
                "tick {}: hull {i}",
                w.tick
            );
            hulls += 1;
            driven += st.is_some() as usize;
        }
        met += w
            .vehicles
            .iter()
            .filter(|v| v.active && v.vel.flat().len() > SPLAT_SPEED)
            .count();
    }
    // The gate only means something if bots drove and walked a lot.
    assert!(bodies > 500_000, "{bodies} bodies");
    assert!(hulls > 140_000 && driven > 5_000, "{hulls} hulls, {driven} driven");
    assert!(met > 5_000, "{met} hull-ticks fast enough to run someone down");
}

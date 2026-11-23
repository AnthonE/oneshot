//! Siege's cards as a player meets them (docs/DESIGN.md §18, phase 1):
//! everyone is offered two cards and may take one, keyed by the salt; a
//! card is held in one of two hands and played on its button; the grapple
//! pulls its fighter to where its hook bit (predicted bit for bit); the
//! rewind takes it back five seconds with the shields it had; the frost
//! bird goes where its holder looks and never kills from full alone; none
//! of it outside Siege; and Siege's respawns grow late in the match.

use sim::card::{BIRD, GRAPPLE, NONE, REWIND};
use sim::consts::*;
use sim::geom::{rad_to_yaw, V3};
use sim::input::*;
use sim::map::Map;
use sim::world::{Phase, Rules, World};

const NO_LAG: [u32; MAX_FIGHTERS] = [0; MAX_FIGHTERS];

/// The open test map with Siege's nodes, four seated (0 and 2 red, 1 and 3
/// blue), countdown over, its minions held off; everyone far from every
/// ring, apart.
fn world_with(rules: Rules, salt: (u64, u64)) -> World {
    let mut map = Map::open();
    map.siege = Some(core::array::from_fn(|n| {
        let k = n % (SIEGE_GENS + 1);
        let x = if k == SIEGE_GENS { 40.0 } else { 30.0 };
        let z = (k as f32 - 1.0) * 15.0;
        V3::new(if n <= SIEGE_GENS { -x } else { x }, 0.0, z)
    }));
    let mut w = World::new(map, rules, 1, salt);
    for s in 0..4 {
        w.seat(s, (s & 1) as u8);
    }
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &NO_LAG);
    }
    w.wave_in = u32::MAX;
    for s in 0..4 {
        w.fighters[s].body.pos = V3::new(-10.0 + 6.0 * s as f32, 0.0, -42.0);
    }
    w
}

fn world() -> World {
    world_with(Rules::siege(), (5, 6))
}

fn press(w: &mut World, s: usize, inp: Input) {
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[s] = inp;
    w.step(&inputs, &NO_LAG);
}

fn idle(w: &mut World) {
    w.step(&[Input::default(); MAX_FIGHTERS], &NO_LAG);
}

/// The yaw and pitch from `from` to `to`.
fn aim(from: V3, to: V3) -> (u16, i16) {
    let d = to - from;
    let pitch = sim::fmath::atan2(d.y, d.flat().len()) * (65536.0 / core::f32::consts::TAU);
    (rad_to_yaw(sim::fmath::atan2(d.z, d.x)), pitch as i16)
}

#[test]
fn everyone_is_offered_two_cards_and_may_take_one() {
    let mut w = world();
    idle(&mut w);
    for s in 0..4 {
        let f = &w.fighters[s];
        assert_eq!(f.offer_left as u32, ticks(OFFER_MS), "seat {s} has an offer");
        assert!(
            f.offer[0] != NONE && f.offer[1] != NONE && f.offer[0] != f.offer[1],
            "{:?}",
            f.offer
        );
        assert_eq!(f.cards, [NONE; 2]);
    }
    // Seat 0 takes the second; seat 1 lets its offer go by.
    let second = w.fighters[0].offer[1];
    press(
        &mut w,
        0,
        Input {
            buttons: PICK_B,
            ..Input::default()
        },
    );
    assert_eq!(w.fighters[0].cards, [second, NONE], "taken into the first hand");
    assert_eq!(w.fighters[0].offer_left, 0, "the offer closed");
    for _ in 0..ticks(OFFER_MS) {
        idle(&mut w);
    }
    assert_eq!(w.fighters[1].offer_left, 0, "it ran out");
    assert_eq!(w.fighters[1].cards, [NONE; 2], "and nothing was taken");

    // Dealt by the salt: another salt, other deals (for some seat).
    let deals = |salt| {
        let mut w = world_with(Rules::siege(), salt);
        idle(&mut w);
        (0..4).map(|s| w.fighters[s].offer).collect::<Vec<_>>()
    };
    assert_eq!(deals((5, 6)), deals((5, 6)), "the same salt, the same deals");
    assert!(
        (1..20).any(|k| deals((5 + k, 6)) != deals((5, 6))),
        "the salt keys the deal"
    );
}

#[test]
fn a_third_card_takes_the_place_of_the_one_held_longest() {
    let mut w = world();
    idle(&mut w);
    w.fighters[0].cards = [GRAPPLE, REWIND];
    w.fighters[0].offer = [BIRD, GRAPPLE];
    press(
        &mut w,
        0,
        Input {
            buttons: PICK_A,
            ..Input::default()
        },
    );
    assert_eq!(w.fighters[0].cards, [REWIND, BIRD]);
}

#[test]
fn no_cards_outside_siege() {
    let mut w = world_with(Rules::big_team(), (5, 6));
    for _ in 0..TICK_HZ {
        idle(&mut w);
    }
    assert!((0..4).all(|s| !w.fighters[s].has_cards()), "no offer in Big Team");
}

/// The grapple's hook goes along the aim and bites the wall; the fighter
/// is pulled to it, up off the floor, and let go there; its client,
/// stepping only itself on the map, keeps up through all of it. A hook at
/// the sky misses and waits a moment only.
#[test]
fn the_grapple_pulls_to_where_it_bit_and_is_predicted() {
    let mut w = world();
    idle(&mut w);
    w.fighters[0].offer_left = 0;
    w.fighters[0].cards = [GRAPPLE, NONE];
    w.fighters[0].body.pos = V3::new(25.0, 0.0, 0.0);
    // At the east wall (its face at x = 50), 5 m up it.
    let (yaw, pitch) = aim(w.fighters[0].eye(), V3::new(50.0, 5.0, 0.0));
    let map = w.map.clone();
    let mut mine = w.fighters[0];
    let fire = Input {
        buttons: CARD_A,
        yaw,
        pitch,
        ..Input::default()
    };
    let hold = Input {
        yaw,
        pitch,
        ..Input::default()
    };
    let mut pulled = 0;
    for t in 0..TICK_HZ * 3 {
        let inp = if t == 0 { fire } else { hold };
        press(&mut w, 0, inp);
        mine.step_local(&inp.sanitized(), &map);
        // The world's to set; the rest is the fighter's own tick.
        mine.offer = w.fighters[0].offer;
        mine.offer_left = w.fighters[0].offer_left;
        assert_eq!(mine.body, w.fighters[0].body, "tick {t}");
        assert_eq!(
            (mine.hook, mine.hooked),
            (w.fighters[0].hook, w.fighters[0].hooked),
            "tick {t}"
        );
        if t == 0 {
            let f = &w.fighters[0];
            assert!(
                f.hooked > 0 && sim::fmath::fabs(f.hook.x - 50.0) < 0.01,
                "bit the wall: {:?}",
                f.hook
            );
            assert_eq!(f.card_cool[0] as u32, ticks(GRAPPLE_COOL_MS));
        }
        pulled += (w.fighters[0].hooked > 0) as u32;
    }
    let f = &w.fighters[0];
    assert!(f.body.pos.x > 46.0, "pulled to the wall: {:?}", f.body.pos);
    assert!(
        pulled > 30 && pulled < ticks(GRAPPLE_MS),
        "let go there, after {pulled} ticks"
    );

    // At the sky: a miss.
    let mut w = world();
    idle(&mut w);
    w.fighters[0].cards = [NONE, GRAPPLE];
    press(
        &mut w,
        0,
        Input {
            buttons: CARD_B,
            pitch: PITCH_MAX,
            ..Input::default()
        },
    );
    assert_eq!(w.fighters[0].hooked, 0);
    assert_eq!(w.fighters[0].card_cool[1] as u32, ticks(GRAPPLE_MISS_COOL_MS));
}

/// The rewind: back where it stood five seconds ago, with the shields and
/// health it had then (more than now), and cooling.
#[test]
fn the_rewind_takes_it_back_five_seconds_with_the_shields_it_had() {
    let mut w = world();
    idle(&mut w);
    w.fighters[0].cards = [NONE, REWIND];
    let run = Input {
        mz: 127,
        yaw: rad_to_yaw(core::f32::consts::FRAC_PI_2),
        ..Input::default()
    };
    let mut was = Vec::new();
    for _ in 0..ticks(7000) {
        press(&mut w, 0, run);
        was.push(w.fighters[0].body.pos);
    }
    let now = w.fighters[0].body.pos;
    w.fighters[0].shield = 0;
    w.fighters[0].health = 30 * PT;
    press(&mut w, 0, Input { buttons: CARD_B, ..run });
    let f = &w.fighters[0];
    let then = was[was.len() - ticks(REWIND_MS) as usize];
    assert!(
        (f.body.pos - then).flat().len() < RUN * REWIND_EVERY_MS as f32 / 1000.0 + 0.5,
        "back at {:?}, it stood at {then:?} (now {now:?})",
        f.body.pos
    );
    assert_eq!((f.shield, f.health), (SHIELD_MAX, HEALTH_MAX), "as it was then");
    assert_eq!(f.card_cool[1] as u32, ticks(REWIND_COOL_MS));
}

/// The frost bird goes along its holder's aim and turns where it turns;
/// it bursts on the enemy it was steered into, stripping its shields and
/// biting, but leaves it standing from full.
#[test]
fn the_frost_bird_goes_where_its_holder_looks_and_never_kills_from_full() {
    let mut w = world();
    idle(&mut w);
    w.fighters[0].cards = [BIRD, NONE];
    w.fighters[0].body.pos = V3::new(0.0, 0.0, 0.0);
    // Its target 25 m off, out of the first line.
    w.fighters[1].body.pos = V3::new(24.0, 0.0, 8.0);
    let ahead = Input {
        yaw: 0,
        ..Input::default()
    };
    press(
        &mut w,
        0,
        Input {
            buttons: CARD_A,
            ..ahead
        },
    );
    assert!(
        w.proj.iter().any(|p| p.active && p.kind == sim::world::ProjKind::Bird),
        "it flies"
    );
    assert_eq!(w.fighters[0].card_cool[0] as u32, ticks(BIRD_COOL_MS));
    for _ in 0..TICK_HZ / 2 {
        press(&mut w, 0, ahead);
    }
    // Then the holder looks at its target, and keeps looking.
    let mut hit = false;
    for _ in 0..ticks(BIRD_MS) {
        let bird = w
            .proj
            .iter()
            .find(|p| p.active && p.kind == sim::world::ProjKind::Bird)
            .copied();
        let Some(_) = bird else {
            hit = true;
            break;
        };
        let (yaw, pitch) = aim(w.fighters[0].eye(), w.fighters[1].body.pos + V3::new(0.0, 0.9, 0.0));
        press(
            &mut w,
            0,
            Input {
                yaw,
                pitch,
                ..Input::default()
            },
        );
    }
    assert!(hit, "it burst");
    let t = &w.fighters[1];
    assert!(t.alive, "never a kill from full");
    assert_eq!(t.shield, 0, "its shields stripped");
    assert!(t.health < HEALTH_MAX, "and bitten: {}", t.health);
}

#[test]
fn siege_respawns_grow_late_and_longest_in_sudden_death() {
    let mut w = world();
    assert_eq!(w.respawn_ticks() as u32, ticks(RESPAWN_MS), "the mode's at first");
    w.clock -= ticks(3 * SIEGE_RESPAWN_STEP_MS);
    assert_eq!(
        w.respawn_ticks() as u32,
        ticks(RESPAWN_MS + 3000),
        "a second more a step"
    );
    w.overtime = 1;
    assert_eq!(w.respawn_ticks() as u32, ticks(SIEGE_RESPAWN_MAX_MS));
    let b = world_with(Rules::big_team(), (5, 6));
    assert_eq!(b.respawn_ticks() as u32, ticks(RESPAWN_MS), "Big Team's never grows");
}

/// Bots take the cards they are offered and play them, in a Siege match
/// of bots on Dry Fork.
#[test]
fn bots_take_and_play_cards() {
    let d = &sim::maps::canyon::DEF;
    let mut w = World::new(d.build(), Rules::siege(), 3, (1, 2));
    let mut bots = [sim::bot::Bot::new(3, 0, 0.5); MAX_FIGHTERS];
    for (s, b) in bots.iter_mut().enumerate() {
        w.seat(s, (s & 1) as u8);
        *b = sim::bot::Bot::new(3, s, SIEGE_BOT_SKILL);
    }
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let (mut held, mut played) = (0, [0u32; 4]);
    for _ in 0..ticks(240_000) {
        for s in 0..MAX_FIGHTERS {
            inputs[s] = bots[s].think(&w, s);
        }
        let before: Vec<[u16; 2]> = w.fighters.iter().map(|f| f.card_cool).collect();
        w.step(&inputs, &NO_LAG);
        for (s, f) in w.fighters.iter().enumerate() {
            for k in 0..2 {
                if f.card_cool[k] > before[s][k] + 1 {
                    played[f.cards[k] as usize] += 1;
                }
            }
        }
        held = held.max(w.fighters.iter().filter(|f| f.cards != [NONE; 2]).count());
    }
    assert!(held >= MAX_FIGHTERS / 2, "only {held} took a card");
    for c in [GRAPPLE, REWIND, BIRD] {
        assert!(
            played[c as usize] > 0,
            "nobody played {}: {played:?}",
            sim::card::name(c)
        );
    }
}

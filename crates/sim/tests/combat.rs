//! The rules as a player meets them: guns, shields, heads, lag compensation,
//! blasts, pads, the score, and the prediction matching the server.

use sim::arms::{AR, LADDER, MAGNUM, NONE, ROCKET, SHOTGUN, SNIPER};
use sim::consts::*;
use sim::events::Kind as Ev;
use sim::fmath;
use sim::geom::{rad_to_yaw, V3};
use sim::input::*;
use sim::world::{Phase, Rules, World};
use sim::Map;

const NO_LAG: [u32; MAX_FIGHTERS] = [0; MAX_FIGHTERS];

/// Two fighters on the open map, countdown over, shooter at the origin
/// facing +x, target `dist` m down the x axis.
fn duel(loadout: [u8; 2], dist: f32) -> World {
    duel_salted(loadout, dist, (0xdead, 0xbeef))
}

fn duel_salted(loadout: [u8; 2], dist: f32, salt: (u64, u64)) -> World {
    let rules = Rules {
        teams: false,
        score_limit: 100,
        time_limit_ms: 0,
        loadout,
        ..Rules::duel()
    };
    let mut w = World::new(Map::open(), rules, 1, salt);
    w.seat(0, 0);
    w.seat(1, 1);
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &NO_LAG);
    }
    w.fighters[0].body.pos = V3::ZERO;
    w.fighters[1].body.pos = V3::new(dist, 0.0, 0.0);
    w
}

/// The yaw and pitch from the shooter's eye to a point.
fn aim(w: &World, s: usize, at: V3) -> (u16, i16) {
    let d = at - w.fighters[s].eye();
    let yaw = rad_to_yaw(fmath::atan2(d.z, d.x));
    let flat = (d.x * d.x + d.z * d.z).sqrt();
    let pitch = fmath::round(fmath::atan2(d.y, flat) * (65536.0 / core::f32::consts::TAU)) as i16;
    (yaw, pitch)
}

fn shoot(w: &mut World, at: V3, buttons: u16, lag: [u32; MAX_FIGHTERS]) {
    let (yaw, pitch) = aim(w, 0, at);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0] = Input {
        buttons,
        yaw,
        pitch,
        ..Input::default()
    };
    w.step(&inputs, &lag);
}

fn chest(w: &World, s: usize) -> V3 {
    w.fighters[s].body.pos + V3::new(0.0, 1.1, 0.0)
}

fn head(w: &World, s: usize) -> V3 {
    w.fighters[s].body.pos + V3::new(0.0, HEAD_Y, 0.0)
}

fn kills(w: &World) -> usize {
    w.events.as_slice().iter().filter(|e| e.kind == Ev::Kill).count()
}

#[test]
fn an_assault_rifle_kills_through_shields_and_the_dead_come_back() {
    let mut w = duel([AR, MAGNUM], 10.0);
    let mut dead_at = None;
    for t in 0..TICK_HZ * 4 {
        let c = chest(&w, 1);
        shoot(&mut w, c, FIRE, NO_LAG);
        if kills(&w) > 0 {
            dead_at = Some(t);
            break;
        }
    }
    let t = dead_at.expect("the rifle never killed");
    // 200 points at 8 a round, 0.088 s apart: about 2.2 s if every round lands.
    assert!(t > TICK_HZ * 2 && t < TICK_HZ * 3, "killed at tick {t}");
    assert_eq!(w.fighters[0].kills, 1);
    assert_eq!(w.score[0], 1);
    assert!(!w.fighters[1].alive);
    let idle = [Input::default(); MAX_FIGHTERS];
    for _ in 0..ticks(RESPAWN_MS) {
        w.step(&idle, &NO_LAG);
    }
    let v = &w.fighters[1];
    assert!(v.alive && v.shield == SHIELD_MAX && v.health == HEALTH_MAX);
    assert_eq!(v.body.pos.x.max(-v.body.pos.x), 40.0, "back at a spawn point");
}

/// Fires the semi-automatic in hand at the target's head once.
fn tap_head(w: &mut World) {
    let h = head(w, 1);
    shoot(w, h, FIRE, NO_LAG);
    for _ in 0..TICK_HZ / 2 {
        let h = head(w, 1);
        shoot(w, h, 0, NO_LAG);
    }
}

#[test]
fn a_magnum_head_shot_kills_only_once_the_shields_are_down() {
    let mut w = duel([MAGNUM, AR], 15.0);
    // 28 a shot: 100 → 72 → 44 → 16, then 16 soaked and 12 to health.
    for _ in 0..4 {
        tap_head(&mut w);
        assert!(w.fighters[1].alive);
    }
    assert_eq!(w.fighters[1].shield, 0);
    tap_head(&mut w);
    assert!(!w.fighters[1].alive, "the fifth, unshielded, kills");
}

#[test]
fn a_sniper_head_shot_always_kills() {
    let mut w = duel([SNIPER, MAGNUM], 60.0);
    tap_head(&mut w);
    assert!(!w.fighters[1].alive);
}

/// A target running across the shooter's view; the shooter aims where it
/// stood 12 ticks ago. The server's rewind decides whether that lands.
fn lagged_shot(depth_q8: u32) -> bool {
    let mut w = duel([SNIPER, MAGNUM], 30.0);
    let run = Input {
        mz: 127,
        yaw: 16384,
        ..Input::default()
    };
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[1] = run;
    for _ in 0..TICK_HZ {
        w.step(&inputs, &NO_LAG);
    }
    let then = chest(&w, 1);
    for _ in 0..11 {
        w.step(&inputs, &NO_LAG);
    }
    let (yaw, pitch) = aim(&w, 0, then);
    inputs[0] = Input {
        buttons: FIRE,
        yaw,
        pitch,
        ..Input::default()
    };
    let mut lag = NO_LAG;
    lag[0] = depth_q8;
    w.step(&inputs, &lag);
    w.events.as_slice().iter().any(|e| e.kind == Ev::Hit && e.b == 1)
}

#[test]
fn lag_compensation_lands_only_as_far_back_as_the_server_says() {
    assert!(lagged_shot(12 << 8), "rewound 12 ticks, the shot lands");
    assert!(!lagged_shot(0), "not rewound, it misses");
    assert!(!lagged_shot(4 << 8), "rewound too little, it misses");
}

#[test]
fn a_rewind_deeper_than_the_ring_is_clamped() {
    let mut w = duel([SNIPER, MAGNUM], 30.0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[1] = Input {
        mz: 127,
        yaw: 16384,
        ..Input::default()
    };
    for _ in 0..TICK_HZ {
        w.step(&inputs, &NO_LAG);
    }
    let then = chest(&w, 1);
    for _ in 0..REWIND_TICKS + 8 {
        w.step(&inputs, &NO_LAG);
    }
    let (yaw, pitch) = aim(&w, 0, then);
    inputs[0] = Input {
        buttons: FIRE,
        yaw,
        pitch,
        ..Input::default()
    };
    let mut lag = NO_LAG;
    lag[0] = u32::MAX;
    w.step(&inputs, &lag);
    assert!(!w.events.as_slice().iter().any(|e| e.kind == Ev::Hit));
}

#[test]
fn shields_come_back_after_five_seconds() {
    let mut w = duel([MAGNUM, AR], 15.0);
    let c = chest(&w, 1);
    shoot(&mut w, c, FIRE, NO_LAG);
    assert_eq!(w.fighters[1].shield, SHIELD_MAX - 28 * PT);
    let idle = [Input::default(); MAX_FIGHTERS];
    for _ in 0..ticks(SHIELD_DELAY_MS) - 2 {
        w.step(&idle, &NO_LAG);
    }
    assert_eq!(w.fighters[1].shield, SHIELD_MAX - 28 * PT, "not before the delay");
    for _ in 0..TICK_HZ {
        w.step(&idle, &NO_LAG);
    }
    assert_eq!(w.fighters[1].shield, SHIELD_MAX);
}

#[test]
fn a_rocket_kills_two_and_a_rocket_jump_costs_half() {
    let mut w = duel([ROCKET, MAGNUM], 12.0);
    w.seat(2, 2);
    w.fighters[2].body.pos = V3::new(12.0, 0.0, 0.8);
    let c = chest(&w, 1);
    for _ in 0..TICK_HZ {
        shoot(&mut w, c, FIRE, NO_LAG);
        if !w.fighters[1].alive {
            break;
        }
    }
    assert!(!w.fighters[1].alive && !w.fighters[2].alive, "both in the blast");
    assert_eq!(w.fighters[0].kills, 2);

    let mut w = duel([ROCKET, MAGNUM], 40.0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0] = Input {
        buttons: FIRE | JUMP,
        pitch: -PITCH_MAX,
        ..Input::default()
    };
    let mut up = 0.0f32;
    for _ in 0..TICK_HZ / 2 {
        w.step(&inputs, &NO_LAG);
        up = up.max(w.fighters[0].body.vel.y);
    }
    let me = &w.fighters[0];
    assert!(me.alive);
    assert_eq!(me.shield, 0, "half of 230 takes the shields");
    assert_eq!(me.health, HEALTH_MAX - 15 * PT);
    assert!(up > JUMP_SPEED, "the blast lifts more than a jump: {up}");
}

#[test]
fn a_frag_goes_off_on_its_fuse() {
    let mut w = duel([AR, MAGNUM], 8.0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0].buttons = GRENADE;
    w.step(&inputs, &NO_LAG);
    assert_eq!(w.fighters[0].grenades, GREN_START - 1);
    inputs[0].buttons = 0;
    let mut at = None;
    for t in 1..TICK_HZ * 3 {
        w.step(&inputs, &NO_LAG);
        if w.events.as_slice().iter().any(|e| e.kind == Ev::Blast) {
            at = Some(t);
            break;
        }
    }
    assert_eq!(at, Some(ticks(GREN_FUSE_MS) - 1));
}

#[test]
fn spread_is_keyed_by_the_salt() {
    let ends = |salt: (u64, u64)| {
        let mut w = duel_salted([AR, MAGNUM], 50.0, salt);
        w.fighters[1].body.pos = V3::new(0.0, 0.0, 45.0);
        let mut v = Vec::new();
        for _ in 0..8 {
            shoot(&mut w, V3::new(50.0, 1.6, 0.0), FIRE, NO_LAG);
            for e in w.events.as_slice() {
                if e.kind == Ev::Shot {
                    v.push((e.end.y.to_bits(), e.end.z.to_bits()));
                }
            }
        }
        v
    };
    assert_eq!(ends((1, 2)), ends((1, 2)));
    assert_ne!(ends((1, 2)), ends((3, 4)));
}

#[test]
fn slayer_ends_at_the_score_limit() {
    let mut w = duel([SNIPER, MAGNUM], 20.0);
    w.rules.score_limit = 1;
    tap_head(&mut w);
    assert_eq!(w.phase, Phase::Over);
    assert_eq!(w.winner, 0);
}

#[test]
fn a_pad_gives_its_gun_and_comes_back() {
    let mut w = duel([AR, MAGNUM], 20.0);
    w.fighters[0].body.pos = V3::new(0.0, 0.0, -30.0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0].buttons = USE;
    w.step(&inputs, &NO_LAG);
    assert_eq!(w.fighters[0].gun().arm, SHOTGUN);
    assert!(w.pads[0].empty_for > 0);
    inputs[0].buttons = 0;
    for _ in 0..ticks(10_000) {
        w.step(&inputs, &NO_LAG);
    }
    assert_eq!(w.pads[0].empty_for, 0);
}

/// The browser predicts its own fighter with `step_local` alone. Through
/// running, jumping, crouching, firing, bursts, reloads, swaps and throws,
/// that must match the server's fighter to the bit, in every field the
/// fighter owns (health and shields are the world's).
#[test]
fn the_prediction_matches_the_server_bit_for_bit() {
    let mut w = duel([sim::arms::BR, MAGNUM], 45.0);
    w.fighters[1].body.pos = V3::new(45.0, 0.0, 45.0);
    let mut mine = w.fighters[0];
    let map = Map::open();
    let mut rng = sim::rng::Pcg32::new(7, 7);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    for t in 0..TICK_HZ * 30 {
        let r = rng.next_u32();
        let inp = Input {
            buttons: (r as u16) & (FIRE | JUMP | CROUCH | RELOAD | SWAP | GRENADE),
            yaw: (t * 97) as u16,
            pitch: 4000,
            mx: (r >> 16) as i8,
            mz: (r >> 24) as i8,
        }
        .sanitized();
        inputs[0] = inp;
        w.step(&inputs, &NO_LAG);
        mine.step_local(&inp, &map);
        let s = &w.fighters[0];
        assert_eq!(s.body, mine.body, "tick {t}");
        assert_eq!(
            (
                s.yaw,
                s.pitch,
                s.guns,
                s.cur,
                s.refire,
                s.reload,
                s.burst_left,
                s.burst_gap
            ),
            (
                mine.yaw,
                mine.pitch,
                mine.guns,
                mine.cur,
                mine.refire,
                mine.reload,
                mine.burst_left,
                mine.burst_gap
            ),
            "tick {t}"
        );
        assert_eq!(
            (s.melee_cool, s.gren_cool, s.grenades, s.shots, s.last_buttons),
            (
                mine.melee_cool,
                mine.gren_cool,
                mine.grenades,
                mine.shots,
                mine.last_buttons
            ),
            "tick {t}"
        );
        if !s.alive {
            break;
        }
    }
    assert!(w.fighters[0].alive, "nothing should have hurt the predicted fighter");
}

#[test]
fn gun_game_moves_the_killer_up_a_rung_and_the_last_rung_wins() {
    let rules = Rules {
        time_limit_ms: 0,
        ..Rules::gun_game()
    };
    let mut w = World::new(Map::open(), rules, 1, (3, 4));
    w.seat(0, 0);
    w.seat(1, 1);
    // Everyone starts on the first rung, with nothing on the back.
    assert_eq!(w.fighters[0].guns.map(|g| g.arm), [LADDER[0], NONE]);
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &NO_LAG);
    }
    w.fighters[0].body.pos = V3::ZERO;
    w.fighters[1].body.pos = V3::new(12.0, 0.0, 0.0);
    for _ in 0..TICK_HZ * 2 {
        let c = chest(&w, 1);
        shoot(&mut w, c, FIRE, NO_LAG);
        if kills(&w) > 0 {
            break;
        }
    }
    assert_eq!(w.score[0], 1);
    assert_eq!(
        w.fighters[0].guns.map(|g| g.arm),
        [LADDER[1], NONE],
        "up a rung, in hand"
    );
    // Pads are off: standing on the shotgun's pad gives nothing.
    w.fighters[0].body.pos = w.map.pads[0].pos;
    let mut use_pad = [Input::default(); MAX_FIGHTERS];
    use_pad[0].buttons = USE;
    w.step(&use_pad, &NO_LAG);
    w.step(&idle, &NO_LAG);
    assert_eq!(w.fighters[0].gun().arm, LADDER[1]);
    // The dead come back on their own rung.
    for _ in 0..ticks(RESPAWN_MS) + 2 {
        w.step(&idle, &NO_LAG);
    }
    assert!(w.fighters[1].alive);
    assert_eq!(w.fighters[1].gun().arm, LADDER[0]);
    // A kill on the last rung wins.
    w.score[0] = rules.score_limit - 1;
    w.fighters[0].guns[0] = sim::fighter::Gun::full(SNIPER);
    w.fighters[0].body.pos = V3::ZERO;
    w.fighters[1].body.pos = V3::new(12.0, 0.0, 0.0);
    w.fighters[0].refire = 0;
    let h = head(&w, 1);
    shoot(&mut w, h, FIRE, NO_LAG);
    assert_eq!(w.phase, Phase::Over);
    assert_eq!(w.winner, 0);
}

#[test]
fn a_seat_taken_in_free_for_all_starts_from_nothing() {
    let mut w = World::new(Map::open(), Rules::ffa(), 1, (3, 4));
    w.seat(2, 0);
    w.score[2] = 9;
    w.seat(2, 0);
    assert_eq!(w.score[2], 0);
}

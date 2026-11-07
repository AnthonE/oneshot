//! The rules as a player meets them: guns, shields, heads, lag compensation,
//! blasts, pads, the score, and the prediction matching the server.

use sim::arms::{AR, BR, LADDER, MAGNUM, NONE, ROCKET, SHOTGUN, SNIPER, SWORD};
use sim::consts::*;
use sim::events::Kind as Ev;
use sim::fighter::Gun;
use sim::fmath;
use sim::geom::{rad_to_yaw, V3};
use sim::input::*;
use sim::world::Dropped;
use sim::world::{Lag, Phase, Rules, World};
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

/// [`lagged_shot`]'s runner, the shooter's depth 12 ticks, seen with
/// `share` of it: aiming where the runner stood then (`then`) or is now.
fn shared_shot(share: u8, then: bool) -> bool {
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
    let was = chest(&w, 1);
    for _ in 0..11 {
        w.step(&inputs, &NO_LAG);
    }
    // Where it stands as the shot is resolved (it runs on a tick first).
    let now = {
        let mut f = w.fighters[1];
        f.step_local(&inputs[1], &w.map);
        f.body.pos + (chest(&w, 1) - w.fighters[1].body.pos)
    };
    let (yaw, pitch) = aim(&w, 0, if then { was } else { now });
    inputs[0] = Input {
        buttons: FIRE,
        yaw,
        pitch,
        ..Input::default()
    };
    let mut lag = Lag::past(NO_LAG);
    lag.depth_q8[0] = 12 << 8;
    lag.seen[0].share[1] = share;
    w.step_with(&inputs, &lag);
    w.events.as_slice().iter().any(|e| e.kind == Ev::Hit && e.b == 1)
}

#[test]
fn a_shot_meets_a_target_where_its_share_of_the_rewind_puts_it() {
    // All of the depth: where it stood 12 ticks ago, as it was drawn.
    assert!(shared_shot(SHARE_FULL, true), "the whole depth: met where it was");
    assert!(!shared_shot(SHARE_FULL, false), "the whole depth: not where it is");
    // None: where it is, as a target drawn on the shooter's present.
    assert!(shared_shot(0, false), "none of it: met where it is");
    assert!(!shared_shot(0, true), "none of it: not where it was");
    // A third of it: 4 ticks back, too little to meet it where it was.
    assert!(!shared_shot(5, true), "a third: not where it was");
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

/// Seat 0 throws a frag at `pitch` (from the open map's origin, facing
/// +x): the tick it first came down, and the tick it went off.
fn frag_at(pitch: i16) -> (Option<u32>, Option<u32>) {
    let mut w = duel([AR, MAGNUM], 8.0);
    w.fighters[1].body.pos = V3::new(0.0, 0.0, 60.0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0] = Input {
        buttons: GRENADE,
        pitch,
        ..Input::default()
    };
    w.step(&inputs, &NO_LAG);
    assert_eq!(w.fighters[0].grenades, GREN_START - 1);
    inputs[0].buttons = 0;
    let (mut down, mut off) = (None, None);
    for t in 1..TICK_HZ * 6 {
        w.step(&inputs, &NO_LAG);
        if down.is_none() && w.proj.live().any(|(_, p)| p.landed) {
            down = Some(t);
        }
        if w.events.as_slice().iter().any(|e| e.kind == Ev::Blast) {
            off = Some(t);
            break;
        }
    }
    (down, off)
}

#[test]
fn a_frag_goes_off_half_a_second_after_it_comes_down_never_before_its_arming() {
    // Thrown at the feet: down at once, so off at the arming time.
    let (_, off) = frag_at(-PITCH_MAX);
    let arm = ticks(GREN_RELEASE_MS) + ticks(GREN_ARM_MS);
    assert!(off.is_some_and(|t| t.abs_diff(arm) <= 2), "{off:?} vs {arm}");
    // Lofted: down after the arming time, so off half a second after.
    let (down, off) = frag_at(1820);
    let down = down.expect("it came down");
    assert!(down > arm, "down at {down}");
    assert!(
        off.is_some_and(|t| t.abs_diff(down + ticks(GREN_FUSE_MS)) <= 1),
        "down {down}, off {off:?}"
    );
}

#[test]
fn a_switch_on_the_tick_it_leaves_the_hand_throws_what_was_spent() {
    // No stickies: a frag spent, the kind switched just as it goes.
    let mut w = duel([AR, MAGNUM], 8.0);
    w.fighters[0].stickies = 0;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0].buttons = GRENADE;
    w.step(&inputs, &NO_LAG);
    let release = (ticks(GREN_COOL_MS) - ticks(GREN_RELEASE_MS)) as u16;
    for _ in 0..TICK_HZ {
        // The fighter's tick counts its cooldown down first.
        inputs[0].buttons = if w.fighters[0].gren_cool == release + 1 {
            GREN_SWITCH
        } else {
            0
        };
        w.step(&inputs, &NO_LAG);
        if let Some((_, p)) = w.proj.live().next() {
            assert_eq!(p.kind, sim::world::ProjKind::Frag);
            return;
        }
    }
    panic!("nothing thrown");
}

#[test]
fn a_frag_still_hurts_at_its_rim() {
    // 5 m off a frag at the feet: near the rim, where Halo 2's still does
    // most of `GREN_EDGE` (a blast falling to nothing would barely scratch).
    let mut w = duel([AR, MAGNUM], 5.0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0] = Input {
        buttons: GRENADE,
        pitch: -PITCH_MAX,
        ..Input::default()
    };
    w.step(&inputs, &NO_LAG);
    inputs[0].buttons = 0;
    for _ in 0..ticks(GREN_RELEASE_MS) {
        w.step(&inputs, &NO_LAG);
    }
    // Set down at the thrower's feet (one thrown down at 30 m/s bounces
    // high).
    for i in 0..MAX_PROJ {
        if w.proj[i].active {
            w.proj[i].pos = V3::new(0.0, 0.0, 0.0);
            w.proj[i].vel = V3::ZERO;
        }
    }
    for _ in 0..TICK_HZ * 2 {
        w.step(&inputs, &NO_LAG);
    }
    let took = (SHIELD_MAX - w.fighters[1].shield) / PT;
    // Half of it, on shields.
    assert!((GREN_EDGE / 2..GREN_DMG / 4).contains(&took), "took {took}");
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

/// Fighter 0 walks onto the open map's pad, holding `power` instead of its
/// shotgun: taken on touch, no USE.
fn take_power(power: u8) -> World {
    let mut w = duel([BR, MAGNUM], 20.0);
    w.map.pads[0].arm = power;
    w.fighters[0].body.pos = V3::new(0.0, 0.0, -30.0);
    w.step(&[Input::default(); MAX_FIGHTERS], &NO_LAG);
    assert!(w.pads[0].empty_for > 0, "taken");
    assert_eq!(w.fighters[0].gun().arm, BR, "a power-up is no gun");
    w.fighters[0].body.pos = V3::ZERO;
    w
}

#[test]
fn the_overshield_climbs_to_three_times_full_then_drains_back() {
    let mut w = take_power(sim::arms::OVERSHIELD);
    let idle = [Input::default(); MAX_FIGHTERS];
    let mut n = 0;
    while w.fighters[0].charging {
        w.step(&idle, &NO_LAG);
        n += 1;
    }
    assert_eq!(w.fighters[0].shield, OVERSHIELD_MAX);
    assert!(n <= ticks(2000), "full in 2 s from full shields, not {n} ticks");
    // A sticky worn still kills, whatever the shield.
    w.fighters[1].body.pos = V3::new(3.0, 0.0, 0.0);
    w.fighters[1].stickies = 1;
    let mut sticky = [Input::default(); MAX_FIGHTERS];
    sticky[1] = Input {
        buttons: GRENADE | GREN_SWITCH,
        yaw: 32768,
        pitch: -2000,
        ..Input::default()
    };
    w.step(&sticky, &NO_LAG);
    sticky[1].buttons = 0;
    for _ in 0..ticks(GREN_RELEASE_MS) {
        w.step(&sticky, &NO_LAG);
    }
    for _ in 0..ticks(3000) {
        w.step(&idle, &NO_LAG);
        if !w.fighters[0].alive {
            break;
        }
    }
    assert!(!w.fighters[0].alive, "a sticky worn kills through an overshield");
    // Another, left alone, drains to full and stays.
    let mut w = take_power(sim::arms::OVERSHIELD);
    for _ in 0..ticks(45_000) {
        w.step(&idle, &NO_LAG);
    }
    assert_eq!(w.fighters[0].shield, SHIELD_MAX);
    assert!(!w.fighters[0].overshield());
}

#[test]
fn camouflage_hides_a_body_standing_still_until_it_fires() {
    let mut w = take_power(sim::arms::CAMO);
    let idle = [Input::default(); MAX_FIGHTERS];
    w.step(&idle, &NO_LAG);
    assert_eq!(w.fighters[0].cloak(), CAMO_BEST, "standing still");
    w.fighters[0].body.vel = V3::new(RUN, 0.0, 0.0);
    assert!(w.fighters[0].cloak() < CAMO_BEST, "running shows it");
    w.fighters[0].body.vel = V3::ZERO;
    let at = w.fighters[1].body.pos + V3::new(0.0, 1.0, 0.0);
    shoot(&mut w, at, FIRE, NO_LAG);
    assert!(w.fighters[0].cloak() < 0.01, "firing shows it whole");
    // The burst's last round, then the flare.
    for _ in 0..ticks(CAMO_FLARE_MS + 500) {
        w.step(&idle, &NO_LAG);
    }
    assert_eq!(w.fighters[0].cloak(), CAMO_BEST, "and it fades back");
    for _ in 0..ticks(CAMO_MS) {
        w.step(&idle, &NO_LAG);
    }
    assert_eq!((w.fighters[0].camo, w.fighters[0].cloak()), (0, 0.0), "it runs out");
}

/// The live drops, by gun.
fn dropped(w: &World, arm: u8) -> Vec<Dropped> {
    w.drops
        .live()
        .filter(|(_, d)| d.gun.arm == arm)
        .map(|(_, d)| *d)
        .collect()
}

#[test]
fn the_dead_drop_what_they_held_and_a_swap_leaves_the_old_gun() {
    let mut w = duel([SNIPER, MAGNUM], 20.0);
    let at = head(&w, 1);
    shoot(&mut w, at, FIRE, NO_LAG);
    assert!(!w.fighters[1].alive);
    // The gun in hand, as it was (the holstered magnum stays with them),
    // and the grenades they carried.
    let gun = dropped(&w, SNIPER);
    assert_eq!(gun.len(), 1);
    assert_eq!((gun[0].gun.mag, gun[0].gun.reserve), (4, 20));
    assert!(dropped(&w, MAGNUM).is_empty());
    let nades = dropped(&w, NONE);
    assert_eq!((nades[0].frags, nades[0].stickies), (GREN_START, STICKY_START));
    // Walking over: the grenades, as many as fit; USE: the sniper, and the
    // rifle let go of lies where it was taken.
    let f = &mut w.fighters[0];
    f.guns = [Gun::full(AR), Gun::full(MAGNUM)];
    f.cur = 0;
    let (frags, stickies) = (f.grenades, f.stickies);
    f.body.pos = gun[0].pos;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0].buttons = USE;
    w.step(&inputs, &NO_LAG);
    let f = &w.fighters[0];
    assert_eq!(f.gun().arm, SNIPER);
    assert_eq!((f.gun().mag, f.gun().reserve), (4, 20));
    assert_eq!(dropped(&w, AR).len(), 1);
    assert!(dropped(&w, SNIPER).is_empty());
    assert_eq!(f.grenades, (frags + GREN_START).min(GREN_MAX));
    assert_eq!(f.stickies, (stickies + STICKY_START).min(GREN_MAX));
}

#[test]
fn a_blast_throws_a_gun_on_the_floor_and_it_comes_to_rest() {
    let mut w = duel([ROCKET, MAGNUM], 8.0);
    w.fighters[1].body.pos = V3::new(0.0, 0.0, 60.0);
    let lay = V3::new(5.0, 0.0, 0.0);
    w.drops[0] = Dropped {
        active: true,
        gun: Gun::full(SNIPER),
        pos: lay,
        ttl: 100_000,
        ..Dropped::default()
    };
    shoot(&mut w, V3::new(4.0, 0.0, 0.0), FIRE, NO_LAG);
    let idle = [Input::default(); MAX_FIGHTERS];
    let mut blast = None;
    let mut high = 0.0f32;
    for _ in 0..TICK_HZ * 6 {
        w.step(&idle, &NO_LAG);
        if let Some(e) = w.events.as_slice().iter().find(|e| e.kind == Ev::Blast) {
            blast = Some(e.pos);
        }
        high = high.max(dropped(&w, SNIPER)[0].pos.y);
    }
    let at = blast.expect("the rocket went off");
    let gun = dropped(&w, SNIPER)[0];
    let was = (lay - at).flat().len();
    let now = (gun.pos - at).flat().len();
    assert!(now > was + 2.0, "thrown from {was:.2} m off the blast to {now:.2}");
    assert!(high > 0.3, "it popped up {high:.2} m");
    assert_eq!(gun.vel, V3::ZERO, "it came to rest");
    assert_eq!(gun.pos.y, 0.0, "on the floor");
}

#[test]
fn walking_over_a_gun_of_ones_own_kind_takes_its_rounds_to_a_full_reserve() {
    let mut w = duel([AR, MAGNUM], 20.0);
    let at = w.fighters[0].body.pos;
    w.fighters[0].guns[0].reserve = 100;
    w.drops[0] = Dropped {
        active: true,
        gun: Gun {
            arm: AR,
            mag: 32,
            reserve: 50,
            heat: 0,
        },
        pos: at,
        ttl: 1000,
        ..Dropped::default()
    };
    let idle = [Input::default(); MAX_FIGHTERS];
    w.step(&idle, &NO_LAG);
    assert_eq!(w.fighters[0].guns[0].reserve, 182);
    assert!(!w.drops[0].active, "spent, it goes");
    // Past a full reserve, the rest stays on the floor.
    w.drops[0] = Dropped {
        active: true,
        gun: Gun::full(AR),
        pos: at,
        ttl: 1000,
        ..Dropped::default()
    };
    w.step(&idle, &NO_LAG);
    assert_eq!(w.fighters[0].guns[0].reserve, 192);
    assert!(w.drops[0].active);
    assert_eq!(w.drops[0].gun.mag + w.drops[0].gun.reserve, 32 + 192 - 10);
}

#[test]
fn a_gun_for_two_hands_taken_while_dual_lets_both_hands_go() {
    let mut w = duel([MAGNUM, AR], 20.0);
    let at = w.fighters[0].body.pos;
    w.fighters[0].left = Gun::full(MAGNUM);
    w.drops[0] = Dropped {
        active: true,
        gun: Gun::full(ROCKET),
        pos: at,
        ttl: 1000,
        ..Dropped::default()
    };
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0].buttons = USE;
    w.step(&inputs, &NO_LAG);
    let f = &w.fighters[0];
    assert_eq!(f.gun().arm, ROCKET);
    assert!(!f.dual(), "the left hand let go");
    assert_eq!(dropped(&w, MAGNUM).len(), 2, "both magnums on the floor");
}

#[test]
fn a_melee_lunges_at_someone_just_past_reach() {
    for (dist, lands) in [(2.8, true), (MELEE_LUNGE_RANGE + 1.0, false)] {
        let mut w = duel([AR, MAGNUM], dist);
        let at = chest(&w, 1);
        let (yaw, pitch) = aim(&w, 0, at);
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        // Facing the striker: a blow, not an assassination.
        inputs[1].yaw = 32768;
        for t in 0..=ticks(MELEE_LUNGE_MS) {
            inputs[0] = Input {
                buttons: if t == 0 { MELEE } else { 0 },
                yaw,
                pitch,
                ..Input::default()
            };
            w.step(&inputs, &NO_LAG);
        }
        let hurt = w.fighters[1].shield < SHIELD_MAX;
        assert_eq!(hurt, lands, "at {dist} m");
        if lands {
            assert_eq!(w.fighters[1].shield, SHIELD_MAX - MELEE_DMG * PT);
            assert!(w.fighters[0].body.pos.x > 1.0, "the striker went to them");
        }
    }
}

#[test]
fn a_near_miss_bends_onto_the_body_never_the_head() {
    // Nearly half a metre wide of the chest, or of the hips, at 20 m: the
    // battle rifle's rounds find the body alike, while magnetism is on;
    // the sniper's (no magnetism) miss.
    for low in [0.0, CHEST_Y - HIP_Y] {
        for (gun, lands) in [(BR, MAGNETISM), (SNIPER, false)] {
            let mut w = duel([gun, MAGNUM], 20.0);
            let at = chest(&w, 1) + V3::new(0.0, -low, 0.45);
            shoot(&mut w, at, FIRE, NO_LAG);
            for _ in 0..ticks(300) {
                shoot(&mut w, at, 0, NO_LAG);
            }
            let name = sim::arms::arm(gun).name;
            assert_eq!(
                w.fighters[1].shield < SHIELD_MAX,
                lands,
                "{name}, {low} m below the chest"
            );
        }
    }
    if !MAGNETISM {
        return;
    }
    // Wide of the shoulder, up by the head, shields down: bent to the
    // chest, so body hits, never the head shot that would kill.
    let mut w = duel([BR, MAGNUM], 20.0);
    w.fighters[1].shield = 0;
    let at = w.fighters[1].body.pos + V3::new(0.0, CHEST_Y + 0.25, 0.4);
    shoot(&mut w, at, FIRE, NO_LAG);
    let mut parts = Vec::new();
    for _ in 0..ticks(300) {
        parts.extend(w.events.as_slice().iter().filter(|e| e.kind == Ev::Hit).map(|e| e.part));
        shoot(&mut w, at, 0, NO_LAG);
    }
    assert!(!parts.is_empty(), "the burst found the body");
    assert!(parts.iter().all(|&p| p < 2), "never the head: {parts:?}");
    assert!(w.fighters[1].alive);
}

#[test]
fn the_dead_come_back_away_from_the_enemy() {
    // The open map's two spawns: the enemy stands on one, so the other.
    let mut w = duel([AR, MAGNUM], 20.0);
    let spawns: Vec<V3> = w.map.spawns.iter().map(|s| s.pos).collect();
    for (i, &at) in spawns.iter().enumerate() {
        w.fighters[1].body.pos = at;
        let inputs = [Input::default(); MAX_FIGHTERS];
        w.fighters[0].health = 0;
        w.fighters[0].shield = 0;
        w.fighters[0].alive = false;
        w.fighters[0].respawn = 1;
        w.step(&inputs, &NO_LAG);
        let back = w.fighters[0].body.pos;
        assert_eq!(back, spawns[1 - i], "with the enemy on spawn {i}");
    }
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

/// The same on Dry Fork: slopes, the creek, a hill, the bunker's steps and
/// a launcher. Runs from red's launcher with the stick mostly forward and
/// the yaw wandering, so the walk crosses all of them.
#[test]
fn the_prediction_matches_the_server_on_terrain_and_launchers() {
    let map = sim::maps::canyon::DEF.build();
    let rules = Rules {
        teams: false,
        score_limit: 100,
        time_limit_ms: 0,
        ..Rules::duel()
    };
    let mut w = World::new(map.clone(), rules, 1, (0xdead, 0xbeef));
    w.seat(0, 0);
    w.seat(1, 1);
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &NO_LAG);
    }
    let l = map.launchers[0];
    let at = (l.area.min + l.area.max) * 0.5;
    w.fighters[0].body.pos = V3::new(at.x, map.ground(at.x, at.z), at.z);
    w.fighters[0].body.grounded = true;
    w.fighters[1].body.pos = V3::new(60.0, 0.0, -20.0);
    let mut mine = w.fighters[0];
    let mut rng = sim::rng::Pcg32::new(9, 9);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let (mut launched, mut climbed) = (false, 0.0f32);
    for t in 0..TICK_HZ * 40 {
        let r = rng.next_u32();
        // Standing on the launcher it is thrown, not jumping off it first.
        let jump = if t == 0 { 0 } else { JUMP };
        let inp = Input {
            // No grenades: a frag's blast on its own thrower is the world's.
            buttons: (r as u16) & (FIRE | jump | CROUCH | RELOAD | SWAP),
            yaw: (fmath::sin(t as f32 * 0.003) * 30000.0) as i16 as u16,
            pitch: 2000,
            mx: (r >> 16) as i8 / 4,
            mz: 127,
        }
        .sanitized();
        inputs[0] = inp;
        w.step(&inputs, &NO_LAG);
        mine.step_local(&inp, &map);
        let s = &w.fighters[0];
        assert_eq!(s.body, mine.body, "tick {t}");
        assert_eq!(
            (s.guns, s.refire, s.reload, s.shots),
            (mine.guns, mine.refire, mine.reload, mine.shots),
            "tick {t}"
        );
        // Thrown faster than anyone runs (a jump only goes up).
        launched |= s.body.vel.flat().len() > 2.0 * RUN;
        climbed = climbed.max(s.body.pos.y);
    }
    assert!(launched, "the launcher threw it");
    assert!(climbed > 2.0, "it went up the ground: {climbed}");
}

/// Seat 0 pulls the sword at seat 1's chest once, then holds still; true
/// if 1 is dead within a second. `inputs[1]` is whatever 1 does meanwhile.
fn lunge_at(w: &mut World, other: Input) -> bool {
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[1] = other;
    for t in 0..TICK_HZ {
        let (yaw, pitch) = aim(w, 0, chest(w, 1));
        inputs[0] = Input {
            buttons: if t == 0 { FIRE } else { 0 },
            yaw,
            pitch,
            ..Input::default()
        };
        w.step(&inputs, &NO_LAG);
        if !w.fighters[1].alive {
            return true;
        }
    }
    false
}

#[test]
fn a_lunge_runs_down_a_target_that_strafes() {
    let mut w = duel([SWORD, MAGNUM], 5.0);
    let strafe = Input {
        mx: 127,
        yaw: 32768,
        ..Input::default()
    };
    assert!(lunge_at(&mut w, strafe), "it steers onto the target");
    assert_eq!(w.fighters[0].kills, 1);
    assert_eq!(w.fighters[0].lunge, 0, "the lunge is over with the kill");
    let me = w.fighters[0].body.vel;
    assert!(me.x * me.x + me.z * me.z < 0.01, "and it stops there: {me:?}");
}

#[test]
fn a_sword_never_lunges_through_a_wall_or_at_a_teammate() {
    let mut w = duel([SWORD, MAGNUM], 5.0);
    let mut map = w.map.clone();
    map.boxes
        .push(sim::geom::Aabb::new(V3::new(2.4, 0.0, -3.0), V3::new(2.6, 4.0, 3.0)));
    w.map = map.indexed();
    assert!(!lunge_at(&mut w, Input::default()));
    assert_eq!(w.fighters[0].lunge, 0);

    for ff in [false, true] {
        let mut w = duel([SWORD, MAGNUM], 4.0);
        w.rules.teams = true;
        w.rules.friendly_fire = ff;
        w.fighters[1].team = w.fighters[0].team;
        assert!(!lunge_at(&mut w, Input::default()));
        assert_eq!(w.fighters[0].lunge, 0, "friendly fire {ff}");
    }
}

#[test]
fn friendly_fire_hurts_a_teammate_only_when_on_and_a_team_kill_costs_a_point() {
    for ff in [false, true] {
        let mut w = duel([SNIPER, MAGNUM], 10.0);
        w.rules.teams = true;
        w.rules.friendly_fire = ff;
        w.fighters[1].team = w.fighters[0].team;
        let side = w.fighters[0].team as usize & 1;
        let before = w.score[side];
        let at = head(&w, 1);
        shoot(&mut w, at, FIRE, NO_LAG);
        for _ in 0..TICK_HZ {
            shoot(&mut w, at, 0, NO_LAG);
        }
        if ff {
            assert_eq!(w.score[side], before - 1, "a teammate shot dead costs the side a point");
        } else {
            assert_eq!(w.fighters[1].shield, SHIELD_MAX, "a teammate is untouched with it off");
            assert_eq!(w.score[side], before);
        }
    }
    assert!(
        Rules::doubles().friendly_fire,
        "ranked doubles plays with friendly fire, as MLG"
    );
    assert!(!Rules::big_team().friendly_fire);
}

#[test]
fn two_swords_lunging_at_each_other_clash() {
    let mut w = duel([SWORD, MAGNUM], 5.0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    for (s, inp) in inputs.iter_mut().enumerate().take(2) {
        let (yaw, pitch) = aim(&w, s, chest(&w, 1 - s));
        *inp = Input {
            buttons: FIRE,
            yaw,
            pitch,
            ..Input::default()
        };
    }
    let mut clashed = false;
    for _ in 0..TICK_HZ {
        w.step(&inputs, &NO_LAG);
        inputs[0].buttons = 0;
        inputs[1].buttons = 0;
        clashed |= w.events.as_slice().iter().any(|e| e.kind == Ev::Melee && e.part == 2);
    }
    assert!(clashed);
    assert!(w.fighters[0].alive && w.fighters[1].alive, "nobody hurt");
    assert_eq!(w.fighters[1].shield, SHIELD_MAX);
    let gap = (w.fighters[1].body.pos - w.fighters[0].body.pos).len();
    assert!(gap > 2.0, "thrown apart: {gap} m");
}

#[test]
fn the_sword_s_melee_swings_without_a_lunge_and_spends_nothing() {
    // Out of the swing's reach, the melee button doesn't lunge.
    let mut w = duel([SWORD, MAGNUM], 4.0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let (yaw, pitch) = aim(&w, 0, chest(&w, 1));
    inputs[0] = Input {
        buttons: MELEE,
        yaw,
        pitch,
        ..Input::default()
    };
    w.step(&inputs, &NO_LAG);
    assert_eq!(w.fighters[0].lunge, 0);
    assert!(w.fighters[1].alive);
    // In reach it kills; the battery is never spent.
    let mut w = duel([SWORD, MAGNUM], 1.6);
    let (yaw, pitch) = aim(&w, 0, chest(&w, 1));
    inputs[0] = Input {
        buttons: MELEE,
        yaw,
        pitch,
        ..Input::default()
    };
    w.step(&inputs, &NO_LAG);
    assert!(!w.fighters[1].alive);
    assert_eq!(w.fighters[0].gun().mag, sim::arms::arm(SWORD).mag);
}

/// Heat, vents and a charge are the fighter's own, so the prediction must
/// match them too: a charge pistol and a beam rifle, swapped and fired at
/// random.
#[test]
fn the_prediction_matches_through_heat_and_charge() {
    let mut w = duel([sim::arms::ION_PISTOL, sim::arms::BEAM_RIFLE], 45.0);
    w.fighters[1].body.pos = V3::new(45.0, 0.0, 45.0);
    let mut mine = w.fighters[0];
    let map = Map::open();
    let mut rng = sim::rng::Pcg32::new(11, 3);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let (mut vents, mut charged) = (0, 0);
    for t in 0..TICK_HZ * 30 {
        let r = rng.next_u32();
        // The trigger held in runs, so charges fill and guns overheat.
        let fire = if t % 120 < 90 { FIRE } else { 0 };
        let swap = if r.is_multiple_of(300) { SWAP } else { 0 };
        let inp = Input {
            buttons: fire | swap | ((r as u16) & JUMP),
            yaw: (t * 31) as u16,
            pitch: 2000,
            mx: (r >> 16) as i8,
            mz: (r >> 24) as i8,
        }
        .sanitized();
        inputs[0] = inp;
        w.step(&inputs, &NO_LAG);
        let local = mine.step_local(&inp, &map);
        charged += local.charged as u32;
        vents += (mine.reload > 0 && mine.gun().heat >= sim::arms::HEAT_FULL) as u32;
        let s = &w.fighters[0];
        assert_eq!(s.body, mine.body, "tick {t}");
        assert_eq!(
            (s.guns, s.cur, s.refire, s.reload, s.charge, s.shots, s.last_buttons),
            (
                mine.guns,
                mine.cur,
                mine.refire,
                mine.reload,
                mine.charge,
                mine.shots,
                mine.last_buttons
            ),
            "tick {t}"
        );
    }
    assert!(charged >= 2, "{charged} charged shots");
    assert!(vents > 0, "never overheated");
}

/// Two guns in hand, each with its own trigger, timers, heat and charge,
/// and the grenade switch: the client predicts all of it, so it must match.
#[test]
fn the_prediction_matches_with_a_gun_in_each_hand() {
    let mut w = duel([sim::arms::SMG, MAGNUM], 45.0);
    w.fighters[1].body.pos = V3::new(45.0, 0.0, 45.0);
    w.fighters[0].left = sim::fighter::Gun::full(sim::arms::ION_PISTOL);
    let mut mine = w.fighters[0];
    let map = Map::open();
    let mut rng = sim::rng::Pcg32::new(5, 9);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let (mut left_shots, mut right_shots) = (0, 0);
    for t in 0..TICK_HZ * 20 {
        let r = rng.next_u32();
        let left = if t % 150 < 100 { FIRE_LEFT } else { 0 };
        let right = if (t / 7) % 2 == 0 { FIRE } else { 0 };
        let taps = if r.is_multiple_of(97) { RELOAD } else { 0 } | if r.is_multiple_of(131) { GREN_SWITCH } else { 0 };
        let inp = Input {
            buttons: left | right | taps,
            yaw: (t * 13) as u16,
            pitch: 1000,
            mx: (r >> 16) as i8,
            mz: (r >> 24) as i8,
        }
        .sanitized();
        inputs[0] = inp;
        w.step(&inputs, &NO_LAG);
        let local = mine.step_local(&inp, &map);
        left_shots += local.fired_left as u32;
        right_shots += local.fired as u32;
        let s = &w.fighters[0];
        assert_eq!(s.body, mine.body, "tick {t}");
        assert_eq!(
            (
                s.guns,
                s.left,
                s.refire,
                s.reload,
                s.left_refire,
                s.left_reload,
                s.left_charge
            ),
            (
                mine.guns,
                mine.left,
                mine.refire,
                mine.reload,
                mine.left_refire,
                mine.left_reload,
                mine.left_charge
            ),
            "tick {t}"
        );
        assert_eq!(
            (s.shots, s.gren_kind, s.grenades, s.stickies),
            (mine.shots, mine.gren_kind, mine.grenades, mine.stickies),
            "tick {t}"
        );
    }
    assert!(
        left_shots > 5 && right_shots > 20,
        "left {left_shots}, right {right_shots}"
    );
}

#[test]
fn a_tap_held_through_a_missing_frame_acts_once() {
    // SWAP held over three ticks, the middle one never arrived (the room
    // repeats the last): one swap, not a swap and a swap back.
    let mut w = duel([sim::arms::BR, MAGNUM], 45.0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let start = w.fighters[0].cur;
    let held = Input {
        buttons: SWAP,
        ..Input::default()
    };
    for inp in [held, held.repeated(), held] {
        inputs[0] = inp;
        w.step(&inputs, &NO_LAG);
    }
    assert_ne!(w.fighters[0].cur, start, "swapped once");
    inputs[0] = Input::default();
    w.step(&inputs, &NO_LAG);
    inputs[0] = held;
    w.step(&inputs, &NO_LAG);
    assert_eq!(w.fighters[0].cur, start, "and a new press swaps back");
}

/// Seat 0, on the open map facing +x, with the grenade `kind` selected: the
/// press, and on until it has left the hand.
fn thrown(w: &mut sim::world::World, kind: u8, pitch: i16) {
    w.fighters[0].gren_kind = kind;
    w.fighters[0].stickies = 2;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0] = Input {
        buttons: GRENADE,
        pitch,
        ..Input::default()
    };
    w.step(&inputs, &NO_LAG);
    inputs[0].buttons = 0;
    for _ in 0..ticks(GREN_RELEASE_MS) {
        w.step(&inputs, &NO_LAG);
    }
}

fn live_grenade(w: &sim::world::World) -> Option<sim::world::Projectile> {
    (0..MAX_PROJ).map(|i| w.proj[i]).find(|p| p.active)
}

#[test]
fn a_grenade_leaves_the_hand_as_the_arm_flings_it() {
    let mut w = duel([AR, MAGNUM], 60.0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0].buttons = GRENADE;
    w.step(&inputs, &NO_LAG);
    assert_eq!(w.fighters[0].grenades, GREN_START - 1, "spent on the press");
    assert!(live_grenade(&w).is_none(), "not yet out of the hand");
    inputs[0].buttons = 0;
    for _ in 0..ticks(GREN_RELEASE_MS) {
        assert!(live_grenade(&w).is_none());
        w.step(&inputs, &NO_LAG);
    }
    assert!(live_grenade(&w).is_some(), "out of the hand");
}

#[test]
fn a_stuck_sticky_stays_exactly_where_it_stuck() {
    let mut w = duel([AR, MAGNUM], 60.0);
    thrown(&mut w, 1, -PITCH_MAX / 2);
    let idle = [Input::default(); MAX_FIGHTERS];
    let mut stuck = None;
    for _ in 0..ticks(4000) {
        w.step(&idle, &NO_LAG);
        match live_grenade(&w) {
            Some(p) if p.landed => {
                let at = *stuck.get_or_insert(p.pos);
                assert_eq!(p.pos, at, "it stays where it stuck");
                assert_eq!(p.vel, V3::ZERO);
            }
            Some(_) => {}
            None => break,
        }
    }
    assert!(stuck.is_some(), "it stuck");
    assert!(live_grenade(&w).is_none(), "and went off");
}

#[test]
fn a_grenade_takes_half_off_shields() {
    // Set off beside them, in full reach: 160 is 80 off a shield of 100.
    let mut w = duel([AR, MAGNUM], 60.0);
    thrown(&mut w, 0, 0);
    let i = (0..MAX_PROJ).find(|&i| w.proj[i].active).expect("thrown");
    w.fighters[1].body.pos = V3::new(30.0, 0.0, 30.0);
    w.proj[i].pos = w.fighters[1].body.pos + V3::new(0.5, 0.9, 0.0);
    w.proj[i].vel = V3::ZERO;
    w.proj[i].ttl = 1;
    let (shield, health) = (w.fighters[1].shield, w.fighters[1].health);
    let idle = [Input::default(); MAX_FIGHTERS];
    w.step(&idle, &NO_LAG);
    assert_eq!(w.fighters[1].health, health, "the shields took it all");
    assert_eq!(shield - w.fighters[1].shield, (GREN_DMG * PT) / 2);
}

#[test]
fn a_blast_sets_off_the_grenades_in_its_reach_and_no_others() {
    let mut w = duel([AR, MAGNUM], 60.0);
    thrown(&mut w, 0, 0);
    let first = (0..MAX_PROJ).find(|&i| w.proj[i].active).expect("thrown");
    let mut near = w.proj[first];
    near.pos = V3::new(40.0, 0.5, 2.0);
    near.vel = V3::ZERO;
    near.ttl = ticks(4000) as u16;
    let mut far = near;
    far.pos = V3::new(40.0, 0.5, 30.0);
    w.proj[first].pos = V3::new(40.0, 0.5, 0.0);
    w.proj[first].vel = V3::ZERO;
    w.proj[first].ttl = 1;
    let (near_at, far_at) = ((first + 1) % MAX_PROJ, (first + 2) % MAX_PROJ);
    w.proj[near_at] = near;
    w.proj[far_at] = far;
    let idle = [Input::default(); MAX_FIGHTERS];
    w.step(&idle, &NO_LAG);
    assert!(!w.proj[first].active);
    assert!(
        w.proj[near_at].ttl <= ticks(GREN_CHAIN_MS) as u16,
        "its fuse is cut short"
    );
    assert!(w.proj[far_at].ttl > ticks(GREN_CHAIN_MS) as u16, "out of its reach");
    for _ in 0..ticks(GREN_CHAIN_MS) + 1 {
        w.step(&idle, &NO_LAG);
    }
    assert!(!w.proj[near_at].active, "the chain went off");
    assert!(w.proj[far_at].active);
}

#[test]
fn a_grenade_pad_gives_what_fits_and_is_left_for_whoever_has_room() {
    let mut map = Map::open();
    map.pads = vec![sim::map::PadDef {
        pos: V3::new(0.0, 0.0, 0.0),
        arm: sim::arms::STICKIES,
        respawn_ms: 30_000,
    }];
    let mut w = World::new(map, Rules::duel(), 1, (1, 2));
    w.seat(0, 0);
    w.seat(1, 1);
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &NO_LAG);
    }
    w.fighters[0].stickies = GREN_MAX;
    w.fighters[0].body.pos = V3::new(0.0, 0.0, 0.0);
    w.step(&idle, &NO_LAG);
    assert_eq!(w.pads[0].empty_for, 0, "full up: left lying");
    w.fighters[0].stickies = GREN_MAX - 1;
    w.step(&idle, &NO_LAG);
    assert_eq!(w.fighters[0].stickies, GREN_MAX);
    assert!(w.pads[0].empty_for > 0, "taken");
}

#[test]
fn a_blast_cooks_off_grenades_lying_on_the_floor() {
    let mut w = duel([AR, MAGNUM], 60.0);
    thrown(&mut w, 0, 0);
    let i = (0..MAX_PROJ).find(|&i| w.proj[i].active).expect("thrown");
    w.drops[0] = sim::world::Dropped {
        active: true,
        frags: 2,
        stickies: 1,
        pos: V3::new(40.0, 0.0, 2.0),
        ttl: ticks(30_000),
        ..Default::default()
    };
    w.proj[i].pos = V3::new(40.0, 0.5, 0.0);
    w.proj[i].vel = V3::ZERO;
    w.proj[i].ttl = 1;
    let idle = [Input::default(); MAX_FIGHTERS];
    w.step(&idle, &NO_LAG);
    assert!(!w.drops[0].active, "nothing left of the pile");
    let lit = (0..MAX_PROJ).filter(|&k| w.proj[k].active).count();
    assert_eq!(lit, 2, "a pile of a kind goes off as one");
}

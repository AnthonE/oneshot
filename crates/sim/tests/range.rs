//! The range: every gun measured in a real match, the way a player meets
//! it. A perfect shooter (on the trigger the tick it may fire, on the spot
//! it aims at) against a target standing still, on the open map; what it
//! counts is what a player feels: trigger pulls to kill, time to kill,
//! where the rounds land, how long a rocket takes and how wide it kills.
//!
//! The pins are the feel we tune to: Halo 2's, carried onto our 100 shield
//! and 100 health (Halo 2 is 70 and 45, so it is the counts that carry, not
//! the points). Each pin says what it is in Halo 2. A change to `arms.rs`
//! that moves a count fails here first, on purpose.
//!
//! The whole table, to tune by:
//! `cargo test -p sim --test range -- --ignored --nocapture`

// The table is printed (as `examples/bot_match.rs` prints): a test's, never
// the sim's.
#![allow(clippy::disallowed_macros, clippy::disallowed_types)]

use sim::arms::{
    self, arm, Kind, AR, BEAM_RIFLE, BR, CANNON, CARBINE, ION_PISTOL, ION_RIFLE, LAUNCHER, MAGNUM, ROCKET, SHARDS,
    SHOTGUN, SMG, SNIPER, SWORD,
};
use sim::consts::*;
use sim::events::Kind as Ev;
use sim::fighter::Gun;
use sim::fmath;
use sim::geom::{aim_dir, rad_to_yaw, V3};
use sim::input::*;
use sim::world::{spread, Phase, ProjKind, Rules, World};
use sim::Map;

const NO_LAG: [u32; MAX_FIGHTERS] = [0; MAX_FIGHTERS];

/// Shooter at the origin, target `dist` m down +x, countdown over.
fn range(loadout: [u8; 2], dist: f32, salt: (u64, u64)) -> World {
    let rules = Rules {
        teams: false,
        score_limit: 1000,
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
    w.fighters[0].body.pos = V3::new(-dist * 0.5, 0.0, 0.0);
    w.fighters[1].body.pos = V3::new(dist * 0.5, 0.0, 0.0);
    // The target faces the shooter (a hit from behind is its own rule).
    w.fighters[1].yaw = 32768;
    w
}

fn look(w: &World, s: usize, at: V3) -> (u16, i16) {
    let d = at - w.fighters[s].eye();
    let yaw = rad_to_yaw(fmath::atan2(d.z, d.x));
    let flat = (d.x * d.x + d.z * d.z).sqrt();
    let pitch = fmath::round(fmath::atan2(d.y, flat) * (65536.0 / core::f32::consts::TAU)) as i16;
    (yaw, pitch)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Spot {
    Chest,
    Head,
}

fn spot(w: &World, s: usize, at: Spot) -> V3 {
    let y = match at {
        Spot::Chest => 1.1,
        Spot::Head => HEAD_Y,
    };
    w.fighters[s].body.pos + V3::new(0.0, y, 0.0)
}

/// A kill, as the shooter counts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Kill {
    /// Trigger pulls (a burst is one).
    pulls: u32,
    /// Rounds (pellets count once a shell) that left the gun.
    rounds: u32,
    /// Hits on the target (a projectile's that landed, when the next may
    /// already be flying).
    hits: u32,
    /// From the first round to the kill.
    ms: u32,
}

/// The perfect shooter: aimed at `at` every tick, the trigger pulled the
/// first tick a pull can fire (held, for an automatic). None if no kill in
/// ten seconds.
fn kill(id: u8, dist: f32, at: Spot, salt: (u64, u64)) -> Option<Kill> {
    let mut w = range([id, arms::NONE], dist, salt);
    let a = arm(id);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let (mut pulls, mut rounds, mut hits, mut first) = (0u32, 0u32, 0u32, None);
    for t in 0..TICK_HZ * 10 {
        // The fighter's timers count down before it fires, so a pull the
        // tick one reaches 1 is the first that can fire.
        let f = w.fighters[0];
        let ready = f.refire <= 1 && f.burst_left == 0 && f.reload <= 1 && f.gun().mag > 0;
        let pull = a.auto || (ready && f.last_buttons & FIRE == 0);
        let (yaw, pitch) = look(&w, 0, spot(&w, 1, at));
        inputs[0] = Input {
            buttons: if pull || f.gun().mag == 0 { FIRE } else { 0 },
            yaw,
            pitch,
            ..Input::default()
        };
        let before = w.fighters[0].shots;
        w.step(&inputs, &NO_LAG);
        let fired = w.fighters[0].shots - before;
        if fired > 0 {
            first.get_or_insert(t);
            rounds += fired * a.pellets as u32;
            // A round with no burst running before it starts a pull.
            pulls += (f.burst_left == 0) as u32;
        }
        hits += w
            .events
            .as_slice()
            .iter()
            .filter(|e| e.kind == Ev::Hit && e.a == 0 && e.b == 1)
            .count() as u32;
        if w.events.as_slice().iter().any(|e| e.kind == Ev::Kill && e.b == 1) {
            let ms = (t - first.unwrap_or(t)) * 1000 / TICK_HZ;
            return Some(Kill {
                pulls,
                rounds,
                hits,
                ms,
            });
        }
    }
    None
}

const SALT: (u64, u64) = (0x5eed, 0xfee1);

fn salts(n: u64) -> impl Iterator<Item = (u64, u64)> {
    (0..n).map(|i| (sim::rng::splitmix64(i), sim::rng::splitmix64(i ^ 0xabcd)))
}

/// How far off the aim a round lands at `dist`, in cm: the worst and the
/// mean of `n` draws. The salted draw the server makes, nothing else.
fn scatter(id: u8, dist: f32, n: u32) -> (f32, f32) {
    let base = aim_dir(0, 0);
    let a = arm(id);
    let (mut worst, mut sum) = (0.0f32, 0.0f32);
    for shot in 0..n {
        let d = spread(base, a.spread, SALT, 0, shot, 0);
        let off = (d * (dist / d.x) - base * dist).len() * 100.0;
        worst = worst.max(off);
        sum += off;
    }
    (worst, sum / n as f32)
}

/// The share of shells that kill a full target in one at `dist`.
fn one_shot(id: u8, dist: f32, n: u64) -> f32 {
    let mut killed = 0;
    for salt in salts(n) {
        let mut w = range([id, arms::NONE], dist, salt);
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        let (yaw, pitch) = look(&w, 0, spot(&w, 1, Spot::Chest));
        inputs[0] = Input {
            buttons: FIRE,
            yaw,
            pitch,
            ..Input::default()
        };
        w.step(&inputs, &NO_LAG);
        killed += (!w.fighters[1].alive) as u32;
    }
    killed as f32 / n as f32
}

/// The farthest a shell kills in one, `p` of the time, to 0.25 m.
fn one_shot_reach(id: u8, p: f32) -> f32 {
    let mut d = 1.0;
    while one_shot(id, d + 0.25, 100) >= p {
        d += 0.25;
    }
    d
}

/// Fires a rocket at a wall `dist` m off and gives the ms to the blast.
fn rocket_flight(dist: f32) -> u32 {
    let mut w = range([ROCKET, arms::NONE], 1.0, SALT);
    w.fighters[0].body.pos = V3::new(50.0 - dist, 0.0, 0.0);
    w.fighters[1].body.pos = V3::new(0.0, 0.0, 40.0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0] = Input {
        buttons: FIRE,
        ..Input::default()
    };
    for t in 0..TICK_HZ * 5 {
        w.step(&inputs, &NO_LAG);
        inputs[0].buttons = 0;
        if w.events.as_slice().iter().any(|e| e.kind == Ev::Blast) {
            return t * 1000 / TICK_HZ;
        }
    }
    u32::MAX
}

/// The rocket's blast: whether a full target `off` m to the side of where
/// it hits the floor dies.
fn rocket_kills_at(off: f32) -> bool {
    let mut w = range([ROCKET, arms::NONE], 20.0, SALT);
    let floor = V3::new(10.0, 0.0, 0.0);
    w.fighters[1].body.pos = V3::new(10.0, 0.0, off);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let (yaw, pitch) = look(&w, 0, floor);
    inputs[0] = Input {
        buttons: FIRE,
        yaw,
        pitch,
        ..Input::default()
    };
    for _ in 0..TICK_HZ * 3 {
        w.step(&inputs, &NO_LAG);
        inputs[0].buttons = 0;
        if !w.fighters[1].alive {
            return true;
        }
    }
    false
}

fn rocket_kill_radius() -> f32 {
    let mut r = 0.0;
    while rocket_kills_at(r + 0.1) {
        r += 0.1;
    }
    r
}

/// A sword pulled at a target `dist` m off, aimed at its chest: the ms to
/// the kill, or None.
fn sword(dist: f32, aim_off: f32) -> Option<u32> {
    let mut w = range([SWORD, arms::NONE], dist, SALT);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let at = spot(&w, 1, Spot::Chest) + V3::new(0.0, 0.0, aim_off);
    let (yaw, pitch) = look(&w, 0, at);
    inputs[0] = Input {
        buttons: FIRE,
        yaw,
        pitch,
        ..Input::default()
    };
    for t in 0..TICK_HZ * 2 {
        w.step(&inputs, &NO_LAG);
        if !w.fighters[1].alive {
            return Some(t * 1000 / TICK_HZ);
        }
    }
    None
}

/// The farthest a sword pull kills from, to 0.25 m.
fn lunge_reach() -> f32 {
    let mut d = 1.0;
    while sword(d + 0.25, 0.0).is_some() {
        d += 0.25;
    }
    d
}

// ---- The pins: Halo 2's counts on our points ----

#[test]
fn the_battle_rifle_is_a_four_shot() {
    // Halo 2: four bursts, the last in the head; seven to the body alone.
    let head = kill(BR, 30.0, Spot::Head, SALT).unwrap();
    assert_eq!(head.pulls, 4, "{head:?}");
    let body = kill(BR, 30.0, Spot::Chest, SALT).unwrap();
    assert_eq!(body.pulls, 7, "{body:?}");
    // Three bursts and a round: about 1.3 s, Halo 2's 4-shot.
    assert!((1200..=1400).contains(&head.ms), "{head:?}");
    // Still a four-shot as far as its scope is for.
    assert_eq!(kill(BR, 80.0, Spot::Head, SALT).unwrap().pulls, 4);
}

#[test]
fn the_magnum_is_a_five_shot_sidearm() {
    let head = kill(MAGNUM, 20.0, Spot::Head, SALT).unwrap();
    assert_eq!(head.pulls, 5, "{head:?}");
    assert_eq!(kill(MAGNUM, 20.0, Spot::Chest, SALT).unwrap().pulls, 8);
}

#[test]
fn the_sniper_is_one_in_the_head_two_in_the_body() {
    // Halo 2 the same.
    assert_eq!(kill(SNIPER, 80.0, Spot::Head, SALT).unwrap().pulls, 1);
    let body = kill(SNIPER, 80.0, Spot::Chest, SALT).unwrap();
    assert_eq!(body.pulls, 2);
    // Halo 2's half a second between rounds.
    assert!((480..=520).contains(&body.ms), "{body:?}");
}

#[test]
fn the_automatics_kill_in_about_two_seconds_up_close() {
    // Halo 2's SMG: 25 rounds at 15 a second, 1.6 s.
    let smg = kill(SMG, 8.0, Spot::Chest, SALT).unwrap();
    assert_eq!(smg.rounds, 25, "{smg:?}");
    assert!((1600..=1800).contains(&smg.ms), "{smg:?}");
    // The rifle (Halo 3's, there being none in Halo 2): 25 rounds, 2.1 s.
    let ar = kill(AR, 10.0, Spot::Chest, SALT).unwrap();
    assert_eq!(ar.rounds, 25, "{ar:?}");
    assert!((2000..=2200).contains(&ar.ms), "{ar:?}");
}

#[test]
fn the_battle_rifle_and_magnum_are_tight() {
    // Halo 2's BR: 0.1° of error in multiplayer, under 10 cm at 50 m; every
    // round of a burst on a head (0.22 m) at 50 m.
    let (worst, _) = scatter(BR, 50.0, 300);
    assert!(worst < 2.0 * HEAD_R * 100.0 * 0.5, "BR worst {worst} cm at 50 m");
    let (worst, _) = scatter(MAGNUM, 30.0, 300);
    assert!(worst < HEAD_R * 100.0, "magnum worst {worst} cm at 30 m");
    assert_eq!(arm(SNIPER).spread, 0.0);
}

#[test]
fn the_shotgun_kills_in_one_close_and_never_far() {
    // Halo 2's: ten pellets, seven kill; a sure kill in a room, a coin toss
    // across one, a tickle past 15 m.
    assert_eq!(arm(SHOTGUN).pellets, 10);
    let sure = one_shot_reach(SHOTGUN, 0.95);
    let half = one_shot_reach(SHOTGUN, 0.5);
    assert!((4.0..=7.0).contains(&sure), "sure to {sure} m");
    assert!((6.0..=10.0).contains(&half), "half the time to {half} m");
    assert_eq!(one_shot(SHOTGUN, 15.0, 100), 0.0);
}

#[test]
fn a_rocket_starts_slow_and_kills_wide() {
    // Halo 2's: 8 wu/s off the tube (24 m/s), 20 wu/s (61 m/s) by 20 wu.
    let a = arm(ROCKET);
    assert_eq!(a.kind, Kind::Rocket);
    let near = rocket_flight(10.0);
    let far = rocket_flight(40.0);
    // 10 m in about 0.37 s, 40 m in about 1.1 s.
    assert!((330..=420).contains(&near), "10 m in {near} ms");
    assert!((1000..=1200).contains(&far), "40 m in {far} ms");
    // It kills a full target about 3.4 m from where it lands (Halo 2's
    // 200 points at 1 wu, 75 at 2 wu, halved on shields).
    let r = rocket_kill_radius();
    assert!((3.0..=3.6).contains(&r), "kills to {r} m");
}

#[test]
fn melee_is_three_to_kill() {
    assert_eq!(200 / MELEE_DMG + (200 % MELEE_DMG != 0) as i32, 3);
}

#[test]
fn the_sword_kills_in_one_and_lunges() {
    // In reach, at once; past it, a lunge that closes and kills; past the
    // lunge, nothing.
    let close = sword(1.5, 0.0).expect("in reach");
    assert!(close < 20, "{close} ms");
    let lunge = sword(5.0, 0.0).expect("a lunge");
    assert!((150..=400).contains(&lunge), "a lunge of {lunge} ms");
    // The range is the eye's to the grown body: feet to feet, a body more.
    let reach = lunge_reach();
    let far = LUNGE_RANGE + HIT_R + LUNGE_PAD;
    assert!((far - 0.5..=far).contains(&reach), "lunges from {reach} m");
    assert!(sword(far + 0.5, 0.0).is_none());
    // The red reticle has some give, but not a metre's.
    assert!(sword(4.0, 0.4).is_some(), "a hair off still locks");
    assert!(sword(4.0, 1.5).is_none(), "well off doesn't");
}

/// Holds `id`'s trigger, aimed off into the open, for `ms`; when the gun
/// first overheats, in ms, or None.
fn overheats(id: u8, ms: u32) -> Option<u32> {
    let mut w = range([id, arms::NONE], 40.0, SALT);
    w.fighters[1].body.pos = V3::new(20.0, 0.0, 40.0);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let semi = !arm(id).auto;
    for t in 0..ms * TICK_HZ / 1000 {
        let f = w.fighters[0];
        // A semi-automatic pulled as fast as it fires.
        let pull = !semi || (f.refire <= 1 && f.last_buttons & FIRE == 0);
        inputs[0].buttons = if pull { FIRE } else { 0 };
        w.step(&inputs, &NO_LAG);
        if w.fighters[0].reload > 0 {
            return Some(t * 1000 / TICK_HZ);
        }
    }
    None
}

/// A charge held to full at the target's chest and let go: the target's
/// shield and health once it lands, and whether the pistol is venting.
fn charged_bolt(dist: f32) -> (i32, i32, bool) {
    let mut w = range([ION_PISTOL, BR], dist, SALT);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let full = ticks(arm(ION_PISTOL).charge_ms);
    for t in 0..full + 1 + TICK_HZ * 2 {
        let (yaw, pitch) = look(&w, 0, spot(&w, 1, Spot::Chest));
        inputs[0] = Input {
            buttons: if t < full { FIRE } else { 0 },
            yaw,
            pitch,
            ..Input::default()
        };
        w.step(&inputs, &NO_LAG);
        if w.fighters[1].shield < SHIELD_MAX {
            break;
        }
    }
    let v = w.fighters[1];
    (v.shield, v.health, w.fighters[0].reload > 0)
}

#[test]
fn the_carbine_is_a_seven_shot() {
    // Halo 2's: six strip the shields, the seventh in the head; a second.
    let head = kill(CARBINE, 30.0, Spot::Head, SALT).unwrap();
    assert_eq!(head.pulls, 7, "{head:?}");
    assert!((950..=1100).contains(&head.ms), "{head:?}");
    assert_eq!(kill(CARBINE, 30.0, Spot::Chest, SALT).unwrap().pulls, 12);
}

#[test]
fn the_beam_rifle_fires_twice_then_vents() {
    // Halo 2's: a sniper's counts, 0.25 s apart, but 65% heat a shot.
    assert_eq!(kill(BEAM_RIFLE, 80.0, Spot::Head, SALT).unwrap().pulls, 1);
    let body = kill(BEAM_RIFLE, 80.0, Spot::Chest, SALT).unwrap();
    assert_eq!(body.pulls, 2);
    assert!((240..=270).contains(&body.ms), "{body:?}");
    let hot = overheats(BEAM_RIFLE, 2000).expect("overheats");
    assert!(hot <= 300, "the second quick shot overheats it: {hot} ms");
    let vent = arm(BEAM_RIFLE).vent_ticks() * 1000 / TICK_HZ;
    assert!((3000..=3500).contains(&vent), "vents in {vent} ms");
}

#[test]
fn the_ion_rifle_strips_shields_and_runs_hot() {
    // Halo 2's plasma rifle: 5 bolts strip the shields, 13 more kill, at
    // 7.5 a second: 2.3 s; held, it overheats in about 3.6 s.
    let k = kill(ION_RIFLE, 10.0, Spot::Chest, SALT).unwrap();
    assert_eq!(k.hits, 18, "{k:?}");
    assert!((2200..=2600).contains(&k.ms), "{k:?}");
    let hot = overheats(ION_RIFLE, 6000).expect("overheats");
    assert!((3000..=4300).contains(&hot), "overheats at {hot} ms");
}

#[test]
fn the_ion_pistol_s_charge_takes_the_shields_and_vents() {
    let (shield, health, venting) = charged_bolt(10.0);
    assert_eq!((shield, health), (0, HEALTH_MAX), "every shield and no health");
    assert!(venting, "a charged shot overheats the pistol");
    // Tapped as fast as it fires, it overheats inside a second and a half.
    let hot = overheats(ION_PISTOL, 3000).expect("overheats");
    assert!(hot < 1500, "{hot} ms");
}

#[test]
fn a_charge_then_a_head_shot_kills() {
    // The combo: the charged bolt takes the shields, one BR burst to the
    // head the rest.
    let mut w = range([ION_PISTOL, BR], 12.0, SALT);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let full = ticks(arm(ION_PISTOL).charge_ms);
    let mut swapped = false;
    for t in 0..TICK_HZ * 4 {
        let stripped = w.fighters[1].shield == 0;
        let at = if stripped { Spot::Head } else { Spot::Chest };
        let (yaw, pitch) = look(&w, 0, spot(&w, 1, at));
        let f = w.fighters[0];
        let buttons = if !swapped && stripped {
            swapped = true;
            SWAP
        } else if swapped {
            if f.refire <= 1 && f.last_buttons & FIRE == 0 {
                FIRE
            } else {
                0
            }
        } else if t < full {
            FIRE
        } else {
            0
        };
        inputs[0] = Input {
            buttons,
            yaw,
            pitch,
            ..Input::default()
        };
        w.step(&inputs, &NO_LAG);
        if !w.fighters[1].alive {
            assert!(t < TICK_HZ * 2, "combo in {t} ticks");
            return;
        }
    }
    panic!("no kill");
}

#[test]
fn eight_shards_go_off_together() {
    // Halo 2's needler: eight stuck, a kill.
    // (Nine hits: the eighth shard's own, then all eight going off.)
    let k = kill(SHARDS, 10.0, Spot::Chest, SALT).unwrap();
    assert_eq!(k.hits, 9, "{k:?}");
    // They turn after a target crossing the aim: fired where it is, not
    // led, at 15 m they still find it.
    let mut w = range([SHARDS, arms::NONE], 15.0, SALT);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[1] = Input {
        mx: 127,
        yaw: 32768,
        ..Input::default()
    };
    for _ in 0..TICK_HZ * 3 {
        let (yaw, pitch) = look(&w, 0, spot(&w, 1, Spot::Chest));
        inputs[0] = Input {
            buttons: FIRE,
            yaw,
            pitch,
            ..Input::default()
        };
        w.step(&inputs, &NO_LAG);
        if !w.fighters[1].alive {
            return;
        }
    }
    panic!("a strafing target never supercombined");
}

#[test]
fn the_launcher_is_three_and_the_cannon_one() {
    let k = kill(LAUNCHER, 10.0, Spot::Chest, SALT).unwrap();
    assert_eq!(k.hits, 3, "{k:?}");
    let k = kill(CANNON, 15.0, Spot::Chest, SALT).unwrap();
    assert_eq!(k.hits, 1, "{k:?}");
    assert_eq!(arm(CANNON).kind, Kind::Cannon);
}

// ---- Dual wielding and the sticky ----

#[test]
fn two_smgs_kill_in_about_half_the_time() {
    // Halo 2's: the same rounds to kill, both triggers at once.
    let mut w = range([SMG, arms::NONE], 8.0, SALT);
    w.fighters[0].left = Gun::full(SMG);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let (mut hits, mut first) = (0, None);
    for t in 0..TICK_HZ * 4 {
        let (yaw, pitch) = look(&w, 0, spot(&w, 1, Spot::Chest));
        inputs[0] = Input {
            buttons: FIRE | FIRE_LEFT,
            yaw,
            pitch,
            ..Input::default()
        };
        w.step(&inputs, &NO_LAG);
        first.get_or_insert(t);
        hits += w
            .events
            .as_slice()
            .iter()
            .filter(|e| e.kind == Ev::Hit && e.b == 1)
            .count();
        if !w.fighters[1].alive {
            let ms = (t - first.unwrap()) * 1000 / TICK_HZ;
            assert_eq!(hits, 25);
            assert!((700..=1000).contains(&ms), "two SMGs kill in {ms} ms");
            return;
        }
    }
    panic!("no kill");
}

#[test]
fn dual_wielding_gives_up_grenades_and_a_swing_drops_the_left() {
    let mut w = range([SMG, MAGNUM], 8.0, SALT);
    w.fighters[0].left = Gun::full(SMG);
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0].buttons = GRENADE;
    w.step(&inputs, &NO_LAG);
    assert_eq!(w.fighters[0].grenades, GREN_START, "no throw with two guns");
    inputs[0].buttons = 0;
    w.step(&inputs, &NO_LAG);
    inputs[0].buttons = MELEE;
    w.step(&inputs, &NO_LAG);
    assert!(!w.fighters[0].dual(), "the swing drops the left gun");
    // A swap with two in hand drops the left and takes the back one.
    w.fighters[0].left = Gun::full(SMG);
    for b in [0, SWAP] {
        inputs[0].buttons = b;
        w.step(&inputs, &NO_LAG);
    }
    assert!(!w.fighters[0].dual());
    assert_eq!(w.fighters[0].gun().arm, MAGNUM);
}

#[test]
fn a_one_handed_gun_off_a_pad_goes_in_the_left_hand() {
    let map = sim::map::by_id(4).expect("the Range");
    let pad = |id: u8| map.pads.iter().find(|p| p.arm == id).unwrap().pos;
    let rules = Rules {
        teams: false,
        time_limit_ms: 0,
        loadout: [SMG, MAGNUM],
        ..Rules::duel()
    };
    let mut w = World::new(map.clone(), rules, 1, SALT);
    w.seat(0, 0);
    w.seat(1, 1);
    let idle = [Input::default(); MAX_FIGHTERS];
    while w.phase == Phase::Countdown {
        w.step(&idle, &NO_LAG);
    }
    let take = |w: &mut World, id: u8| {
        w.fighters[0].body.pos = pad(id);
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        inputs[0].buttons = USE;
        w.step(&inputs, &NO_LAG);
        w.step(&idle, &NO_LAG);
    };
    take(&mut w, ION_RIFLE);
    let f = w.fighters[0];
    assert_eq!((f.gun().arm, f.left.arm), (SMG, ION_RIFLE), "beside the SMG");
    // A two-handed one is taken the usual way, and the left goes with it
    // the moment both hands are wanted (a swap).
    take(&mut w, BR);
    assert_eq!(w.fighters[0].gun().arm, BR);
}

/// A sticky thrown at the target's chest `dist` m off, the target running
/// sideways once it is stuck: (stuck to it, ms from the throw to its death
/// or None).
fn sticky_at(dist: f32) -> (bool, Option<u32>) {
    let mut w = range([BR, arms::NONE], dist, SALT);
    w.fighters[0].gren_kind = 1;
    w.fighters[0].stickies = 1;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let (yaw, pitch) = look(&w, 0, spot(&w, 1, Spot::Chest));
    inputs[0] = Input {
        buttons: GRENADE,
        yaw,
        pitch: pitch + 600,
        ..Input::default()
    };
    let mut stuck = false;
    for t in 0..TICK_HZ * 4 {
        w.step(&inputs, &NO_LAG);
        inputs[0].buttons = 0;
        stuck |= w
            .proj
            .live()
            .any(|(_, p)| p.kind == ProjKind::Sticky && p.landed && p.target == 1);
        if stuck {
            inputs[1] = Input {
                mx: 127,
                yaw: 32768,
                ..Input::default()
            };
        }
        if !w.fighters[1].alive {
            return (stuck, Some((t - ticks(GREN_RELEASE_MS)) * 1000 / TICK_HZ));
        }
    }
    (stuck, None)
}

#[test]
fn a_sticky_sticks_and_whoever_wears_it_dies() {
    assert_eq!(w_sticky_count(), STICKY_START);
    let (stuck, dead) = sticky_at(6.0);
    assert!(stuck, "it stuck");
    let ms = dead.expect("the one it stuck to died, running or not");
    // Halo 2's: never before 1.3 s from the throw (to the tick).
    assert!((1280..=1500).contains(&ms), "off at {ms} ms");
}

fn w_sticky_count() -> u8 {
    range([BR, arms::NONE], 5.0, SALT).fighters[0].stickies
}

#[test]
fn a_sticky_sticks_to_the_floor_and_goes_off_where_it_lands() {
    let mut w = range([BR, arms::NONE], 30.0, SALT);
    w.fighters[0].gren_kind = 1;
    w.fighters[0].stickies = 1;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    inputs[0] = Input {
        buttons: GRENADE,
        pitch: -2000,
        ..Input::default()
    };
    for t in 0..TICK_HZ * 4 {
        w.step(&inputs, &NO_LAG);
        inputs[0].buttons = 0;
        if w.events.as_slice().iter().any(|e| e.kind == Ev::Blast) {
            let ms = (t - ticks(GREN_RELEASE_MS)) * 1000 / TICK_HZ;
            assert!((1280..=1600).contains(&ms), "off at {ms} ms");
            assert!(w.fighters[1].alive && w.fighters[0].alive);
            return;
        }
    }
    panic!("never went off");
}

#[test]
fn the_grenade_switch_picks_the_throw() {
    let mut w = range([BR, arms::NONE], 30.0, SALT);
    w.fighters[0].stickies = 1;
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    let (frags, stickies) = (w.fighters[0].grenades, w.fighters[0].stickies);
    for b in [GREN_SWITCH, 0, GRENADE, 0] {
        inputs[0].buttons = b;
        w.step(&inputs, &NO_LAG);
    }
    assert_eq!((w.fighters[0].grenades, w.fighters[0].stickies), (frags, stickies - 1));
    // Out of stickies, a throw takes a frag.
    for _ in 0..TICK_HZ {
        w.step(&[Input::default(); MAX_FIGHTERS], &NO_LAG);
    }
    for b in [GRENADE, 0] {
        inputs[0].buttons = b;
        w.step(&inputs, &NO_LAG);
    }
    assert_eq!(w.fighters[0].grenades, frags - 1);
}

// ---- The whole table ----

#[test]
#[ignore = "prints the range table: --ignored --nocapture"]
fn the_range_table() {
    println!(
        "\n{:<16} {:>5} {:>13} {:>13} {:>13}",
        "gun", "m", "head", "body", "spread@m"
    );
    let rows = [
        (BR, 30.0),
        (MAGNUM, 20.0),
        (SNIPER, 80.0),
        (AR, 10.0),
        (SMG, 8.0),
        (SHOTGUN, 3.0),
        (CARBINE, 30.0),
        (BEAM_RIFLE, 80.0),
        (ION_RIFLE, 10.0),
        (ION_PISTOL, 8.0),
        (SHARDS, 10.0),
        (LAUNCHER, 10.0),
        (CANNON, 15.0),
        (ROCKET, 15.0),
    ];
    for (id, dist) in rows {
        let a = arm(id);
        let show = |k: Option<Kill>| match k {
            Some(k) => format!("{}p/{}h {:>4}ms", k.pulls, k.hits, k.ms),
            None => "no kill".to_string(),
        };
        let (worst, mean) = scatter(id, dist, 300);
        println!(
            "{:<16} {:>5} {:>13} {:>13} {:>5.0}/{:<4.0}cm",
            a.name,
            dist,
            show(kill(id, dist, Spot::Head, SALT)),
            show(kill(id, dist, Spot::Chest, SALT)),
            worst,
            mean,
        );
    }
    println!(
        "\nSHOTGUN one shot: 95% to {} m, 50% to {} m",
        one_shot_reach(SHOTGUN, 0.95),
        one_shot_reach(SHOTGUN, 0.5)
    );
    for d in [3.0, 5.0, 7.0, 9.0, 12.0, 15.0] {
        print!("  {d} m {:.0}%", one_shot(SHOTGUN, d, 100) * 100.0);
    }
    println!();
    println!(
        "ROCKET: 10 m in {} ms, 20 m in {} ms, 40 m in {} ms; kills to {:.1} m",
        rocket_flight(10.0),
        rocket_flight(20.0),
        rocket_flight(40.0),
        rocket_kill_radius()
    );
    for id in [BEAM_RIFLE, ION_RIFLE, ION_PISTOL] {
        println!("{} held: overheats at {:?} ms", arm(id).name, overheats(id, 8000));
    }
    println!(
        "ION PISTOL charged at 10 m: (shield, health, venting) {:?}",
        charged_bolt(10.0)
    );
    println!(
        "SWORD: 1.5 m {:?} ms, 4 m {:?} ms, lunges from {} m",
        sword(1.5, 0.0),
        sword(4.0, 0.0),
        lunge_reach()
    );
}

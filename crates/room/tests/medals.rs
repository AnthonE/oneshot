//! Medals: each rule on scripted events, and a whole bot match whose
//! carnage report agrees with the world.

use proto::medal as md;
use room::medals::{fewest_pulls, Award, Medals};
use room::Room;
use sim::arms::{BR, MAGNUM, SNIPER};
use sim::consts::{ticks, MAX_FIGHTERS, PT, TICK_HZ};
use sim::events::{Cause, Event, Kind as Ev};
use sim::map::Map;
use sim::world::{Phase, ProjKind, Projectile, Rules, World};

/// A world the script tells what happened, tick by tick.
struct Script {
    w: World,
    m: Medals,
}

impl Script {
    fn new(rules: Rules) -> Script {
        let mut w = World::new(Map::proving_grounds(), rules, 1, (1, 2));
        let mut m = Medals::default();
        for s in 0..rules.seats as usize {
            w.seat(s, (s & 1) as u8);
        }
        // Everyone on the map, full.
        let spawned: Vec<Event> = (0..rules.seats).map(spawn).collect();
        w.events.clear();
        for e in spawned {
            w.events.push(e);
        }
        m.observe(&w);
        Script { w, m }
    }

    /// `ms` later, `events` happened: the world takes them in (hits wear
    /// shields then health, a kill downs, a spawn makes whole) and the
    /// medals read them.
    fn at(&mut self, ms: u32, events: &[Event]) -> Vec<Award> {
        let dt = ticks(ms).max(1);
        self.w.tick += dt;
        for f in self.w.fighters.iter_mut() {
            f.since_hit = f.since_hit.saturating_add(dt);
        }
        self.w.events.clear();
        for e in events {
            let v = &mut self.w.fighters[e.b as usize];
            match e.kind {
                Ev::Hit => {
                    let pts = e.c as i32 * PT;
                    let soak = pts.min(v.shield);
                    v.shield -= soak;
                    v.health -= pts - soak;
                    v.since_hit = 0;
                }
                Ev::Kill => {
                    v.alive = false;
                    v.health = 0;
                    v.shield = 0;
                }
                Ev::Spawn => {
                    let f = &mut self.w.fighters[e.a as usize];
                    f.alive = true;
                    f.shield = sim::consts::SHIELD_MAX;
                    f.health = sim::consts::HEALTH_MAX;
                }
                _ => {}
            }
            self.w.events.push(*e);
        }
        self.m.observe(&self.w).to_vec()
    }
}

fn ev(kind: Ev, a: u8, b: u8, part: u8, c: u16) -> Event {
    Event {
        kind,
        a,
        b,
        part,
        c,
        ..Event::default()
    }
}
fn shot(a: u8, gun: u8) -> Event {
    ev(Ev::Shot, a, 0, 0, gun as u16)
}
fn hit(a: u8, b: u8, pts: u16, head: bool) -> Event {
    ev(Ev::Hit, a, b, if head { 2 } else { 1 }, pts)
}
fn kill(a: u8, b: u8, cause: u16) -> Event {
    ev(Ev::Kill, a, b, 0, cause)
}
/// A kill whose blow was to the head, as the sim marks it.
fn head_kill(a: u8, b: u8, cause: u16) -> Event {
    ev(Ev::Kill, a, b, 2, cause)
}
fn spawn(a: u8) -> Event {
    ev(Ev::Spawn, a, 0, 0, 0)
}

fn has(awards: &[Award], slot: u8, medal: u8) -> bool {
    awards.iter().any(|a| a.slot == slot && a.medal == medal)
}

fn names(awards: &[Award]) -> Vec<&'static str> {
    awards.iter().map(|a| md::info(a.medal).name).collect()
}

const GUN: u16 = BR as u16;

#[test]
fn kills_four_seconds_apart_chain() {
    let mut s = Script::new(Rules::ffa());
    let a = s.at(10, &[kill(0, 1, GUN)]);
    assert_eq!(names(&a), ["First Strike"]);
    let a = s.at(3900, &[kill(0, 2, GUN)]);
    assert_eq!(names(&a), ["Double Kill"]);
    let a = s.at(3900, &[kill(0, 3, GUN)]);
    assert_eq!(names(&a), ["Triple Kill"]);
    // Too slow: a new chain.
    let a = s.at(4100, &[kill(0, 4, GUN)]);
    assert!(a.is_empty(), "{:?}", names(&a));
    let a = s.at(100, &[kill(0, 5, GUN), kill(0, 6, GUN)]);
    assert_eq!(names(&a), ["Double Kill", "Killing Spree", "Triple Kill"]);
}

#[test]
fn a_chain_outlives_its_killer() {
    let mut s = Script::new(Rules::ffa());
    s.at(10, &[kill(0, 1, GUN)]);
    s.at(500, &[kill(5, 0, GUN)]);
    // His grenade, landing after he died.
    let a = s.at(1500, &[kill(0, 2, Cause::Grenade as u16)]);
    assert!(has(&a, 0, md::DOUBLE_KILL), "{:?}", names(&a));
    assert!(has(&a, 0, md::FROM_THE_GRAVE));
    assert!(has(&a, 0, md::GRENADE_KILL));
    // And the one who died to it gets him back, after respawning.
    s.at(3000, &[spawn(0), spawn(2)]);
    let a = s.at(100, &[kill(0, 5, GUN)]);
    assert!(has(&a, 0, md::REVENGE), "{:?}", names(&a));
    assert!(!has(&a, 0, md::FROM_THE_GRAVE));
}

#[test]
fn a_spree_and_its_end() {
    let mut s = Script::new(Rules::ffa());
    let mut got = Vec::new();
    for v in 1..=5 {
        got.extend(s.at(5000, &[kill(0, v, GUN), spawn(v)]));
    }
    assert!(has(&got, 0, md::KILLING_SPREE), "{:?}", names(&got));
    let a = s.at(1000, &[kill(6, 0, Cause::Melee as u16)]);
    assert!(has(&a, 6, md::KILLJOY), "{:?}", names(&a));
    assert!(has(&a, 6, md::BEAT_DOWN));
    // A spree starts again from nothing.
    s.at(3000, &[spawn(0)]);
    let mut got = Vec::new();
    for v in 1..=4 {
        got.extend(s.at(5000, &[kill(0, v, GUN), spawn(v)]));
    }
    assert!(!has(&got, 0, md::KILLING_SPREE));
    let t = s.m.tally(0);
    assert_eq!((t.kills, t.deaths, t.best_spree), (9, 1, 5));
}

#[test]
fn the_fewest_pulls_are_halo_twos() {
    // The battle rifle's 4-shot, the magnum's 5.
    assert_eq!(fewest_pulls(BR), Some(4));
    assert_eq!(fewest_pulls(MAGNUM), Some(5));
    assert_eq!(fewest_pulls(SNIPER), None);
}

/// Seat 0's battle rifle on seat 1, from full: pulls of `bursts[i]`
/// rounds landing (of three), then a last pull whose first round is in
/// the head.
fn rifle_fight(s: &mut Script, bursts: &[u8]) -> Vec<Award> {
    let mut got = Vec::new();
    let gap = 67;
    for &n in bursts {
        for r in 0..3 {
            let on = r < n;
            let mut e = vec![shot(0, BR)];
            if on {
                e.push(hit(0, 1, 10, false));
            }
            got.extend(s.at(if r == 0 { 266 } else { gap }, &e));
        }
    }
    got.extend(s.at(266, &[shot(0, BR), hit(0, 1, 10, true), head_kill(0, 1, GUN)]));
    got
}

#[test]
fn a_perfect_is_four_bursts_from_full() {
    let mut s = Script::new(Rules::duel());
    let a = rifle_fight(&mut s, &[3, 3, 3, 1]);
    // Ten body rounds, but over four pulls: the fifth kills.
    assert!(!has(&a, 0, md::PERFECT), "{:?}", names(&a));
    assert!(has(&a, 0, md::HEADSHOT));
    s.at(3000, &[spawn(1)]);
    s.at(1000, &[]);
    let a = rifle_fight(&mut s, &[3, 3, 3]);
    assert!(has(&a, 0, md::PERFECT), "{:?}", names(&a));
    assert!(has(&a, 0, md::HEADSHOT));
    // Someone else's round in the fight: not all his.
    s.at(3000, &[spawn(1)]);
    s.at(1000, &[]);
    let mut s2 = Script::new(Rules::ffa());
    s2.at(100, &[shot(2, MAGNUM), hit(2, 1, 28, false)]);
    s2.at(176, &[shot(2, MAGNUM), hit(2, 1, 28, false)]);
    let a = rifle_fight(&mut s2, &[3, 3, 3]);
    assert!(!has(&a, 0, md::PERFECT), "{:?}", names(&a));
    assert!(has(&a, 2, md::ASSIST), "{:?}", names(&a));
    // A whole pull missed between: four landed, five fired.
    s.at(3000, &[spawn(1)]);
    s.at(6000, &[]);
    let a = rifle_fight(&mut s, &[3, 0, 3, 3]);
    assert!(!has(&a, 0, md::PERFECT), "{:?}", names(&a));
}

#[test]
fn a_kill_with_the_last_round() {
    let mut s = Script::new(Rules::duel());
    s.w.fighters[0].guns[0].mag = 0;
    let a = s.at(100, &[shot(0, BR), hit(0, 1, 250, false), kill(0, 1, GUN)]);
    assert!(has(&a, 0, md::LAST_SHOT), "{:?}", names(&a));
}

#[test]
fn a_sniper_unscoped() {
    let mut s = Script::new(Rules::duel());
    let a = s.at(
        100,
        &[shot(0, SNIPER), hit(0, 1, 120, true), head_kill(0, 1, SNIPER as u16)],
    );
    for m in [md::HEADSHOT, md::SNIPER_KILL, md::NO_SCOPE] {
        assert!(has(&a, 0, m), "{:?}", names(&a));
    }
    s.at(3000, &[spawn(1)]);
    s.w.fighters[0].last_buttons |= sim::input::ZOOM;
    let a = s.at(
        100,
        &[shot(0, SNIPER), hit(0, 1, 120, true), head_kill(0, 1, SNIPER as u16)],
    );
    assert!(has(&a, 0, md::SNIPER_KILL) && !has(&a, 0, md::NO_SCOPE));
}

/// Seat `s` turns `by` (1/65536 of a turn a tick, signed) for `n` ticks.
fn turn(sc: &mut Script, s: usize, by: i32, n: u32) {
    for _ in 0..n {
        let f = &mut sc.w.fighters[s];
        f.yaw = (f.yaw as i32 + by).rem_euclid(1 << 16) as u16;
        sc.at(0, &[]);
    }
}

fn snipe(sc: &mut Script) -> Vec<Award> {
    sc.at(0, &[shot(0, SNIPER), hit(0, 1, 120, false), kill(0, 1, SNIPER as u16)])
}

#[test]
fn a_full_turn_then_unscoped_is_a_360() {
    use room::medals::{FULL_TURN, SPIN_TICKS};
    // A full turn in half a second, then the shot.
    let mut s = Script::new(Rules::duel());
    let ticks_half = (TICK_HZ / 2) as i32;
    turn(&mut s, 0, FULL_TURN / ticks_half + 1, ticks_half as u32);
    let a = snipe(&mut s);
    assert!(
        has(&a, 0, md::NO_SCOPE) && has(&a, 0, md::NO_SCOPE_360),
        "{:?}",
        names(&a)
    );
    assert_eq!(s.m.tally(0).medals[md::NO_SCOPE_360 as usize], 1);
    // The other way round counts too.
    s.at(3000, &[spawn(1)]);
    turn(&mut s, 0, -(FULL_TURN / ticks_half + 1), ticks_half as u32);
    assert!(has(&snipe(&mut s), 0, md::NO_SCOPE_360));

    // Three quarters, or a wiggle back and forth past a full turn's travel:
    // a No Scope, not a 360.
    s.at(3000, &[spawn(1)]);
    turn(&mut s, 0, FULL_TURN * 3 / 4 / ticks_half, ticks_half as u32);
    let a = snipe(&mut s);
    assert!(
        has(&a, 0, md::NO_SCOPE) && !has(&a, 0, md::NO_SCOPE_360),
        "{:?}",
        names(&a)
    );
    s.at(3000, &[spawn(1)]);
    for _ in 0..4 {
        turn(&mut s, 0, 1200, 20);
        turn(&mut s, 0, -1200, 20);
    }
    assert!(!has(&snipe(&mut s), 0, md::NO_SCOPE_360));

    // A full turn that ended too long ago is spent.
    s.at(3000, &[spawn(1)]);
    turn(&mut s, 0, FULL_TURN / ticks_half + 1, ticks_half as u32);
    turn(&mut s, 0, 0, SPIN_TICKS as u32);
    assert!(!has(&snipe(&mut s), 0, md::NO_SCOPE_360));

    // Scoped, it's no No Scope at all; and a spin with the rifle's scope up
    // is just a spin.
    s.at(3000, &[spawn(1)]);
    turn(&mut s, 0, FULL_TURN / ticks_half + 1, ticks_half as u32);
    s.w.fighters[0].last_buttons |= sim::input::ZOOM;
    let a = snipe(&mut s);
    assert!(!has(&a, 0, md::NO_SCOPE) && !has(&a, 0, md::NO_SCOPE_360));
    s.w.fighters[0].last_buttons &= !sim::input::ZOOM;

    // Any other gun after a full turn is only a kill.
    s.at(3000, &[spawn(1)]);
    turn(&mut s, 0, FULL_TURN / ticks_half + 1, ticks_half as u32);
    let a = s.at(0, &[shot(0, BR), hit(0, 1, 200, false), kill(0, 1, GUN)]);
    assert!(!has(&a, 0, md::NO_SCOPE_360));
}

#[test]
fn a_spawn_is_not_a_turn() {
    // Facing the spawn's way turns nothing: dying and coming back facing
    // the other way, then a half turn, is a half turn.
    use room::medals::FULL_TURN;
    let mut s = Script::new(Rules::duel());
    let half = (TICK_HZ / 4) as i32;
    turn(&mut s, 0, FULL_TURN / 2 / half, half as u32);
    s.at(0, &[kill(1, 0, GUN)]);
    s.w.fighters[0].yaw = s.w.fighters[0].yaw.wrapping_add(1 << 15);
    s.at(0, &[spawn(0)]);
    turn(&mut s, 0, FULL_TURN / 2 / half, half as u32);
    s.at(0, &[spawn(1)]);
    assert!(!has(&snipe(&mut s), 0, md::NO_SCOPE_360));
}

#[test]
fn stuck_or_caught_in_the_blast() {
    let mut s = Script::new(Rules::ffa());
    s.w.proj.alloc(Projectile {
        active: true,
        kind: ProjKind::Sticky,
        owner: 0,
        target: 1,
        landed: true,
        ..Projectile::default()
    });
    s.at(100, &[]);
    s.w.proj = Default::default();
    let sticky = Cause::Sticky as u16;
    let a = s.at(10, &[kill(0, 1, sticky), kill(0, 2, sticky)]);
    assert!(has(&a, 0, md::STICK), "{:?}", names(&a));
    assert!(a.iter().any(|x| x.medal == md::GRENADE_KILL && x.victim == 2));
}

#[test]
fn teams_avenge_protect_and_never_medal_a_betrayal() {
    // Doubles: seats 0 and 2 red, 1 and 3 blue.
    let mut s = Script::new(Rules::doubles());
    let a = s.at(10, &[kill(0, 2, GUN)]);
    assert!(a.is_empty(), "a betrayal: {:?}", names(&a));
    assert_eq!(s.m.tally(0).betrayals, 1);
    s.at(3000, &[spawn(2)]);
    // Blue kills red; red's other pays it back at once.
    s.at(10_000, &[kill(1, 0, GUN)]);
    let a = s.at(1000, &[kill(2, 1, GUN)]);
    assert!(has(&a, 2, md::AVENGER), "{:?}", names(&a));
    s.at(3000, &[spawn(0), spawn(1)]);
    // Blue hurts red 0; red 2 kills blue.
    s.at(10_000, &[shot(3, BR), hit(3, 0, 10, false)]);
    let a = s.at(500, &[kill(2, 3, GUN)]);
    assert!(has(&a, 2, md::PROTECTOR), "{:?}", names(&a));
    assert!(has(&a, 0, md::DISTRACTION), "{:?}", names(&a));
}

#[test]
fn hurt_first_and_won_anyway() {
    let mut s = Script::new(Rules::duel());
    s.at(100, &[shot(1, BR), hit(1, 0, 70, false)]);
    let a = s.at(500, &[shot(0, BR), hit(0, 1, 250, false), kill(0, 1, GUN)]);
    assert!(has(&a, 0, md::REVERSAL), "{:?}", names(&a));
}

/// A free for all of bots, played out: the carnage report counts what the
/// world counted, and the bots earn the common medals.
#[test]
fn a_bot_match_tallies_with_the_world() {
    let mut room = Room::new(Map::proving_grounds(), Rules::ffa(), 9, (3, 4));
    let mut counts = [0u32; md::COUNT];
    let mut medals_out = 0;
    for t in 0..TICK_HZ * 700 {
        let (_, out) = room.step(t as f64 * 1000.0 / TICK_HZ as f64);
        for (to, m) in &out {
            if let proto::Msg::Medal { slot, medal, .. } = *m {
                counts[medal as usize] += 1;
                medals_out += 1;
                let public = md::info(medal).public;
                assert_eq!(to.is_none(), public, "{} sent to {to:?}", md::info(medal).name);
                if let Some(t) = to {
                    assert_eq!(*t, slot, "a medal went to someone else");
                }
            }
        }
        if room.world.phase == Phase::Over {
            break;
        }
    }
    assert_eq!(room.world.phase, Phase::Over);
    let mut kills = 0;
    for s in 0..MAX_FIGHTERS {
        let f = &room.world.fighters[s];
        let t = room.medals().tally(s);
        assert_eq!(t.kills, f.kills, "seat {s}'s kills");
        assert_eq!(t.deaths, f.deaths, "seat {s}'s deaths");
        assert_eq!(
            t.medals.iter().map(|&x| x as u32).sum::<u32>() > 0,
            f.kills > 0 || t.assists > 0
        );
        kills += t.kills;
    }
    for (i, &n) in counts.iter().enumerate().filter(|(_, &n)| n > 0) {
        println!("{:>20} {n}", md::info(i as u8).name);
    }
    assert!(kills > 25, "{kills} kills");
    assert!(medals_out > kills as u32 / 2);
    assert_eq!(counts[md::FIRST_STRIKE as usize], 1);
    // A kill won it, unless the clock ran out.
    let by_score = room.world.score.iter().any(|&x| x >= room.world.rules.score_limit);
    assert_eq!(counts[md::GAME_WINNER as usize], by_score as u32);
    assert!(counts[md::DOUBLE_KILL as usize] > 0 || counts[md::ASSIST as usize] > 0);
}

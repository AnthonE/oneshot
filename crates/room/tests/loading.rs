//! The countdown waits for whoever is seated in it to load its level, at
//! most `LOAD_WAIT_MS`, and the replay holds it the same.

use proto::Name;
use room::{replay, Room, LOAD_WAIT_MS};
use sim::consts::{ticks, COUNTDOWN_MS};
use sim::map::Map;
use sim::world::{Phase, Rules};

fn room() -> (Room, u8) {
    let mut r = Room::new(Map::proving_grounds(), Rules::duel(), 9, (1, 2));
    r.wait_loads = true;
    let (slot, _) = r.join(Name::new("slow"), proto::Look::default()).unwrap();
    (r, slot)
}

fn run(r: &mut Room, n: u32) {
    for _ in 0..n {
        r.step(0.0);
    }
}

#[test]
fn the_countdown_waits_until_its_people_are_loaded() {
    let (mut r, slot) = room();
    let wait = ticks(COUNTDOWN_MS) * 3;
    run(&mut r, wait);
    assert_eq!(r.world.phase, Phase::Countdown, "held while loading");
    r.loaded(slot);
    run(&mut r, ticks(COUNTDOWN_MS) - 1);
    assert_eq!(r.world.phase, Phase::Countdown, "the whole countdown after");
    run(&mut r, 1);
    assert_eq!(r.world.phase, Phase::Playing);
    assert_eq!(r.began, Some(wait + ticks(COUNTDOWN_MS)));
    run(&mut r, 200);
    replay::verify(&r.replay()).expect("the hold replays");
}

#[test]
fn a_page_that_never_loads_holds_it_only_so_long() {
    let (mut r, _) = room();
    run(&mut r, ticks(LOAD_WAIT_MS) + ticks(COUNTDOWN_MS) + 1);
    assert_eq!(r.world.phase, Phase::Playing);
}

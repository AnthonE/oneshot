//! The same inputs give the same bits. Native vs wasm is `ci/parity.sh`;
//! this pins the native answer so a compiler or code change that moves an
//! arithmetic result shows up here first.

use sim::bot::{bot_match, Bot};
use sim::consts::MAX_FIGHTERS;
use sim::input::Input;
use sim::world::{Rules, World};
use sim::Map;

/// `bot_match(1, 20_000)`. If this moves, a rule changed (update it with
/// the commit that changed the rule) or the compiler changed a float
/// (stop: that is a finding).
const GOLDEN: u64 = 0xf52a_0d7f_9785_8729;

#[test]
fn a_bot_match_hashes_the_same_twice() {
    assert_eq!(bot_match(1, 5_000), bot_match(1, 5_000));
    assert_ne!(bot_match(1, 5_000), bot_match(2, 5_000));
}

#[test]
fn the_bot_match_matches_its_golden() {
    let h = bot_match(1, 20_000);
    assert_eq!(h, GOLDEN, "bot_match(1, 20000) = {h:#018x}");
}

/// `bot_match_on(3, 2, 1, 20_000)`: sixteen bots in Big Team on Dry Fork,
/// the terrain's golden (slopes, launchers, rays over the ground). Moves
/// with a terrain or Big Team rule, like [`GOLDEN`] with the rest.
const GOLDEN_DRY_FORK: u64 = 0xcb6c_21c6_0ea9_0e45;

#[test]
fn big_team_on_dry_fork_matches_its_golden() {
    let h = sim::bot::bot_match_on(3, 2, 1, 20_000);
    assert_eq!(h, GOLDEN_DRY_FORK, "bot_match_on(3, 2, 1, 20000) = {h:#018x}");
}

/// The gate only means something if the bots actually fight.
#[test]
fn bots_fight() {
    let rules = Rules {
        score_limit: 1000,
        time_limit_ms: 0,
        ..Rules::doubles()
    };
    let mut w = World::new(Map::proving_grounds(), rules, 3, (1, 2));
    let mut bots = [Bot::new(3, 0, 0.5); MAX_FIGHTERS];
    for (s, b) in bots.iter_mut().enumerate().take(4) {
        w.seat(s, (s & 1) as u8);
        *b = Bot::new(3, s, 0.6);
    }
    let mut inputs = [Input::default(); MAX_FIGHTERS];
    for _ in 0..20_000 {
        for s in 0..4 {
            inputs[s] = bots[s].think(&w, s);
        }
        w.step(&inputs, &[0; MAX_FIGHTERS]);
    }
    let kills: u16 = w.fighters.iter().map(|f| f.kills).sum();
    assert!(kills >= 5, "only {kills} kills in 20000 ticks");
}

//! Decoys (`room::decoy`): what is sent is never where its viewer could
//! see it, by fog's own test at fog's own numbers, and an honest seat's
//! aim has no side.

use room::decoy::{standing, Decoy, Trail};
use room::fog::{self, Fog};
use sim::bot::Bot;
use sim::consts::{MAX_FIGHTERS, TICK_HZ};
use sim::input::Input;
use sim::map::Map;
use sim::world::{Phase, Rules, World};

#[test]
fn a_decoy_is_never_in_sight_and_honest_aim_has_no_side() {
    let (mut episodes, mut sent, mut held, mut out) = (0, 0, 0, 0u32);
    // A few maps: an open one has few spots out of sight.
    for (seed, map) in (1..=12u64).zip([0u8, 1, 2, 5].into_iter().cycle()) {
        let mut rules = Rules::duel();
        rules.score_limit = 1000;
        let map: Map = sim::map::by_id(map).unwrap();
        let mut w = World::new(map, rules, seed, (seed, !seed));
        for s in 0..2 {
            w.seat(s, s as u8);
        }
        let mut bots: Vec<Bot> = (0..2).map(|s| Bot::new(seed, s, 0.5)).collect();
        let mut fogs = vec![Fog::default(); 2];
        let mut decoys = [Decoy::default(); 2];
        let mut trail = Trail::default();
        let rewind = [0u32; MAX_FIGHTERS];
        for _ in 0..180 * TICK_HZ {
            if w.phase == Phase::Over {
                break;
            }
            let next = w.tick + 1;
            trail.note(&w);
            for s in 0..2 {
                fogs[s].update(&w, s, 0.1, next);
                let fog = &fogs[s];
                if let Some((seat, at)) = decoys[s].step(&w, s, seed ^ 0xdec0, &trail, |t| fog.shows(t, next)) {
                    out += 1;
                    // Sent as an enemy fog isn't showing, where fog would
                    // not show one.
                    assert!(seat != s && !fog.shows(seat, next));
                    let ghost = standing(&w.fighters[seat], at);
                    assert!(
                        !fog::sees(&w.map, &w.fighters[s], &ghost, fog::MAX_LEAD_S, fog::PEEK_M),
                        "seed {seed} tick {}: seat {s} could see its decoy",
                        w.tick
                    );
                }
            }
            let mut inputs = [Input::default(); MAX_FIGHTERS];
            for (s, b) in bots.iter_mut().enumerate() {
                inputs[s] = b.think(&w, s);
            }
            w.step(&inputs, &rewind);
        }
        for d in &decoys {
            episodes += d.tally.episodes;
            sent += d.tally.toward_sent;
            held += d.tally.toward_held;
            assert!(d.tally.toward_sent + d.tally.toward_held <= d.tally.episodes);
        }
    }
    eprintln!(
        "{episodes} episodes, {out} ticks a decoy was out: the aim toward the sent spot {sent}, the held one {held}"
    );
    assert!(episodes >= 12, "{episodes} episodes");
    // Nobody here reads a decoy, so the aim leans neither way: well inside
    // what forty coin flips or more allow.
    let n = (sent + held) as f32;
    assert!((sent as f32 - n / 2.0).abs() <= 2.0 * n.sqrt(), "{sent} of {n}");
}

//! The aim review's numbers catch a scripted aimbot and leave the bots
//! alone.

use room::fog::Fog;
use room::watch::Watch;
use sim::bot::Bot;
use sim::consts::{HEAD_Y, MAX_FIGHTERS, TICK_HZ};
use sim::input::{Input, FIRE};
use sim::map::Map;
use sim::world::{Phase, Rules, World};

const TAU: f32 = core::f32::consts::TAU;

/// Plays a duel to its end (or `max_s`), seat 0 aimed by `aim` when it
/// returns Some, a bot otherwise; returns the watch.
fn duel(seed: u64, max_s: u32, aimbot: bool) -> Watch {
    let mut w = World::new(Map::proving_grounds(), Rules::duel(), seed, (seed, !seed));
    w.seat(0, 0);
    w.seat(1, 1);
    let mut bots = [Bot::new(seed, 0, 0.5), Bot::new(seed, 1, 0.5)];
    let mut fog = Fog::default();
    let mut watch = Watch::default();
    let rewind = [0u32; MAX_FIGHTERS];
    for _ in 0..max_s * TICK_HZ {
        if w.phase == Phase::Over {
            break;
        }
        let next = w.tick + 1;
        fog.update(&w, 0, 0.1, next);
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        for (s, b) in bots.iter_mut().enumerate() {
            inputs[s] = b.think(&w, s);
        }
        if aimbot {
            let me = &w.fighters[0];
            let them = &w.fighters[1];
            if me.active() && them.active() && fog.shows(1, next) {
                // Straight to the head, this tick, and pull.
                let to = them.body.pos + sim::geom::V3::new(0.0, HEAD_Y, 0.0) - me.eye();
                let flat = (to.x * to.x + to.z * to.z).sqrt();
                inputs[0].yaw = ((to.z.atan2(to.x) / TAU * 65536.0).round() as i32 & 0xffff) as u16;
                inputs[0].pitch = (to.y.atan2(flat) / TAU * 65536.0).round() as i16;
                inputs[0].buttons &= !FIRE;
                if me.last_buttons & FIRE == 0 {
                    inputs[0].buttons |= FIRE;
                }
            }
        }
        w.step(&inputs, &rewind);
        watch.observe(&w, |v, t| v == 0 && fog.shows(t, w.tick));
    }
    watch
}

#[test]
fn an_aimbot_is_flagged() {
    let w = duel(7, 600, true);
    let s = &w.stats[0];
    eprintln!(
        "aimbot: {} hits, {:.0}% head, median reaction {:?} ms over {}, {} snaps, {} ms pre-aim, {} kills",
        s.hits,
        s.head_rate() * 100.0,
        s.median_reaction_ms(),
        s.reactions_ms.len(),
        s.snaps,
        s.preaim_ms,
        s.kills
    );
    let flags = s.flags();
    assert!(flags.contains(&"head_rate"), "{flags:?}");
    assert!(flags.len() >= 2, "more than one line crossed: {flags:?}");
}

#[test]
fn the_bots_are_not_flagged() {
    for seed in 1..4 {
        let w = duel(seed, 600, false);
        for s in 0..2 {
            let st = &w.stats[s];
            eprintln!(
                "seed {seed} seat {s}: {} hits, {:.0}% head, median reaction {:?} ms over {}, {} snaps, {} ms pre-aim",
                st.hits,
                st.head_rate() * 100.0,
                st.median_reaction_ms(),
                st.reactions_ms.len(),
                st.snaps,
                st.preaim_ms
            );
            assert!(st.hits > 20, "the bots fought");
            assert!(st.flags().is_empty(), "seed {seed} seat {s}: {:?}", st.flags());
        }
    }
}

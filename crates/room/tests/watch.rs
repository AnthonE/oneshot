//! The aim review's numbers catch a scripted aimbot and leave the bots
//! alone.

use room::fog::Fog;
use room::watch::Watch;
use sim::bot::Bot;
use sim::consts::{HEAD_Y, MAX_FIGHTERS, TICK_HZ};
use sim::input::{Input, FIRE, USE};
use sim::map::Map;
use sim::world::{Phase, Rules, World};

const TAU: f32 = core::f32::consts::TAU;

/// Plays a duel to its end (or `max_s`), seat 0 an aimbot or a bot.
fn duel(seed: u64, max_s: u32, aimbot: bool) -> Watch {
    play(Rules::duel(), seed, max_s, aimbot)
}

/// Plays `rules` to its end (or `max_s`) with a bot in every seat, seat 0
/// instead snapping to the head of the nearest enemy fog shows it when
/// `aimbot`; every seat's fog feeds the watch.
fn play(rules: Rules, seed: u64, max_s: u32, aimbot: bool) -> Watch {
    let n = rules.seats as usize;
    let mut w = World::new(Map::proving_grounds(), rules, seed, (seed, !seed));
    for s in 0..n {
        w.seat(s, (s & 1) as u8);
    }
    let mut bots: Vec<Bot> = (0..n).map(|s| Bot::new(seed, s, 0.5)).collect();
    let mut fogs = vec![Fog::default(); n];
    let mut watch = Watch::default();
    let rewind = [0u32; MAX_FIGHTERS];
    for _ in 0..max_s * TICK_HZ {
        if w.phase == Phase::Over {
            break;
        }
        let next = w.tick + 1;
        for (s, f) in fogs.iter_mut().enumerate() {
            f.update(&w, s, 0.1, next);
        }
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        for (s, b) in bots.iter_mut().enumerate() {
            inputs[s] = b.think(&w, s);
        }
        // The aimbot keeps its rifle: what is measured is its aim, not
        // what it picked up.
        if aimbot {
            inputs[0].buttons &= !USE;
        }
        let me = &w.fighters[0];
        let target = (1..n)
            .filter(|&q| !(rules.teams && w.fighters[q].team == me.team))
            .filter(|&q| w.fighters[q].active() && fogs[0].shows(q, next))
            .min_by(|&a, &b| {
                let d = |q: usize| (w.fighters[q].body.pos - me.body.pos).len();
                d(a).total_cmp(&d(b))
            });
        if let (true, true, Some(q)) = (aimbot, me.active(), target) {
            // Straight to the head, this tick, and pull.
            let them = &w.fighters[q];
            let to = them.body.pos + sim::geom::V3::new(0.0, HEAD_Y, 0.0) - me.eye();
            let flat = (to.x * to.x + to.z * to.z).sqrt();
            inputs[0].yaw = ((to.z.atan2(to.x) / TAU * 65536.0).round() as i32 & 0xffff) as u16;
            inputs[0].pitch = (to.y.atan2(flat) / TAU * 65536.0).round() as i16;
            inputs[0].buttons &= !FIRE;
            if me.last_buttons & FIRE == 0 {
                inputs[0].buttons |= FIRE;
            }
        }
        w.step(&inputs, &rewind);
        let tick = w.tick;
        watch.observe(&w, |v, t| v < n && fogs[v].shows(t, tick));
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

#[test]
fn in_doubles_too() {
    let rules = Rules {
        time_limit_ms: 300_000,
        ..Rules::doubles()
    };
    let w = play(rules, 5, 300, true);
    let flags = w.stats[0].flags();
    assert!(flags.contains(&"head_rate"), "{flags:?}");
    for s in 1..4 {
        assert!(w.stats[s].flags().is_empty(), "seat {s}: {:?}", w.stats[s].flags());
    }
    for seed in 1..3 {
        let w = play(rules, seed, 300, false);
        for s in 0..4 {
            let st = &w.stats[s];
            assert!(st.hits > 10, "seed {seed} seat {s}: the bots fought ({} hits)", st.hits);
            assert!(st.flags().is_empty(), "seed {seed} seat {s}: {:?}", st.flags());
        }
    }
}

/// The zoo's blatant assist laid over a bot's own input (`room::assist`,
/// what `tools/aim` cuts its cheated twins with) is the scripted aimbot
/// again: the lines catch it. A bot with no assist in the same match isn't.
#[test]
fn an_assist_over_a_bot_is_flagged() {
    use room::assist::{Assist, Knobs};
    use room::fog::Plain;
    let rules = Rules::duel();
    let mut w = World::new(Map::proving_grounds(), rules, 7, (7, !7));
    for s in 0..2 {
        w.seat(s, s as u8);
    }
    let mut bots: Vec<Bot> = (0..2).map(|s| Bot::new(7, s, 0.5)).collect();
    let mut fogs = vec![Fog::default(); 2];
    let mut assist = Assist::new(Knobs::BLATANT, 0, 1);
    let mut watch = Watch::default();
    let rewind = [0u32; MAX_FIGHTERS];
    for _ in 0..600 * TICK_HZ {
        if w.phase == Phase::Over {
            break;
        }
        let next = w.tick + 1;
        for (s, f) in fogs.iter_mut().enumerate() {
            f.update(&w, s, 0.1, next);
        }
        let mut inputs = [Input::default(); MAX_FIGHTERS];
        for (s, b) in bots.iter_mut().enumerate() {
            inputs[s] = b.think(&w, s);
        }
        inputs[0].buttons &= !USE;
        let from = (w.fighters[0].yaw, w.fighters[0].pitch);
        assist.bend(
            &w,
            &mut Plain::default(),
            |t| fogs[0].shows(t, next),
            from,
            &mut inputs[0],
        );
        w.step(&inputs, &rewind);
        let tick = w.tick;
        watch.observe(&w, |v, t| v < 2 && fogs[v].shows(t, tick));
    }
    let flags = watch.stats[0].flags();
    assert!(flags.contains(&"head_rate") && flags.len() >= 2, "{flags:?}");
    assert!(watch.stats[1].flags().is_empty(), "{:?}", watch.stats[1].flags());
    // The profile: the aim up, the rest of its game the bot's own.
    for (s, st) in watch.stats.iter().take(2).enumerate() {
        eprintln!(
            "seat {s}: on target {:.0}% of {} ticks in sight, jerk/turn {:.2}, {:.1} m/s, {} of {} deaths unanswered",
            st.on_target() * 100.0,
            st.sight_ticks,
            st.jerk_deg / st.turn_deg.max(1.0),
            st.moved_m / (st.alive_ticks.max(1) as f32 / TICK_HZ as f32),
            st.unanswered,
            st.deaths
        );
    }
    assert!(watch.stats[0].on_target() > watch.stats[1].on_target() + 0.1);
}

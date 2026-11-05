//! Watching (`room::spectate`): a watcher is shown every seat's own view,
//! built by `room::build` with that seat's fog, and nothing before it is
//! the delay old: neither a snapshot nor a message (a kill, a medal).

use proto::{decode_snapshot, Msg};
use room::spectate::{DELAY_CASUAL_MS, WATCH_EVERY};
use room::Room;
use sim::consts::{ticks, MAX_FIGHTERS};
use sim::map::Map;
use sim::world::Rules;

#[test]
fn a_watcher_sees_every_seat_late_and_nothing_sooner() {
    let mut room = Room::new(Map::proving_grounds(), Rules::ffa(), 9, (1, 2));
    let first = room.spectate_first();
    let hello = room.spectate_welcome(first);
    let delay = ticks(DELAY_CASUAL_MS);
    // The welcome is to the shown tick, as the followed seat, then names.
    match hello[0] {
        Msg::Welcome { slot, tick, .. } => {
            assert_eq!(slot, first);
            assert_eq!(tick, 0, "the match is younger than the delay");
        }
        m => panic!("{m:?}"),
    }
    assert_eq!(hello.len(), 1 + room.size);

    let mut kills_made = Vec::new();
    let mut shown = 0;
    let mut kills_shown = Vec::new();
    for _ in 0..delay * 3 {
        let (_, out) = room.step(0.0);
        let now = room.world.tick;
        for (_, m) in &out {
            if let Msg::Kill { victim, .. } = m {
                kills_made.push((now, *victim));
            }
        }
        let (frames, msgs) = room.spectate.as_mut().unwrap().due(now);
        for f in frames {
            // Never sooner than the delay; one frame every few ticks.
            assert!(f.tick + delay <= now, "frame {} shown at {now}", f.tick);
            assert_eq!(f.tick % WATCH_EVERY, 0);
            for s in 0..MAX_FIGHTERS {
                let seated = s < room.size;
                assert_eq!(f.snaps[s].is_some(), seated, "seat {s}");
                if let Some(b) = &f.snaps[s] {
                    let snap = decode_snapshot(b).unwrap();
                    assert_eq!(snap.tick, f.tick);
                    // Its own fighter is that seat's.
                    assert!(snap.own.seated);
                    assert!(snap.remotes().iter().all(|r| r.slot as usize != s));
                }
            }
            shown += 1;
        }
        for (_, m) in msgs {
            if let Msg::Kill { victim, .. } = m {
                kills_shown.push((now, victim));
            }
        }
    }
    assert!(shown >= (delay * 2 / WATCH_EVERY) as usize - 1, "{shown} frames");
    assert!(!kills_made.is_empty(), "bots fought");
    // Every kill shown came the delay after it happened, in order.
    for (k, &(at, victim)) in kills_shown.iter().enumerate() {
        let (made, v) = kills_made[k];
        assert_eq!(v, victim);
        assert_eq!(at, made + delay, "kill {k}");
    }
}

#[test]
fn followers_move_through_the_seated_seats() {
    let mut room = Room::new(Map::proving_grounds(), Rules::duel(), 9, (1, 2));
    let a = room.spectate_first();
    let b = room.spectate_next(a);
    assert_ne!(a, b);
    assert_eq!(room.spectate_next(b), a);
    // A ranked room is shown later.
    let mut ranked = Room::ranked(Map::proving_grounds(), Rules::duel(), 9, (1, 2));
    ranked.spectate_welcome(0);
    room.spectate_welcome(a);
    assert!(ranked.spectate.as_ref().unwrap().delay > room.spectate.as_ref().unwrap().delay);
}

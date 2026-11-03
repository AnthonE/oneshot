//! A snapshot always fits one datagram: sixteen seats in plain view, every
//! projectile flying, more events than a snapshot holds. `room::build`
//! trims events, never the rest.

use proto::{encode_snapshot, MAX_DATAGRAM, MAX_REMOTES, MAX_SNAP_PROJ};
use sim::consts::{MAX_FIGHTERS, MAX_PROJ};
use sim::events::{Event, Kind as Ev};
use sim::geom::V3;
use sim::map::Map;
use sim::world::{ProjKind, Projectile, Rules, World};

#[test]
fn the_worst_case_snapshot_fits_a_datagram() {
    let mut w = World::new(Map::proving_grounds(), Rules::big_team(), 1, (2, 3));
    for s in 0..MAX_FIGHTERS {
        w.seat(s, (s & 1) as u8);
        // A row down the open lane at z = 10, all in each other's sight.
        w.fighters[s].body.pos = V3::new(-20.0 + 2.5 * s as f32, 0.0, 10.0);
    }
    for i in 0..MAX_PROJ {
        w.proj[i] = Projectile {
            active: true,
            kind: if i % 2 == 0 { ProjKind::Rocket } else { ProjKind::Frag },
            owner: (i % MAX_FIGHTERS) as u8,
            pos: V3::new(-20.0 + i as f32, 1.5, 10.0),
            vel: V3::new(30.0, 0.0, 0.0),
            ttl: 100,
            age: 20,
            target: sim::world::NO_TARGET,
        };
    }
    let events: Vec<Event> = (0..200)
        .map(|i| Event {
            kind: Ev::Shot,
            a: (i % MAX_FIGHTERS) as u8,
            c: 2,
            pos: V3::new(-20.0, 1.6, 10.0),
            end: V3::new(20.0, 1.6, 10.0),
            ..Event::default()
        })
        .collect();
    let mut queue = room::EventQueue::default();
    let snap = room::build(&w, 0, (1, 0), &events, &mut queue, |_| true, |_| true);
    assert_eq!(snap.n_remote as usize, MAX_REMOTES);
    assert_eq!(snap.n_proj as usize, MAX_SNAP_PROJ);
    assert_eq!(snap.n_score, 2);
    assert!(snap.n_events > 10, "room left for {} events", snap.n_events);
    let mut buf = [0u8; MAX_DATAGRAM];
    let n = encode_snapshot(&snap, &mut buf).expect("fits a datagram");
    assert!(n <= MAX_DATAGRAM);
    // What didn't fit waits, in order, and goes once the client acks.
    assert_eq!(queue.len(), 200);
    let first = snap.events()[0].id;
    let next = room::build(&w, 0, (1, 0), &[], &mut queue, |_| true, |_| true);
    assert_eq!(next.events()[0].id, first, "unacked: sent again");
    queue.ack(w.tick);
    let after = room::build(&w, 0, (1, 0), &[], &mut queue, |_| true, |_| true);
    assert_eq!(
        after.events()[0].id,
        first + snap.n_events as u16,
        "acked: the next ones"
    );
}

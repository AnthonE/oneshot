//! An event's fighters may be nobody (a flag home on its own, a vehicle
//! lost to nobody), and one the wire can't carry is dropped and counted,
//! never left to hold up the queue behind it.

use proto::NOBODY;
use room::EventQueue;
use sim::consts::MAX_FIGHTERS;
use sim::events::{Event, Kind as Ev};
use sim::geom::V3;
use sim::map::Map;
use sim::world::{Rules, World, NO_WINNER};

fn world() -> World {
    let mut w = World::new(Map::proving_grounds(), Rules::ctf(), 1, (2, 3));
    for s in 0..4 {
        w.seat(s, (s & 1) as u8);
    }
    w
}

fn send(w: &World, events: &[Event], queue: &mut EventQueue) -> proto::Snapshot {
    let snap = room::build(w, 0, (1, 0), events, queue, |_| false, |_| false, |_| false);
    let mut buf = [0u8; proto::MAX_DATAGRAM];
    let n = proto::encode_snapshot(&snap, &mut buf).unwrap();
    proto::decode_snapshot(&buf[..n]).unwrap()
}

#[test]
fn a_flag_home_on_its_own_goes_out() {
    let w = world();
    let home = Event {
        kind: Ev::Flag,
        a: NO_WINNER,
        b: 1,
        part: 2,
        pos: V3::new(10.0, 0.0, 0.0),
        ..Event::default()
    };
    let wreck = Event {
        kind: Ev::Wreck,
        a: NO_WINNER,
        part: 1,
        pos: w.fighters[0].eye(),
        ..Event::default()
    };
    let mut queue = EventQueue::default();
    let snap = send(&w, &[home, wreck], &mut queue);
    let got: Vec<_> = snap.events().iter().map(|e| (e.kind, e.a, e.b)).collect();
    assert_eq!(got[0], (Ev::Flag as u8, NOBODY, 1));
    assert!(got.contains(&(Ev::Wreck as u8, NOBODY, 0)), "{got:?}");
    assert_eq!(queue.dropped, 0);
    queue.ack(w.tick);
    assert!(queue.is_empty(), "acked, both are home");
}

#[test]
fn an_event_the_wire_cant_carry_is_dropped_not_stalled_on() {
    let w = world();
    // A hull hit on a vehicle slot past the wire's three bits.
    let bad = Event {
        kind: Ev::VehicleHit,
        a: 0,
        part: 200,
        c: 5,
        ..Event::default()
    };
    let flag = Event {
        kind: Ev::Flag,
        a: 2,
        b: 0,
        part: 0,
        ..Event::default()
    };
    let mut queue = EventQueue::default();
    let snap = send(&w, &[bad, flag], &mut queue);
    assert_eq!(snap.n_events, 1);
    assert_eq!(snap.events()[0].kind, Ev::Flag as u8);
    assert_eq!(queue.dropped, 1);
    assert!(MAX_FIGHTERS < NOBODY as usize);
}

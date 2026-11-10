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
            // Every kind, so the widest goes in.
            kind: [
                ProjKind::Rocket,
                ProjKind::Frag,
                ProjKind::Bolt,
                ProjKind::Charge,
                ProjKind::Needle,
                ProjKind::Lob,
                ProjKind::Cannon,
            ][i % 7],
            owner: (i % MAX_FIGHTERS) as u8,
            pos: V3::new(-20.0 + i as f32, 1.5, 10.0),
            vel: V3::new(30.0, 0.0, 0.0),
            ttl: 100,
            age: 20,
            ..Projectile::default()
        };
    }
    // A floor full of guns, all in sight.
    for i in 0..sim::consts::MAX_DROPS {
        w.drops[i] = sim::world::Dropped {
            active: true,
            gun: sim::fighter::Gun::full(1 + (i % 15) as u8),
            pos: V3::new(-20.0 + 1.5 * i as f32, 0.0, 9.0),
            ttl: 1000,
            ..sim::world::Dropped::default()
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
    let snap = room::build(&w, 0, (1, 0), &events, &mut queue, |_| true, |_| true, |_| true);
    assert_eq!(snap.n_remote as usize, MAX_REMOTES);
    assert_eq!(snap.n_proj as usize, MAX_SNAP_PROJ);
    assert_eq!(snap.n_drops as usize, proto::MAX_SNAP_DROPS);
    assert_eq!(snap.n_score, 2);
    assert!(snap.n_events > 10, "room left for {} events", snap.n_events);
    let mut buf = [0u8; MAX_DATAGRAM];
    let n = encode_snapshot(&snap, &mut buf).expect("fits a datagram");
    assert!(n <= MAX_DATAGRAM);
    // What didn't fit waits, in order, and goes once the client acks.
    assert_eq!(queue.len(), 200);
    let first = snap.events()[0].id;
    let next = room::build(&w, 0, (1, 0), &[], &mut queue, |_| true, |_| true, |_| true);
    assert_eq!(next.events()[0].id, first, "unacked: sent again");
    queue.ack(w.tick);
    let after = room::build(&w, 0, (1, 0), &[], &mut queue, |_| true, |_| true, |_| true);
    assert_eq!(
        after.events()[0].id,
        first + snap.n_events as u16,
        "acked: the next ones"
    );
}

/// More rounds in sight than a snapshot holds: what goes is what matters
/// to this client, its own and the nearest, never just the first slots.
#[test]
fn a_full_sky_sends_the_rounds_that_matter_to_you() {
    let mut w = World::new(Map::proving_grounds(), Rules::big_team(), 1, (2, 3));
    w.seat(0, 0);
    w.seat(1, 1);
    w.fighters[0].body.pos = V3::new(0.0, 0.0, 10.0);
    // Far ones in the low slots, going away; near ones in the high slots.
    for i in 0..MAX_PROJ {
        let x = if i < MAX_PROJ - 8 {
            30.0 + i as f32 * 0.1
        } else {
            2.0 + (MAX_PROJ - i) as f32
        };
        w.proj[i] = Projectile {
            active: true,
            kind: ProjKind::Needle,
            target: sim::world::NO_TARGET,
            owner: 1,
            pos: V3::new(x, 1.5, 10.0),
            vel: V3::new(30.0, 0.0, 0.0),
            ttl: 100,
            age: 20,
            ..Projectile::default()
        };
    }
    // Ours, the farthest of all.
    w.proj[0].owner = 0;
    w.proj[0].pos.x = 39.0;
    let mut queue = room::EventQueue::default();
    let snap = room::build(&w, 0, (1, 0), &[], &mut queue, |_| true, |_| true, |_| true);
    assert_eq!(snap.n_proj as usize, MAX_SNAP_PROJ);
    let sent = &snap.proj[..MAX_SNAP_PROJ];
    assert!(sent.iter().any(|p| p.owner == 0), "our own round goes");
    for i in MAX_PROJ - 8..MAX_PROJ {
        assert!(sent.iter().any(|p| p.pos.x == w.proj[i].pos.x), "near round {i} goes");
    }
}

/// Dry Fork at its fullest with a bubble: every seat in sight and six of
/// them in it, every hull a map may hold (its trucks, and the two
/// skimmers and two fliers it had) in it and driven, as many rounds and drops as a snapshot holds, riding behind a
/// driver. Room for four of the widest events (145 bits each) besides;
/// those that don't fit wait for the next (`room::EventQueue`).
#[test]
fn a_full_bubble_fits_with_room_for_events() {
    use proto::{NearBody, NearHull, MAX_SNAP_DROPS};
    let mut w = World::new(sim::map::by_id(3).unwrap(), Rules::big_team(), 1, (2, 3));
    for s in 0..MAX_FIGHTERS {
        w.seat(s, (s & 1) as u8);
    }
    let mut queue = room::EventQueue::default();
    let mut snap = room::build(&w, 0, (1, 0), &[], &mut queue, |_| true, |_| true, |_| true);
    snap.n_remote = MAX_REMOTES as u8;
    for (k, r) in snap.remotes.iter_mut().enumerate() {
        r.slot = 1 + k as u8;
        r.pos = V3::new(-100.0 + k as f32, 3.0, 50.0);
        r.vel = V3::new(6.5, -9.0, 3.25);
        r.near = (k < 6).then_some(NearBody {
            duck: 16,
            mx: -127,
            mz: 127,
            jump: true,
            crouch: true,
            share: 7,
        });
    }
    let hulls = sim::consts::MAX_VEHICLES;
    snap.n_vehicles = hulls as u8;
    for (i, v) in snap.vehicles.iter_mut().enumerate().take(hulls) {
        // A one-seater at its widest: mid-roll, its tank half spent, its
        // pilot's aim and buttons in its stick.
        let kind = w.map.vehicles.get(i).map_or(
            if i < 6 {
                sim::vehicle::SKIMMER
            } else {
                sim::vehicle::FLIER
            },
            |v| v.kind,
        );
        let one = kind != sim::vehicle::TRUCK;
        let stick = sim::input::Input {
            mx: -127,
            mz: 127,
            yaw: if one { 40_000 } else { 0 },
            pitch: if one { -3_000 } else { 0 },
            buttons: if one { sim::vehicle::DRIVE_BUTTONS } else { 0 },
        };
        *v = proto::VehState {
            slot: i as u8,
            pos: V3::new(-90.0 + 10.0 * i as f32, 1.5, -32.0),
            vel: V3::new(21.0, -3.0, 4.0),
            yaw: 1234,
            health: 200,
            seats: [9, 10],
            near: Some(NearHull {
                grounded: true,
                stick: Some(stick),
                share: 3,
                boost: 200,
                rest: 100,
                trick_cool: 300,
                jump: true,
            }),
            kind,
            pitch: -3_000,
            trick: 40,
            boosting: true,
        };
    }
    snap.n_proj = MAX_SNAP_PROJ as u8;
    snap.n_drops = MAX_SNAP_DROPS as u8;
    snap.own_vehicle = Some(w.vehicles[0]);
    snap.drive = Some(sim::input::Input {
        mx: -127,
        mz: 127,
        ..sim::input::Input::default()
    });
    snap.n_events = 0;
    let mut buf = [0u8; MAX_DATAGRAM];
    let n = encode_snapshot(&snap, &mut buf).expect("fits a datagram");
    let event = 145usize.div_ceil(8);
    assert!(n + 4 * event <= MAX_DATAGRAM, "{n} bytes before events");
}

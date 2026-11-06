//! Delta snapshots (`proto::delta`): what a client makes whole from each
//! datagram is exactly the full snapshot of that tick, over whole matches
//! on every map, with datagrams lost and acks late, and deltas are what
//! mostly goes.

use proto::delta::Receiver;
use proto::{decode_snapshot, encode_snapshot, InputMsg, Name, DG_DELTA, MAX_DATAGRAM};
use room::Room;
use sim::bot::Bot;
use sim::consts::TICK_HZ;
use sim::map::Map;
use sim::world::Rules;

/// One match of `rules` on `map` for the first seat, every `lose`th
/// datagram lost and acks `lag` snapshots late; returns the bytes sent
/// whole and as sent.
fn play(map: Map, rules: Rules, every: u32, lose: u32, lag: usize) -> (usize, usize) {
    let mut room = Room::new(map, rules, 5, (3, 4));
    room.set_snap_every(every);
    let (slot, _) = room.join(Name::new("d"), proto::Look::default()).unwrap();
    let me = slot as usize;
    let mut driver = Bot::new(3, me, 0.6);
    let mut rx = Receiver::default();
    let mut out = Vec::new();
    let mut acks: Vec<u32> = Vec::new();
    let (mut whole, mut sent, mut deltas, mut n) = (0, 0, 0, 0u32);
    let mut newest = 0u32;
    for t in 0..TICK_HZ * 60 {
        let now = t as f64 * 1000.0 / TICK_HZ as f64;
        // The client acks the newest it made whole, `lag` snapshots late.
        let ack = if acks.len() > lag {
            acks[acks.len() - 1 - lag]
        } else {
            0
        };
        let mut m = InputMsg {
            ack_tick: ack,
            playout: 4,
            first_seq: 1000 + t,
            n: 1,
            ..InputMsg::default()
        };
        m.frames[0] = driver.think(&room.world, me);
        room.on_input(slot, &m, now);
        let mut got: Vec<(proto::Snapshot, Vec<u8>)> = Vec::new();
        room.step_into(now, &mut out, |_, s, b| got.push((*s, b.to_vec())));
        for (s, b) in got {
            n += 1;
            let mut full = [0u8; MAX_DATAGRAM];
            let f = encode_snapshot(&s, &mut full).unwrap();
            whole += f;
            sent += b.len();
            if b[0] & 0xf == DG_DELTA {
                deltas += 1;
            }
            if lose > 0 && n.is_multiple_of(lose) {
                continue;
            }
            let made = rx.take(&b).expect("every delta's base is one we acked");
            assert_eq!(made, decode_snapshot(&full[..f]).unwrap(), "tick {}", s.tick);
            if made.tick > newest {
                newest = made.tick;
                acks.push(newest);
            }
        }
    }
    assert!(deltas * 10 > n * 8, "{deltas} deltas of {n}");
    (whole, sent)
}

#[test]
fn a_delta_makes_the_full_snapshot_on_every_map() {
    for d in sim::maps::ALL {
        let rules = Rules {
            time_limit_ms: 50_000,
            ..if d.big() { Rules::big_team() } else { Rules::doubles() }
        };
        let (whole, sent) = play(d.build(), rules, room::SNAP_EVERY, 0, 0);
        eprintln!(
            "{:>16}: {} B whole, {} B sent ({:.0}%)",
            d.name,
            whole,
            sent,
            100.0 * sent as f64 / whole as f64
        );
        assert!(sent * 10 < whole * 8, "{}: deltas saved little", d.name);
    }
}

#[test]
fn a_delta_survives_loss_late_acks_and_a_slower_rate() {
    let big = sim::maps::ALL.iter().find(|d| d.big()).unwrap();
    let rules = Rules {
        time_limit_ms: 50_000,
        ..Rules::big_team()
    };
    // One in seven lost, acks three snapshots late; then at 32 a second.
    play(big.build(), rules, room::SNAP_EVERY, 7, 3);
    play(big.build(), rules, 4, 5, 2);
    play(Map::proving_grounds(), Rules::ffa(), room::SNAP_EVERY, 3, 12);
}

#[test]
fn seats_worked_side_by_side_send_the_same_bytes() {
    let big = sim::maps::ALL.iter().find(|d| d.big()).unwrap();
    let rules = Rules {
        time_limit_ms: 30_000,
        ..Rules::big_team()
    };
    let room = |parallel: bool| {
        let mut r = Room::new(big.build(), rules, 11, (5, 6));
        r.parallel_from = if parallel { 1 } else { usize::MAX };
        let seats: Vec<u8> = (0..sim::consts::MAX_FIGHTERS)
            .filter_map(|i| {
                r.join(Name::new(&format!("p{i}")), proto::Look::default())
                    .map(|(s, _)| s)
            })
            .collect();
        (r, seats)
    };
    let (mut a, seats) = room(false);
    let (mut b, _) = room(true);
    let mut drivers: Vec<Bot> = seats.iter().map(|&s| Bot::new(4, s as usize, 0.5)).collect();
    let mut acks = [0u32; sim::consts::MAX_FIGHTERS];
    let (mut out_a, mut out_b) = (Vec::new(), Vec::new());
    let mut compared = 0;
    for t in 0..TICK_HZ * 20 {
        let now = t as f64 * 1000.0 / TICK_HZ as f64;
        for (k, &s) in seats.iter().enumerate() {
            let mut m = InputMsg {
                ack_tick: acks[s as usize],
                playout: 4,
                first_seq: 1000 + t,
                n: 1,
                ..InputMsg::default()
            };
            m.frames[0] = drivers[k].think(&a.world, s as usize);
            a.on_input(s, &m, now);
            b.on_input(s, &m, now);
        }
        let (mut sa, mut sb) = (Vec::new(), Vec::new());
        a.step_into(now, &mut out_a, |s, snap, bytes| {
            sa.push((s, snap.tick, bytes.to_vec()))
        });
        b.step_into(now, &mut out_b, |s, snap, bytes| {
            sb.push((s, snap.tick, bytes.to_vec()))
        });
        assert_eq!(sa, sb, "tick {}", a.world.tick);
        assert_eq!(out_a, out_b);
        for (s, tick, _) in &sa {
            acks[*s as usize] = *tick;
        }
        compared += sa.len();
    }
    assert!(compared > 16 * 1000, "{compared} snapshots compared");
}

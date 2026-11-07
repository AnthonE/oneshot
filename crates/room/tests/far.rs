//! Others far from a client on a big map are updated every few snapshots
//! and held between (`room::update_every`, `Snapshot::held`): never one
//! within `FAR_M`, never one firing at it, never longer than three
//! snapshots in a row; arenas never hold anyone. `-- --nocapture` prints
//! what it saves.

use proto::{encode_snapshot, InputMsg, Name, MAX_DATAGRAM};
use room::{update_every, Room, FAR_M};
use sim::bot::Bot;
use sim::consts::{MAX_FIGHTERS, TICK_HZ};
use sim::fighter::Fighter;
use sim::geom::V3;
use sim::input::FIRE;
use sim::world::Rules;

fn at(x: f32) -> Fighter {
    let mut f = Fighter {
        seated: true,
        alive: true,
        ..Fighter::default()
    };
    f.body.pos = V3::new(x, 0.0, 0.0);
    f
}

#[test]
fn far_is_every_few_and_firing_at_you_is_every_one() {
    let me = at(0.0);
    assert_eq!(update_every(&me, &at(30.0)), 1);
    assert_eq!(update_every(&me, &at(90.0)), 2);
    assert_eq!(update_every(&me, &at(200.0)), 4);
    // Facing us (yaw toward −x) and firing: every snapshot, however far.
    let mut shooter = at(200.0);
    shooter.last_buttons = FIRE;
    shooter.yaw = (0..64u16)
        .map(|k| k * 1024)
        .min_by(|&a, &b| sim::geom::aim_dir(a, 0).x.total_cmp(&sim::geom::aim_dir(b, 0).x))
        .unwrap();
    assert_eq!(update_every(&me, &shooter), 1);
    // Firing the other way: far is far.
    shooter.yaw = shooter.yaw.wrapping_add(32768);
    assert_eq!(update_every(&me, &shooter), 4);
}

/// A match for 16 people on `map`, `far_rates` as given: the bytes sent,
/// and every snapshot.
fn play(def: &sim::map::Def, far_rates: bool) -> (usize, Vec<(u8, proto::Snapshot, Vec<Fighter>)>) {
    play_with(def, far_rates, true)
}

/// [`play`], `bubbles` as `Room::bubbles`.
fn play_with(def: &sim::map::Def, far_rates: bool, bubbles: bool) -> (usize, Vec<(u8, proto::Snapshot, Vec<Fighter>)>) {
    let rules = Rules {
        time_limit_ms: 60_000,
        ..if def.big() { Rules::big_team() } else { Rules::ffa() }
    };
    let mut room = Room::new(def.build(), rules, 3, (8, 9));
    room.far_rates = far_rates;
    room.bubbles = bubbles;
    let seats: Vec<u8> = (0..MAX_FIGHTERS)
        .filter_map(|i| {
            room.join(Name::new(&format!("p{i}")), proto::Look::default())
                .map(|x| x.0)
        })
        .collect();
    let mut bots: Vec<Bot> = seats.iter().map(|&s| Bot::new(6, s as usize, 0.5)).collect();
    let mut acks = [0u32; MAX_FIGHTERS];
    let (mut out, mut bytes, mut all) = (Vec::new(), 0, Vec::new());
    for t in 0..TICK_HZ * 40 {
        let now = t as f64 * 1000.0 / TICK_HZ as f64;
        for (k, &s) in seats.iter().enumerate() {
            let mut m = InputMsg {
                ack_tick: acks[s as usize],
                playout: 4,
                first_seq: 1000 + t,
                n: 1,
                ..InputMsg::default()
            };
            m.frames[0] = bots[k].think(&room.world, s as usize);
            room.on_input(s, &m, now);
        }
        let mut got = Vec::new();
        room.step_into(now, &mut out, |s, snap, b| got.push((s, *snap, b.len())));
        // Where everyone was when the snapshots were made.
        let fighters = room.world.fighters.to_vec();
        for (s, snap, n) in got {
            acks[s as usize] = snap.tick;
            bytes += n;
            all.push((s, snap, fighters.clone()));
        }
    }
    (bytes, all)
}

#[test]
fn a_big_map_holds_far_ones_briefly_and_an_arena_never() {
    for d in sim::maps::ALL {
        let (bytes, snaps) = play(d, true);
        let mut run = [[0u8; MAX_FIGHTERS]; MAX_FIGHTERS];
        let mut held = 0;
        for (s, snap, fighters) in &snaps {
            let (s, me) = (*s as usize, &fighters[*s as usize]);
            for q in 0..MAX_FIGHTERS {
                if snap.held & (1 << q) == 0 {
                    run[s][q] = 0;
                    continue;
                }
                held += 1;
                run[s][q] += 1;
                let far = (fighters[q].body.pos - me.body.pos).len();
                assert!(far > FAR_M - 2.0, "{}: {q} held for {s} at {far} m", d.name);
                assert!(run[s][q] <= 3, "{}: {q} held {} snapshots running", d.name, run[s][q]);
            }
        }
        if !d.big() {
            assert_eq!(held, 0, "{}: an arena held someone", d.name);
            continue;
        }
        let (every, _) = play(d, false);
        let mut buf = [0u8; MAX_DATAGRAM];
        let whole: usize = snaps
            .iter()
            .map(|(_, s, _)| encode_snapshot(s, &mut buf).unwrap())
            .sum();
        eprintln!(
            "{}: {held} held; {} KB sent, {} KB updating everyone every snapshot ({:.0}%); {} KB whole",
            d.name,
            bytes / 1024,
            every / 1024,
            100.0 * bytes as f64 / every as f64,
            whole / 1024
        );
        assert!(held > 1000, "{}: only {held} held", d.name);
        assert!(bytes < every, "{}: holding saved nothing", d.name);
    }
}

/// Hulls in a bubble go exactly, with their sticks: on Dry Fork, with
/// everyone a bot that drives, it costs little more than coarse.
#[test]
fn a_bubble_costs_little() {
    let def = sim::maps::ALL.iter().find(|d| d.id == 3).expect("Dry Fork");
    let (on, snaps) = play_with(def, true, true);
    let (off, _) = play_with(def, true, false);
    let near: usize = snaps
        .iter()
        .map(|(_, s, _)| s.vehicles().iter().filter(|v| v.near.is_some()).count())
        .sum();
    eprintln!(
        "{}: {near} hulls sent near; {} KB with bubbles, {} KB without ({:+.1}%)",
        def.name,
        on / 1024,
        off / 1024,
        100.0 * (on as f64 / off as f64 - 1.0)
    );
    assert!(near > 1000, "only {near} hulls sent near");
    assert!((on as f64) < off as f64 * 1.15, "{on} bytes against {off}");
}

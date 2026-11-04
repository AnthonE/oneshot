//! A welcome starts a new room's clock: a snapshot from the room before it
//! (a cup's last heat, still sending as the next one seats us) must not
//! become the newest, or every snapshot of the new room looks old.

use client_net::core::NetCore;
use proto::Snapshot;
use sim::consts::TICK_HZ;
use sim::map::Map;
use sim::world::Rules;

const TICK_MS: f64 = 1000.0 / TICK_HZ as f64;

fn snap(tick: u32) -> Snapshot {
    Snapshot {
        tick,
        phase: 1,
        ..Snapshot::default()
    }
}

#[test]
fn the_last_heats_snapshot_is_not_the_new_heats_first() {
    // The old heat ran ten minutes and lingered; the new one starts at 40.
    let mut c = NetCore::new(0, 0, 40, Rules::duel(), Map::proving_grounds());
    c.on_snapshot(0.0, &snap(80_000));
    for k in 0..20 {
        let t = 42 + 2 * k;
        c.on_snapshot(t as f64 * TICK_MS, &snap(t));
    }
    assert_eq!(c.latest().tick, 42 + 2 * 19, "the new heat's snapshots are drawn");
}

#[test]
fn a_welcome_late_in_a_long_room_takes_its_snapshots() {
    // Joining a free for all an hour in: the welcome's tick is the room's.
    let at = 128 * 3600;
    let mut c = NetCore::new(3, 0, at, Rules::ffa(), Map::proving_grounds());
    c.on_snapshot(0.0, &snap(at + 2));
    c.on_snapshot(2.0 * TICK_MS, &snap(at + 4));
    assert_eq!(c.latest().tick, at + 4);
}

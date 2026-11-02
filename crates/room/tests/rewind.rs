//! The rewind depth is the server's: how old the client's newest acked
//! snapshot is, plus a clamped playout, capped at 160 ms. Nothing a client
//! claims can raise it.

use proto::{InputMsg, Name};
use room::{ClientNet, Room, MAX_FAVOUR_MS, PLAYOUT_MAX};
use sim::consts::ticks;
use sim::input::Input;
use sim::map::Map;
use sim::world::Rules;

fn msg(ack: u32, playout: u8, seq: u32) -> InputMsg {
    let mut m = InputMsg {
        ack_tick: ack,
        playout,
        first_seq: seq,
        n: 1,
        ..InputMsg::default()
    };
    m.frames[0] = Input::default();
    m
}

#[test]
fn depth_is_ack_age_plus_playout() {
    let mut c = ClientNet::default();
    c.on_input(&msg(100, 4, 0), 0.0, Some(100));
    assert_eq!(c.rewind_q8(106), (6 + 4) << 8);
}

#[test]
fn an_old_ack_claimed_later_changes_nothing() {
    let mut c = ClientNet::default();
    c.on_input(&msg(100, 4, 0), 0.0, Some(100));
    c.on_input(&msg(40, 4, 1), 0.0, Some(102));
    assert_eq!(c.acked, Some(100));
    assert_eq!(c.rewind_q8(106), (6 + 4) << 8);
}

#[test]
fn an_ack_of_a_snapshot_never_sent_is_ignored() {
    let mut c = ClientNet::default();
    c.on_input(&msg(500, 4, 0), 0.0, Some(100));
    assert_eq!(c.acked, None);
    assert_eq!(c.rewind_q8(106), 0);
}

#[test]
fn playout_and_the_total_are_capped() {
    let mut c = ClientNet::default();
    c.on_input(&msg(100, 255, 0), 0.0, Some(100));
    assert_eq!(c.rewind_q8(101), (1 + PLAYOUT_MAX) << 8);
    assert_eq!(c.rewind_q8(400), ticks(MAX_FAVOUR_MS) << 8);
}

#[test]
fn a_client_cannot_step_faster_than_the_server() {
    let mut room = Room::new(Map::proving_grounds(), Rules::duel(), 1, (1, 2));
    let (slot, _) = room.join(Name::new("speed")).unwrap();
    // Two seconds of frames at once: the server still runs one a tick and
    // keeps at most a few waiting.
    let mut m = InputMsg {
        first_seq: 1000,
        n: 16,
        ..InputMsg::default()
    };
    for f in m.frames.iter_mut() {
        f.mz = 127;
    }
    for k in 0..16 {
        m.first_seq = 1000 + 16 * k;
        room.on_input(slot, &m, 0.0);
    }
    let before = room.world.tick;
    room.step(0.0);
    assert_eq!(room.world.tick, before + 1);
    let d = room.client(slot).unwrap().depth();
    assert!(d <= room::MAX_DEPTH as i8, "{d}");
}

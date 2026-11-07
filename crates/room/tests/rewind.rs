//! The rewind depth is the server's: how old the client's newest acked
//! snapshot was when it sent the frame being run, plus the playout it drew
//! at then, clamped, capped at 160 ms. Nothing a client claims can raise
//! it.

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

/// The depth for the next frame run at `tick`, as `Room::step` takes it.
fn run(c: &mut ClientNet, tick: u32) -> u32 {
    c.take_frame();
    c.rewind_q8(tick)
}

#[test]
fn depth_is_ack_age_plus_playout() {
    let mut c = ClientNet::default();
    c.on_input(&msg(100, 4, 0), 0.0, Some(100));
    assert_eq!(run(&mut c, 106), (6 + 4) << 8);
}

#[test]
fn an_old_ack_claimed_later_changes_nothing() {
    let mut c = ClientNet::default();
    c.on_input(&msg(100, 4, 0), 0.0, Some(100));
    c.on_input(&msg(40, 4, 1), 0.0, Some(102));
    assert_eq!(c.acked, Some(100));
    assert_eq!(run(&mut c, 106), (6 + 4) << 8);
    assert_eq!(run(&mut c, 107), (7 + 4) << 8);
}

#[test]
fn each_frame_is_rewound_to_what_was_seen_when_it_was_sent() {
    // Frame 0 sent having seen snapshot 100 drawn 4 behind; frame 1 (with
    // 0 again, as every datagram carries what isn't acked) having seen 102
    // drawn 3 behind. Both wait, then run on 106 and 107.
    let mut c = ClientNet::default();
    c.on_input(&msg(100, 4, 0), 0.0, Some(100));
    let mut m = msg(102, 3, 0);
    m.n = 2;
    c.on_input(&m, 0.0, Some(102));
    assert_eq!(run(&mut c, 106), (6 + 4) << 8);
    assert_eq!(run(&mut c, 107), (5 + 3) << 8);
    // One that never came is run as a repeat, with the newest view.
    assert_eq!(run(&mut c, 108), (6 + 3) << 8);
}

#[test]
fn an_ack_of_a_snapshot_never_sent_is_ignored() {
    let mut c = ClientNet::default();
    c.on_input(&msg(500, 4, 0), 0.0, Some(100));
    assert_eq!(c.acked, None);
    assert_eq!(run(&mut c, 106), 0);
}

#[test]
fn playout_and_the_total_are_capped() {
    let mut c = ClientNet::default();
    c.on_input(&msg(100, 255, 0), 0.0, Some(100));
    c.take_frame();
    assert_eq!(c.rewind_q8(101), (1 + PLAYOUT_MAX) << 8);
    assert_eq!(c.rewind_q8(400), ticks(MAX_FAVOUR_MS) << 8);
}

#[test]
fn a_client_cannot_step_faster_than_the_server() {
    let mut room = Room::new(Map::proving_grounds(), Rules::duel(), 1, (1, 2));
    let (slot, _) = room.join(Name::new("speed"), proto::Look::default()).unwrap();
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

#[test]
fn a_joiner_acking_nothing_yet_gets_no_rewind() {
    // Seated mid-match: its first datagrams say `ack_tick: 0` (it has no
    // snapshot yet), which must not read as a view from the first tick.
    let mut room = Room::new(Map::proving_grounds(), Rules::duel(), 1, (1, 2));
    for t in 0..300 {
        room.step(t as f64);
    }
    let joined = room.world.tick;
    let (slot, _) = room.join(Name::new("late"), proto::Look::default()).unwrap();
    room.on_input(slot, &msg(0, 4, 0), 300.0);
    room.on_input(slot, &msg(joined, 4, 1), 300.0);
    let c = room.client(slot).unwrap();
    assert_eq!(c.acked, None, "an ack from before it sat is refused");
    assert_eq!(run(c, joined + 1), 0);
}

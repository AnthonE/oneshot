//! A frame that hasn't come: the last that did, held for jitter's worth,
//! then letting go of the triggers and jump and coming to a stop, taps
//! still held so the next real frame can't press them twice.

use proto::InputMsg;
use room::{ClientNet, LOST_HOLD_MS, LOST_STOP_MS};
use sim::consts::ticks;
use sim::input::{Input, CROUCH, FIRE, JUMP, SWAP};

fn send(c: &mut ClientNet, seq: u32, f: Input) {
    let mut m = InputMsg {
        first_seq: seq,
        n: 1,
        ..InputMsg::default()
    };
    m.frames[0] = f;
    c.on_input(&m, 0.0, None);
}

#[test]
fn a_lost_player_stops_and_lets_go() {
    let held = Input {
        buttons: FIRE | JUMP | CROUCH | SWAP,
        yaw: 12_000,
        pitch: -400,
        mx: 127,
        mz: -127,
    };
    let mut c = ClientNet::default();
    send(&mut c, 0, held);
    assert_eq!(c.take_frame(), held);
    let (hold, stop) = (ticks(LOST_HOLD_MS), ticks(LOST_STOP_MS));
    for n in 1..=hold {
        assert_eq!(c.take_frame(), held, "held as it was, tick {n}");
    }
    let mut was = 127;
    for n in 1..=stop {
        let f = c.take_frame();
        assert_eq!(
            f.buttons,
            CROUCH | SWAP,
            "tick {n}: triggers and jump let go, taps and crouch kept"
        );
        assert_eq!((f.yaw, f.pitch), (held.yaw, held.pitch));
        assert!(
            f.mx < was && f.mz == -f.mx,
            "tick {n}: easing to a stop ({} after {was})",
            f.mx
        );
        was = f.mx;
    }
    assert_eq!((was, c.take_frame().mx), (0, 0), "stopped, and stays stopped");

    // The next frame runs as it came: the stall is over.
    let back = Input { mx: 64, ..held };
    let next = c.last_seq + 1;
    send(&mut c, next, back);
    assert_eq!(c.take_frame(), back);
    assert_eq!(c.take_frame(), back, "and a fresh gap starts held again");
}

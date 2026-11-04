//! A tap reaches exactly one tick: pressed and let go between two ticks
//! (a fast frame rate makes frames with no tick), it still goes in the
//! next; held over a frame of several ticks, it goes down once.

use std::collections::BTreeMap;

use client_net::core::NetCore;
use sim::input::{Input, SWAP};
use sim::map::Map;
use sim::world::Rules;

/// Runs `frames` frames `ms` apart, the button down on the frames `down`
/// says, and returns every tick's buttons the client sent, by seq.
fn sent(ms: f64, frames: usize, down: impl Fn(usize) -> bool) -> BTreeMap<u32, u16> {
    let mut c = NetCore::new(0, 0, 0, Rules::duel(), Map::open());
    let mut ticks = BTreeMap::new();
    for i in 0..frames {
        let inp = Input {
            buttons: if down(i) { SWAP } else { 0 },
            ..Input::default()
        };
        for m in c.update(i as f64 * ms, inp) {
            for k in 0..m.n as u32 {
                ticks.insert(m.first_seq + k, m.frames[k as usize].buttons);
            }
        }
    }
    ticks
}

/// How many times the button went down across the ticks.
fn presses(ticks: &BTreeMap<u32, u16>) -> usize {
    let mut last = 0;
    let mut n = 0;
    for &b in ticks.values() {
        if b & SWAP != 0 && last & SWAP == 0 {
            n += 1;
        }
        last = b;
    }
    n
}

#[test]
fn a_tap_between_two_ticks_still_goes_in_once() {
    // 500 fps: four frames to a tick. Down on one frame only, for every
    // phase against the ticks.
    for at in 100..108 {
        let t = sent(2.0, 200, |i| i == at);
        assert_eq!(presses(&t), 1, "pressed on frame {at}");
        assert_eq!(t.values().filter(|b| *b & SWAP != 0).count(), 1, "one tick of it");
    }
}

#[test]
fn a_tap_held_over_a_slow_frame_goes_down_once() {
    // 30 fps: four ticks a frame. Down for one frame.
    let t = sent(1000.0 / 30.0, 60, |i| i == 30);
    assert_eq!(presses(&t), 1);
}

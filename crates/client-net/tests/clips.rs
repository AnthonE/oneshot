//! The clip key's tape: cut mid-stream, it replays every snapshot after the
//! cut into a watcher, who knows every seat from its first frame and draws
//! the player's own fighter off the snapshots.

use client_net::clip::{KEEP_SEGS, LEAD_S, SEG_MS};
use client_net::session::{Session, Wire};
use client_net::tape::{self, Lane, Replay};
use proto::{encode_snapshot, Msg, Name, Snapshot, ACCESSORY_NONE, EMBLEM_NONE, MAX_DATAGRAM, SKIN_STANDARD};
use sim::consts::TICK_HZ;
use sim::fighter::Fighter;
use sim::geom::V3;
use sim::input::Input;
use sim::movement::Body;
use sim::world::Rules;

const TICK_MS: f64 = 1000.0 / TICK_HZ as f64;

struct Drop;

impl Wire for Drop {
    fn send_datagram(&mut self, _: &[u8]) -> bool {
        true
    }
}

fn welcome(tick: u32) -> Msg {
    Msg::Welcome {
        slot: 0,
        team: 0,
        tick,
        tick_hz: TICK_HZ as u16,
        map: sim::maps::ALL[0].id,
        rules: Rules::duel(),
        salt_commit: [7; 32],
    }
}

fn seat(slot: u8, name: &str) -> Msg {
    Msg::Seat {
        slot,
        team: slot & 1,
        bot: true,
        name: Name::new(name),
        skin: SKIN_STANDARD,
        emblem: EMBLEM_NONE,
        accessory: ACCESSORY_NONE,
    }
}

/// Tick `tick`'s snapshot: we walk along x at a metre a second.
fn wire(tick: u32) -> Vec<u8> {
    let s = Snapshot {
        tick,
        phase: 1,
        own: Fighter {
            seated: true,
            alive: true,
            yaw: (tick * 16) as u16,
            body: Body {
                pos: V3::new(tick as f32 / TICK_HZ as f32, 0.0, 3.0),
                vel: V3::new(1.0, 0.0, 0.0),
                grounded: true,
                ..Body::default()
            },
            ..Fighter::default()
        },
        ..Snapshot::default()
    };
    let mut buf = [0u8; MAX_DATAGRAM];
    let n = encode_snapshot(&s, &mut buf).unwrap();
    buf[..n].to_vec()
}

/// A live session `secs` into a match that began at tick `t0`, snapshots
/// every two ticks, a seat changed a quarter of the way in.
fn live(t0: u32, secs: f64) -> (Session, u32) {
    let mut s = Session::new();
    s.on_msg(welcome(t0));
    s.on_msg(seat(0, "you"));
    s.on_msg(seat(1, "early"));
    let n = (secs * 1000.0 / (2.0 * TICK_MS)) as u32;
    let mut tick = t0;
    for k in 1..=n {
        tick = t0 + 2 * k;
        let at = k as f64 * 2.0 * TICK_MS;
        if k == n / 4 {
            s.on_msg(seat(1, "renamed"));
        }
        s.on_datagram(at, &wire(tick));
        s.update(at, Input::default(), &mut Drop);
    }
    (s, tick)
}

#[test]
fn a_clip_cut_mid_stream_replays_every_snapshot_into_a_watcher() {
    // Ten minutes in: the welcome's tick is long gone by the cut.
    let t0 = 128 * 600;
    let (s, last) = live(t0, SEG_MS * 2.5 / 1000.0);
    assert!(s.clip_held_s() > SEG_MS * 2.0 / 1000.0);
    let bytes = s.clip_tape(5.0).expect("a clip");
    let t = tape::read(&bytes).unwrap();
    // At least what was asked and the lead, and not the whole of it.
    assert!(t.end_ms() >= (5.0 + LEAD_S) * 1000.0 - 2.0 * TICK_MS, "{}", t.end_ms());
    assert!(t.end_ms() < SEG_MS * 2.0);
    // It opens on the welcome, its tick the cut's, and every seat as it
    // stood then, at 0.
    let Some(Msg::Welcome { tick, .. }) = t.welcome() else {
        panic!("no welcome");
    };
    // The cut is the start of the segment that holds the ask and its lead:
    // 37.5 s in, 9 s back is in the second, so 15 s after the welcome, and
    // after the seat changed (9.4 s).
    let seg_ticks = (SEG_MS / TICK_MS) as u32;
    assert!(tick >= t0 + seg_ticks, "the welcome is re-keyed to the cut: {tick}");
    let opening: Vec<Msg> = t
        .entries
        .iter()
        .take_while(|e| e.t_ms == 0.0 && e.lane == Lane::Msg)
        .filter_map(|e| proto::decode_msg(&e.bytes).ok())
        .collect();
    assert!(opening.contains(&seat(0, "you")));
    assert!(opening.contains(&seat(1, "renamed")), "{opening:?}");
    assert!(!opening.contains(&seat(1, "early")));

    let on_tape = t.entries.iter().filter(|e| e.lane == Lane::Datagram).count() as u32;
    let mut w = Session::replay();
    let mut r = Replay::new(t);
    let end = r.end_ms();
    while r.clock_ms < end {
        r.clock_ms = (r.clock_ms + 1000.0 / 60.0).min(end);
        r.frame(&mut w);
    }
    assert!(r.done());
    assert!(w.ready && w.core.watch);
    assert_eq!(w.core.stats.snapshots, on_tape, "every snapshot on the tape is drawn");
    assert_eq!(w.core.latest().tick, last);
    // The player's own fighter, off the snapshots: where it walked, a
    // little behind the newest, never predicted, and nothing sent.
    let (f, p) = w.core.own();
    assert!(f.alive);
    let newest_x = last as f32 / TICK_HZ as f32;
    assert!(p.x <= newest_x + 0.2 && p.x > newest_x - 0.5, "{} vs {newest_x}", p.x);
    assert_eq!(w.core.stats.corrections, 0);
}

#[test]
fn a_long_ask_takes_the_oldest_segment_and_runs_forward() {
    let (s, _) = live(40, SEG_MS * (KEEP_SEGS as f64 + 2.0) / 1000.0);
    let held = s.clip_held_s();
    assert!(held <= KEEP_SEGS as f64 * SEG_MS / 1000.0 + 0.1, "{held}");
    let t = tape::read(&s.clip_tape(999.0).unwrap()).unwrap();
    assert!((t.end_ms() / 1000.0 - held).abs() < 0.2);
    let mut last = 0.0;
    for e in &t.entries {
        assert!(e.t_ms >= last, "the tape runs forward");
        last = e.t_ms;
    }
}

#[test]
fn a_new_welcome_starts_the_memory_again() {
    let (mut s, _) = live(40, 20.0);
    assert!(s.clip_held_s() > 15.0);
    s.on_msg(welcome(200_000));
    assert_eq!(s.clip_held_s(), 0.0);
    assert!(s.clip_tape(10.0).is_none(), "nothing of the new room yet");
}

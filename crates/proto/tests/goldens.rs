//! Every message round-trips, and its bytes are pinned. A golden that
//! moves is a wire change: bump `PROTO_VER` in the same commit and paste the
//! new value (the failure prints it).

use proto::*;
use sim::arms::{BR, MAGNUM};
use sim::consts::MAX_FIGHTERS;
use sim::fighter::{Fighter, Gun};
use sim::geom::V3;
use sim::input::Input;
use sim::movement::Body;
use sim::world::Rules;

fn fnv(b: &[u8]) -> u64 {
    let mut h = sim::hash::Fnv::default();
    for &x in b {
        h.u8(x);
    }
    h.finish()
}

fn input() -> InputMsg {
    let mut m = InputMsg {
        ack_tick: 1234,
        ack_bits: 0xF0F0_0F0F,
        playout: 4,
        first_seq: 99_000,
        n: 3,
        ..InputMsg::default()
    };
    for i in 0..3 {
        m.frames[i] = Input {
            buttons: 0x1A5 >> i,
            yaw: 40_000 + i as u16,
            pitch: -1200 + i as i16,
            mx: -127,
            mz: 100,
        };
    }
    m
}

fn own() -> Fighter {
    Fighter {
        seated: true,
        team: 1,
        alive: true,
        body: Body {
            pos: V3::new(-12.5, 2.5, 3.0 + 1.0 / 1024.0),
            vel: V3::new(6.0, -3.5, 0.0),
            grounded: false,
            crouched: true,
        },
        yaw: 12_345,
        pitch: -555,
        shield: 30 * 256,
        health: 100 * 256,
        guns: [
            Gun {
                arm: BR,
                mag: 30,
                reserve: 90,
            },
            Gun {
                arm: MAGNUM,
                mag: 8,
                reserve: 40,
            },
        ],
        cur: 1,
        refire: 7,
        reload: 0,
        burst_left: 2,
        burst_gap: 3,
        melee_cool: 11,
        gren_cool: 0,
        grenades: 3,
        shots: 777,
        last_buttons: 0x41,
        respawn: 0,
        kills: 4,
        deaths: 2,
        ..Fighter::default()
    }
}

fn snapshot() -> Snapshot {
    let mut s = Snapshot {
        tick: 50_000,
        last_seq: 99_001,
        depth: 2,
        own: own(),
        n_remote: 1,
        n_proj: 1,
        n_events: 2,
        n_pads: 2,
        phase: 1,
        clock: 70_000,
        winner: 255,
        ..Snapshot::default()
    };
    s.remotes[0] = Remote {
        slot: 1,
        team: 0,
        alive: true,
        crouched: false,
        grounded: true,
        reloading: true,
        pos: V3::new(10.0, 0.0, -4.25),
        vel: V3::new(-7.0, 0.0, 0.5),
        yaw: 32_768,
        pitch: 100,
        arm: BR,
        shield: 200,
    };
    s.proj[0] = ProjState {
        kind: 0,
        owner: 1,
        pos: V3::new(1.0, 1.5, 2.0),
        vel: V3::new(36.0, 0.0, 0.0),
    };
    s.events[0] = EventState {
        id: 41,
        kind: 1,
        a: 1,
        b: 0,
        part: 0,
        c: BR as u16,
        pos: V3::new(10.0, 1.625, -4.25),
        end: V3::new(-12.0, 3.0, 3.0),
    };
    s.events[1] = EventState {
        kind: 2,
        a: 1,
        b: 0,
        part: 1,
        c: 9,
        ..EventState::default()
    };
    s.pads[1].empty_s = 45;
    // Riding: in the passenger's seat of vehicle 2, sent exactly; and
    // vehicle 5 driven by seat 1.
    s.own.ride = 6;
    s.own_vehicle = Some(sim::vehicle::Vehicle {
        active: true,
        pos: V3::new(-40.0, 1.0390625, -19.5),
        vel: V3::new(12.5, 0.0, -0.25),
        yaw: 1000,
        grounded: true,
        health: 200_000,
        seats: [0, 1],
        respawn: 0,
        home: 2,
    });
    s.n_vehicles = 1;
    s.vehicles[0] = VehState {
        slot: 5,
        pos: V3::new(20.0, 0.5, 12.0),
        vel: V3::new(-15.0, 0.0, 3.0),
        yaw: 40_000,
        health: 128,
        seats: [2, 0],
    };
    s.n_score = 2;
    s.score[0] = 3;
    s.score[1] = -1;
    s
}

fn msgs() -> Vec<Msg> {
    vec![
        Msg::Hello {
            proto: PROTO_VER,
            name: Name::new("player one"),
            pass: Pass::NONE,
            mode: MODE_CASUAL,
            skin: SKIN_STANDARD,
        },
        Msg::Hello {
            proto: PROTO_VER,
            name: Name::new("ranked"),
            pass: Pass::new("ZWxvIGFjY291bnQ.c2ln-_").unwrap(),
            mode: MODE_DUEL,
            skin: 2,
        },
        Msg::Queued {
            mode: MODE_DOUBLES,
            waiting_s: 42,
        },
        Msg::Welcome {
            slot: 1,
            team: 1,
            tick: 384,
            tick_hz: 128,
            map: 0,
            rules: Rules::duel(),
            salt_commit: salt_commit([7, 9]),
        },
        Msg::Welcome {
            slot: 5,
            team: 1,
            tick: 9000,
            tick_hz: 128,
            map: 2,
            rules: Rules::gun_game(),
            salt_commit: salt_commit([1, 2]),
        },
        Msg::Hello {
            proto: PROTO_VER,
            name: Name::new("ffa"),
            pass: Pass::NONE,
            mode: MODE_GUN_GAME,
            skin: 5,
        },
        Msg::Refuse { reason: REFUSE_FULL },
        Msg::Seat {
            slot: 0,
            team: 0,
            bot: true,
            name: Name::new("BOT"),
            skin: SKIN_STANDARD,
        },
        Msg::Seat {
            slot: 6,
            team: 0,
            bot: false,
            name: Name::new("ember"),
            skin: 2,
        },
        Msg::Kill {
            killer: 0,
            victim: 1,
            cause: 102,
        },
        Msg::Phase { phase: 2, winner: 0 },
        Msg::Cup {
            round: 2,
            rounds: 3,
            entrants: 21,
            place: 4,
            points: 17,
            done: false,
        },
        Msg::Bye,
        Msg::Reveal {
            salt: [0x0123_4567_89ab_cdef, u64::MAX],
        },
    ]
}

#[test]
fn inputs_round_trip() {
    let m = input();
    let mut buf = [0u8; MAX_DATAGRAM];
    let n = encode_input(&m, &mut buf).unwrap();
    assert_eq!(decode_input(&buf[..n]).unwrap(), m);
    assert_eq!(datagram_kind(&buf[..n]), Some(DG_INPUT));
    // Sixteen frames still fit a datagram with room to spare.
    let full = InputMsg { n: 16, ..m };
    assert!(encode_input(&full, &mut buf).unwrap() < 200);
}

#[test]
fn snapshots_round_trip_and_the_own_fighter_is_exact() {
    let s = snapshot();
    let mut buf = [0u8; MAX_DATAGRAM];
    let n = encode_snapshot(&s, &mut buf).unwrap();
    let d = decode_snapshot(&buf[..n]).unwrap();
    assert_eq!(
        d.own.body, s.own.body,
        "positions on the sim's grid come back bit for bit"
    );
    assert_eq!(d.own.guns, s.own.guns);
    assert_eq!(d.own.shots, s.own.shots);
    assert_eq!(d.events(), s.events());
    assert_eq!(d.remotes()[0].pos, s.remotes[0].pos);
    assert_eq!(d.score, s.score);
    assert_eq!(d.own.ride, s.own.ride);
    assert_eq!(d.own_vehicle, s.own_vehicle, "the ridden vehicle comes back exactly");
    assert_eq!(d.vehicles()[0].seats, s.vehicles[0].seats);
    assert_eq!(datagram_kind(&buf[..n]), Some(DG_SNAPSHOT));
    // Sixteen seats, every projectile and pad: fits with room for events.
    // (room::build drops events past the datagram, so a full snapshot of
    // everything is never sent.)
    let full = Snapshot {
        n_remote: MAX_REMOTES as u8,
        n_proj: MAX_SNAP_PROJ as u8,
        n_events: 0,
        n_pads: MAX_SNAP_PADS as u8,
        n_vehicles: MAX_SNAP_VEHICLES as u8,
        n_score: MAX_FIGHTERS as u8,
        ..s
    };
    let mut big = [0u8; 4 * MAX_DATAGRAM];
    let n = encode_snapshot(&full, &mut big).unwrap();
    assert!(n + 200 <= MAX_DATAGRAM, "{n} bytes before events");
}

#[test]
fn stream_messages_round_trip() {
    for m in msgs() {
        let mut buf = [0u8; MAX_STREAM_MSG];
        let n = encode_msg(&m, &mut buf).unwrap();
        assert_eq!(decode_msg(&buf[..n]).unwrap(), m);
    }
}

#[test]
fn decoders_refuse_garbage() {
    let mut buf = [0u8; MAX_DATAGRAM];
    let n = encode_input(&input(), &mut buf).unwrap();
    assert!(decode_input(&buf[..n - 1]).is_err(), "truncated");
    assert!(decode_input(&buf[..n + 1]).is_err(), "trailing byte");
    assert!(decode_snapshot(&buf[..n]).is_err(), "wrong kind");
    assert!(decode_msg(&[99]).is_err());
}

#[test]
fn the_bytes_match_their_goldens() {
    let mut buf = [0u8; MAX_DATAGRAM];
    let mut got = Vec::new();
    let n = encode_input(&input(), &mut buf).unwrap();
    got.push(fnv(&buf[..n]));
    let n = encode_snapshot(&snapshot(), &mut buf).unwrap();
    got.push(fnv(&buf[..n]));
    for m in msgs() {
        let n = encode_msg(&m, &mut buf).unwrap();
        got.push(fnv(&buf[..n]));
    }
    let want: &[u64] = &[
        0x21cd_ceaa_a773_0091,
        0x5ae2_2de8_84e4_3501,
        0x7782_079f_49c8_e430,
        0x3330_9d27_c0f6_c857,
        0x7ebe_34b8_2c10_f2d0,
        0x3a33_e7d9_9d0c_85a6,
        0xb8d3_9fbc_28b2_b2b7,
        0x0554_fbd6_e8f5_1d99,
        0x0835_ec07_b4ee_4fb0,
        0x9e6a_3369_2998_8329,
        0x227f_c9e3_d4c4_2705,
        0xb77f_6e56_3cf6_3da6,
        0xc800_e818_6244_f8b3,
        0x522b_412f_5f51_3ede,
        0xaf63_ba4c_8601_b2c6,
        0xdfb3_0ed7_79ac_056f,
    ];
    assert_eq!(got, want, "PROTO_VER {PROTO_VER}: got {got:#018x?}");
}

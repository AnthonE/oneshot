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
        playout: 4,
        first_seq: 99_000,
        n: 3,
        ..InputMsg::default()
    };
    for i in 0..3 {
        m.frames[i] = Input {
            // FIRE_LEFT and GREN_SWITCH (bits 9, 10) among them.
            buttons: 0x6A5 >> i,
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
            duck: 9,
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
                heat: 0,
            },
            Gun {
                arm: MAGNUM,
                mag: 8,
                reserve: 40,
                heat: 1234,
            },
        ],
        cur: 1,
        refire: 7,
        reload: 0,
        burst_left: 2,
        burst_gap: 3,
        melee_cool: 11,
        charge: 21,
        left: Gun {
            arm: sim::arms::SMG,
            mag: 40,
            reserve: 100,
            heat: 0,
        },
        left_refire: 4,
        left_reload: 0,
        left_charge: 0,
        stickies: 1,
        gren_kind: 1,
        gren_cool: 0,
        grenades: 3,
        shots: 777,
        last_buttons: 0x41,
        respawn: 0,
        kills: 4,
        deaths: 2,
        charging: true,
        camo: 3000,
        flare: 40,
        ..Fighter::default()
    }
}

/// One step of the sim's grid (`sim::consts::QUANT`): what goes exactly.
const G: f32 = 1.0 / 1024.0;

fn snapshot() -> Snapshot {
    let mut s = Snapshot {
        tick: 50_000,
        last_seq: 99_001,
        depth: 2,
        own: own(),
        n_remote: 2,
        n_proj: 1,
        n_events: 3,
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
        over: true,
        cloak: 5,
        flag: true,
        near: None,
    };
    // One in our bubble by a hull: exact, with what it last ran.
    s.remotes[1] = Remote {
        slot: 4,
        team: 1,
        alive: true,
        grounded: false,
        pos: V3::new(19.0 + G, 1.25, 13.5),
        vel: V3::new(-6.0, 4.5 + G, 0.25),
        yaw: 12_000,
        arm: BR,
        near: Some(NearBody {
            duck: 7,
            mx: -127,
            mz: 90,
            jump: true,
            crouch: true,
            share: 6,
        }),
        ..Remote::default()
    };
    // Capture the Flag: red's flag held by seat 1, blue's on the floor.
    s.n_flags = 2;
    s.flags[0] = FlagState {
        state: 1,
        carrier: 1,
        ..FlagState::default()
    };
    s.flags[1] = FlagState {
        state: 2,
        carrier: 0,
        pos: V3::new(18.5, 0.0, -2.0),
        down_s: 27,
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
    // A flag gone home on its own: nobody returned it.
    s.events[2] = EventState {
        id: 43,
        kind: sim::events::Kind::Flag as u8,
        a: NOBODY,
        b: 1,
        part: 2,
        pos: V3::new(30.0, 0.0, 4.0),
        ..EventState::default()
    };
    s.pads[1].empty_s = 45;
    // Riding: in the passenger's seat of vehicle 2, sent exactly, seat 2
    // at its wheel with its stick; and vehicle 5 driven by seat 1.
    s.own.ride = 6;
    s.own_vehicle = Some(sim::vehicle::Vehicle {
        active: true,
        pos: V3::new(-40.0, 1.0390625, -19.5),
        vel: V3::new(12.5, 0.0, -0.25),
        yaw: 1000,
        grounded: true,
        health: 200_000,
        seats: [3, 1],
        respawn: 0,
        home: 2,
        turret: sim::vehicle::MINIGUN,
    });
    s.drive = Some((-40, 127));
    // Gunning its minigun.
    s.own.mount = sim::fighter::Mount {
        spin: 45,
        heat: 3000,
        refire: 4,
        vent: false,
    };
    s.n_vehicles = 3;
    s.vehicles[0] = VehState {
        slot: 5,
        pos: V3::new(20.0, 0.5, 12.0),
        vel: V3::new(-15.0, 0.0, 3.0),
        yaw: 40_000,
        health: 128,
        seats: [2, 0],
        near: None,
    };
    // In our bubble: one driven (exact, its stick), one parked on a slope.
    s.vehicles[1] = VehState {
        slot: 6,
        pos: V3::new(-1000.0 - G, 3.5, 7000.25),
        vel: V3::new(21.0 - G, -0.5, -500.0),
        yaw: 9_000,
        health: 255,
        seats: [5, 0],
        near: Some(NearHull {
            grounded: true,
            stick: Some((-127, 64)),
            share: 15,
        }),
    };
    s.vehicles[2] = VehState {
        slot: 7,
        pos: V3::new(3.0, -2.0, 1.0),
        vel: V3::new(0.0, 0.0, 0.25),
        yaw: 1,
        health: 3,
        seats: [0, 0],
        near: Some(NearHull {
            grounded: false,
            stick: None,
            share: 0,
        }),
    };
    // A rocket launcher on the floor, and someone's grenades.
    s.n_drops = 2;
    s.drops[0] = DropState {
        slot: 3,
        arm: 6,
        pos: V3::new(4.0, 0.0, -7.5),
    };
    s.drops[1] = DropState {
        slot: 23,
        arm: 0,
        pos: V3::new(-12.25, 2.5, 1.0),
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
            emblem: EMBLEM_NONE,
            party: 0,
            stamp: 0x1234_5678,
            headwear: GEAR_NONE,
            footwear: GEAR_NONE,
            queued: proto::QueueMatch::NONE,
        },
        Msg::Hello {
            proto: PROTO_VER,
            name: Name::new("ranked"),
            pass: Pass::new("ZWxvIGFjY291bnQ.c2ln-_").unwrap(),
            mode: MODE_DUEL,
            skin: 2,
            emblem: emblem(16, 5, 3, 12),
            party: 0,
            stamp: 0,
            headwear: 1,
            footwear: 0,
            queued: proto::QueueMatch::new("q-Ab3_xYz9-LmN0pQr").unwrap(),
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
            emblem: emblem(1, 0, 15, 0),
            party: party::code_parse("K7Q2XD").unwrap(),
            stamp: u32::MAX,
            headwear: 0,
            footwear: 1,
            queued: proto::QueueMatch::NONE,
        },
        Msg::Refuse { reason: REFUSE_FULL },
        Msg::Moved {
            addr: proto::ServerAddr::new("eu2.game.example:61250").unwrap(),
            queued: proto::QueueMatch::new("q-Ab3_xYz9-LmN0pQr").unwrap(),
        },
        Msg::Seat {
            slot: 0,
            team: 0,
            bot: true,
            name: Name::new("BOT"),
            skin: SKIN_STANDARD,
            emblem: EMBLEM_NONE,
            headwear: GEAR_NONE,
            footwear: GEAR_NONE,
        },
        Msg::Seat {
            slot: 6,
            team: 0,
            bot: false,
            name: Name::new("ember"),
            skin: 2,
            emblem: emblem(3, 1, 9, 2),
            headwear: 1,
            footwear: 1,
        },
        Msg::Kill {
            killer: 0,
            victim: 1,
            cause: 102,
            head: true,
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
        Msg::Follow { slot: 13, team: 1 },
        Msg::Reveal {
            salt: [0x0123_4567_89ab_cdef, u64::MAX],
        },
        Msg::Medal {
            slot: 3,
            medal: medal::TRIPLE_KILL,
            victim: 14,
        },
        Msg::Carnage {
            slot: 15,
            kills: 25,
            deaths: 7,
            assists: 9,
            betrayals: 1,
            shots: 1312,
            hits: 655,
            heads: 31,
            damage: 4321,
            taken: 1789,
            best_spree: 11,
            best_multi: 3,
            medals: {
                let mut m = [0u8; medal::COUNT];
                m[medal::DOUBLE_KILL as usize] = 4;
                m[medal::KILLING_FRENZY as usize] = 1;
                m[medal::EXTERMINATION as usize] = 255;
                m
            },
        },
        Msg::Ticket {
            ticket: Pass::new("os1.Z3Vlc3Q.c2ln").unwrap(),
        },
        Msg::SignIn {
            pass: Pass::new("ZWxvIGFjY291bnQ.c2ln-_").unwrap(),
        },
        Msg::PartyAct {
            act: party::ACT_START,
            arg: MODE_BIG_TEAM as u32 | party::START_PRIVATE,
        },
        Msg::PartyInvite {
            player: party::PlayerId::new("0x1111111111111111111111111111111111111111").unwrap(),
        },
        Msg::Party(a_party()),
        Msg::Go {
            mode: MODE_FFA,
            private: true,
        },
        Msg::Friends { n: 2, reachable: true },
        Msg::Friend {
            i: 1,
            name: Name::new("chief"),
            player: party::PlayerId::new("acct_0123456789abcdef0123").unwrap(),
            at: party::AT_HERE,
            party: 77,
            size: 3,
            playing: party::PLAYING_NONE,
        },
        Msg::Invited {
            from: Name::new("arbiter"),
            code: 0x2aaa_aaaa,
        },
        Msg::Wardrobe(Wardrobe {
            wallet: true,
            known: true,
            skins: 0b10_0101,
            headwear: 0b11,
            footwear: 1,
            floor: Some(1612),
        }),
        Msg::Loaded,
    ]
}

/// A full party: eight members, a guest among them.
fn a_party() -> party::Party {
    let mut p = party::Party {
        code: party::code_parse("K7Q2XD").unwrap(),
        privacy: party::PRIVACY_FRIENDS,
        leader: 3,
        me: 0,
        n: party::MAX_PARTY as u8,
        members: [party::Member::EMPTY; party::MAX_PARTY],
    };
    for (i, m) in p.members.iter_mut().enumerate() {
        *m = party::Member {
            tag: i as u8,
            name: Name::new(&format!("member {i} long name")),
            player: if i == 5 {
                party::PlayerId::NONE
            } else {
                party::PlayerId::new(&format!("0x{:040x}", i + 1)).unwrap()
            },
            away: i == 2,
            playing: if i == 1 { MODE_DOUBLES } else { party::PLAYING_NONE },
        };
    }
    p
}

#[test]
fn a_full_party_fits_a_stream_message() {
    let mut buf = [0u8; MAX_STREAM_MSG];
    let n = encode_msg(&Msg::Party(a_party()), &mut buf).unwrap();
    assert!(n <= MAX_STREAM_MSG / 2, "{n} bytes");
}

#[test]
fn voice_frames_have_their_goldens() {
    let mut f = voice::VoiceFrame::new(300, voice::VOICE_END, &[9 << 3, 0xde, 0xad]).unwrap();
    let mut buf = [0u8; MAX_DATAGRAM];
    let up = voice::encode_voice_up(&f, &mut buf).unwrap();
    assert_eq!(datagram_kind(&buf[..up]), Some(DG_VOICE));
    let a = fnv(&buf[..up]);
    f.from = 5;
    let n = voice::encode_voice(&f, &mut buf).unwrap();
    let b = fnv(&buf[..n]);
    assert_eq!(
        [a, b],
        [0xf76d_1c0a_fc9a_e4cc, 0x90a3_e271_cfc2_d6d1],
        "got {:#018x?}",
        [a, b]
    );
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
    assert_eq!(d.events()[2].a, NOBODY, "nobody comes back as nobody");
    assert_eq!(d.remotes()[0].pos, s.remotes[0].pos);
    assert_eq!(d.score, s.score);
    assert_eq!(d.own.ride, s.own.ride);
    assert_eq!(d.own_vehicle, s.own_vehicle, "the ridden vehicle comes back exactly");
    assert_eq!(d.drive, s.drive, "and its driver's stick");
    // Our bubble's members come back exactly, with what they are
    // predicted on; the rest as coarse as ever.
    assert_eq!(d.remotes()[1], s.remotes[1], "a body in our bubble");
    assert_eq!(&d.vehicles()[1..], &s.vehicles[1..3], "hulls in our bubble");
    assert!(d.vehicles()[0].near.is_none());
    assert_eq!(d.vehicles()[0].seats, s.vehicles[0].seats);
    assert_eq!(d.drops(), s.drops());
    assert_eq!(d.flags(), s.flags());
    assert_eq!(datagram_kind(&buf[..n]), Some(DG_SNAPSHOT));
    // Sixteen seats, every projectile and pad, riding behind a driver,
    // hulls and a body in our bubble: fits with room for about eight
    // events. (room::build drops events past the datagram, so a full
    // snapshot of everything is never sent.)
    let full = Snapshot {
        n_remote: MAX_REMOTES as u8,
        n_proj: MAX_SNAP_PROJ as u8,
        n_events: 0,
        n_pads: MAX_SNAP_PADS as u8,
        n_vehicles: MAX_SNAP_VEHICLES as u8,
        n_drops: MAX_SNAP_DROPS as u8,
        n_score: MAX_FIGHTERS as u8,
        ..s
    };
    let mut big = [0u8; 4 * MAX_DATAGRAM];
    let n = encode_snapshot(&full, &mut big).unwrap();
    assert!(n + 160 <= MAX_DATAGRAM, "{n} bytes before events");
}

#[test]
fn stream_messages_round_trip() {
    for m in msgs() {
        let mut buf = [0u8; MAX_STREAM_MSG];
        let n = encode_msg(&m, &mut buf).unwrap();
        assert_eq!(decode_msg(&buf[..n]).unwrap_or_else(|e| panic!("{m:?}: {e:?}")), m);
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
    // A medal past the table, or none.
    let mut buf = [0u8; 64];
    for medal in [0, medal::COUNT as u8] {
        let n = encode_msg(
            &Msg::Medal {
                slot: 1,
                medal,
                victim: 2,
            },
            &mut buf,
        )
        .unwrap();
        assert!(decode_msg(&buf[..n]).is_err(), "medal {medal}");
    }
}

/// The golden snapshot two ticks before: our fighter a little back, the
/// clock back, a third remote, gone since, and our bubble as it was.
fn delta_base() -> Snapshot {
    let mut base = snapshot();
    base.tick -= 2;
    base.clock -= 1;
    base.own.body.pos.x -= 0.25;
    // Its driver steered the other way then.
    base.drive = Some((40, 127));
    // A third remote, gone since; the one in our bubble then a little back
    // and further in; the driven hull on another stick.
    base.n_remote = 3;
    base.remotes[2] = Remote {
        pos: V3::new(19.5, 1.0, 13.5),
        near: Some(NearBody {
            share: 7,
            ..base.remotes[1].near.unwrap()
        }),
        ..base.remotes[1]
    };
    base.remotes[1] = Remote {
        slot: 3,
        pos: V3::new(-30.0, 2.0, 8.0),
        alive: true,
        ..base.remotes[0]
    };
    base.vehicles[1].near = Some(NearHull {
        stick: Some((0, 127)),
        ..base.vehicles[1].near.unwrap()
    });
    base
}

/// The golden snapshot as a delta against [`delta_base`]: our fighter a
/// little along, the clock on, one remote gone.
fn encode_delta_golden(buf: &mut [u8]) -> usize {
    use proto::delta::{encode_delta, Keyed};
    let (mut k, mut b) = (Keyed::default(), Keyed::default());
    k.fill(&snapshot()).unwrap();
    b.fill(&delta_base()).unwrap();
    encode_delta(&k, &b, buf).unwrap()
}

#[test]
fn a_delta_round_trips_and_is_smaller() {
    use proto::delta::{decode_delta, Keyed};
    let mut buf = [0u8; MAX_DATAGRAM];
    let n = encode_delta_golden(&mut buf);
    let cur = snapshot();
    let mut b = Keyed::default();
    b.fill(&delta_base()).unwrap();
    let mut out = Keyed::default();
    decode_delta(&buf[..n], &b, &mut out).unwrap();
    let mut full = [0u8; MAX_DATAGRAM];
    let f = encode_snapshot(&cur, &mut full).unwrap();
    assert_eq!(out.snapshot().unwrap(), decode_snapshot(&full[..f]).unwrap());
    assert!(n * 2 < f, "{n} B as a delta, {f} B whole");
    // Against the wrong base, or cut short, it is refused.
    let mut wrong = b.clone();
    wrong.tick += 1;
    assert!(decode_delta(&buf[..n], &wrong, &mut out).is_err());
    assert!(decode_delta(&buf[..n - 1], &b, &mut out).is_err());
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
    let n = encode_delta_golden(&mut buf);
    got.push(fnv(&buf[..n]));
    let want: &[u64] = &[
        0x58ae_4fe0_5062_683d,
        0x8e87_a159_d44c_7450,
        0xeb3f_f214_3368_9e1f,
        0x3bb6_874d_51c8_0869,
        0x7ebe_34b8_2c10_f2d0,
        0x7ab6_78d6_f6ba_aa53,
        0x9571_1811_717b_689e,
        0x56f6_415a_e240_2f4e,
        0x0835_ec07_b4ee_4fb0,
        0x27e4_471c_ac0a_7864,
        0x0294_d707_7d31_6639,
        0xb3b0_e0c6_597a_e9cf,
        0xc3c6_2389_966a_c2c5,
        0xc800_e818_6244_f8b3,
        0x522b_412f_5f51_3ede,
        0xaf63_ba4c_8601_b2c6,
        0x0862_2707_b513_eedc,
        0xdfb3_0ed7_79ac_056f,
        0x26d2_6718_97d8_1f69,
        0xcf3f_7643_a904_811f,
        0x4627_86ee_7e74_8b4f,
        0x84e5_706c_e958_be03,
        0x519c_8b9c_47dc_7be6,
        0xba4d_2217_aeb8_fb47,
        0xb06e_95f9_23e4_b9c4,
        0x086f_a507_b51f_4f52,
        0x6c87_7218_bf71_5473,
        0x88d8_ddf9_f436_7356,
        0x8461_28ae_94ef_0f4c,
        0x451f_c0f0_80a6_2b96,
        0xaf63_d44c_8601_def4,
        0x3a44_1838_3809_2ff7,
    ];
    assert_eq!(got, want, "PROTO_VER {PROTO_VER}: got {got:#018x?}");
}

#[test]
fn an_emblem_packs_its_parts_and_no_glyph_is_none() {
    let e = emblem(16, 5, 3, 12);
    assert_eq!(emblem_parts(e), (16, 5, 3, 12));
    assert_eq!(emblem_valid(e), e);
    assert_eq!(emblem(0, 5, 3, 12), EMBLEM_NONE, "no glyph, no emblem");
    assert_eq!(emblem_valid(31 | 5 << 5), EMBLEM_NONE, "a glyph past the list is none");
    assert_eq!(GLYPH_FLOOR.len(), EMBLEM_GLYPHS.len());
}

#[test]
fn an_event_past_the_seats_names_nobody() {
    let mut s = snapshot();
    s.events[1].a = MAX_FIGHTERS as u8;
    s.events[1].b = NOBODY;
    let mut buf = [0u8; MAX_DATAGRAM];
    let n = encode_snapshot(&s, &mut buf).unwrap();
    let d = decode_snapshot(&buf[..n]).unwrap();
    assert_eq!((d.events[1].a, d.events[1].b), (NOBODY, NOBODY));
}

/// A bubble member the server could never send (a crouch past its full, a
/// stick at -128: `Input::sanitized` never runs one) is refused.
#[test]
fn an_impossible_bubble_member_is_refused() {
    let mut buf = [0u8; 4 * MAX_DATAGRAM];
    let base = snapshot();
    for bad in [
        |s: &mut Snapshot| s.remotes[1].near.as_mut().unwrap().duck = sim::consts::CROUCH_TICKS + 1,
        |s: &mut Snapshot| s.remotes[1].near.as_mut().unwrap().mx = i8::MIN,
        |s: &mut Snapshot| s.vehicles[1].near.as_mut().unwrap().stick = Some((0, i8::MIN)),
    ] {
        let mut s = base;
        bad(&mut s);
        let n = encode_snapshot(&s, &mut buf).unwrap();
        assert!(decode_snapshot(&buf[..n]).is_err(), "{:?}", s.remotes[1]);
    }
    assert!(exact_fits(V3::new(8191.0, 0.0, -8191.0), V3::new(511.0, 0.0, 0.0)));
    assert!(!exact_fits(V3::new(8192.0, 0.0, 0.0), V3::ZERO));
    assert!(!exact_fits(V3::ZERO, V3::new(0.0, -512.0, 0.0)));
}

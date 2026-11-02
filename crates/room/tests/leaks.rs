//! The leak gate (docs/DESIGN.md §17): whole matches (Doubles, and an
//! eight-seat Gun Game) with one person seat (played by a bot, through the same input path a client
//! uses), every snapshot and stream message it is sent checked against the
//! server's true world. Fails if the client is told where an enemy is when
//! it could not see them, if fog hides an enemy in plain sight, if an
//! unseen fighter's shot carries a place or a direction, or if any bit of
//! the salt crosses the wire before the match is over.

use std::collections::VecDeque;

use proto::{encode_msg, encode_snapshot, salt_commit, InputMsg, Msg, Name, Snapshot, MAX_DATAGRAM};
use room::fog::{plainly_sees, sees};
use room::{Room, HEARD, HEARD_GRID_M};
use sim::bot::Bot;
use sim::consts::{MAX_FIGHTERS, TICK_HZ};
use sim::events::Kind as Ev;
use sim::fighter::Fighter;
use sim::map::Map;
use sim::world::{Phase, Rules};

const SALT: (u64, u64) = (0x5a17_0123_4567_89ab, 0xfeed_beef_cafe_d00d);
/// The skin the person seat wears (the server checked it owns it).
const SKIN: u8 = 3;

/// Whether the 64 bits of `w` appear in `buf` at any bit offset (the wire
/// is bit-packed, so a leak need not be byte-aligned).
fn contains_bits(buf: &[u8], w: u64) -> bool {
    if buf.len() < 8 {
        return false;
    }
    for start in 0..=buf.len() - 8 {
        let mut window = 0u128;
        for (k, &b) in buf[start..(start + 9).min(buf.len())].iter().enumerate() {
            window |= (b as u128) << (8 * k);
        }
        for shift in 0..8 {
            if (window >> shift) as u64 == w {
                return true;
            }
        }
    }
    false
}

fn on_grid(v: f32) -> bool {
    let k = v / HEARD_GRID_M;
    (k - k.round()).abs() < 1e-4
}

struct Gate {
    room: Room,
    me: usize,
    mate: Option<usize>,
    history: VecDeque<[Fighter; MAX_FIGHTERS]>,
    sent: Vec<Vec<u8>>,
    over: bool,
    revealed: Option<[u64; 2]>,
    commit: [u8; 32],
    checked: usize,
    hidden_seen: usize,
}

impl Gate {
    fn check_snapshot(&mut self, s: &Snapshot) {
        let c = self.room.client(self.me as u8).map(|c| (c.rtt_ms, c.playout as u32));
        let fog_lead = c.map_or(0.0, |(rtt, play)| room::fog::lead_s(rtt, play));
        let w = &self.room.world;
        let me = &w.fighters[self.me];
        let now = &self.history[0];
        let sent: Vec<usize> = s.remotes().iter().map(|r| r.slot as usize).collect();
        let mate = |q: usize| Some(q) == self.mate;
        let known = |q: usize| q == self.me || mate(q) || sent.contains(&q);
        for (q, f) in w.fighters.iter().enumerate() {
            if q == self.me || mate(q) || !f.seated {
                continue;
            }
            if sent.contains(&q) {
                // 1. Never sent while far from visible: a check looser
                // than fog's. Seen from any look-ahead up to 300 ms
                // (fog's own among them: a point further along a path is
                // not a superset of a nearer one) leaning up to 0.7 m, in
                // the last 16 ticks.
                let leads = [fog_lead, 0.0, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3];
                let near = self.history.iter().any(|h| {
                    leads.iter().any(|&l| {
                        [room::fog::PEEK_M, 0.7]
                            .iter()
                            .any(|&p| sees(&w.map, &h[self.me], &h[q], l, p))
                    })
                });
                assert!(
                    near,
                    "{} tick {}: seat {q} sent while hidden from every angle: me {:?} them {:?} alive {} teams {}",
                    w.map.name, w.tick, me.body.pos, f.body.pos, f.alive, w.rules.teams
                );
            } else {
                // 2. Never hidden while plainly visible.
                assert!(
                    !plainly_sees(&w.map, me, &now[q]),
                    "tick {}: seat {q} in plain sight but not sent",
                    w.tick
                );
                self.hidden_seen += 1;
            }
        }
        for e in s.events() {
            let a = e.a as usize;
            if known(a) {
                continue;
            }
            // 3. An unseen fighter's shot or throw: heard, rounded, no aim.
            if e.kind == Ev::Shot as u8 || e.kind == Ev::Throw as u8 {
                assert_eq!(e.part, HEARD, "tick {}: an unseen fighter's shot sent whole", w.tick);
                assert!(on_grid(e.pos.x) && on_grid(e.pos.y) && on_grid(e.pos.z), "{e:?}");
                assert_eq!(e.pos, e.end, "a heard shot names no direction");
            }
            // 6. Where an unseen enemy spawned, picked up or swung: never.
            assert!(
                e.kind != Ev::Spawn as u8 && e.kind != Ev::Pickup as u8 && e.kind != Ev::Melee as u8,
                "tick {}: {e:?} about an unseen fighter",
                w.tick
            );
        }
        self.checked += 1;
    }
}

#[test]
fn nothing_a_client_should_not_know_reaches_it_in_doubles() {
    gate(
        Map::proving_grounds(),
        Rules {
            score_limit: 25,
            time_limit_ms: 120_000,
            ..Rules::doubles()
        },
    );
}

#[test]
fn nothing_a_client_should_not_know_reaches_it_on_any_map() {
    for d in sim::maps::ALL {
        for rules in [Rules::doubles(), Rules::ffa()] {
            gate(
                d.build(),
                Rules {
                    score_limit: 25,
                    time_limit_ms: 120_000,
                    ..rules
                },
            );
        }
    }
}

#[test]
fn nothing_a_client_should_not_know_reaches_it_in_gun_game() {
    gate(
        Map::proving_grounds(),
        Rules {
            time_limit_ms: 120_000,
            ..Rules::gun_game()
        },
    );
}

/// One match of `rules` on `map`, the person in the first seat, checked
/// from start to end.
fn gate(map: Map, rules: Rules) {
    let mut room = Room::new(map, rules, 77, SALT);
    let (slot, out) = room.join(Name::new("gate"), SKIN).unwrap();
    let Some((
        _,
        Msg::Welcome {
            salt_commit: commit, ..
        },
    )) = out.first().copied()
    else {
        panic!("no welcome")
    };
    let me = slot as usize;
    let mut g = Gate {
        mate: rules.teams.then_some(me ^ 2),
        room,
        me,
        history: VecDeque::new(),
        sent: Vec::new(),
        over: false,
        revealed: None,
        commit,
        checked: 0,
        hidden_seen: 0,
    };
    for (_, m) in &out {
        check_seat(m, slot);
        let mut b = [0u8; 256];
        let n = encode_msg(m, &mut b).unwrap();
        g.sent.push(b[..n].to_vec());
    }

    let mut driver = Bot::new(5, me, 0.6);
    let mut acked = 0u32;
    let mut buf = [0u8; MAX_DATAGRAM];
    for t in 0..TICK_HZ * 125 {
        let now = t as f64 * 1000.0 / TICK_HZ as f64;
        // The person seat plays through the client's path: a frame a tick,
        // acking the newest snapshot, a 4-tick playout.
        let mut m = InputMsg {
            ack_tick: acked,
            playout: 4,
            first_seq: 1000 + t,
            n: 1,
            ..InputMsg::default()
        };
        m.frames[0] = driver.think(&g.room.world, me);
        g.room.on_input(slot, &m, now);
        let (snaps, out) = g.room.step(now);
        g.history.push_front(g.room.world.fighters);
        g.history.truncate(16);
        for (_, s) in &snaps {
            acked = s.tick;
            g.check_snapshot(s);
            let n = encode_snapshot(s, &mut buf).unwrap();
            if !g.over {
                g.sent.push(buf[..n].to_vec());
            }
        }
        for (to, m) in &out {
            if to.is_some_and(|t| t != slot) {
                continue;
            }
            if let Msg::Reveal { salt } = m {
                assert!(g.over, "the salt was revealed before the match ended");
                g.revealed = Some(*salt);
                continue;
            }
            check_seat(m, slot);
            if let Msg::Phase { phase, .. } = m {
                g.over |= *phase == Phase::Over as u8;
            }
            let mut b = [0u8; 256];
            let n = encode_msg(m, &mut b).unwrap();
            g.sent.push(b[..n].to_vec());
        }
        if g.revealed.is_some() {
            break;
        }
    }

    assert!(g.over, "the match never ended");
    // 4. Not one bit of the salt before the end.
    for b in &g.sent {
        assert!(
            !contains_bits(b, SALT.0) && !contains_bits(b, SALT.1),
            "the salt crossed the wire"
        );
    }
    // 5. The reveal is the salt, and matches the welcome's commitment.
    assert_eq!(g.revealed, Some([SALT.0, SALT.1]));
    assert_eq!(salt_commit([SALT.0, SALT.1]), g.commit);
    // The gate only means something if fog actually hid someone.
    assert!(g.checked > 1000, "{} snapshots checked", g.checked);
    assert!(g.hidden_seen > 100, "fog hid an enemy only {} times", g.hidden_seen);
}

/// Check 7: skins are public by design (docs/DESIGN.md §15) and say
/// nothing of the world. A seat names the skin its person was checked to
/// own, and a bot's is the standard look.
fn check_seat(m: &Msg, me: u8) {
    if let Msg::Seat { slot, bot, skin, .. } = *m {
        let want = if slot == me && !bot { SKIN } else { proto::SKIN_STANDARD };
        assert_eq!(skin, want, "seat {slot}'s skin");
    }
}

#[test]
fn the_bit_scan_finds_unaligned_words() {
    let w = 0x0123_4567_89ab_cdefu64;
    for shift in 0..8 {
        let v = (w as u128) << (shift + 8);
        let b = v.to_le_bytes();
        assert!(contains_bits(&b, w), "shift {shift}");
    }
    assert!(!contains_bits(&[0u8; 32], w));
}

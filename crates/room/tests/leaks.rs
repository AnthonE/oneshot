//! The leak gate (docs/DESIGN.md §17): whole matches (Doubles, an
//! eight-seat Gun Game, a sixteen-seat Big Team, Capture the Flag) with
//! one person seat (played by a bot, through the same input path a client
//! uses), every snapshot and stream message it is sent checked against the
//! server's true world. Fails if the client is told where an enemy is when
//! it could not see them, if fog hides an enemy in plain sight, if an
//! unseen fighter's shot carries a place or a direction, if a hidden
//! fighter's projectile is sent out of sight, if any bit of the salt
//! crosses the wire before the match is over, or if a medal tells more
//! than the kill feed did.

use std::collections::VecDeque;

use proto::{encode_msg, encode_snapshot, salt_commit, InputMsg, Msg, Name, Snapshot, MAX_DATAGRAM};
use room::fog::{plainly_sees, sees};
use room::{Room, HEARD, HEARD_GRID_M};
use sim::bot::Bot;
use sim::consts::{MAX_FIGHTERS, SIEGE_NODES, TICK_HZ};
use sim::events::Kind as Ev;
use sim::fighter::Fighter;
use sim::geom::V3;
use sim::map::Map;
use sim::world::{Phase, Rules};

const SALT: (u64, u64) = (0x5a17_0123_4567_89ab, 0xfeed_beef_cafe_d00d);
/// The skin the person seat wears (the server checked it owns it).
const SKIN: u8 = 3;
const EMBLEM: proto::Emblem = proto::emblem(3, 2, 5, 9);
const HEADWEAR: u8 = 1;
const FOOTWEAR: u8 = 1;

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
    history: VecDeque<[Fighter; MAX_FIGHTERS]>,
    vhistory: VecDeque<[sim::vehicle::Vehicle; sim::consts::MAX_VEHICLES]>,
    sent: Vec<Vec<u8>>,
    over: bool,
    revealed: Option<[u64; 2]>,
    commit: [u8; 32],
    checked: usize,
    hidden_seen: usize,
    proj_seen: usize,
    vehicles_seen: usize,
    drops_seen: usize,
    /// Others held in view (updated in an earlier snapshot), checked.
    held_seen: usize,
    /// Ticks between the room's snapshots.
    every: u32,
}

impl Gate {
    fn check_snapshot(&mut self, s: &Snapshot) {
        let c = self.room.client(self.me as u8).map(|c| (c.rtt_ms, c.playout as u32));
        let fog_lead = c.map_or(0.0, |(rtt, play)| room::fog::lead_s(rtt, play, self.every));
        let w = &self.room.world;
        let me = &w.fighters[self.me];
        let now = &self.history[0];
        // In view: updated in this snapshot, or held (a far one on a big
        // map, its last update repeated): a held one is one sent, and held
        // to the same checks.
        let sent: Vec<usize> = s.remotes().iter().map(|r| r.slot as usize).collect();
        assert!(
            (0..MAX_FIGHTERS).all(|q| s.held & (1 << q) == 0 || sent.contains(&q)),
            "tick {}: held {:016b} beyond {sent:?}",
            w.tick,
            s.held
        );
        self.held_seen += s.held.count_ones() as usize;
        let mate = |q: usize| q != self.me && w.rules.teams && w.fighters[q].team == me.team;
        let known = |q: usize| q == self.me || mate(q) || sent.contains(&q);
        // Every kind on the wire has its check; no wildcard, so a new kind
        // doesn't compile until it has one.
        for k in sim::thing::Kind::ALL {
            match k {
                // 11. A fighter sent shows its overshield and camouflage
                // as they are now, and nothing more (never the
                // camouflage's time left).
                sim::thing::Kind::Fighter => {
                    for r in s.remotes() {
                        let t = proto::Remote::of(r.slot, &w.fighters[r.slot as usize]);
                        assert_eq!(
                            (r.over, r.cloak),
                            (t.over, t.cloak),
                            "tick {}: seat {}'s overshield or camouflage",
                            w.tick,
                            r.slot
                        );
                    }
                }
                // 8. A projectile is sent only if its owner is known to
                // this client or it is in clear line of sight.
                sim::thing::Kind::Projectile => {
                    let eye = me.eye();
                    for p in s.proj() {
                        self.proj_seen += 1;
                        let truth = w
                            .proj
                            .live()
                            .find(|(_, t)| t.owner == p.owner && (t.pos - p.pos).len() < 0.05)
                            .map(|(_, t)| *t);
                        let t = truth.unwrap_or_else(|| panic!("tick {}: {p:?} is in no true projectile", w.tick));
                        assert!(
                            known(p.owner as usize) || w.map.clear(eye, t.pos),
                            "tick {}: a hidden fighter's projectile out of sight was sent: {p:?}",
                            w.tick
                        );
                    }
                }
                // Pad timers are public (room::build).
                sim::thing::Kind::Pad => {}
                // 11. Capture the Flag: a flag at home or on the floor is
                // public, where it truly is (its base, or where its
                // carrier fell or let go: told to everyone, as Halo's
                // waypoint is); a carried one names its carrier, never
                // where they are.
                sim::thing::Kind::Flag => {
                    assert_eq!(s.flags().len(), if w.rules.ctf { 2 } else { 0 });
                    for (t, f) in s.flags().iter().enumerate() {
                        let truth = &w.flags[t];
                        assert_eq!(f.state, truth.state, "tick {}: flag {t}", w.tick);
                        if f.state == sim::world::FLAG_HELD {
                            assert_eq!(f.carrier, truth.carrier);
                            assert_eq!(f.pos, V3::ZERO, "tick {}: a carried flag's place was sent", w.tick);
                        } else {
                            assert!(
                                (f.pos - truth.pos).len() < 0.05,
                                "tick {}: {f:?} is not where flag {t} is",
                                w.tick
                            );
                        }
                    }
                }
                // 12. Siege: a node's overload, whether it is broken and
                // what its ring holds are public, as Halo's territories
                // are; never who or how many stand in it.
                sim::thing::Kind::Node => {
                    let siege = w.rules.siege && w.map.siege.is_some();
                    assert_eq!(s.nodes().len(), if siege { SIEGE_NODES } else { 0 });
                    for (n, x) in s.nodes().iter().enumerate() {
                        let truth = &w.nodes[n];
                        assert_eq!(x.broken, truth.broken, "tick {}: node {n}", w.tick);
                        if !x.broken {
                            assert_eq!(x.held, truth.held, "tick {}: node {n}", w.tick);
                        }
                    }
                }
                // 10. A gun on the floor only if it is a true one, in clear
                // line of this client's eye: never by who dropped it, so
                // never where a hidden enemy fell.
                sim::thing::Kind::Drop => {
                    let eye = me.eye();
                    for d in s.drops() {
                        self.drops_seen += 1;
                        let t = &w.drops[d.slot as usize];
                        assert!(
                            t.active && t.gun.arm == d.arm && (t.pos - d.pos).len() < 0.05,
                            "tick {}: {d:?} is no true drop",
                            w.tick
                        );
                        assert!(
                            w.map.clear(eye, t.pos + V3::new(0.0, room::DROP_LOOK_Y, 0.0)),
                            "tick {}: a drop out of sight was sent: {d:?}",
                            w.tick
                        );
                    }
                }
                // 9. A vehicle only if it is ours, someone we know rides
                // it, or it was in clear line in the last 16 ticks (from
                // any look ahead up to 300 ms); a rider named only if
                // known; and never one held back in plain sight.
                sim::thing::Kind::Vehicle => {
                    let mine = me.riding().map(|(i, _)| i);
                    assert_eq!(
                        s.own_vehicle.is_some(),
                        mine.is_some(),
                        "tick {}: our own vehicle",
                        w.tick
                    );
                    let leads = [
                        0.0,
                        0.1,
                        0.2,
                        0.3,
                        room::fog::full_lead(fog_lead, 2, self.every),
                        room::fog::full_lead(fog_lead, room::fog::FAR_EVERY, self.every),
                    ];
                    for v in s.vehicles() {
                        self.vehicles_seen += 1;
                        let i = v.slot as usize;
                        let t = &w.vehicles[i];
                        for &x in &v.seats {
                            assert!(
                                x == 0 || known(x as usize - 1),
                                "tick {}: an unknown rider named: {v:?}",
                                w.tick
                            );
                        }
                        let ridden = t.seats.iter().any(|&x| x != 0 && known(x as usize - 1));
                        let seen = self.history.iter().zip(&self.vhistory).any(|(h, hv)| {
                            hv[i].active
                                && leads
                                    .iter()
                                    .any(|&l| room::fog::sees_vehicle(&w.map, h[self.me].eye(), &hv[i], l))
                        });
                        assert!(
                            mine == Some(i) || ridden || seen,
                            "tick {}: a hidden vehicle was sent: {v:?}",
                            w.tick
                        );
                    }
                    for (i, t) in w.vehicles.live() {
                        let sent = s.vehicles().iter().any(|v| v.slot as usize == i);
                        assert!(
                            sent || mine == Some(i) || !room::fog::plainly_sees_vehicle(&w.map, me.eye(), t),
                            "tick {}: vehicle {i} in plain sight but not sent",
                            w.tick
                        );
                    }
                }
            }
        }
        for (q, f) in w.fighters.iter().enumerate() {
            if q == self.me || mate(q) || !f.seated {
                continue;
            }
            if sent.contains(&q) {
                // 1. Never sent while far from visible: a check looser
                // than fog's. Seen from any look-ahead up to 300 ms
                // (fog's own among them, stretched for near and far pairs'
                // full looks: a point further along a path is
                // not a superset of a nearer one) leaning up to 0.7 m, in
                // the last 16 ticks.
                let leads = [
                    fog_lead,
                    room::fog::full_lead(fog_lead, 2, self.every),
                    room::fog::full_lead(fog_lead, room::fog::FAR_EVERY, self.every),
                    0.0,
                    0.05,
                    0.1,
                    0.15,
                    0.2,
                    0.25,
                    0.3,
                ];
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
fn nothing_a_client_should_not_know_reaches_it_in_big_team() {
    for d in sim::maps::ALL {
        gate(
            d.build(),
            Rules {
                score_limit: 60,
                time_limit_ms: 120_000,
                ..Rules::big_team()
            },
        );
    }
}

#[test]
fn nothing_a_client_should_not_know_reaches_it_at_a_slower_snapshot_rate() {
    // A mode given its own rate (the server's `--big-snapshot-hz`): fog
    // holds a pair as many snapshots, never longer than it could be seen.
    for d in sim::maps::ALL.iter().filter(|d| d.big()) {
        gate_at(
            d.build(),
            Rules {
                score_limit: 60,
                time_limit_ms: 120_000,
                ..Rules::big_team()
            },
            4,
        );
    }
}

#[test]
fn nothing_a_client_should_not_know_reaches_it_in_ctf() {
    for d in sim::maps::ALL.iter().filter(|d| d.flags.len() == 2) {
        gate(
            d.build(),
            Rules {
                time_limit_ms: 120_000,
                ..Rules::ctf()
            },
        );
    }
}

#[test]
fn nothing_a_client_should_not_know_reaches_it_in_siege() {
    for d in sim::maps::ALL.iter().filter(|d| d.siege.len() == SIEGE_NODES) {
        gate(
            d.build(),
            Rules {
                time_limit_ms: 120_000,
                ..Rules::siege()
            },
        );
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
    gate_at(map, rules, room::SNAP_EVERY);
}

/// [`gate`], the room's snapshots `every` ticks apart.
fn gate_at(map: Map, rules: Rules, every: u32) {
    let mut room = Room::new(map, rules, 77, SALT);
    room.set_snap_every(every);
    // What the checks look back over: fog's hold, and two snapshots more.
    let looks = (room::fog::hold_ticks(every) + 2 * every) as usize;
    let (slot, out) = room
        .join(
            Name::new("gate"),
            proto::Look {
                skin: SKIN,
                emblem: EMBLEM,
                headwear: HEADWEAR,
                footwear: FOOTWEAR,
            },
        )
        .unwrap();
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
        room,
        me,
        history: VecDeque::new(),
        vhistory: VecDeque::new(),
        sent: Vec::new(),
        over: false,
        revealed: None,
        commit,
        checked: 0,
        hidden_seen: 0,
        proj_seen: 0,
        vehicles_seen: 0,
        drops_seen: 0,
        held_seen: 0,
        every,
    };
    for (_, m) in &out {
        check_seat(m, slot);
        let mut b = [0u8; 256];
        let n = encode_msg(m, &mut b).unwrap();
        g.sent.push(b[..n].to_vec());
    }

    let mut driver = Bot::new(5, me, 0.6);
    let mut feed: Vec<(u8, u8)> = Vec::new();
    let (mut medals, mut carnage) = (0, 0);
    let mut acked = 0u32;
    let mut buf = [0u8; MAX_DATAGRAM];
    let mut rx = proto::delta::Receiver::default();
    // Past the clock: Capture the Flag's sudden death too, and Siege's.
    let overtime = if rules.ctf {
        sim::consts::SUDDEN_DEATH_MS / 1000
    } else if rules.siege {
        sim::consts::SIEGE_SUDDEN_DEATH_MS / 1000
    } else {
        0
    };
    for t in 0..TICK_HZ * (125 + overtime) {
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
        // What goes on the wire, deltas and all, made whole as the client
        // does: exactly the snapshot checked.
        let mut snaps: Vec<(u8, Snapshot, Vec<u8>)> = Vec::new();
        let mut out = Vec::new();
        g.room
            .step_into(now, &mut out, |q, s, b| snaps.push((q, *s, b.to_vec())));
        g.history.push_front(g.room.world.fighters);
        g.history.truncate(looks);
        g.vhistory.push_front(*g.room.world.vehicles);
        g.vhistory.truncate(looks);
        for (_, s, wire) in &snaps {
            acked = s.tick;
            g.check_snapshot(s);
            let n = encode_snapshot(s, &mut buf).unwrap();
            let made = rx.take(wire).expect("the client makes every snapshot whole");
            assert_eq!(made, proto::decode_snapshot(&buf[..n]).unwrap(), "tick {}", s.tick);
            if !g.over {
                g.sent.push(wire.clone());
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
            check_medal(m, slot, g.over, &mut feed);
            medals += matches!(m, Msg::Medal { .. }) as u32;
            carnage += matches!(m, Msg::Carnage { .. }) as u32;
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
    assert!(g.proj_seen > 100, "only {} projectiles sent to check", g.proj_seen);
    if !g.room.world.rules.gun_game {
        assert!(g.drops_seen > 100, "only {} drops sent to check", g.drops_seen);
    }
    // A big map holds far ones between their updates; the gate checks them.
    if sim::maps::ALL.iter().any(|d| d.id == g.room.world.map.id && d.big()) {
        assert!(g.held_seen > 100, "only {} held to check", g.held_seen);
    }
    if !g.room.world.map.vehicles.is_empty() {
        assert!(g.vehicles_seen > 100, "only {} vehicles sent to check", g.vehicles_seen);
    }
    assert!(medals > 0, "no medal to check");
    assert_eq!(
        carnage as usize,
        g.room.world.fighters.iter().filter(|f| f.seated).count(),
        "carnage rows"
    );
}

/// Check 7: skins, emblems and gear are public by design
/// (docs/DESIGN.md §15) and say nothing of the world. A seat names the
/// look its person was checked to wear, and a bot's is the standard one.
fn check_seat(m: &Msg, me: u8) {
    if let Msg::Seat {
        slot,
        bot,
        skin,
        emblem,
        headwear,
        footwear,
        ..
    } = *m
    {
        let mine = slot == me && !bot;
        assert_eq!(
            skin,
            if mine { SKIN } else { proto::SKIN_STANDARD },
            "seat {slot}'s skin"
        );
        assert_eq!(
            emblem,
            if mine { EMBLEM } else { proto::EMBLEM_NONE },
            "seat {slot}'s emblem"
        );
        assert_eq!(
            (headwear, footwear),
            if mine {
                (HEADWEAR, FOOTWEAR)
            } else {
                (proto::GEAR_NONE, proto::GEAR_NONE)
            },
            "seat {slot}'s gear"
        );
    }
}

/// Check 8: a medal says no more than the kill feed. Someone else's
/// medal is a public one, for a kill the feed already told this client;
/// the carnage report comes once the match is over.
fn check_medal(m: &Msg, me: u8, over: bool, feed: &mut Vec<(u8, u8)>) {
    match *m {
        Msg::Kill { killer, victim, .. } => feed.push((killer, victim)),
        Msg::Medal { slot, medal, victim } => {
            let info = proto::medal::info(medal);
            assert!(
                slot == me || info.public,
                "seat {slot}'s {} was sent to {me}",
                info.name
            );
            if info.public {
                assert!(
                    feed.contains(&(slot, victim)),
                    "{} for {slot} on {victim}, a kill the feed never told",
                    info.name
                );
            }
        }
        Msg::Carnage { slot, .. } => assert!(over, "seat {slot}'s carnage report before the end"),
        _ => {}
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

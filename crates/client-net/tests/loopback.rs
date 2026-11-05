//! The client's netcode against the real room over a simulated network:
//! latency, jitter and loss in both directions, in virtual time. No sockets,
//! so it runs anywhere and the same every time.

use client_net::core::NetCore;
use proto::{decode_input, decode_snapshot, encode_input, encode_snapshot, Msg, Name, MAX_DATAGRAM};
use room::{Room, Seat};
use sim::consts::{MAX_FIGHTERS, TICK_HZ};
use sim::input::{Input, FIRE};
use sim::map::Map;
use sim::rng::Pcg32;
use sim::world::Rules;

struct Net {
    rng: Pcg32,
    /// One-way latency, its jitter, and loss, each way.
    delay_ms: f64,
    jitter_ms: f64,
    loss: f64,
    up: Vec<(f64, Vec<u8>)>,
    down: Vec<(f64, Vec<u8>)>,
    /// From this time, this much more latency on the way up (a route
    /// change): the server runs short of our inputs.
    up_step: (f64, f64),
    /// Bytes handed to the network each way, before loss, with QUIC's
    /// per-datagram overhead.
    bytes_up: u64,
    bytes_down: u64,
}

/// What a datagram costs on the wire past its payload: QUIC short header,
/// packet number, the DATAGRAM frame, AEAD tag, UDP and IPv4.
const OVERHEAD: u64 = 1 + 8 + 4 + 3 + 16 + 8 + 20;

impl Net {
    fn unit(&mut self) -> f64 {
        self.rng.next_u32() as f64 / 4_294_967_296.0
    }

    fn send(&mut self, now: f64, bytes: Vec<u8>, up: bool) {
        let cost = bytes.len() as u64 + OVERHEAD;
        if up {
            self.bytes_up += cost;
        } else {
            self.bytes_down += cost;
        }
        if self.unit() < self.loss {
            return;
        }
        let step = if up && now >= self.up_step.0 {
            self.up_step.1
        } else {
            0.0
        };
        let at = now + self.delay_ms + step + self.unit() * self.jitter_ms;
        if up {
            self.up.push((at, bytes));
        } else {
            self.down.push((at, bytes));
        }
    }

    fn due(q: &mut Vec<(f64, Vec<u8>)>, now: f64) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        q.retain(|(at, b)| {
            if *at <= now {
                out.push(b.clone());
                false
            } else {
                true
            }
        });
        out
    }
}

struct Run {
    room: Room,
    client: NetCore,
    slot: u8,
    net: Net,
    now: f64,
    next_tick: f64,
    next_frame: f64,
    /// Frames the page drew, and of those, frames where someone in view was
    /// drawn past the newest snapshot (extrapolated or held).
    frames: u64,
    stalls: u64,
    /// Frames that drew everyone else at an earlier tick than the frame
    /// before (they'd jerk backwards), and the render tick last drawn.
    back: u64,
    last_rt: f64,
    /// How far everyone else moved on each frame, in frames of real time
    /// (1 is even): a hitch shows as a step well under or over 1.
    steps: Vec<f64>,
    /// Kills the server's world raised.
    kills: u32,
    /// Snapshots it says yes to are lost on the way down.
    drop: Option<Lose>,
}

/// Says which snapshots are lost on the way down.
type Lose = Box<dyn FnMut(&proto::Snapshot) -> bool>;

fn start(delay_ms: f64, jitter_ms: f64, loss: f64, harmless: bool) -> Run {
    start_on(Map::proving_grounds(), delay_ms, jitter_ms, loss, harmless)
}

fn start_on(map: Map, delay_ms: f64, jitter_ms: f64, loss: f64, harmless: bool) -> Run {
    let rules = Rules {
        score_limit: 1000,
        time_limit_ms: 0,
        ..Rules::duel()
    };
    start_with(map, rules, delay_ms, jitter_ms, loss, harmless)
}

fn start_with(map: Map, rules: Rules, delay_ms: f64, jitter_ms: f64, loss: f64, harmless: bool) -> Run {
    let mut room = Room::new(map.clone(), rules, 11, (5, 6));
    let (slot, out) = room.join(Name::new("tester"), proto::Look::default()).unwrap();
    let Some((_, Msg::Welcome { tick, team, rules, .. })) = out.first().copied() else {
        panic!("no welcome")
    };
    if harmless {
        // No bot: nothing but our own inputs may move our fighter.
        for s in 0..MAX_FIGHTERS {
            if matches!(room.seats[s], Seat::Bot(_)) {
                room.seats[s] = Seat::Empty;
                room.world.unseat(s);
            }
        }
    }
    let client = NetCore::new(slot, team, tick, rules, map);
    Run {
        room,
        client,
        slot,
        net: Net {
            rng: Pcg32::new(9, 9),
            delay_ms,
            jitter_ms,
            loss,
            up: Vec::new(),
            down: Vec::new(),
            up_step: (f64::INFINITY, 0.0),
            bytes_up: 0,
            bytes_down: 0,
        },
        frames: 0,
        stalls: 0,
        back: 0,
        last_rt: 0.0,
        steps: Vec::new(),
        kills: 0,
        drop: None,
        now: 0.0,
        next_tick: 0.0,
        next_frame: 0.0,
    }
}

/// What a run shows a watcher, as it happens.
enum Seen<'a> {
    /// The server stepped its world.
    Tick(&'a Room),
    /// A snapshot reached the client (before it is handed it).
    Snap(&'a proto::Snapshot),
    /// The page drew a frame at `now`, after `update` made `made` ticks.
    Frame(&'a mut NetCore, f64, u32),
}

impl Run {
    /// Runs `ms` of virtual time: the server on its tick, the page at 60
    /// frames a second, the network in between. `input(t)` is the player.
    fn run(&mut self, ms: f64, input: impl Fn(f64) -> Input) {
        self.run_seen(ms, input, |_| {});
    }

    /// As [`Self::run`], showing `seen` each tick and each frame.
    fn run_seen(&mut self, ms: f64, input: impl Fn(f64) -> Input, mut seen: impl FnMut(Seen)) {
        let end = self.now + ms;
        let mut buf = [0u8; MAX_DATAGRAM];
        while self.now < end {
            for b in Net::due(&mut self.net.up, self.now) {
                let m = decode_input(&b).unwrap();
                self.room.on_input(self.slot, &m, self.now);
            }
            for b in Net::due(&mut self.net.down, self.now) {
                let s = decode_snapshot(&b).unwrap();
                if self.drop.as_mut().is_some_and(|d| d(&s)) {
                    continue;
                }
                seen(Seen::Snap(&s));
                self.client.on_snapshot(self.now, &s);
            }
            while self.next_tick <= self.now {
                let (snaps, _) = self.room.step(self.now);
                self.kills += self
                    .room
                    .world
                    .events
                    .as_slice()
                    .iter()
                    .filter(|e| e.kind == sim::events::Kind::Kill)
                    .count() as u32;
                for (_, s) in snaps {
                    let n = encode_snapshot(&s, &mut buf).unwrap();
                    self.net.send(self.now, buf[..n].to_vec(), false);
                }
                seen(Seen::Tick(&self.room));
                self.next_tick += 1000.0 / TICK_HZ as f64;
            }
            if self.next_frame <= self.now {
                self.frames += 1;
                if self.client.extrapolating(self.now) {
                    self.stalls += 1;
                }
                let rt = self.client.render_tick(self.now);
                if rt < self.last_rt {
                    self.back += 1;
                }
                self.steps
                    .push((rt - self.last_rt) / (1000.0 / 60.0 / (1000.0 / TICK_HZ as f64)));
                self.last_rt = rt;
                let before = self.client.tick();
                for m in self.client.update(self.now, input(self.now)) {
                    let n = encode_input(&m, &mut buf).unwrap();
                    self.net.send(self.now, buf[..n].to_vec(), true);
                }
                let made = self.client.tick().wrapping_sub(before);
                seen(Seen::Frame(&mut self.client, self.now, made));
                self.next_frame += 1000.0 / 60.0;
            }
            self.now += 0.5;
        }
    }
}

/// Runs in circles, jumps now and then, fires bursts.
fn player(t: f64) -> Input {
    let k = (t / 1000.0) as u32;
    Input {
        buttons: if (t as u32 / 700).is_multiple_of(3) { FIRE } else { 0 }
            | if k % 4 == 3 { sim::input::JUMP } else { 0 },
        yaw: (t * 20.0) as u32 as u16,
        pitch: 0,
        mx: 0,
        mz: 127,
    }
}

#[test]
fn a_clean_link_predicts_without_a_single_correction() {
    let mut r = start(20.0, 0.0, 0.0, true);
    r.run(4000.0, player);
    let warm = r.client.stats;
    let warm_repeats = r.room.client(r.slot).unwrap().repeats;
    r.run(10_000.0, player);
    let s = r.client.stats;
    assert!(s.snapshots - warm.snapshots > 600, "{s:?}");
    assert_eq!(s.corrections - warm.corrections, 0, "{s:?}");
    assert!(
        (1..=3).contains(&s.depth),
        "the clock settles near the target depth: {s:?}"
    );
    let c = r.room.client(r.slot).unwrap();
    assert_eq!(c.repeats, warm_repeats, "once settled, every input arrives in time");
}

#[test]
fn a_bad_link_still_mostly_confirms_and_converges() {
    let mut r = start(40.0, 25.0, 0.05, true);
    r.run(15_000.0, player);
    let s = r.client.stats;
    let ratio = s.confirmations as f64 / (s.confirmations + s.corrections) as f64;
    assert!(ratio > 0.9, "{ratio:.3}: {s:?}");
    // Let go of the controls, and the prediction lands on the server's
    // fighter exactly.
    r.net.loss = 0.0;
    r.net.jitter_ms = 0.0;
    r.run(3000.0, |_| Input::default());
    let (own, _) = r.client.own();
    let server = &r.room.world.fighters[r.slot as usize];
    assert!(client_net::core::same_local(own, server));
}

#[test]
fn others_are_drawn_between_where_the_server_had_them() {
    let mut r = start(30.0, 5.0, 0.0, false);
    let bot = 1 - r.slot as usize;
    let mut history = Vec::new();
    for _ in 0..400 {
        r.run(10.0, |_| Input::default());
        history.push((r.room.world.tick, r.room.world.fighters[bot].body.pos));
        let rt = r.client.render_tick(r.now);
        let Some(d) = r.client.others(r.now).into_iter().find(|d| d.slot as usize == bot) else {
            continue;
        };
        // The server's positions on the two ticks around the render tick.
        let lo = history.iter().rev().find(|(t, _)| (*t as f64) <= rt);
        let hi = history.iter().find(|(t, _)| (*t as f64) > rt);
        if let (Some(lo), Some(hi)) = (lo, hi) {
            let span = (hi.1 - lo.1).len() + 0.1;
            assert!(
                (d.pos - lo.1).len() <= span && (d.pos - hi.1).len() <= span,
                "{d:?} vs {lo:?} {hi:?}"
            );
        }
    }
}

#[test]
fn a_stalled_page_catches_up_within_a_second() {
    let mut r = start(20.0, 0.0, 0.0, true);
    r.run(5000.0, player);
    // Two seconds with no frames drawn: the page did nothing at all.
    let saved = r.next_frame;
    r.next_frame = r.now + 2000.0;
    r.run(2000.0, player);
    assert!(r.next_frame >= saved);
    r.run(1000.0, player);
    let settled = r.client.stats;
    r.run(4000.0, player);
    let s = r.client.stats;
    assert_eq!(
        s.corrections, settled.corrections,
        "corrections went on after the stall: {s:?}"
    );
}

#[test]
fn the_reveal_is_checked_against_the_commitment() {
    let mut r = start(10.0, 0.0, 0.0, true);
    r.client.salt_commit = proto::salt_commit([3, 4]);
    assert!(!r.client.on_reveal([3, 5]));
    assert_eq!(r.client.verified, Some(false));
    assert!(r.client.on_reveal([3, 4]));
    assert_eq!(r.client.verified, Some(true));
}

/// The impaired-network gate (docs/DESIGN.md §17): 60 ms each way, 10 ms of
/// jitter and 5% loss both ways, against a bot that fights back. Over its
/// budgets the build is red.
#[test]
fn an_impaired_link_stays_within_its_budgets() {
    // Measured when the gate went in: 0.8% corrected, 96 kbit/s down and
    // 97 up, 0.06% of frames stalled. The budgets leave room, not slack.
    /// Predictions the server corrected, of all it ruled on.
    const MAX_CORRECTION_RATE: f64 = 0.02;
    /// Down (snapshots) and up (inputs), kilobits a second, with QUIC's
    /// overhead. A duel: Doubles and FFA send more remotes.
    const MAX_DOWN_KBPS: f64 = 150.0;
    const MAX_UP_KBPS: f64 = 130.0;
    /// Frames drawn with someone past the newest snapshot.
    const MAX_STALL_RATE: f64 = 0.01;

    let mut r = start(60.0, 10.0, 0.05, false);
    r.run(4000.0, player);
    let (warm, up0, down0, f0, st0, b0, s0) = (
        r.client.stats,
        r.net.bytes_up,
        r.net.bytes_down,
        r.frames,
        r.stalls,
        r.back,
        r.steps.len(),
    );
    let secs = 30.0;
    r.run(secs * 1000.0, player);
    let s = r.client.stats;
    let (conf, corr) = (s.confirmations - warm.confirmations, s.corrections - warm.corrections);
    let correction_rate = corr as f64 / (conf + corr).max(1) as f64;
    let down = (r.net.bytes_down - down0) as f64 * 8.0 / 1000.0 / secs;
    let up = (r.net.bytes_up - up0) as f64 * 8.0 / 1000.0 / secs;
    let stall_rate = (r.stalls - st0) as f64 / (r.frames - f0) as f64;
    let back = r.back - b0;
    let (lo, hi) = spread(&r.steps[s0..]);
    let report = format!(
        "corrections {correction_rate:.4} ({corr} of {}), down {down:.0} kbit/s, up {up:.0} kbit/s, stalls {stall_rate:.4}, \
         drawn backwards {back}, steps {lo:.3} to {hi:.3} over {} frames: {s:?}",
        conf + corr,
        r.frames - f0
    );
    println!("{report}");
    assert!(conf > 1000, "{report}");
    assert!(correction_rate <= MAX_CORRECTION_RATE, "{report}");
    assert!(down <= MAX_DOWN_KBPS, "{report}");
    assert!(up <= MAX_UP_KBPS, "{report}");
    assert!(stall_rate <= MAX_STALL_RATE, "{report}");
    assert_eq!(back, 0, "{report}");
}

/// The least and most anyone else moved on in a frame, in frames.
fn spread(steps: &[f64]) -> (f64, f64) {
    steps.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &s| {
        (lo.min(s), hi.max(s))
    })
}

/// Everyone else moves on with real time through a jittery, lossy link: a
/// late snapshot slows them a little, never stops or pulls them back
/// (Gates' server_est and slewed playout; DESIGN §6).
#[test]
fn others_move_on_evenly_through_jitter() {
    let mut r = start(60.0, 25.0, 0.05, false);
    r.run(4000.0, player);
    let (b0, s0) = (r.back, r.steps.len());
    r.run(20_000.0, player);
    let (lo, hi) = spread(&r.steps[s0..]);
    println!("steps {lo:.3} to {hi:.3}, backwards {}", r.back - b0);
    assert_eq!(r.back, b0, "drawn backwards");
    assert!(lo > 0.85 && hi < 1.15, "steps {lo:.3} to {hi:.3}");
}

/// At the wheel on a clean link: every prediction of the vehicle (and the
/// seat on it) is confirmed, over Dry Fork's slopes and creek. Getting in
/// is the world's (a correction a round trip later), driving is ours.
#[test]
fn a_driver_predicts_its_vehicle_without_a_correction() {
    use sim::geom::V3;
    let mut r = start_on(sim::maps::canyon::DEF.build(), 20.0, 0.0, 0.0, true);
    let s = r.slot as usize;
    let v = r.room.world.vehicles[0].pos;
    let at = V3::new(v.x - 2.0, 0.0, v.z);
    r.room.world.fighters[s].body.pos = V3::new(at.x, r.room.world.map.ground(at.x, at.z), at.z);
    r.run(4000.0, |_| Input::default());
    let t0 = r.now;
    r.run(500.0, |t| Input {
        buttons: if t - t0 < 40.0 { sim::input::USE } else { 0 },
        ..Input::default()
    });
    assert!(r.client.own().0.riding().is_some(), "in the driver's seat");
    r.run(1000.0, |_| Input::default());
    let warm = r.client.stats;
    r.run(10_000.0, |t| Input {
        mz: 127,
        mx: ((t / 900.0).sin() * 70.0) as i8,
        ..Input::default()
    });
    let st = r.client.stats;
    let (_, pos, _) = r.client.own_vehicle().expect("our vehicle");
    assert!((pos - v).flat().len() > 20.0, "it drove: {pos:?}");
    assert!(st.confirmations - warm.confirmations > 500, "{st:?}");
    assert_eq!(st.corrections - warm.corrections, 0, "{st:?}");
}

/// Events ride every snapshot until one carrying them is acked: on a lossy
/// link every kill the server saw reaches the client, once each.
#[test]
fn every_kill_arrives_once_through_loss() {
    let mut r = start(60.0, 10.0, 0.10, false);
    let mut told = 0u32;
    for _ in 0..160 {
        r.run(500.0, player);
        let now = r.now;
        told += r
            .client
            .drain_events(now)
            .iter()
            .filter(|e| e.kind == sim::events::Kind::Kill as u8)
            .count() as u32;
    }
    // A last second for the newest to get home.
    r.run(1000.0, |_| Input::default());
    let now = r.now;
    told += r
        .client
        .drain_events(now)
        .iter()
        .filter(|e| e.kind == sim::events::Kind::Kill as u8)
        .count() as u32;
    assert!(r.kills >= 5, "{} kills to count", r.kills);
    assert_eq!(told, r.kills, "every kill told, none twice");
}

fn rocket_rules() -> Rules {
    Rules {
        score_limit: 1000,
        time_limit_ms: 0,
        loadout: [sim::arms::ROCKET, sim::arms::MAGNUM],
        ..Rules::duel()
    }
}

/// The world's rounds on one tick: its slot in the pool and its age (who
/// it is), owner, kind, where.
type Rounds = Vec<(usize, u16, u8, u8, sim::geom::V3)>;

fn rounds(room: &Room) -> (u32, Rounds) {
    let w = &room.world;
    (
        w.tick,
        w.proj
            .live()
            .map(|(i, p)| (i, p.age, p.owner, p.kind as u8, p.pos))
            .collect(),
    )
}

/// Where the world had a round of `owner`'s and `kind` at fractional tick
/// `rt`, from its ticks either side, the nearest `near` of those in both
/// (None: none was, or the nearest is a metre off, so another round).
fn truth_at(history: &[(u32, Rounds)], rt: f64, owner: u8, kind: u8, near: sim::geom::V3) -> Option<sim::geom::V3> {
    let lo = rt.floor() as u32;
    let a = &history.iter().find(|(t, _)| *t == lo)?.1;
    let b = &history.iter().find(|(t, _)| *t == lo + 1)?.1;
    let u = (rt - lo as f64) as f32;
    a.iter()
        .filter(|r| r.2 == owner && r.3 == kind)
        .filter_map(|ra| {
            let rb = b.iter().find(|r| r.0 == ra.0 && r.1 == ra.1 + 1)?;
            Some(ra.4 + (rb.4 - ra.4) * u)
        })
        .min_by(|x, y| (*x - near).len().total_cmp(&(*y - near).len()))
        .filter(|t| (*t - near).len() < 1.0)
}

/// Our rocket, fired while strafing at a run over 60 ms of round trip:
/// out of the muzzle we are drawn with the frame it fires, then moving on
/// every frame at 70 to 110% of its speed (the glide onto the render tick
/// and the clock's slew, never a pop), on the render tick as anyone's
/// round once it has glided there, and gone the frame its blast plays.
#[test]
fn our_rocket_leaves_our_drawn_muzzle_and_hands_off_without_a_pop() {
    let mut r = start_with(Map::open(), rocket_rules(), 30.0, 10.0, 0.02, true);
    r.run(3000.0, |_| Input::default());
    own_rockets(&mut r, 20);
}

/// The same over a long round trip (300 and 400 ms): the round leaves
/// the muzzle the frame it fires and moves on every frame after, never
/// waiting on snapshots to step it.
#[test]
fn our_rocket_moves_every_frame_over_a_long_round_trip() {
    for one_way in [150.0, 200.0] {
        let mut r = start_with(Map::open(), rocket_rules(), one_way, 2.0, 0.0, true);
        r.run(3000.0, |_| Input::default());
        own_rockets(&mut r, 0);
    }
}

/// The first snapshots with our rocket in them lost: the one that gets
/// through, however late, still finds it, so it keeps its id and its path
/// (never a stand-in that goes and a copy that pops in behind it).
#[test]
fn our_rocket_is_found_when_the_snapshots_with_it_first_are_lost() {
    let mut r = start_with(Map::open(), rocket_rules(), 50.0, 0.0, 0.0, true);
    r.run(3000.0, |_| Input::default());
    let me = r.slot;
    // Each rocket: the first three snapshots it is in are lost.
    let mut lost = 0;
    r.drop = Some(Box::new(move |s: &proto::Snapshot| {
        if !s.proj().iter().any(|p| p.owner == me) {
            lost = 0;
            return false;
        }
        lost += 1;
        lost <= 3
    }));
    own_rockets(&mut r, 20);
}

/// The route up gets 120 ms longer while our rocket flies: the server
/// runs short of our inputs and the clock jumps on. Our body jumps with
/// it; the rocket already in the air does not.
#[test]
fn a_clock_resync_does_not_throw_our_rocket_on() {
    let mut r = start_with(Map::open(), rocket_rules(), 30.0, 0.0, 0.0, true);
    r.run(3000.0, |_| Input::default());
    // The first rocket goes 500 ms in: 140 ms into its flight.
    r.net.up_step = (r.now + 640.0, 120.0);
    let before = r.client.stats.resyncs;
    own_rockets(&mut r, 0);
    assert!(r.client.stats.resyncs > before, "no resync to test");
}

/// Fires two rockets while strafing, as `our_rocket_leaves_our_drawn_muzzle`
/// says, checking each frame; each must be on the render tick for at least
/// `plain_min` frames before it goes.
fn own_rockets(r: &mut Run, plain_min: u32) {
    use sim::geom::{aim_dir, V3};
    let me = r.slot;
    let x0 = r.room.world.fighters[me as usize].body.pos.x;
    let yaw = if x0 < 0.0 { 0 } else { 32768 };
    let t0 = r.now;
    let fires = [t0 + 500.0, t0 + 2600.0];
    let input = move |t: f64| Input {
        buttons: if fires.iter().any(|&f| (f..f + 30.0).contains(&t)) {
            FIRE
        } else {
            0
        },
        yaw,
        pitch: 0,
        mx: if (((t - t0) / 600.0) as u32).is_multiple_of(2) {
            127
        } else {
            -127
        },
        mz: 0,
    };
    let frame_s = 1.0 / 60.0;
    let mut history: Vec<(u32, Rounds)> = Vec::new();
    let mut shots = r.client.own().0.shots;
    // Each of our rounds: its id, where and how fast it was last drawn, the
    // frame it went, and whether it was on the render tick by then.
    struct Ours {
        id: u16,
        pos: V3,
        speed: f32,
        gone: Option<u64>,
        plain: u32,
    }
    let mut ours: Vec<Ours> = Vec::new();
    let mut blasts: Vec<(u64, V3)> = Vec::new();
    let mut frame = 0u64;
    let mut steps = 0;
    r.run_seen(6000.0, input, |seen| match seen {
        Seen::Tick(room) => history.push(rounds(room)),
        Seen::Snap(_) => {}
        Seen::Frame(c, now, made) => {
            frame += 1;
            let drawn: Vec<_> = c.projectiles(now).into_iter().filter(|p| p.owner == me).collect();
            let (f, body) = c.own();
            let fired = f.shots != shots;
            shots = f.shots;
            let muzzle = body + (f.eye() - f.body.pos) + aim_dir(f.yaw, f.pitch) * 0.4;
            let launches = c.launches();
            let rt = c.render_tick(now);
            for e in c.drain_events(now) {
                if e.kind == sim::events::Kind::Blast as u8 && e.a == me {
                    blasts.push((frame, e.pos));
                }
            }
            let new: Vec<_> = drawn.iter().filter(|p| !ours.iter().any(|o| o.id == p.id)).collect();
            if fired {
                assert_eq!(new.len(), 1, "one round the frame it fired: {drawn:?}");
                // Out of the drawn muzzle, as far on as the ticks made since.
                let reach = 0.3 + 24.4 * (made.max(1) - 1) as f32 / TICK_HZ as f32;
                let d = (new[0].pos - muzzle).len();
                assert!(d <= reach, "{d} m from the drawn muzzle ({made} ticks made)");
            } else {
                assert!(new.is_empty(), "a round of ours from nowhere: {new:?}");
            }
            for p in &drawn {
                let open = p.pos.x.abs() < 47.0 && p.pos.z.abs() < 47.0;
                match ours.iter_mut().find(|o| o.id == p.id) {
                    None => ours.push(Ours {
                        id: p.id,
                        pos: p.pos,
                        speed: p.vel.len(),
                        gone: None,
                        plain: 0,
                    }),
                    Some(o) => {
                        assert!(o.gone.is_none(), "drawn again after it went");
                        let v = (o.speed + p.vel.len()) * 0.5;
                        let step = (p.pos - o.pos).len();
                        if open {
                            let (lo, hi) = (0.7 * v * frame_s - 0.02, 1.1 * v * frame_s + 0.02);
                            assert!(
                                (lo..=hi).contains(&step),
                                "stepped {step} at {v} m/s ({lo} to {hi}) at {rt}"
                            );
                            steps += 1;
                        }
                        o.pos = p.pos;
                        o.speed = p.vel.len();
                        if launches == 0 && open && rt <= c.latest().tick as f64 {
                            // On the render tick, where the server had it.
                            let t = truth_at(&history, rt, me, 0, p.pos).expect("in the world");
                            assert!((p.pos - t).len() < 0.05, "{:?} vs {t:?} at {rt}", p.pos);
                            o.plain += 1;
                        }
                    }
                }
            }
            for o in ours.iter_mut() {
                if o.gone.is_none() && !drawn.iter().any(|p| p.id == o.id) {
                    o.gone = Some(frame);
                }
            }
        }
    });
    println!(
        "{steps} steps; on the render tick for {:?} frames",
        ours.iter().map(|o| o.plain).collect::<Vec<_>>()
    );
    assert_eq!(ours.len(), 2, "two rockets");
    assert!(steps > 100, "{steps}");
    for o in &ours {
        assert!(o.plain >= plain_min, "never on the render tick: {}", o.plain);
        let gone = o.gone.expect("it went");
        let (_, at) = blasts
            .iter()
            .find(|(f, _)| *f == gone)
            .unwrap_or_else(|| panic!("no blast the frame it went: {blasts:?} vs {gone}"));
        // Drawn last on the frame before the render tick reached the
        // snapshot without it: at most a frame's flight short of where it
        // went off.
        let near = o.speed * frame_s + 0.3;
        assert!((o.pos - *at).len() < near, "last drawn {:?}, blast {at:?}", o.pos);
    }
}

/// A bot firing rockets at us: each drawn where the server had it at the
/// render tick, while what was sent covers the render tick.
#[test]
fn remote_rounds_are_drawn_where_the_server_had_them() {
    let mut r = start_with(Map::open(), rocket_rules(), 30.0, 5.0, 0.02, false);
    r.run(3000.0, |_| Input::default());
    let me = r.slot;
    let mut history: Vec<(u32, Rounds)> = Vec::new();
    let mut checked = 0;
    r.run_seen(
        30_000.0,
        |_| Input::default(),
        |seen| match seen {
            Seen::Tick(room) => history.push(rounds(room)),
            Seen::Snap(_) => {}
            Seen::Frame(c, now, _) => {
                let rt = c.render_tick(now);
                if rt > c.latest().tick as f64 {
                    return;
                }
                for p in c.projectiles(now) {
                    if p.owner == me || p.kind != 0 {
                        continue;
                    }
                    let Some(t) = truth_at(&history, rt, p.owner, p.kind, p.pos) else {
                        continue; // gone by then: run on to where it went
                    };
                    assert!((p.pos - t).len() < 0.05, "{:?} vs {t:?} at {rt}", p.pos);
                    checked += 1;
                }
            }
        },
    );
    println!("{checked} rockets checked");
    assert!(checked > 30, "{checked} rockets checked");
}

/// Nothing is drawn that this client wasn't sent: every round not ours is
/// near a state of it from a snapshot at or before the render tick, within
/// what running on from it for `EXTRAPOLATE_MS` reaches.
#[test]
fn nothing_is_drawn_that_was_not_sent() {
    use client_net::core::EXTRAPOLATE_MS;
    use sim::consts::ticks;
    let mut r = start_with(Map::open(), rocket_rules(), 40.0, 10.0, 0.05, false);
    r.run(2000.0, |_| Input::default());
    let me = r.slot;
    let mut sent: Vec<(u32, proto::ProjState)> = Vec::new();
    let mut checked = 0;
    r.run_seen(30_000.0, player, |seen| match seen {
        Seen::Tick(_) => {}
        Seen::Snap(s) => sent.extend(s.proj().iter().map(|p| (s.tick, *p))),
        Seen::Frame(c, now, _) => {
            let rt = c.render_tick(now);
            for p in c.projectiles(now) {
                if p.owner == me {
                    continue;
                }
                let ok = sent.iter().any(|(t, q)| {
                    let dt = rt - *t as f64;
                    if dt < 0.0 || q.kind != p.kind || q.owner != p.owner {
                        return false;
                    }
                    let run = dt.min(ticks(EXTRAPOLATE_MS) as f64) as f32 / TICK_HZ as f32;
                    (p.pos - q.pos).len() <= (q.vel.len() + 10.0) * run + 0.25
                });
                assert!(ok, "{p:?} at {rt}: nothing sent puts it there");
                checked += 1;
            }
        }
    });
    println!("{checked} rounds checked");
    assert!(checked > 30, "{checked} rounds checked");
}

/// A shot is checked against where its shooter saw everyone (DESIGN §7):
/// for every input, the tick the server rewinds it to is the render tick
/// the client drew at when it made it, to within the rounding of a tick.
/// The newest ack's view instead, as it once was, put a shot's target up
/// to a frame's worth of ticks further on than the shooter saw it.
#[test]
fn a_shot_is_rewound_to_what_its_shooter_saw() {
    for (delay, jitter) in [(20.0, 0.0), (45.0, 10.0)] {
        let mut r = start(delay, jitter, 0.0, true);
        let slot = r.slot as usize;
        r.run(3000.0, player);
        let mut made = std::collections::BTreeMap::new();
        let mut errs: Vec<f64> = Vec::new();
        r.run_seen(6000.0, player, |s| match s {
            Seen::Frame(c, now, n) => {
                let rt = c.render_tick(now);
                for k in 0..n {
                    made.insert(c.tick().wrapping_sub(n - k), rt);
                }
            }
            Seen::Tick(room) => {
                let Seat::Person(c, _) = &room.seats[slot] else {
                    return;
                };
                let t = room.world.tick;
                let back = t as f64 - c.rewind_q8(t) as f64 / 256.0;
                if let Some(rt) = made.remove(&c.last_seq) {
                    errs.push(back - rt);
                }
            }
            Seen::Snap(_) => {}
        });
        let mean = errs.iter().sum::<f64>() / errs.len() as f64;
        let worst = errs.iter().fold(0.0f64, |m, e| m.max(e.abs()));
        println!(
            "{delay} ms ± {jitter}: {} inputs, mean {mean:+.2} ticks, worst {worst:.2}",
            errs.len()
        );
        assert!(errs.len() > 600, "{}", errs.len());
        assert!(mean.abs() < 0.5, "{delay} ms: rewound {mean:+.2} ticks off on average");
        assert!(worst < 1.6, "{delay} ms: one input rewound {worst:.2} ticks off");
    }
}

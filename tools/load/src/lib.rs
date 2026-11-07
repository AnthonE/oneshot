//! A load test: many headless players on client-net's native transport (the
//! desktop client's netcode, as `crates/server/tests/e2e.rs` drives one),
//! each wandering, turning, jumping and firing, against a server; and what
//! the server's `/health.json` says while they play (docs/SCALING.md).
//!
//! Every player is a tokio task with a session of its own (its own QUIC
//! connection and socket), updated at a page's frame rate. A match that
//! ends sends it back in, as a player would; a refusal waits and asks
//! again. Players join evenly over the ramp, then all play for the hold;
//! the window measured is the hold, less a settling second. The report is
//! the players' side (in, refused, cut, snapshots a second, the prediction
//! confirmed) and the server's over the same window (ticks late or
//! skipped, a tick's percentiles from its buckets, snapshots dropped,
//! sessions cut behind).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

use client_net::native::{NativeClient, NativeLobby};
use client_net::session::Session;
use proto::Msg;
use sim::input::{Input, CROUCH, FIRE, GRENADE, JUMP, RELOAD};

/// Snapshots a second a seated player is sent at the arenas' rate (a big
/// map's may be its own: each player reads its rate off the ticks).
pub const SNAPSHOT_HZ: f64 = sim::consts::TICK_HZ as f64 / room::SNAP_EVERY as f64;

/// A refusal or a failed connect waits this long before asking again.
const RETRY: Duration = Duration::from_secs(2);
/// Players stay this long past the window, so its last read sees them all.
const STAY: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub struct Opts {
    /// The game's `host:port`, or several by commas (a server's endpoints):
    /// players spread over them in turn.
    pub addr: String,
    /// The certificate's SHA-256 to pin (a self-signed server); None trusts
    /// the system's roots.
    pub pin: Option<[u8; 32]>,
    /// The server's `/health.json`, if it can be read from here.
    pub health: Option<String>,
    pub players: usize,
    /// The modes players ask for, in turn (`proto::MODE_*`).
    pub modes: Vec<u8>,
    /// Players join evenly over this, then all play for `hold`.
    pub ramp: Duration,
    pub hold: Duration,
    /// Frames a second each session is updated at (a page's).
    pub fps: u32,
    /// A progress line this often, if any.
    pub every: Option<Duration>,
    /// Each player holds a lobby session beside its matches, as the page
    /// does from the title on (`--no-lobby`: matches alone).
    pub lobby: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            addr: "127.0.0.1:4433".into(),
            pin: None,
            health: None,
            players: 64,
            modes: vec![proto::MODE_BIG_TEAM],
            ramp: Duration::from_secs(10),
            hold: Duration::from_secs(60),
            fps: 60,
            every: Some(Duration::from_secs(5)),
            lobby: true,
        }
    }
}

/// A mode by the name a player knows it by.
pub fn mode(name: &str) -> Option<u8> {
    Some(match name {
        "bot" | "casual" => proto::MODE_CASUAL,
        "ffa" => proto::MODE_FFA,
        "gun" | "gun-game" => proto::MODE_GUN_GAME,
        "big" | "big-team" => proto::MODE_BIG_TEAM,
        "ctf" => proto::MODE_CTF,
        _ => return None,
    })
}

/// What `server.json` at a page says: where the game is and the pin.
pub fn server_json(doc: &str) -> Result<(String, Option<[u8; 32]>), String> {
    let v: serde_json::Value = serde_json::from_str(doc).map_err(|e| format!("server.json: {e}"))?;
    let addr = v["addr"].as_str().ok_or("server.json: no addr")?.to_string();
    let pin = match v["hash"].as_str() {
        Some(h) => Some(hex32(h).ok_or("server.json: hash is not 64 hex")?),
        None => None,
    };
    Ok((addr, pin))
}

/// 64 hex digits as 32 bytes.
pub fn hex32(s: &str) -> Option<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// GETs `url` as text.
pub async fn get(http: &reqwest::Client, url: &str) -> Result<String, String> {
    let r = http.get(url).send().await.map_err(|e| format!("{url}: {e}"))?;
    if !r.status().is_success() {
        return Err(format!("{url}: {}", r.status()));
    }
    r.text().await.map_err(|e| format!("{url}: {e}"))
}

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default()
}

/// What players do: a stick held a while then another, a steady turn,
/// fire in bursts, now and then a jump, a crouch, a reload or a grenade.
struct Wander {
    rng: u64,
    until_ms: f64,
    last_ms: f64,
    yaw: f64,
    /// Turn, in 1/65536 of a turn a millisecond.
    spin: f64,
    firing: bool,
    held: u16,
    input: Input,
}

impl Wander {
    fn new(seed: u64) -> Wander {
        Wander {
            rng: seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1,
            until_ms: 0.0,
            last_ms: 0.0,
            yaw: 0.0,
            spin: 0.0,
            firing: false,
            held: 0,
            input: Input::default(),
        }
    }

    fn next_u32(&mut self) -> u32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 32) as u32
    }

    /// `n` in [0, 1).
    fn unit(&mut self) -> f64 {
        self.next_u32() as f64 / 4_294_967_296.0
    }

    fn at(&mut self, now_ms: f64) -> Input {
        if now_ms >= self.until_ms {
            self.until_ms = now_ms + 300.0 + 1200.0 * self.unit();
            // Mostly forward, as players run.
            self.input.mz = (self.unit() * 254.0 - 64.0).clamp(-127.0, 127.0) as i8;
            self.input.mx = (self.unit() * 254.0 - 127.0) as i8;
            self.input.pitch = ((self.unit() - 0.5) * 4000.0) as i16;
            self.spin = (self.unit() - 0.5) * 60.0;
            self.firing = self.unit() < 0.5;
            let r = self.unit();
            self.held = if r < 0.10 {
                JUMP
            } else if r < 0.14 {
                CROUCH
            } else if r < 0.17 {
                RELOAD
            } else if r < 0.19 {
                GRENADE
            } else {
                0
            };
        }
        let dt = (now_ms - self.last_ms).max(0.0);
        self.last_ms = now_ms;
        self.yaw = (self.yaw + self.spin * dt).rem_euclid(65536.0);
        self.input.yaw = self.yaw as u16;
        // A press, let go, pressed again: semi-automatics fire too.
        let pulse = ((now_ms / 120.0) as u64).is_multiple_of(2);
        let fire = if self.firing && pulse { FIRE } else { 0 };
        // A jump or a grenade is a press, not a hold.
        let tap = if self.until_ms - now_ms > 200.0 { 0 } else { self.held };
        self.input.buttons = fire | tap;
        self.input
    }
}

/// What every player adds to as it plays, for the progress lines.
#[derive(Default)]
struct Live {
    /// Sessions seated now.
    seated: AtomicUsize,
    /// Snapshots taken in, in all.
    snapshots: AtomicU64,
}

/// One player's run.
#[derive(Default)]
struct Played {
    joins: usize,
    failed: usize,
    refused: BTreeMap<String, usize>,
    byes: usize,
    cut: BTreeMap<String, usize>,
    connect_ms: Vec<f64>,
    /// Inside the window: snapshots taken in while seated, and seconds seated.
    window_snaps: u64,
    window_secs: f64,
    confirmations: u64,
    corrections: u64,
    resyncs: u64,
    /// Never welcomed once.
    never: bool,
    /// Its lobby never opened.
    lobby_failed: bool,
    /// Its lobby opened, and the server cut it before the run was over.
    lobby_cut: bool,
    /// Snapshots a second it was sent, by the ticks between them (0: none).
    sent_hz: f64,
}

/// How a session ended.
enum End {
    /// The match was over.
    Bye,
    Refused(String),
    /// The server cut it, or it went quiet.
    Cut(String),
    /// Ours: the test is over.
    Done,
}

async fn player(i: usize, o: Arc<Opts>, start: Instant, window: (Instant, Instant), live: Arc<Live>) -> Played {
    let mode = o.modes[i % o.modes.len()];
    let addrs: Vec<&str> = o.addr.split(',').collect();
    let addr = addrs[i % addrs.len()];
    let end = window.1 + STAY;
    let mut p = Played {
        never: true,
        ..Played::default()
    };
    let mut wander = Wander::new(i as u64 + 1);
    let frame = Duration::from_secs_f64(1.0 / f64::from(o.fps.max(1)));
    tokio::time::sleep_until((start + o.ramp.mul_f64(i as f64 / o.players.max(1) as f64)).into()).await;
    // The title's lobby, held the whole run; a refused or lost one isn't
    // asked for again (the match is what's measured).
    let lobby = if o.lobby {
        let hello = Session::hello(
            &format!("load{i}"),
            proto::Pass::NONE,
            proto::MODE_LOBBY,
            proto::Look::default(),
            0,
        );
        NativeLobby::connect(addr, o.pin, hello).await.ok()
    } else {
        None
    };
    if o.lobby && lobby.is_none() {
        p.lobby_failed = true;
    }
    while Instant::now() < end {
        let name = format!("load{i}");
        let hello = Session::hello(&name, proto::Pass::NONE, mode, proto::Look::default(), 0);
        let asked = Instant::now();
        let mut c = match NativeClient::connect(addr, o.pin, hello).await {
            Ok(c) => c,
            Err(_) => {
                p.failed += 1;
                tokio::time::sleep(RETRY).await;
                continue;
            }
        };
        let mut every = tokio::time::interval(frame);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut seated = false;
        let mut last = Instant::now();
        let mut snaps = 0u32;
        let how = loop {
            every.tick().await;
            let now = Instant::now();
            if now >= end {
                break End::Done;
            }
            c.update(wander.at(c.now_ms()));
            if let Some(l) = &lobby {
                l.take();
                p.lobby_cut |= l.closed();
            }
            let mut said = None;
            for m in c.messages() {
                match m {
                    Msg::Bye => said = Some(End::Bye),
                    Msg::Refuse { reason } => {
                        said = Some(End::Refused(client_net::session::refuse_why(reason).to_string()))
                    }
                    _ => {}
                }
            }
            if c.session.ready && !seated {
                seated = true;
                p.never = false;
                p.joins += 1;
                p.connect_ms.push(asked.elapsed().as_secs_f64() * 1000.0);
                live.seated.fetch_add(1, Relaxed);
            }
            let s = c.session.core.stats.snapshots;
            let got = s.saturating_sub(snaps);
            snaps = s;
            live.snapshots.fetch_add(u64::from(got), Relaxed);
            if seated && now > window.0 && last < window.1 {
                p.window_snaps += u64::from(got);
                p.window_secs += now.min(window.1).duration_since(last.max(window.0)).as_secs_f64();
            }
            last = now;
            if let Some(how) = said {
                break how;
            }
            if c.closed() {
                // Give the close's reason a moment to land.
                tokio::time::sleep(Duration::from_millis(50)).await;
                break End::Cut(c.ended_why().unwrap_or_else(|| "the stream ended".into()));
            }
        };
        let st = c.session.core.stats;
        if st.snap_every > 0 {
            let hz = f64::from(sim::consts::TICK_HZ) / f64::from(st.snap_every);
            p.sent_hz = if p.sent_hz == 0.0 { hz } else { p.sent_hz.min(hz) };
        }
        p.confirmations += u64::from(st.confirmations);
        p.corrections += u64::from(st.corrections);
        p.resyncs += u64::from(st.resyncs);
        if seated {
            live.seated.fetch_sub(1, Relaxed);
        }
        c.close();
        match how {
            End::Done => break,
            End::Bye => p.byes += 1,
            End::Refused(why) => {
                *p.refused.entry(why).or_default() += 1;
                tokio::time::sleep(RETRY).await;
            }
            End::Cut(why) => *p.cut.entry(why).or_default() += 1,
        }
    }
    if let Some(l) = lobby {
        p.lobby_cut |= l.closed();
        l.close();
    }
    p
}

/// The server's numbers over a window: two reads of `/health.json`.
#[derive(Clone, Debug, Default)]
pub struct Window {
    pub secs: f64,
    /// What was true at its end.
    pub players: u64,
    pub rooms: u64,
    pub sim_threads: u64,
    pub ticks: u64,
    pub late: u64,
    pub skipped: u64,
    /// A tick's work over the window, from its log2 buckets (each an upper
    /// edge: at most 2× over).
    pub tick_p50_us: u64,
    pub tick_p99_us: u64,
    pub tick_top_us: u64,
    pub snapshots_dropped: u64,
    pub snapshots_oversize: u64,
    pub cut_behind: u64,
    pub send_errors: u64,
    pub inputs_dropped: u64,
    pub packets_sent: u64,
    pub packets_lost: u64,
    pub bytes_sent: u64,
    /// Since the server started.
    pub rtt_p99_ms: u64,
}

fn num(v: &serde_json::Value, k: &str) -> u64 {
    v[k].as_u64().unwrap_or(0)
}

fn buckets(v: &serde_json::Value, k: &str) -> Vec<u64> {
    v[k].as_array()
        .map(|a| a.iter().map(|x| x.as_u64().unwrap_or(0)).collect())
        .unwrap_or_default()
}

/// The upper edge of the bucket under which `pct` percent fall (as the
/// server's `health::pct`), and of the highest bucket with any.
fn pct(counts: &[u64], pct: u64) -> u64 {
    let total: u64 = counts.iter().sum();
    if total == 0 {
        return 0;
    }
    let want = (total * pct).div_ceil(100);
    let mut seen = 0;
    for (b, n) in counts.iter().enumerate() {
        seen += n;
        if seen >= want {
            return 1 << b;
        }
    }
    1 << counts.len().saturating_sub(1)
}

impl Window {
    pub fn between(a: &serde_json::Value, b: &serde_json::Value, secs: f64) -> Window {
        let d = |k: &str| num(b, k).saturating_sub(num(a, k));
        let (ta, tb) = (buckets(a, "tick_us"), buckets(b, "tick_us"));
        let tick: Vec<u64> = tb
            .iter()
            .enumerate()
            .map(|(i, n)| n.saturating_sub(ta.get(i).copied().unwrap_or(0)))
            .collect();
        let top = tick.iter().rposition(|&n| n > 0).map_or(0, |b| 1u64 << b);
        Window {
            secs,
            players: num(b, "players"),
            rooms: num(b, "rooms"),
            sim_threads: num(b, "sim_threads"),
            ticks: d("ticks"),
            late: d("ticks_late"),
            skipped: d("ticks_skipped"),
            tick_p50_us: pct(&tick, 50),
            tick_p99_us: pct(&tick, 99),
            tick_top_us: top,
            snapshots_dropped: d("snapshots_dropped"),
            snapshots_oversize: d("snapshots_oversize"),
            cut_behind: d("cut_behind"),
            send_errors: d("send_errors"),
            inputs_dropped: d("inputs_dropped"),
            packets_sent: d("packets_sent"),
            packets_lost: d("packets_lost"),
            bytes_sent: d("bytes_sent"),
            rtt_p99_ms: num(b, "rtt_p99_ms"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub players: usize,
    /// Sessions welcomed (a player rejoins when a match ends).
    pub joins: usize,
    /// Players never welcomed.
    pub never_in: usize,
    /// Players whose lobby never opened.
    pub lobby_failed: usize,
    /// Players whose lobby the server cut mid-run.
    pub lobby_cut: usize,
    /// Snapshots a second players were sent: the least rate seen.
    pub sent_hz: f64,
    pub failed: usize,
    pub refused: Vec<(String, usize)>,
    pub byes: usize,
    pub cut: Vec<(String, usize)>,
    pub connect_p50_ms: f64,
    pub connect_max_ms: f64,
    /// Snapshots a second each player took in while seated in the window:
    /// the least, the tenth percentile, the median.
    pub snaps_min: f64,
    pub snaps_p10: f64,
    pub snaps_p50: f64,
    /// Of the prediction's checks, the share it had right.
    pub confirmed: f64,
    pub resyncs: u64,
    pub server: Option<Window>,
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

impl Report {
    /// What says the server didn't keep up, if anything: a player never
    /// in, a lobby that never opened or was cut, snapshots short, a tick skipped or more than 1% late, a session
    /// cut behind, the prediction often wrong.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.never_in > 0 {
            out.push(format!("{} of {} players never got in", self.never_in, self.players));
        }
        if self.lobby_failed > 0 {
            out.push(format!(
                "{} of {} lobbies never opened",
                self.lobby_failed, self.players
            ));
        }
        if self.lobby_cut > 0 {
            out.push(format!("{} of {} lobbies cut mid-run", self.lobby_cut, self.players));
        }
        if self.snaps_p10 < 0.9 * self.sent_hz {
            out.push(format!(
                "a tenth of players took in under {:.1} snapshots a second (sent {})",
                self.snaps_p10, self.sent_hz
            ));
        }
        if self.confirmed < 0.9 {
            out.push(format!(
                "the prediction was right {:.1}% of the time",
                100.0 * self.confirmed
            ));
        }
        if let Some(w) = &self.server {
            if w.skipped > 0 {
                out.push(format!("{} ticks skipped", w.skipped));
            }
            if w.late * 100 > w.ticks {
                out.push(format!("{} of {} ticks late", w.late, w.ticks));
            }
            if w.cut_behind > 0 {
                out.push(format!("{} sessions cut behind", w.cut_behind));
            }
        } else {
            // A run that couldn't read the server's own numbers never says
            // it kept up.
            out.push("the server's /health.json couldn't be read: its ticks unchecked".into());
        }
        out
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |v: &[(String, usize)]| {
            v.iter()
                .map(|(w, n)| format!("{n}× {w}"))
                .collect::<Vec<_>>()
                .join("; ")
        };
        writeln!(
            f,
            "players {}: {} joins, {} never in, {} connects failed, {} matches over, {} lobbies failed, {} cut",
            self.players, self.joins, self.never_in, self.failed, self.byes, self.lobby_failed, self.lobby_cut
        )?;
        if !self.refused.is_empty() {
            writeln!(f, "  refused: {}", list(&self.refused))?;
        }
        if !self.cut.is_empty() {
            writeln!(f, "  cut: {}", list(&self.cut))?;
        }
        writeln!(
            f,
            "  in after {:.0} ms (median), {:.0} ms (most)",
            self.connect_p50_ms, self.connect_max_ms
        )?;
        writeln!(
            f,
            "  snapshots a second (sent {}): median {:.1}, tenth {:.1}, least {:.1}",
            self.sent_hz, self.snaps_p50, self.snaps_p10, self.snaps_min
        )?;
        writeln!(
            f,
            "  prediction right {:.1}%, {} resyncs",
            100.0 * self.confirmed,
            self.resyncs
        )?;
        match &self.server {
            Some(w) => {
                let loss = if w.packets_sent == 0 {
                    0.0
                } else {
                    100.0 * w.packets_lost as f64 / w.packets_sent as f64
                };
                writeln!(
                    f,
                    "server over {:.0} s: {} players, {} rooms, {} sim threads",
                    w.secs, w.players, w.rooms, w.sim_threads
                )?;
                writeln!(
                    f,
                    "  ticks {}: {} late, {} skipped; a tick's work p50 <{} us, p99 <{} us, most <{} us (of {} us)",
                    w.ticks,
                    w.late,
                    w.skipped,
                    w.tick_p50_us,
                    w.tick_p99_us,
                    w.tick_top_us,
                    1_000_000 / sim::consts::TICK_HZ
                )?;
                writeln!(
                    f,
                    "  snapshots dropped {}, oversize {}; cut behind {}; send errors {}; inputs dropped {}",
                    w.snapshots_dropped, w.snapshots_oversize, w.cut_behind, w.send_errors, w.inputs_dropped
                )?;
                let kbps = if w.secs > 0.0 && w.players > 0 {
                    w.bytes_sent as f64 * 8.0 / 1000.0 / w.secs / w.players as f64
                } else {
                    0.0
                };
                write!(
                    f,
                    "  packets {} sent, {loss:.2}% lost; {kbps:.0} kbps down a player; rtt p99 <{} ms",
                    w.packets_sent, w.rtt_p99_ms
                )
            }
            None => write!(f, "server: no /health.json read"),
        }
    }
}

async fn health(http: &reqwest::Client, url: &Option<String>) -> Option<(Instant, serde_json::Value)> {
    let doc = get(http, url.as_deref()?).await.ok()?;
    Some((Instant::now(), serde_json::from_str(&doc).ok()?))
}

/// Runs the test: players join over the ramp and play the hold out.
pub async fn run(o: Opts) -> Report {
    let o = Arc::new(o);
    let http = http_client();
    let start = Instant::now();
    let settle = Duration::from_secs(1).min(o.hold / 4);
    let window = (start + o.ramp + settle, start + o.ramp + o.hold);
    let live = Arc::new(Live::default());
    let players: Vec<_> = (0..o.players)
        .map(|i| tokio::spawn(player(i, o.clone(), start, window, live.clone())))
        .collect();

    // Progress lines, and the window's reads.
    let mut first = None;
    let mut last = health(&http, &o.health).await;
    let mut last_snaps = (Instant::now(), 0u64);
    let every = o.every.unwrap_or(Duration::MAX);
    let mut next_line = start + every.min(o.ramp + o.hold);
    loop {
        let now = Instant::now();
        if first.is_none() && now >= window.0 {
            first = health(&http, &o.health).await;
        }
        if now >= window.1 {
            break;
        }
        if o.every.is_some() && now >= next_line {
            next_line += every;
            let snaps = live.snapshots.load(Relaxed);
            let seated = live.seated.load(Relaxed);
            let secs = now.duration_since(last_snaps.0).as_secs_f64();
            let rate = (snaps - last_snaps.1) as f64 / secs.max(1e-3) / seated.max(1) as f64;
            last_snaps = (now, snaps);
            let mut line = format!(
                "t={:>4.0}s seated={seated}/{} snaps/s={rate:.1}",
                start.elapsed().as_secs_f64(),
                o.players
            );
            let read = health(&http, &o.health).await;
            if let (Some((ta, a)), Some((tb, b))) = (&last, &read) {
                let w = Window::between(a, b, tb.duration_since(*ta).as_secs_f64());
                line += &format!(
                    " rooms={} tick_p50<{}us p99<{}us late=+{} skipped=+{} dropped=+{} cut=+{} inputs_dropped=+{}",
                    w.rooms,
                    w.tick_p50_us,
                    w.tick_p99_us,
                    w.late,
                    w.skipped,
                    w.snapshots_dropped,
                    w.cut_behind,
                    w.inputs_dropped
                );
            }
            if read.is_some() {
                last = read;
            }
            eprintln!("{line}");
        }
        let wake = [
            Some(window.1),
            o.every.map(|_| next_line),
            first.is_none().then_some(window.0),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(window.1);
        tokio::time::sleep_until(wake.max(Instant::now() + Duration::from_millis(10)).into()).await;
    }
    let end = health(&http, &o.health).await;

    let mut all = Played::default();
    let mut rates = Vec::new();
    let mut never_in = 0;
    let mut lobby_failed = 0;
    let mut lobby_cut = 0;
    let mut sent_hz: f64 = 0.0;
    let mut refused: BTreeMap<String, usize> = BTreeMap::new();
    let mut cut: BTreeMap<String, usize> = BTreeMap::new();
    for h in players {
        let Ok(p) = h.await else {
            never_in += 1;
            continue;
        };
        never_in += usize::from(p.never);
        lobby_failed += usize::from(p.lobby_failed);
        lobby_cut += usize::from(p.lobby_cut);
        if p.sent_hz > 0.0 {
            sent_hz = if sent_hz == 0.0 {
                p.sent_hz
            } else {
                sent_hz.min(p.sent_hz)
            };
        }
        if p.window_secs > 0.5 {
            rates.push(p.window_snaps as f64 / p.window_secs);
        }
        all.joins += p.joins;
        all.failed += p.failed;
        all.byes += p.byes;
        all.connect_ms.extend(p.connect_ms);
        all.confirmations += p.confirmations;
        all.corrections += p.corrections;
        all.resyncs += p.resyncs;
        for (w, n) in p.refused {
            *refused.entry(w).or_default() += n;
        }
        for (w, n) in p.cut {
            *cut.entry(w).or_default() += n;
        }
    }
    rates.sort_by(f64::total_cmp);
    all.connect_ms.sort_by(f64::total_cmp);
    let server = match (first, end) {
        (Some((ta, a)), Some((tb, b))) => Some(Window::between(&a, &b, tb.duration_since(ta).as_secs_f64())),
        _ => None,
    };
    Report {
        players: o.players,
        joins: all.joins,
        never_in,
        lobby_failed,
        lobby_cut,
        sent_hz: if sent_hz == 0.0 { SNAPSHOT_HZ } else { sent_hz },
        failed: all.failed,
        refused: refused.into_iter().collect(),
        byes: all.byes,
        cut: cut.into_iter().collect(),
        connect_p50_ms: quantile(&all.connect_ms, 0.5),
        connect_max_ms: all.connect_ms.last().copied().unwrap_or(0.0),
        snaps_min: rates.first().copied().unwrap_or(0.0),
        snaps_p10: quantile(&rates, 0.1),
        snaps_p50: quantile(&rates, 0.5),
        confirmed: all.confirmations as f64 / (all.confirmations + all.corrections).max(1) as f64,
        resyncs: all.resyncs,
        server,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_the_difference_of_two_reads() {
        let a: serde_json::Value = serde_json::from_str(
            r#"{"ticks":100,"ticks_late":1,"tick_us":[0,0,0,0,0,0,0,0,50,50,0],"players":3,"cut_behind":0}"#,
        )
        .unwrap();
        let b: serde_json::Value = serde_json::from_str(
            r#"{"ticks":300,"ticks_late":3,"tick_us":[0,0,0,0,0,0,0,0,50,248,2],"players":5,"cut_behind":1,"sim_threads":4}"#,
        )
        .unwrap();
        let w = Window::between(&a, &b, 2.0);
        assert_eq!(
            (w.ticks, w.late, w.players, w.cut_behind, w.sim_threads),
            (200, 2, 5, 1, 4)
        );
        // 198 in [256, 512), 2 in [512, 1024).
        assert_eq!((w.tick_p50_us, w.tick_p99_us, w.tick_top_us), (512, 512, 1024));
    }

    #[test]
    fn server_json_names_the_game_and_its_pin() {
        let h = "ab".repeat(32);
        let (addr, pin) = server_json(&format!(r#"{{"addr":"127.0.0.1:4433","hash":"{h}"}}"#)).unwrap();
        assert_eq!(addr, "127.0.0.1:4433");
        assert_eq!(pin, Some([0xab; 32]));
        let (_, pin) = server_json(r#"{"addr":"game.example:4433","hash":null}"#).unwrap();
        assert_eq!(pin, None);
    }

    #[test]
    fn players_wander_and_fire() {
        let mut w = Wander::new(7);
        let (mut moved, mut fired, mut yaws) = (false, false, std::collections::BTreeSet::new());
        for f in 0..600 {
            let i = w.at(f as f64 * 16.7);
            moved |= i.mz != 0 || i.mx != 0;
            fired |= i.buttons & FIRE != 0;
            yaws.insert(i.yaw);
        }
        assert!(moved && fired && yaws.len() > 100);
    }
}

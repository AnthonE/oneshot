//! The authoritative server: rooms at the sim's tick, matched and seated on
//! one thread and stepped side by side on a pool of them, WebTransport
//! sessions on tokio around it, and a small HTTP server for the page
//! (docs/DESIGN.md §2, §4, §11, §14; docs/SCALING.md).
//!
//! A casual player gets a room of their own against a bot. A ranked player
//! (an Elo Pros pass with a wallet) waits in the queue until a match forms,
//! then plays in a room of people only; when it ends its result goes to the
//! Elo Pros ladder and its replay to disk. Whoever signed in, ranked or not,
//! has what they did sent to the game's stats, achievements and boards on
//! Elo Pros (`stats`), and is listed in `online.json` while they play
//! (`online`).
//!
//! The sim thread never blocks on the network: sessions hand it what they
//! got over a channel, and it hands each session its datagrams and stream
//! messages through an [`outlet::Outlet`] that never waits. A full datagram
//! lane drops a snapshot (the next replaces it); a full stream lane cuts the
//! session, counted, rather than lose what it was told. Reports and replay
//! files go to a tokio task for the same reason.

pub mod auth;
pub mod bugs;
pub mod cup;
pub mod elo;
pub mod eloqueue;
pub mod health;
pub mod http;
pub mod lobby;
pub mod net;
pub mod online;
pub mod outlet;
pub mod party;
pub mod playlist;
pub mod queue;
pub mod stats;

use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use proto::{
    frame_msg, InputMsg, Msg, Name, MODE_BIG_TEAM, MODE_CASUAL, MODE_CTF, MODE_CUP, MODE_DOUBLES, MODE_DUEL, MODE_FFA,
    MODE_GUN_GAME, MODE_SIEGE,
};
use rayon::prelude::*;
use room::Room;
use sim::consts::{MAX_FIGHTERS, TICK_HZ};
use sim::map::Map;
use sim::world::{Phase, Rules};
use tokio::sync::mpsc as tmpsc;

/// What sessions tell the sim thread.
pub enum ToSim {
    /// Casual: a room against a bot, or a seat in a shared free-for-all or
    /// Gun Game room where bots fill the rest.
    Casual {
        session: u64,
        name: Name,
        mode: u8,
        /// A skin and emblem checked against the player's wallet.
        look: proto::Look,
        /// The party it comes with, if it is in one that started this mode.
        party: Option<party::Seating>,
        /// The party code its hello carried (0 for none), seated with or
        /// not: friends watch its match by it.
        code: u32,
        /// Who signed in: their wallet, else their account's `acct_` id.
        /// None for a guest.
        player: Option<String>,
        /// Where it connects from (None for the box itself): a watcher
        /// from the same place watches no match it sits in.
        ip: Option<IpAddr>,
        tx: outlet::Outlet,
    },
    /// Into the ranked queue.
    Queue {
        session: u64,
        entry: Box<queue::Entry>,
        ip: Option<IpAddr>,
        tx: outlet::Outlet,
    },
    /// Ranked, for a match Elo Pros' queue formed (`queue::Gathers`): its
    /// id, every side's wallets, and the side this one is on, checked
    /// against Elo Pros already.
    Matched {
        session: u64,
        entry: Box<queue::Entry>,
        id: String,
        sides: Vec<Vec<String>>,
        side: usize,
        ip: Option<IpAddr>,
        tx: outlet::Outlet,
    },
    Leave {
        session: u64,
    },
    /// Watch a match (`MODE_WATCH`): party `target`'s (a friend's), or any
    /// public one for 0. `player` is who asks and `ip` where from, so
    /// nobody watches a match they sit in, as a guest or an alt from the
    /// same place either.
    Watch {
        session: u64,
        target: u32,
        player: Option<String>,
        ip: Option<IpAddr>,
        tx: outlet::Outlet,
    },
    /// A watcher follows the next player.
    Follow {
        session: u64,
    },
    /// A player's level is all there (`Msg::Loaded`): its countdown stops
    /// waiting for it.
    Loaded {
        session: u64,
    },
}

/// Watchers one room takes.
pub const MAX_WATCHERS: usize = 16;

/// What the sim thread hands off: slow work, off the tick.
pub enum Job {
    /// What players did, for the game's stats on Elo Pros.
    Stats(Vec<stats::Line>),
    /// Who this slot holds now, for `online.json`.
    Roster(online::Slot),
    /// A file to write: a replay, its watch, a review.
    Write { path: PathBuf, bytes: Vec<u8> },
    /// A match that ended: its replay, hashed here and not on the sim
    /// thread, then (with a replay directory) written beside its aim
    /// review, and a ranked one's result reported with the hash.
    Finished(Box<Finished>),
    /// Answered once every job before it is done (reports included, or
    /// saved: `elo::Elo::post_signed`).
    Flush(tokio::sync::oneshot::Sender<()>),
}

/// What [`Job::Finished`] takes.
pub struct Finished {
    pub replay: Vec<u8>,
    /// Where replays are kept and the match's id, and its aim review (its
    /// `replay_sha256` left to fill in) with whether it crosses a line.
    pub files: Option<(PathBuf, String, serde_json::Value, bool)>,
    /// The result for Elo Pros, its `replay_sha256` left to fill in.
    pub report: Option<elo::Report>,
}

/// Writes a file whole or not at all: a reader (the review tool, a backup)
/// never sees half of one, nor does a crash leave one.
async fn write_whole(path: &std::path::Path, bytes: Vec<u8>) {
    if let Some(dir) = path.parent() {
        let _ = tokio::fs::create_dir_all(dir).await;
    }
    let tmp = path.with_extension("part");
    let wrote = async {
        tokio::fs::write(&tmp, bytes).await?;
        tokio::fs::rename(&tmp, path).await
    };
    if let Err(e) = wrote.await {
        eprintln!("oneshot: {}: {e}", path.display());
    }
}

/// Appends one line to a file, making its folder if need be.
async fn append_line(path: &std::path::Path, line: &serde_json::Value) {
    use tokio::io::AsyncWriteExt;
    if let Some(dir) = path.parent() {
        let _ = tokio::fs::create_dir_all(dir).await;
    }
    let mut bytes = serde_json::to_vec(line).unwrap_or_default();
    bytes.push(b'\n');
    let wrote = async {
        let mut f = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        f.write_all(&bytes).await
    };
    if let Err(e) = wrote.await {
        eprintln!("oneshot: {}: {e}", path.display());
    }
}

/// Where each wallet's profile is kept beside the replays: a line per match,
/// its numbers and nothing else (docs/ANTICHEAT.md §7, §8).
pub const PROFILES: &str = "profiles";

/// A finished match stays up this long, for the end screen (`Config::linger`).
pub const LINGER: Duration = Duration::from_secs(10);

/// How long a drain waits for the reports to go out once its rooms are
/// done: past a report's whole retry budget. What is still unanswered then
/// is saved for `oneshot-review resend`.
pub const FLUSH_MAX: Duration = Duration::from_secs(elo::RETRY_BUDGET.as_secs() + 25);

#[derive(Clone, Debug)]
pub struct Config {
    /// Where WebTransport listens (UDP).
    pub bind: SocketAddr,
    /// Where the page is served (TCP), if anywhere.
    pub http: Option<SocketAddr>,
    /// The built page.
    pub web_dir: PathBuf,
    /// PEM certificate and key; without them a self-signed certificate
    /// (14 days, pinned by the page with its hash).
    pub cert: Option<(String, String)>,
    /// A casual room's rules: against a bot, free for all, Gun Game, Big
    /// Team, Capture the Flag, Siege.
    pub rules: Rules,
    pub ffa: Rules,
    pub gun_game: Rules,
    pub big_team: Rules,
    pub ctf: Rules,
    pub siege: Rules,
    /// Ranked rooms' rules.
    pub duel: Rules,
    pub doubles: Rules,
    /// A cup heat's rules (a free for all), and how a cup forms: once
    /// `cup_min` have entered, it starts when `cup_max` have or `cup_fill`
    /// has passed; `cup_rounds` Swiss rounds, then the final.
    pub cup: Rules,
    pub cup_min: usize,
    pub cup_max: usize,
    pub cup_fill: Duration,
    pub cup_rounds: u8,
    /// The places in a cup whose heats are reviewed before a prize.
    pub cup_review_top: u32,
    /// How long a finished match stays up, for the end screen.
    pub linger: Duration,
    /// Bots that stand still: a target for the browser test.
    pub passive_bots: bool,
    /// Every casual mode open, however few are online (`playlist`): tests,
    /// and a load test's box.
    pub all_modes: bool,
    /// Anyone may wear any skin or gear, wallet or not: a local server for
    /// looking at them (`--dress-all`). Never in production.
    pub dress_all: bool,
    /// The maps rooms are played on (ids, `sim::maps`), one drawn from each
    /// room's seed. Empty: all of them.
    pub maps: Vec<u8>,
    /// This game's name on Elo Pros, what passes are for.
    pub slug: String,
    /// The hosts the page is served from: a pass names the one it was
    /// handed on.
    pub domains: Vec<String>,
    /// Elo Pros' account key (`GET /api/account/pubkey`). Without it no
    /// pass is accepted and nobody can play ranked.
    pub elo_pubkey: Option<[u8; 32]>,
    pub elo_origin: String,
    /// This server's key for reporting results, and its id on Elo Pros.
    pub elo_key: Option<(PathBuf, String)>,
    /// Where replays are written.
    pub replays: Option<PathBuf>,
    /// Where players' bug and crash reports are kept (`bugs.rs`). None:
    /// `/bugs` isn't taken.
    pub bugs: Option<PathBuf>,
    /// Where players reach the game (`host:port`), told to the page in
    /// `server.json`. None: the host the page was asked on, at the bound
    /// port.
    pub public_addr: Option<String>,
    /// Origins whose pages may read `server.json` and `/nonce` (CORS): the
    /// page on elopros.com asking the game box.
    pub allow_origins: Vec<String>,
    /// Players the server takes before a hello is answered "full" (a
    /// player's lobby beside its match isn't another); QUIC Retry past
    /// twice every player's connections, refusal past four times
    /// (docs/DESIGN.md §14, `net::CONNS_PER_PLAYER`).
    pub capacity: usize,
    /// Snapshots a second on a big map (Big Team, and Capture the Flag
    /// there): 64 as every mode by default; 32 halves its packets on a box
    /// short of them. The arenas and ranked always get 64.
    pub big_snapshot_hz: u32,
    /// Snapshots go as deltas against what each client acked when that is
    /// smaller (`proto::delta`); off (`--no-delta`), every one goes whole.
    pub deltas: bool,
    /// On a big map, others far from a client are updated every few
    /// snapshots (`room::update_every`); off (`--no-far-rates`), every one.
    pub far_rates: bool,
    /// The key nonces are signed with, shared by the two slots so either
    /// takes the other's (`auth::Nonces`). None: a key of this process's own.
    pub nonce_key: Option<[u8; 32]>,
    /// A file naming the live slot's `host:port`: what `server.json` tells
    /// the page, from whichever slot answers. Missing or unreadable: our
    /// own address.
    pub live_file: Option<PathBuf>,
    /// A directory both slots share, where each writes who it holds
    /// (`online`), so either answers `online.json` for both. None: this
    /// process's own roster only.
    pub roster_dir: Option<PathBuf>,
    /// What `/online.json` (every signed-in wallet and what it plays) asks
    /// for in an `X-Roster-Key` header; None: anyone may read it.
    pub roster_key: Option<String>,
    /// Threads the rooms step on, side by side: 0 is one a core, 1 the sim
    /// thread alone (one room after another).
    pub sim_threads: usize,
    /// UDP endpoints, each its own socket and receive loop, on `bind`'s port
    /// and every `endpoint_step` after (`net::ports`); `server.json` hands
    /// pages one of them in turn.
    pub endpoints: usize,
    pub endpoint_step: u16,
    /// How far below the sim's the network threads' priority is (Linux's
    /// nice; `lower_this_thread`), so a tick isn't held up by sends.
    pub net_nice: i32,
    /// Ranked waits in Elo Pros' queue (`eloqueue`), which every server of
    /// the game shares, rather than this server's own: needs `elo_key`.
    pub elo_queue: bool,
    /// Addresses whose connections aren't held to a source's share (a load
    /// test's box; `net::Gate`). Loopback never is.
    pub load_sources: Vec<std::net::IpAddr>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            bind: "0.0.0.0:4433".parse().unwrap(),
            http: Some("0.0.0.0:8080".parse().unwrap()),
            web_dir: "crates/client/dist".into(),
            cert: None,
            rules: Rules::duel(),
            ffa: Rules::ffa(),
            gun_game: Rules::gun_game(),
            big_team: Rules::big_ctf(),
            ctf: Rules::ctf(),
            siege: Rules::siege(),
            duel: Rules::duel(),
            doubles: Rules::doubles(),
            cup: Rules {
                score_limit: 15,
                time_limit_ms: 480_000,
                ..Rules::ffa()
            },
            cup_min: 4,
            cup_max: 64,
            cup_fill: Duration::from_secs(60),
            cup_rounds: 3,
            cup_review_top: 3,
            linger: LINGER,
            passive_bots: false,
            all_modes: false,
            dress_all: false,
            maps: Vec::new(),
            slug: "oneshot".into(),
            domains: vec!["localhost".into(), "127.0.0.1".into()],
            elo_pubkey: None,
            elo_origin: "https://elopros.com".into(),
            elo_key: None,
            replays: None,
            bugs: None,
            public_addr: None,
            allow_origins: Vec::new(),
            capacity: 256,
            big_snapshot_hz: TICK_HZ / room::SNAP_EVERY,
            deltas: true,
            far_rates: true,
            nonce_key: None,
            live_file: None,
            roster_dir: None,
            roster_key: None,
            sim_threads: 0,
            endpoints: 1,
            endpoint_step: 10,
            net_nice: 5,
            elo_queue: false,
            load_sources: Vec::new(),
        }
    }
}

impl Config {
    /// The threads rooms step on: `sim_threads`, or one a core for 0.
    pub fn step_threads(&self) -> usize {
        match self.sim_threads {
            0 => std::thread::available_parallelism().map_or(1, |n| n.get()),
            n => n,
        }
    }
}

/// What every session needs to know.
pub struct Ctx {
    pub slug: String,
    pub domains: Vec<String>,
    pub elo_pubkey: Option<[u8; 32]>,
    pub nonces: auth::Nonces,
    pub elo: elo::Elo,
    pub sessions: AtomicU64,
    /// Connections held, and each address's share.
    pub gate: Arc<net::Gate>,
    /// On its way down: no new players, matches play out.
    pub draining: AtomicBool,
    /// Rooms still up (the sim writes it each tick).
    pub rooms: AtomicUsize,
    /// The casual modes open now and the players online they were opened
    /// by (`playlist`; the sim writes them every second).
    pub open_modes: AtomicU16,
    pub online: AtomicUsize,
    /// What `server.json` says.
    pub public_addr: Option<String>,
    pub live_file: Option<PathBuf>,
    pub allow_origins: Vec<String>,
    /// Snapshots dropped and sessions cut (`outlet`).
    pub counts: Arc<outlet::Counts>,
    /// The menu's sessions: parties, friends, voice (`lobby.rs`).
    pub lobby: Arc<lobby::Lobby>,
    /// Who this process holds (the sim writes it every few seconds), and
    /// where the slots share theirs.
    pub roster: std::sync::Mutex<online::Slot>,
    pub roster_dir: Option<PathBuf>,
    /// This slot's file in `roster_dir`.
    pub roster_file: String,
    pub roster_key: Option<String>,
    /// Where replays are kept: a match whose replay is there was played.
    pub replays: Option<PathBuf>,
    /// Everything else counted since start (`health.rs`).
    pub health: health::Health,
    /// Players' reports, if they're taken (`--bugs`).
    pub bugs: Option<bugs::Intake>,
    pub started: Instant,
    /// The threads rooms step on.
    pub sim_threads: usize,
    /// The UDP ports this process listens on, `--bind`'s first; the
    /// endpoints a slot runs and how far apart (`Config::endpoints`); and
    /// the next one `server.json` hands out.
    pub ports: Vec<u16>,
    pub endpoints: (usize, u16),
    pub next_port: AtomicUsize,
    /// Ranked waits in Elo Pros' queue (`Config::elo_queue`), and who waits.
    pub elo_queue: bool,
    pub waiters: eloqueue::Waiters,
    /// `Config::dress_all`.
    pub dress_all: bool,
}

impl Ctx {
    /// What is true now, for `/health.json` and the journal.
    pub fn now(&self) -> health::Now {
        health::Now {
            ready: !self.draining.load(Ordering::Relaxed),
            proto: proto::PROTO_VER,
            players: self.gate.players(),
            capacity: self.gate.capacity,
            rooms: self.rooms.load(Ordering::Relaxed),
            uptime_s: self.started.elapsed().as_secs(),
            sim_threads: self.sim_threads,
            elo: self.elo.counts(),
        }
    }
}

/// A line to the journal every [`health::JOURNAL`] that had anything in it:
/// what changed.
async fn journal(ctx: Arc<Ctx>) {
    let mut every = tokio::time::interval(health::JOURNAL);
    every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    every.tick().await;
    let mut was = ctx.health.read(&ctx.counts);
    loop {
        every.tick().await;
        let max = ctx.health.tick_window_max_us.swap(0, Ordering::Relaxed);
        let (now, is) = (ctx.now(), ctx.health.read(&ctx.counts));
        if health::eventful(now, &was, &is) {
            eprintln!("{}", health::line(now, &was, &is, max));
        }
        was = is;
    }
}

pub struct Handle {
    pub addr: SocketAddr,
    pub http: Option<SocketAddr>,
    /// SHA-256 of the certificate, for `serverCertificateHashes`.
    pub cert_hash: [u8; 32],
    /// The UDP buffers the kernel granted (receive, send).
    pub udp_buffers: (usize, usize),
    pub stop: Arc<AtomicBool>,
    pub ctx: Arc<Ctx>,
    /// Every UDP address listened on, `addr` first.
    pub addrs: Vec<SocketAddr>,
    endpoints: Vec<Arc<wtransport::Endpoint<wtransport::endpoint::endpoint_side::Server>>>,
    jobs: tmpsc::UnboundedSender<Job>,
    cfg: Config,
}

impl Handle {
    /// Goes down without cutting a match short (docs/DESIGN.md §14): no new
    /// players (a hello is answered "full", the queue is let go), every room
    /// plays to its end and its replay and report go out, then the sim
    /// stops. A room still up after `max` is cut. Returns once the files
    /// are written and the reports sent, or (past [`FLUSH_MAX`]) saved.
    pub async fn drain(&self, max: Duration) {
        self.ctx.draining.store(true, Ordering::Relaxed);
        let until = Instant::now() + max;
        // A tick for the sim to see it and count.
        tokio::time::sleep(Duration::from_millis(50)).await;
        while self.ctx.rooms.load(Ordering::Relaxed) > 0 && Instant::now() < until {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let cut = self.ctx.rooms.load(Ordering::Relaxed);
        if cut > 0 {
            eprintln!("oneshot: drain: {cut} room(s) still up after {max:?}, cut");
        }
        self.stop.store(true, Ordering::Relaxed);
        let (tx, rx) = tokio::sync::oneshot::channel();
        if self.jobs.send(Job::Flush(tx)).is_ok() {
            let _ = tokio::time::timeout(FLUSH_MAX, rx).await;
        }
        self.save_unsent();
    }

    /// Saves the reports and stats still being sent (a drain past its
    /// wait, or an exit at once) for `oneshot-review resend`, and says so.
    pub fn save_unsent(&self) {
        let (saved, lost, dir) = self.ctx.elo.save_pending();
        if saved + lost > 0 {
            let at = dir.map_or_else(|| "nowhere (no --replays)".into(), |d| d.display().to_string());
            eprintln!(
                "oneshot: drain: {} report(s) unanswered: {saved} saved to {at} (oneshot-review resend), {lost} abandoned",
                saved + lost
            );
        }
    }

    /// Reads the certificate's PEM files again (after an ACME renewal): new
    /// handshakes present the new one, live sessions keep theirs. Returns
    /// its hash.
    pub async fn reload(&self) -> Result<[u8; 32], String> {
        net::reload(&self.endpoints, &self.cfg).await
    }
}

/// Starts the server on the current tokio runtime. Returns once it is
/// listening.
pub async fn run(cfg: Config) -> Result<Handle, String> {
    // Salts, seeds, nonces and party codes are drawn from the OS: without
    // it there is no server (and `random_u64` would panic mid-match).
    getrandom::fill(&mut [0u8; 8]).map_err(|e| format!("the OS gives no randomness: {e}"))?;
    warm_maps();
    let bound = net::endpoints(&cfg).await?;
    let cert_hash = bound[0].cert_hash;
    let udp_buffers = (bound[0].udp_recv, bound[0].udp_send);
    let endpoints: Vec<_> = bound.into_iter().map(|b| Arc::new(b.endpoint)).collect();
    let addrs = endpoints
        .iter()
        .map(|e| e.local_addr().map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let addr = addrs[0];
    let (to_sim, from_net) = mpsc::channel::<ToSim>();
    let (inputs_tx, inputs) = mpsc::sync_channel::<(u64, InputMsg, f32)>(net::INPUT_LANE);
    let (jobs_tx, jobs_rx) = tmpsc::unbounded_channel::<Job>();
    let stop = Arc::new(AtomicBool::new(false));
    let threads = cfg.step_threads();
    // One thread needs no pool: the sim thread steps them itself.
    let pool = match threads {
        1 => None,
        n => Some(
            rayon::ThreadPoolBuilder::new()
                .num_threads(n)
                .thread_name(|i| format!("sim-{i}"))
                .build()
                .map_err(|e| format!("sim threads: {e}"))?,
        ),
    };

    let key = match &cfg.elo_key {
        Some((path, id)) => {
            let pem = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            Some(elo::ServerKey::from_pem(&cfg.slug, id, &pem)?)
        }
        None => None,
    };
    let ctx = Arc::new(Ctx {
        slug: cfg.slug.clone(),
        domains: cfg.domains.clone(),
        elo_pubkey: cfg.elo_pubkey,
        nonces: cfg
            .nonce_key
            .map_or_else(auth::Nonces::default, |k| auth::Nonces::with_key(&k)),
        elo: elo::Elo::new(&cfg.elo_origin, &cfg.slug, key)
            .saving_unsent_to(cfg.replays.as_ref().map(|d| d.join(elo::UNREPORTED))),
        sessions: AtomicU64::new(1),
        gate: Arc::new(net::Gate::new(cfg.capacity).exempting(&cfg.load_sources)),
        draining: AtomicBool::new(false),
        rooms: AtomicUsize::new(0),
        open_modes: AtomicU16::new(if cfg.all_modes {
            playlist::ALL
        } else {
            playlist::open(0, 0)
        }),
        online: AtomicUsize::new(0),
        public_addr: cfg.public_addr.clone(),
        live_file: cfg.live_file.clone(),
        counts: Arc::new(outlet::Counts::default()),
        health: health::Health::default(),
        // Each slot its own file, by the HTTP port it answers on.
        bugs: cfg.bugs.clone().map(|d| {
            let tag = cfg.http.map_or(String::new(), |a| a.port().to_string());
            bugs::Intake::new(d, &tag)
        }),
        started: Instant::now(),
        allow_origins: cfg.allow_origins.clone(),
        lobby: Arc::default(),
        roster: std::sync::Mutex::new(online::Slot::default()),
        roster_dir: cfg.roster_dir.clone(),
        roster_file: online::file_name(addr.port()),
        roster_key: cfg.roster_key.clone(),
        replays: cfg.replays.clone(),
        sim_threads: threads,
        ports: addrs.iter().map(|a| a.port()).collect(),
        endpoints: (cfg.endpoints.max(1), cfg.endpoint_step),
        next_port: AtomicUsize::new(0),
        elo_queue: cfg.elo_queue && cfg.elo_key.is_some(),
        dress_all: cfg.dress_all,
        waiters: eloqueue::Waiters::default(),
    });
    if cfg.elo_queue && cfg.elo_key.is_none() {
        eprintln!("oneshot: --elo-queue needs --elo-key: ranked uses this server's own queue");
    }

    let sim = Sim {
        cfg: cfg.clone(),
        jobs: jobs_tx.clone(),
        stop: stop.clone(),
        ctx: ctx.clone(),
        pool,
    };
    std::thread::Builder::new()
        .name("sim".into())
        .spawn(move || sim.run(from_net, inputs))
        .map_err(|e| e.to_string())?;
    tokio::spawn(jobs(ctx.clone(), jobs_rx));
    tokio::spawn(journal(ctx.clone()));
    if ctx.elo_queue {
        tokio::spawn(eloqueue::poller(ctx.clone()));
    }
    tokio::spawn(ctx.lobby.clone().sweeper());
    for ep in &endpoints {
        tokio::spawn(net::accept_loop(
            ep.clone(),
            to_sim.clone(),
            inputs_tx.clone(),
            ctx.clone(),
        ));
    }
    drop((to_sim, inputs_tx));
    // A certificate from a CA is checked by the browser against its own
    // roots; only the self-signed one is pinned (and Chrome takes a pin
    // only on a certificate of 14 days or less).
    let pin = cfg.cert.is_none().then_some(cert_hash);
    let http = match cfg.http {
        Some(at) => Some(http::serve(at, cfg.web_dir.clone(), addr, pin, ctx.clone()).await?),
        None => None,
    };
    Ok(Handle {
        addr,
        http,
        cert_hash,
        udp_buffers,
        stop,
        ctx,
        addrs,
        endpoints,
        jobs: jobs_tx,
        cfg,
    })
}

async fn jobs(ctx: Arc<Ctx>, mut rx: tmpsc::UnboundedReceiver<Job>) {
    let mut reports = tokio::task::JoinSet::new();
    while let Some(j) = rx.recv().await {
        while reports.try_join_next().is_some() {}
        match j {
            Job::Flush(done) => {
                while reports.join_next().await.is_some() {}
                let _ = done.send(());
            }
            Job::Write { path, bytes } => write_whole(&path, bytes).await,
            Job::Finished(f) => {
                let Finished { replay, files, report } = *f;
                let sha = elo::sha256_hex(&replay);
                if let Some((dir, id, mut doc, flagged)) = files {
                    doc["replay_sha256"] = sha.clone().into();
                    let text = serde_json::to_vec_pretty(&doc).unwrap_or_default();
                    // None past the recorder's cap (a match with no limit run for hours).
                    if replay.is_empty() {
                        eprintln!("oneshot: {id}: too long to keep a replay of");
                    } else {
                        write_whole(&dir.join(format!("{id}.osr")), replay).await;
                    }
                    if flagged {
                        write_whole(&dir.join("review").join(format!("{id}.json")), text.clone()).await;
                    }
                    write_whole(&dir.join(format!("{id}.watch.json")), text).await;
                    let at = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs());
                    for seat in doc["seats"].as_array().into_iter().flatten() {
                        let Some(wallet) = seat["wallet"].as_str().filter(|w| proto::party::is_wallet(w)) else {
                            continue;
                        };
                        let line = serde_json::json!({
                            "match": id,
                            "at": at,
                            "ranked": doc["ranked"],
                            "mode": doc["mode"],
                            "replay_sha256": sha,
                            "flags": seat["flags"],
                            "aim": seat["profile"],
                            "carnage": seat["carnage"],
                        });
                        append_line(&dir.join(PROFILES).join(format!("{wallet}.jsonl")), &line).await;
                    }
                }
                if let Some(mut r) = report {
                    r.replay_sha256 = sha;
                    let ctx = ctx.clone();
                    reports.spawn(async move {
                        match ctx.elo.report(&r).await {
                            Ok(()) => eprintln!(
                                "oneshot: reported {} ({}) replay sha256 {} salt commit {}",
                                r.id, r.ladder, r.replay_sha256, r.salt_commit
                            ),
                            Err(e) => eprintln!("oneshot: report {}: {e}", r.id),
                        }
                    });
                }
            }
            Job::Stats(lines) => {
                // Each part on its own, so a drain that stops waiting
                // finds every one of them pending and saves it.
                for part in lines.chunks(stats::MAX_REPORTS) {
                    let (ctx, body, n) = (ctx.clone(), stats::report_body(part), part.len());
                    reports.spawn(async move {
                        match ctx.elo.stats(body).await {
                            Ok(answer) => {
                                let v: serde_json::Value = serde_json::from_str(&answer).unwrap_or_default();
                                let unlocked = v["unlocked"].as_array().map_or(0, Vec::len);
                                eprintln!("oneshot: stats for {n} player(s), {unlocked} unlock(s)");
                            }
                            Err(e) => eprintln!("oneshot: stats: {e}"),
                        }
                    });
                }
            }
            Job::Roster(slot) => {
                let file = ctx
                    .roster_dir
                    .as_ref()
                    .map(|d| (d.clone(), serde_json::to_vec(&slot.to_json()).unwrap_or_default()));
                if let Ok(mut r) = ctx.roster.lock() {
                    *r = slot;
                }
                if let Some((dir, bytes)) = file {
                    let path = dir.join(&ctx.roster_file);
                    let _ = tokio::fs::create_dir_all(&dir).await;
                    let tmp = path.with_extension("part");
                    let wrote = async {
                        tokio::fs::write(&tmp, bytes).await?;
                        tokio::fs::rename(&tmp, &path).await
                    };
                    if let Err(e) = wrote.await {
                        eprintln!("oneshot: {}: {e}", path.display());
                    }
                }
            }
        }
    }
}

/// A salt, a seed or an id: never zeros because the OS failed (`run`
/// checks it once at start).
/// Each map built once: a build is up to 0.2 s (Chrysalis' terrain, a
/// big map's waypoints), and on the sim thread it held every room's tick
/// that long. A clone is microseconds.
static BUILT: [std::sync::OnceLock<Map>; 256] = [const { std::sync::OnceLock::new() }; 256];

/// Map `id`, built, from [`BUILT`]; None if there is no such map.
fn built(id: u8) -> Option<Map> {
    let d = sim::maps::ALL.iter().find(|d| d.id == id)?;
    Some(BUILT[id as usize].get_or_init(|| d.build()).clone())
}

/// Builds every map, each on a thread of its own, so the sim thread only
/// clones (one that asks before a map is ready waits for that map alone).
fn warm_maps() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        for d in sim::maps::ALL.iter() {
            let id = d.id;
            let _ = std::thread::Builder::new()
                .name(format!("map-{id}"))
                .spawn(move || built(id));
        }
    });
}

fn random_u64() -> u64 {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).expect("the OS gave no randomness");
    u64::from_le_bytes(b)
}

struct Sess {
    tx: outlet::Outlet,
    /// The room and seat it plays in.
    at: Option<(usize, u8)>,
    /// Who signed in (a wallet, else an `acct_` id); None for a guest.
    player: Option<String>,
    /// Unix seconds it came in.
    since: u64,
    /// What it asked to play.
    mode: u8,
    /// A watcher: the room it watches and the seat it follows.
    watching: Option<(usize, u8)>,
    /// Where it connects from (None for the box itself).
    ip: Option<IpAddr>,
}

/// Seats kept in a room for a party whose first member sat down: `left`
/// more on `team`, until `until`. Nobody else is seated in them.
struct Hold {
    code: u32,
    team: Option<u8>,
    left: u8,
    until: Instant,
}

/// How long a party's seats are kept for its members' hellos.
const PARTY_HOLD: Duration = Duration::from_secs(20);

struct Live {
    room: Room,
    id: String,
    mode: u8,
    /// Made for a party alone: no stranger is seated in it.
    private: bool,
    /// The party code each seat's session carried, and whether it was
    /// seated as that party (else alone, watchable by the code): who sits
    /// here now, never a list of who once did.
    codes: [(u32, bool); MAX_FIGHTERS],
    /// Seats kept for a party's members still on their way.
    holds: Vec<Hold>,
    links: [Option<u64>; MAX_FIGHTERS],
    wallets: [Option<String>; MAX_FIGHTERS],
    /// Who sits in each seat, for stats: the wallet, else the account's
    /// `acct_` id. Cleared when they leave (and are reported then).
    players: [Option<String>; MAX_FIGHTERS],
    /// The world tick each seat's person sat down at.
    joined: [u32; MAX_FIGHTERS],
    over_at: Option<Instant>,
    /// A cup heat: the cup (by index) and the heat in its stage.
    cup: Option<(usize, usize)>,
    /// Sessions watching it (`room::spectate`).
    watchers: Vec<u64>,
    /// The stream messages of its tick, kept between ticks: each room its
    /// own, whichever thread steps it.
    out: room::Outbox,
    /// Its events lost so far, as counted into `/health.json`.
    events_seen: u64,
}

/// The rotation a mode is played in (`sim::map::ROT_*`).
fn rotation_of(mode: u8) -> u16 {
    use sim::map::*;
    match mode {
        MODE_DUEL => ROT_DUEL,
        MODE_DOUBLES => ROT_DOUBLES,
        MODE_FFA => ROT_FFA,
        MODE_GUN_GAME => ROT_GUN_GAME,
        MODE_CUP => ROT_CUP,
        MODE_BIG_TEAM => ROT_BIG_TEAM,
        MODE_CTF => ROT_CTF,
        MODE_SIEGE => ROT_SIEGE,
        _ => ROT_CASUAL,
    }
}

/// The maps a pick is made from: the `given` ones (a server's `--map`), else
/// those in rotation `rot`, else any in the rotation that can play it
/// (flags for Capture the Flag), in id order.
fn pool(given: Vec<u8>, rot: u16) -> Vec<u8> {
    if !given.is_empty() {
        return given;
    }
    let fits: Vec<u8> = sim::maps::ALL.iter().filter(|d| d.plays(rot)).map(|d| d.id).collect();
    if !fits.is_empty() {
        return fits;
    }
    let ctf = rot & sim::map::ROT_CTF != 0;
    let siege = rot & sim::map::ROT_SIEGE != 0;
    sim::maps::ALL
        .iter()
        .filter(|d| !d.practice && (!ctf || d.flags.len() == 2))
        .filter(|d| !siege || d.siege.len() == sim::consts::SIEGE_NODES)
        .map(|d| d.id)
        .collect()
}

/// `ids` without the maps in `avoid`, the newest first, each only while
/// another map is left.
fn steer_off(mut ids: Vec<u8>, avoid: &[u8]) -> Vec<u8> {
    for a in avoid {
        if ids.len() > 1 {
            ids.retain(|id| id != a);
        }
    }
    ids
}

/// The maps each player (or a guest's party) played last, newest first, so
/// the next match they start is on another ([`steer_off`]). Noted as they
/// are seated (however that match then ends: played out, left, dropped),
/// forgotten after [`RECENT_FOR`].
#[derive(Default)]
struct Recent {
    by: BTreeMap<String, ([u8; RECENT_N], Instant)>,
}

/// Maps remembered for each, and for how long.
const RECENT_N: usize = 2;
const RECENT_FOR: Duration = Duration::from_secs(2 * 60 * 60);
/// An empty place in a [`Recent`] row: no map has this id (`sim::map::TEST_MAP`).
const NO_MAP: u8 = sim::map::TEST_MAP;

impl Recent {
    /// Who a seat is to it: the player, or a guest's party.
    fn key(player: Option<&String>, party: u32) -> Option<String> {
        player
            .cloned()
            .or_else(|| (party != 0).then(|| format!("party:{party:08x}")))
    }

    fn note(&mut self, key: String, map: u8, now: Instant) {
        let row = self.by.entry(key).or_insert(([NO_MAP; RECENT_N], now));
        if row.0[0] != map {
            row.0.rotate_right(1);
            row.0[0] = map;
        }
        row.1 = now;
    }

    /// What `keys` played last, newest first, each map once.
    fn of<'a>(&self, keys: impl IntoIterator<Item = &'a str>) -> Vec<u8> {
        let mut out = Vec::new();
        let rows: Vec<&[u8; RECENT_N]> = keys.into_iter().filter_map(|k| self.by.get(k).map(|r| &r.0)).collect();
        for i in 0..RECENT_N {
            for r in &rows {
                if r[i] != NO_MAP && !out.contains(&r[i]) {
                    out.push(r[i]);
                }
            }
        }
        out
    }

    fn prune(&mut self, now: Instant) {
        self.by.retain(|_, r| now.duration_since(r.1) < RECENT_FOR);
    }
}

/// Puts a room in the first free place; returns where.
fn place_room(rooms: &mut Vec<Option<Live>>, live: Live) -> usize {
    if let Some(i) = rooms.iter().position(|r| r.is_none()) {
        rooms[i] = Some(live);
        i
    } else {
        rooms.push(Some(live));
        rooms.len() - 1
    }
}

/// A cup heat that has run its course: (cup, heat, match id, scores in the
/// heat's order).
type HeatDone = (usize, usize, String, Vec<i32>);

/// The sim thread's own: what it needs, and the rooms' threads with it
/// (all of it shared by them, read only, while rooms step).
struct Sim {
    cfg: Config,
    jobs: tmpsc::UnboundedSender<Job>,
    stop: Arc<AtomicBool>,
    ctx: Arc<Ctx>,
    /// The threads rooms step on; None steps them on the sim thread.
    pool: Option<rayon::ThreadPool>,
}

pub(crate) fn send_msg(tx: &outlet::Outlet, m: &Msg) {
    let mut f = [0u8; proto::MAX_STREAM_MSG + 2];
    if let Ok(n) = frame_msg(m, &mut f) {
        tx.stream(&f[..n]);
    }
}

impl Sim {
    /// A new room: fresh salt (never seen until the end) and a seed drawn
    /// apart from it (what clients watch: spawn picks, bots).
    fn new_room(&self, mode: u8, ranked: bool, avoid: &[u8]) -> Room {
        self.new_room_custom(mode, ranked, proto::party::Custom::default(), avoid)
    }

    /// Which casual modes are open (`playlist`), by the players online: a
    /// page on the title holds a lobby, one in a match a seat too.
    fn open_modes(&self, rooms: &[Option<Live>]) {
        let online = self.ctx.lobby.online().max(self.ctx.gate.players());
        let live = rooms
            .iter()
            .flatten()
            .filter(|r| !r.private && r.over_at.is_none() && r.links.iter().any(Option::is_some))
            .fold(0u16, |m, r| m | playlist::bit(r.mode));
        let open = if self.cfg.all_modes {
            playlist::ALL
        } else {
            playlist::open(online, live)
        };
        self.ctx.open_modes.store(open, Ordering::Relaxed);
        self.ctx.online.store(online, Ordering::Relaxed);
    }

    /// [`Sim::new_room`], with a private match's own map, score and clock
    /// over the mode's. A map that doesn't exist, or one with no flags for
    /// Capture the Flag, is the server's pick, off the maps in `avoid` (what
    /// those it seats played last, newest first) where the rotation has
    /// others.
    fn new_room_custom(&self, mode: u8, ranked: bool, custom: proto::party::Custom, avoid: &[u8]) -> Room {
        self.ctx.health.matches.fetch_add(1, Ordering::Relaxed);
        let salt = (random_u64(), random_u64());
        let seed = random_u64();
        let mut rules = match mode {
            MODE_DOUBLES => self.cfg.doubles,
            MODE_CUP => self.cfg.cup,
            MODE_FFA => self.cfg.ffa,
            MODE_BIG_TEAM => self.cfg.big_team,
            MODE_CTF => self.cfg.ctf,
            MODE_SIEGE => self.cfg.siege,
            MODE_GUN_GAME => self.cfg.gun_game,
            _ if ranked => self.cfg.duel,
            _ => self.cfg.rules,
        };
        if !rules.gun_game {
            rules.score_limit = custom.score_limit().unwrap_or(rules.score_limit);
        }
        rules.time_limit_ms = custom.time_limit_ms().unwrap_or(rules.time_limit_ms);
        let chosen = custom
            .map
            .and_then(|id| sim::maps::ALL.iter().find(|d| d.id == id))
            .filter(|d| !rules.ctf || d.flags.len() == 2)
            .filter(|d| !rules.siege || d.siege.len() == sim::consts::SIEGE_NODES)
            .and_then(|d| built(d.id));
        // Big Team keeps to its own maps whatever its rules (its flags
        // are on them); Capture the Flag's rotation is the arenas'.
        let rot = if mode == MODE_BIG_TEAM {
            rotation_of(mode)
        } else if rules.ctf {
            sim::map::ROT_CTF
        } else if rules.siege {
            sim::map::ROT_SIEGE
        } else {
            rotation_of(mode)
        };
        let map = chosen.unwrap_or_else(|| self.pick_map(seed, rot, avoid));
        let big = !ranked && sim::maps::ALL.iter().any(|d| d.id == map.id && d.big());
        let mut room = if ranked {
            Room::ranked(map, rules, seed, salt)
        } else {
            let mut r = Room::new(map, rules, seed, salt);
            r.passive_bots = self.cfg.passive_bots;
            if big {
                r.set_snap_every(snap_every(self.cfg.big_snapshot_hz));
            }
            r
        };
        room.deltas = self.cfg.deltas;
        room.far_rates = self.cfg.far_rates;
        room.wait_loads = true;
        room
    }

    /// The map a room plays in rotation `rot` (`sim::map::ROT_*`), from its
    /// seed (what clients may know anyway): the `--map` ones if any are
    /// given, else the maps in that rotation (`Def::plays`: by a map's own
    /// `modes`, or its size), else any in the rotation that can play it;
    /// Capture the Flag only where there are flags. Off the maps in `avoid`
    /// while that leaves one, so a group doesn't play the same map twice
    /// running (a playlist's rule since Halo 2's).
    fn pick_map(&self, seed: u64, rot: u16, avoid: &[u8]) -> Map {
        let ctf = rot & sim::map::ROT_CTF != 0;
        let siege = rot & sim::map::ROT_SIEGE != 0;
        let has_flags = |id: u8| sim::maps::ALL.iter().any(|d| d.id == id && d.flags.len() == 2);
        let has_nodes = |id: u8| {
            sim::maps::ALL
                .iter()
                .any(|d| d.id == id && d.siege.len() == sim::consts::SIEGE_NODES)
        };
        let given: Vec<u8> = self
            .cfg
            .maps
            .iter()
            .copied()
            .filter(|&id| sim::maps::ALL.iter().any(|d| d.id == id))
            .filter(|&id| !ctf || has_flags(id))
            .filter(|&id| !siege || has_nodes(id))
            .collect();
        let ids = pool(given, rot);
        let ids = steer_off(ids, avoid);
        let Some(&id) = ids.get((seed % ids.len().max(1) as u64) as usize) else {
            return Map::proving_grounds();
        };
        built(id).unwrap_or_else(Map::proving_grounds)
    }

    /// The sim thread: what sessions say (`rx`), their inputs (bounded, and
    /// none of them queued on the heap), then every room's tick.
    fn run(self, rx: mpsc::Receiver<ToSim>, inputs: mpsc::Receiver<(u64, InputMsg, f32)>) {
        let start = Instant::now();
        let tick = Duration::from_nanos(1_000_000_000 / TICK_HZ as u64);
        let mut next = Instant::now();
        let mut sessions: BTreeMap<u64, Sess> = BTreeMap::new();
        let mut rooms: Vec<Option<Live>> = Vec::new();
        let mut q = queue::Queue::default();
        let mut gathers = queue::Gathers::default();
        let mut recent = Recent::default();
        let mut last_second = Instant::now();
        let mut seconds: u64 = 0;

        let place = place_room;
        let mut cups: Vec<Option<cup::Cup>> = Vec::new();
        let mut cup_wait: Vec<queue::Entry> = Vec::new();
        let mut cup_ready_at: Option<Instant> = None;

        while !self.stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            let now_ms = start.elapsed().as_secs_f64() * 1000.0;
            while let Ok(m) = rx.try_recv() {
                match m {
                    ToSim::Casual {
                        session,
                        name,
                        mode,
                        look,
                        party,
                        code,
                        player,
                        ip,
                        tx,
                    } => {
                        // Going down since its hello was checked: to the live
                        // slot (a room opened now would only hold the drain up).
                        if self.ctx.draining.load(Ordering::Relaxed) {
                            send_msg(
                                &tx,
                                &Msg::Refuse {
                                    reason: proto::REFUSE_FULL,
                                },
                            );
                            self.ctx.health.refuse(proto::REFUSE_FULL);
                            tx.close();
                            continue;
                        }
                        let mode = match mode {
                            MODE_FFA | MODE_GUN_GAME | MODE_BIG_TEAM | MODE_CTF | MODE_SIEGE => mode,
                            _ => MODE_CASUAL,
                        };
                        // A mode closed for want of players is played where
                        // the players are.
                        let mode = playlist::route(
                            mode,
                            self.ctx.open_modes.load(Ordering::Relaxed),
                            party.is_some_and(|p| p.private),
                        );
                        let live_room =
                            |r: &Live| r.mode == mode && r.over_at.is_none() && r.room.world.phase != Phase::Over;
                        // Seats kept for parties on their way go when their
                        // time is up.
                        for r in rooms.iter_mut().flatten() {
                            r.holds.retain(|h| h.left > 0 && h.until > now);
                        }
                        // Seats open on `team` to a hello of party `code`
                        // (0: none): less those kept for other parties.
                        let free = |r: &Live, team: Option<u8>, code: u32| {
                            let kept: usize = r
                                .holds
                                .iter()
                                .filter(|h| h.code != code && (team.is_none() || h.team == team))
                                .map(|h| h.left as usize)
                                .sum();
                            r.room.open_seats(team).saturating_sub(kept)
                        };
                        // A party's room: where a member sits now, seated as
                        // the party, on that member's side, while it has a
                        // seat; as private as the start says.
                        let with_party = party.and_then(|p| {
                            rooms.iter().enumerate().find_map(|(i, r)| {
                                let r = r.as_ref()?;
                                if !live_room(r) || r.private != p.private {
                                    return None;
                                }
                                let s = (0..MAX_FIGHTERS)
                                    .find(|&s| r.links[s].is_some() && r.codes[s] == (p.code, true))?;
                                let team = r.room.world.rules.teams.then(|| r.room.world.fighters[s].team);
                                (free(r, team, p.code) > 0).then_some((i, team))
                            })
                        });
                        // A start long gone (a later PLAY alone with the
                        // party's code) seats them only back with the party.
                        let party = party.filter(|p| p.fresh || with_party.is_some());
                        let pcode = party.map_or(0, |p| p.code);
                        // Big Team's side with room for the whole party.
                        let side_for = |r: &Live, n: usize| -> Option<Option<u8>> {
                            if !r.room.world.rules.teams {
                                return (free(r, None, pcode) >= n).then_some(None);
                            }
                            [0u8, 1].into_iter().find(|&t| free(r, Some(t), pcode) >= n).map(Some)
                        };
                        let need = party.map_or(1, |p| p.size as usize);
                        let private = party.is_some_and(|p| p.private);
                        // Free for all, Gun Game and Big Team are shared: a
                        // room still playing with seats for all of us, else
                        // a new room. A party alone always gets its own.
                        let open = (mode != MODE_CASUAL && !private)
                            .then(|| {
                                rooms.iter().enumerate().find_map(|(i, r)| {
                                    let r = r.as_ref()?;
                                    if !live_room(r) || r.private {
                                        return None;
                                    }
                                    side_for(r, need).map(|t| (i, t))
                                })
                            })
                            .flatten();
                        let (i, team) = match with_party.or(open) {
                            Some(at) => at,
                            None => {
                                let i = place(
                                    &mut rooms,
                                    Live {
                                        room: self.new_room_custom(
                                            mode,
                                            false,
                                            party.filter(|p| p.private).map_or_else(Default::default, |p| p.custom),
                                            &recent.of(Recent::key(player.as_ref(), code).as_deref()),
                                        ),
                                        id: format!("os-c{:016x}", random_u64()),
                                        mode,
                                        links: [None; MAX_FIGHTERS],
                                        wallets: Default::default(),
                                        players: Default::default(),
                                        joined: [0; MAX_FIGHTERS],
                                        over_at: None,
                                        cup: None,
                                        watchers: Vec::new(),
                                        private,
                                        codes: [(0, false); MAX_FIGHTERS],
                                        holds: Vec::new(),
                                        out: Vec::new(),
                                        events_seen: 0,
                                    },
                                );
                                let team = side_for(rooms[i].as_ref().unwrap(), need).flatten();
                                (i, team)
                            }
                        };
                        let live = rooms[i].as_mut().unwrap();
                        let Some((slot, out)) = live.room.join_team(name, look, team) else {
                            // No seat after all: told so, and a room made for
                            // it alone goes with it.
                            send_msg(
                                &tx,
                                &Msg::Refuse {
                                    reason: proto::REFUSE_FULL,
                                },
                            );
                            self.ctx.health.refuse(proto::REFUSE_FULL);
                            tx.close();
                            if live.links.iter().all(|l| l.is_none()) && live.watchers.is_empty() && live.cup.is_none()
                            {
                                rooms[i] = None;
                            }
                            continue;
                        };
                        if let Some(k) = Recent::key(player.as_ref(), code) {
                            recent.note(k, live.room.world.map.id, now);
                        }
                        live.links[slot as usize] = Some(session);
                        // Alone, a party's code only makes them watchable by
                        // it; with the party, its first member keeps the
                        // rest of its seats and each who comes takes one.
                        live.codes[slot as usize] = match party {
                            Some(p) => (p.code, true),
                            None => (code, false),
                        };
                        if let Some(p) = party {
                            let others = (0..MAX_FIGHTERS).any(|s| {
                                s != slot as usize && live.links[s].is_some() && live.codes[s] == (p.code, true)
                            });
                            if let Some(h) = live.holds.iter_mut().find(|h| h.code == p.code) {
                                h.left = h.left.saturating_sub(1);
                            } else if !others && need > 1 {
                                live.holds.push(Hold {
                                    code: p.code,
                                    team,
                                    left: (need - 1) as u8,
                                    until: now + PARTY_HOLD,
                                });
                            }
                        }
                        self.ctx.health.joins_casual.fetch_add(1, Ordering::Relaxed);
                        live.players[slot as usize] = player.clone();
                        live.joined[slot as usize] = live.room.world.tick;
                        sessions.insert(
                            session,
                            Sess {
                                tx,
                                at: Some((i, slot)),
                                player,
                                since: auth::now_unix(),
                                mode,
                                watching: None,
                                ip,
                            },
                        );
                        deliver(&sessions, rooms[i].as_ref().unwrap(), out);
                    }
                    ToSim::Queue { session, entry, ip, tx } => {
                        if busy(&rooms, &cups, &cup_wait, &q, &entry.wallet) || gathers.has_wallet(&entry.wallet) {
                            send_msg(
                                &tx,
                                &Msg::Refuse {
                                    reason: proto::REFUSE_BUSY,
                                },
                            );
                            self.ctx.health.refuse(proto::REFUSE_BUSY);
                            tx.close();
                            continue;
                        }
                        send_msg(
                            &tx,
                            &Msg::Queued {
                                mode: entry.mode,
                                waiting_s: 0,
                            },
                        );
                        let (wallet, mode) = (entry.wallet.clone(), entry.mode);
                        if entry.mode == MODE_CUP {
                            cup_wait.push(*entry);
                        } else {
                            q.push(*entry);
                        }
                        self.ctx.health.joins_ranked.fetch_add(1, Ordering::Relaxed);
                        sessions.insert(
                            session,
                            Sess {
                                tx,
                                at: None,
                                player: Some(wallet),
                                since: auth::now_unix(),
                                mode,
                                watching: None,
                                ip,
                            },
                        );
                    }
                    ToSim::Matched {
                        session,
                        entry,
                        id,
                        sides,
                        side,
                        ip,
                        tx,
                    } => {
                        // Busy as at a queue (a match, a cup between its
                        // heats, the queue), or this match already began.
                        let playing = rooms.iter().flatten().any(|r| r.id == id);
                        let elsewhere = busy(&rooms, &cups, &cup_wait, &q, &entry.wallet);
                        let refuse = if gathers.done(&id) {
                            // Started without it, or called off: never twice.
                            Some(proto::REFUSE_MATCH)
                        } else if playing || elsewhere {
                            Some(proto::REFUSE_BUSY)
                        } else {
                            None
                        };
                        if let Some(reason) = refuse {
                            send_msg(&tx, &Msg::Refuse { reason });
                            self.ctx.health.refuse(reason);
                            tx.close();
                            continue;
                        }
                        let (wallet, mode) = (entry.wallet.clone(), entry.mode);
                        send_msg(&tx, &Msg::Queued { mode, waiting_s: 0 });
                        // A reconnect: the session it replaces is done.
                        if let Some(old) = gathers.arrive(&id, sides, side, *entry, now) {
                            if let Some(s) = sessions.remove(&old) {
                                s.tx.close();
                            }
                        }
                        self.ctx.health.joins_ranked.fetch_add(1, Ordering::Relaxed);
                        sessions.insert(
                            session,
                            Sess {
                                tx,
                                at: None,
                                player: Some(wallet),
                                since: auth::now_unix(),
                                mode,
                                watching: None,
                                ip,
                            },
                        );
                    }
                    ToSim::Watch {
                        session,
                        target,
                        player,
                        ip,
                        tx,
                    } => {
                        let pick = watch_room(&rooms, &sessions, target, player.as_deref(), ip);
                        let Some(i) = pick else {
                            send_msg(
                                &tx,
                                &Msg::Refuse {
                                    reason: proto::REFUSE_NOTHING_TO_WATCH,
                                },
                            );
                            self.ctx.health.refuse(proto::REFUSE_NOTHING_TO_WATCH);
                            tx.close();
                            continue;
                        };
                        let r = rooms[i].as_mut().unwrap();
                        let follow = r.room.spectate_first();
                        for m in r.room.spectate_welcome(follow) {
                            send_msg(&tx, &m);
                        }
                        let team = r.room.world.fighters[follow as usize].team;
                        send_msg(&tx, &Msg::Follow { slot: follow, team });
                        r.watchers.push(session);
                        sessions.insert(
                            session,
                            Sess {
                                tx,
                                at: None,
                                player,
                                since: auth::now_unix(),
                                mode: proto::MODE_WATCH,
                                watching: Some((i, follow)),
                                ip,
                            },
                        );
                    }
                    ToSim::Follow { session } => {
                        let Some(s) = sessions.get_mut(&session) else { continue };
                        let Some((i, slot)) = s.watching else { continue };
                        let Some(r) = rooms[i].as_ref() else { continue };
                        let next = r.room.spectate_next(slot);
                        s.watching = Some((i, next));
                        let team = r.room.world.fighters[next as usize].team;
                        send_msg(&s.tx, &Msg::Follow { slot: next, team });
                    }
                    ToSim::Loaded { session } => {
                        let Some((i, slot)) = sessions.get(&session).and_then(|s| s.at) else {
                            continue;
                        };
                        if let Some(r) = rooms[i].as_mut() {
                            r.room.loaded(slot);
                        }
                    }
                    ToSim::Leave { session } => {
                        q.remove(session);
                        gathers.leave(session);
                        cup_wait.retain(|e| e.session != session);
                        for c in cups.iter_mut().flatten() {
                            c.leave(session);
                        }
                        let Some(s) = sessions.remove(&session) else { continue };
                        if let Some((i, _)) = s.watching {
                            if let Some(r) = rooms[i].as_mut() {
                                r.watchers.retain(|&w| w != session);
                                if r.watchers.is_empty() {
                                    r.room.spectate = None;
                                    if r.links.iter().all(|l| l.is_none()) && r.cup.is_none() && !r.room.ranked {
                                        rooms[i] = None;
                                    }
                                }
                            }
                            continue;
                        }
                        let Some((i, slot)) = s.at else { continue };
                        let Some(r) = rooms[i].as_mut() else { continue };
                        r.links[slot as usize] = None;
                        r.codes[slot as usize] = (0, false);
                        // What they did before they left counts; the match
                        // doesn't (a match over was reported at its end).
                        if r.over_at.is_none() {
                            if let Some(p) = played(r, slot as usize, false) {
                                let _ = self.jobs.send(Job::Stats(vec![stats::Line::Match(p)]));
                            }
                        }
                        r.players[slot as usize] = None;
                        // A cup's heat plays out even with nobody left: the
                        // cup waits on its result. So does a ranked match:
                        // its result is reported (a match left at 2–10 is
                        // still lost), never dropped.
                        if r.links.iter().all(|l| l.is_none())
                            && r.cup.is_none()
                            && !r.room.ranked
                            && r.watchers.is_empty()
                        {
                            // Nobody left to play or watch.
                            rooms[i] = None;
                        } else {
                            let out = r.room.leave(slot);
                            deliver(&sessions, r, out);
                        }
                    }
                }
            }

            // Then the inputs (one for a session gone or not seated yet is
            // nothing): the lane is bounded, so this is too.
            while let Ok((session, msg, rtt_ms)) = inputs.try_recv() {
                if let Some((i, slot)) = sessions.get(&session).and_then(|s| s.at) {
                    if let Some(r) = rooms[i].as_mut() {
                        r.room.set_link(slot, rtt_ms);
                        r.room.on_input(slot, &msg, now_ms);
                    }
                }
            }

            // Going down: those still waiting are let go (no match will
            // form), the rooms play out.
            if self.ctx.draining.load(Ordering::Relaxed) {
                for e in q.entries.drain(..).chain(cup_wait.drain(..)).chain(gathers.take_all()) {
                    if let Some(s) = sessions.remove(&e.session) {
                        send_msg(
                            &s.tx,
                            &Msg::Refuse {
                                reason: proto::REFUSE_FULL,
                            },
                        );
                        // Joined, then let go: not a hello turned away.
                        self.ctx.health.let_go.fetch_add(1, Ordering::Relaxed);
                        s.tx.close();
                    }
                }
            }
            self.ctx.rooms.store(rooms.iter().flatten().count(), Ordering::Relaxed);

            // Once a second: form matches, tell the waiting how long.
            if now.duration_since(last_second) >= Duration::from_secs(1) {
                last_second = now;
                seconds += 1;
                recent.prune(now);
                if seconds % ROSTER_EVERY_S == 1 {
                    let _ = self.jobs.send(Job::Roster(roster(&sessions, &rooms)));
                }
                self.open_modes(&rooms);
                while let Some(sides) = q.form(now) {
                    let id = format!("os-{:016x}", random_u64());
                    self.seat_ranked(&sides, id, &mut rooms, &mut sessions, &mut recent, now);
                }
                // Elo Pros' queue: a match everyone has come for starts;
                // one not everyone came for in time is called off.
                let due = gathers.due(now);
                for (id, sides) in due.start {
                    self.seat_ranked(&sides, id, &mut rooms, &mut sessions, &mut recent, now);
                }
                for e in due.called_off {
                    if let Some(s) = sessions.remove(&e.session) {
                        send_msg(
                            &s.tx,
                            &Msg::Refuse {
                                reason: proto::REFUSE_MATCH,
                            },
                        );
                        self.ctx.health.refuse(proto::REFUSE_MATCH);
                        s.tx.close();
                    }
                }
                // A cup: once enough have entered, when it fills or the wait
                // for more is up.
                if cup_wait.len() < self.cfg.cup_min.max(2) {
                    cup_ready_at = None;
                } else {
                    let since = *cup_ready_at.get_or_insert(now);
                    if cup_wait.len() >= self.cfg.cup_max || now.duration_since(since) >= self.cfg.cup_fill {
                        cup_ready_at = None;
                        let n = cup_wait.len().min(self.cfg.cup_max);
                        let entrants = cup_wait
                            .drain(..n)
                            .map(|e| cup::Entrant {
                                session: e.session,
                                wallet: e.wallet,
                                name: e.name,
                                look: e.look,
                                points: 0,
                                here: true,
                                final_place: None,
                            })
                            .collect();
                        let c = cup::Cup::new(format!("cup-{:016x}", random_u64()), entrants, self.cfg.cup_rounds);
                        let ci = match cups.iter().position(|c| c.is_none()) {
                            Some(i) => i,
                            None => {
                                cups.push(None);
                                cups.len() - 1
                            }
                        };
                        cups[ci] = Some(c);
                        self.start_stage(&cups, ci, &mut rooms, &mut sessions, &mut recent, now);
                    }
                }
                for e in q.entries.iter().chain(&cup_wait) {
                    if let Some(s) = sessions.get(&e.session) {
                        let waited = now.duration_since(e.since).as_secs().min(u16::MAX as u64) as u16;
                        send_msg(
                            &s.tx,
                            &Msg::Queued {
                                mode: e.mode,
                                waiting_s: waited,
                            },
                        );
                    }
                }
            }

            // A room whose linger is over closes first, here: it changes
            // the sessions. It sends nothing more: a cup's next heat may
            // seat its entrants this tick, and a snapshot of this room after
            // that welcome is another room's to them.
            let mut heats_done: Vec<HeatDone> = Vec::new();
            for entry in rooms.iter_mut() {
                let Some(r) = entry.as_mut() else { continue };
                // Watched, it plays on until its watchers have seen the end.
                let late = match (&r.room.spectate, r.watchers.is_empty()) {
                    (Some(l), false) => Duration::from_millis(u64::from(l.delay) * 1000 / u64::from(TICK_HZ)),
                    _ => Duration::ZERO,
                };
                let done = r
                    .over_at
                    .is_some_and(|t| now.duration_since(t) >= self.cfg.linger + late);
                if !done {
                    continue;
                }
                for id in r.watchers.iter() {
                    if let Some(s) = sessions.get_mut(id) {
                        send_msg(&s.tx, &Msg::Bye);
                        s.tx.close();
                        s.watching = None;
                    }
                }
                for id in r.links.iter().flatten() {
                    if let Some(s) = sessions.get_mut(id) {
                        // A cup's entrants stay on for the next heat.
                        if r.cup.is_none() {
                            send_msg(&s.tx, &Msg::Bye);
                            s.tx.close();
                        }
                        s.at = None;
                    }
                }
                if let Some((ci, h)) = r.cup {
                    // Seats 0.. are the heat's entrants in its order.
                    let n = cups[ci].as_ref().map_or(0, |c| c.heats.get(h).map_or(0, |x| x.0.len()));
                    heats_done.push((ci, h, r.id.clone(), r.room.world.score[..n].to_vec()));
                }
                *entry = None;
            }
            // Then every other room steps, side by side on the pool. Rooms
            // share nothing and each is stepped by one thread, so this is
            // the same as one after another: each room's world, replay and
            // messages are what they would be alone. The sessions are only
            // read while they step.
            let step = |r: &mut Live| self.step_room(r, &sessions, now, now_ms);
            match &self.pool {
                Some(pool) if rooms.iter().flatten().nth(1).is_some() => {
                    pool.install(|| rooms.par_iter_mut().filter_map(Option::as_mut).for_each(step));
                }
                _ => rooms.iter_mut().filter_map(Option::as_mut).for_each(step),
            }
            for (ci, h, id, scores) in heats_done {
                let Some(c) = cups[ci].as_mut() else { continue };
                let members = c.heats.get(h).map(|x| x.0.clone()).unwrap_or_default();
                let advanced = c.record(h, &id, &scores);
                if !advanced {
                    // Others still play this round: where these stand now.
                    for i in members {
                        tell_cup(c, i, &sessions);
                    }
                } else if c.stage == cup::Stage::Done || self.ctx.draining.load(Ordering::Relaxed) {
                    // Going down: no new stage (its rooms would hold the
                    // drain up); the cup ends at the standings it has.
                    if c.stage != cup::Stage::Done {
                        eprintln!(
                            "oneshot: drain: {} ends after {} heat(s), at its standings",
                            c.id,
                            c.played.len()
                        );
                        c.end_now();
                    }
                    self.finish_cup(c, &mut sessions);
                    cups[ci] = None;
                } else {
                    self.start_stage(&cups, ci, &mut rooms, &mut sessions, &mut recent, now);
                }
            }

            next += tick;
            let done = Instant::now();
            self.ctx.health.tick(done - now, tick);
            if done < next {
                std::thread::sleep(next - done);
            } else if done - next > tick * 8 {
                // Far behind (a stall): don't sprint to catch up; those
                // ticks are never run, and counted.
                let lost = ((done - next).as_nanos() / tick.as_nanos()) as u64;
                self.ctx.health.ticks_skipped.fetch_add(lost, Ordering::Relaxed);
                next = done;
            }
        }
    }

    /// A ranked match whose sides are all here: a room of its own, seats
    /// alternating sides (0 and 2 red, 1 and 3 blue), each told where it
    /// sits. `id` is what its result is reported under: the server's own
    /// for its queue, Elo Pros' match id for one its queue formed.
    fn seat_ranked(
        &self,
        sides: &[Vec<queue::Entry>],
        id: String,
        rooms: &mut Vec<Option<Live>>,
        sessions: &mut BTreeMap<u64, Sess>,
        recent: &mut Recent,
        now: Instant,
    ) {
        let Some(mode) = sides.iter().flatten().next().map(|e| e.mode) else {
            return;
        };
        let avoid = recent.of(sides.iter().flatten().map(|p| p.wallet.as_str()));
        let mut live = Live {
            room: self.new_room(mode, true, &avoid),
            id,
            mode,
            links: [None; MAX_FIGHTERS],
            wallets: Default::default(),
            players: Default::default(),
            joined: [0; MAX_FIGHTERS],
            over_at: None,
            cup: None,
            watchers: Vec::new(),
            private: false,
            codes: [(0, false); MAX_FIGHTERS],
            holds: Vec::new(),
            out: Vec::new(),
            events_seen: 0,
        };
        let mut outs = Vec::new();
        for (side, players) in sides.iter().enumerate() {
            for (k, p) in players.iter().enumerate() {
                // Seats alternate sides: 0 and 2 red, 1 and 3 blue.
                let slot = side + 2 * k;
                outs.push(live.room.seat_person(slot, p.name, p.look));
                live.links[slot] = Some(p.session);
                live.wallets[slot] = Some(p.wallet.clone());
                live.players[slot] = Some(p.wallet.clone());
                recent.note(p.wallet.clone(), live.room.world.map.id, now);
            }
        }
        let i = place_room(rooms, live);
        for side in sides {
            for p in side {
                if let Some(s) = sessions.get_mut(&p.session) {
                    let slot = rooms[i]
                        .as_ref()
                        .unwrap()
                        .links
                        .iter()
                        .position(|l| *l == Some(p.session));
                    s.at = slot.map(|sl| (i, sl as u8));
                }
            }
        }
        for out in outs {
            deliver(sessions, rooms[i].as_ref().unwrap(), out);
        }
    }

    /// One room's tick, on whichever thread steps it: its snapshots and
    /// messages to its sessions and watchers, and its end noticed (its
    /// stats, replay and report handed to the jobs task).
    fn step_room(&self, r: &mut Live, sessions: &BTreeMap<u64, Sess>, now: Instant, now_ms: f64) {
        let mut out = std::mem::take(&mut r.out);
        let links = &r.links;
        r.room.step_into(now_ms, &mut out, |slot, _, bytes| {
            let link = links[slot as usize].and_then(|id| sessions.get(&id));
            if let (Some(s), false) = (link, bytes.is_empty()) {
                // Full means the session is behind: this snapshot goes,
                // the next replaces it.
                s.tx.datagram(bytes);
            }
        });
        deliver(sessions, r, out.drain(..));
        r.out = out;
        let lost = r.room.events_dropped();
        if lost > r.events_seen {
            self.ctx
                .health
                .events_dropped
                .fetch_add(lost - r.events_seen, Ordering::Relaxed);
        }
        r.events_seen = lost;
        spectate(sessions, r);
        if r.room.world.phase == Phase::Over && r.over_at.is_none() {
            r.over_at = Some(now);
            self.finish(r);
        }
    }

    /// Seats the heats of cup `ci`'s current stage, each in a ranked room
    /// of its own, and tells every entrant where they stand.
    fn start_stage(
        &self,
        cups: &[Option<cup::Cup>],
        ci: usize,
        rooms: &mut Vec<Option<Live>>,
        sessions: &mut BTreeMap<u64, Sess>,
        recent: &mut Recent,
        now: Instant,
    ) {
        let Some(c) = cups[ci].as_ref() else { return };
        for i in 0..c.entrants.len() {
            tell_cup(c, i, sessions);
        }
        for (h, (members, _)) in c.heats.iter().enumerate() {
            // A heat is on a map its entrants didn't just play: each round
            // of a cup on another.
            let avoid = recent.of(members.iter().map(|&i| c.entrants[i].wallet.as_str()));
            let mut live = Live {
                room: self.new_room(MODE_CUP, true, &avoid),
                id: format!("{}-{}-{}", c.id, c.round_no(), h + 1),
                mode: MODE_CUP,
                links: [None; MAX_FIGHTERS],
                wallets: Default::default(),
                players: Default::default(),
                joined: [0; MAX_FIGHTERS],
                over_at: None,
                cup: Some((ci, h)),
                watchers: Vec::new(),
                private: false,
                codes: [(0, false); MAX_FIGHTERS],
                holds: Vec::new(),
                out: Vec::new(),
                events_seen: 0,
            };
            let mut outs = Vec::new();
            for (slot, &i) in members.iter().enumerate() {
                let e = &c.entrants[i];
                outs.push(live.room.seat_person(slot, e.name, e.look));
                live.links[slot] = Some(e.session);
                live.wallets[slot] = Some(e.wallet.clone());
                live.players[slot] = Some(e.wallet.clone());
                recent.note(e.wallet.clone(), live.room.world.map.id, now);
            }
            let at = place_room(rooms, live);
            for (slot, &i) in members.iter().enumerate() {
                if let Some(s) = sessions.get_mut(&c.entrants[i].session) {
                    s.at = Some((at, slot as u8));
                }
            }
            for out in outs {
                deliver(sessions, rooms[at].as_ref().unwrap(), out);
            }
        }
    }

    /// A cup is over: the standings to everyone still here, then goodbye;
    /// its file beside the replays; and the top places' heats to the
    /// review queue, which a prize waits on (docs/DESIGN.md §13).
    fn finish_cup(&self, c: &cup::Cup, sessions: &mut BTreeMap<u64, Sess>) {
        let lines = c
            .entrants
            .iter()
            .enumerate()
            .map(|(i, e)| {
                stats::Line::Cup(stats::Cupped {
                    player: e.wallet.clone(),
                    place: c.place_of(i),
                })
            })
            .collect();
        let _ = self.jobs.send(Job::Stats(lines));
        for (i, e) in c.entrants.iter().enumerate() {
            tell_cup(c, i, sessions);
            if let Some(s) = sessions.get_mut(&e.session) {
                send_msg(&s.tx, &Msg::Bye);
                s.tx.close();
                s.at = None;
            }
        }
        let Some(dir) = &self.cfg.replays else { return };
        let doc = c.doc();
        let _ = self.jobs.send(Job::Write {
            path: dir.join("cups").join(format!("{}.json", c.id)),
            bytes: serde_json::to_vec_pretty(&doc).unwrap_or_default(),
        });
        let top = self.cfg.cup_review_top;
        let seats: Vec<serde_json::Value> = c
            .standings()
            .into_iter()
            .filter(|&i| c.place_of(i) <= top)
            .map(|i| {
                let e = &c.entrants[i];
                let replays: Vec<String> = c
                    .played
                    .iter()
                    .filter(|(_, _, m, _)| m.contains(&i))
                    .map(|(_, id, _, _)| format!("{id}.osr"))
                    .collect();
                serde_json::json!({
                    "name": e.name.as_str(),
                    "wallet": e.wallet,
                    "place": c.place_of(i),
                    "flags": [format!("cup prize: place {}", c.place_of(i))],
                    "replays": replays,
                })
            })
            .collect();
        let review = serde_json::json!({
            "id": c.id,
            "kind": "cup prize",
            "why": "a prize is paid only after these heats are watched with fog off (docs/DESIGN.md §13)",
            "seats": seats,
            "cup": doc,
        });
        let _ = self.jobs.send(Job::Write {
            path: dir.join("review").join(format!("{}.json", c.id)),
            bytes: serde_json::to_vec_pretty(&review).unwrap_or_default(),
        });
    }

    /// A match ended: its stats now; its replay and aim review to disk (and
    /// to `review/` if they cross a line), and a ranked one's result to Elo
    /// Pros, once the jobs task has hashed the replay.
    fn finish(&self, r: &mut Live) {
        let lines: Vec<stats::Line> = (0..MAX_FIGHTERS)
            .filter_map(|s| played(r, s, true))
            .map(stats::Line::Match)
            .collect();
        if !lines.is_empty() {
            let _ = self.jobs.send(Job::Stats(lines));
        }
        let salt_commit = hex(&r.room.salt_commit());
        let files = self.cfg.replays.as_ref().map(|dir| {
            let (doc, flagged) = watch_doc(r, &salt_commit);
            (dir.clone(), r.id.clone(), doc, flagged)
        });
        let report = self.report(r, salt_commit);
        if files.is_none() && report.is_none() {
            return;
        }
        let _ = self.jobs.send(Job::Finished(Box::new(Finished {
            replay: r.room.take_replay(),
            files,
            report,
        })));
    }

    /// A ranked match's result, but for the replay's hash.
    fn report(&self, r: &Live, salt_commit: String) -> Option<elo::Report> {
        if !r.room.ranked {
            return None;
        }
        let w = &r.room.world;
        // Sides and their scores: each seat for a duel or a cup heat, each
        // team for doubles (seats 0 and 2 red, 1 and 3 blue).
        let sides: Vec<(Vec<String>, i32)> = if r.mode == MODE_DOUBLES {
            (0..2)
                .map(|t| {
                    let ws = (0..MAX_FIGHTERS)
                        .filter(|s| s & 1 == t)
                        .filter_map(|s| r.wallets[s].clone())
                        .collect();
                    (ws, w.score[t])
                })
                .collect()
        } else {
            (0..MAX_FIGHTERS)
                .filter_map(|s| r.wallets[s].clone().map(|wl| (vec![wl], w.score[s])))
                .collect()
        };
        if sides.len() < 2 || sides.iter().any(|(ws, _)| ws.is_empty()) {
            return None;
        }
        // A duel's or doubles' draw shares first; a cup heat's tie shares
        // the worse place, as its points do (`cup::places`).
        let scores: Vec<i32> = sides.iter().map(|(_, sc)| *sc).collect();
        let places: Vec<u32> = if r.mode == MODE_CUP {
            cup::places(&scores)
        } else {
            scores
                .iter()
                .map(|sc| 1 + scores.iter().filter(|o| *o > sc).count() as u32)
                .collect()
        };
        let placed = sides.iter().zip(places).map(|((ws, _), p)| (ws.clone(), p)).collect();
        Some(elo::Report {
            id: r.id.clone(),
            ladder: match r.mode {
                MODE_DOUBLES => "doubles",
                MODE_CUP => "cup",
                _ => "duel",
            }
            .into(),
            sides: placed,
            replay_sha256: String::new(),
            salt_commit,
        })
    }
}

/// How often the sim hands over who it holds (`online`).
const ROSTER_EVERY_S: u64 = 5;

/// What a mode is called, to the people who see someone play it.
fn mode_name(mode: u8) -> &'static str {
    match mode {
        MODE_DUEL => "Ranked Duel",
        MODE_DOUBLES => "Ranked Doubles",
        MODE_FFA => "Free for all",
        MODE_GUN_GAME => "Gun Game",
        MODE_CUP => "A cup",
        MODE_BIG_TEAM => "Big Team",
        MODE_CTF => "Capture the Flag",
        MODE_SIEGE => "Siege",
        _ => "Against the bot",
    }
}

/// Seat `s`'s side's score and the best other side's, as `own–theirs`.
fn score_line(r: &Live, s: usize) -> String {
    let w = &r.room.world;
    let (own, n) = if w.rules.teams { (s & 1, 2) } else { (s, r.room.size) };
    let theirs = (0..n).filter(|&i| i != own).map(|i| w.score[i]).max().unwrap_or(0);
    format!("{}\u{2013}{}", w.score[own], theirs)
}

/// One player's status line: the mode and the map, and in a duel, doubles
/// or against the bot, the score.
fn presence(r: &Live, s: usize) -> String {
    let mut line = format!("{} on {}", mode_name(r.mode), r.room.world.map.name);
    if matches!(r.mode, MODE_DUEL | MODE_DOUBLES | MODE_CASUAL) {
        line = format!("{line} \u{b7} {}", score_line(r, s));
    }
    online::clip(line)
}

/// Who this slot holds: every session (guests count), and each signed-in
/// player's line.
fn roster(sessions: &BTreeMap<u64, Sess>, rooms: &[Option<Live>]) -> online::Slot {
    let online = sessions
        .values()
        .filter_map(|s| {
            let player = s.player.clone()?;
            let status = match s.at.and_then(|(i, slot)| Some((rooms.get(i)?.as_ref()?, slot))) {
                Some((r, slot)) => presence(r, slot as usize),
                None if s.mode == proto::MODE_WATCH => match s.watching.and_then(|(i, _)| rooms.get(i)?.as_ref()) {
                    Some(r) => online::clip(format!("Watching {} on {}", mode_name(r.mode), r.room.world.map.name)),
                    None => "Watching a match".into(),
                },
                None if s.mode == MODE_CUP => "Waiting for a cup".into(),
                None => format!("In the {} queue", mode_name(s.mode)),
            };
            Some(online::Row {
                player,
                since: s.since,
                status,
            })
        })
        .collect();
    online::Slot {
        at: auth::now_unix(),
        players: sessions.len(),
        rooms: rooms.iter().flatten().count(),
        online,
    }
}

/// Seat `s`'s person's part in room `r`, if they signed in: `finished` at
/// the end of the match, else as they leave it.
fn played(r: &Live, s: usize, finished: bool) -> Option<stats::Played> {
    let player = r.players[s].clone()?;
    if !matches!(r.room.seats[s], room::Seat::Person(..)) || !stats::is_player(&player) {
        return None;
    }
    let w = &r.room.world;
    let side = if w.rules.teams { (s & 1) as u8 } else { s as u8 };
    // A private room is the party's own, bots and all: what they did there
    // counts, a win over it doesn't.
    let won = finished && !r.private && w.winner != sim::world::NO_WINNER && w.winner == side;
    // From when they sat down or play began, whichever was later: the
    // countdown is no one's time (a fastest Gun Game win is comparable).
    let from = r.joined[s].max(r.room.began.unwrap_or(w.tick));
    let ticks = w.tick.saturating_sub(from) as u64;
    Some(stats::Played {
        player,
        mode: r.mode,
        ranked: r.room.ranked,
        finished,
        won,
        ms: ticks * 1000 / TICK_HZ as u64,
        tally: *r.room.medals().tally(s),
    })
}

/// A seat's profile numbers beside the lines (`room::watch`): rates, so
/// matches of any length compare.
fn profile_json(st: &room::watch::Stats) -> serde_json::Value {
    let r3 = |x: f32| (x * 1000.0).round() / 1000.0;
    let alive_s = st.alive_ticks as f32 / TICK_HZ as f32;
    let per_s = |x: f32| if alive_s > 0.0 { r3(x / alive_s) } else { 0.0 };
    serde_json::json!({
        "alive_s": r3(alive_s),
        "accuracy": r3(st.hits as f32 / st.shots.max(1) as f32),
        "head_rate": r3(st.head_rate()),
        "reactions_ms": st.reactions_ms,
        "snap_share": r3(st.snaps as f32 / st.reactions_ms.len().max(1) as f32),
        "preaim_ms": st.preaim_ms,
        "on_target": r3(st.on_target()),
        "sight_s": r3(st.sight_ticks as f32 / TICK_HZ as f32),
        "turn_deg_s": per_s(st.turn_deg),
        "jerk_per_turn": r3(st.jerk_deg / st.turn_deg.max(1.0)),
        "speed_m_s": per_s(st.moved_m),
        "air_share": r3(st.air_ticks as f32 / st.alive_ticks.max(1) as f32),
        "throws": st.throws,
        "melees": st.melees,
        "deaths": st.deaths,
        "unanswered_share": r3(st.unanswered as f32 / st.deaths.max(1) as f32),
    })
}

/// A match's aim review: each person's numbers and the lines they cross
/// (`room::watch`), and their profile numbers. Whether any did.
/// Its `replay_sha256` is the jobs task's to fill in.
fn watch_doc(r: &Live, salt_commit: &str) -> (serde_json::Value, bool) {
    let mut flagged = false;
    let seats: Vec<serde_json::Value> = (0..MAX_FIGHTERS)
        .filter_map(|s| {
            let room::Seat::Person(_, name) = &r.room.seats[s] else {
                return None;
            };
            let st = &r.room.watch().stats[s];
            let flags = st.flags();
            flagged |= !flags.is_empty();
            let t = r.room.medals().tally(s);
            let medals: serde_json::Map<String, serde_json::Value> = t
                .medals
                .iter()
                .enumerate()
                .filter(|(_, &n)| n > 0)
                .map(|(id, &n)| (proto::medal::slug(id as u8), n.into()))
                .collect();
            Some(serde_json::json!({
                "seat": s,
                "name": name.as_str(),
                "wallet": r.wallets[s],
                "shots": st.shots,
                "hits": st.hits,
                "head_hits": st.head_hits,
                "head_rate": (st.head_rate() * 1000.0).round() / 1000.0,
                "kills": st.kills,
                "sightings_hit": st.reactions_ms.len(),
                "median_reaction_ms": st.median_reaction_ms(),
                "snaps": st.snaps,
                "preaim_ms": st.preaim_ms,
                "flags": flags,
                "profile": profile_json(st),
                // The carnage report: what the players saw at the end.
                "carnage": {
                    "kills": t.kills,
                    "deaths": t.deaths,
                    "assists": t.assists,
                    "betrayals": t.betrayals,
                    "suicides": t.suicides,
                    "damage": t.damage,
                    "taken": t.taken,
                    "best_spree": t.best_spree,
                    "best_multi": t.best_multi,
                    "medals": medals,
                },
            }))
        })
        .collect();
    // Those who left before the end: their numbers as they left.
    let mut seats = seats;
    for (s, name, st) in &r.room.departed {
        let flags = st.flags();
        flagged |= !flags.is_empty();
        seats.push(serde_json::json!({
            "seat": s,
            "name": name.as_str(),
            "left": true,
            "shots": st.shots,
            "hits": st.hits,
            "head_hits": st.head_hits,
            "head_rate": (st.head_rate() * 1000.0).round() / 1000.0,
            "kills": st.kills,
            "sightings_hit": st.reactions_ms.len(),
            "median_reaction_ms": st.median_reaction_ms(),
            "snaps": st.snaps,
            "preaim_ms": st.preaim_ms,
            "flags": flags,
            "profile": profile_json(st),
        }));
    }
    let doc = serde_json::json!({
        "id": r.id,
        "ranked": r.room.ranked,
        "mode": r.mode,
        "replay": format!("{}.osr", r.id),
        "replay_sha256": "",
        "salt_commit": salt_commit,
        "seats": seats,
    });
    (doc, flagged)
}

/// Where entrant `i` of a cup stands, to them.
fn tell_cup(c: &cup::Cup, i: usize, sessions: &BTreeMap<u64, Sess>) {
    let e = &c.entrants[i];
    let Some(s) = sessions.get(&e.session) else { return };
    let done = c.stage == cup::Stage::Done;
    send_msg(
        &s.tx,
        &Msg::Cup {
            round: if done { c.rounds + 1 } else { c.round_no() },
            rounds: c.rounds,
            entrants: c.entrants.len().min(255) as u8,
            place: c.place_of(i).min(255) as u8,
            points: e.points.min(u16::MAX as u32) as u16,
            done,
        },
    );
}

/// Whether `wallet` is already somewhere ranked: in a match still playing,
/// waiting for a cup or in one between its heats, or in the queue. One
/// test for every way in (a queue, an Elo Pros match), so they never differ.
fn busy(
    rooms: &[Option<Live>],
    cups: &[Option<cup::Cup>],
    cup_wait: &[queue::Entry],
    q: &queue::Queue,
    wallet: &str,
) -> bool {
    let is = |w: &str| w.eq_ignore_ascii_case(wallet);
    rooms
        .iter()
        .flatten()
        .any(|r| r.over_at.is_none() && r.wallets.iter().flatten().any(|w| is(w)))
        || cup_wait.iter().any(|e| is(&e.wallet))
        || cups
            .iter()
            .flatten()
            .any(|c| c.stage != cup::Stage::Done && c.entrants.iter().any(|e| e.here && is(&e.wallet)))
        || q.has_wallet(wallet)
}

/// The room for a watcher: party `target`'s, or with 0 the public match
/// with the most people in it; never one `player` sits in, nor one anyone
/// connected from `ip` sits in (a second tab, a guest, an alt beside the
/// player would show them the enemy's own view), nor a full one.
fn watch_room(
    rooms: &[Option<Live>],
    sessions: &BTreeMap<u64, Sess>,
    target: u32,
    player: Option<&str>,
    ip: Option<IpAddr>,
) -> Option<usize> {
    let sits = |r: &Live| {
        player.is_some_and(|p| r.players.iter().flatten().any(|q| q == p))
            || (ip.is_some()
                && r.links
                    .iter()
                    .flatten()
                    .any(|id| sessions.get(id).is_some_and(|s| s.ip == ip)))
    };
    rooms
        .iter()
        .enumerate()
        .filter_map(|(i, r)| Some((i, r.as_ref()?)))
        .filter(|(_, r)| r.over_at.is_none() && r.watchers.len() < MAX_WATCHERS && !sits(r))
        .filter(|(_, r)| match target {
            0 => !r.private,
            code => r.links.iter().zip(&r.codes).any(|(l, c)| l.is_some() && c.0 == code),
        })
        .max_by_key(|(i, r)| (r.links.iter().flatten().count(), usize::MAX - i))
        .map(|(i, _)| i)
}

/// What a watched room's watchers are due this tick: the seat each follows,
/// as it was sent `delay` ago, and that seat's messages.
fn spectate(sessions: &BTreeMap<u64, Sess>, r: &mut Live) {
    if r.watchers.is_empty() {
        return;
    }
    let tick = r.room.world.tick;
    let Some(line) = r.room.spectate.as_mut() else { return };
    let (frames, msgs) = line.due(tick);
    // Each message framed once, shared by the watchers it goes to.
    let mut f = [0u8; proto::MAX_STREAM_MSG + 2];
    let framed: Vec<(Option<u8>, std::sync::Arc<[u8]>)> = msgs
        .iter()
        .filter_map(|(to, m)| frame_msg(m, &mut f).ok().map(|n| (*to, f[..n].into())))
        .collect();
    for id in &r.watchers {
        let Some(s) = sessions.get(id) else { continue };
        let Some((_, follow)) = s.watching else { continue };
        for (to, bytes) in &framed {
            if to.is_none_or(|t| t == follow) {
                s.tx.stream(bytes.clone());
            }
        }
        for fr in &frames {
            if let Some(b) = &fr.snaps[follow as usize] {
                s.tx.datagram(b);
            }
        }
    }
}

/// Lowers the calling thread's priority by `nice` (Linux keeps nice a
/// thread; raising it needs no privilege). Elsewhere, nothing.
pub fn lower_this_thread(nice: i32) {
    #[cfg(target_os = "linux")]
    if nice > 0 {
        // SAFETY: gettid and setpriority on the calling thread touch no memory.
        unsafe {
            let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
            libc::setpriority(libc::PRIO_PROCESS, tid, nice.min(19));
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = nice;
}

/// Ticks between snapshots for `hz` a second: the nearest rate that
/// divides the tick (128: 64, 32, 16).
pub fn snap_every(hz: u32) -> u32 {
    (TICK_HZ / hz.clamp(16, TICK_HZ / room::SNAP_EVERY))
        .next_power_of_two()
        .min(8)
}

/// Room messages to their seats' sessions, each framed once and its bytes
/// shared by everyone it goes to.
fn deliver(sessions: &BTreeMap<u64, Sess>, r: &Live, out: impl IntoIterator<Item = (Option<u8>, Msg)>) {
    let mut f = [0u8; proto::MAX_STREAM_MSG + 2];
    for (to, m) in out {
        let Ok(n) = frame_msg(&m, &mut f) else { continue };
        let mut shared: Option<std::sync::Arc<[u8]>> = None;
        for (s, id) in r.links.iter().enumerate() {
            let Some(sess) = id.and_then(|id| sessions.get(&id)) else {
                continue;
            };
            if to.is_none_or(|t| t as usize == s) {
                let bytes = shared.get_or_insert_with(|| f[..n].into());
                sess.tx.stream(bytes.clone());
            }
        }
    }
}

/// Bytes as lower-case hex.
pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::map::{ROT_BIG_TEAM, ROT_CTF, ROT_DOUBLES, ROT_FFA};

    /// Each rotation is the maps that name it or fit it by size, never one
    /// out of the rotation; Capture the Flag only where there are flags.
    #[test]
    fn a_rotation_is_its_maps() {
        let doubles = pool(Vec::new(), ROT_DOUBLES);
        assert!(!doubles.is_empty());
        for id in &doubles {
            let d = sim::maps::ALL.iter().find(|d| d.id == *id).unwrap();
            assert!(!d.practice && !d.big(), "{}", d.name);
        }
        let big = pool(Vec::new(), ROT_BIG_TEAM);
        assert!(big
            .iter()
            .all(|id| sim::maps::ALL.iter().any(|d| d.id == *id && d.big())));
        let ctf = pool(Vec::new(), ROT_CTF);
        assert!(!ctf.is_empty());
        assert!(ctf
            .iter()
            .all(|id| sim::maps::ALL.iter().any(|d| d.id == *id && d.flags.len() == 2)));
        let siege = pool(Vec::new(), sim::map::ROT_SIEGE);
        assert!(!siege.is_empty());
        assert!(siege.iter().all(|id| sim::maps::ALL
            .iter()
            .any(|d| d.id == *id && d.siege.len() == sim::consts::SIEGE_NODES)));
        // Free for all is every map in the rotation, Dry Fork too; but
        // not one kept to Big Team (Icefall: too far across for eight).
        let every: Vec<u8> = sim::maps::ALL
            .iter()
            .filter(|d| !d.practice && d.modes != ROT_BIG_TEAM)
            .map(|d| d.id)
            .collect();
        assert_eq!(pool(Vec::new(), ROT_FFA), every);
        // A server's own list is played as given.
        assert_eq!(pool(vec![7, 0], ROT_FFA), vec![7, 0]);
    }

    /// The maps just played are left out, the newest first, never all.
    #[test]
    fn a_pick_steers_off_what_was_just_played() {
        assert_eq!(steer_off(vec![0, 1, 2], &[1, 2]), vec![0]);
        assert_eq!(steer_off(vec![0, 1], &[1, 0]), vec![0]);
        assert_eq!(steer_off(vec![5], &[5]), vec![5]);
        assert_eq!(steer_off(vec![0, 1, 2], &[]), vec![0, 1, 2]);
    }

    /// What each played, newest first, two deep, forgotten in time; a group
    /// is everyone's, the newest first.
    #[test]
    fn the_recent_maps_are_each_players_newest_first() {
        let mut r = Recent::default();
        let t0 = Instant::now();
        r.note("a".into(), 1, t0);
        r.note("a".into(), 2, t0);
        r.note("a".into(), 3, t0);
        r.note("b".into(), 4, t0);
        assert_eq!(r.of(["a"]), vec![3, 2]);
        assert_eq!(r.of(["a", "b"]), vec![3, 4, 2]);
        assert_eq!(r.of(["nobody"]), Vec::<u8>::new());
        // The same map again is still one.
        r.note("b".into(), 4, t0);
        assert_eq!(r.of(["b"]), vec![4]);
        assert_eq!(Recent::key(None, 0), None);
        assert_eq!(Recent::key(None, 0x2a).as_deref(), Some("party:0000002a"));
        r.prune(t0 + RECENT_FOR);
        assert!(r.by.is_empty());
    }

    /// The server's pick for a group is never what it just played while
    /// the rotation has another.
    #[test]
    fn a_group_is_not_handed_the_map_it_just_played() {
        let rot = ROT_DOUBLES;
        let all = pool(Vec::new(), rot);
        assert!(all.len() >= 2, "{all:?}");
        for seed in 0..64u64 {
            let first = all[(seed % all.len() as u64) as usize];
            let left = steer_off(all.clone(), &[first]);
            assert!(!left.contains(&first));
            assert!(!left.is_empty());
        }
    }
}

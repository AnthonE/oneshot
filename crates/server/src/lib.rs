//! The authoritative server: rooms on one thread at the sim's tick,
//! WebTransport sessions on tokio around it, and a small HTTP server for the
//! page (docs/DESIGN.md §2, §4, §11, §14).
//!
//! A casual player gets a room of their own against a bot. A ranked player
//! (an Elo Pros pass with a wallet) waits in the queue until a match forms,
//! then plays in a room of people only; when it ends its result goes to the
//! Elo Pros ladder and its replay to disk.
//!
//! The sim thread never blocks on the network: sessions hand it what they
//! got over a channel, and it hands each session its datagrams and stream
//! messages over a bounded one it only `try_send`s to. A full queue drops a
//! snapshot (the next replaces it) rather than stall the tick. Reports and
//! replay files go to a tokio task for the same reason.

pub mod auth;
pub mod cup;
pub mod elo;
pub mod http;
pub mod net;
pub mod queue;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use proto::{
    encode_snapshot, frame_msg, InputMsg, Msg, Name, MAX_DATAGRAM, MODE_BIG_TEAM, MODE_CASUAL, MODE_CUP, MODE_DOUBLES,
    MODE_FFA, MODE_GUN_GAME,
};
use room::Room;
use sim::consts::{MAX_FIGHTERS, TICK_HZ};
use sim::map::Map;
use sim::world::{Phase, Rules};
use tokio::sync::mpsc as tmpsc;

/// What a session is sent.
#[derive(Debug)]
pub enum Out {
    Datagram(Vec<u8>),
    Stream(Vec<u8>),
    Close,
}

/// What sessions tell the sim thread.
pub enum ToSim {
    /// Casual: a room against a bot, or a seat in a shared free-for-all or
    /// Gun Game room where bots fill the rest.
    Casual {
        session: u64,
        name: Name,
        mode: u8,
        /// A skin checked against the player's wallet.
        skin: u8,
        tx: tmpsc::Sender<Out>,
    },
    /// Into the ranked queue.
    Queue {
        session: u64,
        entry: Box<queue::Entry>,
        tx: tmpsc::Sender<Out>,
    },
    Input {
        session: u64,
        msg: Box<InputMsg>,
    },
    Leave {
        session: u64,
    },
}

/// What the sim thread hands off: slow work, off the tick.
pub enum Job {
    Report(elo::Report),
    /// A file to write: a replay, its watch, a review.
    Write {
        path: PathBuf,
        bytes: Vec<u8>,
    },
}

/// Room for this many messages to one session before snapshots drop.
pub const OUT_QUEUE: usize = 64;
/// A finished match stays up this long, for the end screen (`Config::linger`).
pub const LINGER: Duration = Duration::from_secs(10);

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
    /// Team.
    pub rules: Rules,
    pub ffa: Rules,
    pub gun_game: Rules,
    pub big_team: Rules,
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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            bind: "0.0.0.0:4433".parse().unwrap(),
            http: Some("0.0.0.0:8080".parse().unwrap()),
            web_dir: "web/dist".into(),
            cert: None,
            rules: Rules::duel(),
            ffa: Rules::ffa(),
            gun_game: Rules::gun_game(),
            big_team: Rules::big_team(),
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
            maps: Vec::new(),
            slug: "oneshot".into(),
            domains: vec!["localhost".into(), "127.0.0.1".into()],
            elo_pubkey: None,
            elo_origin: "https://elopros.com".into(),
            elo_key: None,
            replays: None,
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
}

pub struct Handle {
    pub addr: SocketAddr,
    pub http: Option<SocketAddr>,
    /// SHA-256 of the certificate, for `serverCertificateHashes`.
    pub cert_hash: [u8; 32],
    pub stop: Arc<AtomicBool>,
    pub ctx: Arc<Ctx>,
}

/// Starts the server on the current tokio runtime. Returns once it is
/// listening.
pub async fn run(cfg: Config) -> Result<Handle, String> {
    let (endpoint, cert_hash) = net::endpoint(&cfg).await?;
    let addr = endpoint.local_addr().map_err(|e| e.to_string())?;
    let (to_sim, from_net) = mpsc::channel::<ToSim>();
    let (jobs_tx, jobs_rx) = tmpsc::unbounded_channel::<Job>();
    let stop = Arc::new(AtomicBool::new(false));

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
        nonces: auth::Nonces::default(),
        elo: elo::Elo::new(&cfg.elo_origin, &cfg.slug, key),
        sessions: AtomicU64::new(1),
    });

    let sim = Sim {
        cfg: cfg.clone(),
        rx: from_net,
        jobs: jobs_tx,
        stop: stop.clone(),
    };
    std::thread::Builder::new()
        .name("sim".into())
        .spawn(move || sim.run())
        .map_err(|e| e.to_string())?;
    tokio::spawn(jobs(ctx.clone(), jobs_rx));
    tokio::spawn(net::accept_loop(endpoint, to_sim, ctx.clone()));
    let http = match cfg.http {
        Some(at) => Some(http::serve(at, cfg.web_dir.clone(), addr, cert_hash, ctx.clone()).await?),
        None => None,
    };
    Ok(Handle {
        addr,
        http,
        cert_hash,
        stop,
        ctx,
    })
}

async fn jobs(ctx: Arc<Ctx>, mut rx: tmpsc::UnboundedReceiver<Job>) {
    while let Some(j) = rx.recv().await {
        match j {
            Job::Write { path, bytes } => {
                if let Some(dir) = path.parent() {
                    let _ = tokio::fs::create_dir_all(dir).await;
                }
                if let Err(e) = tokio::fs::write(&path, bytes).await {
                    eprintln!("oneshot: {}: {e}", path.display());
                }
            }
            Job::Report(r) => {
                let ctx = ctx.clone();
                tokio::spawn(async move {
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
    }
}

fn random_u64() -> u64 {
    let mut b = [0u8; 8];
    let _ = getrandom::fill(&mut b);
    u64::from_le_bytes(b)
}

struct Sess {
    tx: tmpsc::Sender<Out>,
    /// The room and seat it plays in.
    at: Option<(usize, u8)>,
}

struct Live {
    room: Room,
    id: String,
    mode: u8,
    links: [Option<u64>; MAX_FIGHTERS],
    wallets: [Option<String>; MAX_FIGHTERS],
    over_at: Option<Instant>,
    /// A cup heat: the cup (by index) and the heat in its stage.
    cup: Option<(usize, usize)>,
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

struct Sim {
    cfg: Config,
    rx: mpsc::Receiver<ToSim>,
    jobs: tmpsc::UnboundedSender<Job>,
    stop: Arc<AtomicBool>,
}

fn send_msg(tx: &tmpsc::Sender<Out>, m: &Msg) {
    let mut f = [0u8; proto::MAX_STREAM_MSG + 2];
    if let Ok(n) = frame_msg(m, &mut f) {
        let _ = tx.try_send(Out::Stream(f[..n].to_vec()));
    }
}

impl Sim {
    /// A new room: fresh salt (never seen until the end) and a seed drawn
    /// apart from it (what clients watch: spawn picks, bots).
    fn new_room(&self, mode: u8, ranked: bool) -> Room {
        let salt = (random_u64(), random_u64());
        let seed = random_u64();
        let rules = match mode {
            MODE_DOUBLES => self.cfg.doubles,
            MODE_CUP => self.cfg.cup,
            MODE_FFA => self.cfg.ffa,
            MODE_BIG_TEAM => self.cfg.big_team,
            MODE_GUN_GAME => self.cfg.gun_game,
            _ if ranked => self.cfg.duel,
            _ => self.cfg.rules,
        };
        let map = self.pick_map(seed, mode == MODE_BIG_TEAM);
        if ranked {
            Room::ranked(map, rules, seed, salt)
        } else {
            let mut r = Room::new(map, rules, seed, salt);
            r.passive_bots = self.cfg.passive_bots;
            r
        }
    }

    /// The map a room plays, from its seed (what clients may know anyway):
    /// the `--map` ones if any are given, else Big Team on the big maps and
    /// everything else on the arenas.
    fn pick_map(&self, seed: u64, big: bool) -> Map {
        let given: Vec<u8> = self
            .cfg
            .maps
            .iter()
            .copied()
            .filter(|id| sim::maps::ALL.iter().any(|d| d.id == *id))
            .collect();
        let sized: Vec<u8> = sim::maps::ALL.iter().filter(|d| d.big() == big).map(|d| d.id).collect();
        let ids = if !given.is_empty() {
            given
        } else if !sized.is_empty() {
            sized
        } else {
            sim::maps::ALL.iter().map(|d| d.id).collect()
        };
        sim::map::by_id(ids[(seed % ids.len() as u64) as usize]).unwrap_or_else(Map::proving_grounds)
    }

    fn run(self) {
        let start = Instant::now();
        let tick = Duration::from_nanos(1_000_000_000 / TICK_HZ as u64);
        let mut next = Instant::now();
        let mut sessions: BTreeMap<u64, Sess> = BTreeMap::new();
        let mut rooms: Vec<Option<Live>> = Vec::new();
        let mut q = queue::Queue::default();
        let mut buf = [0u8; MAX_DATAGRAM];
        let mut last_second = Instant::now();

        let place = place_room;
        let mut cups: Vec<Option<cup::Cup>> = Vec::new();
        let mut cup_wait: Vec<queue::Entry> = Vec::new();
        let mut cup_ready_at: Option<Instant> = None;

        while !self.stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            let now_ms = start.elapsed().as_secs_f64() * 1000.0;
            while let Ok(m) = self.rx.try_recv() {
                match m {
                    ToSim::Casual {
                        session,
                        name,
                        mode,
                        skin,
                        tx,
                    } => {
                        let mode = match mode {
                            MODE_FFA | MODE_GUN_GAME | MODE_BIG_TEAM => mode,
                            _ => MODE_CASUAL,
                        };
                        // Free for all and Gun Game are shared: a seat a bot
                        // holds in a room still playing, else a new room.
                        let open = (mode != MODE_CASUAL)
                            .then(|| {
                                rooms.iter().position(|r| {
                                    r.as_ref().is_some_and(|r| {
                                        r.mode == mode
                                            && r.over_at.is_none()
                                            && r.room.world.phase != Phase::Over
                                            && r.links.iter().take(r.room.size).any(|l| l.is_none())
                                    })
                                })
                            })
                            .flatten();
                        let i = match open {
                            Some(i) => i,
                            None => place(
                                &mut rooms,
                                Live {
                                    room: self.new_room(mode, false),
                                    id: format!("os-c{:016x}", random_u64()),
                                    mode,
                                    links: [None; MAX_FIGHTERS],
                                    wallets: Default::default(),
                                    over_at: None,
                                    cup: None,
                                },
                            ),
                        };
                        let live = rooms[i].as_mut().unwrap();
                        let Some((slot, out)) = live.room.join(name, skin) else {
                            continue;
                        };
                        live.links[slot as usize] = Some(session);
                        sessions.insert(
                            session,
                            Sess {
                                tx,
                                at: Some((i, slot)),
                            },
                        );
                        deliver(&sessions, rooms[i].as_ref().unwrap(), out);
                    }
                    ToSim::Queue { session, entry, tx } => {
                        let playing = rooms
                            .iter()
                            .flatten()
                            .any(|r| r.over_at.is_none() && r.wallets.iter().flatten().any(|w| *w == entry.wallet));
                        let in_cup = cup_wait.iter().any(|e| e.wallet == entry.wallet)
                            || cups.iter().flatten().any(|c| {
                                c.stage != cup::Stage::Done
                                    && c.entrants.iter().any(|e| e.here && e.wallet == entry.wallet)
                            });
                        if playing || in_cup || q.has_wallet(&entry.wallet) {
                            send_msg(
                                &tx,
                                &Msg::Refuse {
                                    reason: proto::REFUSE_BUSY,
                                },
                            );
                            let _ = tx.try_send(Out::Close);
                            continue;
                        }
                        send_msg(
                            &tx,
                            &Msg::Queued {
                                mode: entry.mode,
                                waiting_s: 0,
                            },
                        );
                        if entry.mode == MODE_CUP {
                            cup_wait.push(*entry);
                        } else {
                            q.push(*entry);
                        }
                        sessions.insert(session, Sess { tx, at: None });
                    }
                    ToSim::Input { session, msg } => {
                        if let Some((i, slot)) = sessions.get(&session).and_then(|s| s.at) {
                            if let Some(r) = rooms[i].as_mut() {
                                r.room.on_input(slot, &msg, now_ms);
                            }
                        }
                    }
                    ToSim::Leave { session } => {
                        q.remove(session);
                        cup_wait.retain(|e| e.session != session);
                        for c in cups.iter_mut().flatten() {
                            c.leave(session);
                        }
                        let Some(s) = sessions.remove(&session) else { continue };
                        let Some((i, slot)) = s.at else { continue };
                        let Some(r) = rooms[i].as_mut() else { continue };
                        r.links[slot as usize] = None;
                        // A cup's heat plays out even with nobody left: the
                        // cup waits on its result.
                        if r.links.iter().all(|l| l.is_none()) && r.cup.is_none() {
                            // Nobody left to play or watch.
                            rooms[i] = None;
                        } else {
                            let out = r.room.leave(slot);
                            deliver(&sessions, r, out);
                        }
                    }
                }
            }

            // Once a second: form matches, tell the waiting how long.
            if now.duration_since(last_second) >= Duration::from_secs(1) {
                last_second = now;
                while let Some(sides) = q.form(now) {
                    let mode = sides[0][0].mode;
                    let mut live = Live {
                        room: self.new_room(mode, true),
                        id: format!("os-{:016x}", random_u64()),
                        mode,
                        links: [None; MAX_FIGHTERS],
                        wallets: Default::default(),
                        over_at: None,
                        cup: None,
                    };
                    let mut outs = Vec::new();
                    for (side, players) in sides.iter().enumerate() {
                        for (k, p) in players.iter().enumerate() {
                            // Seats alternate sides: 0 and 2 red, 1 and 3 blue.
                            let slot = side + 2 * k;
                            outs.push(live.room.seat_person(slot, p.name, p.skin));
                            live.links[slot] = Some(p.session);
                            live.wallets[slot] = Some(p.wallet.clone());
                        }
                    }
                    let i = place(&mut rooms, live);
                    for side in &sides {
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
                        deliver(&sessions, rooms[i].as_ref().unwrap(), out);
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
                                skin: e.skin,
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
                        self.start_stage(&cups, ci, &mut rooms, &mut sessions);
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

            let mut heats_done: Vec<HeatDone> = Vec::new();
            for entry in rooms.iter_mut() {
                let Some(r) = entry.as_mut() else { continue };
                let (snaps, out) = r.room.step(now_ms);
                for (slot, snap) in snaps {
                    let link = r.links[slot as usize].and_then(|id| sessions.get(&id));
                    if let (Some(s), Ok(n)) = (link, encode_snapshot(&snap, &mut buf)) {
                        // Full means the session is behind: this snapshot
                        // goes, the next replaces it.
                        let _ = s.tx.try_send(Out::Datagram(buf[..n].to_vec()));
                    }
                }
                deliver(&sessions, r, out);
                if r.room.world.phase == Phase::Over && r.over_at.is_none() {
                    r.over_at = Some(now);
                    self.finish(r);
                }
                if r.over_at.is_some_and(|t| now.duration_since(t) >= self.cfg.linger) {
                    for id in r.links.iter().flatten() {
                        if let Some(s) = sessions.get_mut(id) {
                            // A cup's entrants stay on for the next heat.
                            if r.cup.is_none() {
                                send_msg(&s.tx, &Msg::Bye);
                                let _ = s.tx.try_send(Out::Close);
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
                } else if c.stage == cup::Stage::Done {
                    self.finish_cup(c, &mut sessions);
                    cups[ci] = None;
                } else {
                    self.start_stage(&cups, ci, &mut rooms, &mut sessions);
                }
            }

            next += tick;
            let now = Instant::now();
            if now < next {
                std::thread::sleep(next - now);
            } else if now - next > tick * 8 {
                // Far behind (a stall): don't sprint to catch up.
                next = now;
            }
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
    ) {
        let Some(c) = cups[ci].as_ref() else { return };
        for i in 0..c.entrants.len() {
            tell_cup(c, i, sessions);
        }
        for (h, (members, _)) in c.heats.iter().enumerate() {
            let mut live = Live {
                room: self.new_room(MODE_CUP, true),
                id: format!("{}-{}-{}", c.id, c.round_no(), h + 1),
                mode: MODE_CUP,
                links: [None; MAX_FIGHTERS],
                wallets: Default::default(),
                over_at: None,
                cup: Some((ci, h)),
            };
            let mut outs = Vec::new();
            for (slot, &i) in members.iter().enumerate() {
                let e = &c.entrants[i];
                outs.push(live.room.seat_person(slot, e.name, e.skin));
                live.links[slot] = Some(e.session);
                live.wallets[slot] = Some(e.wallet.clone());
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
        for (i, e) in c.entrants.iter().enumerate() {
            tell_cup(c, i, sessions);
            if let Some(s) = sessions.get_mut(&e.session) {
                send_msg(&s.tx, &Msg::Bye);
                let _ = s.tx.try_send(Out::Close);
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

    /// A match ended: its replay and its aim review's numbers to disk (and
    /// to `review/` if they cross a line), and a ranked one's result to Elo
    /// Pros.
    fn finish(&self, r: &Live) {
        let bytes = r.room.replay();
        let sha = elo::sha256_hex(&bytes);
        let salt_commit = hex(&r.room.salt_commit());
        if let Some(dir) = &self.cfg.replays {
            let _ = self.jobs.send(Job::Write {
                path: dir.join(format!("{}.osr", r.id)),
                bytes,
            });
            let (doc, flagged) = watch_doc(r, &sha, &salt_commit);
            let text = serde_json::to_vec_pretty(&doc).unwrap_or_default();
            if flagged {
                let _ = self.jobs.send(Job::Write {
                    path: dir.join("review").join(format!("{}.json", r.id)),
                    bytes: text.clone(),
                });
            }
            let _ = self.jobs.send(Job::Write {
                path: dir.join(format!("{}.watch.json", r.id)),
                bytes: text,
            });
        }
        if !r.room.ranked {
            return;
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
            return;
        }
        let placed = sides
            .iter()
            .map(|(ws, sc)| (ws.clone(), 1 + sides.iter().filter(|(_, o)| o > sc).count() as u32))
            .collect();
        let _ = self.jobs.send(Job::Report(elo::Report {
            id: r.id.clone(),
            ladder: match r.mode {
                MODE_DOUBLES => "doubles",
                MODE_CUP => "cup",
                _ => "duel",
            }
            .into(),
            sides: placed,
            replay_sha256: sha,
            salt_commit,
        }));
    }
}

/// A match's aim review: each person's numbers and the lines they cross
/// (`room::watch`). Whether any did.
fn watch_doc(r: &Live, sha: &str, salt_commit: &str) -> (serde_json::Value, bool) {
    let mut flagged = false;
    let seats: Vec<serde_json::Value> = (0..MAX_FIGHTERS)
        .filter_map(|s| {
            let room::Seat::Person(_, name) = &r.room.seats[s] else {
                return None;
            };
            let st = &r.room.watch().stats[s];
            let flags = st.flags();
            flagged |= !flags.is_empty();
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
            }))
        })
        .collect();
    let doc = serde_json::json!({
        "id": r.id,
        "ranked": r.room.ranked,
        "mode": r.mode,
        "replay": format!("{}.osr", r.id),
        "replay_sha256": sha,
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

fn deliver(sessions: &BTreeMap<u64, Sess>, r: &Live, out: room::Outbox) {
    let mut f = [0u8; proto::MAX_STREAM_MSG + 2];
    for (to, m) in out {
        let Ok(n) = frame_msg(&m, &mut f) else { continue };
        for (s, id) in r.links.iter().enumerate() {
            let Some(sess) = id.and_then(|id| sessions.get(&id)) else {
                continue;
            };
            if to.is_none_or(|t| t as usize == s) {
                let _ = sess.tx.try_send(Out::Stream(f[..n].to_vec()));
            }
        }
    }
}

/// Bytes as lower-case hex.
pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

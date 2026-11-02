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
    encode_snapshot, frame_msg, InputMsg, Msg, Name, MAX_DATAGRAM, MODE_CASUAL, MODE_DOUBLES, MODE_FFA, MODE_GUN_GAME,
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
/// A finished match stays up this long, for the end screen.
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
    /// A casual room's rules: against a bot, free for all, Gun Game.
    pub rules: Rules,
    pub ffa: Rules,
    pub gun_game: Rules,
    /// Ranked rooms' rules.
    pub duel: Rules,
    pub doubles: Rules,
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
            duel: Rules::duel(),
            doubles: Rules::doubles(),
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
}

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
            MODE_FFA => self.cfg.ffa,
            MODE_GUN_GAME => self.cfg.gun_game,
            _ if ranked => self.cfg.duel,
            _ => self.cfg.rules,
        };
        let map = self.pick_map(seed);
        if ranked {
            Room::ranked(map, rules, seed, salt)
        } else {
            let mut r = Room::new(map, rules, seed, salt);
            r.passive_bots = self.cfg.passive_bots;
            r
        }
    }

    /// The map a room plays, from its seed (what clients may know anyway).
    fn pick_map(&self, seed: u64) -> Map {
        let all: Vec<u8> = sim::maps::ALL.iter().map(|d| d.id).collect();
        let ids: Vec<u8> = self.cfg.maps.iter().copied().filter(|id| all.contains(id)).collect();
        let ids = if ids.is_empty() { all } else { ids };
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

        let place = |rooms: &mut Vec<Option<Live>>, live: Live| -> usize {
            if let Some(i) = rooms.iter().position(|r| r.is_none()) {
                rooms[i] = Some(live);
                i
            } else {
                rooms.push(Some(live));
                rooms.len() - 1
            }
        };

        while !self.stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            let now_ms = start.elapsed().as_secs_f64() * 1000.0;
            while let Ok(m) = self.rx.try_recv() {
                match m {
                    ToSim::Casual {
                        session,
                        name,
                        mode,
                        tx,
                    } => {
                        let mode = match mode {
                            MODE_FFA | MODE_GUN_GAME => mode,
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
                                },
                            ),
                        };
                        let live = rooms[i].as_mut().unwrap();
                        let Some((slot, out)) = live.room.join(name) else {
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
                        if playing || q.has_wallet(&entry.wallet) {
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
                        q.push(*entry);
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
                        let Some(s) = sessions.remove(&session) else { continue };
                        let Some((i, slot)) = s.at else { continue };
                        let Some(r) = rooms[i].as_mut() else { continue };
                        r.links[slot as usize] = None;
                        if r.links.iter().all(|l| l.is_none()) {
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
                    };
                    let mut outs = Vec::new();
                    for (side, players) in sides.iter().enumerate() {
                        for (k, p) in players.iter().enumerate() {
                            // Seats alternate sides: 0 and 2 red, 1 and 3 blue.
                            let slot = side + 2 * k;
                            outs.push(live.room.seat_person(slot, p.name));
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
                for e in &q.entries {
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
                if r.over_at.is_some_and(|t| now.duration_since(t) >= LINGER) {
                    for id in r.links.iter().flatten() {
                        if let Some(s) = sessions.get_mut(id) {
                            send_msg(&s.tx, &Msg::Bye);
                            let _ = s.tx.try_send(Out::Close);
                            s.at = None;
                        }
                    }
                    *entry = None;
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
        // Sides and their scores: each seat for a duel, each team for
        // doubles (seats 0 and 2 red, 1 and 3 blue).
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
            (0..2)
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
            ladder: if r.mode == MODE_DOUBLES { "doubles" } else { "duel" }.into(),
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

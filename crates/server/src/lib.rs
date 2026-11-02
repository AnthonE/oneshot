//! The authoritative server: one room on its own thread at the sim's tick,
//! WebTransport sessions on tokio around it, and a small HTTP server for the
//! page (docs/DESIGN.md §2, §4, §14).
//!
//! The sim thread never blocks on the network: sessions hand it inputs over
//! a channel, and it hands each session its datagrams and stream messages
//! over a bounded one it only `try_send`s to. A full queue drops a snapshot
//! (the next replaces it) rather than stall the tick.

pub mod http;
pub mod net;

use std::net::SocketAddr;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use proto::{encode_snapshot, frame_msg, InputMsg, Msg, Name, MAX_DATAGRAM, REFUSE_FULL};
use room::Room;
use sim::consts::{MAX_FIGHTERS, TICK_HZ};
use sim::map::Map;
use sim::world::Rules;
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
    Join {
        name: Name,
        tx: tmpsc::Sender<Out>,
        reply: tokio::sync::oneshot::Sender<Option<(u8, u64)>>,
    },
    Input {
        slot: u8,
        id: u64,
        msg: Box<InputMsg>,
    },
    Leave {
        slot: u8,
        id: u64,
    },
}

/// Room for this many messages to one session before snapshots drop.
pub const OUT_QUEUE: usize = 64;

#[derive(Clone, Debug)]
pub struct Config {
    /// Where WebTransport listens (UDP).
    pub bind: SocketAddr,
    /// Where the page is served (TCP), if anywhere.
    pub http: Option<SocketAddr>,
    /// The built page.
    pub web_dir: std::path::PathBuf,
    /// PEM certificate and key; without them a self-signed certificate
    /// (14 days, pinned by the page with its hash).
    pub cert: Option<(String, String)>,
    pub rules: Rules,
}

pub struct Handle {
    pub addr: SocketAddr,
    pub http: Option<SocketAddr>,
    /// SHA-256 of the certificate, for `serverCertificateHashes`.
    pub cert_hash: [u8; 32],
    pub stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// Starts the server on the current tokio runtime. Returns once it is
/// listening.
pub async fn run(cfg: Config) -> Result<Handle, String> {
    let (endpoint, cert_hash) = net::endpoint(&cfg).await?;
    let addr = endpoint.local_addr().map_err(|e| e.to_string())?;
    let (to_sim, from_net) = mpsc::channel::<ToSim>();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| e.to_string())?;
    let salt = (
        u64::from_le_bytes(salt[..8].try_into().unwrap()),
        u64::from_le_bytes(salt[8..].try_into().unwrap()),
    );
    let seed = salt.0 ^ 0x9e37_79b9_7f4a_7c15;
    let rules = cfg.rules;
    let s = stop.clone();
    std::thread::Builder::new()
        .name("sim".into())
        .spawn(move || sim_thread(Room::new(Map::proving_grounds(), rules, seed, salt), from_net, s))
        .map_err(|e| e.to_string())?;

    tokio::spawn(net::accept_loop(endpoint, to_sim));
    let http = match cfg.http {
        Some(at) => Some(http::serve(at, cfg.web_dir.clone(), addr, cert_hash).await?),
        None => None,
    };
    Ok(Handle {
        addr,
        http,
        cert_hash,
        stop,
    })
}

struct Link {
    id: u64,
    tx: tmpsc::Sender<Out>,
}

fn sim_thread(mut room: Room, rx: mpsc::Receiver<ToSim>, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let start = Instant::now();
    let tick = Duration::from_nanos(1_000_000_000 / TICK_HZ as u64);
    let mut next = Instant::now();
    let mut links: [Option<Link>; MAX_FIGHTERS] = Default::default();
    let mut ids = 0u64;
    let mut buf = [0u8; MAX_DATAGRAM];
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        let now_ms = start.elapsed().as_secs_f64() * 1000.0;
        while let Ok(m) = rx.try_recv() {
            match m {
                ToSim::Join { name, tx, reply } => match room.join(name) {
                    Some((slot, out)) => {
                        ids += 1;
                        links[slot as usize] = Some(Link { id: ids, tx });
                        let _ = reply.send(Some((slot, ids)));
                        deliver(&links, out);
                    }
                    None => {
                        let mut f = [0u8; proto::MAX_STREAM_MSG + 2];
                        if let Ok(n) = frame_msg(&Msg::Refuse { reason: REFUSE_FULL }, &mut f) {
                            let _ = tx.try_send(Out::Stream(f[..n].to_vec()));
                        }
                        let _ = tx.try_send(Out::Close);
                        let _ = reply.send(None);
                    }
                },
                ToSim::Input { slot, id, msg } => {
                    if links[slot as usize].as_ref().is_some_and(|l| l.id == id) {
                        room.on_input(slot, &msg, now_ms);
                    }
                }
                ToSim::Leave { slot, id } => {
                    if links[slot as usize].as_ref().is_some_and(|l| l.id == id) {
                        links[slot as usize] = None;
                        let out = room.leave(slot);
                        deliver(&links, out);
                    }
                }
            }
        }

        let (snaps, out) = room.step(now_ms);
        for (slot, snap) in snaps {
            if let (Some(l), Ok(n)) = (&links[slot as usize], encode_snapshot(&snap, &mut buf)) {
                // Full means the session is behind: this snapshot goes, the
                // next one replaces it.
                let _ = l.tx.try_send(Out::Datagram(buf[..n].to_vec()));
            }
        }
        deliver(&links, out);

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

fn deliver(links: &[Option<Link>; MAX_FIGHTERS], out: room::Outbox) {
    let mut f = [0u8; proto::MAX_STREAM_MSG + 2];
    for (to, m) in out {
        let Ok(n) = frame_msg(&m, &mut f) else { continue };
        for (s, l) in links.iter().enumerate() {
            if let Some(l) = l {
                if to.is_none_or(|t| t as usize == s) {
                    let _ = l.tx.try_send(Out::Stream(f[..n].to_vec()));
                }
            }
        }
    }
}

/// A hash as the page wants it: lower-case hex.
pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

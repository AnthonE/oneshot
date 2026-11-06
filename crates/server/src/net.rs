//! WebTransport: the endpoint, and one task set per session. Endpoint setup
//! after engine-shim's `engine2/src/net.rs` and Gates' `spawn_shard`; the
//! datagram send checks the live MTU and never waits (Gates' `send_snap`).

use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::AtomicUsize;
use std::sync::{mpsc, Mutex};
use std::time::Duration;

use proto::frame::LEN_PREFIX_BYTES;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use proto::{
    datagram_kind, decode_input, decode_msg, InputMsg, Msg, DG_INPUT, MAX_STREAM_MSG, MODE_CUP, MODE_DOUBLES,
    MODE_DUEL, MODE_LOBBY, MODE_WATCH, PROTO_VER,
};
use wtransport::endpoint::endpoint_side::Server;
use wtransport::endpoint::IncomingSession;
use wtransport::{Endpoint, Identity, RecvStream, ServerConfig};

use crate::outlet::{self, Ended};
use crate::{auth, queue, Config, Ctx, ToSim};

const KEEP_ALIVE: Duration = Duration::from_secs(5);
const IDLE_TIMEOUT: Duration = Duration::from_secs(15);
/// Accept to Hello. A page pressing PLAY while it loads can stall this long
/// before it sends its Hello; each address's limits still hold.
const HANDSHAKE: Duration = Duration::from_secs(60);
/// The UDP socket's buffers (Gates' `bind_udp`): a burst of snapshots to a
/// full server fits rather than drops. Linux clamps to `net.core.rmem_max`
/// and `wmem_max`, so what was granted is read back and printed.
pub const UDP_BUF_BYTES: usize = 8 * 1024 * 1024;
/// Connections one address may hold (an IPv4 address, an IPv6 /64), and one
/// prefix (a /24, a /48): a household plays, a flood from one place doesn't.
/// Each player holds two (the lobby beside the match), so eight at one
/// address fit.
pub const PER_SOURCE: u16 = 16;
pub const PER_PREFIX: u16 = 4 * PER_SOURCE;
/// Input datagrams the sim may have waiting, every session's together: a
/// full server sends about 128 a tick. Past it an input is dropped, not
/// queued: the sim never falls behind on a flood.
pub const INPUT_LANE: usize = 4096;
/// One session's inputs a second, and a burst over that: a client sends
/// 64 a second (`client_net::core::SEND_EVERY`), a page that stalled a
/// little more at once. Each carries the frames before it, so one dropped
/// costs an honest client nothing.
pub const INPUTS_PER_S: f64 = 128.0;
pub const INPUT_BURST: f64 = 64.0;

/// A session's input allowance (as `proto::voice::Bucket`).
#[derive(Clone, Copy, Debug)]
pub struct InputBucket {
    tokens: f64,
    at: std::time::Instant,
}

impl InputBucket {
    pub fn new(now: std::time::Instant) -> Self {
        InputBucket {
            tokens: INPUT_BURST,
            at: now,
        }
    }

    pub fn allow(&mut self, now: std::time::Instant) -> bool {
        let dt = now.saturating_duration_since(self.at).as_secs_f64();
        self.at = now;
        self.tokens = (self.tokens + dt * INPUTS_PER_S).min(INPUT_BURST);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// The sim's end of a session's inputs: its id and what it sent.
pub type Inputs = mpsc::SyncSender<(u64, InputMsg)>;

/// The certificate the server presents: its PEM files, or a fresh
/// self-signed one (14 days, which the page pins by hash).
pub async fn identity(cfg: &Config) -> Result<Identity, String> {
    match &cfg.cert {
        Some((cert, key)) => Identity::load_pemfiles(cert, key)
            .await
            .map_err(|e| format!("{cert}, {key}: {e}")),
        None => Identity::self_signed(["localhost", "127.0.0.1", "::1"]).map_err(|e| e.to_string()),
    }
}

/// SHA-256 of an identity's leaf certificate.
pub fn cert_hash(identity: &Identity) -> Result<[u8; 32], String> {
    Ok(*identity
        .certificate_chain()
        .as_slice()
        .first()
        .ok_or("an empty certificate chain")?
        .hash()
        .as_ref())
}

/// The UDP socket, with buffers asked for before the bind (after Gates'
/// `bind_udp`): dual-stack on `[::]`; a buffer refused is not fatal, a
/// bind refused is. Returns the socket and the receive and send buffers
/// the kernel granted.
pub fn bind_udp(addr: SocketAddr) -> Result<(std::net::UdpSocket, usize, usize), String> {
    use socket2::{Domain, Protocol, Socket, Type};
    let sock = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP)).map_err(|e| e.to_string())?;
    if addr.is_ipv6() {
        let _ = sock.set_only_v6(false);
    }
    let _ = sock.set_recv_buffer_size(UDP_BUF_BYTES);
    let _ = sock.set_send_buffer_size(UDP_BUF_BYTES);
    sock.bind(&addr.into()).map_err(|e| format!("bind {addr}: {e}"))?;
    sock.set_nonblocking(true).map_err(|e| e.to_string())?;
    let recv = sock.recv_buffer_size().unwrap_or(0);
    let send = sock.send_buffer_size().unwrap_or(0);
    Ok((sock.into(), recv, send))
}

fn server_config(
    bind: wtransport::config::ServerConfigBuilder<wtransport::config::states::WantsIdentity>,
    identity: Identity,
) -> Result<ServerConfig, String> {
    Ok(bind
        .with_identity(identity)
        .keep_alive_interval(Some(KEEP_ALIVE))
        .max_idle_timeout(Some(IDLE_TIMEOUT))
        .map_err(|e| e.to_string())?
        .build())
}

/// What the endpoint was given: the socket's buffers and the certificate.
pub struct Bound {
    pub endpoint: Endpoint<Server>,
    pub cert_hash: [u8; 32],
    pub udp_recv: usize,
    pub udp_send: usize,
}

pub async fn endpoint(cfg: &Config) -> Result<Bound, String> {
    let identity = identity(cfg).await?;
    let cert_hash = cert_hash(&identity)?;
    let (socket, udp_recv, udp_send) = bind_udp(cfg.bind)?;
    let config = server_config(ServerConfig::builder().with_bind_socket(socket), identity)?;
    let endpoint = Endpoint::server(config).map_err(|e| format!("bind {}: {e}", cfg.bind))?;
    Ok(Bound {
        endpoint,
        cert_hash,
        udp_recv,
        udp_send,
    })
}

/// Reads the PEM files again and hands the endpoint the new certificate:
/// new handshakes present it, live sessions keep theirs (an ACME renewal
/// without a restart). Returns the new certificate's hash.
pub async fn reload(ep: &Endpoint<Server>, cfg: &Config) -> Result<[u8; 32], String> {
    if cfg.cert.is_none() {
        return Err("no --cert to reload".into());
    }
    let identity = identity(cfg).await?;
    let hash = cert_hash(&identity)?;
    // The bind address is ignored without a rebind: the socket stays.
    let config = server_config(ServerConfig::builder().with_bind_address(cfg.bind), identity)?;
    ep.reload_config(config, false).map_err(|e| e.to_string())?;
    Ok(hash)
}

/// What to do with a connection attempt, given how many connections the
/// server holds (`busy`, handshakes included) and its `capacity` in players
/// (docs/DESIGN.md §14, after Gates' accept loop): past four times capacity
/// refuse outright; past twice, make an address that hasn't proved itself
/// do so with a QUIC Retry first; otherwise take it (and past capacity the
/// hello is answered "full").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admit {
    Take,
    Retry,
    Refuse,
}

pub fn admit(busy: usize, validated: bool, capacity: usize) -> Admit {
    if busy >= 4 * capacity {
        Admit::Refuse
    } else if busy >= 2 * capacity && !validated {
        // `retry()` panics on an address already validated.
        Admit::Retry
    } else {
        Admit::Take
    }
}

/// The connections held, in all and per address and prefix.
pub struct Gate {
    pub capacity: usize,
    /// Addresses held to no share, as loopback isn't (`Config::load_sources`).
    exempt: Vec<IpAddr>,
    conns: AtomicUsize,
    places: Mutex<(BTreeMap<IpAddr, u16>, BTreeMap<IpAddr, u16>)>,
}

/// One connection's place at the gate, given back when it drops (whatever
/// way its session ends).
pub struct Permit {
    gate: Arc<Gate>,
    keys: Option<(IpAddr, IpAddr)>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.gate.conns.fetch_sub(1, Ordering::Relaxed);
        if let (Some((src, pre)), Ok(mut m)) = (self.keys, self.gate.places.lock()) {
            let (by_src, by_pre) = &mut *m;
            for (map, k) in [(by_src, src), (by_pre, pre)] {
                if let Some(n) = map.get_mut(&k) {
                    *n -= 1;
                    if *n == 0 {
                        map.remove(&k);
                    }
                }
            }
        }
    }
}

/// An address's key (itself, or its /64) and its prefix's (/24, /48).
fn source_keys(ip: IpAddr) -> (IpAddr, IpAddr) {
    match ip.to_canonical() {
        IpAddr::V4(v4) => {
            let b = u32::from(v4);
            (IpAddr::V4(v4), IpAddr::V4((b & 0xffff_ff00).into()))
        }
        IpAddr::V6(v6) => {
            let b = u128::from(v6);
            (
                IpAddr::V6((b & !0u128 << 64).into()),
                IpAddr::V6((b & !0u128 << 80).into()),
            )
        }
    }
}

impl Gate {
    pub fn new(capacity: usize) -> Gate {
        Gate {
            capacity: capacity.max(1),
            exempt: Vec::new(),
            conns: AtomicUsize::new(0),
            places: Mutex::new((BTreeMap::new(), BTreeMap::new())),
        }
    }

    /// The same, with `ips` held to no share (a load test's box).
    pub fn exempting(mut self, ips: &[IpAddr]) -> Gate {
        self.exempt = ips.iter().map(|ip| ip.to_canonical()).collect();
        self
    }

    /// Connections held now, handshakes included.
    pub fn busy(&self) -> usize {
        self.conns.load(Ordering::Relaxed)
    }

    /// A place for a connection from `ip`, or None if its address or prefix
    /// holds its share. Loopback is the box itself (tests, local bots) and
    /// isn't counted per source, nor is an exempt address.
    pub fn take(self: &Arc<Self>, ip: IpAddr) -> Option<Permit> {
        let ip = ip.to_canonical();
        let keys = if ip.is_loopback() || self.exempt.contains(&ip) {
            None
        } else {
            let (src, pre) = source_keys(ip);
            let mut m = self.places.lock().ok()?;
            if m.0.get(&src).copied().unwrap_or(0) >= PER_SOURCE || m.1.get(&pre).copied().unwrap_or(0) >= PER_PREFIX {
                return None;
            }
            *m.0.entry(src).or_insert(0) += 1;
            *m.1.entry(pre).or_insert(0) += 1;
            Some((src, pre))
        };
        self.conns.fetch_add(1, Ordering::Relaxed);
        Some(Permit {
            gate: self.clone(),
            keys,
        })
    }
}

pub async fn accept_loop(ep: Arc<Endpoint<Server>>, to_sim: mpsc::Sender<ToSim>, inputs: Inputs, ctx: Arc<Ctx>) {
    loop {
        let incoming = ep.accept().await;
        match admit(ctx.gate.busy(), incoming.remote_address_validated(), ctx.gate.capacity) {
            Admit::Refuse => {
                ctx.health.conn_refused.fetch_add(1, Ordering::Relaxed);
                incoming.refuse();
                continue;
            }
            Admit::Retry => {
                ctx.health.conn_retried.fetch_add(1, Ordering::Relaxed);
                incoming.retry();
                continue;
            }
            Admit::Take => {}
        }
        let Some(permit) = ctx.gate.take(incoming.remote_address().ip()) else {
            ctx.health.conn_refused.fetch_add(1, Ordering::Relaxed);
            incoming.refuse();
            continue;
        };
        tokio::spawn(session(incoming, permit, to_sim.clone(), inputs.clone(), ctx.clone()));
    }
}

pub(crate) async fn read_frame(r: &mut RecvStream) -> Option<Vec<u8>> {
    let mut len = [0u8; LEN_PREFIX_BYTES];
    r.read_exact(&mut len).await.ok()?;
    let n = u16::from_le_bytes(len) as usize;
    if n == 0 || n > MAX_STREAM_MSG {
        return None;
    }
    let mut body = vec![0u8; n];
    r.read_exact(&mut body).await.ok()?;
    Some(body)
}

async fn refuse(send: &mut wtransport::SendStream, reason: u8, ctx: &Ctx) {
    ctx.health.refuse(reason);
    let mut f = [0u8; MAX_STREAM_MSG + LEN_PREFIX_BYTES];
    if let Ok(n) = proto::frame_msg(&Msg::Refuse { reason }, &mut f) {
        let _ = send.write_all(&f[..n]).await;
        let _ = send.finish().await;
    }
}

async fn session(
    incoming: IncomingSession,
    _permit: Permit,
    to_sim: mpsc::Sender<ToSim>,
    inputs: Inputs,
    ctx: Arc<Ctx>,
) {
    let opened = tokio::time::timeout(HANDSHAKE, async {
        let request = incoming.await.ok()?;
        let conn = request.accept().await.ok()?;
        let (send, mut recv) = conn.accept_bi().await.ok()?;
        let hello = decode_msg(&read_frame(&mut recv).await?).ok()?;
        Some((conn, send, recv, hello))
    })
    .await;
    let Ok(Some((conn, mut send, mut recv, hello))) = opened else {
        ctx.health.handshake_failed.fetch_add(1, Ordering::Relaxed);
        return;
    };
    let Msg::Hello {
        proto,
        name,
        pass,
        mode,
        skin,
        emblem,
        party,
        stamp,
    } = hello
    else {
        ctx.health.handshake_failed.fetch_add(1, Ordering::Relaxed);
        return;
    };
    // An old page, or one whose sim isn't ours (a map or a rule changed
    // since it loaded): it would predict against walls we don't have.
    if proto != PROTO_VER || stamp != sim::STAMP {
        return refuse(&mut send, proto::REFUSE_VERSION, &ctx).await;
    }
    // Past capacity, or on its way down for a restart: come back soon.
    if ctx.draining.load(Ordering::Relaxed) || ctx.gate.busy() > ctx.gate.capacity {
        return refuse(&mut send, proto::REFUSE_FULL, &ctx).await;
    }
    // Who: a pass from Elo Pros, a ticket of ours a pass earned, or a
    // guest.
    let ranked = matches!(mode, MODE_DUEL | MODE_DOUBLES | MODE_CUP);
    let ticket = pass.as_str().starts_with(auth::TICKET_PREFIX);
    let who: Option<auth::Who> = if pass.is_empty() {
        None
    } else if ticket {
        match ctx.nonces.check_ticket(pass.as_str()) {
            Some(w) => Some(w),
            None if ranked => return refuse(&mut send, proto::REFUSE_PASS, &ctx).await,
            None => None,
        }
    } else if let Some(key) = ctx.elo_pubkey {
        match auth::verify_pass(
            pass.as_str(),
            &key,
            &ctx.slug,
            &ctx.domains,
            |n| ctx.nonces.take(n),
            auth::now_unix(),
        ) {
            Ok(p) => Some(auth::Who::from(p)),
            Err(_) if ranked => return refuse(&mut send, proto::REFUSE_PASS, &ctx).await,
            // Casual is open to anyone: a pass that doesn't check out (say,
            // older than its ten minutes) only costs its skin.
            Err(_) => None,
        }
    } else if ranked {
        return refuse(&mut send, proto::REFUSE_PASS, &ctx).await;
    } else {
        None
    };

    // The menu's own session: its party, friends and voice (`lobby.rs`).
    if mode == MODE_LOBBY {
        let who = who.unwrap_or_else(auth::Who::guest);
        return crate::lobby::session(conn, send, recv, who, name, party, ctx).await;
    }
    // A match with a party: seated with it if they're in it and its leader
    // started this mode, else alone.
    let seating = match (&who, party) {
        (Some(w), code) if code != 0 => ctx.lobby.parties().seating(code, &w.id, mode),
        _ => None,
    };

    // A skin is worn only if the pass's wallet owns it and, for a rank-locked
    // one, its floor is at the line (docs/DESIGN.md §15); a guest, or a list
    // or rating that can't be read, wears the standard look.
    let skin = match who.as_ref().and_then(|p| p.wallet.as_deref()) {
        Some(w) if skin != proto::SKIN_STANDARD => {
            let want = proto::SKINS.get(skin as usize).copied().unwrap_or("");
            let line = proto::SKIN_FLOOR.get(skin as usize).copied().unwrap_or(u16::MAX);
            let owned = ctx.elo.skins(w).await.iter().any(|s| s == want);
            if owned && (line == 0 || ctx.elo.floor(w).await.is_some_and(|f| f >= f64::from(line))) {
                skin
            } else {
                proto::SKIN_STANDARD
            }
        }
        _ => proto::SKIN_STANDARD,
    };
    // An emblem is anyone's, a guest's too, but a rank-locked glyph
    // (`proto::GLYPH_FLOOR`) is worn only at its line, as a skin.
    let emblem = proto::emblem_valid(emblem);
    let line = proto::GLYPH_FLOOR[proto::emblem_parts(emblem).0 as usize];
    let emblem = match who.as_ref().and_then(|p| p.wallet.as_deref()) {
        _ if line == 0 => emblem,
        Some(w) if ctx.elo.floor(w).await.is_some_and(|f| f >= f64::from(line)) => emblem,
        _ => proto::EMBLEM_NONE,
    };
    let look = proto::Look { skin, emblem };

    // Who to count casual play for: the wallet, else the account itself.
    let player = who.as_ref().filter(|w| w.signed()).map(|w| w.id.clone());
    let session_id = ctx.sessions.fetch_add(1, Ordering::Relaxed);
    let (tx, inlet) = outlet::outlet(ctx.counts.clone());
    let join = match mode {
        // A watcher takes no seat: party `party`'s match, or any public one.
        MODE_WATCH => ToSim::Watch {
            session: session_id,
            target: party,
            player,
            tx,
        },
        MODE_DUEL | MODE_DOUBLES | MODE_CUP => {
            // Ranked is a wallet's: that is what a rating belongs to.
            let Some(wallet) = who.and_then(|p| p.wallet) else {
                return refuse(&mut send, proto::REFUSE_NEEDS_WALLET, &ctx).await;
            };
            // A ban is a person's call on a reviewed replay; a list that
            // can't be read never refuses.
            if ctx.elo.banned(&wallet).await == Some(true) {
                return refuse(&mut send, proto::REFUSE_BANNED, &ctx).await;
            }
            let ladder = match mode {
                MODE_DOUBLES => "doubles",
                MODE_CUP => "cup",
                _ => "duel",
            };
            let rating = ctx.elo.rating(&wallet, ladder).await;
            ToSim::Queue {
                session: session_id,
                entry: Box::new(queue::Entry {
                    session: session_id,
                    wallet,
                    name,
                    mode,
                    rating,
                    since: std::time::Instant::now(),
                    look,
                    party: seating.filter(|_| mode == MODE_DOUBLES),
                }),
                tx,
            }
        }
        _ => ToSim::Casual {
            session: session_id,
            name,
            mode,
            look,
            party: seating,
            code: party,
            player,
            tx,
        },
    };
    // Counted as a join by the sim once it seats or queues it (`health.rs`).
    if to_sim.send(join).is_err() {
        return;
    }

    // The writers: snapshots on their own, the newest waiting, if they fit,
    // so a stream the client is slow to take never holds them back; stream
    // messages in order until the sim closes the session or cuts it.
    let outlet::Inlet {
        mut datagrams,
        mut streams,
    } = inlet;
    let (wconn, wctx) = (conn.clone(), ctx.clone());
    let snapshots = tokio::spawn(async move {
        while let Some(d) = datagrams.recv().await {
            let d = outlet::newest(&mut datagrams, d, &wctx.counts);
            if wconn.max_datagram_size().is_some_and(|max| d.len() <= max) {
                if wconn.send_datagram(d).is_err() {
                    wctx.health.send_errors.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                wctx.counts.datagrams_oversize.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    // The connection's round trip and loss, each journal window and at
    // the end (`health.rs`).
    let seen = Arc::new(Mutex::new((0u64, 0u64)));
    let sample = {
        let (conn, ctx, seen) = (conn.clone(), ctx.clone(), seen.clone());
        move || {
            let q = conn.quic_connection().stats();
            if let Ok(mut last) = seen.lock() {
                ctx.health
                    .sample(conn.rtt(), q.path.sent_packets, q.path.lost_packets, &mut last);
            }
        }
    };
    let sampler = {
        let sample = sample.clone();
        tokio::spawn(async move {
            let mut every = tokio::time::interval(crate::health::JOURNAL);
            every.tick().await;
            loop {
                every.tick().await;
                sample();
            }
        })
    };
    let writer = tokio::spawn(async move {
        let ended = streams.write_stream(&mut send).await;
        if ended == Ended::Closed {
            let _ = send.finish().await;
        }
        ended
    });

    // Inputs as datagrams, up to the session's rate, never waiting on the
    // sim; the stream only says goodbye.
    let (dconn, dctx) = (conn.clone(), ctx.clone());
    let datagrams = tokio::spawn(async move {
        let mut bucket = InputBucket::new(std::time::Instant::now());
        while let Ok(d) = dconn.receive_datagram().await {
            if datagram_kind(&d) != Some(DG_INPUT) {
                continue;
            }
            if !bucket.allow(std::time::Instant::now()) {
                dctx.health.inputs_dropped.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            if let Ok(m) = decode_input(&d) {
                match inputs.try_send((session_id, m)) {
                    Ok(()) => {}
                    Err(mpsc::TrySendError::Full(_)) => {
                        dctx.health.inputs_dropped.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(mpsc::TrySendError::Disconnected(_)) => break,
                }
            }
        }
    });
    // The stream says goodbye, or (a watcher) asks for the next player.
    let fsim = to_sim.clone();
    let reader = async {
        while let Some(f) = read_frame(&mut recv).await {
            match decode_msg(&f) {
                Ok(Msg::Follow { .. }) if mode == MODE_WATCH => {
                    let _ = fsim.send(ToSim::Follow { session: session_id });
                }
                Ok(Msg::Bye) | Err(_) => break,
                _ => {}
            }
        }
    };
    // Whichever ends first: the client says goodbye, or the server is done
    // with it (the writer closed).
    let mut writer = writer;
    let behind = tokio::select! {
        _ = reader => false,
        e = &mut writer => matches!(e, Ok(Ended::Behind)),
    };
    datagrams.abort();
    snapshots.abort();
    sampler.abort();
    sample();
    let _ = to_sim.send(ToSim::Leave { session: session_id });
    writer.abort();
    if behind {
        eprintln!(
            "oneshot: session {session_id} ({}) cut: too far behind on the stream",
            conn.remote_address()
        );
        conn.close(outlet::CLOSE_BEHIND.into(), b"too far behind");
    } else {
        conn.close(0u32.into(), b"");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_is_held_to_its_input_rate() {
        let t0 = std::time::Instant::now();
        let mut b = InputBucket::new(t0);
        // The burst at once, then nothing.
        assert_eq!((0..200).filter(|_| b.allow(t0)).count(), INPUT_BURST as usize);
        // A second on, the rate (the burst caps what was saved).
        let t1 = t0 + Duration::from_secs(1);
        assert_eq!((0..1000).filter(|_| b.allow(t1)).count(), INPUT_BURST as usize);
        // An honest client, 64 a second for ten seconds, loses none.
        let mut b = InputBucket::new(t0);
        assert!((1..=640).all(|k| b.allow(t0 + Duration::from_micros(k * 15_625))));
    }

    #[test]
    fn a_crowd_is_retried_then_refused() {
        // 100 players: anyone up to 200 connections, then an address must
        // prove itself, then past 400 nobody.
        assert_eq!(admit(0, false, 100), Admit::Take);
        assert_eq!(admit(199, false, 100), Admit::Take);
        assert_eq!(admit(200, false, 100), Admit::Retry);
        assert_eq!(admit(200, true, 100), Admit::Take);
        assert_eq!(admit(399, true, 100), Admit::Take);
        assert_eq!(admit(400, true, 100), Admit::Refuse);
        assert_eq!(admit(400, false, 100), Admit::Refuse);
    }

    #[test]
    fn one_place_holds_its_share_and_gets_it_back() {
        let gate = Arc::new(Gate::new(1000));
        let a: IpAddr = "203.0.113.7".parse().unwrap();
        let held: Vec<Permit> = (0..PER_SOURCE).map(|_| gate.take(a).unwrap()).collect();
        assert!(gate.take(a).is_none(), "an address past its share");
        // Its neighbour on the /24 still gets in, up to the prefix's share.
        let b: IpAddr = "203.0.113.8".parse().unwrap();
        assert!(gate.take(b).is_some());
        assert_eq!(gate.busy(), PER_SOURCE as usize);
        drop(held);
        assert_eq!(gate.busy(), 0);
        assert!(gate.take(a).is_some());
        // The same address over IPv4-mapped IPv6 is the same place.
        let mapped: IpAddr = "::ffff:203.0.113.7".parse().unwrap();
        let held: Vec<Permit> = (0..PER_SOURCE).map(|_| gate.take(mapped).unwrap()).collect();
        assert!(gate.take(a).is_none());
        drop(held);
        // An IPv6 /64 is one address; loopback is never counted.
        let v6 = |s: &str| -> IpAddr { s.parse().unwrap() };
        let held: Vec<Permit> = (0..PER_SOURCE)
            .map(|i| gate.take(v6(&format!("2001:db8:0:1::{i:x}"))).unwrap())
            .collect();
        assert!(gate.take(v6("2001:db8:0:1::ffff")).is_none());
        assert!(gate.take(v6("2001:db8:0:2::1")).is_some());
        drop(held);
        let lo: Vec<Permit> = (0..3 * PER_PREFIX)
            .map(|_| gate.take(v6("127.0.0.1")).unwrap())
            .collect();
        assert_eq!(lo.len(), 3 * PER_PREFIX as usize);
    }

    #[test]
    fn a_prefix_holds_its_share() {
        let gate = Arc::new(Gate::new(1000));
        let held: Vec<Permit> = (0..PER_PREFIX)
            .map(|i| {
                gate.take(format!("198.51.100.{}", i / PER_SOURCE).parse().unwrap())
                    .unwrap()
            })
            .collect();
        assert!(gate.take("198.51.100.200".parse().unwrap()).is_none());
        assert!(gate.take("198.51.101.1".parse().unwrap()).is_some());
        drop(held);
    }
}

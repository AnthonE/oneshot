//! A session on wtransport, for a native client (the Bevy client, the
//! server's tests). The connection's two readers are tokio tasks that only
//! queue what arrives; [`NativeClient::update`] is called once a frame on
//! the app's thread and never waits on the network.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use proto::frame::LEN_PREFIX_BYTES;
use proto::{decode_msg, frame_msg, Msg, MAX_STREAM_MSG};
use sim::input::Input;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{channel, unbounded_channel, Receiver, UnboundedReceiver};
use wtransport::datagram::Datagram;
use wtransport::tls::Sha256Digest;
use wtransport::{ClientConfig, Connection, Endpoint};

use crate::session::{Session, Wire};

/// Datagrams held for the game thread at most (`web.rs` keeps as many).
const DATAGRAM_QUEUE: usize = 256;

struct ConnWire<'a>(&'a Connection);

impl Wire for ConnWire<'_> {
    fn send_datagram(&mut self, payload: &[u8]) -> bool {
        self.0.send_datagram(payload).is_ok()
    }
}

pub struct NativeClient {
    pub session: Session,
    conn: Connection,
    msgs: UnboundedReceiver<Msg>,
    /// The datagrams as they came (no copy), bounded as a browser's queue
    /// is: a game that stopped taking them drops the newest.
    datagrams: Receiver<(Instant, Datagram)>,
    closed: Arc<AtomicBool>,
    /// The close's code and reason, once the connection has ended.
    ended: Arc<std::sync::Mutex<Option<(u32, String)>>>,
    t0: Instant,
    /// Our side of the stream: kept open (dropping it would close it), and
    /// what a watcher's asks go out on.
    send: Arc<tokio::sync::Mutex<wtransport::SendStream>>,
    _endpoint: Endpoint<wtransport::endpoint::endpoint_side::Client>,
}

async fn read_frame(r: &mut wtransport::RecvStream) -> Option<Msg> {
    let mut len = [0u8; LEN_PREFIX_BYTES];
    r.read_exact(&mut len).await.ok()?;
    let mut body = vec![0u8; u16::from_le_bytes(len) as usize];
    r.read_exact(&mut body).await.ok()?;
    decode_msg(&body).ok()
}

impl NativeClient {
    /// Connects to `addr` (host:port), pinning the certificate whose
    /// SHA-256 is `cert_hash` (or trusting the system's roots without one),
    /// and says `hello` ([`Session::hello`]). Must run inside a tokio
    /// runtime, which keeps its readers.
    pub async fn connect(addr: &str, cert_hash: Option<[u8; 32]>, hello: Msg) -> Result<NativeClient, String> {
        // Bound to the server's own address family: not every host has
        // both.
        let target = tokio::net::lookup_host(addr)
            .await
            .map_err(|e| format!("resolve {addr}: {e}"))?
            .next()
            .ok_or_else(|| format!("resolve {addr}: no address"))?;
        let local: std::net::SocketAddr = if target.is_ipv4() {
            "0.0.0.0:0".parse().unwrap()
        } else {
            "[::]:0".parse().unwrap()
        };
        let b = ClientConfig::builder().with_bind_address(local);
        let cfg = match cert_hash {
            Some(h) => b.with_server_certificate_hashes([Sha256Digest::from(h)]).build(),
            None => b.with_native_certs().build(),
        };
        let endpoint = Endpoint::client(cfg).map_err(|e| format!("endpoint: {e}"))?;
        let conn = endpoint
            .connect(format!("https://{addr}/"))
            .await
            .map_err(|e| format!("connect: {e}"))?;
        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .map_err(|e| format!("stream: {e}"))?
            .await
            .map_err(|e| format!("stream: {e}"))?;
        let mut f = [0u8; MAX_STREAM_MSG + LEN_PREFIX_BYTES];
        let n = frame_msg(&hello, &mut f).map_err(|_| "hello".to_string())?;
        send.write_all(&f[..n]).await.map_err(|e| format!("hello: {e}"))?;

        let closed = Arc::new(AtomicBool::new(false));
        let (mtx, msgs) = unbounded_channel();
        let c = closed.clone();
        tokio::spawn(async move {
            while let Some(m) = read_frame(&mut recv).await {
                if mtx.send(m).is_err() {
                    break;
                }
            }
            c.store(true, Ordering::Relaxed);
        });
        let (dtx, datagrams) = channel(DATAGRAM_QUEUE);
        let rconn = conn.clone();
        tokio::spawn(async move {
            while let Ok(d) = rconn.receive_datagram().await {
                match dtx.try_send((Instant::now(), d)) {
                    Ok(()) | Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Closed(_)) => break,
                }
            }
        });
        let ended = Arc::new(std::sync::Mutex::new(None));
        let (econn, e) = (conn.clone(), ended.clone());
        tokio::spawn(async move {
            let why = match econn.closed().await {
                wtransport::error::ConnectionError::ApplicationClosed(c) => (
                    c.code().into_inner() as u32,
                    String::from_utf8_lossy(c.reason()).into_owned(),
                ),
                wtransport::error::ConnectionError::TimedOut => (0, "the server stopped answering".into()),
                _ => (0, String::new()),
            };
            if let Ok(mut e) = e.lock() {
                *e = Some(why);
            }
        });
        Ok(NativeClient {
            session: Session::for_hello(&hello),
            conn,
            msgs,
            datagrams,
            closed,
            ended,
            t0: Instant::now(),
            send: Arc::new(tokio::sync::Mutex::new(send)),
            _endpoint: endpoint,
        })
    }

    /// Milliseconds since connecting: the clock `update` runs on.
    pub fn now_ms(&self) -> f64 {
        self.ms(Instant::now())
    }

    fn ms(&self, at: Instant) -> f64 {
        at.duration_since(self.t0).as_secs_f64() * 1000.0
    }

    /// Once a frame: takes in what arrived, advances the clock with the
    /// controls as they are, sends what is due.
    pub fn update(&mut self, input: Input) {
        while let Ok(m) = self.msgs.try_recv() {
            self.session.on_msg(m);
        }
        while let Ok((at, d)) = self.datagrams.try_recv() {
            let t = self.ms(at);
            self.session.on_datagram(t, &d);
        }
        let now = self.now_ms();
        let mut wire = ConnWire(&self.conn);
        self.session.update(now, input, &mut wire);
        for m in self.session.take_outgoing() {
            let send = self.send.clone();
            tokio::spawn(async move {
                let mut f = [0u8; MAX_STREAM_MSG + LEN_PREFIX_BYTES];
                if let Ok(n) = frame_msg(&m, &mut f) {
                    let _ = send.lock().await.write_all(&f[..n]).await;
                }
            });
        }
    }

    /// Closes the connection: we're leaving.
    pub fn close(&self) {
        self.conn.close(0u32.into(), b"bye");
    }

    /// Whether the server closed the stream.
    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    /// Why the connection ended, once it has (`session::close_why`).
    pub fn ended_why(&self) -> Option<String> {
        let e = self.ended.lock().ok()?;
        let (code, reason) = e.as_ref()?;
        Some(crate::session::close_why(*code, reason))
    }

    /// Stream messages since the last call (the welcome is the session's).
    pub fn messages(&mut self) -> Vec<Msg> {
        self.session.take_messages()
    }
}

/// A lobby session on wtransport (`MODE_LOBBY`): the party's acts up, its
/// state down, and its voice both ways. For the server's tests and a
/// native client's party.
pub struct NativeLobby {
    conn: Connection,
    /// To the writer task: messages go out in the order they're given.
    out: tokio::sync::mpsc::UnboundedSender<Msg>,
    msgs: tokio::sync::Mutex<UnboundedReceiver<Msg>>,
    voices: tokio::sync::Mutex<UnboundedReceiver<proto::voice::VoiceFrame>>,
    closed: Arc<AtomicBool>,
    _endpoint: Endpoint<wtransport::endpoint::endpoint_side::Client>,
}

impl NativeLobby {
    /// Connects as [`NativeClient::connect`] does; `hello` asks for
    /// `MODE_LOBBY`.
    pub async fn connect(addr: &str, cert_hash: Option<[u8; 32]>, hello: Msg) -> Result<NativeLobby, String> {
        let c = NativeClient::connect(addr, cert_hash, hello).await?;
        let NativeClient {
            conn,
            msgs,
            datagrams,
            closed,
            send,
            _endpoint,
            ..
        } = c;
        let (vtx, voices) = unbounded_channel();
        let mut datagrams = datagrams;
        tokio::spawn(async move {
            while let Some((_, d)) = datagrams.recv().await {
                if proto::datagram_kind(&d) != Some(proto::DG_VOICE) {
                    continue;
                }
                if let Ok(f) = proto::voice::decode_voice(&d) {
                    if vtx.send(f).is_err() {
                        break;
                    }
                }
            }
        });
        let (out, mut outq) = unbounded_channel::<Msg>();
        tokio::spawn(async move {
            let mut f = [0u8; MAX_STREAM_MSG + LEN_PREFIX_BYTES];
            let mut send = send.lock().await;
            while let Some(m) = outq.recv().await {
                let Ok(n) = frame_msg(&m, &mut f) else { continue };
                if send.write_all(&f[..n]).await.is_err() {
                    break;
                }
            }
        });
        Ok(NativeLobby {
            conn,
            out,
            msgs: tokio::sync::Mutex::new(msgs),
            voices: tokio::sync::Mutex::new(voices),
            closed,
            _endpoint,
        })
    }

    pub async fn send(&self, m: &Msg) -> Result<(), String> {
        self.send_now(*m);
        Ok(())
    }

    /// A message to the server, after every one before it; never waits.
    pub fn send_now(&self, m: Msg) {
        let _ = self.out.send(m);
    }

    /// Stream messages since the last call, without waiting.
    pub fn take(&self) -> Vec<Msg> {
        let mut out = Vec::new();
        if let Ok(mut q) = self.msgs.try_lock() {
            while let Ok(m) = q.try_recv() {
                out.push(m);
            }
        }
        out
    }

    /// Voice frames since the last call, without waiting.
    pub fn take_voice(&self) -> Vec<proto::voice::VoiceFrame> {
        let mut out = Vec::new();
        if let Ok(mut q) = self.voices.try_lock() {
            while let Ok(f) = q.try_recv() {
                out.push(f);
            }
        }
        out
    }

    pub fn send_voice(&self, f: &proto::voice::VoiceFrame) -> bool {
        let mut buf = [0u8; 8 + proto::voice::VOICE_MAX_BYTES];
        match proto::voice::encode_voice_up(f, &mut buf) {
            Ok(n) => self.conn.send_datagram(&buf[..n]).is_ok(),
            Err(_) => false,
        }
    }

    /// The next stream message, if one comes within `wait`.
    pub async fn next(&self, wait: std::time::Duration) -> Option<Msg> {
        tokio::time::timeout(wait, self.msgs.lock().await.recv())
            .await
            .ok()
            .flatten()
    }

    /// The next voice frame, if one comes within `wait`.
    pub async fn voice(&self, wait: std::time::Duration) -> Option<proto::voice::VoiceFrame> {
        tokio::time::timeout(wait, self.voices.lock().await.recv())
            .await
            .ok()
            .flatten()
    }

    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    pub fn close(&self) {
        self.conn.close(0u32.into(), b"bye");
    }
}

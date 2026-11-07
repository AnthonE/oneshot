//! A session on wtransport, for a native client (the Bevy client, the
//! server's tests). The connection's two readers are tokio tasks that only
//! queue what arrives; [`NativeClient::update`] is called once a frame on
//! the app's thread and never waits on the network.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use proto::frame::LEN_PREFIX_BYTES;
use proto::voice::{self, VoiceFrame};
use proto::{decode_msg, frame_msg, Msg, MAX_STREAM_MSG};
use sim::input::Input;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio::sync::Notify;
use wtransport::datagram::Datagram;
use wtransport::tls::Sha256Digest;
use wtransport::{ClientConfig, Connection, Endpoint, RecvStream, SendStream};

use crate::session::{Session, Wire};

/// Datagrams held for the game thread at most (`web.rs` keeps as many).
const DATAGRAM_QUEUE: usize = 256;
/// A lobby's messages and voice frames held at most (`web_lobby.rs`'s
/// `KEEP`).
const LOBBY_KEEP: usize = 256;

type ClientEndpoint = Endpoint<wtransport::endpoint::endpoint_side::Client>;

struct ConnWire<'a>(&'a Connection);

impl Wire for ConnWire<'_> {
    fn send_datagram(&mut self, payload: &[u8]) -> bool {
        self.0.send_datagram(payload).is_ok()
    }
}

/// What a reader task fills and the game takes from, bounded as a
/// browser's queue is: past `cap` the oldest goes, since the newest
/// matter most.
struct Keep<T> {
    q: Mutex<VecDeque<T>>,
    cap: usize,
    more: Notify,
    /// The reader has stopped: nothing more will come.
    done: AtomicBool,
}

impl<T> Keep<T> {
    fn new(cap: usize) -> Arc<Keep<T>> {
        Arc::new(Keep {
            q: Mutex::new(VecDeque::with_capacity(cap)),
            cap,
            more: Notify::new(),
            done: AtomicBool::new(false),
        })
    }

    fn push(&self, t: T) {
        {
            let mut q = self.q.lock().unwrap_or_else(PoisonError::into_inner);
            if q.len() >= self.cap {
                q.pop_front();
            }
            q.push_back(t);
        }
        self.more.notify_one();
    }

    fn end(&self) {
        self.done.store(true, Ordering::Release);
        self.more.notify_one();
    }

    fn pop(&self) -> Option<T> {
        self.q.lock().unwrap_or_else(PoisonError::into_inner).pop_front()
    }

    /// Everything held, without waiting.
    fn take(&self) -> Vec<T> {
        self.q
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
            .collect()
    }

    /// The next one, waiting for it; None once the reader has stopped and
    /// all it gave is taken.
    async fn next(&self) -> Option<T> {
        loop {
            if let Some(t) = self.pop() {
                return Some(t);
            }
            if self.done.load(Ordering::Acquire) {
                // Pass the wake on to anyone else waiting; and the last
                // one may have come since the look above.
                self.more.notify_one();
                return self.pop();
            }
            self.more.notified().await;
        }
    }
}

pub struct NativeClient {
    pub session: Session,
    conn: Connection,
    msgs: UnboundedReceiver<Msg>,
    /// The datagrams as they came (no copy), bounded as a browser's queue
    /// is: a game that stopped taking them loses the oldest.
    datagrams: Arc<Keep<(Instant, Datagram)>>,
    closed: Arc<AtomicBool>,
    /// The close's code and reason, once the connection has ended.
    ended: Arc<Mutex<Option<(u32, String)>>>,
    t0: Instant,
    /// Our side of the stream: kept open (dropping it would close it), and
    /// what a watcher's asks go out on.
    send: Arc<tokio::sync::Mutex<SendStream>>,
    _endpoint: ClientEndpoint,
}

/// The next frame off the stream: None at its end, or at a frame no
/// stream may carry (none long, or past `proto::MAX_STREAM_MSG`: the end,
/// as the browser's reader and the server's take it); `Some(None)` for one
/// that didn't decode (skipped, as a browser's reader skips it).
async fn read_frame(r: &mut RecvStream) -> Option<Option<Msg>> {
    let mut len = [0u8; LEN_PREFIX_BYTES];
    r.read_exact(&mut len).await.ok()?;
    let n = u16::from_le_bytes(len) as usize;
    if n == 0 || n > proto::MAX_STREAM_MSG {
        return None;
    }
    let mut body = vec![0u8; n];
    r.read_exact(&mut body).await.ok()?;
    Some(decode_msg(&body).ok())
}

/// Reads the stream until it ends, handing each message to `put` (false:
/// nobody wants them any more), then marks it `closed` and calls `end`.
fn read_stream(
    mut recv: RecvStream,
    closed: Arc<AtomicBool>,
    mut put: impl FnMut(Msg) -> bool + Send + 'static,
    end: impl FnOnce() + Send + 'static,
) {
    tokio::spawn(async move {
        while let Some(m) = read_frame(&mut recv).await {
            if let Some(m) = m {
                if !put(m) {
                    break;
                }
            }
        }
        closed.store(true, Ordering::Relaxed);
        end();
    });
}

/// A connection to `addr` and its stream, `hello` said on it.
async fn open(
    addr: &str,
    cert_hash: Option<[u8; 32]>,
    hello: &Msg,
) -> Result<(ClientEndpoint, Connection, SendStream, RecvStream), String> {
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
    let (mut send, recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("stream: {e}"))?
        .await
        .map_err(|e| format!("stream: {e}"))?;
    let mut f = [0u8; MAX_STREAM_MSG + LEN_PREFIX_BYTES];
    let n = frame_msg(hello, &mut f).map_err(|_| "hello".to_string())?;
    send.write_all(&f[..n]).await.map_err(|e| format!("hello: {e}"))?;
    Ok((endpoint, conn, send, recv))
}

impl NativeClient {
    /// Connects to `addr` (host:port), pinning the certificate whose
    /// SHA-256 is `cert_hash` (or trusting the system's roots without one),
    /// and says `hello` ([`Session::hello`]). Must run inside a tokio
    /// runtime, which keeps its readers. Dropping it closes the connection.
    pub async fn connect(addr: &str, cert_hash: Option<[u8; 32]>, hello: Msg) -> Result<NativeClient, String> {
        let (endpoint, conn, send, recv) = open(addr, cert_hash, &hello).await?;
        let closed = Arc::new(AtomicBool::new(false));
        let (mtx, msgs) = unbounded_channel();
        read_stream(recv, closed.clone(), move |m| mtx.send(m).is_ok(), || {});
        let datagrams = Keep::new(DATAGRAM_QUEUE);
        let (rconn, q) = (conn.clone(), datagrams.clone());
        tokio::spawn(async move {
            // Ends with the connection (closed by either side, or dropped).
            while let Ok(d) = rconn.receive_datagram().await {
                q.push((Instant::now(), d));
            }
            q.end();
        });
        let ended = Arc::new(Mutex::new(None));
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
        while let Some((at, d)) = self.datagrams.pop() {
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

impl Drop for NativeClient {
    /// The readers end with the connection: nothing is left holding it.
    fn drop(&mut self) {
        self.close();
    }
}

/// A lobby session on wtransport (`MODE_LOBBY`): the party's acts up, its
/// state down, and its voice both ways. For the server's tests and a
/// native client's party. Held as the browser's is (`web_lobby.rs`): what
/// arrives is kept up to [`LOBBY_KEEP`] each, the oldest dropped; a voice
/// frame that isn't one Opus frame is dropped either way.
pub struct NativeLobby {
    conn: Connection,
    /// To the writer task: messages go out in the order they're given.
    out: tokio::sync::mpsc::UnboundedSender<Msg>,
    msgs: Arc<Keep<Msg>>,
    voices: Arc<Keep<VoiceFrame>>,
    closed: Arc<AtomicBool>,
    _endpoint: ClientEndpoint,
}

impl NativeLobby {
    /// Connects as [`NativeClient::connect`] does; `hello` asks for
    /// `MODE_LOBBY`. Dropping it closes the connection.
    pub async fn connect(addr: &str, cert_hash: Option<[u8; 32]>, hello: Msg) -> Result<NativeLobby, String> {
        let (endpoint, conn, mut send, recv) = open(addr, cert_hash, &hello).await?;
        let closed = Arc::new(AtomicBool::new(false));
        let msgs = Keep::new(LOBBY_KEEP);
        let (mq, me) = (msgs.clone(), msgs.clone());
        read_stream(
            recv,
            closed.clone(),
            move |m| {
                mq.push(m);
                true
            },
            move || me.end(),
        );
        let voices = Keep::new(LOBBY_KEEP);
        let (vconn, vq) = (conn.clone(), voices.clone());
        tokio::spawn(async move {
            while let Ok(d) = vconn.receive_datagram().await {
                if proto::datagram_kind(&d) != Some(proto::DG_VOICE) {
                    continue;
                }
                if let Ok(f) = voice::decode_voice(&d) {
                    if voice::is_one_frame(f.payload()) {
                        vq.push(f);
                    }
                }
            }
            vq.end();
        });
        let (out, mut outq) = unbounded_channel::<Msg>();
        tokio::spawn(async move {
            let mut f = [0u8; MAX_STREAM_MSG + LEN_PREFIX_BYTES];
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
            msgs,
            voices,
            closed,
            _endpoint: endpoint,
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
        self.msgs.take()
    }

    /// Voice frames since the last call, without waiting.
    pub fn take_voice(&self) -> Vec<VoiceFrame> {
        self.voices.take()
    }

    /// One Opus frame of ours to the party, checked as the browser's
    /// `WebLobby::talk` is. False if it was dropped (not one 20 ms frame,
    /// too big for the path, or the connection refused it).
    pub fn talk(&self, f: &VoiceFrame) -> bool {
        voice::is_one_frame(f.payload()) && self.send_voice(f)
    }

    /// A voice frame to the party as it is, without [`NativeLobby::talk`]'s
    /// one-frame check: the server's tests send it what a client shouldn't.
    pub fn send_voice(&self, f: &VoiceFrame) -> bool {
        let mut buf = [0u8; 8 + voice::VOICE_MAX_BYTES];
        let Ok(n) = voice::encode_voice_up(f, &mut buf) else {
            return false;
        };
        if self.conn.max_datagram_size().is_some_and(|max| n > max) {
            return false;
        }
        self.conn.send_datagram(&buf[..n]).is_ok()
    }

    /// The next stream message, if one comes within `wait`.
    pub async fn next(&self, wait: std::time::Duration) -> Option<Msg> {
        tokio::time::timeout(wait, self.msgs.next()).await.ok().flatten()
    }

    /// The next voice frame, if one comes within `wait`.
    pub async fn voice(&self, wait: std::time::Duration) -> Option<VoiceFrame> {
        tokio::time::timeout(wait, self.voices.next()).await.ok().flatten()
    }

    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    pub fn close(&self) {
        self.conn.close(0u32.into(), b"bye");
    }
}

impl Drop for NativeLobby {
    /// Its readers and writer end with the connection.
    fn drop(&mut self) {
        self.close();
    }
}

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
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use wtransport::tls::Sha256Digest;
use wtransport::{ClientConfig, Connection, Endpoint};

use crate::session::{Session, Wire};

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
    datagrams: UnboundedReceiver<(Instant, Vec<u8>)>,
    closed: Arc<AtomicBool>,
    t0: Instant,
    /// Kept open: dropping it would close our side of the stream.
    _send: wtransport::SendStream,
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
        let (dtx, datagrams) = unbounded_channel();
        let rconn = conn.clone();
        tokio::spawn(async move {
            while let Ok(d) = rconn.receive_datagram().await {
                if dtx.send((Instant::now(), d.to_vec())).is_err() {
                    break;
                }
            }
        });
        Ok(NativeClient {
            session: Session::new(),
            conn,
            msgs,
            datagrams,
            closed,
            t0: Instant::now(),
            _send: send,
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
    }

    /// Closes the connection: we're leaving.
    pub fn close(&self) {
        self.conn.close(0u32.into(), b"bye");
    }

    /// Whether the server closed the stream.
    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    /// Stream messages since the last call (the welcome is the session's).
    pub fn messages(&mut self) -> Vec<Msg> {
        self.session.take_messages()
    }
}

//! WebTransport: the endpoint, and one task set per session. Endpoint setup
//! after engine-shim's `engine2/src/net.rs` and Gates' `spawn_shard`; the
//! datagram send checks the live MTU and never waits (Gates' `send_snap`).

use std::sync::mpsc;
use std::time::Duration;

use proto::frame::LEN_PREFIX_BYTES;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use proto::{
    datagram_kind, decode_input, decode_msg, Msg, DG_INPUT, MAX_STREAM_MSG, MODE_DOUBLES, MODE_DUEL, PROTO_VER,
};
use tokio::sync::mpsc as tmpsc;
use wtransport::endpoint::endpoint_side::Server;
use wtransport::endpoint::IncomingSession;
use wtransport::{Endpoint, Identity, RecvStream, ServerConfig};

use crate::{auth, queue, Config, Ctx, Out, ToSim, OUT_QUEUE};

const KEEP_ALIVE: Duration = Duration::from_secs(5);
const IDLE_TIMEOUT: Duration = Duration::from_secs(15);
const HANDSHAKE: Duration = Duration::from_secs(10);

pub async fn endpoint(cfg: &Config) -> Result<(Endpoint<Server>, [u8; 32]), String> {
    let identity = match &cfg.cert {
        Some((cert, key)) => Identity::load_pemfiles(cert, key)
            .await
            .map_err(|e| format!("{cert}, {key}: {e}"))?,
        None => Identity::self_signed(["localhost", "127.0.0.1", "::1"]).map_err(|e| e.to_string())?,
    };
    let hash = *identity
        .certificate_chain()
        .as_slice()
        .first()
        .ok_or("an empty certificate chain")?
        .hash()
        .as_ref();
    let config = ServerConfig::builder()
        .with_bind_address(cfg.bind)
        .with_identity(identity)
        .keep_alive_interval(Some(KEEP_ALIVE))
        .max_idle_timeout(Some(IDLE_TIMEOUT))
        .map_err(|e| e.to_string())?
        .build();
    let ep = Endpoint::server(config).map_err(|e| format!("bind {}: {e}", cfg.bind))?;
    Ok((ep, hash))
}

pub async fn accept_loop(ep: Endpoint<Server>, to_sim: mpsc::Sender<ToSim>, ctx: Arc<Ctx>) {
    loop {
        let incoming = ep.accept().await;
        tokio::spawn(session(incoming, to_sim.clone(), ctx.clone()));
    }
}

async fn read_frame(r: &mut RecvStream) -> Option<Vec<u8>> {
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

async fn refuse(send: &mut wtransport::SendStream, reason: u8) {
    let mut f = [0u8; MAX_STREAM_MSG + LEN_PREFIX_BYTES];
    if let Ok(n) = proto::frame_msg(&Msg::Refuse { reason }, &mut f) {
        let _ = send.write_all(&f[..n]).await;
        let _ = send.finish().await;
    }
}

async fn session(incoming: IncomingSession, to_sim: mpsc::Sender<ToSim>, ctx: Arc<Ctx>) {
    let opened = tokio::time::timeout(HANDSHAKE, async {
        let request = incoming.await.ok()?;
        let conn = request.accept().await.ok()?;
        let (send, mut recv) = conn.accept_bi().await.ok()?;
        let hello = decode_msg(&read_frame(&mut recv).await?).ok()?;
        Some((conn, send, recv, hello))
    })
    .await;
    let Ok(Some((conn, mut send, mut recv, hello))) = opened else {
        return;
    };
    let Msg::Hello {
        proto,
        name,
        pass,
        mode,
    } = hello
    else {
        return;
    };
    if proto != PROTO_VER {
        return refuse(&mut send, proto::REFUSE_VERSION).await;
    }
    // Who: a pass from Elo Pros, or a guest.
    let who = if pass.is_empty() {
        None
    } else {
        let Some(key) = ctx.elo_pubkey else {
            return refuse(&mut send, proto::REFUSE_PASS).await;
        };
        match auth::verify_pass(
            pass.as_str(),
            &key,
            &ctx.slug,
            &ctx.domains,
            |n| ctx.nonces.take(n),
            auth::now_unix(),
        ) {
            Ok(p) => Some(p),
            Err(_) => return refuse(&mut send, proto::REFUSE_PASS).await,
        }
    };

    let session_id = ctx.sessions.fetch_add(1, Ordering::Relaxed);
    let (tx, mut rx) = tmpsc::channel::<Out>(OUT_QUEUE);
    let join = match mode {
        MODE_DUEL | MODE_DOUBLES => {
            // Ranked is a wallet's: that is what a rating belongs to.
            let Some(wallet) = who.and_then(|p| p.wallet) else {
                return refuse(&mut send, proto::REFUSE_NEEDS_WALLET).await;
            };
            let ladder = if mode == MODE_DOUBLES { "doubles" } else { "duel" };
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
                }),
                tx,
            }
        }
        _ => ToSim::Casual {
            session: session_id,
            name,
            tx,
        },
    };
    if to_sim.send(join).is_err() {
        return;
    }

    // The writer: stream messages in order, datagrams if they fit.
    let wconn = conn.clone();
    let writer = tokio::spawn(async move {
        while let Some(o) = rx.recv().await {
            match o {
                Out::Datagram(d) => {
                    if wconn.max_datagram_size().is_some_and(|max| d.len() <= max) {
                        let _ = wconn.send_datagram(d);
                    }
                }
                Out::Stream(f) => {
                    if send.write_all(&f).await.is_err() {
                        break;
                    }
                }
                Out::Close => break,
            }
        }
        let _ = send.finish().await;
    });

    // Inputs as datagrams; the stream only says goodbye.
    let dconn = conn.clone();
    let dsim = to_sim.clone();
    let datagrams = tokio::spawn(async move {
        while let Ok(d) = dconn.receive_datagram().await {
            if datagram_kind(&d) != Some(DG_INPUT) {
                continue;
            }
            if let Ok(m) = decode_input(&d) {
                let _ = dsim.send(ToSim::Input {
                    session: session_id,
                    msg: Box::new(m),
                });
            }
        }
    });
    let reader = async {
        while let Some(f) = read_frame(&mut recv).await {
            if matches!(decode_msg(&f), Ok(Msg::Bye) | Err(_)) {
                break;
            }
        }
    };
    // Whichever ends first: the client says goodbye, or the server is done
    // with it (the writer closed).
    let mut writer = writer;
    tokio::select! {
        _ = reader => {}
        _ = &mut writer => {}
    }
    datagrams.abort();
    let _ = to_sim.send(ToSim::Leave { session: session_id });
    writer.abort();
    conn.close(0u32.into(), b"");
}

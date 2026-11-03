//! `oneshot-server [--bind 0.0.0.0:4433] [--http 0.0.0.0:8080|off]
//! [--web web/dist] [--cert cert.pem --key key.pem] [--doubles]
//! [--passive-bots] [--map <id|name>]… [--cup-min 4] [--cup-rounds 3]
//! [--slug oneshot] [--domain host]…
//! [--elo-pubkey <base64>] [--elo-origin https://elopros.com]
//! [--elo-key server.pem --elo-key-id eu-1] [--replays dir]
//! [--public-addr host:port] [--allow-origin https://elopros.com]…
//! [--capacity 256] [--drain-max 900]`
//!
//! Without `--elo-pubkey` nobody can sign in and only casual play is open.
//! Without `--elo-key` ranked results aren't reported.
//!
//! Signals (`ops/oneshot.service`): SIGTERM drains (no new players, every
//! match plays out, at most `--drain-max` seconds) and exits; SIGHUP reads
//! the certificate again (an ACME renewal); SIGINT exits at once.

use std::net::SocketAddr;

use base64::Engine;
use sim::world::Rules;

fn main() {
    let mut cfg = server::Config::default();
    let mut cert = None;
    let mut key = None;
    let mut elo_key = None;
    let mut elo_key_id = None;
    let mut domains = Vec::new();
    let mut drain_max = std::time::Duration::from_secs(900);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| die(&format!("{a} needs a value")));
        match a.as_str() {
            "--bind" => cfg.bind = addr(&val()),
            "--http" => {
                let v = val();
                cfg.http = if v == "off" { None } else { Some(addr(&v)) };
            }
            "--web" => cfg.web_dir = val().into(),
            "--cert" => cert = Some(val()),
            "--key" => key = Some(val()),
            "--doubles" => cfg.rules = Rules::doubles(),
            "--passive-bots" => cfg.passive_bots = true,
            "--map" => {
                let v = val();
                let id = sim::maps::ALL
                    .iter()
                    .find(|d| d.id.to_string() == v || d.name.eq_ignore_ascii_case(&v))
                    .unwrap_or_else(|| die(&format!("no map {v}")))
                    .id;
                cfg.maps.push(id);
            }
            "--cup-min" => cfg.cup_min = val().parse().unwrap_or_else(|_| die("--cup-min is a count")),
            "--cup-rounds" => cfg.cup_rounds = val().parse().unwrap_or_else(|_| die("--cup-rounds is a count")),
            "--slug" => cfg.slug = val(),
            "--domain" => domains.push(val()),
            "--elo-pubkey" => {
                let b = base64::engine::general_purpose::STANDARD
                    .decode(val().trim())
                    .unwrap_or_else(|_| die("--elo-pubkey is standard base64"));
                cfg.elo_pubkey = Some(b.try_into().unwrap_or_else(|_| die("--elo-pubkey is 32 bytes")));
            }
            "--elo-origin" => cfg.elo_origin = val(),
            "--elo-key" => elo_key = Some(val()),
            "--elo-key-id" => elo_key_id = Some(val()),
            "--replays" => cfg.replays = Some(val().into()),
            "--public-addr" => cfg.public_addr = Some(val()),
            "--allow-origin" => cfg.allow_origins.push(val().trim_end_matches('/').to_string()),
            "--capacity" => cfg.capacity = val().parse().unwrap_or_else(|_| die("--capacity is a count")),
            "--drain-max" => {
                drain_max =
                    std::time::Duration::from_secs(val().parse().unwrap_or_else(|_| die("--drain-max is seconds")))
            }
            _ => die(&format!("unknown argument {a}")),
        }
    }
    if let (Some(c), Some(k)) = (cert, key) {
        cfg.cert = Some((c, k));
    }
    if !domains.is_empty() {
        cfg.domains = domains;
    }
    match (elo_key, elo_key_id) {
        (Some(k), Some(id)) => cfg.elo_key = Some((k.into(), id)),
        (None, None) => {}
        _ => die("--elo-key and --elo-key-id go together"),
    }
    let rt = tokio::runtime::Runtime::new().unwrap_or_else(|e| die(&e.to_string()));
    rt.block_on(async {
        let h = server::run(cfg).await.unwrap_or_else(|e| die(&e));
        // Tests read these lines: keep their shape.
        println!("game: {} (WebTransport)", h.addr);
        println!("cert sha-256: {}", server::hex(&h.cert_hash));
        if let Some(http) = h.http {
            println!("play: http://127.0.0.1:{}/", http.port());
        }
        let (recv, send) = h.udp_buffers;
        if recv < server::net::UDP_BUF_BYTES || send < server::net::UDP_BUF_BYTES {
            // Linux reports twice what it keeps; under the ask means
            // `net.core.rmem_max` / `wmem_max` clamped it (ops/README.md).
            eprintln!(
                "oneshot: udp buffers recv {recv} send {send} (asked {})",
                server::net::UDP_BUF_BYTES
            );
        }
        signals(&h, drain_max).await;
    });
}

#[cfg(unix)]
async fn signals(h: &server::Handle, drain_max: std::time::Duration) {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).unwrap_or_else(|e| die(&e.to_string()));
    let mut hup = signal(SignalKind::hangup()).unwrap_or_else(|e| die(&e.to_string()));
    loop {
        tokio::select! {
            _ = term.recv() => {
                eprintln!("oneshot: draining (at most {drain_max:?})");
                h.drain(drain_max).await;
                eprintln!("oneshot: drained");
                return;
            }
            _ = hup.recv() => match h.reload().await {
                Ok(hash) => eprintln!("oneshot: certificate reloaded, sha-256 {}", server::hex(&hash)),
                Err(e) => eprintln!("oneshot: certificate reload: {e}"),
            },
            _ = tokio::signal::ctrl_c() => return,
        }
    }
}

#[cfg(not(unix))]
async fn signals(_: &server::Handle, _: std::time::Duration) {
    let _ = tokio::signal::ctrl_c().await;
}

fn addr(s: &str) -> SocketAddr {
    s.parse().unwrap_or_else(|_| die(&format!("{s} is not host:port")))
}

fn die(msg: &str) -> ! {
    eprintln!("oneshot-server: {msg}");
    std::process::exit(2)
}

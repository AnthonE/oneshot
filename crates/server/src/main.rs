//! `oneshot-server [--bind 0.0.0.0:4433] [--http 0.0.0.0:8080|off]
//! [--web web/dist] [--cert cert.pem --key key.pem] [--doubles]
//! [--passive-bots] [--slug oneshot] [--domain host]…
//! [--elo-pubkey <base64>] [--elo-origin https://elopros.com]
//! [--elo-key server.pem --elo-key-id eu-1] [--replays dir]`
//!
//! Without `--elo-pubkey` nobody can sign in and only casual play is open.
//! Without `--elo-key` ranked results aren't reported.

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
        std::future::pending::<()>().await;
    });
}

fn addr(s: &str) -> SocketAddr {
    s.parse().unwrap_or_else(|_| die(&format!("{s} is not host:port")))
}

fn die(msg: &str) -> ! {
    eprintln!("oneshot-server: {msg}");
    std::process::exit(2)
}

//! `oneshot [--server URL] [--name you] [--mode casual|duel|doubles|ffa|gungame|bigteam|0..6]
//! [--autoplay] [--lite] [--capsules]` (or `oneshot --pose 'yaw=1.57&arm=3'`,
//! no server, for a picture): the desktop client. It reads the game
//! server's address and certificate hash from the page server's
//! `server.json` (what the browser page reads), connects with client-net's
//! native session, and runs the Bevy client.
//!
//! The server is the public one unless `--server` names another:
//! `https://game.elopros.com/oneshot` (production: a CA certificate, so
//! `server.json` sends no hash and the system's roots are trusted), or a
//! local `oneshot-server` as `127.0.0.1:8080` / `http://127.0.0.1:8080`
//! (its own certificate, pinned by the hash it sends).

/// Where the desktop client plays unless told otherwise (ops/README.md).
#[cfg(not(target_arch = "wasm32"))]
const PUBLIC: &str = "https://game.elopros.com/oneshot";

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(e) = run() {
        eprintln!("oneshot: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", HELP.replace("{PUBLIC}", PUBLIC));
        return Ok(());
    }
    let lite = args.iter().any(|a| a == "--lite");
    if let Some(q) = arg("--pose") {
        // No server: one fighter for a picture (`--pose 'yaw=1.57&arm=3'`).
        let pose = oneshot_client::pose::Pose::parse(&q);
        let opts = oneshot_client::Opts {
            lite,
            pose: Some(pose),
            ..Default::default()
        };
        oneshot_client::app(Box::new(oneshot_client::link::Still::on(pose.2.map)), opts).run();
        return Ok(());
    }
    let server = arg("--server").unwrap_or_else(|| PUBLIC.into());
    let name = arg("--name").unwrap_or_else(|| "player".into());
    let mode = match arg("--mode") {
        Some(m) => mode(&m).ok_or_else(|| format!("--mode {m}: one of {MODES}"))?,
        None => proto::MODE_CASUAL,
    };

    let body = fetch::server_json(&server)?;
    let (addr, hash) =
        parse_server_json(&body).ok_or_else(|| format!("{server}: server.json doesn't name a game server"))?;

    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let hello = client_net::session::Session::hello(&name, proto::Pass::NONE, mode, 0);
    let client = rt
        .block_on(client_net::native::NativeClient::connect(&addr, hash, hello))
        .map_err(|e| format!("{addr}: {e}"))?;
    let opts = oneshot_client::Opts {
        autoplay: args.iter().any(|a| a == "--autoplay"),
        lite,
        pose: None,
        capsules: args.iter().any(|a| a == "--capsules"),
        fxdemo: args.iter().any(|a| a == "--fxdemo"),
        fxstage: None,
    };
    oneshot_client::app(Box::new(oneshot_client::link::Native { client, _rt: rt }), opts).run();
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
const HELP: &str = "oneshot: the desktop client

  oneshot                          play on the public server ({PUBLIC})
  oneshot --server 127.0.0.1:8080  play on a local oneshot-server
  --name NAME                      the name others see (default: player)
  --mode MODE                      casual (default), ffa, gungame, bigteam; duel and doubles
                                   are ranked, which needs an Elo Pros sign-in (the web page)
  --lite                           the light tier: for a slow or software GPU
  --pose 'yaw=1.57&arm=3'          no server: one fighter, for a picture
  --pose 'map=6&eye=-20,6,0&look=0,2,0'  no server: a map's view

Keys (by default; rebind them, with the mouse's speed, the field of view
and the volume, in the settings, saved in your config folder under
oneshot/settings.toml): WASD and the mouse, space jumps, Ctrl or C crouches,
R reloads, E uses, G throws a frag, T picks frag or sticky, Q or V melees,
X, 1 or the wheel swaps, N the net graph, Esc lets the mouse go.";

#[cfg(not(target_arch = "wasm32"))]
const MODES: &str = "casual, duel, doubles, ffa, gungame, bigteam (or 0 to 6)";

/// A mode by name or number (proto's `MODE_*`).
#[cfg(not(target_arch = "wasm32"))]
fn mode(m: &str) -> Option<u8> {
    Some(match m.to_ascii_lowercase().as_str() {
        "casual" | "bot" => proto::MODE_CASUAL,
        "duel" => proto::MODE_DUEL,
        "doubles" => proto::MODE_DOUBLES,
        "ffa" => proto::MODE_FFA,
        "gungame" | "gun-game" => proto::MODE_GUN_GAME,
        "cup" => proto::MODE_CUP,
        "bigteam" | "big-team" => proto::MODE_BIG_TEAM,
        n => return n.parse().ok().filter(|&n| n <= proto::MODE_BIG_TEAM),
    })
}

/// server.json's `addr` and, when it pins one, the certificate's hash:
/// `{"addr": "host:port", "hash": "hex" | null, ...}`.
#[cfg(not(target_arch = "wasm32"))]
fn parse_server_json(body: &str) -> Option<(String, Option<[u8; 32]>)> {
    let field = |k: &str| {
        let at = body.find(&format!("\"{k}\""))? + k.len() + 2;
        let rest = body[at..].trim_start().strip_prefix(':')?.trim_start();
        let rest = rest.strip_prefix('"')?;
        Some(rest[..rest.find('"')?].to_string())
    };
    let addr = field("addr")?;
    let hash = field("hash").and_then(|h| {
        let h: String = h.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        (h.len() == 64).then(|| {
            let mut out = [0u8; 32];
            for (i, b) in out.iter_mut().enumerate() {
                *b = u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap_or(0);
            }
            out
        })
    });
    Some((addr, hash))
}

/// `GET <server>/server.json`, over HTTPS (the system's roots) or plain
/// HTTP (a local server): one request, HTTP/1.0, the body back.
#[cfg(not(target_arch = "wasm32"))]
mod fetch {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::Arc;
    use std::time::Duration;

    pub fn server_json(server: &str) -> Result<String, String> {
        let (tls, rest) = match server.split_once("://") {
            Some(("https", r)) => (true, r),
            Some(("http", r)) => (false, r),
            Some((s, _)) => return Err(format!("{server}: http:// or https://, not {s}://")),
            None => (false, server),
        };
        let (hostport, path) = match rest.find('/') {
            Some(i) => (&rest[..i], rest[i..].trim_end_matches('/')),
            None => (rest, ""),
        };
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => {
                (h, p.parse::<u16>().map_err(|_| format!("{server}: bad port {p}"))?)
            }
            _ => (hostport, if tls { 443 } else { 80 }),
        };
        let at = format!("{host}:{port}");
        let tcp = std::net::ToSocketAddrs::to_socket_addrs(&at)
            .map_err(|e| format!("{at}: {e}"))?
            .find_map(|a| TcpStream::connect_timeout(&a, Duration::from_secs(10)).ok())
            .ok_or_else(|| format!("{at}: can't connect (is the server up?)"))?;
        let _ = tcp.set_read_timeout(Some(Duration::from_secs(10)));
        let req = format!(
            "GET {path}/server.json HTTP/1.0\r\nHost: {host}\r\nUser-Agent: oneshot-desktop\r\nConnection: close\r\n\r\n"
        );
        let mut raw = Vec::new();
        if tls {
            let mut roots = rustls::RootCertStore::empty();
            for c in rustls_native_certs::load_native_certs().certs {
                let _ = roots.add(c);
            }
            let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|e| e.to_string())?
                .with_root_certificates(roots)
                .with_no_client_auth();
            let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|e| format!("{host}: {e}"))?;
            let conn = rustls::ClientConnection::new(Arc::new(cfg), name).map_err(|e| e.to_string())?;
            let mut s = rustls::StreamOwned::new(conn, tcp);
            s.write_all(req.as_bytes()).map_err(|e| format!("{at}: {e}"))?;
            // A server that closes without TLS's close_notify still sent the body.
            if let Err(e) = s.read_to_end(&mut raw) {
                if raw.is_empty() {
                    return Err(format!("{at}: {e}"));
                }
            }
        } else {
            let mut s = tcp;
            s.write_all(req.as_bytes()).map_err(|e| format!("{at}: {e}"))?;
            s.read_to_end(&mut raw).map_err(|e| format!("{at}: {e}"))?;
        }
        let text = String::from_utf8_lossy(&raw);
        let (head, body) = text.split_once("\r\n\r\n").ok_or_else(|| format!("{at}: no reply"))?;
        let status = head.split_whitespace().nth(1).unwrap_or("");
        if status != "200" {
            return Err(format!("{server}/server.json: HTTP {status}"));
        }
        Ok(body.to_string())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #[test]
    fn server_json_reads_both_forms() {
        let (a, h) =
            super::parse_server_json(r#"{"addr":"game.example.com:4433","hash":null,"game":"oneshot"}"#).unwrap();
        assert_eq!(a, "game.example.com:4433");
        assert!(h.is_none());
        let hex = "ab".repeat(32);
        let (a, h) = super::parse_server_json(&format!(r#"{{"addr": "127.0.0.1:4433", "hash": "{hex}"}}"#)).unwrap();
        assert_eq!(a, "127.0.0.1:4433");
        assert_eq!(h, Some([0xab; 32]));
    }

    #[test]
    fn modes_by_name() {
        assert_eq!(super::mode("Duel"), Some(proto::MODE_DUEL));
        assert_eq!(super::mode("6"), Some(proto::MODE_BIG_TEAM));
        assert_eq!(super::mode("7"), None);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}

//! `oneshot --server 127.0.0.1:8080 [--name you] [--mode 0..6] [--autoplay] [--lite] [--capsules]`
//! (or `oneshot --pose 'yaw=1.57&arm=3'`, no server, for a picture): the
//! desktop client. It reads the game server's address and certificate
//! hash from the page server's `server.json` (what the browser page reads),
//! connects with client-net's native session, and runs the Bevy client.

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use std::io::{Read, Write};

    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let lite = args.iter().any(|a| a == "--lite");
    if let Some(q) = arg("--pose") {
        // No server: one fighter for a picture (`--pose 'yaw=1.57&arm=3'`).
        let opts = oneshot_client::Opts {
            lite,
            pose: Some(oneshot_client::pose::Pose::parse(&q)),
            ..Default::default()
        };
        oneshot_client::app(Box::new(oneshot_client::link::Still::new()), opts).run();
        return;
    }
    let server = arg("--server").unwrap_or_else(|| "127.0.0.1:8080".into());
    let name = arg("--name").unwrap_or_else(|| "player".into());
    let mode = arg("--mode").and_then(|m| m.parse().ok()).unwrap_or(proto::MODE_CASUAL);

    // server.json: {"addr": "host:port", "hash": "hex", ...}, over plain
    // HTTP from the page server.
    let mut s = std::net::TcpStream::connect(&server).unwrap_or_else(|e| panic!("{server}: {e}"));
    write!(s, "GET /server.json HTTP/1.0\r\nHost: {server}\r\n\r\n").unwrap();
    let mut body = String::new();
    s.read_to_string(&mut body).unwrap();
    let field = |k: &str| {
        let at = body.find(&format!("\"{k}\""))? + k.len() + 2;
        let rest = &body[at..];
        let q = rest.find('"')? + 1;
        Some(rest[q..q + rest[q..].find('"')?].to_string())
    };
    let addr = field("addr").expect("server.json names the game server");
    let hash = field("hash").and_then(|h| {
        let h: String = h.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        (h.len() == 64).then(|| {
            let mut out = [0u8; 32];
            for (i, b) in out.iter_mut().enumerate() {
                *b = u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap();
            }
            out
        })
    });

    let rt = tokio::runtime::Runtime::new().unwrap();
    let hello = client_net::session::Session::hello(&name, proto::Pass::NONE, mode, 0);
    let client = rt
        .block_on(client_net::native::NativeClient::connect(&addr, hash, hello))
        .unwrap_or_else(|e| panic!("{addr}: {e}"));
    let opts = oneshot_client::Opts {
        autoplay: args.iter().any(|a| a == "--autoplay"),
        lite,
        pose: None,
        capsules: args.iter().any(|a| a == "--capsules"),
        fxdemo: args.iter().any(|a| a == "--fxdemo"),
        fxstage: None,
    };
    oneshot_client::app(Box::new(oneshot_client::link::Native { client, _rt: rt }), opts).run();
}

#[cfg(target_arch = "wasm32")]
fn main() {}

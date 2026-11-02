//! `oneshot-server [--bind 0.0.0.0:4433] [--http 0.0.0.0:8080] [--web web/dist]
//! [--cert cert.pem --key key.pem] [--doubles] [--passive-bots]`

use std::net::SocketAddr;

use sim::world::Rules;

fn main() {
    let mut cfg = server::Config {
        bind: "0.0.0.0:4433".parse().unwrap(),
        http: Some("0.0.0.0:8080".parse().unwrap()),
        web_dir: "web/dist".into(),
        cert: None,
        rules: Rules::duel(),
        passive_bots: false,
    };
    let mut cert = None;
    let mut key = None;
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
            _ => die(&format!("unknown argument {a}")),
        }
    }
    if let (Some(c), Some(k)) = (cert, key) {
        cfg.cert = Some((c, k));
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

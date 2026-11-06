//! What hosting needs of the server (docs/DESIGN.md §14), over real QUIC
//! on loopback: past capacity a hello is answered "full"; a drain lets a
//! match play out, writes its replay and turns new players away; a reload
//! hands new handshakes the new certificate and leaves live sessions be;
//! `server.json` names the public address and pins nothing when the
//! certificate is a CA's; and two slots side by side hand players over:
//! both name the live one, and either takes the other's nonces.

use std::time::{Duration, Instant};

use proto::frame::LEN_PREFIX_BYTES;
use proto::{decode_msg, frame_msg, Msg, Name, MAX_STREAM_MSG, PROTO_VER};
use sim::world::Rules;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wtransport::tls::Sha256Digest;
use wtransport::{ClientConfig, Endpoint, Identity};

async fn read_frame(r: &mut wtransport::RecvStream) -> Option<Msg> {
    let mut len = [0u8; LEN_PREFIX_BYTES];
    r.read_exact(&mut len).await.ok()?;
    let mut body = vec![0u8; u16::from_le_bytes(len) as usize];
    r.read_exact(&mut body).await.ok()?;
    decode_msg(&body).ok()
}

struct Player {
    _ep: Endpoint<wtransport::endpoint::endpoint_side::Client>,
    conn: wtransport::Connection,
    recv: wtransport::RecvStream,
    _send: wtransport::SendStream,
}

/// Connects pinning `hash`, says hello for casual, and returns the player
/// and the first message back.
async fn join(port: u16, hash: [u8; 32], name: &str) -> Result<(Player, Option<Msg>), String> {
    join_stamped(port, hash, name, sim::STAMP).await
}

/// [`join`], its hello carrying `stamp` for its sim's.
async fn join_stamped(port: u16, hash: [u8; 32], name: &str, stamp: u32) -> Result<(Player, Option<Msg>), String> {
    let cfg = ClientConfig::builder()
        .with_bind_address("127.0.0.1:0".parse().unwrap())
        .with_server_certificate_hashes([Sha256Digest::from(hash)])
        .build();
    let ep = Endpoint::client(cfg).map_err(|e| e.to_string())?;
    let conn = ep
        .connect(format!("https://127.0.0.1:{port}/"))
        .await
        .map_err(|e| e.to_string())?;
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| e.to_string())?
        .await
        .map_err(|e| e.to_string())?;
    let mut f = [0u8; MAX_STREAM_MSG + LEN_PREFIX_BYTES];
    let n = frame_msg(
        &Msg::Hello {
            proto: PROTO_VER,
            name: Name::new(name),
            pass: proto::Pass::NONE,
            mode: proto::MODE_CASUAL,
            skin: 0,
            emblem: 0,
            party: 0,
            stamp,
        },
        &mut f,
    )
    .unwrap();
    send.write_all(&f[..n]).await.map_err(|e| e.to_string())?;
    let first = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut recv))
        .await
        .ok()
        .flatten();
    Ok((
        Player {
            _ep: ep,
            conn,
            recv,
            _send: send,
        },
        first,
    ))
}

fn local() -> server::Config {
    server::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        http: None,
        ..server::Config::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn past_capacity_a_hello_is_answered_full() {
    let h = server::run(server::Config { capacity: 1, ..local() }).await.unwrap();
    let (_a, first) = join(h.addr.port(), h.cert_hash, "first").await.unwrap();
    assert!(
        matches!(first, Some(Msg::Welcome { .. })),
        "the first gets in: {first:?}"
    );
    let (_b, second) = join(h.addr.port(), h.cert_hash, "second").await.unwrap();
    assert_eq!(
        second,
        Some(Msg::Refuse {
            reason: proto::REFUSE_FULL
        })
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_page_whose_sim_is_not_ours_is_told_to_reload() {
    // A map or a rule changed since the page loaded: the wire is the same,
    // the walls are not.
    let h = server::run(local()).await.unwrap();
    let (_a, first) = join_stamped(h.addr.port(), h.cert_hash, "stale", sim::STAMP ^ 1)
        .await
        .unwrap();
    assert_eq!(
        first,
        Some(Msg::Refuse {
            reason: proto::REFUSE_VERSION
        })
    );
    let (_b, second) = join(h.addr.port(), h.cert_hash, "fresh").await.unwrap();
    assert!(matches!(second, Some(Msg::Welcome { .. })), "{second:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drain_lets_the_match_play_out_and_turns_new_players_away() {
    let dir = std::env::temp_dir().join(format!("oneshot-drain-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let h = server::run(server::Config {
        rules: Rules {
            score_limit: 1000,
            time_limit_ms: 3000,
            ..Rules::duel()
        },
        passive_bots: true,
        linger: Duration::from_millis(500),
        replays: Some(dir.clone()),
        ..local()
    })
    .await
    .unwrap();
    let (mut a, first) = join(h.addr.port(), h.cert_hash, "stays").await.unwrap();
    assert!(matches!(first, Some(Msg::Welcome { .. })));

    let t0 = Instant::now();
    let port = h.addr.port();
    let hash = h.cert_hash;
    let drain = h.drain(Duration::from_secs(30));
    let late = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        join(port, hash, "late").await.unwrap().1
    };
    // The one already playing hears the match out, to its goodbye.
    let hears = async {
        let mut over = false;
        while let Some(m) = read_frame(&mut a.recv).await {
            if matches!(m, Msg::Reveal { .. }) {
                over = true;
            }
            if m == Msg::Bye {
                break;
            }
        }
        over
    };
    let ((), late, over) = tokio::join!(drain, late, hears);
    assert_eq!(
        late,
        Some(Msg::Refuse {
            reason: proto::REFUSE_FULL
        }),
        "nobody new while draining"
    );
    assert!(over, "the match was played to its end");
    assert!(
        t0.elapsed() < Duration::from_secs(20),
        "the drain waited on the match, not its cap"
    );
    let replays: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "osr"))
        .collect();
    assert_eq!(replays.len(), 1, "its replay was written before the drain returned");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reload_hands_new_handshakes_the_new_certificate() {
    let dir = std::env::temp_dir().join(format!("oneshot-cert-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (cert, key) = (dir.join("fullchain.pem"), dir.join("privkey.pem"));
    let write = |id: &Identity| {
        let chain: String = id.certificate_chain().as_slice().iter().map(|c| c.to_pem()).collect();
        std::fs::write(&cert, chain).unwrap();
        std::fs::write(&key, id.private_key().to_secret_pem()).unwrap();
        *id.certificate_chain().as_slice()[0].hash().as_ref()
    };
    let old = write(&Identity::self_signed(["localhost", "127.0.0.1"]).unwrap());
    let h = server::run(server::Config {
        cert: Some((cert.display().to_string(), key.display().to_string())),
        rules: Rules {
            score_limit: 1000,
            time_limit_ms: 0,
            ..Rules::duel()
        },
        ..local()
    })
    .await
    .unwrap();
    assert_eq!(h.cert_hash, old);
    let (mut live, first) = join(h.addr.port(), old, "before").await.unwrap();
    assert!(matches!(first, Some(Msg::Welcome { .. })));

    let new = write(&Identity::self_signed(["localhost", "127.0.0.1"]).unwrap());
    assert_ne!(new, old);
    assert_eq!(h.reload().await.unwrap(), new);

    // A new handshake is shown the new certificate, not the old.
    let (_after, first) = join(h.addr.port(), new, "after").await.unwrap();
    assert!(matches!(first, Some(Msg::Welcome { .. })), "{first:?}");
    assert!(
        join(h.addr.port(), old, "stale").await.is_err(),
        "the old certificate is gone"
    );

    // The session from before still hears its room.
    let more = tokio::time::timeout(Duration::from_secs(2), read_frame(&mut live.recv)).await;
    let alive = live.conn.receive_datagram();
    let dg = tokio::time::timeout(Duration::from_secs(2), alive).await;
    assert!(
        more.is_ok() || dg.is_ok_and(|d| d.is_ok()),
        "the live session kept going"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

async fn get(port: u16, path: &str, origin: Option<&str>) -> (u16, String, String) {
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let origin = origin.map_or(String::new(), |o| format!("Origin: {o}\r\n"));
    s.write_all(format!("GET {path} HTTP/1.1\r\nHost: game.example\r\n{origin}\r\n").as_bytes())
        .await
        .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    let (head, body) = out.split_once("\r\n\r\n").unwrap();
    let code = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (code, head.to_string(), body.to_string())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_page_is_told_the_public_address_and_pins_only_its_own_certificate() {
    // Dev: the page's host at the bound port, and the hash to pin.
    let h = server::run(server::Config {
        http: Some("127.0.0.1:0".parse().unwrap()),
        ..local()
    })
    .await
    .unwrap();
    let port = h.http.unwrap().port();
    let (_, head, body) = get(port, "/server.json", Some("https://elopros.com")).await;
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["addr"], format!("game.example:{}", h.addr.port()));
    assert_eq!(v["hash"], server::hex(&h.cert_hash));
    assert!(!head.contains("Access-Control-Allow-Origin"), "no origin was allowed");

    // Production: a CA's certificate, the public name, the page's origin.
    let dir = std::env::temp_dir().join(format!("oneshot-ca-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let id = Identity::self_signed(["localhost"]).unwrap();
    std::fs::write(dir.join("c.pem"), id.certificate_chain().as_slice()[0].to_pem()).unwrap();
    std::fs::write(dir.join("k.pem"), id.private_key().to_secret_pem()).unwrap();
    let h = server::run(server::Config {
        http: Some("127.0.0.1:0".parse().unwrap()),
        cert: Some((
            dir.join("c.pem").display().to_string(),
            dir.join("k.pem").display().to_string(),
        )),
        public_addr: Some("game.elopros.com:61250".into()),
        allow_origins: vec!["https://elopros.com".into()],
        ..local()
    })
    .await
    .unwrap();
    let port = h.http.unwrap().port();
    let (code, head, body) = get(port, "/server.json", Some("https://elopros.com")).await;
    assert_eq!(code, 200);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["addr"], "game.elopros.com:61250");
    assert!(v["hash"].is_null(), "a CA's certificate is never pinned: {body}");
    assert_eq!(v["proto"], PROTO_VER);
    assert!(
        head.contains("Access-Control-Allow-Origin: https://elopros.com"),
        "{head}"
    );
    let (_, head, _) = get(port, "/nonce", Some("https://evil.example")).await;
    assert!(!head.contains("Access-Control-Allow-Origin"), "only the origins named");

    // Ready until it drains; healthy throughout. It says what the session
    // writers counted.
    let (code, _, body) = get(port, "/ready", None).await;
    assert_eq!(code, 200);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    for k in ["snapshots_dropped", "snapshots_oversize", "cut_behind"] {
        assert_eq!(v[k], 0, "{k}: {body}");
    }
    h.drain(Duration::from_secs(5)).await;
    assert_eq!(get(port, "/ready", None).await.0, 503);
    assert_eq!(get(port, "/health", None).await.0, 200);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_slots_hand_new_players_from_the_old_to_the_new() {
    let dir = std::env::temp_dir().join(format!("oneshot-slots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let live = dir.join("live");
    let slot = |public: &str| server::Config {
        http: Some("127.0.0.1:0".parse().unwrap()),
        public_addr: Some(public.into()),
        nonce_key: Some([5u8; 32]),
        live_file: Some(live.clone()),
        ..local()
    };
    let a = server::run(slot("game.example:61250")).await.unwrap();
    let b = server::run(slot("game.example:61251")).await.unwrap();
    let addr_of = |h: &server::Handle| {
        let port = h.http.unwrap().port();
        async move {
            let (_, _, body) = get(port, "/server.json", None).await;
            serde_json::from_str::<serde_json::Value>(&body).unwrap()["addr"]
                .as_str()
                .unwrap()
                .to_string()
        }
    };

    // No live file yet: each names itself.
    assert_eq!(addr_of(&a).await, "game.example:61250");
    assert_eq!(addr_of(&b).await, "game.example:61251");
    // The deploy points at B: both say B, whichever nginx asked.
    std::fs::write(&live, "game.example:61251\n").unwrap();
    assert_eq!(addr_of(&a).await, "game.example:61251");
    assert_eq!(addr_of(&b).await, "game.example:61251");
    // A live file that isn't an address is never passed on.
    std::fs::write(&live, "\",\"hash\":\"x").unwrap();
    assert_eq!(addr_of(&a).await, "game.example:61250");
    std::fs::write(&live, "game.example:61251").unwrap();

    // A page that got its nonce from A signs in at B.
    let (_, _, body) = get(a.http.unwrap().port(), "/nonce", None).await;
    let nonce = serde_json::from_str::<serde_json::Value>(&body).unwrap()["nonce"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(b.ctx.nonces.take(&nonce), "B takes A's nonce");
    assert!(!b.ctx.nonces.take(&nonce), "once");

    // A drains (nothing to play out here): it turns new players away, and B
    // takes them.
    a.drain(Duration::from_secs(5)).await;
    let (_, at_a) = join(a.addr.port(), a.cert_hash, "late").await.unwrap();
    assert_eq!(
        at_a,
        Some(Msg::Refuse {
            reason: proto::REFUSE_FULL
        })
    );
    let (_, at_b) = join(b.addr.port(), b.cert_hash, "late").await.unwrap();
    assert!(matches!(at_b, Some(Msg::Welcome { .. })), "{at_b:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// One number out of `/health.json` (a flat document of numbers).
fn field(doc: &str, key: &str) -> u64 {
    let at = doc
        .find(&format!("\"{key}\":"))
        .unwrap_or_else(|| panic!("{key} in {doc}"))
        + key.len()
        + 3;
    doc[at..].split([',', '}']).next().unwrap().parse().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_status_counts_joins_refusals_and_ticks() {
    let h = server::run(server::Config {
        capacity: 1,
        http: Some("127.0.0.1:0".parse().unwrap()),
        ..local()
    })
    .await
    .unwrap();
    let port = h.http.unwrap().port();
    let (_a, first) = join(h.addr.port(), h.cert_hash, "first").await.unwrap();
    assert!(matches!(first, Some(Msg::Welcome { .. })), "{first:?}");
    let (_b, second) = join(h.addr.port(), h.cert_hash, "second").await.unwrap();
    assert!(matches!(second, Some(Msg::Refuse { .. })), "{second:?}");
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (code, head, doc) = get(port, "/health.json", None).await;
    assert_eq!(code, 200);
    assert!(head.contains("application/json"), "{head}");
    assert_eq!(field(&doc, "joins_casual"), 1, "{doc}");
    assert_eq!(field(&doc, "matches"), 1, "{doc}");
    assert!(
        doc.contains("\"refused\":[0,0,1,0,0,0,0,0,0,0,0,0,0,0,0]"),
        "one REFUSE_FULL: {doc}"
    );
    assert!(field(&doc, "ticks") > 10, "{doc}");
    assert!(field(&doc, "tick_p50_us") > 0, "{doc}");
    assert_eq!(field(&doc, "proto"), PROTO_VER as u64, "{doc}");
}

/// A POST of `body`, the head and the body sent apart (as a browser may).
async fn post(port: u16, path: &str, headers: &str, body: &[u8]) -> u16 {
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: game.example\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n{headers}\r\n",
        body.len()
    );
    s.write_all(head.as_bytes()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    s.write_all(body).await.unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    out.split_whitespace().nth(1).unwrap().parse().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_report_is_kept_and_a_flood_is_not() {
    let dir = std::env::temp_dir().join(format!("oneshot-bugs-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let h = server::run(server::Config {
        http: Some("127.0.0.1:0".parse().unwrap()),
        bugs: Some(dir.clone()),
        ..local()
    })
    .await
    .unwrap();
    let port = h.http.unwrap().port();
    let report = br#"{"kind":"crash","text":"panicked at src/lib.rs:1:1"}"#;
    assert_eq!(post(port, "/bugs", "", report).await, 204);
    assert_eq!(post(port, "/bugs", "", b"not json").await, 400);
    let big = vec![b'x'; server::bugs::BODY_MAX + 1];
    assert_eq!(post(port, "/bugs", "", &big).await, 413);
    // One address gets its share (the first one counted), then waits;
    // another behind the proxy still may.
    for _ in 1..server::bugs::PER_ADDR {
        assert_eq!(post(port, "/bugs", "", report).await, 204);
    }
    assert_eq!(post(port, "/bugs", "", report).await, 429);
    assert_eq!(post(port, "/bugs", "X-Real-IP: 203.0.113.9\r\n", report).await, 204);
    let files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
    assert_eq!(files.len(), 1);
    let text = std::fs::read_to_string(files[0].path()).unwrap();
    assert_eq!(text.lines().count(), server::bugs::PER_ADDR as usize + 1, "{text}");
    for l in text.lines() {
        let v: serde_json::Value = serde_json::from_str(l).unwrap();
        assert_eq!(v["report"]["kind"], "crash");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_place_for_them_reports_are_not_taken() {
    let h = server::run(server::Config {
        http: Some("127.0.0.1:0".parse().unwrap()),
        ..local()
    })
    .await
    .unwrap();
    assert_eq!(post(h.http.unwrap().port(), "/bugs", "", b"{}").await, 404);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_endpoint_takes_players_and_server_json_hands_them_out_in_turn() {
    let h = server::run(server::Config {
        endpoints: 2,
        http: Some("127.0.0.1:0".parse().unwrap()),
        ..local()
    })
    .await
    .unwrap();
    assert_eq!(h.addrs.len(), 2);
    let (a, b) = (h.addrs[0].port(), h.addrs[1].port());
    assert_ne!(a, b);
    for (port, name) in [(a, "first"), (b, "second")] {
        let (_p, said) = join(port, h.cert_hash, name).await.unwrap();
        assert!(matches!(said, Some(Msg::Welcome { .. })), "on {port}: {said:?}");
    }
    let http = h.http.unwrap().port();
    let mut seen = Vec::new();
    for _ in 0..4 {
        let (_, _, doc) = get(http, "/server.json", None).await;
        let v: serde_json::Value = serde_json::from_str(&doc).unwrap();
        let addr = v["addr"].as_str().unwrap().to_string();
        seen.push(addr.rsplit_once(':').unwrap().1.parse::<u16>().unwrap());
    }
    assert_eq!(seen, vec![a, b, a, b]);
    // The other slot, by the same flags: its port and every step after.
    assert_eq!(server::net::ports(61251, 3, 10), vec![61251, 61261, 61271]);
}

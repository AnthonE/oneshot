//! Ranked, end to end over real QUIC: two players sign in with passes from
//! a stand-in Elo Pros (its account key a test key), queue for a duel, are
//! matched, play it out, and see the salt revealed; the result reaches the
//! stand-in signed with the server's key; the replay on disk re-runs to the
//! same hash. And what is refused: a ranked player without a wallet, and a
//! pass whose nonce was already used.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use proto::frame::LEN_PREFIX_BYTES;
use proto::{decode_msg, frame_msg, Msg, Name, Pass, MAX_STREAM_MSG, MODE_DUEL, PROTO_VER};
use ring::signature::{Ed25519KeyPair, KeyPair};
use sim::world::Rules;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wtransport::tls::Sha256Digest;
use wtransport::{ClientConfig, Endpoint};

const ACCOUNT_SEED: [u8; 32] = [9; 32];
const SERVER_SEED: [u8; 32] = [5; 32];
const WALLETS: [&str; 2] = [
    "0x1111111111111111111111111111111111111111",
    "0x2222222222222222222222222222222222222222",
];

/// What the stand-in Elo Pros got: (headers, body) of each report.
type Seen = Arc<Mutex<Vec<(Vec<(String, String)>, String)>>>;

/// A stand-in Elo Pros: ratings for anyone (1500), and reports recorded.
async fn stand_in() -> (String, Seen) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let seen: Seen = Arc::default();
    let s2 = seen.clone();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = l.accept().await {
            let seen = s2.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let head_end = loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                let mut lines = head.lines();
                let first = lines.next().unwrap_or("").to_string();
                let headers: Vec<(String, String)> = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
                    .collect();
                let len: usize = headers
                    .iter()
                    .find(|(k, _)| k == "content-length")
                    .map_or(0, |(_, v)| v.parse().unwrap_or(0));
                while buf.len() < head_end + len {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                let body = String::from_utf8_lossy(&buf[head_end..]).to_string();
                let reply = if first.starts_with("POST /api/ratings/oneshot/report") {
                    seen.lock().unwrap().push((headers, body));
                    r#"{"ok":true}"#.to_string()
                } else {
                    r#"{"ladders":{"duel":{"rating":1500}}}"#.to_string()
                };
                let _ = sock
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            reply.len(),
                            reply
                        )
                        .as_bytes(),
                    )
                    .await;
            });
        }
    });
    (origin, seen)
}

/// A pass from the stand-in account key, as Elo Pros mints them.
fn pass(wallet: Option<&str>, nonce: &str) -> String {
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let exp = server::auth::now_unix() + 600;
    let text = format!(
        "elo account\nid: acct_0123456789abcdef0123\nfor: play:oneshot\nshard: 127.0.0.1\nnonce: {nonce}\nwallet: {}\nexp: {exp}",
        wallet.unwrap_or("none")
    );
    let k = Ed25519KeyPair::from_seed_unchecked(&ACCOUNT_SEED).unwrap();
    format!("{}.{}", b64.encode(&text), b64.encode(k.sign(text.as_bytes()).as_ref()))
}

async fn nonce(http: std::net::SocketAddr) -> String {
    let mut s = tokio::net::TcpStream::connect(http).await.unwrap();
    s.write_all(b"GET /nonce HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .await
        .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    let at = out.find("\"nonce\":\"").unwrap() + 9;
    out[at..at + 32].to_string()
}

async fn read_msg(r: &mut wtransport::RecvStream) -> Option<Msg> {
    let mut len = [0u8; LEN_PREFIX_BYTES];
    r.read_exact(&mut len).await.ok()?;
    let mut body = vec![0u8; u16::from_le_bytes(len) as usize];
    r.read_exact(&mut body).await.ok()?;
    decode_msg(&body).ok()
}

/// Connects, says hello, and returns every stream message until the server
/// closes the session (or 30 s pass).
async fn play(h: &server::Handle, pass: &str, mode: u8) -> Vec<Msg> {
    let cfg = ClientConfig::builder()
        .with_bind_address("127.0.0.1:0".parse().unwrap())
        .with_server_certificate_hashes([Sha256Digest::from(h.cert_hash)])
        .build();
    let ep = Endpoint::client(cfg).unwrap();
    let conn = ep
        .connect(format!("https://127.0.0.1:{}/", h.addr.port()))
        .await
        .unwrap();
    let (mut send, mut recv) = conn.open_bi().await.unwrap().await.unwrap();
    let mut f = [0u8; MAX_STREAM_MSG + LEN_PREFIX_BYTES];
    let hello = Msg::Hello {
        proto: PROTO_VER,
        name: Name::new("ranked"),
        pass: Pass::new(pass).unwrap(),
        mode,
    };
    let n = frame_msg(&hello, &mut f).unwrap();
    send.write_all(&f[..n]).await.unwrap();
    let mut got = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(m) = read_msg(&mut recv).await {
            let end = matches!(m, Msg::Bye | Msg::Refuse { .. });
            got.push(m);
            if end {
                break;
            }
        }
    })
    .await;
    got
}

fn pem(seed: &[u8; 32]) -> String {
    let mut der = vec![
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
    ];
    der.extend_from_slice(seed);
    format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
        base64::engine::general_purpose::STANDARD.encode(der)
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_ranked_duel_is_matched_played_reported_and_replays() {
    let (origin, seen) = stand_in().await;
    let dir: PathBuf = std::env::temp_dir().join(format!("oneshot-ranked-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let key_path = dir.join("server.pem");
    std::fs::write(&key_path, pem(&SERVER_SEED)).unwrap();
    let account = Ed25519KeyPair::from_seed_unchecked(&ACCOUNT_SEED).unwrap();
    let h = server::run(server::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        http: Some("127.0.0.1:0".parse().unwrap()),
        duel: Rules {
            time_limit_ms: 4000,
            ..Rules::duel()
        },
        domains: vec!["127.0.0.1".into()],
        elo_pubkey: Some(account.public_key().as_ref().try_into().unwrap()),
        elo_origin: origin,
        elo_key: Some((key_path, "t-1".into())),
        replays: Some(dir.clone()),
        ..server::Config::default()
    })
    .await
    .unwrap();
    let http = h.http.unwrap();

    // Refused: ranked without a wallet; a nonce used twice.
    let n = nonce(http).await;
    let got = play(&h, &pass(None, &n), MODE_DUEL).await;
    assert_eq!(
        got,
        vec![Msg::Refuse {
            reason: proto::REFUSE_NEEDS_WALLET
        }]
    );
    let got = play(&h, &pass(Some(WALLETS[0]), &n), MODE_DUEL).await;
    assert_eq!(
        got,
        vec![Msg::Refuse {
            reason: proto::REFUSE_PASS
        }]
    );

    // Two players queue and play.
    let (n0, n1) = (nonce(http).await, nonce(http).await);
    let (p0, p1) = (pass(Some(WALLETS[0]), &n0), pass(Some(WALLETS[1]), &n1));
    let (a, b) = tokio::join!(play(&h, &p0, MODE_DUEL), play(&h, &p1, MODE_DUEL));
    for got in [&a, &b] {
        assert!(matches!(got.first(), Some(Msg::Queued { .. })), "{got:?}");
        let commit = got.iter().find_map(|m| match m {
            Msg::Welcome { salt_commit, .. } => Some(*salt_commit),
            _ => None,
        });
        let salt = got.iter().find_map(|m| match m {
            Msg::Reveal { salt } => Some(*salt),
            _ => None,
        });
        assert_eq!(proto::salt_commit(salt.expect("revealed")), commit.expect("welcomed"));
        assert!(
            got.iter().any(|m| matches!(m, Msg::Phase { phase: 2, .. })),
            "the match ended"
        );
        assert_eq!(got.last(), Some(&Msg::Bye));
    }

    // The report, signed with the server's key.
    let mut report = None;
    for _ in 0..50 {
        if let Some(r) = seen.lock().unwrap().first().cloned() {
            report = Some(r);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let (headers, body) = report.expect("a report reached Elo Pros");
    let get = |k: &str| headers.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap();
    assert_eq!(get("x-elo-server"), "oneshot/t-1");
    let text = server::elo::server_text(
        "POST",
        "/ratings/oneshot/report",
        "oneshot/t-1",
        get("x-elo-time").parse().unwrap(),
        body.as_bytes(),
        &get("x-elo-nonce"),
    );
    let sig = base64::engine::general_purpose::STANDARD
        .decode(get("x-elo-signature"))
        .unwrap();
    let server_pub = Ed25519KeyPair::from_seed_unchecked(&SERVER_SEED).unwrap();
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, server_pub.public_key().as_ref())
        .verify(text.as_bytes(), &sig)
        .expect("signed with the server's key");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let m = &v["matches"][0];
    assert_eq!(m["ladder"], "duel");
    let mut players: Vec<String> = m["teams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["players"][0].as_str().unwrap().to_string())
        .collect();
    players.sort();
    assert_eq!(players, WALLETS.map(String::from).to_vec());
    // Nobody fired: a draw, both first.
    assert!(m["teams"].as_array().unwrap().iter().all(|t| t["place"] == 1));

    // The replay on disk re-runs to the same end; a changed byte doesn't.
    let file = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "osr") && p.to_string_lossy().contains(m["id"].as_str().unwrap()))
        .expect("the ranked match's replay");
    let mut bytes = std::fs::read(&file).unwrap();
    room::replay::verify(&bytes).expect("replays");
    let at = bytes.len() - 20;
    bytes[at] ^= 1;
    assert!(room::replay::verify(&bytes).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

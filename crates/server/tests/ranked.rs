//! Ranked, end to end over real QUIC: two players sign in with passes from
//! a stand-in Elo Pros (its account key a test key), queue for a duel, are
//! matched, play it out, and see the salt revealed; the result reaches the
//! stand-in signed with the server's key; the replay on disk re-runs to the
//! same hash, and the report's evidence names it. And what is refused: a
//! ranked player without a wallet, a pass whose nonce was already used, and
//! a wallet the game has banned.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use proto::frame::LEN_PREFIX_BYTES;
use proto::{decode_msg, frame_msg, Msg, Name, Pass, MAX_STREAM_MSG, MODE_CUP, MODE_DUEL, PROTO_VER};
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
const BANNED: &str = "0x3333333333333333333333333333333333333333";

/// What the stand-in Elo Pros got: (request line, headers, body) of each
/// POST.
type Seen = Arc<Mutex<Vec<(String, Vec<(String, String)>, String)>>>;

/// A stand-in Elo Pros: ratings for anyone (1500), [`BANNED`] banned, and
/// posts recorded. `old`: one from before ratings took `evidence`, which
/// refuses it as Elo Pros refused unknown keys.
async fn stand_in(old: bool) -> (String, Seen) {
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
                let (code, reply) = if old && first.starts_with("POST /api/ratings/") && body.contains("evidence") {
                    (
                        422,
                        r#"{"error":"matches[0]: evidence is not a field here. What may be written: id, ladder, teams, agents."}"#
                            .to_string(),
                    )
                } else if first.starts_with("POST ") {
                    seen.lock().unwrap().push((first, headers, body));
                    (200, r#"{"ok":true}"#.to_string())
                } else if first.starts_with("GET /api/items/of/") && first.contains("game=oneshot") {
                    // The first wallet owns the ember skin; nobody else owns any.
                    let items = if first.contains(WALLETS[0]) {
                        r#"[{"token_id":1,"catalog_id":101,"catalog_name":"Ember"}]"#
                    } else {
                        "[]"
                    };
                    (200, format!(r#"{{"items":{items}}}"#))
                } else if first.starts_with("GET /api/bans/oneshot/of/") {
                    (200, format!(r#"{{"banned":{}}}"#, first.contains(BANNED)))
                } else {
                    (200, r#"{"ladders":{"duel":{"rating":1500}}}"#.to_string())
                };
                let _ = sock
                    .write_all(
                        format!(
                            "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
    let len = out[at..].find('"').unwrap();
    out[at..at + len].to_string()
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
    play_as(h, pass, mode, proto::SKIN_STANDARD).await
}

/// [`play`], asking to wear `skin`.
async fn play_as(h: &server::Handle, pass: &str, mode: u8, skin: u8) -> Vec<Msg> {
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
        skin,
        party: 0,
    };
    let n = frame_msg(&hello, &mut f).unwrap();
    send.write_all(&f[..n]).await.unwrap();
    let mut got = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(m) = read_msg(&mut recv).await {
            // A cup's entrant plays several matches on one session: only a
            // bye or a refusal ends it.
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
    let (origin, seen) = stand_in(false).await;
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
    let got = play(&h, &pass(Some(BANNED), &nonce(http).await), MODE_DUEL).await;
    assert_eq!(
        got,
        vec![Msg::Refuse {
            reason: proto::REFUSE_BANNED
        }]
    );

    // Two players queue and play.
    let (n0, n1) = (nonce(http).await, nonce(http).await);
    let (p0, p1) = (pass(Some(WALLETS[0]), &n0), pass(Some(WALLETS[1]), &n1));
    // Each asks for a skin: the first owns ember, the second doesn't own
    // gilded and plays in the standard look.
    let (a, b) = tokio::join!(play_as(&h, &p0, MODE_DUEL, 2), play_as(&h, &p1, MODE_DUEL, 5));
    let slot_of = |got: &[Msg]| {
        got.iter().find_map(|m| match m {
            Msg::Welcome { slot, .. } => Some(*slot),
            _ => None,
        })
    };
    let skin_of = |got: &[Msg], who: u8| {
        got.iter().rev().find_map(|m| match m {
            Msg::Seat { slot, skin, .. } if *slot == who => Some(*skin),
            _ => None,
        })
    };
    let (sa, sb) = (slot_of(&a).unwrap(), slot_of(&b).unwrap());
    for got in [&a, &b] {
        assert_eq!(skin_of(got, sa), Some(2), "the owned skin, seen by both");
        assert_eq!(
            skin_of(got, sb),
            Some(proto::SKIN_STANDARD),
            "an unowned skin is not worn"
        );
    }
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
    let (_, headers, body) = report.expect("a report reached Elo Pros");
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
    // The report's evidence is this file's hash and the commitment the
    // players were welcomed with.
    assert_eq!(m["evidence"]["replay_sha256"], server::elo::sha256_hex(&bytes));
    let commit = a.iter().find_map(|m| match m {
        Msg::Welcome { salt_commit, .. } => Some(server::hex(salt_commit)),
        _ => None,
    });
    assert_eq!(m["evidence"]["salt_commit"].as_str(), commit.as_deref());
    // The aim review's numbers sit beside it, one row a person.
    let watch: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(file.with_extension("watch.json")).unwrap()).unwrap();
    assert_eq!(watch["replay_sha256"], m["evidence"]["replay_sha256"]);
    assert_eq!(watch["seats"].as_array().unwrap().len(), 2);
    assert!(watch["seats"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["flags"] == serde_json::json!([])));
    assert!(
        !dir.join("review").exists(),
        "nothing to review in a match nobody fired in"
    );
    let at = bytes.len() - 20;
    bytes[at] ^= 1;
    assert!(room::replay::verify(&bytes).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ban_is_signed_and_an_old_elo_pros_still_takes_the_result() {
    let (origin, seen) = stand_in(true).await;
    let key = || Some(server::elo::ServerKey::from_seed("oneshot", "t-1", &SERVER_SEED).unwrap());
    let elo = server::elo::Elo::new(&origin, "oneshot", key());
    assert_eq!(elo.banned(BANNED).await, Some(true));
    assert_eq!(elo.banned(WALLETS[0]).await, Some(false));
    // Elo Pros out of reach: "couldn't look", never "banned".
    let gone = server::elo::Elo::new("http://127.0.0.1:9", "oneshot", None);
    assert_eq!(gone.banned(BANNED).await, None);

    elo.ban(WALLETS[1], true, "aim review", Some("m-1")).await.unwrap();
    let (line, headers, body) = seen.lock().unwrap().last().cloned().unwrap();
    assert!(line.starts_with("POST /api/bans/oneshot "), "{line}");
    let get = |k: &str| headers.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap();
    let text = server::elo::server_text(
        "POST",
        "/bans/oneshot",
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
    assert_eq!(
        v,
        serde_json::json!({"wallet": WALLETS[1], "status": "banned", "reason": "aim review", "match": "m-1"})
    );

    // An Elo Pros that refuses `evidence` gets the result without it.
    let r = server::elo::Report {
        id: "m-old".into(),
        ladder: "duel".into(),
        sides: vec![(vec![WALLETS[0].into()], 1), (vec![WALLETS[1].into()], 2)],
        replay_sha256: "ab".repeat(32),
        salt_commit: "cd".repeat(32),
    };
    elo.report(&r).await.unwrap();
    let (line, _, body) = seen.lock().unwrap().last().cloned().unwrap();
    assert!(line.starts_with("POST /api/ratings/oneshot/report"), "{line}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["matches"][0]["id"], "m-old");
    assert!(v["matches"][0].get("evidence").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cup_plays_its_heats_and_its_final_and_waits_on_a_review() {
    let (origin, seen) = stand_in(false).await;
    let dir: PathBuf = std::env::temp_dir().join(format!("oneshot-cup-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let key_path = dir.join("server.pem");
    std::fs::write(&key_path, pem(&SERVER_SEED)).unwrap();
    let account = Ed25519KeyPair::from_seed_unchecked(&ACCOUNT_SEED).unwrap();
    let h = server::run(server::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        http: Some("127.0.0.1:0".parse().unwrap()),
        cup: Rules {
            time_limit_ms: 3000,
            ..Rules::ffa()
        },
        cup_min: 4,
        cup_fill: Duration::ZERO,
        cup_rounds: 1,
        cup_review_top: 2,
        linger: Duration::from_millis(500),
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
    let wallets = [
        WALLETS[0].to_string(),
        WALLETS[1].to_string(),
        "0x4444444444444444444444444444444444444444".to_string(),
        "0x5555555555555555555555555555555555555555".to_string(),
    ];
    let mut passes = Vec::new();
    for w in &wallets {
        passes.push(pass(Some(w), &nonce(http).await));
    }
    let (a, b, c, d) = tokio::join!(
        play(&h, &passes[0], MODE_CUP),
        play(&h, &passes[1], MODE_CUP),
        play(&h, &passes[2], MODE_CUP),
        play(&h, &passes[3], MODE_CUP),
    );
    let got = [a, b, c, d];
    for g in &got {
        let welcomes = g.iter().filter(|m| matches!(m, Msg::Welcome { .. })).count();
        assert_eq!(welcomes, 2, "a Swiss round and the final: {g:?}");
        let cups: Vec<&Msg> = g.iter().filter(|m| matches!(m, Msg::Cup { .. })).collect();
        assert!(
            matches!(
                cups.first(),
                Some(Msg::Cup {
                    round: 1,
                    rounds: 1,
                    entrants: 4,
                    done: false,
                    ..
                })
            ),
            "{cups:?}"
        );
        // Nobody fired: every heat a four-way tie for first.
        assert!(
            matches!(
                cups.last(),
                Some(Msg::Cup {
                    place: 1,
                    points: 10,
                    done: true,
                    ..
                })
            ),
            "{cups:?}"
        );
        assert_eq!(g.last(), Some(&Msg::Bye));
    }
    // Two heats reported on the cup ladder, each with all four placed.
    let mut reports = Vec::new();
    for _ in 0..50 {
        reports = seen
            .lock()
            .unwrap()
            .iter()
            .filter(|(l, _, b)| l.starts_with("POST /api/ratings/") && b.contains("\"cup\""))
            .map(|(_, _, b)| serde_json::from_str::<serde_json::Value>(b).unwrap())
            .collect();
        if reports.len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(reports.len(), 2);
    for r in &reports {
        assert_eq!(r["matches"][0]["teams"].as_array().unwrap().len(), 4);
        assert!(r["matches"][0]["evidence"]["replay_sha256"].is_string());
    }
    // The cup's file, and its top two waiting on a review before a prize.
    let mut cup = None;
    for _ in 0..50 {
        cup = std::fs::read_dir(dir.join("cups"))
            .ok()
            .and_then(|mut d| d.next())
            .and_then(|e| e.ok());
        if cup.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let cup_path = cup.expect("the cup's file").path();
    let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&cup_path).unwrap()).unwrap();
    assert_eq!(doc["standings"].as_array().unwrap().len(), 4);
    assert_eq!(doc["heats"].as_array().unwrap().len(), 2);
    let id = doc["cup"].as_str().unwrap();
    let mut review = None;
    for _ in 0..50 {
        review = std::fs::read_to_string(dir.join("review").join(format!("{id}.json"))).ok();
        if review.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let review: serde_json::Value = serde_json::from_str(&review.expect("a prize review")).unwrap();
    assert_eq!(review["kind"], "cup prize");
    // A four-way tie for first: all four are in the top two places.
    assert_eq!(review["seats"].as_array().unwrap().len(), 4);
    for s in review["seats"].as_array().unwrap() {
        assert_eq!(s["replays"].as_array().unwrap().len(), 2);
    }
    for heat in doc["heats"].as_array().unwrap() {
        let bytes = std::fs::read(dir.join(heat["replay"].as_str().unwrap())).expect("the heat's replay");
        room::replay::verify(&bytes).expect("replays");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

//! Parties over real QUIC: a page's lobby session gets a party of its own
//! and a ticket; a second page joins with the code; the party's voice
//! reaches the party and nobody else; the leader starts and both land in
//! one room, on one side; a party alone gets a room of its own; and a page
//! back from a match takes its place again with its ticket.

use std::time::Duration;

use client_net::native::NativeLobby;
use client_net::session::Session;
use proto::frame::LEN_PREFIX_BYTES;
use proto::party::{self as pp, Party};
use proto::voice::VoiceFrame;
use proto::{decode_msg, frame_msg, Msg, Pass, MAX_STREAM_MSG};
use wtransport::tls::Sha256Digest;
use wtransport::{ClientConfig, Endpoint};

const WAIT: Duration = Duration::from_secs(5);

async fn lobby(h: &server::Handle, name: &str, pass: &str, party: u32) -> NativeLobby {
    let addr = format!("127.0.0.1:{}", h.addr.port());
    let pass = if pass.is_empty() {
        Pass::NONE
    } else {
        Pass::new(pass).unwrap()
    };
    let hello = Session::hello(name, pass, proto::MODE_LOBBY, proto::Look::default(), party);
    NativeLobby::connect(&addr, Some(h.cert_hash), hello).await.unwrap()
}

/// Reads until `want` picks something out, or [`WAIT`] passes.
async fn until<T>(l: &NativeLobby, mut want: impl FnMut(&Msg) -> Option<T>) -> T {
    let t0 = std::time::Instant::now();
    while t0.elapsed() < WAIT {
        if let Some(m) = l.next(WAIT).await {
            if let Some(t) = want(&m) {
                return t;
            }
        }
    }
    panic!("not told in time");
}

async fn ticket(l: &NativeLobby) -> String {
    until(l, |m| match m {
        Msg::Ticket { ticket } => Some(ticket.as_str().to_string()),
        _ => None,
    })
    .await
}

async fn party(l: &NativeLobby, n: u8) -> Party {
    until(l, |m| match m {
        Msg::Party(p) if p.n == n => Some(*p),
        _ => None,
    })
    .await
}

async fn read_frame(r: &mut wtransport::RecvStream) -> Option<Msg> {
    let mut len = [0u8; LEN_PREFIX_BYTES];
    r.read_exact(&mut len).await.ok()?;
    let mut body = vec![0u8; u16::from_le_bytes(len) as usize];
    r.read_exact(&mut body).await.ok()?;
    decode_msg(&body).ok()
}

/// A match session with `ticket` and party `code`: its welcome's salt
/// commitment (the room) and team. The connection is kept by the caller.
async fn play(
    h: &server::Handle,
    ticket: &str,
    mode: u8,
    code: u32,
) -> (
    [u8; 32],
    u8,
    wtransport::Connection,
    Endpoint<wtransport::endpoint::endpoint_side::Client>,
) {
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
    let pass = if ticket.is_empty() {
        Pass::NONE
    } else {
        Pass::new(ticket).unwrap()
    };
    let n = frame_msg(&Session::hello("p", pass, mode, proto::Look::default(), code), &mut f).unwrap();
    send.write_all(&f[..n]).await.unwrap();
    loop {
        match tokio::time::timeout(WAIT, read_frame(&mut recv)).await {
            Ok(Some(Msg::Welcome { salt_commit, team, .. })) => {
                std::mem::forget(send);
                return (salt_commit, team, conn, ep);
            }
            Ok(Some(_)) => {}
            other => panic!("no welcome: {other:?}"),
        }
    }
}

/// A match session's welcome: its map and rules.
async fn play_rules(h: &server::Handle, ticket: &str, mode: u8, code: u32) -> (u8, sim::world::Rules) {
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
    let pass = Pass::new(ticket).unwrap();
    let n = frame_msg(&Session::hello("p", pass, mode, proto::Look::default(), code), &mut f).unwrap();
    send.write_all(&f[..n]).await.unwrap();
    loop {
        match tokio::time::timeout(WAIT, read_frame(&mut recv)).await {
            Ok(Some(Msg::Welcome { map, rules, .. })) => return (map, rules),
            Ok(Some(_)) => {}
            other => panic!("no welcome: {other:?}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_party_talks_lands_together_and_survives_a_reload() {
    let h = server::run(server::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        http: None,
        ..server::Config::default()
    })
    .await
    .unwrap();

    // Two pages: the first leads a party of one; the second comes in with
    // its code (a new party takes anyone with its link).
    let a = lobby(&h, "alpha", "", 0).await;
    let ta = ticket(&a).await;
    let pa = party(&a, 1).await;
    assert_eq!((pa.leader, pa.me), (pa.members[0].tag, pa.members[0].tag));
    let b = lobby(&h, "bravo", "", pa.code).await;
    let tb = ticket(&b).await;
    let pb = party(&b, 2).await;
    assert_eq!(pb.code, pa.code);
    let pa = party(&a, 2).await;
    assert_eq!(pa.members[1].name.as_str(), "bravo");
    let c = lobby(&h, "charlie", "", 0).await;
    let pc = party(&c, 1).await;
    assert_ne!(pc.code, pa.code);
    // A guest owns nothing: the armory says so.
    let worn = until(&c, |m| match m {
        Msg::Wardrobe(d) => Some(*d),
        _ => None,
    })
    .await;
    assert_eq!(worn, proto::Wardrobe::BARE);

    // Voice: the party hears it, under the talker's tag; nobody else does,
    // and a packet that isn't one 20 ms frame goes nowhere.
    let frame = VoiceFrame::new(7, 0, &[9 << 3, 1, 2, 3]).unwrap();
    assert!(a.send_voice(&frame));
    let heard = b.voice(WAIT).await.expect("the party hears it");
    assert_eq!(
        (heard.from, heard.seq, heard.payload()),
        (pa.me, 7, &[9 << 3, 1, 2, 3][..])
    );
    assert!(c.voice(Duration::from_millis(500)).await.is_none(), "not another party");
    assert!(a.send_voice(&VoiceFrame::new(8, 0, &[(9 << 3) | 3, 1]).unwrap()));
    assert!(b.voice(Duration::from_millis(500)).await.is_none(), "not one frame");

    // A member can't start; the leader starts Big Team for both.
    b.send(&Msg::PartyAct {
        act: pp::ACT_START,
        arg: proto::MODE_BIG_TEAM as u32,
    })
    .await
    .unwrap();
    let why = until(&b, |m| match m {
        Msg::Refuse { reason } => Some(*reason),
        _ => None,
    })
    .await;
    assert_eq!(why, proto::REFUSE_NOT_LEADER);
    a.send(&Msg::PartyAct {
        act: pp::ACT_START,
        arg: proto::MODE_BIG_TEAM as u32,
    })
    .await
    .unwrap();
    let go = |m: &Msg| matches!(m, Msg::Go { .. }).then_some(*m);
    assert_eq!(
        until(&b, go).await,
        Msg::Go {
            mode: proto::MODE_BIG_TEAM,
            private: false
        }
    );
    until(&a, go).await;
    let (room_a, team_a, _ca, _ea) = play(&h, &ta, proto::MODE_BIG_TEAM, pa.code).await;
    let (room_b, team_b, _cb, _eb) = play(&h, &tb, proto::MODE_BIG_TEAM, pa.code).await;
    assert_eq!(room_a, room_b, "one room");
    assert_eq!(team_a, team_b, "one side");

    // Alone: the party's own room, which a stranger isn't put in.
    a.send(&Msg::PartyAct {
        act: pp::ACT_START,
        arg: proto::MODE_FFA as u32 | pp::START_PRIVATE,
    })
    .await
    .unwrap();
    until(&b, go).await;
    let (own_a, _, _c2a, _e2a) = play(&h, &ta, proto::MODE_FFA, pa.code).await;
    let (own_b, _, _c2b, _e2b) = play(&h, &tb, proto::MODE_FFA, pa.code).await;
    assert_eq!(own_a, own_b);
    let (stranger, _, _c3, _e3) = play(&h, "", proto::MODE_FFA, 0).await;
    assert_ne!(stranger, own_a, "a stranger plays elsewhere");

    // Alone, a match of the party's own: its map, score and clock.
    let custom = pp::Custom {
        map: Some(sim::maps::bulkhead::DEF.id),
        score: 1,
        time: 1,
    };
    a.send(&Msg::PartyAct {
        act: pp::ACT_START,
        arg: proto::MODE_CTF as u32 | pp::START_PRIVATE | custom.to_arg(),
    })
    .await
    .unwrap();
    until(&b, go).await;
    let (map, rules) = play_rules(&h, &ta, proto::MODE_CTF, pa.code).await;
    assert_eq!(map, sim::maps::bulkhead::DEF.id);
    assert!(rules.ctf);
    assert_eq!(rules.score_limit, 1);
    assert_eq!(rules.time_limit_ms, 5 * 60_000);

    // The second page reloads (after its match): away, then back in its
    // place on its ticket, with no code at all.
    b.close();
    let p = until(&a, |m| match m {
        Msg::Party(p) if p.members().iter().any(|m| m.away) => Some(*p),
        _ => None,
    })
    .await;
    assert_eq!(p.n, 2, "the place is kept");
    let b2 = lobby(&h, "bravo", &tb, 0).await;
    assert!(!ticket(&b2).await.is_empty());
    let p = party(&b2, 2).await;
    assert_eq!(p.code, pa.code);
    assert!(p.members().iter().all(|m| !m.away));
}

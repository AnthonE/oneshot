//! The real server over real QUIC on loopback, played by a native client
//! running the browser's netcode core: it gets in, snapshots flow at the
//! snapshot rate, the prediction confirms, and the bot plays.

use std::time::{Duration, Instant};

use client_net::native::NativeClient;
use client_net::session::Session;
use proto::frame::LEN_PREFIX_BYTES;
use proto::{decode_msg, frame_msg, Msg, Name, MAX_STREAM_MSG, PROTO_VER};
use sim::input::{Input, FIRE};
use sim::world::Rules;
use wtransport::tls::Sha256Digest;
use wtransport::{ClientConfig, Endpoint};

async fn read_frame(r: &mut wtransport::RecvStream) -> Option<Msg> {
    let mut len = [0u8; LEN_PREFIX_BYTES];
    r.read_exact(&mut len).await.ok()?;
    let mut body = vec![0u8; u16::from_le_bytes(len) as usize];
    r.read_exact(&mut body).await.ok()?;
    decode_msg(&body).ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_native_client_plays_the_bot() {
    let h = server::run(server::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        http: None,
        rules: Rules {
            score_limit: 1000,
            time_limit_ms: 0,
            ..Rules::duel()
        },
        ..server::Config::default()
    })
    .await
    .unwrap();

    // The native client the desktop build will use: client_net's session
    // on wtransport.
    // A skin a guest can't own, and an emblem anyone may wear.
    let star = proto::emblem(1, 1, 3, 4);
    let hello = Session::hello(
        "native",
        proto::Pass::NONE,
        proto::MODE_CASUAL,
        proto::Look { skin: 2, emblem: star },
        0,
    );
    let addr = format!("127.0.0.1:{}", h.addr.port());
    let mut c = NativeClient::connect(&addr, Some(h.cert_hash), hello).await.unwrap();

    let t0 = Instant::now();
    let mut frame = tokio::time::interval(Duration::from_millis(8));
    let mut bot_moved = false;
    let mut bot_start = None;
    let mut seats = 0;
    let mut starred = false;
    while t0.elapsed() < Duration::from_secs(8) {
        frame.tick().await;
        let now = c.now_ms();
        c.update(Input {
            buttons: if (now as u32 / 500).is_multiple_of(2) { FIRE } else { 0 },
            yaw: (now * 10.0) as u32 as u16,
            mz: 127,
            ..Input::default()
        });
        for m in c.messages() {
            if let Msg::Seat { skin, emblem, bot, .. } = m {
                // A guest owns nothing: the skin asked for is not worn; the
                // free emblem is, and a bot wears none.
                assert_eq!(skin, proto::SKIN_STANDARD);
                assert_eq!(emblem, if bot { proto::EMBLEM_NONE } else { star });
                starred |= !bot;
                seats += 1;
            }
        }
        if let Some(b) = c.session.core.latest().remotes().first() {
            let start = *bot_start.get_or_insert(b.pos);
            bot_moved |= (b.pos - start).len() > 1.0;
        }
    }
    assert!(c.session.ready, "welcomed");
    assert!(seats >= 2, "told who sits where");
    assert!(starred, "and the guest's emblem is worn");
    let s = c.session.core.stats;
    eprintln!("{s:?}");
    // 8 s at 64 a second, less the first moments.
    assert!(s.snapshots > 400, "{s:?}");
    let ratio = s.confirmations as f64 / (s.confirmations + s.corrections).max(1) as f64;
    assert!(ratio > 0.9, "{ratio}: {s:?}");
    assert!(bot_moved, "the bot plays");
    assert!(c.session.core.own().0.seated);
}

/// Says hello in `mode` and returns the welcome and the open streams.
async fn hello(
    h: &server::Handle,
    mode: u8,
    name: &str,
) -> (
    Msg,
    wtransport::SendStream,
    wtransport::RecvStream,
    wtransport::Connection,
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
    let n = frame_msg(
        &Msg::Hello {
            proto: PROTO_VER,
            name: Name::new(name),
            pass: proto::Pass::NONE,
            mode,
            skin: 0,
            emblem: 0,
            party: 0,
            stamp: sim::STAMP,
            queued: proto::QueueMatch::NONE,
        },
        &mut f,
    )
    .unwrap();
    send.write_all(&f[..n]).await.unwrap();
    let w = read_frame(&mut recv).await.expect("a welcome");
    (w, send, recv, conn)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gun_game_is_shared_and_bots_fill_the_rest() {
    let h = server::run(server::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        http: None,
        ..server::Config::default()
    })
    .await
    .unwrap();
    let (a, _sa, mut ra, _ca) = hello(&h, proto::MODE_GUN_GAME, "one").await;
    let (b, _sb, mut rb, _cb) = hello(&h, proto::MODE_GUN_GAME, "two").await;
    let (
        Msg::Welcome {
            slot: s0,
            rules: r0,
            salt_commit: c0,
            ..
        },
        Msg::Welcome {
            slot: s1,
            salt_commit: c1,
            ..
        },
    ) = (a, b)
    else {
        panic!("not welcomed")
    };
    assert!(r0.gun_game && r0.seats == 8, "{r0:?}");
    assert_eq!(c0, c1, "one room");
    assert_ne!(s0, s1);
    // The first player hears of the second, and both see seven others.
    let mut heard = false;
    let mut people = std::collections::BTreeSet::new();
    for _ in 0..40 {
        match tokio::time::timeout(Duration::from_secs(2), read_frame(&mut ra)).await {
            Ok(Some(Msg::Seat {
                slot, bot: false, name, ..
            })) => {
                people.insert(slot);
                heard |= name.as_str() == "two" && slot == s1;
            }
            Ok(Some(_)) => {}
            _ => break,
        }
        if heard {
            break;
        }
    }
    assert!(heard, "seat {s1} named to the first player: {people:?}");
    let mut seats = 0;
    while seats < 8 {
        if let Some(Msg::Seat { .. }) = read_frame(&mut rb).await {
            seats += 1;
        }
    }
    // A casual duel against the bot is still a room of its own.
    let (c, _sc, _rc, _cc) = hello(&h, proto::MODE_CASUAL, "solo").await;
    let Msg::Welcome { salt_commit, rules, .. } = c else {
        panic!("not welcomed")
    };
    assert_ne!(salt_commit, c0);
    assert_eq!(rules.seats, 2);
}

/// Watching: a page plays free for all; a watcher asks for any public match
/// and gets that one, as its player saw it, a casual match's delay late;
/// a click follows the next seat; nobody watches a match they sit in.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_watcher_sees_a_live_match_late_and_follows_the_next_player() {
    let h = server::run(server::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        http: None,
        ..server::Config::default()
    })
    .await
    .unwrap();
    let addr = format!("127.0.0.1:{}", h.addr.port());
    let look = proto::Look::default();
    // Nothing to watch yet.
    let hello_watch = || Session::hello("eyes", proto::Pass::NONE, proto::MODE_WATCH, look, 0);
    let mut early = NativeClient::connect(&addr, Some(h.cert_hash), hello_watch())
        .await
        .unwrap();
    let t0 = Instant::now();
    let mut refused = false;
    while t0.elapsed() < Duration::from_secs(3) && !refused {
        tokio::time::sleep(Duration::from_millis(20)).await;
        early.update(Input::default());
        refused = early.messages().iter().any(|m| {
            matches!(
                m,
                Msg::Refuse {
                    reason: proto::REFUSE_NOTHING_TO_WATCH
                }
            )
        });
    }
    assert!(refused, "nothing to watch is said so");

    let play = Session::hello("player", proto::Pass::NONE, proto::MODE_FFA, look, 0);
    let mut p = NativeClient::connect(&addr, Some(h.cert_hash), play).await.unwrap();
    let mut w = NativeClient::connect(&addr, Some(h.cert_hash), hello_watch())
        .await
        .unwrap();
    assert!(w.session.watch, "a watch hello opens a watching session");
    let delay = room::spectate::DELAY_CASUAL_MS as f64;
    let t0 = Instant::now();
    let mut frame = tokio::time::interval(Duration::from_millis(8));
    let (mut first_snap_ms, mut follows, mut asked) = (None, Vec::new(), false);
    let mut names = 0;
    while t0.elapsed() < Duration::from_millis(delay as u64 + 6000) {
        frame.tick().await;
        p.update(Input {
            mz: 127,
            ..Input::default()
        });
        p.messages();
        w.update(Input::default());
        for m in w.messages() {
            match m {
                Msg::Follow { slot, .. } => follows.push(slot),
                Msg::Seat { .. } => names += 1,
                Msg::Refuse { reason } => panic!("refused: {reason}"),
                _ => {}
            }
        }
        if w.session.core.stats.snapshots > 0 && first_snap_ms.is_none() {
            first_snap_ms = Some(t0.elapsed().as_secs_f64() * 1000.0);
        }
        // Shown for a while: follow the next player.
        if !asked && first_snap_ms.is_some_and(|t| t0.elapsed().as_secs_f64() * 1000.0 > t + 2000.0) {
            w.session.follow_next();
            asked = true;
        }
    }
    assert!(w.session.ready, "welcomed");
    assert!(names >= 8, "every seat named: {names}");
    let first = first_snap_ms.expect("snapshots came");
    assert!(
        first >= delay - 500.0,
        "the first snapshot came {first} ms in, sooner than the delay"
    );
    assert!(follows.len() >= 2, "followed one seat, then the next: {follows:?}");
    assert_ne!(follows[0], follows[1]);
    assert_eq!(w.session.core.slot, follows[1]);
    // Its own fighter is the one it follows, seated and drawn.
    assert!(w.session.core.own().0.seated);
}

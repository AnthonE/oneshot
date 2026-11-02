//! The real server over real QUIC on loopback, played by a native client
//! running the browser's netcode core: it gets in, snapshots flow at the
//! snapshot rate, the prediction confirms, and the bot plays.

use std::time::{Duration, Instant};

use client_net::core::NetCore;
use proto::frame::LEN_PREFIX_BYTES;
use proto::{decode_msg, decode_snapshot, encode_input, frame_msg, Msg, Name, MAX_DATAGRAM, MAX_STREAM_MSG, PROTO_VER};
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
            name: Name::new("native"),
            pass: proto::Pass::NONE,
            mode: proto::MODE_CASUAL,
        },
        &mut f,
    )
    .unwrap();
    send.write_all(&f[..n]).await.unwrap();
    let Some(Msg::Welcome {
        slot,
        team,
        tick,
        rules,
        map,
        ..
    }) = read_frame(&mut recv).await
    else {
        panic!("no welcome");
    };
    let mut seats = 0;
    while seats < 2 {
        if let Some(Msg::Seat { .. }) = read_frame(&mut recv).await {
            seats += 1;
        }
    }

    let mut core = NetCore::new(slot, team, tick, rules, sim::map::by_id(map).expect("a map we know"));
    let (dtx, mut drx) = tokio::sync::mpsc::unbounded_channel();
    let rconn = conn.clone();
    tokio::spawn(async move {
        while let Ok(d) = rconn.receive_datagram().await {
            let _ = dtx.send((Instant::now(), d.to_vec()));
        }
    });

    let t0 = Instant::now();
    let ms = |at: Instant| at.duration_since(t0).as_secs_f64() * 1000.0;
    let mut frame = tokio::time::interval(Duration::from_millis(8));
    let mut buf = [0u8; MAX_DATAGRAM];
    let mut bot_moved = false;
    let mut bot_start = None;
    while t0.elapsed() < Duration::from_secs(8) {
        frame.tick().await;
        while let Ok((at, d)) = drx.try_recv() {
            let s = decode_snapshot(&d).unwrap();
            core.on_snapshot(ms(at), &s);
            if let Some(b) = s.remotes().first() {
                let start = *bot_start.get_or_insert(b.pos);
                bot_moved |= (b.pos - start).len() > 1.0;
            }
        }
        let now = ms(Instant::now());
        let input = Input {
            buttons: if (now as u32 / 500).is_multiple_of(2) { FIRE } else { 0 },
            yaw: (now * 10.0) as u32 as u16,
            mz: 127,
            ..Input::default()
        };
        for m in core.update(now, input) {
            let n = encode_input(&m, &mut buf).unwrap();
            let _ = conn.send_datagram(&buf[..n]);
        }
    }
    let s = core.stats;
    eprintln!("{s:?}");
    // 8 s at 64 a second, less the first moments.
    assert!(s.snapshots > 400, "{s:?}");
    let ratio = s.confirmations as f64 / (s.confirmations + s.corrections).max(1) as f64;
    assert!(ratio > 0.9, "{ratio}: {s:?}");
    assert!(bot_moved, "the bot plays");
    assert!(core.own().0.seated);
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

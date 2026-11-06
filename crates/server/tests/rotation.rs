//! Picking the next map, over real QUIC on loopback: a group that plays
//! again is put on another map while the rotation has one (a playlist's
//! rule since Halo 2's), however its last match ended; a stranger is
//! not steered by someone else's history.

use std::time::Duration;

use proto::frame::LEN_PREFIX_BYTES;
use proto::{decode_msg, frame_msg, Msg, Name, MAX_STREAM_MSG, PROTO_VER};
use wtransport::tls::Sha256Digest;
use wtransport::{ClientConfig, Endpoint};

async fn read_frame(r: &mut wtransport::RecvStream) -> Option<Msg> {
    let mut len = [0u8; LEN_PREFIX_BYTES];
    r.read_exact(&mut len).await.ok()?;
    let mut body = vec![0u8; u16::from_le_bytes(len) as usize];
    r.read_exact(&mut body).await.ok()?;
    decode_msg(&body).ok()
}

/// Says hello for a casual match with party code `party`, and returns the
/// map the welcome names; the connection is dropped after (a player who
/// leaves at once: the map still counts as theirs).
async fn play(port: u16, hash: [u8; 32], party: u32) -> u8 {
    let cfg = ClientConfig::builder()
        .with_bind_address("127.0.0.1:0".parse().unwrap())
        .with_server_certificate_hashes([Sha256Digest::from(hash)])
        .build();
    let ep = Endpoint::client(cfg).unwrap();
    let conn = ep.connect(format!("https://127.0.0.1:{port}/")).await.unwrap();
    let (mut send, mut recv) = conn.open_bi().await.unwrap().await.unwrap();
    let mut f = [0u8; MAX_STREAM_MSG + LEN_PREFIX_BYTES];
    let n = frame_msg(
        &Msg::Hello {
            proto: PROTO_VER,
            name: Name::new("again"),
            pass: proto::Pass::NONE,
            mode: proto::MODE_CASUAL,
            skin: 0,
            emblem: 0,
            party,
            stamp: sim::STAMP,
        },
        &mut f,
    )
    .unwrap();
    send.write_all(&f[..n]).await.unwrap();
    match tokio::time::timeout(Duration::from_secs(5), read_frame(&mut recv)).await {
        Ok(Some(Msg::Welcome { map, .. })) => map,
        other => panic!("no welcome: {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_group_playing_again_is_put_on_another_map() {
    let h = server::run(server::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        http: None,
        maps: vec![0, 7],
        ..server::Config::default()
    })
    .await
    .unwrap();
    let (port, hash) = (h.addr.port(), h.cert_hash);
    // One party, match after match: never the same map twice running.
    let party = 0x0123_4567 & ((1 << proto::party::CODE_BITS) - 1);
    let mut last = play(port, hash, party).await;
    for _ in 0..6 {
        let next = play(port, hash, party).await;
        assert_ne!(next, last, "the same map twice running");
        assert!([0, 7].contains(&next));
        last = next;
    }
    // Someone else's history doesn't steer a stranger: over a few, both
    // maps come up.
    let mut seen = [false; 2];
    for k in 0..24u32 {
        let m = play(port, hash, 0x100 + k).await;
        seen[usize::from(m == 7)] = true;
    }
    assert!(seen[0] && seen[1], "strangers get both maps: {seen:?}");
}

//! Many players at once in rooms stepped side by side (`Config::sim_threads`),
//! by the load test's own players (`tools/load`): everyone gets in and is
//! fed, matches end and players go again, and every match's replay re-runs
//! to the server's hash, as it would have stepped alone.

use std::time::Duration;

use sim::world::Rules;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rooms_step_side_by_side_and_every_replay_holds() {
    let dir = std::env::temp_dir().join(format!("oneshot-load-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // Matches of three seconds after the countdown.
    let short = |r: Rules| Rules {
        time_limit_ms: 3000,
        ..r
    };
    let h = server::run(server::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        http: Some("127.0.0.1:0".parse().unwrap()),
        rules: short(Rules::duel()),
        ffa: short(Rules::ffa()),
        linger: Duration::from_millis(500),
        replays: Some(dir.clone()),
        sim_threads: 3,
        ..server::Config::default()
    })
    .await
    .unwrap();
    // Eight rooms against a bot and a free for all of four.
    let report = load::run(load::Opts {
        addr: format!("127.0.0.1:{}", h.addr.port()),
        pin: Some(h.cert_hash),
        health: Some(format!("http://127.0.0.1:{}/health.json", h.http.unwrap().port())),
        players: 12,
        modes: vec![proto::MODE_CASUAL, proto::MODE_CASUAL, proto::MODE_FFA],
        ramp: Duration::from_secs(1),
        hold: Duration::from_secs(9),
        fps: 60,
        every: None,
        lobby: true,
    })
    .await;
    eprintln!("{report}");
    h.drain(Duration::from_secs(5)).await;

    let w = report.server.as_ref().expect("its health read");
    assert_eq!(w.sim_threads, 3);
    assert_eq!(report.never_in, 0, "{report}");
    assert_eq!(report.lobby_failed, 0, "{report}");
    assert!(report.byes >= 9, "every match ended: {report}");
    assert!(report.joins > 12, "and its players went again: {report}");
    assert!(report.snaps_p50 > 0.75 * load::SNAPSHOT_HZ, "{report}");
    assert!(report.confirmed > 0.8, "{report}");
    assert_eq!(w.cut_behind, 0, "{report}");

    let replays: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "osr"))
        .collect();
    assert!(replays.len() >= 9, "{} replays", replays.len());
    for p in &replays {
        let bytes = std::fs::read(p).unwrap();
        if let Err(e) = room::replay::verify(&bytes) {
            panic!("{} does not replay: {e}", p.display());
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

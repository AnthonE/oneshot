//! `oneshot-load [--page http://127.0.0.1:8080 | --addr host:port [--pin hex]
//! [--health url]] [--players 64] [--mode big[,ffa,gun,ctf,bot]]
//! [--ramp 10] [--hold 60] [--fps 60] [--every 5]`
//!
//! Many headless players against a server (`load`'s docs): they join
//! over `--ramp` seconds, play `--hold` more, and the report says whether
//! the server kept up. `--page` is where a page would ask (`server.json`
//! names the game and the pin; `/health.json` is read beside it). Exits 1
//! if it didn't keep up, 2 on a bad argument.
//!
//! Loopback is held to no address's share; from another box, start the
//! server with `--load-from <this box's ip>`.

use std::time::Duration;

fn main() {
    let mut o = load::Opts::default();
    let mut page = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| die(&format!("{a} needs a value")));
        let secs = |v: String| Duration::from_secs_f64(v.parse().unwrap_or_else(|_| die(&format!("{v}: seconds"))));
        match a.as_str() {
            "--page" => page = Some(val().trim_end_matches('/').to_string()),
            "--addr" => o.addr = val(),
            "--pin" => o.pin = Some(load::hex32(&val()).unwrap_or_else(|| die("--pin is 64 hex"))),
            "--health" => o.health = Some(val()),
            "--players" => o.players = val().parse().unwrap_or_else(|_| die("--players is a count")),
            "--mode" => {
                o.modes = val()
                    .split(',')
                    .map(|m| load::mode(m).unwrap_or_else(|| die(&format!("no mode {m}: bot, ffa, gun, big, ctf"))))
                    .collect()
            }
            "--ramp" => o.ramp = secs(val()),
            "--hold" => o.hold = secs(val()),
            "--fps" => o.fps = val().parse().unwrap_or_else(|_| die("--fps is a count")),
            "--every" => {
                let s = secs(val());
                o.every = (!s.is_zero()).then_some(s);
            }
            _ => die(&format!("unknown argument {a}")),
        }
    }
    let rt = tokio::runtime::Runtime::new().unwrap_or_else(|e| die(&e.to_string()));
    let report = rt.block_on(async {
        if let Some(p) = &page {
            let http = load::http_client();
            let doc = load::get(&http, &format!("{p}/server.json"))
                .await
                .unwrap_or_else(|e| die(&e));
            let (addr, pin) = load::server_json(&doc).unwrap_or_else(|e| die(&e));
            o.addr = addr;
            o.pin = o.pin.or(pin);
            o.health = o.health.take().or_else(|| Some(format!("{p}/health.json")));
        }
        eprintln!(
            "oneshot-load: {} players on {} ({}), {:?} ramp, {:?} hold",
            o.players,
            o.addr,
            if o.pin.is_some() { "pinned" } else { "system roots" },
            o.ramp,
            o.hold
        );
        load::run(o).await
    });
    println!("{report}");
    let problems = report.problems();
    if problems.is_empty() {
        println!("kept up");
    } else {
        for p in &problems {
            println!("did not keep up: {p}");
        }
        std::process::exit(1);
    }
}

fn die(msg: &str) -> ! {
    eprintln!("oneshot-load: {msg}");
    std::process::exit(2)
}

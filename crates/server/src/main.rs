//! `oneshot-server [--bind 0.0.0.0:4433] [--http 0.0.0.0:8080|off]
//! [--web crates/client/dist] [--cert cert.pem --key key.pem] [--doubles]
//! [--passive-bots] [--dress-all] [--map <id|name>]… [--loadout <gun>[,<gun>]]
//! [--cup-min 4] [--cup-rounds 3]
//! [--slug oneshot] [--domain host]…
//! [--elo-pubkey <base64>] [--elo-origin https://elopros.com]
//! [--elo-key server.pem --elo-key-id eu-1] [--replays dir] [--bugs dir]
//! [--public-addr host:port] [--allow-origin https://elopros.com]…
//! [--capacity 256] [--drain-max 900] [--big-snapshot-hz 64] [--no-delta] [--no-far-rates]
//! [--nonce-key file] [--live-file file] [--roster-dir dir] [--roster-key file]
//! [--sim-threads 0] [--endpoints 1] [--endpoint-step 10] [--net-nice 5] [--elo-queue] [--load-from ip]… [--match-secs N]`
//!
//! `--match-secs` puts every mode's match on an N-second clock: for tests
//! that play one match after another (`ci/e2e.mjs`'s `E2E_AGAIN`).
//!
//! `--sim-threads` is how many threads the rooms step on: 0 (the default)
//! one a core, 1 the sim thread alone. `--load-from` lets a load test's box
//! (`tools/load`) hold more than an address's share of connections.
//!
//! `--endpoints N` listens on N UDP ports, `--bind`'s and each
//! `--endpoint-step` after (10: slot a on 61250, 61260, 61270, slot b on
//! 61251, 61261, 61271), each its own socket and receive loop; `server.json`
//! hands pages one in turn. Both slots must run the same two flags, and
//! every port must be open in the firewalls. `--elo-queue` queues ranked
//! players in Elo Pros' queue, shared by every server in the game's list,
//! which puts each match on one with room; a player matched onto another
//! server is moved there (`Msg::Moved`). Every server then needs the same
//! `--nonce-key`, so a ticket from one is good on the rest. `--net-nice` is how far below
//! the sim the network's threads run (Linux's nice, 0 for the same).
//!
//! `--capacity` is in players (a player's lobby beside its match isn't
//! another). `--big-snapshot-hz` is the snapshot rate on a big map (Big
//! Team, and Capture the Flag there): 64 as everywhere by default, 32 to
//! halve a box's packets; the arenas and ranked always get 64. `--no-delta`
//! sends every snapshot whole rather than as what changed since the one
//! the client acked (`proto::delta`). `--no-far-rates` updates everyone in
//! every snapshot on a big map too (by default one past 60 m is updated
//! every other, past 120 m every fourth: `room::update_every`).
//!
//! `--loadout` starts everyone (gun game aside) with those guns, by id or
//! name (`--loadout sniper,magnum`): for looking at a gun in a match.
//!
//! `oneshot-server --print-stats` prints the stats, achievements and boards
//! the game declares on Elo Pros (`ops/elo-stats.json`) and exits.
//!
//! Without `--elo-pubkey` nobody can sign in and only casual play is open.
//! Without `--elo-key` ranked results aren't reported.
//!
//! Two slots run side by side on the game box (`ops/oneshot@.service`): they
//! share `--nonce-key` (32 bytes, or 64 hex) so either takes the other's
//! nonces, and `--live-file` (`host:port`), which every `server.json` names.
//!
//! Signals: SIGTERM drains (no new players, every match plays out, at most
//! `--drain-max` seconds) and exits; SIGHUP reads the certificate again (an
//! ACME renewal); SIGINT exits at once, a drain too (its matches cut; the
//! reports still being sent are saved for `oneshot-review resend`).

use std::net::SocketAddr;

use base64::Engine;
use sim::world::Rules;

fn main() {
    let mut cfg = server::Config::default();
    let mut cert = None;
    let mut key = None;
    let mut elo_key = None;
    let mut elo_key_id = None;
    let mut domains = Vec::new();
    let mut drain_max = std::time::Duration::from_secs(900);
    let mut match_secs: Option<u32> = None;
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
            "--dress-all" => cfg.dress_all = true,
            "--map" => {
                let v = val();
                let id = sim::maps::ALL
                    .iter()
                    .find(|d| {
                        // "The Range" answers to "range" too.
                        let bare = d.name.strip_prefix("The ").unwrap_or(d.name);
                        d.id.to_string() == v || d.name.eq_ignore_ascii_case(&v) || bare.eq_ignore_ascii_case(&v)
                    })
                    .unwrap_or_else(|| die(&format!("no map {v}")))
                    .id;
                cfg.maps.push(id);
            }
            "--loadout" => {
                let v = val();
                let mut l = [sim::arms::NONE; 2];
                for (i, g) in v.split(',').take(2).enumerate() {
                    l[i] = gun(g).unwrap_or_else(|| die(&format!("no gun {g}")));
                }
                for r in [
                    &mut cfg.rules,
                    &mut cfg.doubles,
                    &mut cfg.ffa,
                    &mut cfg.big_team,
                    &mut cfg.ctf,
                    &mut cfg.siege,
                    &mut cfg.duel,
                    &mut cfg.cup,
                ] {
                    r.loadout = l;
                }
            }
            "--cup-min" => cfg.cup_min = val().parse().unwrap_or_else(|_| die("--cup-min is a count")),
            "--cup-rounds" => cfg.cup_rounds = val().parse().unwrap_or_else(|_| die("--cup-rounds is a count")),
            "--slug" => cfg.slug = val(),
            "--domain" => domains.push(val()),
            "--elo-pubkey" => {
                let b = base64::engine::general_purpose::STANDARD
                    .decode(val().trim())
                    .unwrap_or_else(|_| die("--elo-pubkey is standard base64"));
                cfg.elo_pubkey = Some(b.try_into().unwrap_or_else(|_| die("--elo-pubkey is 32 bytes")));
            }
            "--elo-origin" => cfg.elo_origin = val(),
            "--elo-key" => elo_key = Some(val()),
            "--elo-key-id" => elo_key_id = Some(val()),
            "--replays" => cfg.replays = Some(val().into()),
            "--bugs" => cfg.bugs = Some(val().into()),
            "--public-addr" => cfg.public_addr = Some(val()),
            "--allow-origin" => cfg.allow_origins.push(val().trim_end_matches('/').to_string()),
            "--capacity" => cfg.capacity = val().parse().unwrap_or_else(|_| die("--capacity is a count")),
            "--drain-max" => {
                drain_max =
                    std::time::Duration::from_secs(val().parse().unwrap_or_else(|_| die("--drain-max is seconds")))
            }
            "--nonce-key" => cfg.nonce_key = Some(nonce_key(&val())),
            "--live-file" => cfg.live_file = Some(val().into()),
            "--roster-dir" => cfg.roster_dir = Some(val().into()),
            "--roster-key" => {
                // What `/online.json` asks for in `X-Roster-Key`: a line in a file.
                let p = val();
                let k = std::fs::read_to_string(&p).unwrap_or_else(|e| die(&format!("--roster-key {p}: {e}")));
                let k = k.trim().to_string();
                if k.is_empty() || !k.bytes().all(|b| b.is_ascii_graphic()) {
                    die(&format!("--roster-key {p}: one line of printable ASCII"));
                }
                cfg.roster_key = Some(k);
            }
            "--no-delta" => cfg.deltas = false,
            "--no-far-rates" => cfg.far_rates = false,
            "--elo-queue" => cfg.elo_queue = true,
            "--net-nice" => cfg.net_nice = val().parse().unwrap_or_else(|_| die("--net-nice is 0 to 19")),
            "--endpoints" => cfg.endpoints = val().parse().unwrap_or_else(|_| die("--endpoints is a count")),
            "--endpoint-step" => {
                cfg.endpoint_step = val().parse().unwrap_or_else(|_| die("--endpoint-step is a port count"))
            }
            "--big-snapshot-hz" => {
                cfg.big_snapshot_hz = val()
                    .parse()
                    .unwrap_or_else(|_| die("--big-snapshot-hz is 64, 32 or 16"))
            }
            "--sim-threads" => cfg.sim_threads = val().parse().unwrap_or_else(|_| die("--sim-threads is a count")),
            "--load-from" => cfg
                .load_sources
                .push(val().parse().unwrap_or_else(|_| die("--load-from is an IP address"))),
            "--match-secs" => {
                match_secs = Some(val().parse().unwrap_or_else(|_| die("--match-secs: whole seconds")));
            }
            "--print-stats" => {
                let set = server::stats::declaration();
                println!("{}", serde_json::to_string_pretty(&set).unwrap_or_default());
                return;
            }
            _ => die(&format!("unknown argument {a}")),
        }
    }
    if let Some(s) = match_secs {
        let ms = s.max(1) * 1000;
        for r in [
            &mut cfg.rules,
            &mut cfg.duel,
            &mut cfg.doubles,
            &mut cfg.cup,
            &mut cfg.ffa,
            &mut cfg.gun_game,
            &mut cfg.big_team,
            &mut cfg.ctf,
            &mut cfg.siege,
        ] {
            r.time_limit_ms = ms;
        }
    }
    if let (Some(c), Some(k)) = (cert, key) {
        cfg.cert = Some((c, k));
    }
    if !domains.is_empty() {
        cfg.domains = domains;
    }
    match (elo_key, elo_key_id) {
        (Some(k), Some(id)) => cfg.elo_key = Some((k.into(), id)),
        (None, None) => {}
        _ => die("--elo-key and --elo-key-id go together"),
    }
    // The network's threads a little below the sim's: on a busy box the
    // tick takes its cores first and sending fills what's left, rather than
    // a burst of sends holding up every room's tick.
    let nice = cfg.net_nice;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .on_thread_start(move || server::lower_this_thread(nice))
        .build()
        .unwrap_or_else(|e| die(&e.to_string()));
    rt.block_on(async {
        let h = server::run(cfg).await.unwrap_or_else(|e| die(&e));
        // Tests read these lines: keep their shape.
        println!("game: {} (WebTransport)", h.addr);
        for a in &h.addrs[1..] {
            println!("game: {a} (WebTransport, another endpoint)");
        }
        println!("cert sha-256: {}", server::hex(&h.cert_hash));
        println!("sim threads: {}", h.ctx.sim_threads);
        if let Some(http) = h.http {
            println!("play: http://127.0.0.1:{}/", http.port());
        }
        let (recv, send) = h.udp_buffers;
        if recv < server::net::UDP_BUF_BYTES || send < server::net::UDP_BUF_BYTES {
            // Linux reports twice what it keeps; under the ask means
            // `net.core.rmem_max` / `wmem_max` clamped it (ops/README.md).
            eprintln!(
                "oneshot: udp buffers recv {recv} send {send} (asked {})",
                server::net::UDP_BUF_BYTES
            );
        }
        signals(&h, drain_max).await;
    });
}

#[cfg(unix)]
async fn signals(h: &server::Handle, drain_max: std::time::Duration) {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).unwrap_or_else(|e| die(&e.to_string()));
    let mut hup = signal(SignalKind::hangup()).unwrap_or_else(|e| die(&e.to_string()));
    loop {
        tokio::select! {
            _ = term.recv() => {
                eprintln!("oneshot: draining (at most {drain_max:?})");
                // SIGINT still ends it at once (`ci/deploy_server.sh --cut`);
                // another SIGTERM changes nothing. A reload still reloads.
                let drain = h.drain(drain_max);
                tokio::pin!(drain);
                loop {
                    tokio::select! {
                        _ = &mut drain => {
                            eprintln!("oneshot: drained");
                            return;
                        }
                        _ = tokio::signal::ctrl_c() => {
                            eprintln!("oneshot: SIGINT while draining: the matches still up are cut");
                            h.save_unsent();
                            return;
                        }
                        _ = term.recv() => {}
                        _ = hup.recv() => reload(h).await,
                    }
                }
            }
            _ = hup.recv() => reload(h).await,
            _ = tokio::signal::ctrl_c() => {
                h.save_unsent();
                return;
            }
        }
    }
}

#[cfg(unix)]
async fn reload(h: &server::Handle) {
    match h.reload().await {
        Ok(hash) => eprintln!("oneshot: certificate reloaded, sha-256 {}", server::hex(&hash)),
        Err(e) => eprintln!("oneshot: certificate reload: {e}"),
    }
}

#[cfg(not(unix))]
async fn signals(_: &server::Handle, _: std::time::Duration) {
    let _ = tokio::signal::ctrl_c().await;
}

/// The shared nonce key: a file of 32 raw bytes, or 64 hex.
fn nonce_key(path: &str) -> [u8; 32] {
    let b = std::fs::read(path).unwrap_or_else(|e| die(&format!("--nonce-key {path}: {e}")));
    if let Ok(k) = <[u8; 32]>::try_from(b.as_slice()) {
        return k;
    }
    let t = String::from_utf8_lossy(&b);
    let t = t.trim();
    if t.len() == 64 && t.is_ascii() {
        let mut k = [0u8; 32];
        for (i, k) in k.iter_mut().enumerate() {
            *k = u8::from_str_radix(&t[2 * i..2 * i + 2], 16)
                .unwrap_or_else(|_| die(&format!("--nonce-key {path}: not 64 hex")));
        }
        return k;
    }
    die(&format!("--nonce-key {path}: 32 bytes or 64 hex"))
}

fn addr(s: &str) -> SocketAddr {
    s.parse().unwrap_or_else(|_| die(&format!("{s} is not host:port")))
}

/// A gun by id or by name, its spaces as dashes or not (`sniper`,
/// `sniper-rifle`, `4`).
fn gun(g: &str) -> Option<u8> {
    let want = g.replace('-', " ");
    (1..=sim::arms::CANNON).find(|&id| {
        let name = sim::arms::arm(id).name;
        id.to_string() == g
            || name.eq_ignore_ascii_case(&want)
            || name.split(' ').next().is_some_and(|w| w.eq_ignore_ascii_case(&want))
    })
}

fn die(msg: &str) -> ! {
    eprintln!("oneshot-server: {msg}");
    std::process::exit(2)
}

//! `oneshot-review`: the person's half of the aim review. The server writes
//! a match's numbers beside its replay, and a copy to `<replays>/review/`
//! when they cross a line (`room::watch`), or when a cup ends, its top
//! places with every heat they played (a prize waits on that review); this
//! lists them, shows one, and
//! bans or lifts a wallet on the game's Elo Pros list. Nothing is banned
//! without a person running `ban` after watching the replay.
//!
//! `resend` posts again, signed now, each rating report and stats report
//! Elo Pros didn't take (`<replays>/unreported/`, written by the server
//! when its tries ran out or a drain stopped waiting), and removes each one
//! it takes. One refused for good (a 4xx) stays, with why.
//!
//! ```text
//! oneshot-review [--replays dir] list
//! oneshot-review [--replays dir] show <match id>
//! oneshot-review [--replays dir] done <match id>
//! oneshot-review --elo-key server.pem --elo-key-id eu-1 [--slug oneshot]
//!     [--elo-origin https://elopros.com] ban <wallet> --reason "…" [--match <id>]
//! oneshot-review --elo-key … --elo-key-id … lift <wallet> [--reason "…"]
//! oneshot-review --replays dir --elo-key … --elo-key-id … resend
//! ```

use std::path::{Path, PathBuf};

fn main() {
    let mut replays = PathBuf::from("replays");
    let mut slug = "oneshot".to_string();
    let mut origin = "https://elopros.com".to_string();
    let (mut key, mut key_id, mut reason, mut match_id) = (None, None, None, None);
    let mut rest = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| die(&format!("{a} needs a value")));
        match a.as_str() {
            "--replays" => replays = val().into(),
            "--slug" => slug = val(),
            "--elo-origin" => origin = val(),
            "--elo-key" => key = Some(val()),
            "--elo-key-id" => key_id = Some(val()),
            "--reason" => reason = Some(val()),
            "--match" => match_id = Some(val()),
            _ => rest.push(a),
        }
    }
    let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
    match rest.as_slice() {
        ["list"] => list(&replays),
        ["show", id] => show(&replays, id),
        ["done", id] => {
            let p = replays.join("review").join(format!("{id}.json"));
            std::fs::remove_file(&p).unwrap_or_else(|e| die(&format!("{}: {e}", p.display())));
            println!("{id}: off the review list (its numbers stay in {id}.watch.json)");
        }
        ["resend"] => {
            let elo = signer(&slug, &origin, key, key_id, "a report");
            resend(&replays.join(server::elo::UNREPORTED), &elo);
        }
        [verb @ ("ban" | "lift"), wallet] => {
            let banned = *verb == "ban";
            let reason = match (banned, reason) {
                (true, None) => die("a ban says why: --reason \"…\""),
                (_, r) => r.unwrap_or_default(),
            };
            let elo = signer(&slug, &origin, key, key_id, "a ban");
            let rt = tokio::runtime::Runtime::new().unwrap_or_else(|e| die(&e.to_string()));
            match rt.block_on(elo.ban(wallet, banned, &reason, match_id.as_deref())) {
                Ok(text) => println!("{text}"),
                Err(e) => die(&e),
            }
        }
        _ => die(
            "list | show <id> | done <id> | ban <wallet> --reason … | lift <wallet> | resend (see the source's header)",
        ),
    }
}

/// Elo Pros, signed with the server's key (`what` is what needs it).
fn signer(slug: &str, origin: &str, key: Option<String>, key_id: Option<String>, what: &str) -> server::elo::Elo {
    let (Some(k), Some(id)) = (key, key_id) else {
        die(&format!(
            "{what} is signed with the server's key: --elo-key server.pem --elo-key-id <id>"
        ))
    };
    let pem = std::fs::read_to_string(&k).unwrap_or_else(|e| die(&format!("{k}: {e}")));
    let sk = server::elo::ServerKey::from_pem(slug, &id, &pem).unwrap_or_else(|e| die(&e));
    server::elo::Elo::new(origin, slug, Some(sk))
}

/// Every saved report in `dir` posted again; each one taken is removed.
fn resend(dir: &Path, elo: &server::elo::Elo) {
    use server::elo::{Posted, Unsent};
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    if files.is_empty() {
        println!("nothing unreported in {}", dir.display());
        return;
    }
    let rt = tokio::runtime::Runtime::new().unwrap_or_else(|e| die(&e.to_string()));
    let (mut sent, mut kept) = (0, 0);
    for p in files {
        let name = p
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let Some(u) = std::fs::read(&p).ok().and_then(|b| Unsent::from_json(&b)) else {
            println!("{name}: not a saved report, left");
            kept += 1;
            continue;
        };
        match rt.block_on(elo.resend(&u)).unwrap_or_else(|e| die(&e)) {
            Posted::Taken(_) => match std::fs::remove_file(&p) {
                Ok(()) => {
                    println!("{name}: taken ({}), removed", u.path);
                    sent += 1;
                }
                Err(e) => die(&format!(
                    "{name}: taken, but couldn't remove it ({e}): remove it by hand"
                )),
            },
            Posted::Refused(e) => {
                println!("{name}: refused, kept (remove it if Elo Pros already has it): {e}");
                kept += 1;
            }
            Posted::Passing(e) => {
                println!("{name}: not taken yet, kept: {e}");
                kept += 1;
            }
        }
    }
    println!("{sent} sent, {kept} left in {}", dir.display());
}

fn read(p: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

fn list(replays: &Path) {
    let dir = replays.join("review");
    let mut ids: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    if ids.is_empty() {
        println!("nothing to review in {}", dir.display());
        return;
    }
    for p in ids {
        let Some(v) = read(&p) else { continue };
        let who: Vec<String> = v["seats"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|s| s["flags"].as_array().is_some_and(|f| !f.is_empty()))
            .map(|s| {
                format!(
                    "{} {} [{}]",
                    s["name"].as_str().unwrap_or("?"),
                    s["wallet"].as_str().unwrap_or("no wallet"),
                    s["flags"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|f| f.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect();
        println!("{}  {}", v["id"].as_str().unwrap_or("?"), who.join("; "));
    }
}

fn show(replays: &Path, id: &str) {
    // A cup's prize review: who, and the heats to watch with fog off.
    if let Some(v) = read(&replays.join("review").join(format!("{id}.json"))).filter(|v| v["kind"] == "cup prize") {
        println!("{}", serde_json::to_string_pretty(&v["seats"]).unwrap_or_default());
        println!("\nwatch each with fog off before the prize is paid:");
        for s in v["seats"].as_array().into_iter().flatten() {
            for r in s["replays"].as_array().into_iter().flatten() {
                println!("  oneshot-replay {}", replays.join(r.as_str().unwrap_or("")).display());
            }
        }
        return;
    }
    let p = replays.join(format!("{id}.watch.json"));
    let v = read(&p).unwrap_or_else(|| die(&format!("{}: not found or not JSON", p.display())));
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    println!(
        "\nre-run it: oneshot-replay {}\nlines: head rate ≥ {:.0}% over ≥ {} hits; snaps ≥ {} and ≥ {:.0}% of sightings; \
         median reaction < {} ms over ≥ {}; pre-aim > {} ms",
        replays.join(format!("{id}.osr")).display(),
        room::watch::HEAD_RATE * 100.0,
        room::watch::HEAD_MIN_HITS,
        room::watch::SNAPS_MIN,
        room::watch::SNAP_SHARE * 100.0,
        room::watch::REACTION_MS,
        room::watch::REACTION_MIN,
        room::watch::PREAIM_MS,
    );
}

fn die(msg: &str) -> ! {
    eprintln!("oneshot-review: {msg}");
    std::process::exit(2)
}

//! `oneshot-replay <file.osr>…`: plays each replay through a fresh copy of
//! the rules and checks it ends where the server said: same ticks, same
//! final hash, same score. Prints the kills and the salt; exits 1 if any
//! file doesn't replay.

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("oneshot-replay <file.osr>…");
        std::process::exit(2);
    }
    let mut bad = 0;
    for f in &files {
        let bytes = match std::fs::read(f) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{f}: {e}");
                bad += 1;
                continue;
            }
        };
        match room::replay::verify(&bytes) {
            Ok(s) => {
                println!(
                    "{f}: replays. {} ticks to {:#018x}, score {:?}, salt {:016x}{:016x}, sha256 {}",
                    s.ticks,
                    s.hash,
                    &s.score[..4],
                    s.salt.0,
                    s.salt.1,
                    server::elo::sha256_hex(&bytes)
                );
                for (tick, a, b, cause) in s.kills {
                    println!("  {:>7.2}s  seat {a} killed seat {b} ({cause})", tick as f64 / 128.0);
                }
            }
            Err(e) => {
                println!("{f}: does NOT replay: {e}");
                bad += 1;
            }
        }
    }
    std::process::exit(if bad > 0 { 1 } else { 0 });
}

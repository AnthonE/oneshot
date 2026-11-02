//! `cargo run -p sim --example bot_match -- <seed> <ticks> [map eight]`: the
//! folded hash of a bot match, native (on a map: eight seats of Gun Game if
//! `eight` is 1). `ci/parity.sh` compares it with wasm's.

// A command-line tool, not the sim: it may print.
#![allow(clippy::disallowed_macros)]

fn main() {
    let mut args = std::env::args().skip(1);
    let seed = args.next().and_then(|s| s.parse().ok()).unwrap_or(1u64);
    let ticks = args.next().and_then(|s| s.parse().ok()).unwrap_or(20_000u32);
    let map: Option<u8> = args.next().and_then(|s| s.parse().ok());
    let eight = args.next().is_some_and(|s| s == "1");
    let h = match map {
        Some(m) => sim::bot::bot_match_on(m, eight, seed, ticks),
        None => sim::bot::bot_match(seed, ticks),
    };
    println!("{h:016x}");
}

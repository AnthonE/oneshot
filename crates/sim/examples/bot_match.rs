//! `cargo run -p sim --example bot_match -- <seed> <ticks> [map mode]`: the
//! folded hash of a bot match, native (on a map, in mode 0 Doubles, 1 Gun
//! Game or 2 Big Team). `ci/parity.sh` compares it with wasm's.

// A command-line tool, not the sim: it may print.
#![allow(clippy::disallowed_macros)]

fn main() {
    let mut args = std::env::args().skip(1);
    let seed = args.next().and_then(|s| s.parse().ok()).unwrap_or(1u64);
    let ticks = args.next().and_then(|s| s.parse().ok()).unwrap_or(20_000u32);
    let map: Option<u8> = args.next().and_then(|s| s.parse().ok());
    let mode = args.next().and_then(|s| s.parse().ok()).unwrap_or(0u8);
    let h = match map {
        Some(m) => sim::bot::bot_match_on(m, mode, seed, ticks),
        None => sim::bot::bot_match(seed, ticks),
    };
    println!("{h:016x}");
}

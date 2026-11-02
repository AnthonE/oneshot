//! `cargo run -p sim --example bot_match -- <seed> <ticks>`: the folded hash
//! of a bot match, native. `ci/parity.sh` compares it with wasm's.

// A command-line tool, not the sim: it may print.
#![allow(clippy::disallowed_macros)]

fn main() {
    let mut args = std::env::args().skip(1);
    let seed = args.next().and_then(|s| s.parse().ok()).unwrap_or(1u64);
    let ticks = args.next().and_then(|s| s.parse().ok()).unwrap_or(20_000u32);
    println!("{:016x}", sim::bot::bot_match(seed, ticks));
}

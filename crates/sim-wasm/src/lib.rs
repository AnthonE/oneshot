//! The sim compiled to wasm with no bindings: `ci/parity.mjs` loads it and
//! calls [`bot_match`], `ci/parity.sh` compares the answer with native's.

/// The folded hash of a bot match of `ticks` ticks from `seed`.
#[no_mangle]
pub extern "C" fn bot_match(seed: u64, ticks: u32) -> u64 {
    sim::bot::bot_match(seed, ticks)
}

/// The same on map `map` in `mode` (0 Doubles, 1 Gun Game, 2 Big Team).
#[no_mangle]
pub extern "C" fn bot_match_on(map: u32, mode: u32, seed: u64, ticks: u32) -> u64 {
    sim::bot::bot_match_on(map as u8, mode.min(255) as u8, seed, ticks)
}

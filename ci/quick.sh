#!/bin/sh
# The local check: format, clippy with -D warnings (the sim's walls live in
# crates/sim/clippy.toml), then the tests of any crate named.
# ci/quick.sh                       fmt + clippy
# ci/quick.sh sim proto room        ... and those crates' tests
set -eu
cd "$(dirname "$0")/.."
# The maps' Rust is what their JSON compiles to.
node tools/maps/compile.mjs --check
cargo fmt --all
cargo clippy -q --workspace --all-targets -- -D warnings
cargo clippy -q -p sim-wasm -p client-net --target wasm32-unknown-unknown -- -D warnings
# Fog of war at sixteen seats stays inside its budget (room/examples/fog_cost.rs).
cargo run -q --release -p room --example fog_cost -- --check >/dev/null
for c in "$@"; do
  cargo test -q -p "$c"
done

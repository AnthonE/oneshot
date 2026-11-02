#!/bin/sh
# Native vs wasm: the same bot matches must hash the same (docs/DESIGN.md
# §17). Needs the wasm32-unknown-unknown target and Node.
set -eu
cd "$(dirname "$0")/.."
cargo build -q --release -p sim-wasm --target wasm32-unknown-unknown
wasm=target/wasm32-unknown-unknown/release/sim_wasm.wasm
cargo build -q --release -p sim --example bot_match
fail=0
for seed in 1 2 3; do
  for ticks in 1000 20000; do
    n=$(target/release/examples/bot_match "$seed" "$ticks")
    w=$(node ci/parity.mjs "$wasm" "$seed" "$ticks")
    if [ "$n" = "$w" ]; then
      echo "seed $seed, $ticks ticks: $n"
    else
      echo "MISMATCH seed $seed, $ticks ticks: native $n, wasm $w"
      fail=1
    fi
  done
done
exit $fail

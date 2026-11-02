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
# Every map, Doubles and an eight-seat Gun Game.
for map in $(sed -n 's/^    id: \([0-9]*\),$/\1/p' crates/sim/src/maps/*.rs); do
  for eight in 0 1; do
    n=$(target/release/examples/bot_match 7 6000 "$map" "$eight")
    w=$(node ci/parity.mjs "$wasm" 7 6000 "$map" "$eight")
    if [ "$n" = "$w" ] && [ "$n" != "0000000000000000" ]; then
      echo "map $map, eight $eight: $n"
    else
      echo "MISMATCH map $map, eight $eight: native $n, wasm $w"
      fail=1
    fi
  done
done
exit $fail

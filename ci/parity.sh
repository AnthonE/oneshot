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
# Every map: Doubles, an eight-seat Gun Game, a sixteen-seat Big Team.
for map in $(sed -n 's/^    id: \([0-9]*\),$/\1/p' crates/sim/src/maps/*.rs); do
  for mode in 0 1 2; do
    n=$(target/release/examples/bot_match 7 6000 "$map" "$mode")
    w=$(node ci/parity.mjs "$wasm" 7 6000 "$map" "$mode")
    if [ "$n" = "$w" ] && [ "$n" != "0000000000000000" ]; then
      echo "map $map, mode $mode: $n"
    else
      echo "MISMATCH map $map, mode $mode: native $n, wasm $w"
      fail=1
    fi
  done
done
exit $fail

#!/usr/bin/env bash
# tools/hd.sh [view…]: the HD loop's shooter. Each view is a fixed picture of
# a map (the client's pose mode, no server), drawn by the desktop client on
# this box's own GPU at HIGH and saved by its own `--shot`, which waits on
# the level being there, not on a clock: seconds a view, where ci/look.mjs
# on SwiftShader takes minutes and draws the browser's rung.
#
#   tools/hd.sh                    every view → traces/hd/<view>.png
#   tools/hd.sh beach cliff        those two
#   tools/hd.sh list               the views and their poses
#   HD_QUALITY=medium tools/hd.sh  another rung (potato low medium high)
#   HD_TAG=before tools/hd.sh      → traces/hd/before/<view>.png, to keep a round
#
# The board lays them beside the references and the last pass
# (`ELO_REF_DIR=moodboard node tools/board.mjs look --views hd-beach,…`,
# board/board.json's hd-* views; the hd skill is the loop). Build first:
# cd crates/client && cargo build. The settings are a fresh file, so a
# player's own rows never move a shot. Needs a display (DISPLAY, default :0).
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)

# view | pose | seconds after the level is there (the deck and TAA settle)
VIEWS=(
  "beach|map=3&eye=127,-0.2,4&look=150,-5,-6|6"
  "cliff|map=3&eye=-20,4.5,8&look=0,22,70|2"
  "meadow|map=3&eye=-68,5.3,20&look=-40,3,38|2"
  "vista|map=3&eye=-100,7.3,0&look=150,0,0|2"
  "inland|map=3&eye=118,1.8,6&look=-150,34,-10|2"
  "base|map=3&eye=-82,2.2,-13&look=-100,3.4,0|2"
)

if [ "${1:-}" = list ]; then
  for v in "${VIEWS[@]}"; do IFS='|' read -r id q _ <<<"$v"; printf '%-10s %s\n' "$id" "$q"; done
  exit 0
fi

bin=${ONESHOT_BIN:-$root/crates/client/target/debug/oneshot}
[ -x "$bin" ] || { echo "hd: no $bin (cd crates/client && cargo build)" >&2; exit 1; }
out=$root/traces/hd${HD_TAG:+/$HD_TAG}
mkdir -p "$out"
conf=$(mktemp -d)
trap 'rm -rf "$conf"' EXIT
export DISPLAY=${DISPLAY:-:0} BEVY_ASSET_ROOT="$root/crates/client" XDG_CONFIG_HOME="$conf"

want=("$@")
fail=0
for v in "${VIEWS[@]}"; do
  IFS='|' read -r id q secs <<<"$v"
  if [ ${#want[@]} -gt 0 ] && [[ ! " ${want[*]} " =~ " $id " ]] && [[ ! " ${want[*]} " =~ " hd-$id " ]]; then continue; fi
  png=$out/$id.png
  rm -f "$png"
  if timeout 120 "$bin" --pose "$q" --quality "${HD_QUALITY:-high}" --shot "$png" --shot-at "$secs" >"$out/$id.log" 2>&1 && [ -s "$png" ]; then
    echo "$png  $q"
  else
    echo "hd: $id was not shot (see $out/$id.log)" >&2
    fail=1
  fi
done
exit $fail

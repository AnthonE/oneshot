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
  "ring|map=7&eye=-6,4.5,-13&look=4,4,8|2"
  "coast|map=7&eye=36,4,-20&look=4,-16,2|2"
  "void|map=7&eye=0,1.7,-21&look=-25,-6,-60|2"
  "spire|map=7&eye=0,1.7,7&look=0,11,20|2"
  "gate|map=7&eye=3,2.1,-2&look=-24,2.5,1|2"
  "deck|map=7&eye=-21,4.2,-1&look=6,2.5,2|2"
  "rim|map=7&eye=-2,1.8,-20&look=-24,-3,-8|2"
  "climb|map=7&eye=-11,1.7,18.5&look=-3,7,23|2"
  "well|map=7&eye=-0.7,1.7,20.1&look=0.25,13,21.6|2"
  "room|map=7&eye=-2.7,6.1,22.8&look=1.2,5.6,18|2"
  "gallery|map=7&eye=-0.2,10.1,22.95&look=0,8.6,0|2"
  "backwalk|map=7&eye=-4.85,8.5,24.8&look=1,9.6,24.9|2"
  "behind|map=7&eye=-17,4,33&look=0,5,24|2"
  "northwalk|map=7&eye=-10.3,0.3,22.6&look=-2,-0.4,26.3|2"
  "southwalk|map=7&eye=-16.8,0.3,-19.4&look=-6,-0.2,-20.9|2"
  "hold|map=6&eye=-18,8.6,-2.5&look=4,8.5,1|2"
  "pit|map=6&eye=-7,1.6,-6.2&look=0,9,2|2"
  "topmid|map=6&eye=-2.5,8.6,-5&look=14,8,0|2"
  "lane|map=6&eye=-20,5.1,-13&look=2,7,-9|2"
  "door|map=6&eye=-17,5.2,3&look=6,6,0|2"
  "arcade|map=6&eye=-11,5.1,17.2&look=9,5.2,8|2"
  "pulpit|map=6&eye=0,8.6,-23.5&look=0,7.5,6|2"
  "keel|map=6&eye=-24,1.6,1&look=-8,1.6,0|2"
  "over|map=6&eye=-21,12,-13&look=8,4,6|2"
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
  if timeout "${HD_TIMEOUT:-120}" "$bin" --pose "$q" --quality "${HD_QUALITY:-high}" --shot "$png" --shot-at "$secs" >"$out/$id.log" 2>&1 && [ -s "$png" ]; then
    echo "$png  $q"
  else
    echo "hd: $id was not shot (see $out/$id.log)" >&2
    fail=1
  fi
done
exit $fail

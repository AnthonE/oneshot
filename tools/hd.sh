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
#   HD_QUALITY=medium tools/hd.sh  another rung (potato low medium high ultra)
#   HD_TAG=before tools/hd.sh      → traces/hd/before/<view>.png, to keep a round
#   HD_MAP=9 tools/hd.sh           only that map's views (with no view named): a
#                                  map's own loop, and what the board shoots for it
#   HD_JOBS=2 tools/hd.sh          that many at once, where there is the memory (a
#                                  debug client on lavapipe holds 6 to 8 GB: one
#                                  at a time on a 16 GB box, or the second is killed)
#   tools/hd.sh refs               fetch the board's references that name a source
#                                  (public photographs) into $ELO_REF_DIR (moodboard)
#
# The board lays them beside the references and the last pass
# (`ELO_REF_DIR=moodboard node tools/board.mjs look --views hd-beach,…`,
# board/board.json's hd-* views; the hd skill is the loop). Build first:
# cd crates/client && cargo build. The settings are a fresh file, so a
# player's own rows never move a shot. It draws on the display there is
# (DISPLAY, default :0); on a box with none (a cloud box) it starts Xvfb and
# draws on lavapipe, as ci/desktop_shot.sh does: the same picture a rung
# down in speed, a minute and a half a view where a GPU takes ten seconds
# (most of it the level's load), so shoot a map's views, several at once.
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
  "flyover|map=1&eye=-2,6.6,-17&look=0,6,10|2"
  "underpass|map=1&eye=-12,1.7,-3&look=6,2.5,2|2"
  "slip|map=1&eye=-20,1.7,14&look=-4,4,14|2"
  "block|map=1&eye=-27,5.1,-7&look=8,3,3|2"
  "alley|map=1&eye=-19,1.7,19.5&look=4,2.5,19|2"
  "skyline|map=1&eye=2,6.6,14&look=40,14,60|2"
  "range_lane|map=0&eye=-22,1.7,-18&look=8,2.2,6|2"
  "range_mid|map=0&eye=-6,1.7,-9&look=8,1.8,6|2"
  "range_bunker|map=0&eye=-17,4.1,-2&look=14,2.5,3|2"
  "range_wall|map=0&eye=8,1.7,20&look=-22,5,23|2"
  "dock_top|map=5&eye=-14,4.6,0&look=6,5,8|2"
  "dock_pit|map=5&eye=-9,1.7,-8&look=6,3.4,4|2"
  "dock_lane|map=5&eye=-20,3.1,14&look=8,4,13|2"
  "dock_base|map=5&eye=-22,4.6,-6&look=4,3,3|2"
  "dock_view|map=5&eye=-2,3.1,13&look=-14,14,60|2"
  "tank|map=2&eye=-13,1.6,-1&look=3,2.6,1|2"
  "dais|map=2&eye=-5,1.6,-9&look=0,5,0|2"
  "quay|map=2&eye=-20,3.4,-17&look=-3,2,0|2"
  "bridge|map=2&eye=-17,6.8,0&look=2,6,1|2"
  "oculus|map=2&eye=4,6.8,0.6&look=-1,2,0|2"
  "cistern_stair|map=2&eye=-12,3.4,22&look=3,6,20|2"
  "hold|map=6&eye=-18,8.6,-2.5&look=4,8.5,1|2"
  "pit|map=6&eye=-7,1.6,-6.2&look=0,9,2|2"
  "topmid|map=6&eye=-2.5,8.6,-5&look=14,8,0|2"
  "lane|map=6&eye=-20,5.1,-13&look=2,7,-9|2"
  "door|map=6&eye=-17,5.2,3&look=6,6,0|2"
  "arcade|map=6&eye=-11,5.1,17.2&look=9,5.2,8|2"
  "pulpit|map=6&eye=0,8.6,-23.5&look=0,7.5,6|2"
  "keel|map=6&eye=-24,1.6,1&look=-8,1.6,0|2"
  "over|map=6&eye=-21,12,-13&look=8,4,6|2"
  "pit_yard|map=8&eye=-12.4,1.8,2.5&look=6,4,0|2"
  "pit_crown|map=8&eye=-18.5,8.7,0.6&look=17,7.6,-2|2"
  "pit_deck|map=8&eye=-11.5,5.2,10.5&look=9,4,9|2"
  "pit_edge|map=8&eye=-17,1.7,-11.5&look=-34,-9,-32|2"
  "pit_gantry|map=8&eye=9,8.7,1&look=-6,1,12|2"
  "pit_hall|map=8&eye=-20,1.7,-1.5&look=-14,2.4,9|2"
  "pit_over|map=8&eye=-34,16,-30&look=0,2,0|2"
  "pit_collar|map=8&eye=-6.5,0.7,2.5&look=4,5,-2|2"
  "pit_sheave|map=8&eye=-1,12.1,-2.4&look=-17,5,8|2"
  "pit_shaft|map=8&eye=-9,0.7,2.5&look=4,-14,-12|2"
  "ice_base|map=9&eye=-186,5.9,14&look=-206,8.5,-4|2"
  "ice_hangar|map=9&eye=-211,6.7,5&look=-238,6.5,-2|2"
  "ice_terrace|map=9&eye=-208.5,14.2,3&look=-90,-2,80|2"
  "ice_lake|map=9&eye=-158,2.4,56&look=0,6,96|2"
  "ice_col|map=9&eye=-4,4,-34&look=-80,16,-6|2"
  "ice_perch|map=9&eye=-88,26.3,0&look=-206,20,0|2"
  "ice_moraine|map=9&eye=-134,7.3,-80&look=0,4,-116|2"
  "ice_over|map=9&eye=0,70,-120&look=0,0,40|2"
)

if [ "${1:-}" = list ]; then
  for v in "${VIEWS[@]}"; do IFS='|' read -r id q _ <<<"$v"; printf '%-10s %s\n' "$id" "$q"; done
  exit 0
fi

# The references a view is read against: those the board gives a source are
# public photographs, fetched once into the board's private folder (never
# the repo). Other games' screenshots have no source and are the operator's.
if [ "${1:-}" = refs ]; then
  dir=${ELO_REF_DIR:-moodboard}
  node -e 'for (const v of require("./board/board.json").views) for (const r of v.refs || []) if (r.source) console.log(r.file + "\t" + r.source)' | sort -u |
    while IFS=$'\t' read -r f url; do
      [ -s "$dir/$f" ] && continue
      mkdir -p "$(dirname "$dir/$f")"
      if curl -fsSL -m 60 -A 'oneshot-hd-refs/1.0' -o "$dir/$f" "$url"; then echo "$dir/$f"; else echo "hd: $f was not fetched ($url)" >&2; rm -f "$dir/$f"; fi
    done
  exit 0
fi

bin=${ONESHOT_BIN:-$root/crates/client/target/debug/oneshot}
[ -x "$bin" ] || { echo "hd: no $bin (cd crates/client && cargo build)" >&2; exit 1; }
out=$root/traces/hd${HD_TAG:+/$HD_TAG}
mkdir -p "$out"
conf=$(mktemp -d)
xvfb=
trap '[ -z "$xvfb" ] || kill "$xvfb" 2>/dev/null; rm -rf "$conf"' EXIT
export DISPLAY=${DISPLAY:-:0} BEVY_ASSET_ROOT="$root/crates/client" XDG_CONFIG_HOME="$conf"
# No display: one of our own, and lavapipe unless the caller named a driver.
limit=${HD_TIMEOUT:-120}
d=${DISPLAY#*:}
if [ ! -e "/tmp/.X11-unix/X${d%%.*}" ]; then
  command -v Xvfb >/dev/null || { echo "hd: no display at $DISPLAY and no Xvfb (apt-get install xvfb mesa-vulkan-drivers)" >&2; exit 1; }
  n=90
  while [ -e "/tmp/.X11-unix/X$n" ] || [ -e "/tmp/.X$n-lock" ]; do n=$((n + 1)); done
  Xvfb ":$n" -screen 0 1280x720x24 -nolisten tcp >/dev/null 2>&1 &
  xvfb=$!
  for _ in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  lvp=
  for f in /usr/share/vulkan/icd.d/lvp_icd.json /usr/share/vulkan/icd.d/lvp_icd.*.json; do
    [ -e "$f" ] && { lvp=$f; break; }
  done
  if [ -z "${VK_ICD_FILENAMES:-}${VK_DRIVER_FILES:-}" ] && [ -n "$lvp" ]; then
    export VK_ICD_FILENAMES=$lvp VK_DRIVER_FILES=$lvp
  fi
  limit=${HD_TIMEOUT:-600}
fi

shoot() {
  local id=$1 q=$2 secs=$3 png=$out/$1.png
  rm -f "$png"
  if timeout "$limit" "$bin" --pose "$q" --quality "${HD_QUALITY:-high}" --shot "$png" --shot-at "$secs" >"$out/$id.log" 2>&1 && [ -s "$png" ]; then
    echo "$png  $q"
  else
    echo "hd: $id was not shot (see $out/$id.log)" >&2
    return 1
  fi
}

want=("$@")
jobs=${HD_JOBS:-1}
fail=0
running=0
for v in "${VIEWS[@]}"; do
  IFS='|' read -r id q secs <<<"$v"
  if [ ${#want[@]} -gt 0 ]; then
    if [[ ! " ${want[*]} " =~ " $id " ]] && [[ ! " ${want[*]} " =~ " hd-$id " ]]; then continue; fi
  elif [ -n "${HD_MAP:-}" ] && [[ "$q" != "map=$HD_MAP&"* ]]; then
    continue
  fi
  shoot "$id" "$q" "$secs" &
  running=$((running + 1))
  if [ "$running" -ge "$jobs" ]; then
    wait -n || fail=1
    running=$((running - 1))
  fi
done
while [ "$running" -gt 0 ]; do
  wait -n || fail=1
  running=$((running - 1))
done
exit $fail

#!/usr/bin/env bash
# tools/craft.sh [skimmer|flier]… : the one-seaters' loop (look/craft.rs; the
# `craft` skill). One pass of a craft:
#
#   1. cut    its script in Blender (tools/models/<name>.py on craft.py's kit):
#             the GLB into crates/client/assets/models and the sheet,
#             traces/craft/<name>.png (six views, the sim's hull drawn over
#             three), in about ten seconds;
#   2. shoot  the game's fixed views of it (the desktop client's pose mode on
#             this box's GPU, HIGH, a few seconds each) into
#             traces/craft/<name>/<view>.png, the pass before kept beside
#             each as <view>.last.png;
#   3. board  traces/craft/<name>/board.png: the views four across, each over
#             its last pass, the far view cut in close.
#
#   tools/craft.sh                      both
#   tools/craft.sh flier                one
#   tools/craft.sh list                 the views and their poses
#   CRAFT_VIEWS=side,chase tools/craft.sh skimmer    some views
#   CRAFT_CUT=0 tools/craft.sh          shoot what is built (a change in craft.rs, a shader)
#   CRAFT_SHOOT=0 tools/craft.sh        the cut and its sheet only
#   CRAFT_STAGE='3 -90 -12' tools/craft.sh   on Dry Fork, where they fly (map, x, z; it needs 6 GB free)
#   CRAFT_QUALITY=low tools/craft.sh    another rung (potato low medium high)
#   CRAFT_TAG=before tools/craft.sh     → traces/craft/before/<name>/…, to keep a round by hand
#
# A model's GLB is read when the client starts: a new cut needs no build. A
# change in crates/client does: cd crates/client && cargo build.
#
# No window opens on the desktop: the client draws in an X server of this
# script's own (Xvfb, from PATH, $XVFB or sources/xvfb/usr/bin) on the
# NVIDIA card where there is one. With no Xvfb it draws on DISPLAY. A shot
# waits for free memory and for any compile to end (the box has no swap),
# and the client runs as the first thing the kernel would take.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)

# The views. A camera is in the craft's own frame (`craftcam`: metres ahead
# of its middle, over the ground under it, to its right), so no view reads
# a height off a map; `craftchase` is the camera a player has at its
# controls, the game's own.
#   view | height over the ground | pose | seconds after the level is there
SKIMMER=(
  "side|0|craftseat=1&craftcam=0.1,1.1,3.4&craftaim=0,0.8,0|2"
  "quarter|0|craftseat=1&craftcam=3.0,1.6,2.4&craftaim=0,0.6,0|2"
  "rear|0|craftseat=1&craftcam=-3.1,1.4,-2.1&craftaim=0,0.7,0|2"
  "chase|0|craftseat=1&craftchase=1|2"
  "boost|0|craftseat=1&craftboost=1&craftchase=1|2"
  "bare|0|craftcam=2.3,2.2,-2.2&craftaim=0,0.5,0|2"
  "far|0|craftseat=1&craftcam=17.4,1.7,-14.1&craftaim=0,0.8,0|2"
)
FLIER=(
  "side|0|craftcam=0.2,1.3,6.0&craftaim=0,0.8,0|2"
  "quarter|0|craftcam=4.6,2.0,4.2&craftaim=0,0.7,0|2"
  "rear|0|craftcam=-5.0,1.8,-3.6&craftaim=0,0.8,0|2"
  "chase|0|craftchase=1|2"
  "boost|6|craftboost=1&craftpitch=0.12&craftchase=1|2"
  "under|7|craftpitch=0.1&craftroll=0.45&craftcam=4.5,-5.3,3.5&craftaim=0,0.8,0|2"
  "far|9|craftroll=-0.3&craftcam=17.4,-7.3,-14.1&craftaim=0,0.8,0|2"
)

views_of() { if [ "$1" = flier ]; then printf '%s\n' "${FLIER[@]}"; else printf '%s\n' "${SKIMMER[@]}"; fi; }
kind_of() { if [ "$1" = flier ]; then echo 2; else echo 1; fi; }

names=()
for a in "$@"; do
  case $a in
    list) for n in skimmer flier; do views_of $n | while IFS='|' read -r id h q _; do printf '%-8s %-8s h=%s  %s\n' "$n" "$id" "$h" "$q"; done; done; exit 0 ;;
    skimmer|flier) names+=("$a") ;;
    *) echo "craft: $a is not a craft (skimmer, flier)" >&2; exit 2 ;;
  esac
done
[ ${#names[@]} -gt 0 ] || names=(skimmer flier)

read -r map sx sz <<<"${CRAFT_STAGE:-4 0 -8}"
yaw=${CRAFT_YAW:-0.5}
out=$root/traces/craft${CRAFT_TAG:+/$CRAFT_TAG}
mkdir -p "$out"

if [ "${CRAFT_CUT:-1}" != 0 ]; then
  tools/models/build.sh "${names[@]}"
fi
[ "${CRAFT_SHOOT:-1}" != 0 ] || exit 0

bin=${ONESHOT_BIN:-$root/crates/client/target/debug/oneshot}
[ -x "$bin" ] || { echo "craft: no $bin (cd crates/client && cargo build)" >&2; exit 1; }

# An X server of our own, so nothing opens on the desktop.
xvfb=${XVFB:-$(command -v Xvfb || true)}
[ -n "$xvfb" ] || { [ -x "$root/sources/xvfb/usr/bin/Xvfb" ] && xvfb=$root/sources/xvfb/usr/bin/Xvfb; }
conf=$(mktemp -d)
xpid=
trap '[ -z "$xpid" ] || kill "$xpid" 2>/dev/null; rm -rf "$conf"' EXIT
if [ -n "$xvfb" ]; then
  for n in $(seq 90 99); do
    [ -e "/tmp/.X11-unix/X$n" ] && continue
    "$xvfb" ":$n" -screen 0 1920x1080x24 -nolisten tcp >"$conf/xvfb.log" 2>&1 &
    xpid=$!
    export DISPLAY=":$n"
    for _ in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
    break
  done
else
  echo "craft: no Xvfb (PATH, \$XVFB, sources/xvfb): the client's window opens on ${DISPLAY:-:0}" >&2
  export DISPLAY=${DISPLAY:-:0}
fi
icd=/usr/share/vulkan/icd.d/nvidia_icd.json
if [ -z "${VK_ICD_FILENAMES:-}" ] && [ -e "$icd" ]; then export VK_ICD_FILENAMES=$icd VK_DRIVER_FILES=$icd; fi
export BEVY_ASSET_ROOT="$root/crates/client" XDG_CONFIG_HOME="$conf"

# One shot, held to a share of the box and the kernel's first pick if it
# runs out anyway. The client's load peaks near 5 GB with every gun's and
# every hat's maps whole; with less than that free, the rung is drawn with
# its shadow map a step down, which loads those maps at half size
# (`quality::Tier::half_maps`): a softer shadow edge, and nothing else a
# craft's view shows.
mkdir -p "$conf/oneshot"
whole=${CRAFT_WHOLE_MB:-$([ "$map" = 3 ] && echo 7000 || echo 5800)}
need=${CRAFT_FREE_MB:-$([ "$map" = 3 ] && echo 5600 || echo 4400)}
shoot() {
  local png=$1 q=$2 secs=$3 free
  for _ in $(seq 60); do
    free=$(awk '/MemAvailable/ {print int($2 / 1024)}' /proc/meminfo)
    [ "$free" -ge "$need" ] && ! pgrep -x rustc >/dev/null && break
    sleep 5
  done
  [ "$free" -ge "$need" ] || { echo "craft: ${free} MB free, under the ${need} a shot wants: not shot" >&2; return 1; }
  printf 'quality = %s\n' "${CRAFT_QUALITY:-high}" >"$conf/oneshot/settings.toml"
  if [ "$free" -lt "$whole" ]; then
    echo "shadow_map_px = 1024" >>"$conf/oneshot/settings.toml"
    echo "craft: ${free} MB free: the guns' maps at half size, the shadow map at 1024" >&2
  fi
  local run=(timeout "${CRAFT_TIMEOUT:-120}" "$bin" --pose "$q" --shot "$png" --shot-at "$secs")
  if command -v systemd-run >/dev/null; then
    systemd-run --user --scope --quiet -p MemoryMax=5G -p MemorySwapMax=0 -- \
      sh -c 'echo 900 > /proc/self/oom_score_adj; exec "$@"' craft "${run[@]}"
  else
    "${run[@]}"
  fi
}

IFS=',' read -r -a want <<<"${CRAFT_VIEWS:-}"
fail=0
for name in "${names[@]}"; do
  dir=$out/$name
  mkdir -p "$dir"
  row=()
  while IFS='|' read -r id h q secs; do
    if [ ${#want[@]} -gt 0 ] && [[ ! " ${want[*]} " =~ " $id " ]]; then continue; fi
    png=$dir/$id.png
    [ -s "$png" ] && mv -f "$png" "$dir/$id.last.png"
    pose="map=$map&craft=$sx,$h,$sz&craftkind=$(kind_of "$name")&craftyaw=$yaw&$q"
    if shoot "$png" "$pose" "$secs" >"$dir/$id.log" 2>&1 && [ -s "$png" ]; then
      echo "$png  $pose"
      row+=("$id")
    else
      echo "craft: $name's $id was not shot (see $dir/$id.log)" >&2
      # The pass before stays the one to look at.
      [ -s "$dir/$id.last.png" ] && mv -f "$dir/$id.last.png" "$png"
      fail=1
    fi
  done < <(views_of "$name")
  # The board: four views across, each over its last pass; the far view is
  # its middle, four times the size, since that is what a far craft is.
  if [ ${#row[@]} -gt 0 ] && command -v montage >/dev/null; then
    tiles=()
    for ((i = 0; i < ${#row[@]}; i += 4)); do
      for pass in "" .last; do
        for id in "${row[@]:i:4}"; do
          src=$dir/$id$pass.png
          if [ ! -s "$src" ]; then tiles+=("null:"); continue; fi
          if [ "$id" = far ]; then
            convert "$src" -gravity center -crop 25%x25%+0+0 +repage -filter point -resize 400% "$conf/$id$pass.png"
            src=$conf/$id$pass.png
          fi
          tiles+=("$src")
        done
        for ((k = ${#row[@]} - i; k < 4; k++)); do tiles+=("null:"); done
      done
    done
    montage "${tiles[@]}" -tile 4x -geometry 640x360+4+4 -background '#202020' "$dir/board.png"
    echo "$dir/board.png  (${row[*]}; four across, each over its last pass)"
  fi
done
exit $fail

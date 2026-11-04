#!/usr/bin/env bash
# ci/desktop_shot.sh [out.png] [seconds] [mode] [map] [autoplay 0|1] [more...]:
# the desktop client under Xvfb on lavapipe (Mesa's software Vulkan), one
# frame by its own `--shot`: the desktop twin of ci/client_shot.mjs, with
# its arguments. It waits on the screen, not the clock: the frame is taken
# `seconds` after the screen is there (its level built and dressed, its
# shaders compiled, no curtain or loading overlay over it), and the client
# exits non-zero, saying why, if it never is.
#
#   autoplay 0: the title (no server); 1: a match of `mode` on `map` against
#   a local target/release/oneshot-server, the autopilot driving.
#   more: menu (the Esc menu over the match), settings (over the title, or
#   the match), fxdemo, lite, capsules, name=NAME, pose=QUERY (no server:
#   a picture, `map=6&eye=-20,6,0&look=0,2,0` or `yaw=1.57&arm=3`).
#
#   ci/desktop_shot.sh traces/title.png 2 0 0 0
#   ci/desktop_shot.sh traces/settings.png 2 0 0 0 settings
#   ci/desktop_shot.sh traces/pose.png 1 0 0 0 'pose=map=6&eye=-20,6,0&look=0,2,0'
#   ci/desktop_shot.sh traces/menu.png 4 6 3 1 menu
#
# Needs Xvfb and lavapipe (apt-get install xvfb mesa-vulkan-drivers), the
# desktop client (ci/client.sh builds crates/client/target/debug/oneshot;
# ONESHOT_BIN names another) and for a match the release server (cargo
# build --release -p server). The settings file is a fresh one, so a
# player's own field of view or keys never move a shot. The client's log
# goes beside the shot (<out>.log).
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)
out=${1:-traces/desktop.png}
secs=${2:-2}
mode=${3:-6}
map=${4:-3}
auto=${5:-0}
more=("${@:6}")
bin=${ONESHOT_BIN:-crates/client/target/debug/oneshot}
[ -x "$bin" ] || { echo "desktop_shot: no $bin (ci/client.sh, or cd crates/client && cargo build)" >&2; exit 1; }
command -v Xvfb >/dev/null || { echo "desktop_shot: no Xvfb (apt-get install xvfb mesa-vulkan-drivers)" >&2; exit 1; }
case $out in /*) ;; *) out=$root/$out ;; esac
case $bin in /*) ;; *) bin=$root/$bin ;; esac
mkdir -p "$(dirname "$out")"
log=$out.log

args=(--shot "$out" --shot-at "$secs")
match=0
[ "$auto" = 1 ] && { match=1; args+=(--autoplay); }
for w in "${more[@]}"; do
  case $w in
    menu | menu=1) args+=(--menu) ;;
    settings | settings=1) args+=(--settings) ;;
    title | title=1) ;;
    fxdemo | fxdemo=1) match=1; args+=(--fxdemo) ;;
    lite | lite=1) args+=(--lite) ;;
    capsules | capsules=1) args+=(--capsules) ;;
    name=*) args+=(--name "${w#name=}") ;;
    pose=*) args+=(--pose "${w#pose=}") ;;
    *) echo "desktop_shot: what is '$w'? (menu settings title fxdemo lite capsules name= pose=)" >&2; exit 1 ;;
  esac
done
case " ${args[*]} " in *" --pose "*) match=0 ;; esac

tmp=$(mktemp -d)
pids=()
cleanup() {
  for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done
  rm -rf "$tmp"
}
trap cleanup EXIT

# A display of our own: the first free one from :90.
n=90
while [ -e "/tmp/.X11-unix/X$n" ] || [ -e "/tmp/.X$n-lock" ]; do n=$((n + 1)); done
Xvfb ":$n" -screen 0 1280x720x24 -nolisten tcp >/dev/null 2>&1 &
pids+=($!)
for _ in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done

if [ "$match" = 1 ]; then
  [ -x target/release/oneshot-server ] || { echo "desktop_shot: no target/release/oneshot-server (cargo build --release -p server)" >&2; exit 1; }
  target/release/oneshot-server --bind 127.0.0.1:0 --http 127.0.0.1:0 --map "$map" >"$tmp/server.out" 2>"$tmp/server.err" &
  pids+=($!)
  url=
  for _ in $(seq 200); do
    url=$(sed -n 's/.*play: \(http:\/\/[^ ]*\).*/\1/p' "$tmp/server.out" | head -1)
    [ -n "$url" ] && break
    sleep 0.1
  done
  [ -n "$url" ] || { echo "desktop_shot: the server never said where to play" >&2; cat "$tmp/server.err" >&2; exit 1; }
  args+=(--server "$url" --mode "$mode")
fi

# lavapipe, unless the caller named a driver.
lvp=/usr/share/vulkan/icd.d/lvp_icd.json
if [ -z "${VK_ICD_FILENAMES:-}${VK_DRIVER_FILES:-}" ] && [ -e "$lvp" ]; then
  export VK_ICD_FILENAMES=$lvp VK_DRIVER_FILES=$lvp
fi
mkdir -p "$tmp/config"
set +e
DISPLAY=":$n" XDG_CONFIG_HOME="$tmp/config" BEVY_ASSET_ROOT="$root/crates/client" \
  timeout 900 "$bin" "${args[@]}" >"$log" 2>&1
code=$?
set -e
if [ "$code" != 0 ] || [ ! -s "$out" ]; then
  echo "desktop_shot: the client exited $code; the end of $log:" >&2
  tail -n 25 "$log" >&2
  exit 1
fi
grep -h "oneshot: " "$log" | grep -v "^\s*$" | tail -n 4
echo "shot $out"

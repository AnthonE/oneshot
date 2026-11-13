#!/usr/bin/env bash
# Trailer footage and clips, on a box with no GPU (after Gates' ci/film.sh).
#
#   ci/film.sh record TAPE [SECONDS] [MODE] [MAP]  # tape a match the autopilot plays
#   ci/film.sh scan TAPE [EVERY_S]                 # who is where, and the kills
#   ci/film.sh shoot SHOTS.json [MORE.json ...]    # draw shot lists, each shot into ffmpeg
#   ci/film.sh clip TAPE [STYLE]                   # a tape as the clip key's video
#   ci/film.sh pose OUT.png 'QUERY' [SECONDS]      # a pose (crates/client/src/pose.rs) drawn at HIGH:
#                                                  # the trailer's title plate is one
#
# How it works: `record` plays a match on a local target/release/oneshot-server
# (MODE 6, Big Team, on MAP 3 unless named; SECONDS 90) with the desktop
# client's autopilot, which keeps everything the match sends it and writes
# it as a tape (`oneshot --record`, crates/client/src/clip.rs). `oneshot
# --film` hands those bytes back to a watching session on a fixed frame
# clock with a scripted camera and pipes each shot into ffmpeg
# (crates/client/src/film/), so lavapipe at a frame a second still makes
# smooth 30 fps footage. `scan` prints a tape's timeline, which is how a shot
# list finds its moments. Shot lists are JSON, documented at the top of
# crates/client/src/film/script.rs; the house one is ci/film/trailer.json
# (`ci/film.sh record traces/film/match.tape 90`, then `ci/film.sh shoot
# ci/film/trailer.json`: traces/film/trailer/NN-name.mp4 beside its .wav).
# The trailer's lists are ci/film/trailer-<tape>.json: one tape each,
# written by ci/film/plan.py from the tapes' scans (Big Team's by hand),
# and ci/trailer/make.sh cuts the trailer from what they shoot.
#
# FILM_SERVER_ARGS adds to the server's flags for a record: the trailer's
# sword fights are `FILM_SERVER_ARGS='--loadout sword,br'`, everyone spawning
# with the sword. FILM_CLIENT_ARGS adds to the recorder's: the craft's tapes
# are `FILM_CLIENT_ARGS='--pilot flier'` (or skimmer), the autopilot boarding
# the nearest free one and flying it (Big Team on Icefall, map 9).
#
# A tape runs from the join, and the level takes a minute to build on
# lavapipe: ask for the load plus the play you want (170 s for a minute
# and a half of it). One recorder at a time: a client is about 5 GB.
# `clip` is what F9 makes of a tape on the desktop: STYLE is wide (the
# default), pov, chase, mix or long; the video lands beside the tape.
#
# A tape is only what that client was sent (fog included): anyone but the
# player is on it only while the player could have seen them. It is wire
# bytes: tape it again after a PROTO_VER bump.
#
# FILM_QUALITY=potato|low|medium|high|ultra draws every shot at that rung, whatever
# the list says (a clip's styles say high: slow on lavapipe).
#
# Needs Xvfb and lavapipe (apt-get install xvfb mesa-vulkan-drivers), ffmpeg
# for shoot and clip, the desktop client (crates/client/target/debug/oneshot,
# built if missing; ONESHOT_BIN names another) and for record the release
# server (cargo build --release -p server). The settings file is a fresh one,
# so a player's own keys or field of view never move a shot.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)
fail() { echo "film: $*" >&2; exit 1; }

cmd="${1:-}"
[ -n "$cmd" ] || { sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
shift

bin=${ONESHOT_BIN:-crates/client/target/debug/oneshot}
case $bin in /*) ;; *) bin=$root/$bin ;; esac
if [ ! -x "$bin" ]; then
  (cd crates/client && cargo build -q) || fail "the desktop client did not build (CLAUDE.md: a fresh box needs libwayland-dev libudev-dev)"
fi
[ -x "$bin" ] || fail "no $bin"

tmp=$(mktemp -d)
pids=()
cleanup() {
  for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done
  rm -rf "$tmp"
}
trap cleanup EXIT

# A display of our own, big enough for a 9:16 frame and a supersample.
display() {
  command -v Xvfb >/dev/null || fail "no Xvfb (apt-get install xvfb mesa-vulkan-drivers)"
  n=90
  while [ -e "/tmp/.X11-unix/X$n" ] || [ -e "/tmp/.X$n-lock" ]; do n=$((n + 1)); done
  Xvfb ":$n" -screen 0 4096x4096x24 -nolisten tcp >/dev/null 2>&1 &
  pids+=($!)
  for _ in $(seq 50); do [ -e "/tmp/.X11-unix/X$n" ] && break; sleep 0.1; done
  export DISPLAY=":$n"
  lvp=/usr/share/vulkan/icd.d/lvp_icd.json
  if [ -z "${VK_ICD_FILENAMES:-}${VK_DRIVER_FILES:-}" ] && [ -e "$lvp" ]; then
    export VK_ICD_FILENAMES=$lvp VK_DRIVER_FILES=$lvp
  fi
  mkdir -p "$tmp/config"
  export XDG_CONFIG_HOME="$tmp/config" BEVY_ASSET_ROOT="$root/crates/client"
}

# The client, its log kept and its film lines shown as they come.
run() {
  local log=$1
  shift
  set +e
  "$bin" "$@" 2>&1 | tee "$log" | grep --line-buffered -E "film:|oneshot:|ERROR|panicked" | sed -u 's/\x1b\[[0-9;]*m//g'
  local code=${PIPESTATUS[0]}
  set -e
  return "$code"
}

case "$cmd" in
record)
  tape="${1:?record needs a tape path}"
  secs="${2:-90}"
  mode="${3:-6}"
  map="${4:-3}"
  [ -x target/release/oneshot-server ] || fail "no target/release/oneshot-server (cargo build --release -p server)"
  case $tape in /*) ;; *) tape=$root/$tape ;; esac
  mkdir -p "$(dirname "$tape")"
  display
  target/release/oneshot-server --bind 127.0.0.1:0 --http 127.0.0.1:0 --all-modes --map "$map" ${FILM_SERVER_ARGS:-} >"$tmp/server.out" 2>"$tmp/server.err" &
  pids+=($!)
  url=
  for _ in $(seq 200); do
    url=$(sed -n 's/.*play: \(http:\/\/[^ ]*\).*/\1/p' "$tmp/server.out" | head -1)
    [ -n "$url" ] && break
    sleep 0.1
  done
  [ -n "$url" ] || { cat "$tmp/server.err" >&2; fail "the server never said where to play"; }
  echo "film: taping $secs s of mode $mode on map $map"
  # The recorder draws small: a tape is the server's bytes, never its
  # pixels, and on lavapipe a full window draws too slowly for its inputs
  # to reach the server in time (past room::LOST_HOLD_MS the stick eases
  # to still, and the autopilot crawls at a third of its speed).
  export WINIT_X11_SCALE_FACTOR=${FILM_RECORD_SCALE:-0.4}
  run "$tape.log" --server "$url" --mode "$mode" --autoplay --lite ${FILM_CLIENT_ARGS:-} --record "$tape" "$secs" \
    || fail "the recorder exited non-zero (its log: $tape.log)"
  [ -s "$tape" ] || fail "no tape at $tape"
  ;;

scan)
  "$bin" --scan "${1:?scan needs a tape}" --every "${2:-5}"
  ;;

shoot)
  [ $# -gt 0 ] || fail "shoot needs a shot list"
  command -v ffmpeg >/dev/null || fail "no ffmpeg (apt-get install ffmpeg)"
  display
  for shots in "$@"; do
    echo "film: shooting $shots"
    run "$tmp/film.log" --film "$shots" ${FILM_QUALITY:+--quality "$FILM_QUALITY"} || { tail -n 25 "$tmp/film.log" >&2; fail "$shots did not finish"; }
  done
  ;;

clip)
  tape="${1:?clip needs a tape}"
  command -v ffmpeg >/dev/null || fail "no ffmpeg (apt-get install ffmpeg)"
  display
  run "$tmp/clip.log" --clip "$tape" --style "${2:-wide}" ${FILM_QUALITY:+--quality "$FILM_QUALITY"} || { tail -n 25 "$tmp/clip.log" >&2; fail "the clip did not finish"; }
  ;;

pose)
  # No tape: one fighter (or a map's view) as pose.rs draws it, the frame
  # taken SECONDS after the screen is there. The window is the client's
  # default, 1280x720. The trailer's title plate:
  #   ci/film.sh pose traces/film/trailer/90-still-tt.png \
  #     'yaw=3.1416&arm=2&headwear=2&fighter=1&eye=-0.3,1.4,8.0&look=1.5,1.3,8.0' 3
  # (a moved eye stands the fighter only with fighter=1)
  out="${1:?pose needs an output png}"
  query="${2:?pose needs a query (pose.rs)}"
  case $out in /*) ;; *) out=$root/$out ;; esac
  mkdir -p "$(dirname "$out")"
  display
  run "$tmp/pose.log" --pose "$query" --quality "${FILM_QUALITY:-high}" --shot "$out" --shot-at "${3:-3}" \
    || { tail -n 25 "$tmp/pose.log" >&2; fail "the pose was not drawn"; }
  [ -s "$out" ] || fail "no picture at $out"
  ;;

*) fail "unknown command $cmd (record, scan, shoot, clip, pose)" ;;
esac

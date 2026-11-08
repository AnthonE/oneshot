#!/usr/bin/env bash
# tools/reel.sh <gun>…: a gun's reel. The real game through the gun's eyes,
# every frame, where the fx stage (tools/fx.sh) shows one effect alone.
#
# The autopilot's range drill (`oneshot --autoplay --drill`, lib.rs' `drill`)
# works the gun on The Range against a local server: at rest, a burst at a
# wall, a burst into the floor, a melee, a frag thrown at the wall, a
# reload. The match is taped and drawn back first person at 60 fps on this
# box's GPU (the film pipeline: every frame is there, however long it took
# to draw), and tools/reel.py cuts it into sheets of consecutive frames
# (tools/strip.py), each captioned with its frame and time:
#
#   tools/reel.sh rifle sniper       → traces/reel/<gun>/{pov.mp4, rest, burst, wall, floor, melee, frag, reload}.jpg
#   tools/reel.sh list               the guns
#   REEL_QUALITY=low tools/reel.sh rifle    another rung (potato low medium high; high has bloom)
#   REEL_TAG=before tools/reel.sh rifle     → traces/reel/before/<gun>/, to keep a round
#   REEL_SIZE=1920x1080 …                   the frame (1280x720)
#   REEL_KNOBS='Sparks.speed=4,12;…' tools/reel.sh rifle   retuned, no rebuild (the fx skill's knobs)
#   REEL_REFILM=1 tools/reel.sh rifle       its last tape drawn again, for a change that doesn't touch the sim
#
# Build first: cargo build --release -p server; cd crates/client && cargo
# build. Needs a display (DISPLAY, default :0), ffmpeg, and Python's PIL and
# numpy. The settings are a fresh file, so a player's own never move a shot.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$(pwd)

# name | the server's --loadout id
GUNS="rifle:1 br:2 magnum:3 sniper:4 shotgun:5 rocket:6 smg:7 sword:8 carbine:9 beam:10 ion:11 pistol:12 shards:13 launcher:14 cannon:15"
if [ "${1:-}" = list ] || [ $# -eq 0 ]; then
  for g in $GUNS; do echo "${g%%:*}"; done
  [ $# -gt 0 ] || { sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
  exit 0
fi

bin=${ONESHOT_BIN:-$root/crates/client/target/debug/oneshot}
srv=$root/target/release/oneshot-server
[ -x "$bin" ] || { echo "reel: no $bin (cd crates/client && cargo build)" >&2; exit 1; }
[ -x "$srv" ] || { echo "reel: no $srv (cargo build --release -p server)" >&2; exit 1; }
size=${REEL_SIZE:-1280x720}
tmp=$(mktemp -d)
spid=
cleanup() { [ -z "$spid" ] || kill "$spid" 2>/dev/null || true; rm -rf "${tmp:?}"; }
trap cleanup EXIT
mkdir -p "$tmp/config"
export DISPLAY=${DISPLAY:-:0} BEVY_ASSET_ROOT="$root/crates/client" XDG_CONFIG_HOME="$tmp/config"
# The drill's wait (4 s), its walk to its wall (9 s at most, a few as a
# rule) and its beats (14.2 s), and what of that is drawn: from a second
# before its stand to the end of its beats.
SECS=${REEL_SECS:-27}
LEN=16

fail=0
for name in "$@"; do
  id=
  for g in $GUNS; do [ "${g%%:*}" = "$name" ] && id=${g##*:}; done
  [ -n "$id" ] || { echo "reel: no gun $name (tools/reel.sh list)" >&2; fail=1; continue; }
  out=$root/traces/reel${REEL_TAG:+/$REEL_TAG}/$name
  mkdir -p "$out"
  rm -f "${out:?}"/*.jpg "${out:?}/pov.mp4"
  tape=$out/drill.tape
  if [ -z "${REEL_REFILM:-}" ] || [ ! -s "$tape" ]; then
    rm -f "${tape:?}"
    "$srv" --bind 127.0.0.1:0 --http 127.0.0.1:0 --all-modes --map range --loadout "$id" --passive-bots >"$tmp/server.out" 2>"$tmp/server.err" &
    spid=$!
    url=
    for _ in $(seq 200); do
      url=$(sed -n 's/.*play: \(http:\/\/[^ ]*\).*/\1/p' "$tmp/server.out" | head -1)
      [ -n "$url" ] && break
      sleep 0.1
    done
    [ -n "$url" ] || { cat "$tmp/server.err" >&2; echo "reel: the server never said where to play" >&2; exit 1; }
    if ! timeout $((SECS + 120)) "$bin" --server "$url" --mode casual --autoplay --drill --lite --record "$tape" "$SECS" >"$out/tape.log" 2>&1; then
      echo "reel: $name was not taped (see $out/tape.log)" >&2
      fail=1
    fi
    kill "$spid" 2>/dev/null || true
    wait "$spid" 2>/dev/null || true
    spid=
  fi
  [ -s "$tape" ] || { fail=1; continue; }
  # When the drill stood, counted back from the tape's end: both on the
  # link's clock, in the recorder's log.
  clean() { sed 's/\x1b\[[0-9;]*m//g' "$out/tape.log"; }
  began=$(clean | sed -n 's/.*recording [0-9]* s from \([0-9]*\) ms.*/\1/p' | head -1)
  stood=$(clean | sed -n 's/.*drill stands at \([0-9]*\) ms.*/\1/p' | head -1)
  [ -n "$began" ] && [ -n "$stood" ] || { echo "reel: $name never took its stand (see $out/tape.log)" >&2; fail=1; continue; }
  back=$(python3 -c "print(round(($began + $SECS * 1000 - $stood) / 1000 + 1.0, 3))")
  len=$(python3 -c "print(min($back, $LEN))")
  cat >"$tmp/$name.json" <<JSON
{"tape": "$tape", "out": "$tmp/$name", "width": ${size%x*}, "height": ${size#*x}, "fps": 60,
 "quality": "${REEL_QUALITY:-high}",
 "shots": [{"name": "pov", "at": -$back, "len": $len, "pov": true}]}
JSON
  if ! timeout 900 "$bin" --film "$tmp/$name.json" ${REEL_KNOBS:+--knobs "$REEL_KNOBS"} >"$out/film.log" 2>&1 || [ ! -s "$tmp/$name/00-pov.mp4" ]; then
    echo "reel: $name was not drawn (see $out/film.log)" >&2
    fail=1
    continue
  fi
  mv "$tmp/$name/00-pov.mp4" "$out/pov.mp4"
  python3 tools/reel.py "$out" "$name" --stood 1.0 --quality "${REEL_QUALITY:-high}"
done
exit $fail

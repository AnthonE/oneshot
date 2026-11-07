#!/bin/bash
# ci/native_shot.sh out.png 'pose query' [seconds]      the desktop client's pose mode
# ci/native_shot.sh out.png match [map] [mode] [seconds] [--autoplay]   a match on a local server
# A picture from the desktop client on this box's display and GPU (DISPLAY,
# default :0), grabbed with ImageMagick's import: the desktop tier (bloom,
# shadows) in seconds, where ci/look.mjs on SwiftShader takes minutes on a
# ship's hall. Build first: cd crates/client && cargo build; the match
# needs target/release/oneshot-server. ci/look.mjs stays the board's tier.
set -u
cd "$(dirname "$0")/.."
out=$1
shift
export DISPLAY=${DISPLAY:-:0}
export BEVY_ASSET_ROOT="$PWD/crates/client"
mkdir -p traces
spid=
if [ "${1:-}" = match ]; then
  map=${2:-6}; mode=${3:-3}; wait=${4:-8}; auto=${5:-}
  target/release/oneshot-server --bind 127.0.0.1:0 --http 127.0.0.1:0 --web crates/client/dist --map "$map" > traces/native-server.log 2>&1 &
  spid=$!
  url=
  for _ in $(seq 1 100); do url=$(sed -n 's/.*play: \(http:\/\/[^ ]*\).*/\1/p' traces/native-server.log | head -1); [ -n "$url" ] && break; sleep 0.1; done
  host=${url#http://}; host=${host%%/*}
  crates/client/target/debug/oneshot --windowed --server "$host" --mode "$mode" $auto --name shot > traces/native-client.log 2>&1 &
else
  q=$1; wait=${2:-8}
  crates/client/target/debug/oneshot --pose "$q" > traces/native-client.log 2>&1 &
fi
cpid=$!
sleep "$wait"
id=$(xwininfo -root -tree 2>/dev/null | grep -i ' "oneshot"' | head -1 | awk '{print $1}')
if [ -n "$id" ]; then import -window "$id" "$out"; else import -window root "$out"; fi
kill $cpid 2>/dev/null; wait $cpid 2>/dev/null
if [ -n "$spid" ]; then kill -INT $spid 2>/dev/null; wait $spid 2>/dev/null; fi
echo "$out"

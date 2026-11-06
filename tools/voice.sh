#!/bin/sh
# An announcer's line cut from its recording (tools/voice.mjs makes them; the web
# app's serve as well): the silent ends off, mono, levelled like the rest, into
# crates/client/assets/sound/voice/ as <slug>.ogg, the name lower-cased, spaces
# to underscores (medal::slug). The lines are the voiced medals
# (crates/proto/src/medal.rs, its `m("Name", Family, true, …)` rows) and those
# about the match (crates/client/src/sound/mod.rs, `LINES`).
#   tools/voice.sh cut double.mp3 "Double Kill"
# Needs ffmpeg built with libvorbis.
set -eu
cd "$(dirname "$0")/.."
out=crates/client/assets/sound/voice
mkdir -p "$out"

slug() {
  printf '%s' "$1" | tr 'A-Z ' 'a-z_' | tr -cd 'a-z0-9_'
}

# A recording's line. Only its ends are trimmed (a line may pause); then
# every line to one loudness (EBU R128), its peaks held by a limiter.
recorded() {
  file="$out/$(slug "$2").ogg"
  trim="aformat=channel_layouts=mono,\
silenceremove=start_periods=1:start_threshold=-50dB,afade=d=0.005,areverse,\
silenceremove=start_periods=1:start_threshold=-50dB,afade=d=0.03,areverse"
  lufs=$(ffmpeg -nostdin -hide_banner -i "$1" -af "$trim,ebur128" -f null - 2>&1 |
    awk '/^ +I:/ { v = $2 } END { print v }')
  gain=$(awk "BEGIN { print -15 - ($lufs) }")
  ffmpeg -nostdin -hide_banner -loglevel error -y -i "$1" \
    -af "$trim,volume=${gain}dB,alimiter=limit=0.84:attack=2:release=60:level=disabled" \
    -ac 1 -ar 44100 -c:a libvorbis -q:a 4 "$file"
  echo "$file"
}

if [ "${1:-}" = cut ] && [ $# -eq 3 ]; then
  recorded "$2" "$3"
  exit 0
fi
echo 'usage: tools/voice.sh cut <recording> "<Name>"' >&2
exit 1

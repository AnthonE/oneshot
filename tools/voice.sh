#!/bin/sh
# The announcer's lines: one clip per voiced medal (crates/proto/src/medal.rs,
# its `m("Name", Family, true, …)` rows) and per line about the match
# (crates/client/src/sound/mod.rs, `LINES`) into
# crates/client/assets/sound/voice/ as <slug>.ogg, the name lower-cased,
# spaces to underscores (medal::slug).
#
# A recorded line is cut from its recording: the silent ends off, mono,
# levelled like the rest:
#   tools/voice.sh cut double.mp3 "Double Kill"
# A medal with no recording has a placeholder: synthesized with ffmpeg's
# Flite (CMU, BSD-style licence; what it says is ours), pitched down, given a
# little room and levelled:
#   tools/voice.sh                      # every voiced medal with no clip yet
#   tools/voice.sh "Double Kill" Perfect   # these again (over a recording too)
# Needs ffmpeg built with libflite and libvorbis.
set -eu
cd "$(dirname "$0")/.."
out=crates/client/assets/sound/voice
mkdir -p "$out"
voice=${VOICE:-rms}

slug() {
  printf '%s' "$1" | tr 'A-Z ' 'a-z_' | tr -cd 'a-z0-9_'
}

say() {
  name=$1
  file="$out/$(slug "$name").ogg"
  # Flite reads the text as filter options: quotes and colons escaped.
  text=$(printf '%s' "$name" | sed "s/[':]//g")
  ffmpeg -nostdin -hide_banner -loglevel error -y \
    -f lavfi -i "flite=text='$text!':voice=$voice" \
    -af "asetrate=16000*0.86,aresample=44100,atempo=1.08,\
aecho=0.8:0.5:40|80:0.25|0.15,\
acompressor=threshold=-18dB:ratio=4:attack=5:release=80,\
loudnorm=I=-14:TP=-1.5,\
silenceremove=start_periods=1:start_threshold=-50dB:stop_periods=1:stop_threshold=-50dB:stop_duration=0.2" \
    -ac 1 -ar 44100 -c:a libvorbis -q:a 4 "$file"
  echo "$file"
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

if [ "${1:-}" = cut ]; then
  recorded "$2" "$3"
  exit 0
fi
if [ $# -gt 0 ]; then
  for n in "$@"; do say "$n"; done
  exit 0
fi
grep -o 'm("[^"]*", [A-Za-z]*, true' crates/proto/src/medal.rs | sed 's/^m("\([^"]*\)".*/\1/' |
  while read -r n; do [ -e "$out/$(slug "$n").ogg" ] || say "$n"; done

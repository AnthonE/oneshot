#!/bin/sh
# The announcer's lines: one clip per voiced medal (crates/proto/src/medal.rs,
# its `m("Name", Family, true, …)` rows) into crates/client/assets/sound/voice/
# as <slug>.ogg, the name lower-cased, spaces to underscores (medal::slug).
#
# Placeholders: synthesized with ffmpeg's Flite (CMU, BSD-style licence;
# what it says is ours), pitched down, given a little room and levelled.
# A recorded line dropped in under the same name replaces one; run with
# names to redo only those:
#   tools/voice.sh                      # every voiced medal
#   tools/voice.sh "Double Kill" Perfect
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

if [ $# -gt 0 ]; then
  for n in "$@"; do say "$n"; done
  exit 0
fi
grep -o 'm("[^"]*", [A-Za-z]*, true' crates/proto/src/medal.rs | sed 's/^m("\([^"]*\)".*/\1/' |
  while read -r n; do say "$n"; done

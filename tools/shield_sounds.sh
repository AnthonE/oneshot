#!/bin/sh
# The shield's two cues, ours, synthesized with ffmpeg into
# crates/client/assets/sound/: shield_low_0.ogg, the alarm's one beep
# (`sound.rs` repeats it while the shield is empty), and shield_charge_0.ogg,
# the hum climbing as it comes back. Needs ffmpeg with libvorbis.
set -eu
cd "$(dirname "$0")/.."
out=crates/client/assets/sound
gen() {
  ffmpeg -nostdin -hide_banner -loglevel error -y -f lavfi -i "aevalsrc=$2:s=44100:d=$3" \
    -af "$4" -ac 1 -c:a libvorbis -q:a 4 "$out/$1.ogg"
}
# A bright double-pitched beep, 90 ms, clicked on and off.
gen shield_low_0 \
  "0.5*sin(2*PI*1320*t)+0.25*sin(2*PI*2640*t)+0.12*sin(2*PI*3960*t)" 0.09 \
  "afade=t=in:d=0.004,afade=t=out:st=0.06:d=0.03,volume=0.8"
# A chirp from 180 Hz to 760 Hz with a fifth over it and a shimmer,
# swelling in and fading out over 0.9 s.
gen shield_charge_0 \
  "(0.55*sin(2*PI*(180*t+322*t*t))+0.25*sin(2*PI*1.5*(180*t+322*t*t)))*(0.8+0.2*sin(2*PI*31*t))" 0.9 \
  "afade=t=in:d=0.25,afade=t=out:st=0.55:d=0.35,volume=0.7"
echo "$out/shield_low_0.ogg $out/shield_charge_0.ogg"

#!/usr/bin/env bash
# The sounds made here rather than by ElevenLabs (tools/sfx.mjs): ours.
#   kill_0          a kill of ours: a low thump under two rising notes
#   pump_shotgun_0  the shotgun's pump after a shot: the rack at the end of
#                   its reload, cut out
# Mono 44.1 kHz Vorbis into crates/client/assets/sound. Needs ffmpeg.
set -euo pipefail
cd "$(dirname "$0")/.."
out=crates/client/assets/sound
ff() { ffmpeg -loglevel error -y "$@"; }

thump='0.9*sin(2*PI*(55*t+90*0.035*(1-exp(-t/0.035))))*exp(-t/0.06)'
ding='(0.55*sin(2*PI*1568*t)+0.2*sin(2*PI*3136*t))*(1-exp(-t/0.002))*exp(-t/0.11)'
dong='gte(t,0.075)*(0.6*sin(2*PI*2349*(t-0.075))+0.18*sin(2*PI*4698*(t-0.075)))*(1-exp(-(t-0.075)/0.002))*exp(-(t-0.075)/0.16)'
tmp=$(mktemp --suffix=.wav)
ff -f lavfi -i "aevalsrc='0.5*($thump+$ding+$dong)':s=44100:d=0.55" -af "afade=t=out:st=0.45:d=0.1" "$tmp"
# Its peak to -3 dB, as the sfx' are.
top=$(ffmpeg -hide_banner -i "$tmp" -af volumedetect -f null - 2>&1 | sed -n 's/.*max_volume: \(-*[0-9.]*\) dB/\1/p')
ff -i "$tmp" -af "volume=$(awk "BEGIN { print -3 - ($top) }")dB" -ac 1 -ar 44100 -c:a libvorbis -q:a 4 "$out/kill_0.ogg"
rm -f "$tmp"

ff -ss 0.84 -i "$out/reload_shotgun_0.ogg" \
  -af "afade=t=in:d=0.015,afade=t=out:st=0.25:d=0.05" \
  -ac 1 -ar 44100 -c:a libvorbis -q:a 4 "$out/pump_shotgun_0.ogg"

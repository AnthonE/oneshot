#!/usr/bin/env bash
# The oneshot trailer, drawn in a browser from the game's own footage, its
# own models and its own sounds (after Gates' ci/trailer/make.sh, MIT):
#
#   ci/trailer/make.sh                 # → ci/trailer/out/oneshot-trailer.mp4 (1080p), -720p.mp4,
#                                      #   the poster and the key art beside them
#   ci/trailer/make.sh stills 4.2,21.4 # just those frames, as JPEGs in build/stills (to look at a cut)
#   ci/trailer/make.sh preview         # every 4th frame at 720p, quick to check the whole cut
#   ci/trailer/make.sh art             # the page's still compositions alone: the poster, the key art,
#                                      #   the capsule → out/
#   ci/trailer/make.sh cues            # the scene times and the audio cue list → build/cues.json
#
# trailer.html draws every frame as a pure function of t: render.mjs
# screenshots it in headless Chromium (SwiftShader: no GPU needed) and pipes
# the frames to ffmpeg; mix.py lays the theme, the announcer and the hits on
# the cue list the page exports; sfx.py makes the hits.
#
# Footage: each NN-name.mp4 in SHOTS (default traces/film/trailer, written by
# `ci/film.sh shoot ci/film/trailer-*.json`) becomes clips/<name>/ as JPEG
# frames at 1920x1080. A scene whose clip is missing falls back to a store
# still from scry-forge (FORGE=…, art/oneshot/play-*.jpg) with a slow push,
# so the cut can be looked at before every shot is drawn.
#
# Needs node, ffmpeg, python3 with numpy, Playwright's Chromium
# (/opt/pw-browsers/chromium, CHROMIUM_PATH, or `npx playwright install
# chromium`), and the network once for three.js.
set -euo pipefail
cd "$(dirname "$0")"
HERE="$PWD"
ONESHOT="$(cd ../.. && pwd)"
FORGE="${FORGE:-$ONESHOT/../scry-forge}"
SHOTS="${SHOTS:-$ONESHOT/traces/film/trailer}"
PAGE=trailer.html
fail() { echo "trailer: $*" >&2; exit 1; }
command -v ffmpeg >/dev/null || fail "no ffmpeg"
command -v node >/dev/null || fail "no node"
python3 -c "import numpy" 2>/dev/null || fail "no numpy (pip install numpy)"

MODE=video; STILLS=""
while [ $# -gt 0 ]; do
  case "$1" in
    stills) MODE=stills; STILLS="$2"; shift 2 ;;
    preview) MODE=preview; shift ;;
    art) MODE=art; shift ;;
    cues) MODE=cues; shift ;;
    *) fail "unknown option $1" ;;
  esac
done

B="$HERE/build"
mkdir -p "$B"/{lib,fonts,img,glb,clips,music,snd,voice,sfx,node_modules} "$HERE/out"

# three.js r160 and the glTF loader (the game's GLBs are plain JPEG-textured)
T3=https://cdn.jsdelivr.net/npm/three@0.160.0
for f in build/three.module.js examples/jsm/loaders/GLTFLoader.js examples/jsm/utils/BufferGeometryUtils.js \
         examples/jsm/environments/RoomEnvironment.js; do
  [ -s "$B/lib/$f" ] || { mkdir -p "$(dirname "$B/lib/$f")"; curl -fsSL -o "$B/lib/$f" "$T3/$f"; }
done
# Playwright, from wherever npm has it: the page's renderer imports it beside itself.
if [ ! -e "$B/node_modules/playwright" ]; then
  PW=""
  for d in "$ONESHOT/node_modules/playwright" "$(npm root -g 2>/dev/null)/playwright"; do [ -d "$d" ] && { PW="$d"; break; }; done
  [ -n "$PW" ] || fail "no playwright (npm ci in the repo, or npm i -g playwright)"
  ln -sfn "$PW" "$B/node_modules/playwright"
fi

cp "$PAGE" "$B/trailer.html"
cp render.mjs "$B/"
# the game's own type: Unitblock (the wordmark, the HUD) and Aileron (the menus)
cp "$ONESHOT"/crates/client/assets/fonts/Unitblock.ttf "$ONESHOT"/crates/client/assets/fonts/Aileron-{Regular,SemiBold,Bold}.otf "$B/fonts/"

# the game's models: the guns and the gear, as they ship
M="$ONESHOT/crates/client/assets/models"
for m in ar br magnum sniper shotgun rocket-launcher smg sword carbine beam-rifle ion-rifle ion-pistol shards launcher cannon \
         frag rocket helmet beanie ten-gallon hardhat aviators shades goggles respirator welder skull demon visor \
         hightops-right spurs-right magboots-right loggers-right wellies-right truck turret turret-barrels; do
  [ -s "$M/$m.glb" ] && cp "$M/$m.glb" "$B/glb/" || echo "trailer: no model $m (skipped)" >&2
done

# store stills: the stand-in for any footage not shot yet
for s in bigteam bulkhead chrysalis cistern dryfork fighter square; do
  [ -s "$FORGE/art/oneshot/play-$s.jpg" ] && cp "$FORGE/art/oneshot/play-$s.jpg" "$B/img/" || true
done

# footage: every NN-name.mp4 a shot list wrote, as JPEG frames at 1920x1080
python3 - "$SHOTS" "$B/clips" <<'PY'
import glob, json, os, re, subprocess, sys
src, dst = sys.argv[1], sys.argv[2]
index = {}
for mp4 in sorted(glob.glob(os.path.join(src, "*.mp4"))) if os.path.isdir(src) else []:
    name = re.sub(r"^\d+-", "", os.path.basename(mp4)[:-4])
    out = os.path.join(dst, name)
    stamp = os.path.join(out, ".src")
    if not (os.path.exists(stamp) and open(stamp).read() == str(os.path.getmtime(mp4))):
        os.makedirs(out, exist_ok=True)
        for f in glob.glob(os.path.join(out, "*.jpg")):
            os.remove(f)
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", mp4, "-vf",
                        "scale=1920:1080:flags=lanczos", "-q:v", "2",
                        os.path.join(out, "%03d.jpg")], check=True)
        open(stamp, "w").write(str(os.path.getmtime(mp4)))
    index[name] = len(glob.glob(os.path.join(out, "*.jpg")))
# a still (NN-name.png, a shot with `still: true`) is a one-frame clip
for png in sorted(glob.glob(os.path.join(src, "*.png"))) if os.path.isdir(src) else []:
    name = re.sub(r"^\d+-", "", os.path.basename(png)[:-4])
    out = os.path.join(dst, name)
    stamp = os.path.join(out, ".src")
    if not (os.path.exists(stamp) and open(stamp).read() == str(os.path.getmtime(png))):
        os.makedirs(out, exist_ok=True)
        for f in glob.glob(os.path.join(out, "*.jpg")):
            os.remove(f)
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", png, "-vf", "scale=1920:1080:flags=lanczos", "-q:v", "2",
                        os.path.join(out, "001.jpg")], check=True)
        open(stamp, "w").write(str(os.path.getmtime(png)))
    index[name] = 1
json.dump(index, open(os.path.join(dst, "index.json"), "w"))
print("trailer: footage", index or "(none yet: store stills stand in)")
PY

# what the mix plays: the theme, the game's own recordings, the announcer, the trailer hits
S="$ONESHOT/crates/client/assets/sound"
cp "$S/music/intro.ogg" "$B/music/"
for s in shot_sniper_0 shot_sniper_far_0 headshot_0 headshot_1 shield_pop_0 shield_hit_0 kill_0 death_0 scope_in_0 scope_out_0 \
         shot_br_0 shot_br_1 shot_ar_0 shot_rifle_0 shot_magnum_0 shot_shotgun_0 shot_rocket_0 shot_smg_0 sword_swing_0 sword_on_0 \
         shot_carbine_0 shot_beam_0 shot_ion_0 shot_ionp_0 shot_ion_charged_0 shot_shards_0 shot_launcher_0 shot_cannon_0 shot_minigun_0 \
         blast_0 blast_1 rocket_fly_0 reload_br_0 reload_sniper_0 swap_0 swap_1 pickup_0 pickup_1 launch_0 jump_0 land_0 \
         engine_0 ride_0 skid_0 crash_0 truck_land_0 flyby_0 flyby_1 ricochet_0 bullet_metal_0 impact_metal_0 melee_hit_0 \
         match_found_0 go_0 ui_click_0 ui_hover_0 tick_0 over_on_0 camo_on_0 spawn_0 sticky_stick_0 grenades_0 throw_0 \
         room_hum_0 wind_0 surf_0 vent_0 charge_hold_0 shield_charge_0 shield_beep_0 step_metal_0 step_concrete_0; do
  [ -s "$S/$s.ogg" ] && cp "$S/$s.ogg" "$B/snd/" || true
done
for v in slayer team_slayer free_for_all gun_game capture_the_flag double_kill triple_kill overkill killstorm killing_spree \
         running_riot rampage untouchable unstoppable extermination perfection game_over game_over_you_win taken_lead one_minute; do
  [ -s "$S/voice/$v.ogg" ] && cp "$S/voice/$v.ogg" "$B/voice/" || true
done
[ -s "$B/sfx/trailer_boom.wav" ] || python3 sfx.py "$B/sfx" >/dev/null

PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
python3 -m http.server "$PORT" --bind 127.0.0.1 -d "$B" >/dev/null 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null' EXIT
export TRAILER_URL="http://127.0.0.1:$PORT/trailer.html" TRAILER_Q="${TRAILER_Q:-}"
sleep 0.5

case "$MODE" in
stills)
  node "$B/render.mjs" stills "$B/stills" "$STILLS"
  echo "trailer: frames in $B/stills"; exit 0 ;;
cues)
  node "$B/render.mjs" cues "$B/cues.json"
  python3 -c "import json; t = json.load(open('$B/cues.json')); print('trailer: %.1f s' % t['TOTAL']); [print('  %6.2f  %s' % (s['t0'], s['id'])) for s in t['SCENES']]"
  exit 0 ;;
art)
  node "$B/render.mjs" art "$HERE/out" poster,keyart,capsule,square,scope
  # the poster as the store's JPEG beside the PNGs
  ffmpeg -v error -y -i "$HERE/out/poster.png" -q:v 2 "$HERE/out/oneshot-trailer-poster.jpg"
  ls -la "$HERE"/out/; exit 0 ;;
esac

node "$B/render.mjs" cues "$B/cues.json"
python3 mix.py "$B" "$B/cues.json" "$B/mix.wav"
# loudness to -14 LUFS, two passes so the level is linear rather than pumped
LN=$(ffmpeg -hide_banner -nostats -i "$B/mix.wav" -af loudnorm=I=-14:TP=-1.5:LRA=11:print_format=json -f null - 2>&1 \
  | python3 -c "import sys, json; t = sys.stdin.read(); j = json.loads(t[t.rindex('{'):]); print(':'.join(f'{k}={j[v]}' for k, v in (('measured_I', 'input_i'), ('measured_TP', 'input_tp'), ('measured_LRA', 'input_lra'), ('measured_thresh', 'input_thresh'), ('offset', 'target_offset'))))")
AF="loudnorm=I=-14:TP=-1.5:LRA=11:$LN:linear=true"

if [ "$MODE" = preview ]; then
  TRAILER_STEP=4 node "$B/render.mjs" video "$B/preview.mkv"
  ffmpeg -v error -y -i "$B/preview.mkv" -i "$B/mix.wav" -map 0:v -map 1:a -af "$AF" -ar 48000 \
    -vf scale=1280:720 -r 30 -c:v libx264 -preset veryfast -crf 26 -pix_fmt yuv420p -c:a aac -b:a 160k -shortest \
    "$HERE/out/oneshot-trailer-preview.mp4"
  ls -la "$HERE"/out/; exit 0
fi

# In ten-second parts, each its own browser, kept until the cut changes: a
# render is half an hour, and a part already drawn is not drawn again when
# a run is cut short. A changed page or footage index starts the parts over.
STAMP=$(cat "$PAGE" "$B/clips/index.json" | sha256sum | cut -c1-16)
[ "$(cat "$B/parts/.stamp" 2>/dev/null)" = "$STAMP" ] || { rm -rf "$B/parts"; mkdir -p "$B/parts"; echo "$STAMP" > "$B/parts/.stamp"; }
TOTAL=$(python3 -c "import json; print(json.load(open('$B/cues.json'))['TOTAL'])")
N=$(python3 -c "import math; print(math.ceil($TOTAL / 10))")
: > "$B/parts/list.txt"
for i in $(seq 0 $((N - 1))); do
  part="$B/parts/$(printf %02d "$i").mkv"
  if [ ! -s "$part" ]; then
    node "$B/render.mjs" video "$B/parts/tmp.mkv" $((i * 10)) $(python3 -c "print(min($TOTAL, ($i + 1) * 10))")
    mv "$B/parts/tmp.mkv" "$part"
  fi
  echo "file '$part'" >> "$B/parts/list.txt"
done
ffmpeg -v error -y -f concat -safe 0 -i "$B/parts/list.txt" -c copy "$B/silent.mkv"
ffmpeg -v error -y -i "$B/silent.mkv" -i "$B/mix.wav" -map 0:v -map 1:a -af "$AF" -ar 48000 \
  -c:v libx264 -preset slow -crf 19 -maxrate 8M -bufsize 16M -pix_fmt yuv420p -c:a aac -b:a 192k -movflags +faststart -shortest \
  "$HERE/out/oneshot-trailer.mp4"
ffmpeg -v error -y -i "$HERE/out/oneshot-trailer.mp4" -vf scale=1280:720:flags=lanczos -c:v libx264 -preset slow -crf 23 -maxrate 3M -bufsize 6M \
  -pix_fmt yuv420p -c:a copy -movflags +faststart "$HERE/out/oneshot-trailer-720p.mp4"
node "$B/render.mjs" art "$HERE/out" poster,keyart,capsule,square,scope
ffmpeg -v error -y -i "$HERE/out/poster.png" -q:v 2 "$HERE/out/oneshot-trailer-poster.jpg"
ls -la "$HERE"/out/

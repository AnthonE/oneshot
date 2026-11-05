#!/usr/bin/env bash
# tools/ground.sh: the ground's and the rock's photographs, cut from Poly
# Haven's CC0 sets (polyhaven.com; no attribution required, credited in
# NOTICE by choice). Each set is fetched once at 2K into sources/polyhaven
# (git-ignored: a download) and cut to what ships, Gates' encode: 1024²,
# the colour at q82, the normal (OpenGL's green) at q90, and for the rock,
# which the boxes wear too, its roughness and occlusion.
#
#   tools/ground.sh            fetch what is missing, cut all four
#   tools/ground.sh rock       just that one
#
# The size each photograph was taken at is `GROUND_TILE_M` in
# crates/client/src/look/textures.rs: change both. tools/assets.mjs no
# longer copies these four from Gates. Needs curl, python3 and ImageMagick.
set -euo pipefail
cd "$(dirname "$0")/.."

# our name | Poly Haven's id | what it is
SETS=(
  "sand|gravelly_sand|warm desert sand with grit, 2.48 m"
  "grass|grass_ground|short turf worn to soil in places, 2.51 m"
  "gravel|rocky_trail|stones and pebbles in dirt, 2.0 m"
  "rock|cliff_side|bedded sandstone, its strata level, 1.83 m"
)
src=sources/polyhaven
out=crates/client/assets/textures
px=1024

fetch() { # id
  local id=$1 d=$src/$1
  [ -s "$d/${id}_diff_2k.jpg" ] && [ -s "$d/${id}_nor_gl_2k.jpg" ] && [ -s "$d/${id}_rough_2k.jpg" ] && [ -s "$d/${id}_ao_2k.jpg" ] && return 0
  mkdir -p "$d"
  curl -fsS -m 60 -A oneshot-ground/1 "https://api.polyhaven.com/files/$id" -o "$d/files.json"
  python3 - "$d/files.json" <<'E' | while read -r name url; do curl -fsS -m 300 -A oneshot-ground/1 "$url" -o "$d/$name"; done
import json, sys
f = json.load(open(sys.argv[1]))
for m, n in [('Diffuse', 'diff'), ('nor_gl', 'nor_gl'), ('Rough', 'rough'), ('AO', 'ao')]:
    u = f[m]['2k']['jpg']['url']
    print(u.rsplit('/', 1)[1], u)
E
}

for row in "${SETS[@]}"; do
  IFS='|' read -r name id what <<<"$row"
  if [ $# -gt 0 ] && [[ ! " $* " =~ " $name " ]]; then continue; fi
  fetch "$id"
  d=$src/$id
  convert "$d/${id}_diff_2k.jpg" -filter Lanczos -resize ${px}x${px} -strip -quality 82 "$out/${name}_albedo.jpg"
  convert "$d/${id}_nor_gl_2k.jpg" -filter Lanczos -resize ${px}x${px} -strip -quality 90 "$out/${name}_normal.jpg"
  if [ "$name" = rock ]; then
    convert "$d/${id}_rough_2k.jpg" -filter Lanczos -resize ${px}x${px} -colorspace Gray -strip -quality 82 "$out/${name}_rough.jpg"
    convert "$d/${id}_ao_2k.jpg" -filter Lanczos -resize ${px}x${px} -colorspace Gray -strip -quality 82 "$out/${name}_ao.jpg"
  fi
  # The photograph's mean in linear light: what the tints in look/ground.rs are set against.
  mean=$(convert "$out/${name}_albedo.jpg" -colorspace RGB -resize 1x1 -format '%[fx:r] %[fx:g] %[fx:b]' info:)
  printf '%-7s %-16s linear mean %s   (%s)\n' "$name" "$id" "$mean" "$what"
done

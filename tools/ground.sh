#!/usr/bin/env bash
# tools/ground.sh: the ground's and the rock's photographs, cut from Poly
# Haven's CC0 sets (polyhaven.com; no attribution required, credited in
# NOTICE by choice). Each set is fetched once at 2K into sources/polyhaven
# (git-ignored: a download) and cut to what ships, Gates' encode: 1024²,
# the colour at q82, the normal (OpenGL's green) at q90, and for the rock,
# which the boxes wear too, its roughness and occlusion.
#
#   tools/ground.sh            fetch what is missing, cut them all
#   tools/ground.sh rock       just that one
#
# The size each photograph was taken at is `GROUND_TILE_M` in
# crates/client/src/look/textures.rs: change both. tools/assets.mjs no
# longer copies these four from Gates. Needs curl, python3 and ImageMagick.
#
# And the bases' metal, the same way from ambientCG (ambientcg.com, CC0):
# `steel` (plates with seams and bolts) and `tread` (tread
# plate, for a deck or a ramp), each with its
# roughness and occlusion as the boxes' sets have them (`textures::BOXES`).
#
#   tools/ground.sh steel tread
set -euo pipefail
cd "$(dirname "$0")/.."

# our name | Poly Haven's id | what it is
SETS=(
  "sand|gravelly_sand|warm desert sand with grit, 2.48 m"
  "grass|grass_ground|short turf worn to soil in places, 2.51 m"
  "gravel|rocky_trail|stones and pebbles in dirt, 2.0 m"
  "rock|cliff_side|bedded sandstone, its strata level, 1.83 m"
  "isle_rock|old_sandstone_02|an island's wind-cut sandstone, rounded beds and ledges"
  "stone|large_sandstone_blocks_01|Cloudbreak's masonry, aged blocks with chiselled faces, greyed"
  "brick|brick_wall_001|Cistern's vaults, old brick, red and black"
  "marble|marble_01|Cistern's columns, pale cut marble"
  "road|asphalt_02|Overpass' carriageway, cracked asphalt"
  "blast|brushed_concrete_2|cast concrete, board-marked and stained: Overpass' and Proving Grounds' structures"
  "snow|snow_floor|Proving Grounds' floor, wind-crusted snow, fine-grained"
  "rust|rusty_metal_02|flaking paint over rust: signs, shutters, old steel"
  "drift|snow_01|Pithead's ground, snow trodden over by boots"
  "ice_rock|marble_cliff_05|Pithead's rock, grey and fractured, cut cold"
)
# our name | ambientCG's id | what it is
METAL=(
  "steel|MetalPlates001|steel plates a quarter of the tile across, staggered, bolted at their corners"
  "tread|DiamondPlate008C|worn tread plate"
)
src=${POLYHAVEN_SRC:-sources/polyhaven}
acg=${AMBIENTCG_SRC:-sources/ambientcg}
out=crates/client/assets/textures
px=1024

fetch() { # id
  local id=$1 d=$src/$1
  [ -s "$d/${id}_diff_2k.jpg" ] && [ -s "$d/${id}_nor_gl_2k.jpg" ] && [ -s "$d/${id}_rough_2k.jpg" ] && [ -s "$d/${id}_ao_2k.jpg" ] && return 0
  mkdir -p "$d"
  curl -fsS -m 60 -A oneshot-ground/1 "https://api.polyhaven.com/files/$id" -o "$d/files.json"
  python3 - "$d/files.json" "$id" <<'E' | while read -r name url; do curl -fsS -m 300 -A oneshot-ground/1 "$url" -o "$d/$name"; done
import json, sys
f = json.load(open(sys.argv[1]))
for m, n in [('Diffuse', 'diff'), ('nor_gl', 'nor_gl'), ('Rough', 'rough'), ('AO', 'ao')]:
    u = f[m]['2k']['jpg']['url']
    # Saved by what it is: some sets name their diffuse `_diffuse_`, not `_diff_`.
    print(f'{sys.argv[2]}_{n}_2k.jpg', u)
E
}

for row in "${SETS[@]}"; do
  IFS='|' read -r name id what <<<"$row"
  if [ $# -gt 0 ] && [[ ! " $* " =~ " $name " ]]; then continue; fi
  fetch "$id"
  d=$src/$id
  # The masonry is cut grey: warm sand against cool stone is the island's contrast.
  grade=()
  [ "$name" = stone ] && grade=(-modulate 118,22,100)
  # The cold rock is cut grey too: its tint (`ground::ICE_TINT`) gives it the frost's blue.
  [ "$name" = ice_rock ] && grade=(-modulate 100,12,100)
  convert "$d/${id}_diff_2k.jpg" -filter Lanczos -resize ${px}x${px} "${grade[@]}" -strip -quality 82 "$out/${name}_albedo.jpg"
  convert "$d/${id}_nor_gl_2k.jpg" -filter Lanczos -resize ${px}x${px} -strip -quality 90 "$out/${name}_normal.jpg"
  if [[ " rock stone brick marble road blast snow rust " =~ " $name " ]]; then
    convert "$d/${id}_rough_2k.jpg" -filter Lanczos -resize ${px}x${px} -colorspace Gray -strip -quality 82 "$out/${name}_rough.jpg"
    convert "$d/${id}_ao_2k.jpg" -filter Lanczos -resize ${px}x${px} -colorspace Gray -strip -quality 82 "$out/${name}_ao.jpg"
  fi
  # The photograph's mean in linear light: what the tints in look/ground.rs are set against.
  mean=$(convert "$out/${name}_albedo.jpg" -colorspace RGB -resize 1x1 -format '%[fx:r] %[fx:g] %[fx:b]' info:)
  printf '%-7s %-16s linear mean %s   (%s)\n' "$name" "$id" "$mean" "$what"
done

for row in "${METAL[@]}"; do
  IFS='|' read -r name id what <<<"$row"
  if [ $# -gt 0 ] && [[ ! " $* " =~ " $name " ]]; then continue; fi
  d=$acg/${id}_1K-JPG
  if [ ! -s "$d/${id}_1K-JPG_Color.jpg" ]; then
    mkdir -p "$d"
    curl -fsSL -m 300 -A oneshot-ground/1 "https://ambientcg.com/get?file=${id}_1K-JPG.zip" -o "$d.zip"
    unzip -oq "$d.zip" -d "$d" && rm "$d.zip"
  fi
  f=$d/${id}_1K-JPG
  convert "${f}_Color.jpg" -filter Lanczos -resize ${px}x${px} -strip -quality 84 "$out/${name}_albedo.jpg"
  convert "${f}_NormalGL.jpg" -filter Lanczos -resize ${px}x${px} -strip -quality 90 "$out/${name}_normal.jpg"
  convert "${f}_Roughness.jpg" -filter Lanczos -resize ${px}x${px} -colorspace Gray -strip -quality 84 "$out/${name}_rough.jpg"
  # A set with no occlusion map of its own has none to speak of.
  if [ -s "${f}_AmbientOcclusion.jpg" ]; then
    convert "${f}_AmbientOcclusion.jpg" -filter Lanczos -resize ${px}x${px} -colorspace Gray -strip -quality 84 "$out/${name}_ao.jpg"
  else
    convert -size ${px}x${px} xc:white -colorspace Gray -strip -quality 84 "$out/${name}_ao.jpg"
  fi
  mean=$(convert "$out/${name}_albedo.jpg" -colorspace RGB -resize 1x1 -format '%[fx:r] %[fx:g] %[fx:b]' info:)
  rough=$(convert "$out/${name}_rough.jpg" -resize 1x1 -format '%[fx:r]' info:)
  printf '%-7s %-16s linear mean %s   roughness %s   (%s)\n' "$name" "$id" "$mean" "$rough" "$what"
done

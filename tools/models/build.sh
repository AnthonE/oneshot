#!/bin/sh
# Every model: each tools/models/<name>.py builds its GLB into
# crates/client/assets/models and its sheet into traces/guns (lib.py). A
# generated one needs its raw mesh in sources/meshy (meshy_gen.py <name>).
#   tools/models/build.sh            all
#   tools/models/build.sh ar smg     some
#   tools/models/build.sh truck      by name only: it needs its download in
#                                    sources/ (truck.py)
set -eu
cd "$(dirname "$0")/../.."
# A model's script calls finish (rocks.py, the boulders and the sea stack,
# exports its own); the kit (lib.py, which defines finish) and the lines'
# shared parts don't.
if [ $# -gt 0 ]; then names="$*"; else names=$(grep -lE 'finish\(|^BOULDERS|^HANDS|^CLIPS|^WIDTH|^LENGTH' tools/models/*.py | grep -v '/lib\.py$' | xargs -n1 basename | sed 's/\.py$//'); fi
# Blender held to a share of the box, and the kernel's first pick if it
# runs out anyway: the box has no swap, and a mesh grown too fine has taken
# 13 GB in the exporter (MODEL_MAX, default 6G).
capped() {
  if command -v systemd-run >/dev/null; then
    systemd-run --user --scope --quiet -p MemoryMax="${MODEL_MAX:-6G}" -p MemorySwapMax=0 -- \
      sh -c 'echo 900 > /proc/self/oom_score_adj; exec "$@"' model "$@"
  else
    "$@"
  fi
}
for n in $names; do
  echo "== $n"
  PYTHONUNBUFFERED=1 capped blender -b --python "tools/models/$n.py" 2>&1 | grep -E '^(MODEL|SHEET|GLOW|FIT|SCRUB|BLOT|LIGHT|PAINT|SHADE|GLOSS)|Error|Traceback|line [0-9]+|Killed' || true
done

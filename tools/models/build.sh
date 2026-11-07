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
for n in $names; do
  echo "== $n"
  blender -b --python "tools/models/$n.py" 2>&1 | grep -E '^(MODEL|SHEET|GLOW|FIT|SCRUB|BLOT)|Error|Traceback|line [0-9]+' || true
done

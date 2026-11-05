#!/bin/sh
# Every model: each tools/models/<name>.py builds its GLB into
# crates/client/assets/models and its sheet into traces/guns (lib.py). A
# generated one needs its raw mesh in sources/meshy (meshy_gen.py <name>).
#   tools/models/build.sh            all
#   tools/models/build.sh ar smg     some
set -eu
cd "$(dirname "$0")/../.."
# A model's script calls finish; the kit and the lines' shared parts don't.
if [ $# -gt 0 ]; then names="$*"; else names=$(grep -l 'finish(' tools/models/*.py | xargs -n1 basename | sed 's/\.py$//'); fi
for n in $names; do
  echo "== $n"
  blender -b --python "tools/models/$n.py" 2>&1 | grep -E '^(MODEL|SHEET|GLOW)|Error|Traceback|line [0-9]+' || true
done

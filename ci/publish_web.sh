#!/usr/bin/env bash
# Publish the page (the Bevy client's browser build, crates/client/dist) to
# elopros.com/games/oneshot/play/, after Gates' ci/publish_web.sh. Run from a
# workstation that can `ssh morr`:
#
#   ci/publish_web.sh             # build, upload, flip `current`
#   ci/publish_web.sh --no-build  # upload what crates/client/dist already holds
#   ci/publish_web.sh --dry-run   # everything but the upload and the flip
#   ci/publish_web.sh --strand    # publish a page the live server will refuse
#
# Every publish lands whole in <root>/<build>/ and the page served is
# <root>/current, a symlink flipped last: a half upload is never served, and a
# rollback is one `ln -sfn`. --link-dest hard-links what didn't change.
set -euo pipefail
cd "$(dirname "$0")/.."

HOST=morr
ROOT=/data/apps/scry-data/oneshot-web
SERVER=https://game.elopros.com/oneshot/
BUILD=1
DRY=0
STRAND=0
while [ $# -gt 0 ]; do
  case "$1" in
    --no-build) BUILD=0 ;;
    --dry-run) DRY=1 ;;
    --strand) STRAND=1 ;;
    --host) HOST="$2"; shift ;;
    --root) ROOT="$2"; shift ;;
    --server) SERVER="$2"; shift ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
  shift
done

# A page on another wire is refused at every hello: ask the live server first.
want=$(grep -oP 'pub const PROTO_VER: u16 = \K[0-9]+' crates/proto/src/lib.rs)
live=$(curl -s --max-time 15 "${SERVER}ready" | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("proto",""))
except Exception: print("")')
if [ "$live" = "$want" ]; then
  echo "== wire ok: the server speaks proto $live, and so does this page"
elif [ "$STRAND" = 1 ]; then
  echo "== !! the server speaks proto ${live:-<unreported>} and this page $want: publishing anyway (--strand)"
else
  echo "the server (${SERVER}ready) speaks proto ${live:-<unreported>} and this page $want:" >&2
  echo "every join would be refused. Deploy the server first (ci/deploy_server.sh), or pass --strand." >&2
  exit 1
fi

OUT=crates/client/dist
if [ "$BUILD" = 1 ]; then
  ci/client.sh
fi
[ -f "$OUT/index.html" ] || { echo "$OUT/index.html is missing: run ci/client.sh" >&2; exit 1; }
# Both modules, from the gated build: the page picks WebGPU or WebGL2 by
# build.json, and `ci/client.sh dev` skips the fat LTO and the 9 MB gate.
for f in oneshot_client.js oneshot_client_bg.wasm oneshot_client_gpu.js oneshot_client_gpu_bg.wasm build.json; do
  [ -f "$OUT/$f" ] || { echo "$OUT/$f is missing: run ci/client.sh" >&2; exit 1; }
done
grep -q '"profile":"web"' "$OUT/build.json" || {
  echo "$OUT is a dev build (ci/client.sh dev): run ci/client.sh for the gated one" >&2; exit 1; }
# Where the page finds the game box: server.json and /nonce (its index.html).
printf '{"server":"%s"}\n' "$SERVER" > "$OUT/origin.json"
# nginx serves .gz beside a file when there is one (gzip_static).
find "$OUT" -type f \( -name '*.wasm' -o -name '*.js' -o -name '*.css' -o -name '*.html' -o -name '*.json' \) \
  -exec gzip -kf9 {} \;

sha=$(git rev-parse --short=12 HEAD)
dirty=$(git diff --quiet HEAD -- . && echo "" || echo "-dirty")
ID="p${want}-g${sha}${dirty}"
DEST="$ROOT/$ID"
echo "== build $ID: $(du -sh "$OUT" | cut -f1), $(find "$OUT" -type f | wc -l) files"
echo "== to $HOST:$DEST (current -> $ID after the upload)"
if [ "$DRY" = 1 ]; then
  echo "dry run: nothing uploaded, nothing flipped"
  exit 0
fi
ssh -o BatchMode=yes "$HOST" "mkdir -p '$ROOT'"
rsync -a --info=stats1 --link-dest="../current" "$OUT/" "$HOST:$DEST/"
ssh -o BatchMode=yes "$HOST" "ln -sfn '$ID' '$ROOT/current.new' && mv -T '$ROOT/current.new' '$ROOT/current' && ls -l '$ROOT/current'"
echo "== live: https://elopros.com/games/oneshot/play/"

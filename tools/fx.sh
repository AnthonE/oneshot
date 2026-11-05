#!/bin/sh
# The fx stage in the browser: one effect at a time in front of a dark card
# and a light card, a frame per step on a clock that steps 1/60 s, each
# composed into a strip with its numbers and a diff against the last run
# (look/fx/stage.rs, tools/fxstrip.mjs, the fx skill).
#
#   tools/fx.sh list                                   # every effect it can stage
#   tools/fx.sh hit-bullet-metal grenade               # → traces/fx/<name>/strip.png, numbers.json, diff.png
#   tools/fx.sh hit-bullet-metal --knobs 'Sparks.speed=8,16;Flash.life=0.03,0.05'   # retune, no rebuild
#   tools/fx.sh grenade --seed 7 --frames 15 --span 1.5
#   tools/fx.sh --no-build …                           # reuse the last page
#
# Builds the page in release (thin LTO: minutes, where ci/client.sh's web
# profile is far longer) into crates/client/target/fx-dist and serves it
# itself; no game server. Needs wasm-bindgen-cli at the lock's version and
# Playwright's Chromium (npm ci).
set -eu
cd "$(dirname "$0")/.."
OUT=${FX_OUT:-traces/fx}
BUILD=1
NAMES=""
KNOBS=""
SEED=0
MORE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --no-build) BUILD=0 ;;
    --knobs) KNOBS=$2; shift ;;
    --seed) SEED=$2; shift ;;
    --frames|--span|--size|--steps) MORE="$MORE $1 $2"; shift ;;
    -h|--help) sed -n '2,18p' "$0"; exit 0 ;;
    *) NAMES="$NAMES $1" ;;
  esac
  shift
done
[ -n "$NAMES" ] || { sed -n '2,18p' "$0"; exit 2; }
if [ "$NAMES" = " list" ]; then
  (cd crates/client && cargo run -q --example fx_list)
  exit 0
fi
DIST=crates/client/target/fx-dist
if [ "$BUILD" = 1 ]; then
  (cd crates/client && cargo build -q --release --lib --target wasm32-unknown-unknown)
  want=$(grep -A1 '^name = "wasm-bindgen"$' crates/client/Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p' | head -1)
  have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
  if [ "$want" != "$have" ]; then
    echo "fx: wasm-bindgen $want is needed (have: ${have:-none}): cargo install wasm-bindgen-cli --version $want --locked" >&2
    exit 1
  fi
  rm -rf "$DIST"
  wasm-bindgen --target web --out-dir "$DIST" crates/client/target/wasm32-unknown-unknown/release/oneshot_client.wasm
  # The page and its scripts (boot.js starts it), as ci/client.sh copies them.
  cp crates/client/web/*.html crates/client/web/*.js "$DIST/"
  ln -s ../../assets "$DIST/assets"
fi
[ -f "$DIST/oneshot_client_bg.wasm" ] || { echo "fx: no page at $DIST: run without --no-build" >&2; exit 1; }
K=$(node -p 'encodeURIComponent(process.argv[1])' "$KNOBS")
for n in $NAMES; do
  # shellcheck disable=SC2086 # MORE is flags and their values
  node tools/fxstrip.mjs page "$OUT/$n" --serve "$DIST" --hook fx \
    --query "pose=1&fx=$n&seed=$SEED&knobs=$K" $MORE
done

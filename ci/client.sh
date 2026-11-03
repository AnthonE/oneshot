#!/bin/sh
# The Bevy client (crates/client, its own workspace): clippy and a native
# build, then the browser build into crates/client/dist, which the server
# serves by default:
#   target/release/oneshot-server
# Needs wasm-bindgen-cli at the version in crates/client/Cargo.lock.
# The browser build is the `web` profile (Cargo.toml says why), shrunk by
# binaryen's wasm-opt when there is one (WASM_OPT=path, or wasm-opt on the
# PATH; WASM_OPT=npx runs the pinned npm build: 14 min here, for 1% of
# the gzipped bytes, measured 2026-11-03, so it is off by default), and its
# gzipped size must stay under WASM_GZ_BUDGET_MB.
set -eu
WASM_GZ_BUDGET_MB=${WASM_GZ_BUDGET_MB:-11}
# PROFILE=release: a quicker browser build to iterate on (thin LTO; the
# budget is the web profile's, so it may fail).
PROFILE=${PROFILE:-web}
cd "$(dirname "$0")/../crates/client"
cargo fmt
cargo clippy -q --all-targets -- -D warnings
cargo build -q
cargo build -q --profile "$PROFILE" --lib --target wasm32-unknown-unknown
want=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p' | head -1)
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
if [ "$want" != "$have" ]; then
  echo "wasm-bindgen $want is needed (have: ${have:-none}): cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi
rm -rf dist
wasm-bindgen --target web --remove-name-section --remove-producers-section \
  --out-dir dist "target/wasm32-unknown-unknown/$PROFILE/oneshot_client.wasm"
m=dist/oneshot_client_bg.wasm
opt=${WASM_OPT:-$(command -v wasm-opt || true)}
if [ "$opt" = npx ]; then
  opt="npx -y -p binaryen@132.0.0 wasm-opt"
fi
if [ -n "$opt" ]; then
  before=$(stat -c%s $m)
  $opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int --enable-mutable-globals \
    --enable-sign-ext --enable-reference-types --enable-multivalue -o $m.opt $m
  mv $m.opt $m
  echo "wasm-opt: $before -> $(stat -c%s $m) bytes"
else
  echo "wasm-opt: none (WASM_OPT=npx for binaryen from npm); shipping wasm-bindgen's output"
fi
raw=$(stat -c%s $m)
gz=$(gzip -9 -n -c $m | wc -c)
echo "oneshot_client_bg.wasm: $raw bytes, $gz gzipped (budget ${WASM_GZ_BUDGET_MB} MB)"
if [ "$gz" -gt $((WASM_GZ_BUDGET_MB * 1024 * 1024)) ]; then
  echo "the browser build is over its gzipped budget" >&2
  exit 1
fi
cp web/index.html dist/
cp -r assets dist/assets
du -sh dist

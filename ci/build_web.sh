#!/bin/sh
# The page: client-net to wasm, wasm-bindgen's glue into web/src/pkg, then
# Vite into web/dist (which the server serves).
# Needs wasm-bindgen-cli at the version in Cargo.lock:
#   cargo install wasm-bindgen-cli --version <it> --locked
set -eu
cd "$(dirname "$0")/.."
want=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p' | head -1)
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
if [ "$want" != "$have" ]; then
  echo "wasm-bindgen $want is needed (have: ${have:-none}): cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi
cargo build -q --release -p client-net --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir web/src/pkg target/wasm32-unknown-unknown/release/client_net.wasm
cd web
[ -d node_modules ] || npm ci --silent
# three's basis transcoder, for the models' KTX2 textures.
mkdir -p public/basis
cp node_modules/three/examples/jsm/libs/basis/basis_transcoder.js node_modules/three/examples/jsm/libs/basis/basis_transcoder.wasm public/basis/
[ -f public/models/triple-t.glb ] || echo "no models in web/public: node tools/assets.mjs <Gates checkout>" >&2
npx tsc
npx vite build --logLevel warn
du -sh dist

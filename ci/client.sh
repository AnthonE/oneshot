#!/bin/sh
# The Bevy client (crates/client, its own workspace): clippy and a native
# build, then the browser build into crates/client/dist, which the server
# serves like web/dist:
#   target/release/oneshot-server --web crates/client/dist
# Needs wasm-bindgen-cli at the version in crates/client/Cargo.lock.
set -eu
cd "$(dirname "$0")/../crates/client"
cargo fmt
cargo clippy -q --all-targets -- -D warnings
cargo build -q
cargo build -q --release --lib --target wasm32-unknown-unknown
want=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p' | head -1)
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
if [ "$want" != "$have" ]; then
  echo "wasm-bindgen $want is needed (have: ${have:-none}): cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi
rm -rf dist
wasm-bindgen --target web --out-dir dist target/wasm32-unknown-unknown/release/oneshot_client.wasm
cp web/index.html dist/
du -sh dist

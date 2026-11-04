#!/bin/sh
# The Bevy client (crates/client, its own workspace): clippy and a native
# build, then the browser build into crates/client/dist, which the server
# serves by default:
#   target/release/oneshot-server --web crates/client/dist
# The browser's build is its own profile (crates/client/Cargo.toml: small,
# fat LTO) without wasm's name section: ~31 MB, ~8 MB gzipped, against 95
# MB for a plain release build. Needs wasm-bindgen-cli at the version in
# crates/client/Cargo.lock.
set -eu
cd "$(dirname "$0")/../crates/client"
cargo fmt
cargo clippy -q --all-targets -- -D warnings
# Its own tests (textures, the pointer lock): not run anywhere else.
cargo test -q --lib --bins
cargo build -q
cargo build -q --profile web --lib --target wasm32-unknown-unknown
want=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p' | head -1)
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
if [ "$want" != "$have" ]; then
  echo "wasm-bindgen $want is needed (have: ${have:-none}): cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi
rm -rf dist
wasm-bindgen --target web --remove-name-section --remove-producers-section --out-dir dist \
  target/wasm32-unknown-unknown/web/oneshot_client.wasm
# The size gate (docs/DESIGN.md §2): what a player downloads, gzipped.
gz=$(gzip -9c dist/oneshot_client_bg.wasm | wc -c)
echo "wasm: $(wc -c < dist/oneshot_client_bg.wasm) bytes, $gz gzipped"
if [ "$gz" -gt 9000000 ]; then
  echo "the wasm is $gz bytes gzipped, past the 9 MB gate" >&2
  exit 1
fi
# What the page's download bar counts to: the module's own size (a
# published page serves it gzipped, so Content-Length isn't it).
printf '{"wasm_bytes":%d,"wasm_gz_bytes":%d,"commit":"%s"}\n' \
  "$(wc -c < dist/oneshot_client_bg.wasm)" "$gz" "$(git rev-parse --short HEAD 2>/dev/null || echo unknown)" \
  > dist/build.json
cp web/index.html dist/
cp -r assets dist/assets
du -sh dist

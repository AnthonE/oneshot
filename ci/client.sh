#!/bin/sh
# The Bevy client (crates/client, its own workspace).
#
#   ci/client.sh       the check: fmt, clippy (native and both browser
#                      modules), its tests, a native build, then the page
#                      into crates/client/dist, each module under the gate
#   ci/client.sh dev   just the page, quickly: release (thin LTO, where the
#                      web profile is fat), no size gate; both modules,
#                      bound and proved, into dist all the same
#
# The page is two wasm modules (after Gates' ci/build_web.sh, "the game,
# twice"): Bevy picks its GPU API when it is compiled, so `oneshot_client`
# draws through WebGL2 and `oneshot_client_gpu` (`--features webgpu`)
# through WebGPU, the desktop's frame. web/index.html asks the browser for
# a WebGPU adapter before either downloads and fetches the one it can draw
# with. The server serves dist by default:
#   target/release/oneshot-server --web crates/client/dist
# The browser's build is its own profile (crates/client/Cargo.toml: small,
# fat LTO) without wasm's name section: ~31 MB, ~8 MB gzipped a module,
# against 95 MB for a plain release build. Needs wasm-bindgen-cli at the
# version in crates/client/Cargo.lock.
set -eu
mode=${1:-check}
case "$mode" in
  check|dev) ;;
  *) echo "usage: ci/client.sh [dev]" >&2; exit 2 ;;
esac
cd "$(dirname "$0")/../crates/client"
want=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p' | head -1)
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
if [ "$want" != "$have" ]; then
  echo "wasm-bindgen $want is needed (have: ${have:-none}): cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi
cargo fmt
if [ "$mode" = check ]; then
  cargo clippy -q --all-targets -- -D warnings
  # Each module compiles its own side of `cfg(webgl2)` (build.rs).
  cargo clippy -q --lib --target wasm32-unknown-unknown -- -D warnings
  cargo clippy -q --lib --target wasm32-unknown-unknown --features webgpu -- -D warnings
  # Its own tests (textures, the pointer lock, the GPU tiers): not run anywhere else.
  cargo test -q --lib
  cargo build -q
  profile=web
else
  profile=release
fi
rm -rf dist
mkdir -p dist
mods=""
for m in webgl2 webgpu; do
  case "$m" in
    webgl2) stem=oneshot_client; feat="" ;;
    webgpu) stem=oneshot_client_gpu; feat="--features webgpu" ;;
  esac
  # Both builds write the same oneshot_client.wasm, so each is bound before
  # the next is built.
  # shellcheck disable=SC2086 # feat is a flag and its value, or nothing
  cargo build -q --profile "$profile" --lib --target wasm32-unknown-unknown $feat
  wasm-bindgen --target web --remove-name-section --remove-producers-section --out-name "$stem" --out-dir dist \
    "target/wasm32-unknown-unknown/$profile/oneshot_client.wasm"
  # Prove which one was bound: only wgpu's WebGPU backend imports
  # `requestAdapter`. One that has it under the WebGL2 name (a feature
  # unified in from somewhere), or lacks it under the WebGPU one, is the
  # wrong page, and it would load and draw nothing rather than fail here.
  if grep -qa '__wbg_requestAdapter_' "dist/${stem}_bg.wasm"; then got=webgpu; else got=webgl2; fi
  if [ "$got" != "$m" ]; then
    echo "$stem: bound a $got module where the page expects $m" >&2
    exit 1
  fi
  raw=$(wc -c < "dist/${stem}_bg.wasm")
  if [ "$mode" = check ]; then
    # The size gate (docs/DESIGN.md §2): what a player downloads, gzipped,
    # whichever module their browser fetches.
    gz=$(gzip -9c "dist/${stem}_bg.wasm" | wc -c)
    echo "$stem ($m): $raw bytes, $gz gzipped"
    if [ "$gz" -gt 9000000 ]; then
      echo "$stem is $gz bytes gzipped, past the 9 MB gate" >&2
      exit 1
    fi
  else
    gz=0
    echo "$stem ($m): $raw bytes (dev: no size gate)"
  fi
  # The pair's hash: the page asks for glue and module by it (a cached glue
  # never meets another build's module) and remembers a WebGPU failure
  # against it.
  sha=$(cat "dist/$stem.js" "dist/${stem}_bg.wasm" | sha256sum | cut -c1-64)
  mods="$mods${mods:+,}\"$m\":{\"js\":\"$stem.js\",\"wasm\":\"${stem}_bg.wasm\",\"wasm_bytes\":$raw,\"wasm_gz_bytes\":$gz,\"sha256\":\"$sha\"}"
done
commit=$(git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)
# What the page reads before it downloads (index.html, pickModule), and what
# ci/publish_web.sh checks it is publishing (both modules, the web profile).
printf '{"commit":"%s","profile":"%s","modules":{%s}}\n' "$commit" "$profile" "$mods" > dist/build.json
cp web/*.html web/*.js dist/
cp ../../NOTICE dist/
cp -r assets dist/assets
du -sh dist

#!/bin/sh
# The Bevy client (crates/client, its own workspace).
#
#   ci/client.sh       the check: fmt, clippy (native and both browser
#                      modules), its tests, a native build, then the page
#                      into crates/client/dist, each module under the gate
#   ci/client.sh dev   just the page, quickly: release (thin LTO, where the
#                      web profile is fat); both modules,
#                      bound and proved, into dist all the same, or one
#                      (WEB_MODULES=webgl2 or webgpu: a dev loop; the page
#                      takes the one there, publish_web.sh refuses it)
#
# The page is two wasm modules (after Gates' ci/build_web.sh, "the game,
# twice"): Bevy picks its GPU API when it is compiled, so `oneshot_client`
# draws through WebGL2 and `oneshot_client_gpu` (`--features webgpu`)
# through WebGPU, the desktop's frame. web/index.html asks the browser for
# a WebGPU adapter before either downloads and fetches the one it can draw
# with. The server serves dist by default:
#   target/release/oneshot-server --web crates/client/dist
# The browser's build is its own profile (crates/client/Cargo.toml: opt-level
# 3, fat LTO, no name section), then wasm-opt -Os where there is one (`npm ci`
# brings binaryen). Needs wasm-bindgen-cli at the version in
# crates/client/Cargo.lock.
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
  cargo test -q --lib --bins
  cargo build -q
  profile=web
else
  profile=release
fi
# wasm-opt: $WASM_OPT, else one on PATH, else npm's (`npm ci`: binaryen).
opt=""
if [ "$mode" = check ]; then
  opt=${WASM_OPT:-$(command -v wasm-opt || true)}
  [ -n "$opt" ] || [ ! -x ../../node_modules/binaryen/bin/wasm-opt ] || opt=../../node_modules/binaryen/bin/wasm-opt
  [ -n "$opt" ] || echo "== wasm-opt: none (npm ci, or WASM_OPT=...): the modules ship unoptimised" >&2
fi
rm -rf dist
mkdir -p dist
mods=""
modules="webgl2 webgpu"
if [ "$mode" = dev ]; then
  modules=${WEB_MODULES:-$modules}
fi
for m in $modules; do
  case "$m" in
    webgl2) stem=oneshot_client; feat="" ;;
    webgpu) stem=oneshot_client_gpu; feat="--features webgpu" ;;
    *) echo "WEB_MODULES: webgl2 and/or webgpu, not $m" >&2; exit 2 ;;
  esac
  # Both builds write the same oneshot_client.wasm, so each is bound before
  # the next is built.
  # shellcheck disable=SC2086 # feat is a flag and its value, or nothing
  cargo build -q --profile "$profile" --lib --target wasm32-unknown-unknown $feat
  wasm-bindgen --target web --remove-name-section --remove-producers-section --out-name "$stem" --out-dir dist \
    "target/wasm32-unknown-unknown/$profile/oneshot_client.wasm"
  # binaryen, after Gates' ci/build_web.sh: what actually shrinks a Bevy
  # module. The real build only (dev stays quick). The features are the
  # module's own: rustc's wasm32 defaults and +simd128 (.cargo/config.toml).
  if [ -n "$opt" ]; then
    "$opt" -Os --enable-bulk-memory --enable-nontrapping-float-to-int \
      --enable-mutable-globals --enable-sign-ext --enable-reference-types \
      --enable-simd --enable-multivalue \
      -o "dist/${stem}_bg.wasm.opt" "dist/${stem}_bg.wasm"
    mv "dist/${stem}_bg.wasm.opt" "dist/${stem}_bg.wasm"
  fi
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
    # What a player downloads, gzipped, whichever module their browser
    # fetches: printed, never a gate.
    gz=$(gzip -9c "dist/${stem}_bg.wasm" | wc -c)
    echo "$stem ($m): $raw bytes, $gz gzipped"
  else
    gz=0
    echo "$stem ($m): $raw bytes (dev)"
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
wasm_opt=$([ -n "$opt" ] && echo true || echo false)
printf '{"commit":"%s","profile":"%s","wasm_opt":%s,"modules":{%s}}\n' "$commit" "$profile" "$wasm_opt" "$mods" > dist/build.json
# The page, the clip page F9 opens in a frame of its own (web/clip.js), the
# dashboard's scripts, and the credits.
cp web/*.html web/*.js dist/
cp ../../NOTICE dist/
cp -r assets dist/assets
du -sh dist

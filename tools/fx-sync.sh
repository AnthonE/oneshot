#!/bin/sh
# Vendor the shared effects crate and its strip tool from a Gates checkout:
#
#   tools/fx-sync.sh [GATES_DIR]      # default $GATES_DIR, then ../Gates
#
# Gates holds the one copy (crates/fx: the particle pool, sprite sheet,
# recipes and the fx stage's contract, Bevy-free; ci/fxstrip.mjs). This copies
# what Gates committed into crates/fx and tools/fxstrip.mjs, writes
# crates/fx/VENDORED (a sha256 per file), and checks it here. Never edit the
# copies: crates/fx/tests/vendored.rs fails; change them in Gates and sync.
set -eu
cd "$(dirname "$0")/.."
G=${1:-${GATES_DIR:-../Gates}}
[ -f "$G/crates/fx/Cargo.toml" ] || { echo "fx-sync: no Gates checkout at $G" >&2; exit 1; }
if [ -n "$(git -C "$G" status --porcelain -- crates/fx ci/fxstrip.mjs)" ]; then
  echo "fx-sync: commit Gates' crates/fx and ci/fxstrip.mjs first" >&2
  exit 1
fi
rev=$(git -C "$G" rev-parse --short HEAD)
rm -rf crates/fx
git -C "$G/crates/fx" ls-files | while read -r f; do
  mkdir -p "crates/fx/$(dirname "$f")"
  cp "$G/crates/fx/$f" "crates/fx/$f"
done
cp "$G/ci/fxstrip.mjs" tools/fxstrip.mjs
{
  echo "# Vendored from Gates $rev by tools/fx-sync.sh. Do not edit here: change it in Gates, then sync."
  (cd crates/fx && find . -type f ! -name VENDORED ! -path './target/*' | sed 's|^\./||' | LC_ALL=C sort | xargs sha256sum)
  (cd crates/fx && sha256sum ../../tools/fxstrip.mjs)
} > crates/fx/VENDORED
cargo fmt -p fx -- --check
cargo clippy -q -p fx --all-targets -- -D warnings
cargo test -q -p fx
echo "fx-sync: crates/fx and tools/fxstrip.mjs from Gates $rev"

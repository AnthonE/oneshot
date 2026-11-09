#!/usr/bin/env bash
# Ship HEAD everywhere a player meets it, after Gates' ci/release.sh: the
# server, the page, the launcher's depots (Linux, Windows), and the desktop
# downloads (the desktop-v* tag; .github/workflows/desktop.yml).
#
#   ci/release.sh                 # the checks and the plan; changes nothing
#   ci/release.sh --go            # ship it (live is the test: no CI wait)
#   ci/release.sh --go --ci       # ship only if HEAD's CI run is green
#
# An OPERATOR ACT. Run it on the game box, in the checkout: the server
# deploys there (ci/deploy_server.sh), the page and the depots go to the
# origin over `ssh morr`.
#
# The order is the point. Everything builds before anything moves, so a
# failed build strands nobody. The server goes first, because the page and
# the depots each refuse a wire the live server doesn't speak, and a desktop
# build left behind is a launcher whose every join is refused. A step that
# fails stops the release and prints the steps that finish it by hand.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

SERVER_JSON=https://game.elopros.com/oneshot/server.json
PAGE=https://elopros.com/games/oneshot/play/build.json
MANIFEST=https://elopros.com/api/launcher/manifest/oneshot
PLATFORMS=(linux-x86_64 win-x86_64)

GO=0; SKIP_CI=1
for a in "$@"; do
  case "$a" in
    --go) GO=1 ;;
    --skip-ci) SKIP_CI=1 ;;
    --ci) SKIP_CI=0 ;;
    -h|--help) sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "release: unknown flag $a" >&2; exit 2 ;;
  esac
done
run() { if [ "$GO" = 1 ]; then echo "+ $*"; "$@"; else echo "  would run: $*"; fi; }
REFUSED=()
refuse() { REFUSED+=("$1"); echo "  !! $1"; }

SHA=$(git rev-parse HEAD)
SHORT=$(git rev-parse --short=9 HEAD)
PROTO=$(grep -oP 'pub const PROTO_VER: u16 = \K[0-9]+' crates/proto/src/lib.rs)
BUILD=$(python3 -c 'import sys; sys.path.insert(0, "ci"); import depot; print(depot.build_id())')
TAG="desktop-v$BUILD"

echo "== 0. HEAD: $SHORT · proto $PROTO · build $BUILD"
[ -z "$(git status --porcelain)" ] || refuse "the tree has uncommitted changes: a release ships a commit"
if [ "$(git ls-remote origin refs/heads/main | cut -f1)" != "$SHA" ]; then
  refuse "HEAD is not origin/main: merge and pull first"
fi

echo
echo "== 1. HEAD's CI run (gh workflow run ci --ref main, about an hour)"
if [ "$SKIP_CI" = 1 ]; then
  echo "  not asked for (--ci waits for a green one)"
elif ! command -v gh >/dev/null; then
  refuse "gh is not installed, so nothing says HEAD is green (or pass --skip-ci)"
else
  verdict=$(gh run list --commit "$SHA" --workflow ci --limit 20 --json status,conclusion,url \
    --jq 'if any(.[]; .status != "completed") then "PENDING " + (map(select(.status != "completed"))[0].url)
          elif any(.[]; .conclusion == "success") then "GREEN " + (map(select(.conclusion == "success"))[0].url)
          elif length > 0 then "RED " + .[0].url else "NONE" end' 2>&1)
  case "$verdict" in
    GREEN*) echo "  green: ${verdict#GREEN }" ;;
    PENDING*) refuse "HEAD's CI run hasn't finished: ${verdict#PENDING }" ;;
    RED*) refuse "HEAD's CI run is red: ${verdict#RED }" ;;
    NONE) refuse "no CI run of $SHORT: gh workflow run ci --ref main (or pass --skip-ci)" ;;
    *) refuse "couldn't ask GitHub: $verdict" ;;
  esac
fi

echo
echo "== 2. the tools the builds need"
command -v x86_64-w64-mingw32-gcc >/dev/null \
  || refuse "no mingw for the Windows depot: apt-get install gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 binutils-mingw-w64-x86-64, the -posix alternatives, rustup target add x86_64-pc-windows-gnu"
if x86_64-w64-mingw32-gcc -v 2>&1 | grep -q 'Thread model: win32'; then
  refuse "mingw is on win32 threads: update-alternatives --set x86_64-w64-mingw32-gcc /usr/bin/x86_64-w64-mingw32-gcc-posix (and g++)"
fi
python3 ci/depot.py --self-test >/dev/null || refuse "ci/depot.py --self-test fails"
for p in "${PLATFORMS[@]}"; do
  curl -fsS -m 30 "$MANIFEST" | python3 -c "import json,sys; sys.exit(0 if any(b.get('platform') == '$p' for b in json.load(sys.stdin).get('builds', [])) else 1)" \
    || refuse "the launcher's manifest has no $p slot (Elo Pros repo, data/launcher/oneshot.manifest.json): its depot would publish to nothing"
done

if [ "$GO" = 1 ] && [ "${#REFUSED[@]}" -gt 0 ]; then
  echo; echo "== refused: nothing was built or changed"; exit 1
fi

echo
echo "== 3. build all of it, before anything moves"
# Niced: the box's other games keep their cores. Two big builds at a time
# at most (the page's two modules, then the two desktops), each in a build
# folder of its own, so neither waits on the other's lock.
t0=$SECONDS
run nice -n 10 cargo build -q --release -p server || exit 1
run nice -n 10 ci/client.sh ship || exit 1
if [ "$GO" = 1 ]; then
  pids=()
  for p in "${PLATFORMS[@]}"; do
    echo "+ python3 ci/depot.py --platform $p (logged to target/depot/$p.log)"
    mkdir -p target/depot
    nice -n 10 python3 ci/depot.py --platform "$p" > "target/depot/$p.log" 2>&1 &
    pids+=($!)
  done
  for i in "${!pids[@]}"; do
    wait "${pids[$i]}" || { echo "  !! the ${PLATFORMS[$i]} depot failed: target/depot/${PLATFORMS[$i]}.log"; tail -20 "target/depot/${PLATFORMS[$i]}.log"; exit 1; }
  done
  echo "  built in $((SECONDS - t0)) s"
else
  for p in "${PLATFORMS[@]}"; do echo "  would run: python3 ci/depot.py --platform $p (side by side)"; done
fi

echo
echo "== 4. publish: the server first"
STEPS=(
  "ci/deploy_server.sh --go"
  "ci/publish_web.sh --no-build"
  "python3 ci/publish_depot.py --platform linux-x86_64 --no-build --notarize"
  "python3 ci/publish_depot.py --platform win-x86_64 --no-build --notarize"
)
for i in "${!STEPS[@]}"; do
  read -ra cmd <<< "${STEPS[$i]}"
  if ! run "${cmd[@]}"; then
    echo
    echo "  !! stopped at: ${STEPS[$i]}"
    echo "     finish by hand, in this order:"
    for ((j = i; j < ${#STEPS[@]}; j++)); do echo "       ${STEPS[$j]}"; done
    echo "       then: git tag $TAG $SHA && git push origin $TAG"
    exit 1
  fi
done

echo
echo "== 5. every surface speaks proto $PROTO and names $SHORT"
if [ "$GO" = 1 ]; then
  python3 - "$PROTO" "$SHORT" "$BUILD" "$SERVER_JSON" "$PAGE" "$MANIFEST" "${PLATFORMS[@]}" <<'PY' || { echo "  !! a surface doesn't match HEAD; nothing is tagged"; exit 1; }
import json, sys, urllib.request
proto, short, build, server, page, manifest, *plats = sys.argv[1:]
def get(u):
    try:
        with urllib.request.urlopen(u + ("&" if "?" in u else "?") + "release=" + short, timeout=30) as r:
            return json.load(r)
    except Exception as e:
        return {"_err": str(e)}
bad = 0
def check(ok, msg):
    global bad
    bad += not ok
    print(("  ok  " if ok else "  !!  ") + msg)
s, p, m = get(server), get(page), get(manifest)
check(s.get("proto") == int(proto), f"server.json: proto {s.get('proto', s.get('_err'))}")
check(str(p.get("commit", "")).startswith(short),
      f"page build.json: commit {p.get('commit', p.get('_err'))}")
rows = {b.get("platform"): b for b in m.get("builds", [])}
for plat in plats:
    b = rows.get(plat, {})
    check(b.get("version") == build and b.get("depot_state") == "published",
          f"launcher {plat}: {b.get('version')} ({b.get('depot_state')})")
sys.exit(1 if bad else 0)
PY
else
  echo "  would check: $SERVER_JSON proto, $PAGE commit, $MANIFEST serving $BUILD on ${PLATFORMS[*]}"
fi

echo
echo "== 6. the downloads: $TAG builds Linux, Windows and macOS onto a GitHub release (desktop.yml)"
if git ls-remote --exit-code --tags origin "refs/tags/$TAG" >/dev/null 2>&1; then
  echo "  $TAG is tagged already"
else
  run git tag -a "$TAG" -m "oneshot desktop $BUILD" "$SHA" && run git push origin "$TAG"
fi

echo
if [ "$GO" = 1 ]; then
  echo "== released $BUILD."
elif [ "${#REFUSED[@]}" -gt 0 ]; then
  echo "== dry run: --go would refuse:"
  for r in "${REFUSED[@]}"; do echo "  - $r"; done
  exit 1
else
  echo "== dry run. Re-run with --go to release."
fi

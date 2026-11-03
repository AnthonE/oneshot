#!/usr/bin/env bash
# Release the server on the game box (ops/README.md), after Gates'
# ci/deploy_shard.sh. Run it on the box, in the checkout:
#
#   ci/deploy_server.sh          # a dry run: what it would do
#   ci/deploy_server.sh --go     # build, stage, install, restart, check
#
# The restart drains (ops/oneshot.service): every match plays out first, so
# this can wait up to --drain-max while new players are told to come back.
set -uo pipefail
cd "$(dirname "$0")/.."

DEPLOY=/home/master/oneshot-deploy
DATA=/home/master/oneshot-data
UNIT=oneshot.service
HOOK=/etc/letsencrypt/renewal-hooks/deploy/copy-oneshot-cert.sh
PORT=61250
READY=http://127.0.0.1:8450/ready
GO=0
for a in "$@"; do
  case "$a" in
    --go) GO=1 ;;
    *) echo "unknown flag: $a" >&2; exit 2 ;;
  esac
done
run() { if [ "$GO" = 1 ]; then echo "+ $*"; "$@" || exit 1; else echo "  would run: $*"; fi; }
differs() { ! sudo -n cmp -s "$1" "$2" 2>/dev/null; }

want=$(grep -oP 'pub const PROTO_VER: u16 = \K[0-9]+' crates/proto/src/lib.rs)
live=$(curl -fsS -m 3 "$READY" 2>/dev/null | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("proto",""))
except Exception: print("")')
echo "== 0. the wire: this build speaks proto $want, the running server ${live:-<not running>}"
if [ -n "$live" ] && [ "$live" != "$want" ]; then
  echo "  !! the published page speaks $live: republish it after (ci/publish_web.sh)"
fi

echo "== 1. build"
run cargo build -q --release -p server
BIN=target/release/oneshot-server
if [ "$GO" = 1 ] && [ ! -x "$BIN" ]; then echo "  !! no $BIN after the build"; exit 1; fi

echo "== 2. stage"
run mkdir -p "$DEPLOY" "$DATA/replays"
for b in oneshot-server oneshot-replay oneshot-review; do
  run cp -f "target/release/$b" "$DEPLOY/$b.new"
  run mv -f "$DEPLOY/$b.new" "$DEPLOY/$b"   # atomic; a running server keeps its inode
done
run cp -f ops/game-elopros-oneshot.nginx.conf "$DEPLOY/game-elopros-oneshot.nginx.conf"
# The unit passes --elo-key: without the file the server won't start.
if [ ! -f "$DATA/server.pem" ]; then
  echo "  !! no $DATA/server.pem, which the unit signs ranked results with. Make it first:"
  echo "     openssl genpkey -algorithm ed25519 -out $DATA/server.pem && chmod 600 $DATA/server.pem"
  echo "     and enter its public half on Elo Pros' dev desk (ops/README.md)"
  [ "$GO" = 1 ] && exit 1
fi

echo "== 3. the unit and the certbot hook"
if differs ops/oneshot.service "/etc/systemd/system/$UNIT"; then
  run sudo cp -f ops/oneshot.service "/etc/systemd/system/$UNIT"
  run sudo systemctl daemon-reload
else
  echo "  unit unchanged"
fi
if differs ops/certbot-deploy-hook.sh "$HOOK"; then
  run sudo cp -f ops/certbot-deploy-hook.sh "$HOOK"
  run sudo chmod +x "$HOOK"
else
  echo "  hook unchanged"
fi
if [ ! -f /home/master/oneshot-certs/fullchain.pem ]; then
  run sudo "$HOOK"
fi

echo "== 4. restart (drains: matches finish first)"
run sudo systemctl enable "$UNIT"
run sudo systemctl restart "$UNIT"

echo "== 5. is it up"
if [ "$GO" = 1 ]; then
  ok=0
  for _ in $(seq 1 30); do
    if ss -ulnp 2>/dev/null | grep -q ":$PORT " && curl -fsS -m 2 "$READY" >/dev/null 2>&1; then
      ok=1; break
    fi
    sleep 1
  done
  if [ "$ok" = 0 ]; then
    echo "  !! not bound on udp $PORT or $READY not answering after 30 s"
    sudo systemctl status "$UNIT" --no-pager | sed 's/^/  /'
    exit 1
  fi
  echo "  bound on udp $PORT; $(curl -fsS -m 2 "$READY")"
  # The certificate must answer for the name players dial.
  if ! openssl x509 -noout -checkhost game.elopros.com -in /home/master/oneshot-certs/fullchain.pem \
       | grep -q "does match certificate"; then
    echo "  !! /home/master/oneshot-certs doesn't cover game.elopros.com: run $HOOK"
    exit 1
  fi
  echo "  certificate covers game.elopros.com"
else
  echo "  would check: udp $PORT bound, $READY answering, the certificate covers game.elopros.com"
fi

#!/usr/bin/env bash
# Release the server on the game box without a moment where nobody can join
# (ops/README.md), after Gates' ci/deploy_shard.sh. Run it on the box, in
# the checkout:
#
#   ci/deploy_server.sh          # a dry run: what it would do
#   ci/deploy_server.sh --go     # build, stage, start the idle slot, switch, drain the old
#   ci/deploy_server.sh --go --cut   # the idle slot is still draining: cut its matches (SIGINT, step 4)
#
# Two slots, oneshot@a (UDP 61250) and oneshot@b (UDP 61251). The live one is
# named in /home/master/oneshot-data/live. A deploy starts the new build in
# the OTHER slot, waits until it takes players, writes its address to `live`
# (every server.json says it from then on), and only then stops the old slot,
# which drains: its matches play out while new players go to the new one.
set -uo pipefail
cd "$(dirname "$0")/.."

# The unit names these paths; the overrides are for a dry run elsewhere.
DEPLOY=${DEPLOY:-/home/master/oneshot-deploy}
DATA=${DATA:-/home/master/oneshot-data}
LIVE="$DATA/live"
HOST=game.elopros.com
HOOK=/etc/letsencrypt/renewal-hooks/deploy/copy-oneshot-cert.sh
GO=0
CUT=0
for a in "$@"; do
  case "$a" in
    --go) GO=1 ;;
    --cut) CUT=1 ;;
    *) echo "unknown flag: $a" >&2; exit 2 ;;
  esac
done
run() { if [ "$GO" = 1 ]; then echo "+ $*"; "$@" || exit 1; else echo "  would run: $*"; fi; }
# put <file> <text>: written whole (a temp file, then a rename).
put() {
  if [ "$GO" = 1 ]; then
    echo "+ $1 <- $2"
    printf '%s\n' "$2" > "$1.new" && mv -f "$1.new" "$1" || exit 1
  else
    echo "  would write $1: $2"
  fi
}
differs() { ! sudo -n cmp -s "$1" "$2" 2>/dev/null; }
game_port() { [ "$1" = a ] && echo 61250 || echo 61251; }
http_port() { [ "$1" = a ] && echo 8450 || echo 8451; }

# Which slot is live, and so which one this build goes to.
now=$(cat "$LIVE" 2>/dev/null | tr -d '[:space:]')
case "$now" in
  "$HOST:61250") OLD=a; NEW=b ;;
  "$HOST:61251") OLD=b; NEW=a ;;
  *) OLD=; NEW=a ;;
esac
GPORT=$(game_port $NEW)
READY="http://127.0.0.1:$(http_port $NEW)/ready"
if [ -n "$OLD" ]; then
  echo "== live: oneshot@$OLD ($now); this build goes to oneshot@$NEW (:$GPORT)"
else
  echo "== no live slot yet (${LIVE} names none): this build goes to oneshot@$NEW (:$GPORT)"
fi

# The slot we're about to use may still be playing out the last deploy.
state=$(systemctl is-active "oneshot@$NEW.service" 2>/dev/null)
if [ "$state" = active ] || [ "$state" = deactivating ]; then
  if [ "$CUT" = 1 ]; then
    echo "  !! oneshot@$NEW is $state (draining from the last deploy): its matches are cut at step 4 (--cut)"
  else
    echo "  !! oneshot@$NEW is $state: it is still draining the last deploy's matches."
    echo "     Wait for it (journalctl -u oneshot@$NEW -f), or pass --cut to end them now."
    [ "$GO" = 1 ] && exit 1
  fi
fi

want=$(grep -oP 'pub const PROTO_VER: u16 = \K[0-9]+' crates/proto/src/lib.rs)
if [ -n "$OLD" ]; then
  have=$(curl -fsS -m 3 "http://127.0.0.1:$(http_port $OLD)/ready" 2>/dev/null | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("proto",""))
except Exception: print("")')
  echo "== 0. the wire: this build speaks proto $want, the live slot ${have:-<not answering>}"
  if [ -n "$have" ] && [ "$have" != "$want" ]; then
    echo "  !! the published page speaks $have: republish it right after (ci/publish_web.sh)"
  fi
fi

echo "== 1. build"
run cargo build -q --release -p server
if [ "$GO" = 1 ] && [ ! -x target/release/oneshot-server ]; then echo "  !! no oneshot-server after the build"; exit 1; fi

echo "== 2. stage oneshot@$NEW"
SLOT="$DEPLOY/$NEW"
run mkdir -p "$SLOT" "$DATA/replays" "$DATA/bugs"
# Replays (ban evidence) and bug reports (strangers' text) are the slots'
# alone on a shared box.
run chmod 750 "$DATA" "$DATA/replays" "$DATA/bugs"
for b in oneshot-server oneshot-replay oneshot-review; do
  run cp -f "target/release/$b" "$SLOT/$b.new"
  run mv -f "$SLOT/$b.new" "$SLOT/$b"
done
put "$SLOT/slot.env" "GAME_PORT=$GPORT
HTTP_PORT=$(http_port $NEW)"
# The locations name nginx's `oneshot_bugs` zone, which only the box's own
# http-level config can define (ops/README.md step 4). Staged without it, the
# box's next nginx reload (Gates' too) would fail: then the old file stays.
NGINX_DIR=${NGINX_DIR:-/etc/nginx}
if grep -rqs 'limit_req_zone.*zone=oneshot_bugs' "$NGINX_DIR"; then
  run cp -f ops/game-elopros-oneshot.nginx.conf "$DEPLOY/game-elopros-oneshot.nginx.conf"
else
  echo "!! nginx defines no oneshot_bugs zone: add the limit_req_zone line from" >&2
  echo "   ops/game-elopros-oneshot.nginx.conf to oneshot-upstream.conf; the nginx file isn't staged" >&2
fi
# The unit passes --elo-key: without the file the server won't start.
if [ ! -f "$DATA/server.pem" ]; then
  echo "  !! no $DATA/server.pem, which the slots sign ranked results with. Make it first:"
  echo "     openssl genpkey -algorithm ed25519 -out $DATA/server.pem && chmod 600 $DATA/server.pem"
  echo "     and enter its public half on Elo Pros' dev desk (ops/README.md)"
  [ "$GO" = 1 ] && exit 1
fi
# One nonce key for both slots, made once.
if [ ! -f "$DATA/nonce.key" ]; then
  if [ "$GO" = 1 ]; then
    echo "+ $DATA/nonce.key <- 32 random bytes"
    (umask 077 && head -c 32 /dev/urandom > "$DATA/nonce.key.new" && mv -f "$DATA/nonce.key.new" "$DATA/nonce.key") || exit 1
  else
    echo "  would make $DATA/nonce.key: 32 random bytes, mode 600"
  fi
fi

echo "== 3. the unit and the certbot hook"
if differs "ops/oneshot@.service" /etc/systemd/system/oneshot@.service; then
  run sudo cp -f "ops/oneshot@.service" /etc/systemd/system/oneshot@.service
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

echo "== 4. start oneshot@$NEW (nobody on it yet)"
# A draining server ignores another SIGTERM (a restart's stop): SIGINT ends
# it at once, its matches cut (the reports still going out are saved for
# `oneshot-review resend`).
if [ "$CUT" = 1 ] && { [ "$state" = active ] || [ "$state" = deactivating ]; }; then
  run sudo systemctl kill -s SIGINT "oneshot@$NEW.service"
  if [ "$GO" = 1 ]; then
    for _ in $(seq 1 15); do
      [ "$(systemctl is-active "oneshot@$NEW.service" 2>/dev/null)" = deactivating ] || break
      sleep 1
    done
  fi
fi
run sudo systemctl restart "oneshot@$NEW.service"
if [ "$GO" = 1 ]; then
  ok=0
  for _ in $(seq 1 30); do
    if ss -ulnp 2>/dev/null | grep -q ":$GPORT " && curl -fsS -m 2 "$READY" >/dev/null 2>&1; then
      ok=1; break
    fi
    sleep 1
  done
  if [ "$ok" = 0 ]; then
    echo "  !! oneshot@$NEW isn't bound on udp $GPORT or $READY isn't answering after 30 s."
    echo "     Players are still on oneshot@${OLD:-<none>}; nothing was switched."
    sudo systemctl status "oneshot@$NEW.service" --no-pager | sed 's/^/  /'
    exit 1
  fi
  echo "  bound on udp $GPORT; $(curl -fsS -m 2 "$READY")"
  if ! openssl x509 -noout -checkhost "$HOST" -in /home/master/oneshot-certs/fullchain.pem \
       | grep -q "does match certificate"; then
    echo "  !! /home/master/oneshot-certs doesn't cover $HOST: run $HOOK. Nothing was switched."
    exit 1
  fi
  echo "  certificate covers $HOST"
else
  echo "  would check: udp $GPORT bound, $READY answering, the certificate covers $HOST"
fi

echo "== 5. switch: new players go to oneshot@$NEW"
put "$LIVE" "$HOST:$GPORT"
run sudo systemctl enable "oneshot@$NEW.service"

echo "== 6. drain the old"
if [ -n "$OLD" ]; then
  run sudo systemctl disable "oneshot@$OLD.service"
  # Returns at once; the slot plays its matches out (up to --drain-max) and exits.
  run sudo systemctl stop --no-block "oneshot@$OLD.service"
  echo "  oneshot@$OLD is draining: journalctl -u oneshot@$OLD -f"
else
  echo "  nothing to drain"
fi

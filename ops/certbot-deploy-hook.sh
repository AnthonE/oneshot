#!/bin/bash
# Certbot deploy hook: copy the renewed chain to where the oneshot server
# reads it, and RELOAD it (SIGHUP) so new handshakes get the new certificate
# while nobody in a match is dropped. After Gates' ops/certbot-deploy-hook.sh.
#
#   sudo cp ops/certbot-deploy-hook.sh \
#           /etc/letsencrypt/renewal-hooks/deploy/copy-oneshot-cert.sh
#   sudo chmod +x /etc/letsencrypt/renewal-hooks/deploy/copy-oneshot-cert.sh
#
# `ci/deploy_server.sh` installs it. It is its own file, not an edit of
# Gates' or thrml's hook: the games share a certificate and nothing else, and
# one game's hook must never restart another.
set -e

# The name players dial: --public-addr in ops/oneshot@.service.
NAME=game.elopros.com
DEST=/home/master/oneshot-certs
SLOTS="a b"

# `openssl x509 -checkhost` exits 0 either way; the verdict is on stdout.
covers() {  # $1 = name, $2 = a fullchain.pem
  local verdict
  verdict=$(openssl x509 -noout -checkhost "$1" -in "$2" 2>/dev/null) || return 1
  case "$verdict" in
    *"does match certificate"*) return 0 ;;
    *) return 1 ;;
  esac
}

# The lineage is found, not typed: the first under live/ that covers $NAME
# (certbot may have named it <name>-0001).
LIVE=
for cand in /etc/letsencrypt/live/"$NAME" /etc/letsencrypt/live/*; do
  [ -f "$cand/fullchain.pem" ] || continue
  if covers "$NAME" "$cand/fullchain.pem"; then
    LIVE=$cand
    break
  fi
done

# Refuse, before copying anything, a chain that can't answer for the name.
if [ -z "$LIVE" ]; then
  echo "[copy-oneshot-cert] REFUSING: no lineage under /etc/letsencrypt/live covers $NAME" >&2
  exit 1
fi

# A renewal of some other lineage is none of ours. Unset (run by hand) copies.
case "${RENEWED_LINEAGE:-$LIVE}" in
  "$LIVE"|"$LIVE"/) ;;
  *) [ -n "${RENEWED_LINEAGE:-}" ] && exit 0 ;;
esac

install -d -o master -g master -m 755 "$DEST"
install -o master -g master -m 644 "$LIVE/fullchain.pem" "$DEST/fullchain.pem"
install -o master -g master -m 600 "$LIVE/privkey.pem"   "$DEST/privkey.pem"

# Every slot that runs reads it again (a draining one too: it may still take
# a handshake from a page that hasn't moved yet).
reloaded=
for s in $SLOTS; do
  if systemctl is-active --quiet "oneshot@$s.service"; then
    systemctl reload "oneshot@$s.service"
    reloaded="$reloaded oneshot@$s"
  fi
done
echo "[copy-oneshot-cert] refreshed $DEST from $LIVE; reloaded:${reloaded:- nothing running}"

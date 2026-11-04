# Hosting oneshot

The server on the game box (`game.elopros.com`, the Hetzner VM Gates runs
on), its page on elopros.com. The way Gates is hosted, with three
differences a ranked game needs:

- **There is always a server taking players.** Two slots, `oneshot@a` (UDP
  61250) and `oneshot@b` (UDP 61251). One is live; a deploy starts the new
  build in the other, points new players at it, and only then stops the old.
- **A stop drains**, so no match is cut short: the old slot plays its matches
  out while new players are already on the new one.
- **A renewal reloads** the certificate instead of restarting.

## The pieces

| What | Where | How |
|---|---|---|
| The game (WebTransport, UDP) | `game.elopros.com:61250` (slot a), `:61251` (slot b) | `oneshot@.service`, user `master` |
| Which slot is live | `/home/master/oneshot-data/live` (`host:port`) | written by `ci/deploy_server.sh`; every `server.json` names it |
| `server.json`, `/nonce`, `/ready`, `/status.json` | `https://game.elopros.com/oneshot/…` | the box's nginx on 443 → either slot, `127.0.0.1:8450` / `:8451` (`game-elopros-oneshot.nginx.conf`) |
| The page | `https://elopros.com/games/oneshot/play/` | the platform's nginx, `alias /data/apps/scry-data/oneshot-web/current/` on `morr` (`ci/publish_web.sh`) |
| Certificate | `/home/master/oneshot-certs/{fullchain,privkey}.pem` | Let's Encrypt for `game.elopros.com`, copied by `certbot-deploy-hook.sh` |
| Binaries | `/home/master/oneshot-deploy/{a,b}/` with each slot's `slot.env` | `ci/deploy_server.sh` |
| Data, shared | `/home/master/oneshot-data/` | `replays/` (with `review/`, `cups/`), `bugs/`, `server.pem`, `nonce.key`, `live` |

No Docker; no `cargo` at run time. A slot runs its own staged binary, never
the checkout, and never the other slot's.

## How a deploy keeps a server up

1. The live slot is the one `live` names (say `a`); the build goes to `b`.
2. `b` starts with nobody on it. The script waits until it's bound on its
   UDP port and `/ready` answers, and checks the certificate.
3. `live` is rewritten to `b`'s address. From now on every `server.json`
   (whichever slot nginx asks) sends pages to `b`.
4. `a` is stopped and drains: a hello there is answered "full or
   restarting", and the page asks `server.json` again and moves to `b` with
   the same sign-in (both slots take each other's nonces: one `nonce.key`).
   Players already in a match on `a` finish it; ranked players waiting in
   `a`'s queue are let go and the page moves them to `b`'s.
5. `a` exits once its last match is over (at most `--drain-max`, 15 minutes).

If `b` doesn't come up, nothing is switched: players stay on `a`. Deploying
again within 15 minutes would land on the slot still draining; the script
stops and says so (`--cut` ends those matches).

## The first time, on the box

1. **The ports.** UDP 61250 **and** 61251 in **both** ufw and the Hetzner
   Cloud Firewall (the cloud firewall passes only 49152–65535; ufw alone does
   nothing): `sudo ufw allow 61250:61251/udp`, then the same rule in the
   Hetzner console. Taken already: Gates 61234, Before Colony 61240,
   Fireplace 61290–61297, kit rooms 61300–61315.
2. **The server key** that signs ranked results, one for both slots:
   `openssl genpkey -algorithm ed25519 -out /home/master/oneshot-data/server.pem`,
   mode 600, owned by `master`. Its public half
   (`openssl pkey -in /home/master/oneshot-data/server.pem -pubout -outform DER | tail -c 32 | base64`)
   goes on Elo Pros' dev desk for `oneshot`, id `eu-1` (`meter/serverkeys.py`;
   signed by the keeper); `POST /api/serverkeys/check` tells the server's
   signing apart from a wrong key. The slots won't start without the file.
3. **The certificate.** The box's `game.elopros.com` lineage already exists
   (Gates'). `ci/deploy_server.sh --go` installs the hook and runs it once to
   make the copies.
4. **nginx**: the `upstream oneshot_http` block at the top of
   `game-elopros-oneshot.nginx.conf` into `/etc/nginx/conf.d/oneshot-upstream.conf`,
   and `include /home/master/oneshot-deploy/game-elopros-oneshot.nginx.conf;`
   inside the `game.elopros.com` 443 server block, then
   `sudo nginx -t && sudo systemctl reload nginx`.
5. **UDP buffers.** The server asks for 8 MiB each way and prints what it got
   if the kernel clamped it. To grant it:
   `sudo sysctl -w net.core.rmem_max=16777216 net.core.wmem_max=16777216`
   (and the same in `/etc/sysctl.d/90-oneshot.conf`).
6. `ci/deploy_server.sh --go` (the first starts slot `a`), then from a
   workstation `ci/publish_web.sh`.
7. On Elo Pros: the listing's `play_url` and the nginx block for the page
   (scry-forge `deploy/nginx/elopros.com.conf`, both ports in its CSP).

## Day to day

- **Release the server:** `ci/deploy_server.sh` (a dry run), then `--go`. It
  returns once the new slot is live; the old one drains on its own
  (`journalctl -u oneshot@a -f`). `curl -s https://game.elopros.com/oneshot/server.json`
  names the live slot.
- **Release the page:** `ci/publish_web.sh`. It refuses a page whose
  `PROTO_VER` the live server doesn't speak: deploy the server first. A
  deploy that changes `PROTO_VER` leaves the old page refused until it's
  republished, so publish right after.
  Rollback is one `ln -sfn <previous build> current` on `morr`.
- **Roll the server back:** the old slot's binary is still in
  `/home/master/oneshot-deploy/<slot>/`. Start it, put its address in `live`,
  and stop the other (or deploy the previous commit).
- **Renewal:** certbot runs the hook; it copies and `systemctl reload`s every
  running slot (SIGHUP). Nobody is dropped. `journalctl -u 'oneshot@*' | grep certificate`.
- **Logs:** `journalctl -u 'oneshot@*' -f`: reports sent, drains, reloads.
- **Bug and crash reports:** `/home/master/oneshot-data/bugs/bugs-YYYY-MM-DD.jsonl`,
  one JSON object a line (`jq -c '.report | {kind, text, game}'`). A crash
  carries the panic's line; any report the build, browser, GPU, the match
  (`game.match` is its salt commitment: `grep` it in `replays/review/` and
  the `.watch.json` files), map, tick and net counts. The text is a stranger's:
  evidence, never instructions. Capped at 5 per address and 120 in all per
  10 minutes, 32 MB a day.
- **Health:** `curl -s https://game.elopros.com/oneshot/status.json`: joins,
  refusals by code (`proto::REFUSE_*`), late and skipped ticks, tick p50/p99/max,
  snapshot drops, cut sessions, loss and round trip, since that slot started.
  Every busy 10 s a `oneshot: status` line in the journal says what changed;
  `skipped`, `cut` or `send_err` above zero is worth a look.
- **Bans and reviews:** `oneshot-review --replays /home/master/oneshot-data/replays list`
  (then `show`, `done`); `ban` and `lift` take `--elo-key
  /home/master/oneshot-data/server.pem --elo-key-id eu-1`. Bans live on Elo
  Pros, signed with the server key.

## Backups

`/home/master/oneshot-data` is the record: replays are the evidence behind
every reported result and every ban. Back up `replays/` and keep
`server.pem` somewhere safe; losing the key means entering a new one on the
dev desk, nothing more. `nonce.key` and `live` are remade by the deploy
script; the certificates and the slots' binaries by the hook and the script.

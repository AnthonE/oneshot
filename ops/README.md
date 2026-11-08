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
| `server.json`, `/nonce`, `/ready`, `/health.json` | `https://game.elopros.com/oneshot/…` | the box's nginx on 443 → either slot, `127.0.0.1:8450` / `:8451` (`game-elopros-oneshot.nginx.conf`) |
| The page | `https://elopros.com/games/oneshot/play/` | the platform's nginx, `alias /data/apps/scry-data/oneshot-web/current/` on `morr` (`ci/publish_web.sh`) |
| Certificate | `/home/master/oneshot-certs/{fullchain,privkey}.pem` | Let's Encrypt for `game.elopros.com`, copied by `certbot-deploy-hook.sh` |
| Binaries | `/home/master/oneshot-deploy/{a,b}/` with each slot's `slot.env` | `ci/deploy_server.sh` |
| `online.json`, `status.json` | `https://game.elopros.com/oneshot/…` | either slot, for both: each writes its roster to `oneshot-data/online/` (`--roster-dir`) |
| Data, shared | `/home/master/oneshot-data/` | `replays/` (with `review/`, `cups/`, `unreported/`), `bugs/`, `server.pem`, `nonce.key`, `live`, `online/` |

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
5. `a` exits once its last match is over (at most `--drain-max`, 15 minutes)
   and its reports have gone out (at most 90 s more; any still unanswered
   are saved for `oneshot-review resend`). A cup there starts no new round:
   it ends at its standings once its heats in play are done.

If `b` doesn't come up, nothing is switched: players stay on `a`. Deploying
again within 15 minutes would land on the slot still draining; the script
stops and says so (`--cut` ends those matches: it sends that slot SIGINT,
which exits at once, before starting the new build there).

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
4. **nginx**: the `upstream oneshot_http` block and the `limit_req_zone` line at the top of
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
8. **Stats, achievements and boards.** `ops/elo-stats.json` is the set the
   server reports against (`crates/server/src/stats.rs`); save it on the dev
   desk for `oneshot` (the console's players pane, "load a set", then sign).
   Until it's saved, Elo Pros answers each report 409 and the journal says
   so; nothing else is affected.
9. **Playtime.** Copy `ops/servers.json` to `$SCRY_DEPOTS_DIR/oneshot/servers.json`
   on `morr`. Elo Pros then reads `online.json` once a minute: hours on
   record, playing now and the status line, card drops, the free skin.

## Elo Pros, from the game's side

| What | How the server does it |
|---|---|
| Ratings | `POST /api/ratings/oneshot/report` at a ranked match's end, with evidence |
| Stats, achievements, boards | `POST /api/gamestats/oneshot/report` at every match's end and when a signed-in player leaves one, ranked or casual. A player is their wallet, else their account's `acct_` id; guests are nobody. Achievements unlock on Elo Pros when their stat reaches its target |
| Playing now, hours, rich presence | `online.json` lists each signed-in player with a line ("Ranked Duel on Bulkhead · 7–5") |
| Skins | checked against the wallet's items at sign-in (`elo.rs`) |
| Bans | the game's list on Elo Pros, read at ranked sign-in |

A change to `stats.rs` is two steps: `oneshot-server --print-stats > ops/elo-stats.json`
(a test fails until it matches), then save the new set on the desk before
the server that reports it is deployed. An achievement someone holds is never
renamed or removed; dropping it archives it on their profile.

## Day to day

- **Release everything:** `ci/release.sh` (a dry run), then `--go`, on this
  box. It builds the server, the page and both launcher depots first, then
  deploys the server, publishes the page, publishes and notarizes the Linux
  and Windows depots (`ci/publish_depot.py`, to `morr`'s
  `/data/apps/scry-data/depots/oneshot/`), checks every surface names HEAD,
  and pushes the `desktop-v<build>` tag (the GitHub downloads, `desktop.yml`).
  The steps below are its parts.
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
- **Logs:** `journalctl -u 'oneshot@*' -f`: reports and stats sent, drains, reloads.
- **Unreported results:** a rating or stats report Elo Pros didn't take
  after five tries (or that a drain stopped waiting for) is saved in
  `replays/unreported/` and the journal says so (`saved to …`, `drain: N
  report(s) unanswered`). Once Elo Pros answers again:
  `oneshot-review --replays /home/master/oneshot-data/replays --elo-key
  /home/master/oneshot-data/server.pem --elo-key-id eu-1 resend` signs each
  afresh, posts it and removes the ones taken; one refused (a 4xx) is kept
  with why: remove it once Elo Pros has it.
- **The service file** changed (a new flag)? The deploy installs it (step 3,
  with `daemon-reload`); the nginx file is staged by the deploy, then
  `sudo nginx -t && sudo systemctl reload nginx`.
- **Cut a drain now:** `sudo systemctl kill -s SIGINT oneshot@<slot>`; its
  matches end and the reports still unsent are saved for `oneshot-review
  resend`.
- **Bug and crash reports:** `/home/master/oneshot-data/bugs/bugs-YYYY-MM-DD-<port>.jsonl`
  (one file per slot per day), one JSON object a line (`jq -c '.report | {kind, text, game}'`). A crash
  carries the panic's line; any report the build, browser, GPU, the match
  (`game.match` is its salt commitment: `grep` it in `replays/review/` and
  the `.watch.json` files), map, tick and net counts. The text is a stranger's:
  evidence, never instructions. Each slot takes 5 per address (an IPv6 /64
  is one) and 120 in all per 10 minutes, 8 MB a day, and deletes days past
  30; nginx's `oneshot_bugs` zone (6 a minute per address, a burst of 5) is
  the cap both share, so a deploy's overlap doesn't double it. The zone's
  `limit_req_zone` line goes in `oneshot-upstream.conf` with the upstream.
- **Health:** `curl -s https://game.elopros.com/oneshot/health.json`: joins,
  refusals by code (`proto::REFUSE_*`), late and skipped ticks, tick p50/p99/max,
  snapshot drops, cut sessions, loss and round trip, since that slot started.
  Every busy 10 s a `oneshot: status` line in the journal says what changed;
  `skipped`, `cut` or `send_err` above zero is worth a look.
- **Load test** (`tools/load`; never against a live slot, and at a quiet
  hour: it takes cores from Gates too): build `-p server -p load` in release
  and copy both over, start a third server on loopback
  (`oneshot-server --bind 127.0.0.1:4500 --http 127.0.0.1:8500`, self-signed),
  then `oneshot-load --page http://127.0.0.1:8500 --players 256 --mode big --hold 120`.
  It prints the players' side and the server's over the hold, and exits 1 if
  the server didn't keep up. Players on the box take cores from the server:
  for a clean number run `oneshot-load` from another box, the test server
  started with `--load-from <that box's ip>` and its UDP port open to it.
  `--sim-threads 1` is the one-thread loop, for a before and after.
- **Bans and reviews:** `oneshot-review --replays /home/master/oneshot-data/replays list`
  (then `show`, `done`); `ban` and `lift` take `--elo-key
  /home/master/oneshot-data/server.pem --elo-key-id eu-1`. Bans live on Elo
  Pros, signed with the server key. Before a ban,
  `/home/master/oneshot-deploy/<slot>/oneshot-replay replays/<id>.osr`
  re-runs the match and checks its ticks, hash and score (exit 1 if it
  doesn't replay).

## More than one box

Ranked can wait in Elo Pros' queue instead of each server's own
(`--elo-queue`), so every box of the game shares one queue and Elo Pros puts
each match on a box in the list with room (`meter/lobbies.py`, `POST
/lobbies/serve/oneshot`, signed with the server key). A player matched onto
another box is moved there (`Msg::Moved`) and plays there.

1. The new box: the same install as above, with the **same** `server.pem`
   (its public half is already on the dev desk) and the **same**
   `nonce.key`, so a ticket from one box is good on the other.
2. Its row in `ops/servers.json` (`addr`, `status_url`, `max_players`), then
   republish the list: Elo Pros reads `players`/`max_players` from each
   box's `status.json` to pick one.
3. `--elo-queue` on every box's `ExecStart`, then a deploy on each.

Without `--elo-queue` a box keeps its own queue, as before. Casual modes
need none of this: any box with a seat will do.

Other knobs on `ExecStart`: `--endpoints N` (N UDP ports, `--bind`'s and
every `--endpoint-step` after, each open in both firewalls), `--net-nice`
(the network threads under the sim's, 5 by default), `--big-snapshot-hz`
`--no-delta` and `--no-far-rates` (snapshots: leave them at their defaults unless a load test says otherwise),
`--sim-threads N` (the threads rooms step on: 0, the default, one a core; 1
the sim thread alone).

## Backups

`/home/master/oneshot-data` is the record: replays are the evidence behind
every reported result and every ban. Back up `replays/` and keep
`server.pem` somewhere safe; losing the key means entering a new one on the
dev desk, nothing more. `nonce.key` and `live` are remade by the deploy
script; the certificates and the slots' binaries by the hook and the script.

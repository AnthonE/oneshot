# Hosting oneshot

One server process on the game box (`game.elopros.com`, the Hetzner VM
Gates runs on), its page on elopros.com. The way Gates is hosted, with two
differences a ranked game needs: a renewal **reloads** the certificate
instead of restarting, and a stop **drains**, so no match is cut short.

## The pieces

| What | Where | How |
|---|---|---|
| The game (WebTransport, UDP) | `game.elopros.com:61250` | `oneshot.service`, user `master` |
| `server.json`, `/nonce`, `/ready` | `https://game.elopros.com/oneshot/…` | the box's nginx on 443 → `127.0.0.1:8450` (`game-elopros-oneshot.nginx.conf`) |
| The page | `https://elopros.com/games/oneshot/play/` | the platform's nginx, `alias /data/apps/scry-data/oneshot-web/current/` on `morr` (`ci/publish_web.sh`) |
| Certificate | `/home/master/oneshot-certs/{fullchain,privkey}.pem` | Let's Encrypt for `game.elopros.com`, copied by `certbot-deploy-hook.sh` |
| Binary | `/home/master/oneshot-deploy/oneshot-server` | `ci/deploy_server.sh` |
| Data | `/home/master/oneshot-data/` | `replays/` (with `review/`, `cups/`), `server.pem` |

No Docker; no `cargo` at run time. The unit runs the staged binary, never the
checkout.

## The first time, on the box

1. **The port.** UDP 61250 in **both** ufw and the Hetzner Cloud Firewall (the
   cloud firewall passes only 49152–65535; ufw alone does nothing):
   `sudo ufw allow 61250/udp`, then the same rule in the Hetzner console.
   Taken already: Gates 61234, Before Colony 61240, Fireplace 61290–61297,
   kit rooms 61300–61315.
2. **The server key** that signs ranked results:
   `openssl genpkey -algorithm ed25519 -out /home/master/oneshot-data/server.pem`,
   mode 600, owned by `master`. Its public half
   (`openssl pkey -in /home/master/oneshot-data/server.pem -pubout -outform DER | tail -c 32 | base64`)
   goes on Elo Pros' dev desk for `oneshot`, id `eu-1` (`meter/serverkeys.py`;
   signed by the keeper); `POST /api/serverkeys/check` tells the server's
   signing apart from a wrong key. The unit won't start without the file.
3. **The certificate.** The box's `game.elopros.com` lineage already exists
   (Gates'). Install the hook and run it once by hand to make the copies:
   `ci/deploy_server.sh --go` installs it; `sudo /etc/letsencrypt/renewal-hooks/deploy/copy-oneshot-cert.sh`.
4. **nginx**: `include /home/master/oneshot-deploy/game-elopros-oneshot.nginx.conf;`
   inside the `game.elopros.com` 443 server block, then `sudo nginx -t && sudo systemctl reload nginx`.
5. **UDP buffers.** The server asks for 8 MiB each way and prints what it got
   if the kernel clamped it. To grant it:
   `sudo sysctl -w net.core.rmem_max=16777216 net.core.wmem_max=16777216`
   (and the same in `/etc/sysctl.d/90-oneshot.conf`).
6. `ci/deploy_server.sh --go`, then from a workstation `ci/publish_web.sh`.
7. On Elo Pros: the listing's `play_url` (`/games/oneshot/play/`) and the
   nginx block for the page (scry-forge `deploy/nginx/elopros.com.conf`).

## Day to day

- **Release the server:** `ci/deploy_server.sh` (a dry run), then `--go`. The
  restart drains: matches finish first, so a deploy can take up to
  `--drain-max` (15 minutes) while new players are told to come back in a
  minute. `curl -s https://game.elopros.com/oneshot/ready` says when it's back.
- **Release the page:** `ci/publish_web.sh`. It refuses a page whose
  `PROTO_VER` the live server doesn't speak: deploy the server first.
  Rollback is one `ln -sfn <previous build> current` on `morr`.
- **Renewal:** certbot runs the hook; the hook copies and `systemctl reload
  oneshot` (SIGHUP). Nobody is dropped. `journalctl -u oneshot | grep certificate`.
- **Logs:** `journalctl -u oneshot -f` — reports sent, drains, reloads.
- **Bans and reviews:** `oneshot-review --replays /home/master/oneshot-data/replays list`
  (then `show`, `done`); `ban` and `lift` take `--elo-key
  /home/master/oneshot-data/server.pem --elo-key-id eu-1`. Bans live on Elo
  Pros, signed with the server key.

## Backups

`/home/master/oneshot-data` is the record: replays are the evidence behind
every reported result and every ban. Back up `replays/` and keep
`server.pem` somewhere safe; losing the key means entering a new one on the
dev desk, nothing more. The certificates and the deploy directory are
rebuilt by the hook and the deploy script.

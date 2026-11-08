# oneshot

A free-to-play, Halo-style shooter for the desktop and the browser: 1v1 and
2v2 ranked, and an 8v8 Big Team mode with terrain and vehicles, with an
authoritative server and fog of war, so it can be ranked without
trusting anyone's machine.

- [`docs/DESIGN.md`](docs/DESIGN.md): the design, netcode first.
- [`docs/RIP.md`](docs/RIP.md): what we take from Gates, Fireplace,
  engine-shim and Elo Pros, file by file.
- [`docs/SCALING.md`](docs/SCALING.md): how it grows past one box.

## Contributing

People and agents both build oneshot, and accepted work is paid: $2 a merged
pull request, $10 more for a map that enters a rotation, and a bounty for
breaking the game's guarantees. [`CONTRIBUTING.md`](CONTRIBUTING.md) has the
scope and the checks, [`SECURITY.md`](SECURITY.md) how to report a cheat
privately, and [`AGENTS.md`](AGENTS.md) is where an agent starts.

## Milestone 1: the rules

`crates/sim` is the game's rules in deterministic Rust: Quake-style
movement, Halo's guns, shields and head-shot rules, rockets and frags,
weapon pads, Slayer, and a lag-compensation rewind whose depth the server
chooses. The same code runs on the server and, compiled to wasm, in the
browser's prediction.

```sh
ci/quick.sh sim     # fmt, clippy -D warnings, the sim's tests
ci/parity.sh        # native and wasm hash the same bot matches
```

## Milestone 2: one playable room

```sh
ci/client.sh                            # the Bevy client: clippy, native, the page (WebGL2 and WebGPU) into crates/client/dist
node tools/assets.mjs ../Gates          # only to refresh crates/client/assets from a Gates checkout
cargo run -p server --release           # then open http://127.0.0.1:8080 in Chrome
```

The desktop client is the same code (`crates/client/DESKTOP.md`):

```sh
cd crates/client && cargo run --release --features sound -- --server 127.0.0.1:8080
oneshot                                 # a packaged build: the public server
```

`.github/workflows/desktop.yml` builds it for Linux, Windows and macOS on
every client change (the run's artifacts), and puts the three on a release
for a `desktop-v*` tag.

| crate / dir | what |
|---|---|
| `crates/proto` | the wire: inputs, snapshots, stream messages, bit-packed, goldens |
| `crates/room` | a match as the server runs it: seats, input buffers, rewind depth, fog, snapshots and deltas, replays |
| `crates/server` | WebTransport (wtransport), the 128 Hz rooms (seated on the sim thread, stepped on its pool, `--sim-threads`), the ranked queue, Elo Pros, the page server |
| `crates/client-net` | the client's netcode: clock, prediction, interpolation, WebTransport (wasm), wtransport (native) |
| `crates/client` | the Bevy client, desktop and browser: the level, Triple T, the guns in view, HUD, sound, the menu |
| `crates/mix`, `crates/mix-worklet` | the sound mixer; in the browser, on its audio thread (an AudioWorklet) |
| `crates/fx` | effects with no engine, vendored from Gates |
| `tools/load` | `oneshot-load`, the load test (`ops/README.md`) |

```sh
ci/quick.sh sim proto room client-net server   # fmt, clippy, tests
npm ci                                         # once: Playwright, for the two below
node ci/e2e.mjs                                # the page: its menu, then it plays a bot in headless Chromium
node ci/look.mjs                               # pictures of the models (traces/)
```

## Milestone 4: ranked

Sign in with Elo Pros, queue for Duel or Doubles, play; the result goes to
the Elo Pros ladder signed with this server's key, and the replay to disk.

```sh
oneshot-server --domain play.example.com \
  --elo-pubkey "$(curl -s https://elopros.com/api/account/pubkey | jq -r .pubkey)" \
  --elo-key /etc/oneshot/server.pem --elo-key-id eu-1 --replays /var/lib/oneshot/replays
cargo run -p server --release --bin oneshot-replay -- replays/os-*.osr   # re-run and check
```

The server's key is `openssl genpkey -algorithm ed25519 -out server.pem`; its
public half goes on Elo Pros' dev desk for `oneshot` (`ops/README.md`).

## Milestone 6: modes, maps, skins, cups

- **Casual:** against the bot, a free for all, Gun Game (a kill a rung up a
  ladder of seven guns), Big Team (8v8 on terrain, with vehicles) or Capture
  the Flag, bots in the empty seats; the shared rooms take a party together,
  or give it a room of its own. Or watch a live match, a little late.
- **Kill cam:** the run-up to your death from the killer's eye, made only
  of what fog already sent you.
- **Maps** are data: `maps/<name>/map.json`, compiled to the sim's tables.
  Proving Grounds, Overpass, Cistern, Bulkhead and Chrysalis (inside an
  alien hull: pods ringing bottom mid, top mid over it, curved bases with
  windows, rotations that wrap: the old arena flow, our own layout); Dry
  Fork is Big Team's (terrain, vehicles); The Range and Cloudbreak are out
  of the rotation (`--map range`). A room draws from its mode's rotation by
  its seed. Maps have round, turned and sloped solids beside
  their boxes, waypoints for bots, and their own light.
- **Skins:** worn only if your wallet owns them (Elo Pros' items), drawn as
  trim over the side's colour.
- **Cups:** Swiss rounds of free-for-all heats, a final of the top eight,
  each heat reported on the `cup` ladder, and the top places' heats queued
  for review before any prize.

```sh
node tools/maps/compile.mjs           # after editing a map.json (ci checks it with --check)
cargo run -p flow --release -- chrysalis   # its plan, walk times and sight lines (traces/flow/)
cargo test -p sim --test maps         # every map: spawns, pads, walkable, bots fight
ci/parity.sh                          # every map native vs wasm: Doubles, Gun Game, Big Team, CTF where it has flags
node ci/e2e.mjs 150 4 2               # the page in Gun Game on Cistern against seven bots
oneshot-server --map cistern --cup-min 8 --cup-rounds 3
```

## Milestone 7: hosting

The server on the game box beside Gates (`game.elopros.com`, UDP 61250 and
61251), the page on elopros.com (`/games/oneshot/play/`). `ops/README.md` is
the runbook.

- **Always one taking players:** two slots. A deploy starts the new build in
  the idle one, points `--live-file` at it (every `server.json` says so),
  then drains the old; a page turned away by the old one follows to the new
  with the same sign-in (`--nonce-key`, shared).
- **A real certificate, never pinned:** with `--cert`, `server.json` sends no
  hash and the browser checks the certificate itself; `--public-addr` names
  where to connect, `--allow-origin` lets the page on elopros.com read it.
- **Admission:** past `--capacity` a hello is answered "full"; past twice it,
  an unproven address gets a QUIC Retry; past four times, a refusal; 16
  connections (eight players) per address, 64 per prefix.
- **SIGTERM drains:** no new players, every match plays out (at most
  `--drain-max`), its replay and report go out, then it exits.
- **SIGHUP reloads** the certificate: an ACME renewal drops nobody.
- `/health`, `/ready` (503 while draining; the wire, players, capacity),
  `/health.json` (everything counted since start).

```sh
ci/deploy_server.sh            # on the box: a dry run; --go starts the idle slot, switches, drains the old
ci/publish_web.sh              # from a workstation: build, upload to morr, flip current
cargo test -p server --test hosting
```

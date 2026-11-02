# oneshot

A free-to-play, Halo-style arena shooter for the browser: 1v1 and 2v2 ranked,
with an authoritative server and fog of war, so it can be ranked without
trusting anyone's machine.

- [`docs/DESIGN.md`](docs/DESIGN.md): the design, netcode first.
- [`docs/RIP.md`](docs/RIP.md): what we take from Gates, Fireplace,
  engine-shim and Elo Pros, file by file.

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
node tools/assets.mjs ../Gates          # once: models, sounds, textures from a Gates checkout
ci/build_web.sh                         # client-net to wasm, then the page into web/dist
cargo run -p server --release           # then open http://127.0.0.1:8080 in Chrome
```

| crate / dir | what |
|---|---|
| `crates/proto` | the wire: inputs, snapshots, stream messages, bit-packed, goldens |
| `crates/room` | a match as the server runs it: seats, input buffers, rewind depth, snapshots |
| `crates/server` | WebTransport (wtransport), the 128 Hz sim thread, the page server |
| `crates/client-net` | the browser's netcode: clock, prediction, interpolation, WebTransport (wasm) |
| `web/` | three.js: the level, Triple T, the guns in view, HUD, sound |

```sh
ci/quick.sh sim proto room client-net server   # fmt, clippy, tests
node ci/e2e.mjs                                # the page plays a bot in headless Chromium
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
public half goes in the game's `elo.json` `server_keys` on Elo Pros.

## Milestone 6: modes, maps, skins, cups

- **Casual:** against the bot, a free for all, or Gun Game (a kill a rung
  up a ladder of seven guns), bots in the empty seats; the free-for-all
  rooms are shared.
- **Kill cam:** the run-up to your death from the killer's eye, made only
  of what fog already sent you.
- **Maps** are data: `maps/<name>/map.json`, compiled to the sim's tables.
  Proving Grounds, Overpass and Cistern; a room draws one from its seed.
- **Skins:** worn only if your wallet owns them (Elo Pros' items), drawn as
  trim over the side's colour.
- **Cups:** Swiss rounds of free-for-all heats, a final of the top eight,
  each heat reported on the `cup` ladder, and the top places' heats queued
  for review before any prize.

```sh
node tools/maps/compile.mjs           # after editing a map.json (ci checks it with --check)
cargo test -p sim --test maps         # every map: spawns, pads, walkable, bots fight
ci/parity.sh                          # every map native vs wasm, Doubles and Gun Game
node ci/e2e.mjs 40 4 2                # the page in Gun Game on Cistern against seven bots
oneshot-server --map cistern --cup-min 8 --cup-rounds 3
```

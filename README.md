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

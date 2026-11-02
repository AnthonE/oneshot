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

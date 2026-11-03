# CLAUDE.md: oneshot

A free-to-play, Halo-style shooter for the desktop and the browser: 1v1 and
2v2 ranked, and an 8v8 Big Team mode with terrain and vehicles on the way.
The server is the truth; the client predicts only its own fighter (and the
vehicle it drives) and is sent only what it could see. `docs/DESIGN.md` is the design of
record; `docs/RIP.md` says what to take from Gates, Fireplace, engine-shim
and Elo Pros. Milestones are `DESIGN.md` §16; 7 to 12 are the growth into a full shooter
(entity pools, 16 seats, terrain, vehicles, the delta wire, the Bevy client).

| crate | what |
|---|---|
| `crates/sim` | the rules: movement (slopes, launchers), guns (`arms.rs`: hitscan, projectiles, heat, charge, the sword's lunge), shields, pads, vehicles (`vehicle.rs`), Slayer, rewind; the map's box grid (`grid.rs`) and terrain (`terrain.rs`). Native on the server, wasm in the browser |
| `crates/sim-wasm` | the sim as a bare wasm module, for the parity gate |
| `crates/proto` | the wire: bit-packed messages, goldens, `PROTO_VER` |
| `crates/room` | a match as the server runs it, no transport: seats, input buffers, rewind depth, fog, snapshots, replays, the aim review's numbers (`watch.rs`) |
| `crates/server` | wtransport sessions, rooms on the sim thread, the ranked queue, Elo Pros sign-in (`auth.rs`) and reports (`elo.rs`), the page server; `oneshot-server`, `oneshot-replay`, `oneshot-review` |
| `crates/client-net` | the client's netcode: `core` (clock, prediction, interpolation; native-tested), `session` (handshake, snapshots in, inputs out over a `Wire`), `web` (wasm, WebTransport), `native` (feature `native`, wtransport) |
| `crates/client` | the Bevy client, desktop (`oneshot --server host:port`) and browser (`crates/client/dist`): draws, never decides. Its own workspace; `ci/client.sh` is its check |
| `web/` | three.js page: draws, never decides. Replaced by the Bevy client once it plays as well (`DESIGN.md` §16, milestone 12) |
| `maps/`, `tools/maps` | maps as `map.json`; the compiler writes `crates/sim/src/maps/` |
| `ops/`, `ci/deploy_server.sh`, `ci/publish_web.sh` | hosting: the systemd unit, the certbot hook, the game box's nginx, the runbook (`ops/README.md`) |

## The loop

1. Write the code.
2. `ci/quick.sh sim`: fmt, clippy with `-D warnings`, sim's tests.
3. `ci/parity.sh` when you touch the sim: native and wasm must hash equal.
   Touching a gun (`arms.rs`, the sword's numbers in `consts.rs`): the range,
   `cargo test -p sim --test range` (Halo 2's counts and times, pinned), and
   `-- --ignored --nocapture` prints the whole table to tune by. To feel
   them: `oneshot-server --map range` (every gun on a pad, never in the
   rotation). New guns borrow a model in `web/src/art.ts` and
   `crates/client/src/look/guns.rs` until they have their own.
4. Touching the page or the netcode: `ci/build_web.sh && node ci/e2e.mjs`
   (and `node ci/e2e.mjs 40 4 2` for eight seats on another map,
   `node ci/e2e.mjs 60 6 3` for sixteen in Big Team on Dry Fork's terrain).
5. Touching the Bevy client: `ci/client.sh` (clippy, native, wasm into
   `crates/client/dist`; serve it with `oneshot-server --web crates/client/dist`),
   then `node ci/client_e2e.mjs` (it must score a kill) and
   `node ci/client_shot.mjs traces/x.png 25 6 3 1` to look at it. Native
   sound is `--features sound` (needs ALSA); the browser always has it.
6. Touching a map: edit its `map.json`, `node tools/maps/compile.mjs`, then
   `cargo test -p sim --test maps`. Never edit `crates/sim/src/maps/` by
   hand; Proving Grounds is the golden's map.
6. Touching the look: `node tools/board.mjs look` (after the two builds):
   the art's views beside their references and the operator's marks, one
   sheet (`board/board.json`; the elo-look skill is the loop).
7. Commit with a short message.

## The wire's walls

- **A wire change bumps `PROTO_VER`** and moves `crates/proto/tests/goldens.rs`.
- **The sim thread never blocks on the network**: sessions get `try_send`,
  a full queue drops a snapshot.
- **The clock steers on the low point** of the server's input buffer
  (`ClientNet::take_low`); a page drawing slowly sends in bursts.
- **The client draws, never decides** (`web/` today, the Bevy client next).
  Gameplay state lives in `sim` and `client-net`.
- **web/ draws.** Gameplay state lives in `sim` and `client-net`.
- **Production never pins.** With `--cert`, `server.json` sends no hash;
  SIGTERM drains (matches finish), SIGHUP reloads the certificate. Hosting
  is the game box beside Gates, two slots so one always takes players
  (`--live-file`, a shared `--nonce-key`); `ops/README.md`.
- **Every byte to a client goes through `room::build`** and its fog
  (`room/src/fog.rs`): an enemy only if it could be seen soon, an unseen
  fighter's shot only as a rounded sound, never an unseen spawn. The leak
  gate (`room/tests/leaks.rs`) plays a whole match and fails on any leak;
  a new field on the wire needs a line there.
- **Every call that moves a world goes through `Room`** (`world_seat`,
  `step`), so the replay records it. A replay that doesn't re-run to the
  server's hash is a bug in the sim or the room, never in the replay.
- **Ranked is a wallet's.** A pass without a wallet plays casual only; a
  rating is reported only for wallets. A cup is ranked.
- **Skins are cosmetic and checked.** Worn only if the wallet owns it; never
  in the sim, never a hitbox, a sound or a fighter's visibility.
- **The seed is not the salt.** What clients watch (spawns, bots) comes from
  the seed; the salt keys spread and is revealed only at `Phase::Over`.

## The sim's walls (CI stops you on these)

- **Deterministic.** No I/O, clock, threads, `HashMap` iteration or heap
  strings (`crates/sim/clippy.toml`). Floats are `+ − × ÷ sqrt min max`;
  trig, round, floor and friends go through `sim::fmath` (pure-Rust libm),
  never `f32::sin`. No `mul_add`.
- **No allocation in `World::step`** after setup (`tests/alloc.rs`). Fixed
  arrays, caps in `consts.rs`.
- **Durations in ms, speeds per second**, turned into ticks by
  `consts::ticks`. `TICK_HZ` is one constant; never write a tick count.
- **The golden moves only on purpose.** `tests/determinism.rs` pins
  `bot_match(1, 20000)`. A rule change updates it in the same commit; a
  compiler bump that moves it is a finding.
- **`Fighter::step_local` is what the client predicts** (and, once vehicles
  land, `Vehicle::step_local` for a driver). Each may read only its own
  state, its seat's input and the map. Anything touching another fighter or
  vehicle goes in `World`. `the_prediction_matches_the_server_bit_for_bit`
  holds it.
- **The client never names its own lag.** `World::step` takes each seat's
  rewind depth from the server.
- **The salt never leaves the server** while a match runs.

## Rules that stay

- Numbers go in code (`consts.rs`, `arms.rs`), not in docs.
- Assets are CC0, CC-BY (credited in NOTICE) or ours. No NC or SA. Never
  another game's art.

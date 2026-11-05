# CLAUDE.md — oneshot

**MVP fast-iteration mode**, as in Gates (operator, 2026-11-04). Ship code.
This page is the whole process. The longer page it replaced is in git:
`git show 93e49f5:CLAUDE.md`. Grep it when something weird happens. Don't
read it up front.

## What this is

A free-to-play, Halo-style shooter for the desktop and the browser: 1v1 and
2v2 ranked, and an 8v8 Big Team mode with terrain and vehicles. The server is
the truth. The client predicts only its own fighter (and the vehicle it
drives) and is sent only what it could see. `docs/DESIGN.md` is the design of
record (§16 is the milestones); `docs/RIP.md` says what to take from Gates,
Fireplace, engine-shim and Elo Pros.

| crate | what |
|---|---|
| `sim` | the rules: movement, guns (`arms.rs`), shields, pads, vehicles (`vehicle.rs`), Slayer, rewind; maps' box grids, terrain, solids (`solid.rs`), bots' waypoints. Native on the server, wasm in the browser |
| `sim-wasm` | the sim as a bare wasm module, for the parity gate |
| `proto` | the wire: bit-packed messages, goldens, `PROTO_VER` |
| `room` | a match with no transport: seats, input buffers, rewind, fog, snapshots, replays, the aim review (`watch.rs`) |
| `server` | sessions, rooms on the sim thread, the ranked queue, Elo Pros sign-in (`auth.rs`) and reports (`elo.rs`), the page; `oneshot-server`, `-replay`, `-review` |
| `client-net` | the client's netcode: clock, prediction, interpolation (`core`), the session, the web and native transports |
| `client` | the Bevy client, desktop and the page (`crates/client/dist`; `web/boot.js` finds the server, signs in and does the party's voice, `web/voice.js`). Its screens are Gates' style, on both: the title (`front.rs`: PLAY, PARTY, FRIENDS; connecting is its state, `door.rs`), the Esc menu, the settings; a pad moves on them (`pad.rs`). The party's lobby is held from the title on (`party.rs`). Its own workspace (`ci/client.sh`). Draws, never decides |
| `mix`, `mix-worklet` | the game's sound mixer, no engine in it: the desktop plays it as a Bevy audio source, the browser runs it on its audio thread (`mix-worklet`, an AudioWorklet: `web/mix.js`, `web/mix-worklet.js`); `crates/client/src/sound` decides what it plays |
| `fx` | effects with no engine: the particle pool, sprite sheet, recipes and knobs, the fx stage's contract. **Vendored from Gates** (`tools/fx-sync.sh`) |
| `maps/`, `tools/maps`, `tools/flow` | maps as `map.json`, compiled into `crates/sim/src/maps/`; `oneshot-flow` reads a map's flow |
| `tools/models` | the guns, ours: a Blender script each on `lib.py`'s kit, exported to `crates/client/assets/models`; `truck.py` makes the truck (`look/truck.rs`) from a download kept in `sources/` (CC-BY, NOTICE) |
| `tools/assets.mjs`, `tools/glb` | Triple T, sounds and textures from a Gates checkout into `crates/client/assets` |
| `ops/`, `ci/deploy_server.sh`, `ci/publish_web.sh` | hosting: two slots beside Gates; the runbook is `ops/README.md` |

## The loop

1. Write the code.
2. `ci/quick.sh`: the maps' check, fmt, clippy (`-D warnings`; the sim's
   walls are `crates/sim/clippy.toml`) and the wasm clippy.
   Add crate names to run their tests too (`ci/quick.sh sim room`). Then
   only what you touched:
   - **the sim:** `ci/parity.sh`;
   - **a gun:** `cargo test -p sim --test range`;
   - **a map:** `cargo test -p sim --test maps`;
   - **the client or the netcode:** `ci/client.sh`, then
     `cargo build --release -p server && node ci/e2e.mjs`;
   - **an effect:** `tools/fx.sh <name>` (the `fx` skill);
   - **a model:** `tools/models/build.sh <name>` (Blender headless; the
     sheet lands in `traces/guns/`, the numbers go in `look/guns.rs`);
   - **the look:** `node tools/board.mjs look`.
3. Commit with a short message. Push your branch.

CI (`.github/workflows/ci.yml`) runs all of it on every PR: quick's checks,
the crates' tests, parity, `ci/client.sh` and two e2e matches.

## Don't

- Update `docs/` unless asked. `DESIGN.md` changes when the operator changes
  the design.
- Write findings notes, research docs or long commit bodies.
- Add a test for every change. The goldens, the range and the leak gate
  already hold a lot; add one when the logic is tricky or has broken before.
- Read the big docs before starting. Grep `docs/` when you need an answer.
- Edit `crates/fx`, `tools/fxstrip.mjs` or `tools/board.mjs`. They are
  vendored (Gates; the Elo Pros agent kit): fix them upstream, then sync.
- Edit `crates/sim/src/maps/` by hand. Edit the map's `map.json`, then
  `node tools/maps/compile.mjs`.
- Write numbers anywhere but code (`consts.rs`, `arms.rs`, `look/fx/table.rs`).

## What CI will stop you on (so you know why)

1. **The sim is deterministic.** No I/O, clock, threads, `HashMap`
   iteration or heap strings (`crates/sim/clippy.toml`). Floats are
   `+ − × ÷ sqrt min max`; trig, round and floor go through `sim::fmath`
   (pure-Rust libm), never `f32::sin`. No `mul_add`.
2. **Native and wasm hash equal** (`ci/parity.sh`).
3. **No allocation in `World::step`** after setup (`tests/alloc.rs`). Fixed
   arrays, caps in `consts.rs`.
4. **The golden moves only on purpose.** `tests/determinism.rs` pins
   `bot_match(1, 20000)`. A rule change updates it in the same commit; a
   compiler bump that moves it is a finding.
5. **`Fighter::step_local` and `Vehicle::step_local` are what the client
   predicts.** Each reads only its own state, its seat's input and the map;
   anything touching another body goes in `World`
   (`the_prediction_matches_the_server_bit_for_bit`).
6. **Wire change:** bump `PROTO_VER`; `crates/proto/tests/goldens.rs` moves
   with it.
7. **Every byte to a client goes through `room::build` and its fog**
   (`room/src/fog.rs`): an enemy only if it could be seen soon, an unseen
   shot only as a rounded sound, never an unseen spawn. The leak gate
   (`room/tests/leaks.rs`) plays a whole match and fails on any leak; a new
   field on the wire needs a line there.
8. **Maps:** `node tools/maps/compile.mjs --check` (the Rust matches the
   JSON), `cargo test -p sim --test maps`. Proving Grounds is the golden's
   map.
9. **Guns:** `cargo test -p sim --test range` pins Halo 2's counts and times.
   `-- --ignored --nocapture` prints the table to tune by;
   `oneshot-server --map range` puts every gun on a pad to feel them.
10. **The page is two modules** (`ci/client.sh`): `oneshot_client` (WebGL2)
    and `oneshot_client_gpu` (`--features webgpu`, the desktop's frame); it
    picks one before either downloads. Their gzipped sizes are printed, not
    gated (no size, fog or frame-rate budget stops a build any more; the
    tools that measure them stay). What WebGL2 can't do is `cfg(webgl2)`
    (`build.rs`), never `target_arch = "wasm32"`; the tier is `look/quality.rs`
    (`Tier`, `for_target`, `effective_on`), the device's caps `look/gpu.rs`.
11. **`crates/fx` is Gates'** (`crates/fx/tests/vendored.rs`).

## Commands

```
ci/quick.sh [crate…]                                 # the local check
ci/parity.sh                                         # native vs wasm
ci/client.sh                                         # the client: clippy, native, the page's two modules into crates/client/dist
ci/client.sh dev                                     # just the page, quickly (release, no fat LTO); both modules
cargo build --release -p server && target/release/oneshot-server   # serves crates/client/dist
target/release/oneshot-server --map range            # every gun on a pad, never in the rotation
node ci/e2e.mjs [150 4 2 | 150 6 3]                  # the title, a match: predictions confirmed, a kill (E2E_WEBGPU=1: the WebGPU module too)
node ci/client_shot.mjs traces/x.png 25 6 3 1        # look at a match in a browser (… 30 0 0 0 fxdemo=1 for the demo;
                                                     #   GFX=webgpu for the WebGPU module, ?gfx=webgpu|webgl2 on the page)
ci/desktop_shot.sh traces/x.png 4 6 3 1 menu         # the same on the desktop (Xvfb, lavapipe; title: … 2 0 0 0,
                                                     #   settings, pose='map=6&eye=…&look=…'); F12 in game saves one
                                                     # either: quality=potato|low|medium|high draws that rung (a software
                                                     #   GPU is otherwise POTATO in a browser: look/potato.rs), graphics opens
                                                     #   SETTINGS > GRAPHICS (`?graphics=1` / `--graphics`)
tools/fx.sh list | <name>… [--knobs 'Layer.key=v;…'] # the fx stage: one effect's strip → traces/fx/<name>/
node tools/board.mjs look [--views a,b]              # the reference board: views beside refs and marks
ci/native_shot.sh traces/x.png 'map=6&eye=…' | match 6 3   # a GPU shot of the desktop client in seconds (a pose, a match)
tools/models/build.sh [name…]                        # the guns' GLBs from tools/models/*.py (Blender 4.0, headless)
node tools/maps/compile.mjs                          # maps' JSON → crates/sim/src/maps/
cargo run -p flow --release -- <map>                 # a map's flow (traces/flow/; node tools/flow/png.mjs)
node tools/assets.mjs [GATES_DIR]                    # art and sound from a Gates checkout
tools/fx-sync.sh [GATES_DIR]                         # the fx crate and fxstrip from a Gates checkout
```

## Traps that already cost real time

- The client is its own workspace. `cargo` at the root never builds it;
  `ci/client.sh` (or `cd crates/client`) does.
- The page's real build (`--profile web`, fat LTO, two modules) is slow.
  `ci/client.sh dev` and `tools/fx.sh` build release, which is fine for
  looking but not what ships (`ci/publish_web.sh` refuses a dev page).
- Headless Chromium draws WebGPU only on SwiftShader's Vulkan
  (`--enable-unsafe-webgpu --enable-features=Vulkan --use-vulkan=swiftshader
  --use-angle=vulkan`); with ANGLE on GL the device is lost at once and the
  page falls back to WebGL2. Its adapter is a software one, so the page
  takes WebGPU there only with `?gfx=webgpu`.
- `ci/client_shot.mjs` and `ci/desktop_shot.sh` count their seconds from
  when the screen is there (`loading.rs`: level dressed, shaders compiled),
  then on the wall clock, so whether a 50 ms flash is in the shot is luck.
  Stage effects with `tools/fx.sh`, which steps the clock.
- `ci/e2e.mjs` needs the release server built first.
- `wasm-bindgen` must match `crates/client/Cargo.lock` exactly; `ci/client.sh`
  names the version it wants.

## Fresh box

- `rustup show` installs the pinned toolchain and the wasm target.
- `cargo install wasm-bindgen-cli --version <crates/client/Cargo.lock's> --locked` (0.2.130 today).
- `npm ci` once for Playwright. Chromium at `/opt/pw-browsers/chromium` is
  used if present (`CHROMIUM_PATH` overrides), else `npx playwright install chromium`.
- The desktop client needs `libwayland-dev libudev-dev libxkbcommon-x11-0`;
  native sound (`--features sound`) needs `libasound2-dev`. Its headless
  shots (`ci/desktop_shot.sh`) need `apt-get install xvfb mesa-vulkan-drivers`.
- A Gates checkout beside this one (`../Gates`) for `tools/assets.mjs` and
  `tools/fx-sync.sh`.

## Rules that stay

- **The client draws, never decides.** Gameplay state lives in `sim` and
  `client-net`; `crates/client` reads a session and sends the controls.
- **The sim thread never blocks on the network.** Sessions get `try_send`;
  a full queue drops a snapshot.
- **The clock steers on the low point** of the server's input buffer
  (`ClientNet::take_low`, `room`). A page drawing slowly sends in bursts.
- **The client never names its own lag.** `World::step` takes each seat's
  rewind depth from the server.
- **Every call that moves a world goes through `Room`** (`world_seat`,
  `step`), so the replay records it. A replay that doesn't re-run to the
  server's hash is a bug in the sim or the room, never in the replay.
- **Production never pins.** With `--cert`, `server.json` sends no hash;
  SIGTERM drains, SIGHUP reloads the certificate (`ops/README.md`).
- **Ranked is a wallet's.** A pass without a wallet plays casual; a rating
  is reported only for wallets. A cup is ranked.
- **Skins are cosmetic and checked:** worn only if the wallet owns it,
  never in the sim, a hitbox, a sound or a fighter's visibility.
- **The seed is not the salt.** What clients watch comes from the seed; the
  salt keys spread, never leaves the server while a match runs, and is
  revealed only at `Phase::Over`.
- **Gates is a donor, never a dependency** (`docs/RIP.md` §9). We copy and
  trim; `crates/fx` is vendored, not depended on.
- `tools/board.mjs` is the Elo Pros agent kit's
  (`skills/elo-look/scripts/board.mjs`, not in this repo). Fix it there and
  copy it back.
- Assets are CC0, CC-BY (credited in NOTICE) or ours. No NC or SA. Never
  another game's art.

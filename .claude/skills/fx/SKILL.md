---
name: fx
description: Make or tune a visual effect in oneshot (impacts, muzzle flashes, tracers, shields taking rounds, a frag or rocket going off, plasma) on the fx stage: stage it alone in the browser build in front of a dark and a light card, read its filmstrip and numbers, retune it with knobs without a rebuild, compare against the last run, and bake the result into the recipe. Use when asked to add, fix, tune, compare or review an effect or a particle look.
---

# Effects in oneshot

An effect is judged as it moves, never from a single still. The loop is:
1. stage the effect alone;
2. read its **strip** (every frame, its envelope and its numbers);
3. change one thing;
4. read the strip again and its **diff** against the last run.

The engine is the shared `fx` crate (`crates/fx`, Bevy-free), **vendored from Gates**. Gates
holds the one copy; `tools/fx-sync.sh` brings it here with `tools/fxstrip.mjs`. Our half is
`crates/client/src/look/fx/`.

```bash
tools/fx.sh list                                  # every effect the stage can fire
tools/fx.sh hit-bullet-metal                      # → traces/fx/hit-bullet-metal/{strip.png,numbers.json,diff.png}
tools/fx.sh hit-bullet-metal --knobs 'Sparks.speed=8,16;Flash.life=0.03,0.05'   # retune, no rebuild
tools/fx.sh grenade --seed 7                      # another roll of the dice
tools/fx.sh grenade --frames 15 --span 1.5        # more frames over a shorter span
tools/fx.sh --no-build …                          # reuse the last page
node tools/board.mjs look --views fx-grenade      # the strip on the board, beside its refs and marks
```

**Read `strip.png` first.** Its parts:
- **The hero frame:** the brightest frame, drawn large.
- **The envelope:** how much each card changed over time; the last run is dashed.
- **Every frame:** captioned with its step, time, live particles (glow/soft) and per-card numbers.

Then read `numbers.json` for anything the picture leaves open, and `diff.png` after a change.

Setup and speed:
- `tools/fx.sh` builds the page in release (thin LTO, minutes cold, seconds warm) into
  `crates/client/target/fx-dist`, serves it itself and drives `?pose=1&fx=<name>` in headless
  Chromium on SwiftShader. It needs:
  - `wasm-bindgen-cli` at `crates/client/Cargo.lock`'s version;
  - Playwright (`npm ci`);
  - no game server.
- A rerun with nothing changed shows 0% moved.

## The stage

- **One effect, fired twice at once** (`look/fx/stage.rs`, high over the map):
  - in front of a **dark card** (the left half; light reads here);
  - in front of a **light card** (the right half; smoke and dust read here).
- **The throw:** out of the card, toward the camera and a little up.
- **The camera:** fixed (`fx::stage::Framing`), sized to the effect's radius.
- **The look:** the browser's renderer, so **no bloom**, which is what most players see.
- **The clock** steps exactly 1/60 s (`fx::stage::Clock`, run in `First` before time moves):
  - step 0 is the **plate**, before the effect fires;
  - the effect fires on step 1;
  - the clock holds wherever `window.__fxStep` asks;
  - `window.__fx` reports where it stands.
- **What it fires:** the game's own entry points:
  - a blow: `Fx::impact(weapon, matter, …)`, which leaves its mark on the card too;
  - a muzzle: `shots::flash` with `table::gun(arm)`;
  - an explosion: `blast::blast`;
  - a shield breaking: `bodies::pop`.
- **Names:**
  - `hit-<bullet|pellet|sniper|melee>-<concrete|metal|dirt|shield|flesh|hull>`;
  - `muzzle-<rifle|br|magnum|shotgun|sniper|rocket>`;
  - `grenade`, `rocket`, `cannon`, `shards`, `shield-pop`.

## The numbers

Every number is measured per card against the plate, on sRGB 0–255 Rec.601 luma.

| number | what it says | read it as |
|---|---|---|
| `add` | mean light added over the card | glow: sparks, flash, fire, plasma. Peaks in the first frames of an impact |
| `sub` | mean light taken away | smoke, dust, blood. Mostly on the light card |
| `cover` | % of the card that changed at all | how much of the frame the effect owns |
| `peak` | 99.9th-percentile luma of what changed | 255 = something hit white |
| `clip` | % of the changed pixels at 250+ | a flash may clip for a frame; a cloud never should |
| `bbox`, `c` | where it changed, 0–1 of the card; its centre | spread and drift; `travel` is how far `c` moved |
| `envelope` | `peak`/`peak_t`, `fall_t` (half), `life_t` (last seen), `area_max`, `travel` | the effect's shape in time |
| `vs_prev` | % of each frame that moved since the last run | a change should move only the frames it means to |
| `plate.ok` | the cards read ~25 and ~185 | if not, the light changed and nothing else is comparable |

The numbers keep a floor under the eye; they never replace it. Look at the strip.

## Make or tune one

1. **Write the brief before touching anything** (3–5 lines). It covers:
   - the **envelope**: an instant or sustained, and how long;
   - the **size**;
   - the **colour and temperature** over life;
   - what must read on **each card**;
   - the **reference** (a board ref, the operator's words; Halo's feel, never its art).
2. **Find the recipe.**
   - A blow's layers are `look/fx/table.rs`:
     - `effect(weapon, matter)` picks the layers and counts;
     - `built(layer)` is each layer's `Recipe` (orient, cell, speed, spray, up, gravity, drag, life, size, grow, c0→c1, tinted, spin, stretch, len_max, bounce);
     - `gun(arm)` is each gun's look: muzzle stars, tracer, light;
     - the sprites are `look/fx/atlas.rs` (`CELLS`: Gates' sheet with an arc and a shard).
   - Hand-built effects live in `flying.rs` (plasma, bolts, shards), `blast.rs`, `bodies.rs` and `shots.rs`.
3. **Stage it.**
   - New: add it to `look/fx/stage.rs`'s `list()` (a name, a radius, a span, how it fires).
   - Something `Fire` doesn't cover: give `Fire` a variant that calls its code.
4. **Shoot, read, judge** against the brief, or send the `fx-judge` agent (`.claude/agents/fx-judge.md`) for a read that is not anchored on yours.
5. **Tune with knobs**, one idea at a time: `--knobs 'Layer.key=value;…'`.
   - Values: a number, a range `a,b`, a colour `r,g,b,a` (linear, HDR allowed), `true`/`false`, or `billboard`/`stretch`/`flat`.
   - Layer names are `table::Layer::name()`.
   - `numbers.json`'s `recipes` prints every layer's effective recipe in knob syntax.
6. **Bake it in.** Copy the numbers into `built(layer)` and shoot without knobs (it rebuilds). `diff.png` must show only the frames you meant to change.
7. **Check what you can't see on one roll:**
   - two more `--seed`s;
   - a real match (`node ci/client_shot.mjs traces/x.png 25 6 3 1`), for the effect among everything else;
   - the desktop (`cargo run` in `crates/client`), which has bloom.
8. **Board.** `node tools/board.mjs look --views fx-<name>`.
   - You may `mark fx-<name> note "…" --by <you>`.
   - `right` and `wrong` are the operator's.
   - A new effect gets an `fx-<name>` view in `board/board.json`, sharing the fx views' `shoot` list.
9. **Then:** `ci/client.sh` (the real web build and its 9 MB gate), then commit.

## Judging (after Gates' threejs-procedural-vfx and threejs-visual-validation skills)

- **Every layer has a role:** silhouette, motion, light or residue.
- **One event drives every layer.** An impact peaks in the first two or three frames (`peak_t` ≤ 0.05 s); its residue comes after.
- **It reads with bloom off,** always, here: a hot core on the dark card, streaks never thinner than `fx::pool::MIN_PX`.
- **Alpha layers read on the light card** (`sub` well above 0).
- **Clip is for an instant.** `clip` over ~5% past the first frames is a blown-out blob.
- **Life matches the brief.** `life_t`, `gone`: nothing hangs, nothing pops.
- **Motion from the strip, never one frame; three seeds** for anything random.
- **Budget.** `live` (glow/soft) against the browser's caps (`look/fx/mod.rs` `LOW`: glow 512, soft 256): a 2v2's firefight is dozens of blows at once.
- **Readability before beauty.** A shield hit must say "shield", and a hit on flesh "health", at a glance and at range.

## Rules

- **Never edit `crates/fx` or `tools/fxstrip.mjs` here** (`crates/fx/tests/vendored.rs` fails). Change them in Gates (`crates/fx`, `ci/fxstrip.mjs`), commit there, then run `tools/fx-sync.sh`.
- Our sheet is pinned (`the_sprite_sheet_is_unchanged`). A sprite change is every effect that draws it: shoot them, then repin.
- Numbers live in code (`table.rs`), never in docs. Effects draw; they never decide.

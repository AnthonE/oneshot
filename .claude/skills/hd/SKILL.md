---
name: hd
description: Make a map in oneshot look more like the HD references (the moodboard's hdlook pictures) by the look loop: shoot its fixed views on the GPU in seconds, read each beside its reference and the last pass, name the largest gap, change one thing, shoot again. Use when asked to improve how a map looks (terrain, rock, ground cover, sky, sea, scenery, distance, light), to compare the game with reference pictures, or to add a prop or a texture set to a map.
---

# The HD look loop

A map is judged from fixed eyes, beside the picture it is measured against, never from memory. One step of the loop:

1. **shoot** the views;
2. **read** each beside its reference and the last pass;
3. **name the gap** that owns the most of the frame;
4. **change one thing**;
5. shoot again, and keep the pass.

```bash
tools/hd.sh                         # every view → traces/hd/<view>.png, ~10 s each (the desktop client on this box's GPU, HIGH)
tools/hd.sh beach cliff             # some
tools/hd.sh list                    # the views and their poses
HD_TAG=before tools/hd.sh           # → traces/hd/before/…, to keep a round by hand
ONESHOT_BIN=crates/client/target/release/oneshot tools/hd.sh   # the release client: the sky as it ships (below)
ELO_REF_DIR=moodboard node tools/board.mjs look --views hd-beach,hd-cliff,hd-meadow,hd-vista,hd-inland,hd-base
                                    # shoot, then the sheet: reference | now | last pass, with the marks
node ci/look.mjs 'map=3&eye=127,-0.2,4&look=150,-5,-6'         # the same eye in the WebGL2 page (SwiftShader, minutes)
HD_MAP=9 ELO_REF_DIR=moodboard node tools/board.mjs look --views hd-ice_base,hd-ice_lake,…
                                    # one map's loop: only its views are shot (HD_JOBS=2: two at once, on a GPU)
tools/hd.sh refs                    # the references the board gives a source (public photographs) → moodboard/
```

On a box with no display or GPU (a cloud session) `tools/hd.sh` starts Xvfb and draws on lavapipe: the same frame, about a minute and a half a view (most of it the level's load), so shoot one map (`HD_MAP`), one at a time (a debug client on lavapipe holds 6 to 8 GB, and a second on a 16 GB box is killed; nothing else heavy beside it).

Build first: `cd crates/client && cargo build` (a minute or two after a change; a shader in `assets/shaders` or a file in `assets` needs none). The references are other games' screenshots in `moodboard/` (the hdlook pictures for nature up close, the Blood Gulch ones for a canyon's walls, trails and bases; git-ignored, the board's private refs): a target for the look, never an asset. Nothing is traced, copied or published from them, and no prompt names them.

Icefall's and Pithead's references are real photographs (for light, colour, texture and weather, never a plan). Icefall's are from Wikimedia Commons, each on the board with its source and licence, so `tools/hd.sh refs` fetches them on any box.

The views are `tools/hd.sh`'s `VIEWS` and the board's `hd-*` (`board/board.json`: each with what to take from its reference and what to ignore). A new one goes in both. `hd-judge` (`.claude/agents/hd-judge.md`) reads a view against its reference without your hopes for it.

## A round with judges

One `hd-judge` a view, all at once, each told three things: the pass it is reading (`traces/hd/<view>.png`, the same frame as the newest folder under `board/passes/`), the pass before it by its folder's name (the judge's own rule would take the newest, which is this one), and what changed since, one line a change, so its "since the last pass" can say what each did. Then:

- **Freeze the tree while they read.** A judge reads the code a gap points at; a file that changes under it is reported as a change it could not see in the frame.
- **Read the sheet yourself first.** A defect you can see (something growing through a floor, a colour that is plainly a bug, a pattern that is worse than the last pass) is fixed and shot again before a judge is spent on it. Their read is for what you no longer see.
- **Rank across the views, not within one.** The gap most of them put first is the round's; a view's own gap that no other shares waits, unless it is its whole frame.
- **A view that cannot see its take is the first gap.** Half of Icefall's first eyes stood where a lip, a deck or the view's own subject hid what the view was named for; the judges said so before anything else, and a moved eye loosens one round's comparison and no more.
- **Batch what does not overlap.** A pass is ten minutes on a box with no GPU: a round may change the rock, the snow and the concrete at once, each its own constant and its own line in what the judges are told.

## What the references have

Read a view against this list, top down; the first line that fails over a large share of the frame is the gap.

| layer | the reference | where ours is |
|---|---|---|
| **focus** | every edge crisp | `quality.rs` (`TAA_SHARPEN`), the render scale |
| **ground** | a photograph with grain at the feet, never one flat value across a field | `tools/ground.sh` (the four sets), `textures::GROUND_TILE_M`, `ground::TINT`, `ground.wgsl` (`SOIL_PATCH`) |
| **cover** | grass as a carpet with clumps in it, flowers in drifts, bare tracks | `grass.rs` (where it grows, how thick, the trails), `clutter.rs` (stones, twigs, flowers) |
| **rock** | faces, beds and cracks; a boulder is a boulder | `ground.wgsl` (the beds, the rock face), `cliffs.rs`, `rock.rs`, `boulders.rs` |
| **things standing** | trees with crowns, stacks, structures breaking the skyline | `trees.rs`, `boulders.rs` (`scenery`), the map's `map.json` |
| **distance** | each further thing a step paler; something beyond the map | `rig.rs` (the air), `mood.rs` (dust, mist), `boulders.rs` (scenery out of bounds), `far.rs` (the land beyond the rim) |
| **sky** | cloud with lit tops and grey bases, a deep clear blue | `deck.rs`, `cloud.wgsl`, `deck.wgsl`, `mood.rs` |
| **water** | a hard horizon, the sky in it, rock standing in it | `water.rs`, `water.wgsl` |
| **life** | something moving: spray, dust, motes, grass in wind | `look/fx/` (the pools), `foliage.rs` |
| **light** | sun and shade far apart, shade with colour in it | `mood.rs`, `rig.rs`, `fill.rs`, `quality.rs` (shadows, AO) |

## A valley under snow

Icefall is the third land (`level::Land::Alpine`: a terrain with no void whose mood snows). The same list, where its own numbers are:

| layer | where ours is |
|---|---|
| **ground** | snow is the sand's lane reading the drift set (`ground::ICE_TINT`, `textures::DRIFT_TILE_M`, `tools/snow.py`); the lake is the terrain's own flat (`alpine.lake` in `maps/icefall/map.json`) |
| **rock** | where snow gives way to rock: `level::SNOW_ROCK_SLOPE` a vertex, `ground::SNOW_CLIFF_TAN` a pixel; the wall's shape is the `alpine` block (`cliff`, `crags`, `peaks`, a massif's `flank`), at 2 m a cell |
| **things standing** | the bases, the col's station and the snow shed are boxes in `map.json` (`blast`, `blast_dark`, `steel`, `tread`); boulders in `boulders::COLD_TINT`; snags (`dead_tree`, never in leaf) |
| **distance** | `far.rs` (`SNOW`, `SNOW_ROCK`), `mood::FROST`'s mist |
| **light** | `mood::FROST`: the sun in the south, the south wall's shade over the moraine |

## Changing one thing

- **A texture set.** `tools/ground.sh` fetches a CC0 set from Poly Haven into `sources/polyhaven` and cuts it; it prints the photograph's mean, which the tint in `ground::TINT` is set against, and the size it was shot at goes in `GROUND_TILE_M`. A cliff wants its rock far larger than life (a bed a metre thick reads from the floor; at its own size a wall is one value).
- **A rock or a prop.** Commission it (`tools/models/meshy_gen.py <slug> --multiview --tris 12000 --image-prompt "…"`: the picture first with `--image-only`, look at it, then the mesh; about 40 credits), cut it (`tools/models/rocks.py`: thinned as a copy with its own map, Meshy's paint baked onto it), then give it a place: `boulders::GLB` and, for scenery, a line in `boulders::scenery`.
- **The terrain's own shape.** The canyon is one height function (`maps/canyon/map.json`'s `canyon` block, `tools/maps/compile.mjs`). Above where anyone can stand (the cliff's knots past 42 m, `mesa`, `crags`) it is the skyline's to shape: change it, `node tools/maps/compile.mjs`, diff the compiled heights to see nothing reachable moved, then `cargo test -p sim --test determinism` (Dry Fork's golden moves with any ray that meets the new rock: update it in the same commit) and `--test maps`. Twenty seconds, both.
- **A structure.** Dry Fork's bases are curved solids the sim collides (`maps/canyon/map.json`, "red's base": `lathe` walls with a battered face, `deck`s, `arc`s, `column`s, a `ramp`), in a base's metal looks (`steel`, `steel_red`/`steel_blue`, `tread`: `tools/ground.sh steel tread` cuts their sets from ambientCG, `level::dress` tints them) and a `pylon` look (a turned box drawn as the Meshy pylon inside it, `boulders::Pylon`). A change there is a layout change: `cargo test -p sim --test maps` (spawns clear, everything walked to, the roof by its ramp) and the golden, both in seconds.
- **A structure's trim.** A structure with `"dress": true` is drawn and never collided (the bases' light rings, the slivers of light in their pylons): keep it a hand proud of what it dresses, and nothing thick at head height where someone stands.
- **Where things grow.** `grass::by_height` and `cover`, `clutter`'s `Grows` and `drift`. The ground's colour and its tufts read the same rule.
- **A shader.** No rebuild; shoot again. A number the shader holds is a `const` at its top with why.
- **The air and the light.** A map's mood (`mood.rs`), one for every player on it.

## Rules a prettier frame must keep

- **What is drawn is inside what collides, where anyone can go.** A boulder sits in its box (`boulders::INSET_*`); scenery stands out of bounds (`scenery_stands_out_of_bounds`). The sim's maps are not the look's to change: a new rock in play is a `map.json` change, a golden, and the operator's call.
- **Cover never hides a head.** Grass, flowers and clutter stay under a crouching fighter's hit box (the asserts in `grass.rs` and `clutter.rs`).
- **Every rung draws what can hide or show a fighter the same** (`quality.rs`' fairness): rock, grass, smoke. What a rung may drop only shows.
- **WebGL2 draws it too**: sixteen textures a draw, twelve uniform blocks, no compute. A new map on the ground or a new block in its shader is counted (`gpu::FRAGMENT_TEXTURES`, `ground.rs`'s header).
- **Assets are ours, CC0 or CC-BY**, credited in NOTICE; the raw download stays in `sources/` (git-ignored) and a tool makes the cut that ships.

## Traps

- **A deck's rolled rim can't be stepped onto.** `"rim"` rounds a `deck`'s edge, and a steep top is never floor: a floor 12 cm up with a rim was a ring nobody could cross, and a roof with one could not be reached from its ramp. Leave `rim` off any deck that is walked onto from its side.
- **A floor's height must be exact in f32** (0.125, not 0.15): `maps.rs` wants a spawn's `y` to equal its floor's to the bit.
- **Thinning a built thing buckles it.** A hard-surface mesh wants a light push to its cube, three times a boulder's triangles and its hard edges kept (`rocks.py`'s `BLOCKS`, `hard`).
- **`Surface::Steel` rusts in streaks**, which on a curved wall read as wood grain: a base's plate weathers as `Surface::Plate`.

- **A debug client's sky is not the game's.** `deck::DETAIL_N` and `sky::FIELD_N` are cut down in a debug build, so its clouds are smooth lumps. Judge the sky with the release client (`cd crates/client && cargo build --release`, then `ONESHOT_BIN=…`).
- **TAA softens the whole frame**; the sharpening after it is what a still is judged with. Compare rungs with `HD_QUALITY=low`.
- **Cloud shadows cross Dry Fork**, so two passes minutes apart differ in light; the pose mode's clock starts the same each run, so two shots a run apart do not.
- **This box's Blender has no denoiser**: a Cycles bake that asks for one comes back black (`rocks.py` turns it off). Run from a script, `bpy.ops.object.bake` reads its own arguments, not the scene's.
- **Meshy's mesh is cut along every seam of its map**: weld it before thinning it, and bake onto a copy with a map of its own, or the paint smears. What Meshy calls metal has no colour to bake; bake the paint as emitted light.
- **A view inside a hill measures nothing.** Put an eye 1.7 m over the ground (the scratch probe: a crate that prints `Terrain::height`).

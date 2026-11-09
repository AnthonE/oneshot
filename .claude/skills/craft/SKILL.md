---
name: craft
description: Make or improve how a one-seater looks in oneshot (the skimmer, a hover bike; the flier, an attack craft; any new vehicle model) by the craft loop: cut its mesh in Blender to the sim's hull, read its sheet, shoot the game's fixed views of it on the GPU with no window, lay each beside the last pass, name the largest gap, change one thing, go again. Use when asked to make, fix, restyle or review a vehicle's model, its paint, its lights, its rider's fit or its thrusters' flames.
---

# The craft loop

A craft is judged from fixed eyes, in the game, beside its last pass. One step:

1. **cut** the mesh (Blender, ten seconds);
2. **read the sheet**: is it the right way round, inside the sim's hull, its rider where its saddle is;
3. **shoot** the game's views of it (a few seconds each, no window on the desktop);
4. **read the board**: this pass over the last, and name the gap that owns the most of the frame;
5. **change one thing**, and go again.

```bash
tools/craft.sh                        # both: cut, shoot, board
tools/craft.sh flier                  # one
tools/craft.sh list                   # the views and their poses
CRAFT_SHOOT=0 tools/craft.sh skimmer  # the cut and its sheet only        → traces/craft/skimmer.png
CRAFT_CUT=0 tools/craft.sh            # shoot what is built (craft.rs or a shader changed)
CRAFT_VIEWS=chase,boost tools/craft.sh skimmer
CRAFT_STAGE='3 -90 -12' tools/craft.sh   # on Dry Fork, where they fly (map x z)
CRAFT_TAG=before tools/craft.sh       # keep a round by hand            → traces/craft/before/…
```

| what | where |
|---|---|
| the sheet (six views; the sim's hull in yellow, the rider and the muzzles in orange) | `traces/craft/<name>.png` |
| the game's views, and the pass before each | `traces/craft/<name>/<view>.png`, `<view>.last.png` |
| the board: every view, this pass over the last (the far view cut in close) | `traces/craft/<name>/board.png` |
| the numbers: size, belly, nozzles, how much skin is outside what stops a shot | `traces/craft/<name>.json` |

`craft-judge` (`.claude/agents/craft-judge.md`) reads a board without your hopes for it.

## What a craft is made of

- **The picture and the mesh** are Meshy's, by the prompt in the craft's own script (`tools/models/skimmer.py`, `flier.py`: `IMAGE`, `VIEW`, `TEXTURE`). The picture first (`tools/models/meshy_gen.py <name> --image-only`, 9 credits), looked at, then the mesh of it (`meshy_gen.py <name> --tris 30000`, 15 more). The raw stays in `sources/meshy` (git-ignored: never delete one, a craft cannot be rebuilt without it). Another roll is `--roll`.
- **The cut** is the script's, on `tools/models/craft.py`'s kit: `hull` turns the raw into the sim's hull frame (+X forward, the origin on the ground under the hull's middle), sizes it to the sim's own numbers (read from `consts.rs` by `sim('SKIM_HALF_L')`, never copied), cuts the paint down and lights what was painted cyan; `slide` moves a saddle to where the sim seats its rider; `deliver` exports the GLB and renders the sheet.
- **Paint by rule, not by roll.** `texels(o)` says where each texel of the map lies on the craft (its point and its face's normal, hull space), so a light or a colour is a line of numpy written in metres: `light(o, mask)` lights it, `paint(o, mask, colour)` repaints it, `band` makes a soft-edged strip, `thrusters(o, tx, rings(o, STERN))` finds the engines by the rings Meshy painted, lights their throats and turns their steel cans dark iron. The skimmer's spine seams and the flier's belly lights are these. Ten seconds, no credits, and the mesh and the rest of its paint stay as they were; a `--paint` roll repaints everything and may come back worse.
- **Move a part, add a part.** `pieces(o, lo, hi)` finds the loose pieces of the mesh inside a box (Meshy models bars, levers and barrels apart from the hull), `shift` moves them as one, and `rod` adds a round bar of the kit's own paint to carry them (`join` it in before `deliver`). The skimmer's bars were raised to its rider's hands this way.
- **The game's side** is `crates/client/src/look/craft.rs`: the GLB under the body that leans and bobs, the boost's flames at the nozzles (`SKIM_FLAMES`, `FLY_FLAMES`: the sheet's json prints the nozzles), the skimmer's grips (`SKIM_GRIPS`: its rider's hands are put on them by `bodies::pose_limbs`, leaning with the bike). A new GLB is read when the client starts, so a new cut needs no build; a change in `craft.rs` does (`cd crates/client && cargo build`).
- **The stage** is the pose mode (`crates/client/src/pose.rs`): `craft=x,y,z&craftkind=1|2`, `craftseat=1` puts the fighter at its controls, `craftboost=1`, `craftpitch=`, `craftroll=`, and `craftcam=x,y,z` with `craftaim=` put the camera in the craft's own frame, so a view never reads a height off a map. `tools/craft.sh`'s `SKIMMER` and `FLIER` rows are the views.

## Reading a pass

Top down; the first line that fails is the gap.

| read | the question | where it is changed |
|---|---|---|
| **silhouette** (`far`, `under`) | at 25 m, in two seconds, is it a bike or a flier, and which way is it going? | the picture's prompt (a new roll); `hull`'s `width`, `height` |
| **the hull's fit** (the sheet's top row) | is it inside the yellow boxes, and is most of each box filled? A shot that meets drawn metal should stop | `hull`'s `width`, `height`, `belly`; a new roll if the plan is wrong |
| **the rider** (`side`, `rear`) | does it sit on the saddle, hands on the grips, nothing through its legs? | `slide`, then `pieces`/`shift`/`rod` in the craft's script and `SKIM_GRIPS`; never the sim's seat for a look |
| **the paint** (`quarter`) | plates, seams and wear that read at three metres; not chrome under the sky, not flat plastic | `hull`'s `rough`, `metal`; the script's `TEXTURE` and a `--paint` roll |
| **the light** (`rear`, `chase`, `under`) | is something lit where each camera looks (the back from the saddle, the belly from the ground), and do nose and tail differ by it? | `light` and `thrusters` in the craft's script; `hull`'s `glow_strength`, `glow_floor`, `pale` |
| **the two halves** (the sheet's top view, `chase`) | is the side the picture never saw as good as the one it did? | `hull`'s `mirror` |
| **the boost** (`boost`) | a flame from each nozzle, in the nozzle | `craft.rs`'s nozzles and flames |
| **from the saddle** (`chase`) | the view a player has for as long as it flies: is the back of it worth looking at? | all of the above |

## Rules a prettier craft must keep

- **Our own designs.** No proper noun in a prompt, no other game's craft as the brief, and a silhouette that is not one of theirs. The Veil's line is the brief: deep violet plates turning blue at the edges, dark seams, dark iron in the gaps, cyan light.
- **What is drawn is what collides and what stops a shot.** The sim's hull is the sim's (`Vehicle::hull`, `mount`, `gun_at`): the model is cut to it, never the other way round for a look. A rule that should move (a seat, a muzzle) is the operator's call, a golden and a `PROTO`-free change of its own.
- **Looks only.** Nothing in `craft.rs` decides anything; the lean, the bob and the flames read what the sim sent.
- **Every rung draws it the same shape.** A lower rung may drop bloom, never the hull.
- **Assets are ours, CC0 or CC-BY**; the raw stays in `sources/`.

## Traps

- **The box has no swap and the client's load peaks near 5 GB.** `tools/craft.sh` waits for free memory before each shot and runs the client as the kernel's first pick; if another session is compiling, wait for it. Dry Fork wants 6 GB free; the Range (the default stage) less.
- **A picture's unseen side is made up.** One three-quarter view gives a good near side and a guessed far one: look at the sheet's top view before anything else, and `mirror` the seen half if they differ.
- **Meshy paints its lights nearly white.** `pale=True` counts them; a hull's own blue sheen is 20 to 30 degrees from cyan, so keep `glow_floor` up or the plates light too.
- **The mesh comes with its nose to −X.** `TURN = 180` in both scripts; check a new roll's first sheet.
- **A stretch has a limit.** `width` and `height` past about a tenth make it another craft; `slide` draws out whatever lies between its marks, so keep them on a plain run of hull.
- **The rider's arms are 40 cm.** Triple T sits bolt upright in the sim's seat and cannot reach a sport bike's bars: bring the bars to the hands (the clip's own, about chest high and a hand ahead of the trunk), not the trunk to the bars. A lean that puts the head outside the sim's seat box is a head that cannot be shot.
- **The board's last pass is one pass back.** A second run overwrites it: `CRAFT_TAG=<name>` keeps a round worth comparing against.
- **`build.sh` hides Blender's noise**, and an exception with it: a script that printed no `MODEL` line failed; run it bare (`blender -b --python tools/models/<name>.py`).

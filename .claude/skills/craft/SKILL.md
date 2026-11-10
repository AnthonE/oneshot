---
name: craft
description: Make or improve how a one-seater looks in oneshot (the skimmer, a hover bike; the flier, an attack craft; any new vehicle model) by the craft loop: grow its mesh in Blender by rule to the sim's hull, read its sheet, shoot the game's fixed views of it on the GPU with no window, lay each beside the last pass, name the largest gap, change one thing, go again. Use when asked to make, fix, restyle or review a vehicle's model, its paint, its lights, its rider's fit, its engines' flames or its trails.
---

# The craft loop

A craft is judged from fixed eyes, in the game, beside its last pass. One step:

1. **cut** the mesh (Blender, twenty seconds);
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
CRAFT_STAGE='3 -90 -12' tools/craft.sh   # on Dry Fork, where they fly (map x z); the `wake` view wants it
CRAFT_TAG=before tools/craft.sh       # keep a round by hand            → traces/craft/before/…
VEIL_COUNT=1 tools/models/build.sh flier   # its triangles, part by part and seam by seam
```

| what | where |
|---|---|
| the sheet (six views; the sim's hull in yellow, the rider and the muzzles in orange) | `traces/craft/<name>.png` |
| the game's views, and the pass before each | `traces/craft/<name>/<view>.png`, `<view>.last.png` |
| the board: every view, this pass over the last (the far view cut in close) | `traces/craft/<name>/board.png` |
| the numbers: size, belly, nozzles, triangles, how much skin is outside what stops a shot | `traces/craft/<name>.json` |

`craft-judge` (`.claude/agents/craft-judge.md`) reads a board without your hopes for it. Freeze a pass for it first (copy the sheet, the json and the views to a folder of their own): the default paths are overwritten by the next cut.

## What a craft is made of

- **Its form is grown by rule**, in its own script (`tools/models/skimmer.py`, `flier.py`) on `tools/models/veil.py`'s kit. No mesh is commissioned and no picture rolled: a craft is numbers at the top of its script. `curve` is a smooth line through points (a hull's top line, a wing's leading edge); `oval` and `foil` are sections; `loft` skins a run of them and subdivides it smooth, with creases where asked (a chine, a bill's ridge); `stem` is a limb or a blade along a path (a horn, a feather, a spindle of light). A new shape is a new line of numbers and twenty seconds.
- **Its plates are what its seams leave.** `seam` cuts a groove where a rule is zero (a plane, `beside` a wing's edge): dark iron in it, or light out of it. `within` (a rule, metres inside the part the seam runs over) ends it square; `where` just stops it. `plate` lays that part of the skin again, proud of it, so plates lap like a wing case. `lens` is a domed light in an iron rim (a lift's grille, a port), `mouth` an engine's: a throat (lit, or with a `wall` of dark iron down to a lit floor, which is what reads as depth from astern), a core, vanes.
- **It carries no map.** `shade` traces the shade each vertex stands in against the whole craft and writes it, with the paint's colour, each plate's own tone, the pale blue of a worn edge and any soot (`stain`), into the mesh's colours (COLOR_0; their alpha is the shade alone). `PAINTS` in `veil.py` names the paints, and a paint's name is what the game knows it by. `gloss` gives every cut face the smooth skin's own normals back, so a seam does not show as facets in a reflection.
- **The cut** is still `craft.py`'s: the sim's numbers read from its own code (`sim('SKIM_HALF_L')`, never copied), `deliver` exporting the GLB and rendering the sheet with the sim's hull over it. (`craft.py` also keeps the other way to make a craft: `hull`, `slide`, `texels`, `light`, `paint` for a mesh commissioned from Meshy, as the guns are. The one-seaters were made so first; their raw meshes are still in `sources/meshy`.)
- **The game's side** is `crates/client/src/look/craft.rs`. Its paints by name wear a wing case's shader (`shaders/veil_shell.wgsl`, `shell_of`: the violet turns blue edge on, the sky's photograph the guns reflect lies in its coat, dimmed by the traced shade, and a faint grain of duller patches and hairline fore-and-aft scratches is laid by where each pixel lies on the craft, which `shade` writes into the mesh's two UV sets; it is gone by eight metres, and its roughness barely moves: on a coat this glossy more of it cuts a highlight into islands); `throat`, `glow` and `vein` are each craft's own and burn with its going (`BURN`), so anything painted so flares with the boost; a `vein` sleeps until the boost (`VEIN_ASLEEP`). The engines' flames (`SKIM_FLAMES`, `FLY_FLAMES`: the json prints the mouths) are short at speed and long boosting (the skimmer's a round dart of its own, slim at speed and rooted inside its throat: `DART_*`; the flier's a flat blade); the trails (`SKIM_TRAILS`, `FLY_TRAILS`) leave from its tail's tips; the light it throws on the ground (`SKIM_LIGHT`, `FLY_LIGHT`); the skimmer's grips (`SKIM_GRIPS`: its rider's wrists are put there by `bodies::pose_limbs`, and his fists close a hand's length up and on from them, which is where the model's grips are). A new GLB is read when the client starts, so a new cut needs no build; a change in `craft.rs` does (`cd crates/client && cargo build`); a change in the shader does not.
- **The stage** is the pose mode (`crates/client/src/pose.rs`): `craft=x,y,z&craftkind=1|2`, `craftseat=1` puts the fighter at its controls, `craftboost=1`, `craftpitch=`, `craftroll=`, `craftcam=x,y,z` with `craftaim=` put the camera in the craft's own frame, so a view never reads a height off a map, and `craftfly=` with `craftturn=` put it under way (its lean, its flames at speed, its trails). `tools/craft.sh`'s `SKIMMER` and `FLIER` rows are the views.

## Reading a pass

Top down; the first line that fails is the gap.

| read | the question | where it is changed |
|---|---|---|
| **silhouette** (`far`, `under`) | at 25 m, in two seconds, is it a bike or a flier, and which way is it going? | the curves at the top of its script: a top line, a width, where a wing's edge runs |
| **the hull's fit** (the sheet's top row) | is it inside the yellow boxes, and is most of each box filled? A shot that meets drawn metal should stop | the same curves; the json's `outside_steel` |
| **the rider** (`side`, `rear`) | does it sit on the saddle, fists on the grips, feet on something, nothing through its legs? | the script's `SEAT_TOP`, `GRIP`, the horns; `SKIM_GRIPS`; never the sim's seat for a look |
| **the plates** (`quarter`) | plates that lap, iron in the gaps, three values of paint that hold at 25 m; not one smooth toy | `seam`'s width and depth, `plate`'s lift, which paint a plate wears, `shade`'s `tone` and `stain` |
| **the coat** (`quarter`, `chase`) | a wing case, not chrome and not plastic: deep violet face on, blue and the sky at the edge | `craft.rs`'s `shell_of` and `EDGE`; `veil_shell.wgsl` |
| **the light** (`rear`, `chase`, `under`) | is something lit where each camera looks (the back from the saddle, the belly from the ground, a flank from abeam), and do nose and tail differ by it? | `seam(floor='light')`, `lens`, `stem(material='light')`, a `repaint` of an end |
| **the boost** (`boost`) | a flame from each mouth, in the mouth, a core in it and no edge to it, the rim round it still showing | `craft.rs`'s flames and `Kit` |
| **under way** (`wake`) | does it lean into its turn, its engines brighter, a flame at speed, a wake that is its own? | `craft.rs`: `LEAN_*`, `BURN`, `FLAME_*`, the trails |
| **from the saddle** (`chase`) | the view a player has for as long as it flies: is the back of it worth looking at? | all of the above |

## Rules a prettier craft must keep

- **Our own designs.** No other game's craft as the brief, and a silhouette that is not one of theirs: the flier is a swift (a bill, feathers, a forked tail), the skimmer a sled with mandibles. The Veil's line is the brief: deep violet plates turning blue at the edges, dark seams, dark iron in the gaps, cyan light. Never hood the flier's glass in shell, droop its wings to pods or trail light from its wing tips; never give the skimmer a hood before its rider or drooping prows.
- **What is drawn is what collides and what stops a shot.** The sim's hull is the sim's (`Vehicle::hull`, `mount`, `gun_at`): the model is grown to it, never the other way round for a look. A rule that should move (a seat, a muzzle) is the operator's call, a golden and a `PROTO`-free change of its own.
- **Looks only.** Nothing in `craft.rs` decides anything; the lean, the bob, the flames and the trails read what the sim sent and where the hull was drawn.
- **Every rung draws it the same shape.** A lower rung may drop bloom, never the hull.
- **Glass stays dark.** Clear glass promises a head shot the hull's box does not give.

## Traps

- **Blender is run under a memory cap** (`tools/models/build.sh`, `MODEL_MAX`), and must be: the box has no swap, and a mesh cut too fine (a hundred thousand vertices) took 13 GB in the glTF exporter and was killed by the kernel three times in a row. If a build prints no `FIT` line, it was killed or it threw: run it bare under the same cap and read the end of its output.
- **The box has no swap and the client's load peaks near 5 GB.** `tools/craft.sh` waits for free memory before each shot and runs the client as the kernel's first pick; if another session is compiling, wait for it. Dry Fork wants 6 GB free; the Range (the default stage) less.
- **Cuts cost triangles, and a bevel on a plate costs most**: every seam under it is bevelled too. `plate`'s `bevel` is off unless asked; count with `VEIL_COUNT=1` and keep a craft near 60,000.
- **A seam that curves in plan is a polygon** at the skin's own grain: build a round light as a `lens` and a feather as a `stem`, not as cuts. A cut that would pass within `SNAP` of a vertex goes through it, so no sliver is left to tear.
- **A paint chosen by a rule over face middles has a staircase for an edge** unless the rule follows the skin's own lines (a share of a chord, a ring of a loft).
- **A loft's subdivided skin lies a little inside its sections.** Read the fit off the json, not off the numbers typed.
- **The rider's arms are 40 cm.** Triple T sits bolt upright in the sim's seat: bring the grips to his fists and the saddle to the foot of his trunk (measured in the game's side view), not him to them. A lean that puts the head outside the sim's seat box is a head that cannot be shot.
- **The board's last pass is one pass back.** A second run overwrites it: `CRAFT_TAG=<name>` keeps a round worth comparing against.

# Cloudbreak: a map design

Draft, 2026-11-05. A working name. Map id 7 when it is built (`maps/cloudbreak/`).

An arena for Doubles and casual FFA: a sandstone island standing out of a
sea of cloud, a grey stone spire at its north edge, and a round plaza in the
middle under two leaning struts that never meet. Falling off kills. It is
our first outdoor arena, our first arena on terrain, and our first map where
the edge is a weapon.

## 1 · What we take from the references

**Take:**
- A small island high over nothing: the edge always in view, the void a third
  of the frame, distance that reads as height.
- One landmark tall enough to orient by from anywhere on the map.
- Warm, rounded sandstone against cool, hard-edged grey architecture.
- A round floor with a strong silhouette over it (the leaning struts).
- An overcast sky with a break in it, a flat sea of mist below, stacks of rock
  far off getting paler.
- The island's underside seen from its edge: undercut rock and arches.

## 2 · Numbers

| | |
|---|---|
| modes | Doubles (2v2 ranked), Duel, casual FFA up to 6 |
| bounds | `[-34, 34, -26, 26]`: 68 × 52 m, 3,536 m² (under `BIG_M2`, so an arena) |
| the island | about 58 × 38 m at its widest; ~1,500 m² of ground plus decks |
| mirror | `"x"`: red at −x, blue at +x; the spire and the ring on x = 0 |
| levels | the void −12 (death) · the shelf −2 · the plaza 0 · the shoulders +4 · the gate roofs +4.5 · the crown +9 · strut tips +12 · the spire's top +32 (dressing) |
| run | 6.86 m/s: 20 m is 3 s |

The island is as big as Bulkhead (1,536 m²) and a little bigger than
Chrysalis (1,206 m²), but more of it is open and you can see across it, so it
plays bigger. Duel may find it too big; that's the flow tool's call (§6).

## 3 · Layout

Top down, north up. `#` the spire, `O` the ring, `/ \` the struts, `G` the
gates (bases), `^` sandstone fingers, `=` ramps, `~` the void.

```
 z
+26 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ ######## ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
+16 ~~~~~~~~~~~ ^^ shoulder ======## spire##====== shoulder ^^ ~~~~~~~~~~~
    ~~~~~~~ ^  (+4)          crown ##(+9)##          (+4)  ^ ~~~~~~~~~~~~
+10 ~~~~  ____                   [ the hall ]                  ____  ~~~~~
    ~~ |  G   |   ^^                 |                   ^^   |   G  | ~~
  0 ~~ | red  |      \_____      .---O---.      _____/      | blue | ~~
    ~~ | gate |  ^^       \_____ |  ring | _____/       ^^  | gate | ~~
-10 ~~ |______|    fingers       '---O---'      fingers     |______| ~~
    ~~~~    ^                                                  ^   ~~~~
-16 ~~~~~~~~  ======= the shelf (−2), the rocket ========  ~~~~~~~~~~~~
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
-26 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
   -34        -24        -12         0         12         24        34  x
```

The places, which are also the `callouts`:

- **The ring** (centre (0, −3), r 9 m, its top 0.4 m over the sand). A round
  stone deck, cut with rings. Four waist-high plinths on its diagonals are its
  only cover. The overshield sits in the middle, under the gap between the
  struts. Everything on the map can see some of the ring; nobody can hold all
  of it from cover.
- **The struts.** One from each side, mirrored: its foot a block 2.4 × 2.4 ×
  3 m on the sand at x = ±12, its beam leaning in over the ring at 50° to its
  tip at x = ±1.6, 12 m up. The tips don't meet; a 3 m gap stays open over
  the overshield. At the ring's edge the beam's underside is 3 to 4 m up, so
  you walk under it. Steeper than `MAX_CLIMB` (46°), so never climbed. A strut is cover at its foot, a
  silhouette from everywhere, and something to bank a grenade off.
- **The spire** (x = 0, z 11 to 19), its back flush with the north cliff.
  - *The hall* (ground floor, y 0 to 4): a door to the west and the east, one
    south to the ring. The shotgun. A lift in its middle.
  - *The lift*: a launcher from the hall's floor to the crown (1.5 s).
    One way, loud, seen from the ring through the south door.
  - *The crown* (y 9): a balcony on the spire's south face, 10 × 2.5 m, a
    parapet at the waist. The sniper. It sees the ring and both shoulders; it
    can't see the shelf, into the gates, or behind the fingers.
  - Above 12 m the spire is scenery: buttresses, a window band, a light at the
    top. Collides (rockets stop on it), never reached.
- **The crown ramps.** From each end of the crown down along x to that side's
  shoulder: 12 m long, 9 → 4 m, no rail. The way up for whoever holds a
  shoulder; exposed to the ring the whole way.
- **The shoulders** (x = ±16, z 8 to 16, up to y = 4). Rock rising toward the
  north-west and north-east corners, fingers on top for cover, a drop to the
  gate's roof.
- **The gates** (x = ±24, z −6 to 6). Each side's base, set into the island's
  tip: a room 6 × 9 m under a roof deck at 4.5 m, framed by two big leaning
  slabs (our own, not the struts' shape). Doors to the fingers and to the
  shoulder; a ramp outside up to the roof. Its back opens onto a balcony over
  the void. The carbine. Most of the side's spawns.
- **The fingers** (x = ±11, z −6 to −12). A cluster of sandstone spires 2 to
  5 m high between the gate and the ring: the ground route's cover.
- **The shelf** (z ≈ −16, x −14 to 14, y = −2). A ledge along the south cliff,
  3 m wide, 2 m under the plaza, no rail. The rocket at its middle. Ramps down
  at each end, drops from the plaza's rim anywhere. The crown can't see it
  (the plaza's southern rim and its rocks are in the way), so it's the flank,
  and the place a blast puts you in the void.

## 4 · Pads

Ranked starts as ever: the BR and the magnum, two grenades.

| pad | where | respawn | why |
|---|---|---|---|
| sniper | the crown | 120 s | the high ground, reached by the lift (seen) or a ramp (exposed) |
| rocket | the shelf's middle | 120 s | the risky route; a rocket on an island is a fall waiting |
| shotgun | the spire's hall | 60 s | holds the lift and the hall |
| overshield | the ring's middle | 120 s | in the open, under the crown's eye |
| carbine ×2 | each gate | 30 s | the side's own rifle |
| ion pistol ×2 | each strut's foot | 45 s | strips a shield to finish across the ring |

No camo: an open island with a sniper over it is the wrong place for it.

## 5 · Spawns

Six a side, mirrored: two in the gate, one on its roof, one behind the
fingers, one on the shoulder, one at the shelf's end. None on the shelf
itself or in sight of the crown. `oneshot-flow` must show no spawn seeing an
enemy spawn.

## 6 · Flow targets

Checked with `cargo run -p flow --release -- cloudbreak` before the look is
started, and again after any change to the layout.

| from a gate to | target |
|---|---|
| the ring's edge | 2 to 2.5 s |
| the overshield | 3.5 s |
| the crown, by the ramp | 4 to 4.5 s |
| the crown, by the lift | 4.5 to 5 s |
| the rocket, by the shelf | 4 s |
| the other gate | 8 to 9 s |

- The longest clean line is under 60 m. Gate roof to gate roof along x is
  broken by the struts' feet and the ring's plinths.
- The crown sees under 45% of the ground (the ring, the shoulders, the
  ramps), not the shelf, the gates' rooms or behind the fingers.
- The mirror makes red and blue equal; what flow checks is that no power pad
  is more than a second nearer one route than its other.

## 7 · The edge

New rules, the map's reason to exist (and gap 1, §10):

- **The void.** Below y = −12 (`"void": -12` in `map.json`) a fighter is dead
  by `Cause::Fall`; a vehicle is gone; a dropped gun goes back to its pad.
- **Credit.** The last enemy who hurt you in the 4 s before you fell gets the
  kill, with the fall as its cause (Halo's rule). With nobody, it's a
  suicide, and a suicide costs a point in ranked. A teammate's blast in
  Doubles (friendly fire on) is a betrayal.
- **Where you can fall.** A low rock lip (0.6 m, a step) runs along the coast
  wherever a route runs beside it, so walking never kills by accident. The
  edge is open only where it's a choice: the shelf, the gates' back
  balconies, the crown ramps' outer side, the north cliff behind the spire.
- **The push.** A blast already pushes a body (`BLAST_PUSH`, 9 m/s at full).
  The rocket and the frag will knock people off the shelf and the ramps; that
  is the point. Tune `BLAST_PUSH` only if the range says it's silly.
- **The cliffs** are steeper than `WALK_NY` (46°) everywhere, so a body that
  goes over the lip slides off rather than climbing back.

## 8 · Look

Driven by the `hd` loop: fixed views beside the references, one change at a
time (`tools/hd.sh`, the board's `hd-*` views). New views: `hd-cb-ring` (from
the red gate's roof, the struts against the sky), `hd-cb-edge` (from the
shelf, looking along the island's underside into the mist), `hd-cb-crown`
(from the crown, down over the ring).

- **Sky.** Overcast: a low grey cloud deck with darker bases, a lighter band
  at the horizon, and one break that lets a low, warm sun onto the island.
  Sun and shade far apart, the shade cool.
- **Below.** No sea. A flat sea of mist at about −30, its top soft and
  slowly moving, the island's underside fading into it. Mist thicker the
  lower you look.
- **Distance.** Sea stacks and two or three other islands out of bounds,
  standing out of the mist, each step further a step paler.
- **Rock.** Warm sandstone, rounded and wind-cut: fingers with soft tops,
  cliffs undercut with an arch or two under the island (scenery under the
  terrain, never in play). Beds and cracks, not one flat value.
- **Ground.** Sand in the low parts and drifted against rock, bare sandstone
  where it slopes, a dry tuft here and there. No grass carpet.
- **The architecture.** Pale grey stone in big panels with recessed seams,
  chamfered edges, thin cool-white light in the seams at doors and the lift.
  Grey, not Chrysalis's purple. The ring's deck cut with concentric rings.
  The struts and the gates' slabs the same stone.
- **Life.** Sand blowing over the edge, wisps of mist rising past the coast,
  the light at the spire's top.
- **Sound.** Wind, louder at the edges.

## 9 · Building it

What `map.json` can already say, and how:

- **The island**: `terrain` with `base` at −40 and `shapes`: a `plateau` for
  the top, `hill`s for the shoulders and the fingers' rise, a `ridge` along
  the coast for the lip, and the cliffs falling to the base inside the
  margin. No `gen`: the canyon generator is for gulches.
- **The struts**: a `sweep` each (`tools/maps/curves.mjs`), its axis level
  across the beam (1.2 m along z), its profile the beam's side view in x
  (bottom and top rising together from the foot to the tip). A profile's
  every upright line meets it once, which a leaning beam does. Steeper than
  `MAX_CLIMB`, so never climbed. The foot a plain box.
- **The ring**: a `deck`. The plinths: boxes.
- **The spire**: boxes and `wall`s; the crown a `walk` or a box with a
  parapet; above 12 m boxes for collision and `dress` for the trim.
- **The crown ramps**: `ramp`s along ±x.
- **The lift**: a `launcher` from the hall to the crown, as Chrysalis's.
- **The fingers**: `round` solids with the `rock` look by hand, where
  they're cover that matters; the `rocks` planner for the rest. Either way
  they draw as boxes until there's a finger model (gap 11).
- **The shelf**: a `plateau` step in the terrain, or a `walk` against the cliff.
- **The callouts**: §3's places, for `oneshot-flow`.

## 10 · What we can't do yet

Ranked by what blocks the map. The first four are the sim; nothing about the
look matters until they're done. Most of these aren't Cloudbreak's alone: an
outdoor arena of any kind meets 5 to 10 and 13, and any map with an edge
meets 1 to 3.

### Blocks the map

1. **Nothing kills a fall.** `Cause::Fall` exists, and the client already
   names it (`hud.rs`, `tally.rs`, `look/rag.rs`), but the sim never kills
   with it. `Map::ground` is 0 wherever there's no terrain
   (`crates/sim/src/map.rs`), so there's a floor under the whole world, and
   `movement::step` clamps a body to `bounds`. Needed: a `void` height on the
   map (`sim::map::Def`, the compiler), and a fighter, a vehicle and a drop
   below it dealt with in `World::step` (not in `step_local`, which only
   keeps falling). `DROP_LOST_Y` (−50, a const) should follow the map's void.
   A map without `void` behaves as now, so the golden doesn't move.
2. **No kill credit for a knock-off.** A fighter doesn't remember who last
   hurt it, so a fall is always a suicide. Needed: the last enemy and a tick
   on `Fighter`, read by the fall; the leak gate needs a line if the kill
   feed names them differently.
3. **Bots walk off the edge.** The nav flood (`sim/src/nav.rs`) and the maps
   test's reach flood (`tests/maps.rs`) both "drop off ledges", so they'll
   route through the void's floor; bots without `nav` walk straight at a
   roam point. Needed: a floor under the void isn't floor to the flood, and a
   bot never takes a step whose floor is below the void.
4. **Big Team or arena is decided by bounds area** (`Def::big`, `BIG_M2`).
   An island's bounds include the void, so they overstate it; Cloudbreak fits
   at 3,536 m², a bigger island won't. Better: an explicit key, or the
   ground's area.

### Blocks the look

In the order they'd own the frame.

5. **The sky is chosen by map id, in the client.** `look/mood.rs` `of()`
   matches ids 1 to 6; a new id gets `NOON`. The sun's direction and colour,
   cloud, mist and dust all come from there, not from `map.json` (`RIP.md`
   says a map's sky block should; there isn't one). The deck is cumulus at
   1,400 m with `CLOUD_COVER` 0.57, and overcast is avoided on purpose
   (`look/sky.rs`); a storm deck exists and is never used. Needed: a mood for
   Cloudbreak (overcast, a break, a low warm sun), better read from
   `map.json`.
6. **Nothing below the horizon but water.** `light.sea` draws a water plane,
   on terrain maps only (`level.rs`); a flat map always gets a slab at 0. No
   cloud sea, no void, and fog thins with distance, never with height
   (`rig.rs` `fog_for`). Needed: a mist layer (a plane out to the horizon as
   the water has, a cloud shader on it) and fog thicker the lower you look.
7. **An island has no underside.** The terrain is a heightfield with no
   skirt, and the cliff skin stays within 4 cm of it (`look/cliffs.rs`): no
   undercut, no arch. The terrain under the void (at −40) would be drawn as
   a floor. Needed: the terrain below the void not drawn, and an underside
   hung under the coast as scenery (a commissioned rock mesh, cut by
   `tools/models/rocks.py`).
8. **No grey stone architecture.** The nearest are `hull` and `plate`
   (bolted grey plating, industrial, no glow) and `alien` (purple pearl with
   magenta seams). Needed: a look family of our own (big stone panels,
   recessed seams, chamfers, cool-white light in the seams) in
   `look/textures.rs` beside `panel` and `alien`, and its names in
   `compile.mjs`'s `LOOKS`.
9. **Terrain brings Dry Fork's dressing with it.** Any terrain map gets
   grass by height (`look/grass.rs`), clutter, a ring of rim trees
   (`level.rs`, `trees::rim`) and soil tints from one global table
   (`ground::TINT`). Needed: a map says what grows (here: sand, bare rock, a
   few dry tufts, no trees).
10. **Scenery is a list per map id, in the client** (`boulders::scenery`:
    only Dry Fork), made of three boulders and a sea stack. No general GLB
    path for level art either: every structure is sim boxes and profiles
    (good for collision, limiting for a spire's silhouette). Needed: scenery
    from `map.json`, a floating-island prop, and a way to dress a structure
    with a mesh while the sim keeps the boxes (as boulders already do inside
    their boxes).
11. **No finger rock.** A planned boulder is one of three models stretched
    in its box up to 2.6 times, then a faceted box (`look/boulders.rs`). A
    sandstone finger 1.5 m across and 5 m tall is past that. Needed: a
    finger or spire model.
12. **The `hd` views are all Dry Fork's** (`tools/hd.sh`, `board/board.json`).
    Needed: §8's three views and their references.

### Tooling and process

13. **No outdoor arena exists, and no arena on terrain.** All five arenas are
   walled boxes on a flat floor at 0 (Proving Grounds, Cistern, Overpass,
   Bulkhead, Chrysalis); only Dry Fork has terrain, and it's Big Team. So the
   terrain's arena-scale use (shapes only, no `gen`, a 0.5 or 1 m cell) has
   never been tried; expect the compiler's checks and the cliffs' look to
   need work.
14. **Terrain shapes are blunt for a coastline.** `hill`, `ridge`, `plateau`
   and `pad` all fall off round or straight; an island's coast wants an
   outline (a polygon or a spline) with its own edge profile. Without one the
   coast is a stack of hills and ridges.
15. **Every arena is mirror-symmetric** (`"x"` or `"xz"`); the compiler has
   no 180° turn about y. Fine for ranked; it rules out the asymmetric
   one-tower maps the references come from unless spawns are made fair by
   hand.
16. **The flow tool doesn't know about falls.** It should mark ground within
   a few metres of an open edge and the places a blast from a pad's
   neighbourhood can put you over it, so "how risky is this route" is a
   number, not a feeling.
17. **No per-map sound.** The wind plays on a big map only (`sound/mod.rs`);
   an arena on a cliff wants it too.

Not a gap: the wire and fog. A fall is a kill like any other, a body below
the island is out of sight like any other, and `Cause::Fall` (103) is already
on the wire, so no `PROTO_VER` bump.

## 11 · Order of work

1. The void, fall credit and bots at the edge (gaps 1 to 3), with a test that
   a fighter pushed off a ledge dies, credited, and a bot match on a void
   map loses nobody to the edge on its own.
2. Cloudbreak's `map.json` in grey boxes: terrain, the spire, the ring, the
   struts, the gates, pads, spawns, callouts. `node tools/maps/compile.mjs`,
   `cargo test -p sim --test maps`, `oneshot-flow` against §6. Iterate the
   layout here; play it with bots and people before any art.
3. The look gaps the map needs, in the order the `hd` loop names them.
4. Into the Doubles rotation once it's been played.

## 12 · For the operator

- A fall with nobody to credit: a suicide (−1 in ranked), as Halo has it?
- The crown: the sniper, or the beam rifle (Chrysalis has the beam rifle)?
- Duel on it, or Doubles and FFA only?
- The name.

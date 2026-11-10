# oneshot today: netcode, input and combat feedback (codebase audit, 2026-11-09, HEAD 1778766)

Sources are files in `/home/user/oneshot`, cited as `path:line`. Nothing was built or run. 1 tick = 1/128 s = **7.8125 ms** throughout. "(inferred)" marks arithmetic or reasoning of mine on top of what the code says; everything else was read.

Tick conversions used below: 2 ticks = 15.6 ms, 4 = 31.25 ms, 12 = 93.75 ms, 16 = 125 ms, 20 = 156.25 ms, 32 = 250 ms, 64 = 500 ms. `ticks(ms)` rounds to nearest (`crates/sim/src/consts.rs:12-14`), so `ticks(160)` = 20 (156.25 ms), `ticks(150)` = 19 (148.4 ms), `ticks(125)` = 16.

## 1. Rates and delays: tick, inputs, snapshots, input buffer, clock, interpolation, prediction

### Takeaway
128 Hz sim, 64 Hz snapshots, inputs made one per tick and sent at min(frame rate, 64) Hz with up to 16 frames of redundancy; the client clock is steered so about 1 to 2.5 input frames (8 to 20 ms) wait at the server. Other players are drawn at least 4 ticks (31.25 ms) behind the newest snapshot, adapting up to 12 ticks (93.75 ms) with jitter, with up to 125 ms of velocity extrapolation on a gap; the local fighter is fully predicted, and a misprediction is eased out over 100 to 325 ms (snap past 4 m).

### Cited Findings
**Tick**
- Sim tick rate 128 Hz, `DT = 1/128` s, on the server and in the client's prediction — `crates/sim/src/consts.rs:5-8`.
- Server loop: one iteration per `1e9/128` ns, `thread::sleep` to the next deadline; more than 8 ticks behind, the missed ticks are skipped and counted, never sprinted — `crates/server/src/lib.rs:1115`, `crates/server/src/lib.rs:1708-1719`.
- Input datagrams are drained once per loop iteration, before the rooms step — `crates/server/src/lib.rs:1519-1528`.

**Inputs (client to server)**
- One input frame per client tick: buttons (11 bits), yaw u16, pitch i16, move x/z i8 each — `crates/sim/src/input.rs:21-38`, `crates/proto/src/lib.rs:211-217`.
- A frame's input is "the controls as they are now" when the render frame runs; every tick owed in that frame gets the same yaw/pitch/stick, and the first tick made also takes every button seen down since the last tick (sticky OR), so a sub-frame click still fires once — `crates/client-net/src/core.rs:822-840`, `crates/client-net/src/core.rs:887-893`.
- A datagram is sent when 2 new frames exist (`SEND_EVERY = 2`) at the end of the render frame's update, or inside the loop when 16 pile up; each datagram carries every unacked frame, at most 16 (`MAX_INPUT_FRAMES`) — `crates/client-net/src/core.rs:288`, `crates/client-net/src/core.rs:934-942`, `crates/client-net/src/core.rs:1298-1316`, `crates/proto/src/lib.rs:35`.
- The code's own note: at 64 fps or slower every frame leaves with the render frame that made it; faster, a frame may wait one render frame, "a tick at most" — `crates/client-net/src/core.rs:848-858`.
- Each datagram also carries `ack_tick` (newest snapshot the client has) and `playout` (how far behind that it draws others, measured, in ticks) — `crates/proto/src/lib.rs:171-181`, `crates/client-net/src/core.rs:1302-1304`, `crates/client-net/src/core.rs:2329-2334`.
- Server-side rate limit per session: 128 input datagrams/s, burst 64 — `crates/server/src/net.rs:48-53`.
- Stall handling: owed more than 64 ticks (500 ms), the client skips to the newest instead of making them all — `crates/client-net/src/core.rs:290`, `crates/client-net/src/core.rs:868-879`.

**Server input buffer**
- One frame consumed per tick per player; at most 16 frames (125 ms) may wait, older ones are dropped; ring of 32 by seq — `crates/room/src/lib.rs:33-37`, `crates/room/src/lib.rs:316-326`.
- A missing frame runs the last one held as it was for 250 ms (`LOST_HOLD_MS`), then fire/zoom/jump are released and the stick eases to zero over 150 ms (`LOST_STOP_MS`) — `crates/room/src/lib.rs:38-47`, `crates/room/src/lib.rs:78-93`.
- The server tracks the low point of "frames waiting behind the one just run" since the last snapshot (−1 if a tick had none) and sends it in every snapshot as `depth` — `crates/room/src/lib.rs:139-143`, `crates/room/src/lib.rs:349`, `crates/room/src/lib.rs:363-368`, `crates/room/src/lib.rs:1109-1113`.

**Client clock steering (`ClientNet::take_low` → `clock_rate`)**
- Target low-point depth 1.0 frame (`TARGET_DEPTH`); depth < 1.0 → clock runs 5% fast; depth in 1.0 to 2.5 → rate 1.0; depth > 2.5 → 1% to 5% slow (gain 0.015 per frame over) — `crates/client-net/src/core.rs:279-281`, `crates/client-net/src/core.rs:2748-2763`.
- Depth outside −8..=16 → hard resync (clock jumps by `1.0 − depth`); what was on screen eases on as a correction rather than stepping — `crates/client-net/src/core.rs:1449-1478`.
- The first predicted seq is the welcome's tick + 8 — `crates/client-net/src/core.rs:744`.

**Snapshots (server to client)**
- Every 2 ticks (`SNAP_EVERY = 2`, 64 Hz), on ticks that are multiples of it, one per client, delta-encoded against the last acked when smaller — `crates/room/src/lib.rs:59-60`, `crates/room/src/lib.rs:1023`, `crates/room/src/lib.rs:1130-1141`.
- Ranked and arena rooms are always 64 Hz. A big map's casual room may be set to 64/32/16 Hz with `--big-snapshot-hz`; the default is 64 — `crates/server/src/lib.rs:275-277`, `crates/server/src/lib.rs:357`, `crates/server/src/lib.rs:1062-1069`, `crates/server/src/lib.rs:2346-2352`.
- On a big (non-ranked) map with far rates on (default), another fighter farther than 60 m is updated every other snapshot (32 Hz) and farther than 120 m every fourth (16 Hz), unless riding or firing within 15° of you; only position and facing are ever held back — `crates/room/src/lib.rs:1289-1293`, `crates/room/src/lib.rs:1330-1347`, `crates/room/src/lib.rs:1358-1386`, `crates/server/src/lib.rs:359`.
- Remote bodies go on the wire at 1/64 m position and 1/64 m/s velocity (16 bits a component), yaw/pitch 16 bits each, shield as 0..255; the client's own fighter is sent exactly — `crates/proto/src/lib.rs:62-63`, `crates/proto/src/lib.rs:549-570`, `crates/proto/src/lib.rs:526-546`, `crates/room/src/lib.rs:1197`.
- At most 32 events per snapshot; an event is resent in every snapshot until one carrying it is acked — `crates/proto/src/lib.rs:39`, `crates/room/src/lib.rs:164-169`, `crates/room/src/lib.rs:1252-1255`.
- Datagrams are timestamped on arrival but only taken in once per render frame — `crates/client-net/src/web.rs:343`, `crates/client-net/src/web.rs:403-416`, `crates/client-net/src/native.rs:216`, `crates/client-net/src/native.rs:259-265`.

**Interpolation of other players**
- Render tick = estimated server tick − playout delay; everyone else is drawn between the two kept snapshots around it (linear position, shortest-arc yaw, linear pitch) — `crates/client-net/src/core.rs:2336-2340`, `crates/client-net/src/core.rs:2535-2548`.
- Playout delay starts at 4 ticks (31.25 ms) and its target is `clamp(2 × snapshot gap + 2 × jitter, 2, 12)` ticks, i.e. 4 ticks at 64 Hz with no jitter, at most 12 (93.75 ms); jitter is an EWMA (gain 1/16) of |arrival error| with each sample capped at 8 ticks — `crates/client-net/src/core.rs:790-794`, `crates/client-net/src/core.rs:1413`, `crates/client-net/src/core.rs:1421-1428`, `crates/client-net/src/core.rs:322-324`.
- It adapts: the delay slews up at 4 ticks per second and down at 1 tick per second — `crates/client-net/src/core.rs:325-329`.
- Server-clock estimate: each snapshot moves it 10% toward what it says; the drawn clock follows at ±10% of real time, up to ±50% when 10+ ticks off; a snapshot more than 19 ticks (148 ms) early re-takes it at once, a late one never does — `crates/client-net/src/core.rs:310-321`, `crates/client-net/src/core.rs:1408-1420`.
- 8 snapshots are kept for interpolation (`INTERP_FRAMES`) — `crates/client-net/src/core.rs:277-278`.
- Extrapolation exists: past the newest snapshot a live remote runs on along its velocity for at most 16 ticks (125 ms, `EXTRAPOLATE_MS`), then is held; two updates more than 250 ms apart are not drawn between — `crates/client-net/src/core.rs:304-309`, `crates/client-net/src/core.rs:2541-2553`, `crates/client-net/src/core.rs:2767-2769`.
- A remote's `alive` flag, shield byte, crouch and gun are taken from the snapshot at or before the render tick, not blended — `crates/client-net/src/core.rs:2536`, `crates/client-net/src/core.rs:2845-2861`.
- Duels on foot stay "drawn in the past and lag-compensated"; only bodies next to a moving vehicle (and the vehicles) are predicted forward in the client's "bubble" — `crates/room/src/bubble.rs:1-17`.
- The net graph (bindable key) shows snapshots, confirmed/corrected counts, depth, clock rate and "interp N ticks" — `crates/client/src/hud.rs:1438-1444`.

**Local prediction and reconciliation**
- The client steps its own fighter with the sim's `step_in` the moment each input is made, keeps 64 inputs/results by seq (500 ms) — `crates/client-net/src/core.rs:267-268`, `crates/client-net/src/core.rs:894-923`.
- Each snapshot carries the server's exact own fighter after `last_seq`; equal to the stored prediction (`same_local`: body, yaw, pitch, guns, refire, reload, burst, melee, charge, grenades, shots…) is a confirmation, otherwise the client adopts the server's and replays the unacked inputs — `crates/client-net/src/core.rs:1680-1709`, `crates/client-net/src/core.rs:1814-1923`, `crates/client-net/src/core.rs:2665-2690`.
- Shield, health, alive, respawn, kills, deaths, camo are never predicted: copied from the server on every snapshot — `crates/client-net/src/core.rs:1928-1940`.
- Correction smoothing: the position jump goes into a render-only offset that decays exponentially, time constant 100 ms for 1 m or more, 325 ms for 0.25 m or less, blended between; below 1 cm dropped; snaps if the total would exceed 4 m or on death/respawn — `crates/client-net/src/core.rs:291-303`, `crates/client-net/src/core.rs:1915-1922`, `crates/client-net/src/core.rs:2730-2746`.

### Inferences
- Effective aim/trigger sampling rate is min(render fps, 128 Hz): at 60 fps about 2.13 ticks are made per frame, all with that frame's yaw. A press on a frame where no tick is due (fps above 128) waits up to one tick (7.8 ms).
- Input send rate is about 60 datagrams/s at 60 fps and 64/s at high fps (the `SEND_EVERY` gate).
- With the low point held at 1.0 to 2.5 frames, an input frame waits roughly 1 to 3.5 ticks (8 to 27 ms) at the server before it runs (arrival quantised to the tick loop, then its place in the pair).
- With zero jitter the interpolation delay sits at its 4-tick (31.25 ms) working floor; the clamp's lower bound of 2 ticks is unreachable at 64 Hz because `2 × gap` is already 4.
- In a browser, datagram arrival is stamped when the main thread services the read promise (`web.rs:343`), so a busy 16.7 ms render frame can add arrival-time noise that the jitter estimator turns into extra playout delay (each 1 tick of measured jitter adds 2 ticks = 15.6 ms). Not measured here; the net graph's "interp" line would show it.

### Gaps
- No measured numbers for real matches (typical depth, corrections per minute, actual interp ticks in Chrome versus desktop): the code has the counters, the repo has no recorded values I could cite other than the loopback test's budgets (`crates/client-net/tests/loopback.rs:351-361`: correction rate ≤ 2%, stall rate ≤ 1% at 60 ms delay, 10 ms jitter, 5% loss).
- Whether production runs big maps at 64 or 32 Hz: `ops/oneshot@.service` does not pass `--big-snapshot-hz` in the lines I grepped, so the default 64 applies, but I did not read the whole unit file.

## 2. Lag compensation, who decides a hit, and the timing chain of a shot and of being shot

### Takeaway
Hits are decided only on the server, rewinding targets to what the shooter saw: depth = (ticks since the snapshot the shooter had acked when it made that input) + (its reported playout, clamped 2..12), capped at 20 ticks (156.25 ms) and at what the link's measured round trip justifies. The shooter sees muzzle flash, tracer, viewmodel kick, casing, ammo and fire sound in the frame the predicted tick fires; hitmarker, hit/kill sound, blood, enemy shield flare and the kill arrive only with the server's snapshot, about RTT + 25 to 55 ms later (inferred).

### Cited Findings
**Rewind**
- Pose ring: every fighter's feet position, crouch depth and alive flag for the last 32 ticks (250 ms); reads between two ticks are interpolated; deepest usable depth 30 ticks; a rewind never resurrects (dead in either row = gone) — `crates/sim/src/consts.rs:41-42`, `crates/sim/src/rewind.rs:19-41`, `crates/sim/src/rewind.rs:60-62`, `crates/sim/src/rewind.rs:88-112`.
- "The depth of a rewind is the server's to choose, never the shooter's" — `crates/sim/src/rewind.rs:3-4`; the client's `playout` is "its own words", clamped by the server — `crates/room/src/lib.rs:50-58`, `crates/proto/src/lib.rs:174-176`.
- Depth formula: `age = tick − acked` (the snapshot the client had acked when the datagram that first carried this input frame was sent), `play = clamp(playout, 2, 12)`, `favour = min(age + play, ticks(160) = 20)`, then `min(favour, favour_cap)`; whole ticks — `crates/room/src/lib.rs:95-103`, `crates/room/src/lib.rs:278-281`, `crates/room/src/lib.rs:302-307`, `crates/room/src/lib.rs:387-397`.
- Link cap: `favour_cap = ticks(min(transport RTT, 160) + 40 ms) + 2 × snap_every`; the RTT is QUIC's own measurement, passed with every input datagram — `crates/room/src/lib.rs:399-406`, `crates/server/src/net.rs:936-937`, `crates/server/src/lib.rs:1521-1525`.
- Acks only move forward, never past what was sent, never before the seat's join tick — `crates/room/src/lib.rs:258-275`.
- Rewound: hitscan rounds, melee, turret fire, being run down — `crates/sim/src/world.rs:764-800`, `crates/sim/src/world.rs:1157-1175`. Slow projectiles fly in the present (design statement) — `docs/DESIGN.md:230-231`.
- Trades: a fighter killed "since the tick its player was drawing" still gets its shots for as long as its rewind depth (at most 20 ticks), fired from itself as it died — `crates/sim/src/world.rs:728-747`, commit 929e967 (2026-11-08).
- Bullet magnetism exists in the sim but is switched off (`MAGNETISM = false`) — `crates/sim/src/consts.rs:103-107`, `crates/sim/src/world.rs:1162`.
- Spread is drawn server-side from the match salt; the Shot event carries the round's true end — `crates/sim/src/world.rs:883-896`, `crates/sim/src/world.rs:1169-1175`.

**Server only**
- "Damage happens only on the server. The client never reports a hit." — `docs/DESIGN.md:129`. The client's projectile code: "The client never decides: no body tests, no blasts" — `crates/client-net/src/proj.rs:11`.
- The client's predicted shot is a ray against the map only, straight down the aim (no spread, no bodies) — `crates/client-net/src/core.rs:1318-1332`.

**What the shooter gets immediately (same render frame as the predicted fire tick)**
- A local Shot event the tick `local.fired` is predicted — `crates/client-net/src/core.rs:925-927`.
- The whole chain runs in one frame, in order: `pump` (input → predict) → `see` → `happened` (events → fx, HUD) → camera → viewmodel → fx draw → HUD — `crates/client/src/lib.rs:666-724`.
- Muzzle flash, tracer and smoke "drawn the frame its trigger breaks" — `crates/client/src/look/fx/shots.rs:1-4`, `crates/client/src/look/fx/shots.rs:192-279`, `crates/client/src/lib.rs:1974-1984`.
- Viewmodel kick and casing from the predicted `shots` counter — `crates/client/src/look/viewmodel.rs:717-748`, `crates/client/src/look/fx/shots.rs:51-57`.
- Own fire sound, centred, gain 0.75, decided after `happened` in the same frame — `crates/client/src/sound/mod.rs:59`, `crates/client/src/sound/mod.rs:1196-1201`.
- A wall impact (sparks, hole) at the end of the predicted map ray — `crates/client/src/look/fx/shots.rs:112-117`, `crates/client/src/look/fx/shots.rs:484-537`.
- Own rockets/grenades/bolts are drawn from the predicted muzzle at once and glide onto the render tick (20% of real time), at most 500 ms ahead of the newest snapshot — `crates/client-net/src/proj.rs:34-43`, `crates/client-net/src/core.rs:1033-1045`.

**What waits for the server**
- The server's own Shot event for our round is discarded by the client ("Played the tick it was predicted") — `crates/client-net/src/core.rs:1612-1614`, `crates/client-net/src/core.rs:1633-1634`.
- Hit, Kill and VehicleHit events (and anything of ours except blasts/wrecks) are handed on the moment their snapshot is processed; everyone else's shots, throws, blasts, spawns, pickups and melees are held until the render tick reaches that snapshot, at most 250 ms (`HOLD_MAX_MS`) — `crates/client-net/src/core.rs:1615-1649`, `crates/client-net/src/core.rs:2271-2323`, `crates/client-net/src/proj.rs:47-48`.
- A Hit event is sent only to the shooter and the victim — `crates/room/src/lib.rs:1770-1781`.
- Hitmarker and hit/headshot/kill sounds fire from those events — `crates/client/src/hud.rs:1180-1186`, `crates/client/src/hud.rs:1200-1202`, `crates/client/src/sound/mod.rs:1244-1272`, `crates/client/src/sound/mod.rs:1373-1382`.
- Blood on an enemy we hit: only for health hits (part 1 or 2), drawn at the victim's chest from the Hit event — `crates/client/src/look/fx/mod.rs:667-703`.
- An enemy's shield flare and break are driven by the drawn (render-tick) shield byte dropping, not by the event — `crates/client/src/look/fx/bodies.rs:5-7`, `crates/client/src/look/fx/bodies.rs:182-195`.
- Kill feed line, "KILLED X" line and the death card come from `Msg::Kill` on the reliable stream, pushed the tick of the kill — `crates/room/src/lib.rs:961-973`, `crates/client/src/hud.rs:1255-1298`.

**Being shot**
- Own shield/health change with the snapshot (never predicted) — `crates/client-net/src/core.rs:1933-1934`.
- Hurt rim, direction arc and hit-on-us sound play at once from the Hit event — `crates/client/src/hud.rs:1187-1199`, `crates/client/src/sound/mod.rs:1261-1262`, `crates/client/src/sound/mod.rs:1383-1393`.
- The attacker's muzzle flash, tracer and gun sound come from their Shot event, which is held for the render tick — `crates/client-net/src/core.rs:1642-1648`, `crates/client/src/lib.rs:1935` (the same drained list feeds fx and `sound::Heard`).

### Inferences
- **Hit-confirm latency (inferred):** wait for a tick boundary 0 to 7.8 ms → datagram batching 0 to 7.8 ms → uplink → server buffer about 8 to 27 ms → next snapshot tick 0 to 7.8 ms → downlink → next render frame 0 to 16.7 ms at 60 Hz. Total ≈ RTT + 8 to 67 ms, typically RTT + about 4 ticks (31 ms) + part of a frame. At 40 ms RTT that is roughly 70 to 95 ms from click to hitmarker.
- **Typical rewind depth (inferred):** age ≈ RTT + 2 to 4 ticks, plus playout 4 ticks → about RTT + 47 to 62 ms. Link cap in ticks: RTT 20 ms → 12 ticks (94 ms); 50 ms → 16 (125 ms); ≥ 85 ms → 20 (the absolute cap). So the 156 ms cap starts to bite around 95 to 110 ms RTT; above it the shooter must lead.
- **"Shot behind cover" (inferred):** on the victim's screen the worst case is the shooter's favour (≤ 156 ms) + the victim's own lead on the server (uplink + 8 to 20 ms buffer) + downlink + a frame. At 40 ms RTT on both sides: about 70 to 86 ms favour + about 50 to 70 ms ≈ 120 to 155 ms after the victim saw themselves reach cover; worst case at high shooter ping about 156 ms + victim RTT + about 30 ms.
- **Peeker's advantage (inferred):** time from the peeker's screen clearing the corner to the holder's screen showing them = peeker uplink + peeker buffer (8 to 20 ms) + snapshot wait (0 to 15.6 ms) + holder downlink + holder playout (≥ 31.25 ms) + frames. Even at zero ping that is about 47 to 75 ms; at 20 ms one-way each, about 85 to 115 ms. The fixed parts are the 4-tick interpolation delay and the 1 to 2.5 frame input buffer.
- Order of feedback for the shooter on a hit: flash/tracer/sound at t0; hitmarker + hit sound at t0 + RTT + ~30 ms; the enemy's shield flare/pop and death ragdoll one playout delay (≥ 31 ms) after the hitmarker, because those follow the interpolated body.
- Order for the victim: rim + arc + hit sound + shield bar first; the attacker's tracer, flash and gunshot sound at least 31 ms later (they wait for the render tick), plus speed-of-sound delay past 25 m.

### Gaps
- No telemetry in the repo of actual rewind depths per shot or of how often the cap is hit; `crates/room/tests/rewind.rs` holds the bound, not a distribution.
- I did not read the sim's melee/lunge rewind details (the sim researcher's area).

## 3. Fog of war: how early an enemy is revealed, pop-in risk, unseen shots

### Takeaway
An enemy is sent when any of 7 viewer eye points (now, velocity-led, ±0.4 m sideways, +0.6 m up) has a clear line to any of 10 target points (head, chest, feet, chest edges; now and velocity-led). The lead is `min(RTT/2 + one snapshot + playout, 200 ms)`, the full test runs every other snapshot (every fourth past 40 m), and a seen enemy stays sent for 12 ticks (94 ms). Unseen gunfire within 60 m is sent as a sound only, its position rounded to a 2 m grid with no direction.

### Cited Findings
- Constants: `HOLD_TICKS = 12` (94 ms), `PEEK_M = 0.4`, `JUMP_M = 0.6`, `MAX_LEAD_S = 0.2` — `crates/room/src/fog.rs:24-31`.
- Lead = `rtt_ms/2000 + snap_every/128 + clamp(playout, 2, 12)/128` seconds, capped at 0.2 s; `rtt_ms` is the room's own snapshot-ack round trip (EWMA 1/8) — `crates/room/src/fog.rs:33-39`, `crates/room/src/lib.rs:266-274`, `crates/room/src/lib.rs:1107`.
- Eyes: now, now + velocity × lead, each ± 0.4 m across the line to the target, and now + 0.6 m up — `crates/room/src/fog.rs:55-69`.
- Target points: head, chest, feet (+0.15 m), chest ± (HIT_R + 0.1 m), at its place now and at place + velocity × lead — `crates/room/src/fog.rs:71-91`.
- Schedule: fog looks on snapshot ticks only; each enemy gets the full look every 2nd snapshot (31.25 ms), with the lead stretched by the skipped snapshot, and the "plain" test (chest or head in clear line now) on the snapshots between; past 40 m flat the full look is every 4th snapshot (62.5 ms), without the side leans and chest edges (18 rays, not 70) — `crates/room/src/fog.rs:144-177`, `crates/room/src/fog.rs:303-332`, `crates/room/src/fog.rs:342-387`.
- A respawned enemy loses any hold: "never an unseen spawn" — `crates/room/src/fog.rs:360-364`. Teammates are always sent — `crates/room/src/fog.rs:368-369`. Camouflage past its range is not sent at all — `crates/room/src/fog.rs:370-373`.
- Client side, a newly sent enemy is drawn only once the render tick reaches the first snapshot that carried it (the frame at or before the render tick must contain the slot) — `crates/client-net/src/core.rs:2380-2387`, `crates/client-net/src/core.rs:2409-2418`.
- Enemy name tags need the crosshair on the body and a clear line to the head now, because "being sent is not being seen" — `crates/client/src/tags.rs:5-13`.
- Unseen shots: a Shot by a shooter the client may not know of is sent only within `HEAR_SHOT_M = 60` m, with `part = HEARD` and `pos = end =` the muzzle rounded to `HEARD_GRID_M = 2` m; a Throw within 30 m likewise; a Blast in clear line is sent whole, otherwise heard within 60 m — `crates/room/src/lib.rs:1679-1691`, `crates/room/src/lib.rs:1758-1787`.
- A hit on someone the client may not know of hides victim, place, part and amount — `crates/room/src/lib.rs:1693-1702`, `crates/room/src/lib.rs:1776-1779`.
- Client: a HEARD shot draws nothing and plays the gun's sound at the rounded place (the far take past 35 m or through a wall); no fly-by or impact sound — `crates/client/src/look/fx/mod.rs:639`, `crates/client/src/sound/mod.rs:1203-1216`.
- Projectiles are sent if the owner is known or the round itself is in clear line — `crates/room/src/lib.rs:1388-1408`.
- Tests: `a_runner_is_seen_before_it_clears_the_corner` (7 m/s, 200 ms lead), `a_lean_around_a_pillar_shows_them_before_the_chest_is_in_line` — `crates/room/src/fog.rs:504-527`. The leak gate tests leaks, not late reveals — `docs/DESIGN.md:633`.

### Inferences
- Worked lead at 40 ms RTT, 4-tick playout: 20 + 15.6 + 31.25 = 66.9 ms (82.5 ms on the full-look snapshot; 113.8 ms for a far pair). At run speed 6.86 m/s (`crates/sim/src/consts.rs:49`) that is 0.46 m / 0.57 m of look-ahead on each body's velocity, plus the 0.4 m side lean.
- **A holder never sees a late pop:** the enemy is revealed on where it will be `lead` seconds ahead, and it is then drawn a further downlink + playout in the past.
- **A fast peeker can (inferred, needs an in-game test):** the viewer's own screen is ahead of the server's copy of the viewer by uplink + input buffer, and a first-seen enemy is drawn downlink + playout after the snapshot, so the viewer's eye would need a lead of about RTT + buffer + playout. Fog gives RTT/2 + snapshot + playout. The shortfall, about RTT/2 + 8..20 ms − 15.6 ms, is covered by the 0.4 m side lean only while `shortfall × strafe speed ≤ 0.4 m`, i.e. up to roughly 58 to 66 ms of shortfall (RTT of roughly 110 to 145 ms, depending on the input buffer). Above that, or when the full look lands on the off snapshot, an enemy could appear a few tens of ms after the corner is cleared.
- The lead uses current velocity, so a standing start is covered only by the fixed 0.4 m lean and the chest-edge points, and a jump peek by the single +0.6 m eye (no jump + lean combination).
- After the hold lapses (94 ms out of sight) the body simply stops being drawn; it reappears one playout delay after the snapshot that re-reveals it.

### Gaps
- No test or metric in the repo measures "first drawn versus first geometrically visible on the viewer's own screen"; the pop-in risk above is arithmetic, not observation.

## 4. Mouse and pad input

### Takeaway
Mouse look is raw accumulated motion × a Source-scale sensitivity (0.022°/count × setting, default 3.0 = 0.066°/count), no smoothing or acceleration in the game's code, applied to the camera every render frame and sent as 16-bit angles (0.0055° steps). Scoped sensitivity is divided by the eased magnification with no separate setting. There is **no gamepad play at all**: the pad only navigates menus, and there is no aim assist of any kind (bullet magnetism is also off).

### Cited Findings
- Look is driven by Bevy's `AccumulatedMouseMotion` (the frame's summed mouse delta): `yaw += dx × sens`, `pitch −= dy × sens × (±1 invert)`, pitch clamped to ±`PITCH_MAX` (16202/65536 turn = 89.0°) — `crates/client/src/lib.rs:60`, `crates/client/src/lib.rs:917`, `crates/client/src/lib.rs:1017-1028`, `crates/sim/src/consts.rs:116`.
- No smoothing, filtering or acceleration term anywhere in that path — `crates/client/src/lib.rs:1017-1028`.
- Sensitivity scale: `SENS_DEG = 0.022°` per count at 1.0 ("Source's, so CS2's and Apex's numbers mean the same here"); default 3.0; range 0.1 to 20; steps 0.01 below 1, 0.05 to 5, 0.25 above — `crates/client/src/settings.rs:24-42`, `crates/client/src/lib.rs:280-283`.
- Scoped: `sens = base / drawn_mag`, the magnification eased at `1 − exp(−14·dt)` (halfway to 2× in 50 ms); FOV is divided by the same number; no ADS-sensitivity option in the settings rows — `crates/client/src/lib.rs:99-101`, `crates/client/src/lib.rs:1018-1021`, `crates/client/src/lib.rs:1797`, `crates/client/src/settings.rs:212-238`.
- Zoom levels per gun are 2× (rifles) or 5×/10× (snipers); a tap steps the level, a press held longer than 300 ms unscopes on release; any damage, reload, melee, throw or swap knocks the scope off — `crates/sim/src/arms.rs:212-226`, `crates/client/src/scope.rs:1-2`, `crates/client/src/lib.rs:114-115`, `crates/client/src/lib.rs:976-1012`.
- Browser: the page wraps `canvas.requestPointerLock` to ask for `{ unadjustedMovement: true }`; not asked on Linux (non-ChromeOS), and a `NotSupportedError` is remembered in localStorage and the plain lock used from then on — `crates/client/web/boot.js:12-32`.
- The mouse turns and fires only while the pointer is actually locked; the click that takes the lock fires nothing — `crates/client/src/lib.rs:924-928`, `crates/client/src/lib.rs:1035-1041`, `crates/client/src/pointer.rs:72-86`.
- Wire quantisation: yaw `u16` and pitch `i16` in 1/65536 turn (0.00549°), made by truncation (`as u32 as u16`, `as i16`) — `crates/sim/src/input.rs:28-33`, `crates/client/src/lib.rs:1056-1063`, `crates/proto/src/lib.rs:213-214`.
- The camera uses the float `look.yaw/pitch` every render frame; the eye position is the predicted body interpolated between its last two ticks — `crates/client/src/lib.rs:1795-1796`, `crates/client-net/src/core.rs:1943-1967`.
- FOV: default 90° horizontal on 16:9 (vertical 58.7°, Hor+ on other shapes), range 70 to 120 in 1° steps — `crates/client/src/look/rig.rs:28-38`, `crates/client/src/settings.rs:43-46`, `crates/client/src/settings.rs:114`.
- Default binds: fire Mouse 1, zoom Mouse 2 (shared with the left gun's trigger), melee Q, grenade F — `crates/client/src/binds.rs:570-574`.
- Pad: "A pad on the game's screens (the title, the Esc menu, the settings)"; stick threshold 0.55, repeat 0.35 s then 0.12 s; gilrs always in a browser, behind the `pad` feature on desktop (the shipped depots enable it) — `crates/client/src/pad.rs:1-16`, `crates/client/src/pad.rs:25-28`, `crates/client/Cargo.toml:63`, `ci/depot.py:47`, `ci/depot.py:55`.
- `Gamepad` is referenced nowhere in `crates/client/src` outside `pad.rs` (grep), and `pump` builds its input from keys, mouse buttons and the wheel only — `crates/client/src/lib.rs:947`, `crates/client/src/lib.rs:1032-1052`.
- The red reticle test uses the gun's `aim_range` and, when magnetism is on, the widened body; with `MAGNETISM = false` it is red only on a body — `crates/client/src/hud.rs:2146-2196`, `crates/sim/src/consts.rs:103-107`.

### Inferences
- Aim is not visibly quantised to the tick rate: the camera turns every frame. What the server uses is the frame's angle truncated to 0.0055°, a worst-case 4.8 mm at 50 m.
- At the default 3.0 one mouse count is 12 wire steps; at the minimum 0.1 a count (0.0022°) is finer than a wire step, so several counts can map to the same sent angle while the camera still moves (sub-step, not perceptible).
- On desktop, Bevy/winit's mouse motion is the OS's raw device delta (not in this repo: Bevy 0.18 / winit 0.30 behaviour), so OS pointer acceleration should not apply there; on Linux browsers it can, because unadjusted movement is not requested.
- With no stick-to-look mapping, there are no pad curves, dead zones, turn acceleration or aim assist to report: a controller player cannot aim or move in a match today.

### Gaps
- Whether Bevy 0.18's web backend coalesces pointer events per frame (losing sub-frame motion resolution) was not checked; it is outside the repo.

## 5. Frame pacing

### Takeaway
The local fighter is drawn interpolated between its last two predicted ticks (so up to one tick, 7.8 ms, behind the prediction), view angles are applied per frame, desktop defaults are vsync on (`AutoVsync`) with no frame cap, and a browser always paces to `requestAnimationFrame`.

### Cited Findings
- Own body: `prev_pos + (pred_pos − prev_pos) × alpha + correction offset`, alpha = clock fraction through the tick; "one tick behind the clock, so every frame moves by its own share of time" — `crates/client-net/src/core.rs:1943-1967`.
- Camera position comes from that plus the sim's eye height; rotation from the per-frame look — `crates/client/src/lib.rs:1733-1736`, `crates/client/src/lib.rs:1792-1796`.
- Everyone else, vehicles and projectiles are sampled once per frame at one timestamp (`Now`), read in `First` — `crates/client/src/lib.rs:511`, `crates/client/src/lib.rs:1877-1911`.
- Desktop: `vsync = true` → `PresentMode::AutoVsync`, off → `AutoNoVsync`; `max_fps = 0` (uncapped) by default, ladder 30/60/120/144/240/none; the cap is a `thread::sleep` to a running deadline at the end of the frame — `crates/client/src/settings.rs:54`, `crates/client/src/settings.rs:121-124`, `crates/client/src/display.rs:79-86`, `crates/client/src/display.rs:230-249`.
- Browser: "the page paces every frame to the display (`requestAnimationFrame`), so VSYNC and FRAME CAP say so on the screen rather than doing nothing" — `crates/client/src/display.rs:13-15`.
- Camera shake and landing dip are added to the drawn camera only, never to the aim sent — `crates/client/src/lib.rs:1810-1812`, `crates/client/src/look/shake.rs:17-19`.

### Inferences
- The desktop build enables Bevy's `multi_threaded` feature (`crates/client/Cargo.toml:73`) and nothing in `lib.rs` disables pipelined rendering (no `.disable::<…>` on `DefaultPlugins`, `crates/client/src/lib.rs:462-478`), so the render world likely runs one frame behind the main world, adding up to a frame of display latency on desktop. This is Bevy behaviour, not verified in this repo.
- No `desired_maximum_frame_latency` is set on the window (`crates/client/src/lib.rs:469-476`), so wgpu's default swap-chain depth applies with vsync on.
- With vsync on by default at 60 Hz, input-to-photon is at least one frame for sampling plus the present queue; the game adds nothing on top for the player's own actions (same-frame prediction and effects).

### Gaps
- No measured input-to-photon latency exists in the repo for either target.

## 6. Hit, damage, kill and death feedback: inventory

### Takeaway
The basics are there (hitmarker, distinct hit/shield/headshot/kill sounds, kill feed, medals, announcer, hurt rim, direction arc, shield-break effects, low-shield alarm, ragdoll, death card), but the hitmarker has only two states (hit, kill), the reticle shows neither spread nor bloom, nothing moves the camera on firing or being hit (deliberately, since 2026-11-09), and the shooter's confirmed-hit visuals on the target (blood, shield flare) are either delayed by the interpolation buffer or absent at the true impact point.

### Cited Findings
| Element | State | What the code does, key numbers | Where |
|---|---|---|---|
| Hitmarker | present | 26 px X (two 26×2 px strokes at 45°), white, shown 0.22 s, no fade (visibility toggle) | `crates/client/src/hud.rs:45`, `crates/client/src/hud.rs:906-938`, `crates/client/src/hud.rs:1467-1491` |
| …on kill | present | red (1.0, 0.25, 0.2), 0.5 s, pops in at 1.9× and settles to 1.3× over the first 30% | `crates/client/src/hud.rs:46-51`, `crates/client/src/hud.rs:1474-1488` |
| …shield hit vs health hit vs shield break vs headshot | absent | one white marker for every non-kill hit; `hit_kill` is the only state | `crates/client/src/hud.rs:1180-1186`, `crates/client/src/hud.rs:1200-1202` |
| Hit-confirm sound | present | per body per frame, by worst part: `shield_hit` (3 takes, gain 0.35), `hit_0` (health, 0.5), `headshot` (2 takes, 0.5); +12% per extra hit that frame up to 1.5× | `crates/client/src/sound/mod.rs:156-159`, `crates/client/src/sound/mod.rs:1250-1260`, `crates/client/src/sound/mod.rs:1373-1382` |
| Headshot sound | partial | only for a head hit that reached health (part 2); a head hit absorbed by shields (part 3) sounds as a shield hit | `crates/sim/src/events.rs:15-17`, `crates/client/src/sound/mod.rs:1250-1257` |
| Kill sound | present | `kill_0`, gain 0.7 | `crates/client/src/sound/mod.rs:1268-1272` |
| Enemy shield flare | present, delayed | gold shell, peak 1.0 on a hit, 1.6 on break, fading 3.0 per second (0.33 s / 0.53 s); triggered when the drawn shield byte drops | `crates/client/src/look/fx/bodies.rs:20-24`, `crates/client/src/look/fx/bodies.rs:182-195`, `crates/client/src/look/shield.rs:1-5` |
| Enemy shield break effect + sound | present | zap, 10 arcs, 4×5 sparks, light flash 0.18 s; `shield_pop_0` at the body (gain 0.8, carries 40 m) | `crates/client/src/look/fx/bodies.rs:257-287`, `crates/client/src/sound/mod.rs:1554-1557` |
| Blood | partial | health hits only, at the victim's chest (feet + 1.0 m), from the Hit event; other players' rounds bleed at the true impact point | `crates/client/src/look/fx/mod.rs:667-703`, `crates/client/src/look/fx/shots.rs:484-490` |
| Own-shot impact on the target | absent | the predicted event ends at the map; the server's own-Shot event is dropped | `crates/client-net/src/core.rs:1318-1332`, `crates/client-net/src/core.rs:1633-1634`, `crates/client/src/look/fx/shots.rs:112-117` |
| Damage numbers | absent | no such code (grep); the design mentions them | `docs/DESIGN.md:156-157` |
| Kill feed | present | top left, 5 lines, 5 s each, last 0.8 s fading; "killer · how · HEAD · victim" | `crates/client/src/hud.rs:32-40`, `crates/client/src/hud.rs:1255-1269`, `crates/client/src/hud.rs:1543-1546` |
| "KILLED X · HEAD" line | present | under the crosshair, 1.8 s, last 0.5 s fading | `crates/client/src/hud.rs:52-57`, `crates/client/src/hud.rs:1281-1289`, `crates/client/src/hud.rs:1515-1541` |
| Medals | present | 43 medals (ids 1 to 43); up to 6 on screen, 4 s each | `crates/proto/src/medal.rs:151`, `crates/client/src/hud.rs:199-201` |
| Announcer | present | voiced: multi-kills, sprees, Extermination, Perfection; plus 19 match lines (mode, flag, lead, one minute, thirty seconds, game over, betrayal); queue of 4 | `crates/proto/src/medal.rs:158-204`, `crates/client/src/sound/mod.rs:1065-1091` |
| Reticle | present, static | 4 strokes of 7×2 px in a 22 px box, white 0.85 alpha; hidden scoped or dead | `crates/client/src/hud.rs:883-905`, `crates/client/src/hud.rs:1458-1466` |
| Reticle red on target | present | red when the aim ray meets an enemy's hit volume (as drawn) within the gun's `aim_range` before the map; the scope's ring too | `crates/client/src/hud.rs:2146-2211`, `crates/client/src/scope.rs:25-26` |
| Reticle spread / bloom | absent | no size change with spread, movement or fire; the same reticle for every gun | `crates/client/src/hud.rs:883-905` |
| Enemy name on aim | present | only with the crosshair on them and their head in clear line | `crates/client/src/tags.rs:5-13` |
| Damage direction indicator | present, minimal | one 56×6 px red arc 90 px from centre, 0.6 s linear fade from 0.85 alpha; angle fixed at the moment of the hit; from where the shooter is drawn, else where they were last heard firing, else no arc | `crates/client/src/hud.rs:58`, `crates/client/src/hud.rs:65-66`, `crates/client/src/hud.rs:950-955`, `crates/client/src/hud.rs:1187-1196`, `crates/client/src/hud.rs:1494-1511` |
| Screen flash when hit | present, faint | edge rim: red alpha 0.38 on health, gold alpha 0.09 on shield, 0.5 s linear fade, clear inside 52% of the corner radius | `crates/client/src/hud.rs:59-68`, `crates/client/src/hud.rs:2267-2289` |
| Own shield break | present | blue-white rim flicker alpha 0.55, 0.45 s, 22 Hz; light flash 0.2 s; `shield_pop_0` (0.7) + `hurt` (0.35) | `crates/client/src/hud.rs:69-75`, `crates/client/src/hud.rs:2290-2296`, `crates/client/src/look/fx/bodies.rs:227-242`, `crates/client/src/sound/mod.rs:1406-1411` |
| Hit-on-us sound | present | `shield_hit` 0.55 or `flesh` 0.6 | `crates/client/src/sound/mod.rs:1383-1393` |
| Camera kick / flinch when hit | absent (deliberate) | "no hit on us moves the frame" | `crates/client/src/look/shake.rs:9-13`, `crates/client/src/look/fx/mod.rs:657-661`, commit 97b9606 |
| Low-shield warning | present | bar turns red and its frame pulses (sin at 9 rad/s ≈ 1.4 Hz) under 25%; alarm bip every 0.28 s while the shield is at 0 and not recharging | `crates/client/src/hud.rs:113-114`, `crates/client/src/hud.rs:1974-1988`, `crates/client/src/sound/mod.rs:116-117`, `crates/client/src/sound/mod.rs:1420-1426` |
| Shield recharge | present | `shield_charge_0` (0.45) once as it starts; arcs at 7 per second and a 0.22 shimmer on bodies | `crates/client/src/sound/mod.rs:1412-1419`, `crates/client/src/look/fx/bodies.rs:25-28`, `crates/client/src/look/fx/bodies.rs:196-201` |
| Low-health heartbeat / vignette | absent | none (grep); a health bar shows only when below full | `crates/client/src/hud.rs:1989-1996` |
| Viewmodel recoil | present | spring 6.5 Hz, damping 0.55; single shot: 0.04 m back, 0.012 m up, 0.085 rad climb, 0.035 rad roll; automatic ×0.4, burst ×0.55, heavy ×1.3, stacking to ×1.8; shotgun pump 0.3 s | `crates/client/src/look/viewmodel.rs:216-244` |
| Camera recoil / aim punch | absent (deliberate) | "No jolt of the frame: the crosshair stays where the rounds go" | `crates/client/src/look/fx/shots.rs:253-254`, `crates/client/src/look/shake.rs:9-13` |
| Muzzle flash | present, predicted | own: 62 to 78 ms (powder), 160 ms (energy); a view light of 18,000 lm, 0.9 m | `crates/client/src/look/fx/shots.rs:35-39`, `crates/client/src/look/viewmodel.rs:125-126` |
| Tracers | present | own: one per shot down the crosshair to the map (shotgun: 6 lines with a client-random 0.09 rad spread); others: every `every`-th round (AR every 2), along the server's true line | `crates/client/src/look/fx/shots.rs:71`, `crates/client/src/look/fx/shots.rs:258-278`, `crates/client/src/look/fx/shots.rs:153-157`, `crates/client/src/look/fx/table.rs:959`, `crates/client/src/look/fx/table.rs:1052` |
| Shell casings | present | brass or shell from the port | `crates/client/src/look/fx/shots.rs:51-57`, `crates/client/src/look/fx/shots.rs:621-627` |
| Screen shake | blasts and hard landings only | trauma 1.0 within 4 m falling to 0 at 28 m (squared); max 0.018 rad pitch, 0.011 rad yaw, 0.02 m; decays 1.4 per second; 35 ms attack; no roll | `crates/client/src/look/shake.rs:23-47`, `crates/client/src/look/fx/blast.rs:278` |
| FOV punch | absent, except the sword lunge (+14%) | "no field-of-view pulse" | `crates/client/src/look/shake.rs:14-15`, `crates/client/src/look/cels.rs:57-59`, `crates/client/src/look/cels.rs:212-214` |
| Death: ragdoll | present | 18-point Verlet from the frame of death, pushed by the blow (shot 3.2 m/s, head 2.5, melee 3.5), limp up to 8 s; own body too | `crates/client/src/look/rag.rs:1-28`, `crates/client/src/look/rag.rs:264`, `crates/client/src/look/rag.rs:273-275` |
| Death cam | present | third person over our own body, 3.4 m back, pulled out of the eye over 0.55 s, mouse orbits | `crates/client/src/lib.rs:1686-1693`, `crates/client/src/lib.rs:1744-1767` |
| Killer display | present | death card band: killer's emblem, name, gun, landing in 0.22 s; scoreboard after 1.5 s dead; respawn 3 s | `crates/client/src/scores.rs:3-5`, `crates/client/src/scores.rs:37`, `crates/client/src/hud.rs:76-78`, `crates/sim/src/consts.rs:132` |
| Kill cam (killer's eye replay) | partial | computed in the netcode (2.5 s run-up) but the Bevy client only reports whether one exists to the e2e hook; it is never drawn | `crates/client-net/src/core.rs:282-283`, `crates/client-net/src/core.rs:2590-2662`, `crates/client/src/lib.rs:2073` |

### Inferences
- The shooter's hit feedback is two channels with different clocks: the hitmarker and sound are immediate on snapshot arrival, the target's own reaction (flare, pop, falling over) is one interpolation delay later.
- When our round hits an enemy, the only world-space effect at once is a bullet mark on the wall behind them (the predicted map ray); nothing is drawn where the round actually stopped.
- A 0.09-alpha gold rim is the only screen response to shield damage besides the bar and sound; with no camera flinch it may be easy to miss being shot at range.

### Gaps
- I did not open every recipe in `look/fx/table.rs` (1321 lines) for per-gun impact particle counts; `tools/fx.sh` is the repo's way to look at those.

## 7. Audio

### Takeaway
One custom mixer (48 kHz, 128-frame blocks, 40 one-shots + 32 loops) on the audio thread; every world sound is panned, distance-attenuated, low-passed with distance and behind the listener, and muffled through walls by a single line-of-sight ray. The player's own fire sound is played in the frame the shot is predicted; enemy footsteps are synthesised client-side from drawn bodies, so only enemies the fog sends can be heard walking.

### Cited Findings
- Mixer: 48 kHz, block 128 frames (2.7 ms), 40 one-shot voices (quietest gives way), 32 held loops, peak limiter last — `crates/mix/src/lib.rs:26-41`.
- Browser: an AudioWorklet with `latencyHint: 'interactive'`; desktop: a Bevy audio source, only with the `sound` feature, which the shipped depots enable — `crates/client/web/mix.js:73`, `crates/client/src/sound/mod.rs:1-5`, `crates/client/src/sound/mod.rs:37-41`, `ci/depot.py:47`, `ci/depot.py:55`.
- Spatial model: gain `near/(near+d) × (1 − d/reach)²`; equal-power pan at 85% width; low-pass from 16 kHz falling with distance, ×0.6 behind the listener; a blocked line (one `Map::clear` ray, sources beyond 1.5 m) halves the gain and caps the low-pass at 900 Hz — `crates/client/src/sound/mod.rs:110-115`, `crates/client/src/sound/mod.rs:376-398`.
- Speed of sound 343 m/s applied only beyond 25 m ("closer, it would read as lag"); Doppler ×1.6 exaggerated, clamped 0.6..1.7 — `crates/client/src/sound/mod.rs:105-109`, `crates/client/src/sound/mod.rs:399`, `crates/client/src/sound/mod.rs:411-422`.
- Carry (full to about `near`, silent at `reach`): guns 25/260 m, blasts 40/320 m, steps 4/28 m, body sounds 4/22 m, shields 6/40 m — `crates/client/src/sound/mod.rs:122-138`.
- Enemy gunfire: positional; the far take beyond 35 m or through a wall; fly-by (within 6 m of the line) and near-impact sounds for hitscan — `crates/client/src/sound/mod.rs:97-98`, `crates/client/src/sound/mod.rs:1203-1241`.
- Enemy footsteps: one step per 2.6 m travelled above 2.5 m/s on the ground, by surface, gain 0.6 (crouched 0.15), positional; own steps 0.3 (crouched 0.12), unplaced — `crates/client/src/sound/mod.rs:118-120`, `crates/client/src/sound/mod.rs:1502-1509`, `crates/client/src/sound/mod.rs:1567-1576`.
- "Footsteps never cross the wire: only a fighter fog sends is heard walking" — `docs/DESIGN.md:288-289`; the client loop runs over the drawn `others` only — `crates/client/src/sound/mod.rs:1532-1536`.
- Own fire: `ear.mine(shot, 0.75)` from the predicted Shot event, same frame (`decide` runs after `happened`) — `crates/client/src/sound/mod.rs:59`, `crates/client/src/sound/mod.rs:1189-1201`.
- Other players' reloads, landings, shield pops/recharges and deaths are positional, derived from drawn state — `crates/client/src/sound/mod.rs:1554-1589`.

### Inferences
- Occlusion is binary (one ray to a point 0.2 m above the source), so a sound flips between open and muffled as that single line crosses an edge; there is no partial occlusion or diffraction.
- Because steps come from fog-sent bodies, footstep audio begins at the same moment the body becomes drawable (fog lead ≈ 70 to 115 ms before line of sight): you cannot hear an approaching enemy through a wall much before you could see them, unlike games that send footsteps beyond visibility.
- Enemy gunshot sound follows the render tick (≥ 31 ms after arrival) while "you were hit" sounds are immediate, so the hit sound precedes the shot sound.

### Gaps
- Output latency (AudioContext `baseLatency`/`outputLatency`, rodio's buffer on desktop) is not set or logged in the repo beyond the `interactive` hint.

## 8. DESIGN.md versus the implementation, design targets, recent commits

### Takeaway
The implementation matches DESIGN §3 to §8 closely on rates, prediction, interpolation and rewind; the differences are small wording gaps plus three feedback items the design names that the Bevy client does not show (damage numbers, the kill cam), and the design sets bounds (160 ms favour, impaired-network budgets) rather than an end-to-end latency budget.

### Cited Findings
- Design rates: 128 Hz tick, 64 Hz snapshots, inputs "sampled every tick, sent at 64 Hz, each datagram resending every unacked frame (at most 16)" — `docs/DESIGN.md:91-95`; matches code (section 1).
- Design: "Own shots are drawn now: tracer, muzzle flash, recoil kick, ammo count, reload. Their direction is the centre of the reticle (spread is the server's secret, §9). Hit markers, damage numbers and kills arrive from the server, a round trip later." — `docs/DESIGN.md:154-157`. Damage numbers are not implemented (section 6).
- Design: correction offset "decays (100 to 325 ms, blended by size), snapping past 4 m"; clock band "1 to 2.5 inputs" — `docs/DESIGN.md:144-153`; matches `crates/client-net/src/core.rs:291-303`, `crates/client-net/src/core.rs:2754-2763`.
- Design: playout "starts at 2 snapshot intervals (31 ms at 64 Hz) and adapts from measured jitter … within 1 to 6 intervals, slewed (up 4 ticks a second, down 1)"; "extrapolate at most 125 ms from velocity, then hold" — `docs/DESIGN.md:175-183`; matches code, though the first bullet's heading says remotes are "never guessed ahead" — `docs/DESIGN.md:165`.
- Design: "at most 160 ms of favour. A shooter above 200 ms RTT gets no more favour than 200 ms would earn" — `docs/DESIGN.md:224-226`. Code: 20 ticks = 156.25 ms; the link cap clamps RTT at 160 ms before adding 40 ms — `crates/room/src/lib.rs:48-49`, `crates/room/src/lib.rs:403-406`.
- Design: a missing frame "repeats the last one with tap buttons cleared" — `docs/DESIGN.md:117-119`. Code keeps taps held on purpose (clearing would make the next frame a second press) — `crates/sim/src/input.rs:53-60` (`Input::repeated`).
- Design: fog pushes the viewer's eye ahead by "its one-way latency + a snapshot interval + its playout delay", hold 12 ticks — `docs/DESIGN.md:271-281`; matches `crates/room/src/fog.rs:33-39`.
- Design admits the residual: "an enemy is sent up to about a latency's worth before they are truly on screen … the price of no pop" — `docs/DESIGN.md:300-302`.
- Kill cam: milestone 6 lists "the kill cam … Done" and §8 says it "replays only what this client was already sent" — `docs/DESIGN.md:283-284`, `docs/DESIGN.md:557-558`; the Bevy client does not draw it (section 6).
- Gates the design names: lag-comp bound (no shot with more than 160 ms of favour) and the impaired-network budgets at 60 ms delay, 10 ms jitter, 5% loss — `docs/DESIGN.md:634-635`; the budgets are correction rate ≤ 0.02 and stall rate ≤ 0.01 — `crates/client-net/tests/loopback.rs:351-361`.
- "Numbers in this page that come from measurements cite their source. Every other number is a starting value, set in code and tuned from matches." — `docs/DESIGN.md:639-640`.
- CLAUDE.md: CI runs only when asked; "no size, fog or frame-rate budget stops a build any more" — `CLAUDE.md` (The loop; item 10).
- `ci/e2e.mjs` "predictions noted": it prints the share of predictions confirmed once the page draws steadily; "Noted, never a gate" (SwiftShader's 2 to 20 fps makes it luck); "A real GPU draws 60+ and confirms 99%+" — `ci/e2e.mjs:262-266`.
- Recent netcode/feel commits (`git log`): 97b9606 (2026-11-09) removed camera kick on our shots and flinch on hits, because the crosshair was "up to a degree off where the rounds went (an automatic's climbed and stayed)"; 929e967 (2026-11-08) trades under lag; 53e25e8 map builds no longer hold every room's tick (up to 185 ms); 785dfcc (2026-11-05) blast shake no longer pops the frame; f3893e9 (2026-11-07) tracers by their own shader; 119105f (2026-11-07) magnetism reworked and switched off; e3e4a85 (2026-11-07) lag compensation per target; 693e7bd (2026-11-07) clock stall fix; c56f7ad (2026-11-07) Halo 2 PC keys by default.

### Inferences
- There is no end-to-end latency budget (click-to-hitmarker, peeker's advantage, input-to-photon) written in the design or enforced by a test; the only numeric targets are the 160 ms favour cap and the loopback budgets.
- Combat feel changed materially in the last five days (camera kick removed on the audit date itself, magnetism off, trades added), so "something feels off" may partly reflect very recent changes rather than long-standing tuning.

### Gaps
- I grepped DESIGN.md (§3 to §8, §16, §17) rather than reading all 700+ lines; §9 (spread/salt) and §13 (anti-cheat) were not read.

## 9. Things that look off (my own observations; each needs an in-game check)

### Takeaway
Nothing in the netcode is broken on paper: the numbers are in line with a modern 128-tick design. The candidates for "feels off" are mostly on the feedback side (thin, two-clock hit confirmation; own-shot effects that disagree with the server's result), plus a few structural latency floors and one fog edge case.

### Cited Findings
1. **Own shots never show where they really went.** The predicted tracer runs dead centre to the map; server-side spread is secret and the server's own-Shot event (the true line and end) is thrown away, so with any spread the shooter's tracer and wall impact can disagree with the round the server fired, and a round that hit a body still marks the wall behind it at once — `crates/client-net/src/core.rs:1318-1332`, `crates/client-net/src/core.rs:1633-1634`, `crates/client/src/look/fx/shots.rs:112-117`, `crates/client/src/look/fx/shots.rs:528-537`, `crates/sim/src/world.rs:883-896`. The file's own header says the server's word "draws only what the round met", which the netcode no longer delivers — `crates/client/src/look/fx/shots.rs:3-4`.
2. **No impact effect on an enemy's shield from our own rounds.** Shield sparks at the impact point exist only for other players' shots; for ours there is the hitmarker, a 0.35-gain sound and the shell flare one interpolation delay later — `crates/client/src/look/fx/shots.rs:489-491`, `crates/client/src/look/fx/mod.rs:672-675`, `crates/client/src/look/fx/bodies.rs:184-195`.
3. **Hit confirmation arrives on two clocks.** Hitmarker/sound at snapshot arrival; flare, shield pop and death animation at least 31 ms later (render tick) — `crates/client-net/src/core.rs:1615-1624`, `crates/client-net/src/core.rs:2845-2861`.
4. **The hitmarker carries almost no information.** One white X for shield hit, health hit, headshot and shield break; 0.22 s with no fade; only the kill differs. Halo-style play turns on knowing the instant shields pop — `crates/client/src/hud.rs:45-51`, `crates/client/src/hud.rs:1180-1186`.
5. **Headshots on shields sound like body shots on shields** (part 3 is treated as a shield hit), so there is no audio cue that aim is on the head until shields are gone — `crates/client/src/sound/mod.rs:1250-1257`.
6. **The reticle is static.** No bloom, spread or per-gun shape, while spread is real and hidden; combined with centre-line tracers, a player has no way to see why a centred shot missed — `crates/client/src/hud.rs:883-905`.
7. **Bullet magnetism is off and there is no other aim help**, on a game otherwise built to Halo 2's counts and speeds; the red reticle now means "exactly on the body" — `crates/sim/src/consts.rs:103-107`.
8. **Being shot is quiet on screen.** No flinch (removed 2026-11-09), a gold rim at 0.09 alpha for shield hits, and one small arc whose angle is frozen at the moment of the hit and replaced by the next hit — `crates/client/src/look/shake.rs:9-13`, `crates/client/src/hud.rs:59-68`, `crates/client/src/hud.rs:1187-1196`, `crates/client/src/hud.rs:1494-1511`.
9. **Damage is felt before its cause is seen or heard.** Rim/arc/hit sound are immediate; the attacker's tracer, flash and gunshot wait for the render tick — `crates/client-net/src/core.rs:1640-1648`.
10. **Structural latency floors.** 4-tick (31.25 ms) minimum interpolation delay and a 1 to 2.5 frame input buffer are fixed costs in every peek and every hit confirm, and the delay comes down at only 1 tick per second after a jitter spike — `crates/client-net/src/core.rs:1421-1428`, `crates/client-net/src/core.rs:325-329`, `crates/client-net/src/core.rs:2754-2763`.
11. **Snapshots are processed once per render frame**, so on a 60 Hz display server news waits up to 16.7 ms, and in a browser the arrival stamps that feed the jitter estimate are taken on the busy main thread — `crates/client-net/src/web.rs:343`, `crates/client-net/src/web.rs:403-416`.
12. **Fog's look-ahead for the viewer is one-way latency, but the viewer's screen is a full round trip plus the input buffer ahead of the server's copy of them**; the 0.4 m lean covers the difference only up to roughly 110 to 145 ms RTT (inferred, section 3) — `crates/room/src/fog.rs:33-39`, `crates/room/src/fog.rs:55-69`.
13. **Footsteps only exist for fog-sent bodies**, so there is essentially no audio warning of an enemy behind cover — `docs/DESIGN.md:288-289`, `crates/client/src/sound/mod.rs:1534-1576`.
14. **Far enemies on big casual maps update at 32/16 Hz while the playout delay stays 4 ticks**, so a body beyond 120 m is alternately extrapolated up to about 4 ticks and then re-interpolated; a strafing sniper target may jitter (inferred) — `crates/room/src/lib.rs:1358-1386`, `crates/client-net/src/core.rs:2508-2529`, `crates/client-net/src/core.rs:2535-2556`.
15. **Controllers cannot play.** The pad drives menus only: no stick look, no curves, no aim assist — `crates/client/src/pad.rs:1-16`, `crates/client/src/lib.rs:947`.
16. **The kill cam is built but not shown**; death shows a third-person cam on your own ragdoll and a card naming the killer, which does not explain a death the way a killer's-eye replay would — `crates/client-net/src/core.rs:2590-2662`, `crates/client/src/lib.rs:2073`.
17. **Mouse acceleration can leak in on Linux browsers** (unadjusted movement is not requested there) — `crates/client/web/boot.js:12-21`.
18. **Scope transition changes sensitivity continuously** (sens ÷ eased magnification over roughly 100 to 200 ms), and scoped sensitivity is a straight 1/zoom with no setting; muscle memory for 2×/5×/10× cannot be tuned — `crates/client/src/lib.rs:99-101`, `crates/client/src/lib.rs:1018-1021`.

### Inferences
- If I had to rank by likely contribution to "combat feels off": items 1 to 4 and 8 (feedback that is sparse, late or contradicts the server), then 7 (no magnetism at Halo-like time-to-kill), then 10 to 12 (latency floors and peeks), then desktop render pipelining if confirmed (section 5).
- Items 1 and 2 are cheap to confirm in a private match against a bot at close range: fire at a shielded body in front of a wall and watch where sparks appear.

### Gaps
- None of these was observed in a running build; all are read from code. The audit ran no build, test or capture, by instruction.
- I found no player-facing telemetry (click-to-hitmarker times, rewind depth histograms, interp ticks by platform) that would let the report writer rank these with data.

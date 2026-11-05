# oneshot: scaling, what's next

As of 2026-11-05. Numbers marked "measured" come from a dev box after the
hot-path pass (release build); the Hetzner VM's cores will differ.

## Worlds vs rooms

oneshot scales like a minigame network, not like a survival server: it runs
many small throwaway matches, and none of them share anything.

| | Minecraft survival server | oneshot |
| --- | --- | --- |
| What one process runs | One world that lives for months | Many "rooms", each one match |
| Size of one world | Grows without limit (chunks, entities, redstone) | Fixed: at most 16 players, fixed-size arrays, no allocation per tick |
| How long it lives | Forever | 5–15 minutes, then thrown away |
| What limits it | One main thread running one world | One thread running every room, one after another |
| How you add players | A bigger machine, or split the world | More rooms, on more cores, processes and machines |
| Closest Minecraft analogy | — | A BungeeCord or Velocity network of minigame servers |

The catch is the same in both: today one thread does all the game work. The
difference is that oneshot's work is already cut into independent pieces, so
spreading it out is much easier than splitting a Minecraft world.

## How players get into rooms today

Every player connects to one server process, and that process both matches
them and runs their room (`crates/server/src/lib.rs`, `Sim::run`).

| Mode | How a room is found | Room size |
| --- | --- | --- |
| Against the bot | A new room of your own; bots fill the empty seats | 2 |
| Free for all, Gun Game | A running room with a free seat, else a new one; a party sits together | 8 |
| Big Team | The same, on the side with room for the whole party | 16 |
| Ranked duel, doubles | The queue (`queue.rs`), checked once a second: the longest waiter takes the nearest ratings within ±100 Elo, widening by 5 a second; doubles pairs the best with the worst | 2 or 4 |
| Cup | Swiss heats, each its own ranked room (`cup.rs`) | up to 8 a heat |

- **One sim thread** steps every room 128 times a second, one room after
  another. However many rooms there are, that is one core.
- **The network is already spread out:** each player's connection (QUIC
  encryption, sending snapshots) runs on the tokio worker threads.
- **Hosting:** one Hetzner VM shared with Gates, one region, two slots (a and
  b) so a deploy never cuts a match. A slot takes 256 players before it
  answers "full" (`Config::capacity`).

## What one room costs

One core holds about 15–20 Big Team rooms (250–300 players) with headroom for
spikes; Big Team is the mode that fills a box first.

A tick has 7.8 ms (128 a second). Measured:

| Room | Cost per tick | Rooms one core runs flat out | At half load, leaving headroom |
| --- | --- | --- | --- |
| Big Team, 16 players | about 0.2 ms | about 35 | 15–20 rooms, 250–300 players |
| Free for all, 8 players | about 0.04 ms | about 190 | about 100 rooms |
| Duel, 2 players | a few microseconds | well over 1,000 | not the limit |

- **Where a Big Team tick goes:** seeing (fog of war plus the aim review's
  line-of-sight checks) was about 70% before the pass. Fog's full look is
  still about 44%. The game rules themselves are about 3%.
- **So the per-process ceiling is one core and Big Team.** Smaller modes hit
  the 256-player cap long before the thread fills.
- Measure on the VM with `/health.json` (`tick_p99_us`, `ticks_late`) under a
  load test before trusting these.

## Step 1: rooms in parallel

Stepping rooms side by side on every core is the cheapest big win: a few days
of work for roughly 2–3× the rooms on a 4-core box.

```
Today:  Connections (tokio) ──▶ Sim thread: Room 1 ▶ Room 2 ▶ Room 3 ▶ … Room N   (one core)

Next:   Connections (tokio) ──▶ Sim thread ──▶ Worker pool, one thread a core
                                (matches,       Core 1: rooms A, D, G
                                 seats,         Core 2: rooms B, E, H
                                 hands out)     Core 3: rooms C, F, I
```

- **Why it is safe:** rooms share nothing, and each room is still stepped by
  one thread at a time. The sim stays deterministic, replays still re-run to
  the server's hash, and the native-vs-browser parity check is untouched.
- **Smallest version:** in `Sim::run`, step the rooms on a thread pool (for
  example rayon) each tick. The sessions map is only read while rooms step.
  What crosses rooms (a match ending, cups, the end-screen linger) stays on
  the sim thread, after the step.
- **Bigger version, only if needed:** workers that own their rooms and take
  their players' inputs directly, so no tick waits on the slowest room. Worth
  it only if a load test shows that wait.
- **What it buys:** about one core's worth of rooms per core given, minus what
  Gates and the network threads use on the shared VM.
- **The next limit is the network.** One UDP socket and one QUIC endpoint send
  about 64 snapshots a second per player: 256 players is about 16,000 packets
  a second. Measure that before splitting it.

## Step 2: more processes, machines, regions

Past one box, the ranked queue has to move out of the server first;
everything else already works the way a proxy network does.

1. **More processes on one box.** The two-slot deploy already proves the
   pattern: the page asks `server.json` where to go, and moves when a slot
   says "full". N slots is the same idea with a directory of who has free
   seats (`status.json` and `online.json` already exist per slot).
2. **Move the ranked queue to one place.** Today each process matches only
   the players it holds (`queue.rs`). With two processes, two people who
   should meet can sit in different queues. The design already names the
   home: Elo Pros' `lobbies.py` queue, which writes a match id and a server
   onto each party's lobby (DESIGN §11). Casual modes don't need this: any
   process with a seat will do.
3. **More machines.** The same directory, now across boxes. Replays, reviews
   and the roster already go to shared files; across machines they need
   shared storage instead of one VM's disk.
4. **Regions.** A box per region, and matching that knows each player's ping.
   DESIGN §14 calls this "next"; `lobbies.py` has no ping signal yet.

The Minecraft version of this is BungeeCord/Velocity in front of many backend
servers, with a shared database for anything that must be global (here:
ratings and the queue).

## How big the fog work is

The fog table is medium-sized work (about 1–2 weeks). It is tiny on the arena
maps; only Dry Fork is big, at roughly 16–48 MB raw and a few MB compressed.

**What fog does now.** Every snapshot, for every player and every enemy, the
server casts rays from the player's eye to the enemy's head, chest, feet and
edges, to decide whether to send that enemy at all (`crates/room/src/fog.rs`).
That is what stops wallhacks. A near pair can take up to 70 rays and a far one
18. A hidden pair is the expensive case, because every ray has to fail. At
8v8 that is 128 pairs a snapshot, and it is the top line of a Big Team tick.

**The fix the design already names** (DESIGN §8): a precomputed "potentially
visible set". Cut the map into 2 m cells and store, for each cell, which other
cells can possibly see it. Then a pair whose cells can't see each other skips
its rays.

| Map | Size (m) | 2 m cells | Table, one height layer |
| --- | --- | --- | --- |
| Dry Fork (terrain, vehicles) | 290 × 156 | 11,310 | about 16 MB |
| Proving Grounds | 48 × 48 | 576 | about 41 KB |
| Overpass | 60 × 40 | 600 | about 45 KB |
| The Range (practice) | 60 × 40 | 600 | about 45 KB |
| Cistern | 40 × 40 | 400 | about 20 KB |
| Bulkhead | 48 × 32 | 384 | about 18 KB |
| Chrysalis | 40 × 26 | 260 | about 9 KB |

One bit per pair of cells. A map with floors at several heights (or jumps and
vehicles) needs 2–3 layers, so Dry Fork is 16–48 MB raw. Most of the table is
the same answer over long runs, so it compresses to a few MB.

- **Baking it:** Dry Fork has about 64 million cell pairs. At about 16 sample
  rays a pair and about 0.7 µs a ray (measured), that is roughly 10–15 minutes
  on one core, a few minutes on four. It runs when a map is compiled
  (`tools/maps`), never on the live server.
- **The safety rule:** the table may only skip the expensive "could they see
  each other soon" look. The plain test (is the chest or head in clear line
  right now) always runs. So a table that is slightly wrong means an enemy
  appears a moment late; it can never show anyone through a wall. The leak
  gate (`room/tests/leaks.rs`) stays as it is.
- **The payoff:** the full look is about 44% of a Big Team tick, and on Dry
  Fork 42% of pairs are hidden, which are the costly ones. Expect roughly a
  third or more off a Big Team tick.
- **The work:** the bake tool, a file format and loading it with the map,
  wiring it into `Fog::update`, then running `fog_cost`, the leak gate and a
  check that nobody pops in late.

Sizes are estimates from the maps' bounds; the bake time is from rays
measured on a dev box.

## Recommended order

Do the cheap multiplier first, measure, then spend effort where the
measurement points.

1. **Rooms in parallel** (a few days): the biggest gain for the least change.
2. **A load test** (a few days): many headless clients through `client-net`'s
   native transport, as `crates/server/tests/e2e.rs` already drives one. Run
   it on the real VM, not a dev box.
3. **The fog table** (1–2 weeks): when Big Team and Dry Fork rooms are what
   fill the box.
4. **The shared ranked queue, then more processes** (larger; touches Elo
   Pros): when one box is no longer enough.
5. **Regions and ping-aware matching**: when players come from far enough
   apart to feel it.

**What to watch while doing it** (all exist today):

- `/health.json`: `tick_p50_us`, `tick_p99_us`, `ticks_late`, `ticks_skipped`
  (the sim thread keeping up), `snapshots_dropped` and `cut_behind` (the
  network keeping up), `inputs_dropped` (clients over their rate or the input
  lane full).
- `cargo run --release -p room --example fog_cost`: fog's cost per map at 16
  players.

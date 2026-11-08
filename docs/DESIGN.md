# DESIGN.md: an arena shooter, ranked from day one

Working name to come. A free-to-play, Halo-style shooter on the desktop and in
the browser: 1v1 and 2v2 ranked, cups, an 8v8 Big Team mode on canyon maps
with vehicles, skins as Solana NFTs. This page is the design of record. Netcode comes first because the game is only
worth ranking if the server can't be lied to. What to take from our other
repos is `RIP.md`.

## 1 · The game

| | |
|---|---|
| **Duel** | 1v1 Slayer, first to 15 kills or 10 minutes. Ladder `duel`. |
| **Doubles** | 2v2 Team Slayer, first to 30 or 12 minutes. Ladder `doubles`. |
| **Cups** | FFA heats of 8, points per placement, Swiss rounds, the top eight into a final (`server/src/cup.rs`). Same match server; ladder `cup`. |
| **Casual** | FFA, Gun Game and Capture the Flag (4v4, first to 3 captures or 15 minutes) with bots filling empty seats. A party may take a room of its own, with its own map, score and clock. Unranked. |
| **Big Team** | 8v8 Team Slayer on terrain maps with vehicles. Casual first; a ladder once it plays well. |
| **Siege** (to come) | Team ARAM: everyone the same Spartan, two ability cards at most picked up in the match, generators to break, then the base. Casual (§18). |

Halo's rules, the ones Fireplace's arena already plays:
- Shields over health. Shields recharge 5 s after the last hit; health follows.
- Equal starts: a rifle and a sidearm, two frags.
- Power weapons (sniper, rocket, shotgun, the sword) on pads with timed respawns, so a match is about map control.
- Halo 2's arsenal as archetypes, ours by name and look: a carbine, a beam rifle that runs hot, ion guns that strip shields (a charge shot that homes), homing shards that go off together, a skipping grenade launcher, a heavy ion cannon. Its counts and times are the baseline (`sim/tests/range.rs`); tuning makes them our own.
- Dual wielding, as Halo 2's: a one-handed gun (magnum, SMG, the ion guns, the shard gun) taken beside another goes in the left hand, on its own trigger; no grenades while dual, a melee or a swap drops the left, both reload together and slower.
- Two grenades, as Halo's: frags, and stickies that stick to whatever they touch (a body, a hull, the map) and kill whoever wears one; a switch picks the next throw, and a kind run out goes over to the other. Everyone starts with two frags and no stickies; both lie on pads about the map and fall from the dead, four of each at most. A blast sets off the grenades in its reach, those lying on the floor too.
- Power-ups on pads, taken by walking over them: an overshield and active camo.
- Medals as Halo's (multi-kills, sprees, assists and the rest), read from the world's events after each tick so a replay gives the same ones (`room/src/medals.rs`); the announcer calls them, and a carnage report ends the match.
- The Range: a practice map with every gun on a pad, out of the rotation.
- Melee, a head-shot rule per gun, respawn after 3 s at the spawn farthest from enemies.
- No motion tracker in ranked, as Halo's ranked playlists have it. That also removes a radar leak (§8).

**Ranked plays MLG's way** (Duel, Doubles, cups):
- No motion tracker and no minimap, in any mode. What you know is what you see and hear.
- Equal starts, every life: the rifle (BR) and the magnum, two grenades. Nothing random on the map: power weapons sit on fixed pads with fixed timers per map.
- Friendly fire on in Doubles (`Rules::friendly_fire`): your rounds, blasts and blows hurt your teammate, and a teammate killed costs your side a point. Nobody homes on, lunges at or is magnetised toward a teammate. Casual and Big Team keep it off.
- A suicide costs a point. No vehicles on ranked maps.
- Sides read as red and blue; a skin is trim on the side's colour, never in place of it.

Free to play. Skins are cosmetic only: money never changes a measured result,
whether a kill, a rating or a placement (Elo Pros' invariant 1).

Abilities live only in Siege and its Mayhem test (§18); Duel, Doubles and cups
never have them, and Big Team's own playlist keeps none. Not yet: hidden-information modes beyond fog of war.
Vehicles and terrain play in Big Team (§16, milestones 9 and 10), to rules
our own: one ground vehicle, the truck, a driver and a passenger; two of Dry
Fork's four carry a minigun their passenger guns. Maps and vehicles
are original; never another game's layout or art.

## 2 · Architecture

```
       desktop or browser                             server (native)
 ┌──────────────────────────────┐            ┌──────────────────────────────┐
 │ client  Bevy: world, HUD,    │            │ server  rooms, auth, fog,    │
 │   ▲ draws       audio        │ WebTransport│         lag comp, telemetry │
 │ client-net (native or wasm)  │◄──────────►│   ▲                          │
 │   session · clock · interp   │ datagrams + │ sim (native)  the truth     │
 │   predict ── sim             │ one stream  │                              │
 └──────────────────────────────┘            └──────────────────────────────┘
```

| Crate / dir | What | Notes |
|---|---|---|
| `sim` | The game's rules: movement, guns, shields, pads, modes, rewind | Deterministic. Built native for the server and wasm for the browser's prediction. No I/O, clock, threads, `HashMap` iteration or allocation in the tick. Floats are `+ − × ÷ sqrt min max`; trig, exp, pow and rounding go through `sim::fmath` (pure-Rust libm, same bits native and wasm). |
| `proto` | Wire format: bit packing, inputs, snapshots, events, handshake | `PROTO_VER`, byte-exact goldens. Depends on nothing game-specific beyond small state structs. |
| `room` | A match with no transport: seats, input buffers, rewind, fog of war, snapshots and deltas, watching, replays, the aim review | What every byte to a client goes through. |
| `server` | Sessions, transport, sign-in, the queue, cups, parties, telemetry, reports | Native, wtransport over quinn; rooms step side by side on a pool. |
| `client-net` | Session (handshake), clock, prediction, reconciliation, interpolation | `core` and `session` build everywhere; the transport is wtransport natively and WebTransport in wasm, behind one `Wire` trait. |
| `client` | Bevy renderer: terrain, sky, lighting, viewmodel, HUD, menus | Milestone 12. Native, and the same crate built for wasm is the page: two modules, WebGL2 and WebGPU, one picked before either downloads. Draws; never decides. |
| `mix`, `mix-worklet` | The game's sound mixer, no engine in it | A Bevy audio source on the desktop, an AudioWorklet in the browser. |
| `fx` | Effects with no engine: particle pool, recipes, knobs | Vendored from Gates. |
| `tools/maps` | Map compiler | JS, from Fireplace (`RIP.md`). |
| `tools/load` | `oneshot-load`, the load test | Headless players on `client-net`'s native transport. |

Why Bevy now: the look we want (sky, clouds, terrain, grass, real lighting)
is what Gates' Bevy renderer already draws, and a desktop build comes free.
Gates' renderer is a donor, not a dependency: we port modules (`RIP.md` §9).
The cost is the browser download: Gates' wasm is 8.57 MB gzipped with a heap
around 700 MB (`Gates/docs/NOW.md` §0web). Ours is two modules (its own
`web` profile, no name section): WebGPU draws the desktop's frame, WebGL2
drops the atmosphere, SSAO, bloom and TAA. `ci/client.sh` prints their
gzipped sizes; no size gates a build.

**The rule that keeps it honest: the client draws, the server decides.**
Gameplay state lives in `sim`. The client reads from `client-net` and never
feeds anything back but inputs.

## 3 · Tick and timing

| | |
|---|---|
| Sim tick | **128 Hz** (7.8 ms) on the server and in the prediction. Valorant runs 128. |
| Snapshots | **64 Hz**, every 2nd tick, per client. |
| Inputs | Sampled every tick, sent at 64 Hz, each datagram resending every unacked frame (at most 16). |

Every duration (refire, reload, respawn, shield delay, rewind window) is
written in seconds and turned into ticks from `TICK_HZ` in one place. So 60/60
is one constant if hosting cost ever demands it. Fireplace hard-codes its 125
Hz in about 88 files; don't repeat that.

Cost: an arena tick with 8 fighters measured 0.04 to 0.15 ms in Fireplace
(`Fireplace/docs/arena.md`). A 2v2 room at 128 Hz is noise. One process holds
hundreds of rooms.

## 4 · Authority: what the server owns

The server owns every truth: positions, health, shields, ammo, refire,
reloads, pads, score, the clock. A client sends only what its player pressed,
one frame per tick:

```
input frame:  buttons u11 · yaw u16 · pitch i16 · move_x i8 · move_z i8
input datagram: newest snapshot tick u32 · playout ticks u8 · first seq u32 · count u5 · frames…
```

- **The server steps on its own clock.** One frame is consumed per tick per
  player. A missing frame repeats the last one with tap buttons cleared, then
  decays movement to zero (Gates' `input::decay_frame`). A client sending
  faster gains nothing: the buffer is capped (16) and a second frame in a tick
  costs catch-up credit, so a speedhack or a lag switch buys at most about 3%
  (Gates' `INPUT_CATCHUP_CREDIT_CAP`, `server/src/client.rs` `consume_input`).
- **Inputs are sanitized.** Only known buttons fit the wire, and the server
  clears any other (`Input::sanitized`). Pitch is
  clamped to ±89°. The stick is clamped to unit length. Yaw is free (an
  aimbot's turn rate is evidence, §13, not a refusal).
- **Fire rate, ammo, reloads, swaps, melee and grenades** are gated by sim
  timers. A client can ask to fire every tick; the gun fires when the sim says.
- **Damage happens only on the server.** The client never reports a hit.

## 5 · Prediction and reconciliation

The client runs the same `sim` crate for **its own body and its own weapon
state only**: movement, jumps, refire, magazine, reload and swap timers. A
driver also predicts **the vehicle it drives** (`Vehicle::step_local`: the
vehicle, the driver's input and the map, nothing else); a gunner predicts its
turret's aim. Vehicle-on-vehicle hits, splatter and getting in or out are the
world's, and reach the client by reconciliation. Everything else is the
server's.

- **Prediction ring:** each input is stepped immediately, and its result is
  stored by `seq` in a ring of 64 (half a second at 128 Hz). This is Gates'
  `client-core/src/predict.rs`.
- **Reconcile on every snapshot:** compare the server's body at
  `last_executed_seq` with the ring. Equal → confirmed. Different → adopt the
  server's, replay the unacked tail, and put the jump into a render-only offset
  that decays (100 to 325 ms, blended by size), snapping past 4 m.
- **Clock:** the client runs ahead of the server by its one-way latency plus a
  small buffer, steered by the low point of the server's input buffer (a band
  of 1 to 2.5 inputs): short of it 5% fast (the server is guessing, which
  costs corrections), past it slower in proportion, 1% to 5% (only latency
  is lost, so it eases down rather than swing across), hard resync if far
  off. Gates' `client-core/src/clock.rs`.
- **Own shots are drawn now:** tracer, muzzle flash, recoil kick, ammo count,
  reload. Their direction is the centre of the reticle (spread is the server's
  secret, §9). Hit markers, damage numbers and kills arrive from the server, a
  round trip later.
- **Quantize both sides.** Positions and velocities are quantized inside the
  sim step, so the server's body and the predicted body are bit-equal when
  nothing went wrong. Confirmations are counted; corrections are a health
  metric (§17).

## 6 · Other players: interpolation

Remote fighters are drawn **between** two snapshots, never guessed ahead:

- The render tick runs on real time from an estimate of the server's clock
  that each snapshot moves a tenth of the way to what it says; what is drawn
  follows the estimate at 90 to 110% of real time, faster the further off
  (to half again: a latency step is caught up in a fraction of a second), so
  a late snapshot slows everyone else a little and never stops or pulls them
  back. Snapshots over 150 ms early re-take the estimate at once (that only
  moves everyone on); late ones never do. The client reports how far behind
  its newest snapshot it draws, as measured (§7).
- A playout delay starts at 2 snapshot intervals (31 ms at 64 Hz) and adapts
  from measured jitter (RFC 3550 estimator) within 1 to 6 intervals, slewed
  (up 4 ticks a second, down 1). The client reports its delay to the server
  in each input datagram (it feeds lag compensation, §7).
- Linear interpolation of position, slerp of yaw and pitch, and animation state
  (moving, airborne, firing, reloading, dead) from the snapshot flags.
- On a gap: extrapolate at most 125 ms from velocity, then hold. Two
  snapshots more than 250 ms apart aren't drawn between: run on and held as
  if the second hadn't come, then where they are.
- This is Gates' `client-core/src/interp.rs` and its `server_est`, retuned
  from 30 Hz; `client-net/tests/loopback.rs` holds it to even steps through
  25 ms of jitter.

Rounds in flight share that render tick: a remote's rocket is drawn from the
same snapshots, at the same tick, as the remote who fired it, so it leaves
his drawn hands and never leads or lags him; run on and held by the same
gap rule, never drawn before it was first sent, kept by an id the client
gives it (the wire has none). Events are played on that tick too (a
remote's shot, a Blast, a pickup are held until the render tick reaches the
snapshot that brought them, never long: `HOLD_MAX_MS`), so a rocket goes the frame
its Blast plays, and a hull its Wreck. Hits, kills and our own shots stay
immediate, and the wreck of the hull we ride. Our own round
is drawn from our predicted muzzle on the frame we fire, flown as the sim
flies it, then glides onto the render tick (its clock runs a little slow
until it gets there, `GLIDE`); when the server's copy comes it takes
it over without a pop, and one the server never made is held where it is
drawn until the render tick reaches the snapshot that said so. A passenger
predicts no hull, so its rounds are drawn as anyone's. A stuck sticky rides
its host as drawn, and one flying onto its host lands on it as drawn. The kill cam
draws no rounds. `client-net/src/proj.rs`, `NetCore::projectiles`;
`client-net/tests/projectiles.rs` and `loopback.rs` hold it.

## 7 · Lag compensation: the server chooses

A hitscan shot is checked against where the shooter saw the target. **The
server decides how far back that was. The client never says.** (Fireplace's
arena trusts a client-claimed lag of up to 248 ms on every shot; this is the
fix.)

- **Rewind ring:** every fighter's pose (feet position, crouch) and every
  vehicle's (position, yaw) for the last 32 ticks (250 ms), stamped per tick, in the sim (Gates'
  `sim-core/src/rewind.rs`, extended to interpolate between two ticks so
  rewind isn't stuck to whole ticks).
- **Depth** for a shot fired at server tick `T` by player `p`:
  `depth = (T − tick of the newest snapshot p acked) + p's playout delay`,
  where the acked snapshot is the fresher of what `p` claims and what the
  server saw arrive, and the playout delay is clamped by measured RTT. This is
  Gates' `server/src/stats.rs` `favour_for`, with its cross-check against
  `newest_acked`.
- **Caps:** at most 160 ms of favour. A shooter above 200 ms RTT gets no more
  favour than 200 ms would earn; they lead their shots. A target can't be shot
  behind cover they reached more than 160 ms ago.
- **Hitboxes:** an upright body cylinder and a head sphere per fighter (the
  shapes Fireplace's `$a_ray` uses), crouch lowering both. Head-shot rules are
  per gun.
- **What is rewound:** hitscan bullets and melee. Rockets, frags and any
  slow projectile fly in the present, as everyone sees them.
- **The favour is deliberate and bounded.** The shooter gets what they saw up
  to 160 ms; the victim's worst case ("shot after reaching cover") is bounded
  by the same number. Both are tuned from real matches, not guessed.

## 8 · Fog of war: what a client may know

A client is sent an enemy only when that enemy could be on its screen soon.
Wallhacks and ESP then have nothing to read. Valorant does the same.

1. **Rays, every snapshot** (`crates/room/src/fog.rs`, built in milestone
   3). A baked potentially-visible set (2 m cells, a bitset of the cells
   each sees) is the optimisation for when maps and rooms grow; at 2v2 the
   rays below cost a few thousand box tests a snapshot. At 8v8 they were
   not cheap: 0.6 to 1.9 ms a snapshot on our three maps (`cargo run
   --release -p room --example fog_cost`). Two things brought that to
   0.1 to 0.3 ms, under 2% of a core per room:
   - **A box grid** (`sim/src/grid.rs`, 8 m cells): a ray tests only the
     boxes in the cells it crosses and stops at the first hit. It answers
     bit for bit as the full scan did.
   - **Half the room a snapshot:** each enemy gets the full look every
     other snapshot, with the look ahead stretched by the snapshot it
     skips; between, the plain test (chest or head in clear line). So
     nobody in plain sight is ever held back.

   Terrain made it dear again: on Dry Fork, 140 m long and hilly, 42% of
   pairs are hidden and the mean line is 74 m (0.7 ms a snapshot; measured
   when Dry Fork was 140 m long, it is now 290). Three
   more brought it to 0.3 ms:
   - **Far pairs** (past 40 m) get the full look every fourth snapshot,
     without the leans and the chest's edges: 18 lines, not 70. At 40 m
     those turn a line by half a degree.
   - **The ground's proof:** a group of lines (one eye, the target now or
     ahead) is dropped when the ground stands above a line over all of
     them by more than it can rise across their width.
   - **Terrain in 16 m blocks** with their highest point, so a line clear
     above a block skips its cells.

   A pair found hidden still gets the plain test. `fog_cost` measures fog
   at sixteen seats on every map; nothing gates on it.
2. **The rays:** for each enemy, cast rays from
   the viewer's eye against the map, through the sim's own ray query:
   - the viewer's eye pushed ahead along its velocity by its one-way latency +
     a snapshot interval + its playout delay, and that eye moved ±0.4 m to each
     side (a peek around a corner);
   - to the target's head, chest, feet, and left and right edges, each pushed
     ahead by the target's velocity over the same time.

   Any clear ray makes the target visible.
3. **Hysteresis:** a target stays visible for 12 ticks (94 ms) after the last
   clear ray, so a peek never flickers or pops.
4. **Always sent:** teammates; every fighter's name, kills and deaths for
   the scoreboard (never their position). The kill cam replays only what
   this client was already sent.
5. **Sound is a deliberate leak**, bounded:
   - a gunshot is sent with a position quantized to 2 m within 60 m (unheard
     past it);
   - footsteps never cross the wire: only a fighter fog sends is heard
     walking;
   - a thrown grenade if the thrower is visible or within 30 m; its explosion
     everywhere it can hurt you.
6. **Damage direction** comes with each hit on you, as an arrow, not a
   position.
7. **Projectiles** are filtered like fighters. A rocket from a hidden shooter
   appears when it becomes visible.

The fog code knows the exact tick each enemy became visible to each client.
§13 uses that.

What's left: an enemy is sent up to about a latency's worth before they are
truly on screen. A cheat can show that sliver. That's the price of no pop,
and it's the same one Valorant pays.

## 9 · Hidden randomness

Bullet spread and every random roll come from a keyed hash
(`sip24(salt, match, player, shot, pellet)`, Gates' `sim-core/src/rng.rs`
pattern) with a per-match **salt that never leaves the server** while the match
runs. A no-spread cheat has nothing to cancel.

- At match start the server publishes `sha256(salt)` in the welcome.
- At match end it reveals the salt. Anyone with the replay can check every shot.
- The client's predicted tracers ignore spread (§5). The server's shot is the
  real one, and its tracer arrives with the snapshot.

## 10 · The wire

| Channel | Carries |
|---|---|
| Datagrams, client → server | inputs (§4) |
| Datagrams, server → client | snapshots, at most 1,100 B each, newest wins |
| One bidirectional stream | handshake, sign-in, seats and their looks, match start and end, the kill feed, medals, cups, the salt reveal |

- **WebTransport only** (Chrome, Edge, Firefox; check Safari at launch). A
  WebSocket fallback is not planned for v1.
- **Bit packing:** Gates' `protocol/src/bits.rs` (LSB-first, fixed caller
  buffers, no allocation, no panics, typed errors).
- **Acks:** every input datagram names the newest snapshot tick the client
  has (u32): enough for deltas and events.
- **Drop-oldest:** a snapshot replaces the one queued before it; a datagram
  over the live MTU is dropped and counted, never fragmented (Gates' `send_snap`,
  browser `WebWire::send_datagram`).
- **A snapshot, per client:**
  - tick, last executed input seq, input buffer depth, time-dilation nudge;
  - the client's own full state (body, health, shields, guns, magazines,
    timers): what reconciliation needs;
  - each visible fighter, about 16 B: position (1/64 m), velocity, yaw and
    pitch, state flags (moving, airborne, crouched, firing, reloading, dead,
    shield flare), held gun (looks go once, in `Seat`);
  - visible projectiles, events since the last acked snapshot (shots heard,
    hits on you, kills, pickups), pad timers, score.

  About 200 B at 2v2, about 13 KB/s down, sent whole. Each snapshot goes as
  a delta when that is smaller (`proto::delta`; `--no-delta` turns it off):
  record by record (own fighter, each fighter and vehicle by slot, pads,
  flags, score, rounds, events), XOR'd against the newest snapshot the
  client acked within 64 ticks; both ends keep that window, and a base
  either lacks means a whole one. Big Team went from 195 to 91 kbps a
  player. A snapshot carries at most 16 rounds, the client's own and the
  nearest closing ones first. Events are cleared when a snapshot carrying
  them is acked, not when it is sent.
- **Big maps send less:** a fighter past 60 m is updated every other
  snapshot, past 120 m every fourth (`room::update_every`;
  `--no-far-rates`), and `--big-snapshot-hz 32` halves a big map's rate.
- **Goldens:** every message has a seeded fixture encoded byte-exact in CI.
  A wire change bumps `PROTO_VER` and regenerates them.

## 11 · Identity, queue and ratings

- **Sign-in:** the Elo Pros pass ("Sign in with Elo Pros"). The page gets a
  10-minute pass bound to game, server and nonce from
  `POST /api/account/play`; the server verifies it offline against
  `/api/account/pubkey` (Gates' `server/src/auth.rs` `verify_pass`). It covers
  EVM and Solana wallets and Elo logins.
- **Ranked needs a wallet.** `ratings.py` refuses login-only `acct_` ids. A
  login plays casual.
- **One seat per wallet per match.**
- **Parties, friends, voice** (`server/src/party.rs`, `lobby.rs`; the
  client's `party.rs`, PARTY and FRIENDS on the title): the game keeps a
  lobby session (`MODE_LOBBY`) beside any match. Everyone is in a
  party, of one until they join someone's by its code; the leader picks who
  may join (anyone with the link, the leader's Elo Pros friends, invite only)
  and starts a mode for everyone present, who are then seated together (one
  side in Big Team, one unit in Doubles, a room of their own if they ask).
  Friends are Elo Pros' list (`GET /api/friends/<player>`), with where they
  are here; `online.json` gives Elo Pros the presence back. Party voice is
  Opus over the lobby's datagrams (Gates' layout), relayed to the party and
  nobody else, never decoded by the server. A page back from a match comes
  in on its ticket (`auth.rs`) and takes its place.
- **Queue:** the server's own (`server/src/queue.rs`), by Elo Pros' rule: a
  window of 100 that grows 5 a second, sides balanced. With `--elo-queue`,
  Elo Pros' `lobbies.py` queue, shared by every server in the game's list
  (`server/src/eloqueue.rs`): it puts each match on a server with room, the
  server reads `GET /lobbies/match/{id}` for who goes where and seats only
  those wallets, and a player matched elsewhere is moved there
  (`Msg::Moved`).
- **Server rows:** the shard-list row and `status.json` the store reads, from
  engine-shim's `elo.rs`.
- **Ratings:** at match end the server posts `POST /ratings/<slug>/report`,
  signed with its server key (`serverkeys.py`: Ed25519, nonce, 300 s skew), on
  ladders `duel`, `doubles` and `cup`. A report that can't get through is
  saved and resent (`oneshot-review resend`); a drain waits out the
  retries. The math is Weng-Lin Plackett-Luce shown on the
  Elo scale; a new player is 1500 ± 246 and appears on the board after 5
  matches. The player sees the exact rating ± rd after every match: no hidden
  number beside it, no update held back for five wins.
- **The board orders by the floor**, mu − 3 sigma on the Elo scale, as Halo 3
  ranked by TrueSkill's. Five lucky wins on a wide sigma don't top a record.
  Sigma widens on reads while a player sits idle (back to a new player's
  within 90 days), so nobody keeps a spot by not playing. The floor is also
  what a rank-locked skin reads (§15).
- **Seasons are windows over the log, never a reset** (`GET
  /ratings/oneshot/season/<ladder>?from=&to=&min=`): the ratings at the
  window's end, by floor, among wallets with at least 50 rated matches inside
  it. A row whose most-met opponent is half or more of its 10 or more matches
  is marked for review (win-trading between two wallets).

## 12 · Replays and evidence

The sim is deterministic, so a match is its seed, its salt, its map and its
input log. The server writes that for every match. It gives us:

- watching a match: a watcher follows any seat and sees exactly what that
  seat was sent, 10 s late (30 s ranked; `room/src/spectate.rs`);
- review of any match with fog off (§13);
- **evidence on the rating:** the report carries
  `evidence: {replay_sha256, salt_commit}`, which Elo Pros keeps in its
  hash-chained match log (`ratings.py`). That's the store's `replay` tier.
  An Elo Pros that refuses the key gets the result without it.

CI checks the same input log hashes equal native and wasm (§17).

## 13 · Catching what a browser can't stop

Authority (§4), fog (§8) and the salt (§9) remove speedhacks, teleports,
rapid fire, wallhacks, ESP and no-spread. What remains is aim: an aimbot
or triggerbot in the browser. That's caught by evidence, not prevented.

**Server telemetry** (`room/src/watch.rs`), every tick, written beside the
replay as `<id>.watch.json`. A match is flagged when any line is crossed:
- head-shot rate: 60% or more over at least 20 hitscan hits, shields or not;
- snap-hits: a sighting's first hit after more than 30° of turn in 100 ms,
  on half or more of the sightings (at least 5);
- **reaction time from the tick the target came into plain sight**, which
  only a fog-of-war server knows exactly: a median under 150 ms over at
  least 5 sightings;
- pre-aim: over 3 s aimed within 2° of an enemy that fog sent early but that
  has been out of plain sight for 250 ms, which is a wallhack using what fog
  lets through.

`room/tests/watch.rs` holds the lines to a scripted aimbot (all four
crossed) and the bots (none).

**What happens:**
- Nothing automatic. A flagged match is copied to `<replays>/review/`;
  `oneshot-review list | show | done` works through it.
- A per-wallet ban list on Elo Pros (`meter/gamebans.py`), written by
  `oneshot-review ban | lift` signed with the server key, read at ranked
  sign-in (cached a minute; a list that can't be read refuses nobody).
  A ban is the game's call, made on a reviewed replay. Casual stays open.
- **Prizes need a record.** Cup prize eligibility needs a history (for example
  50 or more ranked matches in good standing) and the top N's replays reviewed
  with fog off before anything is paid. Wallets are free, so a ban alone
  doesn't stop a cheater coming back; a record and a review do.

**What comes next** is `ANTICHEAT.md` (operator, 2026-11-07): a profile per
wallet before any payout, read three ways (against the player's own past,
against the rest of their game, against everyone); our own aimbots, bred and
laid over real replays, as the detector's training data; and the detector
opened with the neuron tools. Its sources are `ANTICHEAT-SOURCES.md`.

## 14 · Hosting

- One server process, many rooms (a room is a sim plus its clients; a 2v2 tick
  costs microseconds), on the game box beside Gates (`game.elopros.com`), run
  by systemd. The page is on elopros.com (`/games/oneshot/play/`) and finds
  the game box through its `origin.json`; the box's nginx fronts
  `server.json`, `/nonce` and `/ready` on 443. `ops/README.md` is the runbook.
- **Always a server taking players:** two slots (`ops/oneshot@.service`), UDP
  61250 and 61251. A deploy starts the new build in the idle slot, names it
  in the shared `live` file (every `server.json` says it, whichever slot
  answers), then drains the old one. Two ports, not one socket shared with
  SO_REUSEPORT: adding a socket re-hashes live QUIC connections onto the new
  process. A page refused "full" by the old slot asks `server.json` again and
  moves with the same pass: both slots sign nonces with one key
  (`--nonce-key`, `auth::Nonces`), and each takes a nonce once.
- Certificates from ACME (Gates' `ops/certbot-deploy-hook.sh` model), so
  production needs no certificate-hash pinning: with a real certificate
  `server.json` sends no hash and the browser checks it against its roots.
  Dev uses a self-signed 14-day certificate pinned by hash. A renewal
  **reloads** (SIGHUP, `Endpoint::reload_config`): new handshakes get the new
  certificate, nobody in a match is dropped.
- A stop **drains** (SIGTERM): a hello is answered `REFUSE_FULL`, the queue is
  let go, every running match plays to its end and its replay and report go
  out, then the process exits; `--drain-max` caps it. Gates restarts on both;
  a ranked match can't be cut short, and with two slots nobody waits on it.
- QUIC admission with Retry past 2× capacity and refusal past 4× (Gates'
  `spawn_shard`), sixteen connections per address (an IPv6 /64; a player holds two) and 64 per
  prefix (/24, /48), loopback exempt; past capacity a hello is answered
  `REFUSE_FULL`. The UDP socket asks for 8 MiB buffers (Gates' `bind_udp`).
  The wtransport commit stays pinned (Gates and engine-shim pin the same rev).
- One region at launch. Regions and ping-aware matching come next; `lobbies.py`
  has no ping signal yet.
- A server behind NAT can register with engine-shim's identity relay
  (`relay.rs`); not needed for our own hosts.

## 15 · Skins and money (short)

What Halo, Valorant and Overwatch taught: gameplay in random packs (Halo 5's
REQ), heroes behind a pass (Overwatch 2, reversed), a rank hidden behind
batched updates (Overwatch 2, reversed), passes that expire, and a shop with
no path through play (Overwatch 2 after the boxes) cost more goodwill than
they earned. Reach's earned armory, Overwatch's golden guns and Infinite's
passes that never expire are what players kept.

- Weapon and character skins are Metaplex Core NFTs on Solana, one collection
  for the game, 0 royalty, the house paying rent (Elo Pros'
  `docs/items/SOLANA-ITEMS.md`, `meter/solana_items.py`, `deploy/solana-items/`).
  Sold directly, priced in PROS, and they trade freely.
- **Rank-locked skins.** A skin may carry a line (`proto::SKIN_FLOOR`): a
  wallet wears it only while its best floor on Elo Pros (§11) is at the line.
  Owning one is a market; wearing one is a record. Below the line a wallet
  wears the standard look, and can sell the skin to someone above it. Money
  buys the item, never the right to show it, so a bought skin still says what
  its wearer measured.
- The server checks a player's skins against their wallet, and a locked one
  against their floor, at sign-in, and only then names it in the seat's
  `Seat` message. A client can't claim one it doesn't own or hasn't earned.
- **Gear**, a hat and boots, is checked the way a skin is. The title's ARMORY
  shows what the wallet owns, what is locked and its floor
  (`Msg::Wardrobe`).
- **Emblems**, Halo's way: a glyph on a shape, each in one of 16 colours,
  painted on Triple T's back (`proto::Emblem`, drawn by `look/emblem.rs`, the
  menu's preview the same pixels). Ours, drawn from geometry: no uploads.
  Anyone wears one, a guest too; a few glyphs are rank-locked
  (`proto::GLYPH_FLOOR`) the way skins are. Never lit, never on the head.
- The free Pill skin after 10 minutes of play. Everything in the store has a
  path through play too. A pass, if there is one, never expires.
- **No cases at launch.** If they come, cosmetic only, odds above the pull,
  no duplicates.
- **No coin per match.** Wallets are free and casual seats fill with bots, so
  a per-match payout is a printer for alts. Money goes out on placements
  reviewed by a person: cups and each season's top (§11, §13), with the
  record §13 asks for; and, once the game's coin is out, a **daily pot**: a
  fixed sum a day split among the players who finished that day's
  challenges, capped per wallet, its file read by a person before it pays,
  and gated (people only, then ranked, then placed) as players come
  (operator, 2026-11-07; Elo Pros' `docs/launch/ONESHOT-COIN.md` §9).
- Never: a gun, a character or anything that changes play behind a price or
  an unlock. Skins never change a hitbox or a sound's loudness, and never
  hide a fighter. A skin may glow: that only shows its wearer more (operator,
  2026-11-08).

## 16 · Milestones

1. **`sim`:** movement, guns, shields, pads, Slayer. Native and wasm hash
   equal over a recorded bot match.
2. **One playable room:** `server`, `client-net` and `web/` with Triple T and
   the guns. A 1v1 against a server-side bot; prediction, reconciliation and
   interpolation in place.
3. **Competitive core:** server-chosen lag compensation, fog of war, the salt,
   and the leak gate green.
4. **Ranked:** sign-in, queue, ratings with evidence, replays and kill cam.
5. **Review:** telemetry, the review queue, bans.
6. **Doubles, three maps, skins,** then cups. Also: casual free for all and
   Gun Game, the kill cam, the impaired-network gate. Done; cups too
   (`server/src/cup.rs`), and Capture the Flag.
7. **Hosting:** the game box and elopros.com, an unpinned CA certificate
   reloaded on renewal, admission, a drain that lets matches finish, two
   slots so a deploy never leaves nobody taking players, the deploy and
   publish scripts (`ops/`). Done (`ops/README.md`).

From here the game grows into a full shooter. The sim comes first; each step
keeps every gate in §17 green and moves a golden only on purpose.

7. **Entity pools:** fighters, projectiles and pads behind one `Thing` shape,
   so a new kind is a short list the compiler checks. No behaviour change;
   the golden does not move. Done (`sim/src/thing.rs`, `pool.rs`).
8. **16 seats and Big Team:** 8v8 Team Slayer, casual. Done.
9. **Terrain:** a deterministic heightfield beside the boxes, slopes and
   sliding, a box grid for rays; an original canyon map with its own golden.
   Done: Dry Fork (`maps/canyon/`), 290 × 156 m, a gulch to the sea, Big Team's map (a map of
   4,000 m² or more is a Big Team map; the rest are arenas).
   **Launchers** too: a pad in the map that throws a fighter along a set
   velocity. It reads only the fighter and the map, so it predicts exactly
   and costs no wire. Skiing (low-friction slides down slopes) is a later
   experiment.
10. **Vehicles:** one ground vehicle, a driver and a passenger who shoots,
    rewound for hitscan, splatter. Done (`sim/src/vehicle.rs`): it turns
    about y only and meets boxes as three hull circles; USE gets in and out
    (a vehicle in reach beats a pad); hitscan, melee, rockets and blasts
    hurt it, and it blows up with its riders and comes back after its
    respawn; past 11 m/s it runs down whoever is on foot in its way. Two a
    side on Dry Fork. Bots drive, back up when stuck, and get out to fight.
    Fog shows a vehicle as it shows a fighter (held, the full look every
    other snapshot, the plain test between) and names a rider only to a
    client that may know them. A minigun turret on two of Dry Fork's four
    trucks, gunned by the passenger standing in its ring, predicted by the
    gunner.
11. **The delta wire:** snapshots as deltas against the acked baseline, a
    byte budget, events cleared on ack. Events first (done): each event a
    client may be told gets an id and rides every snapshot until one that
    carried it is acked; what doesn't fit waits; the client keeps the first
    and drops repeats (`room::EventQueue`). Every kill gets through 10% loss
    once (`client-net/tests/loopback.rs`). Deltas done (`proto::delta`,
    `room/tests/delta.rs`): each record XOR'd against the newest acked
    snapshot within 64 ticks, sent only when smaller; Big Team 195 → 91
    kbps a player. A snapshot still always fits one datagram (at most 16
    rounds, ranked for its client: `room/tests/budget.rs`), so none is split
    across datagrams.
12. **The Bevy client:** a session layer in `client-net` and a native
    transport (done: `client_net::session::Session` over a `Wire`, the
    browser's WebTransport in `web.rs`, wtransport in `native.rs`; the
    server's end-to-end test plays through it), then the client (C1 done:
    `crates/client`, Bevy 0.18, its own workspace; the level from the
    sim's map, fighters and vehicles as plain shapes, first-person and
    chase cameras, the controls, a text HUD; native on wtransport, and in
    the browser on WebGL2, where it joins a Big Team match on Dry Fork): a playable skeleton, Gates' lighting, sky,
    terrain and grass, the game's feel, and last its wasm build replacing
    `web/`. Done: the page has the three.js page's menu (name, skin, Elo
    Pros sign-in, every mode, the queue's wait, the move to the live slot
    when a server drains, back to the menu at the end), its controls (the
    same keys, taps that land inside a frame, the wheel, `N`'s net graph),
    skins and its `?pose` for the look board. `ci/e2e.mjs` plays it, the
    server serves it, `ci/publish_web.sh` publishes it, and `web/` (three.js)
    is gone. Since: the page as two modules (WebGL2 and WebGPU); the title
    (PLAY, ARMORY, PARTY, FRIENDS, CHALLENGES), the Esc menu and settings, a pad on
    them; our own mixer (`mix`, on the audio thread in the browser); the
    kill cam, medals and the carnage report.
13. **Siege:** ability cards, the generators and the base, the caches, in
    the phases of §18, each kept only if it plays.

## 17 · What CI blocks a merge on

| Gate | Fails when |
|---|---|
| Determinism | a recorded match hashes differently native vs wasm, or a golden moves without a rule change |
| Zero allocation | the sim tick allocates after warmup |
| Goldens | any wire message's bytes change without a `PROTO_VER` bump |
| Prediction | the predicted own body (or driven vehicle) differs from the server's through runs, jumps, bursts, swaps, reloads, frags and driving |
| **Leak gate** | whole matches (Doubles, Gun Game, Big Team, Capture the Flag), one seat played through the client's input path: any enemy, round, event or medal reaches it that fog shouldn't allow, or any bit of the salt crosses the wire before match end (`room/tests/leaks.rs`) |
| Lag-comp bounds | a shot lands with more than 160 ms of favour, or a client's claimed ack beats the server's view |
| Impaired network | at 60 ms delay, 10 ms jitter, 5% loss: correction rate, bandwidth and interpolation stalls over their budgets (`client-net/tests/loopback.rs`) |
| End to end | the page in headless Chromium fails a match (WebGL2 and WebGPU), Gun Game, three matches back to back or a party (`ci/e2e.mjs`, `ci/party.mjs`); the desktop client's shots run but don't gate |
| Maps | a map's compiled tables aren't what its `map.json` compiles to; a spawn or pad off its floor or unreachable on foot; bots that don't fight on it |

Numbers in this page that come from measurements cite their source. Every
other number is a starting value, set in code and tuned from matches.

## 18 · Siege: team ARAM with guns

A longer Big Team match with a purpose. Everyone starts as the same Spartan
with the same rifle, sidearm and grenades: no heroes, no kits, nothing chosen
before the match. What a fighter becomes is what they pick up inside it. The
fight is in the middle; a side wins by breaking the other's generators, then
its base. Casual.

**Cards.** An ability is a card, as ARAM Mayhem's augments:
- **Two at most.** A fighter holds none, one or two. A third means dropping
  one of the two, and a dropped card lies where it fell for anyone to take.
- **Offered, never owed.** A fighter is offered cards as they play (kills,
  assists, damage to generators), a pick of one from a few, and may take
  none. Some never get an offer they want. Not every card is useful
  everywhere; that is the game.
- **Caches.** Air drops come down deep in each side's half on a timer, each
  an ability cache anyone may loot, either side: a reason to push deep and a
  reason to guard home.
- **Ultimates are rare cards**, mostly from caches: aimed, timed or
  telegraphed, each with a counter, never a kill at the press of a key.
- **Cards are verbs, not percentages:** a grapple, a hover, a rewind to five
  seconds ago, a wall, a frost bird the holder steers. No card changes a
  gun's counts or times, a shield or health.
- **Balance is not equal, it is fair over time.** Who gets what is luck;
  skill wins across a long match. A card that wins the match by itself is
  cut, not tuned.
- Skins stay cosmetic (§15): never a card, an offer or a cache.

**The match:**
- **The middle is a town:** the fight is there. Power weapons on pads, and
  the healing gun among them (restores a teammate's shields; anyone may carry
  it). There is no healer and no role; shields recharge as everywhere.
- **Generators.** Each side has three, out from the middle; each powers the
  side's core shield. A generator is broken by standing in its ring, as a
  territory is taken, not by shooting it: an enemy alone in the ring
  overloads it in 15 s, more of them faster (three at most); a defender in
  it holds it; defenders alone wind it back; an empty ring keeps what it
  had. A broken one stays broken and scores. With all three down the core's
  shield falls; the core is overloaded the same way (30 s) and breaking it
  wins. The ring's state is public, as Halo's territories are: how far, and
  whether it is under attack, never who or how many.
- **Towers.** Every generator standing, and every core, fires a needle at
  the nearest enemy in 30 m and clear line, minions first, about 13 a second
  (half that on a minion): a lone player in a ring dies about as fast as
  they overload it, so a push wants a teammate or a wave. A tower's kill is
  nobody's.
- **Minions.** Three a side come out of each base every 30 s (eight alive at
  most) and walk to the enemy's first generator standing, then the core
  once it is open, shooting what they see on the way: a weak gun that hits
  half the time. In a ring they count as one more attacker, however many.
  45 health, no shield: a burst or two of the rifle. Even waves trade in the
  middle; a player breaking that is the push. Killing one is no kill.
- **Bots** in Siege's seats aim worse than elsewhere, so people decide it.
- **Longer than Halo's:** 25 minutes, then two minutes of sudden death with
  every core open, then the side that broke more wins. Respawn is Halo's 3 s
  (lengthening late is still to come).
- No motion tracker and no minimap, as every mode (§1).

**How it sits in the sim:**
- A fighter's cards are a fixed array of two; offers, cooldowns and charge are
  ticks; caps are in `consts.rs`. No allocation in `World::step`.
- A card that reads only its fighter and the map (grapple, hover, rewind) is
  in `Fighter::step_local` and predicted like walking; one that touches
  another body is `World`'s (§17's prediction gate).
- Generators and cores are a kind of `Thing` (`World::nodes`), placed by a
  map's `"siege"` (Dry Fork's), always sent; caches will be one too.
- Fog: an enemy's cards are never sent; a card is known when it is used
  where it can be seen or heard, as a shot is (§8). An offer is told only to
  its fighter. Every card's projectile, area and reveal has a rule in
  `fog.rs` and a line in the leak gate.
- The rewind needs 5 s of each fighter's pose and shields, beside the 250 ms
  that lag compensation keeps (§7).
- Offers, a cache's contents and where a drop lands are keyed by the salt
  (§9): nothing a client could know before it happens.
- Bots play Siege: they take cards, use them, loot caches and go for the
  generators.

**The phases**, each kept only if it plays:

| Phase | What | The question |
|---|---|---|
| 0 · The base | generators and cores on Dry Fork, Siege's playlist with bots, no cards (done) | does a purpose keep Big Team's players? |
| 1 · Cards | a few cards (grapple, rewind, the frost bird), offered in Siege | do cards feel right in this sim and netcode, and on maps built for walking? |
| 2 · Caches | air drops, rare ultimates, a deck worth dealing from | do players come back for the luck? |
| 3 · A ladder | a Siege ladder, then cups | only if phase 2 holds players |

The same objectives can carry other modes (Capture the Flag is already
one); Siege is the first built for it.

Open: team size (Big Team's 8v8, or fewer to fill matches sooner); how many
cards are offered and how often.

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
| **Cups** (later) | FFA heats of 8, points per placement, swiss rounds, the top seats into a final. Same match server, same reports. |
| **Casual** | FFA and Gun Game with bots filling empty seats. Unranked. |
| **Big Team** | 8v8 Team Slayer on terrain maps with vehicles. Casual first; a ladder once it plays well. |

Halo's rules, the ones Fireplace's arena already plays:
- Shields over health. Shields recharge 5 s after the last hit; health follows.
- Equal starts: a rifle and a sidearm, two frags.
- Power weapons (sniper, rocket, shotgun) on pads with timed respawns, so a match is about map control.
- Melee, a head-shot rule per gun, respawn after 3 s at the spawn farthest from enemies.
- No motion tracker in ranked, as Halo's ranked playlists have it. That also removes a radar leak (§8).

Free to play. Skins are cosmetic only: money never changes a measured result,
whether a kill, a rating or a placement (Elo Pros' invariant 1).

Not yet: abilities, hidden-information modes beyond fog of war, voice.
Vehicles and terrain come with Big Team (§16, milestones 7 to 11), to rules
our own: one ground vehicle first, a driver and a gunner. Maps and vehicles
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
| `server` | Rooms, transport, sign-in, fog of war, lag-comp depth, snapshots, telemetry, reports | Native, wtransport over quinn. |
| `client-net` | Session (handshake), clock, prediction, reconciliation, interpolation | `core` and `session` build everywhere; the transport is wtransport natively and WebTransport in wasm, behind one `Wire` trait. |
| `client` | Bevy renderer: terrain, sky, lighting, viewmodel, HUD, audio, menus | Milestone 12 on. Native first; the same crate built for wasm (WebGPU, WebGL2 fallback) replaces `web/`. Draws; never decides. |
| `web/` | three.js renderer | Today's browser client. Kept until the Bevy wasm build plays as well, then deleted. |
| `tools/maps` | Map compiler and editor | JS, from Fireplace (`RIP.md`). |

Why Bevy now: the look we want (sky, clouds, terrain, grass, real lighting)
is what Gates' Bevy renderer already draws, and a desktop build comes free.
Gates' renderer is a donor, not a dependency: we port modules (`RIP.md` §9).
The cost is the browser download: Gates' wasm is 8.57 MB gzipped with a heap
around 700 MB (`Gates/docs/NOW.md` §0web). The wasm build gets a size gate,
and WebGL2 drops the atmosphere, SSAO, bloom and TAA.

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
input frame:  seq u16 · buttons u16 · yaw u16 · pitch u16 · move_x i8 · move_z i8 · slot u3
input datagram: ack of newest snapshot u16 · ack bits u32 · playout ticks u4 · first seq · frames…
```

- **The server steps on its own clock.** One frame is consumed per tick per
  player. A missing frame repeats the last one with tap buttons cleared, then
  decays movement to zero (Gates' `input::decay_frame`). A client sending
  faster gains nothing: the buffer is capped (16) and a second frame in a tick
  costs catch-up credit, so a speedhack or a lag switch buys at most about 3%
  (Gates' `INPUT_CATCHUP_CREDIT_CAP`, `server/src/client.rs` `consume_input`).
- **Inputs are sanitized.** Unknown button bits drop the datagram. Pitch is
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
  small buffer, steered by the server-reported input buffer depth (target 2
  ticks, deadband ±1), time-dilated within ±5%, hard resync if far off.
  Gates' `client-core/src/clock.rs`.
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

- A playout delay starts at 2 snapshot intervals (31 ms at 64 Hz) and adapts
  from measured jitter (RFC 3550 estimator) within 1 to 6 intervals. The client
  reports its delay to the server in each input datagram (it feeds lag
  compensation, §7).
- Linear interpolation of position, slerp of yaw and pitch, and animation state
  (moving, airborne, firing, reloading, dead) from the snapshot flags.
- On a gap: extrapolate at most 2 intervals from velocity, then freeze.
- This is Gates' `client-core/src/interp.rs`, retuned from 30 Hz.

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
   rays below cost a few thousand box tests a snapshot.
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
4. **Always sent:** teammates; the viewer's own killer for the kill cam; every
   fighter's name, kills and deaths for the scoreboard (never their position).
5. **Sound is a deliberate leak**, bounded:
   - a gunshot is sent with a position quantized to 2 m within 60 m (unheard
     past it);
   - footsteps within 12 m, only when the walker is moving fast (Halo footsteps
     are quiet);
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
| One bidirectional stream | handshake, sign-in, match start and end, chat, kill feed, the salt reveal |

- **WebTransport only** (Chrome, Edge, Firefox; check Safari at launch). A
  WebSocket fallback is not planned for v1.
- **Bit packing:** Gates' `protocol/src/bits.rs` (LSB-first, fixed caller
  buffers, no allocation, no panics, typed errors).
- **Acks:** Gaffer-style, a 16-bit snapshot ack plus 32 ack bits on every input
  datagram, so the server knows what each client has.
- **Drop-oldest:** a snapshot replaces the one queued before it; a datagram
  over the live MTU is dropped and counted, never fragmented (Gates' `send_snap`,
  browser `WebWire::send_datagram`).
- **A snapshot, per client:**
  - tick, last executed input seq, input buffer depth, time-dilation nudge;
  - the client's own full state (body, health, shields, guns, magazines,
    timers): what reconciliation needs;
  - each visible fighter, about 16 B: position (1/64 m), velocity, yaw and
    pitch, state flags (moving, airborne, crouched, firing, reloading, dead,
    shield flare), held gun, skin id;
  - visible projectiles, events since the last acked snapshot (shots heard,
    hits on you, kills, pickups), pad timers, score.

  About 200 B at 2v2, about 13 KB/s down. Full snapshots until milestone 10,
  which encodes each snapshot as a delta against the client's acked baseline
  (Gates' 64-entry sent ring, zero baseline when the ack is stale), so 16
  fighters and vehicles fit in a datagram. Events are cleared when a snapshot
  carrying them is acked, not when it is sent.
- **Goldens:** every message has a seeded fixture encoded byte-exact in CI.
  A wire change bumps `PROTO_VER` and regenerates them.

## 11 · Identity, queue and ratings

- **Sign-in:** the Elo Pros pass ("Sign in with Elo Pros"). The page gets a
  10-minute pass bound to game, server and nonce from
  `POST /api/account/play`; the server verifies it offline against
  `/api/account/pubkey` (Gates' `server/src/auth.rs` `verify_pass`). It covers
  EVM and Solana wallets and Elo logins. Raw SIWE (engine-shim's `siwe.rs`) is
  the fallback.
- **Ranked needs a wallet.** `ratings.py` refuses login-only `acct_` ids. A
  login plays casual.
- **One seat per wallet per match.**
- **Queue:** Elo Pros' `lobbies.py` op `queue`. It matches parties inside a
  rating window that grows while they wait and balances sides, then writes a
  match id and a server onto each party's lobby. The server reads
  `GET /lobbies/match/{id}` for who goes where, opens a room, and seats only
  those wallets.
- **Server rows:** the shard-list row and `status.json` the store reads, from
  engine-shim's `elo.rs`.
- **Ratings:** at match end the server posts `POST /ratings/<slug>/report`,
  signed with its server key (`serverkeys.py`: Ed25519, nonce, 300 s skew), on
  ladders `duel` and `doubles`. The math is Weng-Lin Plackett-Luce shown on the
  Elo scale; a new player is 1500 ± 246 and appears on the board after 5
  matches.

## 12 · Replays and evidence

The sim is deterministic, so a match is its seed, its salt, its map and its
input log. The server writes that for every match. It gives us:

- kill cams and spectating from the same data;
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

## 14 · Hosting

- One server process, many rooms (a room is a sim plus its clients; a 2v2 tick
  costs microseconds).
- Certificates from ACME (Gates' `ops/certbot-deploy-hook.sh` model), so
  production needs no certificate-hash pinning. Dev uses a self-signed 14-day
  certificate pinned by hash.
- QUIC admission with Retry past 2× capacity and refusal past 4× (Gates'
  `spawn_shard`). The wtransport commit stays pinned (Gates and engine-shim pin
  the same rev).
- One region at launch. Regions and ping-aware matching come next; `lobbies.py`
  has no ping signal yet.
- A server behind NAT can register with engine-shim's identity relay
  (`relay.rs`); not needed for our own hosts.

## 15 · Skins and money (short)

- Weapon and character skins are Metaplex Core NFTs on Solana, one collection
  for the game, 0 royalty, the house paying rent (Elo Pros'
  `docs/items/SOLANA-ITEMS.md`, `meter/solana_items.py`, `deploy/solana-items/`).
- The server checks a player's skins against their wallet at sign-in and only
  then puts a skin id in snapshots. A client can't claim one it doesn't own.
- The free Pill skin after 10 minutes of play. Store prices in PROS. Cases
  show their odds above the pull. Season prizes are paid by the house to the
  top of each ladder after review (§13).
- Skins never change a hitbox, a sound's loudness or a colour's visibility.

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
   Gun Game, the kill cam, the impaired-network gate.

From here the game grows into a full shooter. The sim comes first; each step
keeps every gate in §17 green and moves a golden only on purpose.

7. **Entity pools:** fighters, projectiles and pads behind one `Thing` shape,
   so a new kind is a short list the compiler checks. No behaviour change;
   the golden does not move.
8. **16 seats and Big Team:** 8v8 Team Slayer, casual.
9. **Terrain:** a deterministic heightfield beside the boxes, slopes and
   sliding, a box grid for rays; an original canyon map with its own golden.
   **Launchers** too: a pad in the map that throws a fighter along a set
   velocity. It reads only the fighter and the map, so it predicts exactly
   and costs no wire. Skiing (low-friction slides down slopes) is a later
   experiment.
10. **Vehicles:** one ground vehicle, four ray-cast wheels, a driver and a
    gunner, rewound for hitscan, splatter.
11. **The delta wire:** snapshots as deltas against the acked baseline, a
    byte budget, events cleared on ack.
12. **The Bevy client:** a session layer in `client-net` and a native
    transport, then the client: a playable skeleton, Gates' lighting, sky,
    terrain and grass, the game's feel, and last its wasm build replacing
    `web/`.

## 17 · What CI blocks a merge on

| Gate | Fails when |
|---|---|
| Determinism | a recorded match hashes differently native vs wasm, or a golden moves without a rule change |
| Zero allocation | the sim tick allocates after warmup |
| Goldens | any wire message's bytes change without a `PROTO_VER` bump |
| Prediction | the predicted own body (or driven vehicle) differs from the server's through runs, jumps, bursts, swaps, reloads, frags and driving |
| **Leak gate** | headless Chrome plus bots: any enemy reaches a client while not visible within the lead window, or any byte of the salt crosses the wire before match end |
| Lag-comp bounds | a shot lands with more than 160 ms of favour, or a client's claimed ack beats the server's view |
| Impaired network | at 60 ms delay, 10 ms jitter, 5% loss: correction rate, bandwidth and interpolation stalls over their budgets (`client-net/tests/loopback.rs`) |
| Maps | a map's compiled tables aren't what its `map.json` compiles to; a spawn or pad off its floor or unreachable on foot; bots that don't fight on it |

Numbers in this page that come from measurements cite their source. Every
other number is a starting value, set in code and tuned from matches.

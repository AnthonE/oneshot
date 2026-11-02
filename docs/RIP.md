# RIP.md: what we take, from where

What this repo takes from our other four, file by file. **Copy** means lift it
and trim. **Port** means rewrite it here from that source. **Reference** means
read it, don't take it. **API** means we call it and don't copy it. Line counts
were measured on 2026-11-02 with `wc -l`. The design these pieces serve is
`DESIGN.md`.

| Repo | Licence | What it gives us |
|---|---|---|
| `AnthonE/Gates` | MIT code (© 2026 MoreRight DAO); models are Meshy commissions (ours, no credit owed); sounds and textures CC0 or public domain | the authoritative netcode, the player model, the guns, the sounds |
| `AnthonE/Fireplace` | CC0 | Halo's rules, the bot, the map toolchain, maps |
| `AnthonE/engine-shim` | MIT (its generated `slots.rs` is Facepunch's MIT; we take none of it) | server-side wtransport, SIWE, store rows, the identity relay |
| `AnthonE/scry-forge` (Elo Pros) | MIT | sign-in, queue, ratings, Solana items, called as APIs |

## 1 · Netcode and server, from Gates (Rust → Rust)

The pieces are good. The crates they live in are welded to a 100k-line survival
sim (`protocol` and `client-core` both depend on `sim-core`). So we **copy
files and trim**; we never depend on Gates' crates.

| Source (`Gates/crates/…`) | Lines | → here | How | What we keep |
|---|---|---|---|---|
| `client/src/net/web.rs` | 605 | `client-net` | Copy | Browser WebTransport: `serverCertificateHashes`, `requireUnreliable`, the datagram writer that never awaits (drops on backpressure) |
| `client/src/net/frame.rs` | 198 | `client-net` | Copy | Length-prefixed framing for browser streams that arrive in pieces |
| `client/src/net/mod.rs` | 589 | `client-net` | Copy, trim | The `Wire` trait (`send_datagram`, `rtt_ms`) |
| `client/src/net/native.rs` | 71 | `client-net` | Copy | Native wire for bots and tests |
| `client/src/net/handshake.rs` | 1,238 | `client-net` | Port | Hello → challenge → auth → welcome, on our messages |
| `client-web/src/lib.rs` | 566 | `client-net` | Copy, trim | wasm-bindgen entry (`join`, `join_pass`, `watch`) |
| `protocol/src/bits.rs` | 246 | `proto` | Copy as is | `BitWriter`/`BitReader`, no allocation, no panics |
| `protocol/src/auth.rs` | 1,339 | `proto` | Copy, trim | SIWE text, the Elo pass format, challenge and auth messages |
| `protocol/src/lib.rs` | 7,290 | `proto` | Port about 1,200 | `encode_input` and `SnapshotEncoder` as templates: ack plus ack bits, redundant input frames, the entity record, quantization, MTU shedding. Survival fields stay behind. |
| `protocol/src/goldens.rs` + `examples/gen_goldens.rs` | 3,350 | `proto` | Pattern | Seeded fixtures checked byte-exact |
| `server/src/net.rs` | 7,105 | `server` | Copy about 1,500 | `spawn_shard` (wtransport config, BBR/CUBIC, keep-alive), `bind_udp` (8 MiB buffers), Retry admission, `handshake_task`, `accept_input`, `writer_task` (newest snapshot wins), `send_snap` (MTU check). Leave saves, skins, admin and spectator plumbing. |
| `server/src/client.rs` | 1,864 | `server` | Copy, trim | `ClientNetState`: acks, `newest_acked`, the input buffer (`INPUT_BUFFER_CAP`), catch-up credit, decayed repeats |
| `server/src/stats.rs` (around 100–330) | 1,345 | `server` | Port | `favour_for` and its caps: the server-chosen lag-comp depth |
| `server/src/auth.rs` | 870 | `server` | Copy | `verify` (SIWE secp256k1), `verify_solana_any`, `verify_pass` (Elo pass, offline) |
| `server/src/handoff.rs` | 587 | `server` | Copy (later) | Signed hand-off between servers: lobby → match server |
| `server/src/pace.rs` | 244 | `server` | Copy | Minimum gap per action kind |
| `server/src/cheat.rs` | 856 | `server` | Port | Snap-hit, head-shot rate, tracking-through-walls checks; evidence, never a kick |
| `server/src/anomaly.rs` | 524 | `server` | Copy, trim | The evidence log `cheat.rs` writes to |
| `client-core/src/predict.rs` | 798 | `client-net` | Port | Prediction ring, reconcile, render-only error decay |
| `client-core/src/interp.rs` | 728 | `client-net` | Port | Snapshot interpolation, adaptive playout from jitter, bounded extrapolation |
| `client-core/src/clock.rs` | 344 | `client-net` | Port | Run-ahead clock, ±5% dilation on buffer depth, hard resync |
| `sim-core/src/rewind.rs` | 269 | `sim` | Copy, extend | The per-tick pose ring (`write_row`, `pose_at`, `forget`); add sub-tick interpolation |
| `sim-core/src/fmath.rs` | 151 | `sim` | Copy as is | Pure-Rust trig/exp/pow, same bits native and wasm |
| `sim-core/src/rng.rs` | 157 | `sim` | Copy | PCG plus `sip24` keyed hash (the salted spread) |
| `sim-core/src/yaw_lut.rs`, `pitch_lut.rs` | 273, 285 | `sim` | Copy | Exact aim directions from quantized angles |
| `sim-core/src/ranged.rs` | 3,285 | `sim` | Reference | `hitscan_in`, `aim_dir`, `cone_dir`, the fire-rate gate, bloom |
| `sim-core/src/collide.rs` | 2,725 | `sim` | Reference | Head/chest/limb bands on a body; we use body cylinder plus head sphere (§2) |
| `sim-core/src/movement.rs` | 666 | `sim` | Reference | Quantize-both-sides rules; the movement itself comes from Fireplace (§2) |
| `content/weapons.toml` | 906 | `sim` data | Reference | Recoil pitch/yaw ranges, bloom, falloff, head and limb percentages |
| `ops/certbot-deploy-hook.sh` | | `ops/` | Copy | ACME renewal for the server's certificate |
| `docs/NETCODE.md`, `reference/NETWORK.md` | | | Reference | Why each number is what it is; read before changing one |

**Not taken from Gates:** `sim-core`'s survival world (`world.rs`, terrain,
building, crafting), `protocol/src/event.rs` (12,889 lines of survival
events), `server/src/core.rs` and `client-core/src/core.rs` (whole-game
state), the Bevy client and its shaders.

## 2 · Game rules, bot and maps, from Fireplace (WAT/JS → Rust/JS)

Fireplace's arena already plays Halo. We port its rules to Rust `sim` and
keep its JS map tools nearly as they are.

| Source (`Fireplace/…`) | Lines | → here | How | What we keep |
|---|---|---|---|---|
| `src/layout.mjs` `ARMS` (line 560 on) and `docs/arena.md` "The guns" | 1,146 | `sim` | Port | Every gun's fire mode, damage, refire, magazine, spread, head-shot rule, range; shields 100, health 100, 5 s recharge, respawn 3 s, frags |
| `src/layout.mjs` `PHYS` (line 263 on) | | `sim` | Port | Quake-style movement: `RUN 10` m/s, `STOP 3`, friction 4, accelerate 10, air cap, gravity 20 m/s², jump 7, eye 1.6, top 1.8, radius 0.5. Restate in seconds, not per tick. Tune run speed down toward Halo's feel. |
| `src/players.wat` | 387 | `sim` | Port | Ground/air movement and step-up against boxes and heightfield |
| `src/arena.wat` | 2,148 | `sim` | Port | `$a_shoot`, `$a_round`, `$a_hitscan`, `$a_ray` (hitbox shapes: body cylinder, head sphere 0.22 m at 1.62 m), `$a_hurt`, `$a_kill`, `$a_melee`, pads, spawns (farthest of four from enemies), respawn |
| `src/weapons.wat` | 326 | `sim` | Port | Rockets, frags, splash, rocket jumps |
| `src/am/slayer.*`, `src/am/*` | | `sim` | Port Slayer; reference the rest | The team-mode kit's shape: `start`, `tick`, `kill`, `bar` hooks |
| `client/arena-bot.mjs` | 646 | `server` (Rust) | Port | The AI fighter: target choice by sight, settling aim with human wobble (1.4–2.8° rms), 0.25–0.6 s reaction, blind past 60°, gun by range, strafing, frags, pads. It runs server-side against the true world. |
| `tools/map-compile.mjs` | 366 | `tools/maps` | Port (done: `tools/maps/compile.mjs`) | Boxes, prefabs, quarter turns; our flat floor (no heightfield, no movers), team mirroring, output as Rust tables the sim builds a `Map` from. The visibility bake (`DESIGN.md` §8) is still to come. |
| `tools/png-codec.mjs` | 125 | `tools/maps` | Copy | PNG encode/decode in typed arrays |
| `tools/scene-map.mjs` | 607 | `tools/maps` | Copy | Convex solids to boxes, ramps to stairs |
| `tools/map.mjs` | 56 | `tools/maps` | Copy | CLI; documents `map.json` |
| `tools/roblox.mjs`, `rbx-map.mjs`, `rbx-file.mjs` | 91, 386, 284 | `tools/maps` | Copy | A Roblox place as a map |
| `tools/vox.mjs`, `vox-map.mjs`, `vox-file.mjs` | 60, 192, 189 | `tools/maps` | Copy | A MagicaVoxel model as a map |
| `client/map-editor.html`, `map-editor.mjs` | 86, 1,356 | `tools/maps` | Copy | Draw a map in the page; it runs the compiler live |
| `maps/canyon/`, `maps/lifts/` | | `maps/` | Copy | Box Canyon (bases, cave, teleporters, pads) and Lifts as the first test maps |
| `maps/crossroads/` | | `maps/` | Optional | Roblox's open-source level; **keep its `LICENSE` (MIT © 2020 Roblox)** |
| `net/rooms.mjs` | 174 | | Reference | Room pool: keep N free, grow to a cap, recycle idle |
| `net/replay.mjs` | 87 | | Reference | Replay file shape (wasm hash, seed, hashes) |
| `docs/netcode.md`, `docs/arena.md` | | | Reference | Input slack and redundancy; lag-comp tests worth keeping (a sniper shot at 12 and 25 ticks back) |

**Not taken from Fireplace:** the lockstep netcode itself (every client holds
the whole world), the WAT build, the Deno relay, the box renderer.

## 3 · Player model and guns, from Gates (GLB → three.js)

**Every Gates GLB stores its textures as KTX2 without declaring
`KHR_texture_basisu`, so three.js's `GLTFLoader` won't load them as they are.**
Run them through `ci/store_pack.py` (KTX2 → PNG; it also undoes a
linear-colour bug in data maps packed between 2026-08-11 and 2026-09-05), or
patch the extension in and use `KTX2Loader`. `ci/store_preview.mjs` is the
check that a pack loads in three.js.

### The player: Triple T

| Source | Size | Notes |
|---|---|---|
| `assets/models/stumpy.glb` | 4.2 MB | Triple T. One skin, 54 joints (fingers added by `rig_fingers.py`), about 31k tris, stands 1.8 m at scale 1. Two meshes, `char1_arms` and `char1_body`, share one vertex buffer: the arms are the first-person view, the body is what others see. One 1024² albedo, **very dark (luma 0.056): lift it for readability.** |

The clips we use, from its 55:
- Gun: `Rifle_Hold_Loop`, `Rifle_Reload`, `Pistol_Idle_Loop`, `Pistol_Aim_Down/Neutral/Up`, `Pistol_Shoot`, `Pistol_Reload`.
- Movement: `Idle_Loop`, `Walk_Loop`, `Jog_Fwd_Loop`, `Sprint_Loop`, `Crouch_Idle_Loop`, `Crouch_Fwd_Loop`, `Jump_Start/Loop/Land`.
- Hits and death: `Hit_Chest`, `Hit_Head`, `Death01`.
- Melee: `Punch_*`. Fun: `Dance_Loop`, and "Greet" (the UUID-named clip).

The store pack `gates-triple-t` is already exported for three.js.

For a full room, decimate the body to about 8–12k tris for third person.

### Guns: Halo roles to Gates models

Each gun is about 1 MB. They're stored **barrel-up (+Z top)** with a measured
grip; re-orient for the viewmodel (`ci/stand_grip.py` has the grip numbers).

| Role (Fireplace `ARMS`) | Gates model (`assets/models/held/`) |
|---|---|
| Assault rifle (start) | `assault_rifle.glb` |
| Battle rifle, 3-round burst | `semi_automatic_rifle.glb` |
| Magnum / sidearm (start) | `python_revolver.glb` (or `semi_automatic_pistol.glb`) |
| SMG | `custom_smg.glb` (or `thompson.glb`) |
| Sniper (power, pad) | `bolt_action_rifle.glb` (modelled with its scope) |
| Shotgun (power, pad) | `pump_shotgun.glb` |
| Rocket launcher (power, pad) | `rocket_launcher.glb` + `rocket.glb` |
| Frag | `beancan_grenade.glb` |
| Plasma rifle, seeker, sword | none; drop from v1 or model new |

No scope or attachment models exist (icons only).

### Weapon skins (the first NFTs)

`assets/models/held/skins/`, 1.9–3.9 MB each, repainted on the original UVs by
`ci/meshy_retexture.py`:

| Skin | File |
|---|---|
| Verdigris | `verdigris_rifle.glb`, `verdigris_python.glb` |
| Lapis | `lapis_thompson.glb`, `lapis_bolt_rifle.glb` |
| Tidewrack | `tidewrack_sar.glb`, `tidewrack_smg.glb`, `tidewrack_shotgun.glb` |
| Gilded | `gilded_revolver.glb` |

Store packs `gates-verdigris-weapons`, `gates-lapis-weapons` and
`gates-tidewrack-weapons` (`assets/store/packs.toml`) already group them. The
Founder skins are Gates' alpha gift; leave them there.

### The pipeline (`Gates/ci/`, Python with Blender or the `ktx` CLI)

| Script | Lines | Use here |
|---|---|---|
| `store_pack.py` | 182 | GLB with KTX2 → GLB with PNG, plus `elo-asset.json` |
| `unbake_ktx.py` | 255 | KTX2 → PNG for one file, undoing the linear bug |
| `store_preview.mjs` | 191 | Load every GLB in three.js in headless Chromium and render a preview: our asset gate |
| `ktx_pack.py` | 314 | Back to KTX2 later if VRAM matters (then use `KTX2Loader`) |
| `stand_grip.py` | 201 | Stand a gun on its grip axis |
| `split_arms.py` | 228 | Split a skinned character into first-person arms and body |
| `retarget_anim.py` | 607 | Move an animation library onto Triple T's rig (more gun clips from the CC0 mannequin) |
| `import_char.py` | 248 | Normalise a rigged Meshy character |
| `import_meshy.py` | 412 | Normalise a Meshy prop (scale, centre, turn) |
| `meshy_gen.py` | 275 | Generate new models (plasma rifle, scopes, map props) |
| `meshy_retexture.py` | 384 | New weapon skins on existing UVs |
| `curl_hands.py`, `rig_fingers.py` | 510, 256 | Hand pose and finger bones (already applied to Triple T) |

`assets/models/mannequin.gltf` (3.2 MB, Quaternius, CC0, 46 clips) is the
source rig for retargeting. It ships with nothing.

## 4 · Sound, from Gates (CC0 / public domain, mono ogg)

All from `Gates/assets/sound/`, credits in its `MANIFEST.md`.

| Need | Files (`_0`… takes) |
|---|---|
| Shots, near | `pd/shot_rifle`, `shot_smg`, `shot_pistol`, `shot_shotgun`, `shot_bolt`, `shot_heli` (heavy auto), `shot_rocket`; `freesound/shot_gun` (revolver) |
| Shots, far | `pd/shot_{rifle,smg,pistol,shotgun,bolt,heli}_far`; `freesound/shot_gun_far` |
| Reloads | `pd/reload_rifle`, `reload_pistol`, `reload_shotgun`, `reload_bolt`; `freesound/reload` |
| Dry fire | `pd/dry_fire` |
| Bullets | `kenney/impactSoft_medium`, `impactGeneric_light`, `impactWood_light`, `impactMetal_light`; `freesound/ricochet`; `pd/flyby` |
| Explosions | `freesound/blast`, `freesound/fuse` (frag) |
| Hit feedback | `kenney/hit` (marker), `kenney/hit_limb`, `pd/headshot` |
| Bodies | `freesound/flesh`, `freesound/hurt`, `freesound/death` |
| Footsteps | `pd/step_concrete`, `step_metal`, `step_wood`; `freesound/step_rock`, `step_sand`, `step_grass`, `land` |
| Melee | `freesound/swing` |
| UI | `kenney/ui_click`, `ui_hover`, `refused`, `equip` |

Fireplace's `client/sound.mjs` (162 lines, synthesised WebAudio) is a
reference for zero-download fallbacks.

Not taken: `Gates/assets/music/` (ElevenLabs, a vendor licence, not CC; read
its terms before using).

## 5 · Textures and UI

- `Gates/assets/textures/`: CC0 (Poly Haven, ambientCG, TextureCan) albedo,
  normal, roughness and AO sets for `concrete`, `metal`, `corrugated`,
  `ashlar`, `stone`, `rock`, `gravel`, `sand`, `wood`, `aggregate`. That's
  enough to dress arena boxes. Don't take the four candidates in
  `CANDIDATES_CC_BY.md`; they aren't shipped.
- `Gates/assets/icons/`: game-icons.net, **CC BY 3.0, credit required**. Take
  the credits from `assets/icons/CREDITS.md` (it and Gates' `NOTICE` disagree on
  the authors; check the files we take). Gates' own item renders in there are
  ours.
- Fireplace `client/hudfont.mjs` (125) and `client/font.mjs` (34): a HUD
  font atlas drawn at startup from the system font, no font file.

## 6 · From engine-shim (Rust → Rust, MIT)

| Source (`engine-shim/engine2/src/…`) | Lines | → here | How | What we keep |
|---|---|---|---|---|
| `net.rs` | 1,217 | `server` | Reference / copy parts | wtransport server: one reliable stream in order, datagrams dropped rather than waited for, self-signed dev certs with the hash printed, `ENGINE2_TLS_*` env shape |
| `siwe.rs` | 247 | `server` | Copy | EIP-4361 checked locally: keccak plus secp256k1 recovery, domain, nonce once, age |
| `elo.rs` | 348 | `server` | Copy, rename env | The store side: the `elo-shardlist-v1` row, `GET /status.json` and `/online.json`, the copy check at join and every two minutes ("could not look" never kicks) |
| `relay.rs`, `bin/relay.rs` | 275 | | Later | Identity relay for servers behind NAT, billed per player-hour through Elo Pros |
| `Cargo.toml` | | workspace | Copy the pin | `wtransport` at rev `a11e6a8e7ff33c6eef522d308007da55d1df9f81` (the same rev Gates pins: crates.io 0.7.1 has a remote panic) |
| `physics.rs` (Rapier) | 2,229 | | Reference | If maps outgrow boxes: Rapier's query pipeline for rays and shape casts against triangle meshes (turn on `enhanced-determinism` so native and wasm agree) |
| `render/` | about 1,500 | | Reference | A wgpu skinned-model renderer, if we ever want the client all-Rust |

## 7 · Elo Pros (scry-forge): APIs we call

Nothing copied. These are the services the server and page talk to.

| Endpoint / module | For |
|---|---|
| `POST /api/account/play`, `GET /api/account/pubkey` (`meter/accounts.py`), `/sdk/elo-signin-1.js` | The sign-in pass the page gets and the server verifies offline |
| `POST /api/signin/check` (`meter/signin_check.py`) | Raw SIWE / Solana sign-in fallback |
| `lobbies.py`: `POST /lobbies/prepare`, `/lobbies/act` op `queue`, `GET /lobbies/{id}`, `GET /lobbies/match/{id}` | The rated queue and who is seated in a match |
| `ratings.py`: `POST /ratings/{slug}/report` (signed per `serverkeys.py`), `GET /ratings/{slug}/matches` | Ratings on ladders `duel` and `doubles`; the public match log |
| `gamestats.py`: `POST /gamestats/{slug}/report` | Achievements and stats |
| `solana_items.py`, `deploy/solana-items/`, `docs/items/SOLANA-ITEMS.md` | Skins as Metaplex Core NFTs; ownership at sign-in |
| `skin_rotation.py`, `itemstore.py`, `EloGacha` | Store rotations and cases (odds shown above the pull: `docs/start/DO-NOT-STRIKE.md` §3–4) |

**Owed on the Elo Pros side:**
1. An optional `evidence` key in `ratings.py` `MATCH_KEYS` and the match log
   (`replay_sha256`, `salt_commit`).
2. `meter/bans.py`: a per-game, per-wallet ban list the server reads at
   sign-in.
3. A `server_keys` block and listing row for the new game.
4. Later: a ping signal in `lobbies.py` for regions.

## 8 · This repo's NOTICE

- **Required:** game-icons.net authors (CC BY 3.0), if any icon is taken.
- **Required:** Roblox's MIT notice, if `maps/crossroads/` is taken.
- **Credited by choice:** CC0 sound sources (Kenney, Freesound uploaders, USC
  archive, OpenGameArt's Free Firearm Sound Library), CC0 textures (Poly Haven,
  ambientCG, TextureCan), Quaternius (CC0) if the mannequin is used.
- **Ours:** Meshy-commissioned models (Triple T, guns, skins): no credit owed;
  don't post them to Meshy's community feed.
- Never another game's art. The guns were designed after Rust's (Facepunch's);
  name and present them as our own (as Gates' store packs do), and never ship
  Facepunch's own assets.
- No NC or SA licence anywhere: the game is sold.

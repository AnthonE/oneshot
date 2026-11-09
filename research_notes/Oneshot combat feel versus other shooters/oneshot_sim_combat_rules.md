# oneshot: the combat rules as implemented in `crates/sim` (audit of the repo at HEAD `1778766`, 2026-11-09)

Conventions used in these notes:

- Every fact carries a `path:line` reference into `/home/user/oneshot`. Paths are repo-relative.
- **[code]** = read directly from the source. **[measured]** = printed by the repo's own range test, which I ran once (`cargo test -p sim --test range -- --ignored --nocapture`, exit 0). **[derived]** = my own arithmetic from the code's constants and formulas (tick rounding, discrete re-simulation of the movement step in a scratch script); not a number that appears in the repo. **[inferred]** = concluded from reading logic or from the absence of code, not pinned by a test.
- Durations in the code are milliseconds; the sim turns them into whole ticks with `ticks(ms) = round(ms × 128 / 1000)` (`crates/sim/src/consts.rs:12-14`), so the *actual* interval is often a few ms off the written one. Both are given.
- Points: shields and health are each 100 points, stored in 1/256 point (`consts.rs:124-126`).

## What is the sim tick rate, and what is the world's unit scale (hitbox size, head hitbox)?

### Takeaway
The sim runs at 128 Hz and is written directly in metres and seconds; Halo 2 figures are imported at 3.048 m per Halo world unit. A fighter is a 1.8 m tall, 0.70 m wide walking box; for bullets it is a 0.70 m wide, 1.45 m tall cylinder with a 0.44 m diameter head sphere centred at 1.62 m.

### Cited Findings
- Tick rate: `TICK_HZ = 128`, `DT = 1/128 s` (7.8125 ms), "on the server and in the browser's prediction" — `crates/sim/src/consts.rs:5-8`. Design doc: "Sim tick 128 Hz (7.8 ms) ... Valorant runs 128", snapshots 64 Hz, inputs sampled every tick and sent at 64 Hz — `docs/DESIGN.md:93-95`.
- Unit scale: the sim's unit is the metre. The movement block says "Its world unit is 10 ft (3.048 m), the scale the guns are in: 2.25 wu/s ahead, 2.0 backing up and to the side (the Editing Kit's player metrics)" — i.e. Halo 2's world unit is converted at 3.048 m — `consts.rs:44-48`. Same conversion quoted for the rocket (`arms.rs:413-414`), grenades (`consts.rs:223-225`) and the needler supercombine (`consts.rs:207-210`).
- Positions and velocities are snapped to a 1/1024 m grid every tick — `consts.rs:118-121`, `movement.rs:52-55, 282-283`.
- Walking body (collision with the map): half-width `BODY_R = 0.35` (a 0.70 m box), height `BODY_H = 1.8`, crouched `CROUCH_H = 1.3` — `consts.rs:71-74`. Header comment: "The body: a box for walking, a cylinder and a sphere for being shot" — `consts.rs:71`.
- Eye height `EYE = 1.6`, crouched `CROUCH_EYE = 1.1` — `consts.rs:75-76`. Shots and melee rays start at the eye (`world.rs:885, 1202`).
- Hitscan hit volume: body cylinder radius `HIT_R = 0.35`, from the feet up to `HIT_TOP = 1.45` (crouched `0.95`); head sphere radius `HEAD_R = 0.22` centred at `HEAD_Y = 1.62` (crouched `1.12`) — `consts.rs:97-102`. Tested in `World::trace`: head sphere first, then the cylinder, the nearer hit along the ray wins — `world.rs:1017-1030`; geometry in `geom.rs:137-186`.
- A crouch moves the eye and both hit volumes over `CROUCH_TICKS = ticks(125)` = 16 ticks = 125 ms down, and the same back up ("so a crouch can't snap a head out of a sight in a tick (Halo 2 lowers it over a moment)") — `consts.rs:77-81`, `movement.rs:28-31, 115-120`.
- Projectiles (rockets, bolts, shards, lobs, stickies) use a *different* volume: a cylinder of radius `HIT_R + 0.1 = 0.45` over the full body height (1.8 standing, 1.3 crouched), no head — `world.rs:1551-1557`.
- Melee uses the hitscan volumes grown by 0.25 m (`world.rs:1211`); the melee lunge check grows them by `MELEE_LUNGE_PAD = 0.4` (`world.rs:1228`, `consts.rs:181`); the sword by `LUNGE_PAD = 0.5` (`world.rs:1269`, `consts.rs:194`).
- Pitch limit ±89° (`PITCH_MAX = 16202` of 65536) — `consts.rs:115-116`.

### Inferences
- [derived] Head sphere spans 1.40–1.84 m: it overlaps the top 5 cm of the body cylinder and its top is 4 cm *above* the 1.8 m walking box. Hitscan silhouette is therefore 1.84 m tall.
- [derived] Head width 0.44 m is 63% of the body's 0.70 m width. Frontal areas: head disc π·0.22² ≈ 0.152 m²; body rectangle 0.70 × 1.45 ≈ 1.015 m²; the head is ≈ 13% of the total silhouette.
- [derived] The eye (1.60 m) sits 2 cm below the head sphere's centre (1.62 m).
- [derived] Crouching drops the head centre 0.50 m in 125 ms (4 m/s of vertical head travel).

### Gaps
- The repo does not state how tall or wide Halo 2's player is in world units, so I cannot say from the code whether a 1.8 m × 0.70 m body is the same *relative* size as Halo's at the 3.048 m/wu conversion used for speeds. The jump comment says the jump was scaled "in proportion to our body" (`consts.rs:63-65`), while run speeds were converted by world unit (`consts.rs:45-47`) — two different scaling rules.

## Guns: full stat lines, shots-to-kill, TTK, hitscan vs projectile, spread/bloom/recoil, falloff, magnetism and aim assist, loadout, pads

### Takeaway
Fifteen handheld guns, all declared as "Tuned to Halo 2's feel: its counts ... and times carried onto our 100 shield and 100 health (Halo 2's are 70 and 45)". Every hitscan gun has a fixed, salted random cone, no recoil, no bloom, no movement or scope modifier; **bullet magnetism exists in code but is switched off (`MAGNETISM = false`)**, and there is no aim assist, friction or auto-aim anywhere. Everyone spawns with a Battle Rifle and a Magnum; the Magnum's perfect time-to-kill (719 ms) is much shorter than the Battle Rifle's (1289 ms).

### Cited Findings

**What it says it is modelled on**
- "The guns. Ported from Fireplace's `ARMS` ... Halo's set: an equal start of a rifle and a sidearm, power weapons on pads. Tuned to Halo 2's feel: its counts (shots to kill, rounds to kill) and times carried onto our 100 shield and 100 health (Halo 2's are 70 and 45). `tests/range.rs` measures every gun in a match and pins it" — `crates/sim/src/arms.rs:1-8`.
- "The pins are the feel we tune to: Halo 2's, carried onto our 100 shield and 100 health (Halo 2 is 70 and 45, so it is the counts that carry, not the points)" — `crates/sim/tests/range.rs:7-10`. The range's shooter is "a perfect shooter (on the trigger the tick it may fire, on the spot it aims at) against a target standing still" — `range.rs:1-5`.
- Design of record: "Halo 2's arsenal as archetypes ... Its counts and times are the baseline (`sim/tests/range.rs`); tuning makes them our own" — `docs/DESIGN.md:24`.

**Stat lines [code]** (interval = written ms → ticks → actual ms; rate = rounds/s at the actual interval [derived])

| Gun (id) | Kind | Dmg/round | Head rule | Trigger and interval | Mag / reserve | Reload | Cone half-angle | Max range; falloff | Zoom | Source |
|---|---|---|---|---|---|---|---|---|---|---|
| Assault Rifle (1) | hitscan | 8 | none | auto; 88 ms → 11 t → 85.9 ms (11.64/s) | 32 / 192 | 1760 ms | q14(40) = 0.879° | 90 m; none | 1× | `arms.rs:315-329` |
| Battle Rifle (2) | hitscan | 10 | kills if shields are 0 | 3-round burst; 67 ms between rounds → 9 t → 70.3 ms; 266 ms after the last round → 34 t → 265.6 ms; burst cycle 52 t = 406.25 ms | 36 / 108 | 1760 ms | q14(6) = 0.132° | 160 m; none | 2× | `arms.rs:330-350` |
| Magnum (3) | hitscan | 28 | kills if shields are 0 | semi; 176 ms → 23 t → 179.7 ms (5.57/s) | 12 / 48 | 1280 ms | q14(4) = 0.088° (dual 0.308°) | 110 m; none | none | `arms.rs:351-368` |
| Sniper Rifle (4) | hitscan | 120 | always kills | semi; 500 ms → 64 t → 500 ms | 4 / 20 | 1920 ms | 0 | 420 m; none | 5× / 10× | `arms.rs:369-383` |
| Shotgun (5) | hitscan, 10 pellets | 29 per pellet | none | semi; 1000 ms → 128 t | 12 / 36 | 2000 ms (whole tube) | q14(260) = 5.71° | 28 m; full to 8 m, linear to 20% at 28 m | 1× | `arms.rs:384-401` |
| Rocket Launcher (6) | projectile, blast | 230 | – | semi; 880 ms → 113 t → 882.8 ms | 2 / 6 | 2000 ms | 0 | – | 2× | `arms.rs:402-422` |
| SMG (7) | hitscan | 8 | none | auto; 67 ms → 9 t → 70.3 ms (14.22/s) | 60 / 180 | 1600 ms | q14(80) = 1.758° (dual 2.197°) | 50 m; full to 20 m, linear to 60% at 50 m | 1× | `arms.rs:423-444` |
| Sword (8) | melee swing / lunge | 200 | – | 560 ms → 72 t → 562.5 ms | battery never spent | – | – | lunge from 6 m | 1× | `arms.rs:445-459` |
| Carbine (9) | hitscan | 17 | kills if shields are 0 | semi; 167 ms → 21 t → 164.1 ms (6.10/s) | 18 / 72 | 1800 ms | 0.3° | 122 m; none | 2× | `arms.rs:462-480` |
| Beam Rifle (10) | hitscan, heat | 120 | always kills | semi; 250 ms → 32 t; 65% heat a shot, cools 58%/s, vents 30%/s | battery 18 | vent 427 t = 3336 ms | 0 | 366 m; none | 5× / 10× | `arms.rs:481-498` |
| Ion Rifle (11) | bolt projectile 64 m/s, heat | 8 (×2.5 on shields) | none | auto; 133 ms → 17 t → 132.8 ms (7.53/s); 15% heat a shot, cools 85%/s, vents 35%/s | battery 400 | vent 366 t = 2859 ms | 0.5° (dual 2.0°) | lives 100 m | 1× | `arms.rs:499-521` |
| Ion Pistol (12) | bolt projectile 50 m/s, heat, charge | 5 (×3.0 on shields) | none | semi on release; 67 ms → 9 t → 70.3 ms; 16% heat a shot, cools 60%/s, vents 45%/s; charge 600 ms → charged bolt 23 m/s, homes 80°/s, costs 37 battery and 100% heat | battery 250 | vent 284 t = 2219 ms | 0 (dual 0.5°) | lives 122 m | 1× | `arms.rs:522-547` |
| Shard Gun (13) | homing needle 18.3 m/s, homes 30°/s | 4 (×1.5 on shields); 8 stuck go off | none | auto; 133 ms → 17 t → 132.8 ms | 30 / 90 | 2000 ms | 1.0° (dual 2.0°) | lives 73 m | 1× | `arms.rs:548-571` |
| Grenade Launcher (14) | lob, blast | 75 | – | semi; 300 ms → 38 t → 296.9 ms | 4 / 12 | 2200 ms | 0.6° | 61 → 30 m/s by 46 m, 30% gravity, 200 ms fuse after first touch | 1× | `arms.rs:572-594` |
| Ion Cannon (15) | projectile, blast | 210 | – | semi; 400 ms → 51 t → 398.4 ms | 5 / 25 | 2400 ms | 0 | 46 → 20 m/s by 15 m, 2.5% gravity | 2× | `arms.rs:595-615` |

- Defaults every gun inherits unless overridden: range 100 m, `falloff: 1000.0` with `falloff_min: 1.0` (i.e. no falloff), reload 2000 ms, `shield_k: 1.0`, `magnet: 0.0`, `bloom: 0.0` — `arms.rs:267-311`.
- `q14(n)` is "1/16384 turn (Fireplace's spread unit) in radians" — `arms.rs:257-260`.
- Swapping guns locks the trigger for `SWAP_MS = 320` ms — `arms.rs:707-708`, `fighter.rs:378-392`. Dual wielding reloads both hands together, 1.5× slower (`consts.rs:264-265`, `fighter.rs:400-405`).
- A reload fills the magazine in one step when its timer ends, for every gun including the shotgun — `fighter.rs:522-538`.
- Gun Game's ladder: Rocket, Shotgun, SMG, AR, Sniper, BR, Magnum — `arms.rs:57-59`.

**Hitscan vs projectile [code]**
- Hitscan (a ray resolved the tick it is fired, lag-compensated): AR, BR, Magnum, Sniper, Shotgun, SMG, Carbine, Beam Rifle — `arms.rs:62-80, 267-270`, `world.rs:887-893, 1158-1192`.
- Projectiles (fly in the present, never rewound): Rocket 24.4 m/s off the tube rising to 61 m/s after 61 m ("Halo 2's: 8 wu/s off the tube to 20 wu/s by 20 wu") — `arms.rs:413-417`; Ion Rifle bolt 64 m/s (`arms.rs:513`); Ion Pistol bolt 50 m/s, charged 23 m/s (`arms.rs:535, 541`); Shard 18.3 m/s (`arms.rs:563`); Launcher 61 → 30 m/s (`arms.rs:586-588`); Cannon 46 → 20 m/s (`arms.rs:608-610`). They spawn 0.4 m ahead of the eye (`world.rs:924`) and clear their owner for their first 16 ticks (`world.rs:1545`). Speed ramp per metre flown — `world.rs:1633-1646`.
- "What is rewound: hitscan bullets and melee. Rockets, frags and any slow projectile fly in the present" — `docs/DESIGN.md:230-231`. Hitscan rewind is capped at 160 ms (`MAX_FAVOUR_MS`, `crates/room/src/lib.rs:48-49`; `docs/DESIGN.md:224`), the pose history is 32 ticks = 250 ms (`consts.rs:41-42`).
- Trades are possible by design: a fighter killed within its own rewind window still fires what it pressed ("its rounds go (a trade: the shooter favoured as far as its rewind, no further)") — `world.rs:728-747`; and "Everyone moves first, then what they fired is resolved against where everyone now is, so seat order never decides a trade" — `world.rs:715-716`.

**Spread, bloom, recoil [code]**
- One function draws every round's direction: "`base` turned off-axis by up to `cone` radians, uniform over the cone's disc, from a draw keyed by the match's salt. A client that knows its own shot counter still can't name the draw" — `world.rs:3663-3685` (`r = cone × sqrt(u)`, line 3673; key is `sip24(salt, slot, shot, pellet)`, line 3670). So spread is *deterministic given the secret salt* but unpredictable to the player; the salt is revealed only when the match is over (`docs/DESIGN.md:306-313`).
- The cone passed in is the gun's fixed `spread` (or `dual_spread` when dual wielding) — `world.rs:881-890`. Nothing in `fire_as` reads velocity, grounded state, crouch, or zoom. The ZOOM button is "Drawing only; the sim ignores it" — `crates/sim/src/input.rs:15-16`.
- Bloom exists only for a hot *turret* gun: `Arm::cone(heat)` opens from `spread` to `bloom` with heat, and only the truck's minigun sets `bloom` — `arms.rs:186-188, 228-235, 635-636`; used only in `fire_turret` (`world.rs:957`).
- There is no recoil in the sim: a search of `crates/sim/src` for "recoil" returns nothing [inferred from absence]. The client's view kick was removed today: "The frame never kicks for our shots or flinches for a hit on us, as Halo's ... the gun in hand still kicks" — commit `97b9606` (2026-11-09).
- The client's own predicted tracer goes down the centre of the reticle; "spread is the server's" — `docs/DESIGN.md:154-155, 313`.

**Damage falloff [code]**
- Hitscan damage is full to `falloff` metres, then linear down to `falloff_min` × damage at `range` — `world.rs:1179-1185`. Only the Shotgun (8 m → 20% at 28 m) and SMG (20 m → 60% at 50 m) set it (`arms.rs:396-398, 436-438`); every other handheld hitscan gun does full damage to its maximum range.

**Headshot rule [code]**
- Three rules: `Head::None` "Just damage", `Head::Unshielded` "Kills once the shields are down", `Head::Always` "Always kills" — `arms.rs:92-101`.
- Applied as `lethal = head && (rule == Always || (rule == Unshielded && v.shield == 0))`, evaluated *before* the hit's damage is applied — `world.rs:2464`. So the round that breaks the shield never kills by itself; a head hit on any shield does plain body damage (there is no head damage multiplier anywhere).
- Unshielded-rule guns: BR, Magnum, Carbine. Always-rule guns: Sniper, Beam Rifle. All others: none — `arms.rs:344, 361, 377, 474, 490`.

**Damage split, shields vs health [code]**
- `damage_k`: shields take `pts × shield_k`; "what gets past them reaches health as itself" — the remainder of a hit that breaks the shield carries into health — `world.rs:2451-2483`.
- `shield_k` by source: all hitscan guns 1.0 (default, `arms.rs:293`); Ion Rifle 2.5 (`arms.rs:510`); Ion Pistol 3.0 (`arms.rs:533`); Shard 1.5 (`arms.rs:560`); shard supercombine 0.5 (`consts.rs:212`); frag and sticky blasts 0.5 (`consts.rs:236-237`); rocket, launcher and cannon blasts 1.0 (`world.rs:1657, 1963-1965`).

**Shots-to-kill and optimal TTK** (full target = 100 shield + 100 health; TTK = first round to the killing round, perfect aim, stationary target)

| Gun | Shielded, last shot to head | Shielded, body only | Shields already down (100 health) | To strip a full shield | Pinned / measured where |
|---|---|---|---|---|---|
| Battle Rifle | 4 bursts (11 rounds: 10 strip, the 11th to the head) — **1289 ms** [measured] | 7 bursts (20 rounds) — **2507 ms** [measured] | head: 1 round; body: 10 rounds, 1219 ms [derived] | 10 rounds, 1219 ms [derived] | `range.rs:277-287` ("Halo 2: four bursts, the last in the head; seven to the body alone ... about 1.3 s, Halo 2's 4-shot") |
| Magnum | 5 shots — **718 ms** [measured] | 8 shots — **1257 ms** [measured] | head: 1; body: 4 shots, 539 ms [derived] | 4 shots, 539 ms [derived] | `range.rs:290-294` |
| Carbine | 7 shots — **984 ms** [measured] | 12 shots — **1804 ms** [measured] | head: 1; body: 6 shots, 820 ms [derived] | 6 shots, 820 ms [derived] | `range.rs:424-430` ("Halo 2's: six strip the shields, the seventh in the head; a second") |
| Sniper | 1 shot | 2 shots — **500 ms** [measured] | 1 | 1 | `range.rs:297-304` ("Halo 2 the same") |
| Beam Rifle | 1 shot | 2 shots — **250 ms** [measured], then a 3.3 s vent | 1 | 1 | `range.rs:433-443` |
| Assault Rifle | no head rule | 25 rounds — **2062 ms** [measured] | 13 rounds, 1031 ms [derived] | 13 rounds, 1031 ms [derived] | `range.rs:307-316` ("The rifle (Halo 3's, there being none in Halo 2): 25 rounds, 2.1 s") |
| SMG | no head rule | 25 rounds — **1687 ms** [measured]; dual, 25 rounds in 700–1000 ms | 13 rounds, 844 ms [derived] | 13 rounds, 844 ms [derived] | `range.rs:307-311, 551-581` ("Halo 2's SMG: 25 rounds at 15 a second, 1.6 s") |
| Shotgun | no head rule | 7 of 10 pellets (203 points) in one shell; 1 shell every 1000 ms | 4 pellets | 4 pellets | `range.rs:330-339` ("Halo 2's: ten pellets, seven kill") |
| Rocket | – | 1 (direct hit, or within about 3.2 m on the floor) | 1 | 1 | `range.rs:342-355` |
| Sword | – | 1 swing or lunge (200 points) | 1 | 1 | `range.rs:363-378` |
| Ion Rifle | no head rule | 18 hits (5 strip, 13 more) — **2398 ms** at 10 m [measured] | 13 bolts, 1594 ms + flight [derived] | 5 bolts, 531 ms + flight [derived] | `range.rs:446-454` |
| Ion Pistol | no head rule | 27 hits (7 strip, 20 more) — **6468 ms** at 8 m [measured] because it overheats 632 ms in | 20 bolts | 7 bolts, 422 ms + flight [derived]; or one charged bolt (600 ms hold) takes every shield | `range.rs:457-464` |
| Shard Gun | – | 8 shards stuck go off together — **1429 ms** at 10 m [measured] | – | – | `range.rs:509-537` |
| Grenade Launcher | – | 3 hits — **757–781 ms** at 10 m [measured] | 2 | 2 | `range.rs:540-546` |
| Ion Cannon | – | 1 direct hit — 429 ms flight at 15 m [measured] | 1 | 1 | `range.rs:540-546` |

**The repo's own range table, as printed today [measured]** (`p` = trigger pulls, `h` = hits, spread column = worst/mean miss distance in cm at that range over 300 draws; `range.rs:738-807`):

```
gun                  m          head          body      spread@m
BATTLE RIFLE        30 4p/11h 1289ms 7p/20h 2507ms     7/5   cm
MAGNUM              20  5p/5h  718ms  8p/8h 1257ms     3/2   cm
SNIPER RIFLE        80  1p/1h    0ms  2p/2h  500ms     0/0   cm
ASSAULT RIFLE       10 25p/25h 2062ms 25p/25h 2062ms    15/10  cm
SMG                  8 32p/25h 2179ms 25p/25h 1687ms    25/17  cm
SHOTGUN              3  2p/7h 1000ms  1p/7h    0ms    30/20  cm
CARBINE             30  7p/7h  984ms 12p/12h 1804ms    16/11  cm
BEAM RIFLE          80  1p/1h    0ms  2p/2h  250ms     0/0   cm
ION RIFLE           10 19p/18h 2398ms 19p/18h 2398ms     9/6   cm
ION PISTOL           8 27p/27h 6468ms 27p/27h 6468ms     0/0   cm
SHARD GUN           10 11p/10h 1429ms 11p/10h 1429ms    17/12  cm
GRENADE LAUNCHER    10  3p/3h  757ms  3p/3h  781ms    10/7   cm
ION CANNON          15  2p/1h  429ms  2p/1h  429ms     0/0   cm
ROCKET LAUNCHER     15  1p/1h  492ms  1p/1h  492ms     0/0   cm

SHOTGUN one shot: 95% to 4.5 m, 50% to 6 m
  3 m 100%  5 m 84%  7 m 30%  9 m 0%  12 m 0%  15 m 0%
ROCKET: 10 m in 351 ms, 20 m in 656 ms, 40 m in 1132 ms; kills to 3.2 m
BEAM RIFLE held: overheats at Some(250) ms
ION RIFLE held: overheats at Some(3054) ms
ION PISTOL held: overheats at Some(632) ms
ION PISTOL charged at 10 m: (shield, health, venting) (0, 25600, true)
SWORD: 1.5 m Some(0) ms, 4 m Some(164) ms, lunges from 6.75 m
```

**Power weapons' blasts [code]**
- Blast damage is whole to `full`, then linear to `edge` at `radius`; nothing through the map; distance is measured to the victim's chest, 0.9 m over the feet — `world.rs:2000-2007, 2020-2027`. Self-damage from one's own rocket, cannon or launcher round is halved (`self_k 0.5`, `world.rs:1657, 2028-2030`).
- Rocket: 230 points, radius 6.0 m, full to 3.0 m, zero at the rim — `arms.rs:410, 418-420`. Launcher: 75, radius 3.5, full 1.0 — `arms.rs:583, 590-591`. Cannon: 210, radius 4.5, full 1.0 — `arms.rs:605, 612-613`.
- Blasts push bodies away at up to `BLAST_PUSH = 9.0` m/s × the falloff factor — `consts.rs:271-272`, `world.rs:2036-2040`. A rocket jump: "half of 230 takes the shields", health −15, lift greater than a jump — `tests/combat.rs:267-298`.

**Bullet magnetism, aim assist, auto-aim, reticle friction**
- `pub const MAGNETISM: bool = false;` with the comment "Bullet magnetism, every gun's at once (`World::magnet`, each gun's `Arm::magnet`) and the red reticle's widening with it. Off: a round goes where the cone sends it, and the reticle is red only on a body." — `consts.rs:103-107`.
- The dormant mechanism: a hitscan round that misses everyone but passes through an enemy's body cylinder widened by `distance × magnet` radians, within the gun's `aim_range`, is bent onto the body between hip (0.45 m) and chest (1.0 m) height, "Never onto the head" — `arms.rs:174-179, 193-210`, `world.rs:1101-1153`, gated at `world.rs:1162`. Per-gun values that would apply: AR 0.3° / 25 m, BR 0.4° / 45 m, Magnum 0.35° / 30 m, SMG 0.3° / 18 m, Carbine 0.4° / 40 m; Sniper, Shotgun and Beam Rifle have none — `arms.rs:317-318, 332-333, 353-354, 425-426, 464-465`.
- The test that covers it is written to pass either way: BR near misses land only "while magnetism is on" (`(BR, MAGNETISM)`), and it returns early `if !MAGNETISM` — `tests/combat.rs:683-705`.
- History: magnetism was reworked and switched off on 2026-11-07: "Bullet magnetism: bend from the whole body widened (as the red reticle draws it), not a ball at the chest; the head never widened; MAGNETISM switch, off for now. The goldens move." — commit `119105f`. The function first appears in history on 2026-11-05 (commit `52f126b`, found with `git log -S'fn magnet'`).
- The red reticle is display only: the client turns the crosshair red when the aim ray meets an enemy body within the gun's `aim_range` ("with `MAGNETISM` off nothing is [widened]") — `crates/client/src/hud.rs:2146-2196`.
- No other aim help exists: a search of `crates/client/src`, `crates/client-net/src`, `crates/room/src`, `crates/server/src` for aim assist, friction, magnet, sticky aim, slowdown, auto-aim finds only the red reticle above and unrelated cloth/truck physics [inferred from absence].
- Pad vs mouse: mouse look is `yaw += dx × 0.022° × sensitivity ÷ zoom` (default sensitivity 3.0, "CS2 / Apex" scale; scoped, "the mouse turns slower by the zoom") — `crates/client/src/lib.rs:1016-1027`, `crates/client/src/settings.rs:24-32`, `crates/client/src/options.rs:229-234`. The only file in `crates/client/src` that references a gamepad is `pad.rs`, whose header says it moves a focus on "the game's screens (the title, the Esc menu, the settings)" — `crates/client/src/pad.rs:1-15`. I found no code that turns a pad's stick into aim in a match, so there is no pad-specific aim help to differ [inferred from absence].
- Homing is the only steering of rounds in the sim: shards and the charged ion bolt lock whoever the aim passed within `HOMING_PAD = 1.0` m of at the shot and turn at 30°/s and 80°/s — `consts.rs:202-204`, `world.rs:905-912, 1600-1615`. The sword and the melee lunge steer the *body* at a target (see melee).

**Starting loadout, pads and pickups [code]**
- Every non-Gun-Game mode's loadout is `[BR, MAGNUM]` (BR in hand) — `world.rs:69, 85, 101, 117`; plus 2 frags and 0 stickies (`consts.rs:244-247`, `fighter.rs:286, 299`). Design: "Equal starts, every life: the rifle (BR) and the magnum, two grenades" — `docs/DESIGN.md:35`.
- A gun on a pad is taken with USE while within 1.0 m in x and z and 1.5 m in height; the pad is then empty for its map-defined `respawn_ms`, counted from the pickup — `consts.rs:279-280`, `world.rs:29-32, 3225-3251`. Pads start full (`Pad::default()` has `empty_for: 0`) — `world.rs:249-253, 562` [inferred from the default]. Taking a gun of a kind already carried refills it (`world.rs:3272-3311`).
- Power-ups and grenade pads are taken by walking on — `arms.rs:41-55`, `world.rs:3422-3468`; a grenade pad gives 2, up to 4 carried (`consts.rs:248-249, 266`).
- The dead drop the gun in hand, the left-hand gun and their grenades; drops lie 30 s; walking over a dropped gun of a kind you carry takes its rounds — `world.rs:2536-2550, 3318-3420`, `consts.rs:286-287`.
- Pad respawn timers in the maps (seconds): frags and stickies 30 on every map; Sniper 120 (Bulkhead `maps/bulkhead/map.json:590`, Pithead `maps/pithead/map.json:113`), 90 (Overpass `:804`, Cloudbreak `maps/cloudbreak/map.json:197`, Proving Grounds `:340`), 60 (Dry Fork `maps/canyon/map.json:187`); Rocket 120 (Overpass `:813`, Cloudbreak `:196`, Proving Grounds `:331`, Dry Fork `:190`); Sword 120 (Bulkhead `:599`, Cistern `maps/cistern/map.json:1371`, Pithead `:114`); Shotgun 45–90 (Dry Fork `:188` 45; Cloudbreak `:198` 60; Pithead `:119` 60; Chrysalis `maps/chrysalis/map.json:967` 90); Beam Rifle 90 (Chrysalis `:966`); Overshield 120 on most, 90 on Cloudbreak (`:199`) and Pithead (`:115`); Camo 120, 90 on Pithead (`:116`); BR, Magnum, Carbine 30–45; Shards 45; Launcher 60 (Dry Fork `:189`). The Range has every gun on a 5 s pad (`maps/range/map.json:68` onward). Default when a map omits it: 30 s — `tools/maps/compile.mjs:471`.
- The Assault Rifle is on a pad only on the Range (`maps/range/map.json:68`; no other `"weapon": "ar"` in `maps/`) and on Gun Game's ladder (`arms.rs:59`).

### Inferences
- [derived] Sustained damage per second with every round landing at full-damage range: Magnum 155.8, SMG 113.8, Carbine 103.6, AR 93.1, **BR 73.8** (30 points per 406 ms cycle), Sniper 240, Shotgun up to 290. The spawn sidearm out-damages the spawn rifle by 2.1× on the body and kills in 56% of the time with a finishing head shot (719 vs 1289 ms). The Magnum's body-only kill (1257 ms) is also quicker than the BR's best case.
- [derived] BR burst timing: rounds leave at 0, 70.3 and 140.6 ms; the code comment says "Halo 2's: 15 rounds a second in a burst" (`arms.rs:336`) but 67 ms rounds *up* to 9 ticks, so in-burst rate is 14.2/s.
- [derived] The BR four-burst kill tolerates exactly one missed round out of 12 (10 hits anywhere, then one head hit with shields at exactly 0).
- [derived] Kills per magazine with perfect aim: BR 3 (36 rounds), Magnum 2 (12), Carbine 2 (18), AR 1 (32), SMG 2 (60), Sniper up to 4.
- [derived] Cone radius at range (uniform over the disc): BR 2.3 cm @ 10 m, 4.6 @ 20 m, 6.9 @ 30 m; Magnum 1.5 / 3.1 / 4.6 cm; Carbine 5.2 / 10.5 / 15.7 cm; AR 15.3 / 30.7 / 46.0 cm; SMG 30.7 / 61.4 / 92.0 cm. The whole AR cone fits inside the 0.35 m body radius out to about 23 m, the SMG's to about 11 m; the BR's fits a 0.22 m head to about 96 m and the Magnum's to about 143 m.
- [inferred] A Sniper or Beam Rifle head shot kills through an overshield (the lethal branch zeroes shield and health whatever they were — `world.rs:2464-2468`); a charged ion bolt removes an entire overshield (`strip` sets shield to 0 — `world.rs:1681-1688`); the sword's 200 points do *not* kill through an overshield.
- [inferred] Because nothing in `fire_as` reads the shooter's motion, running, jumping, crouching and scoping do not change accuracy at all.

### Gaps
- I did not find any recorded player-vs-player accuracy or real-match TTK data in the repo; all TTKs above are perfect-aim, stationary-target figures.
- No evidence in the repo of what magnetism values "felt right" before it was switched off, or why it was switched off beyond the commit message "off for now".

## Shields and health: capacities, recharge, regeneration, headshots through shields, powerups

### Takeaway
100 shield over 100 health; shields start recharging 5.0 s after the last damage and refill in 1.67 s; health also regenerates (30 points/s) once shields are full. Overshield (3× shields) and active camo are pad power-ups.

### Cited Findings
- `SHIELD_MAX = 100`, `HEALTH_MAX = 100` points — `consts.rs:123-126`.
- `SHIELD_DELAY_MS = 5000` (640 ticks); `SHIELD_RATE = 60` points/s "(full in 1.67 s)"; `HEALTH_RATE = 30` points/s, "Health comes back once shields are full" — `consts.rs:127-131`; applied in `World::recover`: if `since_hit ≥ 5 s`, shields climb, else-if shields are full, health climbs — `world.rs:3509-3515`. Any damage (and a charged-bolt strip) resets `since_hit` — `world.rs:2460, 1689`. Test: `shields_come_back_after_five_seconds` — `tests/combat.rs:250-264`.
- Design: "Shields over health. Shields recharge 5 s after the last hit; health follows." — `docs/DESIGN.md:21`.
- Damage order: shields first, the remainder of the same hit into health — `world.rs:2470-2483`.
- Headshots vs shields: see the head rule above (`world.rs:2464`): BR/Magnum/Carbine head hits kill only when the shield is exactly 0 before the hit; Sniper/Beam head hits always kill.
- Overshield: shields climb to `OVERSHIELD_MAX = 3 × 100` at 100 points/s from wherever they were ("health whole at once"), then drain at 5 points/s back to 100 ("40 s from the top"); "Above full, shields never recharge" — `consts.rs:139-146`, `world.rs:3438-3441, 3502-3508`.
- Active camo: 45 s; hidden up to 92%; moving faster than 2.0 m/s starts to show the body, fully at 9.0 m/s; firing, throwing, a melee or taking a hit shows it for 1200 ms — `consts.rs:147-155`, `fighter.rs:218-229`.
- No other power-ups: pad ids are guns plus `OVERSHIELD`, `CAMO`, `FRAGS`, `STICKIES` — `arms.rs:41-55`.
- Respawn restores full shields and health — `fighter.rs:264-277`.

### Inferences
- [derived] From shields broken to shields full again with no further damage: 5.0 + 1.67 = 6.67 s. From 1 health and 0 shield to completely full: 5.0 + 1.67 + 3.3 ≈ 10 s.
- [derived] The shield delay (5 s) is longer than the respawn timer (3 s).
- [derived] Regen per tick is 120/256 point for shields and 60/256 for health (`RATE × 256 / 128`).

### Gaps
- None on the rules themselves. Whether health regeneration is intended to match a particular Halo title is not stated in the code (only "health follows" in the design).

## Movement: speeds, acceleration, strafe reversal, air control, jump, crouch, other mechanics

### Takeaway
"Quake's model on the ground ... at Halo 2's speeds and jump": 6.86 m/s forward, 6.10 m/s back and sideways, full speed from rest in about 156 ms, a strafe reversal reaching zero velocity in about 78 ms and full opposite speed in about 234 ms, and a very high, long jump (apex about 2.76 m, 1.40 s in the air) with weak, speed-capped air control. No sprint, slide, clamber, dash or fall damage.

### Cited Findings
- Model: "Movement: Quake's model on the ground (Fireplace's `PHYS`), restated per second, at Halo 2's speeds and jump" — `consts.rs:44-48`; "Walking, jumping and falling: Quake's model, ported from Fireplace's `$move`" — `movement.rs:1-7`.
- Speeds: `RUN = 6.86` m/s forward, `RUN_BACK = 6.10`, `RUN_SIDE = 6.10`, `CROUCH_RUN = 3.5` — `consts.rs:49-52`. "Halo 2's: full speed ahead, a little less backing up or to the side; between, on the ellipse" — `movement.rs:136-141`. So strafe = backpedal = 88.9% of forward; a forward diagonal is 6.49 m/s (test `backs_up_and_strafes_slower_than_it_runs_ahead`, `movement.rs:418-445`).
- Ground friction: each tick, speed drops by `max(speed, STOP = 2.5) × FRICTION = 6.0 × DT` — `consts.rs:53-55`, `movement.rs:146-153`.
- Ground acceleration: `accelerate(v, wishdir, wish, ACCEL × wish × DT)` with `ACCEL = 10.0`, i.e. up to 10 × the wished speed per second (61 m/s² strafing, 68.6 m/s² forward), then capped so flat speed never exceeds the greater of the speed before and the wish speed ("Quake's accelerate ... but Halo's top speed") — `consts.rs:56`, `movement.rs:61-83, 154`.
- Air: `AIR_ACCEL = 8.0` m/s² toward the stick, same cap — "Halo's air, not Quake's ... nothing in the air makes it faster than it took off or than that run (no strafe jumping, no bunny hops). A jump is a commitment" — `consts.rs:57-62`, `movement.rs:163-165`. Tests: `air_strafing_and_bunny_hops_never_gain_speed` (top < 1.02 × RUN, `movement.rs:484-536`); from a standing jump, 3.0–4.5 m/s after 0.5 s; a running jump pulled back is still above 4.0 m/s after 0.25 s; stick released keeps its speed exactly (`movement.rs:539-576`).
- Jump: `GRAVITY = 11.3` m/s², `JUMP_SPEED = 7.9` m/s; "Halo 2's jump in proportion to our body (it jumps half again its own height and is 1.4 s in the air: Halo 3's measured, the same jump velocity): an apex of v²/2g = 2.76 m, 2v/g = 1.40 s up and down" — `consts.rs:63-68`. Test asserts apex 2.6–2.9 m and 1.35–1.45 s — `movement.rs:386-415`. Jump is taken on any grounded tick the button is *held* (`movement.rs:159-162`), including from a crouch.
- Crouch: walking box 1.3 m, speed 3.5 m/s at once; eye and hit volume move over 125 ms — `consts.rs:74-81`, `movement.rs:93-120, 133-134`. Crouching in the air tucks the feet up `CROUCH_LIFT = 0.5` m ("Halo's crouch jump ... a higher ledge is reached") without slowing — `consts.rs:82-85`, `movement.rs:111-114, 132`.
- Step-up `STEP = 0.5` m; stays grounded over a drop of up to `SNAP = 0.25` m — `consts.rs:86-89`, `movement.rs:176, 249-252`.
- Slopes steeper than 46° cannot be walked and slide the body at 14 m/s² — `consts.rs:90-96`, `movement.rs:258-269`.
- Speed clamp `VMAX = 60` m/s — `consts.rs:69`. Launch pads set the body's velocity outright — `movement.rs:270-280`.
- A flag carrier moves at 85% — `consts.rs:521-523`, `contact.rs:58-61`.
- Not present in the sim: a search of `crates/sim/src` for sprint, slide (as a move), clamber, mantle, dash, fall damage returns nothing [inferred from absence]. Falling only kills below a map's void height (`world.rs:2392-2424`).
- The client sends keys as a full-tilt stick: `mx`/`mz` are i8, ±127 (`input.rs:34-37`, `crates/client/src/lib.rs:1060-1061`).

**Derived timings** [derived: my re-implementation of `movement.rs:146-154` and `68-83` at 128 Hz with the 1/1024 quantisation; not printed anywhere in the repo]

| | Forward (6.86 m/s) | Strafe (6.10 m/s) | Crouched (3.5 m/s) |
|---|---|---|---|
| Standstill to top speed | 20 ticks = 156 ms (0.64 m travelled) | 20 ticks = 156 ms (0.56 m) | 23 ticks = 180 ms |
| 50% / 90% of top speed | 62 ms / 133 ms | 70 ms / 133 ms | 86 ms / 156 ms |
| Stick released, to a stop | 43 ticks = 336 ms, 0.89 m slide | 40 ticks = 312 ms, 0.77 m slide | 29 ticks = 227 ms |
| Stick reversed: time to zero velocity | 10 ticks = 78 ms (0.21 m overshoot) | **10 ticks = 78 ms (0.19 m overshoot)** | 9 ticks = 70 ms |
| Stick reversed: back through the reversal point | 164 ms | 164 ms | 164 ms |
| Stick reversed: 90% opposite speed | 211 ms | 211 ms | 227 ms |
| Stick reversed: full opposite speed | 30 ticks = 234 ms | **30 ticks = 234 ms** | 32 ticks = 250 ms |

- [derived] Jump: analytic apex 7.9²/(2 × 11.3) = 2.76 m (my discrete re-run gives 2.79 m), airtime 1.40 s; a running jump carries 9.6 m forward (8.5 m sideways). A crouch jump clears about 3.26 m of ledge.
- [derived] Air control: from a standing jump the stick reaches full run speed only after about 1.0 s; a 6.1 m/s strafe pulled back in the air takes about 766 ms to reach zero.

### Inferences
- [derived] The jump apex is 1.53 × the 1.8 m body height, and gravity (11.3 m/s²) is 1.15 × Earth's, so the height comes from a 7.9 m/s take-off. For 1.4 s a jumper is on a nearly fixed arc (air acceleration is 8 m/s², against 61–69 m/s² on the ground).
- [derived] Reversing a strafe is far quicker than stopping by letting go (78 ms to zero vs 312 ms), since release relies on friction alone.
- [inferred] There is no movement state that changes weapon accuracy, so strafing and jumping carry no shooting penalty for the mover.

### Gaps
- The code gives no source measurement for Halo 2's ground acceleration or friction; `ACCEL`, `FRICTION` and `STOP` are described as Quake's / Fireplace's (`consts.rs:44`), only the top speeds and the jump are attributed to Halo.

## Melee and grenades

### Takeaway
A melee does 70 of a target's 200 points (three to kill; a hit from behind kills), with a short Halo-2-style lunge from up to 3 m and an 800 ms cooldown. Frags do at most 160, halved on shields, so a single frag cannot kill a full-shield fighter; stickies kill whoever they stick to.

### Cited Findings
**Melee [code]**
- `MELEE_DMG = 70`, `MELEE_RANGE = 2.0` m, `MELEE_COOL_MS = 800` (102 ticks = 797 ms) — `consts.rs:173-175`. Test: `melee_is_three_to_kill` — `tests/range.rs:357-360`.
- A melee is a ray from the eye along the aim, 2.0 m long, against hit volumes grown by 0.25 m, rewound like a shot — `world.rs:1202-1211`. It also locks the gun for `MELEE_COOL_MS / 2` = 400 ms and cancels a reload or burst; dual wielding, it drops the left gun — `fighter.rs:426-434`.
- Lunge: "Halo 2's melee lunge: a melee with the aim (the body grown by `MELEE_LUNGE_PAD`) on someone past `MELEE_RANGE` but within `MELEE_LUNGE_RANGE` drives the body at them at `MELEE_LUNGE_SPEED`, for up to `MELEE_LUNGE_MS`, and lands the blow on reaching `STRIKE_R`": range 3.0 m, pad 0.4 m, speed 12.0 m/s, 250 ms, strike radius 1.2 m feet to feet — `consts.rs:176-183, 197-198`, `world.rs:1225-1234, 1288-1345`. Never at a teammate (`lock`, `world.rs:994-997, 1228`). Test: lands from 2.8 m, not from 4.0 m — `tests/combat.rs:655-680`.
- Back-smack: "A hit within this cosine of straight behind the victim kills", `BEHIND_COS = 0.5` (within 60° of directly behind, by the victim's facing) — `consts.rs:184-185`, `world.rs:1237-1243`. It deals `KILL_PTS = HEALTH_MAX + OVERSHIELD_MAX` = 400 points, so it kills through an overshield — `world.rs:24-26`.
- Against shields: plain damage with carry-over into health, no special shield rule — `world.rs:1248, 2470-2483`. A flag carrier's melee kills (`world.rs:1244-1246`).
- Sword: a pull kills at once within `SWORD_REACH = 2.0` m; from there to `LUNGE_RANGE = 6.0` m (body grown by 0.5 m) the body is driven at the target at `LUNGE_SPEED = 18.0` m/s for up to `LUNGE_MS = 400` ms; two swords lunging at each other clash and both are thrown back at 8 m/s unhurt — `consts.rs:187-200`, `world.rs:1252-1284, 1359-1385`. Measured: 4 m lunge kills in 164 ms; lunges from 6.75 m feet to feet — range table above, pinned at `range.rs:363-378`. Sword refire 560 ms (`arms.rs:450`).

**Grenades [code]**
- Two kinds, frag and sticky; start with 2 frags and 0 stickies, carry at most 4 of each; a grenade pad gives 2 — `consts.rs:244-249, 266`. **Each kill also gives the killer one frag** (`k.grenades + 1`, capped at 4) — `world.rs:2560, 2572`.
- Throw: the grenade leaves the hand `GREN_RELEASE_MS = 170` ms after the press ("Ours: the sources give no number"); `GREN_COOL_MS = 600` ms between throws; no throw while dual wielding, carrying a flag or riding — `consts.rs:238-240, 267`, `fighter.rs:420-461`.
- Velocity: aim direction × `GREN_SPEED = 30.5` m/s plus 2.0 m/s straight up; full gravity ("Grenades fall at it whole (Halo's '100% gravity')") — `consts.rs:66, 235`, `world.rs:1389-1390, 1880`. "thrown at 10 wu/s" — `consts.rs:225`.
- Bounce: keeps 45% of its speed into a surface and 70% along it — `consts.rs:268-270`, `world.rs:1738-1766`.
- Frag fuse: goes off `GREN_FUSE_MS = 500` ms "after it first comes down on a floor (not a wall), never sooner than `GREN_ARM_MS` [1300 ms] from the throw, and at `GREN_LIFE_MS` [5000 ms] if it never does" — `consts.rs:223-234`, `world.rs:203-207, 1878-1883`. Test: `tests/combat.rs:329`.
- Frag blast: "A frag, as Halo 2's multiplayer throws it (Halopedia's H2 weapon data, at 3.048 m a world unit): 160 out to 0.5 wu, down to 50 at 1.75 wu, half that on shields": `GREN_DMG = 160`, full to `GREN_FULL = 1.5` m, down to `GREN_EDGE = 50` at `GREN_RADIUS = 5.3` m, `GREN_SHIELD_K = 0.5` — `consts.rs:223-237`, `world.rs:1884-1896`. Test: "160 is 80 off a shield of 100 ... the shields took it all" — `tests/combat.rs:1269-1285`.
- Sticky: "sticks to the first thing it touches, a body, a hull or the map (Halo 2's bounces off the map; here it never does), and goes off `STICKY_FUSE_MS` [900 ms] after it sticks, never before `STICKY_ARM_MS` [1300 ms] from the throw ... the one it is stuck to dies whatever"; blast 200, full to 1.5 m, 50 at the 5.3 m rim, half on shields — `consts.rs:250-263`, `world.rs:1423-1499`. Pinned: a stuck target dies 1280–1500 ms after the throw — `range.rs:676-684`.
- Chain: a blast sets off grenades in its reach 70 ms later, including ones lying on the floor — `consts.rs:241-243`, `world.rs:2104-2142`.
- Blast push on bodies 9 m/s at full strength — `consts.rs:271-272`.

### Inferences
- [derived] Melee as a finisher: a fighter with shields down and full health survives one melee (100 − 70 = 30 health). With carry-over, two melees leave 60 health; the third kills. Three swings at an 800 ms cooldown take about 1.6 s.
- [derived] BR + melee: bursts of 30 plus a 70-point melee total 200 only after five bursts (150 + 70 = 220); after four body bursts (120) a melee leaves 10 health.
- [derived] Frag vs a full fighter: a point-blank frag removes 80 shield and no health; a second one then kills (80 shield-points used up 20 remaining shield, 120 reaches health). With shields already down, one frag inside 1.5 m kills (160 ≥ 100).
- [inferred] Sticky blast vs a full fighter *not* stuck: 200 × 0.5 = 100 on shields removes the shield exactly and, by the `damage_k` arithmetic (`world.rs:2470-2483`), nothing reaches health. The constant's comment says "Its blast kills to `STICKY_FULL`" (`consts.rs:253-254`); I found no test of the un-stuck case, so this is my reading of the code, not a pinned fact.
- [derived] Shortest press-to-blast time for a frag: 170 ms release + 1300 ms arming = 1.47 s. A flat throw from eye height first touches the floor about 0.74 s and about 22 m out.
- [derived] Melee reach in feet-to-feet terms is about 2.0 m + 0.60 m padded radius ≈ 2.6 m straight on; the lunge triggers out to about 3.0 m + 0.75 m ≈ 3.75 m and covers at most 12 m/s × 250 ms = 3.0 m.

### Gaps
- No test pins what a sticky's blast does to a bystander with full shields; needs a one-line check to confirm the inference above.

## Derived ratios for feel: strafing vs hitbox, TTK vs getting to cover, respawn and spawns

### Takeaway
At the repo's own tuning ranges (BR 30 m, Magnum 20 m, AR 10 m) and on arenas 46–64 m long, a strafing fighter moves 8.7 body-widths (13.9 head-widths) per second and crosses its own body width in 115 ms — less than one BR burst (141 ms). A perfect BR kill takes long enough for the target to run about 8–9 m. Respawn is 3 s with no spawn protection.

### Cited Findings
- Strafe speed 6.10 m/s (`consts.rs:51`); body hit width 0.70 m, head 0.44 m (`consts.rs:97, 100`).
- Ranges the repo itself measures guns at: BR 30 m, Magnum 20 m, Sniper 80 m, AR 10 m, SMG 8 m, Shotgun 3 m, Carbine 30 m — `tests/range.rs:745-760`. Bots hold 6–14 m (`bot.rs:751-758`).
- Map footprints (`bounds`, metres): Bulkhead 48 × 32 (`maps/bulkhead/map.json:5`), Cistern 46 × 46 (`maps/cistern/map.json:5`), Proving Grounds 48 × 48 (`maps/proving_grounds/map.json:5`), Pithead 48 × 34 (`maps/pithead/map.json:7`), Chrysalis 58 × 52 (`maps/chrysalis/map.json:5`), Cloudbreak 62 × 55 (`maps/cloudbreak/map.json:6`), Overpass 64 × 44 (`maps/overpass/map.json:5`), Dry Fork 290 × 156 (`maps/canyon/map.json:6`; "a map of 4,000 m² or more is a Big Team map; the rest are arenas" — `docs/DESIGN.md:574-575`).
- Respawn: `RESPAWN_MS = 3000` (384 ticks) — `consts.rs:132`, `world.rs:2532`. Match countdown 3000 ms — `consts.rs:278`.
- Spawn selection: among the spawns the side may use, the one "whose nearest living enemy is farthest, counted only to `SPAWN_FAR_M` [30 m]; one a living enemy can see counts `SPAWN_SEEN_M` [25 m] less, one with a living teammate within `SPAWN_MATE_R` [15 m] counts `SPAWN_MATE_M` [10 m] more (Halo's: back beside your side); and each draws up to `SPAWN_JITTER_M` [3 m] more" — `consts.rs:160-171`, `world.rs:3518-3554`.
- Spawn state: full shields and health, the loadout, 2 frags, all timers zero — `fighter.rs:264-305`. No invulnerability or spawn-protection field exists on `Fighter` and a search of `crates/sim/src` for "invuln"/"protect" finds nothing [inferred from absence].
- Scoring: Duel first to 15 or 10 min; Doubles first to 30 or 12 min, friendly fire on; FFA (8 seats) first to 25 or 10 min; Big Team 8v8 first to 50 or 15 min — `world.rs:63-124`. A suicide or team kill costs a point — `world.rs:2562-2569`.

### Inferences
- [derived] Strafing target, hitbox-widths per second: 6.10 / 0.70 = **8.7 body-widths/s**; 6.10 / 0.44 = **13.9 head-widths/s**. Time to cross its own width: body 115 ms, head 72 ms. Running forward across the view: 9.8 body-widths/s.
- [derived] Angular picture for a full-speed strafer:

| Range | Strafe angular speed | Body width | Head width |
|---|---|---|---|
| 5 m | 69.9°/s | 8.02° | 5.04° |
| 10 m | 35.0°/s | 4.01° | 2.52° |
| 20 m | 17.5°/s | 2.01° | 1.26° |
| 30 m | 11.7°/s | 1.34° | 0.84° |
| 40 m | 8.7°/s | 1.00° | 0.63° |

- [derived] One BR burst spans 140.6 ms; a full-speed strafer moves 0.86 m in that time = 1.23 body-widths (1.95 head-widths). Without tracking during the burst, all three rounds cannot land on a strafing target, and nothing in the sim bends a near miss (magnetism off).
- [derived] A strafe reversal reaches zero velocity in 78 ms and full opposite speed in 234 ms; so a target can go from full speed one way to full speed the other within 0.6 of a BR burst cycle (406 ms).
- [derived] Distance a target can cover during a perfect kill (strafing at 6.10 / running at 6.86 m/s): Magnum head 0.72 s → 4.4 / 4.9 m; Carbine head 0.98 s → 6.0 / 6.8 m; BR head 1.29 s → 7.9 / 8.8 m; SMG 1.69 s → 10.3 / 11.6 m; AR 2.06 s → 12.6 / 14.1 m; BR body 2.51 s → 15.3 / 17.2 m. On arenas 32–55 m across, a BR kill therefore lasts long enough to cross roughly a sixth to a quarter of the map.
- [derived] Spawn-to-spawn separation on the arenas, computed from the spawn lists in each `map.json` (all pairs, mirrored): medians 35–54 m, maxima 44–61 m; Dry Fork median about 181 m.
- [derived] After breaking line of sight, full shields are back 6.67 s later; a killed player is back in 3.0 s (plus travel).

### Gaps
- Typical sightline lengths and cover spacing per map are not recorded in the repo. `oneshot-flow` computes "lines of sight, median / 90th" per map (`tools/flow/src/main.rs:919-964`) but writes to `traces/`, which is not checked in, and running it was outside this audit's constraints. So "time to reach cover" could only be expressed as distance covered during a TTK, not against measured cover gaps.

## Vehicles (brief): weapons and damage

### Takeaway
Three hulls on Dry Fork only: a truck (optionally with a hitscan minigun for the passenger), a hover bike with twin hitscan "bolt guns", and a flier with wing guns and a dropped ion-cannon bomb. Each vehicle gun kills a fighter in roughly 1.6–1.8 s on target; being run down at 5 m/s closing speed or more is lethal.

### Cited Findings
- Minigun (truck bed, passenger): hitscan, 7 points, 62 ms → 8 ticks (16/s), spin-up 350 ms, cone 0.55° cold opening to 2.64° at full heat, full damage to 45 m then down to 40% at 130 m, 3% heat a round, cools 20%/s, vents 40%/s, ×0.5 on hulls; "Halo's chaingun ... About two seconds on target kill" — `arms.rs:618-651`.
- Skimmer "bolt guns" (pilot): hitscan (no `kind` override), 10 points, 95 ms → 12 ticks (10.67/s), cone 0.66°, full to 40 m then to 50% at 110 m, ×0.6 on hulls, cold while boosting — `arms.rs:653-671`, `fighter.rs:343-345`.
- Flier wing guns: hitscan, 12 points, 105 ms → 13 ticks (9.85/s), cone 0.77°, full to 45 m then 50% at 120 m; bomb = an Ion Cannon round (210, radius 4.5 m) at most every 2600 ms — `arms.rs:673-689`, `consts.rs:504-506`, `world.rs:961-992`.
- Hull health: truck 900, skimmer 450, flier 500 points — `consts.rs:375-376, 448, 496`. Top speeds: truck 21 m/s, skimmer 17 (27 boosting), flier 19 (32 boosting) — `consts.rs:355, 436-437, 469-470`.
- Run-down: closing at `SPLAT_KILL = 5.0` m/s or more kills (400 points); between `SPLAT_SPEED = 2.5` and 5.0 it deals that share of 200 — `consts.rs:380-386`, `world.rs:3717-3728`.
- A wreck's blast: 150 points, radius 5.0, full to 1.0 m — `consts.rs:273-276`.
- Vehicles are on Dry Fork only (two trucks, one with a minigun, a skimmer and a flier per side) — `maps/canyon/map.json` `vehicles`; "No vehicles on ranked maps" — `docs/DESIGN.md:37`.

### Inferences
- [derived] Time on target to kill a full fighter: minigun 29 rounds → 1750 ms after the 352 ms spin-up; skimmer 20 rounds → 1781 ms; flier 17 rounds → 1625 ms.

### Gaps
- Not examined further (driving physics, seats, boarding) as out of scope for gunfights on foot.

## Bots: aim, reaction, error, strafing, difficulty

### Takeaway
There is one live difficulty: fill-bots at skill 0.35 (0.25 in Siege). They read the true world, need about 480 ms on a target before firing (the timer restarts whenever sight is lost), aim at the chest (never the head below skill 0.7) with a drifting wobble of roughly ±1.4° and an aim that trails a target's change of direction, and strafe at full speed in straight legs of 0.5–1.5 s.

### Cited Findings
- "A simple AI fighter that reads the true world, for the server's empty seats and for the determinism gate ... Fireplace's `client/arena-bot.mjs` is the fuller one to port next" — `crates/sim/src/bot.rs:1-9`.
- Skill: `BOT_SKILL = 0.35` "beatable, so a person new to the game wins some fights against them"; `SIEGE_BOT_SKILL = 0.25` — `consts.rs:586-592`; assigned to every bot seat in `crates/room/src/lib.rs:726-731`. No per-room or per-player difficulty setting was found. (The determinism match uses 0.3–0.9 by seat — `bot.rs:900`.)
- Perception: looks every 4th tick (31 ms), staggered — `bot.rs:84-86, 541`. It notices an enemy only if it is its current target, or it was hurt within `BOT_HURT_MS = 2000` ms, or the enemy is within `BOT_HEAR_M = 4.0` m, or within its flat view cone `BOT_VIEW_COS = 0.5` (±60°); and only with clear line of sight to the chest; picks the nearest — `consts.rs:593-599`, `bot.rs:399-423`. A camouflaged enemy hidden more than 60% is not seen beyond 12 m (`consts.rs:156-159`, `bot.rs:418`).
- Reaction: fires only when `seen > react`, `react = 128 × (0.6 − 0.35 × skill)` ticks — `bot.rs:763-764`. `seen` is reset to 0 whenever the chosen target changes, including to "nobody" when sight is lost — `bot.rs:438-445`.
- Fire gate: `on = |yaw error| < 0.006 + 0.01 × (1 − skill)` turns and `seen > react`; pitch error is not part of the gate — `bot.rs:764`. Then it fires if within the gun's range (pellet guns: 1.5 × the full-damage reach) — `bot.rs:765-781`. A semi-automatic is re-pulled every other tick (`bot.rs:772-775`).
- Turn rate: at most `(AIM_TURN 0.6 + AIM_TURN_SKILL 1.4 × skill)` turns per second on each axis — `bot.rs:103-108, 714-723`.
- Error model 1, wobble: each tick `wob = wob × 0.98 + (U − 0.5) × 0.004 × (1 − skill)` turns in yaw, half that in pitch, added to the aim point — `bot.rs:109, 607-609, 711-713`.
- Error model 2, trailing aim: the bot aims where the target would be had it kept its earlier velocity: `caught` follows the target's velocity with a 0.25 s time constant, and the aim point is offset by `(vel − caught) × 0.3 s × (1 − skill)` — "A person's aim follows a target's going, and a change in it (a strafe turned) throws the aim off until it has caught up" — `bot.rs:110-111, 700-707`.
- Aim height: 1.2 m above the feet (chest) unless `skill > 0.7`, then 1.55 m (head) — `bot.rs:707`.
- Movement in a fight: full strafe (`mx = ±127`); forward if the target is beyond 14 m (or with a sword), backward at −100 if nearer than 6 m, otherwise no forward/back — `bot.rs:751-759`. Strafe direction is re-drawn 50/50 every 64–191 ticks (0.5–1.49 s) — `bot.rs:602-606`. Jumps with probability 0.004 per tick — `bot.rs:623-625`. No crouch input is ever set in `decide` [inferred from reading `bot.rs:521-789`].
- Other actions: melee whenever the target is within 2.0 m (`bot.rs:782-784`); a grenade with probability 0.003 per tick while on target at 8–20 m (`bot.rs:785-787`); reloads on empty, swaps when out (`bot.rs:611-622`); goes for higher-`rank` guns and power-ups within 20 m on its level (`bot.rs:96-99, 177-227`).
- Recent changes: 2026-11-07 "Gunplay feel: bots' aim trails a target's change of going and turns at a human rate, a shotgun bot fires only where it kills" (commit `1b9cc17`); 2026-11-08 "Fill-bots weaker: skill 0.35 and they notice only who is in front, near or just shot them" (commit `5cfd793`); 2026-11-08 "a bot looks around every fourth tick, staggered" (commit `943eca7`).
- `tools/aim/src/hand.rs` (a human-like "hand" on the bots' aim) is used only by the anti-cheat data tool; `crates/room` and `crates/server` do not reference it [inferred from a search].

### Inferences
- [derived] At skill 0.35: reaction = 61 ticks, first shot from the 62nd tick ≈ **484 ms** after acquiring; turn rate 1.09 turns/s = **392°/s**; fire gate ±0.0125 turn = **±4.5° of yaw**. At Siege's 0.25: about 516 ms, 342°/s, ±4.9°.
- [derived] Wobble steady state (AR(1), φ = 0.98, uniform noise ±0.0013 turn at skill 0.35): standard deviation ≈ 0.0038 turn ≈ **1.36° in yaw**, 0.68° in pitch. That is about 0.24 m at 10 m, 0.47 m at 20 m and 0.71 m at 30 m sideways, against a 0.35 m body half-width — an approximation, since the bot also slews toward the wobbling point at a finite rate.
- [derived] Trailing aim at skill 0.35 weights a velocity change by 0.195 s: an instantaneous 12.2 m/s strafe reversal would put the aim up to 2.4 m off; in practice the reversal itself takes 234 ms and the catch-up constant is 250 ms, so the peak error is smaller (order of 1 m) but still several body widths.
- [derived] Because fill-bots aim at the chest, their best case with a BR is the body count: 7 bursts, at least 2.5 s of every round landing; with a Magnum 8 shots, at least 1.26 s.
- [derived] Mean time between a bot's actual strafe reversals is about 2 s (a re-draw every ≈1.0 s on average, half of which keep the direction), so bots present long constant-velocity strafes of roughly 3–9 m.
- [inferred] Breaking line of sight for one look tick (31 ms) resets a bot's 484 ms reaction timer, so peeking repeatedly keeps a bot from firing.

### Gaps
- No recorded bot accuracy, kill/death or time-to-kill statistics were found in the repo; the feel of bot fights is inferred from the model above, not measured.

## Design of record vs implementation, and recent combat tuning history

### Takeaway
The design doc describes the combat rules in prose (Halo 2 counts as the baseline, 128 Hz, 5 s shield delay, 3 s respawn, 160 ms lag favour) and the code matches on those; it contains almost no combat numbers of its own. Differences found: magnetism is implied by the design but off in code, and the vehicle run-down speed. Combat was reworked heavily between 2026-11-04 and 2026-11-09.

### Cited Findings
- Matches: tick 128 Hz (`docs/DESIGN.md:93` vs `consts.rs:6`); shields recharge 5 s after the last hit, health follows (`DESIGN.md:21` vs `consts.rs:127-131`); equal starts of a rifle, a sidearm and two frags (`DESIGN.md:22, 35` vs `world.rs:69`, `consts.rs:244`); respawn after 3 s away from enemies (`DESIGN.md:30` vs `consts.rs:132`, `world.rs:3518-3554`); at most 160 ms of lag favour (`DESIGN.md:224` vs `crates/room/src/lib.rs:49`); salted spread (`DESIGN.md:306-309` vs `world.rs:3663-3685`); friendly fire in Doubles (`DESIGN.md:36` vs `world.rs:88`).
- Difference 1, magnetism: the design's only mention is "Nobody homes on, lunges at or is magnetised toward a teammate" (`docs/DESIGN.md:36`), which presumes magnetism exists; in code `MAGNETISM = false` (`consts.rs:107`).
- Difference 2, vehicle run-down: design says "past 11 m/s it runs down whoever is on foot in its way" (`docs/DESIGN.md:585`); code kills from 5.0 m/s closing and hurts from 2.5 (`consts.rs:384-385`), changed by commit `11073f5` (2026-11-07).
- Difference 3, pad timers: design says "power weapons sit on fixed pads with fixed timers per map" (`DESIGN.md:35`); in code each pad's timer starts when its item is taken, not on a match clock (`world.rs:3250, 3459`).
- The design gives no numbers for run speed, jump, melee, grenade damage, spread or TTK; it defers to `sim/tests/range.rs` (`DESIGN.md:24`).
- Recent combat-relevant commits (newest first; from `git log`):
  - `97b9606` 2026-11-09 — client: "The frame never kicks for our shots or flinches for a hit on us, as Halo's: the crosshair is drawn at its middle and aim never moved with it, so each round put it up to a degree off where the rounds went".
  - `5cfd793` 2026-11-08 — "Fill-bots weaker: skill 0.35 and they notice only who is in front, near or just shot them".
  - `943eca7` 2026-11-08 — "a bot looks around every fourth tick, staggered".
  - `8b8dd1f` 2026-11-08 — the skimmer and flier with bolt guns and a bomb.
  - `119105f` 2026-11-07 — "Bullet magnetism ... MAGNETISM switch, off for now."
  - `e3e4a85` 2026-11-07 — lag compensation per target (shots, magnetism and run-downs rewind each target by its share).
  - `1b9cc17` 2026-11-07 — "Gunplay feel: bots' aim trails a target's change of going and turns at a human rate ... the view 90° across by default".
  - `74b08a7` 2026-11-07 — shard supercombine "as Halo 2's tag has it".
  - `11073f5` 2026-11-07 — run-down kills from 5 m/s closing.
  - `46be74d` 2026-11-06 — "Grenades as Halo's ... grenades take half off shields ... two frags and no stickies at a spawn".
  - `97e84c1` 2026-11-05 — "A crouch takes a moment: the eye and hit volume go over 125 ms".
  - `f4c6ca4` 2026-11-05 — "Movement and aim to Halo's: no air-strafe gain, jump from a crouch, shots rewound per input" (air 8 m/s², speed cap on accelerate).
  - `dfa87d7` 2026-11-05 — "Halo 2's scopes and mags: snipers 5x/10x, no magnum scope, unscope on a hit".
  - `73f0c0c` 2026-11-05 — camo and overshields.
  - `6169f2f` 2026-11-04 — "Halo 2's movement and grenades: slower backing up and strafing, its high floaty jump, crouch jumps, frags thrown at 10 wu/s that go off half a second after coming down".

### Inferences
- [inferred] The whole combat model is five to seven days old and has been moved toward "Halo 2's numbers" in several passes; the one Halo-family ingredient that was added and then deliberately disabled within that week is bullet magnetism.

### Gaps
- Commit messages do not record *why* magnetism was turned off or whether playtests preceded it.

## Things that look off (my observations, not facts from the repo)

### Takeaway
These are my readings of where the implemented rules look unusual, internally inconsistent or placeholder-like for a Halo-style shooter. Each is an observation built on the cited facts above; whether it is actually wrong is for the comparison with other games to decide.

### Cited Findings
- (Facts these observations rest on are cited in the sections above; the key references are repeated per item below.)

### Inferences
1. **Halo's counts without Halo's aim help.** Shots-to-kill and times are explicitly Halo 2's (`arms.rs:5-8`, `range.rs:7-10`), but bullet magnetism is off (`consts.rs:107`) and there is no other aim assistance of any kind. Hitting a 0.70 m body or 0.44 m head with a 0.09–0.13° cone is entirely on the mouse. If the source game's times assume assisted aim, the same counts will play slower and harder here. The range table's TTKs are also perfect-aim numbers on a stationary target (`range.rs:1-5`).
2. **A burst that outlasts the target's width.** The BR's three rounds span 141 ms; a strafer crosses its own body in 115 ms and its head in 72 ms; a reversal reaches zero speed in 78 ms [derived]. With no magnetism, the 4-burst kill needs 11 of 12 rounds on a moving target with the last on the head.
3. **The sidearm beats the rifle.** Magnum: 5-shot, 719 ms, 156 points/s, tighter cone (0.088° vs 0.132°), no falloff to 110 m. BR: 4 bursts, 1289 ms, 74 points/s (`arms.rs:330-368`; measured table). The Carbine (984 ms) also beats the BR. The only things the BR has over the Magnum are a 2× scope (display only) and magazine size.
4. **Melee is weak as a finisher.** 70 points: three to kill, and it cannot finish an unshielded full-health fighter (`consts.rs:173`, `range.rs:357-360`). The test pins "three to kill" as intended.
5. **Explosives vs shields.** A frag does at most 80 to shields and nothing to health on a full fighter (`tests/combat.rs:1269-1285`); an un-stuck sticky's blast appears to strip shields exactly and do no health damage, despite the comment "Its blast kills to `STICKY_FULL`" (`consts.rs:253-254`) [inferred]. Rockets, by contrast, are *not* halved on shields (`world.rs:1657, 1963-1965`), although the range test's comment describes Halo 2's rocket as "halved on shields" (`range.rs:351-352`).
6. **Jump scale.** Apex 2.76 m (1.53 body heights), 1.40 s airtime, 9.6 m running jump, weak air control (`consts.rs:57-68`). A jumper is a slow, predictable arc for 1.4 s, but with no magnetism still has to be tracked by hand.
7. **Two scaling rules in one body.** Speeds, projectile speeds and blast radii are Halo world units × 3.048 m; the jump is "in proportion to our body" (`consts.rs:45-47, 63-65`); the body itself is 1.8 m × 0.70 m with a 0.44 m head. If Halo's player is not 1.8 m tall at 3.048 m/wu, body-widths-per-second differ from the source game even though m/s match. The repo does not say.
8. **Ground acceleration is Quake's, not attributed to Halo.** 61–69 m/s² (full speed in 156 ms, direction reversed in 234 ms) while releasing the stick takes 312–336 ms to stop (`consts.rs:53-56`). Strafing is very snappy relative to a 1.3–2.5 s rifle TTK.
9. **No accuracy states at all.** No recoil, no bloom on handhelds, no movement, jump or crouch penalty, scope has no effect on the cone (`world.rs:881-890`, `input.rs:15-16`). Automatic guns have a fixed random cone from the first round (AR 0.88°, SMG 1.76°), so tap-firing is no more accurate than holding the trigger.
10. **Spread the player cannot see.** The client's own tracer is drawn down the reticle while the server applies the salted cone (`DESIGN.md:154-155, 313`), so the AR/SMG's misses inside the cone are not what the shooter's own tracer shows at the moment of firing.
11. **Head rule edge.** Head hits on shields get no bonus and the shield-breaking round never kills (`world.rs:2464`); the BR's kill depends on shields reaching exactly 0 on the 10th round (10 × 10 = 100). Sniper and Beam head shots kill through a 300-point overshield, and a charged ion bolt deletes one [inferred from `world.rs:2464-2468, 1681-1688`].
12. **Projectile and hitscan hit volumes differ.** Hitscan: 0.35 m × 1.45 m cylinder plus head; projectiles: 0.45 m × 1.8 m cylinder, no head (`world.rs:1017-1030` vs `1551-1557`).
13. **Comment vs code on the Ion Rifle.** The comment cites Halo 2's "1.5× on shields and 0.35× on health"; the code uses 8 points with `shield_k: 2.5` (`arms.rs:505-510`). The counts (5 + 13) are what is kept.
14. **Rocket kill radius comment.** "A kill to 3.4 m from where it lands" (`arms.rs:418`) vs 3.2 m measured, because distance is taken to the chest 0.9 m up (`world.rs:2020`).
15. **Tick rounding shifts stated rates.** e.g. "15 rounds a second" for the BR burst and SMG becomes 14.2/s (67 ms → 9 ticks = 70.3 ms); the Carbine's 167 ms becomes 164 ms (`consts.rs:12-14`).
16. **A frag for every kill** (`world.rs:2560, 2572`) and the Shotgun reloading its whole 12-shell tube in 2.0 s (`fighter.rs:522-538`) are simplifications I would not expect from "Halo 2's" rules as described elsewhere in the code.
17. **The Assault Rifle is effectively absent** from normal play: not in the loadout and on no pad outside the Range (`world.rs:69`; `maps/range/map.json:68`).
18. **What a tester mostly fights.** Fill-bots never aim for the head, wait about 480 ms (reset by any break in sight), trail reversals, fire on a yaw-only gate and strafe in long straight legs at full speed without crouching (`bot.rs:602-606, 700-707, 751-764`). Fights against them are therefore long body-shot exchanges against a constantly strafing 35°/s target (at 10 m) that the player must track unaided; that may be a large part of "combat feels off" independent of the weapon numbers.
19. **No spawn protection** and a 3 s respawn against a 5 s shield delay (`consts.rs:127, 132`).
20. **Design drift:** run-down speed (11 m/s in the doc vs 5 m/s in code) and magnetism implied by the doc but off (`DESIGN.md:36, 585`; `consts.rs:107, 384-385`).

### Gaps
- None of the above has playtest data behind it in the repo; they are flags for the comparison, not verdicts.
- I could not determine from the repo how a gamepad would aim in a match at all (only menu navigation code references a pad), so any pad-vs-mouse question is open.

# Halo combat reference: concrete tuning and hidden systems (Halo 2 primary; CE, 3, Reach, 5, Infinite as comparisons), as of 2026-11-09

Reading key, used on every line below:

- **[TAG]** = a game-file value (the official Editing Kits' `.biped`, `.weapon`, `.damage_effect`, `.equipment` tags) as transcribed by Halopedia's per-game stat pages. Hard data, but second-hand transcription: re-check a value on the linked page before pinning a test to it.
- **[DEV]** = a developer statement (Bungie, 343 Industries / Halo Studios).
- **[EST]** = a community measurement or estimate (stopwatch, frame count, stat-site scrape).
- **[SNIPPET]** = seen only in a search-result summary; the page itself could not be opened (403/404/blocked host). Lowest confidence.
- Units: **wu** = world unit. 1 wu = 10 ft = 3.048 m ([Bungie Weekly Update 6/20/08](https://halo.bungie.org/bwu/index.html?item=182); [HEK tutorial](https://nikon.bungie.org/misc/hek_tutorial/references/general/halo_player_stats.html)). Original-trilogy tick = 1/30 s.

## 1. Player durability, headshot rules and kill times per game

### Takeaway
From Halo 2 through Halo 5 the player is the same 70 shield + 45 health (115 total, shields 61%), with a 5 s shield delay and 2 s recharge in Halo 2/3; what changed between games was the utility weapon, not the body. Halo 2's BR is 6 damage x 3 rounds x 4 bursts (72 damage, the last burst to the head) for roughly 1.3 s on paper and 1.45 s measured, against 0.6 s for the CE pistol, about 1.6 s for the Halo 3 BR and Reach DMR, and 1.55 s for Infinite's BR75.

### Cited Findings

**Durability (multiplayer biped tags)**

| Game | Health | Shield | Shield delay / full recharge | Health regen (delay / recharge) | Class | Source |
|---|---|---|---|---|---|---|
| Halo CE | 75 | 75 | 6 s / 4 s | none listed (health packs) | [TAG] `cyborg_mp.biped` | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo 2 | 45 | 70 | 5 s / 2 s | 10 s / 5 s (regenerates, no health bar) | [TAG] `masterchief_mp.biped` | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo 3 | 45 | 70 | 5 s / 2 s | 10 s / 5 s | [TAG] | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo: Reach | 45 | 70 | 6 s / 3 s | 4 s / 9 s, only back to the next third (100 / 66.6 / 33.3%) | [TAG] `spartans.biped` | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo 4 | 45 | 70 | 6 s / 2 s | 4 s / 6 s | [TAG] | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo 2: Anniversary MP | 45 | 70 | 5 s / 2 s | 10 s / 5 s | [TAG] | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo 5 | 45 | 70 | 4 s / 2 s | 4 s / 6 s | unofficial tools, "possibly inaccurate" | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo Infinite | 90 | 140 | 4 s / 2 s | 4 s / 2 s | unofficial tools, "possibly inaccurate" | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |

- The Halo 2 campaign Master Chief has 30 health, not 45; the multiplayer biped is a separate tag. [TAG] — [Halopedia: Master Chief (gameplay)](https://www.halopedia.org/Master_Chief_(gameplay))
- Conflict: Halopedia's general shield article gives Halo 2 as 4.25 s delay and 1.75 s to recharge, Halo 3 as 5 s and 2 s, and CE as 5 s and 4 s "at standard game settings" (its CE and Halo 3 point values are marked citation-needed). The tag-cited figures in the table are better supported. — [Halopedia: Energy shielding](https://www.halopedia.org/Energy_shielding); contradicted by [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer))

**Headshot rules**

- Halo CE pistol: 25 damage per shot, x1.5 vs Spartan health; a fully shielded player dies to 3 headshots or 5 body shots; three shots deplete the 75 shield exactly and the third, if a headshot, still kills. [TAG] — [Halopedia: CE:Pistol](https://www.halopedia.org/CE:Pistol)
- Halo 2 BR in multiplayer: "4 bursts (12 rounds) if no bullets miss". — [Halopedia: H2:Battle Rifle](https://www.halopedia.org/H2:Battle_Rifle)
- Halo 2 and 3 BR: four headshot bursts, or three body bursts followed by one to the head, kill a fully shielded player. [SNIPPET] — [Giant Bomb: Four Shot](https://giantbomb.com/wiki/Concepts/Four_Shot)
- Halo Infinite: once the shield is popped, one pistol or BR headshot or one melee kills; a sniper headshot kills through a full shield. — [GamerGuides: Managing and capitalizing on TTK](https://gamerguides.com/halo-infinite/guide/multiplayer/getting-started/managing-and-capitalizing-on-ttk)

**Halo 2 weapon table (multiplayer values where the page separates them; "as of Auto-Update 3 / 1.1a")** [TAG]

| Weapon | Damage | Fire rate / recovery | Magazine (reserve) | Projectile | Error (spread) | Source |
|---|---|---|---|---|---|---|
| Battle Rifle | 6 per round, 3 per burst (18) | 15 rounds/s inside the burst; 0.26 s recovery (~0.266 s per footnote) | 36 (108) | 400 wu/s; instant-hit to ~26.66 wu; max 40 wu | MP 0.1 deg (SP 0.3 to 0.6 deg) | [H2:Battle Rifle](https://www.halopedia.org/H2:Battle_Rifle) |
| Magnum | MP 5.5 (SP 8) | 0.1 s recovery (10/s) | 12 (48) | 400 wu/s; instant to ~26.66 wu; max 40 wu | 0.25 to 1.5 deg | [H2:Magnum](https://www.halopedia.org/H2:Magnum) |
| SMG | MP 4.625 (SP 4.75); dual-wield x0.75 each | 15/s | 60 (180) | 400 wu/s; instant to ~13.33 wu; max 40 wu | single 1 to 2.75 deg; dual 1.5 to 3 deg; recoil 0 to 0.7 deg per shot | [H2:SMG](https://www.halopedia.org/H2:SMG) |
| Covenant Carbine | 11 | 0.14 s recovery (tag ~7.1/s; 6/s after tick rounding; ~6.66/s on Vista) | 18 (72) | 400 wu/s; instant to ~26.66 wu | 0.3 to 0.6 deg | [H2:Covenant Carbine](https://www.halopedia.org/H2:Covenant_Carbine) |
| Sniper Rifle | 45 (x2 vs shields) | 0.5 s recovery (2/s) | 4 (20) | 1200 wu/s; instant to 80 wu; max 120 wu | 0.5 deg unzoomed, 0 zoomed; zoom 3.5x to 9.5x | [H2:Sniper Rifle](https://www.halopedia.org/H2:Sniper_Rifle) |
| Shotgun | 10 pellets x 16.5 (165) inside 5 wu, falling to 5 per pellet (50) by 8 wu | 1 s recovery (1/s) | 12 (36), loaded one shell at a time | 120 wu/s; instant to 4 wu; max 8 wu | 10 deg | [H2:Shotgun](https://www.halopedia.org/H2:Shotgun) |
| Rocket Launcher | direct 150; splash 200 at 1 wu falling to 75 at 2 wu; all x0.5 vs shields | 0.8 s recovery | 2 (6) | 8 wu/s at launch rising to 20 wu/s after 20 wu; turns 45 deg/s when tracking a vehicle | 0 deg; zoom 1.8x | [H2:Rocket Launcher](https://www.halopedia.org/H2:Rocket_Launcher) |
| Plasma Rifle | 10 per bolt; x1.5 vs shields, x0.35 vs Spartan/Elite health; dual x0.7 in MP | tag 6 to 9/s, 7.5/s after tick rounding (~8.6/s on Vista) | battery 400 shots; +15% heat per shot | 21 wu/s when player-fired (14 base x1.5) | single 0.25 to 0.75 deg; dual 1.5 to 3 deg | [H2:Plasma Rifle](https://www.halopedia.org/H2:Plasma_Rifle) |
| Plasma Pistol | bolt 7 (x1.5 shields, x0.35 health); charged bolt 1 damage x500 vs shields (strips them) | bolt 0.05 s recovery (tag 20/s, 15/s on Xbox after tick rounding); charge time 0.6 s | battery 250 bolts or 7 charged shots | bolt 16.5 wu/s; charged 7.5 wu/s, homing at 80 deg/s | — | [H2:Plasma Pistol](https://www.halopedia.org/H2:Plasma_Pistol) |
| Needler | impact 1 per needle (x1.5 shields); supercombine at 8 needles: 400 (x0.5 vs shields) plus splash 50 at 0.2 wu to 30 at 0.5 wu | tag 8/s, 7.5/s effective | 30 (90) | 6 wu/s; homing 15 to 30 deg/s; max 24 wu | single 0.5 to 1.5 deg | [H2:Needler](https://www.halopedia.org/H2:Needler) |
| Brute Shot | impact 20; MP splash 60 at 0.75 wu to 30.5 at 1.5 wu (x0.5 vs shields); detonates 0.2 s after first bounce | 0.3 s recovery (~3.33/s) | 4 (12) | 20 wu/s to 4 wu, slowing to 10 wu/s after 15 wu; 30% gravity | 0.6 deg | [H2:Brute Shot](https://www.halopedia.org/H2:Brute_Shot) |
| Energy Sword | splash 150 within 0.5 wu; battery infinite in MP | — | — | — | — | [H2:Energy Sword](https://www.halopedia.org/H2:Energy_Sword) |

- None of the Halopedia H2 pages lists reload time; the shotgun page only says one round per reload. — [H2:Shotgun](https://www.halopedia.org/H2:Shotgun)

**Other games' utility weapons**

- Halo CE pistol: tag rate 3.5 rounds/s, about 3.33/s after tick rounding; 12-round magazine; 300 wu/s projectile with instant hit to 10 wu; MP error 0.2 to 2 deg that grows only while the trigger is held; 2x zoom. [TAG] — [Halopedia: CE:Pistol](https://www.halopedia.org/CE:Pistol)
- Halo 3 BR: 6 per round, 3 rounds at 15/s, 0.28 s recovery that tick rounding makes 0.3 s; 36 rounds; 180 wu/s; instant hit to 12 wu; max 60 wu; error 0.15 to 0.5 deg; 2x zoom. [TAG] — [Halopedia: H3:Battle Rifle](https://www.halopedia.org/H3:Battle_Rifle)
- Halo 3 sniper: time between shots was raised from 0.5 s to 0.7 s during development. [DEV] — [GDC Vault: Griesemer, Design in Detail](https://gdcvault.com/play/1012211)
- Halo Infinite BR75: 36-round magazine, 3-round burst, 2.5x zoom; body round 130, headshot 600 (in Infinite's larger internal scale); 11 rounds break a full shield; perfect kill is 11 body rounds + 1 head round (4 bursts) in 1.55 s; body-only kill is 16 rounds (6 bursts) in 2.40 s; the Ranked variant kills with two body bursts plus a melee. The page's own figures are not fully consistent with each other (it also states 9 unshielded body shots and a 7-burst kill). — [Halopedia: BR75 battle rifle](https://www.halopedia.org/BR75_battle_rifle)
- Halo Infinite MK50 Sidekick: 12-round magazine; 7 to 10 shots to kill a full-shield Spartan (6 to break shields, then 1 headshot or 4 body shots); bloom grows sharply under rapid fire; no kill time listed. — [Halopedia: Sidekick](https://www.halopedia.org/Sidekick)
- Halo Infinite Bandit: 5 headshots, 1.2 s TTK in pre-release coverage; a July 2024 sandbox update later slowed its rate of fire in response to time-to-kill feedback. [SNIPPET] — [Xboxygen](https://www.xboxygen.com/?p=163409); [r/halo repost of the Waypoint sandbox update](https://reddit.birdcat.cafe/r/halo/comments/1ec224c/sandbox_update_halo_infinite)

**Measured and community kill times**

- Stopwatch timing over many theater films (2010 forum post): CE pistol 0.8 s (3 shots); Halo 2 BR 1.45 s (4 bursts) at 10 m, 25 m and 75 m; Halo 3 BR 1.59 s (4 bursts) at 10 m and 25 m, but 1.99 s (5 bursts, scoped) and 2.385 s (6 bursts, unscoped) at 75 m; Reach DMR 1.67 s (5 shots) at 25 m. The post says Bungie stated the Reach DMR is within a tenth of a second of the Halo 3 BR up close. [EST] — [Bungie.net forum: The Progression of Kill Times in HALO](https://www.bungie.net/en/Forums/Post/2326524?page=0&path=0)
- Halo 4 at launch, frame-based forum arithmetic: DMR about 1.46 s (5 shots), BR about 1.83 s (5 bursts). [EST][SNIPPET] — [343industries.org forum](https://www.343industries.org/forum/topic/10039-brdmrcarbine-kill-times/)
- Halo 5 per the analyst Aozolai (August 2021): AR 1.20 s, Carbine 1.20 s, Magnum 1.30 s, BR 1.40 s, DMR 1.50 s; Halo Infinite technical preview: AR 1.15 s, BR 1.55 s, with the comment that the Infinite BR "needs a buff". The list reads ambiguously in the search rendering, so the weapon-to-number pairing is my reading of it. [EST][SNIPPET] — [Aozolai on X](https://x.com/Aozolai/status/1422895546809798658)
- Reach Title Update with bleed-through: three DMR shots plus a melee kill where vanilla Reach needed four; minimum kill shots are four for the Magnum and six for the Needle Rifle. — [Halopedia: Halo: Reach Title Update](https://www.halopedia.org/Halo:_Reach_Title_Updates)
- Bungie's own verdict on the Halo 2 BR in 2008: "astonishingly lethal. Arguably too lethal." [DEV] — [Bungie Weekly Update 6/20/08](https://halo.bungie.org/bwu/index.html?item=182)

### Inferences
- Halo 2 BR arithmetic from the tags: 12 x 6 = 72 damage against a 70 shield, so shields break only on the twelfth bullet and the kill needs that bullet to be a headshot; any miss or a body-only last burst pushes it to five or more bursts. A clone that gives the BR spare damage (or a smaller shield) loses that knife-edge.
- Halo 2 BR time on paper: if the 0.266 s recovery runs after the 0.133 s burst (three rounds at 15/s), trigger-to-trigger is 0.40 s and the fourth burst starts at 1.20 s and ends at 1.33 s. The stopwatch figure of 1.45 s is consistent with that plus human timing. Halo 3 by the same sum is about 1.43 s on paper and 1.59 s measured.
- CE pistol on paper: two intervals at 3.33/s is 0.6 s; the 0.8 s stopwatch figure is the weaker number (the same post shows identical times at every range, which looks like copying).
- Halo 2 Magnum: 13 body shots at 5.5 are needed to break a 70 shield and the magazine holds 12, so on its own it cannot finish a full-shield player in one magazine without the shield already being down. That is what makes the plasma-pistol "noob combo" (0.6 s charge, a homing bolt worth 500 against shields, then one headshot) the Magnum's real role.
- Halo 2 sniper: 45 damage doubled against shields is 90, so one body shot removes the shield and a second kills 0.5 s later; this is the close-range body-shot strength Griesemer later slowed to 0.7 s in Halo 3.
- Plasma rifle on paper: 5 bolts strip the shield (15 each) and 13 more kill 45 health (3.5 each), 18 bolts at 7.5/s is about 2.3 s. It is a shield-stripper, not a finisher, by design of its two multipliers.
- Carbine on paper: 11 damage at 6 rounds/s means the seventh shot crosses the 70 shield; if the shield-crossing headshot rule that the BR's twelfth bullet relies on also applies, that is a 1.0 s kill, faster than the BR. I found no source stating the Halo 2 carbine's multiplayer shots-to-kill, so treat this as arithmetic only.
- The durable constant is the ratio: shields are 61% of the pool in Halo 2, 3, Reach, 4, 5 (70 of 115) and Infinite (140 of 230), and 50% in CE. A full recovery in Halo 2 takes about 7 s (5 s delay + 2 s recharge) against a 1.3 to 1.45 s perfect kill, about five kill-times.

### Gaps
- Reload times: not on any Halopedia H2 stat page opened; no other source found.
- "Typical real-world average TTK": I found no measured average for any Halo game, only perfect-kill figures.
- Halo: Reach DMR and Halo 5 Magnum tag values (damage, recovery, bloom angles): the Halopedia pages either 404 or carry no stats; only the community times above.
- Halo Infinite numbers come from unofficial tools and a wiki table that contradicts itself; no official 343 stat sheet was found.
- Halo 2 multiplayer shots-to-kill for the SMG, carbine, plasma rifle and needler: not stated on the pages; they depend on material damage modifiers I could not source.
- Halo 2 sword lunge damage and Halo 2 shotgun pellet falloff wording ("down to 5 (range)") are ambiguous on the source page.

## 2. Hitscan versus projectile, projectile speeds, spread, and reception

### Takeaway
No classic Halo weapon is true hitscan: each is a fast projectile that hits instantly inside the distance it covers in its first tick or two, and the feel difference between games is how far that distance is and how much error is added. Halo 2's BR is 400 wu/s with 0.1 degrees of error and client-reported hits; Halo 3's is 180 wu/s with up to 0.38 to 0.5 degrees and host-fired shots, which is exactly what the community called "broken" at range.

### Cited Findings
- Halo 2 BR: 400 wu/s (about 1,219 m/s), instant hit inside ~26.66 wu (about 81 m), max range 40 wu, multiplayer error 0.1 deg. [TAG] — [Halopedia: H2:Battle Rifle](https://www.halopedia.org/H2:Battle_Rifle)
- Halo 3 BR: 180 wu/s (about 549 m/s), instant hit inside 12 wu (about 37 m), max 60 wu, error 0.15 to 0.5 deg. [TAG] — [Halopedia: H3:Battle Rifle](https://www.halopedia.org/H3:Battle_Rifle)
- Bungie (Luke Smith, 20 June 2008): "There are technically no hitscan weapons in Halo 3"; the BR's lower velocity was a design decision "requiring players to lead their targets at range"; inside the distance a burst covers in one frame it behaves as instant-hit. [DEV] — [Bungie Weekly Update 6/20/08](https://halo.bungie.org/bwu/index.html?item=182)
- Bungie's stated Halo 3 BR spread: first bullet 0 to 0.15 deg from the aim vector, third bullet 0 to 0.38 deg, second in between, deliberately, so bullets 2 and 3 fall off outside the intended range; worked example: 0.15 deg over 18 wu is about 0.047 wu of error against a helmet about 0.094 wu wide and a Spartan model 0.75 wu. [DEV] — [Bungie Weekly Update 6/20/08](https://halo.bungie.org/bwu/index.html?item=182)
- Bungie's intent for the Halo 3 BR: "bring the kill range closer"; it is "an anti-long range weapon with a focus on mid-range combat" and "no longer a mid-to-long range killing machine"; the sniper is meant to control spaces about 30 wu away; "no designed one-weapon-to-rule-them-all". [DEV] — [Bungie Weekly Update 6/20/08](https://halo.bungie.org/bwu/index.html?item=182)
- Reception: the fan news archive for that week records the hardcore view that the Halo 3 BR was "'broken' - its bullet spread is too random at long range". [SNIPPET] — [halo.bungie.org news](https://halo.bungie.org/news.html?item=23189)
- The measured cost of that spread: the Halo 3 BR needs 5 bursts scoped (1.99 s) or 6 unscoped (2.385 s) at 75 m where the Halo 2 BR still kills in 4 (1.45 s). [EST] — [Bungie.net forum: The Progression of Kill Times in HALO](https://www.bungie.net/en/Forums/Post/2326524?page=0&path=0)
- The Halo 2 1.1 update "tightened" the BR's three-bullet packet. [DEV] — [Bungie Weekly Update 6/20/08](https://halo.bungie.org/bwu/index.html?item=182)
- Other Halo 2 speeds: sniper 1200 wu/s (instant to 80 wu); Magnum, SMG, carbine 400 wu/s; shotgun 120 wu/s; plasma rifle 21 wu/s; plasma pistol bolt 16.5 wu/s, charged 7.5 wu/s; needler 6 wu/s; brute shot 20 falling to 10 wu/s; rocket 8 rising to 20 wu/s. [TAG] — [H2:Sniper Rifle](https://www.halopedia.org/H2:Sniper_Rifle); [H2:SMG](https://www.halopedia.org/H2:SMG); [H2:Shotgun](https://www.halopedia.org/H2:Shotgun); [H2:Plasma Rifle](https://www.halopedia.org/H2:Plasma_Rifle); [H2:Plasma Pistol](https://www.halopedia.org/H2:Plasma_Pistol); [H2:Needler](https://www.halopedia.org/H2:Needler); [H2:Brute Shot](https://www.halopedia.org/H2:Brute_Shot); [H2:Rocket Launcher](https://www.halopedia.org/H2:Rocket_Launcher)
- Why "instant-hit range" exists: 343's MCC notes explain that in Halo 2 the tick that fires the weapon also moves the projectile as far as it travels in that 1/30 s, and that MCC's doubled tick rate had made Halo 2 projectiles cover only half the expected distance per tick until fixed. [DEV][SNIPPET] — [Halo Support: MCC Update February 2024](https://support.halowaypoint.com/hc/en-us/articles/23707515298324-Halo-The-Master-Chief-Collection-MCC-Update-February-2024)
- Reach bloom and the Title Update: the TU (released 21 September 2011) made bloom configurable and set the standard to 85% of the original; "Zero Bloom" variants removed reticle expansion (some spread remains); matchmaking data showed "strong player preference" for Zero Bloom Slayer, whose playlist weighting was raised on 1 November 2011; Team Slayer got a two-week vanilla-versus-TU vote in February 2012; MLG adopted zero bloom in December 2011. — [Halopedia: Halo: Reach Title Update](https://www.halopedia.org/Halo:_Reach_Title_Updates)
- Weapon spread is applied after auto-aim, so spread or bloom can miss a stationary target under a red reticle. — [Halopedia: Auto-aim](https://www.halopedia.org/Auto-aim)
- Halo Infinite BR75: slight non-progressive recoil, no bloom; the Sidekick has bloom under rapid fire and a "very high, near instantaneous" projectile. — [Halopedia: BR75 battle rifle](https://www.halopedia.org/BR75_battle_rifle); [Halopedia: Sidekick](https://www.halopedia.org/Sidekick)

### Inferences
- The "hitscan range" Halopedia lists is two ticks of travel: 400 wu/s over 2/30 s is 26.67 wu, 180 wu/s is 12 wu, 1200 wu/s is 80 wu. So a simulation at a different tick rate with the same muzzle speed has a different instant-hit distance; this is a number a 60 Hz or 120 Hz clone silently changes.
- At Bungie's own figures the Halo 3 third bullet's worst case at 18 wu is sin(0.38 deg) x 18 = 0.12 wu, wider than the 0.094 wu helmet; at 0.1 deg (Halo 2) the same distance gives 0.03 wu. Halo 2's BR lands all three rounds on a head at ranges where Halo 3's cannot, which is the whole of the "BR spread" argument.
- Halo 2's long reach (40 wu max, 0.1 deg) is why its 4-burst time is flat across 10 m, 25 m and 75 m in the stopwatch data; if a clone's utility rifle slows down with distance, it is behaving like Halo 3 or Reach, not Halo 2.

### Gaps
- Reach DMR bloom angles and decay rate, and Halo Infinite projectile speeds, were not found as numbers.
- No per-weapon hitscan-or-projectile list for Halo 5 or Infinite from an official source.

## 3. Aim assistance: bullet magnetism, reticle friction, ranges, and mouse users

### Takeaway
Halo has two separate systems with per-weapon cones and ranges in the tags: "autoaim" (bullet magnetism, which bends shots and turns the reticle red) and "magnetism" (reticle friction and target-following, controller only). Mouse players get the first and never the second, and in Halo Infinite controllers out-shot mouse by a reported 12 to 16 percent in accuracy at launch.

### Cited Findings

**Halo 2 per-weapon cones (tag values; "scales with zoom" where noted)** [TAG]

| Weapon | Autoaim (bullet magnetism) | Magnetism (sticky aim) | Source |
|---|---|---|---|
| Battle Rifle | 3 deg out to 17 wu (about 52 m), scales with zoom | 6 deg out to 21 wu (about 64 m) | [H2:Battle Rifle](https://www.halopedia.org/H2:Battle_Rifle) |
| Covenant Carbine | 3 deg to 17 wu | 6 deg to 21 wu | [H2:Covenant Carbine](https://www.halopedia.org/H2:Covenant_Carbine) |
| Magnum | 3 deg to 12 wu | 6 deg to 16 wu | [H2:Magnum](https://www.halopedia.org/H2:Magnum) |
| SMG | 6 deg to 12 wu | 12 deg to 18 wu | [H2:SMG](https://www.halopedia.org/H2:SMG) |
| Plasma Rifle | 6 deg to 12 wu | 9 deg to 18 wu | [H2:Plasma Rifle](https://www.halopedia.org/H2:Plasma_Rifle) |
| Plasma Pistol | 6 deg to 12 wu | 9 deg to 16 wu | [H2:Plasma Pistol](https://www.halopedia.org/H2:Plasma_Pistol) |
| Needler | 7 deg to 20 wu | 9 deg to 20 wu | [H2:Needler](https://www.halopedia.org/H2:Needler) |
| Shotgun | 8 deg to 6 wu | 16 deg to 6 wu | [H2:Shotgun](https://www.halopedia.org/H2:Shotgun) |
| Sniper Rifle | 1 deg to 10 wu, zoom only, scales with zoom | 4 deg to 14 wu, zoom only | [H2:Sniper Rifle](https://www.halopedia.org/H2:Sniper_Rifle) |
| Rocket Launcher | 5 deg to 45 wu | 10 deg to 25 wu | [H2:Rocket Launcher](https://www.halopedia.org/H2:Rocket_Launcher) |
| Brute Shot | 4 deg to 15 wu | 6 deg to 25 wu | [H2:Brute Shot](https://www.halopedia.org/H2:Brute_Shot) |
| Energy Sword | 6 deg to 6 wu (blade); 6 deg to 1.5 wu (handle) | 8 deg to 12 wu | [H2:Energy Sword](https://www.halopedia.org/H2:Energy_Sword) |

- Other games: CE pistol autoaim 3 deg to 30 wu and magnetism 6 deg to 30 wu; Halo 3 BR autoaim 3 deg to 17 wu and magnetism 6 deg to 21 wu (identical to Halo 2's). [TAG] — [Halopedia: CE:Pistol](https://www.halopedia.org/CE:Pistol); [Halopedia: H3:Battle Rifle](https://www.halopedia.org/H3:Battle_Rifle)
- How autoaim works: it steers shots toward an enemy near the reticle; a red reticle means it is fully engaged; it often still pulls shots when the reticle is not red; beyond the weapon's range it stops and the weapon "becomes far less effective"; the cone narrows when zoomed. It applies to mouse and keyboard as well as controller. — [Halopedia: Auto-aim](https://www.halopedia.org/Auto-aim)
- How magnetism works: controller only; it slows turning while an enemy is inside a per-weapon radius and subtly moves the camera to follow a strafing target; it weakens with distance and stops beyond a per-weapon range; some games have a weaker outer radius and a stronger inner one. "Magnetism" is the name in the game files, despite the clash with "bullet magnetism". — [Halopedia: Magnetism](https://halopedia.org/Magnetism)
- 343's own description (Halo 5 "The Sprint"): the systems are meant to be subtle so that players feel their accuracy is their own. [DEV] — [Halopedia: Aim assist](https://www.halopedia.org/Aim_assist)
- Halo Infinite accuracy by input, from Halo Tracker data for 100 players per group in input-restricted playlists (article dated 30 November 2021): top-100 controller players were 16.4% more accurate than top-100 mouse players; at Platinum 6 controllers were 12.3% more accurate; the average controller player was within 3.1% of the top-100 mouse players. Raw accuracy values are not given. [EST] — [MeinMMO](https://mein-mmo.de/en/fan-proves-as-controller-players-you-have-a-huge-advantage-in-halo-infinite,756017)
- Halo Infinite's December 2022 update reworked mouse and keyboard input and added red reticles for it, with a walk key and scroll-wheel fixes. [SNIPPET] — [TechRaptor](https://techraptor.net/gaming/news/halo-infinite-december-update-showcases-new-custom-game-browser-and-improved-controls)
- 343's community manager Unyshek, answering a December 2022 complaint that mouse aim had changed: there had been "no changes", and aiming or weapon tuning can only ship in "a content update (a game download, not via a backend services deployment)". [DEV] — [devtrackers.gg](https://devtrackers.gg/halo/p/b83950e9-343-has-the-mnk-input-been-nerfed-and-reverted-back-to-its-state-prior-to-dec-6th-drop-pod)
- Halo Infinite Winter Update (November 2022) tuned bullet magnetism per weapon, including the Commando and the BR75. [SNIPPET] — [MP1st](https://mp1st.com/news/halo-infinite-winter-update-balance-adjustments-include-stat-tuning-for-several-weapons)
- A player-made video claimed the unscoped sniper on mouse had "anti" aim assist pushing the reticle away; the outlet reporting it notes it could be faked. [EST] — [Game Rant](https://gamerant.com/halo-infinite-sniper-aim-assist-mouse-and-keyboard/)
- Halo Infinite shipped a ranked playlist split by input (mouse versus gamepad) alongside crossplay. [SNIPPET] — [PCGamesN](https://www.pcgamesn.com/halo-infinite/ranked-playlists)
- Splitgate, the closest commercial clone, gives aim assist to console players only. [SNIPPET] — [Shacknews](https://shacknews.com/article/126314/how-does-aim-assist-work-in-splitgate)

### Inferences
- The autoaim range is the weapon's real effective range for a controller and the red-reticle distance: 17 wu for the Halo 2 BR versus 12 wu for the Magnum and SMG and 6 wu for the shotgun. A weapon's "range" in Halo is therefore three numbers (max range, instant-hit range, autoaim range), and a clone that copies only damage and rate of fire has none of them.
- For a mouse-first game the consequence is asymmetric: Halo's time-to-kill, head size and strafe speed were tuned for a player whose shots are bent up to 3 degrees toward the target and whose reticle is slowed and dragged along with it. Remove both and the same numbers produce much lower hit rates and much longer real kill times than the paper figure; the Infinite accuracy gap is the visible symptom.
- Since bullet magnetism applies to mouse in Halo's own PC ports, a mouse-played clone with literally zero magnetism is harsher than any official Halo on PC.

### Gaps
- No tag-level friction strengths (how much turning is slowed, how fast the camera follows) were found for any game; only cone angles and ranges.
- Halo 2 Vista and MCC mouse behaviour: nothing citable beyond Halopedia's general statement that autoaim applies to mouse and magnetism does not.
- A forum claim that 343 published top-100 accuracy by input during the 2021 flights (about a 5% gap) could not be traced to a primary source.
- The r/halo accuracy threads with raw percentages (hosts redlib.ssps.io, twt.omada.cafe) could not be opened from this session.
- Halo 5 and Infinite per-weapon red-reticle ranges: not found as numbers.

## 4. Movement: speeds, strafe, jump, sprint, hitbox, field of view

### Takeaway
The classic Spartan runs at 2.25 wu/s (about 6.9 m/s) with a slightly slower strafe and backpedal, has no sprint, and jumps high and slowly; Halo 2 and 3 share the same run speed and a jump velocity 47% above CE's. Later games raised base speed to 2.6 wu/s and added sprint, and Halo Infinite's instant strafe acceleration was contentious enough that it was cut to 0.78 of its value in February 2025.

### Cited Findings

| Game | Run speed | Sprint | Jump velocity | Class | Source |
|---|---|---|---|---|---|
| Halo CE | 2.25 wu/s (6.86 m/s) | none | 2.1 wu/s | [TAG] | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo 2 | 2.25 wu/s | none | 3.08 wu/s | [TAG] | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo 3 | 2.25 wu/s | none | 3.08 wu/s | [TAG] | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo: Reach | 2.2 wu/s | x1.623 (3.5706 wu/s), an armor ability | 2.3 wu/s | [TAG] | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo 4 | 2.2 wu/s | x1.65 (3.63 wu/s) | 2.36 x 1.29 global = 3.0444 wu/s | [TAG] | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo 2: Anniversary MP | 2.3 wu/s | none | 2.34 x 1.29 = 3.0186 wu/s | [TAG] | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo 5 | 2.6 wu/s | x1.3 (3.38 wu/s) | 3.0444 wu/s | unofficial tools | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |
| Halo Infinite | 2.6 wu/s (7.92 m/s) | x1.11 (2.886 wu/s) | 2.5 x 1.29 = 3.225 wu/s; gravity scale 1.14 | unofficial tools | [Halopedia: Spartan (multiplayer)](https://halopedia.org/Spartan_(multiplayer)) |

- Halo CE directional speeds: forward 2.25, backward 2.0, sideways 2.0 wu/s. — [HEK tutorial: Halo player stats](https://nikon.bungie.org/misc/hek_tutorial/references/general/halo_player_stats.html)
- Halo CE body and jump, in 3ds Max units (100 = 1 wu): standing 70.1 tall by 29.8 wide; crouched 42.5 tall; jump height 80 standing, 99 crouch-jump; running jump distance 312, crouch-jump 330; step height 12; max slope 45 deg; field of view 70 deg. — [HEK tutorial: Halo player stats](https://nikon.bungie.org/misc/hek_tutorial/references/general/halo_player_stats.html)
- Halo 3 target sizes per Bungie: Spartan model 0.75 wu, helmet about 0.094 wu wide. [DEV] — [Bungie Weekly Update 6/20/08](https://halo.bungie.org/bwu/index.html?item=182)
- 343 on Infinite: full sprint is about 8.5 m/s and the grapple about 24 m/s. [DEV] — [Halo Waypoint: Closer Look: Halo Infinite's Online Experience](https://www.halowaypoint.com/news/closer-look-halo-infinite-online-experience)
- Community measurement on Forge grids: Halo 3 base speed 7.69 m/s, Reach 6.82 m/s (grid scale differs between maps, so only roughly comparable). [EST][SNIPPET] — [Bungie.net forum](https://www.bungie.net/en/Forums/Post/3291870)
- Halo 5 after its beta (Josh Holmes, February 2015): base movement speed raised, strafe acceleration increased "making strafing quicker and more responsive", top sprint speed reduced to narrow the gap; nearly 11% of surveyed players said Halo should not have sprint at all. [DEV] — [MP1st](https://mp1st.com/news/heres-beta-feedback-helped-improve-halo-5-guardians)
- Halo Infinite's instant strafe was criticised in 2021 as a departure: "in all of the Halo games leading up to this there has been a slight inertial tendency which slowed things down". [EST][SNIPPET] — [r/halo weekly thread, August 2021](https://twt.omada.cafe/r/halo/comments/owg9wh/weekly_discussion_thread/h7kf337/?context=3)
- Halo Infinite Spring Update (4 February 2025): acceleration strafe scalar 1 to 0.78 and forward acceleration scalar 1 to 0.78; players read it as the same top speed reached about 20% slower, and note sprint cannot start until top speed; one competitive player's reaction: "they only did half of what we wanted. They nerfed the instant strafe but they didn't reduce the aim assist on the bandit". [DEV numbers via SNIPPET; reaction EST] — [Halo Support: Spring Update 2025 patch notes](https://support.halowaypoint.com/hc/en-us/articles/34335369079700-Halo-Infinite-Spring-Update-2025-Patch-Notes); [Sportskeeda](https://www.sportskeeda.com/esports/halo-infinite-spring-update-release-date-features-explored); [r/CompetitiveHalo](https://pol1.lr.ggtyler.dev/r/CompetitiveHalo/comments/1ieqabn/spring_update_2025_includes_big_balancing_updates/maaug9j/?context=3)
- Field of view: Halo Infinite defaults to 78 on every platform with a slider from 65 to 120; the Reach PC preview defaulted to about 75 and went to about 90. [SNIPPET] — [Prima Games](https://primagames.com/tips/what-is-the-best-fov-for-halo-infinite-answered); [PCWorld](https://www.pcworld.com/article/397581/halo-reach-on-pc-plays-pretty-much-like-youd-expect-and-thats-a-good-thing.html)
- Custom-game defaults keep speed and gravity at 100% in Halo 3 and Halo 5; Reach's Juggernaut preset shows the range the engine exposes (speed 150%, jump 150%). — [Halopedia: Custom game](https://www.halopedia.org/Custom_game)
- Jaime Griesemer (2025): the original Halo was built without sprint and its levels were authored around a single traversal speed, so adding sprint means redesigning encounters and pacing. [DEV][SNIPPET] — [Windows Central](https://www.windowscentral.com/gaming/bungie-developer-halo-sprint-mechanics)
- Critics' description of the jump: the "floaty hangtime" is part of the series' identity and lets a player out-manoeuvre an opponent for a headshot. [EST][SNIPPET] — [TheGamer](https://www.thegamer.com/halo-infinite-tech-preview-jump/)

### Inferences
- Target-speed-to-target-size is the feel ratio to compare: at 2.0 wu/s sideways a classic Spartan crosses its own 0.3 wu width in 0.15 s and a 0.094 wu helmet width in under 0.05 s, yet takes 1.3 to 1.6 s to kill. Fights are long tracking contests against a slow, wide target; a clone with a faster or narrower body, or a shorter kill time, is a different game even with identical shot counts.
- Halo 2/3's jump velocity is 1.47 times CE's (3.08 versus 2.1 wu/s); at equal gravity that is about 2.15 times the height. Reach dropped it to 2.3 wu/s, which is the "heavy" Reach jump players describe. Gravity itself was not sourced, so heights and hang times cannot be derived here.
- CE's 80-unit jump (0.8 wu) clears more than the player's own 0.70 wu standing height: the jump is a traversal and dodge tool, and an airborne Spartan is on a slow, predictable arc, which is why it reads as "floaty" and why jumping in a duel is a commitment.
- Sprint multipliers fell with each game that kept it (1.62, 1.65, 1.3, 1.11) while base speed rose from 2.2 to 2.6 wu/s: 343 converged back toward a single speed.
- The classic "inertial" strafe is a duelling mechanic: with acceleration below instant, reversing direction costs time, so strafe timing can be read and punished. Instant acceleration (Infinite at launch) makes the strafe unreadable for mouse and strongly favours friction-assisted aim; too little makes a target a rail.

### Gaps
- Acceleration, deceleration and strafe-reversal times for CE, 2, 3 and Reach: no numbers found anywhere.
- Gravity and hang time for any game; Halo 2 and 3 backward and strafe speeds; crouch speeds; Halo 2/3 console field of view; Halo 5 field of view.
- Slide, clamber and thrust distances and timings (Halo 5, Infinite): not found.
- Hitbox (as opposed to model) dimensions for any game.

## 5. Melee

### Takeaway
Melee in Halo 2 is velocity-scaled (75 at speed, 55 standing, against a 70 shield and 115 total) and its recovery can be cancelled into a shot, which is where BXR and the double shot come from; Halo 3 fixes it at 70, exactly one shield. Bleed-through, a lunge on red reticle, and a one-hit back smack are constants; Reach removed bleed-through at launch and restored it in the 2011 Title Update.

### Cited Findings
- Halo 2 multiplayer melee damage (as of 1.1a): strike 75 falling to 55 by velocity; smash 90 falling to 55; slice (bladed weapons) 120 falling to 90. Campaign: strike 60 to 20, smash 120 to 40, slice 70. [TAG] — [Halopedia: H2:Melee](https://www.halopedia.org/H2:Melee)
- Halo 3 melee damage: strike 70, smash 80, slice 90, cut 72. [TAG] — [Halopedia: H3:Melee](https://www.halopedia.org/H3:Melee)
- Halo CE melee: 50 to 60 (random) falling to 40 by velocity, x1.4 vs Spartans, radius 0.5 wu; no lunge. [TAG] — [Halopedia: CE:Melee](https://www.halopedia.org/CE:Melee); [Halopedia: Lunge](https://www.halopedia.org/Lunge)
- Halo 2 qualitative rules: a standing melee does least (some weapons need six hits on a full-shield Spartan), a running melee removes about half the shield, a jumping melee nearly all; melee range was cut from CE but a lunge was added; meleeing while dual-wielding drops the left weapon; melee cancels various animations and delays. — [Halopedia: Melee](https://www.halopedia.org/Melee)
- Halo 2 Auto-Update 3 (1.1a, 18 April 2005): melee damage raised across the board, jumping strikes "often killing instantly", Brute Shot melee a two-hit kill; Bungie's rationale was that the melee boost counters close-range dual wielding. Sword flying was removed from multiplayer. [DEV] — [Halopedia: Halo 2 Auto-Updates](https://www.halopedia.org/Halo_2_Auto-Updates)
- Lunge conditions (Halo 2 onward): the target must be close enough that the reticle is red; the attacker then lunges before striking, which delays the hit and gives the target time to react; sword lunge range is far greater than a normal weapon's. No distances are given. — [Halopedia: Lunge](https://www.halopedia.org/Lunge)
- Halo 2 sword aim cones: autoaim 6 deg to 6 wu, magnetism 8 deg to 12 wu. [TAG] — [Halopedia: H2:Energy Sword](https://www.halopedia.org/H2:Energy_Sword)
- Halo 5 sword lunge: 6.87 m unzoomed, 11.92 m with Smart-Link. [SNIPPET] — [Halopedia: Vorpal Talon](https://www.halopedia.org/Vorpal_Talon)
- Halo 3: two blows kill a normally shielded player; after a title update, two unshielded players who melee each other both die and both score. Back hits are lethal (Assassin medal); a normal melee kill is a Beat Down. — [Halopedia: Melee](https://www.halopedia.org/Melee)
- Halo 3's "BXB-style glitch" was treated as a bug and removed by the first Auto-Update on 19 February 2008; Bungie "worked hard to prevent" melee-fire combos in Halo 3. — [Halopedia: Button combos](https://www.halopedia.org/Button_combos); [Halopedia: BXR](https://www.halopedia.org/BXR)
- BXR (Halo 2): melee (B), cancel the rest of the melee animation and its recovery with reload (X), fire (R); the melee must connect; it needs a full magazine or X starts a real reload; B, Y, Y, R works with a partial magazine and is slightly slower. Done right it "takes down the shields first before shooting" and "can provide an instant kill". — [Halopedia: BXR](https://www.halopedia.org/BXR)
- Double shot (R, R, X) fires two BR bursts back to back by skipping the delay between bursts; it chains with Y, Y; a "quad shot" is two double shots (R R X, Y Y, R R X), and BXR can lead into it. [EST][SNIPPET] — [HaloRuns wiki: Halo 2 MCC](https://wiki.haloruns.com/Halo_2_MCC); [Halopedia: H2:Battle Rifle](https://www.halopedia.org/H2:Battle_Rifle); [Bungie.net forum guide](https://www.bungie.net/en/Forums/Post/414304)
- Halo 2 button rules behind the combos: X cancels melee or grenade animation timers "(off-host)"; Y switches weapons and cancels reloads and melee animations; a plasma overheat resets shoot, melee and grenade timers. Bungie's Tom Gioconda: by the letter of the law button glitching is cheating. [DEV] — [Halopedia: Button combos](https://www.halopedia.org/Button_combos)
- Reach at launch had no bleed-through (a melee on a sliver of shield only popped the shield); the Title Update restored it "consistent with the behavior of previous Halo games", making 3 DMR shots + melee a kill instead of 4, and disabled sword block. — [Halopedia: Halo: Reach Title Update](https://www.halopedia.org/Halo:_Reach_Title_Updates)
- Reach also added the melee "clang" (two simultaneous melees bounce off) and hold-to-assassinate from behind; Halo 5's melee deals Halo 3 damage (two hits kill) plus Spartan charge and ground pound; Halo Infinite "uses Halo 3's basic melee mechanics", melee weapons clash, and assassinations are gone. — [Halopedia: Melee](https://www.halopedia.org/Melee)
- Halo Infinite Ranked BR75: two body bursts plus one melee kill; the variant exists to preserve that combo after melee damage was reduced. — [Halopedia: BR75 battle rifle](https://www.halopedia.org/BR75_battle_rifle)
- Halo Infinite melee over the network: lunges use latency compensation, which causes apparent phasing through opponents and double back smacks; player collision against enemies is on and phasing is unintended; a melee thrown just before dying still lands through "a special code path". [DEV] — [Halo Waypoint: Closer Look: Halo Infinite's Online Experience](https://www.halowaypoint.com/news/closer-look-halo-infinite-online-experience)

### Inferences
- The melee number is tuned against the shield number. Halo 3: 70 damage against a 70 shield, so one hit pops a full shield exactly and anything already chipped bleeds into health; two hits (140) kill. Halo 2: 75 at speed pops the shield with 5 to spare, but two standing hits (110) do not kill 115 while two moving hits (150) do. Movement into a melee is a damage input in Halo 2.
- Gun-then-melee thresholds on the Halo 2 tags: three BR bursts (54) + a moving melee (75) = 129, a kill; two bursts + melee = 111, not a kill. In Halo 3 three bursts + melee = 124 kills, two = 106 does not. Infinite's ranked rule is two bursts + melee. So "how many shots before the punch finishes it" is a pinned Halo number (3 in Halo 2/3), and bleed-through is what makes it exist: without bleed-through every melee on a shielded target only pops the shield and close fights degenerate into double-melee trades, which is the launch-Reach complaint.
- BXR on the tags is melee (75: shield gone, 5 into health) then one headshot burst inside the melee's own recovery: a two-input kill from full shields at arm's length. The Halo 2 close-range game the community remembers is faster than its paper numbers because of these cancels. A clone pinned to Halo 2's paper counts without the cancels has Halo 2's slowest version of close combat.

### Gaps
- Lunge distances for normal melee and the sword in Halo 2 and 3, melee recovery time, and the melee animation length: not found as numbers.
- Frame timings for BXR and the double shot, and resulting kill times: not found.
- Halo 3's rule for simultaneous shielded melees (who wins) and Infinite's melee damage value: not found.
- Whether MCC's Halo 2 Classic, Halo 2 Vista and Halo 2: Anniversary keep the combos: not stated in the sources opened.

## 6. Grenades

### Takeaway
A Halo 2 multiplayer frag is a 10 wu/s lob that explodes 0.5 s after its first ground bounce (never sooner than 1.3 s after the throw) for 160 damage inside 0.5 wu falling to 50 at 1.75 wu, halved against shields. Bungie buffed grenades in the 1.1 update because they were "too weak", and players spawn with two.

### Cited Findings
- Halo 2 frag, multiplayer: detonates 0.5 s after the first ground bounce and no earlier than 1.3 s after being thrown (campaign 1.5 s); 160 damage inside 0.5 wu falling to 50 at 1.75 wu (campaign 150 inside 0.75 wu); x0.5 vs shields; thrown at 10 wu/s under 100% gravity; reserve cap 4; players start with 2 on every map except Midship and Gemini (plasmas) and Backwash (none). [TAG] — [Halopedia: H2:Frag Grenade](https://www.halopedia.org/H2:Frag_Grenade)
- Halo 2 plasma grenade, multiplayer: detonates 0.9 s after coming to rest and no sooner than 1.3 s after the throw; stuck damage 400; splash 200 inside 0.5 wu falling to 50 at 1.75 wu; x0.5 vs shields; 10 wu/s; sticks to units, bounces off geometry. The page's campaign figure (1.5 s after rest) conflicts with its own prose "fuse of three seconds". [TAG] — [Halopedia: H2:Plasma Grenade](https://www.halopedia.org/H2:Plasma_Grenade)
- Halo 3 frag: 160 inside 0.5 wu falling to 60 at 1.75 wu; x0.5 vs shields and x0.5 vs Spartan health; 0.5 s after first ground bounce, minimum 1.3 s; 10 wu/s at 90% gravity; carry 2. [TAG] — [Halopedia: H3:Frag Grenade](https://www.halopedia.org/H3:Frag_Grenade)
- Halo 2 Auto-Update 3 (April 2005): frag and plasma fuse timers shortened, plasma damage "significantly" and frag damage "slightly" increased, because grenades were considered too weak. [DEV] — [Halopedia: Halo 2 Auto-Updates](https://www.halopedia.org/Halo_2_Auto-Updates)
- Halo 5 default loadout: Assault Rifle, Magnum and 2 frag grenades; Halo 5 removed mid-air detonation of grenades by weapon fire after its beta. — [Halopedia: Custom game](https://www.halopedia.org/Custom_game); [MP1st](https://mp1st.com/news/heres-beta-feedback-helped-improve-halo-5-guardians)
- Reach networking: the throw animation starts at once on the button press, but the grenade is created on the release frame by a request to the host confirmed about one round trip later; players did not notice gaps up to 150 ms and casual players tolerated 200 ms. Player-owned grenades are prioritised in replication; idle grenades update about 3 times a second. [DEV] — [Edgegap: Halo: Reach netcode deep dive](https://edgegap.com/blog/game-backend-deep-dive-halo-reach-netcode-host-migration)
- Halo 3: grenades and rockets are created by the host, so clients see them appear a few frames late, while sniper rounds appear at once. — [halo.bungie.org network guide](https://halo.bungie.org/misc/networkguide.html)
- Reach Title Update: a grenade going off next to a player in Armor Lock now drains the ability; a stuck player can no longer shed the plasma. — [Halopedia: Halo: Reach Title Update](https://www.halopedia.org/Halo:_Reach_Title_Updates)

### Inferences
- The frag is a shield tool: 160 x 0.5 = 80 against a 70 shield, so a frag landing within 0.5 wu (1.5 m) leaves a full-shield player without shields and one headshot from dead, while at the 1.75 wu edge it takes 25 of 70. That is the "grenade first, then the headshot" opener, and it only works if the fuse is short and predictable (0.5 s after the bounce) so the thrower can time the shot to the pop.
- The bounce-armed fuse is the mechanic clones miss: the timer starts at the first ground contact, not the throw, so a grenade thrown at someone's feet goes off in half a second while a long lob takes as long as its flight. A fixed fuse from the throw (as in most shooters) removes the skill of placing the first bounce.
- Two grenades per life against a 115-point body with 7 s shield recovery means each life has two fight-openers; four (Halo 2's cap) is a stockpile from pickups. Starting count is part of time-to-kill.

### Gaps
- Bounce restitution, throw arc angle, and the "golden triangle" phrase from a developer source: not found; the phrase is community shorthand for guns, grenades and melee as far as I could establish, with no primary citation.
- Halo CE, Reach, Halo 5 and Infinite grenade numbers were not retrieved.

## 7. Spawns, weapon timers, starting loadout, radar

### Takeaway
Respawn is 5 s by default from Halo 2 through Halo 4 and 8 s in Halo 5 Slayer; radar is short (20 m in Halo 2, 25 m in Halo 3, 18 m in Halo 5 and Infinite arena); and the starting weapon is the single most argued-over setting in the series, with Bungie conceding on SMG starts and adding BR starts to Halo 2 playlists in 2005.

### Cited Findings
- Respawn and penalties (custom-game defaults): Halo 2 Slayer 5 s respawn, 5 s suicide penalty, 10 s betrayal penalty; Halo 3 5 s respawn, 10 s suicide and betrayal; Reach and Halo 4 5 s with 5 s penalties; Halo 5 Slayer 8 s. — [Halopedia: Custom game](https://www.halopedia.org/Custom_game)
- Starting weapons: Halo 2 "Map Default" with starting grenades on; Halo 3 Assault Rifle primary and no secondary; Halo 5 Assault Rifle + Magnum + 2 frags. — [Halopedia: Custom game](https://www.halopedia.org/Custom_game)
- Bungie Weekly Update, 12 August 2005: BR starts added on Terminal and Relic for Rumble Pit "(yes, we hear you)"; objective games keep default weapon sets; "Most of the complainers only complain about the starting weapon set in Slayer games anyway." [DEV] — [Bungie Weekly Update 8/12/05](https://www.halo.bungie.org/bwu/index.html?item=37)
- Halo 2's own loading-screen tip: "SMG Starts!" "Okay, we admit it. It was a bad idea." [DEV][SNIPPET] — [Halopedia: List of Halo 2 Did You Know? quotes](https://www.halopedia.org/List_of_Halo_2_Did_You_Know%3F_quotes)
- Reach: feedback led 343 to remove the Magnum as the Zero Bloom spawn weapon in favour of the Assault Rifle (December 2011), and the Needle Rifle was pulled from Slayer Pro loadouts. — [Halopedia: Halo: Reach Title Update](https://www.halopedia.org/Halo:_Reach_Title_Updates)
- Motion tracker range: CE 15 m; Halo 2 20 m; Halo 3 25 m by default with options of 15, 25, 50, 75 and 150 m. In CE multiplayer a player does not show while crouched, crouch-walking or standing still. — [Halopedia: Motion tracker](https://www.halopedia.org/Motion_tracker); [Halopedia: Custom game](https://www.halopedia.org/Custom_game)
- Halo 5's tracker range was 18 m; a 2017 Proving Grounds test raised it to 25 m with other changes 343 said suited ranked play. [SNIPPET] — [GameSpot](https://gamespot.com/articles/halo-5-is-testing-major-radar-changes-that-could-g/1100-6448396/)
- Halo Infinite's flights used a "combat sensor" that showed only sprinting or shooting enemies; players disliked it and 343 restored a classic motion tracker for launch; Big Team Battle's range later went from 18 m to 24 m. [SNIPPET] — [GameSpot](https://gamespot.com/articles/halo-infinite-makes-a-major-change-to-radar-after-the-beta/1100-6495958/); [EconoTimes](https://www.econotimes.com/Halo-Infinite-mid-season-update-to-improve-the-motion-tracker-in-BTB-and-the-games-anti-cheat-system-1627668)
- Weapon timers are per map, not global. Halo 3 in MCC (community-tested March 2015): rockets 1:30 to 3:30, sniper and shotgun about 1:30 to 3:00 depending on map. [EST][SNIPPET] — [Bungie.net forum](https://www.bungie.net/Ne/Forums/Post/106060001?page=0)
- Halo 2 weapons start their respawn timer only once a player has dropped them, which rewards the first pick-up. [EST][SNIPPET] — [Rampancy.net](https://rampancy.net/broken_halo_or_broken_record)
- Halo Infinite: active camo and overshield respawn at 2:00. [EST][SNIPPET] — [SteamAH](https://steamah.com/halo-infinite-spawn-times-for-every-map-weapons-powerups-vehicles/)
- Halo 2 matchmaking defaults: overshields on the map on, active camouflage on, motion sensor on. — [Halopedia: Custom game](https://www.halopedia.org/Custom_game)
- Reach Title Update cut stationary active camo from 44.3 s to 28 s to curb sniper camping. — [Halopedia: Halo: Reach Title Update](https://www.halopedia.org/Halo:_Reach_Title_Updates)

### Inferences
- With a 20 to 25 m radar and a utility rifle effective to 50 m and beyond, the tracker only governs the last second of an approach: it punishes sprinting and running into melee or shotgun range and rewards crouch-walking. In 1v1 and 2v2 it largely removes flanks; the competitive community's long-standing preference for radar off or shortened follows from that.
- The starting-weapon fight is a time-to-kill fight in disguise: an SMG or AR start means a spawning player cannot kill a BR holder at mid range inside the BR's 1.3 to 1.45 s, so the first team to pick up rifles controls the map. BR starts make every spawn a threat. For a ranked 1v1 or 2v2 mode this is likely the single setting with the largest effect on whether fights "feel off".

### Gaps
- Spawn-influence systems (how Halo 2/3 weight spawn points by enemy and teammate proximity, line of sight and recent deaths): no source found with numbers.
- Halo 2 map-by-map weapon and power-up timers; overshield strength and decay; active-camo duration in Halo 2 and 3.
- A dated account of when Halo 2 Team Slayer moved from SMG to BR starts; ranked radar rules in Halo 5 and Infinite beyond the above.

## 8. Netcode and tick rate

### Takeaway
The original trilogy and Reach simulate at 30 Hz on a player-hosted game (several weapon fire rates in the tags are quantised by it), with Halo 2 trusting clients' hit reports and Halo 3 making the host fire every shot. Halo Infinite runs dedicated servers at 60 Hz for 4v4 and 30 Hz for Big Team Battle, "favors the shooter", and spent 2022 to 2024 fixing desync before overhauling the networking model in March 2024.

### Cited Findings
- 30 Hz evidence in the tags: the Halo 2 plasma rifle's 6 to 9 rounds/s becomes 7.5/s "after tick rounding", the carbine's 7.1/s becomes 6/s, the plasma pistol's 20/s becomes 15/s, the needler's 8/s becomes 7.5/s; Halo 2 Vista's higher tick rate produced different rates (8.6, 6.66 and 20/s) until MCC patched it. The CE pistol's 3.5/s becomes 3.33/s; the Halo 3 BR's 0.28 s becomes 0.3 s. — [H2:Plasma Rifle](https://www.halopedia.org/H2:Plasma_Rifle); [H2:Covenant Carbine](https://www.halopedia.org/H2:Covenant_Carbine); [H2:Plasma Pistol](https://www.halopedia.org/H2:Plasma_Pistol); [H2:Needler](https://www.halopedia.org/H2:Needler); [CE:Pistol](https://www.halopedia.org/CE:Pistol); [H3:Battle Rifle](https://www.halopedia.org/H3:Battle_Rifle)
- MCC moved to a 60 Hz tick in April 2019 for Halo 2, Halo 3, Halo 4 and Halo 2: Anniversary matches. [SNIPPET] — [Kotaku](https://kotaku.com/halo-the-master-chief-collection-now-has-a-60-hz-tick-1833787526); [GameRevolution](https://www.gamerevolution.com/news/519901-halo-mcc-april-update-increases-tick-rate)
- Halo 2 let clients report "I shot, and here is where I hit", which gave clients a sniping advantage over the host; Halo 3 was rebuilt to remove it: the client sends who it aimed at and how far ahead, the host fires its own shot from that, the hit lands about two ticks later on both machines, and if the target is not where predicted no hit registers. Each BR bullet is networked individually. "The system is not perfect." [DEV] — [Bungie Weekly Update 6/20/08](https://halo.bungie.org/bwu/index.html?item=182)
- Halo 3: the host authorises nearly all actions including damage; the host's shots "always count when they occur"; there is a latency window of about 300 ms inside which a client's shot should count; jumping and firing are instant on the client and rolled back (teleport, bullets refunded) if the input misses the window; shots at players hugging geometry may be rejected by a line-of-sight check, intentionally. [EST, community guide] — [halo.bungie.org network guide](https://halo.bungie.org/misc/networkguide.html)
- Halo 2 countermeasures in Auto-Update 3: the modem "standby" cheat addressed and host-abuse limits added. — [Halopedia: Halo 2 Auto-Updates](https://www.halopedia.org/Halo_2_Auto-Updates)
- Reach (David Aldridge, GDC 2011): Tribes-derived semi-reliable replication with per-client priorities; 16 players in 250 kbps against a 384 kbps goal, over 80% less than Halo 3; "any host-adjudicated mechanic has lag somewhere", the design task is choosing where; Armor Lock's shield is activated early on the host by the measured round-trip time; assassinations hide position error with a 0.75 s blend. [DEV] — [Edgegap: Halo: Reach netcode deep dive](https://edgegap.com/blog/game-backend-deep-dive-halo-reach-netcode-host-migration)
- Reach shot validation as summarised from the talk: the client claims a hit ("I shot this player in the head") and the host checks for a clean line from the gun to that head. [SNIPPET] — [Wolfire: GDC Session Summary: Halo networking](https://wolfire-blog.appspot.com/2011/03/GDC-Session-Summary-Halo-networking)
- Halo Infinite (Richard Watson, lead sandbox engineer, early 2022): 4v4 servers run at 60 Hz and Big Team Battle at 30 Hz; "in Halo we choose to favor the shooter", the server rewinds targets to what the shooter saw; delay between views is shooter latency + command buffer (0 or 1 frame, up to 16 ms) + server processing (16 to 33 ms) + target latency: 50 ms with both players at 33 ms round trip, 83 ms with a 100 ms shooter, 108 ms with a 150 ms shooter. [DEV] — [Halo Waypoint: Closer Look: Halo Infinite's Online Experience](https://www.halowaypoint.com/news/closer-look-halo-infinite-online-experience)
- Infinite's named causes of bad registration: nondeterminism between client and server, bandwidth limits on state updates, and death latency (the server has killed you before your client knows, so your last shots are discarded, though a melee is honoured). PC players were advised to set Simulation Quality to Ultra. [DEV] — [Halo Waypoint: Closer Look: Halo Infinite's Online Experience](https://www.halowaypoint.com/news/closer-look-halo-infinite-online-experience)
- Later in 2022, 343 said melee desync was not being worked on because the team was "allocated to other Infinite work". [DEV][SNIPPET] — [VGC](https://www.videogameschronicle.com/news/halo-infinites-melee-desync-isnt-being-fixed-because-devs-have-been-allocated-elsewhere-343-says); [PC Gamer](https://pcgamer.com/halo-infinites-desync-problem-is-on-hold-because-developers-are-busy-with-other-things)
- November 2023: 343 (John Junyszek) said improving the existing system "bit by bit" was not having the impact wanted and the team would "pursue fundamental changes to the underlying networking model". [DEV] — [Pure Xbox](https://www.purexbox.com/news/2023/11/halo-infinites-networking-is-being-fundamentally-changed-to-address-de-sync-issues)
- 19 March 2024: an update shipped a "completely overhauled networking model" together with Easy Anti-Cheat on PC. [SNIPPET] — [Wccftech](https://wccftech.com/halo-infinite-update-march-2024-networking-overhaul-easy-anti-cheat/)
- Current state: Halo Infinite's final major content update, Operation: Infinite, went live on 19 November 2025 with no balance or netcode changes; Halo Studios is winding down ongoing support, with possible bug-fix patches. — [ComicBook.com](https://comicbook.com/gaming/news/halo-infinite-update-multiplayer-final-patch-notes)

### Inferences
- "Halo 2's counts and times" are 30 Hz numbers. A recovery of 0.14 s is 5 ticks (0.167 s) at 30 Hz but 9 ticks (0.15 s) at 60 Hz; 343 found this the hard way twice (Vista and MCC). Any reimplementation at another tick rate must decide whether to pin the tag value or the 30 Hz-rounded value that players actually felt, and the two differ by up to 25% (plasma pistol 20 versus 15 per second).
- Halo 2's remembered crispness on Xbox Live is partly client-side hit reporting: on a good connection the BR felt hitscan because the shooter's console decided the hit. Halo 3's host-fired, leadable, spread BR is the documented regression in feel. A server-authoritative clone with rewind is closer to Infinite's model, and inherits its complaint class (shot behind cover; trades and phasing in melee) rather than Halo 2's (host advantage).
- 343's formula gives a floor: even with both players at 33 ms, 50 ms separates what the shooter and the target see, about 0.4 m of travel at Infinite's sprint speed. The slower classic Spartan (6.9 m/s, no sprint) hides the same latency better, another reason old Halo felt fair on poor connections.

### Gaps
- An explicit Bungie statement of the 30 Hz simulation and packet rates for Halo 2, 3 and Reach was not found; the rate is established by the tag-rounding notes and 343's MCC notes.
- How the March 2024 Infinite networking overhaul performed, and any description of the new model, were not found.
- Halo 5's server tick rate and model: not found in this pass.
- The Aldridge talk itself (GDC Vault video) was not readable here; details come from two summaries.

## 9. Feedback: shields, hit confirmation, damage direction, medals

### Takeaway
Bungie-era Halo confirmed hits in the world, not on the HUD: the target's shield flares on every hit, pops visibly and audibly, and the reticle is red only when the shot will land. HUD hit markers arrived with 343 (Halo 4 onward), and Infinite colours the enemy itself to show a shield that is about to break or already broken.

### Cited Findings
- Hit markers have been in the series since Halo 4; the Bungie games relied on in-world cues ("an elite's energy shield flickered, a grunt died, or a jackal shield popped"); the 2026 remake Halo: Campaign Evolved makes hit markers the default in campaign and multiplayer. [SNIPPET, opinion piece] — [PC Gamer: Hit markers are an FPS crutch](https://www.pcgamer.com/games/fps/hit-markers-are-an-fps-crutch/)
- Halo 5's hit marker turns red on a critical hit. [SNIPPET] — [Twinfinite](https://twinfinite.net/2016/12/reticle-unsung-hero-modern-shooters/)
- Halo Infinite reads shield state off the enemy model: a player glowing red still has a shield but is close to losing it; a player glowing bright pink or white with a honeycomb pattern has had the shield popped; from there a pistol or BR headshot or a melee kills. — [GamerGuides: Managing and capitalizing on TTK](https://gamerguides.com/halo-infinite/guide/multiplayer/getting-started/managing-and-capitalizing-on-ttk)
- The red reticle is itself the hit predictor: it shows that auto-aim is fully engaged and the shot will go at the enemy, and it disappears beyond the weapon's range. — [Halopedia: Auto-aim](https://www.halopedia.org/Auto-aim)
- Griesemer on feedback in the 2010 talk (attendee notes): "Causality must be obvious, even at the cost of realism"; "Your game has to look good on youtube, so you can tell what is making what happen"; add spectacle through sound, control and animation; the sniper's zoom-out on reload is delayed 0.5 s so the player sees the target die. [DEV, second-hand notes] — [VGChartz forum notes on the GDC 2010 talk](https://gamrconnect.vgchartz.com/post.php?id=3259176)
- Medals named in the melee rules: Beat Down for a melee kill, Assassin for a kill from behind (Halo 3), Yoink and Showstopper for stolen or interrupted assassinations (Reach), Ninja for a jump-over back hit (Infinite). — [Halopedia: Melee](https://www.halopedia.org/Melee)
- Reach's plasma impacts were described at preview as landing "with meaty thuds" and continuing to burn on the target, "giving visual and audio feedback that leaves players in no doubt why their shield has been obliterated". [SNIPPET] — [GamesRadar](https://www.gamesradar.com/halo-reachs-huge-battles-and-new-guns/2)
- Reach networking gives dead bodies near-maximum replication priority for several seconds after a kill, so the kill is seen promptly. [DEV] — [Edgegap: Halo: Reach netcode deep dive](https://edgegap.com/blog/game-backend-deep-dive-halo-reach-netcode-host-migration)

### Inferences
- The shield is the fight's state display. Because headshots only matter after the pop, both players need to know the instant it happens: the attacker to move the reticle to the head, the victim to break line of sight. Every Halo marks that instant twice (a flash or colour change on the body, and a distinct sound). A clone can have Halo's numbers and still feel wrong if the pop is not the loudest, clearest event in a duel, since players cannot time the finishing headshot or the retreat.
- In-world confirmation (flare on the target) also tells the attacker where on a strafing body the shots are landing, which a centre-screen hit marker does not. For a mouse-first game without friction or magnetism this matters more, not less.

### Gaps
- No developer statement was found that describes shields explicitly as a readable state machine, nor one on the shield-pop or low-shield alarm sound design, damage-direction indicators, the kill feed, or the announcer's purpose.
- Whether Reach or Halo 4 first showed a hit marker for grenades was not confirmed; the one source says "since Halo 4" without separating grenades.
- The PC Gamer and Twinfinite pieces could not be opened in full; their content here is from search summaries.

## 10. Design intent from the developers

### Takeaway
Bungie's stated method is role-first: at most one weapon per role, strengths pushed to the maximum, limitations (not weaknesses) to bound each role, tuning by feel and by large steps, never by fairness. The famous examples are the "30 seconds of fun" loop and the Halo 3 sniper's time between shots going from 0.5 s to 0.7 s to weaken it up close without touching its main job.

### Cited Findings
- Griesemer's full "30 seconds of fun" remark: "In Halo 1, there was maybe 30 seconds of fun that happened over and over and over and over again"; the half cut from the documentary: "No 30 second stretch of Halo is ever repeated; the missions are constantly changing the context on you"; and "you have a 3-second loop inside of a 30-second loop inside of a 3-minute loop that is always different". [DEV] — [Engadget: Half-Minute Halo, an interview with Jaime Griesemer](https://www.engadget.com/2011-07-14-half-minute-halo-an-interview-with-jaime-griesemer.html)
- Griesemer on pace: "Halo is fundamentally different in that it lets the player set the pace"; "it waits to see what you will do, how hard you will push, and then it pushes back at just the right resistance". [DEV] — [Engadget](https://www.engadget.com/2011-07-14-half-minute-halo-an-interview-with-jaime-griesemer.html)
- The GDC 2010 talk's premise: Halo's multiplayer balances over 1,000 objects and more than 40 weapons; the sniper rifle alone has over 200 fields; the talk follows one of them. [DEV] — [GDC Vault](https://gdcvault.com/play/1012211)
- "Be wary of fairness. It's not balance... Halo's not fair." [DEV] — [Game Developer](https://gamedeveloper.com/design/video-0-2-seconds-to-balancing-i-halo-3-i-s-sniper-rifle)
- Attendee notes on the talk's method: balance in ordered passes (Role, Flow, Strength, Limitations, Detail) and do not reopen a settled pass; start each pass on paper; balancing is intuition that something feels wrong plus reasoning about the fix. [DEV, second-hand notes] — [VGChartz forum notes](https://gamrconnect.vgchartz.com/post.php?id=3259176)
- Roles: a shooter should have at most one weapon per role and every role needs a weapon; extra weapons in a role add complexity, not depth; "If it looks like a gun it should be a gun"; cut about half of what was planned. [DEV, second-hand notes] — [VGChartz forum notes](https://gamrconnect.vgchartz.com/post.php?id=3259176)
- Flow: cadence matters ("too slow is boring, too fast makes events blur together"); low floor and high ceiling. Strength: "Halo should resemble ketchup", a few core flavours each pushed to the maximum; if a strength needs explaining it is not a real strength; "Framerate above graphics quality. It has to work." [DEV, second-hand notes] — [VGChartz forum notes](https://gamrconnect.vgchartz.com/post.php?id=3259176)
- Limitations: "Limitations are not weaknesses"; they bound where a role succeeds; if one role wins too often add limitations, if outcomes feel random there are too many. Playtest: "I don't like x" is useful, "You should do x" is not; negative feedback usually means the player's mental model does not match the game. [DEV, second-hand notes] — [VGChartz forum notes](https://gamrconnect.vgchartz.com/post.php?id=3259176)
- The sniper problem and the fix: it was too strong at close range and for body shots with no cover nearby. Rejected: lower damage or range (weakens the main role); a 3-round magazine (cannot kill two ranged enemies without a reload); longer reload or zoom time (fixes half); less total ammo (fixes the average, not the moment players notice). Chosen: time between shots 0.5 s to 0.7 s, leaving the long-range role intact. [DEV, second-hand notes] — [VGChartz forum notes](https://gamrconnect.vgchartz.com/post.php?id=3259176)
- Step size: never change anything by less than 10%, because small changes go unnoticed; overshoot and come back. [DEV, second-hand notes] — [VGChartz forum notes](https://gamrconnect.vgchartz.com/post.php?id=3259176); [Unknown Worlds forum](https://forums.unknownworlds.com/discussion/111009/processes-to-go-through-when-balancing-weapons)
- Bungie on the utility weapon in 2008: Halo 3 has "no designed one-weapon-to-rule-them-all", on purpose, to push players across the sandbox; the BR's first bullet was retuned to be a reliable headshot and anti-sniper tool. [DEV] — [Bungie Weekly Update 6/20/08](https://halo.bungie.org/bwu/index.html?item=182)
- Bungie's 2005 reasons for the Halo 2 1.1 changes: grenades were too weak, the melee boost counters close-range dual wielding, and the Magnum change favours accurate headshots over dual-wield combos. [DEV] — [Halopedia: Halo 2 Auto-Updates](https://www.halopedia.org/Halo_2_Auto-Updates)
- Griesemer on movement (2025): Halo's levels and encounters were authored around a single traversal speed. [DEV][SNIPPET] — [Windows Central](https://www.windowscentral.com/gaming/bungie-developer-halo-sprint-mechanics)
- The Halo 2 sniper ranked near the top of Bungie's list of proposed Halo 2 changes but was never patched. [SNIPPET] — [halo.bungie.org news](https://halo.bungie.org/news.html?item=39878)

### Inferences
- Bungie and its competitive players disagreed about the utility weapon, and the numbers record it: the community's preferred Halo (Halo 2 post-1.1, BR starts, zero-bloom Reach) is the one where a single precise mid-range rifle is reliable at every range; Bungie's stated ideal (Halo 3) deliberately limits it. A game that pins its guns to Halo 2 has chosen the community's side and should expect its other weapons to matter mostly as shield tools and finishers around that rifle.
- "Paper versus feel" in practice: the sniper fix did not show up in damage-per-second terms at range, only in how often a close-range body-shot pair landed before the victim could react. The lesson for diagnosing "something feels off" is to look for a weapon winning outside its role, then change cadence rather than damage, in a step of 10% or more.
- The 3 s / 30 s / 3 min loop maps onto multiplayer as: a burst exchange, a fight including shield recovery (about 7 s recovery against a 1.3 to 1.6 s kill), and a weapon or power-up cycle (1.5 to 3.5 minutes). If any tier is missing or compressed, pacing feels wrong even with correct gun numbers.

### Gaps
- No Bungie or 343 statement was found on why the player is slow with a high jump, or on a target time-to-kill figure, beyond the Halo 2 BR being "arguably too lethal".
- The talk's own slides and video were not readable; the method above is from one attendee's notes that the note-taker calls possibly incomplete.
- No Jason Jones or Max Hoberman combat-design quotes were retrieved in this pass.

## 11. Why clones and fan projects feel right or wrong

### Takeaway
The clones that are said to feel right copied timing rather than counts: Splitgate's developers say they took Halo 1 to 3's gunplay and measured basic movement timing against Halo 2. The recurring ways to feel wrong are sprint, faster or instant strafe, a changed kill time, and input-dependent aim assist; Halo Infinite itself drew each of those complaints.

### Cited Findings
- Splitgate's creative director Ian Proulx (2019): "As hardcore fans of Halo one to three, we specifically tried to take the best parts of those three games when it came to gunplay. We wanted a TTK (time-to-kill) that was long enough that players had time to use portals to strategically flank, but not so long that enemies could escape via portal every time they were getting shot." [DEV][SNIPPET] — [Unreal Engine developer interview](https://www.unrealengine.com/developer-interviews/splitgate-arena-warfare-infuses-elements-of-halo-and-portal-to-create-a-new-breed-of-fps?lang=es-ES)
- A Splitgate developer called Halo 2 "the greatest game of all time" and said a Halo-like kill time leaves enough time and health to do cool moves but not to run away the instant you are about to die. [SNIPPET] — [Edge via PressReader](https://www.pressreader.com/australia/edge/20211007/281741272586342)
- Splitgate's movement and gunplay "feel very much like classic shooters of yore – think Halo 2"; its developers measured basic movement timing such as turning against Halo 2; sprint is deliberately "not overwhelmingly fast". [SNIPPET] — [Last Minute Continue](https://lastminutecontinue.com/review/hands-on-splitgate/)
- Splitgate details reviewers call out: a fairly long kill time (but not as long as Halo Infinite's), mostly hitscan guns that make hitting lenient, and aim assist for console players only. [SNIPPET] — [Shacknews](https://shacknews.com/article/126314/how-does-aim-assist-work-in-splitgate); [Multiplayer.it](https://multiplayer.it/recensioni/splitgate-recensione.html)
- Splitgate 2 moved away: "Imagine Halo 3 with faster movement and a much quicker time-to-kill"; after its alpha the studio was "experimenting internally with a longer TTK". [SNIPPET] — [GamesRadar](https://gamesradar.com/games/fps/splitgate-2-is-faster-and-more-complex-but-portals-arent-the-star-of-the-show-anymore); [PCGamesN](https://www.pcgamesn.com/splitgate-2/closed-alpha-changes-new-maps)
- Halo Online (the cancelled Russian free-to-play game that ElDewrito unlocks) added sprint to Halo 3-style play; an ElDewrito developer said it is on "a variant of the Halo ODST engine", not Halo 3's, and that some things do not behave as in Halo 3 for that reason; the community's stated wish was "the original gameplay and feel". [SNIPPET] — [r/HaloOnline developer post](https://cal1.lr.ggtyler.dev/r/HaloOnline/comments/43m1h6/on_halo_3_and_other_things_tldr); [Halopedia: ElDewrito](https://www.halopedia.org/ElDewrito)
- Installation 01 is a fan game with its own assets whose gameplay is "very obviously akin to early Halo". [SNIPPET] — [Gamereactor](https://www.gamereactor.eu/installation-01-is-an-artistic-tribute-to-halo/)
- Official Halo failing the same test: MCC's Halo 2 fired some weapons at the wrong rate and moved projectiles half as far per tick because of a tick-rate difference; Halo 2 Vista had the same class of error. — [H2:Plasma Rifle](https://www.halopedia.org/H2:Plasma_Rifle); [Halo Support: MCC Update February 2024](https://support.halowaypoint.com/hc/en-us/articles/23707515298324-Halo-The-Master-Chief-Collection-MCC-Update-February-2024)
- Halo Infinite's departures that players named: instant strafe acceleration versus the classic "slight inertial tendency" (cut to 0.78 in February 2025), controller accuracy 12 to 16% above mouse, the flight's sprint-and-shoot-only radar (reverted), and a BR slower than Halo 5's that one analyst said "needs a buff". — [r/halo, August 2021](https://twt.omada.cafe/r/halo/comments/owg9wh/weekly_discussion_thread/h7kf337/?context=3); [Sportskeeda](https://www.sportskeeda.com/esports/halo-infinite-spring-update-release-date-features-explored); [MeinMMO](https://mein-mmo.de/en/fan-proves-as-controller-players-you-have-a-huge-advantage-in-halo-infinite,756017); [GameSpot](https://gamespot.com/articles/halo-infinite-makes-a-major-change-to-radar-after-the-beta/1100-6495958/); [Aozolai on X](https://x.com/Aozolai/status/1422895546809798658)
- Halo 5's beta drew the same notes and 343 answered them by speeding the strafe, raising base speed and slowing sprint. [DEV] — [MP1st](https://mp1st.com/news/heres-beta-feedback-helped-improve-halo-5-guardians)
- The Reach lesson: a utility weapon with bloom and no melee bleed-through was rejected by enough players that 343 shipped 85% and zero-bloom settings and restored bleed-through a year after launch. — [Halopedia: Halo: Reach Title Update](https://www.halopedia.org/Halo:_Reach_Title_Updates)

### Inferences
- A checklist of what a counts-and-rates clone of Halo 2 will be missing, each with a number above to compare against: (1) the 12th-bullet knife edge (72 damage against 70 shields); (2) three ranges per gun (max, instant-hit, autoaim) and 0.1 degrees of BR error; (3) bullet magnetism of 3 degrees on the rifle, on mouse as well; (4) strafe at 2.0 wu/s with inertia against a 0.3 wu wide body and 0.094 wu head; (5) velocity-scaled melee (75 / 55), bleed-through, the lunge on red reticle, and animation cancels (BXR, double shot); (6) the bounce-armed grenade (0.5 s after first bounce, minimum 1.3 s) at half damage to shields; (7) 5 s shield delay + 2 s recharge and invisible health regeneration after 10 s; (8) 30 Hz quantisation of every timer; (9) the shield pop as the loudest event in the fight; (10) BR starts and a 20 m radar.
- For a mouse-and-keyboard-first game the highest-risk items are (3) and (4): every official Halo number assumes magnetised bullets and, on controller, friction. Halo's own PC history (the Infinite input gap, the split ranked playlist, the 2025 strafe change) shows the developers never fully solved the same problem, so matching Halo's numbers exactly will reproduce Halo's mouse experience, which its mouse players complained about.
- "Feels like Halo" claims for clones are almost all impressions from reviews and developer intent; I found no published frame-data comparison of any clone against a Halo title.

### Gaps
- No detailed, sourced analysis of Installation 01's or ElDewrito's movement and aim values against Halo 3 was found; the above is intent and impression only.
- Splitgate's actual numbers (kill times, strafe speed, magnetism) were not retrieved.
- Several clone-related sources (Edge, Unreal Engine interview, Reddit mirrors) were seen only as search summaries.

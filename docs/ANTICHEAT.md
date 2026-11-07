# ANTICHEAT.md: the long-term plan for catching aim cheats

The plan of record for anti-cheat past what `DESIGN.md` §13 already ships
(the four lines in `room/src/watch.rs`, the review queue, the per-wallet ban
list). oneshot goes first: 128 Hz aim, replays that re-run to the server's
hash, and fog that knows the tick an enemy came into plain sight. Gates
follows (its `docs/ANTICHEAT.md`); the payout side is Elo Pros'
(`docs/store/PLAYER-PROFILES.md` in scry-forge). The sources, with what to
take from each, are `ANTICHEAT-SOURCES.md`.

Operator, 2026-11-07: *"we would require a profile on a player before we ever
payout from games. then we can catch if they deviate? … then doing that for
all should show some out of place distribution of cheaters."* That is the
spine. Nothing here is new as statistics (fraud detection has profiled
customers against themselves since the 1990s); what is ours is the
combination: exact replays, cheats laid over real play, bred adversaries, and
a detector we can open up.

## 1 · What authority already removed, and what is left

Authority, fog and the salt (§4, §8, §9) remove speedhacks, teleports, rapid
fire, wallhacks past fog, ESP past fog and no-spread. What is left is aim:
an aimbot or triggerbot, and a wallhack using what fog sends early. Every
input such a client sends is legal, so it can't be refused, only measured.

## 2 · Why one match can't decide, and many can

Take the best published detector's numbers (XGuardian: 90.7% recall at 4.1%
false positives) and a population where 1% cheat. Of 1,000 players, the 10
cheaters make about 9 flags and the 990 honest players about 40. **About 80%
of single-match flags are innocent.** A per-match verdict is not usable at
any threshold that also catches people.

Many matches fix it. Each match is weak, roughly independent evidence;
Wald's sequential probability ratio test multiplies the ratios. Twenty
matches that each lean 2:1 make about a million to one. A cheater can look
human in a match; looking human in all of them is the hard part. **Watching
a player over time is the strong signal**, and a profile before a payout is
how we get one.

## 3 · Three views, because each misses something

**1. You against your own past.** Real improvement is a slow ramp (practice,
a new mouse, a sensitivity change). A cheat switched on is a step: reaction
drops 80 ms overnight and stays. CUSUM and Bayesian online change-point
detection tell a step from a ramp. A toggler's own numbers split into two
clumps (on sessions and off); that split is a signal by itself.
*Blind spot:* a cheater from match one has a cheated baseline.

**2. Your aim against the rest of your game.** An aimbot raises aim. It
doesn't raise movement, positioning, rotations, grenades or decisions. Honest
players are skilled in matched amounts; a cheater sits off that surface (aim
at the 99th percentile, the rest at the 40th). This needs no history, so it
catches day-one cheaters. Fog gives the sharpest version: reaction better
than what the player could have seen.

**3. You against everyone.** Every profile, conditioned on skill bracket,
gun and map. Honest players make one smooth cloud; cheaters make a tail or a
second clump. A mixture model fitted to it estimates *how much* cheating
there is without accusing anyone: a health number for ranked, not a verdict.

**Same person?** A profile that suddenly belongs to someone else is a booster,
a sold account, or a banned cheater back on a fresh wallet whose new profile
matches the old one. All three matter for payouts.

## 4 · The lever: a ban costs the profile

Wallets are free (§13); profiles aren't. If a payout needs, say, 50 ranked
matches with a stable profile and no open review, every ban sends the
cheater back to zero. Detection only has to make that loop not worth
running. Today a cup prize already waits on its heats being watched with fog
off (`server/src/lib.rs`, `oneshot-review`); the profile gate is the same
hold, with a record behind it.

## 5 · Our own aimbots, bred

Cheats we build, on our own servers and in our own sim, are the detector's
training data. Three pieces:

- **The overlay.** A real aimbot *assists* a human. So the cheat is a function
  from an input stream to an input stream: it bends the aim a person sent
  toward a target and leaves everything else. Laid over a recorded match and
  re-run through `Room`, it gives **the same player, the same match, the same
  moment, with and without the cheat**. The only difference is the cheat.
  A detector trained on these pairs learns *assisted vs unassisted*, not
  *bot vs human* (the trap with bot-only data).
- **The genome.** An assist's knobs (field of view, smoothing, reaction delay,
  bone, deliberate miss rate, trigger only, pre-aim on fogged targets) as a
  genome in Gates' encoding (`sim-core/src/genome.rs`: a base seed and one
  16-bit seed per accepted mutation, Such et al. 2017), copied over under
  `RIP.md`'s rule. Every cheat in the zoo is a 280-character line that
  rebuilds exactly.
- **The arms race** (`oneshot-zoo`, after Gates' `examples/zoo.rs` and
  `examples/school.rs`). Fitness is advantage gained minus how hard the
  detector flags it. The archive is a quality-diversity grid (MAP-Elites, as
  Rainbow Teaming does for jailbreaks), one elite per style: blatant, closet,
  trigger only, fogged pre-aim, slow ramp. Each round retrains the detector
  on the grid, then breeds again.

Two numbers come out per round, and they answer "how strong is it":
**the largest advantage a cheat gets without a flag**, and **how many matches
it takes to catch a given cheat**. If the first falls and the second stays
small, the anti-cheat works, by a measured amount.

## 6 · Opening the detector (the neuron tools)

The detector is a small sequence model over aim windows (yaw and pitch
relative to the target, the fog's sight tick, the gun). The tools morr used
on Qwen (EXP-JUDAS, EXP-ZOO) work better here, because we hold ground truth:

- **Transplant (activation patching).** Swap the detector's middle layers
  between a cheated window and its clean twin; find the ticks that carry the
  call. `oneshot-review show` jumps there: "the lock was here, 40 ms after he
  came round the corner." That is the explanation a reviewer needs.
- **Probes and transcoders.** Readable features from inside the detector. When
  a bred cheat slips past, read which feature it learned to keep quiet; that
  is the gap to close.
- **JUDAS's lesson, as controls.** There the instruction was legible and the
  act wasn't, and a pooled result was Simpson's paradox. So the detector must
  beat a baseline that knows only gun, map and skill bracket; it is reported
  per gun and per map, never pooled; and it is compared with random-feature
  nulls. Otherwise it learns "plays the sniper on Cloudbreak".
- **The production cascade.** Cheap reads on every match, an expensive read
  on the few, a person on the flagged: `watch.rs`'s lines, then the model's
  ranking, then `oneshot-review`. Anthropic and DeepMind run their probes
  this way (about 5% of traffic to the second stage).

## 7 · The stages, by population, not by date

**Stage 0 — now, no players needed.**
- A profile row per player per match, beside the replay: `watch.json`
  extended with aim (reaction to plain sight, snap share, head rate,
  pre-aim, tracking smoothness) and the rest of the game (movement,
  positioning, utility, deaths taken to first shots).
- A profile store keyed on the wallet: on the server, those numbers only.
- Aim windows exported from replays; the overlay; `genome.rs` copied over.
- A first offline detector on bots with overlays, labelled as plumbing, not
  as a result.

**Stage 1 — alpha, the first real players.**
- The payout gate: a minimum of ranked matches and no open review.
- `watch.rs` keeps flagging; every review a person finishes is a label.
- Per-player change-point watch, flags only.
- First population plots, per gun and per map.
- **The measure:** how often reviewed honest players get flagged. Nothing is
  tightened until that number is known.

**Stage 2 — hundreds of players.**
- The aim-against-the-rest model (view 2).
- The mixture model and the estimate of how much cheating there is (view 3).
- A self-supervised player embedding: the same player's matches land close.
  That one model is the profile, the change-point watch and the matcher for
  a banned player on a new wallet (BEACON's direction).
- `oneshot-zoo` against all three views, plotting §5's two numbers.

**Stage 3 — thousands of players.**
- Overlays on real human replays at scale; the transformer detector.
- Transplants and probes feeding `oneshot-review`.
- **The operator's call:** one aim profile per wallet across every game on
  Elo Pros, as a platform service. Pitch/yaw models seem to carry across
  games (XGuardian), and a ban would cost the cheater every game at once.
  Powerful, and a larger privacy decision (scry-forge's
  `PLAYER-PROFILES.md`).

## 8 · Rules at every stage

- **Evidence, never a verdict.** Models rank; a person watches the replay and
  decides. A payout waits; a model never takes it away.
- **Declared agents have their own lane.** An agent behind a person's wallet
  should show up in view 1 as "not this person".
- **Minimal.** Keyed on the wallet; aim and play numbers only; on the server;
  deleted after a set time unless a case is open.
- **Said before payouts start.** The terms say play is profiled before a
  payout. Someone who knows the law checks whether a behavioural profile is
  biometric data where our players are.
- **Off the repo:** trained weights, the bred zoo and profile data. A
  published detector is one a cheat can be bred against, the same reason the
  salt never leaves the server. The code and this plan are public.

## 9 · What can still go wrong

- **Base rates.** §2. Everything accumulates over matches.
- **Honest steps.** A new monitor, controller to mouse, a long break. Review
  needs a place for "new setup on the 12th".
- **An always-on, slowly ramped, mild cheat** fools views 1 and 3. Only view 2
  and the zoo's frontier say how much advantage that buys.
- **Probes break under shift and against adaptive attackers** (the Gemini
  probes paper). The zoo exists to do the adapting first.
- **People are the limit.** Overlays and breeding need human play beneath
  them; every match from launch on is that data.

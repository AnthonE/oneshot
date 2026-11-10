# ANTICHEAT-SOURCES.md: what `ANTICHEAT.md` stands on

Gathered 2026-11-07; a second pass on 2026-11-10 added what the first
missed (marked *11-10*). Each entry says what it is, what we take, and how
far it was read. **Unless marked, only the abstract or a summary was read:**
check the paper before quoting a number from it. *mem* is weaker still: named
from memory, not looked up again. Numbers below are the authors' own claims,
not replicated by us.

## Aim-cheat detection, the current field

- **XGuardian** — Zhang, Sun, Qian (HKU). USENIX Security 2026; arXiv
  2601.18068. <https://arxiv.org/abs/2601.18068> ·
  <https://www.usenix.org/conference/usenixsecurity26/presentation/zhang-jiayi>
  Server-side, from replays, input is pitch and yaw only; temporal features of
  the aim trajectory; explains its calls. ~3,000 CS2 matches, tested on two
  more games. Claims 90.7% recall at 4.1% FPR. Code and data said released.
  *Take:* pitch/yaw is enough and carries across games; the 4.1% FPR is the
  base-rate example in `ANTICHEAT.md` §2; a pretraining set if its licence
  allows.
- **AntiCheatPT** — Loo, Luzkov, Burelli (ITU Copenhagen). arXiv 2508.06348,
  IEEE CoG 2025. <https://arxiv.org/abs/2508.06348>
  A transformer on CS2 kill windows; releases **CS2CD**, 795 labelled matches
  (90,707 windows, 18,423 cheater kills). Claims 89.17% accuracy, 93.36% AUC.
  *Take:* the window-per-kill framing; a public labelled set.
- **YAACS** — arXiv 2607.04336 (Pune Institute of Computer Technology, 2026).
  <https://arxiv.org/pdf/2607.04336>
  LSTMs over aim velocity, shots and distance on the server; a decision tree
  baseline; temporal context cuts false accusations. *Take:* a baseline.
- **BotScreen** — Choi et al. USENIX Security 2023.
  <https://usenix.org/conference/usenixsecurity23/presentation/choi>
  Client-side aimbot detection on CS:GO. *Take:* the client-vs-server
  trade-off, which we settled for the server.
- **VacNet 3.0** — Valve began testing it on a limited set of CS2 matches
  (release notes, as reported). No architecture paper.
  <https://dust2.in/news/51939/vacnet-30-rolled-out-for-initial-testing>
  *Take:* only that it exists.
- **VACnet, the talk** *(11-10)* — McDonald (Valve). "Robocalypse Now: Using
  Deep Learning to Combat Cheating in Counter-Strike: Global Offensive", GDC
  2018. <https://gdcvault.com/play/1024994/> The primary source there is:
  trained on Overwatch's verdicts (a jury of players watching replays),
  started with aimbots because a shot is a moment you can cut a window at,
  and sends its cases back to that jury (reported as convicting 80–95% of
  what it sent; press reports of the talk, not the talk itself, were read).
  *Take:* §8's "a model ranks, a person decides" is how the one deployed
  system of this kind works, and its labels were people's verdicts, which
  we do not have yet.
- **HAWK** *(11-10)* — Zhang et al. (HKU, XGuardian's group). arXiv
  2409.14830. <https://arxiv.org/abs/2409.14830> Server-side, several views
  of a CS:GO player at once; reports recall and accuracy in the 80s and
  tests against cheats that change. *Take:* the three views of §3 have a
  published cousin.
- **Chess** *(11-10, mem)* — Regan, Haworth. "Intrinsic Chess Ratings", AAAI
  2011: a player's moves against an engine's, as a z-score for their
  rating, added up over games, read by a person. Lichess's Irwin
  (<https://github.com/clarkerubber/irwin>) is a net that ranks accounts
  for its moderators. *Take:* the longest-running practice of §2 and §8:
  evidence over a career, a person deciding.

## Cheater against detector

- **GAN-Aimbots** *(11-10)* — Kanervisto, Kinnunen, Hautamäki. IEEE
  Transactions on Games 2022; arXiv 2205.07060.
  <https://arxiv.org/abs/2205.07060> An aimbot whose mouse movement is
  learned from people's (a GAN), laid on a person's own play: it helped,
  and it got past both automatic detection and people watching. The
  nearest published thing to §5's zoo, from the cheat's side. *Take:* a
  cheat can be learned, not only set by knobs, so the zoo breeds a net as
  well (`room::assist::Cheat`, `on1:`); and its warning, that movement
  copied from people is the hard case, is what our bots' hands cannot
  test.
- **The hall of fame** *(11-10, mem)* — Rosin, Belew. "New Methods for
  Competitive Coevolution", *Evolutionary Computation* 5(1), 1997. Two
  sides bred against each other's latest go in circles; keeping the old
  champions as opponents stops it. AlphaStar's league (Vinyals et al.,
  *Nature* 575, 2019) is the same remedy at scale. *Take:* `zoo.py`
  `--hall`.
- **Evolution finds the watcher** *(11-10)* — Lehman et al. "The Surprising
  Creativity of Digital Evolution", *Artificial Life* 26(2), 2020; arXiv
  1803.03453. <https://arxiv.org/abs/1803.03453> Ofria's Avida organisms,
  measured in a separate test run, came to recognise the test and stop
  replicating in it (read in summaries, not the paper). *Take:* a bred
  cheat judged only where the detector looks will learn where it looks;
  morr's bred golfers (EXP-EDEN-SEL2) did the same thing.

- **Adversarial RL for ESP cheater simulation** — KRAFTON and KAIST. arXiv
  2509.24274 (Sep 2025, revised Dec). <https://arxiv.org/abs/2509.24274>
  Cheater and detector as a minimax game, both adapting; the detector
  classifies trajectories; names the lack of labels as the main obstacle.
  *Take:* the arms race in `ANTICHEAT.md` §5 is a published design.
- **ShellForge** — arXiv 2601.22182 (Jan 2026). <https://arxiv.org/abs/2601.22182>
  Webshells, not games: a generator and detector trade hard samples over
  rounds. *Take:* the round structure.
- **Rainbow Teaming** — Samvelyan et al. NeurIPS 2024; arXiv 2402.16822.
  <https://arxiv.org/abs/2402.16822>
  Adversarial prompts as quality-diversity search on MAP-Elites: the best
  attack of every style, not one champion; training on the archive hardens
  the target. Follow-ups: **QDRT** (arXiv 2506.07121,
  <https://arxiv.org/abs/2506.07121>) and **RainbowPlus** (arXiv 2504.15047,
  <https://arxiv.org/abs/2504.15047>; several elites per cell).
  *Take:* the zoo's archive is a style grid, not a leaderboard.
- **MAP-Elites** — Mouret, Clune. "Illuminating search spaces by mapping
  elites", arXiv 1504.04909 (2015). <https://arxiv.org/abs/1504.04909>
  *Take:* the archive itself.
- **Deep Neuroevolution** — Such et al., arXiv 1712.06567 (2017).
  <https://arxiv.org/abs/1712.06567>
  The seed-chain genome encoding Gates' `sim-core/src/genome.rs` and Quake
  Golf use. *Take:* a whole bred cheat fits in 280 characters and rebuilds
  bit for bit.

## Telling a player by how they play

- **BEACON** — a multimodal gameplay dataset for behavioural fingerprints.
  arXiv 2605.10867 (2026). <https://arxiv.org/pdf/2605.10867>
  Proposes self-supervised behavioural foundation models; names mimicry
  attacks (copying a target's aim curvature) as the open risk. *Take:* the
  player embedding of Stage 2.
- **Adversarial mouse trajectories** — arXiv 1905.11831.
  <https://arxiv.org/abs/1905.11831>
  Generated trajectories bypass mouse-dynamics authentication. *Take:* a
  fingerprint can be forged, so it is evidence, never a key.
- **Keystroke dynamics, the start of the field** — Gaines, Lisowski, Press,
  Shapiro. "Authentication by Keystroke Timing", RAND R-2526-NSF (1980).
  <https://www.rand.org/pubs/reports/R2526.html>
- **Statistical fraud detection: a review** — Bolton, Hand. *Statistical
  Science* 17(3), 2002. <https://doi.org/10.1214/ss/1042727940>
  Profiling an account against its own history and its peers. *Take:* views 1
  and 3 of `ANTICHEAT.md` §3 are this, applied to aim.

## A question an honest player cannot answer

- **Dota 2's honeypot** *(11-10, mem)* — Valve, "Cheaters will never be
  welcome in Dota", February 2023: a part of the game's data no honest
  client ever reads, read by cheats; some 40,000 accounts banned on it.
  Older anti-cheats sent fake players for the same reason. *Take:*
  `room::decoy`. Ours is weaker evidence a time (an aim that leans, not a
  read that is certain), so it is paired with a control and added up.
- **Bait for a cheat that reads the screen** *(11-10)* — AimTrap, arXiv
  2606.25734 (<https://arxiv.org/abs/2606.25734>): textures that hide a
  player from a vision aimbot and decoys that draw it. Honeytoken patches,
  arXiv 2606.07650 (<https://arxiv.org/abs/2606.07650>): small pictures a
  cheat's object detector takes for a player. *Take:* a decoy on the wire
  catches only a cheat that reads the wire; these are the other half, not
  built here.

## Accumulating evidence and finding the step

- **Sequential tests** — Wald. "Sequential Tests of Statistical Hypotheses",
  *Annals of Mathematical Statistics* 16(2), 1945.
  <https://doi.org/10.1214/aoms/1177731118>
  *Take:* §2's arithmetic: weak per-match evidence multiplied to a decision,
  with error rates set in advance.
- **Looking after every match** *(11-10, mem)* — Ville, *Étude critique de la
  notion de collectif*, 1939 (the inequality); Robbins, "Statistical
  methods related to the law of the iterated logarithm", *Annals of
  Mathematical Statistics* 41(5), 1970 (a likelihood ratio mixed over
  alternatives); Howard, Ramdas, McAuliffe, Sekhon, "Time-uniform,
  nonparametric, nonasymptotic confidence sequences", *Annals of
  Statistics* 49(2), 2021. *Take:* `players.py`'s odds. A p-value read
  after every match and acted on the first time it is small is not Wald's
  test: ours opened a case on up to 1% of honest careers at a line of
  0.1%, measured (`players.py --selftest`).
- **The Athlete Biological Passport** *(11-10, mem)* — Sottas, Robinson,
  Rabin, Saugy. *Clinical Chemistry* 57(7), 2011. An athlete's own blood
  values over years as their baseline; a departure from it goes to a
  panel, never straight to a ban. *Take:* view 1 of §3 and §8, practised
  for fifteen years where the stakes are careers.
- **CUSUM** — Page. "Continuous Inspection Schemes", *Biometrika* 41(1/2),
  1954. <https://doi.org/10.1093/biomet/41.1-2.100>
  *Take:* the cheapest change detector for a per-player stream.
- **Bayesian online change-point detection** — Adams, MacKay. arXiv 0710.3742
  (2007). <https://arxiv.org/abs/0710.3742>
  *Take:* step vs ramp with a run-length posterior; tells a cheat switched on
  from a player improving.

## Reading inside a model

- **Mechanistic interpretability for time-series transformers** — Kalnāre et
  al. (LIACS). arXiv 2511.21514 (2025). <https://arxiv.org/abs/2511.21514>
  Activation patching, attention saliency and SAEs on time-series
  classifiers; class-specific temporal motifs. Attention heads only.
  *Take:* the transplant in `ANTICHEAT.md` §6 has been done on this kind of
  model.
- **Transcoders find interpretable LLM feature circuits** — Dunefsky,
  Chlenski, Nanda. arXiv 2406.11944 (2024). <https://arxiv.org/abs/2406.11944>
- **Transcoders beat sparse autoencoders for interpretability** — arXiv
  2501.18823 (2025). <https://arxiv.org/abs/2501.18823>
  *Take:* transcoders over SAEs for the detector's features.
- **Auditing for a hidden objective** *(11-10)* — Marks et al. (Anthropic).
  arXiv 2503.10965. <https://arxiv.org/abs/2503.10965> One team trains a
  model with a planted aim and hands it over; blinded teams try to find
  it. Three of four did; the one without the training data did not.
  *Take:* the design of §5's ground truth (we wrote the hidden channel) and
  of Gates' `ops/openheart`; and that what a reader is given decides more
  than the reader.
- **Detecting high-stakes interactions with activation probes** — McKenzie et
  al. NeurIPS 2025; arXiv 2506.10805. <https://arxiv.org/abs/2506.10805>
  Probes match prompted or fine-tuned mid-sized LLM monitors at about six
  orders of magnitude less compute; recommends probes first, uncertain cases
  routed on. *Take:* the cascade.
- **Building production-ready probes for Gemini** — Google DeepMind. arXiv
  2601.11516 (2026). <https://arxiv.org/abs/2601.11516>
  Cascades match heavier classifiers; probes trained short fail on long
  context; do not stop adaptive attacks. *Take:* §9's warning.
- **Anthropic's production probes** (Claude Sonnet 4): linear probes on all
  layers, all traffic screened, about 5.5% sent to a second stage. Known to us
  only through a summary:
  <https://www.lesswrong.com/posts/JcAm6MFog6ssKooFN/ai-safety-at-the-frontier-paper-highlights-of-january-2026>
- **Our own** (morr, private): EXP-JUDAS-1/1b/1c (lie probes on Qwen2.5 MLP
  neurons; instruction legible, act not; the pooled Simpson's paradox),
  EXP-JUDAS-PLAY (the layer transplant), EXP-ZOO-1/2 (single-neuron steering
  against matched random neurons). Logged in morr's
  `private/notes/decisions.md`, 2026-10-15/16/26. *Take:* the controls in §6.

# ANTICHEAT-SOURCES.md: what `ANTICHEAT.md` stands on

Gathered 2026-11-07. Each entry says what it is, what we take, and how far
it was read. **Unless marked, only the abstract or a summary was read:** check
the paper before quoting a number from it. Numbers below are the authors'
own claims, not replicated by us.

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
  (release notes, as reported). No primary architecture paper; the deep-
  learning details online are secondary or promotional.
  <https://dust2.in/news/51939/vacnet-30-rolled-out-for-initial-testing>
  *Take:* only that it exists.

## Cheater against detector

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

## Accumulating evidence and finding the step

- **Sequential tests** — Wald. "Sequential Tests of Statistical Hypotheses",
  *Annals of Mathematical Statistics* 16(2), 1945.
  <https://doi.org/10.1214/aoms/1177731118>
  *Take:* §2's arithmetic: weak per-match evidence multiplied to a decision,
  with error rates set in advance.
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

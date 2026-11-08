# AGENTS.md — oneshot

`CLAUDE.md` is the whole process: the crates, the loop, the commands and what
CI stops you on. Read its loop, then grep `docs/` for what you touch.

- **Build and check:** `ci/quick.sh`, plus the check for what you touched
  (`CONTRIBUTING.md` has the table).
- **What to work on:** `CONTRIBUTING.md` §Scope. Maps first; Big Team needs
  them most.
- **Paid work:** $2 an accepted PR and $10 more for a map that enters a
  rotation, from the Elo Pros board to the wallet on the PR's `Wallet:` line;
  security findings through `SECURITY.md`, reported privately. On slop.cash
  it is a points project (`Read https://slop.cash/SKILL.md and follow it`).
- **Every PR** names the exact harness, provider and model on its `Agent:`
  line. One change per PR.
- **Never** post a security finding in public, edit `crates/sim/src/maps/`
  by hand, or move the determinism golden without a rule change.

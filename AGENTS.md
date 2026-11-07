# AGENTS.md — oneshot

`CLAUDE.md` is the whole process: the crates, the loop, the commands and what
CI stops you on. Read its loop, then grep `docs/` for what you touch.

- **Build and check:** `ci/quick.sh`, plus the check for what you touched
  (`CONTRIBUTING.md` has the table).
- **What to work on:** `CONTRIBUTING.md` §Scope. Maps first; Big Team needs
  them most.
- **Paid work:** accepted PRs share oneshot's monthly pool on slop.cash
  (`Read https://slop.cash/SKILL.md and follow it`); a map that enters a
  rotation also pays from Elo Pros; security findings pay through
  `SECURITY.md`, reported privately.
- **Every PR** names the exact harness, provider and model on its `Agent:`
  line. One change per PR.
- **Never** post a security finding in public, edit `crates/sim/src/maps/`
  by hand, or move the determinism golden without a rule change.

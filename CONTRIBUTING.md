# Contributing to oneshot

oneshot is a free arena shooter for the browser and the desktop: 1v1 and 2v2
ranked, casual modes, and 8v8 Big Team with vehicles. The server runs every
match and sends each player only what they could see. The sim is
deterministic, so every match is a replay that re-runs to the server's hash.
That makes almost every claim about this game checkable by anyone, which is
what the paid work below is built on.

People and agents are equally welcome. Use any harness and any model, and say
which in the PR (below).

## What gets paid

| work | what counts | pay |
|---|---|---|
| **An accepted pull request** | anything in scope below, merged | **$2** |
| **A map into rotation** | a new map that enters a rotation (Duel, Doubles, FFA, CTF or Big Team) | **$10** more, once, when it enters, and your name on the map's card. Big Team maps are the gap: there is one |
| **A security finding** | a confirmed break of the guarantees in [SECURITY.md](SECURITY.md), reported privately | **$20 / $15 / $10** by severity |

**How you get paid.** Put your wallet in the PR's description
(`Wallet: 0x…`). When the PR merges, the Elo Pros board pays it in ELO at the
pool's price on the day it pays; `curl -s "https://elopros.com/api/munus?game=oneshot"`
shows the posted prices. Or sign in at https://elopros.com/login.html with the
GitHub that opened the PR and link a wallet there. Nothing is owed until the
work is merged.

**On slop.cash** oneshot is a points project: merged PRs and useful reviews
score on its leaderboard there, and its agent door is
`Read https://slop.cash/SKILL.md and follow it to contribute to github.com/AnthonE/oneshot.`
The money above comes from Elo Pros, not from a Slop pool.

## Scope

**In scope**

- **Maps.** `maps/<name>/map.json`, compiled by `node tools/maps/compile.mjs`.
  Read `docs/maps/cloudbreak.md` for what a map design says (modes, bounds,
  levels, sightlines), and `cargo run -p flow --release -- <map>` for its
  flow. A map of 4,000 m² or more is a Big Team map; the rest are arenas.
- **Bugs you can show**: a failing test, a replay, or a trace that the fix
  turns green.
- **Anti-cheat stage 0** (`docs/ANTICHEAT.md` §7): the per-match profile row
  beside the replay (aim and the rest of the game), aim windows exported from
  replays, bot overlays and an offline detector on bots, labelled as plumbing.
  Trained weights, bred aimbots and profile data stay off the repo (§8).
- **Bots**: bots that fight better on a map, without reading what fog hides.
- **The client on real hardware**: a GPU, browser or OS where the page fails,
  fixed with the device named.
- **Milestones** in `docs/DESIGN.md` §16 and Siege's phases in §18.

**Out of scope**, closed by quoting this list

- UI or copy changes, unless they fix a broken flow.
- Tuning a gun's numbers: `cargo test -p sim --test range` pins them.
- New modes, weapons or cards not in `docs/DESIGN.md`.
- Docs-only PRs.
- Assets that are not CC0, CC-BY (credited in NOTICE) or your own. Never
  another game's art, nothing NC or SA.
- Edits to vendored code: `crates/fx`, `tools/fxstrip.mjs`, `tools/board.mjs`.

## The check a PR must pass

`ci/quick.sh` green, then what you touched (the full list is in `CLAUDE.md`):

| touched | run |
|---|---|
| the sim | `ci/parity.sh` (native and wasm hash equal) |
| a map | `node tools/maps/compile.mjs --check` and `cargo test -p sim --test maps` |
| the wire | bump `PROTO_VER`; `crates/proto/tests/goldens.rs` moves with it |
| anything a client receives | `cargo test -p room --test leaks` |
| the client or netcode | `ci/client.sh`, then `node ci/e2e.mjs` |

## What gets a PR closed

- Its check is not green, or it was not run.
- `crates/sim/src/maps/` edited by hand instead of the map's `map.json`.
- The determinism golden moved without a rule change that explains it.
- A wire change without a `PROTO_VER` bump.
- A new field to clients without a line in the leak gate.
- More than one change in one PR.
- A security finding posted in public. Report it privately (SECURITY.md).
- No `Agent:` line on agent-written work.

## The PR

Fill in the template: what changed, the check you ran, `Agent:` with the
exact harness, provider and model (or `none`), and `Wallet:` if you want the
map payment. One change per PR. Maintainers merge; nobody needs to claim an
issue first.

## Reviews

A review that finds a real problem or confirms a change is welcome as a PR
comment, and Slop scores useful reviews.

# Security and cheating

oneshot makes three promises, and each one is checkable because the sim is
deterministic and every match replays to the server's hash:

1. **The server decides.** A client sends inputs. Movement, fire rate, damage,
   pickups and results are the server's.
2. **A client is sent only what it could see** (`crates/room/src/fog.rs`). A
   wallhack has nothing to draw, and the salt that keys spread never leaves
   the server until the match is over.
3. **Lag compensation is bounded**: no shot lands with more than 160 ms of
   favour, and a client cannot claim an ack the server has not seen.

Aimbots and triggerbots are not here: every input they send is legal, so they
are measured over many matches, not prevented (`docs/ANTICHEAT.md`).

## Paid findings

Paid through Slop's private security rail: the researcher receives 75% and
slop.cash 25%. One award per underlying finding, to its first valid report.

| severity | what | award |
|---|---|---|
| **Critical** | a client learns what fog should hide (an unseen enemy, round, event or medal) through the real client path; the salt before the match ends; a forged match result or rating accepted by the server or reported to Elo Pros; playing ranked as a wallet you don't hold | **$2,000** |
| **High** | breaking the server's authority (speed, rate of fire, damage, a pickup out of reach); a shot landing past the 160 ms bound; a replay from legal inputs that does not re-run to the server's hash; wearing a skin the wallet does not own | **$750** |
| **Medium** | one client's crafted traffic (not volume) crashes or stalls a room or a server for everyone in it | **$250** |

The program holds $5,000. When it is spent it pauses until it is refilled,
and the page says so.

## How to report

- **Once oneshot is listed on slop.cash:** the project's *Report privately*
  form, signed in with your GitHub.
- **Until then:** GitHub's private vulnerability report on this repository
  (Security, Report a vulnerability).

Never in a public issue, PR, comment or test name.

A good report names the commit, the steps, and the evidence: a replay, a
client log, or best of all a failing test in the shape of
`crates/room/tests/leaks.rs`. It names the exact provider, model and client
if an agent helped.

## Rules for testing

- Test against your own server: `cargo build --release -p server &&
  target/release/oneshot-server`, with your own clients and bots.
- Never against matches with other people in them, and never volume.
- No one else's account, wallet or data.
- Out of scope: aimbots and triggerbots, volumetric denial of service,
  elopros.com and other services we call, and social engineering.

We fix first and disclose after, with credit if you want it.

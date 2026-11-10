#!/usr/bin/env python3
"""A player's evidence over matches (docs/ANTICHEAT.md §2): the detector's
flags on every sighting of a league, added up over each player's career.

    PY=/data/LLM/.venv/bin/python
    $PY tools/aim/players.py traces/aim/<league> --detector traces/aim/<set>/run/detector.pt

`oneshot-aim league` seats a fixed population match after match, some of
them cheating every match with one assist. Here:

- **The line is set on honest players.** Half the honest players, picked at
  random, set it: the score above 1% of their windows, as a review would set
  its line on players it has already cleared. The other half and every
  cheater are judged by it.
- **A player's evidence** after each match is the odds their flags so far
  give against the honest 1%: a likelihood ratio, mixed over the rates a
  cheater might flag at (Wald's sequential test, with no rate guessed). It
  is looked at after every match, so it has to stay honest under looking:
  the chance an honest player's odds *ever* reach 1/`--p` is at most `--p`
  (0.001), however long they play. A binomial tail read after every match
  is not that: at 0.001 it opened a case on up to 1% of honest careers
  (`--selftest` shows both).
- **`--null`** is the honest rate tested against (default the line's own
  1%). Honest players don't all flag at the line's rate, so before this
  decides anything it wants a margin read off the spread printed below.
- **What it reports:** how well players are told apart after 1, 3, 10
  matches and the whole career; each cheater's matches to a case; honest
  players with a case (false accusations); and the honest flag rates' spread.
  If honest players don't each flag at 1%, sightings aren't independent
  coin flips, and the line needs a margin before any of this is used.

On bots with hands, plumbing; the method is what carries to people.
"""

import argparse
import json
import math
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

FPR = 0.01
# The flag rates a cheater might show, as multiples of the honest rate: the
# test's alternative is all of them at once, equally weighted.
RATES = (2.0, 4.0, 8.0, 16.0, 32.0, 64.0)


def odds(k, n, p0=FPR, alts=None):
    """The odds `k` flags in `n` sightings give against an honest player who
    flags at `p0` or less: the likelihood ratio, averaged over the rates a
    cheater might show (`alts`, or `p0` times each of [`RATES`]).
    Under honest play it is a supermartingale that starts at 1, so the
    chance it ever reaches `x` is at most 1/x (Ville's inequality): it may
    be read after every match."""
    if n <= 0:
        return 1.0
    logs = []
    for p1 in alts or [min(p0 * r, 0.95) for r in RATES]:
        if p1 <= p0:
            continue
        logs.append(k * math.log(p1 / p0) + (n - k) * math.log((1 - p1) / (1 - p0)))
    top = max(logs)
    return math.exp(min(700.0, top + math.log(sum(math.exp(x - top) for x in logs) / len(logs))))


def tail(k, n, p=FPR):
    """P(X >= k) for X ~ Binomial(n, p), in logs so a long career doesn't
    underflow."""
    if k <= 0:
        return 1.0
    lp, lq = math.log(p), math.log1p(-p)
    base = math.lgamma(n + 1)
    total, i = 0.0, k
    while i <= n:
        t = math.exp(base - math.lgamma(i + 1) - math.lgamma(n - i + 1) + i * lp + (n - i) * lq)
        total += t
        if i > n * p and t < total * 1e-12:
            break
        i += 1
    return min(1.0, total)


def selftest(p=1e-3, players=20000, seed=0):
    """Honest players at exactly the line's rate, looked at after every
    match: how many ever get a case, by the odds and by the binomial tail
    this replaced; then how soon cheaters do."""
    rng = np.random.default_rng(seed)
    print(f"{players} honest players flagging at {FPR:.0%}, a case at {p:g}:")
    worst = 0.0
    for per, matches in ((20, 50), (40, 50), (40, 200)):
        k = rng.binomial(per, FPR, size=(players, matches)).cumsum(1)
        by_odds = by_tail = 0
        seen = {}
        for row in k:
            o = t = False
            for m in range(matches):
                key = (int(row[m]), per * (m + 1))
                if key not in seen:
                    seen[key] = (odds(*key) >= 1 / p, tail(*key) < p)
                o, t = o or seen[key][0], t or seen[key][1]
                if o and t:
                    break
            by_odds += o
            by_tail += t
        worst = max(worst, by_odds / players)
        print(f"  {per} sightings a match, {matches:>3} matches: a case for {by_odds / players:.3%} by the odds, "
              f"{by_tail / players:.2%} by the tail read every match")
    print("cheaters, 40 sightings a match, median matches to a case (by the odds, by the tail):")
    for rate in (0.03, 0.05, 0.10, 0.25):
        k = rng.binomial(40, rate, size=(2000, 200)).cumsum(1)
        at = [[next((m + 1 for m in range(200) if hit(int(row[m]), 40 * (m + 1))), 201) for row in k]
              for hit in (lambda k, n: odds(k, n) >= 1 / p, lambda k, n: tail(k, n) < p)]
        print(f"  flagged {rate:.0%}: {np.median(at[0]):.0f}, {np.median(at[1]):.0f}")
    if worst > p:
        sys.exit(f"the odds opened a case on {worst:.3%} of honest players, over {p:.3%}")
    print("ok")


# A decoy episode where the aim came to sit on one spot only: for an honest
# player, the sent one half the time. These are the rates a cheat might
# show. (Which spot the aim merely drew nearer is printed beside it: every
# episode has one, but an assist that didn't lock leaves it a coin flip,
# and that drowns the ones that did.)
DECOY_ALTS = (0.7, 0.8, 0.9, 0.97)


def decoys(league, p):
    """A league played with `oneshot-aim league --decoys`: each player's
    decoy episodes added up, match by match, and the odds they give that
    the sent spot draws the aim more than the held one. No detector."""
    info = json.load(open(os.path.join(league, "info.json")))
    roster = {r["player"]: r for r in info["league"]}
    tot = {q: [0, 0, 0, 0, 0] for q in roster}
    case = {}
    for mi, m in enumerate(info["matches"]):
        for q, t in zip(m.get("players", []), m.get("decoys", [])):
            tot[q] = [a + b for a, b in zip(tot[q], t)]
            if q not in case and odds(tot[q][3], tot[q][3] + tot[q][4], 0.5, DECOY_ALTS) >= 1 / p:
                case[q] = sum(q in x.get("players", []) for x in info["matches"][: mi + 1])
    groups = {"honest": [], "cheating, through walls": [], "cheating, in sight only": []}
    for q, r in roster.items():
        g = "honest" if r["cheat"] is None else "cheating, through walls" if r.get("walls") else "cheating, in sight only"
        groups[g].append(q)
    print(f"{league}: decoys, a case at odds of {1 / p:g}:1 that the sent spot draws the aim")
    for g, qs in groups.items():
        e, s, h, ts, th = (sum(tot[q][i] for q in qs) for i in range(5))
        print(f"{g} ({len(qs)} players): {e} episodes; the aim sat on the sent spot only {ts}, the held one only {th} "
              f"(all of them at once, {odds(ts, ts + th, 0.5, DECOY_ALTS):.3g}:1); drawn toward the sent one {s}, "
              f"the held one {h}; {sum(q in case for q in qs)} with a case")
        for q in sorted(qs, key=lambda q: case.get(q, 1 << 30)):
            if g != "honest" or q in case:
                at = f"case after {case[q]} matches" if q in case else "no case"
                print(f"  player {q:>2} {roster[q].get('style') or '':<8} sat on sent {tot[q][3]:>2}, held {tot[q][4]:>2} "
                      f"({odds(tot[q][3], tot[q][3] + tot[q][4], 0.5, DECOY_ALTS):.3g}:1): {at}")
    json.dump({"p": p, "totals": tot, "case_at": case}, open(os.path.join(league, "decoys.json"), "w"), indent=1)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("league", nargs="?")
    ap.add_argument("--detector")
    ap.add_argument("--p", type=float, default=1e-3,
                    help="a case opens at odds of 1/p: at most this share of honest players ever get one")
    ap.add_argument("--null", type=float, default=FPR, help="the honest flag rate tested against (the line's, or it with a margin)")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--selftest", action="store_true", help="the test on made-up players, no detector or torch")
    ap.add_argument("--decoys", action="store_true", help="the league's decoys (oneshot-aim league --decoys), no detector")
    args = ap.parse_args()
    if args.selftest:
        return selftest(args.p)
    if args.decoys:
        return decoys(args.league, args.p)
    if not args.league or not args.detector:
        ap.error("a league and --detector (or --selftest)")
    import torch

    import train as det
    import zoo

    ck = torch.load(args.detector, map_location=det.DEV, weights_only=False)
    w, meta, info, col = zoo.load(args.league)
    if info["from"] != "league":
        sys.exit(f"{args.league}: not a league (oneshot-aim league)")
    model = det.Detector(w.shape[2], ck["guns"], ck["maps"], ck.get("width", 64)).to(det.DEV)
    model.load_state_dict(ck["state"])
    score = zoo.score(model, w, meta, col, ck["mu"], ck["sd"], ck["guns"], ck["maps"])

    player, cheat, match = meta[:, col["player"]], meta[:, col["label"]] == 1, meta[:, col["match"]]
    roster = {r["player"]: r for r in info["league"]}
    honest = sorted(p for p, r in roster.items() if r["cheat"] is None)
    cheaters = sorted(p for p, r in roster.items() if r["cheat"] is not None)
    rng = np.random.default_rng(args.seed)
    calib = set(rng.permutation(honest)[: len(honest) // 2].tolist())
    judged_honest = [p for p in honest if p not in calib]
    in_calib = np.isin(player, list(calib))
    thr = float(np.quantile(score[in_calib], 1 - FPR))
    flag = score > thr
    print(f"{args.league}: {len(w)} windows, {len(roster)} players ({len(cheaters)} cheating), "
          f"{match.max() + 1} matches; the line set on {len(calib)} honest players")

    # Each player's career, match by match: sightings, flags, mean score.
    careers = {}
    for p in roster:
        rows = np.where(player == p)[0]
        ms = sorted(set(match[rows].tolist()))
        steps, n, k = [], 0, 0
        for m in ms:
            r = rows[match[rows] == m]
            n += len(r)
            k += int(flag[r].sum())
            steps.append({"match": m, "n": n, "k": k, "odds": odds(k, n, args.null), "mean": float(score[r].mean())})
        careers[p] = steps

    def case_at(p):
        return next((i + 1 for i, s in enumerate(careers[p]) if s["odds"] >= 1 / args.p), None)

    report = {"league": args.league, "detector": args.detector, "threshold": thr, "p": args.p, "null": args.null}
    judged = judged_honest + cheaters
    yw = np.isin(player, cheaters)
    sel = np.isin(player, judged)
    report["window_auc"] = det.auc(score[sel], yw[sel])
    by_m = {}
    for m in (1, 3, 5, 10, 20, None):
        xs, ys = [], []
        for p in judged:
            c = careers[p] if m is None else careers[p][:m]
            if not c or (m is not None and len(careers[p]) < m):
                continue
            last = c[-1]
            xs.append(last["k"] / max(1, last["n"]))
            ys.append(p in cheaters)
        if len(set(ys)) == 2:
            by_m["career" if m is None else str(m)] = {"auc": det.auc(np.array(xs), np.array(ys)), "players": len(xs)}
    report["player_auc_by_matches"] = by_m

    rows = []
    for p in cheaters:
        c, r = careers[p], roster[p]
        rows.append({"player": p, "style": r["style"], "genome": r["genome"], "matches": len(c),
                     "flag_rate": c[-1]["k"] / max(1, c[-1]["n"]) if c else 0.0, "case_at": case_at(p)})
    report["cheaters"] = rows
    hon = []
    for p in judged_honest:
        c = careers[p]
        hon.append({"player": p, "matches": len(c), "flag_rate": c[-1]["k"] / max(1, c[-1]["n"]) if c else 0.0,
                    "case_at": case_at(p)})
    report["honest"] = hon
    calib_rates = [careers[p][-1]["k"] / max(1, careers[p][-1]["n"]) for p in calib if careers[p]]
    report["calibration_rates"] = calib_rates
    json.dump(report, open(os.path.join(args.league, "players.json"), "w"), indent=1)

    print(f"one sighting: AUC {report['window_auc']:.3f}")
    print("a player, by flag rate after N matches: " + ", ".join(
        f"{k} {v['auc']:.3f}" for k, v in by_m.items()))
    print(f"cheaters (a case at odds of {1 / args.p:g}:1 against an honest {args.null:.1%}):")
    for r in sorted(rows, key=lambda r: (r["case_at"] is None, r["case_at"] or 0)):
        at = f"case after {r['case_at']} of {r['matches']} matches" if r["case_at"] else f"no case in {r['matches']} matches"
        print(f"  player {r['player']:>2} {r['style']:<8} {r['genome']:<12} flags {r['flag_rate']:>5.1%}  {at}")
    rates = np.array([h["flag_rate"] for h in hon])
    falsely = [h for h in hon if h["case_at"]]
    print(f"honest, judged ({len(hon)}): flag rate median {np.median(rates):.1%}, "
          f"from {rates.min():.1%} to {rates.max():.1%}; {len(falsely)} with a case"
          + (": " + ", ".join(f"player {h['player']} after {h['case_at']}" for h in falsely) if falsely else ""))
    cr = np.array(calib_rates)
    print(f"honest, the line's own ({len(cr)}): from {cr.min():.1%} to {cr.max():.1%} (pooled 1% by construction)")
    caught = [r for r in rows if r["case_at"]]
    if caught:
        print(f"{len(caught)} of {len(rows)} cheaters with a case, after a median of "
              f"{np.median([r['case_at'] for r in caught]):.0f} matches")
    print(f"-> {args.league}/players.json")


if __name__ == "__main__":
    main()

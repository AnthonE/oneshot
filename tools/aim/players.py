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
- **A player's evidence** after each match is their flags so far against
  their sightings so far, as a binomial tail at the honest 1%: the chance an
  honest player flags that often. A case opens below `--p` (0.001).
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
import torch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import train as det  # noqa: E402
import zoo  # noqa: E402

FPR = 0.01


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


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("league")
    ap.add_argument("--detector", required=True)
    ap.add_argument("--p", type=float, default=1e-3, help="a case opens below this chance for an honest player")
    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args()

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
            steps.append({"match": m, "n": n, "k": k, "p": tail(k, n), "mean": float(score[r].mean())})
        careers[p] = steps

    def case_at(p):
        return next((i + 1 for i, s in enumerate(careers[p]) if s["p"] < args.p), None)

    report = {"league": args.league, "detector": args.detector, "threshold": thr, "p": args.p}
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
    print(f"cheaters (a case at p < {args.p:g} under the honest 1%):")
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

#!/usr/bin/env python3
"""The zoo's arms race (docs/ANTICHEAT.md §5): our assists bred against the
detector, and the detector trained on what survives, round after round.

    PY=/data/LLM/.venv/bin/python    # torch and numpy
    $PY tools/aim/zoo.py --base traces/aim/<set> --out traces/aim/zoo [--rounds 8] [--hands]

`--base` is a set `oneshot-aim bots` cut (the detector's first lessons).
Each round:

1. **Candidates:** every elite in the archive, `--children` mutants of each
   (its genome with one more 16-bit seed: Gates' encoding, so a child is
   its parent plus one step) and `--fresh` new genomes.
2. **The same moments for all:** every candidate laid on every sighting of
   fresh bot matches (`oneshot-aim bots --each`, a new seed a round), so
   what each gains over the clean window is paired, moment for moment.
3. **Fitness:** hits gained a sighting, less `--lam` times the share of its
   windows the detector flags at 1% of clean windows flagged.
4. **The archive** (MAP-Elites): a cell per style, per aim point (head or
   body) and per whether it picks its moments (past a range, or only with
   its shields down: what its client can see); each keeps the fittest candidate of the round, so the zoo stays a
   spread of kinds of cheat, not one leader.
5. **The detector trains on**, from where it was, on the base set and the
   last `--keep-rounds` rounds' elites (older ones it learned already; the
   exam says whether it kept them).

Each round prints §5's two numbers: the most a candidate gained while
flagged in under 5% of its windows, and how many sightings a sequential
test needs to reach 1000:1 on each elite. If the first falls and the second
stays small, the detector keeps up. Last, an exam: every elite ever, on
fresh matches, against the first detector and the last.

On bots, all of it is plumbing. Everything goes to `--out`, off the repo.
"""

import argparse
import base64
import json
import os
import shutil
import struct
import subprocess
import sys
import time

import numpy as np
import torch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import train as det  # noqa: E402

PREFIX = "oa2:"
MAX_MUTATIONS = 101
FPR = det.FPR
FREE = 0.05
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
AIM = os.path.join(ROOT, "target", "release", "oneshot-aim")


def child(genome, seed):
    """`genome` with one more mutation (`room::genome::Genome::child`)."""
    body = genome[len(PREFIX):]
    raw = base64.urlsafe_b64decode(body + "=" * (-len(body) % 4))
    if (len(raw) - 4) // 2 >= MAX_MUTATIONS:
        return None
    return PREFIX + base64.urlsafe_b64encode(raw + struct.pack("<H", seed)).decode().rstrip("=")


def fresh(base):
    return PREFIX + base64.urlsafe_b64encode(struct.pack("<I", base)).decode().rstrip("=")


def cell(k):
    """The archive's grid: style, aim point, and whether it picks its
    moments. Selection seldom finds a condition from scratch (morr's bred
    golfers settled on never or always), so a picky cheat keeps a cell of
    its own until it is worth its condition."""
    when = "/picky" if k.get("selective") else ""
    return f"{k['style']}/{'head' if k['bone'] >= 0.5 else 'body'}{when}"


def load(path):
    info = json.load(open(os.path.join(path, "info.json")))
    meta = np.load(os.path.join(path, "meta.npy"))
    w = np.load(os.path.join(path, "windows.npy"), mmap_mode="r")
    return w, meta, info, {n: i for i, n in enumerate(info["meta"])}


@torch.no_grad()
def score(model, w, meta, col, mu, sd, guns, maps, chunk=512):
    """The detector's score for every window, a chunk at a time."""
    model.eval()
    out = []
    for i in range(0, len(w), chunk):
        x = torch.tensor((np.asarray(w[i : i + chunk], np.float32) - mu) / sd, device=det.DEV)
        g = torch.tensor(np.minimum(meta[i : i + chunk, col["gun"]], guns - 1), device=det.DEV, dtype=torch.long)
        m = torch.tensor(np.minimum(meta[i : i + chunk, col["map"]], maps - 1), device=det.DEV, dtype=torch.long)
        out.append(model(x, g, m).float().cpu().numpy())
    return np.concatenate(out)


def judge(scores, meta, col, zoo):
    """Each candidate's gain, flag share and fitness on one evaluation."""
    label, pair, cheat = meta[:, col["label"]], meta[:, col["pair"]], meta[:, col["cheat"]]
    hits, helped = meta[:, col["hits"]], meta[:, col["helped"]]
    clean = label == 0
    thr = float(np.quantile(scores[clean], 1 - FPR))
    clean_hits = np.zeros(pair.max() + 1)
    clean_hits[pair[clean]] = hits[clean]
    rows = []
    for k, z in enumerate(zoo):
        m = cheat == k
        if not m.any():
            continue
        caught = float((scores[m] > thr).mean())
        rows.append({
            "genome": z["genome"], "cell": cell(z), "style": z["style"],
            "gain": float((hits[m] - clean_hits[pair[m]]).mean()),
            "caught": caught, "acted": float((helped[m] > 0).mean()),
            "to_catch": det.sightings_to_catch(caught, FPR),
            "knobs": {key: z[key] for key in ("fov_deg", "smooth", "max_deg", "delay_ms", "bone", "miss", "trigger", "walls", "range_m", "low")},
        })
    return rows, thr


def keep_elites(src, dst, elites, info):
    """The elites' windows of one evaluation and their sightings' clean
    windows, kept for the detector's next lessons."""
    w, meta, info, col = load(src)
    ids = [k for k, z in enumerate(info["zoo"]) if z["genome"] in elites]
    mine = np.isin(meta[:, col["cheat"]], ids) & (meta[:, col["label"]] == 1)
    rows = mine | (meta[:, col["label"]] == 0)
    rows &= det.acted_rows(meta, col)
    os.makedirs(dst, exist_ok=True)
    np.save(os.path.join(dst, "windows.npy"), np.asarray(w[np.where(rows)[0]], np.float32))
    np.save(os.path.join(dst, "meta.npy"), meta[rows])
    json.dump(info, open(os.path.join(dst, "info.json"), "w"))


def lessons(base, extra, seed):
    """The detector's training set: the base set's acted rows and every kept
    round of elites, each split by match (15% held for validation, so the
    best epoch is the one that keeps the old lessons and the new)."""
    rng = np.random.default_rng(seed)

    def held(match):
        n = match.max() + 1
        return np.isin(match, rng.permutation(n)[: max(1, n * 15 // 100)])

    w, meta, info, col = load(base)
    names = info["meta"]
    parts = [(w, meta, det.acted_rows(meta, col), col)]
    for d in extra:
        ew, em, _, ec = load(d)
        parts.append((ew, em, np.ones(len(em), bool), ec))
    # One array, filled in place: no copy of the whole set on the way.
    n = sum(int(k.sum()) for _, _, k, _ in parts)
    x = np.empty((n, w.shape[1], w.shape[2]), np.float32)
    ms, va, at = [], [], 0
    for pw, pm, k, pc in parts:
        rows = np.where(k)[0]
        for i in range(0, len(rows), 4096):
            chunk = rows[i : i + 4096]
            x[at : at + len(chunk)] = pw[chunk]
            at += len(chunk)
        # Columns by name: a set cut by a later build may have more.
        ms.append(np.stack([pm[k, pc[c]] for c in names], 1))
        va.append(held(pm[k, pc["match"]]))
    return x, np.concatenate(ms), np.concatenate(va), col


def fit(base, extra, args, init=None, mu=None, sd=None, epochs=None):
    x, meta, va, col = lessons(base, extra, args.seed)
    if mu is None:
        mu = x[~va].reshape(-1, x.shape[2]).mean(0)
        sd = x[~va].reshape(-1, x.shape[2]).std(0) + 1e-4
    x -= mu
    x /= sd
    gun, mp = meta[:, col["gun"]].astype(np.int64), meta[:, col["map"]].astype(np.int64)
    guns, maps = max(int(gun.max()) + 1, 32), max(int(mp.max()) + 1, 16)
    y = meta[:, col["label"]].astype(np.float32)
    targs = argparse.Namespace(epochs=epochs or args.epochs, batch=256, lr=args.lr, width=64)
    model, *_ = det.train(x, gun, mp, y, np.where(~va)[0], np.where(va)[0], targs, guns, maps,
                          seed=args.seed, init=init)
    del x
    return model, mu, sd, guns, maps


def cut(zoo, out, seed, args, matches=None):
    path = os.path.join(out, "zoo.txt")
    os.makedirs(out, exist_ok=True)
    open(path, "w").write("\n".join(zoo) + "\n")
    cmd = [AIM, "bots", "--each", "--zoo", path, "--modes", args.modes, "--matches", str(matches or args.matches),
           "--secs", str(args.secs), "--sightings", str(args.sightings), "--seed", str(seed),
           "--threads", str(args.threads), "--out", out] + (["--hands"] if args.hands else [])
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    info = json.load(open(os.path.join(out, "info.json")))
    made = [z["genome"] for z in info["zoo"]]
    assert made == zoo, "oneshot-aim read the zoo back differently"
    return info


def show(rows, title):
    print(title)
    print(f"  {'cell':<20} {'genome':<22} {'gain':>6} {'caught':>7} {'fitness':>8} {'to 1000:1':>10}")
    for r in sorted(rows, key=lambda r: r["cell"]):
        tc = "never" if r["to_catch"] == float("inf") else f"{r['to_catch']:.0f}"
        print(f"  {r['cell']:<20} {r['genome'][:22]:<22} {r['gain']:>+6.2f} {r['caught']:>7.0%} "
              f"{r.get('fitness', 0):>8.2f} {tc:>10}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--out", default="traces/aim/zoo")
    ap.add_argument("--rounds", type=int, default=8)
    ap.add_argument("--children", type=int, default=3)
    ap.add_argument("--fresh", type=int, default=10)
    ap.add_argument("--lam", type=float, default=1.0, help="hits a sighting a flag on every window costs")
    ap.add_argument("--matches", type=int, default=12)
    ap.add_argument("--secs", type=int, default=150)
    ap.add_argument("--sightings", type=int, default=40)
    ap.add_argument("--modes", default="duel,doubles")
    ap.add_argument("--threads", type=int, default=2)
    ap.add_argument("--hands", action="store_true", help="a person's hand on the bots' aim, as the base set had")
    ap.add_argument("--epochs", type=int, default=30)
    ap.add_argument("--retrain-epochs", type=int, default=6)
    ap.add_argument("--keep-rounds", type=int, default=6, help="the rounds of elites the detector trains on (memory)")
    ap.add_argument("--lr", type=float, default=2e-3)
    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args()
    if not os.path.exists(AIM):
        sys.exit(f"{AIM}: cargo build --release -p aim first")
    os.makedirs(args.out, exist_ok=True)
    rng = np.random.default_rng(args.seed)

    t0 = time.time()
    print(f"the first detector, on {args.base} ({det.DEV})")
    model, mu, sd, guns, maps = fit(args.base, [], args)
    first = {k: v.detach().clone() for k, v in model.state_dict().items()}
    archive = {}  # cell -> row
    kept, history, ever = [], [], {}

    for r in range(args.rounds):
        zoo = list(dict.fromkeys(e["genome"] for e in archive.values()))
        for e in list(zoo):
            for _ in range(args.children):
                c = child(e, int(rng.integers(0, 1 << 16)))
                if c:
                    zoo.append(c)
        zoo += [fresh(int(rng.integers(0, 1 << 32))) for _ in range(args.fresh if archive else 4 * args.fresh)]
        zoo = list(dict.fromkeys(zoo))
        rd = os.path.join(args.out, f"round{r}")
        ev = os.path.join(rd, "eval")
        info = cut(zoo, ev, 1000 + r, args)
        w, meta, _, col = load(ev)
        s = score(model, w, meta, col, mu, sd, guns, maps)
        rows, thr = judge(s, meta, col, info["zoo"])
        for row in rows:
            row["fitness"] = row["gain"] - args.lam * row["caught"]
        # The archive is this round's fittest in each cell: elites are
        # judged again beside their children, against today's detector.
        archive = {}
        for row in rows:
            if row["gain"] > 0 and (row["cell"] not in archive or row["fitness"] > archive[row["cell"]]["fitness"]):
                archive[row["cell"]] = row
        for e in archive.values():
            ever.setdefault(e["genome"], r)
        free = [x for x in rows if x["caught"] < FREE]
        best_free = max(free, key=lambda x: x["gain"], default=None)
        caught_elites = [e["to_catch"] for e in archive.values()]
        history.append({
            "round": r, "candidates": len(zoo), "sightings": int(info["pairs"]), "threshold": thr,
            "free_gain": best_free["gain"] if best_free else 0.0,
            "free_genome": best_free["genome"] if best_free else None,
            "elites_median_to_catch": float(np.median(caught_elites)) if caught_elites else None,
            "elites_caught": float(np.mean([e["caught"] for e in archive.values()])) if archive else 0.0,
            "elites_gain": float(np.mean([e["gain"] for e in archive.values()])) if archive else 0.0,
            "archive": list(archive.values()),
        })
        print()
        show(list(archive.values()), f"round {r}: {len(zoo)} candidates on {info['pairs']} sightings, "
                                     f"{time.time() - t0:.0f} s in")
        if best_free:
            print(f"  most gained under {FREE:.0%} caught: {best_free['gain']:+.2f} hits a sighting "
                  f"({best_free['cell']}, {best_free['genome']})")
        print(f"  elites: {history[-1]['elites_gain']:+.2f} hits a sighting, caught {history[-1]['elites_caught']:.0%}, "
              f"median {history[-1]['elites_median_to_catch']:.0f} sightings to 1000:1")
        json.dump({"archive": list(archive.values()), "history": history}, open(os.path.join(args.out, "zoo.json"), "w"), indent=1)

        # The detector's next lessons: this round's elites, on top of the rest.
        keep = os.path.join(rd, "elites")
        keep_elites(ev, keep, {e["genome"] for e in archive.values()}, info)
        shutil.rmtree(ev)
        kept.append(keep)
        state = {k: v.detach().clone() for k, v in model.state_dict().items()}
        model, *_ = fit(args.base, kept[-args.keep_rounds :], args, init=state, mu=mu, sd=sd, epochs=args.retrain_epochs)

    # The exam: every elite ever, on matches no round saw, before and after.
    zoo = list(ever)
    ex = os.path.join(args.out, "exam")
    info = cut(zoo, ex, 9999, args)
    w, meta, _, col = load(ex)
    exam = {}
    for name, state in (("first", first), ("last", model.state_dict())):
        m = det.Detector(w.shape[2], guns, maps, 64).to(det.DEV)
        m.load_state_dict(state)
        rows, _ = judge(score(m, w, meta, col, mu, sd, guns, maps), meta, col, info["zoo"])
        for row in rows:
            row["bred_in"] = ever[row["genome"]]
        exam[name] = rows
    shutil.rmtree(ex)
    print()
    print(f"the exam: {len(zoo)} elites from every round on {info['pairs']} fresh sightings, at {FPR:.0%} of clean flagged")
    print(f"  {'cell':<20} {'round':>5} {'gain':>6} {'first caught':>13} {'last caught':>12}")
    for a, b in sorted(zip(exam["first"], exam["last"]), key=lambda p: (p[0]["bred_in"], p[0]["cell"])):
        print(f"  {a['cell']:<20} {a['bred_in']:>5} {a['gain']:>+6.2f} {a['caught']:>13.0%} {b['caught']:>12.0%}")
    for name in ("first", "last"):
        rows = exam[name]
        free = [x["gain"] for x in rows if x["caught"] < FREE]
        print(f"  the {name} detector: caught {np.mean([x['caught'] for x in rows]):.0%} on average; "
              f"most gained under {FREE:.0%} caught {max(free, default=0.0):+.2f}")
    torch.save({"state": model.state_dict(), "mu": mu, "sd": sd, "guns": guns, "maps": maps, "width": 64},
               os.path.join(args.out, "detector.pt"))
    json.dump({"archive": list(archive.values()), "history": history, "exam": exam},
              open(os.path.join(args.out, "zoo.json"), "w"), indent=1)
    print(f"-> {args.out}/zoo.json, {args.out}/detector.pt ({time.time() - t0:.0f} s)")


if __name__ == "__main__":
    main()

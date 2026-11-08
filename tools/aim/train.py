#!/usr/bin/env python3
"""The aim detector, first cut (docs/ANTICHEAT.md §6, §7's stage 0): a small
sequence model over the aim windows `oneshot-aim` cuts, trained on the GPU.

    PY=/data/LLM/.venv/bin/python    # any Python with torch and numpy
    $PY tools/aim/train.py traces/aim/<set> [--epochs 30] [--out traces/aim/<set>/run]

Each sighting is a pair: the window as played and its cheated twin, so the
two share the player, match, moment, gun and map, and only the cheat tells
them apart. What it reports, and why:

- **Split by match and by cheat.** Test matches are never trained on, and a
  quarter of the zoo's assists are never trained on: "unseen" is the number
  that says whether it learned cheating or learned the zoo.
- **Controls (§6).** A baseline that knows only gun, map and skill (twins
  share those, so it must sit at 0.5); the review's own lines as features
  (reaction, snap, head rate) in a logistic regression, which the model
  must beat; and the model trained on shuffled labels, which must not.
- **Never pooled alone.** Per gun, per map and per style beside the total.
- **§5's two numbers, per assist.** At a threshold flagging 1% of clean
  windows: how much the assist gained (hits a window over its twin) and how
  often it was caught, and from that how many sightings a sequential test
  needs to reach 1000:1 on it.

On bots, all of it is plumbing, not a result: the bots aim like bots.
Weights and the report go to `--out`, off the repo (§8).
"""

import argparse
import json
import math
import os
import time

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

FPR = 0.01
ODDS = 1000.0


def auc(score, label):
    """The chance a cheated window outscores a clean one (Mann-Whitney)."""
    score, label = np.asarray(score, np.float64), np.asarray(label, bool)
    pos, neg = label.sum(), (~label).sum()
    if pos == 0 or neg == 0:
        return float("nan")
    order = np.argsort(score, kind="mergesort")
    ranks = np.empty(len(score))
    s = score[order]
    # Ties share their mean rank.
    i = 0
    while i < len(s):
        j = i
        while j + 1 < len(s) and s[j + 1] == s[i]:
            j += 1
        ranks[order[i : j + 1]] = (i + j) / 2 + 1
        i = j + 1
    return float((ranks[label].sum() - pos * (pos + 1) / 2) / (pos * neg))


class Detector(nn.Module):
    """Rows in, one logit out: dilated convolutions for the flick's shape,
    a GRU for its order, attention to pool, then the gun and the map."""

    def __init__(self, f, guns, maps, width=64):
        super().__init__()
        self.inp = nn.Linear(f, width)
        self.convs = nn.ModuleList(
            nn.Conv1d(width, width, 5, padding=2 * d, dilation=d) for d in (1, 2, 4, 8)
        )
        self.norms = nn.ModuleList(nn.LayerNorm(width) for _ in self.convs)
        self.gru = nn.GRU(width, width, batch_first=True, bidirectional=True)
        self.att = nn.Linear(2 * width, 1)
        self.gun = nn.Embedding(guns, 8)
        self.map = nn.Embedding(maps, 8)
        self.head = nn.Sequential(nn.Linear(2 * width + 16, width), nn.GELU(), nn.Linear(width, 1))

    def forward(self, x, gun, mp, keep=None):
        h = self.inp(x)
        for conv, norm in zip(self.convs, self.norms):
            h = norm(h + F.gelu(conv(h.transpose(1, 2)).transpose(1, 2)))
        h, _ = self.gru(h)
        if keep is not None:
            keep["seq"] = h
        a = torch.softmax(self.att(h).squeeze(-1), dim=1)
        pooled = (a.unsqueeze(-1) * h).sum(1)
        if keep is not None:
            keep["att"] = a
        z = torch.cat([pooled, self.gun(gun), self.map(mp)], 1)
        return self.head(z).squeeze(-1)


def hand_features(w, info):
    """The review's lines (room::watch), per window: what a model must
    beat. Reaction from plain sight to the first hit, the most turn in the
    100 ms before it, head hits over hits, how close the aim sat while
    firing in plain sight, and the largest single tick of turn."""
    fi = {n: i for i, n in enumerate(info["features"])}
    pre, hz = info["pre"], info["tick_hz"]
    hit = w[:, :, fi["hit"]] > 0
    n = len(w)
    first = np.where(hit[:, pre:].any(1), hit[:, pre:].argmax(1), w.shape[1] - pre)
    turn = np.hypot(w[:, :, fi["turn_yaw"]], w[:, :, fi["turn_pitch"]])
    snap = np.zeros(n)
    back = max(1, hz // 10)
    for i in range(n):
        t = pre + first[i]
        if t < w.shape[1]:
            snap[i] = turn[i, max(0, t - back) : t + 1].sum()
    heads = w[:, :, fi["head"]].sum(1) / np.maximum(1, hit.sum(1))
    err = np.hypot(w[:, :, fi["err_yaw"]], w[:, :, fi["err_pitch"]])
    on = (w[:, :, fi["plain"]] > 0) & (w[:, :, fi["fire"]] > 0)
    close = np.where(on.any(1), (err * on).sum(1) / np.maximum(1, on.sum(1)), 45.0)
    return np.stack([first / hz, snap, heads, np.log1p(close), turn.max(1), hit.any(1)], 1).astype(np.float32)


def logistic(xtr, ytr, xte, epochs=300):
    """A logistic regression, fitted on the GPU: the baselines."""
    mu, sd = xtr.mean(0), xtr.std(0) + 1e-6
    a = torch.tensor((xtr - mu) / sd, dtype=torch.float32, device=DEV)
    b = torch.tensor((xte - mu) / sd, dtype=torch.float32, device=DEV)
    y = torch.tensor(ytr, dtype=torch.float32, device=DEV)
    lin = nn.Linear(a.shape[1], 1).to(DEV)
    opt = torch.optim.LBFGS(lin.parameters(), max_iter=epochs)

    def step():
        opt.zero_grad()
        loss = F.binary_cross_entropy_with_logits(lin(a).squeeze(-1), y) + 1e-4 * lin.weight.pow(2).sum()
        loss.backward()
        return loss

    opt.step(step)
    with torch.no_grad():
        return lin(b).squeeze(-1).cpu().numpy()


def train(x, gun, mp, y, idx_tr, idx_va, args, guns, maps, shuffle=False, seed=0, init=None):
    """The detector fitted to `idx_tr`, kept at its best validation AUC;
    from `init`'s weights if given (the zoo's next round)."""
    torch.manual_seed(seed)
    model = Detector(x.shape[2], guns, maps, args.width).to(DEV)
    if init is not None:
        model.load_state_dict(init)
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-2)
    ytr = y.copy()
    if shuffle:
        ytr = np.random.default_rng(seed).permutation(ytr)
    # The set stays in memory (no copy) and goes to the GPU a batch at a
    # time: the card is shared with the game's own renders.
    X = torch.from_numpy(np.ascontiguousarray(x))
    G = torch.from_numpy(np.ascontiguousarray(gun))
    M = torch.from_numpy(np.ascontiguousarray(mp))
    Y = torch.from_numpy(np.ascontiguousarray(ytr, dtype=np.float32))
    best, best_state = -1.0, None
    steps = args.epochs * math.ceil(len(idx_tr) / args.batch)
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, max_lr=args.lr, total_steps=steps)
    for ep in range(args.epochs):
        model.train()
        perm = np.random.default_rng(seed * 1000 + ep).permutation(idx_tr)
        for i in range(0, len(perm), args.batch):
            b = torch.from_numpy(perm[i : i + args.batch])
            xb, gb, mb, yb = (t[b].to(DEV, non_blocking=True) for t in (X, G, M, Y))
            loss = F.binary_cross_entropy_with_logits(model(xb, gb, mb), yb)
            opt.zero_grad()
            loss.backward()
            nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()
            sched.step()
        va = auc(predict(model, X, G, M, idx_va), y[idx_va] if not shuffle else ytr[idx_va])
        if va > best:
            best, best_state = va, {k: v.detach().clone() for k, v in model.state_dict().items()}
        if not shuffle and (ep % 5 == 4 or ep == args.epochs - 1):
            print(f"  epoch {ep + 1:>3}: loss {loss.item():.4f}, validation AUC {va:.4f}", flush=True)
    model.load_state_dict(best_state)
    return model, X, G, M


@torch.no_grad()
def predict(model, X, G, M, idx, batch=512):
    model.eval()
    out = []
    for i in range(0, len(idx), batch):
        b = torch.from_numpy(np.asarray(idx[i : i + batch]))
        out.append(model(*(t[b].to(DEV) for t in (X, G, M))).float().cpu().numpy())
    return np.concatenate(out) if out else np.zeros(0)


def sightings_to_catch(tpr, fpr, odds=ODDS):
    """Wald's test on flags: expected sightings for the log of the
    likelihood ratio to reach `odds` against a cheater caught at `tpr` with
    honest flags at `fpr` (§2)."""
    if tpr <= fpr:
        # Flagged no more than honest play: no test ever leans toward it.
        return float("inf")
    tpr = min(tpr, 1 - 1e-6)
    gain = tpr * math.log(tpr / fpr) + (1 - tpr) * math.log((1 - tpr) / (1 - fpr))
    return math.log(odds) / gain


def acted_rows(meta, col):
    """The rows worth learning from: a cheated window whose assist acted, and
    a clean one with at least one such twin. A twin whose assist never acted
    is the clean window again."""
    label, pair = meta[:, col["label"]], meta[:, col["pair"]]
    cheat = label == 1
    acted = cheat & (meta[:, col["helped"]] > 0)
    has = np.zeros(pair.max() + 1, bool)
    has[pair[acted]] = True
    return acted | (~cheat & has[pair])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("data")
    ap.add_argument("--out")
    ap.add_argument("--epochs", type=int, default=30)
    ap.add_argument("--batch", type=int, default=256)
    ap.add_argument("--lr", type=float, default=2e-3)
    ap.add_argument("--width", type=int, default=64)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--keep-idle", action="store_true", help="keep pairs whose assist never acted (twins identical)")
    args = ap.parse_args()
    out = args.out or os.path.join(args.data, "run")
    os.makedirs(out, exist_ok=True)

    info = json.load(open(os.path.join(args.data, "info.json")))
    w = np.load(os.path.join(args.data, "windows.npy"), mmap_mode="r")
    meta = np.load(os.path.join(args.data, "meta.npy"))
    col = {n: i for i, n in enumerate(info["meta"])}
    label, pair, match = meta[:, col["label"]], meta[:, col["pair"]], meta[:, col["match"]]
    print(f"{args.data}: {len(w)} windows, {pair.max() + 1} pairs, {match.max() + 1} matches, from {info['from']}")

    # A pair whose assist never acted is two identical windows: nothing to
    # learn, and nothing a detector could ever catch.
    cheat_rows = label == 1
    acted = meta[cheat_rows, col["helped"]] > 0
    print(f"the assist acted in {acted.mean():.0%} of its windows")
    keep = np.ones(len(w), bool) if args.keep_idle else acted_rows(meta, col)

    rng = np.random.default_rng(args.seed)
    matches = rng.permutation(match.max() + 1)
    n_te = max(1, len(matches) * 15 // 100)
    te_m, va_m = set(matches[:n_te]), set(matches[n_te : 2 * n_te])
    zoo = info["zoo"]
    unseen = set(rng.permutation(len(zoo))[: max(1, len(zoo) // 4)].tolist())
    # Held out: an unseen assist's cheated windows (the clean ones are honest
    # play, and train as such).
    pair_unseen = cheat_rows & np.isin(meta[:, col["cheat"]], list(unseen))
    in_te = np.isin(match, list(te_m))
    in_va = np.isin(match, list(va_m))
    idx_tr = np.where(keep & ~in_te & ~in_va & ~pair_unseen)[0]
    idx_va = np.where(keep & in_va & ~pair_unseen)[0]
    idx_te = np.where(keep & in_te)[0]
    print(f"train {len(idx_tr)}, validation {len(idx_va)}, test {len(idx_te)} windows "
          f"({len(unseen)} of {len(zoo)} assists held out of training)")

    # The review's lines from the rows as they are, then the rows scaled in
    # place: one copy of the set in memory.
    hand = hand_features(w, info)
    x = np.array(w, dtype=np.float32)
    mu = x[idx_tr].reshape(-1, x.shape[2]).mean(0)
    sd = x[idx_tr].reshape(-1, x.shape[2]).std(0) + 1e-4
    x -= mu
    x /= sd
    gun = meta[:, col["gun"]].astype(np.int64)
    mp = meta[:, col["map"]].astype(np.int64)
    guns, maps = int(gun.max()) + 1, int(mp.max()) + 1
    y = label.astype(np.float32)

    t0 = time.time()
    print(f"training on {torch.cuda.get_device_name(0) if DEV.type == 'cuda' else 'the CPU'}")
    model, X, G, M = train(x, gun, mp, y, idx_tr, idx_va, args, guns, maps, seed=args.seed)
    score = predict(model, X, G, M, idx_te)
    print(f"  {time.time() - t0:.0f} s")

    yte = label[idx_te] == 1
    report = {"data": args.data, "from": info["from"], "windows": int(len(w)), "acted": float(acted.mean())}
    report["test_auc"] = auc(score, yte)
    te_pairs = pair[idx_te]
    # A sighting's clean window against each of its cheated twins.
    clean_score = {p: s for p, s, l in zip(te_pairs, score, yte) if not l}
    twins = [s > clean_score[p] for p, s, l in zip(te_pairs, score, yte) if l and p in clean_score]
    report["pair_accuracy"] = float(np.mean(twins)) if twins else float("nan")
    unseen_te = pair_unseen[idx_te]
    report["test_auc_seen"] = auc(score[~unseen_te], yte[~unseen_te])
    # Unseen cheats against every clean window (their own twins included).
    sel = unseen_te | ~yte
    report["test_auc_unseen"] = auc(score[sel], yte[sel])

    # Controls.
    skill = meta[:, col["skill"]]
    ctx = np.concatenate([np.eye(guns)[gun], np.eye(maps)[mp], (skill[:, None] // 250 == np.arange(-1, 4)).astype(float)], 1)
    report["control_context_only"] = auc(logistic(ctx[idx_tr], y[idx_tr], ctx[idx_te]), yte)
    report["control_review_lines"] = auc(logistic(hand[idx_tr], y[idx_tr], hand[idx_te]), yte)
    null, *_ = train(x, gun, mp, y, idx_tr, idx_va, argparse.Namespace(**{**vars(args), "epochs": max(5, args.epochs // 3)}),
                     guns, maps, shuffle=True, seed=args.seed + 1)
    report["control_shuffled_labels"] = auc(predict(null, X, G, M, idx_te), yte)

    # Never pooled alone: by gun, map and style.
    names = {d["id"]: d["name"] for d in info["maps"]}
    clean_te = ~yte

    def split(key, values, name):
        rows = {}
        for v in np.unique(values[idx_te]):
            m = values[idx_te] == v
            # A slice's cheats against the clean windows of the same slice.
            if (m & yte).sum() >= 10 and (m & clean_te).sum() >= 10:
                rows[name(v)] = {"auc": auc(score[m], yte[m]), "windows": int(m.sum())}
        report[key] = rows

    gun_names = info.get("guns", [])
    split("by_gun", gun, lambda v: gun_names[v] if v < len(gun_names) else f"arm {v}")
    split("by_map", mp, lambda v: names.get(int(v), str(v)))
    styles = info["styles"]
    by_style = {}
    st = meta[idx_te, col["style"]]
    for k, s in enumerate(styles):
        m = (st == k) & yte
        if m.sum() >= 10:
            sel = m | clean_te
            by_style[s] = {"auc": auc(score[sel], yte[sel]), "windows": int(m.sum())}
    report["by_style"] = by_style

    # §5's numbers per assist, at a threshold flagging FPR of clean windows.
    thr = float(np.quantile(score[clean_te], 1 - FPR))
    report["threshold"] = {"fpr": FPR, "score": thr}
    clean_hits = {pair[i]: meta[i, col["hits"]] for i in idx_te if label[i] == 0}
    assists = []
    cheat_te = meta[idx_te, col["cheat"]]
    for k, g in enumerate(zoo):
        m = (cheat_te == k) & yte
        if m.sum() < 5:
            continue
        caught = float((score[m] > thr).mean())
        gain = [h - clean_hits[p] for p, h in zip(pair[idx_te][m], meta[idx_te, col["hits"]][m]) if p in clean_hits]
        assists.append({
            "cheat": k, "genome": g["genome"], "style": g["style"], "unseen": k in unseen,
            "windows": int(m.sum()), "caught": caught, "hits_gained": float(np.mean(gain)) if gain else 0.0,
            "sightings_to_catch": sightings_to_catch(caught, FPR),
        })
    assists.sort(key=lambda a: -a["hits_gained"])
    report["assists"] = assists
    free = [a for a in assists if a["caught"] < 0.05]
    report["largest_gain_uncaught"] = max((a["hits_gained"] for a in free), default=0.0)

    torch.save({"state": model.state_dict(), "mu": mu, "sd": sd, "guns": guns, "maps": maps,
                "width": args.width, "features": info["features"]}, os.path.join(out, "detector.pt"))
    json.dump(report, open(os.path.join(out, "report.json"), "w"), indent=2)

    print()
    print(f"test AUC {report['test_auc']:.3f} (seen assists {report['test_auc_seen']:.3f}, "
          f"unseen {report['test_auc_unseen']:.3f}); the cheated twin scores higher in "
          f"{report['pair_accuracy']:.0%} of pairs")
    print(f"controls: context only {report['control_context_only']:.3f}, the review's lines "
          f"{report['control_review_lines']:.3f}, shuffled labels {report['control_shuffled_labels']:.3f}")
    for key in ("by_gun", "by_map", "by_style"):
        print(f"{key[3:]:>6}: " + ", ".join(f"{k} {v['auc']:.3f} ({v['windows']})" for k, v in report[key].items()))
    print(f"at {FPR:.0%} of clean windows flagged: per assist, hits gained a sighting, caught, sightings to {ODDS:.0f}:1")
    for a in assists[:12]:
        print(f"  {a['genome']:<12} {a['style']:<8}{' unseen' if a['unseen'] else '       '} "
              f"+{a['hits_gained']:.2f} hits, caught {a['caught']:.0%}, {a['sightings_to_catch']:.0f} sightings")
    print(f"largest gain with under 5% caught: +{report['largest_gain_uncaught']:.2f} hits a sighting")
    print(f"-> {out}/report.json, {out}/detector.pt")


DEV = torch.device("cuda" if torch.cuda.is_available() else "cpu")
if DEV.type == "cuda":
    # A share of the card at most: past it this job fails, not the game's
    # renders beside it.
    torch.cuda.set_per_process_memory_fraction(float(os.environ.get("AIM_GPU_SHARE", "0.35")))

if __name__ == "__main__":
    main()

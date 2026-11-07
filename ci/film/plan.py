#!/usr/bin/env python3
"""Write the trailer's shot lists from the tapes' scans.

    python3 ci/film/plan.py                # every tape with a scan in traces/film/
    python3 ci/film/plan.py ffa-chrysalis  # one

A tape is a match the autopilot played (`ci/film.sh record`); its scan
(`oneshot --scan`) says when it died, where it stood and who it saw. The
autopilot is a roamer that dies a lot, which suits a trailer: each death is
a kill cam, the kill from the killer's eye, and `--film`'s pov draws it.
So the ramp's cuts are kill cams, the mode cards are a camera on the
player's shoulder in the thick of it, and the maps' roll call is a fixed
move over each arena from the HD loop's eyes (tools/hd.sh). The Big Team
list is written by hand (its trucks and launches are read off the scan by
speed): ci/film/trailer-bigteam.json.
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FILM = ROOT / "traces" / "film"
OUT = ROOT / "ci" / "film"
LOOK = {"saturation": 1.06, "contrast": 1.05, "shutter": 0.5}
FOCUS = {"focus": "look", "fstop": 4}
SHOULDER = {"back": 3.4, "up": 1.6, "side": 0.6, "lag": 0.25}

# The maps' roll call and their stills: a move from the HD loop's eyes.
MAPS = {
    "ffa-proving": [
        {"name": "map-proving", "len": 2.6, "ease": True, "fov": 52,
         "orbit": {"center": [0, 1.5, 0], "radius": [30, 26], "height": [13, 11], "from": -62, "to": -38}},
        {"name": "still-proving", "still": True, "fov": 50, "keys": [{"t": 0, "pos": [-19, 2.2, -14], "look": [6, 2.5, 8]}], "look": FOCUS},
    ],
    "ffa-overpass": [
        {"name": "map-overpass", "len": 2.6, "ease": True, "fov": 54, "keys": [{"t": 0, "pos": [-2, 6.6, -17], "look": [0, 6, 10]}, {"t": 2.6, "pos": [-2, 6.2, -12.5], "look": [0, 6, 10]}]},
        {"name": "still-overpass", "still": True, "fov": 52, "keys": [{"t": 0, "pos": [-19, 1.7, 19.5], "look": [4, 2.5, 19]}], "look": FOCUS},
    ],
    "ffa-cistern": [
        {"name": "map-cistern", "len": 2.6, "ease": True, "fov": 52,
         "orbit": {"center": [0, 2.5, 0], "radius": [21, 18], "height": [7.5, 5.5], "from": 196, "to": 226}},
        {"name": "still-cistern", "still": True, "fov": 52, "keys": [{"t": 0, "pos": [-17, 6.8, 0], "look": [2, 6, 1]}], "look": FOCUS},
    ],
    "ffa-bulkhead": [
        {"name": "map-bulkhead", "len": 2.6, "ease": True, "fov": 56, "keys": [{"t": 0, "pos": [-22, 4.6, -6], "look": [4, 3, 3]}, {"t": 2.6, "pos": [-18.5, 4.3, -4.5], "look": [4, 3, 3]}]},
        {"name": "still-bulkhead", "still": True, "fov": 54, "keys": [{"t": 0, "pos": [-14, 4.6, 0], "look": [6, 5, 8]}], "look": FOCUS},
    ],
    "ffa-chrysalis": [
        {"name": "map-chrysalis", "len": 2.6, "ease": True, "fov": 56, "keys": [{"t": 0, "pos": [-21, 12, -13], "look": [8, 4, 6]}, {"t": 2.6, "pos": [-16.5, 10.5, -10.5], "look": [8, 4, 6]}]},
        {"name": "still-chrysalis", "still": True, "fov": 54, "keys": [{"t": 0, "pos": [-18, 8.6, -2.5], "look": [4, 8.5, 1]}], "look": FOCUS},
    ],
    "ffa-cloudbreak": [
        {"name": "map-cloudbreak", "len": 2.6, "ease": True, "fov": 54, "keys": [{"t": 0, "pos": [-21, 4.2, -1], "look": [6, 2.5, 2]}, {"t": 2.6, "pos": [-17.5, 4.4, -0.5], "look": [6, 2.5, 2]}]},
        {"name": "still-cloudbreak", "still": True, "fov": 52, "keys": [{"t": 0, "pos": [0, 1.7, -21], "look": [-25, -6, -60]}]},
        {"name": "still-cloudbreak-spire", "still": True, "fov": 52, "keys": [{"t": 0, "pos": [0, 1.7, 7], "look": [0, 11, 20]}]},
    ],
}
# What each tape gives the cut besides its map: a mode card (over the
# shoulder, 3.6 s), kill cams (1.9 s each), a pov with enemies near.
ROLES = {
    "ffa-chrysalis": {"shoulder": "ffa", "kills": ["kill-1"], "pov": "corner-b"},
    "ffa-cistern": {"kills": ["kill-2"]},
    "ffa-bulkhead": {"kills": ["kill-3"], "pov": "corner"},
    "ffa-overpass": {"kills": ["kill-4"]},
    "ffa-proving": {"kills": ["kill-6"]},
    "ffa-cloudbreak": {"kills": ["kill-7"]},
    "gungame": {"shoulder": "gungame", "kills": ["kill-5"]},
    "ctf": {"shoulder": "ctf", "kills": ["kill-8"]},
    "duel": {"shoulder": "duel", "pov": "duel-pov"},
    "doubles": {"shoulder": "doubles"},
    "snipe": {"scope": "scope"},
    "snipe2": {"snipe": "snipe-kill"},
}


def scan(tape):
    rows, deaths, kills = [], [], []
    for line in open(FILM / f"{tape}.scan"):
        m = re.match(r"\s*([\d.]+) s\s+(.*?) killed (.*)$", line)
        if m:
            t, killer, victim = float(m.group(1)), m.group(2).strip(), m.group(3).strip()
            if victim == "player":
                deaths.append(t)
            elif killer == "player":
                kills.append(t)
            continue
        m = re.match(r"\s*([\d.]+) s\s+seat (\d+) at \(([^)]*)\) (alive|dead )\s+(\d+)/(\d+)\s+sees (.*)$", line)
        if m:
            seen = m.group(7).strip()
            rows.append({"t": float(m.group(1)), "pos": tuple(float(v) for v in m.group(3).split(",")),
                         "alive": m.group(4).strip() == "alive", "n": 0 if seen == "nobody" else len(seen.split())})
    return rows, deaths, kills


def moved_at(rows):
    for r in rows[1:]:
        if r["pos"] != rows[0]["pos"]:
            return r["t"]
    return 30.0


def alive_window(rows, deaths, start, length, avoid=()):
    """The start of a `length`-second stretch after `start` where the
    player is alive throughout and sees the most, a second clear of any
    death and of `avoid`."""
    best, score = None, -1
    for r in rows:
        t0, t1 = r["t"], r["t"] + length
        if t0 < start:
            continue
        if any(t0 - 1.0 < d < t1 + 1.0 for d in deaths):
            continue
        if any(abs(t0 - a) < length + 1 for a in avoid):
            continue
        win = [x for x in rows if t0 <= x["t"] <= t1]
        if not win or not all(x["alive"] for x in win):
            continue
        s = sum(x["n"] for x in win)
        if s > score:
            best, score = t0, s
    return best


# Times chosen by hand after a look at a scan, by tape and shot: a Gun Game
# shoulder that holds the player's own kill at 65.4 s and the rung it climbs.
HAND = {
    "gungame": {"gungame": 62.4, "kill-5": 64.0},
}


def plan(tape):
    rows, deaths, kills = scan(tape)
    go = moved_at(rows) + 4.0
    shots = []
    roles = ROLES.get(tape, {})
    used = []
    # the map and its stills, when the match is in full swing
    for s in MAPS.get(tape, []):
        shots.append({**s, "at": round(go + 26.0 + 0.4 * len(shots), 1)})
    # a camera on the player's shoulder in the thick of it
    if "shoulder" in roles:
        t = alive_window(rows, deaths, go, 3.6)
        if t is not None:
            used.append(t)
            shots.append({"name": roles["shoulder"], "at": t, "len": 3.6, "fov": 62, "follow": dict(SHOULDER),
                          "look": {"focus": "subject", "fstop": 4, "shake": 0.05}})
    # the player's eyes with enemies about
    if "pov" in roles:
        t = alive_window(rows, deaths, go, 4.8, avoid=used)
        if t is not None:
            used.append(t)
            shots.append({"name": roles["pov"], "at": t, "len": 4.8, "pov": True})
    # kill cams: the later deaths, spread out
    late = [d for d in deaths if d > go + 6]
    for i, name in enumerate(roles.get("kills", [])):
        if not late:
            break
        d = late[(len(late) * (i + 1)) // (len(roles["kills"]) + 1)]
        shots.append({"name": name, "at": round(d + 0.15, 2), "len": 1.6, "pov": True})
    # the scoped sweep: the longest alive stretch
    if "scope" in roles:
        t = alive_window(rows, deaths, go, 8.6)
        if t is not None:
            shots.append({"name": roles["scope"], "at": t, "len": 8.6, "pov": True, "look": {"shake": 0.04}})
            shots.append({"name": "still-scope", "at": t + 2.0, "still": True, "pov": True})
    # the sniper's kill: the player's first kill after the load, the shot a second in
    if "snipe" in roles and kills:
        k = [x for x in kills if x > go] or kills
        shots.append({"name": roles["snipe"], "at": round(k[0] - 1.2, 2), "len": 4.0, "pov": True,
                      "speed": [[0, 1], [1.5, 1], [1.9, 0.35]]})
    for x in shots:
        if x["name"] in HAND.get(tape, {}):
            x["at"] = HAND[tape][x["name"]]
    if not shots:
        return None
    return {"tape": f"traces/film/{tape}.tape", "out": "traces/film/trailer", "width": 1600, "height": 900,
            "fps": 30, "quality": "high", "settle": 20, "hold": 24, "look": LOOK, "shots": shots}


def main():
    want = sys.argv[1:] or sorted(p.stem for p in FILM.glob("*.scan") if p.stem != "bigteam")
    for tape in want:
        if not (FILM / f"{tape}.scan").exists():
            print(f"{tape}: no scan")
            continue
        s = plan(tape)
        if not s:
            print(f"{tape}: nothing wanted")
            continue
        json.dump(s, open(OUT / f"trailer-{tape}.json", "w"), indent=1)
        print(f"trailer-{tape}.json:", ", ".join(f"{x['name']}@{x['at']}" for x in s["shots"]))


if __name__ == "__main__":
    main()

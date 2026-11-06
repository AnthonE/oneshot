#!/usr/bin/env python3
"""reel.py <traces/reel/gun> <gun> --stood S [--quality Q]: cut a drill's
first-person film (pov.mp4, tools/reel.sh) into its sheets, by the drill's
beats (crates/client/src/lib.rs, DRILL_*: seconds from its stand, which is
S seconds into the film). A beat that flashes is found in the picture to
the frame; one that doesn't is cut where the drill's clock says."""
import argparse, os, re, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import strip


def beats():
    """The drill's beats, read from the client: one place holds the numbers."""
    src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "../crates/client/src/lib.rs")).read()
    out = {}
    for name, val in re.findall(r"pub const DRILL_(\w+)_S: (?:\(f64, f64\)|f64) = \(?([\d., ]+)\)?;", src):
        v = [float(x) for x in val.split(",")]
        out[name.lower()] = v if len(v) > 1 else v[0]
    return out


def first_rise(clip, t0, t1, crop, least=4.0):
    """The first frame from t0 to t1 that jumps brighter in `crop`, or None."""
    for t, v in strip.rises(clip, crop, t0, t1):
        if v >= least:
            return t
    return None


def biggest_rise(clip, t0, t1, crop="", least=3.0):
    r = strip.rises(clip, crop, t0, t1)
    t, v = max(r, key=lambda x: x[1], default=(None, 0.0))
    return t if v >= least else None


def main():
    p = argparse.ArgumentParser()
    p.add_argument("out")
    p.add_argument("gun")
    p.add_argument("--stood", type=float, default=1.0)
    p.add_argument("--quality", default="high")
    a = p.parse_args()
    clip = os.path.join(a.out, "pov.mp4")
    _, _, fps = strip.probe(clip)
    b, f = beats(), 1.0 / fps
    who = f"oneshot ({a.quality}): {a.gun}"

    def cut(name, at, n, step, crop, what, cols=6, w=400):
        strip.sheet(argparse.Namespace(
            clip=clip, out=os.path.join(a.out, name + ".jpg"), at=max(0.0, at), n=n, step=step,
            cols=cols, w=w, crop=crop, title=f"{who}, {what}"))

    # The gun and the middle of the view; the wall where the rounds land;
    # the floor ahead.
    GUN, WALL, FLOOR = "0.3,0.25,0.7,0.75", "0.38,0.36,0.24,0.27", "0.2,0.35,0.6,0.65"
    cut("rest", a.stood + 0.5, 1, 1, "", "at rest", cols=1, w=1280)
    t0 = a.stood + b["burst"][0]
    shot = first_rise(clip, t0 - 0.3, t0 + 0.9, "0.4,0.3,0.6,0.7")
    found = "" if shot is not None else " (no flash found: by the clock)"
    shot = (shot if shot is not None else t0) - 2 * f
    cut("burst", shot, 18, 1, GUN, f"first person, the burst from its first round, every frame{found}")
    cut("wall", shot, 18, 1, WALL, "where the rounds land (the wall, 6 m or clear of the blast), every frame")
    cut("wall-after", a.stood + b["burst"][1] + 0.1, 6, 8, WALL, "the wall after the burst, every 8th frame", cols=6)
    t0 = a.stood + b["floor"][0]
    shot = first_rise(clip, t0 + 0.1, t0 + 0.7, FLOOR)
    if shot is not None:
        cut("floor", shot - 2 * f, 18, 1, FLOOR, "rounds into the floor 3 m ahead, every frame")
    cut("melee", a.stood + b["melee"] - 2 * f, 12, 2, "", "a melee, every 2nd frame", w=320)
    t0 = a.stood + b["throw"]
    cut("throw", t0 - 2 * f, 12, 4, "", "a frag thrown, every 4th frame", w=320)
    # It fuses at least 1.3 s from the throw.
    blast = biggest_rise(clip, t0 + 1.0, t0 + 3.4, "0.15,0.15,0.7,0.7")
    if blast is not None:
        cut("frag", blast - 2 * f, 18, 1, "0.15,0.1,0.7,0.8", "the frag going off against the wall, every frame")
        cut("frag-after", blast + 16 * f, 12, 6, "0.15,0.1,0.7,0.8", "the frag's smoke, every 6th frame")
    cut("reload", a.stood + b["reload"], 12, 8, GUN, "the reload, every 8th frame")


if __name__ == "__main__":
    main()

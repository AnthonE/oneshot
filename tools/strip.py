#!/usr/bin/env python3
"""strip.py <clip.mp4> <out.jpg> [--at T] [--n 12] [--step 1] [--cols 6] [--w 400]
              [--crop x,y,w,h (0-1 of the frame)] [--title TEXT]

Consecutive frames of a video as one sheet, each captioned with its frame
number and its time from the first: an effect is read as it moves. For the
game's own footage (tools/reel.sh, ci/film.sh) and for reference footage
alike, so the two are read in the same form.

strip.py peaks <clip.mp4> [--top 12] [--crop x,y,w,h] [--from T] [--to T]
Frames where the picture jumps brighter (a shot, a blast): time, rise.

Needs ffmpeg, and Python's PIL and numpy.
"""
import subprocess, sys, tempfile, os, glob, argparse
from PIL import Image, ImageDraw, ImageFont
import numpy as np


def probe(clip):
    out = subprocess.check_output(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
         "stream=r_frame_rate,width,height", "-of", "csv=p=0", clip], text=True).strip()
    w, h, r = out.split(",")
    a, b = r.split("/")
    return int(w), int(h), float(a) / float(b)


def crop_filter(crop):
    if not crop:
        return ""
    x, y, w, h = (float(v) for v in crop.split(","))
    return f"crop=iw*{w}:ih*{h}:iw*{x}:ih*{y},"


def frames(clip, at, n, step, width, crop):
    _, _, fps = probe(clip)
    d = tempfile.mkdtemp(prefix="strip")
    vf = f"{crop_filter(crop)}select='not(mod(n\\,{step}))',scale={width}:-2"
    subprocess.check_call(
        ["ffmpeg", "-v", "error", "-ss", f"{at}", "-i", clip, "-vf", vf, "-vsync", "vfr",
         "-frames:v", str(n), "-q:v", "3", os.path.join(d, "%04d.jpg")])
    return sorted(glob.glob(os.path.join(d, "*.jpg"))), fps


def font(size):
    for p in ("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",):
        if os.path.exists(p):
            return ImageFont.truetype(p, size)
    return ImageFont.load_default()


def sheet(args):
    files, fps = frames(args.clip, args.at, args.n, args.step, args.w, args.crop)
    if not files:
        sys.exit("no frames")
    ims = [Image.open(f) for f in files]
    w, h = ims[0].size
    cols = min(args.cols, len(ims))
    rows = (len(ims) + cols - 1) // cols
    head = 26 if args.title else 0
    out = Image.new("RGB", (cols * w, rows * h + head), (12, 12, 12))
    d = ImageDraw.Draw(out)
    f = font(14)
    if args.title:
        d.text((6, 5), args.title, fill=(235, 235, 235), font=f)
    for i, im in enumerate(ims):
        x, y = (i % cols) * w, (i // cols) * h + head
        out.paste(im, (x, y))
        ms = i * args.step / fps * 1000
        label = f"f{i * args.step} +{ms:.0f}ms"
        d.rectangle((x, y, x + 9 * len(label), y + 18), fill=(0, 0, 0))
        d.text((x + 3, y + 1), label, fill=(255, 220, 90), font=f)
    out.save(args.out, quality=88)
    print(args.out, f"{len(ims)} frames at {fps:.0f} fps, every {args.step}")


def rises(clip, crop="", t0=0.0, t1=None):
    """Each frame's rise over the one before, of the brightest tenth of the
    picture (or of `crop`), from `t0` to `t1` seconds: [(time, rise)]."""
    _, _, fps = probe(clip)
    sw, sh = 96, 54
    vf = f"{crop_filter(crop)}scale={sw}:{sh},format=gray"
    span = ["-ss", f"{t0}"] + (["-t", f"{t1 - t0}"] if t1 is not None else [])
    raw = subprocess.check_output(
        ["ffmpeg", "-v", "error", *span, "-i", clip, "-vf", vf, "-f", "rawvideo", "-"])
    a = np.frombuffer(raw, np.uint8).reshape(-1, sh, sw).astype(np.float32)
    if len(a) == 0:
        return []
    top = np.sort(a.reshape(len(a), -1), axis=1)[:, -sw * sh // 10:].mean(axis=1)
    rise = np.diff(top, prepend=top[0])
    return [(t0 + i / fps, float(r)) for i, r in enumerate(rise)]


def peaks(args):
    _, _, fps = probe(args.clip)
    r = rises(args.clip, args.crop, args.t0, args.t1)
    seen = []
    for t, v in sorted(r, key=lambda x: -x[1]):
        if v < 4:
            break
        if all(abs(t - u) > 0.25 for u, _ in seen):
            seen.append((t, v))
        if len(seen) >= args.top:
            break
    for t, v in sorted(seen):
        print(f"t={t:7.3f}s  frame {round(t * fps):5d}  rise {v:5.1f}")


def main():
    p = argparse.ArgumentParser()
    if len(sys.argv) > 1 and sys.argv[1] == "peaks":
        p.add_argument("cmd")
        p.add_argument("clip")
        p.add_argument("--top", type=int, default=12)
        p.add_argument("--crop", default="")
        p.add_argument("--from", dest="t0", type=float, default=0.0)
        p.add_argument("--to", dest="t1", type=float, default=None)
        peaks(p.parse_args())
    else:
        p.add_argument("clip")
        p.add_argument("out")
        p.add_argument("--at", type=float, default=0.0)
        p.add_argument("--n", type=int, default=12)
        p.add_argument("--step", type=int, default=1)
        p.add_argument("--cols", type=int, default=6)
        p.add_argument("--w", type=int, default=400)
        p.add_argument("--crop", default="")
        p.add_argument("--title", default="")
        sheet(p.parse_args())


if __name__ == "__main__":
    main()

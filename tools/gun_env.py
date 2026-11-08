#!/usr/bin/env python3
"""tools/gun_env.py: what the guns reflect. A sky photograph from Poly Haven
(CC0), cut to a small cube with a level for each roughness, from a mirror's
(level 0) to a rough metal's (the last), packed as RGB9E5 for
`look/gun_skin.rs` (`crates/client/assets/textures/gun_env.rgb9e5`).

Guns are metal, and metal is what it reflects: the scene's own fill is a
two-tone cube (sky and ground, `look/fill.rs`), so a gun lit by it reads as
paint. This gives them a sky with a sun, clouds, a horizon and ground in it.

    tools/gun_env.py            fetch the photograph if missing, cut it

Needs python3 with numpy, and curl.
"""
import math
import os
import struct
import subprocess
import sys

import numpy as np

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ID = "kloofendal_48d_partly_cloudy"
SRC = os.path.join(ROOT, "sources", "polyhaven", f"{ID}_1k.hdr")
URL = f"https://dl.polyhaven.org/file/ph-assets/HDRIs/hdr/1k/{ID}_1k.hdr"
OUT = os.path.join(ROOT, "crates", "client", "assets", "textures", "gun_env.rgb9e5")

# The cube's face at level 0, and its levels (128 down to 4): keep both in
# step with `look/gun_skin.rs`' ENV_FACE and ENV_LEVELS.
FACE = 128
LEVELS = 6
# The brightest a texel may be, over the photograph's mean: the sun stays a
# hard glint on a mirror without a level's blur smearing it to a lamp.
CLAMP = 120.0


def read_hdr(path):
    """A Radiance .hdr (RGBE, run-length scanlines) as linear float RGB."""
    with open(path, "rb") as f:
        data = f.read()
    i = 0
    while True:
        j = data.index(b"\n", i)
        line = data[i:j]
        i = j + 1
        if line.startswith(b"-Y") or line.startswith(b"+Y"):
            parts = line.split()
            h, w = int(parts[1]), int(parts[3])
            break
    img = np.zeros((h, w, 4), np.uint8)
    for y in range(h):
        if data[i] == 2 and data[i + 1] == 2:
            i += 4
            for c in range(4):
                x = 0
                while x < w:
                    n = data[i]
                    i += 1
                    if n > 128:
                        n -= 128
                        img[y, x : x + n, c] = data[i]
                        i += 1
                    else:
                        img[y, x : x + n, c] = np.frombuffer(data[i : i + n], np.uint8)
                        i += n
                    x += n
        else:
            img[y] = np.frombuffer(data[i : i + 4 * w], np.uint8).reshape(w, 4)
            i += 4 * w
    e = img[..., 3].astype(np.float32)
    scale = np.where(e > 0, np.ldexp(1.0, (e - 136).astype(np.int32)), 0.0)
    return img[..., :3].astype(np.float32) * scale[..., None]


def blur(eq, sigma):
    """The photograph blurred by `sigma` radians on the sphere, near enough:
    a gaussian down its columns, and along each row one widened by the
    row's latitude, wrapping round."""
    if sigma <= 0:
        return eq
    h, w, _ = eq.shape
    out = np.empty_like(eq)
    freqs = np.fft.rfftfreq(w)
    spec = np.fft.rfft(eq, axis=1)
    for y in range(h):
        lat = (0.5 - (y + 0.5) / h) * math.pi
        s = sigma * w / (2 * math.pi) / max(math.cos(lat), 1e-3)
        s = min(s, w)
        g = np.exp(-2 * (math.pi * freqs * s) ** 2)
        out[y] = np.fft.irfft(spec[y] * g[:, None], n=w, axis=0)
    s = sigma * h / math.pi
    r = int(min(3 * s, h))
    if r >= 1:
        k = np.exp(-0.5 * (np.arange(-r, r + 1) / s) ** 2)
        k /= k.sum()
        pad = np.pad(out, ((r, r), (0, 0), (0, 0)), mode="edge")
        res = np.zeros_like(out)
        for t, kv in enumerate(k):
            res += kv * pad[t : t + h]
        out = res
    return np.maximum(out, 0.0)


def face_dirs(face, n, sub):
    """Each texel's directions on a cube face (GL's order and frame: +X −X
    +Y −Y +Z −Z), `sub`² of them a texel."""
    m = n * sub
    c = (np.arange(m) + 0.5) / m * 2 - 1
    s, t = np.meshgrid(c, c)
    one = np.ones_like(s)
    d = [
        (one, -t, -s),
        (-one, -t, s),
        (s, one, t),
        (s, -one, -t),
        (s, -t, one),
        (-s, -t, -one),
    ][face]
    v = np.stack(d, -1)
    return v / np.linalg.norm(v, axis=-1, keepdims=True)


def sample(eq, dirs):
    h, w, _ = eq.shape
    x, y, z = dirs[..., 0], dirs[..., 1], dirs[..., 2]
    u = (0.5 + np.arctan2(x, -z) / (2 * math.pi)) * w - 0.5
    v = (0.5 - np.arcsin(np.clip(y, -1, 1)) / math.pi) * h - 0.5
    x0 = np.floor(u).astype(int)
    y0 = np.floor(v).astype(int)
    fx = (u - x0)[..., None]
    fy = (v - y0)[..., None]
    x1 = (x0 + 1) % w
    x0 %= w
    y1 = np.clip(y0 + 1, 0, h - 1)
    y0 = np.clip(y0, 0, h - 1)
    top = eq[y0, x0] * (1 - fx) + eq[y0, x1] * fx
    bot = eq[y1, x0] * (1 - fx) + eq[y1, x1] * fx
    return top * (1 - fy) + bot * fy


def rgb9e5(c):
    n, b = 9, 15
    maxv = (2**n - 1) / 2**n * 2 ** (31 - b)
    c = np.clip(c, 0, maxv)
    m = c.max(-1)
    e = np.maximum(-b - 1, np.floor(np.log2(np.maximum(m, 1e-30)))) + 1 + b
    denom = np.exp2(e - b - n)
    mm = np.floor(m / denom + 0.5)
    over = mm >= 2**n
    denom = np.where(over, denom * 2, denom)
    e = np.where(over, e + 1, e)
    q = np.floor(c / denom[..., None] + 0.5).astype(np.uint32)
    return (q[..., 0] | (q[..., 1] << 9) | (q[..., 2] << 18) | (e.astype(np.uint32) << 27)).astype("<u4")


def main():
    if not os.path.exists(SRC):
        os.makedirs(os.path.dirname(SRC), exist_ok=True)
        subprocess.run(["curl", "-sSfL", "-o", SRC, URL], check=True)
    eq = read_hdr(SRC)
    lum = eq @ np.array([0.2126, 0.7152, 0.0722], np.float32)
    eq /= lum.mean()
    eq = np.minimum(eq, CLAMP)
    faces = [[None] * LEVELS for _ in range(6)]
    for lvl in range(LEVELS):
        rough = lvl / (LEVELS - 1)
        alpha = rough * rough
        sigma = 0.55 * alpha
        b = blur(eq, sigma)
        n = FACE >> lvl
        sub = 2 if lvl < 2 else 1
        for f in range(6):
            px = sample(b, face_dirs(f, n, sub))
            if sub > 1:
                px = px.reshape(n, sub, n, sub, 3).mean((1, 3))
            faces[f][lvl] = rgb9e5(px)
        print(f"level {lvl}: {n}² rough {rough:.1f}", file=sys.stderr)
    # wgpu's layer-major order: each face, every level of it.
    with open(OUT, "wb") as f:
        for face in faces:
            for lvl in face:
                f.write(lvl.tobytes())
    print(f"{os.path.relpath(OUT, ROOT)}: {os.path.getsize(OUT)} bytes")


if __name__ == "__main__":
    main()

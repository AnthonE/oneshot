#!/usr/bin/env python3
"""The flipbook sheets the effects play (look/fx/sheets.rs reads them):
smoke, dust, fire, plasma, muzzle flashes and explosions, each a strip of
frames baked from a small fluid simulation (Stam's stable fluids: advection,
buoyancy, vorticity, a pressure solve) and lit six ways, so the game lights
a puff from wherever its sun stands. Ours, made here; no pack.

    python3 tools/flipbooks/bake.py            # every sheet into crates/client/assets/fx
    python3 tools/flipbooks/bake.py cloud      # one sheet
    python3 tools/flipbooks/bake.py --preview  # traces/fx/sheets/*.png, lit, to look at

Each sheet is a pair of RGBA PNGs, linear (not sRGB):
  <name>_a.png  R, G, B: light arriving from the sprite's right, left and
                top, 0–1 (how much of it gets through the smoke to this
                texel); A: the smoke's coverage.
  <name>_b.png  R, G, B: light from below, from the front (the viewer's
                side) and from behind; A: heat, 0–1, which the shader
                turns into fire or plasma through a ramp.
A pixel with coverage 0 and heat above 0 is pure light (a flash); the pool
draws premultiplied, so it adds.

The cut of each sheet (cells, frames, variants) is a constant here and the
same constant in look/fx/sheets.rs; change both.
"""
import os
import sys
import time

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
OUT = os.path.join(ROOT, "crates", "client", "assets", "fx")
PREVIEW = os.path.join(ROOT, "traces", "fx", "sheets")

# ---------------------------------------------------------------- noise ----


def smoothstep(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def upsample(coarse, n):
    """`coarse` (m+1 × m+1) spread over n × n, cosine-smooth between the knots."""
    m = coarse.shape[0] - 1
    s = np.linspace(0.0, m, n, endpoint=False, dtype=np.float32)
    i0 = np.floor(s).astype(np.int32)
    f = s - i0
    f = f * f * (3.0 - 2.0 * f)
    i1 = np.minimum(i0 + 1, m)
    fy, fx = f[:, None], f[None, :]
    a = coarse[i0][:, i0]
    b = coarse[i0][:, i1]
    c = coarse[i1][:, i0]
    d = coarse[i1][:, i1]
    return a * (1 - fx) * (1 - fy) + b * fx * (1 - fy) + c * (1 - fx) * fy + d * fx * fy


def fbm(rng, n, octaves=4, base=4, gain=0.5):
    """Layered value noise over n × n, about zero, amplitude ~1."""
    out = np.zeros((n, n), np.float32)
    amp = 1.0
    for o in range(octaves):
        m = base * (2**o)
        coarse = rng.random((m + 1, m + 1), dtype=np.float32)
        out += amp * (upsample(coarse, n) - 0.5) * 2.0
        amp *= gain
    return out


def curl_noise(rng, n, octaves=3, base=3):
    """A divergence-free velocity field from a noise potential."""
    psi = fbm(rng, n, octaves, base)
    u = np.zeros_like(psi)
    v = np.zeros_like(psi)
    u[1:-1, :] = 0.5 * (psi[2:, :] - psi[:-2, :])
    v[:, 1:-1] = -0.5 * (psi[:, 2:] - psi[:, :-2])
    return u, v


# ------------------------------------------------------------- pressure ----


def sweep(p, rhs, iters):
    """Red-black Gauss–Seidel on ∇²p = rhs, p = 0 at the border, in place."""
    for _ in range(iters):
        for colour in (0, 1):
            for i0, j0 in ((1, 1), (2, 2)) if colour == 0 else ((1, 2), (2, 1)):
                up = p[i0 - 1 : -2 : 2, j0:-1:2]
                dn = p[i0 + 1 :: 2, j0:-1:2]
                lf = p[i0:-1:2, j0 - 1 : -2 : 2]
                rt = p[i0:-1:2, j0 + 1 :: 2]
                h = min(up.shape[0], dn.shape[0], lf.shape[0], rt.shape[0])
                w = min(up.shape[1], dn.shape[1], lf.shape[1], rt.shape[1])
                p[i0 : i0 + 2 * h : 2, j0 : j0 + 2 * w : 2] = 0.25 * (
                    up[:h, :w] + dn[:h, :w] + lf[:h, :w] + rt[:h, :w] - rhs[i0 : i0 + 2 * h : 2, j0 : j0 + 2 * w : 2]
                )
    return p


def laplacian(p):
    out = np.zeros_like(p)
    out[1:-1, 1:-1] = p[1:-1, 2:] + p[1:-1, :-2] + p[2:, 1:-1] + p[:-2, 1:-1] - 4.0 * p[1:-1, 1:-1]
    return out


def prolong(ec, n):
    """A coarse grid's values spread over the fine one, smoothed between."""
    e = np.repeat(np.repeat(ec, 2, axis=0), 2, axis=1)[:n, :n]
    o = e.copy()
    o[1:-1, 1:-1] = (
        0.25 * e[1:-1, 1:-1]
        + 0.125 * (e[:-2, 1:-1] + e[2:, 1:-1] + e[1:-1, :-2] + e[1:-1, 2:])
        + 0.0625 * (e[:-2, :-2] + e[:-2, 2:] + e[2:, :-2] + e[2:, 2:])
    )
    return o


def vcycle(p, rhs):
    """One multigrid V-cycle on ∇²p = rhs with p = 0 at the border: a few
    sweeps can't carry pressure across a grid this wide, so the smooth
    part of the error is solved on coarser grids and brought back up."""
    n = p.shape[0]
    if n <= 8:
        return sweep(p, rhs, 200)
    p = sweep(p, rhs, 2)
    r = rhs - laplacian(p)
    m = n // 2
    rc = r[: m * 2, : m * 2].reshape(m, 2, m, 2).mean(axis=(1, 3)) * 4.0
    ec = vcycle(np.zeros_like(rc), rc)
    p = p + prolong(ec, n)
    return sweep(p, rhs, 2)


# ---------------------------------------------------------------- fluid ----


class Fluid:
    """Stable fluids on an n × n grid. Image coordinates: y down, so `up` is
    −v. Open boundaries: what leaves is gone."""

    def __init__(self, n, seed):
        self.n = n
        self.rng = np.random.default_rng(seed)
        z = lambda: np.zeros((n, n), np.float32)
        self.u, self.v = z(), z()
        self.d, self.T, self.fuel = z(), z(), z()
        # Fine grain carried with the flow: the texture a grid this coarse
        # can't make, laid over the density when a frame is drawn.
        self.grain = fbm(self.rng, n, 4, 12, 0.55)
        self.p = z()
        ys, xs = np.mgrid[0:n, 0:n]
        self.xs = xs.astype(np.float32)
        self.ys = ys.astype(np.float32)
        self.t = 0.0

    def sample(self, f, x, y):
        n = self.n
        x = np.clip(x, 0.0, n - 1.001)
        y = np.clip(y, 0.0, n - 1.001)
        x0 = np.floor(x).astype(np.int32)
        y0 = np.floor(y).astype(np.int32)
        fx = x - x0
        fy = y - y0
        x1 = np.minimum(x0 + 1, n - 1)
        y1 = np.minimum(y0 + 1, n - 1)
        return (
            f[y0, x0] * (1 - fx) * (1 - fy)
            + f[y0, x1] * fx * (1 - fy)
            + f[y1, x0] * (1 - fx) * fy
            + f[y1, x1] * fx * fy
        )

    def advect(self, f, dt, drift=0.0):
        """Semi-Lagrangian, midpoint: where did what's here come from?
        `drift` settles a field (dust) downward on top of the flow."""
        xm = self.xs - 0.5 * dt * self.u
        ym = self.ys - 0.5 * dt * (self.v + drift)
        um = self.sample(self.u, xm, ym)
        vm = self.sample(self.v, xm, ym) + drift
        return self.sample(f, self.xs - dt * um, self.ys - dt * vm)

    def project(self, iters, expand=None):
        """Take the divergence out of the flow — all but `expand`, the
        divergence hot gas is allowed (it swells as it burns)."""
        u, v = self.u, self.v
        div = np.zeros_like(u)
        div[1:-1, 1:-1] = 0.5 * (u[1:-1, 2:] - u[1:-1, :-2] + v[2:, 1:-1] - v[:-2, 1:-1])
        if expand is not None:
            div -= expand
        # Warm-started from last step's pressure: the flow changes little.
        p = self.p
        for _ in range(iters):
            p = vcycle(p, div)
        self.p = p
        u[1:-1, 1:-1] -= 0.5 * (p[1:-1, 2:] - p[1:-1, :-2])
        v[1:-1, 1:-1] -= 0.5 * (p[2:, 1:-1] - p[:-2, 1:-1])

    def confine(self, eps, dt):
        """Vorticity confinement: put back the swirl the grid smears out."""
        u, v = self.u, self.v
        w = np.zeros_like(u)
        w[1:-1, 1:-1] = 0.5 * ((v[1:-1, 2:] - v[1:-1, :-2]) - (u[2:, 1:-1] - u[:-2, 1:-1]))
        aw = np.abs(w)
        gx = np.zeros_like(u)
        gy = np.zeros_like(u)
        gx[1:-1, 1:-1] = 0.5 * (aw[1:-1, 2:] - aw[1:-1, :-2])
        gy[1:-1, 1:-1] = 0.5 * (aw[2:, 1:-1] - aw[:-2, 1:-1])
        mag = np.sqrt(gx * gx + gy * gy) + 1e-5
        u += eps * dt * (gy / mag) * w
        v -= eps * dt * (gx / mag) * w

    def step(self, dt, buoy=1.0, weight=0.0, cool=0.5, burn=0.0, heat=1.0, soot=1.0,
             ignite=0.2, diss=0.0, damp=0.0, eps=0.0, iters=40, drift=0.0, up=1.0, visc=0.3,
             swell=0.0, bulge=0.0, floor=None):
        n = self.n
        if floor is not None:
            # The ground: nothing moves through it, and what reaches it
            # rolls along it.
            fy = int(floor * n)
            self.v[fy:, :] = np.minimum(self.v[fy:, :], 0.0)
            self.u[fy + 2 :, :] = 0.0
            self.v[fy + 2 :, :] = 0.0
        # Forces: hot air rises, heavy smoke sinks, and a wind of `up`.
        self.v -= dt * (buoy * self.T * up - weight * self.d) * n
        # What burns this step: heat and soot, and the swelling of the gas.
        r = None
        if burn > 0.0:
            r = self.fuel * smoothstep(ignite * 0.5, ignite, self.T) * min(1.0, burn * dt)
        expand = None
        if swell > 0.0:
            expand = swell * (self.T * 0.25 + (r * 12.0 if r is not None else 0.0) + self.d * bulge)
        if eps > 0.0:
            self.confine(eps, dt)
        if visc > 0.0:
            self.u = self.u * (1.0 - visc) + blur(self.u, 1) * visc
            self.v = self.v * (1.0 - visc) + blur(self.v, 1) * visc
        if damp > 0.0:
            k = np.exp(-damp * dt)
            self.u *= k
            self.v *= k
        self.project(iters, expand)
        # Carry everything along.
        u2 = self.advect(self.u, dt)
        v2 = self.advect(self.v, dt)
        self.d = self.advect(self.d, dt, drift * n)
        self.T = self.advect(self.T, dt)
        self.grain = self.advect(self.grain, dt)
        if burn > 0.0:
            self.fuel = self.advect(self.fuel, dt)
        self.u, self.v = u2, v2
        # Fuel burns where it is hot: heat and soot.
        if r is not None:
            r = self.advect(r, dt)
            self.T += r * heat
            self.d += r * soot
            self.fuel -= r
        self.T *= np.exp(-cool * dt)
        if diss > 0.0:
            self.d *= np.exp(-diss * dt)
        self.t += dt


# ------------------------------------------------------------- lighting ----


def blur(f, k=2):
    """A cheap box blur, k texels each way."""
    out = f.copy()
    for _ in range(k):
        o = out.copy()
        o[1:-1, 1:-1] = (
            out[1:-1, 1:-1] * 0.4
            + (out[:-2, 1:-1] + out[2:, 1:-1] + out[1:-1, :-2] + out[1:-1, 2:]) * 0.15
        )
        out = o
    return out


def six_way(d, h, k=1.0, kf=0.6, kb=1.6, ao=0.5):
    """Light through smoke of density `d` (texel size `h`) from the sprite's
    six sides: right, left, top, bottom, front, back, each 0–1."""
    od = d * h * k
    # The light from the right has crossed everything to the right of here.
    from_right = np.cumsum(od[:, ::-1], axis=1)[:, ::-1] - od
    from_left = np.cumsum(od, axis=1) - od
    from_top = np.cumsum(od, axis=0) - od
    from_bottom = np.cumsum(od[::-1, :], axis=0)[::-1, :] - od
    occ = np.exp(-ao * blur(d, 3) * 0.6)
    R = np.exp(-from_right) * occ
    L = np.exp(-from_left) * occ
    T = np.exp(-from_top) * occ
    B = np.exp(-from_bottom) * occ
    # The viewer's side: the surface facing it is lit, the thick inside less.
    F = np.exp(-kf * blur(d, 2) * 0.35) * (0.6 + 0.4 * occ)
    # From behind: only the thin edges glow through.
    Bk = np.exp(-kb * d)
    return R, L, T, B, F, Bk


def window(n, inner=0.40, outer=0.49):
    """A square fade near a cell's edge, so nothing touches the border."""
    c = (np.arange(n, dtype=np.float32) + 0.5) / n - 0.5
    m = np.maximum(np.abs(c)[:, None], np.abs(c)[None, :])
    return smoothstep(outer, inner, m)


def render(F, sd=2.2, st=1.0, t0=0.25, t1=1.6, gamma=0.75, k=1.0, grain=0.6, hide=0.0):
    """One frame from the fluid: (A, B) float arrays n × n × 4. `hide`:
    how much the soot in front hides the fire behind it."""
    n = F.n
    g = np.clip(1.0 + grain * F.grain, 0.2, 2.0)
    d = np.maximum(F.d, 0.0) * g
    T = np.maximum(F.T, 0.0) * np.clip(1.0 + 0.8 * grain * F.grain, 0.3, 1.8)
    w = window(n)
    alpha = (1.0 - np.exp(-(sd * d + st * np.clip(T - t0 * 0.5, 0.0, None)))) * w
    heat = np.clip((T - t0) / (t1 - t0), 0.0, 1.0) ** gamma * w
    if hide > 0.0:
        heat = heat * np.exp(-hide * blur(d, 1))
    R, L, Tp, B, Fr, Bk = six_way(d, 1.0 / n * 6.0, k=k)
    A = np.stack([R, L, Tp, alpha], axis=-1)
    Bq = np.stack([B, Fr, Bk, heat], axis=-1)
    return A.astype(np.float32), Bq.astype(np.float32)


def down(f, s):
    """Box-filter f (n × n × c) down by s."""
    n = f.shape[0] // s
    c = f.shape[2]
    return f[: n * s, : n * s].reshape(n, s, n, s, c).mean(axis=(1, 3))


# --------------------------------------------------------------- scenes ----


def turbulence(rng, n, k=3, octaves=3, base=5):
    """A few curl fields to blend over time: the churn inside hot gas."""
    return [curl_noise(rng, n, octaves, base) for _ in range(k)]


def churn(F, fields, strength, where, t):
    """Push `where` (0–1) around with the fields blended at time `t`."""
    k = len(fields)
    for i, (cu, cv) in enumerate(fields):
        w = 0.5 + 0.5 * np.cos(t * 2.1 + i * 2.0 * np.pi / k)
        F.u += cu * strength * w * where
        F.v += cv * strength * w * where


def explosion(seed, n, frames):
    """A charge going off: a ball of fuel lit all at once, blown outward,
    burning to a fireball that rolls up into smoke."""
    F = Fluid(n, seed)
    rng = F.rng
    cx, cy = n * 0.5, n * 0.6
    r = np.sqrt((F.xs - cx) ** 2 + (F.ys - cy) ** 2) / n
    nz = fbm(rng, n, 4, 3)
    r0 = 0.135 * (1.0 + 0.35 * nz)
    ball = smoothstep(r0, r0 * 0.45, r)
    lumps = np.clip(fbm(rng, n, 5, 6), -1, 1)
    F.fuel = ball * (1.0 + 0.8 * lumps)
    F.T = ball * (1.6 + 0.5 * lumps)
    dx = (F.xs - cx) / (r * n + 1e-3)
    dy = (F.ys - cy) / (r * n + 1e-3)
    v0 = 1.35 * n
    shell = smoothstep(0.02, 0.1, r) * ball
    F.u = dx * v0 * shell
    F.v = dy * v0 * shell - 0.1 * n * ball
    cu, cv = curl_noise(rng, n, 4, 5)
    F.u += cu * 1.6 * n * ball
    F.v += cv * 1.6 * n * ball
    fields = turbulence(rng, n, 3, 4, 8)
    out = []
    dt = 1.0 / 60.0
    for i in range(frames):
        for _ in range(4):
            hot = np.clip(F.T * 0.8, 0.0, 1.0)
            churn(F, fields, 1.2 * n * dt, hot, F.t)
            late = smoothstep(0.3, 1.3, F.t)
            F.step(
                dt, buoy=0.7 - 0.45 * late, weight=0.03, cool=0.95, burn=7.0, heat=1.3,
                soot=2.2, ignite=0.3, diss=0.1, damp=0.8 + 1.6 * late, eps=4.0,
                iters=2, swell=2.6 * (1.0 - 0.8 * late), visc=0.05,
            )
        out.append(render(F, sd=1.7, st=0.9, t0=0.28, t1=1.6, gamma=0.8, k=2.4, hide=0.2))
    return out


def puff(seed, n, frames, dust=False):
    """Smoke thrown up by a blow: a lump of it, pushed out and up, that
    swells, rolls, thins and drifts off. Dust is thrown off the ground: it
    rolls out wide and low along it and settles."""
    F = Fluid(n, seed)
    rng = F.rng
    floor = 0.78 if dust else None
    cx = n * (0.5 + rng.uniform(-0.04, 0.04))
    cy = n * (0.7 if dust else 0.58)
    r = np.sqrt(((F.xs - cx) / (1.4 if dust else 1.0)) ** 2 + (F.ys - cy) ** 2) / n
    nz = fbm(rng, n, 4, 3)
    r0 = (0.11 if dust else 0.12) * (1.0 + 0.45 * nz)
    lump = smoothstep(r0, r0 * 0.4, r)
    lumps = np.clip(fbm(rng, n, 5, 6), -1, 1)
    F.d = lump * (1.8 + 1.0 * lumps) * (1.4 if dust else 1.0)
    F.T = lump * (0.5 if dust else 0.9)
    dx = (F.xs - cx) / (r * n + 1e-3)
    dy = (F.ys - cy) / (r * n + 1e-3)
    v0 = (1.4 if dust else 1.2) * n
    shell = smoothstep(0.02, 0.1, r) * lump
    skew = 1.0 + 0.5 * np.clip(fbm(rng, n, 2, 2), -1, 1)
    F.u = dx * v0 * shell * (1.4 if dust else 1.0) * skew
    F.v = dy * v0 * shell * (0.7 if dust else 1.0) - (0.35 if dust else 0.35) * n * lump
    cu, cv = curl_noise(rng, n, 4, 5)
    F.u += cu * 1.6 * n * lump
    F.v += cv * 1.6 * n * lump
    fields = turbulence(rng, n, 3, 4, 7)
    out = []
    dt = 1.0 / 60.0
    for i in range(frames):
        for _ in range(4):
            where = np.clip(F.d * 0.7, 0.0, 1.0)
            churn(F, fields, (0.8 if dust else 1.0) * n * dt, where, F.t)
            late = smoothstep(0.2, 1.0, F.t)
            F.step(
                dt, buoy=(0.3 if dust else 0.6) * (1.0 - 0.6 * late), weight=0.1 if dust else 0.01,
                cool=1.2, diss=0.7 if dust else 0.55, damp=(0.8 if dust else 0.6) + 1.6 * late,
                eps=3.5, iters=2, drift=0.05 if dust else 0.0, visc=0.05,
                swell=(2.0 if dust else 2.2) * (1.0 - late), bulge=0.8, floor=floor,
            )
        A, B = render(F, sd=2.2 if dust else 1.8, st=0.0, t0=9.0, t1=10.0, k=2.0 if dust else 2.6, grain=0.8)
        out.append((A, B))
    return out


def fire(seed, n, frames):
    """Tongues of fire off a burning thing: fuel fed in at the base from a
    few flickering spots, hot, rising fast, torn apart, dark at the tips."""
    F = Fluid(n, seed)
    rng = F.rng
    cx, cy = n * 0.5, n * 0.84
    spots = [(cx + rng.uniform(-0.16, 0.16) * n, cy + rng.uniform(-0.02, 0.02) * n, rng.uniform(0.03, 0.05) * n, rng.uniform(0.0, 6.3), rng.uniform(5.0, 11.0)) for _ in range(5)]
    fields = turbulence(rng, n, 4, 4, 10)
    out = []
    dt = 1.0 / 60.0
    total = frames * 4 + 36
    for k in range(total):
        src = np.zeros((n, n), np.float32)
        for sx, sy, sr, ph, hz in spots:
            flick = 0.55 + 0.45 * np.sin(F.t * hz + ph) * np.sin(F.t * hz * 0.37 + ph * 1.7)
            rr = np.sqrt((F.xs - sx) ** 2 + ((F.ys - sy) * 1.5) ** 2)
            src += smoothstep(sr, sr * 0.3, rr) * flick
        F.fuel += src * 0.6
        F.T += src * 0.8
        churn(F, fields, 2.4 * n * dt, np.clip(F.T * 0.8, 0.0, 1.0), F.t * 1.5)
        F.step(
            dt, buoy=1.7, weight=0.0, cool=3.8, burn=18.0, heat=1.0, soot=0.03,
            ignite=0.2, diss=2.0, damp=0.5, eps=5.0, iters=2, visc=0.03, swell=1.0,
        )
        if k >= 36 and (k - 36) % 4 == 3:
            out.append(render(F, sd=1.2, st=1.6, t0=0.35, t1=1.5, gamma=0.7, k=0.8, grain=0.9))
    return out


def plasma(seed, n, frames):
    """Energy going off: no soot, all heat, a shell blown out and torn by
    its own turbulence, cooling fast to nothing."""
    F = Fluid(n, seed)
    rng = F.rng
    cx, cy = n * 0.5, n * 0.5
    r = np.sqrt((F.xs - cx) ** 2 + (F.ys - cy) ** 2) / n
    nz = fbm(rng, n, 4, 4)
    r0 = 0.16 * (1.0 + 0.3 * nz)
    ball = smoothstep(r0, r0 * 0.4, r)
    F.T = ball * (2.6 + 0.8 * np.clip(fbm(rng, n, 5, 6), -1, 1))
    dx = (F.xs - cx) / (r * n + 1e-3)
    dy = (F.ys - cy) / (r * n + 1e-3)
    v0 = 1.6 * n
    shell = smoothstep(0.02, 0.09, r) * ball
    F.u = dx * v0 * shell
    F.v = dy * v0 * shell
    cu, cv = curl_noise(rng, n, 4, 6)
    F.u += cu * 2.2 * n * ball
    F.v += cv * 2.2 * n * ball
    fields = turbulence(rng, n, 3, 4, 9)
    out = []
    dt = 1.0 / 60.0
    for i in range(frames):
        for _ in range(3):
            churn(F, fields, 1.8 * n * dt, np.clip(F.T * 0.6, 0.0, 1.0), F.t)
            late = smoothstep(0.1, 0.6, F.t)
            F.step(
                dt, buoy=0.0, weight=0.0, cool=2.2, damp=0.3 + 3.0 * late, eps=4.0, iters=2,
                visc=0.05, swell=5.0 * (1.0 - late),
            )
        A, B = render(F, sd=0.0, st=0.0, t0=0.12, t1=1.8, gamma=0.65, grain=0.9)
        A[..., 3] = 0.0
        out.append((A, B))
    return out


def wisp(seed, n, frames):
    """The breath of smoke off a muzzle: thin, quick, curling."""
    F = Fluid(n, seed)
    rng = F.rng
    cx, cy = n * 0.5, n * 0.58
    r = np.sqrt((F.xs - cx) ** 2 + (F.ys - cy) ** 2) / n
    lump = smoothstep(0.14, 0.04, r) * smoothstep(-0.5, 0.3, fbm(rng, n, 4, 5))
    F.d = lump * (1.0 + 0.6 * np.clip(fbm(rng, n, 4, 6), -1, 1))
    F.T = lump * 0.6
    cu, cv = curl_noise(rng, n, 4, 4)
    F.u = cu * 2.4 * n * smoothstep(0.25, 0.05, r)
    F.v = cv * 2.4 * n * smoothstep(0.25, 0.05, r) - 0.4 * n * lump
    fields = turbulence(rng, n, 3, 4, 8)
    out = []
    dt = 1.0 / 60.0
    for i in range(frames):
        for _ in range(3):
            churn(F, fields, 1.0 * n * dt, np.clip(F.d, 0.0, 1.0), F.t)
            late = smoothstep(0.1, 0.6, F.t)
            F.step(dt, buoy=0.5, cool=1.5, diss=0.8, damp=1.0 + 2.0 * late, eps=3.0, iters=2,
                   visc=0.05, swell=1.8 * (1.0 - late), bulge=0.5)
        out.append(render(F, sd=1.6, st=0.0, t0=9.0, t1=10.0, k=2.0, grain=0.7))
    return out


def muzzle(seed, n, frames, kind="powder"):
    """A gun's flash, drawn, not simulated: a hot core and petals of fire
    flung out the barrel and sideways, that grow, split and go. `plasma`:
    a soft bloom with a ring and no petals."""
    rng = np.random.default_rng(seed)
    ys, xs = np.mgrid[0:n, 0:n]
    x = (xs + 0.5) / n - 0.5
    y = (ys + 0.5) / n - 0.5
    r = np.sqrt(x * x + y * y)
    ang = np.arctan2(y, x)
    petals = []
    k = int(rng.integers(5, 9))
    for i in range(k):
        a = (i / k) * 2 * np.pi + rng.uniform(-0.25, 0.25)
        petals.append((a, rng.uniform(0.22, 0.46), rng.uniform(0.06, 0.13), rng.uniform(0.6, 1.0)))
    grain = fbm(rng, n, 4, 6)
    out = []
    for f in range(frames):
        t = f / max(frames - 1, 1)
        grow = smoothstep(0.0, 0.45, t)
        fade = 1.0 - smoothstep(0.35, 1.0, t)
        if kind == "plasma":
            core = np.exp(-((r / (0.06 + 0.1 * grow)) ** 2)) * 1.4
            ring_r = 0.08 + 0.3 * t
            ring = np.exp(-(((r - ring_r) / 0.035) ** 2)) * (1.0 - t) * 0.9
            halo = np.exp(-((r / (0.2 + 0.15 * grow)) ** 1.5)) * 0.35
            heat = (core + ring + halo) * fade * (1.0 + 0.25 * grain)
        else:
            core = np.exp(-((r / (0.07 + 0.05 * grow)) ** 2)) * 1.6 * (1.0 - 0.5 * t)
            heat = core
            for a, length, width, bright in petals:
                da = np.angle(np.exp(1j * (ang - a)))
                along = r / (length * (0.35 + 0.65 * grow))
                # The petal tears into bits as it goes.
                tear = 1.0 - smoothstep(0.3, 0.9, t) * smoothstep(-0.2, 0.3, grain)
                p = np.exp(-((da / width) ** 2) * (1.0 + 3.0 * along)) * smoothstep(1.0, 0.25, along) * bright
                heat += p * tear * (0.4 + 0.6 * smoothstep(0.0, 0.15, along))
            heat *= fade * (1.0 + 0.3 * grain)
        heat = np.clip(heat, 0.0, 1.0) * window(n)
        A = np.zeros((n, n, 4), np.float32)
        B = np.zeros((n, n, 4), np.float32)
        B[..., 3] = heat
        out.append((A, B))
    return out


# --------------------------------------------------------------- sheets ----

# The cut of each sheet: name → (cells a side, texels a cell, [(what, variants, frames, how)]).
# look/fx/sheets.rs holds the same numbers.
SHEETS = {
    "cloud": (16, 128, [
        ("smoke", 4, 16, lambda s, n, f: puff(s, n, f)),
        ("dust", 4, 16, lambda s, n, f: puff(s, n, f, dust=True)),
        ("fire", 2, 16, fire),
        ("plasma", 2, 16, plasma),
        ("flash", 6, 4, lambda s, n, f: muzzle(s, n, f)),
        ("bloom", 2, 4, lambda s, n, f: muzzle(s, n, f, kind="plasma")),
        ("wisp", 2, 16, wisp),
    ]),
    "boom": (8, 256, [
        ("explosion", 2, 32, explosion),
    ]),
}
# The simulation runs at this many times the cell's texels, then is filtered down.
OVERSAMPLE = 2


def bake(name, preview):
    cells, tex, parts = SHEETS[name]
    side = cells * tex
    A = np.zeros((side, side, 4), np.float32)
    B = np.zeros((side, side, 4), np.float32)
    cell = 0
    for what, variants, frames, how in parts:
        for v in range(variants):
            t0 = time.time()
            seed = (hash(name) & 0xFFFF) * 131 + (hash(what) & 0xFFFF) * 17 + v
            seed = abs(seed) % (2**31)
            frames_out = how(seed, tex * OVERSAMPLE, frames)
            for a, b in frames_out:
                assert cell < cells * cells, f"{name}: out of cells at {what} {v}"
                cx, cy = (cell % cells) * tex, (cell // cells) * tex
                A[cy : cy + tex, cx : cx + tex] = down(a, OVERSAMPLE)
                B[cy : cy + tex, cx : cx + tex] = down(b, OVERSAMPLE)
                cell += 1
            print(f"  {name}: {what} {v} ({frames} frames) {time.time() - t0:.1f}s", flush=True)
    assert cell == cells * cells, f"{name}: {cell} of {cells * cells} cells filled"
    os.makedirs(OUT, exist_ok=True)
    for tag, arr in (("a", A), ("b", B)):
        img = Image.fromarray((np.clip(arr, 0, 1) * 255.0 + 0.5).astype(np.uint8), "RGBA")
        path = os.path.join(OUT, f"{name}_{tag}.png")
        img.save(path, optimize=True)
        print(f"  {path}: {os.path.getsize(path) // 1024} KB")
    if preview:
        os.makedirs(PREVIEW, exist_ok=True)
        lit = light(A, B)
        Image.fromarray(lit, "RGB").save(os.path.join(PREVIEW, f"{name}.png"))


def light(A, B, sun=(0.6, 0.7, 0.4), bg=0.35):
    """What a sheet looks like lit from the top right over a grey card, the
    shader's arithmetic in numpy, for looking at."""
    lx, ly, lz = sun
    s = np.sqrt(lx * lx + ly * ly + lz * lz)
    lx, ly, lz = lx / s, ly / s, lz / s
    lit = (
        A[..., 0] * max(lx, 0) + A[..., 1] * max(-lx, 0)
        + A[..., 2] * max(ly, 0) + B[..., 0] * max(-ly, 0)
        + B[..., 1] * max(lz, 0) + B[..., 2] * max(-lz, 0)
    )
    smoke = 0.75 * (0.25 + 0.75 * lit)
    heat = np.clip(B[..., 3], 0, 1)
    # Black body-ish: dark red, orange, yellow, white, brighter as it goes.
    fire = np.stack([
        smoothstep(0.0, 0.25, heat),
        smoothstep(0.2, 0.7, heat) * 0.9,
        smoothstep(0.55, 1.0, heat) * 0.8,
    ], axis=-1) * (0.4 + 3.0 * heat * heat)[..., None]
    alpha = A[..., 3][..., None]
    rgb = smoke[..., None] * alpha + fire * np.clip(heat, 0, 1)[..., None]
    out = rgb + bg * (1.0 - alpha)
    out = out / (1.0 + out)
    return (np.clip(out, 0, 1) ** (1 / 2.2) * 255.0 + 0.5).astype(np.uint8)


def main(argv):
    preview = "--preview" in argv
    names = [a for a in argv if not a.startswith("--")] or list(SHEETS)
    for n in names:
        print(f"== {n}", flush=True)
        bake(n, preview)


if __name__ == "__main__":
    main(sys.argv[1:])

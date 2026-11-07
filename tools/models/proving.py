"""Proving Grounds' test range: everything built, in the map's own coordinates
(GLBs at the origin, `look/built.rs` draws them). A blast range under snow: cast
revetments round it, an observation bunker each side, a scorched target in the
middle, gauges for cover, floodlights on its corners, mountains all round.

    blender -b --python tools/models/proving.py

Its boxes are `maps/proving_grounds/map.json`'s, the golden's: this draws inside
them and moves none.
"""
import math
import os
import random
import sys

import bpy
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import arch  # noqa: E402
from arch import BACK, cbox, digits, export, make_objects, paint, panel, part, quad, rod, spin, strip_light, surface, to_blender, torus, tri  # noqa: E402,F401

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, '..', '..'))
OUT = os.path.join(ROOT, 'crates', 'client', 'assets', 'models', 'proving')

arch.TILES.update({'blast': 3.6, 'blast_dark': 3.6, 'snow': 2.6, 'rust': 2.4})
arch.UNTRACED.update({'decal', 'track', 'peaks', 'shaft'})

W = 24.0


def turn(k, x, z):
    for _ in range(k):
        x, z = -z, x
    return x, z


def qbox(k, x0, y0, z0, x1, y1, z1, mat='blast', c=0.04, **kw):
    a, b = turn(k, x0, z0), turn(k, x1, z1)
    cbox(min(a[0], b[0]), y0, min(a[1], b[1]), max(a[0], b[0]), y1, max(a[1], b[1]), mat, c=c, **kw)


def snow_cap(x0, y, z0, x1, z1, k=0):
    """Snow lying on a flat top: a low soft cushion inside its plan."""
    n = 5
    rows = []
    for j in range(n + 1):
        row = []
        for i in range(n + 1):
            u, v = i / n, j / n
            e = min(u, 1 - u, v, 1 - v) * 2
            h = 0.09 * (1 - (1 - min(1.0, e * 2)) ** 2) * (0.75 + 0.25 * math.sin(u * 7.1 + v * 5.3))
            px, pz = turn(k, x0 + (x1 - x0) * u, z0 + (z1 - z0) * v)
            row.append((px, y + 0.004 + h, pz))
        rows.append(row)
    surface(rows, 'snow', (0, 1, 0), lambda p: (p[0], p[2]))


def wall_side(k):
    """North side, turned: revetments in eight-metre bays out to the plane at their
    buttresses, a setback ledge with snow on it, a hazard band, a lamp every other."""
    for i, x in enumerate(range(-24, 24, 8)):
        x0, x1 = float(x), float(x + 8)
        # The lower wall, set back from the plane between its buttresses.
        qbox(k, x0, 0.0, W + 0.42, x1, 3.0, W + 1.0, 'blast', c=0.02, cell=1.2)
        qbox(k, x0, 3.0, W + 0.78, x1, 8.0, W + 1.0, 'blast', c=0.02, cell=1.2)
        snow_cap(x0, 3.0, W + 0.44, x1, W + 0.78, k)
        qbox(k, x0, 8.0, W + 0.78, x1, 8.14, W + 1.0, 'blast_dark', c=0.03, cell=2.0)
        snow_cap(x0, 8.14, W + 0.8, x1, W + 1.0, k)
        if i % 2 == 1:
            # A blast deflector over the bay, raked in toward the range.
            qbox(k, x0 + 1.4, 8.14, W + 0.5, x1 - 1.4, 9.9, W + 1.0, 'blast', c=0.06, cell=1.6)
            snow_cap(x0 + 1.45, 9.9, W + 0.52, x1 - 1.45, W + 0.98, k)
        # Its buttress, out to the plane.
        qbox(k, x0 - 0.45, 0.0, W, x0 + 0.45, 3.3, W + 0.6, 'blast', c=0.05)
        qbox(k, x0 - 0.45, 3.3, W + 0.1, x0 + 0.45, 6.0, W + 0.78, 'blast', c=0.05)
        snow_cap(x0 - 0.4, 3.3, W + 0.02, x0 + 0.4, W + 0.1, k)
        snow_cap(x0 - 0.4, 6.0, W + 0.12, x0 + 0.4, W + 0.42, k)
        # A hazard band along it, chevrons.
        u = x0 + 0.5
        n = 0
        while u < x1 - 0.5:
            mat = 'paint_y' if n % 2 == 0 else 'paint_k'
            a = (u, 1.2, W + 0.412)
            b = (u + 0.5, 1.2, W + 0.412)
            c = (u + 0.82, 1.62, W + 0.412)
            d = (u + 0.32, 1.62, W + 0.412)
            def t(p):
                px, pz = turn(k, p[0], p[2])
                return (px, p[1], pz)
            ox, oz = turn(k, 0.0, -1.0)
            quad(t(a), t(b), t(c), t(d), mat, (ox, 0, oz), cell=9.0)
            u += 0.5
            n += 1
        # The bay's number, big, to call out.
        mx = (x0 + x1) / 2
        # Read from the range: its right is the wall's -x, turned.
        at = turn(k, mx + 1.0, W + 0.77)
        right = turn(k, -1.0, 0.0)
        ox, oz = turn(k, 0.0, -1.0)
        digits(f'{k * 6 + i + 1:02d}', (at[0], 4.2, at[1]), right, (ox, oz), 2.0, 'paint_w')
        if i % 2 == 0:
            # A caged lamp on the buttress.
            lx, lz = turn(k, x0 + 4.0, W + 0.38)
            ax, az = turn(k, 0.0, -0.22)
            cbox(lx + min(ax, 0) - 0.14, 6.2, lz + min(az, 0) - 0.14, lx + max(ax, 0) + 0.14, 6.46, lz + max(az, 0) + 0.14, 'steel', c=0.03, cell=2.0)
            cbox(lx + min(ax, 0) - 0.1, 6.16, lz + min(az, 0) - 0.1, lx + max(ax, 0) + 0.1, 6.2, lz + max(az, 0) + 0.1, 'light_amber', c=0.0, cell=2.0)


def bunker(sx):
    """An observation bunker in the map's box, x 12..20, z -4..4, 2.5 high. A slit
    each long side, a steel door at its back, snow on its roof, a side's light."""
    X = lambda x: x * sx
    light = 'light_red' if sx < 0 else 'light_blue'
    def bx(x0, y0, z0, x1, y1, z1, mat='blast', c=0.04, **kw):
        a, b = sorted((X(x0), X(x1)))
        cbox(a, y0, z0, b, y1, z1, mat, c=c, **kw)
    # The body, set back; its corners and its roof slab out to the plane.
    bx(12.0 + BACK, 0.0, -4.0 + BACK, 20.0 - BACK, 2.08, 4.0 - BACK, 'blast', c=0.02, cell=1.2)
    bx(12.0, 2.08, -4.0, 20.0, 2.5, 4.0, 'blast', c=0.04, cell=1.4)
    for x, z in ((12.0, -4.0), (19.4, -4.0), (12.0, 3.4), (19.4, 3.4)):
        bx(x, 0.0, z, x + 0.6, 2.08, z + 0.6, 'blast', c=0.05)
    # Its long faces' slits, amber behind them.
    for z, o in ((-4.0 + BACK - 0.012, -1), (4.0 - BACK + 0.012, 1)):
        a, b = sorted((X(13.0), X(19.0)))
        quad((a, 1.3, z), (b, 1.3, z), (b, 1.62, z), (a, 1.62, z), 'blast_dark', (0, 0, o), cell=9.0)
        z2 = z + o * 0.008
        quad((a + 0.08, 1.36, z2), (b - 0.08, 1.36, z2), (b - 0.08, 1.56, z2), (a + 0.08, 1.56, z2), 'light_amber', (0, 0, o), cell=30.0)
    # Its back's door, rust in a steel frame.
    px = X(20.0 - BACK + 0.012)
    o = (X(1), 0, 0)
    quad((px, 0.02, -1.1), (px, 0.02, 1.1), (px, 1.98, 1.1), (px, 1.98, -1.1), 'steel', o, cell=9.0)
    px2 = X(20.0 - BACK + 0.024)
    quad((px2, 0.08, -0.98), (px2, 0.08, 0.98), (px2, 1.9, 0.98), (px2, 1.9, -0.98), 'rust', o, cell=2.0)
    # Its front's face over the steps: the side's light and a hazard band.
    px3 = X(12.0 + BACK - 0.012)
    quad((px3, 1.92, -3.3), (px3, 1.92, 3.3), (px3, 1.98, 3.3), (px3, 1.98, -3.3), light, (-X(1), 0, 0), cell=30.0)
    # Snow on the roof, thin, leaving the middle trodden.
    for z0, z1 in ((-3.9, -2.2), (2.2, 3.9)):
        a, b = sorted((X(12.1), X(19.9)))
        snow_cap(a, 2.5, z0, b, z1)
    # The roof's own light along its inner edge.
    a, b = sorted((X(12.04), X(12.1)))
    strip_light(a, 2.5, -3.4, b, 2.512, 3.4, light)
    # The steps down, in the map's boxes, snow in their corners.
    for x0, x1, h in ((11.0, 12.0, 2.0), (10.0, 11.0, 1.5), (9.0, 10.0, 1.0), (8.0, 9.0, 0.5)):
        bx(x0, 0.0, -1.5, x1, h, 1.5, 'blast', c=0.03)
        for z0, z1 in ((-1.48, -1.08), (1.08, 1.48)):
            a, b = sorted((X(x0 + 0.04), X(x1 - 0.04)))
            snow_cap(a, h, z0, b, z1)


def target():
    """The middle: a two-step dais, rings painted on it, scorched."""
    cbox(-3.0, 0.0, -3.0, 3.0, 0.5, 3.0, 'blast', c=0.05, cell=1.0)
    cbox(-2.0, 0.5, -2.0, 2.0, 1.0, 2.0, 'blast_dark', c=0.05, cell=1.0)
    for r, w in ((1.62, 0.09), (1.1, 0.08)):
        spin(0.0, 0.0, [(r + w, 1.004), (r - w, 1.004)], 'paint_w', sides=40, smooth=False)
    spin(0.0, 0.0, [(0.3, 1.004), (0.001, 1.004)], 'paint_y', sides=20, smooth=False)
    for s in (-1, 1):
        cbox(s * 2.4 - 0.08, 0.5, -2.6, s * 2.4 + 0.08, 0.504, 2.6, 'paint_y', c=0.0, cell=6.0)
        cbox(-2.6, 0.5, s * 2.4 - 0.08, 2.6, 0.504, s * 2.4 + 0.08, 'paint_y', c=0.0, cell=6.0)


def gauge(x, z):
    """A blast gauge in its 1.5 m box, 3 high: a steel-clad post, bands of hazard,
    a sensor head, a beacon on its side."""
    a = 0.75
    cbox(x - a + 0.06, 0.0, z - a + 0.06, x + a - 0.06, 2.8, z + a - 0.06, 'rust', c=0.06, cell=1.2)
    cbox(x - a, 0.0, z - a, x + a, 0.32, z + a, 'blast', c=0.05)
    cbox(x - a, 2.78, z - a, x + a, 3.0, z + a, 'steel', c=0.04, cell=2.0)
    snow_cap(x - a + 0.04, 3.0, z - a + 0.04, x + a - 0.04, z + a - 0.04)
    for y0, y1 in ((0.9, 1.08), (1.9, 2.08)):
        cbox(x - a + 0.04, y0, z - a + 0.04, x + a - 0.04, y1, z + a - 0.04, 'paint_y', c=0.01, cell=3.0)
    for sx, sz in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        px, pz = x + sx * (a - 0.05), z + sz * (a - 0.05)
        cbox(px - 0.06 - abs(sz) * 0.06, 2.3, pz - 0.06 - abs(sx) * 0.06, px + 0.06 + abs(sz) * 0.06, 2.46, pz + 0.06 + abs(sx) * 0.06, 'light_white', c=0.0, cell=3.0)


def low_wall(z):
    cbox(-6.0, 0.0, z - 0.25, 6.0, 1.05, z + 0.25, 'blast', c=0.04, cell=1.0)
    cbox(-6.0, 1.05, z - 0.25, 6.0, 1.2, z + 0.25, 'blast_dark', c=0.04, cell=2.0)
    snow_cap(-5.96, 1.2, z - 0.23, 5.96, z + 0.23)
    for x in (-4.0, 0.0, 4.0):
        cbox(x - 0.32, 0.22, z - 0.262, x + 0.32, 0.86, z + 0.262, 'paint_y', c=0.0, cell=3.0)


def decal_disc(x, z, r, k, rng, mat='decal', lobes=11):
    """A soft dark mark on the snow: dark in the middle, gone at its uneven edge."""
    pa = part(mat)
    n, rows = 28, 5
    phase = rng.uniform(0, 6)
    pts = [(x, 0.012, z)]
    for j in range(1, rows + 1):
        for i in range(n):
            t = math.tau * i / n
            wob = 1.0 + 0.16 * math.sin(t * lobes + phase) + 0.08 * math.sin(t * 5 + phase * 2)
            rr = r * j / rows * wob
            pts.append((x + math.cos(t) * rr, 0.012, z + math.sin(t) * rr))
    base = pa.add(pts)
    pa.colors[base] = (1, 1, 1, k)
    for j in range(1, rows + 1):
        a = k * (1.0 - j / rows) ** 1.3
        for i in range(n):
            pa.colors[base + 1 + (j - 1) * n + i] = (1, 1, 1, a)
    for i in range(n):
        pa.faces.append(((base, base + 1 + (i + 1) % n, base + 1 + i), [(0, 0)] * 3, True))
    for j in range(rows - 1):
        for i in range(n):
            i1 = (i + 1) % n
            p, q = base + 1 + j * n + i, base + 1 + j * n + i1
            r_, s = base + 1 + (j + 1) * n + i, base + 1 + (j + 1) * n + i1
            pa.faces.append(((p, q, s, r_), [(0, 0)] * 4, True))


def tracks(pts, gauge_w=1.5, width=0.34, alpha=0.5):
    """A vehicle's two ruts along a curve of points, their edges soft."""
    pa = part('track')
    for s in (-1, 1):
        rows = []
        for i, (x, z) in enumerate(pts):
            p, q = pts[max(i - 1, 0)], pts[min(i + 1, len(pts) - 1)]
            dx, dz = q[0] - p[0], q[1] - p[1]
            l = math.hypot(dx, dz) or 1.0
            nx, nz = -dz / l, dx / l
            cx, cz = x + nx * s * gauge_w / 2, z + nz * s * gauge_w / 2
            rows.append([(cx + nx * o, 0.01, cz + nz * o) for o in (-width, -width * 0.4, width * 0.4, width)])
        base = pa.add([p for row in rows for p in row])
        for j, row in enumerate(rows):
            wear = alpha * (0.72 + 0.28 * math.sin(j * 1.7))
            for k, o in enumerate((0.0, 1.0, 1.0, 0.0)):
                pa.colors[base + j * 4 + k] = (1, 1, 1, wear * o)
        for j in range(len(rows) - 1):
            for k in range(3):
                a = base + j * 4 + k
                pa.faces.append(((a, a + 1, a + 5, a + 4), [(0, 0)] * 4, True))


def floor():
    rng = random.Random(14)
    # Scorch: round the target hard, a few old shots out on the range.
    decal_disc(0.0, 0.0, 6.6, 0.92, rng)
    for x, z, r, k in ((-10.0, 10.0, 2.6, 0.6), (12.0, -13.0, 3.2, 0.66), (-15.0, -19.0, 2.2, 0.5), (5.0, 19.0, 2.4, 0.5), (19.0, 18.0, 1.9, 0.5), (-6.0, -6.0, 1.7, 0.42)):
        decal_disc(x, z, r, k, rng)
    # The carriers' ruts: lanes the range's vehicles wore.
    def curve(a, b, bend, n=26):
        pts = []
        for i in range(n + 1):
            t = i / n
            pts.append((a[0] + (b[0] - a[0]) * t + bend * math.sin(t * math.pi) * (b[1] - a[1]) / 40.0, a[1] + (b[1] - a[1]) * t - bend * math.sin(t * math.pi) * (b[0] - a[0]) / 40.0))
        return pts
    tracks(curve((-23.5, -14.0), (23.5, -10.0), 3.0))
    tracks(curve((-23.5, 14.0), (23.5, 10.0), -3.0))
    tracks(curve((-11.0, -23.5), (-6.0, 23.5), 5.0), alpha=0.32)
    # Drifts along the walls and in the lee of things: a hand high, so no one's
    # feet are hidden by more than a fall of snow would hide them.
    for k in range(4):
        n, m = 48, 5
        rows = []
        for j in range(m + 1):
            row = []
            for i in range(n + 1):
                u = -W + 2 * W * i / n
                d = j / m
                reach = 2.6 + 1.4 * math.sin(u * 0.29 + k * 1.7) * math.sin(u * 0.13 + k * 0.6)
                h = 0.2 * (1.0 - d) ** 1.6 * (0.7 + 0.3 * math.sin(u * 0.73 + k * 2.1) * math.sin(u * 0.31 + k))
                px, pz = turn(k, u, W + 0.42 - reach * d)
                row.append((px, 0.002 + h, pz))
            rows.append(row)
        surface(rows, 'snow', (0, 1, 0), lambda p: (p[0], p[2]))


def flood_masts():
    """A floodlight on each corner, over the wall: a mast, a bank of lamps, the
    light coming down soft."""
    sh = part('shaft')
    for sx in (-1, 1):
        for sz in (-1, 1):
            x, z = sx * 24.6, sz * 24.6
            rod((x, 8.0, z), (x, 13.0, z), 0.2, 'steel', sides=8, taper=0.6)
            for dx, dz in ((0.8, 0.8), (-0.8, 0.8), (0.8, -0.8), (-0.8, -0.8)):
                rod((x + dx * 0.6, 8.0, z + dz * 0.6), (x, 12.5, z), 0.05, 'steel', sides=6)
            # The bank, turned in over the range.
            ix, iz = -sx * 0.5, -sz * 0.5
            cbox(x + ix - 1.1, 13.0, z + iz - 1.1, x + ix + 1.1, 13.4, z + iz + 1.1, 'steel', c=0.05, cell=2.0)
            for i in (-0.6, 0.0, 0.6):
                for j in (-0.6, 0.0, 0.6):
                    cbox(x + ix + i - 0.22, 12.96, z + iz + j - 0.22, x + ix + i + 0.22, 13.0, z + iz + j + 0.22, 'light_white', c=0.0, cell=2.0)
            # Its cone, added: crossed sheets soft at their edges.
            tip = (x + ix, 12.9, z + iz)
            to = (x + ix - sx * 12.0, 0.2, z + iz - sz * 12.0)
            for a in (0.0, 1.05, 2.1):
                ca, sa = math.cos(a), math.sin(a)
                wx, wz = ca * 9.0, sa * 9.0
                rows, cols = 5, 4
                base = sh.add([
                    (tip[0] + (to[0] - tip[0]) * j / rows + wx * (j / rows) * (-1 + 2 * i / cols), tip[1] + (to[1] - tip[1]) * j / rows, tip[2] + (to[2] - tip[2]) * j / rows + wz * (j / rows) * (-1 + 2 * i / cols))
                    for j in range(rows + 1) for i in range(cols + 1)
                ])
                for j in range(rows + 1):
                    for i in range(cols + 1):
                        e = 1.0 - abs(-1 + 2 * i / cols)
                        v = 0.03 * e * e * (1.0 - j / rows * 0.9)
                        sh.colors[base + j * (cols + 1) + i] = (v * 1.0, v * 0.84, v * 0.66)
                for j in range(rows):
                    for i in range(cols):
                        q = base + j * (cols + 1) + i
                        sh.faces.append(((q, q + 1, q + cols + 2, q + cols + 1), [(0, 0)] * 4, True))


def ridged(x, z, seed):
    """Ridged noise over (x, z) arrays: sharp crests, smooth valleys, in 0..1."""
    rng = np.random.default_rng(seed)
    n = 256
    table = rng.random((n, n))
    def value(px, pz):
        ix, iz = np.floor(px).astype(int), np.floor(pz).astype(int)
        fx, fz = px - ix, pz - iz
        ux, uz = fx * fx * (3 - 2 * fx), fz * fz * (3 - 2 * fz)
        i0, i1, j0, j1 = ix % n, (ix + 1) % n, iz % n, (iz + 1) % n
        return (table[i0, j0] * (1 - ux) + table[i1, j0] * ux) * (1 - uz) + (table[i0, j1] * (1 - ux) + table[i1, j1] * ux) * uz
    total, amp, f, weight = 0.0, 0.5, 1.0 / 90.0, np.ones_like(x)
    for _ in range(7):
        v = 1.0 - np.abs(value(x * f, z * f) * 2.0 - 1.0)
        v = v * v * weight
        weight = np.clip(v * 2.0, 0.0, 1.0)
        total = total + v * amp
        amp *= 0.52
        f *= 2.07
    return total


def peaks():
    """The plain outside and the mountains round it, one sheet painted by slope and
    height: snow where it can lie, dark rock where it can't."""
    pa = part('peaks')
    rings, segs = 90, 520
    r = 30.0 * (575.0 / 30.0) ** (np.arange(rings + 1) / rings)
    a = np.arange(segs) / segs * math.tau
    rr, aa = np.meshgrid(r, a, indexing='ij')
    x, z = rr * np.cos(aa), rr * np.sin(aa)
    # They rise from a plain well outside the walls and keep to a band of the view.
    t = np.clip((rr - 240.0) / 230.0, 0.0, 1.0)
    rise = t * t * (3.0 - 2.0 * t)
    y = -0.4 + rise * (26.0 + 150.0 * ridged(x * 0.42, z * 0.42, 12) ** 1.3)
    y += np.clip((rr - 45.0) / 140.0, 0.0, 1.0) * 3.0 * ridged(x * 2.1, z * 2.1, 5)
    # Slope, by neighbours round and out.
    da = np.abs(np.roll(y, -1, axis=1) - np.roll(y, 1, axis=1)) / np.maximum(2.0 * rr * math.tau / segs, 1e-3)
    dr = np.abs(np.gradient(y, axis=0)) / np.maximum(np.gradient(rr, axis=0), 1e-3)
    slope = np.maximum(da, dr)
    rock = np.clip((slope - 0.42) / 0.4, 0.0, 1.0)
    # Bare rock shows in streaks down the steep, more of it low down.
    streak = ridged(x * 4.0, z * 4.0, 7)
    rock = np.clip(rock * (0.7 + 1.2 * streak), 0.0, 1.0)
    snow = np.array([0.74, 0.79, 0.88])
    stone = np.array([0.09, 0.1, 0.13])
    col = snow[None, None, :] * (1 - rock[..., None]) + stone[None, None, :] * rock[..., None]
    # Into the air: the far range a step paler and bluer than the near.
    air = np.array([0.42, 0.5, 0.64])
    far = np.clip((rr - 260.0) / 300.0, 0.0, 1.0)[..., None] * 0.55
    col = col * (1 - far) + air[None, None, :] * far
    base = pa.add([(float(x[j, i]), float(y[j, i]), float(z[j, i])) for j in range(rings + 1) for i in range(segs)])
    for j in range(rings + 1):
        for i in range(segs):
            c = col[j, i]
            pa.colors[base + j * segs + i] = (float(c[0]), float(c[1]), float(c[2]))
    for j in range(rings):
        for i in range(segs):
            i1 = (i + 1) % segs
            p, q = base + j * segs + i, base + j * segs + i1
            u, v = base + (j + 1) * segs + i1, base + (j + 1) * segs + i
            pa.faces.append(((p, v, u, q), [(0, 0)] * 4, True))


def install():
    """What stands out on the plain: a radar dish, a gantry for the big shots, old
    hulks, seen over the walls."""
    # The dish.
    x, z = 52.0, 66.0
    cbox(x - 3.0, 0.0, z - 3.0, x + 3.0, 12.0, z + 3.0, 'blast', c=0.1, cell=2.0)
    rod((x, 12.0, z), (x, 17.0, z), 0.7, 'steel', sides=10)
    tilt = math.radians(52.0)
    n, m = 32, 6
    r = 9.0
    rows = []
    for j in range(m + 1):
        row = []
        rr = r * j / m
        cup = (rr / r) ** 2 * 2.6
        for i in range(n + 1):
            a = math.tau * i / n
            px, py, pz = math.cos(a) * rr, cup, math.sin(a) * rr
            # Turned toward the range.
            py2 = py * math.cos(tilt) - pz * math.sin(tilt)
            pz2 = py * math.sin(tilt) + pz * math.cos(tilt)
            ca, sa = math.cos(2.5), math.sin(2.5)
            row.append((x + px * ca - pz2 * sa, 19.0 + py2, z + px * sa + pz2 * ca))
        rows.append(row)
    surface(rows, 'steel', (-0.5, 0.6, -0.5), lambda p: (p[0], p[2]))
    surface([row[::-1] for row in rows], 'steel', (0.5, -0.6, 0.5), lambda p: (p[0], p[2]))
    # The gantry: a lattice tower, its hoist.
    gx, gz = -58.0, 48.0
    h = 46.0
    for sx in (-1, 1):
        for sz in (-1, 1):
            rod((gx + sx * 6.0, 0.0, gz + sz * 6.0), (gx + sx * 1.6, h, gz + sz * 1.6), 0.34, 'steel', sides=6)
    for k in range(0, 9):
        y = k * h / 9.0
        q = 6.0 + (1.6 - 6.0) * k / 9.0
        q2 = 6.0 + (1.6 - 6.0) * (k + 1) / 9.0
        y2 = (k + 1) * h / 9.0
        for (ax, az, bx, bz) in ((-1, -1, 1, -1), (1, -1, 1, 1), (1, 1, -1, 1), (-1, 1, -1, -1)):
            rod((gx + ax * q, y, gz + az * q), (gx + bx * q, y, gz + bz * q), 0.11, 'steel', sides=5)
            rod((gx + ax * q, y, gz + az * q), (gx + bx * q2, y2, gz + bz * q2), 0.08, 'steel', sides=5)
    cbox(gx - 12.0, h, gz - 1.0, gx + 8.0, h + 1.6, gz + 1.0, 'steel', c=0.1, cell=3.0)
    spin(gx - 11.0, gz, [(0.6, h + 1.6), (0.6, h + 1.9), (0.001, h + 1.9)], 'light_red', sides=8, smooth=False)
    rod((gx - 11.0, h, gz), (gx - 11.0, h - 14.0, gz), 0.06, 'steel', sides=5)
    # Bunkers and hulks out on the plain.
    for bx_, bz_, w, d, hh in ((-44.0, -40.0, 9.0, 6.0, 4.0), (38.0, -52.0, 7.0, 7.0, 3.2), (-70.0, -12.0, 6.0, 12.0, 3.6), (68.0, 8.0, 12.0, 5.0, 4.2)):
        cbox(bx_ - w, -0.2, bz_ - d, bx_ + w, hh, bz_ + d, 'blast_dark', c=0.4, cell=3.0)
        snow_cap(bx_ - w + 0.2, hh, bz_ - d + 0.2, bx_ + w - 0.2, bz_ + d - 0.2)


def tone(p, n, name):
    x, y, z = p
    k = 0.88 + 0.12 * math.sin(x * 0.53 + z * 0.37 + y * 0.21) * math.sin(z * 0.47 - x * 0.27 + y * 0.67)
    if abs(n.y) < 0.5 and name == 'blast' and y % 1.2 < 0.08:
        k *= 0.72
    if abs(n.y) < 0.5:
        streak = 0.5 + 0.5 * math.sin((x + z) * 2.9 + math.sin((x - z) * 1.2) * 1.9)
        k *= 1.0 - 0.16 * streak
        # Snow's wet at a wall's foot.
        k *= 1.0 - 0.22 * max(0.0, 1.0 - y / 0.8) if y >= 0 else 0.8
    return (k, k, k)


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    for k in range(4):
        wall_side(k)
    for sx in (-1, 1):
        bunker(sx)
        for sz in (-1, 1):
            gauge(sx * 8.0, sz * 12.0)
    target()
    low_wall(-16.0)
    low_wall(16.0)
    floor()
    flood_masts()
    peaks()
    install()
    objs = make_objects()
    def ground(verts, polys):
        base = len(verts)
        s = 34.0
        verts.extend([to_blender((-s, 0.0, -s)), to_blender((s, 0.0, -s)), to_blender((s, 0.0, s)), to_blender((-s, 0.0, s))])
        polys.append((base, base + 1, base + 2, base + 3))
    paint(objs, ground, tone)
    export(objs, OUT, 'range', bare={'shaft'})


main()

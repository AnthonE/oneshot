"""Cistern's reservoir: everything built, in the map's own coordinates (GLBs at
the origin, `look/built.rs` draws them). Forty-six metres square under brick
groin vaults on thirty-six marble columns: a stone quay round a flooded tank, four
bridges to a ring under the oculus, stairs along the walls, lamps in the niches.

    blender -b --python tools/models/cistern.py

One side is written and turned round, so it's the same from every side. What
collides is `maps/cistern/map.json`'s hidden solids; these numbers follow it.
A mirrored copy of it all stands under the water line for its reflection
(`look/pool.rs`).
"""
import json
import math
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import arch  # noqa: E402
from arch import BACK, cbox, loft, make_objects, mirror_parts, paint, export, part, plank, quad, ring8, rod, spin, strip_light, surface, tbox, torus, tri, to_blender  # noqa: E402,F401

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, '..', '..'))
OUT = os.path.join(ROOT, 'crates', 'client', 'assets', 'models', 'cistern')
MAP = os.path.join(ROOT, 'maps', 'cistern', 'map.json')

arch.TILES.update({'brick': 1.7, 'marble': 3.0, 'stone': 4.0, 'stone_dark': 4.0})

W = 23.0      # the walls' plane
Q = 16.0      # the tank's edge
Y1 = 1.8      # the quay
Y2 = 5.2      # the bridges
SPRING = 7.75 # where the vaults spring
CROWN = 9.85  # their crown
WATER = 0.07  # the water line
GRID = (-17.5, -10.5, -3.5, 3.5, 10.5, 17.5)
BAYS = (-21.0, -14.0, -7.0, 0.0, 7.0, 14.0, 21.0)
SPAN = 3.5
# The columns' cutting: how many hollows, how deep, m.
FLUTES = 18
FLUTE_DEPTH = 0.038
# What a reflection keeps of what it reflects: the sheet's depth and the dark it
# hangs in are in this, not in a light.
DIM = {'stone': 0.13, 'stone_dark': 0.06, 'brick': 0.1, 'marble': 0.2, 'bronze': 0.04}
# The marble's colour over its photograph: the photograph is cream, this is cold.
MARBLE_TONE = (0.52, 0.6, 0.8)
OCULI = ((0.0, 0.0, 1.55, 2.7, 1.0),) + tuple((sx * 7.0, sz * 7.0, 0.72, 1.35, 0.5) for sx in (-1, 1) for sz in (-1, 1))


def turn(k, x, z):
    """(x, z) turned k quarter turns about the middle."""
    for _ in range(k):
        x, z = -z, x
    return x, z


def qbox(k, x0, y0, z0, x1, y1, z1, mat='stone', c=0.04, **kw):
    """A box written for the north side, on side k."""
    a, b = turn(k, x0, z0), turn(k, x1, z1)
    cbox(min(a[0], b[0]), y0, min(a[1], b[1]), max(a[0], b[0]), y1, max(a[1], b[1]), mat, c=c, **kw)


def qquad(k, a, b, c, d, mat, out, cell=arch.CELL):
    """A face written for the north side, on side k."""
    def t(p):
        x, z = turn(k, p[0], p[2])
        return (x, p[1], z)
    ox, oz = turn(k, out[0], out[2])
    quad(t(a), t(b), t(c), t(d), mat, (ox, out[1], oz), cell)


# ---------------------------------------------------------------- the vaults
def web(cx, cz, along_z, sign, n=9):
    """One of a bay's four webs, from its side to the bay's middle: part of a
    barrel, its crease with its neighbours hard."""
    rise = CROWN - SPRING
    rows = []
    for j in range(n + 1):
        s = max(j / n, 0.002)
        row = []
        for i in range(n + 1):
            t = -1.0 + 2.0 * i / n
            a = t * s
            y = SPRING + rise * math.sqrt(max(0.0, 1.0 - a * a))
            if along_z:
                row.append((cx + a * SPAN, y, cz + sign * s * SPAN))
            else:
                row.append((cx + sign * s * SPAN, y, cz + a * SPAN))
        rows.append(row)
    radius = (SPAN + rise) / 2
    if along_z:
        uv = lambda p: (radius * math.asin(max(-1.0, min(1.0, (p[0] - cx) / SPAN))) + cx, p[2])
    else:
        uv = lambda p: (p[0], radius * math.asin(max(-1.0, min(1.0, (p[2] - cz) / SPAN))) + cz)
    surface(rows, 'brick', (0, -1, 0), uv)


def rib(x0, z0, x1, z1):
    """A stone band under the vault along a bay's side, column to column."""
    n = 12
    rise = CROWN - SPRING
    dx, dz = x1 - x0, z1 - z0
    ln = math.hypot(dx, dz)
    nx, nz = -dz / ln * 0.32, dx / ln * 0.32
    rows = []
    for i in range(n + 1):
        a = -1.0 + 2.0 * i / n
        px, pz = (x0 + x1) / 2 + a * dx / 2, (z0 + z1) / 2 + a * dz / 2
        y = SPRING + rise * math.sqrt(max(0.0, 1.0 - a * a))
        rows.append(((px - nx, y + 0.02, pz - nz), (px - nx, y - 0.2, pz - nz), (px + nx, y - 0.2, pz + nz), (px + nx, y + 0.02, pz + nz)))
    for i in range(n):
        a, b = rows[i], rows[i + 1]
        quad(a[0], a[1], b[1], b[0], 'stone', (-nx, 0, -nz), cell=9.0)
        quad(a[1], a[2], b[2], b[1], 'stone', (0, -1, 0), cell=9.0)
        quad(a[2], a[3], b[3], b[2], 'stone', (nx, 0, nz), cell=9.0)


def vaults():
    for cx in BAYS:
        for cz in BAYS:
            for sign in (-1, 1):
                web(cx, cz, True, sign)
                web(cx, cz, False, sign)
    lines = (-24.5,) + GRID + (24.5,)
    for g in GRID:
        for a, b in zip(lines, lines[1:]):
            rib(g, a, g, b)
            rib(a, g, b, g)
    # The oculi: a ring of bronze and stone round a disc of sky, the light itself.
    for cx, cz, r, _, _ in OCULI:
        spin(cx, cz, [(r, CROWN - 0.12), (r + 0.2, CROWN - 0.12), (r + 0.34, CROWN - 0.3), (r + 0.34, CROWN + 0.3)], 'stone_dark', sides=32, smooth=False)
        torus((cx, CROWN - 0.16, cz), r + 0.04, 0.05, (0, 1, 0), 'bronze', segs=40, sides=6)
        spin(cx, cz, [(0.001, CROWN + 0.01), (r, CROWN + 0.01)][::-1], 'light_white', sides=32, smooth=False)


# --------------------------------------------------------------- the columns
def column(x, z, foot):
    """A marble shaft swelling a little and narrowing, on a base inside its
    collider's flare, under a capital and an impost carrying the ribs."""
    top = SPRING - 0.1
    h = top - foot
    f = lambda t: foot + 0.42 + (h - 1.6) * t
    r0 = 0.54
    shaft = [(r0 * (1.0 + 0.02 * math.sin(t * math.pi) - 0.09 * t), f(t)) for t in (0.0, 0.2, 0.45, 0.7, 1.0)]
    spin(x, z, [(0.82, foot), (0.82, foot + 0.1), (0.74, foot + 0.16), (0.74, foot + 0.24), (0.62, foot + 0.32), (shaft[0][0], foot + 0.42)], 'stone', sides=24)
    fluted(x, z, shaft)
    y = shaft[-1][1]
    r = shaft[-1][0]
    # The capital: a ring, a bell with its leaves' lobes, the abacus turned to the ribs.
    spin(x, z, [(r, y), (r + 0.05, y + 0.04), (r + 0.05, y + 0.1), (r, y + 0.14)], 'stone_dark', sides=24)
    y += 0.14
    bell = 0.78
    for lobe in range(12):
        a = math.tau * lobe / 12
        ca, sa = math.cos(a), math.sin(a)
        w = math.tau / 12 * 0.5
        rows = []
        for j in range(4):
            t = j / 3
            rr = r + (0.82 - r) * t * t
            lean = 0.06 * math.sin(t * math.pi)
            row = []
            for s in (-1.0, -0.5, 0.0, 0.5, 1.0):
                bulge = 1.0 - 0.1 * s * s
                aa = a + s * w
                row.append((x + math.cos(aa) * (rr + lean) * bulge, y + bell * t, z + math.sin(aa) * (rr + lean) * bulge))
            rows.append(row)
        surface(rows, 'stone', (ca, 0.1, sa), lambda p: (p[0] + p[2], p[1]))
    y += bell
    # The abacus and the impost that takes the ribs' feet.
    loft([ring8(0.94, 0.94, 0.16, z, y, cx=x), ring8(1.0, 1.0, 0.16, z, y + 0.1, cx=x), ring8(1.0, 1.0, 0.16, z, y + 0.2, cx=x), ring8(0.8, 0.8, 0.1, z, y + 0.3, cx=x)], 'stone_dark', cap=True, cell=3.0)


def fluted(x, z, shaft):
    """A shaft cut in FLUTES hollows with a flat between each, inside its radius,
    each hollow running out at its foot and its head."""
    pa = part('marble')
    tile = arch.TILES['marble']
    per = 4
    sides = FLUTES * per
    rows = []
    for j in range(len(shaft) - 1):
        n = 3 if j in (0, len(shaft) - 2) else 1
        for k in range(n):
            t = k / n
            rows.append((shaft[j][0] + (shaft[j + 1][0] - shaft[j][0]) * t, shaft[j][1] + (shaft[j + 1][1] - shaft[j][1]) * t))
    rows.append(shaft[-1])
    y0, y1 = shaft[0][1], shaft[-1][1]
    base = len(pa.verts)
    for r, y in rows:
        run = min(1.0, min(y - y0, y1 - y) / 0.34)
        for i in range(sides):
            a = math.tau * i / sides
            cut = {0: 0.0, 1: 0.65, 2: 1.0, 3: 0.65}[i % per] * FLUTE_DEPTH * run
            pa.add([(x + (r - cut) * math.cos(a), y, z + (r - cut) * math.sin(a))])
    for j in range(len(rows) - 1):
        for i in range(sides):
            i1 = (i + 1) % sides
            p, q = base + j * sides + i, base + j * sides + i1
            s, t = base + (j + 1) * sides + i1, base + (j + 1) * sides + i
            u0, u1 = i * 0.11 / tile * 1.0, (i + 1) * 0.11 / tile
            v0, v1 = rows[j][1] / tile, rows[j + 1][1] / tile
            pa.faces.append(((p, t, s, q), [(u0, v0), (u0, v1), (u1, v1), (u1, v0)], True))


def columns():
    for x in GRID:
        for z in GRID:
            column(x, z, Y1 if max(abs(x), abs(z)) > Q else 0.0)


# ----------------------------------------------------------------- the walls
def wall_side(k, lamps):
    """The north wall, turned: a niche in each bay between pilasters, a stone
    dado, brick over. Its plane is the sim's; the niches go back from it."""
    z = W
    back = W + 0.55
    edges = (-W, ) + GRID + (W,)
    for a, b in zip(edges, edges[1:]):
        full = a in GRID and b in GRID
        pier = 0.72
        u0, u1 = a + (pier if a in GRID else 0.2), b - (pier if b in GRID else 0.2)
        mid, half = (u0 + u1) / 2, (u1 - u0) / 2
        sill, spring = 2.9, 4.3
        crown = spring + half * 0.86
        # The brick over and round it, strips down to the arch's curve.
        strips = 14
        xs = [u0 + (u1 - u0) * i / strips for i in range(strips + 1)]
        ceil = 10.0
        def arch_y(u):
            t = (u - mid) / half
            return spring + (crown - spring) * math.sqrt(max(0.0, 1.0 - t * t))
        for i in range(strips):
            x0, x1 = xs[i], xs[i + 1]
            qquad(k, (x0, arch_y(x0), z), (x1, arch_y(x1), z), (x1, ceil, z), (x0, ceil, z), 'brick', (0, 0, -1), cell=2.0)
            # The arch's soffit, back to the niche, in stone.
            qquad(k, (x0, arch_y(x0), z), (x1, arch_y(x1), z), (x1, arch_y(x1), back), (x0, arch_y(x0), back), 'stone', (0, -1, 0), cell=2.0)
            # Its dark frame on the wall's face.
            f = 0.24
            qquad(k, (x0, arch_y(x0), z - 0.012), (x1, arch_y(x1), z - 0.012), (x1, arch_y(x1) + f, z - 0.012), (x0, arch_y(x0) + f, z - 0.012), 'stone', (0, 0, -1), cell=2.0)
        # The jambs, the niche's back and its sill.
        for u, s in ((u0, 1), (u1, -1)):
            qquad(k, (u, sill, z), (u, spring, z), (u, spring, back), (u, sill, back), 'stone_dark', (s, 0, 0), cell=2.0)
        qquad(k, (u0, Y1 - 0.2, back), (u1, Y1 - 0.2, back), (u1, ceil, back), (u0, ceil, back), 'brick', (0, 0, -1), cell=1.4)
        qquad(k, (u0, sill, z), (u1, sill, z), (u1, sill, back), (u0, sill, back), 'stone', (0, 1, 0), cell=2.0)
        # The dado under it, to the plane.
        qbox(k, u0, Y1, z, u1, sill, z + 0.3, 'stone', c=0.03)
        qbox(k, u0, Y1, z - 0.0, u1, Y1 + 0.26, z + 0.05, 'stone_dark', c=0.0, cell=3.0)
        # The pilasters between bays, full height.
        if a in GRID:
            qbox(k, a - 0.72, Y1, z, a + 0.72, ceil, z + 0.6, 'brick', c=0.03)
            qbox(k, a - 0.4, Y1, z - 0.0, a + 0.4, SPRING, z + 0.05, 'stone', c=0.02)
    # End strips by the corners.
    qbox(k, -W, Y1, z, -W + 0.2, 10.0, z + 0.6, 'brick', c=0.0)
    qbox(k, W - 0.2, Y1, z, W, 10.0, z + 0.6, 'brick', c=0.0)


def lantern(x, z, wall_k, mat):
    """A lamp on a bracket off the wall, at the height map.json's lamp is."""
    y = 4.2
    wx, wz = turn(wall_k, 0.0, W)
    # toward the wall
    dx, dz = (1.0 if wx > 0 else -1.0 if wx < 0 else 0.0), (1.0 if wz > 0 else -1.0 if wz < 0 else 0.0)
    rod((x, y + 0.7, z), (x + dx * 1.1, y + 0.7, z + dz * 1.1), 0.035, 'bronze', sides=6)
    rod((x + dx * 0.55, y + 0.7, z + dz * 0.55), (x + dx * 1.1, y + 0.2, z + dz * 1.1), 0.025, 'bronze', sides=6)
    rod((x, y + 0.7, z), (x, y + 0.2, z), 0.02, 'bronze', sides=6)
    spin(x, z, [(0.02, y - 0.22), (0.2, y - 0.1), (0.24, y), (0.2, y + 0.12), (0.12, y + 0.2)], 'bronze', sides=12, smooth=True)
    spin(x, z, [(0.17, y - 0.08), (0.2, y + 0.0), (0.16, y + 0.1), (0.001, y + 0.14)], mat, sides=12, smooth=True)


# ------------------------------------------------------- the quay and the stairs
def steps_on(k, x0, x1, z0, z1, y0, y1, along_x, up_positive, mat='stone', tread=0.42):
    """A flight of stone steps whose noses lie on the sim's ramp, so a foot never
    goes through one, written for the north side."""
    run = (x1 - x0) if along_x else (z1 - z0)
    n = max(2, int(run / tread))
    for i in range(n):
        t0, t1 = i / n, (i + 1) / n
        if not up_positive:
            t0, t1 = 1 - t1, 1 - t0
        # A step's top stays at or under the ramp's plane at its low nose.
        h = t0 if up_positive else t1
        top = y0 + (y1 - y0) * (i + 1) / n - (y1 - y0) / n
        if along_x:
            a, b = x0 + run * t0, x0 + run * t1
            qbox(k, a, y0 - 0.5, z0, b, max(top, y0 + 0.001), z1, mat, c=0.03)
        else:
            a, b = z0 + run * t0, z0 + run * t1
            qbox(k, x0, y0 - 0.5, a, x1, max(top, y0 + 0.001), b, mat, c=0.03)


def quay_side(k):
    # The paving and its edge; the retaining wall to the water, stained below.
    qbox(k, -Q, 1.3, Q, Q, Y1, W, 'stone', c=0.0, cell=1.4)
    # Its face toward the tank, stone in courses with an arched drain in each bay.
    a0 = -Q
    edges = (-Q, -10.5, -3.5, 3.5, 10.5, Q)
    for a, b in zip(edges, edges[1:]):
        if a == -3.5:
            continue  # the steps are here
        qbox(k, a, -0.1, Q, b, Y1 - 0.26, Q + 0.5, 'stone', c=0.02, cell=1.0)
        qbox(k, a, Y1 - 0.26, Q, b, Y1, Q + 0.5, 'stone_dark', c=0.03, cell=1.4)
        if b - a > 5:
            m = (a + b) / 2
            spin_x, spin_z = turn(k, m, Q - 0.012)
    # Steps down to the water, with a dark string each side.
    steps_on(k, -2.5, 2.5, Q - 3.4, Q, 0.0, Y1, False, True)
    for s in (-1, 1):
        lo, hi = sorted((s * 2.5, s * 3.5))
        qbox(k, lo, -0.1, Q - 0.1, hi, Y1, Q + 0.5, 'stone', c=0.03)
    # Corner: the paving's other half at each end comes from the next side round.
    # The stairs along the wall, quay to bridge, on a brick wedge.
    for s in (-1, 1):
        x0, x1 = sorted((s * 3.5, s * 10.5))
        steps_on(k, x0, x1, 21.0, 23.0, Y1, Y2, True, s < 0, tread=0.46)
        # Its inner cheek in brick with a stone coping following the rise.
        low, high = (x0, x1) if s < 0 else (x1, x0)
        qquad(k, (x0, Y1, 21.0), (x1, Y1, 21.0), (x1, Y2 if s > 0 else Y1, 21.0), (x0, Y2 if s < 0 else Y1, 21.0), 'brick', (0, 0, -1), cell=1.0)
        qquad(k, (low, Y1, 21.0), (high, Y2 - 0.0, 21.0), (high, Y2 + 0.16, 20.95), (low, Y1 + 0.16, 20.95), 'stone_dark', (0, 0.3, -1), cell=3.0)


def low_walls():
    """The cover, a quarter of it, in the map's four images."""
    for sx in (-1, 1):
        for sz in (-1, 1):
            def bx(x0, y0, z0, x1, y1, z1, mat='stone', c=0.05):
                a, b = sorted((x0 * sx, x1 * sx)), sorted((z0 * sz, z1 * sz))
                cbox(a[0], y0, b[0], a[1], y1, b[1], mat, c=c)
            for x0, z0, x1, z1 in ((-13.6, -7.3, -8.2, -6.7), (-7.3, -13.6, -6.7, -8.2)):
                bx(x0, -0.1, z0, x1, 1.0, z1, 'stone', c=0.03)
                bx(x0, 1.0, z0, x1, 1.25, z1, 'stone_dark', c=0.06)
            for x0, z0, x1, z1 in ((-20.2, -18.6, -18.6, -18.0), (-18.6, -20.2, -18.0, -18.6)):
                bx(x0, Y1, z0, x1, Y1 + 1.0, z1, 'stone', c=0.03)
                bx(x0, Y1 + 1.0, z0, x1, Y1 + 1.2, z1, 'stone_dark', c=0.06)
            # The fallen drum, inside its turned box, a section of a shaft lying
            # half in the water.
            cx, cz = -5.0 * sx, -14.3 * sz
            a = math.atan2(math.sin(math.radians(22)) * sz, math.cos(math.radians(22)) * sx)
            drum(cx, cz, a)


def drum(cx, cz, angle):
    ux, uz = math.cos(angle), math.sin(angle)
    r = 0.54
    sides = 18
    for s in (-1, 0, 1):
        pass
    rows = []
    n = 6
    for i in range(n + 1):
        t = -1.45 + 2.9 * i / n
        row = []
        for j in range(sides + 1):
            a = math.tau * j / sides
            ny, nv = math.sin(a), math.cos(a)
            bump = 1.0 + 0.02 * math.sin(i * 2.1 + j)
            row.append((cx + ux * t - uz * nv * r * bump, 0.55 + ny * r * bump, cz + uz * t + ux * nv * r * bump))
        rows.append(row)
    pa = part('marble')
    tile = arch.TILES['marble']
    base = pa.add([p for row in rows for p in row])
    for i in range(n):
        for j in range(sides):
            a, b, c, d = base + i * (sides + 1) + j, base + i * (sides + 1) + j + 1, base + (i + 1) * (sides + 1) + j + 1, base + (i + 1) * (sides + 1) + j
            uv = [(q0 / tile, q1 / tile) for q0, q1 in ((i * 0.5, j * 0.19), (i * 0.5, (j + 1) * 0.19), ((i + 1) * 0.5, (j + 1) * 0.19), ((i + 1) * 0.5, j * 0.19))]
            pa.faces.append(((a, b, c, d), uv, True))
    for end, t in ((0, -1.45), (n, 1.45)):
        c = (cx + ux * t, 0.55, cz + uz * t)
        for j in range(sides):
            tri(rows[end][j], rows[end][j + 1], c, 'stone', (ux * t, 0, uz * t))


# ------------------------------------------------------- the bridges and the ring
def bridge(k):
    # The landing on the wall, its soffit dark.
    qbox(k, -3.5, Y2 - 0.5, 19.5, 3.5, Y2, W, 'stone', c=0.03, cell=1.4)
    # The arm: a slab, its top the sim's, on two beams between the columns.
    qbox(k, -1.3, Y2 - 0.5, 5.8, 1.3, Y2, 19.5, 'stone', c=0.03, cell=1.4)
    for z in (10.5, 17.5):
        qbox(k, -3.0, 4.2, z - 0.3, 3.0, 4.7, z + 0.3, 'stone_dark', c=0.05)
    # Its soffit arched a little between them, in brick.
    for z0, z1 in ((10.8, 17.2), (6.0, 10.2)):
        n = 8
        rows = []
        for j in range(n + 1):
            t = -1.0 + 2.0 * j / n
            y = Y2 - 0.5 - 0.42 * (t * t)
            z = (z0 + z1) / 2 + t * (z1 - z0) / 2
            row = [(-1.28, y, z), (1.28, y, z)]
            rows.append(row)
        for a, b in zip(rows, rows[1:]):
            qquad(k, a[0], a[1], b[1], b[0], 'brick', (0, -1, 0), cell=2.0)
        for s in (-1, 1):
            for a, b in zip(rows, rows[1:]):
                qquad(k, (s * 1.3, Y2 - 0.5, a[0][2]), (s * 1.3, Y2 - 0.5, b[0][2]), (s * 1.3, b[0][1], b[0][2]), (s * 1.3, a[0][1], a[0][2]), 'stone', (s, 0, 0), cell=2.0)
    # The parapets, a gap over the water, a bronze rail on each.
    for s in (-1, 1):
        lo, hi = sorted((s * 1.08, s * 1.3))
        for z0, z1 in ((5.9, 9.0), (13.2, 19.5)):
            qbox(k, lo, Y2, z0, hi, Y2 + 0.72, z1, 'stone', c=0.03)
            qbox(k, lo + 0.03, Y2 + 0.72, z0, hi - 0.03, Y2 + 0.85, z1, 'bronze', c=0.02, cell=4.0)
    # A strip of light along the landing's edge, each side's colour on its own.
    mat = {0: 'light_amber', 2: 'light_amber', 1: 'light_red', 3: 'light_blue'}[k]
    qbox(k, -3.45, Y2 - 0.28, 19.47, 3.45, Y2 - 0.22, 19.5, mat, c=0.0, cell=30.0)


def ring():
    spin(0, 0, [(2.7, Y2 - 0.5), (5.2, Y2 - 0.5), (5.7, Y2 - 0.3), (6.0, Y2 - 0.3), (6.0, Y2), (2.7, Y2), (2.7, Y2 - 0.5)], 'stone', sides=48, smooth=False)
    # Under it a dark moulding on the columns' side; on it a bronze ring let in.
    torus((0, Y2 + 0.004, 0), 4.0, 0.012, (0, 1, 0), 'bronze', segs=64, sides=4)
    # The inner lip, four runs, the gaps at the diagonals to drop through.
    for a0, a1 in ((-30, 30), (60, 120), (150, 210), (240, 300)):
        arc = (math.radians(a0), math.radians(a1))
        spin(0, 0, [(2.7, Y2 + 0.01), (2.9, Y2 + 0.01), (2.9, Y2 + 0.72), (2.7, Y2 + 0.72), (2.7, Y2 + 0.01)], 'stone', sides=14, smooth=False, arc=arc)
        spin(0, 0, [(2.72, Y2 + 0.72), (2.88, Y2 + 0.72), (2.88, Y2 + 0.85), (2.72, Y2 + 0.85), (2.72, Y2 + 0.72)], 'bronze', sides=14, smooth=False, arc=arc)
    # The light in its hole, cool, under the rim.
    torus((0, Y2 - 0.42, 0), 2.74, 0.02, (0, 1, 0), 'light_white', segs=64, sides=4)


def dais():
    spin(0, 0, [(3.4, -0.1), (3.4, 0.5), (2.2, 0.5), (2.2, 1.0), (0.001, 1.0)], 'stone', sides=48, smooth=False)
    # Let into its top, a bronze ring and the eight lines of the winds.
    torus((0, 1.003, 0), 1.7, 0.012, (0, 1, 0), 'bronze', segs=48, sides=4)
    for i in range(8):
        a = math.tau * i / 8
        w = 0.014
        c, s = math.cos(a), math.sin(a)
        p = lambda r, o: (c * r - s * o, 1.006, s * r + c * o)
        quad(p(0.5, -w), p(0.5, w), p(1.7, w), p(1.7, -w), 'bronze', (0, 1, 0), cell=9.0)
    torus((0, 0.504, 0), 2.8, 0.012, (0, 1, 0), 'bronze', segs=48, sides=4)


def trim():
    """Each side's colour along the quay's edge, at the coping's foot."""
    for k, mat in ((1, 'light_red'), (3, 'light_blue'), (0, 'light_amber'), (2, 'light_amber')):
        for a, b in ((-15.8, -3.7), (3.7, 15.8)):
            qbox(k, a, Y1 - 0.3, Q - 0.012, b, Y1 - 0.26, Q, mat, c=0.0, cell=30.0)


def shafts():
    """The light down from the oculi: crossed sheets added over the frame, soft at
    their edges and fading to the water; their colour is their brightness."""
    pa = part('shaft')
    for cx, cz, r0, r1, k in OCULI:
        planes = 6
        for p in range(planes):
            a = math.pi * p / planes
            ux, uz = math.cos(a), math.sin(a)
            rows = 7
            cols = 6
            base = pa.add([
                (cx + ux * (r0 + (r1 - r0) * (j / rows)) * (-1 + 2 * i / cols), CROWN - (CROWN - WATER) * j / rows, cz + uz * (r0 + (r1 - r0) * (j / rows)) * (-1 + 2 * i / cols))
                for j in range(rows + 1) for i in range(cols + 1)
            ])
            for j in range(rows + 1):
                for i in range(cols + 1):
                    edge = 1.0 - abs(-1 + 2 * i / cols)
                    fall = (1.0 - j / rows * 0.82)
                    v = k * 0.075 * edge * edge * fall
                    pa.colors[base + j * (cols + 1) + i] = (v, v, v)
            for j in range(rows):
                for i in range(cols):
                    q = base + j * (cols + 1) + i
                    pa.faces.append(((q, q + 1, q + cols + 2, q + cols + 1), [(0, 0)] * 4, True))


def water():
    n = 12
    rows = [[(-Q + 2 * Q * i / n, WATER, -Q + 2 * Q * j / n) for i in range(n + 1)] for j in range(n + 1)]
    surface(rows, 'water', (0, 1, 0), lambda p: (p[0], p[2]), smooth=True)


def tone(p, n, name):
    """Over the occlusion: wet and stained low in the tank, damp under the vault's
    feet, a slow mottle over everything so no wall is one value."""
    x, y, z = p
    mottle = 0.9 + 0.1 * math.sin(x * 0.61 + z * 0.37) * math.sin(z * 0.53 - y * 0.91 + x * 0.17)
    r, g, b = mottle, mottle, mottle
    if max(abs(x), abs(z)) <= Q + 0.6:
        wet = max(0.0, min(1.0, 1.0 - (y - WATER) / 1.1))
        k = 1.0 - 0.62 * wet
        r, g, b = r * k * (1 - 0.1 * wet), g * k, b * k * (1 - 0.14 * wet)
    # Streaks from the vault's feet down the walls.
    if abs(n.z) > 0.8 or abs(n.y) > 0.8 and y > SPRING:
        streak = 0.5 + 0.5 * math.sin((x + z) * 2.3 + math.sin((x - z) * 0.7) * 2.0)
        r, g, b = r * (1 - 0.1 * streak), g * (1 - 0.1 * streak), b * (1 - 0.1 * streak)
    if name == 'marble':
        r, g, b = r * MARBLE_TONE[0], g * MARBLE_TONE[1], b * MARBLE_TONE[2]
    return (r, g, b)


def build_all(lamps):
    vaults()
    columns()
    for k in range(4):
        wall_side(k, lamps)
        quay_side(k)
        bridge(k)
    for at, y, mat, k in lamps:
        lantern(at[0], at[1], k, mat)
    ring()
    dais()
    low_walls()
    trim()
    shafts()
    water()


def lamps_of():
    with open(MAP) as f:
        spec = json.load(f)
    out = []
    for l in spec['light']['lamps']:
        x, z, y = l['at']
        if y >= 13:
            continue
        mat = 'light_red' if l['color'][1] < 0.5 else 'light_amber'
        pts = [(x * sx, z * sz) for sx in (-1, 1) for sz in (-1, 1)] if l.get('mirror') else [(x, z)]
        for px, pz in pts:
            # Which wall it hangs off: the nearer. Team colours by side.
            if abs(px) > abs(pz):
                k = 1 if px < 0 else 3
                m = 'light_red' if px < 0 and mat == 'light_red' else ('light_blue' if px > 0 and mat == 'light_red' else 'light_amber')
            else:
                k = 0 if pz > 0 else 2
                m = 'light_amber'
            out.append(((px, pz), y, m, k))
    return out


def seen_in_water(p):
    """What the water can show: out past the quay only the vault and the walls'
    upper parts are in it, and the quay's own paving never is."""
    x, y, z = p
    out = max(abs(x), abs(z))
    if out > W + 0.7:
        return False
    return out <= Q + 1.5 or y > Y2 - 0.6


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    build_all(lamps_of())
    objs = make_objects()
    def ground(verts, polys):
        # The water's surface hides what's under it from the occlusion's rays.
        base = len(verts)
        s = 30.0
        verts.extend([to_blender((-s, WATER, -s)), to_blender((s, WATER, -s)), to_blender((s, WATER, s)), to_blender((-s, WATER, s))])
        polys.append((base, base + 1, base + 2, base + 3))
    paint(objs, ground, tone)
    refl = mirror_parts(objs, {
        'reflect_stone': ['stone', 'stone_dark'],
        'reflect_brick': ['brick'],
        'reflect_marble': ['marble'],
        'reflect_light': ['light_white', 'light_amber', 'light_red', 'light_blue', 'bronze'],
    }, WATER, keep=seen_in_water, dim=DIM)
    objs.update(refl)
    export(objs, OUT, 'cistern', bare=set(refl))


main()

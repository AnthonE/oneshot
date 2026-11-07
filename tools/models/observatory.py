"""Cloudbreak's observatory: every built thing on the island, in the map's own
coordinates (GLBs at the origin, `look/observatory.rs` draws them). An ancient
sky-watchers' station on a floating rock: a tapering tower with a crown of
rings, a sundial plaza under a broken meridian arch, trilithon gates, bastions.

    blender -b --python tools/models/observatory.py

**Drawn inside what collides, where anyone can go.** Each structure is the box
the sim collides (`maps/cloudbreak/map.json`, look `hidden`), its body set back
BACK m from that plane and its frames, pilasters and string courses standing
out to it, so the panels are real and nothing proud of a face is outside it.
Above where anyone reaches the tower is free to taper inside its boxes, and its
cornices and rings may stand out.

One GLB a material: `stone`, `stone_dark`, `bronze`, `crown` (the rings, turned
slowly by the client round CROWN_AT), `light_white`, `light_red`, `light_blue`.
No images: the client wears its own `stone` set on them with the
level's weathering, UVs in tiles of TILE_M. COLOR_0 is painted here, ambient
occlusion traced against the whole island and sand dust up from the ground.

Numbers for the game go in `look/observatory.rs` and `maps/cloudbreak/map.json`.
"""
import math
import os
import random
import sys

import bpy
from mathutils import Vector
from mathutils.bvhtree import BVHTree

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, '..', '..'))
OUT = os.path.join(ROOT, 'crates', 'client', 'assets', 'models', 'isle')
SHEETS = os.path.join(ROOT, 'traces', 'rocks')

# The `stone` set's tile, m (`level::dress`'s STONE).
TILE_M = 4.0
# How far a body sits behind the plane the sim collides, m: what its frames
# stand out of.
BACK = 0.14
# The longest side of a tessellated cell, m: AO and dust are per vertex.
CELL = 0.7
# AO: rays a vertex, how far they look, how dark full occlusion is.
AO_RAYS = 32
AO_REACH = 5.0
AO_FLOOR = 0.22
# Sand dust: how far up from the ground it reaches, m, and its tint.
DUST_M = 1.6
DUST_RGB = (1.12, 0.98, 0.78)
# Where the crown's rings turn about (x, y, z), `look/observatory.rs`.
CROWN_AT = (0.0, 33.0, 21.0)


class Part:
    """A mesh in the making: game coordinates (y up, right-handed)."""

    def __init__(self):
        self.verts = []
        self.faces = []  # (vertex indices, uvs, smooth)

    def add(self, pts):
        i = len(self.verts)
        self.verts.extend(pts)
        return i


PARTS = {}


def part(mat):
    return PARTS.setdefault(mat, Part())


def v3(p):
    return Vector((p[0], p[1], p[2]))


def box_uv(p, n):
    """Metres along the face, in tiles: courses stay level on a wall."""
    a = (abs(n.x), abs(n.y), abs(n.z))
    if a[1] >= a[0] and a[1] >= a[2]:
        return (p[0] / TILE_M, p[2] / TILE_M)
    if a[0] >= a[2]:
        return (p[2] / TILE_M, p[1] / TILE_M)
    return (p[0] / TILE_M, p[1] / TILE_M)


def quad(a, b, c, d, mat, out, cell=CELL):
    """A flat four-sided face, turned to face `out`, cut into cells."""
    a, b, c, d = v3(a), v3(b), v3(c), v3(d)
    n = (b - a).cross(d - a)
    if n.length < 1e-9:
        n = (c - b).cross(a - b)
    if n.length < 1e-9:
        return
    if n.dot(Vector(out)) < 0:
        a, b, c, d = b, a, d, c
        n = -n
    n.normalize()
    nu = max(1, int(math.ceil(max((b - a).length, (c - d).length) / cell)))
    nv = max(1, int(math.ceil(max((d - a).length, (c - b).length) / cell)))
    pa = part(mat)
    pts = []
    for j in range(nv + 1):
        t = j / nv
        for i in range(nu + 1):
            s = i / nu
            pts.append(tuple(a.lerp(b, s).lerp(d.lerp(c, s), t)))
    base = pa.add(pts)
    for j in range(nv):
        for i in range(nu):
            k = base + j * (nu + 1) + i
            idx = (k, k + 1, k + nu + 2, k + nu + 1)
            pa.faces.append((idx, [box_uv(pa.verts[q], n) for q in idx], False))


def tri(a, b, c, mat, out):
    a, b, c = v3(a), v3(b), v3(c)
    n = (b - a).cross(c - a)
    if n.length < 1e-9:
        return
    if n.dot(Vector(out)) < 0:
        b, c = c, b
        n = -n
    n.normalize()
    pa = part(mat)
    base = pa.add([tuple(a), tuple(b), tuple(c)])
    idx = (base, base + 1, base + 2)
    pa.faces.append((idx, [box_uv(pa.verts[q], n) for q in idx], False))


def cbox(x0, y0, z0, x1, y1, z1, mat='stone', c=0.05, top=True, bottom=False, cell=CELL):
    """A box with its upright and top edges cut back by `c`, inside its bounds."""
    c = min(c, (x1 - x0) * 0.45, (z1 - z0) * 0.45, (y1 - y0) * 0.45)
    mid = ((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2)
    plan = [(x0 + c, z0), (x1 - c, z0), (x1, z0 + c), (x1, z1 - c), (x1 - c, z1), (x0 + c, z1), (x0, z1 - c), (x0, z0 + c)]
    inner = [(x0 + c, z0 + c), (x1 - c, z0 + c), (x1 - c, z0 + c), (x1 - c, z1 - c),
             (x1 - c, z1 - c), (x0 + c, z1 - c), (x0 + c, z1 - c), (x0 + c, z0 + c)]
    yt = y1 - c
    yb = y0 + (c if bottom else 0.0)
    for i in range(8):
        p, q = plan[i], plan[(i + 1) % 8]
        fm = ((p[0] + q[0]) / 2 - mid[0], 0.0, (p[1] + q[1]) / 2 - mid[2])
        quad((p[0], yb, p[1]), (q[0], yb, q[1]), (q[0], yt, q[1]), (p[0], yt, p[1]), mat, fm, cell)
        if top:
            r, s = inner[i], inner[(i + 1) % 8]
            o = (fm[0], 1.0, fm[2])
            if abs(r[0] - s[0]) + abs(r[1] - s[1]) < 1e-6:
                tri((p[0], yt, p[1]), (q[0], yt, q[1]), (r[0], y1, r[1]), mat, o)
            else:
                quad((p[0], yt, p[1]), (q[0], yt, q[1]), (s[0], y1, s[1]), (r[0], y1, r[1]), mat, o, cell)
    if top:
        quad((x0 + c, y1, z0 + c), (x1 - c, y1, z0 + c), (x1 - c, y1, z1 - c), (x0 + c, y1, z1 - c), mat, (0, 1, 0), cell)
    if bottom or y0 > 0.05:
        # Anything off the ground is closed under: looked up at, an open box
        # shows what stands on its top through it.
        for i in range(8):
            p, q = plan[i], plan[(i + 1) % 8]
            tri((p[0], y0, p[1]), (q[0], y0, q[1]), (mid[0], y0, mid[2]), mat, (0, -1, 0))


def ring8(hx, hz, ch, cz, y, cx=0.0):
    """An octagon's points at height y: a box's plan with its corners cut by ch."""
    ch = min(ch, hx * 0.95, hz * 0.95)
    return [(cx - hx + ch, y, cz - hz), (cx + hx - ch, y, cz - hz), (cx + hx, y, cz - hz + ch), (cx + hx, y, cz + hz - ch),
            (cx + hx - ch, y, cz + hz), (cx - hx + ch, y, cz + hz), (cx - hx, y, cz + hz - ch), (cx - hx, y, cz - hz + ch)]


def loft(rings, mat='stone', cap=True, cell=CELL):
    """Rings of one count stacked up: flat faces between, the last capped."""
    n = len(rings[0])
    for a, b in zip(rings, rings[1:]):
        cx = sum(p[0] for p in a) / n
        cz = sum(p[2] for p in a) / n
        for i in range(n):
            p, q = a[i], a[(i + 1) % n]
            r, s = b[(i + 1) % n], b[i]
            fm = ((p[0] + q[0]) / 2 - cx, b[0][1] - a[0][1] and (sum(x[1] for x in (p, q)) - sum(x[1] for x in (r, s))) * -0.0, (p[2] + q[2]) / 2 - cz)
            # An eave's underside looks down, a step's top up.
            da = math.hypot((p[0] + q[0]) / 2 - cx, (p[2] + q[2]) / 2 - cz)
            db = math.hypot((r[0] + s[0]) / 2 - cx, (r[2] + s[2]) / 2 - cz)
            rise = (s[1] - p[1])
            up = 0.0
            if abs(rise) < 1e-6:
                up = 1.0 if db < da else -1.0
            else:
                up = (da - db) / abs(rise)
            quad(p, q, r, s, mat, (fm[0], up, fm[2]), cell)
    if cap:
        top = rings[-1]
        cx = sum(p[0] for p in top) / n
        cz = sum(p[2] for p in top) / n
        cy = top[0][1]
        for i in range(n):
            tri(top[i], top[(i + 1) % n], (cx, cy, cz), mat, (0, 1, 0))


def torus(centre, major, minor, axis, mat, segs=72, sides=10, arc=None):
    """A ring about `axis`, smooth, whole or from arc[0] to arc[1] radians."""
    ax = Vector(axis).normalized()
    u = ax.orthogonal().normalized()
    w = ax.cross(u).normalized()
    c = Vector(centre)
    pa = part(mat)
    a0, a1 = arc if arc else (0.0, math.tau)
    whole = arc is None
    rows = segs if whole else segs + 1
    pts = []
    for i in range(rows):
        t = a0 + (a1 - a0) * i / segs
        rad = u * math.cos(t) + w * math.sin(t)
        for j in range(sides):
            s = math.tau * j / sides
            pts.append(tuple(c + rad * (major + minor * math.cos(s)) + ax * (minor * math.sin(s))))
    base = pa.add(pts)
    n_rows = segs if whole else segs
    for i in range(n_rows):
        i1 = (i + 1) % rows
        for j in range(sides):
            j1 = (j + 1) % sides
            idx = (base + i * sides + j, base + i1 * sides + j, base + i1 * sides + j1, base + i * sides + j1)
            uv = [(i / 8, j / sides)] * 4
            pa.faces.append((idx, uv, True))


def rod(a, b, r, mat, sides=8, taper=1.0):
    """A straight round bar from a to b, its far end `taper` of its near."""
    a, b = Vector(a), Vector(b)
    d = (b - a).normalized()
    u = d.orthogonal().normalized()
    w = d.cross(u)
    pa = part(mat)
    pts = []
    for end, k in ((a, 1.0), (b, taper)):
        for j in range(sides):
            s = math.tau * j / sides
            pts.append(tuple(end + (u * math.cos(s) + w * math.sin(s)) * (r * k)))
    base = pa.add(pts)
    for j in range(sides):
        j1 = (j + 1) % sides
        idx = (base + j, base + j1, base + sides + j1, base + sides + j)
        # Faces out of the bar: the sides' winding is (u, w, d).
        pa.faces.append((idx, [(0, 0)] * 4, True))


def wedge(x0, z0, x1, z1, up, y_lo, y_hi, base, mat='stone'):
    """A ramp the sim's `ramp` solid walks: its top the same plane, to the bit."""
    def h(x, z):
        t = {'+x': (x - x0) / (x1 - x0), '-x': (x1 - x) / (x1 - x0), '+z': (z - z0) / (z1 - z0), '-z': (z1 - z) / (z1 - z0)}[up]
        return y_lo + (y_hi - y_lo) * t
    corners = [(x0, z0), (x1, z0), (x1, z1), (x0, z1)]
    tops = [(x, h(x, z), z) for x, z in corners]
    bots = [(x, base, z) for x, z in corners]
    quad(tops[0], tops[1], tops[2], tops[3], mat, (0, 1, 0))
    mx, mz = (x0 + x1) / 2, (z0 + z1) / 2
    for i in range(4):
        p, q = i, (i + 1) % 4
        o = ((corners[p][0] + corners[q][0]) / 2 - mx, 0, (corners[p][1] + corners[q][1]) / 2 - mz)
        quad(bots[p], bots[q], tops[q], tops[p], mat, o)


def strip_light(x0, y0, z0, x1, y1, z1, mat):
    """A thin bar of light (never collided; a hand proud at most)."""
    cbox(x0, y0, z0, x1, y1, z1, mat, c=0.0, top=True, cell=20.0)


def slit(x, y0, y1, z, w=0.26, face=-1, axis='z', depth=0.02, light=True):
    """A sighting slit on a face: dark stone with a line of light down it."""
    if axis == 'z':
        zz = (z - depth, z) if face > 0 else (z, z + depth)
        cbox(x - w, y0, zz[0], x + w, y1, zz[1], 'stone_dark', c=0.0, top=True, cell=2.0)
        if light:
            zl = (zz[1], zz[1] + 0.015) if face > 0 else (zz[0] - 0.015, zz[0])
            strip_light(x - 0.035, y0 + 0.15, zl[0], x + 0.035, y1 - 0.15, zl[1], 'light_white')
    else:
        xx = (x - depth, x) if face > 0 else (x, x + depth)
        cbox(xx[0], y0, z - w, xx[1], y1, z + w, 'stone_dark', c=0.0, top=True, cell=2.0)
        if light:
            xl = (xx[1], xx[1] + 0.015) if face > 0 else (xx[0] - 0.015, xx[0])
            strip_light(xl[0], y0 + 0.15, z - 0.035, xl[1], y1 - 0.15, z + 0.035, 'light_white')


# ------------------------------------------------------------------ the hall
def hall():
    """The tower's foot: x +-4, z 17..24, its walls 0.8 thick, doors south, east
    and west, the roof slab at 4..4.5 and the balcony south of it."""
    y0, y1 = -0.5, 4.0
    # South wall either side of the door, its lintel, set back and framed.
    for s in (-1, 1):
        lo, hi = sorted((s * 1.5, s * 4.0))
        cbox(lo, y0, 17.0 + BACK, hi, y1, 17.8, 'stone', c=0.03)
        # The door's jamb and the wall's corner pier stand out to the plane.
        a, b = sorted((s * 1.5, s * 2.05))
        cbox(a, y0, 17.0, b, y1, 17.8, 'stone', c=0.06)
        a, b = sorted((s * 3.35, s * 4.0))
        cbox(a, y0, 17.0, b, y1, 17.8, 'stone', c=0.08)
        # Plinth and string course between them.
        a, b = sorted((s * 2.05, s * 3.35))
        cbox(a, y0, 17.0, b, 0.55, 17.2, 'stone_dark', c=0.04)
        cbox(a, 3.45, 17.0, b, 4.0, 17.2, 'stone', c=0.04)
        # A slit in the panel.
        slit(s * 2.7, 1.05, 2.95, 17.0 + BACK, w=0.2, face=-1)
    cbox(-1.5, 2.8, 17.0, 1.5, 4.0, 17.8, 'stone', c=0.06)
    # The lintel's face carries the scale: small cuts of bronze.
    for i in range(-5, 6):
        x = i * 0.26
        h = 0.32 if i % 5 == 0 else 0.17
        cbox(x - 0.018, 3.62 - h, 16.985, x + 0.018, 3.62, 17.0, 'bronze', c=0.0, cell=2.0)
    cbox(-1.42, 3.62, 16.985, 1.42, 3.67, 17.0, 'bronze', c=0.0, cell=4.0)
    # Light in the door's head.
    strip_light(-1.3, 2.74, 17.15, 1.3, 2.8, 17.45, 'light_white')

    # Side walls with a door each.
    for s in (-1, 1):
        def bx(a, y_a, z_a, b, y_b, z_b, mat='stone', c=0.05):
            lo, hi = sorted((s * a, s * b))
            cbox(lo, y_a, z_a, hi, y_b, z_b, mat, c=c)
        bx(4.0 - BACK, y0, 17.8, 3.2, y1, 19.5, c=0.03)
        bx(4.0 - BACK, y0, 21.5, 3.2, y1, 23.2, c=0.03)
        bx(4.0, 2.6, 19.5, 3.2, y1, 21.5, c=0.06)
        # Door jambs out to the plane.
        bx(4.0, y0, 19.0, 3.2, y1, 19.5, c=0.05)
        bx(4.0, y0, 21.5, 3.2, y1, 22.0, c=0.05)
        # Corner piers.
        bx(4.0, y0, 17.0, 3.2, y1, 17.85, c=0.08)
        bx(4.0, y0, 23.3, 3.2, y1, 24.0, c=0.08)
        # Plinth and string course.
        bx(4.0, y0, 17.85, 3.86, 0.55, 19.0, 'stone_dark', c=0.03)
        bx(4.0, y0, 22.0, 3.86, 0.55, 23.3, 'stone_dark', c=0.03)
        bx(4.0, 3.45, 17.85, 3.86, 4.0, 19.0, c=0.03)
        bx(4.0, 3.45, 22.0, 3.86, 4.0, 23.3, c=0.03)
        lo, hi = sorted((s * 3.35, s * 3.85))
        strip_light(lo, 2.54, 19.6, hi, 2.6, 21.4, 'light_white')
    # North wall.
    cbox(-3.2, y0, 23.2, 3.2, y1, 24.0 - BACK, 'stone', c=0.03)
    for x in (-1.2, 1.2):
        cbox(x - 0.28, y0, 23.2, x + 0.28, y1, 24.0, 'stone', c=0.05)
    cbox(-3.3, y0, 23.86, 3.3, 0.55, 24.0, 'stone_dark', c=0.03)
    cbox(-3.3, 3.45, 23.86, 3.3, y1, 24.0, 'stone', c=0.03)
    # Roof slab, with a cornice under its edge that stays inside the slab's
    # plan: the balcony is walked on, so its top is the sim's to the bit.
    cbox(-4, 4.0, 17.0, 4, 4.5, 24.0, 'stone', c=0.04)
    cbox(-3.86, 3.86, 17.14, 3.86, 4.0, 23.86, 'stone_dark', c=0.0, cell=3.0)

    # The balcony: a slab on two corbels, its parapet cut into merlons that
    # stay inside the parapet's box.
    cbox(-3, 4.0, 14.5, 3, 4.5, 17.0, 'stone', c=0.04)
    for x in (-2.6, 2.6):
        # Corbels under it: a stepped bracket against the wall.
        cbox(x - 0.3, 3.1, 16.45, x + 0.3, 4.0, 17.0, 'stone_dark', c=0.04)
        cbox(x - 0.3, 3.5, 15.75, x + 0.3, 4.0, 16.45, 'stone_dark', c=0.04)
        cbox(x - 0.3, 3.78, 15.05, x + 0.3, 4.0, 15.75, 'stone_dark', c=0.03)
    cbox(-3, 4.5, 14.5, 3, 4.72, 14.8, 'stone_dark', c=0.03)
    for i in range(-3, 4):
        x = i * 0.86
        w = 0.26
        cbox(x - w, 4.72, 14.5, x + w, 5.14, 14.8, 'stone', c=0.03)
        cbox(x - w - 0.02, 5.14, 14.5, x + w + 0.02, 5.3, 14.8, 'stone_dark', c=0.03)
    # Under-light along the balcony's face.
    strip_light(-2.9, 3.98, 14.62, 2.9, 4.0, 14.72, 'light_white')


def ramps():
    for s in (-1, 1):
        x0, x1 = sorted((s * 10.0, s * 3.0))
        wedge(x0, 14.5, x1, 16.5, '+x' if s < 0 else '-x', 0.0, 4.5, -0.5)
        # A kerb along the open side's foot, dark, and piers under the high end.
        a, b = sorted((s * 5.5, s * 4.1))
        cbox(a, -0.5, 14.5, b, 2.6, 16.5, 'stone_dark', c=0.04)


# ----------------------------------------------------------------- the tower
def tower():
    cz = 20.9
    # Tier one, y 4.5..12: from the slab, inside its box, leaning in.
    tiers = [
        # y, half x, half z, corner cut
        (4.5, 3.86, 3.0, 0.2),
        (6.6, 3.8, 2.96, 0.3),
        (11.4, 3.46, 2.72, 0.6),
    ]
    loft([ring8(hx, hz, ch, cz, y) for y, hx, hz, ch in tiers], cap=False)
    # Its cornice, up and out: free above reach.
    loft([
        ring8(3.46, 2.72, 0.6, cz, 11.4),
        ring8(3.62, 2.88, 0.62, cz, 11.55),
        ring8(3.86, 3.1, 0.68, cz, 11.95),
        ring8(3.86, 3.1, 0.68, cz, 12.2),
        ring8(3.5, 2.8, 0.66, cz, 12.32),
    ], 'stone_dark', cap=False)
    # Tier two, y 12.3..22: the long shaft, more octagon as it rises.
    loft([
        ring8(3.42, 2.74, 0.68, cz, 12.32),
        ring8(3.3, 2.68, 0.9, cz, 15.0),
        ring8(2.86, 2.38, 1.08, cz, 21.4),
    ], cap=False)
    loft([
        ring8(2.86, 2.38, 1.08, cz, 21.4),
        ring8(3.0, 2.52, 1.12, cz, 21.55),
        ring8(3.24, 2.76, 1.2, cz, 21.95),
        ring8(3.24, 2.76, 1.2, cz, 22.2),
        ring8(2.72, 2.32, 1.08, cz, 22.34),
    ], 'stone_dark', cap=False)
    # Tier three, y 22.3..27: the lantern, near a true octagon.
    loft([
        ring8(2.46, 2.1, 0.86, cz, 22.34),
        ring8(2.28, 2.02, 0.84, cz, 26.5),
    ], cap=False)
    loft([
        ring8(2.28, 2.02, 0.84, cz, 26.5),
        ring8(2.4, 2.14, 0.88, cz, 26.62),
        ring8(2.62, 2.36, 0.96, cz, 26.95),
        ring8(2.62, 2.36, 0.96, cz, 27.2),
        ring8(2.1, 2.0, 0.8, cz, 27.32),
    ], 'stone_dark', cap=True)

    # Long shallow pilasters up the first two tiers, one each side of each
    # face, following the lean: lofted as thin slabs from the shaft.
    def lean(y, table):
        for (y0, a0), (y1, a1) in zip(table, table[1:]):
            if y0 <= y <= y1:
                t = (y - y0) / (y1 - y0)
                return a0 + (a1 - a0) * t
        return table[-1][1]

    hx1 = [(4.5, 3.86), (6.6, 3.8), (11.4, 3.46)]
    hz1 = [(4.5, 3.0), (6.6, 2.96), (11.4, 2.72)]
    hx2 = [(12.32, 3.42), (15.0, 3.3), (21.4, 2.86)]
    hz2 = [(12.32, 2.74), (15.0, 2.68), (21.4, 2.38)]
    for tier, (yy, hx, hz) in enumerate(((((6.8, 11.3)), hx1, hz1), ((12.5, 21.3), hx2, hz2))):
        y0, y1 = yy
        # The south and north faces' slits, the east and west faces'.
        for f in (-1, 1):
            z_a = cz + f * lean(y0, hz)
            z_b = cz + f * lean(y1, hz)
            xs = (-1.5, 1.5) if tier == 0 else (0.0,)
            for x in xs:
                w = 0.32 if tier == 0 else 0.42
                y_a, y_b = (y0 + 0.7, y1 - 0.7)
                za = z_a + (z_b - z_a) * 0.14
                zb = z_a + (z_b - z_a) * 0.86
                quad((x - w, y_a, za + f * 0.012), (x + w, y_a, za + f * 0.012), (x + w, y_b, zb + f * 0.012), (x - w, y_b, zb + f * 0.012),
                     'stone_dark', (0, 0, f), cell=3.0)
                quad((x - 0.04, y_a + 0.2, za + f * 0.024), (x + 0.04, y_a + 0.2, za + f * 0.024), (x + 0.04, y_b - 0.2, zb + f * 0.024),
                     (x - 0.04, y_b - 0.2, zb + f * 0.024), 'light_white', (0, 0, f), cell=30.0)
            for z in ((-1.0, 1.0) if tier == 0 else (0.0,)):
                x_a = f * lean(y0, hx)
                x_b = f * lean(y1, hx)
                y_a, y_b = (y0 + 0.7, y1 - 0.7)
                xa = x_a + (x_b - x_a) * 0.14
                xb = x_a + (x_b - x_a) * 0.86
                w = 0.32 if tier == 0 else 0.42
                quad((xa + f * 0.012, y_a, cz + z - w), (xa + f * 0.012, y_a, cz + z + w), (xb + f * 0.012, y_b, cz + z + w), (xb + f * 0.012, y_b, cz + z - w),
                     'stone_dark', (f, 0, 0), cell=3.0)
                quad((xa + f * 0.024, y_a + 0.2, cz + z - 0.04), (xa + f * 0.024, y_a + 0.2, cz + z + 0.04), (xb + f * 0.024, y_b - 0.2, cz + z + 0.04),
                     (xb + f * 0.024, y_b - 0.2, cz + z - 0.04), 'light_white', (f, 0, 0), cell=30.0)
    # Ribs up each face, following its lean: the shaft's vertical rhythm, and
    # what catches the low sun. Proud of the face, inside the sim's boxes.
    def rib(face_z, f, off, y0, y1, hx, hz, w, proud):
        def at(y, side, out):
            if face_z:
                return (off + side * w, y, cz + f * (lean(y, hz) + out))
            return (f * (lean(y, hx) + out), y, cz + off + side * w)
        n = (0, 0, f) if face_z else (f, 0, 0)
        quad(at(y0, -1, proud), at(y0, 1, proud), at(y1, 1, proud), at(y1, -1, proud), 'stone', n, cell=0.9)
        for side in (-1, 1):
            o = (side, 0, 0) if face_z else (0, 0, side)
            quad(at(y0, side, 0.0), at(y0, side, proud), at(y1, side, proud), at(y1, side, 0.0), 'stone', o, cell=0.9)
        up = (0, 1, f) if face_z else (f, 1, 0)
        quad(at(y1, -1, proud), at(y1, 1, proud), at(y1 + 0.16, 1, 0.0), at(y1 + 0.16, -1, 0.0), 'stone', up, cell=2.0)
    for f in (-1, 1):
        for off in (-3.02, -2.34, -0.72, 0.72, 2.34, 3.02):
            rib(True, f, off, 6.6, 11.05, hx1, hz1, 0.13, 0.1)
        for off in (-2.28, -1.72, -0.42, 0.42, 1.72, 2.28):
            rib(False, f, off, 6.6, 11.05, hx1, hz1, 0.13, 0.1)
        for off in (-1.5, -0.86, 0.86, 1.5):
            rib(True, f, off, 12.6, 20.3, hx2, hz2, 0.11, 0.08)
            rib(False, f, off * 0.82, 12.6, 20.3, hx2, hz2, 0.11, 0.08)
    # String courses: the shaft's scale, read from the far side of the island.
    for y in (15.6, 18.0):
        hxs, hzs = lean(y, hx2), lean(y, hz2)
        ch = 0.9 + (y - 15.0) / 6.4 * 0.18
        loft([ring8(hxs, hzs, ch, cz, y), ring8(hxs + 0.13, hzs + 0.13, ch, cz, y + 0.08),
              ring8(hxs + 0.13, hzs + 0.13, ch, cz, y + 0.26), ring8(hxs, hzs, ch, cz, y + 0.34)], 'stone_dark', cap=False, cell=3.0)
    # And one where the first tier leaves the roof.
    loft([ring8(3.86, 3.0, 0.2, cz, 6.3), ring8(3.92, 3.06, 0.22, cz, 6.36), ring8(3.92, 3.06, 0.22, cz, 6.58), ring8(3.8, 2.96, 0.3, cz, 6.66)],
         'stone_dark', cap=False, cell=3.0)
    # Bronze scale bands round the shaft, under the second cornice.
    for y, hx, hz in ((20.55, 2.93, 2.43), (13.2, 3.4, 2.73)):
        loft([ring8(hx + 0.03, hz + 0.03, 1.0, cz, y), ring8(hx + 0.03, hz + 0.03, 1.0, cz, y + 0.22)], 'bronze', cap=False, cell=3.0)

    # The lantern's eight openings: dark panels with light in them.
    for k in range(8):
        a = math.tau * (k + 0.5) / 8.0
        cx, sz = math.cos(a), math.sin(a)
        rx, rz = 2.12, 1.95
        o = Vector((cx * rx, 0, cz + sz * rz))
        t = Vector((-sz, 0, cx))
        w = 0.34
        for y_a, y_b, wd, mat, off in ((23.0, 26.1, w, 'stone_dark', 0.0), (23.2, 25.9, 0.05, 'light_white', 0.02)):
            n = Vector((cx, 0, sz))
            p = o + n * (0.16 + off)
            a0 = p - t * wd
            a1 = p + t * wd
            quad((a0.x, y_a, a0.z), (a1.x, y_a, a1.z), (a1.x, y_b, a1.z), (a0.x, y_b, a0.z), mat, tuple(n), cell=20.0)

    # The crown: a mast on the platform, brackets, rings. The mast and its
    # brackets stand; the rings (`crown`) turn.
    mx_, my, mz = CROWN_AT
    rod((0, 27.3, cz), (0, 39.0, cz), 0.3, 'bronze', sides=10, taper=0.16)
    loft([ring8(0.72, 0.72, 0.28, cz, 27.32), ring8(0.5, 0.5, 0.2, cz, 28.3), ring8(0.34, 0.34, 0.13, cz, 29.4)], 'stone_dark', cap=True, cell=4.0)
    # Four curved arms from the platform's corners up to the great ring.
    r_arm = 3.5
    for k in range(4):
        a = math.tau * (k + 0.5) / 4
        d = Vector((math.cos(a), 0, math.sin(a)))
        pts = []
        for i in range(9):
            t = i / 8
            # A bow: out low, up to the ring's hoop.
            r = 1.9 + (r_arm - 1.9) * (1 - (1 - t) ** 2)
            y = 27.32 + 3.2 * t
            pts.append((d.x * r, y, cz + d.z * r))
        for p, q in zip(pts, pts[1:]):
            rod(p, q, 0.11, 'bronze', sides=6)
    # The fixed hoop the arms carry, level, stays still; the rings inside it turn.
    torus((0, 30.52, cz), r_arm, 0.14, (0, 1, 0), 'bronze', segs=64, sides=8)
    c = (mx_, my, mz)
    torus(c, 3.0, 0.1, (0, 1, 0), 'crown', segs=64, sides=8)
    torus(c, 2.72, 0.09, (1, 0, 0), 'crown', segs=64, sides=8)
    t = math.radians(23.4)
    torus(c, 2.44, 0.08, (0.0, math.cos(t), math.sin(t)), 'crown', segs=64, sides=8)
    torus(c, 2.15, 0.07, (math.cos(t), math.sin(t), 0.0), 'crown', segs=64, sides=8)


# --------------------------------------------------------- the meridian arch
ARCH_R = 9.1
ARCH_THICK = (1.78, 0.74)
ARCH_DEPTH = (0.68, 0.46)
ARCH_FOOT_DEG = 180.0
ARCH_TIP_DEG = 99.0
ARCH_STEPS = 56


def arch():
    """Red's half of the broken arch (and blue's by its sign): a stone rib on a
    circle over the plaza, voussoirs cut in it, a bronze scale along its side.
    Its collision is the `sweep` in map.json: same radii, its depth DEPTH[0]."""
    for side in (-1, 1):
        def at(deg, r):
            a = math.radians(deg)
            return (side * -r * math.cos(a) * -1 * -1, r * math.sin(a))

        rows = []
        for i in range(ARCH_STEPS + 1):
            t = i / ARCH_STEPS
            deg = ARCH_FOOT_DEG + (ARCH_TIP_DEG - ARCH_FOOT_DEG) * t
            th = ARCH_THICK[0] + (ARCH_THICK[1] - ARCH_THICK[0]) * t ** 0.8
            dp = ARCH_DEPTH[0] + (ARCH_DEPTH[1] - ARCH_DEPTH[0]) * t
            rows.append((deg, ARCH_R, ARCH_R - th, dp, t))
        # A voussoir's joint every few steps: the section pinched a little.
        def section(deg, ro, ri, dp, pinch):
            a = math.radians(deg)
            dx, dy = side * math.cos(math.pi - a), math.sin(a)
            c = 0.07 + pinch
            r_o, r_i = ro - pinch, ri + pinch
            d = dp - pinch
            out = lambda r, z: (dx * r, dy * r, z)
            return [
                out(r_o - c, -d), out(r_i + c, -d), out(r_i, -d + c), out(r_i, d - c),
                out(r_i + c, d), out(r_o - c, d), out(r_o, d - c), out(r_o, -d + c),
            ]
        rings = []
        for i, (deg, ro, ri, dp, t) in enumerate(rows):
            rings.append(section(deg, ro, ri, dp, 0.0))
            if 0 < i < ARCH_STEPS and i % 7 == 0:
                d2 = deg + (ARCH_TIP_DEG - ARCH_FOOT_DEG) / ARCH_STEPS * 0.05
                rings.append(section(d2, ro, ri, dp, 0.035))
                rings.append(section(d2 + (ARCH_TIP_DEG - ARCH_FOOT_DEG) / ARCH_STEPS * 0.05, ro, ri, dp, 0.0))
        for a_ring, b_ring in zip(rings, rings[1:]):
            mid = Vector((0.0, 0.0, 0.0))
            for i in range(8):
                p, q = a_ring[i], a_ring[(i + 1) % 8]
                r, s = b_ring[(i + 1) % 8], b_ring[i]
                # Faces outward from the section's middle.
                cx = sum(x[0] for x in a_ring) / 8
                cy = sum(x[1] for x in a_ring) / 8
                cz = sum(x[2] for x in a_ring) / 8
                fm = ((p[0] + q[0]) / 2 - cx, (p[1] + q[1]) / 2 - cy, (p[2] + q[2]) / 2 - cz)
                quad(p, q, r, s, 'stone', fm, cell=0.9)
        # The tip's end, cut square, its face toward the gap.
        tip = rings[-1]
        cy = sum(p[1] for p in tip) / 8
        cx = sum(p[0] for p in tip) / 8
        for i in range(8):
            tri(tip[i], tip[(i + 1) % 8], (cx, cy, 0.0), 'stone_dark', (-side * 0.99, 0.15, 0.0))
        # The bronze scale on both faces: a band along the rib and a tick each
        # few degrees, a long one each fifth.
        for f in (-1, 1):
            band_rows = []
            for i in range(ARCH_STEPS + 1):
                deg, ro, ri, dp, t = rows[i]
                if t < 0.08 or t > 0.97:
                    continue
                a = math.radians(deg)
                dx, dy = side * math.cos(math.pi - a), math.sin(a)
                z = f * (dp + 0.006)
                mid_r = ri + (ro - ri) * 0.32
                w = 0.05
                band_rows.append(((dx * (mid_r - w), dy * (mid_r - w), z), (dx * (mid_r + w), dy * (mid_r + w), z)))
            for (a0, a1), (b0, b1) in zip(band_rows, band_rows[1:]):
                quad(a0, a1, b1, b0, 'bronze', (0, 0, f), cell=4.0)
            k = 0
            deg = ARCH_FOOT_DEG - 8.0
            while deg > ARCH_TIP_DEG + 3.0:
                t = (ARCH_FOOT_DEG - deg) / (ARCH_FOOT_DEG - ARCH_TIP_DEG)
                th = ARCH_THICK[0] + (ARCH_THICK[1] - ARCH_THICK[0]) * t ** 0.8
                dp = ARCH_DEPTH[0] + (ARCH_DEPTH[1] - ARCH_DEPTH[0]) * t
                ri = ARCH_R - th
                a = math.radians(deg)
                dx, dy = side * math.cos(math.pi - a), math.sin(a)
                mid_r = ri + th * 0.32
                ln = 0.34 if k % 5 == 0 else 0.17
                wa = math.radians(0.14 if k % 5 == 0 else 0.09)
                z = f * (dp + 0.007)
                for aa, bb in (((-wa), wa),):
                    p0 = (side * math.cos(math.pi - a - aa) * (mid_r + 0.05), math.sin(a + aa) * (mid_r + 0.05), z)
                    p1 = (side * math.cos(math.pi - a - bb) * (mid_r + 0.05), math.sin(a + bb) * (mid_r + 0.05), z)
                    p2 = (side * math.cos(math.pi - a - bb) * (mid_r + 0.05 + ln), math.sin(a + bb) * (mid_r + 0.05 + ln), z)
                    p3 = (side * math.cos(math.pi - a - aa) * (mid_r + 0.05 + ln), math.sin(a + aa) * (mid_r + 0.05 + ln), z)
                    quad(p0, p1, p2, p3, 'bronze', (0, 0, f), cell=4.0)
                deg -= 2.6
                k += 1
        # Its foot on a stepped plinth (a `hidden` box in the map).
        x0, x1 = sorted((side * 9.75, side * 7.05))
        cbox(x0, -0.5, -1.1, x1, 0.82, 1.1, 'stone_dark', c=0.08)
        x0, x1 = sorted((side * 9.5, side * 7.2))
        cbox(x0, 0.82, -0.86, x1, 1.14, 0.86, 'stone', c=0.06)
        # A line of light in the rib's underside, its foot to its tip.
        lit = []
        for i in range(ARCH_STEPS + 1):
            deg, ro, ri, dp, t = rows[i]
            if t < 0.04:
                continue
            a = math.radians(deg)
            dx, dy = side * math.cos(math.pi - a), math.sin(a)
            lit.append(((dx * (ri - 0.006), dy * (ri - 0.006), -0.05), (dx * (ri - 0.006), dy * (ri - 0.006), 0.05)))
        for (a0, a1), (b0, b1) in zip(lit, lit[1:]):
            mid_a = ((a0[0] + b0[0]) / 2, (a0[1] + b0[1]) / 2)
            quad(a0, a1, b1, b0, 'light_white', (-mid_a[0], -mid_a[1], 0), cell=30.0)


# ------------------------------------------------------------------ the gates
def gates():
    for s, light in ((-1, 'light_red'), (1, 'light_blue')):
        x0, x1 = sorted((s * 11.6, s * 10.6))
        cxm = (x0 + x1) / 2
        for f in (-1, 1):
            z0, z1 = sorted((f * 2.2, f * 3.2))
            # A squared post on a plinth, leaning in by a hand, inside its box.
            cbox(x0, -0.5, z0, x1, 0.5, z1, 'stone_dark', c=0.08)
            loft([
                ring8(0.46, 0.46, 0.08, (z0 + z1) / 2, 0.5, cx=cxm),
                ring8(0.4, 0.4, 0.1, (z0 + z1) / 2, 3.42, cx=cxm),
                ring8(0.5, 0.5, 0.05, (z0 + z1) / 2, 3.6, cx=cxm),
            ], cap=False)
            # A sighting slot's dark stripe on each of its broad faces, and
            # its side's light in it.
            for g in (-1, 1):
                x = cxm + g * 0.43
                quad((x + g * 0.004, 1.2, (z0 + z1) / 2 - 0.09), (x + g * 0.004, 1.2, (z0 + z1) / 2 + 0.09),
                     (x - g * 0.03, 3.15, (z0 + z1) / 2 + 0.09), (x - g * 0.03, 3.15, (z0 + z1) / 2 - 0.09), 'stone_dark', (g, 0, 0), cell=3.0)
                quad((x + g * 0.012, 1.34, (z0 + z1) / 2 - 0.03), (x + g * 0.012, 1.34, (z0 + z1) / 2 + 0.03),
                     (x - g * 0.022, 3.02, (z0 + z1) / 2 + 0.03), (x - g * 0.022, 3.02, (z0 + z1) / 2 - 0.03), light, (g, 0, 0), cell=30.0)
        # The lintel: one great stone, a dark seat under it and its side's line
        # of light along both faces.
        cbox(x0 + 0.05, 3.6, -3.2, x1 - 0.05, 3.78, 3.2, 'stone_dark', c=0.03)
        cbox(x0, 3.78, -3.2, x1, 4.4, 3.2, 'stone', c=0.1)
        for g in (-1, 1):
            x = cxm + g * 0.502
            strip_light(*sorted((x - 0.012, x + 0.012))[:1], 3.97, -2.9, *sorted((x - 0.012, x + 0.012))[1:], 4.03, 2.9, light)
        # Bronze straps over the lintel's ends.
        for z in (-2.7, 2.7):
            cbox(x0 - 0.012, 3.76, z - 0.08, x1 + 0.012, 4.412, z + 0.08, 'bronze', c=0.01, cell=3.0)


# ---------------------------------------------------------------- the bastions
def decks():
    for s, light in ((-1, 'light_red'), (1, 'light_blue')):
        def bx(a, y0, z0, b, y1, z1, mat='stone', c=0.05, cell=CELL):
            lo, hi = sorted((s * a, s * b))
            cbox(lo, y0, z0, hi, y1, z1, mat, c=c, cell=cell)
        # The floor's top is the sim's to the bit; its walls are set back from
        # the plane behind piers.
        bx(25.5 - BACK, -0.5, -6 + BACK, 19 + BACK, 2.22, 6 - BACK, 'stone', c=0.02)
        bx(25.5, 2.22, -6, 19, 2.5, 6, 'stone', c=0.05)  # the coping that is walked on
        bx(25.5, -0.5, -6, 19, 0.5, -5.86, 'stone_dark', c=0.03)
        bx(25.5, -0.5, 5.86, 19, 0.5, 6, 'stone_dark', c=0.03)
        for z in (-6, -2.9, 2.9 - 0.5, 6 - 0.5):
            bx(25.5, -0.5, z, 25.36, 2.22, z + 0.5, 'stone', c=0.05)
        for x in (19.0, 21.1, 23.2, 25.0):
            bx(x + 0.5, -0.5, -6, x, 2.22, -5.86, 'stone', c=0.05)
            bx(x + 0.5, -0.5, 5.86, x, 2.22, 6, 'stone', c=0.05)
        # The west parapet, in merlons on a base, inside its box.
        bx(25.5, 2.5, -6, 25, 2.82, 6, 'stone_dark', c=0.04)
        for k in range(-5, 6):
            z = k * 1.08
            bx(25.5, 2.82, z - 0.32, 25, 3.5, z + 0.32, 'stone', c=0.05)
        # Its side's light along the parapet's inner face.
        lo, hi = sorted((s * 25.008, s * 24.992))
        strip_light(lo, 2.77, -5.8, hi, 2.8, 5.8, light)
        # The cover stone: a low altar in the middle.
        bx(23, 2.5, -1, 22, 3.25, 1, 'stone', c=0.08)
        bx(23.0, 3.25, -1.0, 22.0, 3.7, 1.0, 'stone_dark', c=0.08)
        lo, hi = sorted((s * 23.01, s * 21.99))
        strip_light(lo, 3.22, -0.9, hi, 3.25, 0.9, light)
        # The east parapets, walling it toward the ring but for the ramp.
        for z0, z1 in ((-6, -2), (2, 6)):
            bx(19.5, 2.5, z0, 19, 2.92, z1, 'stone_dark', c=0.04)
            bx(19.5, 2.92, z0, 19, 3.95, z1, 'stone', c=0.04)
            bx(19.5, 3.95, z0, 19, 4.3, z1, 'stone_dark', c=0.05)
            lo, hi = sorted((s * 19.51, s * 18.99))
            strip_light(lo, 3.74, z0 + 0.15, hi, 3.77, z1 - 0.15, light)
            # A long slit each, looking at the ring.
            zm = (z0 + z1) / 2
            bx(19.5, 3.2, zm - 0.9, 18.985, 3.55, zm + 0.9, 'stone_dark', c=0.0, cell=4.0)
        # A threshold where the ramp meets the floor: the coping's cut edge would
        # leave a dark line across it.
        bx(19.06, 2.42, -2, 18.97, 2.5, 2, 'stone', c=0.0, cell=4.0)
        # Its ramp down to the gate.
        x0, x1 = sorted((s * 19.0, s * 14.5))
        wedge(x0, -2, x1, 2, '-x' if s < 0 else '+x', 0.0, 2.5, -0.5)


def build_all():
    hall()
    ramps()
    tower()
    arch()
    gates()
    decks()


# ------------------------------------------------------------ into Blender
def to_blender(p):
    """Game (x, y, z) to Blender (x, -z, y): the glTF exporter's +Y up undoes it."""
    return (p[0], -p[2], p[1])


def make_objects():
    objs = {}
    for name, pa in PARTS.items():
        me = bpy.data.meshes.new(name)
        verts = [to_blender(p) for p in pa.verts]
        faces = [f[0] for f in pa.faces]
        me.from_pydata(verts, [], faces)
        me.update()
        uv = me.uv_layers.new(name='UVMap')
        loop = 0
        for poly, (_, uvs, smooth) in zip(me.polygons, pa.faces):
            poly.use_smooth = smooth
            for u in uvs:
                uv.data[loop].uv = u
                loop += 1
        o = bpy.data.objects.new(name, me)
        bpy.context.scene.collection.objects.link(o)
        objs[name] = o
    return objs


def fibonacci_hemisphere(n):
    out = []
    golden = math.pi * (3 - math.sqrt(5))
    for i in range(n):
        y = (i + 0.5) / n
        r = math.sqrt(1 - y * y)
        a = golden * i
        out.append(Vector((r * math.cos(a), r * math.sin(a), y)))
    return out


def paint(objs):
    """COLOR_0: traced occlusion and the sand's dust, per vertex."""
    # All the geometry, and the ground at about y 0, as one tree.
    verts = []
    polys = []
    for o in objs.values():
        base = len(verts)
        verts.extend(tuple(v.co) for v in o.data.vertices)
        polys.extend([tuple(i + base for i in p.vertices) for p in o.data.polygons])
    base = len(verts)
    s = 80.0
    verts.extend([(-s, -s, 0.0), (s, -s, 0.0), (s, s, 0.0), (-s, s, 0.0)])
    polys.append((base, base + 1, base + 2, base + 3))
    tree = BVHTree.FromPolygons(verts, polys, all_triangles=False)
    dirs = fibonacci_hemisphere(AO_RAYS)
    rng = random.Random(7)
    for name, o in objs.items():
        me = o.data
        attr = me.color_attributes.new(name='Col', type='FLOAT_COLOR', domain='POINT')
        me.calc_normals_split() if hasattr(me, 'calc_normals_split') else None
        normals = [Vector((0, 0, 0)) for _ in me.vertices]
        for p in me.polygons:
            for i in p.vertices:
                normals[i] += p.normal
        lit = name.startswith('light_')
        for i, v in enumerate(me.vertices):
            n = normals[i].normalized() if normals[i].length > 1e-6 else Vector((0, 0, 1))
            if lit:
                attr.data[i].color = (1, 1, 1, 1)
                continue
            t = n.orthogonal().normalized()
            b = n.cross(t)
            at = v.co + n * 0.03
            hit = 0.0
            for d in dirs:
                w = t * d.x + b * d.y + n * d.z
                loc, _, _, dist = tree.ray_cast(at, w, AO_REACH)
                if loc is not None:
                    hit += (1.0 - dist / AO_REACH) ** 1.5
            ao = 1.0 - hit / AO_RAYS
            ao = AO_FLOOR + (1.0 - AO_FLOOR) * min(1.0, ao * 1.15)
            y = v.co.z
            dust = max(0.0, min(1.0, 1.0 - y / DUST_M)) ** 1.5
            d = (1 + (DUST_RGB[0] - 1) * dust, 1 + (DUST_RGB[1] - 1) * dust, 1 + (DUST_RGB[2] - 1) * dust)
            # A little of each block's own, so no course is one value.
            cell = (int(v.co.x // 1.3), int(v.co.y // 1.3), int(v.co.z // 0.7))
            rng.seed(hash(cell) & 0xffff)
            jitter = 0.94 + 0.12 * rng.random()
            attr.data[i].color = (ao * d[0] * jitter, ao * d[1] * jitter, ao * d[2] * jitter, 1.0)
        me.color_attributes.active_color_index = 0


def export(objs):
    """One GLB a material, `models/isle/observatory_<name>.glb`."""
    os.makedirs(OUT, exist_ok=True)
    total = 0
    for name, o in objs.items():
        bpy.ops.object.select_all(action='DESELECT')
        o.select_set(True)
        bpy.context.view_layer.objects.active = o
        path = os.path.join(OUT, f'observatory_{name}.glb')
        bpy.ops.export_scene.gltf(
            filepath=path,
            export_format='GLB',
            use_selection=True,
            export_yup=True,
            export_apply=False,
            export_normals=True,
            export_tangents=True,
            export_texcoords=True,
            export_materials='NONE',
            export_colors=True,
            export_animations=False,
        )
        tris = sum(len(p.vertices) - 2 for p in o.data.polygons)
        total += tris
        print(f'MODEL observatory_{name}: {tris} tris, {os.path.getsize(path) / 1e6:.2f} MB')
    print(f'MODEL observatory: {total} tris in all')


def sheet(objs):
    """A clay picture from four sides: Cycles on the CPU, a few samples."""
    os.makedirs(SHEETS, exist_ok=True)
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.device = 'CPU'
    scene.cycles.samples = 24
    scene.cycles.use_denoising = False
    scene.render.resolution_x, scene.render.resolution_y = 960, 720
    pal = {'stone': (0.62, 0.61, 0.58, 1), 'stone_dark': (0.32, 0.31, 0.3, 1), 'bronze': (0.45, 0.3, 0.14, 1), 'crown': (0.45, 0.3, 0.14, 1)}
    for name, o in objs.items():
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        b = m.node_tree.nodes['Principled BSDF']
        col = pal.get(name, (1, 1, 1, 1))
        attr = m.node_tree.nodes.new('ShaderNodeVertexColor')
        mix = m.node_tree.nodes.new('ShaderNodeMixRGB')
        mix.blend_type = 'MULTIPLY'
        mix.inputs[0].default_value = 1.0
        mix.inputs[1].default_value = col
        m.node_tree.links.new(attr.outputs[0], mix.inputs[2])
        m.node_tree.links.new(mix.outputs[0], b.inputs['Base Color'])
        if name.startswith('light_'):
            tint = {'light_red': (1, 0.2, 0.15, 1), 'light_blue': (0.2, 0.45, 1, 1)}.get(name, (0.8, 0.9, 1, 1))
            b.inputs['Emission Color'].default_value = tint
            b.inputs['Emission Strength'].default_value = 6.0
        o.data.materials.append(m)
    bpy.ops.mesh.primitive_plane_add(size=120, location=(0, 0, 0))
    g = bpy.context.active_object
    gm = bpy.data.materials.new('sand')
    gm.use_nodes = True
    gm.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value = (0.62, 0.48, 0.3, 1)
    g.data.materials.append(gm)
    world = bpy.data.worlds.new('w')
    world.use_nodes = True
    world.node_tree.nodes['Background'].inputs[0].default_value = (0.55, 0.62, 0.75, 1)
    world.node_tree.nodes['Background'].inputs[1].default_value = 0.8
    scene.world = world
    sun = bpy.data.lights.new('sun', 'SUN')
    sun.energy = 3.5
    so = bpy.data.objects.new('sun', sun)
    so.rotation_euler = (math.radians(52), 0, math.radians(25))
    scene.collection.objects.link(so)
    cam = bpy.data.cameras.new('c')
    cam.lens = 30
    co = bpy.data.objects.new('c', cam)
    scene.collection.objects.link(co)
    scene.camera = co
    # (game eye, game look)
    views = {
        'spire': ((-14, 6, 2), (0, 13, 20)),
        'over': ((-30, 22, -30), (0, 4, 4)),
        'arch': ((-6, 2, -14), (0, 5, 2)),
        'deck': ((-13, 4, -8), (-22, 2, 0)),
    }
    which = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else list(views)
    for vname in which:
        eye, look = views[vname]
        e, l = Vector(to_blender(eye)), Vector(to_blender(look))
        co.location = e
        co.rotation_euler = (l - e).to_track_quat('-Z', 'Y').to_euler()
        scene.render.filepath = os.path.join(SHEETS, f'observatory_{vname}.png')
        bpy.ops.render.render(write_still=True)
        print('SHEET', scene.render.filepath)


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    build_all()
    objs = make_objects()
    paint(objs)
    export(objs)
    if '--no-sheet' not in sys.argv:
        sheet(objs)


main()

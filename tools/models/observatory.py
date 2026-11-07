"""Cloudbreak's observatory: every built thing on the island, in the map's own
coordinates (GLBs at the origin, `look/observatory.rs` draws them). An ancient
sky-watchers' station on a floating rock: a tower of three floors round a sight
well, stairs climbing round it to a walk over the void, a tapering shaft with a
crown of rings, a sundial plaza under a broken meridian arch, trilithon gates,
bastions, and walks below the lip of the coast.

    blender -b --python tools/models/observatory.py

**Drawn inside what collides, where anyone can go.** Each structure is the box
the sim collides (`maps/cloudbreak/map.json`, look `hidden`), its body set back
BACK m from that plane and its frames, pilasters and string courses standing
out to it, so the panels are real and nothing proud of a face is outside it.
Above where anyone reaches the tower is free to taper inside its boxes, and its
cornices and rings may stand out. The rim walks are read from the map's own
`walkway` items, so a node moved there moves here too.

One GLB a material: `stone`, `stone_dark`, `bronze`, `crown` (the rings, turned
slowly by the client round CROWN_AT), `light_white`, `light_red`, `light_blue`.
No images: the client wears its own `stone` set on them with the
level's weathering, UVs in tiles of TILE_M. COLOR_0 is painted here, ambient
occlusion traced against the whole island and sand dust up from the ground.

Numbers for the game go in `look/observatory.rs` and `maps/cloudbreak/map.json`.
"""
import json
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
MAP = os.path.join(ROOT, 'maps', 'cloudbreak', 'map.json')

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
# The well through the tower's floors, x0, z0, x1, z1 (the holes in map.json).
WELL = (-1.05, 19.6, 1.05, 22.7)
# How far apart a rim walk's corbels and its wall's pilasters stand, m.
CORBEL_M = 1.9
PILASTER_M = 2.85


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


def plank(a, b, ya, yb, off0, off1, thick, mat='stone', ends=(True, True), cell=CELL):
    """A slab from a to b (x, z), its top at ya rising to yb, from off0 to off1
    across its way (along (-dz, dx)): a slope the sim's sweep walks, to the bit."""
    dx, dz = b[0] - a[0], b[1] - a[1]
    ln = math.hypot(dx, dz)
    nx, nz = -dz / ln, dx / ln
    def p(end, off, y):
        e = a if end == 0 else b
        return (e[0] + nx * off, y, e[1] + nz * off)
    quad(p(0, off0, ya), p(0, off1, ya), p(1, off1, yb), p(1, off0, yb), mat, (0, 1, 0), cell)
    quad(p(0, off0, ya - thick), p(0, off1, ya - thick), p(1, off1, yb - thick), p(1, off0, yb - thick), mat, (0, -1, 0), cell)
    quad(p(0, off0, ya - thick), p(1, off0, yb - thick), p(1, off0, yb), p(0, off0, ya), mat, (-nx, 0, -nz), cell)
    quad(p(0, off1, ya - thick), p(1, off1, yb - thick), p(1, off1, yb), p(0, off1, ya), mat, (nx, 0, nz), cell)
    if ends[0]:
        quad(p(0, off0, ya - thick), p(0, off1, ya - thick), p(0, off1, ya), p(0, off0, ya), mat, (-dx, 0, -dz), cell)
    if ends[1]:
        quad(p(1, off0, yb - thick), p(1, off1, yb - thick), p(1, off1, yb), p(1, off0, yb), mat, (dx, 0, dz), cell)


def tbox(cx, cz, ux, uz, hu, hv, y0, y1, mat='stone', cell=1.2):
    """A box turned to run along (ux, uz), hu to either side of (cx, cz) and hv
    across it, closed all round."""
    vx, vz = -uz, ux
    c = [(cx + s * ux * hu + t * vx * hv, cz + s * uz * hu + t * vz * hv) for s, t in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
    for i in range(4):
        p, q = c[i], c[(i + 1) % 4]
        o = ((p[0] + q[0]) / 2 - cx, 0.0, (p[1] + q[1]) / 2 - cz)
        quad((p[0], y0, p[1]), (q[0], y0, q[1]), (q[0], y1, q[1]), (p[0], y1, p[1]), mat, o, cell)
    quad((c[0][0], y1, c[0][1]), (c[1][0], y1, c[1][1]), (c[2][0], y1, c[2][1]), (c[3][0], y1, c[3][1]), mat, (0, 1, 0), cell)
    quad((c[0][0], y0, c[0][1]), (c[1][0], y0, c[1][1]), (c[2][0], y0, c[2][1]), (c[3][0], y0, c[3][1]), mat, (0, -1, 0), cell)


def panel(x0, y0, z0, x1, y1, z1, face, pier=0.3, band=0.22, mat='stone'):
    """A span of wall: the box the sim collides, its body set back BACK from
    `face` ('-x', '+x', '-z', '+z'), piers at its ends and a plinth and a course
    between them out to the plane."""
    up = face[0] == '+'
    if face[1] == 'x':
        bx0, bx1 = (x0, x1 - BACK) if up else (x0 + BACK, x1)
        a, b = (bx1, x1) if up else (x0, bx0)
        cbox(bx0, y0, z0, bx1, y1, z1, mat, c=0.02)
        cbox(a, y0, z0, b, y1, z0 + pier, mat, c=0.03)
        cbox(a, y0, z1 - pier, b, y1, z1, mat, c=0.03)
        cbox(a, y0, z0 + pier, b, y0 + band, z1 - pier, 'stone_dark', c=0.02)
        cbox(a, y1 - band, z0 + pier, b, y1, z1 - pier, mat, c=0.02)
    else:
        bz0, bz1 = (z0, z1 - BACK) if up else (z0 + BACK, z1)
        a, b = (bz1, z1) if up else (z0, bz0)
        cbox(x0, y0, bz0, x1, y1, bz1, mat, c=0.02)
        cbox(x0, y0, a, x0 + pier, y1, b, mat, c=0.03)
        cbox(x1 - pier, y0, a, x1, y1, b, mat, c=0.03)
        cbox(x0 + pier, y0, a, x1 - pier, y0 + band, b, 'stone_dark', c=0.02)
        cbox(x0 + pier, y1 - band, a, x1 - pier, y1, b, mat, c=0.02)


def floor_with_well(y0, y1, z0, z1, hx):
    """A floor, the well through it, a bronze lip round the hole and a line of
    light in it. Its top is the sim's to the bit."""
    wx0, wz0, wx1, wz1 = WELL
    cbox(-hx, y0, z0, hx, y1, wz0, 'stone', c=0.03)
    cbox(-hx, y0, wz1, hx, y1, z1, 'stone', c=0.03)
    for s in (-1, 1):
        a, b = sorted((s * wx1, s * hx))
        cbox(a, y0, wz0, b, y1, wz1, 'stone', c=0.03)
    t = y1 + 0.01
    lip = 0.09
    cbox(wx0 - lip, y1, wz0 - lip, wx1 + lip, t, wz0 - 0.001, 'bronze', c=0.0, cell=3.0)
    cbox(wx0 - lip, y1, wz1 + 0.001, wx1 + lip, t, wz1 + lip, 'bronze', c=0.0, cell=3.0)
    for s in (-1, 1):
        a, b = sorted((s * (wx1 + 0.001), s * (wx1 + lip)))
        cbox(a, y1, wz0 - 0.001, b, t, wz1 + 0.001, 'bronze', c=0.0, cell=3.0)
    ly = (y0 + y1) / 2
    strip_light(wx0, ly - 0.03, wz0, wx1, ly + 0.03, wz0 + 0.012, 'light_white')
    strip_light(wx0, ly - 0.03, wz1 - 0.012, wx1, ly + 0.03, wz1, 'light_white')
    strip_light(wx0, ly - 0.03, wz0, wx0 + 0.012, ly + 0.03, wz1, 'light_white')
    strip_light(wx1 - 0.012, ly - 0.03, wz0, wx1, ly + 0.03, wz1, 'light_white')


def dial(cx, cy, z, r, face, a0=180.0, a1=360.0):
    """A sighting scale inlaid on a wall facing along z: bronze ticks round an
    arc and its rule, a long one each fifth, a hair proud."""
    z0, z1 = sorted((z, z + face * 0.006))
    n = int((a1 - a0) / 3.0)
    for k in range(n + 1):
        a = math.radians(a0 + (a1 - a0) * k / n)
        ln = 0.22 if k % 5 == 0 else 0.11
        ca, sa = math.cos(a), math.sin(a)
        w = 0.012 if k % 5 == 0 else 0.008
        p0 = (cx + ca * r - sa * w, cy + sa * r + ca * w)
        p1 = (cx + ca * r + sa * w, cy + sa * r - ca * w)
        p2 = (cx + ca * (r - ln) + sa * w, cy + sa * (r - ln) - ca * w)
        p3 = (cx + ca * (r - ln) - sa * w, cy + sa * (r - ln) + ca * w)
        quad((p0[0], p0[1], z1 if face > 0 else z0), (p1[0], p1[1], z1 if face > 0 else z0), (p2[0], p2[1], z1 if face > 0 else z0), (p3[0], p3[1], z1 if face > 0 else z0), 'bronze', (0, 0, face), cell=4.0)
    cbox(cx - r, cy - 0.012, z0, cx + r, cy + 0.012, z1, 'bronze', c=0.0, cell=6.0)


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
    strip_light(-1.3, 2.77, 17.27, 1.3, 2.8, 17.33, 'light_white')

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
        lo, hi = sorted((s * 3.57, s * 3.63))
        strip_light(lo, 2.57, 19.6, hi, 2.6, 21.4, 'light_white')
    # North wall.
    cbox(-3.2, y0, 23.2, 3.2, y1, 24.0 - BACK, 'stone', c=0.03)
    for x in (-1.2, 1.2):
        cbox(x - 0.28, y0, 23.2, x + 0.28, y1, 24.0, 'stone', c=0.05)
    cbox(-3.3, y0, 23.86, 3.3, 0.55, 24.0, 'stone_dark', c=0.03)
    cbox(-3.3, 3.45, 23.86, 3.3, y1, 24.0, 'stone', c=0.03)
    # Its floor's flags, and its roof with the well through it: a cornice under
    # the edge stays inside the slab's plan, since the balcony is walked on.
    cbox(-3.2, -0.2, 17.8, 3.2, 0.02, 23.2, 'stone_dark', c=0.0, cell=1.0)
    floor_with_well(4.0, 4.5, 17.0, 24.0, 4.0)
    wx0, wz0, wx1, wz1 = WELL
    cbox(-3.86, 3.86, 17.14, 3.86, 4.0, wz0, 'stone_dark', c=0.0, cell=3.0)
    cbox(-3.86, 3.86, wz1, 3.86, 4.0, 23.86, 'stone_dark', c=0.0, cell=3.0)
    for s in (-1, 1):
        a, b = sorted((s * wx1, s * 3.86))
        cbox(a, 3.86, wz0, b, 4.0, wz1, 'stone_dark', c=0.0, cell=3.0)

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


# ------------------------------------------------------- the rooms over the hall
def sighting_room():
    """The floor over the hall, y 4.5..8: a door and two windows on the balcony,
    a door each side out to the stairs, a window each side on the stairs."""
    y0, y1 = 4.5, 8.0
    for s in (-1, 1):
        def bx(a, ya, za, b, yb, zb, mat='stone', c=0.05):
            lo, hi = sorted((s * a, s * b))
            cbox(lo, ya, za, hi, yb, zb, mat, c=c)
        # South wall: door jamb, sill and head of the window, corner.
        bx(1.0, y0, 17.8, 1.9, y1, 18.4)
        bx(1.9, y0, 17.8, 2.9, 5.55, 18.4, 'stone_dark', c=0.04)
        bx(1.9, 7.0, 17.8, 2.9, y1, 18.4)
        bx(2.9, y0, 17.8, 4.0, y1, 18.6, c=0.07)
        lo, hi = sorted((s * 2.0, s * 2.8))
        strip_light(lo, 6.97, 18.07, hi, 7.0, 18.13, 'light_white')
        # Side wall: the door to the stair, its window on the stair.
        bx(3.3, 6.9, 18.6, 4.0, y1, 20.0, c=0.05)
        lo, hi = sorted((s * 3.3, s * 4.0))
        panel(lo, y0, 20.0, hi, y1, 21.2, '+x' if s > 0 else '-x')
        bx(3.3, y0, 21.2, 4.0, 5.55, 22.2, 'stone_dark', c=0.04)
        bx(3.3, 6.9, 21.2, 4.0, y1, 22.2)
        panel(lo, y0, 22.2, hi, y1, 23.4, '+x' if s > 0 else '-x')
        a, b = sorted((s * 3.45, s * 3.85))
        strip_light(a, 6.87, 18.75, b, 6.9, 18.81, 'light_white')
        strip_light(a, 6.87, 21.3, b, 6.9, 21.36, 'light_white')
    cbox(-1.0, 7.0, 17.8, 1.0, y1, 18.4, c=0.05)
    strip_light(-0.9, 6.97, 18.07, 0.9, 7.0, 18.13, 'light_white')
    # The lintel's face carries a scale, as the hall's does.
    for i in range(-3, 4):
        x = i * 0.26
        h = 0.26 if i % 3 == 0 else 0.14
        cbox(x - 0.016, 7.72 - h, 17.788, x + 0.016, 7.72, 17.8, 'bronze', c=0.0, cell=2.0)
    panel(-4.0, y0, 23.4, 4.0, y1, 24.0, '+z')
    dial(0.0, 6.5, 23.4, 1.5, -1)
    # A line of light under the roof, inside, each long side.
    for s in (-1, 1):
        lo, hi = sorted((s * 3.28, s * 3.3))
        strip_light(lo, 7.72, 20.1, hi, 7.76, 21.1, 'light_white')
    floor_with_well(8.0, 8.5, 17.8, 24.0, 4.0)


def gallery():
    """The top floor, y 8.5..11.4: three windows looking south over the island
    and two narrow ones each side, a door north onto the back walk. The sills
    stand to a fighter's ribs, the heads above its eye."""
    y0, sill, head, y1 = 8.5, 9.55, 10.9, 11.4
    x, zs, zn = 3.85, 17.95, 23.85
    cbox(-x, y0, zs, x, sill, 18.45, 'stone_dark', c=0.05)
    cbox(-x, head, zs, x, y1, 18.45, c=0.05)
    for s in (-1, 1):
        def bx(a, ya, za, b, yb, zb, mat='stone', c=0.05):
            lo, hi = sorted((s * a, s * b))
            cbox(lo, ya, za, hi, yb, zb, mat, c=c)
        bx(0.75, sill, zs, 1.75, head, 18.45)
        bx(3.0, sill, zs, x, head, 18.45, c=0.07)
        bx(3.35, y0, 18.45, x, sill, 23.4, 'stone_dark', c=0.05)
        bx(3.35, head, 18.45, x, y1, 23.4)
        bx(3.35, sill, 18.45, x, head, 20.4)
        bx(3.35, sill, 20.8, x, head, 21.8)
        bx(3.35, sill, 22.2, x, head, 23.4)
        bx(1.0, y0, 23.4, x, y1, zn, c=0.05)
        dial(s * 2.3, 10.1, 23.4, 0.72, -1)
        # A line of light in each window's head.
        lo, hi = sorted((s * 1.85, s * 2.9))
        strip_light(lo, 10.87, 18.17, hi, 10.9, 18.23, 'light_white')
        lo, hi = sorted((s * 3.45, s * 3.75))
        strip_light(lo, 10.87, 20.45, hi, 10.9, 20.75, 'light_white')
        strip_light(lo, 10.87, 21.85, hi, 10.9, 22.15, 'light_white')
    cbox(-1.0, head, 23.4, 1.0, y1, zn, c=0.05)
    strip_light(-0.65, 10.87, 18.17, 0.65, 10.9, 18.23, 'light_white')
    strip_light(-0.9, 10.87, 23.6, 0.9, 10.9, 23.66, 'light_white')
    # Its ceiling, and a bronze ring over the well.
    cbox(-x, 11.4, zs, x, 11.5, zn, 'stone_dark', c=0.0, cell=2.0)
    torus((0.0, 11.36, (WELL[1] + WELL[3]) / 2), 1.32, 0.035, (0, 1, 0), 'bronze', segs=48, sides=6)


def stairs():
    """Out of the room's side doors and up round the tower: a shoulder, a stair
    along each flank, a corner over the coast, a walk over the void along its
    back rising to the gallery's door. Each side's light runs under its stair."""
    for s, light in ((-1, 'light_red'), (1, 'light_blue')):
        x0, x1 = 4.0 * s, 5.7 * s
        lo, hi = sorted((x0, x1))
        # The shoulder, on corbels against the wall.
        cbox(lo, 4.0, 17.0, hi, 4.5, 20.0, c=0.04)
        for z in (17.6, 19.4):
            for reach, y in ((1.3, 3.5), (0.75, 3.0)):
                a, b = sorted((s * 4.0, s * (4.0 + reach)))
                cbox(a, y, z - 0.26, b, 4.0 if y == 3.5 else 3.5, z + 0.26, 'stone_dark', c=0.04)
        # The flank's stair: the sim's sweep, plain on top, and its soffit
        # carried on corbels that follow it up.
        mx = (x0 + x1) / 2
        half = abs(x1 - x0) / 2
        plank((mx, 20.0), (mx, 24.0), 4.5, 6.9, -half, half, 0.5)
        for z in (21.1, 22.2, 23.3):
            top = 4.0 + 0.6 * (z - 20.0)
            for reach, drop, h in ((1.3, 0.28, 0.5), (0.75, 0.74, 0.46)):
                a, b = sorted((s * 4.0, s * (4.0 + reach)))
                cbox(a, top - drop, z - 0.24, b, top - drop + h, z + 0.24, 'stone_dark', c=0.03)
        # A line of its side's light under the outer edge.
        out = s * 5.71
        def lit(z):
            return 4.5 + 0.6 * (z - 20.0) - 0.28
        quad((out, lit(20.1) - 0.03, 20.1), (out, lit(23.9) - 0.03, 23.9), (out, lit(23.9) + 0.03, 23.9), (out, lit(20.1) + 0.03, 20.1), light, (s, 0, 0), cell=30.0)
        # The corner over the coast, and its kerb.
        cbox(lo, 6.4, 24.0, hi, 6.9, 25.8, c=0.04)
        cbox(lo, 6.9, 25.6, hi, 7.5, 25.8, 'stone_dark', c=0.03)
        a, b = sorted((s * 5.5, s * 5.7))
        cbox(a, 6.9, 24.0, b, 7.5, 25.6, 'stone_dark', c=0.03)
        for cx in (s * 4.5, s * 5.3):
            for reach, drop, h in ((1.3, 0.5, 0.5), (0.65, 1.0, 0.55)):
                c0 = cx - 0.22
                c1 = cx + 0.22
                cbox(c0, 6.4 - drop, 24.0, c1, 6.4 - drop + h, 24.0 + reach, 'stone_dark', c=0.03)
        # The walk along the back, rising to the door: the sim's sweep.
        a0, a1 = (s * 4.0, 24.9), (s * 1.2, 24.9)
        plank(a0, a1, 6.9, 8.5, -0.9, 0.9, 0.5, ends=(False, False))
        # Its kerb on the outer side (+z), rising with it.
        kerb_off = (-0.9, -0.7) if s > 0 else (0.7, 0.9)
        plank(a0, a1, 7.5, 9.1, kerb_off[0], kerb_off[1], 0.6, 'stone_dark', ends=(False, False))
        for x in (2.0, 3.2):
            top = 6.9 + (4.0 - x) * 1.6 / 2.8 - 0.5
            for reach, drop in ((1.3, 0.5), (0.65, 1.0)):
                cbox(s * x - 0.2, top - drop, 24.0, s * x + 0.2, top - drop + 0.5, 24.0 + reach, 'stone_dark', c=0.03)
        quad((s * 3.9, 6.9 + 0.1 * 1.6 / 2.8 - 0.28, 25.805), (s * 1.3, 6.9 + 2.7 * 1.6 / 2.8 - 0.28, 25.805),
             (s * 1.3, 6.9 + 2.7 * 1.6 / 2.8 - 0.22, 25.805), (s * 3.9, 6.9 + 0.1 * 1.6 / 2.8 - 0.22, 25.805), light, (0, 0, 1), cell=30.0)
    # The landing at the door, and its kerb.
    cbox(-1.2, 8.0, 23.85, 1.2, 8.5, 25.8, c=0.04)
    cbox(-1.2, 8.5, 25.6, 1.2, 9.1, 25.8, 'stone_dark', c=0.03)
    for x in (-0.7, 0.7):
        for reach, drop in ((1.5, 0.5), (0.8, 1.0)):
            cbox(x - 0.22, 8.0 - drop, 23.85, x + 0.22, 8.0 - drop + 0.5, 23.85 + reach, 'stone_dark', c=0.03)


# --------------------------------------------------------------- the rim walks
def offset_chain(nodes, off, out):
    """Where a chain's edge runs, `off` across its way (toward `out`), mitred."""
    pts = []
    n = len(nodes)
    for i in range(n):
        p = nodes[max(i - 1, 0)]
        q = nodes[i]
        r = nodes[min(i + 1, n - 1)]
        d0 = Vector((q[0] - p[0], q[1] - p[1])) if i > 0 else Vector((r[0] - q[0], r[1] - q[1]))
        d1 = Vector((r[0] - q[0], r[1] - q[1])) if i < n - 1 else d0
        d0.normalize()
        d1.normalize()
        n0 = Vector((-d0.y, d0.x)) * out
        n1 = Vector((-d1.y, d1.x)) * out
        m = (n0 + n1).normalized()
        k = off / max(0.5, m.dot(n1))
        pts.append((q[0] + m.x * k, q[1] + m.y * k, q[2]))
    return pts


def ribbon(nodes, off0, off1, top, bottom, out, mat, sides=True, under=True, cell=1.0):
    """A strip along a chain between two offsets: its top `top(y)`, its underside
    `bottom(y)`, mitred at each bend. y is the chain's at that node."""
    a = offset_chain(nodes, off0, out)
    b = offset_chain(nodes, off1, out)
    for i in range(len(nodes) - 1):
        def v(p, f):
            return (p[0], f(p[2]), p[1])
        quad(v(a[i], top), v(b[i], top), v(b[i + 1], top), v(a[i + 1], top), mat, (0, 1, 0), cell)
        if under:
            quad(v(a[i], bottom), v(b[i], bottom), v(b[i + 1], bottom), v(a[i + 1], bottom), mat, (0, -1, 0), cell)
        if sides:
            mx, mz = (a[i][0] + a[i + 1][0]) / 2, (a[i][1] + a[i + 1][1]) / 2
            nx, nz = (b[i][0] + b[i + 1][0]) / 2, (b[i][1] + b[i + 1][1]) / 2
            quad(v(a[i], bottom), v(a[i + 1], bottom), v(a[i + 1], top), v(a[i], top), mat, (mx - nx, 0, mz - nz), cell)
            quad(v(b[i], bottom), v(b[i + 1], bottom), v(b[i + 1], top), v(b[i], top), mat, (nx - mx, 0, nz - mz), cell)
    # Its ends, cut square.
    for i, j in ((0, 1), (len(nodes) - 1, len(nodes) - 2)):
        def v(p, f):
            return (p[0], f(p[2]), p[1])
        o = (nodes[i][0] - nodes[j][0], 0, nodes[i][1] - nodes[j][1])
        quad(v(a[i], bottom), v(b[i], bottom), v(b[i], top), v(a[i], top), mat, o, cell)


def rim_walk(it, sx):
    """One `walkway` of map.json (the mirror image for sx = -1): its deck on
    corbels, a kerb of two courses, a line of its side's light along the kerb's
    foot, and a pilastered revetment under a coping laid over the lip's sand."""
    nodes = [(x * sx, z, y) for x, z, y in it['walkway']]
    out = it['out'] * sx
    w = it['width'] / 2
    thick = it['thick']
    kw, kh = it['kerb']
    cw, cy = it['wall']
    light = 'light_red' if sx > 0 else 'light_blue'
    foot = min(y for _, _, y in nodes) - thick

    # The deck runs back under the wall's set-back body, or cloud shows between.
    ribbon(nodes, -w - BACK - 0.03, w, lambda y: y, lambda y: y - thick, out, 'stone')
    # The kerb: a course, then a dark cap.
    ribbon(nodes, w - kw, w, lambda y: y + kh - 0.14, lambda y: y, out, 'stone', under=False)
    ribbon(nodes, w - kw, w, lambda y: y + kh, lambda y: y + kh - 0.14, out, 'stone_dark', under=False)
    # The light along its foot, on its inner face, where the walk is level.
    inner = offset_chain(nodes, w - kw - 0.006, out)
    for i in range(len(nodes) - 1):
        p, q = inner[i], inner[i + 1]
        if p[2] != q[2] or math.hypot(q[0] - p[0], q[1] - p[1]) < 1.2:
            continue
        quad((p[0], p[2] + 0.07, p[1]), (q[0], q[2] + 0.07, q[1]), (q[0], q[2] + 0.11, q[1]), (p[0], p[2] + 0.11, p[1]), light, (0, 1, 0), cell=30.0)
    # The revetment's body, set back; the coping over it, walked on, its top the
    # sim's and a hand over the sand.
    ribbon(nodes, -w - cw, -w - BACK, lambda y: cy - 0.2, lambda y: foot, out, 'stone', under=False)
    ribbon(nodes, -w - cw, -w, lambda y: cy, lambda y: cy - 0.2, out, 'stone_dark')
    # Along each leg: pilasters out to the wall's plane, corbels under the deck.
    for i in range(len(nodes) - 1):
        p, q = nodes[i], nodes[i + 1]
        dx, dz = q[0] - p[0], q[1] - p[1]
        ln = math.hypot(dx, dz)
        if ln < 0.6:
            continue
        ux, uz = dx / ln, dz / ln
        nx, nz = -uz * out, ux * out
        for k in range(int(ln / PILASTER_M) + 1):
            t = PILASTER_M * (k + 0.5)
            if t > ln:
                break
            y = p[2] + (q[2] - p[2]) * t / ln
            cx, cz = p[0] + ux * t + nx * (-w - BACK / 2), p[1] + uz * t + nz * (-w - BACK / 2)
            if cy - 0.2 - y > 0.15:
                tbox(cx, cz, ux, uz, 0.24, BACK / 2, y - 0.02, cy - 0.2, 'stone')
        for k in range(int(ln / CORBEL_M) + 1):
            t = CORBEL_M * (k + 0.5)
            if t > ln:
                break
            y = p[2] + (q[2] - p[2]) * t / ln - thick
            bx, bz = p[0] + ux * t, p[1] + uz * t
            # Steps of stone from the rock out to the deck's edge.
            for reach, drop, h in ((w * 2 - 0.1, 0.0, 0.42), (w * 1.35, 0.42, 0.5), (w * 0.8, 0.92, 0.62)):
                c = -w + reach / 2
                tbox(bx + nx * c, bz + nz * c, nx, nz, reach / 2, 0.26, y - drop - h, y - drop, 'stone_dark')


def rim_walks():
    with open(MAP) as f:
        spec = json.load(f)
    for st in spec['structures']:
        for it in st.get('boxes', []):
            if isinstance(it, dict) and 'walkway' in it:
                rim_walk(it, 1)
                if st.get('mirror'):
                    rim_walk(it, -1)


# ----------------------------------------------------------------- the tower
def tower():
    cz = 20.9
    # The gallery's cornice, up and out: free above reach.
    loft([
        ring8(3.85, 2.95, 0.14, cz, 11.4),
        ring8(3.9, 3.0, 0.2, cz, 11.55),
        ring8(4.0, 3.12, 0.3, cz, 11.95),
        ring8(4.0, 3.12, 0.3, cz, 12.2),
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

    hx2 = [(12.32, 3.42), (15.0, 3.3), (21.4, 2.86)]
    hz2 = [(12.32, 2.74), (15.0, 2.68), (21.4, 2.38)]
    for tier, (yy, hx, hz) in ((1, ((12.5, 21.3), hx2, hz2)),):
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
        for off in (-1.5, -0.86, 0.86, 1.5):
            rib(True, f, off, 12.6, 20.3, hx2, hz2, 0.11, 0.08)
            rib(False, f, off * 0.82, 12.6, 20.3, hx2, hz2, 0.11, 0.08)
    # String courses: the shaft's scale, read from the far side of the island.
    for y in (15.6, 18.0):
        hxs, hzs = lean(y, hx2), lean(y, hz2)
        ch = 0.9 + (y - 15.0) / 6.4 * 0.18
        loft([ring8(hxs, hzs, ch, cz, y), ring8(hxs + 0.13, hzs + 0.13, ch, cz, y + 0.08),
              ring8(hxs + 0.13, hzs + 0.13, ch, cz, y + 0.26), ring8(hxs, hzs, ch, cz, y + 0.34)], 'stone_dark', cap=False, cell=3.0)
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
    sighting_room()
    gallery()
    stairs()
    ramps()
    tower()
    rim_walks()
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
    # All the geometry, and the island's top and its rim, as one tree: what
    # hangs below the lip sees the sky past it.
    verts = []
    polys = []
    for o in objs.values():
        base = len(verts)
        verts.extend(tuple(v.co) for v in o.data.vertices)
        polys.extend([tuple(i + base for i in p.vertices) for p in o.data.polygons])
    base = len(verts)
    with open(MAP) as f:
        coast = next(sh['outline'] for sh in json.load(f)['terrain']['shapes'] if 'outline' in sh)
    verts.append(to_blender((0.0, 0.0, 0.0)))
    n = len(coast)
    for x, z in coast:
        verts.append(to_blender((x, 0.0, z)))
        verts.append(to_blender((x, -6.0, z)))
    for i in range(n):
        a, b = base + 1 + 2 * i, base + 1 + 2 * ((i + 1) % n)
        polys.append((base, a, b))
        polys.append((a, b, b + 1, a + 1))
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
            # Off the sand, under the lip, less of it reaches.
            dust *= max(0.4, min(1.0, 1.0 + y / 2.0))
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

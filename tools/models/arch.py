"""The kit a map's built things are made with, in the map's own coordinates:
flat faces cut into cells, boxes, lofts, rings, planks, walks; COLOR_0 traced in
Blender (occlusion times a tone); one GLB a material at the origin, its UVs in
metres over each material's tile. `observatory.py`'s and the arenas' generators
use it. Numbers for the game go in the client and the map's JSON.
"""
import math
import os
import random

import bpy
from mathutils import Vector
from mathutils.bvhtree import BVHTree

# A set's tile, m: what a material's UVs are in, TILES by name over this.
TILE_M = 4.0
TILES = {}
# How far a body sits behind the plane the sim collides, m: what its frames
# stand out of.
BACK = 0.14
# The longest side of a tessellated cell, m: AO and tone are per vertex.
CELL = 0.7
# AO: rays a vertex, how far they look, how dark full occlusion is.
AO_RAYS = 32
AO_REACH = 5.0
AO_FLOOR = 0.22
# Dust: how far up from the ground it reaches, m, and its tint.
DUST_M = 1.6
DUST_RGB = (1.12, 0.98, 0.78)

PARTS = {}
# Parts given no occlusion, as a light isn't: their COLOR_0 is theirs.
UNTRACED = {'shaft', 'water'}


class Part:
    """A mesh in the making: game coordinates (y up, right-handed)."""

    def __init__(self):
        self.verts = []
        self.faces = []  # (vertex indices, uvs, smooth)
        self.colors = {}  # vertex index: (r, g, b), painted here, not traced

    def add(self, pts):
        i = len(self.verts)
        self.verts.extend(pts)
        return i


def part(mat):
    return PARTS.setdefault(mat, Part())


def v3(p):
    return Vector((p[0], p[1], p[2]))


def box_uv(p, n, mat=None):
    """Metres along the face, in tiles: courses stay level on a wall."""
    t = TILES.get(mat, TILE_M)
    a = (abs(n.x), abs(n.y), abs(n.z))
    if a[1] >= a[0] and a[1] >= a[2]:
        return (p[0] / t, p[2] / t)
    if a[0] >= a[2]:
        return (p[2] / t, p[1] / t)
    return (p[0] / t, p[1] / t)


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
            pa.faces.append((idx, [box_uv(pa.verts[q], n, mat) for q in idx], False))


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
    pa.faces.append((idx, [box_uv(pa.verts[q], n, mat) for q in idx], False))


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


def panel(x0, y0, z0, x1, y1, z1, face, pier=0.3, band=0.22, mat='stone', plinth='stone_dark'):
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
        cbox(a, y0, z0 + pier, b, y0 + band, z1 - pier, plinth, c=0.02)
        cbox(a, y1 - band, z0 + pier, b, y1, z1 - pier, mat, c=0.02)
    else:
        bz0, bz1 = (z0, z1 - BACK) if up else (z0 + BACK, z1)
        a, b = (bz1, z1) if up else (z0, bz0)
        cbox(x0, y0, bz0, x1, y1, bz1, mat, c=0.02)
        cbox(x0, y0, a, x0 + pier, y1, b, mat, c=0.03)
        cbox(x1 - pier, y0, a, x1, y1, b, mat, c=0.03)
        cbox(x0 + pier, y0, a, x1 - pier, y0 + band, b, plinth, c=0.02)
        cbox(x0 + pier, y1 - band, a, x1 - pier, y1, b, mat, c=0.02)


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


def paint(objs, ground=None, tone=None):
    """COLOR_0: traced occlusion and the ground's dust, per vertex. `ground(verts,
    polys)` adds what stands for the ground to the tree; `tone(game_xyz, normal,
    name)` gives a vertex's colour over the occlusion, or the dust by height."""
    # All the geometry, and the ground, as one tree.
    verts = []
    polys = []
    for o in objs.values():
        base = len(verts)
        verts.extend(tuple(v.co) for v in o.data.vertices)
        polys.extend([tuple(i + base for i in p.vertices) for p in o.data.polygons])
    if ground:
        ground(verts, polys)
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
        lit = name.startswith('light_') or name in UNTRACED
        own = PARTS[name].colors if name in PARTS else {}
        for i, v in enumerate(me.vertices):
            n = normals[i].normalized() if normals[i].length > 1e-6 else Vector((0, 0, 1))
            if lit:
                c = own.get(i, (1, 1, 1))
                attr.data[i].color = (c[0], c[1], c[2], c[3] if len(c) > 3 else 1)
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
            if tone:
                d = tone((v.co.x, v.co.z, -v.co.y), n, name)
            else:
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


def export(objs, out, prefix, bare=()):
    """One GLB a material, `<out>/<prefix>_<name>.glb`. What's in `bare` is unlit:
    it needs no normals or tangents."""
    os.makedirs(out, exist_ok=True)
    total = 0
    for name, o in objs.items():
        bpy.ops.object.select_all(action='DESELECT')
        o.select_set(True)
        bpy.context.view_layer.objects.active = o
        path = os.path.join(out, f'{prefix}_{name}.glb')
        bpy.ops.export_scene.gltf(
            filepath=path,
            export_format='GLB',
            use_selection=True,
            export_yup=True,
            export_apply=False,
            export_normals=name not in bare,
            export_tangents=name not in bare,
            export_texcoords=True,
            export_materials='NONE',
            export_colors=True,
            export_animations=False,
        )
        tris = sum(len(p.vertices) - 2 for p in o.data.polygons)
        total += tris
        print(f'MODEL {prefix}_{name}: {tris} tris, {os.path.getsize(path) / 1e6:.2f} MB')
    print(f'MODEL {prefix}: {total} tris in all')


def spin(cx, cz, prof, mat, sides=24, smooth=True, arc=None, y=0.0):
    """A section spun about an upright axis, whole or from arc[0] to arc[1]
    radians. Its points (r, y) go the way a closed section goes: out along the
    bottom, up the outside, in along the top, down the inside; a face looks
    outward going up, up going in."""
    pa = part(mat)
    t = TILES.get(mat, TILE_M)
    whole = arc is None
    a0, a1 = arc if arc else (0.0, math.tau)
    cols = sides if whole else sides + 1
    base = pa.add([(cx + r * math.cos(a0 + (a1 - a0) * i / sides), yy + y, cz + r * math.sin(a0 + (a1 - a0) * i / sides))
                   for r, yy in prof for i in range(cols)])
    for j in range(len(prof) - 1):
        r = (prof[j][0] + prof[j + 1][0]) / 2
        step = (a1 - a0) * r / sides / t
        for i in range(sides):
            i1 = (i + 1) % cols
            a, b = base + j * cols + i, base + j * cols + i1
            c, d = base + (j + 1) * cols + i1, base + (j + 1) * cols + i
            u0, u1 = i * step, (i + 1) * step
            v0, v1 = (prof[j][1] + prof[j][0]) / t, (prof[j + 1][1] + prof[j + 1][0]) / t
            pa.faces.append(((a, d, c, b), [(u0, v0), (u0, v1), (u1, v1), (u1, v0)], smooth))


def surface(grid, mat, out, uv, smooth=True):
    """A sheet of rows of points (each row one long), each cell turned to face
    `out`; `uv(p)` gives a point's map, in metres."""
    pa = part(mat)
    t = TILES.get(mat, TILE_M)
    rows, cols = len(grid), len(grid[0])
    base = pa.add([p for row in grid for p in row])
    o = Vector(out)
    for j in range(rows - 1):
        for i in range(cols - 1):
            a, b = base + j * cols + i, base + j * cols + i + 1
            c, d = base + (j + 1) * cols + i + 1, base + (j + 1) * cols + i
            pa_, pb_, pd_ = v3(pa.verts[a]), v3(pa.verts[b]), v3(pa.verts[d])
            n = (pb_ - pa_).cross(pd_ - pa_)
            idx = (a, b, c, d) if n.dot(o) >= 0 else (b, a, d, c)
            pa.faces.append((idx, [tuple(x / t for x in uv(pa.verts[q])) for q in idx], smooth))


def mirror_parts(objs, parts, plane, keep=None, dim=None, stretch=1.0):
    """The architecture over a water line again under it, its faces turned to
    match, for a reflection: {made: [source, ...]}. A face looking up is never
    seen upside down, so it's left out, as is what `keep((x, y, z))` refuses at a
    face's middle; `dim[source]` scales a source's COLOR_0, and `stretch` draws it
    longer than it stands, as a wet road draws a lamp."""
    made = {}
    for name, sources in parts.items():
        verts, polys, uvs, cols = [], [], [], []
        for src in sources:
            o = objs.get(src)
            if o is None:
                continue
            me = o.data
            attr = me.color_attributes[0]
            uv = me.uv_layers[0]
            k = (dim or {}).get(src, 1.0)
            for poly in me.polygons:
                if poly.normal.z > 0.5:
                    continue
                c = poly.center
                if c.z < plane or (keep and not keep((c.x, c.z, -c.y))):
                    continue
                base = len(verts)
                loops = list(poly.loop_indices)[::-1]
                for li in loops:
                    v = me.loops[li].vertex_index
                    p = me.vertices[v].co
                    verts.append((p.x, p.y, plane - (p.z - plane) * stretch))
                    uvs.append(tuple(uv.data[li].uv))
                    col = attr.data[v].color
                    cols.append((col[0] * k, col[1] * k, col[2] * k, 1.0))
                polys.append(tuple(range(base, base + len(loops))))
        if not polys:
            continue
        me = bpy.data.meshes.new(name)
        me.from_pydata(verts, [], polys)
        me.update()
        layer = me.uv_layers.new(name='UVMap')
        for i, u in enumerate(uvs):
            layer.data[i].uv = u
        attr = me.color_attributes.new(name='Col', type='FLOAT_COLOR', domain='POINT')
        for i, c in enumerate(cols):
            attr.data[i].color = c
        o = bpy.data.objects.new(name, me)
        bpy.context.scene.collection.objects.link(o)
        made[name] = o
    return made


def extrude(a, b, section, mat, caps=True, cell=1.2, ya=0.0, yb=0.0):
    """A closed section (s across the way, y) carried straight from a to b (x, z),
    its y lifted by ya at a and yb at b. Its points go round counter-clockwise
    looking along the way, the faces out; `s` is along (-dz, dx)."""
    dx, dz = b[0] - a[0], b[1] - a[1]
    ln = math.hypot(dx, dz)
    nx, nz = -dz / ln, dx / ln
    def at(end, p):
        e, lift = (a, ya) if end == 0 else (b, yb)
        return (e[0] + nx * p[0], p[1] + lift, e[1] + nz * p[0])
    cx = sum(p[0] for p in section) / len(section)
    cy = sum(p[1] for p in section) / len(section)
    n = len(section)
    for i in range(n):
        p, q = section[i], section[(i + 1) % n]
        mid = ((p[0] + q[0]) / 2 - cx, (p[1] + q[1]) / 2 - cy)
        quad(at(0, p), at(0, q), at(1, q), at(1, p), mat, (nx * mid[0], mid[1], nz * mid[0]), cell)
    if caps:
        for end, s in ((0, -1), (1, 1)):
            c = at(end, (cx, cy))
            for i in range(n):
                tri(at(end, section[i]), at(end, section[(i + 1) % n]), c, mat, (dx * s, 0, dz * s))


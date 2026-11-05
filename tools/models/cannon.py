"""Veil cannon: a thick tube out of a bulb, a ring of core light near the
muzzle and a slit of it down the tube's back, dark bands, a shoulder rest
curling under the rear, a front loop and the main loop."""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

scene()


# -- the Veil helpers (the same block in every ship gun) --------------------
def halves(points, w, material='shell', levels=2, bevel=0.006, tap=None):
    """A clamshell body: one half's shell off-centre, mirrored across y = 0."""
    h = shell(points, w / 2, at=w / 4, material=material, levels=levels, bevel=bevel)
    if tap:
        taper(h, 'x', *tap)
    return mirror_y(h)


def seam(points, w=0.012, material='shell_dark', tap=None, kz=0.94, dz=0.0):
    """The dark strip in the groove between the halves (top and bottom)."""
    pts = [(x, z * kz + dz) for x, z in points]
    s = shell(pts, w, at=0, material=material, levels=1, bevel=0.003)
    if tap:
        taper(s, 'x', *tap)
    return s


def surf(body, origin, direction):
    """Where a ray from `origin` along `direction` meets `body` (modifiers
    applied): the point and the normal, or None."""
    dg = bpy.context.evaluated_depsgraph_get()
    ev = body.evaluated_get(dg)
    mi = body.matrix_world.inverted()
    ok, loc, nrm, _ = ev.ray_cast(mi @ Vector(origin), (mi.to_3x3() @ Vector(direction)).normalized())
    if not ok:
        return None
    return body.matrix_world @ loc, (body.matrix_world.to_3x3() @ nrm).normalized()


def slit(body, origin, direction, length, tall=0.008, thick=0.003, material='cyan', proud=0.5, along=(1, 0, 0)):
    """A slit of light: a thin box laid on the surface the ray finds, sunk
    half in (proud 0.5 is half out)."""
    hit = surf(body, origin, direction)
    if hit is None:
        print('SLIT MISS', origin)
        return None
    p, n = hit
    x = Vector(along)
    x = (x - n * x.dot(n)).normalized()
    z = x.cross(n)
    o = box(length, thick, tall, material=material, bevel=0.0006, seg=1)
    o.matrix_world = Matrix.Translation(p + n * (thick * (proud - 0.5))) @ Matrix((x, n, z)).transposed().to_4x4()
    return o


def _mesh(bm, name, material):
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(o)
    me.materials.append(mat(material))
    return o


def ribbon(body, origins, direction, width=0.005, thick=0.003, material='cyan', proud=0.5):
    """A strip of light that follows the surface: a ray at each origin, the
    hits bridged (bmesh); for the long grooves a straight box can't lie in."""
    hits = [h for h in (surf(body, p, direction) for p in origins) if h]
    bm = bmesh.new()
    rings = []
    for i, (p, n) in enumerate(hits):
        t = (hits[min(i + 1, len(hits) - 1)][0] - hits[max(i - 1, 0)][0]).normalized()
        b = t.cross(n).normalized()
        c = p + n * (thick * (proud - 0.5))
        rings.append([bm.verts.new(c + b * width / 2 + n * thick / 2), bm.verts.new(c - b * width / 2 + n * thick / 2),
                      bm.verts.new(c - b * width / 2 - n * thick / 2), bm.verts.new(c + b * width / 2 - n * thick / 2)])
    for r0, r1 in zip(rings, rings[1:]):
        for k in range(4):
            bm.faces.new((r0[k], r0[(k + 1) % 4], r1[(k + 1) % 4], r1[k]))
    bm.faces.new(rings[0][::-1])
    bm.faces.new(rings[-1])
    return smooth(_mesh(bm, 'ribbon', material), 60)


def sweep(path, w, h, material='shell', levels=1, taper_to=1.0, cut=0.3, name='sweep'):
    """A smooth tube swept along `path` ((x, z) points in the centre plane):
    an octagon section `w` wide (y) by `h` tall, scaled to `taper_to` at the
    far end, bridged and subdivided (bmesh). Loops, horns, claws, tails."""
    pts = [Vector((x, 0.0, z)) for x, z in path]
    n = len(pts)
    bm = bmesh.new()
    rings = []
    for i, p in enumerate(pts):
        t = (pts[min(i + 1, n - 1)] - pts[max(i - 1, 0)]).normalized()
        nv = Vector((-t.z, 0.0, t.x))
        k = 1.0 + (taper_to - 1.0) * (i / (n - 1))
        hw, hh, c = w / 2 * k, h / 2 * k, 1 - cut
        sec = [(hw, -hh * c), (hw, hh * c), (hw * c, hh), (-hw * c, hh), (-hw, hh * c), (-hw, -hh * c), (-hw * c, -hh), (hw * c, -hh)]
        rings.append([bm.verts.new(p + Vector((0, u, 0)) + nv * v) for u, v in sec])
    for r0, r1 in zip(rings, rings[1:]):
        for k in range(8):
            bm.faces.new((r0[k], r0[(k + 1) % 8], r1[(k + 1) % 8], r1[k]))
    bm.faces.new(rings[0][::-1])
    bm.faces.new(rings[-1])
    o = _mesh(bm, name, material)
    return subsurf(o, levels)


def arc(cx, cz, r, a0, a1, n=5):
    """Points on an arc about (cx, cz), from angle a0 to a1 (degrees)."""
    return [(cx + r * math.cos(math.radians(a0 + (a1 - a0) * i / (n - 1))),
             cz + r * math.sin(math.radians(a0 + (a1 - a0) * i / (n - 1)))) for i in range(n)]


def loop_grip(x0, x1, z_top, z_bot, w=0.030, h=0.028, material='shell'):
    """A loop grip under the body: a U from the rear leg at `x0` to the front
    leg at `x1`, the legs rising into the body to `z_top`."""
    r = h * 1.1
    path = [(x0, z_top), (x0, z_bot + r)] + arc(x0 + r, z_bot + r, r, 180, 270, 4)[1:] + \
        arc(x1 - r, z_bot + r, r, 270, 360, 4)[1:] + [(x1, z_bot + r), (x1, z_top)]
    return sweep(path, w, h, material=material, levels=2, name='grip')


# -- the cannon --------------------------------------------------------------
BODY = [(0.00, -0.050), (0.06, -0.095), (0.18, -0.118), (0.32, -0.115), (0.44, -0.100), (0.52, -0.085),
        (0.52, 0.085), (0.44, 0.100), (0.32, 0.118), (0.18, 0.120), (0.06, 0.095), (0.00, 0.050)]
TAP = (0.26, 0.0, 0.76)  # the bulb rounds off toward the rear
parts = []
body = halves(BODY, 0.220, tap=TAP)
parts.append(body)
top_seam = seam(BODY, tap=TAP)
parts.append(top_seam)

# The tube, narrowing a little, three dark bands, the dark muzzle ring and the bore with the core's glow.
tube_ = cyl(0.092, 0.78, 'x', at=(0.85, 0, 0), r2=0.084, material='shell', verts=40, bevel=0.006, seg=3)
parts.append(tube_)
for x in (0.62, 0.76, 0.90):
    r = 0.092 + (0.084 - 0.092) * (x - 0.46) / 0.78
    parts.append(cyl(r + 0.002, 0.030, 'x', at=(x, 0, 0), material='shell_dark', verts=40, bevel=0.001))
parts.append(cyl(0.088, 0.050, 'x', at=(1.225, 0, 0), material='shell_dark', verts=40, bevel=0.003))
parts.append(cyl(0.070, 0.14, 'x', at=(1.18, 0, 0), material='shell_dark', verts=32, bevel=0))
parts.append(cyl(0.050, 0.016, 'x', at=(1.242, 0, 0), material='core', verts=32, bevel=0))

# The core's ring near the muzzle and its slit down the tube's back.
parts.append(torus(0.098, 0.011, axis='x', at=(1.08, 0, 0), material='core'))
parts.append(ribbon(tube_, [(0.50 + 0.50 * i / 24, 0, 0.3) for i in range(25)], (0, 0, -1), width=0.009, material='core'))

# The shoulder rest: a dark saddle pad under the bulb's rear.
PAD = [(0.02, -0.190), (0.08, -0.205), (0.18, -0.200), (0.25, -0.170), (0.26, -0.130), (0.22, -0.090), (0.06, -0.080), (0.00, -0.120)]
parts.append(shell(PAD, 0.110, at=0, material='shell_dark', levels=1, bevel=0.010))

# The main loop and the front loop under the tube, the blade in the main one.
parts.append(loop_grip(0.55, 0.67, -0.070, -0.205, w=0.038, h=0.036))
parts.append(box(0.006, 0.012, 0.048, at=(0.645, 0, -0.120), material='iron', bevel=0.001))
parts.append(loop_grip(0.84, 0.94, -0.060, -0.180, w=0.036, h=0.034))

# Three slits a side on the bulb.
for x in (0.10, 0.20, 0.30):
    for sy in (1, -1):
        s = slit(body, (x, sy * 0.3, 0.020), (0, -sy, 0), 0.08, tall=0.010)
        if s:
            parts.append(s)

finish('cannon', parts, grip=0.55, long=True, note='Veil cannon: bulb and tube, a ring and a slit of core light')

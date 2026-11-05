"""Veil sword: an iron hilt in shell plates with a knuckle bow, and from its
front two curved blades of cyan light diverging up and down like a tuning
fork. +X is the blade's direction."""
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


# -- the sword ---------------------------------------------------------------
HILT = [(0.02, -0.025), (0.06, -0.034), (0.15, -0.036), (0.24, -0.040), (0.29, -0.052),
        (0.29, 0.052), (0.24, 0.040), (0.15, 0.036), (0.06, 0.034), (0.02, 0.025)]
parts = []
hilt = halves(HILT, 0.056)
parts.append(hilt)
parts.append(seam(HILT))
parts.append(cyl(0.018, 0.31, 'x', at=(0.155, 0, 0), material='iron', verts=24, bevel=0.002))
parts.append(cyl(0.024, 0.02, 'x', at=(0.0, 0, 0), material='iron', verts=24, bevel=0.003))

# The knuckle bow under the hand, and the emitter disc the blades leave from.
parts.append(sweep(arc(0.15, -0.045, 0.11, 0, -180, 9), 0.022, 0.018, material='shell_light', levels=1, name='bow'))
parts.append(cyl(0.052, 0.024, 'x', at=(0.305, 0, 0), material='shell_dark', verts=32, bevel=0.004))
parts.append(cyl(0.030, 0.030, 'x', at=(0.318, 0, 0), material='cyan_dim', verts=24, bevel=0.002))

# The blades: two curved slivers of light, 4 mm thick, diverging from the emitter.
BLADE = [(0.31, 0.012), (0.50, 0.030), (0.70, 0.058), (0.88, 0.085), (1.00, 0.102),
         (0.97, 0.122), (0.85, 0.126), (0.66, 0.108), (0.47, 0.078), (0.31, 0.052)]
parts.append(profile(BLADE, 0.004, plane='xz', at=0, material='cyan', bevel=0.001))
parts.append(profile([(x, -z) for x, z in BLADE], 0.004, plane='xz', at=0, material='cyan', bevel=0.001))

# Two slits a side on the hilt.
for x in (0.09, 0.17):
    for sy in (1, -1):
        s = slit(hilt, (x, sy * 0.3, 0.0), (0, -sy, 0), 0.04, tall=0.006)
        if s:
            parts.append(s)

finish('sword', parts, grip=0.12, long=False, note='Veil sword: iron hilt in shell plates, two blades of light')

"""Veil beam rifle: a long slim clamshell with a crescent tail curling under
its rear, one long slit of light down the top seam, a forked muzzle, a scope
bump, a loop grip."""
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


# -- the beam rifle ----------------------------------------------------------
BODY = [(0.10, -0.030), (0.25, -0.045), (0.45, -0.048), (0.70, -0.040), (0.95, -0.028), (1.06, -0.020),
        (1.06, 0.020), (0.95, 0.034), (0.70, 0.050), (0.45, 0.056), (0.25, 0.050), (0.10, 0.034)]
TAP = (0.60, 1.06, 0.60)
parts = []
body = halves(BODY, 0.064, tap=TAP)
parts.append(body)
top_seam = seam(BODY, tap=TAP)
parts.append(top_seam)

# The crescent tail: a horn that sweeps back and down from the rear and curls forward, thinning to a point.
parts.append(sweep(arc(0.155, -0.13, 0.14, 95, 228, 12), 0.056, 0.076, material='shell', levels=1, taper_to=0.14, name='tail'))

# The forked muzzle: two slim prongs, above and below the bore, and the emitter bead between their roots.
PRONG = [(1.02, 0.010), (1.30, 0.030), (1.30, 0.036), (1.26, 0.044), (1.02, 0.046)]
parts.append(profile(PRONG, 0.014, plane='xz', at=0, material='shell_light', bevel=0.003))
parts.append(profile([(x, -z) for x, z in PRONG], 0.014, plane='xz', at=0, material='shell_light', bevel=0.003))
parts.append(box(0.05, 0.014, 0.014, at=(1.075, 0, 0), material='cyan', bevel=0.003))

# The scope bump on the seam behind the slit, a dim lens at its rear.
parts.append(sphere(0.030, at=(0.56, 0, 0.052), material='shell', scale=(1.9, 0.75, 0.8), segments=24))
parts.append(cyl(0.009, 0.012, 'x', at=(0.505, 0, 0.058), material='cyan_dim', verts=16, bevel=0))

# The loop grip and its blade.
parts.append(loop_grip(0.45, 0.565, -0.020, -0.130, w=0.034, h=0.032))
parts.append(box(0.006, 0.012, 0.042, at=(0.545, 0, -0.072), material='iron', bevel=0.001))

# The long slit of light along the top seam, either side of the scope bump; two dim vents a side at the rear.
for a, b in ((0.17, 0.49), (0.63, 1.00)):
    parts.append(ribbon(top_seam, [(a + (b - a) * i / 24, 0, 0.3) for i in range(25)], (0, 0, -1), width=0.006))
for x in (0.24, 0.31):
    for sy in (1, -1):
        s = slit(body, (x, sy * 0.3, 0.0), (0, -sy, 0), 0.04, tall=0.006, material='cyan_dim')
        if s:
            parts.append(s)

finish('beam-rifle', parts, grip=0.45, long=True, note='Veil beam rifle: crescent tail, forked muzzle, a slit down the spine')

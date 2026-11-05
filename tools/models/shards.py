"""Veil shards: a bulbous clamshell on a bone keel, a crest of magenta
crystals leaning back along its spine (the tallest at the front), two slits a
side, a loop grip, a short dark muzzle glowing magenta."""
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


# -- the shards --------------------------------------------------------------
BODY = [(0.00, -0.030), (0.05, -0.058), (0.16, -0.072), (0.30, -0.066), (0.42, -0.048), (0.50, -0.025),
        (0.50, 0.025), (0.42, 0.060), (0.30, 0.084), (0.16, 0.086), (0.05, 0.064), (0.00, 0.034)]
TAP = (0.28, 0.50, 0.55)
parts = []
body = halves(BODY, 0.130, tap=TAP)
parts.append(body)
top_seam = seam(BODY, tap=TAP)
parts.append(top_seam)

# The bone keel: a wide flat sweep along the belly line, half in the body.
keel = sweep([(0.05, -0.056), (0.10, -0.066), (0.16, -0.071), (0.23, -0.071), (0.30, -0.065), (0.37, -0.057), (0.43, -0.044), (0.47, -0.029)],
             0.100, 0.026, material='bone', levels=1, cut=0.4, name='keel')
taper(keel, 'x', *TAP)
parts.append(keel)

# The crest: eight crystals in the top seam, leaning back, the tallest at the front.
for i in range(8):
    x = 0.10 + 0.04 * i
    h = 0.050 + 0.060 * i / 7
    zt = 0.062 + 0.022 * (1 - abs(x - 0.23) / 0.17) ** 0.7
    c = cyl(0.010, h, 'z', at=(0, 0, h / 2 - 0.012), r2=0.0, material='magenta', verts=6, bevel=0)
    rotate(c, ry=-30)
    parts.append(move(c, dx=x, dz=zt))

# The muzzle: a short dark cone with a magenta core.
parts.append(cyl(0.027, 0.11, 'x', at=(0.52, 0, 0), r2=0.018, material='shell_dark', verts=24))
parts.append(cyl(0.008, 0.115, 'x', at=(0.52, 0, 0), material='magenta', verts=12, bevel=0))

# The loop grip and its blade.
parts.append(loop_grip(0.235, 0.355, -0.040, -0.150, w=0.036, h=0.034))
parts.append(box(0.006, 0.012, 0.045, at=(0.330, 0, -0.088), material='iron', bevel=0.001))

# Two slits a side.
for x in (0.11, 0.21):
    for sy in (1, -1):
        s = slit(body, (x, sy * 0.3, 0.010), (0, -sy, 0), 0.06)
        if s:
            parts.append(s)

finish('shards', parts, grip=0.235, long=True, note='Veil shards: bone keel, a crest of magenta crystals')

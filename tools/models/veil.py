"""The Veil's forms, grown by rule: what a one-seater is built of when it is
made here and not commissioned (skimmer.py, flier.py, on craft.py's kit).

    curve     a smooth line through points, as a function: a hull's top line,
              a wing's leading edge
    oval      a hull's cross-section; foil, a wing's
    loft      a smooth skin over a run of them
    seam      a groove cut into a skin where a rule is zero (a plane, a
              cylinder, any surface), its floor dark iron or a line of light:
              plates are what the seams leave between them
    shade     the shade each corner stands in, traced, with each plate's own
              tone and the pale blue of a worn edge, kept in the mesh's
              colours (COLOR_0): a craft so made carries no map at all

Hull space throughout (craft.py): +X forward, +Y the craft's left, +Z up,
metres, the origin on the ground under the hull's middle.
"""
import random

from mathutils.bvhtree import BVHTree

from lib import *
from lib import _bevel

TAU = math.tau

# The Veil's paint, by what it is: (sRGB hex, metallic, roughness, emissive
# (hex, strength) or None, the weight of a clear coat over it). A shell is barely metal (under an open sky a
# metal takes the sky's colour and reads white); its colour is the middle of
# what `shade` then turns each plate about.
PAINTS = {
    'shell': ('#5a2396', 0.35, 0.27, None, 0.0),
    'shell_deep': ('#34185c', 0.35, 0.3, None, 0.0),
    'shell_pale': ('#8f62c9', 0.30, 0.27, None, 0.0),
    'iron': ('#1c1b24', 0.75, 0.50, None, 0.0),
    'glass': ('#07070d', 0.60, 0.07, None, 0.0),
    'pad': ('#141218', 0.00, 0.85, None, 0.0),
    'light': ('#06232a', 0.00, 0.30, ('#4ff2ff', 6.0), 0.0),
    'light_dim': ('#06232a', 0.00, 0.30, ('#4ff2ff', 2.2), 0.0),
    'throat': ('#0a2a33', 0.00, 0.40, ('#7df6ff', 9.0), 0.0),
    'glow': ('#06232a', 0.00, 0.50, ('#3fd0e8', 0.9), 0.0),
    'vein': ('#06232a', 0.00, 0.40, ('#4ff2ff', 2.5), 0.0),
    'ember': ('#2a0620', 0.00, 0.30, ('#ff4fd8', 5.0), 0.0),
}
# Paints that take a plate's tone and a worn edge (`shade`); the rest take
# the traced shade alone, and a light none of it.
PLATED = ('shell', 'shell_deep', 'shell_pale')
LIT = ('light', 'light_dim', 'throat', 'glow', 'vein', 'ember')

_paints = {}


def skin(name):
    """One of `PAINTS` as a material. A light is its own colour; any other
    is drawn in the mesh's colours (COLOR_0, which `shade` fills with the
    paint's colour and the shade it stands in: what the exporter writes and
    the game multiplies a white paint by)."""
    if name in _paints and _paints[name].name in bpy.data.materials:
        return _paints[name]
    hexs, metallic, rough, emis, coat = PAINTS[name]
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    m.use_backface_culling = True
    nt = m.node_tree
    b = nt.nodes['Principled BSDF']
    b.inputs['Metallic'].default_value = metallic
    b.inputs['Roughness'].default_value = rough
    if coat:
        b.inputs['Coat Weight'].default_value = coat
        b.inputs['Coat Roughness'].default_value = 0.08
    if emis:
        b.inputs['Base Color'].default_value = (*srgb(hexs), 1.0)
        b.inputs['Emission Color'].default_value = (*srgb(emis[0]), 1.0)
        b.inputs['Emission Strength'].default_value = emis[1]
    else:
        col = nt.nodes.new('ShaderNodeVertexColor')
        col.layer_name = 'Col'
        nt.links.new(col.outputs['Color'], b.inputs['Base Color'])
    _paints[name] = m
    return m


def curve(*pts):
    """A smooth line through `pts` ((x, value) pairs, x rising) as a
    function of x that never overshoots them (held at its ends outside
    them): a top line, a width, a wing's edge."""
    xs = np.array([p[0] for p in pts], dtype=float)
    ys = np.array([p[1] for p in pts], dtype=float)
    h = np.diff(xs)
    d = np.diff(ys) / h
    m = np.zeros_like(xs)
    for i in range(1, len(xs) - 1):
        if d[i - 1] * d[i] > 0:
            w1, w2 = 2 * h[i] + h[i - 1], h[i] + 2 * h[i - 1]
            m[i] = (w1 + w2) / (w1 / d[i - 1] + w2 / d[i])
    m[0], m[-1] = d[0], d[-1]

    def f(x):
        x = np.clip(np.asarray(x, dtype=float), xs[0], xs[-1])
        i = np.clip(np.searchsorted(xs, x) - 1, 0, len(xs) - 2)
        t = (x - xs[i]) / h[i]
        out = ((2 * t ** 3 - 3 * t ** 2 + 1) * ys[i] + (t ** 3 - 2 * t ** 2 + t) * h[i] * m[i]
               + (-2 * t ** 3 + 3 * t ** 2) * ys[i + 1] + (t ** 3 - t ** 2) * h[i] * m[i + 1])
        return float(out) if out.ndim == 0 else out

    return f


def _pow(v, e):
    return math.copysign(abs(v) ** e, v)


def oval(x, y, z, w, up, dn=None, n=16, p=2.4, pb=None):
    """A hull's cross-section at `x`: `n` points round (`y`, `z`), `w` to
    each side, `up` over it and `dn` under. `p` squares it (2 an ellipse, 4
    nearly a box, under 2 a diamond), `pb` its underside alone."""
    dn = up if dn is None else dn
    pb = p if pb is None else pb
    out = []
    for i in range(n):
        c, s = math.cos(TAU * i / n), math.sin(TAU * i / n)
        e = p if s >= 0 else pb
        out.append(Vector((x, y + w * _pow(c, 2 / e), z + (up if s >= 0 else dn) * _pow(s, 2 / e))))
    return out


def foil(y, le, te, z, t, n=12, under=0.7, tilt=0.0, upright=False):
    """A wing's section at span `y`: from its leading edge at x = `le` to
    its trailing edge at `te`, `t` thick at its thickest (a third back),
    round ahead and sharp behind, its underside `under` as full as its top;
    `tilt` raises its leading edge by that much. `upright`: a fin's, at
    height `z` and standing at `y`, as thick across."""
    out = []
    for i in range(n):
        a = TAU * i / n
        u = (1 - math.cos(a)) / 2
        s = math.sin(a)
        thick = t * 1.3 * math.sqrt(max(u, 0.0)) * (1 - u) * (1.0 if s >= 0 else -under) * (1 if abs(s) > 1e-9 else 0)
        x = le + (te - le) * u
        out.append(Vector((x, y + thick, z)) if upright else Vector((x, y, z + tilt * (1 - u) + thick)))
    return out


def loft(rings, material='shell', ends=('tip', 'tip'), levels=2, name='loft', sharp=(), inside=False):
    """A smooth skin over `rings` (each the same count of points): quads
    from each ring to the next, an end closed on its middle (`tip`), by one
    face (`flat`) or left `open`; then subdivided `levels` times. `sharp`
    names the points of a ring (their index) whose line along the skin stays
    a crease: a chine, a trailing edge; `(index, first, last)` creases it
    from ring `first` to ring `last` alone. `inside`: it is seen from within
    (a throat, a well)."""
    bm = bmesh.new()
    vs = [[bm.verts.new(p) for p in r] for r in rings]
    n = len(rings[0])
    for a, b in zip(vs, vs[1:]):
        for i in range(n):
            bm.faces.new((a[i], a[(i + 1) % n], b[(i + 1) % n], b[i]))
    for ring, end in ((vs[0], ends[0]), (vs[-1], ends[1])):
        if end == 'tip':
            c = bm.verts.new(sum((v.co for v in ring), Vector()) / n)
            for i in range(n):
                bm.faces.new((ring[i], ring[(i + 1) % n], c))
        elif end == 'flat':
            bm.faces.new(ring)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    if inside:
        bmesh.ops.reverse_faces(bm, faces=bm.faces)
    if sharp:
        layer = bm.edges.layers.float.get('crease_edge') or bm.edges.layers.float.new('crease_edge')
        for k, (a, b) in enumerate(zip(vs, vs[1:])):
            for i in sharp:
                i, first, last = (i, 0, len(vs)) if isinstance(i, int) else i
                if not first <= k < last:
                    continue
                e = bm.edges.get((a[i % n], b[i % n]))
                if e:
                    e[layer] = 1.0
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    o = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(o)
    me.materials.append(skin(material))
    o['veil_skin'] = True
    if levels:
        m = o.modifiers.new('subsurf', 'SUBSURF')
        m.levels = m.render_levels = levels
    return smooth(o, 40)


def bow(a, b, c, steps=8):
    """Points along a bow from `a` to `c`, drawn toward `b` on the way."""
    a, b, c = Vector(a), Vector(b), Vector(c)
    return [(1 - t) ** 2 * a + 2 * t * (1 - t) * b + t ** 2 * c for t in (i / steps for i in range(steps + 1))]


def stem(points, radii, n=10, material='shell', name='stem', levels=1, ends=('tip', 'tip'), flat=1.0, thin=None):
    """A round limb along `points`, `radii` thick at each (one number for
    all): rings square to its run, skinned. `flat` squeezes it one way (a
    blade under 1): along `thin` if given (the way it is thin: up, for a
    feather lying flat), or across its first bend's plane."""
    pts = [Vector(p) for p in points]
    radii = [radii] * len(pts) if isinstance(radii, (int, float)) else radii
    rings = []
    side = None
    for i, p in enumerate(pts):
        t = (pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]).normalized()
        if thin is not None:
            up = Vector(thin) - t * Vector(thin).dot(t)
            side = up.normalized().cross(t)
        else:
            side = t.orthogonal().normalized() if side is None else (side - t * side.dot(t)).normalized()
        up = t.cross(side)
        rings.append([p + (side * math.cos(TAU * k / n) + up * math.sin(TAU * k / n) * flat) * radii[i] for k in range(n)])
    return loft(rings, material, ends=ends, levels=levels, name=name)


def lip(at, w, h, n=20, p=2.6):
    """The two rings that turn a skin's open end in on itself to be the rim
    of an engine's mouth (`mouth`) at `at`, the middle of the end, `w` to
    each side and `h` over and under: the first rings of a skin that runs
    forward from there."""
    x, y, z = at
    return [oval(x + 0.1, y, z, w * 0.96, h * 0.96, n=n, p=p), oval(x - 0.03, y, z, w, h, n=n, p=p)]


def mouth(at, w, h, deep=0.26, core=(0.17, 0.14, 0.8, 0.72), vanes=2, n=20, p=2.6, lining='glow', wall=None):
    """An engine's mouth looking aft from `at` (`lip`): a throat `deep`
    metres in, lined with `lining` (lit from within, or dark iron round a
    core that is the only light in it), or with `wall` down to a floor of
    `lining` (a dark throat with light at the bottom of it: a lit wall is a
    lamp's lens from astern); its core, a swell of light whose middle is
    `core[0]` in from the end, `core[1]` long, and `core[2]` and `core[3]` of
    the mouth across and up; thin iron vanes across it. Its parts."""
    x, y, z = at
    rim, foot = oval(x, y, z, w * 0.98, h * 0.98, n=n, p=p), oval(x + deep, y, z, w * 0.8, h * 0.8, n=n, p=p)
    if wall is None:
        parts = [loft([rim, foot], lining, ends=('open', 'flat'), levels=1, name='throat', inside=True)]
    else:
        parts = [loft([rim, foot], wall, ends=('open', 'open'), levels=1, name='throat', inside=True),
                 loft([oval(x + deep - 0.004, y, z, w * 0.81, h * 0.81, n=n, p=p), foot], lining, ends=('open', 'flat'), levels=1,
                      name='floor', inside=True)]
    swell = sphere(1.0, at=(x + core[0], y, z), material=None, scale=(core[1], w * core[2], h * core[3]), segments=24)
    swell.data.materials.append(skin('throat'))
    parts.append(swell)
    for i in range(vanes):
        vane = box(0.05, w * 1.9, 0.014, at=(x + 0.005, y, z + h * 1.3 * ((i + 1) / (vanes + 1) - 0.5)), material=None,
                   bevel=0.003)
        vane.data.materials.append(skin('iron'))
        parts.append(vane)
    return parts


def lens(at, a, b, rise, paint='light_dim', rim='iron', down=True, vanes=0):
    """A lens of light let into a skin: an oval dome at `at`, `a` along the
    craft and `b` across it, standing `rise` out of the skin (downward, or
    up), in a rim of dark iron a little wider, `vanes` thin iron vanes
    across it. A lift's plate under a hull; a port. Its parts."""
    x, y, z = at
    k = -1.0 if down else 1.0
    glass = sphere(1.0, at=(x, y, z), material=None, scale=(a, b, rise), segments=20)
    glass.data.materials.append(skin(paint))
    seat = sphere(1.0, at=(x, y, z + k * -0.006), material=None, scale=(a + 0.035, b + 0.035, rise * 0.75), segments=20)
    seat.data.materials.append(skin(rim))
    parts = [glass, seat]
    for i in range(vanes):
        t = (i + 1) / (vanes + 1) * 2 - 1
        vane = box(0.022, 2 * b * math.sqrt(1 - t * t) + 0.04, rise * 0.9, at=(x + a * t, y, z + k * rise * 0.62),
                   material=None, bevel=0.003)
        vane.data.materials.append(skin(rim))
        parts.append(vane)
    return parts


# The name of the layer that keeps a skin's own normals through every cut
# (`solid` stamps it, `gloss` reads it and takes it off).
OWN = 'n0'


def solid(objs, name):
    """The parts as one plain mesh (modifiers applied), ready for `seam`.
    A skin fresh from `loft` has its normals kept with it (a layer of its
    vertices that every later cut carries along), so however finely it is
    cut it is still shaded as the one smooth skin it was (`gloss`)."""
    objs = objs if isinstance(objs, (list, tuple)) else [objs]
    fresh = [bool(o.get('veil_skin')) and OWN not in o.data.attributes for o in objs]
    dg = bpy.context.evaluated_depsgraph_get()
    plain = []
    for o, new in zip(objs, fresh):
        me = bpy.data.meshes.new_from_object(o.evaluated_get(dg))
        me.transform(o.matrix_world)
        if new:
            n = np.empty(len(me.vertices) * 3, dtype=np.float32)
            me.vertices.foreach_get('normal', n)
            me.attributes.new(OWN, 'FLOAT_VECTOR', 'POINT').data.foreach_set('vector', n)
        if os.environ.get('VEIL_COUNT') and len(objs) > 3:
            print(f'PART {o.name}: {sum(len(f.vertices) - 2 for f in me.polygons)} triangles', flush=True)
        p = bpy.data.objects.new(o.name + '_m', me)
        bpy.context.scene.collection.objects.link(p)
        plain.append(p)
    for o in objs:
        bpy.data.objects.remove(o, do_unlink=True)
    bpy.ops.object.select_all(action='DESELECT')
    for p in plain:
        p.select_set(True)
    bpy.context.view_layer.objects.active = plain[0]
    if len(plain) > 1:
        bpy.ops.object.join()
    o = bpy.context.view_layer.objects.active
    o.name = o.data.name = name
    return o


def repaint(o, material, where=None, of=None):
    """Gives its faces the paint `material`: all of them, or those whose
    middle `where` picks (a rule over an (n, 3) array of points), or those
    now painted `of`."""
    me = o.data
    names = [m.name for m in me.materials]
    if material not in names:
        me.materials.append(skin(material))
        names.append(material)
    k = names.index(material)
    mid = np.empty(len(me.polygons) * 3)
    me.polygons.foreach_get('center', mid)
    pick = np.ones(len(me.polygons), dtype=bool) if where is None else np.asarray(where(mid.reshape(-1, 3)), dtype=bool)
    idx = np.empty(len(me.polygons), dtype=np.int32)
    me.polygons.foreach_get('material_index', idx)
    if of is not None:
        pick &= idx == (names.index(of) if of in names else -1)
    idx[pick] = k
    me.polygons.foreach_set('material_index', idx)
    me.update()
    return o


def _slot(me, material):
    names = [m.name for m in me.materials]
    if material not in names:
        me.materials.append(skin(material))
        names.append(material)
    return names.index(material)


# A cut that would pass this near a vertex, metres, goes through it instead:
# no sliver is left for a bevel or a groove to tear.
SNAP = 0.004


def _cut(bm, g, level, where):
    """Splits every edge that crosses g = `level` there, and joins the new
    points across each face they cut. A face it crosses is made triangles
    first (a triangle is cut one way only), and a vertex it would pass
    within `SNAP` of is taken to lie on it."""
    co = np.array([v.co[:] for v in bm.verts])
    val = g(co) - level
    on = np.abs(val) < SNAP
    val[on] = 0.0
    index = {v: i for i, v in enumerate(bm.verts)}
    across = [f for f in bm.faces
              if len(f.verts) > 3 and any(val[index[v]] > 0 for v in f.verts) and any(val[index[v]] < 0 for v in f.verts)]
    if across:
        bmesh.ops.triangulate(bm, faces=across)
    new = [v for v in bm.verts if on[index[v]] and (where is None or bool(where(np.array([v.co[:]]))[0]))]
    for e in list(bm.edges):
        a, b = e.verts
        da, db = val[index[a]], val[index[b]]
        if da * db >= 0:
            continue
        t = da / (da - db)
        if where is not None and not bool(where(np.array([a.co.lerp(b.co, t)[:]]))[0]):
            continue
        _, nv = bmesh.utils.edge_split(e, a, t)
        new.append(nv)
    if new:
        bmesh.ops.connect_verts(bm, verts=new, check_degenerate=True)
    return new


def seam(o, g, width=0.03, depth=0.012, floor='iron', wall=None, flat=0.0, where=None, within=None, end=0.012):
    """Cuts a groove into `o` along g = 0 (`g` a rule over an (n, 3) array
    of points that gives each a distance in metres from the seam's surface:
    a plane, a wing's edge moved in), `width` wide and `depth` deep. Its
    floor is the share `flat` of its width (0: a V) and is painted `floor`
    (dark iron, or a light: light out of the gap between two plates); its
    walls `wall` (the floor's if not given).

    `within` keeps it to a part of the craft: a rule that is positive where
    the seam runs, metres from where it stops, and there it ends square, its
    end wall `end` long. `where` (a rule true where it runs) is the cheap
    way, for an end nobody sees: the seam just stops at whatever edge of
    the skin is nearest."""
    me = o.data
    k_floor = _slot(me, floor)
    k_wall = _slot(me, wall or floor)
    bm = bmesh.new()
    bm.from_mesh(me)
    half = width / 2
    near = where
    if within is not None:
        near = lambda q: (within(q) > -0.03) & (True if where is None else where(q))
    levels = [-half, half] + ([-half * flat, half * flat] if flat > 0 else [0.0])
    for level in levels:
        _cut(bm, g, level, near)
    if within is not None:
        band = lambda q: np.abs(g(q)) < half + SNAP
        _cut(bm, within, 0.0, band)
        _cut(bm, within, end, band)
    bm.normal_update()
    bm.verts.ensure_lookup_table()
    co = np.array([v.co[:] for v in bm.verts])
    d = np.abs(g(co))
    on = np.ones(len(co), dtype=bool) if where is None else np.asarray(where(co), dtype=bool)
    run = np.full(len(co), 1.0) if within is None else np.clip(within(co) / end, 0.0, 1.0)
    reach = np.full(len(co), 1.0) if within is None else within(co)
    inside = (d < half - 1e-5) & on & (run > 0)
    sink = np.clip((1.0 - d / half) / max(1.0 - flat, 1e-6), 0.0, 1.0) * depth * run
    for v, i, s in zip(bm.verts, inside, sink):
        if i:
            v.co -= v.normal * s
    index = {v: i for i, v in enumerate(bm.verts)}
    for f in bm.faces:
        ids = [index[v] for v in f.verts]
        if not all(on[i] and d[i] <= half + SNAP and reach[i] >= -SNAP for i in ids):
            continue
        f.material_index = k_floor if all(d[i] <= half * flat + SNAP and reach[i] >= end - SNAP for i in ids) else k_wall
    bm.to_mesh(me)
    bm.free()
    me.update()
    if os.environ.get('VEIL_COUNT'):
        print(f'SEAM {o.name}: {len(me.polygons)} faces', flush=True)
    return o


def plate(o, inside, lift=0.016, material='shell', bevel=0.0, name='plate', clear=0.05, keep=('iron',) + LIT):
    """A plate laid over `o`'s skin where the rule `inside` is positive (a
    rule over an (n, 3) array of points, metres from the plate's edge): that
    part of the skin again, `lift` over it, with an edge of its own
    thickness (rounded by `bevel`, which is dear where seams cross the
    plate); a seam already cut there is in it too. What of `o`
    lies under it, further than `clear` from its edge, is taken off. Its
    object, for `solid`."""
    me = o.data.copy()
    bm = bmesh.new()
    bm.from_mesh(me)
    _cut(bm, inside, 0.0, None)
    mid = np.array([f.calc_center_median()[:] for f in bm.faces])
    bmesh.ops.delete(bm, geom=[f for f, k in zip(bm.faces, inside(mid) <= 0) if k], context='FACES')
    bm.normal_update()
    for v in bm.verts:
        v.co += v.normal * lift
    # The skin's paint becomes the plate's; what a seam painted (`keep`: its
    # iron, its light) it keeps.
    k = _slot(me, material)
    kept = {i for i, m in enumerate(me.materials) if m.name in keep}
    for f in bm.faces:
        if f.material_index not in kept:
            f.material_index = k
    bm.to_mesh(me)
    bm.free()
    if os.environ.get('VEIL_COUNT'):
        hist = {}
        for f in me.polygons:
            hist[me.materials[f.material_index].name] = hist.get(me.materials[f.material_index].name, 0) + 1
        print(f'PLATE {name}: {hist} keep {sorted(keep)}', flush=True)
    p = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(p)
    m = p.modifiers.new('solid', 'SOLIDIFY')
    m.thickness = lift + 0.014
    m.offset = -1.0
    # Its top and its edge alone: nothing sees its underside.
    m.use_rim_only = True
    _bevel(p, bevel, 2)
    if clear is not None:
        bm = bmesh.new()
        bm.from_mesh(o.data)
        mid = np.array([f.calc_center_median()[:] for f in bm.faces])
        bmesh.ops.delete(bm, geom=[f for f, k in zip(bm.faces, inside(mid) > clear) if k], context='FACES')
        bm.to_mesh(o.data)
        bm.free()
        o.data.update()
    return smooth(p, 40)


def ellipse(x, y, a, b):
    """The rule of an ellipse in plan, middle (`x`, `y`), half-axes `a`
    along the craft and `b` across it: metres inside its line (positive)
    or outside it."""
    return lambda q: (1.0 - np.hypot((q[:, 0] - x) / a, (q[:, 1] - y) / b)) * min(a, b)


def beside(f, by=0.0):
    """The rule of the line x = `f`(|y|) in plan, moved `by` metres ahead
    of itself (square to itself, so a seam along it keeps its width where
    the line turns): a wing's edge, and what runs beside it."""
    def g(q):
        y = np.abs(q[:, 1])
        slope = (f(y + 0.01) - f(y - 0.01)) / 0.02
        return (q[:, 0] - f(y)) / np.sqrt(1.0 + slope ** 2) - by
    return g


def plane(point, normal):
    """The rule of a plane through `point` across `normal`, for `seam`."""
    n = np.array(Vector(normal).normalized()[:])
    p = np.array(point, dtype=float)
    return lambda q: (q - p) @ n


def gloss(o, angle=38, within=24):
    """Its shading: smooth, hard past `angle` degrees; and wherever a face
    still lies in the skin it was cut from (within `within` degrees of the
    normals `solid` kept), that skin's own normals, so a seam's fine cuts do
    not show as facets in a reflection. A groove's wall and a plate's edge
    are shaded as what they are."""
    me = o.data
    smooth(o, angle)
    if OWN not in me.attributes:
        return o
    nv, nl = len(me.vertices), len(me.loops)
    own = np.empty(nv * 3, dtype=np.float32)
    me.attributes[OWN].data.foreach_get('vector', own)
    own = own.reshape(nv, 3)
    length = np.linalg.norm(own, axis=1)
    own = own / np.maximum(length, 1e-9)[:, None]
    me.calc_normals_split()
    now = np.empty(nl * 3, dtype=np.float32)
    me.loops.foreach_get('normal', now)
    now = now.reshape(nl, 3)
    lv = np.empty(nl, dtype=np.int32)
    me.loops.foreach_get('vertex_index', lv)
    face = np.empty(len(me.polygons) * 3, dtype=np.float32)
    me.polygons.foreach_get('normal', face)
    face = face.reshape(-1, 3)
    lp = np.empty(nl, dtype=np.int32)
    for p in me.polygons:
        lp[p.loop_start:p.loop_start + p.loop_total] = p.index
    keep = (length[lv] > 0.5) & ((face[lp] * own[lv]).sum(axis=1) > math.cos(math.radians(within)))
    now[keep] = own[lv][keep]
    me.normals_split_custom_set([tuple(n) for n in now])
    me.attributes.remove(me.attributes[OWN])
    me.update()
    print(f'GLOSS {o.name}: {float(keep.mean()) * 100:.0f}% of its corners shaded as the skin they were cut from')
    return o


def shade(o, rays=40, reach=1.4, floor=0.22, power=1.0, tone=0.07, edge=0.5, edge_colour=(0.72, 0.86, 1.25),
          under=0.82, seed=5, stain=None):
    """Its colours (COLOR_0, a corner each), which the game multiplies its
    paint by: the shade each vertex stands in, traced against the whole
    craft (`rays` a vertex, reaching `reach` metres, never darker than
    `floor`); each plate of a `PLATED` paint its own tone, `tone` either way
    (a plate is what no seam crosses, and a plate and its twin across the
    craft are one tone); a convex edge of one worn toward `edge_colour` by
    up to `edge`; what faces the ground darkened to `under`; and `stain`, a
    rule over an (n, 3) array of points that gives each how much of its
    paint is left under soot and dust (1 clean). A light keeps its own. The
    colours' alpha is the shade alone. It also writes where each corner lies
    on the craft into two UV sets (TEXCOORD_0: along it and across it;
    TEXCOORD_1: up it), metres, for the game's grain."""
    me = o.data
    me.calc_loop_triangles()
    nv = len(me.vertices)
    co = np.empty(nv * 3)
    me.vertices.foreach_get('co', co)
    co = co.reshape(nv, 3)
    polys = [tuple(p.vertices) for p in me.polygons]
    tree = BVHTree.FromPolygons([tuple(c) for c in co], polys, all_triangles=False)
    normal = np.empty(nv * 3)
    me.vertices.foreach_get('normal', normal)
    normal = normal.reshape(nv, 3)

    # Directions over a hemisphere, evenly, weighted by their cosine.
    dirs = []
    for i in range(rays):
        z = (i + 0.5) / rays
        r = math.sqrt(1.0 - z * z)
        a = i * math.pi * (3.0 - math.sqrt(5.0))
        dirs.append((r * math.cos(a), r * math.sin(a), z))
    total = sum(d[2] for d in dirs)
    ao = np.ones(nv)
    for i in range(nv):
        n = Vector(normal[i])
        if n.length < 1e-6:
            continue
        t = n.orthogonal().normalized()
        b = n.cross(t)
        at = Vector(co[i]) + n * 0.004
        hit = 0.0
        for dx, dy, dz in dirs:
            w = t * dx + b * dy + n * dz
            loc, _, _, dist = tree.ray_cast(at, w, reach)
            if loc is not None:
                hit += dz * (1.0 - dist / reach)
        ao[i] = 1.0 - hit / total
    ao = floor + (1.0 - floor) * np.clip(ao, 0.0, 1.0) ** power

    # How convex the skin is at each vertex: its neighbours lie under it.
    ne = len(me.edges)
    ev = np.empty(ne * 2, dtype=np.int32)
    me.edges.foreach_get('vertices', ev)
    ev = ev.reshape(ne, 2)
    d = co[ev[:, 1]] - co[ev[:, 0]]
    length = np.linalg.norm(d, axis=1) + 1e-9
    bend = np.zeros(nv)
    count = np.zeros(nv)
    np.add.at(bend, ev[:, 0], -(d * normal[ev[:, 0]]).sum(axis=1) / length)
    np.add.at(bend, ev[:, 1], (d * normal[ev[:, 1]]).sum(axis=1) / length)
    np.add.at(count, ev[:, 0], 1)
    np.add.at(count, ev[:, 1], 1)
    bend = bend / np.maximum(count, 1)
    worn = np.clip((bend - 0.06) / 0.25, 0.0, 1.0)

    # Plates: faces of a plated paint joined across their edges.
    names = [m.name for m in me.materials]
    plated = np.array([n in PLATED for n in names] or [False])
    lit = np.array([n in LIT for n in names] or [False])
    np_ = len(me.polygons)
    mat_of = np.empty(np_, dtype=np.int32)
    me.polygons.foreach_get('material_index', mat_of)
    mid = np.empty(np_ * 3)
    me.polygons.foreach_get('center', mid)
    mid = mid.reshape(np_, 3)
    area = np.empty(np_)
    me.polygons.foreach_get('area', area)
    root = list(range(np_))

    def find(i):
        while root[i] != i:
            root[i] = root[root[i]]
            i = root[i]
        return i

    by_edge = {}
    for p in me.polygons:
        if not plated[mat_of[p.index]]:
            continue
        for key in p.edge_keys:
            other = by_edge.setdefault(key, p.index)
            if other != p.index and mat_of[other] == mat_of[p.index]:
                ra, rb = find(other), find(p.index)
                if ra != rb:
                    root[ra] = rb
    plate = np.array([find(i) for i in range(np_)])
    tint = np.ones((np_, 3))
    for k in np.unique(plate[plated[mat_of]]):
        own = plate == k
        if area[own].sum() < 0.03:
            # A sliver between two seams: no tone of its own.
            continue
        c = (mid[own] * area[own, None]).sum(axis=0) / area[own].sum()
        rng = random.Random(hash((seed, round(c[0], 1), round(abs(c[1]), 1), round(c[2], 1))) & 0xffffffff)
        v = 1.0 + tone * (rng.random() * 2 - 1)
        hue = 0.05 * (rng.random() * 2 - 1)
        tint[own] = (v * (1 + hue), v, v * (1 - hue))

    attr = me.color_attributes.get('Col') or me.color_attributes.new('Col', 'FLOAT_COLOR', 'CORNER')
    nl = len(me.loops)
    lv = np.empty(nl, dtype=np.int32)
    me.loops.foreach_get('vertex_index', lv)
    lp = np.empty(nl, dtype=np.int32)
    for p in me.polygons:
        lp[p.loop_start:p.loop_start + p.loop_total] = p.index
    lm = mat_of[lp]
    col = np.ones((nl, 4))
    shadow = ao[lv] * np.where(normal[lv, 2] < 0, 1.0 + (under - 1.0) * np.clip(-normal[lv, 2], 0, 1), 1.0)
    col[:, :3] = shadow[:, None]
    isp = plated[lm]
    w = (worn[lv] * edge)[:, None] * isp[:, None]
    col[:, :3] = col[:, :3] * np.where(isp[:, None], tint[lp], 1.0) * (1 - w) + np.array(edge_colour) * shadow[:, None] * w
    pal = np.array([srgb(PAINTS[n][0]) if n in PAINTS and n not in LIT else (1.0, 1.0, 1.0) for n in names] or [(1.0, 1.0, 1.0)])
    col[:, :3] *= pal[lm]
    if stain is not None:
        col[:, :3] *= np.clip(np.asarray(stain(co), dtype=float), 0.0, 1.0)[lv][:, None]
    # The shade alone, for what the game lays over the paint (a reflection
    # is dimmed by it, a paint's colour is not).
    col[:, 3] = shadow
    col[lit[lm]] = 1.0
    attr.data.foreach_set('color', col.ravel())
    # Where each corner lies on the craft, metres, in its two UV sets (along
    # and across it; up it): what the game's shell stage lays its grain by
    # (`veil_shell.wgsl`), since a craft has no map to carry any.
    for name, pick in (('hull_xy', (0, 1)), ('hull_z', (2, 2))):
        uv = me.uv_layers.get(name) or me.uv_layers.new(name=name)
        at = np.stack([co[lv, pick[0]], co[lv, pick[1]] if name == 'hull_xy' else np.zeros(nl)], axis=1)
        uv.data.foreach_set('uv', at.astype(np.float32).ravel())
    me.color_attributes.active_color = attr
    me.color_attributes.render_color_index = list(me.color_attributes).index(attr)
    me.update()
    print(f'SHADE {o.name}: {nv} vertices, shade {float(ao.min()):.2f} to {float(ao.max()):.2f}, '
          f'{len(np.unique(plate[plated[mat_of]]))} plates')
    return o

"""The models' kit: every gun is a Blender script in this folder that builds
it from these primitives and hands the parts to `finish`, which joins them,
exports `crates/client/assets/models/<name>.glb` in the client's frame
(`look/guns.rs`: barrel along +Y, top toward +Z, stock at y = 0, metres),
writes the numbers guns.rs wants to `traces/guns/<name>.json`, and renders a
three-view sheet to `traces/guns/<name>.png` to judge it by.

    blender -b --python tools/models/ar.py          one gun
    tools/models/build.sh                            all of them

A script models in **gun space**: +X forward (the muzzle at the far end),
+Z up, +Y the gun's left, the barrel's axis along X at y = z = 0, the rear
of the stock near x = 0 (finish moves the rear to exactly 0). Metres.

Materials are named from PALETTE, two lines of them: Forge, the human guns
(gunmetal, polymer, steel, a display that glows blue), and Veil, the ship's
(pearl-purple shells, cyan and magenta light). No textures: colour, metal,
roughness, glow and coat on each part; the silhouette, bevels and the light
slits carry the look (DESIGN.md: ours by name and look, never another
game's art).
"""

import json
import math
import os
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
MODELS = os.path.join(ROOT, 'crates', 'client', 'assets', 'models')
TRACES = os.path.join(ROOT, 'traces', 'guns')

# name: (sRGB hex, metallic, roughness, emissive (hex, strength) or None, coat weight)
PALETTE = {
    # Forge: the human line.
    'gunmetal': ('#4b5149', 0.65, 0.50, None, 0.0),
    'gunmetal_dark': ('#2e322d', 0.65, 0.55, None, 0.0),
    'polymer': ('#1b1b1e', 0.00, 0.85, None, 0.0),
    'rubber': ('#232326', 0.00, 0.95, None, 0.0),
    'steel': ('#8d9398', 0.90, 0.35, None, 0.0),
    'olive': ('#5a6248', 0.30, 0.60, None, 0.0),
    'brass': ('#b08d3c', 0.90, 0.40, None, 0.0),
    'glass': ('#101418', 0.00, 0.10, None, 0.8),
    'display': ('#0a1a26', 0.00, 0.30, ('#49b8ff', 4.0), 0.0),
    'display_dim': ('#0a1a26', 0.00, 0.30, ('#49b8ff', 1.5), 0.0),
    'amber': ('#2a1a00', 0.00, 0.30, ('#ffb000', 3.0), 0.0),
    # Veil: the ship's line. The shells are barely metal and carry no
    # coat: in the game either takes the sky's colour and reads white.
    'shell': ('#5a2d8a', 0.15, 0.40, None, 0.0),
    'shell_dark': ('#2a1540', 0.15, 0.50, None, 0.0),
    'shell_light': ('#8a5cc8', 0.15, 0.40, None, 0.0),
    'shell_red': ('#8a2d4a', 0.15, 0.40, None, 0.0),
    'bone': ('#c9c2d6', 0.20, 0.50, None, 0.0),
    'iron': ('#3a3a44', 0.80, 0.45, None, 0.0),
    'cyan': ('#06232a', 0.00, 0.30, ('#4ff2ff', 5.0), 0.0),
    'cyan_dim': ('#06232a', 0.00, 0.30, ('#4ff2ff', 1.5), 0.0),
    'magenta': ('#2a0620', 0.00, 0.30, ('#ff4fd8', 5.0), 0.0),
    'core': ('#1a2a10', 0.00, 0.30, ('#a8ff80', 5.0), 0.0),
}


def srgb(hexs):
    h = hexs.lstrip('#')
    out = []
    for i in (0, 2, 4):
        v = int(h[i:i + 2], 16) / 255.0
        out.append(v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4)
    return out


def scene():
    """An empty scene in metres."""
    bpy.ops.wm.read_factory_settings(use_empty=True)
    s = bpy.context.scene
    s.unit_settings.system = 'METRIC'
    s.unit_settings.scale_length = 1.0
    return s


_mats = {}


def mat(name):
    """A palette material (cached per scene)."""
    if name in _mats and _mats[name].name in bpy.data.materials:
        return _mats[name]
    hexs, metallic, rough, emis, coat = PALETTE[name]
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    b = m.node_tree.nodes['Principled BSDF']
    b.inputs['Base Color'].default_value = (*srgb(hexs), 1.0)
    b.inputs['Metallic'].default_value = metallic
    b.inputs['Roughness'].default_value = rough
    if emis:
        b.inputs['Emission Color'].default_value = (*srgb(emis[0]), 1.0)
        b.inputs['Emission Strength'].default_value = emis[1]
    if coat:
        b.inputs['Coat Weight'].default_value = coat
        b.inputs['Coat Roughness'].default_value = 0.1
    _mats[name] = m
    return m


def _link(me, name, material):
    o = bpy.data.objects.new(name or me.name, me)
    bpy.context.scene.collection.objects.link(o)
    if material:
        me.materials.append(mat(material))
    return o


def _bevel(o, width, segments):
    if width > 0:
        m = o.modifiers.new('bevel', 'BEVEL')
        m.width = width
        m.segments = segments
        m.limit_method = 'ANGLE'
        m.angle_limit = math.radians(40)
        m.harden_normals = False


def smooth(o, angle=35):
    """Smooth shading, hard past `angle` degrees."""
    for p in o.data.polygons:
        p.use_smooth = True
    o.data.use_auto_smooth = True
    o.data.auto_smooth_angle = math.radians(angle)
    return o


def box(dx, dy, dz, at=(0, 0, 0), material='gunmetal', bevel=0.004, seg=2, name=None):
    """A block of full size dx × dy × dz centred on `at`."""
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=Vector((dx, dy, dz)), verts=bm.verts)
    bmesh.ops.translate(bm, vec=Vector(at), verts=bm.verts)
    me = bpy.data.meshes.new(name or 'box')
    bm.to_mesh(me)
    bm.free()
    o = _link(me, name, material)
    _bevel(o, bevel, seg)
    return smooth(o)


def cyl(r, length, axis='x', at=(0, 0, 0), material='steel', verts=32, bevel=0.002, seg=2, name=None, r2=None):
    """A cylinder of radius `r` (r2 at its far end, for a taper) and `length`
    along `axis`, centred on `at`."""
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=verts, radius1=r, radius2=r if r2 is None else r2, depth=length)
    rot = {'z': Matrix.Identity(4), 'x': Matrix.Rotation(math.radians(90), 4, 'Y'), 'y': Matrix.Rotation(math.radians(-90), 4, 'X')}[axis]
    bmesh.ops.transform(bm, matrix=rot, verts=bm.verts)
    bmesh.ops.translate(bm, vec=Vector(at), verts=bm.verts)
    me = bpy.data.meshes.new(name or 'cyl')
    bm.to_mesh(me)
    bm.free()
    o = _link(me, name, material)
    _bevel(o, bevel, seg)
    return smooth(o)


def tube(r_out, r_in, length, axis='x', at=(0, 0, 0), material='steel', verts=32, name=None):
    """A hollow cylinder (a barrel's muzzle, a scope) along `axis`."""
    bm = bmesh.new()
    outer = bmesh.ops.create_circle(bm, cap_ends=False, segments=verts, radius=r_out)['verts']
    inner = bmesh.ops.create_circle(bm, cap_ends=False, segments=verts, radius=r_in)['verts']
    eo = [e for e in bm.edges if all(v in outer for v in e.verts)]
    ei = [e for e in bm.edges if all(v in inner for v in e.verts)]
    bmesh.ops.bridge_loops(bm, edges=eo + ei)
    bmesh.ops.translate(bm, vec=Vector((0, 0, -length / 2)), verts=bm.verts)
    ext = bmesh.ops.extrude_face_region(bm, geom=bm.faces[:])
    bmesh.ops.translate(bm, vec=Vector((0, 0, length)), verts=[g for g in ext['geom'] if isinstance(g, bmesh.types.BMVert)])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    rot = {'z': Matrix.Identity(4), 'x': Matrix.Rotation(math.radians(90), 4, 'Y'), 'y': Matrix.Rotation(math.radians(-90), 4, 'X')}[axis]
    bmesh.ops.transform(bm, matrix=rot, verts=bm.verts)
    bmesh.ops.translate(bm, vec=Vector(at), verts=bm.verts)
    me = bpy.data.meshes.new(name or 'tube')
    bm.to_mesh(me)
    bm.free()
    return smooth(_link(me, name, material))


def profile(points, width, plane='xz', at=0.0, material='gunmetal', bevel=0.004, seg=2, name=None):
    """A polygon (counter-clockwise points in `plane`) extruded `width`
    across the third axis, centred at `at` on it: a receiver from its side
    profile (`xz`), a grip from its top profile (`xy`), a fin from `yz`."""
    bm = bmesh.new()
    third = {'xz': 1, 'xy': 2, 'yz': 0}[plane]
    axes = {'xz': (0, 2), 'xy': (0, 1), 'yz': (1, 2)}[plane]
    vs = []
    for p in points:
        v = [0.0, 0.0, 0.0]
        v[axes[0]], v[axes[1]] = p
        v[third] = at - width / 2
        vs.append(bm.verts.new(v))
    f = bm.faces.new(vs)
    ext = bmesh.ops.extrude_face_region(bm, geom=[f])
    d = [0.0, 0.0, 0.0]
    d[third] = width
    bmesh.ops.translate(bm, vec=Vector(d), verts=[g for g in ext['geom'] if isinstance(g, bmesh.types.BMVert)])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    me = bpy.data.meshes.new(name or 'profile')
    bm.to_mesh(me)
    bm.free()
    o = _link(me, name, material)
    _bevel(o, bevel, seg)
    return smooth(o)


def sphere(r, at=(0, 0, 0), material='shell', scale=(1, 1, 1), name=None, segments=32):
    """A sphere, scaled per axis (a pod, a knuckle, a bulb)."""
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=segments, v_segments=segments // 2, radius=r)
    bmesh.ops.scale(bm, vec=Vector(scale), verts=bm.verts)
    bmesh.ops.translate(bm, vec=Vector(at), verts=bm.verts)
    me = bpy.data.meshes.new(name or 'sphere')
    bm.to_mesh(me)
    bm.free()
    return smooth(_link(me, name, material), 60)


def shell(points, width, at=0.0, material='shell', plane='xz', levels=2, bevel=0.006, name=None):
    """An organic body: a profile, bevelled, then subdivided smooth (the
    ship's clamshells). Keep the profile coarse: 6 to 12 points."""
    o = profile(points, width, plane=plane, at=at, material=material, bevel=bevel, seg=1, name=name)
    m = o.modifiers.new('subsurf', 'SUBSURF')
    m.levels = levels
    m.render_levels = levels
    return smooth(o, 60)


def torus(r_major, r_minor, axis='x', at=(0, 0, 0), material='steel', segments=32, ring=12, name=None):
    """A ring (a sight's hoop, a lift ring) whose hole runs along `axis`."""
    bpy.ops.mesh.primitive_torus_add(major_radius=r_major, minor_radius=r_minor, major_segments=segments, minor_segments=ring)
    o = bpy.context.object
    o.name = name or 'torus'
    if material:
        o.data.materials.append(mat(material))
    rot = {'z': (0, 0, 0), 'x': (0, math.radians(90), 0), 'y': (math.radians(90), 0, 0)}[axis]
    o.rotation_euler = rot
    o.location = at
    return smooth(o, 60)


def subsurf(o, levels=2):
    m = o.modifiers.new('subsurf', 'SUBSURF')
    m.levels = levels
    m.render_levels = levels
    return smooth(o, 60)


def mirror_y(o):
    """Mirrored across the gun's centre plane (y = 0)."""
    m = o.modifiers.new('mirror', 'MIRROR')
    m.use_axis = (False, True, False)
    m.use_clip = True
    return o


def taper(o, axis='x', start=0.0, end=1.0, factor=0.5):
    """Scales the cross-section linearly from `start` (×1) to `end`
    (×factor) along `axis`, in the object's mesh."""
    i = 'xyz'.index(axis)
    for v in o.data.vertices:
        t = (v.co[i] - start) / (end - start) if end != start else 0.0
        t = max(0.0, min(1.0, t))
        k = 1.0 + (factor - 1.0) * t
        for j in range(3):
            if j != i:
                v.co[j] *= k
    return o


def move(o, dx=0, dy=0, dz=0):
    o.location = (o.location.x + dx, o.location.y + dy, o.location.z + dz)
    return o


def rotate(o, rx=0, ry=0, rz=0):
    """Turned by degrees about its own origin (the gun space's origin unless
    you moved it)."""
    o.rotation_euler = (math.radians(rx), math.radians(ry), math.radians(rz))
    return o


def _apply_all(objs):
    """Modifiers applied, transforms baked: plain meshes."""
    dg = bpy.context.evaluated_depsgraph_get()
    out = []
    for o in objs:
        ev = o.evaluated_get(dg)
        me = bpy.data.meshes.new_from_object(ev)
        me.transform(o.matrix_world)
        for m in o.data.materials:
            if m and m.name not in [x.name for x in me.materials if x]:
                pass
        n = bpy.data.objects.new(o.name + '_m', me)
        bpy.context.scene.collection.objects.link(n)
        out.append(n)
    for o in objs:
        bpy.data.objects.remove(o, do_unlink=True)
    return out


def join(objs, name='Mesh_0'):
    """One mesh of all the parts (modifiers applied)."""
    parts = _apply_all(objs)
    bpy.ops.object.select_all(action='DESELECT')
    for p in parts:
        p.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    o = bpy.context.view_layer.objects.active
    o.name = name
    o.data.name = name
    return o


# Gun space (fwd X, up Z, left Y) to the client's export frame (fwd Z, top −Y)
# in Blender; Blender's glTF export then makes fwd +Y, top +Z.
TO_EXPORT = Matrix(((0, -1, 0, 0), (0, 0, -1, 0), (1, 0, 0, 0), (0, 0, 0, 1)))


def bounds(o):
    lo = Vector((1e9,) * 3)
    hi = Vector((-1e9,) * 3)
    for v in o.data.vertices:
        w = o.matrix_world @ v.co
        lo = Vector(map(min, lo, w))
        hi = Vector(map(max, hi, w))
    return lo, hi


def finish(name, parts, grip, long=True, note=''):
    """Joins `parts`, puts the stock's rear at x = 0, exports the GLB and the
    numbers (`grip`: the x of the hand on the pistol grip, gun space), and
    renders the sheet. Call last."""
    o = join(parts)
    lo, hi = bounds(o)
    o.data.transform(Matrix.Translation(Vector((-lo.x, 0, 0))))
    o.data.update()
    height = hi.x - lo.x
    grip_x = grip - lo.x
    os.makedirs(MODELS, exist_ok=True)
    os.makedirs(TRACES, exist_ok=True)
    sheet(o, os.path.join(TRACES, name + '.png'), title=name)
    lo2, hi2 = bounds(o)
    o.data.transform(TO_EXPORT)
    o.data.update()
    bpy.ops.object.select_all(action='DESELECT')
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    path = os.path.join(MODELS, name + '.glb')
    bpy.ops.export_scene.gltf(filepath=path, export_format='GLB', export_apply=True, export_yup=True, use_selection=True)
    info = {
        'file': name,
        'height': round(height, 4),
        'grip': round(grip_x / height, 4),
        'long': bool(long),
        'tris': sum(len(p.vertices) - 2 for p in o.data.polygons),
        'size_m': [round(v, 3) for v in (hi2 - lo2)],
        'note': note,
    }
    with open(os.path.join(TRACES, name + '.json'), 'w') as f:
        json.dump(info, f, indent=1)
    print('MODEL', json.dumps(info))
    return info


def sheet(o, path, title=''):
    """Three views of a gun-space object on one 1500 × 500 sheet: its left
    side, a three-quarter from the front left and above, and from the top.
    Eevee, a sun and a soft fill, a mid-grey plate."""
    s = bpy.context.scene
    s.render.engine = 'BLENDER_EEVEE'
    s.eevee.taa_render_samples = 24
    s.eevee.use_bloom = True
    s.eevee.bloom_intensity = 0.04
    s.view_settings.view_transform = 'Standard'
    s.render.resolution_x = 1500
    s.render.resolution_y = 500
    s.render.film_transparent = False
    w = bpy.data.worlds.new('w')
    w.use_nodes = True
    w.node_tree.nodes['Background'].inputs['Color'].default_value = (0.22, 0.22, 0.24, 1)
    w.node_tree.nodes['Background'].inputs['Strength'].default_value = 1.0
    s.world = w
    lo, hi = bounds(o)
    size = max(hi - lo)
    centre = (lo + hi) / 2
    # Three copies on a line along X, one view each, facing a camera on -Y.
    # The camera is on -Y: tipping the top toward it is a positive turn
    # about X.
    views = [
        Matrix.Identity(4),
        Matrix.Rotation(math.radians(25), 4, 'X') @ Matrix.Rotation(math.radians(35), 4, 'Z'),
        Matrix.Rotation(math.radians(90), 4, 'X'),
    ]
    # The original stays out of the picture.
    was_hidden = o.hide_render
    o.hide_render = True
    copies = []
    for i, m in enumerate(views):
        c = o.copy()
        c.data = o.data.copy()
        # A copy inherits the hiding; these are the picture.
        c.hide_render = False
        bpy.context.scene.collection.objects.link(c)
        c.matrix_world = Matrix.Translation(Vector(((i - 1) * size * 1.15, 0, 0))) @ m @ Matrix.Translation(-centre)
        copies.append(c)
    cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam'))
    s.collection.objects.link(cam)
    cam.data.type = 'ORTHO'
    cam.data.ortho_scale = size * 3.5
    cam.location = (0, -5, 0)
    cam.rotation_euler = (math.radians(90), 0, 0)
    s.camera = cam
    sun = bpy.data.objects.new('sun', bpy.data.lights.new('sun', 'SUN'))
    s.collection.objects.link(sun)
    sun.data.energy = 2.5
    sun.rotation_euler = (math.radians(55), math.radians(-20), math.radians(-30))
    fill = bpy.data.objects.new('fill', bpy.data.lights.new('fill', 'AREA'))
    s.collection.objects.link(fill)
    fill.data.energy = 60
    fill.data.size = 6
    fill.location = (0, -4, 3)
    fill.rotation_euler = (math.radians(60), 0, 0)
    s.render.filepath = path
    bpy.ops.render.render(write_still=True)
    for c in copies:
        bpy.data.objects.remove(c, do_unlink=True)
    for x in (cam, sun, fill):
        bpy.data.objects.remove(x, do_unlink=True)
    o.hide_render = was_hidden
    print('SHEET', path)


def sheet_glb(path, out):
    """An existing GLB's sheet (brought back to gun space), to compare."""
    scene()
    bpy.ops.import_scene.gltf(filepath=path)
    objs = [o for o in bpy.data.objects if o.type == 'MESH']
    o = join(objs, 'glb')
    o.data.transform(TO_EXPORT.inverted())
    o.data.update()
    sheet(o, out)


if __name__ == '__main__':
    # blender -b --python tools/models/lib.py -- sheet a.glb out.png
    argv = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
    if argv[:1] == ['sheet']:
        sheet_glb(argv[1], argv[2])

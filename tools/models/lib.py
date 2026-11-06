"""The models' kit. A gun is a Blender script in this folder, of two kinds:

  - from Meshy: `meshy_gen.py <name>` commissions it from the script's
    IMAGE and TEXTURE into sources/meshy, and the script brings that raw
    mesh into gun space with `generated` (sized, turned, its bore on the
    axis, its textures cut down, what it painted bright set glowing);
  - from the kit: built of these primitives (the rocket, the frag, and any
    part bolted onto a generated gun).

Either hands its parts to `finish`, which joins them, exports
`crates/client/assets/models/<name>.glb` in the client's frame
(`look/guns.rs`: barrel along +Y, top toward +Z, stock at y = 0, metres),
writes the numbers guns.rs wants to `traces/guns/<name>.json`, and renders a
four-view sheet to `traces/guns/<name>.png` to judge it by.

    blender -b --python tools/models/ar.py          one gun
    tools/models/build.sh                            all of them

**Gun space**: +X forward (the muzzle at the far end), +Z up, +Y the gun's
left, the barrel's axis along X at y = z = 0, the rear of the stock near
x = 0 (finish moves the rear to exactly 0). Metres.

The kit's materials are named from PALETTE, two lines of them: Forge, the
human guns (gunmetal, polymer, steel, a display that glows blue), and Veil,
the ship's (pearl-purple shells, cyan and magenta light): colour, metal,
roughness, glow and coat, no textures. A generated gun carries Meshy's maps
(DESIGN.md: ours by name and look, never another game's art).
"""

import json
import math
import os
import sys

import bmesh
import bpy
import numpy as np
import struct
from mathutils import Matrix, Quaternion, Vector

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
MODELS = os.path.join(ROOT, 'crates', 'client', 'assets', 'models')
TRACES = os.path.join(ROOT, 'traces', 'guns')
# Meshy's raw meshes (meshy_gen.py), not in the repo.
SOURCES = os.environ.get('MESHY_SRC', os.path.join(ROOT, 'sources', 'meshy'))
# The longest side a generated gun's colour map keeps, and its other maps
# (the normals, the roughness and metal, the glow); a small prop, a rocket or
# a frag, keeps less of each (`generated`'s `tex_px`, `map_px`).
TEX_PX = 2048
MAP_PX = 1024
SMALL_TEX_PX = 1024
SMALL_MAP_PX = 512
# Another paint on a gun's mesh (meshy_gen.py --paint): built beside it as
# `<name>~<tag>` from `<name>~<tag>_raw.glb`.
PAINT = os.environ.get('MODEL_PAINT', '')
# The middle of the fist under the axis a gun is held by
# (look/viewmodel.rs's FIST); where a long gun's other hand goes unless its
# script says, as a share of the barrel past the grip; and how far under
# the gun's belly that hand's middle sits (viewmodel.rs's FORE_CUP).
FIST_DROP = 0.05
FORE_SHARE = 0.6
FORE_CUP = 0.02
# Meshy paints a gun glossier and more of a metal than the game's sky
# likes (in the hand, seen along its top, it takes the sky's colour and
# reads chrome): the least roughness a generated gun keeps (the rest
# squeezed above it), its metal's scale, and its normal map's strength.
ROUGH = 0.5
METAL = 0.4
NORMAL = 0.5
# `temper`: where a paint's pale lines start, how much of them over that is
# kept, and how rough its paint and its steel come out at least.
TEMPER_KNEE = 0.38
TEMPER_SQUEEZE = 0.3
TEMPER_PAINT = 0.8
TEMPER_STEEL = 0.62
# How much of the light baked into a paint `temper` takes back out, and how
# wide that light is, as a share of the map's side.
TEMPER_FLATTEN = 0.8
TEMPER_LIGHT_SIZE = 0.02
# And its contrast: how much of the swing round each material's middle is kept,
# and how dark that middle may be for steel (near-black beside a pale line is
# what reads wet).
TEMPER_CONTRAST = 0.55
TEMPER_STEEL_MID = 0.26

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


def _hsv(rgb):
    """Hue (degrees), saturation and value of an (h, w, 3) array."""
    hi = rgb.max(axis=-1)
    lo = rgb.min(axis=-1)
    d = hi - lo
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    safe = np.where(d > 1e-6, d, 1.0)
    hue = np.where(hi == r, ((g - b) / safe) % 6.0, np.where(hi == g, (b - r) / safe + 2.0, (r - g) / safe + 4.0))
    return np.where(d > 1e-6, hue * 60.0, 0.0), np.where(hi > 1e-6, d / np.where(hi > 1e-6, hi, 1.0), 0.0), hi


def _hue_of(hexs):
    h = hexs.lstrip('#')
    rgb = np.array([[[int(h[i:i + 2], 16) / 255.0 for i in (0, 2, 4)]]], dtype=np.float32)
    return float(_hsv(rgb)[0][0, 0])


def _ramp(x, lo, hi):
    return np.clip((x - lo) / (hi - lo), 0.0, 1.0)


def _glow(material, hues, strength, floor=0.45, pale=False, window=24.0, map_px=MAP_PX):
    """Lights what the paint made bright: an emissive map that is the base
    colour where its hue is within `window` degrees of one of `hues` (all
    of it within half that), it is saturated, and its value is over
    `floor` (all of it a quarter above); black elsewhere (Gates' fel_veins
    rule, by hue). `pale`: a light near white (a blade's) counts, by a
    tenth of the saturation, and burns in the first of `hues`. Returns the
    lit share of the map."""
    nt = material.node_tree
    bsdf = next(n for n in nt.nodes if n.type == 'BSDF_PRINCIPLED')
    node = bsdf.inputs['Base Color'].links[0].from_node
    while node.type != 'TEX_IMAGE':
        node = next(i.links[0].from_node for i in node.inputs if i.links)
    im = node.image
    w, h = im.size
    px = np.empty(w * h * 4, dtype=np.float32)
    im.pixels.foreach_get(px)
    rgb = px.reshape(h, w, 4)[..., :3]
    hue, sat, val = _hsv(rgb)
    mask = np.zeros((h, w), dtype=np.float32)
    for hx in hues:
        d = np.abs((hue - _hue_of(hx) + 180.0) % 360.0 - 180.0)
        full = _ramp(sat, 0.08, 0.20) if pale else _ramp(sat, 0.35, 0.55)
        mask = np.maximum(mask, _ramp(window - d, 0.0, window / 2) * full * _ramp(val, floor, floor + 0.25))
    out = np.ones((h, w, 4), dtype=np.float32)
    # A pale light burns in the colour named: its own paint is near white.
    out[..., :3] = (np.array(srgb(hues[0]), dtype=np.float32) if pale else rgb) * mask[..., None]
    lit = bpy.data.images.new(material.name + '_glow', w, h, alpha=False)
    lit.pixels.foreach_set(out.ravel())
    k = map_px / max(w, h)
    if k < 1.0:
        lit.scale(max(1, round(w * k)), max(1, round(h * k)))
    lit.pack()
    tex = nt.nodes.new('ShaderNodeTexImage')
    tex.image = lit
    for l in node.inputs['Vector'].links:
        nt.links.new(l.from_socket, tex.inputs['Vector'])
    nt.links.new(tex.outputs['Color'], bsdf.inputs['Emission Color'])
    bsdf.inputs['Emission Strength'].default_value = strength
    return float(mask.mean())


def _surface(material, rough, metal):
    """Squeezes the roughness (the map's green) into `rough`..1 and scales
    the metal (its blue) by `metal`, in the map itself: the exporter writes
    maps, not node arithmetic."""
    nt = material.node_tree
    bsdf = next(n for n in nt.nodes if n.type == 'BSDF_PRINCIPLED')
    if not bsdf.inputs['Roughness'].links:
        return
    node = bsdf.inputs['Roughness'].links[0].from_node
    while node.type != 'TEX_IMAGE':
        node = next(i.links[0].from_node for i in node.inputs if i.links)
    im = node.image
    px = np.empty(im.size[0] * im.size[1] * 4, dtype=np.float32)
    im.pixels.foreach_get(px)
    px = px.reshape(-1, 4)
    px[:, 1] = rough + (1.0 - rough) * px[:, 1]
    px[:, 2] *= metal
    im.pixels.foreach_set(px.ravel())
    im.pack()


def _painted(name):
    return f'{name}~{PAINT}' if PAINT else name


def generated(name, length, hold=None, flip=None, over=False, width=None, bore=None, glow=(), glow_strength=4.0,
              glow_floor=0.45, pale=False, tris=None, rough=ROUGH, metal=METAL, tex_px=TEX_PX, map_px=MAP_PX):
    """Meshy's raw mesh of `name` (sources/meshy/<name>_raw.glb, from
    meshy_gen.py) in gun space, `length` metres from its rear at x = 0 to
    its muzzle, scaled evenly: `(object, rise)`, the rise for `finish`.

    Its longest side is taken for the barrel, its thinnest for its width;
    the muzzle is the end with the smaller cross-section (`flip` says
    otherwise: True turns it end for end) and up is as it came (`over`
    turns it over). `width` squeezes or fattens it across to that many
    metres: one picture's depth is a guess.

    The axis the client holds it by (y = z = 0) runs FIST_DROP over `hold`:
    where the middle of the fist goes on its grip, as a share of the gun's
    height from its bottom (read it off the sheet's ruler). The muzzle is
    the middle of its foremost 3% (`bore`: its height instead, the same
    kind of share), and `rise` is how far over the axis that is. With no
    `hold` (a rocket) the axis is the muzzle's own.

    The colour map comes down to `tex_px`, the others to `map_px`. The glow map
    Meshy ships is dropped (a stick's peaked at 0.53); `glow` names the
    colours of light the paint carries (hex) and those texels are lit at
    `glow_strength` (`glow_floor`: how bright a texel must be painted to
    count, where the hull's own sheen is near the light's hue; `pale`:
    light that is nearly white counts too). `rough` and `metal` are ROUGH
    and METAL for this gun; its normal map is drawn at NORMAL. `tris` thins
    the mesh to about that many."""
    path = os.path.join(SOURCES, _painted(name) + '_raw.glb')
    if not os.path.exists(path):
        print(f'Error: no {path}; tools/models/meshy_gen.py {name} commissions it (credits)')
        sys.exit(1)
    bpy.ops.import_scene.gltf(filepath=path)
    meshes = [o for o in bpy.context.scene.objects if o.type == 'MESH']
    bpy.ops.object.select_all(action='DESELECT')
    for m in meshes:
        m.select_set(True)
    bpy.context.view_layer.objects.active = meshes[0]
    bpy.ops.object.parent_clear(type='CLEAR_KEEP_TRANSFORM')
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    if len(meshes) > 1:
        bpy.ops.object.join()
    o = bpy.context.view_layer.objects.active
    for x in [x for x in bpy.context.scene.objects if x.type != 'MESH']:
        bpy.data.objects.remove(x, do_unlink=True)
    while len(o.data.uv_layers) > 1:
        o.data.uv_layers.remove(o.data.uv_layers[-1])
    o.name = name

    # Into gun space: the long side forward, the thin side across.
    lo, hi = bounds(o)
    ext = hi - lo
    side_i, up_i, fwd_i = sorted(range(3), key=lambda i: ext[i])
    co = np.array([v.co[:] for v in o.data.vertices])

    def section(a, b):
        part = co[(co[:, fwd_i] >= lo[fwd_i] + a * ext[fwd_i]) & (co[:, fwd_i] <= lo[fwd_i] + b * ext[fwd_i])]
        return float(np.ptp(part[:, up_i]) * np.ptp(part[:, side_i])) if len(part) else 0.0

    front = 1.0 if section(0.9, 1.0) < section(0.0, 0.1) else -1.0
    if flip:
        front = -front
    f = Vector([front if i == fwd_i else 0.0 for i in range(3)])
    u = Vector([(-1.0 if over else 1.0) if i == up_i else 0.0 for i in range(3)])
    turn = Matrix((f, u.cross(f), u)).to_4x4()
    k = length / ext[fwd_i]
    o.data.transform(Matrix.Scale(k, 4) @ turn)
    if width:
        lo, hi = bounds(o)
        o.data.transform(Matrix.Scale(width / (hi.y - lo.y), 4, Vector((0, 1, 0))))
    o.data.update()
    lo, hi = bounds(o)
    co = np.array([v.co[:] for v in o.data.vertices])
    nose = co[co[:, 0] >= hi.x - 0.03 * (hi.x - lo.x)]
    y = (nose[:, 1].min() + nose[:, 1].max()) / 2
    muzzle = (nose[:, 2].min() + nose[:, 2].max()) / 2
    if bore is not None:
        y, muzzle = (lo.y + hi.y) / 2, lo.z + bore * (hi.z - lo.z)
    axis = muzzle if hold is None else lo.z + hold * (hi.z - lo.z) + FIST_DROP
    o.data.transform(Matrix.Translation(Vector((-lo.x, -y, -axis))))
    o.data.update()

    count = sum(len(p.vertices) - 2 for p in o.data.polygons)
    if tris and count > tris:
        m = o.modifiers.new('thin', 'DECIMATE')
        m.ratio = tris / count

    colour = set()
    for material in o.data.materials:
        bsdf = next(n for n in material.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
        node = bsdf.inputs['Base Color'].links[0].from_node
        while node.type != 'TEX_IMAGE':
            node = next(i.links[0].from_node for i in node.inputs if i.links)
        colour.add(node.image)
    for im in bpy.data.images:
        w, h = im.size
        px = tex_px if im in colour else map_px
        if max(w, h) > px:
            s = px / max(w, h)
            im.scale(max(1, round(w * s)), max(1, round(h * s)))
    for material in o.data.materials:
        nt = material.node_tree
        bsdf = next(n for n in nt.nodes if n.type == 'BSDF_PRINCIPLED')
        for l in list(bsdf.inputs['Emission Color'].links):
            nt.links.remove(l)
        bsdf.inputs['Emission Strength'].default_value = 0.0
        _surface(material, rough, metal)
        for n in nt.nodes:
            if n.type == 'NORMAL_MAP':
                n.inputs['Strength'].default_value = NORMAL
        if glow:
            lit = _glow(material, glow, glow_strength, glow_floor, pale, map_px=map_px)
            print(f'GLOW {name}: {lit * 100:.2f}% of the map is lit')
    return o, muzzle - axis


def cut_above(o, x0, x1, margin=0.004):
    """Takes off what stands up from a gun between x0 and x1 (gun space, as
    `generated` leaves it): a sight, a hoop. Its height is judged from the
    gun's top just either side, and the hole it leaves is closed."""
    co = [v.co for v in o.data.vertices]
    span = x1 - x0
    beside = [c.z for c in co if x0 - span < c.x < x0 or x1 < c.x < x1 + span]
    top = max(beside) + margin
    bm = bmesh.new()
    bm.from_mesh(o.data)
    gone = [v for v in bm.verts if x0 < v.co.x < x1 and v.co.z > top]
    print(f'CUT {o.name}: {len(gone)} vertices above z {top:.3f}')
    bmesh.ops.delete(bm, geom=gone, context='VERTS')
    bmesh.ops.holes_fill(bm, edges=bm.edges, sides=128)
    bm.to_mesh(o.data)
    bm.free()
    o.data.update()


def dim_paint(o, hues, k, window=24.0):
    """The colour map's paint times `k` where its hue is within `window` degrees
    of one of `hues` and it is saturated: a glow's paint, which reads as a flat
    patch close up, whatever its light does."""
    for material in o.data.materials:
        bsdf = next(n for n in material.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
        node = bsdf.inputs['Base Color'].links[0].from_node
        while node.type != 'TEX_IMAGE':
            node = next(i.links[0].from_node for i in node.inputs if i.links)
        im = node.image
        w, h = im.size
        px = np.empty(w * h * 4, np.float32)
        im.pixels.foreach_get(px)
        rgba = px.reshape(h, w, 4)
        hue, sat, _ = _hsv(rgba[..., :3])
        mask = np.zeros((h, w), np.float32)
        for hx in hues:
            d = np.abs((hue - _hue_of(hx) + 180.0) % 360.0 - 180.0)
            mask = np.maximum(mask, _ramp(window - d, 0.0, window / 2) * _ramp(sat, 0.25, 0.45))
        rgba[..., :3] *= (1.0 - mask * (1.0 - k))[..., None]
        im.pixels.foreach_set(rgba.ravel())
        im.update()


def darken(o, k):
    """The colour map's paint times `k`, in the map itself (the glTF exporter
    drops node arithmetic)."""
    for material in o.data.materials:
        bsdf = next(n for n in material.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
        node = bsdf.inputs['Base Color'].links[0].from_node
        while node.type != 'TEX_IMAGE':
            node = next(i.links[0].from_node for i in node.inputs if i.links)
        im = node.image
        px = np.empty(im.size[0] * im.size[1] * 4, np.float32)
        im.pixels.foreach_get(px)
        px[0::4] *= k
        px[1::4] *= k
        px[2::4] *= k
        im.pixels.foreach_set(px)
        im.update()


def _image_of(socket):
    node = socket.links[0].from_node
    while node.type != 'TEX_IMAGE':
        node = next(i.links[0].from_node for i in node.inputs if i.links)
    return node.image


def _pixels(im):
    w, h = im.size
    px = np.empty(w * h * 4, np.float32)
    im.pixels.foreach_get(px)
    return px.reshape(h, w, 4)


def _blur(a, r):
    """A box of `r` texels each way, twice."""
    for _ in range(2):
        for axis in (0, 1):
            pad = [(r + 1, r) if i == axis else (0, 0) for i in range(2)]
            c = np.cumsum(np.pad(a, pad, mode='edge'), axis=axis, dtype=np.float64)
            hi = np.take(c, range(2 * r + 1, c.shape[axis]), axis=axis)
            lo = np.take(c, range(0, c.shape[axis] - 2 * r - 1), axis=axis)
            a = ((hi - lo) / (2 * r + 1)).astype(np.float32)
    return a


def temper(o, knee=TEMPER_KNEE, squeeze=TEMPER_SQUEEZE, paint_rough=TEMPER_PAINT, steel_rough=TEMPER_STEEL,
           flatten=TEMPER_FLATTEN):
    """What a generated paint gets wrong, in its own maps. It bakes its light
    in: the wide swings of brightness within paint and within steel are taken
    back out by `flatten`, and the scratches stay. It draws a pale line along
    every edge: pale, unsaturated texels over `knee` come down by `squeeze`.
    And it has one surface for paint and steel, so all of it reads as wet
    plastic: what is saturated goes matte and non-metal (`paint_rough`), the
    rest at least `steel_rough`."""
    for material in o.data.materials:
        bsdf = next(n for n in material.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
        im = _image_of(bsdf.inputs['Base Color'])
        rgba = _pixels(im)
        _, sat, val = _hsv(rgba[..., :3])
        paint = _ramp(sat, 0.12, 0.3)
        r = max(1, round(im.size[0] * TEMPER_LIGHT_SIZE))
        for share in (paint, 1.0 - paint):
            # Its light, measured over this material alone.
            weight = _blur(share, r) + 1e-3
            light = _blur(val * share, r) / weight
            mean = float((val * share).sum() / max(share.sum(), 1.0))
            gain = (mean / np.maximum(light, 0.02)) ** flatten
            rgba[..., :3] *= (1.0 + share * (np.clip(gain, 0.4, 2.5) - 1.0))[..., None]
        _, _, val = _hsv(rgba[..., :3])
        for share, floor in ((paint, 0.0), (1.0 - paint, TEMPER_STEEL_MID)):
            mid = float((val * share).sum() / max(share.sum(), 1.0))
            calm = max(mid, floor) + (val - mid) * TEMPER_CONTRAST
            gain = np.maximum(calm, 0.02) / np.maximum(val, 0.02)
            rgba[..., :3] *= (1.0 + share * (gain - 1.0))[..., None]
        np.clip(rgba[..., :3], 0.0, 1.0, out=rgba[..., :3])
        _, sat, val = _hsv(rgba[..., :3])
        pale = _ramp(val, knee, knee + 0.2) * (1.0 - _ramp(sat, 0.15, 0.35))
        over = knee + np.maximum(val - knee, 0.0) * squeeze
        k = np.where(val > knee, over / np.maximum(val, 1e-4), 1.0)
        rgba[..., :3] *= (1.0 + pale * (k - 1.0))[..., None]
        im.pixels.foreach_set(rgba.ravel())
        im.update()
        if not bsdf.inputs['Roughness'].links:
            continue
        mr = _image_of(bsdf.inputs['Roughness'])
        maps = _pixels(mr)
        h, w = maps.shape[:2]
        rows = np.arange(h) * sat.shape[0] // h
        cols = np.arange(w) * sat.shape[1] // w
        paint = _ramp(sat[np.ix_(rows, cols)], 0.12, 0.3)
        maps[..., 1] = np.maximum(maps[..., 1], steel_rough + paint * (paint_rough - steel_rough))
        maps[..., 2] *= 1.0 - paint
        mr.pixels.foreach_set(maps.ravel())
        mr.pack()


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


def finish(name, parts, grip, long=True, note='', rise=0.0, fore=None, under=None):
    """Joins `parts`, puts the stock's rear at x = 0, exports the GLB and the
    numbers (`grip`: the x of the hand on the pistol grip, gun space;
    `rise`: the muzzle's height over the axis, `generated`'s), and renders
    the sheet, the hands and the muzzle marked on it. Call last.

    A long gun's other hand cups its belly FORE_SHARE of the way from the
    grip to the muzzle, where the belly is measured; `fore` is another x
    for it (a foregrip's), `under` how far under the axis what it holds is
    (a foregrip's middle, not its foot)."""
    o = join(parts)
    lo, hi = bounds(o)
    o.data.transform(Matrix.Translation(Vector((-lo.x, 0, 0))))
    o.data.update()
    height = hi.x - lo.x
    grip_x = grip - lo.x
    fore_x = grip_x + FORE_SHARE * (height - grip_x) if fore is None else fore - lo.x
    if under is None:
        belly = [v.co.z for v in o.data.vertices if abs(v.co.x - fore_x) < 0.03]
        under = -min(belly) if belly else FIST_DROP
    os.makedirs(MODELS, exist_ok=True)
    os.makedirs(TRACES, exist_ok=True)
    sheet(o, os.path.join(TRACES, _painted(name) + '.png'), title=_painted(name), fist=(grip_x, -FIST_DROP), muzzle=(height, rise),
          other=(fore_x, -under - FORE_CUP) if long else None)
    lo2, hi2 = bounds(o)
    o.data.transform(TO_EXPORT)
    o.data.update()
    bpy.ops.object.select_all(action='DESELECT')
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    path = os.path.join(MODELS, _painted(name) + '.glb')
    # A generated gun's maps go out as JPEG; the kit's guns have none.
    bpy.ops.export_scene.gltf(filepath=path, export_format='GLB', export_apply=True, export_yup=True, use_selection=True,
                              export_image_format='JPEG', export_jpeg_quality=88)
    info = {
        'file': name,
        'height': round(height, 4),
        'grip': round(grip_x / height, 4),
        'rise': round(rise, 4),
        'fore': round((fore_x - grip_x) / (height - grip_x), 4),
        'under': round(under, 4),
        'long': bool(long),
        'tris': sum(len(p.vertices) - 2 for p in o.data.polygons),
        'size_m': [round(v, 3) for v in (hi2 - lo2)],
        'bytes': os.path.getsize(path),
        'note': note,
    }
    with open(os.path.join(TRACES, name + '.json'), 'w') as f:
        json.dump(info, f, indent=1)
    print('MODEL', json.dumps(info))
    return info


def _ruler(lo, hi, fist, muzzle, other):
    """The side view's ruler, in gun space, between the gun and the camera:
    a white tick every tenth of the length under the gun and of the height
    behind it (the ends and the middle longer), a yellow cross on the fist,
    a red one on the muzzle and a green one on the other hand (each an
    (x, z), or None)."""
    size = max(hi - lo)
    t = size * 0.004
    y = lo.y - 0.01
    bm = bmesh.new()

    def bar(x, z, dx, dz, slot):
        verts = bmesh.ops.create_cube(bm, size=1.0)['verts']
        bmesh.ops.scale(bm, vec=Vector((dx, t, dz)), verts=verts)
        bmesh.ops.translate(bm, vec=Vector((x, y, z)), verts=verts)
        for f in {f for v in verts for f in v.link_faces}:
            f.material_index = slot

    for i in range(11):
        n = size * (0.03 if i % 5 == 0 else 0.015)
        bar(lo.x + (hi.x - lo.x) * i / 10, lo.z - size * 0.04, t, n, 0)
        bar(lo.x - size * 0.04, lo.z + (hi.z - lo.z) * i / 10, n, t, 0)
    for at, slot in ((fist, 1), (muzzle, 2), (other, 3)):
        if at:
            bar(at[0], at[1], size * 0.04, t, slot)
            bar(at[0], at[1], t, size * 0.04, slot)
    me = bpy.data.meshes.new('ruler')
    bm.to_mesh(me)
    bm.free()
    for name, hexs in (('tick', '#ffffff'), ('fist', '#ffd400'), ('muzzle', '#ff2020'), ('other', '#30ff40')):
        m = bpy.data.materials.new(name)
        m.use_nodes = True
        b = m.node_tree.nodes['Principled BSDF']
        b.inputs['Base Color'].default_value = (0, 0, 0, 1)
        b.inputs['Emission Color'].default_value = (*srgb(hexs), 1.0)
        b.inputs['Emission Strength'].default_value = 1.5
        me.materials.append(m)
    r = bpy.data.objects.new('ruler', me)
    bpy.context.scene.collection.objects.link(r)
    return r


def sheet(o, path, title='', fist=None, muzzle=None, other=None):
    """Four views of a gun-space object on one 2000 × 500 sheet: its side,
    a three-quarter from the front and above, from the top, and from behind
    its left shoulder, as its holder sees it (the end a side-on picture
    never showed Meshy, so the one it made up). Eevee, a sun and a soft
    fill, a mid-grey plate. With `fist` or `muzzle` the side view carries a
    ruler (`_ruler`)."""
    s = bpy.context.scene
    s.render.engine = 'BLENDER_EEVEE'
    s.eevee.taa_render_samples = 24
    s.eevee.use_bloom = True
    s.eevee.bloom_intensity = 0.04
    s.view_settings.view_transform = 'Standard'
    s.render.resolution_x = 2000
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
    # Four copies on a line along X, one view each, facing a camera on -Y.
    # The camera is on -Y: tipping the top toward it is a positive turn
    # about X.
    views = [
        Matrix.Identity(4),
        Matrix.Rotation(math.radians(25), 4, 'X') @ Matrix.Rotation(math.radians(35), 4, 'Z'),
        Matrix.Rotation(math.radians(90), 4, 'X'),
        Matrix.Rotation(math.radians(26), 4, 'X') @ Matrix.Rotation(math.radians(121), 4, 'Z'),
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
        c.matrix_world = Matrix.Translation(Vector(((i - 1.5) * size * 1.15, 0, 0))) @ m @ Matrix.Translation(-centre)
        copies.append(c)
    if fist or muzzle:
        r = _ruler(lo, hi, fist, muzzle, other)
        r.matrix_world = Matrix.Translation(Vector((-1.5 * size * 1.15, 0, 0))) @ Matrix.Translation(-centre)
        copies.append(r)
    cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam'))
    s.collection.objects.link(cam)
    cam.data.type = 'ORTHO'
    cam.data.ortho_scale = size * 4.67
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


def rest_bone(name):
    """Bone `name`'s rest transform in triple-t.glb's scene (4×4, metres;
    its own units are centimetres): what gear worn on it (`shades.py`,
    `boots.py`) is placed from, read from the file's nodes as the client
    reads them, not from an import, whose armature Blender rebuilds."""
    with open(os.path.join(MODELS, 'triple-t.glb'), 'rb') as f:
        b = f.read()
    n = struct.unpack('<I', b[12:16])[0]
    nodes = json.loads(b[20:20 + n])['nodes']
    up = {c: i for i, x in enumerate(nodes) for c in x.get('children', [])}

    def trs(x):
        if 'matrix' in x:
            return Matrix(np.array(x['matrix']).reshape(4, 4).T.tolist())
        q = x.get('rotation', [0, 0, 0, 1])
        t = Matrix.Translation(x.get('translation', [0, 0, 0]))
        r = Quaternion((q[3], q[0], q[1], q[2])).to_matrix().to_4x4()
        s = x.get('scale', [1, 1, 1])
        return t @ r @ Matrix.Diagonal((*s, 1.0))

    i = next(i for i, x in enumerate(nodes) if x.get('name') == name)
    m = trs(nodes[i])
    while i in up:
        i = up[i]
        m = trs(nodes[i]) @ m
    return m


def fit_line(const, bone, place):
    """The client's numbers for gear placed at `place` (4×4, triple-t.glb's
    scene at rest) on `bone`: a `look/bodies.rs` Fit, printed as FIT."""
    at, turn, scale = (rest_bone(bone).inverted() @ place).decompose()
    s = sum(scale) / 3
    print(f'FIT {const}: Fit {{ bone: "{bone}", at: Vec3::new({at.x:.3f}, {at.y:.3f}, {at.z:.3f}), '
          f'turn: Quat::from_xyzw({turn.x:.5f}, {turn.y:.5f}, {turn.z:.5f}, {turn.w:.5f}), scale: {s:.3f} }}')

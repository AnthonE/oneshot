"""The truck: the vehicle everyone drives, from a download rather than
from the kit's primitives.

    "Tesla Cybertruck" (https://skfb.ly/6QSBr) by hashikemu, CC-BY 4.0
    (NOTICE), as Sketchfab gives it: sources/tesla_cybertruck.glb (not in
    the repo; TRUCK_SRC names another place).

    blender -b --python tools/models/truck.py     (tools/models/build.sh truck)

What is done to it: its far LOD and the cracked-glass decal go; the tyres,
one mesh there, become four wheels with their origins on their axles, so
the client can turn them (`wheel_fl`, `wheel_fr`, `wheel_rl`, `wheel_rr`);
the rest is one `body`, the cover off its bed (there is a bed under it, and
the passenger stands in it); the textures it came with, flat colour mostly,
come down to TEX_PX and JPEG where nothing is see-through.

Its steel and its rubber are photographed sets from ambientCG (CC0, NOTICE),
unzipped beside the download: sources/Metal009_1K-JPG (brushed stainless)
and sources/Rubber004_1K-JPG. The steel panels take the first on UVs of
their own, a box projection in metres with the grain along the truck. The
tyres keep the map they came with (its tread, its rim) and take the
rubber's grain, a relief cut from the tread, and a rubber's roughness, where
the map they came with had the tread blocks glossy. Written as
crates/client/assets/models/truck.glb in the hull's frame (`look/truck.rs`):
+X forward, +Y up, +Z its right, the tyres standing on y = 0, the body's
footprint centred on the origin, metres, at the size it came (a real one's).
The wheels' places and radius are printed for truck.rs; the sheet goes to
traces/guns/truck.png.
"""
import json
import math
import os
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import MODELS, ROOT, TRACES, bounds, scene, sheet

SRC = os.environ.get('TRUCK_SRC', os.path.join(ROOT, 'sources', 'tesla_cybertruck.glb'))
# The longest side a texture keeps.
TEX_PX = 1024
SOURCES = os.path.dirname(SRC)
STEEL = os.path.join(SOURCES, 'Metal009_1K-JPG', 'Metal009_1K-JPG_{}.jpg')
RUBBER = os.path.join(SOURCES, 'Rubber004_1K-JPG', 'Rubber004_1K-JPG_{}.jpg')
# A tile of the steel covers this many metres of a panel.
STEEL_M = 1.2
# The tyres' map, as it came: this share of its height from the top is the
# rim and the sidewall around it, the rest the tread's strip.
TYRE_RIM = 0.256
# How steep the tread's relief is cut, and how much of the rubber's grain
# goes over it.
TREAD_CUT = 10.0
GRAIN = 0.5
# The bed's deck, over the ground: where the sim stands its passenger
# (`Vehicle::mount`). The bed as it came is a tub to the underbody.
DECK_Z = 0.9
# Gone: the far LOD (the client has one), and the decal of the cracked glass.
DROP = ('body_LOD1_body_LOD1_0', '0')
TYRES = 'tires_rubber_0'

if not os.path.exists(SRC):
    print(f'Error: no {SRC} (download it from https://skfb.ly/6QSBr, or set TRUCK_SRC)')
    sys.exit(1)

scene()
bpy.ops.import_scene.gltf(filepath=SRC)


def only(objs):
    bpy.ops.object.select_all(action='DESELECT')
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]


# Plain meshes in the world: no parents, no transforms left on them, one UV.
meshes = [o for o in bpy.data.objects if o.type == 'MESH']
for o in meshes:
    if o.name in DROP:
        bpy.data.objects.remove(o, do_unlink=True)
meshes = [o for o in bpy.data.objects if o.type == 'MESH']
only(meshes)
bpy.ops.object.parent_clear(type='CLEAR_KEEP_TRANSFORM')
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
for o in [o for o in bpy.data.objects if o.type != 'MESH']:
    bpy.data.objects.remove(o, do_unlink=True)
for o in meshes:
    while len(o.data.uv_layers) > 1:
        o.data.uv_layers.remove(o.data.uv_layers[-1])

# As it came (in Blender, Z up): its nose toward -Y, its left toward +X.
# The hull's frame here is +X forward, +Y left (the export makes that +Z
# right), so a quarter turn about Z; then its footprint centred, tyres on 0.
turn = Matrix.Rotation(math.radians(90), 4, 'Z')
for o in meshes:
    o.data.transform(turn)
    o.data.update()
lo = Vector((1e9,) * 3)
hi = Vector((-1e9,) * 3)
for o in meshes:
    a, b = bounds(o)
    lo = Vector(map(min, lo, a))
    hi = Vector(map(max, hi, b))
centre = Matrix.Translation(Vector((-(lo.x + hi.x) / 2, -(lo.y + hi.y) / 2, -lo.z)))
for o in meshes:
    o.data.transform(centre)
    o.data.update()

# The wheels: the tyres' loose pieces by the corner they stand in.
tyres = bpy.data.objects[TYRES]
only([tyres])
bpy.ops.mesh.separate(type='LOOSE')
pieces = [o for o in bpy.context.selected_objects]
CORNERS = {(1, 1): 'wheel_fl', (1, -1): 'wheel_fr', (-1, 1): 'wheel_rl', (-1, -1): 'wheel_rr'}
groups = {name: [] for name in CORNERS.values()}
for p in pieces:
    a, b = bounds(p)
    c = (a + b) / 2
    groups[CORNERS[(1 if c.x > 0 else -1, 1 if c.y > 0 else -1)]].append(p)
wheels = {}
for name, mine in groups.items():
    only(mine)
    bpy.ops.object.join()
    w = bpy.context.view_layer.objects.active
    w.name = name
    w.data.name = name
    bpy.ops.object.origin_set(type='ORIGIN_GEOMETRY', center='BOUNDS')
    wheels[name] = w

# The rest is the body.
rest = [o for o in bpy.data.objects if o.type == 'MESH' and o.name not in wheels]
only(rest)
bpy.ops.object.join()
body = bpy.context.view_layer.objects.active
body.name = 'body'
body.data.name = 'body'

# The bed's cover off: the dark panel over it (the strip at the tail is
# the lamp's glass, and stays).
bm = bmesh.new()
bm.from_mesh(body.data)
glass = [i for i, m in enumerate(body.data.materials) if m.name == 'glass']
cover = [
    f
    for f in bm.faces
    if f.material_index in glass and -2.8 < f.calc_center_median().x < -0.9 and f.calc_center_median().z > 1.2
]
bmesh.ops.delete(bm, geom=cover, context='FACES')

# The steel's own UVs: each panel projected along the axis it faces, in
# metres, so the brushing runs along the truck (and across its nose and tail).
steel_slots = [i for i, m in enumerate(body.data.materials) if m.name == 'body']
uv = bm.loops.layers.uv.active
for f in bm.faces:
    if f.material_index not in steel_slots:
        continue
    n = f.normal
    for loop in f.loops:
        c = loop.vert.co
        if abs(n.z) >= abs(n.x) and abs(n.z) >= abs(n.y):
            at = (c.x, c.y)
        elif abs(n.y) >= abs(n.x):
            at = (c.x, c.z)
        else:
            at = (c.y, c.z)
        loop[uv].uv = (at[0] / STEEL_M, at[1] / STEEL_M)
bm.to_mesh(body.data)
bm.free()
body.data.update()

# A deck across the bed for the passenger to stand on: a plate from wall
# to wall at DECK_Z, in the tub's own grey.
mid = Vector((-1.8, 0.0, DECK_Z + 0.1))
reach = lambda d: body.ray_cast(mid, Vector(d))[1]
x0, x1 = reach((-1, 0, 0)).x + 0.01, reach((1, 0, 0)).x - 0.01
y0, y1 = reach((0, -1, 0)).y + 0.01, reach((0, 1, 0)).y - 0.01
_, _, _, tub = body.ray_cast(mid, Vector((0, 0, -1)))
bm = bmesh.new()
bm.from_mesh(body.data)
bm.faces.ensure_lookup_table()
uv = bm.loops.layers.uv.active
grey = bm.faces[tub]
at = sum((loop[uv].uv for loop in grey.loops), Vector((0.0, 0.0))) / len(grey.loops)
deck = bm.faces.new([bm.verts.new((x, y, DECK_Z)) for x, y in ((x0, y0), (x1, y0), (x1, y1), (x0, y1))])
deck.material_index = grey.material_index
for loop in deck.loops:
    loop[uv].uv = at
bm.to_mesh(body.data)
bm.free()
body.data.update()


def pixels(path_or_image):
    """An image's pixels, rows from the bottom: (h, w, 4) floats as stored."""
    im = bpy.data.images.load(path_or_image) if isinstance(path_or_image, str) else path_or_image
    w, h = im.size
    a = np.empty(w * h * 4, dtype=np.float32)
    im.pixels.foreach_get(a)
    return a.reshape(h, w, 4)


def image(name, a, data):
    """A new image of `a` ((h, w, 3) floats); `data` for one that isn't colour."""
    h, w, _ = a.shape
    im = bpy.data.images.new(name, w, h, alpha=False)
    im.colorspace_settings.name = 'Non-Color' if data else 'sRGB'
    rgba = np.ones((h, w, 4), dtype=np.float32)
    rgba[..., :3] = np.clip(a, 0.0, 1.0)
    im.pixels.foreach_set(rgba.ravel())
    im.pack()
    return im


def pbr(name, colour, orm, normal):
    """A material of three maps: colour, occlusion-roughness-metal as glTF
    packs them (G rough, B metal), and a normal."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    out = nt.nodes['Principled BSDF']
    tex = {}
    for key, im in (('colour', colour), ('orm', orm), ('normal', normal)):
        t = nt.nodes.new('ShaderNodeTexImage')
        t.image = im
        tex[key] = t
    nt.links.new(tex['colour'].outputs['Color'], out.inputs['Base Color'])
    split = nt.nodes.new('ShaderNodeSeparateColor')
    nt.links.new(tex['orm'].outputs['Color'], split.inputs['Color'])
    nt.links.new(split.outputs['Green'], out.inputs['Roughness'])
    nt.links.new(split.outputs['Blue'], out.inputs['Metallic'])
    bump = nt.nodes.new('ShaderNodeNormalMap')
    nt.links.new(tex['normal'].outputs['Color'], bump.inputs['Color'])
    nt.links.new(bump.outputs['Normal'], out.inputs['Normal'])
    return m


def resized(a, w, h):
    """`a` at another size (Blender's own scaling)."""
    im = image('tmp', a[..., :3], True)
    im.scale(w, h)
    out = pixels(im)[..., :3].copy()
    bpy.data.images.remove(im)
    return out


# The steel: the scan's colour and normal as they are, its roughness as G
# of the packed map, all of it metal.
# (Roughness in R as well as G: a JPEG keeps what is in its luma.)
rough = pixels(STEEL.format('Roughness'))[..., 0]
orm = np.stack([rough, rough, np.ones_like(rough)], axis=-1)
steel = pbr(
    'steel',
    image('steel_colour', pixels(STEEL.format('Color'))[..., :3], False),
    image('steel_orm', orm, True),
    image('steel_normal', pixels(STEEL.format('NormalGL'))[..., :3], True),
)
for i in steel_slots:
    body.data.materials[i] = steel

# The tyres: the map they came with at twice its size, under the rubber.
was = wheels['wheel_fl'].data.materials[0]
base = next(n.image for n in was.node_tree.nodes if n.type == 'TEX_IMAGE' and n.image.colorspace_settings.name == 'sRGB')
w, h = base.size[0] * 2, base.size[1] * 2
base = resized(pixels(base), w, h)
lum = base.mean(axis=-1)
tile = lambda a: np.tile(resized(a, w, w), (h // w, 1, 1))
grain = tile(pixels(RUBBER.format('Color')))
grain_n = tile(pixels(RUBBER.format('NormalGL'))) * 2.0 - 1.0
grain_r = tile(pixels(RUBBER.format('Roughness')))[..., 0]
# Rows from the bottom: the rim and its sidewall are the top of the map.
rows = np.arange(h)[:, None] >= round(h * (1.0 - TYRE_RIM))
tread = ~rows & np.ones((h, w), dtype=bool)
# The wheel's cover is the grey disc in the rim's square; the rest is rubber.
cover_disc = rows & (lum > 0.2) & (base.max(axis=-1) - base.min(axis=-1) < 0.03)
rubber = ~cover_disc
colour = base * np.where(rubber, 0.75 + 0.5 * grain.mean(axis=-1) / grain.mean(), 1.0)[..., None]
# The relief: the tread's blocks stand over its grooves (the map is lighter
# on them), softened a texel, wrapping where the strip repeats.
soft = sum(np.roll(np.roll(lum, i, axis=1), j, axis=0) for i in (-1, 0, 1) for j in (-1, 0, 1)) / 9.0
dx = (np.roll(soft, -1, axis=1) - np.roll(soft, 1, axis=1)) * TREAD_CUT
dy = (np.roll(soft, -1, axis=0) - np.roll(soft, 1, axis=0)) * TREAD_CUT
n = np.stack([-dx, -dy, np.ones_like(dx)], axis=-1)
n[..., :2] += grain_n[..., :2] * GRAIN * rubber[..., None]
n /= np.linalg.norm(n, axis=-1, keepdims=True)
# Rubber is matt, its grooves more than its blocks; the cover is a satin
# plastic.
rough = np.where(rubber, 0.95 - 0.6 * np.clip(soft, 0.0, 0.25) + (grain_r - grain_r.mean()) * 0.5, 0.45)
orm = np.stack([rough, rough, np.zeros_like(rough)], axis=-1)
tyre = pbr('tyre', image('tyre_colour', colour, False), image('tyre_orm', orm, True), image('tyre_normal', n * 0.5 + 0.5, True))
for wheel in wheels.values():
    wheel.data.materials[0] = tyre
# The roughness needs no more texels than the map it came with had.
bpy.data.images['tyre_orm'].scale(w // 2, h // 2)

# Smaller textures: what it came with is flat colour and baked shade.
for im in bpy.data.images:
    w, h = im.size
    if max(w, h) > TEX_PX and not im.name.startswith('tyre'):
        k = TEX_PX / max(w, h)
        im.scale(max(1, round(w * k)), max(1, round(h * k)))

os.makedirs(MODELS, exist_ok=True)
os.makedirs(TRACES, exist_ok=True)
path = os.path.join(MODELS, 'truck.glb')
only([body, *wheels.values()])
bpy.ops.export_scene.gltf(
    filepath=path,
    export_format='GLB',
    export_apply=True,
    export_yup=True,
    use_selection=True,
    export_image_format='JPEG',
    # High: the steel's brushing is fine lines a harder squeeze rubs out.
    # (Blender 4.0 reads this one; its `export_jpeg_quality` does nothing.)
    export_image_quality=95,
)

blo, bhi = bounds(body)
radius = max((bounds(w)[1].z - bounds(w)[0].z) / 2 for w in wheels.values())
info = {
    'file': 'truck',
    'size_m': [round(v, 3) for v in (max(bhi.x, hi.x - (lo.x + hi.x) / 2) - min(blo.x, lo.x - (lo.x + hi.x) / 2), bhi.z, hi.y - lo.y)],
    'wheel_radius': round(radius, 3),
    # In the hull's frame: x forward, z right.
    'wheels': {n: [round(w.location.x, 3), round(w.location.z, 3), round(-w.location.y, 3)] for n, w in wheels.items()},
    'deck': [round(v, 3) for v in (x0, x1, y0, y1, DECK_Z)],
    'tris': sum(len(p.vertices) - 2 for o in (body, *wheels.values()) for p in o.data.polygons),
    'bytes': os.path.getsize(path),
}
with open(os.path.join(TRACES, 'truck.json'), 'w') as f:
    json.dump(info, f, indent=1)
print('MODEL', json.dumps(info))

# The sheet: one mesh of it all, nose along +X as a gun's muzzle is.
only([body, *wheels.values()])
bpy.ops.object.duplicate()
bpy.ops.object.join()
whole = bpy.context.view_layer.objects.active
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
sheet(whole, os.path.join(TRACES, 'truck.png'), title='truck')

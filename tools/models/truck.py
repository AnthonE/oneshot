"""The truck: the vehicle everyone drives, from a download rather than
from the kit's primitives.

    "Tesla Cybertruck" (https://skfb.ly/6QSBr) by hashikemu, CC-BY 4.0
    (NOTICE), as Sketchfab gives it: sources/tesla_cybertruck.glb (not in
    the repo; TRUCK_SRC names another place).

    blender -b --python tools/models/truck.py     (tools/models/build.sh truck)

What is done to it: its far LOD and the cracked-glass decal go; the tyres,
one mesh there, become four wheels with their origins on their axles, so
the client can turn them (`wheel_fl`, `wheel_fr`, `wheel_rl`, `wheel_rr`);
the rest is one `body`; the textures, flat colour mostly, come down to
TEX_PX and JPEG where nothing is see-through. Written as
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

import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import MODELS, ROOT, TRACES, bounds, scene, sheet

SRC = os.environ.get('TRUCK_SRC', os.path.join(ROOT, 'sources', 'tesla_cybertruck.glb'))
# The longest side a texture keeps.
TEX_PX = 1024
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

# Smaller textures: they are flat colour and baked shade.
for im in bpy.data.images:
    w, h = im.size
    if max(w, h) > TEX_PX:
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
    export_jpeg_quality=85,
)

blo, bhi = bounds(body)
radius = max((bounds(w)[1].z - bounds(w)[0].z) / 2 for w in wheels.values())
info = {
    'file': 'truck',
    'size_m': [round(v, 3) for v in (max(bhi.x, hi.x - (lo.x + hi.x) / 2) - min(blo.x, lo.x - (lo.x + hi.x) / 2), bhi.z, hi.y - lo.y)],
    'wheel_radius': round(radius, 3),
    # In the hull's frame: x forward, z right.
    'wheels': {n: [round(w.location.x, 3), round(w.location.z, 3), round(-w.location.y, 3)] for n, w in wheels.items()},
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

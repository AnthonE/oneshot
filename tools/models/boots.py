"""The boots: black platform boots, buckled up the shaft, a deep-cleated
sole. Footwear (`proto::FOOTWEAR`), worn over Triple T's root feet. The
operator's own model on Meshy (task 79e9b68b-28fc-4bd8-9d9c-4d57c6defbb1,
"gothic_platform_boot", one boot, fetched by the API into
sources/meshy/boot_raw.glb).

Meshy's boot is 1.6 million triangles, cut into thousands of islands for
its maps; the game's is a few thousand, welded, thinned and laid out
afresh, its colour and the lost detail baked onto it from the whole one
(`thin`, `bake`).

One boot comes in; two go out. The right is Meshy's as it is, the left its
mirror image: the mesh reflected across its middle and its faces turned
back the right way out (`mirror`), a boot of its own, not the right one
turned round. Each is exported with the middle of its sole at the origin,
+Y up, +Z toward the toe, in metres: models/boot-right.glb and
models/boot-left.glb.

`look/bodies.rs` hangs each on its foot's bone; the numbers for that
(`BOOT_LEFT`, `BOOT_RIGHT` there) are printed here as FIT, from where the
feet are in triple-t.glb at rest (`FEET`). Look at them in the game:

  ci/desktop_shot.sh traces/boots.png 2 0 0 0 \
    'pose=yaw=3.7&footwear=1&fighter=1&eye=0.2,0.6,8.6&look=1.5,0.25,8'"""
import json
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

NAME = 'boot'
# Heel to toe and across, metres: Triple T's feet are roots 0.36 long and
# splayed to 0.2 wide at the toes; the boot is chunky enough to cover them.
LENGTH = 0.46
WIDTH = 0.27
TRIS = 4000
# The thin boot's maps, baked from Meshy's whole mesh (`bake`): colour,
# and the detail its triangles lost as a normal map; px.
COLOUR_PX = TEX_PX
DETAIL_PX = MAP_PX
# Its surface where no map says: black leather and steel buckles.
ROUGH_ALL = 0.45
METAL_ALL = 0.25
# Each sole's middle in triple-t.glb's scene at rest (metres, +Y up, +Z the
# way it faces; the left foot is +X), and how far each foot is turned out,
# degrees: the toes splay outward.
FEET = (0.17, 0.0, 0.04)
TOE_OUT = 12.0


def raw():
    """Meshy's whole mesh joined, its toe toward -Y (glTF's +Z), up +Z,
    LENGTH long and WIDTH across, the middle of its sole at the origin."""
    path = os.path.join(SOURCES, NAME + '_raw.glb')
    if not os.path.exists(path):
        print(f'Error: no {path}: the operator\'s model on Meshy, by the API (task in this file\'s notes)')
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
    o.name = NAME

    # Long side forward, the toe the low end of it (the shaft is at the heel).
    lo, hi = bounds(o)
    ext = hi - lo
    fwd = 0 if ext.x >= ext.y else 1
    co = np.array([v.co[:] for v in o.data.vertices])
    a = co[co[:, fwd] <= lo[fwd] + 0.25 * ext[fwd]]
    b = co[co[:, fwd] >= hi[fwd] - 0.25 * ext[fwd]]
    toe_low = a[:, 2].max() < b[:, 2].max()  # the toe at the low end of fwd
    # The toe to -Y.
    if fwd == 0:
        o.data.transform(Matrix.Rotation(math.radians(90 if toe_low else -90), 4, 'Z'))
    elif not toe_low:
        o.data.transform(Matrix.Rotation(math.pi, 4, 'Z'))
    lo, hi = bounds(o)
    k = LENGTH / (hi.y - lo.y)
    o.data.transform(Matrix.Diagonal((WIDTH / (hi.x - lo.x), k, k, 1.0)))
    lo, hi = bounds(o)
    o.data.transform(Matrix.Translation(Vector(((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, lo.z)) * -1))
    o.data.update()
    return o


def thin(high):
    """The game's boot from the whole one: welded where Meshy split it for
    its maps (thousands of islands, which a thinning can't cross: it came
    out crumpled), thinned to about TRIS, smooth, and laid out afresh for
    its own maps."""
    bpy.ops.object.select_all(action='DESELECT')
    high.select_set(True)
    bpy.context.view_layer.objects.active = high
    bpy.ops.object.duplicate()
    o = bpy.context.view_layer.objects.active
    o.name = 'boot'
    high.name = 'boot_whole'
    o.data.materials.clear()
    while o.data.uv_layers:
        o.data.uv_layers.remove(o.data.uv_layers[0])
    bm = bmesh.new()
    bm.from_mesh(o.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts[:], dist=1e-4)
    bm.to_mesh(o.data)
    bm.free()
    count = sum(len(p.vertices) - 2 for p in o.data.polygons)
    if count > TRIS:
        m = o.modifiers.new('thin', 'DECIMATE')
        m.ratio = TRIS / count
        bpy.ops.object.modifier_apply(modifier='thin')
    if o.data.has_custom_normals:
        bpy.ops.mesh.customdata_custom_splitnormals_clear()
    bpy.ops.object.shade_smooth()
    o.data.use_auto_smooth = True
    o.data.auto_smooth_angle = math.radians(50)
    o.data.uv_layers.new(name='UVMap')
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.uv.smart_project(angle_limit=math.radians(60), island_margin=0.004)
    bpy.ops.object.mode_set(mode='OBJECT')
    return o


def bake(high, o):
    """The whole boot's colour and its detail baked onto the thin one's
    own maps (Cycles, the CPU), and its material made of them."""
    s = bpy.context.scene
    s.render.engine = 'CYCLES'
    s.cycles.device = 'CPU'
    s.cycles.samples = 4
    s.render.bake.use_selected_to_active = True
    s.render.bake.cage_extrusion = 0.012
    s.render.bake.max_ray_distance = 0.03
    s.render.bake.margin = 6
    # Meshy's own maps to a working size first: its colour is 4K.
    for im in bpy.data.images:
        w, h = im.size
        if max(w, h) > 2048:
            im.scale(2048, 2048)
    m = bpy.data.materials.new('boot')
    m.use_nodes = True
    o.data.materials.append(m)
    nt = m.node_tree
    bsdf = nt.nodes['Principled BSDF']
    images = {}
    for kind, px, data in (('colour', COLOUR_PX, False), ('detail', DETAIL_PX, True)):
        im = bpy.data.images.new(f'boot_{kind}', px, px, alpha=False, is_data=data)
        node = nt.nodes.new('ShaderNodeTexImage')
        node.image = im
        images[kind] = node
    bpy.ops.object.select_all(action='DESELECT')
    high.select_set(True)
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    for kind, args in (('colour', dict(type='DIFFUSE', pass_filter={'COLOR'})),
                       ('detail', dict(type='NORMAL', normal_space='TANGENT'))):
        for n in nt.nodes:
            n.select = False
        nt.nodes.active = images[kind]
        bpy.ops.object.bake(**args)
        images[kind].image.pack()
    nt.links.new(images['colour'].outputs['Color'], bsdf.inputs['Base Color'])
    nmap = nt.nodes.new('ShaderNodeNormalMap')
    nmap.inputs['Strength'].default_value = NORMAL
    nt.links.new(images['detail'].outputs['Color'], nmap.inputs['Color'])
    nt.links.new(nmap.outputs['Normal'], bsdf.inputs['Normal'])
    bsdf.inputs['Roughness'].default_value = ROUGH_ALL
    bsdf.inputs['Metallic'].default_value = METAL_ALL
    bpy.data.objects.remove(high, do_unlink=True)


def mirror(o):
    """`o` reflected across X (its left for its right), its faces turned
    back so they face out."""
    o.data.transform(Matrix.Diagonal((-1.0, 1.0, 1.0, 1.0)))
    bm = bmesh.new()
    bm.from_mesh(o.data)
    bmesh.ops.reverse_faces(bm, faces=bm.faces[:])
    bm.to_mesh(o.data)
    bm.free()
    o.data.update()


def export(o, name):
    bpy.ops.object.select_all(action='DESELECT')
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    out = os.path.join(MODELS, name + '.glb')
    # Blender's -Y forward, +Z up is glTF's +Z forward, +Y up.
    bpy.ops.export_scene.gltf(filepath=out, export_format='GLB', export_apply=True, export_yup=True,
                              use_selection=True, export_tangents=True, export_image_format='JPEG', export_jpeg_quality=88)
    lo, hi = bounds(o)
    print('MODEL', json.dumps({'file': name, 'tris': sum(len(p.vertices) - 2 for p in o.data.polygons),
                               'size_m': [round(v, 3) for v in (hi - lo)], 'bytes': os.path.getsize(out)}))


def fit():
    """Each boot on its foot's bone: its sole under the foot, turned out
    TOE_OUT, as the client's numbers."""
    x, y, z = FEET
    for side, s in (('Left', 1.0), ('Right', -1.0)):
        place = Matrix.Translation((s * x, y, z)) @ Matrix.Rotation(math.radians(s * TOE_OUT), 4, 'Y')
        fit_line('BOOT_' + side.upper(), side + 'Foot', place)


scene()
os.makedirs(MODELS, exist_ok=True)
high = raw()
o = thin(high)
bake(high, o)
export(o, 'boot-right')
mirror(o)
export(o, 'boot-left')
fit()

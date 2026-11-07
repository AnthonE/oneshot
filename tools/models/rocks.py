"""The rocks: Dry Fork's boulders and its sea stack, Cloudbreak's wind-carved
boulders, stacks and hanging island, from Meshy
(tools/models/meshy_gen.py boulder-a --multiview --tris 12000 …; the prompts
are in each raw's sidecar, sources/meshy/<slug>.json).

    blender -b --python tools/models/rocks.py            all of them
    blender -b --python tools/models/rocks.py -- boulder_a

A BOULDER is drawn in place of a rock box the sim collides (`look/boulders.rs`),
so it is cut to fill one: brought to a unit cube by its own bounds, then
every point pushed FILL of the way out to the cube's surface along the ray
from its middle, which squares a lump up into a block and keeps (1 − FILL)
of its relief. Nothing of it is outside the cube, so nothing drawn is
outside what collides; how far inside it sits is printed (`inset`: the mean
and the worst, as shares of the cube's side) and goes in `boulders.rs`
beside the size it came at (`native`), which says which boxes it suits.

The STACK is scenery out at sea, nobody's to reach: only thinned, stood on
its base at the origin and scaled to STACK_M tall.

Each is thinned as a copy with a map of its own (`low`), and Meshy's mesh
and paint are baked onto that (Cycles, colour and tangent normals): thinned
in place, a mesh cut along every seam of Meshy's map smears the paint
across them. It goes to crates/client/assets/models/rock/<name>.glb with
the two maps at PX, as JPEG; roughness is one number (ROUGH) and Meshy's
other maps are dropped. A sheet of each lands in traces/rocks/.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

import bmesh

# name: its raw's slug
BOULDERS = {'boulder_a': 'boulder-a', 'boulder_b': 'boulder-b', 'boulder_c': 'boulder-c'}
# An island's boulders: name: (its raw's slug, how far it is pushed to its
# cube). Wind-carved round, so hardly: what they lose at the box's corners they
# keep in their shape.
ISLE_BOULDERS = {
    'isle_boulder_a': ('isle-boulder-a', 0.15),
    'isle_boulder_b': ('isle-boulder-b', 0.15),
    'isle_boulder_c': ('isle-boulder-c', 0.12),
}
ISLE_BOULDER_TRIS = 5000
# Built things cut the same way, to fill the box the sim collides: name:
# (its raw's slug, how far it is pushed out to its cube, its folder under
# models). The pylon is a base's buttress (a `pylon` look's turned box).
BLOCKS = {'pylon': ('pylon', 0.2, 'base'), 'husk': ('husk', 0.04, 'ship'), 'font': ('font', 0.04, 'ship'),
          'wreck_car': ('wreck-sedan', 0.02, 'wreck'), 'wreck_van': ('wreck-van', 0.02, 'wreck')}
# A built thing's faces are flat and its edges hard: thinned as far as a
# boulder, its panels buckle.
BLOCK_TRIS = 9000
# name: (its raw's slug, triangles, map's side). The hanging island is seen up
# close from the rim.
STACKS = {
    'sea_stack': ('sea-stack', 7000, 1024),
    'isle_stack_a': ('isle-stack-a', 12000, 1536),
    'isle_stack_b': ('isle-stack-b', 12000, 1536),
    'isle_float': ('isle-float', 22000, 2048),
}
# A ship's prop's creases: its occlusion, baked over this reach on its unit
# cube, at this many samples, raised to this power over its colour.
AO_REACH = 0.22
AO_SAMPLES = 48
AO_POWER = 1.5
# How far a boulder's surface is pushed out toward its cube, 0..1.
FILL = 0.35
BOULDER_TRIS = 2600
STACK_M = 1.0
PX = 1024
ROUGH = 0.92
# The bake's cage and its reach, as shares of a boulder's side (of a stack's
# height): how far off the thinned copy Meshy's surface may lie.
BAKE_CAGE = 0.03
BAKE_REACH = 0.12
OUT = os.path.join(MODELS, 'rock')
SHEETS = os.path.join(ROOT, 'traces', 'rocks')


def load(slug):
    path = os.path.join(SOURCES, slug + '_raw.glb')
    if not os.path.exists(path):
        print(f'Error: no {path}; tools/models/meshy_gen.py {slug} commissions it (credits)')
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
    return o


def low(hi, tris, name, hard=False, ao=False, px=PX):
    """A thinned copy of `hi` with one map of its own, Meshy's mesh and paint
    baked onto it: (the copy, its colour's mean, linear). With `ao`, its own
    occlusion is multiplied into the colour, so a crease reads dark."""
    bpy.ops.object.select_all(action='DESELECT')
    o = hi.copy()
    o.data = hi.data.copy()
    o.name = name
    bpy.context.scene.collection.objects.link(o)
    bpy.context.view_layer.objects.active = o
    o.select_set(True)
    # Its normals are its own from here: Meshy's were the scan's.
    if o.data.has_custom_normals:
        bpy.ops.mesh.customdata_custom_splitnormals_clear()
    # Welded first: Meshy's mesh is cut along every seam of its map.
    bm = bmesh.new()
    bm.from_mesh(o.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    bm.to_mesh(o.data)
    bm.free()
    count = sum(len(p.vertices) - 2 for p in o.data.polygons)
    if count > tris:
        m = o.modifiers.new('thin', 'DECIMATE')
        m.ratio = tris / count
        bpy.ops.object.modifier_apply(modifier=m.name)
    bm = bmesh.new()
    bm.from_mesh(o.data)
    bmesh.ops.triangulate(bm, faces=bm.faces)
    bmesh.ops.dissolve_degenerate(bm, edges=bm.edges, dist=1e-6)
    bm.to_mesh(o.data)
    bm.free()
    o.data.validate(clean_customdata=False)
    for p in o.data.polygons:
        p.use_smooth = True
    if hard:
        # A built thing's faces meet at edges: smoothed across them, each
        # flat panel is shaded as a pillow.
        o.data.use_auto_smooth = True
        o.data.auto_smooth_angle = math.radians(38)
    o.data.update()

    # A map of its own.
    while o.data.uv_layers:
        o.data.uv_layers.remove(o.data.uv_layers[0])
    o.data.uv_layers.new(name='UVMap')
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.uv.smart_project(angle_limit=math.radians(66), island_margin=0.012)
    bpy.ops.object.mode_set(mode='OBJECT')

    material = bpy.data.materials.new(name)
    material.use_nodes = True
    nt = material.node_tree
    bsdf = next(n for n in nt.nodes if n.type == 'BSDF_PRINCIPLED')
    bsdf.inputs['Metallic'].default_value = 0.0
    bsdf.inputs['Roughness'].default_value = ROUGH
    colour = bpy.data.images.new(name + '_colour', px, px, alpha=False)
    normal = bpy.data.images.new(name + '_normal', px, px, alpha=False)
    normal.colorspace_settings.name = 'Non-Color'
    ct = nt.nodes.new('ShaderNodeTexImage')
    ct.image = colour
    nm = nt.nodes.new('ShaderNodeTexImage')
    nm.image = normal
    o.data.materials.clear()
    o.data.materials.append(material)

    s = bpy.context.scene
    s.render.engine = 'CYCLES'
    s.cycles.device = 'CPU'
    s.cycles.samples = 4
    # This box's Blender is built without the denoiser, and a bake that
    # asks for it comes back black.
    s.cycles.use_denoising = False
    bpy.ops.object.select_all(action='DESELECT')
    hi.select_set(True)
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    for node, kind, extra in ((nm, 'NORMAL', {'normal_space': 'TANGENT'}), (ct, 'EMIT', {})):
        if kind == 'EMIT':
            # Meshy's paint as light, so the bake carries the colour alone:
            # as a surface it is what the scene's light makes of it (and of
            # what Meshy calls metal, nothing).
            for src in hi.data.materials:
                st = src.node_tree
                b = next(n for n in st.nodes if n.type == 'BSDF_PRINCIPLED')
                out = next(n for n in st.nodes if n.type == 'OUTPUT_MATERIAL')
                glow = st.nodes.new('ShaderNodeEmission')
                st.links.new(b.inputs['Base Color'].links[0].from_socket, glow.inputs['Color'])
                st.links.new(glow.outputs['Emission'], out.inputs['Surface'])
        for n in nt.nodes:
            n.select = False
        node.select = True
        nt.nodes.active = node
        # Said here: run from a script, the operator reads its own
        # settings and not the scene's.
        bpy.ops.object.bake(type=kind, use_selected_to_active=True, cage_extrusion=BAKE_CAGE,
                            max_ray_distance=BAKE_REACH, margin=12, **extra)
    if ao:
        occl = bpy.data.images.new(name + '_ao', px, px, alpha=False)
        occl.colorspace_settings.name = 'Non-Color'
        at = nt.nodes.new('ShaderNodeTexImage')
        at.image = occl
        for n in nt.nodes:
            n.select = False
        at.select = True
        nt.nodes.active = at
        hi.hide_render = True
        hi.select_set(False)
        s.cycles.samples = AO_SAMPLES
        if s.world is None:
            s.world = bpy.data.worlds.new('w')
        s.world.light_settings.distance = AO_REACH
        bpy.ops.object.bake(type='AO', use_selected_to_active=False, margin=12)
        c = np.array(colour.pixels[:]).reshape(-1, 4)
        a = np.array(occl.pixels[:]).reshape(-1, 4)[:, :1]
        c[:, :3] *= a ** AO_POWER
        colour.pixels[:] = c.ravel()
        nt.nodes.remove(at)
    nt.links.new(ct.outputs['Color'], bsdf.inputs['Base Color'])
    bump = nt.nodes.new('ShaderNodeNormalMap')
    nt.links.new(nm.outputs['Color'], bump.inputs['Color'])
    nt.links.new(bump.outputs['Normal'], bsdf.inputs['Normal'])
    # The exporter writes a generated image from its file.
    for im, tag in ((colour, 'colour'), (normal, 'normal')):
        im.filepath_raw = os.path.join(SHEETS, f'{name}_{tag}.png')
        im.file_format = 'PNG'
        im.save()
    px = np.array(colour.pixels[:]).reshape(-1, 4)[:, :3]
    # The map's unpainted texels are black: the mean of what is painted.
    painted = px[px.sum(axis=1) > 0.01]
    mean = [round(float(v), 4) for v in painted.mean(axis=0)]
    bpy.data.objects.remove(hi, do_unlink=True)
    return o, mean


def export(o, name, out=OUT):
    os.makedirs(out, exist_ok=True)
    os.makedirs(SHEETS, exist_ok=True)
    sheet(o, os.path.join(SHEETS, name + '.png'), title=name)
    bpy.ops.object.select_all(action='DESELECT')
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    path = os.path.join(out, name + '.glb')
    bpy.ops.export_scene.gltf(filepath=path, export_format='GLB', export_apply=True, export_yup=True, use_selection=True,
                              export_image_format='JPEG', export_jpeg_quality=86, export_tangents=True)
    return path


def volume(o):
    bm = bmesh.new()
    bm.from_mesh(o.data)
    v = abs(bm.calc_volume())
    bm.free()
    return v


def boulder(name, slug, fill=None, out=OUT, rough=None, tris=BOULDER_TRIS, hard=False, ao=False):
    fill = FILL if fill is None else fill
    scene()
    o = load(slug)
    lo, hi = bounds(o)
    native = hi - lo
    mid = (lo + hi) / 2
    # To the unit cube, by its own bounds.
    o.data.transform(Matrix.Diagonal((1 / native.x, 1 / native.y, 1 / native.z, 1)) @ Matrix.Translation(-mid))
    co = np.array([v.co[:] for v in o.data.vertices])
    far = np.abs(co).max(axis=1, keepdims=True)
    on_cube = co * (0.5 / np.maximum(far, 1e-6))
    co = co + (on_cube - co) * fill
    # Exactly the cube again: the push never leaves it, but a face it did
    # not reach is brought up to it.
    lo2, hi2 = co.min(axis=0), co.max(axis=0)
    co = (co - (lo2 + hi2) / 2) / (hi2 - lo2)
    o.data.vertices.foreach_set('co', co.ravel())
    o.data.update()
    os.makedirs(SHEETS, exist_ok=True)
    o, mean = low(o, tris, name, hard, ao)
    if rough is not None:
        for material in o.data.materials:
            next(n for n in material.node_tree.nodes if n.type == 'BSDF_PRINCIPLED').inputs['Roughness'].default_value = rough
    co = np.array([v.co[:] for v in o.data.vertices])
    inset = 0.5 - np.abs(co).max(axis=1)
    path = export(o, name, out)
    # Blender's (x, y, z) leaves as glTF's (x, z, −y): the size it came at,
    # in the client's axes (x, up, z).
    info = {
        'file': name,
        'native': [round(native.x, 3), round(native.z, 3), round(native.y, 3)],
        'fill': round(volume(o), 3),
        'inset_mean': round(float(inset.mean()), 4),
        'inset_max': round(float(inset.max()), 4),
        # How far it reaches from its upright axis: over 0.5, it leaves a
        # round solid of the cube's width.
        'radial': round(float(np.hypot(co[:, 0], co[:, 1]).max()), 4),
        'colour_mean': mean,
        'tris': sum(len(p.vertices) - 2 for p in o.data.polygons),
        'bytes': os.path.getsize(path),
    }
    with open(os.path.join(SHEETS, name + '.json'), 'w') as f:
        json.dump(info, f, indent=1)
    print('MODEL', json.dumps(info))


def stack(name, slug, tris, px):
    scene()
    o = load(slug)
    lo, hi = bounds(o)
    k = STACK_M / (hi.z - lo.z)
    mid = (lo + hi) / 2
    o.data.transform(Matrix.Scale(k, 4) @ Matrix.Translation(Vector((-mid.x, -mid.y, -lo.z))))
    o.data.update()
    os.makedirs(SHEETS, exist_ok=True)
    o, mean = low(o, tris, name, px=px)
    lo, hi = bounds(o)
    path = export(o, name)
    info = {
        'file': name,
        'size': [round(hi.x - lo.x, 3), round(hi.z - lo.z, 3), round(hi.y - lo.y, 3)],
        'colour_mean': mean,
        'tris': sum(len(p.vertices) - 2 for p in o.data.polygons),
        'bytes': os.path.getsize(path),
    }
    with open(os.path.join(SHEETS, name + '.json'), 'w') as f:
        json.dump(info, f, indent=1)
    print('MODEL', json.dumps(info))


if __name__ == '__main__':
    want = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else []
    for name, slug in BOULDERS.items():
        if not want or name in want:
            boulder(name, slug)
    for name, (slug, fill, folder) in BLOCKS.items():
        if not want or name in want:
            boulder(name, slug, fill, os.path.join(MODELS, folder), rough=0.5, tris=BLOCK_TRIS, hard=True, ao=folder == 'ship')
    for name, (slug, fill) in ISLE_BOULDERS.items():
        if not want or name in want:
            boulder(name, slug, fill, tris=ISLE_BOULDER_TRIS)
    for name, (slug, tris, px) in STACKS.items():
        if not want or name in want:
            stack(name, slug, tris, px)

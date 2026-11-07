"""The shades: wraparound shield sunglasses, smoked lens, camo arms, an
orange nose pad. Headwear (`proto::HEADWEAR`), worn on Triple T's face. The operator's own model on Meshy (task 01a1af22-a5dd-76ba-a94c-
ed9aa0d8aba3, fetched by the API into sources/meshy/shades_raw.glb).

The lettering Meshy painted on the lens's corners is a sunglasses maker's
mark: it is painted out here (`scrub`), so what ships carries no brand.

Exported with the middle of the lens's front at the origin, +Y up, +Z
forward, in metres. `look/bodies.rs` hangs it on Triple T's Head bone;
the numbers for that (`HEADWEAR` there, which every piece of headwear
shares: `gear.py`) are printed here as FIT, from `EYES`. Look at them in the game (a render of an import of
triple-t.glb in Blender put the face 0.1 m off):

  ci/desktop_shot.sh traces/shades.png 2 0 0 0 \
    'pose=yaw=3.14&headwear=1&fighter=1&eye=0.2,1.45,8&look=1.5,1.35,8'"""
import json
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

NAME = 'shades'
# Across the lens, metres: Triple T's head is 0.35 wide at the eyes.
WIDTH = 0.40
TRIS = 2500
# Where the lens's front middle sits, in triple-t.glb's scene at rest
# (metres, +Y up, +Z the way it faces), set by the game's picture: over
# the eyes, under the brow, the nose through the notch.
EYES = (0.0, 1.57, 0.21)
# Tipped down at the front this much, degrees, to sit on the snout.
TIP = 4.0
# The lettering's patches on the colour map (u0, v0, u1, v1, v down from
# the top, as an image viewer shows it): every orange stroke in one goes.
LOGO = [(0.42, 0.33, 0.58, 0.48), (0.11, 0.50, 0.28, 0.645), (0.30, 0.515, 0.45, 0.655), (0.22, 0.915, 0.36, 1.0)]


def box_mean(m, r):
    """The mean of `m` over a (2r+1)² window, edges held."""
    p = np.pad(m, ((r + 1, r), (r + 1, r)), mode='edge')
    c = p.cumsum(0).cumsum(1)
    n = 2 * r + 1
    return (c[n:, n:] - c[:-n, n:] - c[n:, :-n] + c[:-n, :-n]) / (n * n)


def scrub(im):
    """Paints the lettering out of colour map `im`: an orange stroke in a
    LOGO patch, or one with the dark lens all round it, takes the lens's
    colour from round it."""
    w, h = im.size
    px = np.empty(w * h * 4, dtype=np.float32)
    im.pixels.foreach_get(px)
    a = px.reshape(h, w, 4)[::-1].copy()  # Blender's rows run bottom up
    rgb = a[..., :3]
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    hi, lo = rgb.max(-1), rgb.min(-1)
    orange = (r > 0.45) & (r > g * 1.35) & (g > b * 1.2) & ((hi - lo) > 0.25)
    dark = (hi < 0.28).astype(np.float32)
    k = max(1, w // 100)
    logo = orange & (box_mean(dark, k) > 0.55)
    for u0, v0, u1, v1 in LOGO:
        patch = np.zeros_like(logo)
        patch[int(v0 * h):int(v1 * h), int(u0 * w):int(u1 * w)] = True
        logo |= orange & patch
    logo = box_mean(logo.astype(np.float32), 2) > 0
    weight = box_mean(dark, 2 * k) + 1e-6
    for c in range(3):
        fill = box_mean(rgb[..., c] * dark, 2 * k) / weight
        rgb[..., c] = np.where(logo, fill, rgb[..., c])
    im.pixels.foreach_set(a[::-1].ravel())
    im.pack()
    print(f'SCRUB {logo.mean() * 100:.2f}% of the colour map painted out')


def raw():
    """Meshy's mesh joined, its front toward -Y, up +Z, WIDTH across, the
    lens's front middle at the origin, thinned to about TRIS."""
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

    # The front is the end of the long side with the bigger section: the
    # lens, not the arms' tips.
    lo, hi = bounds(o)
    co = np.array([v.co[:] for v in o.data.vertices])
    depth = hi.y - lo.y

    def section(a, b):
        part = co[(co[:, 1] >= lo.y + a * depth) & (co[:, 1] <= lo.y + b * depth)]
        return float(np.ptp(part[:, 0]) * np.ptp(part[:, 2])) if len(part) else 0.0

    if section(0.9, 1.0) > section(0.0, 0.1):
        o.data.transform(Matrix.Rotation(math.pi, 4, 'Z'))
    lo, hi = bounds(o)
    o.data.transform(Matrix.Scale(WIDTH / (hi.x - lo.x), 4))
    lo, hi = bounds(o)
    co = np.array([v.co[:] for v in o.data.vertices])
    lens = co[co[:, 1] <= lo.y + 0.15 * (hi.y - lo.y)]
    o.data.transform(Matrix.Translation(Vector(((lo.x + hi.x) / 2, lo.y, (lens[:, 2].min() + lens[:, 2].max()) / 2)) * -1))
    o.data.update()

    count = sum(len(p.vertices) - 2 for p in o.data.polygons)
    if count > TRIS:
        m = o.modifiers.new('thin', 'DECIMATE')
        m.ratio = TRIS / count
        bpy.ops.object.modifier_apply(modifier='thin')
    return o


def maps(o):
    """The colour map scrubbed and brought to TEX_PX, the rest to MAP_PX;
    no glow."""
    colour = set()
    for material in o.data.materials:
        bsdf = next(n for n in material.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
        node = bsdf.inputs['Base Color'].links[0].from_node
        while node.type != 'TEX_IMAGE':
            node = next(i.links[0].from_node for i in node.inputs if i.links)
        colour.add(node.image)
    for im in bpy.data.images:
        w, h = im.size
        px = TEX_PX if im in colour else MAP_PX
        if max(w, h) > px:
            s = px / max(w, h)
            im.scale(max(1, round(w * s)), max(1, round(h * s)))
        if im in colour:
            scrub(im)
    for material in o.data.materials:
        nt = material.node_tree
        bsdf = next(n for n in nt.nodes if n.type == 'BSDF_PRINCIPLED')
        for link in list(bsdf.inputs['Emission Color'].links):
            nt.links.remove(link)
        bsdf.inputs['Emission Strength'].default_value = 0.0
        for n in nt.nodes:
            if n.type == 'NORMAL_MAP':
                n.inputs['Strength'].default_value = NORMAL


def fit():
    """Where the shades hang on the Head bone: the lens's place (EYES,
    tipped TIP), as the client's numbers."""
    fit_line('HEADWEAR', 'Head', Matrix.Translation(EYES) @ Matrix.Rotation(math.radians(TIP), 4, 'X'))



scene()
o = raw()
maps(o)
os.makedirs(MODELS, exist_ok=True)
# Blender's -Y forward, +Z up is glTF's +Z forward, +Y up.
bpy.ops.object.select_all(action='DESELECT')
o.select_set(True)
bpy.context.view_layer.objects.active = o
out = os.path.join(MODELS, NAME + '.glb')
bpy.ops.export_scene.gltf(filepath=out, export_format='GLB', export_apply=True, export_yup=True, use_selection=True,
                          export_image_format='JPEG', export_jpeg_quality=88)
lo, hi = bounds(o)
print('MODEL', json.dumps({'file': NAME, 'tris': sum(len(p.vertices) - 2 for p in o.data.polygons),
                           'size_m': [round(v, 3) for v in (hi - lo)], 'bytes': os.path.getsize(out)}))
fit()

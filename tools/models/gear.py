"""The wardrobe's kit: what every piece of gear (`proto::HEADWEAR`,
`proto::FOOTWEAR`) is cut with, and the fitting room it is looked at in.

Every piece of headwear hangs from one place on the Head bone and every boot
from one on its foot's (`HEADWEAR`, `BOOT_LEFT`, `BOOT_RIGHT` in
`look/bodies.rs`; `blender -b --python tools/models/gear.py` prints them as
FIT). What differs between pieces is in their files: a script works in the
body's own frame at rest (metres: x to Triple T's left, up from the soles,
forward from the face; `paint.rs` measures the same way) and `head_export`
takes it into the Fit's.

The body is read from triple-t.glb's own skin, the way the game draws it: a
vertex is on a bone by that bone's inverse bind, so what is hung on the bone
sits against it wherever the clips take it. An import of the file into Blender
rebuilds the armature and puts the face 0.1 m off.

The rules a piece keeps (the room shows them, the headband painted on):
  - The headband is how a side is read (`paint.rs`, 1.653 to 1.747 up). A mask
    stops under it; a hat sits above it, on the log's flat top.
  - No lettering, no maker's mark, nothing of another game's.

    tools/models/build.sh aviators        builds it, its room in traces/gear/

Look at one in the game (its id is its place in the list):

    ci/desktop_shot.sh traces/gear.png 2 0 0 0 'pose=yaw=3.14&headwear=2&footwear=1&fighter=1&eye=0.2,1.45,8&look=1.5,1.35,8'"""
import json
import math
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *
from lib import _surface

# The Fits: where the shades' lens sat over the eyes, tipped onto the nose, in
# triple-t.glb's scene at rest; each sole's middle there, and how far each foot
# is turned out, degrees (the left foot is +X).
EYES = (0.0, 1.57, 0.21)
TIP = 4.0
FEET = (0.17, 0.0, 0.04)
TOE_OUT = 12.0
# What a piece's triangles and maps come to.
HEAD_TRIS = 8000
BOOT_TRIS = 8000
GEAR_TEX_PX = 1024
GEAR_MAP_PX = 512
# Its surface: a touch less gloss and metal than Meshy paints (lib's ROUGH, METAL).
GEAR_ROUGH = 0.45
GEAR_METAL = 0.6
# The headband, as `paint.rs` has it, and the trunk's round in plan: what a mask
# or a hat is fitted to, and what is hidden inside it. Its flat top is at TOP.
BAND_LO = 1.653
BAND_HI = 1.747
TOP = 1.80
TRUNK_Z = -0.047
TRUNK_R = 0.18
# Under this far inside the trunk's round a face can't be seen.
HIDDEN_R = 0.155
HIDDEN_LO = 1.24
HIDDEN_HI = 1.785
# Glasses' arms: how far out from the trunk's middle they lie, how far round it
# they go (degrees, from where it is widest toward the back), and how thick
# against the frame's wire.
ARM_R = 0.188
ARM_ROUND = (-30.0, 0.0, 22.0, 42.0)
ARM_WIRE = 0.85
# A foot and its toes tucked into a boot (`bodies::FOOT_TUCK`, `TOE_TUCK`).
FOOT_TUCK = 0.62
TOE_TUCK = 0.5
# What the room's lit by and how it's framed.
ROOM_PX = 500
ROOM_DIR = os.path.join(ROOT, 'traces', 'gear')
LIFT = 2.6
# A glTF file's frame (and the body's: x, up, forward) in Blender's (x, -forward, up).
C = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))


def _glb():
    with open(os.path.join(MODELS, 'triple-t.glb'), 'rb') as f:
        b = f.read()
    n = struct.unpack('<I', b[12:16])[0]
    return json.loads(b[20:20 + n]), b[20 + n + 8:]


def _accessor(j, data, i):
    a = j['accessors'][i]
    v = j['bufferViews'][a['bufferView']]
    kind, size = {5126: ('f', 4), 5123: ('H', 2), 5121: ('B', 1), 5125: ('I', 4)}[a['componentType']]
    many = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4, 'MAT4': 16}[a['type']]
    at = v.get('byteOffset', 0) + a.get('byteOffset', 0)
    stride = v.get('byteStride', size * many)
    if stride == size * many:
        return np.frombuffer(data, dtype=np.dtype(kind), count=many * a['count'], offset=at).reshape(a['count'], many)
    return np.array([np.frombuffer(data, dtype=np.dtype(kind), count=many, offset=at + k * stride) for k in range(a['count'])])


def _from_mesh(p):
    """A mesh-space point (the skin's, its head down -Z) in the body's frame."""
    return (p[..., 0], -p[..., 2], p[..., 1])


def place_of(bone, place):
    """What takes a file hung on `bone` at `place` (4×4, the Fit, as `lib.fit_line`
    reads it) into the body's frame: the Fit, the bone's bind, the mesh's frame."""
    j, data = _glb()
    names = [j['nodes'][i].get('name') for i in j['skins'][0]['joints']]
    ibm = _accessor(j, data, j['skins'][0]['inverseBindMatrices']).reshape(-1, 4, 4).transpose(0, 2, 1)
    bind = Matrix(np.linalg.inv(ibm[names.index(bone)]).tolist())
    # The skin's mesh, its head down -Z, turns into the body's frame as a file does
    # into Blender's.
    return C @ bind @ rest_bone(bone).inverted() @ place


def head_fit():
    return Matrix.Translation(EYES) @ Matrix.Rotation(math.radians(TIP), 4, 'X')


def foot_fit(s):
    x, y, z = FEET
    return Matrix.Translation((s * x, y, z)) @ Matrix.Rotation(math.radians(s * TOE_OUT), 4, 'Y')


HEAD_PLACE = place_of('Head', head_fit())
LEFT_PLACE = place_of('LeftFoot', foot_fit(1.0))
RIGHT_PLACE = place_of('RightFoot', foot_fit(-1.0))


def raw(name):
    """Meshy's mesh of `name`, whole, as one object with its transforms applied."""
    path = os.path.join(SOURCES, name + '_raw.glb')
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
    return o


def turned(o, yaw=0.0, pitch=0.0, roll=0.0):
    """`o` turned as it came, degrees: about up (its front to -Y, Blender's
    forward), about across (+: its top toward the back), about forward."""
    o.data.transform(Matrix.Rotation(math.radians(roll), 4, 'Y') @ Matrix.Rotation(math.radians(pitch), 4, 'X')
                     @ Matrix.Rotation(math.radians(yaw), 4, 'Z'))
    o.data.update()


def maps(o, tex_px=GEAR_TEX_PX, map_px=GEAR_MAP_PX, rough=GEAR_ROUGH, metal=GEAR_METAL, glow=False):
    """Its colour map to `tex_px`, the rest to `map_px`, the gloss squeezed, the
    normal map drawn at lib's NORMAL. Meshy's glow map is dropped unless `glow`."""
    colour = set()
    for material in o.data.materials:
        bsdf = next(n for n in material.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
        if bsdf.inputs['Base Color'].links:
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
        if not glow:
            for l in list(bsdf.inputs['Emission Color'].links):
                nt.links.remove(l)
            bsdf.inputs['Emission Strength'].default_value = 0.0
        _surface(material, rough, metal)
        for n in nt.nodes:
            if n.type == 'NORMAL_MAP':
                n.inputs['Strength'].default_value = NORMAL


def hue_turned(o, was, now, window=30.0, sat=1.0, val=1.0):
    """The colour map's paint within `window` degrees of hue `was` turned to hue
    `now` (degrees), and its saturation and value scaled: a side's colour off a
    big piece of paint, since a side is read by colour (`paint.rs`)."""
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
        rgb = rgba[..., :3]
        hi, lo = rgb.max(-1), rgb.min(-1)
        chroma = hi - lo
        s = np.where(hi > 1e-4, chroma / np.maximum(hi, 1e-4), 0.0)
        r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
        c = np.maximum(chroma, 1e-5)
        hue = np.where(hi == r, ((g - b) / c) % 6.0, np.where(hi == g, (b - r) / c + 2.0, (r - g) / c + 4.0)) * 60.0
        d = np.abs((hue - was + 180.0) % 360.0 - 180.0)
        # How much of it turns: all of what is near, none of a pale or grey texel.
        k = np.clip((window - d) / (window * 0.4), 0.0, 1.0) * np.clip((s - 0.2) / 0.2, 0.0, 1.0)
        v2 = hi * (1.0 + (val - 1.0) * k)
        s2 = np.clip(s * (1.0 + (sat - 1.0) * k), 0.0, 1.0)
        h2 = (hue + ((now - was + 180.0) % 360.0 - 180.0) * (k > 0)) / 60.0
        f = h2 - np.floor(h2)
        p, q, t = v2 * (1 - s2), v2 * (1 - s2 * f), v2 * (1 - s2 * (1 - f))
        i = np.floor(h2).astype(int) % 6
        out = np.select([(i == n)[..., None] for n in range(6)],
                        [np.stack(x, -1) for x in ((v2, t, p), (q, v2, p), (p, v2, t), (p, q, v2), (t, p, v2), (v2, p, q))])
        rgba[..., :3] = np.where((k > 0)[..., None], out, rgb)
        im.pixels.foreach_set(rgba.ravel())
        im.pack()


def blotted(o, lo, hi, tolerance=0.03, facing=None):
    """Paints out of the colour map whatever stands out on the faces inside the box
    `lo`..`hi` (Blender's frame, as `o` is now), of those turned toward `facing` if
    it says: lettering Meshy paints unasked. Every texel there further than
    `tolerance` from their median takes the median."""
    material = o.data.materials[0]
    bsdf = next(n for n in material.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
    node = bsdf.inputs['Base Color'].links[0].from_node
    while node.type != 'TEX_IMAGE':
        node = next(i.links[0].from_node for i in node.inputs if i.links)
    im = node.image
    w, h = im.size
    px = np.empty(w * h * 4, np.float32)
    im.pixels.foreach_get(px)
    rgba = px.reshape(h, w, 4)
    mask = np.zeros((h, w), bool)
    uv = o.data.uv_layers.active.data
    for p in o.data.polygons:
        mid = p.center
        if not all(lo[i] <= mid[i] <= hi[i] for i in range(3)):
            continue
        if facing and p.normal.dot(Vector(facing)) < 0.5:
            continue
        at = [uv[i].uv * Vector((w, h)) for i in p.loop_indices]
        for a, b, c in ((at[0], at[i], at[i + 1]) for i in range(1, len(at) - 1)):
            x0, x1 = int(min(a.x, b.x, c.x)), int(max(a.x, b.x, c.x)) + 1
            y0, y1 = int(min(a.y, b.y, c.y)), int(max(a.y, b.y, c.y)) + 1
            xs, ys = np.meshgrid(np.arange(x0, x1), np.arange(y0, y1))
            area = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)
            if abs(area) < 1e-6:
                continue
            u = ((xs - a.x) * (c.y - a.y) - (c.x - a.x) * (ys - a.y)) / area
            v = ((b.x - a.x) * (ys - a.y) - (xs - a.x) * (b.y - a.y)) / area
            inside = (u >= -0.05) & (v >= -0.05) & (u + v <= 1.05)
            mask[np.clip(ys[inside], 0, h - 1), np.clip(xs[inside], 0, w - 1)] = True
    mid = np.median(rgba[mask][:, :3], axis=0)
    off = np.abs(rgba[..., :3] - mid).max(-1) > tolerance
    rgba[mask & off, :3] = mid
    im.pixels.foreach_set(rgba.ravel())
    im.pack()
    print(f'BLOT {o.name}: {(mask & off).sum()} texels')


def drawn_in(o, keep, by):
    """What lies more than `keep` out to either side brought in to `by` of that:
    horns, say, whose spread the head needn't carry."""
    for v in o.data.vertices:
        d = abs(v.co.x) - keep
        if d > 0:
            v.co.x = math.copysign(keep + d * by, v.co.x)
    o.data.update()


def strapped(o, behind, out=0.192, bins=48):
    """A strap's loop, what of it is behind `behind` (the body's frame), brought
    out round the trunk at `out` from its middle, wherever it would sink in. Each
    way round the loop is scaled whole, so the strap keeps its thickness."""
    mids = []
    for v in o.data.vertices:
        z = -v.co.y
        if z < behind:
            mids.append((math.atan2(z - TRUNK_Z, v.co.x), math.hypot(v.co.x, z - TRUNK_Z)))
    far = [0.0] * bins
    for a, r in mids:
        i = int((a + math.pi) / (2 * math.pi) * bins) % bins
        far[i] = max(far[i], r)
    # A bin no vertex fell in borrows from its neighbours.
    for _ in range(bins):
        far = [f or max(far[(i - 1) % bins], far[(i + 1) % bins]) for i, f in enumerate(far)]
    for v in o.data.vertices:
        z = -v.co.y
        if z >= behind:
            continue
        a = math.atan2(z - TRUNK_Z, v.co.x)
        r = far[int((a + math.pi) / (2 * math.pi) * bins) % bins]
        if 0 < r < out:
            k = out / r
            v.co.x *= k
            v.co.y = -(TRUNK_Z + (z - TRUNK_Z) * k)
    o.data.update()


def thin(o, tris):
    """`o` down to about `tris` triangles, if it is over (Meshy's smart
    topology comes near what it was asked: this keeps its seams)."""
    count = sum(len(p.vertices) - 2 for p in o.data.polygons)
    if count <= tris:
        return
    bpy.ops.object.select_all(action='DESELECT')
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    m = o.modifiers.new('thin', 'DECIMATE')
    m.ratio = tris / count
    bpy.ops.object.modifier_apply(modifier='thin')


def hollow(o):
    """Takes off what the trunk hides (a body-framed piece): a mask's back, the
    inside of a hat. What the player can't see isn't drawn."""
    bm = bmesh.new()
    bm.from_mesh(o.data)

    def hidden(v):
        # Blender's frame: x, -forward, up.
        z = -v.co.y
        return HIDDEN_LO < v.co.z < HIDDEN_HI and math.hypot(v.co.x, z - TRUNK_Z) < HIDDEN_R

    gone = [f for f in bm.faces if all(hidden(v) for v in f.verts)]
    bmesh.ops.delete(bm, geom=gone, context='FACES')
    bm.to_mesh(o.data)
    bm.free()
    o.data.update()
    print(f'HOLLOW {o.name}: {len(gone)} faces inside the trunk')


def sized(o, width=None, length=None, height=None):
    """`o` scaled evenly to `width` across, `length` front to back or `height`."""
    lo, hi = bounds(o)
    k = (width / (hi.x - lo.x)) if width else (length / (hi.y - lo.y)) if length else (height / (hi.z - lo.z))
    o.data.transform(Matrix.Scale(k, 4))
    o.data.update()


def stood(o, bottom=None, top=None, front=None, back=None, middle=None, x=0.0):
    """`o` moved to where it goes in the body's frame: its lowest at `bottom` or
    its highest at `top` (up), its foremost at `front`, its rearmost at `back`
    or its middle at `middle` (forward), its middle across at `x`."""
    lo, hi = bounds(o)
    up = 0.0 if bottom is None and top is None else (bottom - lo.z if bottom is not None else top - hi.z)
    # Blender's -Y is forward: its foremost is its lowest y.
    fwd = (front + lo.y) if front is not None else (back + hi.y) if back is not None else (middle + (lo.y + hi.y) / 2) if middle is not None else 0.0
    o.data.transform(Matrix.Translation((x - (lo.x + hi.x) / 2, -fwd, up)))
    o.data.update()


def _export(o, name, place, image_format='JPEG'):
    """`o`, in the body's frame, written as `name`.glb in the frame of the Fit
    at `place`, and put back."""
    to_file = (C @ place @ C.inverted()).inverted()
    o.data.transform(to_file)
    o.data.update()
    bpy.ops.object.select_all(action='DESELECT')
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    out = os.path.join(MODELS, name + '.glb')
    # Blender's -Y forward, +Z up is glTF's +Z forward, +Y up.
    bpy.ops.export_scene.gltf(filepath=out, export_format='GLB', export_apply=True, export_yup=True, use_selection=True,
                              export_image_format=image_format, export_jpeg_quality=88)
    lo, hi = bounds(o)
    print('MODEL', json.dumps({'file': name, 'tris': sum(len(p.vertices) - 2 for p in o.data.polygons),
                               'size_m': [round(v, 3) for v in (hi - lo)], 'bytes': os.path.getsize(out)}))
    o.data.transform(to_file.inverted())
    o.data.update()


def head_export(o, name, image_format='JPEG'):
    """A piece of headwear, in the body's frame: its file, and the room."""
    _export(o, name, HEAD_PLACE, image_format)
    room(name, [(name, HEAD_PLACE)], HEAD_VIEWS)


def boots_export(o, name):
    """One right boot, its sole's middle at the origin, its toe to -Y, up +Z: the
    right and its mirror image, the left (`mirror`), and the room."""
    identity = Matrix.Identity(4)
    _export(o, name + '-right', identity)
    o.data.transform(Matrix.Diagonal((-1.0, 1.0, 1.0, 1.0)))
    bm = bmesh.new()
    bm.from_mesh(o.data)
    bmesh.ops.reverse_faces(bm, faces=bm.faces[:])
    bm.to_mesh(o.data)
    bm.free()
    o.data.update()
    _export(o, name + '-left', identity)
    room(name, [(name + '-left', LEFT_PLACE), (name + '-right', RIGHT_PLACE)], FEET_VIEWS, tuck=True)


def head_piece(name, width, yaw=0.0, pitch=0.0, roll=0.0, deep=1.0, tall=1.0, tris=HEAD_TRIS, inside=True, glow=False,
               paint=None, shape=None, wrap=None, **at):
    """Meshy's `name` as headwear: turned to face forward, `width` across, `deep`
    and `tall` times that front to back and up (a picture's depth is a guess),
    `shape` done to it, stood where `at` says (`stood`), `wrap` then done to it
    there, the part the trunk hides off, its maps cut, `paint` then done to the
    colour map."""
    scene()
    o = raw(name)
    turned(o, yaw, pitch, roll)
    sized(o, width=width)
    o.data.transform(Matrix.Diagonal((1.0, deep, tall, 1.0)))
    if shape:
        shape(o)
    stood(o, **at)
    if wrap:
        wrap(o)
    if inside:
        hollow(o)
    thin(o, tris)
    maps(o, glow=glow)
    if paint:
        paint(o)
    head_export(o, name)


def boots_piece(name, length, yaw=0.0, pitch=0.0, roll=0.0, wide=None, high=None, tris=BOOT_TRIS, glow=False, coat=None,
                paint=None):
    """Meshy's `name` as a pair: turned so its toe is forward, `length` heel to
    toe, `wide` across if a picture's guess at it needs help (one picture's
    depth is a guess, and it has a root foot to cover), `high` from the sole if
    it would climb past the knee. `coat`: another paint of it (meshy_gen.py
    --paint), `<name>~<coat>_raw.glb`; `paint` is then done to the colour map."""
    scene()
    o = raw(f'{name}~{coat}' if coat else name)
    turned(o, yaw, pitch, roll)
    sized(o, length=length)
    lo, hi = bounds(o)
    o.data.transform(Matrix.Diagonal((wide / (hi.x - lo.x) if wide else 1.0, 1.0, high / (hi.z - lo.z) if high else 1.0, 1.0)))
    lo, hi = bounds(o)
    o.data.transform(Matrix.Translation(Vector(((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, lo.z)) * -1))
    o.data.update()
    thin(o, tris)
    maps(o, glow=glow)
    if paint:
        paint(o)
    boots_export(o, name)


# The room's views: (from, at, lens) in Blender's frame, the body's -Y its front.
HEAD_VIEWS = [((0.85, -1.7, 1.78), (0, -0.02, 1.62), 60), ((1.95, -0.02, 1.62), (0, -0.02, 1.62), 60),
              ((-1.05, 1.5, 2.05), (0, -0.02, 1.62), 60), ((1.5, -3.6, 1.5), (0, 0, 1.0), 40)]
FEET_VIEWS = [((0.85, -1.15, 0.5), (0, -0.05, 0.13), 50), ((1.5, -0.05, 0.25), (0, -0.05, 0.13), 50),
              ((-0.7, 1.2, 0.5), (0, -0.05, 0.13), 50), ((1.5, -3.6, 1.5), (0, 0, 0.95), 40)]


def _tucked(j, data, p, pos):
    """A mesh's points with its feet tucked into boots, as `bodies::TUCKED` draws them."""
    names = [j['nodes'][i].get('name') for i in j['skins'][0]['joints']]
    ibm = _accessor(j, data, j['skins'][0]['inverseBindMatrices']).reshape(-1, 4, 4).transpose(0, 2, 1)
    joints = _accessor(j, data, p['attributes']['JOINTS_0'])
    weights = _accessor(j, data, p['attributes']['WEIGHTS_0'])

    def about(bone, k):
        i = names.index(bone)
        return np.linalg.inv(ibm[i]) @ np.diag((k, k, k, 1.0)) @ ibm[i]

    moves = {}
    for foot, toe in (('LeftFoot', 'LeftToeBase'), ('RightFoot', 'RightToeBase')):
        moves[names.index(foot)] = about(foot, FOOT_TUCK)
        moves[names.index(toe)] = about(foot, FOOT_TUCK) @ about(toe, TOE_TUCK)
    out = pos.astype(np.float64).copy()
    hom = np.c_[pos, np.ones(len(pos))]
    for i, m in moves.items():
        w = (weights * (joints == i)).sum(1)
        out += w[:, None] * ((hom @ m.T)[:, :3] - pos)
    return out


def _body(folder, tuck=False):
    """Triple T at rest, whole, its headband painted on by height, its feet tucked
    in if it wears boots."""
    j, data = _glb()
    objs = []
    prim = j['meshes'][0]['primitives'][0]
    bv = j['bufferViews'][j['images'][j['textures'][j['materials'][prim['material']]['pbrMetallicRoughness']['baseColorTexture']['index']]['source']]['bufferView']]
    albedo = os.path.join(folder, 'triple-t-albedo.jpg')
    with open(albedo, 'wb') as f:
        f.write(data[bv.get('byteOffset', 0):bv.get('byteOffset', 0) + bv['byteLength']])
    wood = bpy.data.materials.new('wood')
    wood.use_nodes = True
    nt = wood.node_tree
    bsdf = nt.nodes['Principled BSDF']
    bsdf.inputs['Roughness'].default_value = 0.85
    tex = nt.nodes.new('ShaderNodeTexImage')
    tex.image = bpy.data.images.load(albedo)
    lift = nt.nodes.new('ShaderNodeMixRGB')
    lift.blend_type = 'MULTIPLY'
    lift.inputs[0].default_value = 1.0
    lift.inputs[2].default_value = (LIFT, LIFT, LIFT, 1)
    nt.links.new(tex.outputs[0], lift.inputs[1])
    coords = nt.nodes.new('ShaderNodeTexCoord')
    axes = nt.nodes.new('ShaderNodeSeparateXYZ')
    nt.links.new(coords.outputs['Object'], axes.inputs[0])
    over = nt.nodes.new('ShaderNodeMath')
    over.operation = 'GREATER_THAN'
    over.inputs[1].default_value = BAND_LO
    under = nt.nodes.new('ShaderNodeMath')
    under.operation = 'LESS_THAN'
    under.inputs[1].default_value = BAND_HI
    both = nt.nodes.new('ShaderNodeMath')
    both.operation = 'MULTIPLY'
    nt.links.new(axes.outputs['Z'], over.inputs[0])
    nt.links.new(axes.outputs['Z'], under.inputs[0])
    nt.links.new(over.outputs[0], both.inputs[0])
    nt.links.new(under.outputs[0], both.inputs[1])
    band = nt.nodes.new('ShaderNodeMixRGB')
    nt.links.new(both.outputs[0], band.inputs[0])
    nt.links.new(lift.outputs[0], band.inputs[1])
    band.inputs[2].default_value = (0.75, 0.06, 0.05, 1)
    nt.links.new(band.outputs[0], bsdf.inputs['Base Color'])
    for mesh in j['meshes']:
        p = mesh['primitives'][0]
        pos = _accessor(j, data, p['attributes']['POSITION'])
        if tuck:
            pos = _tucked(j, data, p, pos)
        uv = _accessor(j, data, p['attributes']['TEXCOORD_0'])
        tris = _accessor(j, data, p['indices']).ravel().reshape(-1, 3)
        x, up, fwd = _from_mesh(pos)
        me = bpy.data.meshes.new(mesh['name'])
        me.from_pydata(np.c_[x, -fwd, up].tolist(), [], tris.tolist())
        me.update()
        layer = me.uv_layers.new(name='UVMap')
        layer.data.foreach_set('uv', np.c_[uv[tris.ravel(), 0], 1 - uv[tris.ravel(), 1]].ravel())
        me.polygons.foreach_set('use_smooth', [True] * len(me.polygons))
        me.update()
        me.materials.append(wood)
        o = bpy.data.objects.new(mesh['name'], me)
        bpy.context.scene.collection.objects.link(o)
        objs.append(o)
    return objs


def room(name, worn, views, tuck=False):
    """Triple T dressed in `worn` (each file and its place, re-read from what was
    exported, as the game would), four views on a sheet in traces/gear."""
    scene()
    s = bpy.context.scene
    os.makedirs(ROOM_DIR, exist_ok=True)
    _body(ROOM_DIR, tuck)
    for file, place in worn:
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=os.path.join(MODELS, file + '.glb'))
        for o in [x for x in bpy.data.objects if x not in before and x.parent is None]:
            o.matrix_world = C @ place @ C.inverted() @ o.matrix_world
    s.render.engine = 'BLENDER_EEVEE'
    s.eevee.taa_render_samples = 32
    s.view_settings.view_transform = 'Standard'
    s.render.resolution_x = ROOM_PX
    s.render.resolution_y = ROOM_PX
    w = bpy.data.worlds.new('w')
    w.use_nodes = True
    nt = w.node_tree
    coords = nt.nodes.new('ShaderNodeTexCoord')
    ramp = nt.nodes.new('ShaderNodeValToRGB')
    axes = nt.nodes.new('ShaderNodeSeparateXYZ')
    nt.links.new(coords.outputs['Generated'], axes.inputs[0])
    ramp.color_ramp.elements[0].position = 0.45
    ramp.color_ramp.elements[0].color = (0.24, 0.22, 0.2, 1)
    ramp.color_ramp.elements[1].position = 0.75
    ramp.color_ramp.elements[1].color = (0.62, 0.72, 0.9, 1)
    remap = nt.nodes.new('ShaderNodeMapRange')
    remap.inputs['From Min'].default_value = -1.0
    remap.inputs['From Max'].default_value = 1.0
    nt.links.new(axes.outputs['Z'], remap.inputs['Value'])
    nt.links.new(remap.outputs['Result'], ramp.inputs['Fac'])
    nt.links.new(ramp.outputs[0], nt.nodes['Background'].inputs['Color'])
    s.world = w
    sun = bpy.data.objects.new('sun', bpy.data.lights.new('sun', 'SUN'))
    s.collection.objects.link(sun)
    sun.data.energy = 3.2
    sun.rotation_euler = (math.radians(50), math.radians(-18), math.radians(-30))
    cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam'))
    s.collection.objects.link(cam)
    s.camera = cam
    shots = []
    for i, (eye, look, lens) in enumerate(views):
        cam.data.lens = lens
        cam.location = eye
        cam.rotation_euler = (Vector(look) - Vector(eye)).to_track_quat('-Z', 'Y').to_euler()
        path = os.path.join(ROOM_DIR, f'.{name}-{i}.png')
        s.render.filepath = path
        bpy.ops.render.render(write_still=True)
        shots.append(path)
    sheet = bpy.data.images.new(name, ROOM_PX * len(shots), ROOM_PX, alpha=False)
    out = np.ones((ROOM_PX, ROOM_PX * len(shots), 4), np.float32)
    for i, path in enumerate(shots):
        im = bpy.data.images.load(path)
        px = np.empty(ROOM_PX * ROOM_PX * 4, np.float32)
        im.pixels.foreach_get(px)
        out[:, i * ROOM_PX:(i + 1) * ROOM_PX] = px.reshape(ROOM_PX, ROOM_PX, 4)
        bpy.data.images.remove(im)
        os.remove(path)
    sheet.pixels.foreach_set(out.ravel())
    sheet.filepath_raw = os.path.join(ROOM_DIR, name + '.png')
    sheet.file_format = 'PNG'
    sheet.save()
    print('SHEET', sheet.filepath_raw)


if __name__ == '__main__':
    fit_line('HEADWEAR', 'Head', head_fit())
    fit_line('BOOT_LEFT', 'LeftFoot', foot_fit(1.0))
    fit_line('BOOT_RIGHT', 'RightFoot', foot_fit(-1.0))
    print('PLACE', [round(v, 3) for v in HEAD_PLACE.to_translation()], [round(math.degrees(a), 2) for a in HEAD_PLACE.to_euler()])


def _b(x, up, fwd):
    """A point in the body's frame, in Blender's."""
    return Vector((x, -fwd, up))


def pipe(bm, points, r, closed=False, sides=8):
    """A round wire along `points` into bmesh `bm`, its rings kept square to the
    line by carrying one normal along it."""
    n = len(points)
    prev = None
    rings = []
    for i, p in enumerate(points):
        a = points[(i - 1) % n] if closed or i > 0 else p
        b = points[(i + 1) % n] if closed or i < n - 1 else p
        t = (b - a).normalized()
        if prev is None:
            prev = t.orthogonal().normalized()
        side = (prev - t * prev.dot(t)).normalized()
        prev = side
        up = t.cross(side)
        ring = []
        for k in range(sides):
            a = 2 * math.pi * k / sides
            ring.append(bm.verts.new(p + (side * math.cos(a) + up * math.sin(a)) * r))
        rings.append(ring)
    for i in range(n if closed else n - 1):
        for k in range(sides):
            a, b = rings[i], rings[(i + 1) % n]
            bm.faces.new((a[k], a[(k + 1) % sides], b[(k + 1) % sides], b[k]))
    if not closed:
        bm.faces.new(rings[0][::-1])
        bm.faces.new(rings[-1])


def _colored(me, colours):
    """A mesh's vertex colours (linear RGBA, a vertex each)."""
    a = me.color_attributes.new('Col', 'FLOAT_COLOR', 'POINT')
    a.data.foreach_set('color', np.array(colours, np.float32).ravel())


def _object(bm, name, material, smooth=True):
    me = bpy.data.meshes.new(name)
    bm.normal_update()
    bm.to_mesh(me)
    bm.free()
    if smooth:
        me.polygons.foreach_set('use_smooth', [True] * len(me.polygons))
    me.update()
    me.materials.append(material)
    o = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(o)
    return o


def surface(name, colour, metal, rough, glow=None):
    """A plain material: colour (hex), metal, roughness, and glow (hex, strength)."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    b = m.node_tree.nodes['Principled BSDF']
    b.inputs['Base Color'].default_value = (*srgb(colour), 1.0)
    b.inputs['Metallic'].default_value = metal
    b.inputs['Roughness'].default_value = rough
    if glow:
        b.inputs['Emission Color'].default_value = (*srgb(glow[0]), 1.0)
        b.inputs['Emission Strength'].default_value = glow[1]
    return m


def lens(outline, centre, yaw, bulge, top, bottom, material, rings=6):
    """One lens: a shallow dome over `outline` ((across, up) points round it),
    standing at `centre` (the body's frame) turned out `yaw` degrees, shaded by
    height from `top` to `bottom` (linear RGB)."""
    bm = bmesh.new()
    turn = Matrix.Rotation(math.radians(yaw), 3, 'Z')
    mid = _b(*centre)

    def at(u, v, d):
        return mid + turn @ Vector((u, -d, v))

    tip = bm.verts.new(at(0.0, 0.0, bulge))
    last = None
    for r in range(1, rings + 1):
        k = r / rings
        ring = [bm.verts.new(at(u * k, v * k, bulge * (1.0 - k * k))) for u, v in outline]
        for i in range(len(ring)):
            j = (i + 1) % len(ring)
            bm.faces.new((tip, ring[i], ring[j]) if last is None else (last[i], ring[i], ring[j], last[j]))
        last = ring
    # Each face out the front.
    bm.normal_update()
    out = turn @ Vector((0, -1, 0))
    for f in bm.faces:
        if f.normal.dot(out) < 0:
            f.normal_flip()
    lo = min(v for _, v in outline)
    hi = max(v for _, v in outline)
    bm.verts.ensure_lookup_table()
    cols = []
    for v in bm.verts:
        s = max(0.0, min(1.0, (v.co.z - centre[1] - lo) / (hi - lo)))
        cols.append([*(bottom[i] + (top[i] - bottom[i]) * s for i in range(3)), 1.0])
    o = _object(bm, 'lens', material)
    _colored(o.data, cols)
    return o


def round_head(a, up):
    """Where a wire lying on the trunk's side goes `a` degrees round it from
    straight out (toward the back), `ARM_R` from its middle."""
    r = math.radians(a)
    return _b(ARM_R * math.cos(r), up, TRUNK_Z - ARM_R * math.sin(r))


def wire_glasses(outline, x, up, fwd, yaw, bulge, wire, frame, glass, top, bottom, bridge):
    """A pair of wire-framed glasses, each lens's middle `x` out from the nose at
    height `up`, `fwd` forward, turned out `yaw` degrees, its wire `wire` thick,
    its arm going back round the trunk's side: every object, to `join`. `bridge`:
    each bar across the nose, as (share of the lens's height, how far it arches)."""
    objs = []
    wires = bmesh.new()
    for s in (1.0, -1.0):
        turn = Matrix.Rotation(math.radians(s * yaw), 3, 'Z')
        mid = _b(s * x, up, fwd)
        points = [(s * u, v) for u, v in outline]
        objs.append(lens(points, (s * x, up, fwd), s * yaw, bulge, top, bottom, glass))
        pipe(wires, [mid + turn @ Vector((u, 0, v)) for u, v in points], wire, closed=True)
        # The arm, from the lens's outer corner to behind where an ear would be.
        hinge = max(points, key=lambda p: s * p[0] + 0.2 * p[1])
        h = mid + turn @ Vector((hinge[0], 0, hinge[1]))
        tail = [round_head(a, up + 0.012 - 0.0012 * max(0.0, a)) for a in ARM_ROUND]
        mirrored = [Vector((s * p.x, p.y, p.z)) for p in tail]
        pipe(wires, [h, h + Vector((s * 0.008, 0.012, 0.004))] + mirrored, wire * ARM_WIRE, sides=6)
    # The bars across the nose, from one lens's inner edge to the other's.
    for share, arch in bridge:
        ends = []
        for s in (1.0, -1.0):
            turn = Matrix.Rotation(math.radians(s * yaw), 3, 'Z')
            points = [(s * u, v) for u, v in outline]
            v = min(v for _, v in outline) + share * (max(v for _, v in outline) - min(v for _, v in outline))
            u = min(points, key=lambda p: abs(p[1] - v) + 2.0 * abs(s * p[0] + abs(s * x)) )
            ends.append(_b(s * x, up, fwd) + turn @ Vector((u[0], 0, u[1])))
        a, b = ends
        rise = Vector((0, -0.006, arch))
        pipe(wires, [a + (b - a) * (i / 8) + rise * (1.0 - ((i / 4) - 1.0) ** 2) for i in range(9)], wire * 0.9, sides=6)
    objs.append(_object(wires, 'wires', frame))
    return objs

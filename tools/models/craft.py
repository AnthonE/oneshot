"""The one-seaters' kit (look/craft.rs): the skimmer and the flier, each a
script in this folder that holds its prompt and its cut, as a gun's does
(lib.py). `meshy_gen.py <name>` commissions the raw mesh from the script's
IMAGE, VIEW and TEXTURE; `hull` brings it into the sim's hull frame, sized
to the sim's hull, its paint cut down and what it painted cyan set glowing;
`light` and `paint` then light and repaint it by rule, where each texel lies
on the craft (`texels`); `deliver` exports `crates/client/assets/models/<name>.glb`, writes the
numbers to `traces/craft/<name>.json` and renders the sheet it is judged by,
`traces/craft/<name>.png`: six views, the sim's hull drawn over three.

    tools/craft.sh skimmer                       the loop: this, then the game's views
    blender -b --python tools/models/skimmer.py  this alone (tools/models/build.sh skimmer)

**Hull space** (Blender, Z up): +X forward, +Y the craft's left, +Z up, the
origin on the ground under the hull's middle, metres: the frame of
`Vehicle::hull`, which the export turns Y up (+Z its right), as truck.py's.

The sim's numbers are read from its own code (`sim`), never kept here: a
craft is cut to the hull the sim collides, and the sheet draws that hull
(yellow: what stops a shot; orange: where the rider is and where the bolts
leave) so what is drawn can be held to it.
"""
import re

from lib import *
from lib import _glow, _surface

OUT = os.path.join(ROOT, 'traces', 'craft')
# A sheet's cell, pixels; six of them, three across.
CELL = (960, 600)
# The sheet's wires: what stops a shot, and the rider and the muzzles.
STEEL = (1.0, 0.85, 0.1)
RIDER = (1.0, 0.35, 0.05)


def _const(path, name):
    text = open(os.path.join(ROOT, path)).read()
    m = re.search(rf'const {name}: f32 = ([-0-9.]+);', text)
    if not m:
        print(f'Error: no {name} in {path}')
        sys.exit(1)
    return float(m.group(1))


def sim(name):
    """A number of the sim's (crates/sim/src/consts.rs)."""
    return _const('crates/sim/src/consts.rs', name)


def cam(name):
    """A number of the chase camera's (crates/client/src/lib.rs)."""
    return _const('crates/client/src/lib.rs', name)


def hull(name, length, turn=0, width=None, height=None, belly=0.0, mirror=None, glow=('#4ff2ff',), glow_strength=3.0,
         glow_floor=0.55, pale=False, tris=None, rough=ROUGH, metal=METAL, tex_px=TEX_PX, map_px=MAP_PX):
    """Meshy's raw mesh of `name` (sources/meshy/<name>_raw.glb) in hull
    space, `length` metres nose to tail, scaled evenly.

    Up is as it came. `turn` (degrees about up) brings its nose to +X: read
    it off the first sheet. `width` and `height` then squeeze or stretch it
    across and up to that many metres (a picture's depth is a guess; a few
    per cent, or it is another craft). Its footprint is centred on the
    origin and its lowest point stands `belly` over the ground.

    `mirror` ('left' or 'right') keeps that half and mirrors it over the
    other: a mesh made from one three-quarter picture has a side it never
    saw, and a craft is watched from behind for as long as it is flown.

    The paint is cut down, tempered and lit as a generated gun's is
    (`lib.generated`): `glow` names the colours of light it carries, `pale`
    counts a light painted nearly white."""
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

    o.data.transform(Matrix.Rotation(math.radians(turn), 4, 'Z'))
    lo, hi = bounds(o)
    o.data.transform(Matrix.Scale(length / (hi.x - lo.x), 4))
    lo, hi = bounds(o)
    if width:
        o.data.transform(Matrix.Scale(width / (hi.y - lo.y), 4, Vector((0, 1, 0))))
    if height:
        o.data.transform(Matrix.Scale(height / (hi.z - lo.z), 4, Vector((0, 0, 1))))
    lo, hi = bounds(o)
    o.data.transform(Matrix.Translation(Vector((-(lo.x + hi.x) / 2, -(lo.y + hi.y) / 2, belly - lo.z))))
    o.data.update()

    if mirror:
        bm = bmesh.new()
        bm.from_mesh(o.data)
        bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], plane_co=(0, 0, 0),
                               plane_no=(0, 1 if mirror == 'left' else -1, 0), clear_inner=True)
        bm.to_mesh(o.data)
        bm.free()
        m = o.modifiers.new('mirror', 'MIRROR')
        m.use_axis = (False, True, False)
        m.use_clip = True
        m.merge_threshold = 0.002
    count = sum(len(p.vertices) - 2 for p in o.data.polygons) * (2 if mirror else 1)
    if tris and count > tris:
        # Meshy's mesh is cut along every seam of its map: welded, or it tears.
        w = o.modifiers.new('weld', 'WELD')
        w.merge_threshold = 0.0005
        d = o.modifiers.new('thin', 'DECIMATE')
        d.ratio = tris / count
    if o.modifiers:
        me = bpy.data.meshes.new_from_object(o.evaluated_get(bpy.context.evaluated_depsgraph_get()))
        o.modifiers.clear()
        o.data = me

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
    return o


def slide(o, back, at, ahead, by):
    """Moves what is at x = `at` (hull space) `by` metres forward, easing
    to nothing at `back` behind it and at `ahead` before it: a saddle
    brought to where the sim seats its rider, the stern and the prows left
    where the sim's hull has them. What lies between is drawn out or closed
    up evenly, so keep to a plain run of hull there."""
    for v in o.data.vertices:
        x = v.co.x
        if back < x <= at:
            v.co.x += by * (x - back) / (at - back)
        elif at < x < ahead:
            v.co.x += by * (ahead - x) / (ahead - at)
    o.data.update()


def pieces(o, lo, hi, least=20):
    """Which of its vertices belong to loose pieces lying wholly inside the
    box `lo`..`hi` (hull space): a part Meshy modelled apart from the hull
    (a handlebar, a lever), found by what joins what once the seams its map
    cut are closed. A mask over its vertices."""
    me = o.data
    n = len(me.vertices)
    co = np.empty(n * 3, dtype=np.float64)
    me.vertices.foreach_get('co', co)
    co = co.reshape(n, 3)
    _, at = np.unique(np.round(co / 0.0005).astype(np.int64), axis=0, return_inverse=True)
    at = at.ravel()
    edges = np.empty(len(me.edges) * 2, dtype=np.int32)
    me.edges.foreach_get('vertices', edges)
    root = list(range(int(at.max()) + 1))

    def find(i):
        while root[i] != i:
            root[i] = root[root[i]]
            i = root[i]
        return i

    for a, b in at[edges].reshape(-1, 2):
        ra, rb = find(int(a)), find(int(b))
        if ra != rb:
            root[ra] = rb
    piece = np.array([find(int(i)) for i in at])
    inside = np.all((co >= np.array(lo)) & (co <= np.array(hi)), axis=1)
    mask = np.zeros(n, dtype=bool)
    for k in np.unique(piece[inside]):
        own = piece == k
        if inside[own].all() and own.sum() >= least:
            mask |= own
    return mask


def shift(o, mask, by):
    """Moves the vertices `mask` picks by `by` (hull space), as one piece."""
    co = np.empty(len(o.data.vertices) * 3, dtype=np.float32)
    o.data.vertices.foreach_get('co', co)
    co = co.reshape(-1, 3)
    co[mask] += np.array(by, dtype=np.float32)
    o.data.vertices.foreach_set('co', co.ravel())
    o.data.update()


def rod(a, b, r, material='iron'):
    """A round bar from `a` to `b` (hull space), `r` thick, of the kit's own
    paint (lib.py's PALETTE): a riser, a crossbar, what a mesh needs added
    to carry a piece that was moved."""
    a, b = Vector(a), Vector(b)
    o = cyl(r, (b - a).length, axis='z', material=material, verts=16, bevel=0)
    o.matrix_world = Matrix.Translation((a + b) / 2) @ (b - a).to_track_quat('Z', 'Y').to_matrix().to_4x4()
    return o


def texels(o, px=MAP_PX):
    """Where each texel of its map lies on the craft: `(pos, nor, on)`, the
    hull-space point and the face's normal under each of `px` × `px` texels
    (rows from the map's bottom, as Blender keeps an image) and whether any
    face is there at all. What `light` and `paint` pick their texels by: a
    rule written in metres (`pos[..., 2] > 0.6`), not a brush."""
    me = o.data
    me.calc_loop_triangles()
    n = len(me.loop_triangles)
    loops = np.empty(n * 3, dtype=np.int32)
    verts = np.empty(n * 3, dtype=np.int32)
    normal = np.empty(n * 3, dtype=np.float32)
    me.loop_triangles.foreach_get('loops', loops)
    me.loop_triangles.foreach_get('vertices', verts)
    me.loop_triangles.foreach_get('normal', normal)
    uv = np.empty(len(me.uv_layers.active.data) * 2, dtype=np.float32)
    me.uv_layers.active.data.foreach_get('uv', uv)
    co = np.empty(len(me.vertices) * 3, dtype=np.float32)
    me.vertices.foreach_get('co', co)
    uv = uv.reshape(-1, 2)[loops].reshape(n, 3, 2) % 1.0 * px
    co = co.reshape(-1, 3)[verts].reshape(n, 3, 3)
    normal = normal.reshape(n, 3)
    pos = np.zeros((px, px, 3), dtype=np.float32)
    nor = np.zeros((px, px, 3), dtype=np.float32)
    on = np.zeros((px, px), dtype=bool)
    for a, p, face in zip(uv, co, normal):
        x0, y0 = np.floor(a.min(axis=0)).astype(int)
        x1, y1 = np.ceil(a.max(axis=0)).astype(int)
        x0, y0, x1, y1 = max(x0 - 1, 0), max(y0 - 1, 0), min(x1 + 1, px), min(y1 + 1, px)
        if x1 <= x0 or y1 <= y0:
            continue
        gx, gy = np.meshgrid(np.arange(x0, x1) + 0.5, np.arange(y0, y1) + 0.5)
        d = (a[1, 1] - a[2, 1]) * (a[0, 0] - a[2, 0]) + (a[2, 0] - a[1, 0]) * (a[0, 1] - a[2, 1])
        if abs(d) < 1e-9:
            continue
        w0 = ((a[1, 1] - a[2, 1]) * (gx - a[2, 0]) + (a[2, 0] - a[1, 0]) * (gy - a[2, 1])) / d
        w1 = ((a[2, 1] - a[0, 1]) * (gx - a[2, 0]) + (a[0, 0] - a[2, 0]) * (gy - a[2, 1])) / d
        w2 = 1.0 - w0 - w1
        # A texel whose middle is just off the face is still drawn from it
        # at a seam: half a texel of margin, a face already there kept.
        m = 0.5 / max(abs(d) ** 0.5, 1e-6)
        inside = (w0 >= -m) & (w1 >= -m) & (w2 >= -m)
        strict = (w0 >= 0) & (w1 >= 0) & (w2 >= 0)
        take = inside & (strict | ~on[y0:y1, x0:x1])
        if not take.any():
            continue
        at = w0[..., None] * p[0] + w1[..., None] * p[1] + w2[..., None] * p[2]
        pos[y0:y1, x0:x1][take] = at[take]
        nor[y0:y1, x0:x1][take] = face
        on[y0:y1, x0:x1] |= take
    return pos, nor, on


def _image(o, socket):
    """The image behind its paint's `socket` (the first material's)."""
    nt = o.data.materials[0].node_tree
    bsdf = next(x for x in nt.nodes if x.type == 'BSDF_PRINCIPLED')
    node = bsdf.inputs[socket].links[0].from_node
    while node.type != 'TEX_IMAGE':
        node = next(i.links[0].from_node for i in node.inputs if i.links)
    return node.image


def _fit(mask, im):
    """`mask` (texels' shape) at `im`'s size."""
    w, h = im.size
    k = h // mask.shape[0]
    return np.kron(mask, np.ones((k, k), dtype=np.float32)) if k > 1 else mask


def light(o, mask, colour='#4ff2ff', k=1.0):
    """Lights the texels `mask` picks (0..1, `texels`' shape) in `colour`,
    over what `hull` lit: a strip where a camera looks, a nozzle's throat."""
    im = _image(o, 'Emission Color')
    w, h = im.size
    px = np.empty(w * h * 4, dtype=np.float32)
    im.pixels.foreach_get(px)
    px = px.reshape(h, w, 4)
    m = np.clip(_fit(mask.astype(np.float32), im) * k, 0.0, 1.0)
    px[..., :3] = np.maximum(px[..., :3], np.array(srgb(colour), dtype=np.float32) * m[..., None])
    im.pixels.foreach_set(px.ravel())
    im.pack()
    print(f'LIGHT {o.name}: {float(m.mean()) * 100:.2f}% of the map')


def paint(o, mask, colour, mix=1.0, rough=None, metal=None):
    """Turns the paint under `mask` toward `colour` by `mix` (its detail
    kept by what is left), and its roughness and metal to `rough` and
    `metal` where given: bare steel made dark iron, a belly the top's hue."""
    im = _image(o, 'Base Color')
    w, h = im.size
    px = np.empty(w * h * 4, dtype=np.float32)
    im.pixels.foreach_get(px)
    px = px.reshape(h, w, 4)
    m = np.clip(_fit(mask.astype(np.float32), im), 0.0, 1.0)[..., None]
    grey = px[..., :3].mean(axis=-1, keepdims=True)
    tint = np.array(srgb(colour), dtype=np.float32)
    # The colour named, shaded by how light the old paint was beside its own middle.
    shade = np.clip(grey / max(float((grey * m).sum() / max(m.sum(), 1e-6)), 1e-3), 0.3, 2.0)
    px[..., :3] = px[..., :3] * (1.0 - m * mix) + tint * shade * (m * mix)
    im.pixels.foreach_set(px.ravel())
    im.pack()
    if rough is not None or metal is not None:
        im = _image(o, 'Roughness')
        w, h = im.size
        px = np.empty(w * h * 4, dtype=np.float32)
        im.pixels.foreach_get(px)
        px = px.reshape(h, w, 4)
        m = np.clip(_fit(mask.astype(np.float32), im), 0.0, 1.0)
        if rough is not None:
            px[..., 1] = px[..., 1] * (1.0 - m) + rough * m
        if metal is not None:
            px[..., 2] = px[..., 2] * (1.0 - m) + metal * m
        im.pixels.foreach_set(px.ravel())
        im.pack()
    print(f'PAINT {o.name}: {float(m.mean()) * 100:.2f}% of the map toward {colour}')


def band(x, lo, hi, soft):
    """1 between `lo` and `hi`, easing to 0 over `soft` outside them."""
    return np.clip((x - lo) / soft + 1.0, 0.0, 1.0) * np.clip((hi - x) / soft + 1.0, 0.0, 1.0)


def _lit(o):
    """Each face's middle, its area, and how lit its paint is there (0..1):
    the glow map `hull` made, read at the face's own place on it."""
    me = o.data
    n = len(me.polygons)
    mid = np.empty(n * 3)
    area = np.empty(n)
    me.polygons.foreach_get('center', mid)
    me.polygons.foreach_get('area', area)
    lit = np.zeros(n)
    uv = me.uv_layers.active.data if me.uv_layers.active else None
    for material in me.materials:
        bsdf = next(x for x in material.node_tree.nodes if x.type == 'BSDF_PRINCIPLED')
        links = bsdf.inputs['Emission Color'].links
        if not links or uv is None:
            continue
        im = links[0].from_node.image
        w, h = im.size
        px = np.empty(w * h * 4, dtype=np.float32)
        im.pixels.foreach_get(px)
        val = px.reshape(h, w, 4)[..., :3].max(axis=-1)
        for p in me.polygons:
            u = sum((uv[i].uv for i in p.loop_indices), Vector((0, 0))) / p.loop_total
            lit[p.index] = val[min(h - 1, int((u.y % 1.0) * h)), min(w - 1, int((u.x % 1.0) * w))]
    return mid.reshape(n, 3), area, lit


def rings(o, behind):
    """Its thrusters' ends, found by the rings of light Meshy painted round
    them aft of `behind` (x, hull space): `(x, y, z, r)` each in hull
    space, the ring's rearmost x, its middle and its radius; the one on
    the left, the one on the right, then the one on the centre line if it
    has one."""
    mid, area, lit = _lit(o)
    out = []
    for lo, hi in ((0.15, 9.0), (-9.0, -0.15), (-0.15, 0.15)):
        on = (lit > 0.3) & (mid[:, 0] < behind) & (mid[:, 1] > lo) & (mid[:, 1] < hi)
        if not on.any():
            continue
        # The ring at its end, not a light strip that runs up to it.
        on &= mid[:, 0] < mid[on][:, 0].min() + 0.2
        w = area[on] * lit[on]
        c = (mid[on] * w[:, None]).sum(axis=0) / w.sum()
        r = float((np.hypot(mid[on][:, 1] - c[1], mid[on][:, 2] - c[2]) * w).sum() / w.sum())
        out.append((float(mid[on][:, 0].min()), float(c[1]), float(c[2]), r))
    return out


def nozzles(o, behind):
    """`rings`, as `craft.rs` wants them for its flames: `(x, y, z)` each
    in the client's frame (x forward, y up, z right)."""
    return [(round(x, 3), round(z, 3), round(-y, 3)) for x, y, z, _ in rings(o, behind)]


def thrusters(o, tx, found, iron='#2b2a36', throat=0.8, can=2.1, deep=0.16, long=0.3):
    """Each of `found` (`rings`) with its throat alight and its can dark
    iron: the paint inside `throat` of the ring's radius and within `deep`
    metres of its end is lit; what is within `can` radii of its axis and
    `long` metres of its end, bare steel as Meshy paints an engine, is
    painted `iron`. `tx` is `texels(o)`."""
    pos, _, on = tx
    lit = np.zeros(on.shape, dtype=np.float32)
    dark = np.zeros(on.shape, dtype=np.float32)
    for x, y, z, r in found:
        off = np.hypot(pos[..., 1] - y, pos[..., 2] - z)
        lit = np.maximum(lit, band(pos[..., 0], x - 0.05, x + deep, 0.02) * np.clip((r * throat - off) / (r * 0.25), 0.0, 1.0))
        dark = np.maximum(dark, band(pos[..., 0], x - 0.05, x + long, 0.04) * np.clip((r * can - off) / (r * 0.3), 0.0, 1.0))
    paint(o, on * dark * (1.0 - lit), iron, mix=0.8, rough=0.55, metal=0.8)
    light(o, on * lit)


def _wire(lo, hi, colour, name):
    """A box's edges, thin and lit, for the sheet."""
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    for v in bm.verts:
        v.co = Vector(((lo[i] + hi[i]) / 2 + v.co[i] * (hi[i] - lo[i]) for i in range(3)))
    bm.to_mesh(me)
    bm.free()
    ob = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(ob)
    w = ob.modifiers.new('wire', 'WIREFRAME')
    w.thickness = 0.014
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    e = nt.nodes.new('ShaderNodeEmission')
    e.inputs['Color'].default_value = (*colour, 1)
    e.inputs['Strength'].default_value = 1.0
    nt.links.new(e.outputs[0], nt.nodes.new('ShaderNodeOutputMaterial').inputs[0])
    me.materials.append(m)
    return ob


def _aim(ob, eye, at):
    ob.location = eye
    ob.rotation_euler = (Vector(at) - Vector(eye)).to_track_quat('-Z', 'Y').to_euler()


def sheet(o, path, steel=(), rider=(), marks=(), chase=None):
    """Six views of a hull-space craft on one sheet, three across:

        its side          from over it      from ahead
        three-quarter on  from the chase    from under and behind
                          camera

    The top row is flat (no perspective), with the sim's hull over it:
    `steel`, the boxes that stop a shot (yellow); `rider`, the box the pilot
    is (orange); `marks`, where its bolts leave (orange). The bottom row is
    as a player meets it: `chase` is the camera at its own controls (its
    eye and what it looks at, hull space). A sun, a sky's fill and a floor
    under it, so a hover's gap and the paint's sheen read as they will."""
    s = bpy.context.scene
    s.render.engine = 'BLENDER_EEVEE'
    s.eevee.taa_render_samples = 32
    s.eevee.use_bloom = True
    s.eevee.bloom_intensity = 0.05
    s.eevee.use_gtao = True
    s.eevee.use_ssr = True
    s.view_settings.view_transform = 'Standard'
    s.render.resolution_x, s.render.resolution_y = CELL
    s.render.film_transparent = False
    w = bpy.data.worlds.new('w')
    w.use_nodes = True
    w.node_tree.nodes['Background'].inputs['Color'].default_value = (0.42, 0.52, 0.66, 1)
    w.node_tree.nodes['Background'].inputs['Strength'].default_value = 0.9
    s.world = w
    lo, hi = bounds(o)
    size = max(hi - lo)
    mid = (lo + hi) / 2

    floor = bpy.data.objects.new('floor', bpy.data.meshes.new('floor'))
    bm = bmesh.new()
    bmesh.ops.create_grid(bm, x_segments=1, y_segments=1, size=60.0)
    bm.to_mesh(floor.data)
    bm.free()
    s.collection.objects.link(floor)
    fm = bpy.data.materials.new('floor')
    fm.use_nodes = True
    fb = fm.node_tree.nodes['Principled BSDF']
    fb.inputs['Base Color'].default_value = (0.36, 0.33, 0.28, 1)
    fb.inputs['Roughness'].default_value = 0.9
    floor.data.materials.append(fm)
    sun = bpy.data.objects.new('sun', bpy.data.lights.new('sun', 'SUN'))
    s.collection.objects.link(sun)
    sun.data.energy = 4.0
    sun.data.color = (1.0, 0.95, 0.86)
    sun.rotation_euler = (math.radians(50), math.radians(-12), math.radians(-35))

    wires = [_wire(a, b, STEEL, 'steel') for a, b in steel] + [_wire(a, b, RIDER, 'rider') for a, b in rider]
    for at in marks:
        r = Vector((0.05, 0.05, 0.05))
        wires.append(_wire(Vector(at) - r, Vector(at) + r, RIDER, 'mark'))

    camera = bpy.data.objects.new('cam', bpy.data.cameras.new('cam'))
    s.collection.objects.link(camera)
    s.camera = camera
    far = size * 6
    span = size * 1.18
    aspect = CELL[0] / CELL[1]
    # name, flat?, eye, what it looks at, the hull over it?
    views = [
        ('side', True, (mid.x, -far, mid.z), mid, True),
        ('top', True, (mid.x, mid.y, far), mid, True),
        ('front', True, (far, mid.y, mid.z), mid, True),
        ('quarter', False, (mid.x + size * 0.95, size * 0.8, hi.z + size * 0.32), mid, False),
        ('chase', False, chase[0] if chase else (mid.x - size * 1.4, 0, hi.z + size * 0.4),
         chase[1] if chase else mid, False),
        ('under', False, (mid.x - size * 0.9, -size * 0.75, lo.z - size * 0.02 + 0.25), mid + Vector((0, 0, size * 0.05)),
         False),
    ]
    cells = []
    os.makedirs(os.path.dirname(path), exist_ok=True)
    stem = os.path.splitext(path)[0]
    for name, flat, eye, at, over in views:
        camera.data.type = 'ORTHO' if flat else 'PERSP'
        camera.data.clip_end = far * 4
        if flat:
            # A craft wider than it is long (the flier from over it) still fits.
            camera.data.ortho_scale = max(span, (hi.y - lo.y) * 1.18 * aspect if name == 'top' else 0.0)
            _aim(camera, eye, at)
            if name == 'top':
                # The nose to the right, as the side view has it.
                camera.rotation_euler = (0, 0, 0)
        else:
            camera.data.angle = math.radians(90 if name == 'chase' else 50)
            _aim(camera, eye, at)
        for x in wires:
            x.hide_render = not over
        # The floor from the side is a line through the craft's shadow.
        floor.hide_render = flat and name != 'top'
        s.render.filepath = f'{stem}.{name}.png'
        bpy.ops.render.render(write_still=True)
        cells.append(s.render.filepath)
    for x in wires + [camera, sun, floor]:
        bpy.data.objects.remove(x, do_unlink=True)

    cw, ch = CELL
    out = np.zeros((2 * ch, 3 * cw, 4), dtype=np.float32)
    for i, f in enumerate(cells):
        im = bpy.data.images.load(f)
        px = np.empty(cw * ch * 4, dtype=np.float32)
        im.pixels.foreach_get(px)
        row, col = 1 - i // 3, i % 3
        out[row * ch:(row + 1) * ch, col * cw:(col + 1) * cw] = px.reshape(ch, cw, 4)
        bpy.data.images.remove(im)
        os.remove(f)
    whole = bpy.data.images.new('sheet', 3 * cw, 2 * ch, alpha=False)
    whole.pixels.foreach_set(out.ravel())
    whole.filepath_raw = path
    whole.file_format = 'PNG'
    whole.save()
    print('SHEET', path)


def deliver(name, o, steel, rider=(), marks=(), chase=None, behind=0.0, note=''):
    """Exports hull-space `o` as the craft `name` and renders its sheet.
    `steel`: the sim's boxes that stop a shot, `(lo, hi)` each in hull space
    (`Vehicle::hull`); the fit printed is the craft against them, and the
    nozzles (`nozzles`, aft of `behind`) are for `craft.rs`. Call last."""
    lo, hi = bounds(o)
    mid, area, _ = _lit(o)
    inside = np.zeros(len(area), dtype=bool)
    for a, b in steel:
        inside |= np.all((mid >= np.array(a) - 0.03) & (mid <= np.array(b) + 0.03), axis=1)
    out = float(area[~inside].sum() / area.sum())
    info = {
        'file': name,
        'size_m': [round(v, 3) for v in (hi - lo)],
        'belly': round(lo.z, 3),
        'top': round(hi.z, 3),
        'outside_steel': round(out, 3),
        'nozzles': nozzles(o, behind),
        'tris': sum(len(p.vertices) - 2 for p in o.data.polygons),
        'note': note,
    }
    os.makedirs(OUT, exist_ok=True)
    sheet(o, os.path.join(OUT, name + '.png'), steel, rider, marks, chase)
    os.makedirs(MODELS, exist_ok=True)
    o.name = o.data.name = name
    bpy.ops.object.select_all(action='DESELECT')
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    path = os.path.join(MODELS, name + '.glb')
    bpy.ops.export_scene.gltf(filepath=path, export_format='GLB', export_apply=True, export_yup=True, use_selection=True,
                              export_image_format='JPEG', export_jpeg_quality=88)
    info['bytes'] = os.path.getsize(path)
    with open(os.path.join(OUT, name + '.json'), 'w') as f:
        json.dump(info, f, indent=1)
    print(f"FIT {name}: {info['size_m'][0]} long, {info['size_m'][1]} wide, {info['belly']} to {info['top']} over the "
          f"ground; {out * 100:.0f}% of its skin is outside what stops a shot")
    print('MODEL', json.dumps(info))
    return info

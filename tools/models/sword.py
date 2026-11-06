"""Veil sword: a short, plain hilt in shell plates, and from its mouth one
blade of hard light, which is the game's (`look/blade.rs`, `shaders/blade.wgsl`):
the mesh here is only its shape. +X is the blade's direction.
The hilt from Meshy (tools/models/meshy_gen.py sword)."""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lib
import numpy as np
from lib import *

IMAGE = ('A single minimal alien sword hilt with no blade. A short, slim, gently waisted handle of deep violet '
         'metal with an iridescent sheen turning blue at the edges like a beetle wing case, two long overlapping '
         'plates with dark seams, and a band of dark iron ribbing where the hand grips it. Its front end narrows '
         'to a small collar of dark iron round a narrow slit of a mouth. Its rear is a smooth rounded pommel. No '
         'guard, no crossbar, no blade, no beam, nothing coming out of it. It is alien-made: smooth and organic, '
         'with no screws, no rails and no straight edges. Battle-used. 24 centimetres long and 4 centimetres wide.')
TEXTURE = ('Deep violet metal with an iridescent sheen turning blue at the edges like a beetle wing case, overlapping '
           'plates with dark seams, dark iron ribbing and a dark iron collar, battle-used')
ASPECT = '16:9'
FRONT = 'the narrow mouth of the hilt'

# The hilt's length, the fist's place on it (the middle of its ribbed band) and
# how far the mouth's slit lets the blade start inside it, metres.
HILT_M = 0.24
FIST_X = 0.092
ROOT_IN_M = 0.012
# The blade: how long, its half-width over and under its middle line, its half
# thickness and how far its tip sweeps up, metres.
BLADE_M = 0.84
OVER_M = 0.026
UNDER_M = 0.04
THICK_M = 0.011
SWEEP_M = 0.085
# How far up from its rear the pommel's paint is cleaned, and how far from grey
# (on sRGB's curve) counts as painted.
POMMEL_M = 0.06
GREY_CHROMA = 0.16
# Rings along it (the shader's slices jump a ring at a time) and points round one.
RINGS = 64
ROUND = 12


def clean_pommel(o):
    """Meshy painted a grey plate on each flank of the pommel, in the rear
    POMMEL_M: the colour map's grey there goes the violet round it."""
    me = o.data
    bsdf = next(n for n in me.materials[0].node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
    im = lib._image_of(bsdf.inputs['Base Color'])
    rgba = lib._pixels(im)
    h, w = rgba.shape[:2]
    uv = me.uv_layers[0].data
    mask = np.zeros((h, w), bool)
    for f in me.polygons:
        if max(me.vertices[v].co.x for v in f.vertices) >= POMMEL_M:
            continue
        pts = [np.array(uv[i].uv[:]) * (w, h) for i in f.loop_indices]
        for b, c in zip(pts[1:], pts[2:]):
            a = pts[0]
            lo = np.floor(np.min([a, b, c], axis=0)).astype(int)
            hi = np.ceil(np.max([a, b, c], axis=0)).astype(int) + 1
            xs, ys = np.meshgrid(np.arange(lo[0], hi[0]), np.arange(lo[1], hi[1]))
            d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1])
            if abs(d) < 1e-9:
                continue
            l0 = ((b[1] - c[1]) * (xs - c[0]) + (c[0] - b[0]) * (ys - c[1])) / d
            l1 = ((c[1] - a[1]) * (xs - c[0]) + (a[0] - c[0]) * (ys - c[1])) / d
            on = (l0 >= 0) & (l1 >= 0) & (l0 + l1 <= 1)
            mask[np.clip(ys[on], 0, h - 1), np.clip(xs[on], 0, w - 1)] = True
    tone = np.sqrt(np.maximum(rgba[..., :3], 0.0))
    grey = mask & (tone.max(axis=2) - tone.min(axis=2) < GREY_CHROMA)
    for _ in range(2):
        grey = mask & (grey | np.roll(grey, 1, 0) | np.roll(grey, -1, 0) | np.roll(grey, 1, 1) | np.roll(grey, -1, 1))
    print(f'POMMEL {mask.sum()} texels, {grey.sum()} grey')
    rgba[grey, :3] = np.median(rgba[mask & ~grey, :3], axis=0)
    im.pixels.foreach_set(rgba.ravel())
    im.update()


def blade(root_x, root_z, uv_name):
    """The blade's shape, one curved lens from the mouth to a long fine point,
    UV (across, -1 to 1 edge to edge; along, 0 at the mouth to 1 at the tip,
    after glTF's flip). Its material's name is what the game looks for."""
    verts, faces, uvs = [], [], []
    for i in range(RINGS + 1):
        t = i / RINGS
        neck = 0.2 + 0.8 * min(t / 0.07, 1.0) ** 0.6
        point = max(t - 0.52, 0.0) / 0.48
        taper = max(1.0 - point ** 2.1, 0.012)
        belly = 1.0 + 0.1 * math.sin(math.pi * min(t / 0.55, 1.0))
        k = neck * taper * belly
        z = root_z + SWEEP_M * t ** 2.2
        for j in range(ROUND):
            turn = 2.0 * math.pi * j / ROUND
            u, side = math.cos(turn), math.sin(turn)
            over = OVER_M if u > 0 else UNDER_M
            verts.append((root_x + BLADE_M * t, THICK_M * k * side, z + over * k * u))
            uvs.append((u, 1.0 - t))
    for i in range(RINGS):
        for j in range(ROUND):
            a, b = i * ROUND + j, i * ROUND + (j + 1) % ROUND
            faces.append((a, b, b + ROUND, a + ROUND))
    me = bpy.data.meshes.new('blade')
    me.from_pydata(verts, [], faces)
    me.update()
    layer = me.uv_layers.new(name=uv_name)
    for loop in me.loops:
        layer.data[loop.index].uv = uvs[loop.vertex_index]
    material = bpy.data.materials.new('blade')
    material.use_nodes = True
    bsdf = next(n for n in material.node_tree.nodes if n.type == 'BSDF_PRINCIPLED')
    bsdf.inputs['Base Color'].default_value = (0.5, 0.95, 1.0, 1.0)
    bsdf.inputs['Emission Color'].default_value = (0.5, 0.95, 1.0, 1.0)
    bsdf.inputs['Emission Strength'].default_value = 1.0
    me.materials.append(material)
    o = bpy.data.objects.new('blade', me)
    bpy.context.collection.objects.link(o)
    for f in me.polygons:
        f.use_smooth = True
    return o


scene()
gun, rise = generated('sword', HILT_M, hold=0.50)
clean_pommel(gun)
light = blade(HILT_M - ROOT_IN_M, rise, gun.data.uv_layers[0].name)
tip = rise + SWEEP_M
finish('sword', [gun, light], grip=FIST_X, rise=tip, long=False, note='Veil sword: a plain hilt, one blade of hard light')

"""The visor: a band of dark steel wrapped round the trunk over the eyes with a slit of
hard light in it, a pod at each temple. Headwear (`proto::HEADWEAR`). Made here, no
maps. Warm, not cyan or red: a side's colours are the headband's."""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *
from gear import _b, _object

WIDTH = 0.44
UP = 1.552
# How far out from the trunk's middle it stands (it clears the nose's bridge), how
# far round it goes either side of straight ahead (degrees), its half-height at the
# middle and at its ends, and the slit's share of that and how far it stops short.
OUT = 0.212
SWEEP = 82.0
HALF_MID = 0.05
HALF_END = 0.022
SLIT = 0.3
SLIT_SWEEP = 58.0
STEPS = 36
# A band's bevel: how far its top and bottom edges fall back toward the trunk.
LIP = 0.016


def round_it(a, r=OUT, up=UP):
    t = math.radians(a)
    return _b(r * math.sin(t), up, TRUNK_Z + r * math.cos(t))


def half_at(a):
    return HALF_END + (HALF_MID - HALF_END) * math.cos(math.radians(a * 90 / SWEEP)) ** 0.8


def strip(bm, rows):
    """Quads between each row of points and the next, facing out."""
    verts = [[bm.verts.new(p) for p in row] for row in rows]
    for a, b in zip(verts, verts[1:]):
        for i in range(len(a) - 1):
            bm.faces.new((a[i], a[i + 1], b[i + 1], b[i]))
    bm.normal_update()
    for f in bm.faces:
        out = f.calc_center_median() - Vector((0, TRUNK_Z * -1, f.calc_center_median().z))
        if f.normal.dot(out) < 0:
            f.normal_flip()


scene()
steel = surface('steel', '#2a2d33', 0.75, 0.38)
light = surface('light', '#3a2708', 0.0, 0.3, glow=('#ffc871', 2.2))
band = bmesh.new()
rows = []
for i in range(STEPS + 1):
    a = -SWEEP + 2 * SWEEP * i / STEPS
    h = half_at(a)
    rows.append([round_it(a, OUT - LIP, UP + h + 0.004), round_it(a, OUT, UP + h * 0.8), round_it(a, OUT, UP - h * 0.8),
                 round_it(a, OUT - LIP, UP - h - 0.004)])
strip(band, rows)
for s in (1.0, -1.0):
    # A pod at each temple, and the strap from it round the side.
    pipe(band, [round_it(s * (SWEEP - 2 + k * 5), r=OUT - 0.004) for k in range(3)], 0.024, sides=8)
    pipe(band, [round_it(s * (SWEEP + 8 + k * 14), r=OUT - 0.014 - k * 0.002, up=UP) for k in range(5)], 0.007, sides=6)
slit = bmesh.new()
rows = []
for i in range(STEPS + 1):
    a = -SLIT_SWEEP + 2 * SLIT_SWEEP * i / STEPS
    fade = 1.0 - (abs(a) / SLIT_SWEEP) ** 4
    h = HALF_MID * SLIT * max(0.05, fade)
    rows.append([round_it(a, OUT + 0.0015, UP + h), round_it(a, OUT + 0.0015, UP - h)])
strip(slit, rows)
o = join([_object(band, 'band', steel), _object(slit, 'slit', light)], 'visor')
head_export(o, 'visor')

"""The aviators: gold wire, a double bridge, big teardrop lenses mirrored from
amber down to rose. Headwear (`proto::HEADWEAR`). Made here, no maps: a thin wire
is what a picture's mesh can't do."""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

WIDTH = 0.40
# A lens's half-width and half-height, and how far its outer corner droops.
HALF_W = 0.086
HALF_H = 0.074
DROOP = 0.034


def teardrop(n=40):
    """Round the lens, across outward: flat on top, sagging toward the outside."""
    out = []
    for i in range(n):
        t = 2 * math.pi * i / n
        u, v = HALF_W * math.cos(t), HALF_H * math.sin(t)
        if v > 0:
            v *= 0.72
        else:
            v -= DROOP * ((u / HALF_W + 1) / 2) ** 2 * -math.sin(t)
        out.append((u, v))
    return out


scene()
gold = surface('gold', '#d9b25a', 1.0, 0.3)
glass = surface('mirror', '#ffffff', 0.92, 0.12)
parts = wire_glasses(teardrop(), 0.097, 1.58, 0.178, 11.0, 0.012, 0.0042, gold, glass,
                     top=(0.5, 0.26, 0.06), bottom=(0.74, 0.34, 0.26), bridge=((0.8, 0.0), (0.56, 0.008)))
o = join(parts, 'aviators')
head_export(o, 'aviators')

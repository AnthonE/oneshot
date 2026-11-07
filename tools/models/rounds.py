"""The rounds: small round lenses on a thin silver wire, a keyhole bridge, dark plum
glass. Headwear (`proto::HEADWEAR`). Made here, no maps."""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

WIDTH = 0.36
RADIUS = 0.074

scene()
silver = surface('silver', '#c9ced6', 1.0, 0.28)
glass = surface('plum', '#ffffff', 0.7, 0.1)
circle = [(RADIUS * math.cos(2 * math.pi * i / 36), RADIUS * math.sin(2 * math.pi * i / 36)) for i in range(36)]
parts = wire_glasses(circle, 0.09, 1.572, 0.172, 8.0, 0.009, 0.0032, silver, glass,
                     top=(0.09, 0.02, 0.12), bottom=(0.2, 0.05, 0.2), bridge=((0.62, 0.02),))
o = join(parts, 'rounds')
head_export(o, 'rounds')

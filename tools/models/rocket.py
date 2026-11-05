"""The rocket: a cylinder with a conical nose, a flared nozzle and four
swept fins at the tail, an amber band under the nose. 0.60 m, radius
0.05. Not held: the grip at its middle."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *
from forge import *

scene()
P = []

P.append(cyl(0.050, 0.42, 'x', (0.235, 0, 0), 'gunmetal', verts=40, bevel=0.003))
P.append(cyl(0.050, 0.155, 'x', (0.5225, 0, 0), 'gunmetal_dark', verts=40, bevel=0.002, r2=0.008))
P.append(cyl(0.0515, 0.014, 'x', (0.435, 0, 0), 'amber', verts=40, bevel=0.001))
P.append(cyl(0.0515, 0.020, 'x', (0.060, 0, 0), 'gunmetal_dark', verts=40, bevel=0.001))
P.append(cyl(0.030, 0.028, 'x', (0.014, 0, 0), 'steel', verts=32, bevel=0.001, r2=0.046))
P.append(cyl(0.022, 0.004, 'x', (0.001, 0, 0), 'polymer', verts=32, bevel=0))
# Four swept fins, one profile turned about the axis.
for a in (0, 90, 180, 270):
    f = profile([(0.028, 0.046), (0.150, 0.046), (0.105, 0.092), (0.028, 0.092)], 0.005, 'xz', material='gunmetal_dark', bevel=0.001, seg=1)
    P.append(rotate(f, rx=a))

finish('rocket', P, grip=0.30, long=True, note='the projectile, not held')

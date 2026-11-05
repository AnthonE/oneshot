"""The frag: a canister grenade. A dark cylinder with a polymer band, a
bottom cap, a top cap narrowing to the fuze head with its clip ring, an
amber LED ring at the shoulder. 0.16 m along X, radius 0.045. Not held:
the grip at its middle."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *
from forge import *

scene()
P = []

P.append(cyl(0.045, 0.110, 'x', (0.070, 0, 0), 'gunmetal_dark', verts=32, bevel=0.003))
P.append(cyl(0.038, 0.016, 'x', (0.008, 0, 0), 'polymer', verts=32, bevel=0.002, r2=0.045))
P.append(cyl(0.0465, 0.026, 'x', (0.062, 0, 0), 'polymer', verts=32, bevel=0.002))
for x in (0.040, 0.092):
    P.append(cyl(0.0455, 0.004, 'x', (x, 0, 0), 'polymer', verts=32, bevel=0))
P.append(cyl(0.0458, 0.004, 'x', (0.125, 0, 0), 'amber', verts=32, bevel=0))
P.append(cyl(0.045, 0.020, 'x', (0.135, 0, 0), 'gunmetal', verts=32, bevel=0.002, r2=0.034))
P.append(cyl(0.024, 0.016, 'x', (0.152, 0, 0), 'gunmetal_dark', verts=24, bevel=0.002))
P.append(torus(0.014, 0.0035, 'y', (0.150, 0, 0.034), 'steel', segments=24, ring=8))
P.append(box(0.010, 0.016, 0.006, (0.150, 0, 0.024), 'steel', bevel=0.001, seg=1))

finish('frag', P, grip=0.08, long=False, note='a canister, not held')

"""The rocket launcher: one fat olive tube with gunmetal bands, a bell at
the muzzle, a breech at the rear over a shoulder rest, a rail under it
carrying the pistol grip and a foregrip, the display box on the left, a
carry handle on top. 1.05 m, shouldered."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *
from forge import *

scene()
P = []

# The tube, its bands, the muzzle bell with its dark bore, the breech with its vent.
P.append(cyl(0.070, 0.90, 'x', (0.53, 0, 0), 'olive', verts=48, bevel=0.003))
for x in (0.16, 0.52, 0.90):
    P.append(cyl(0.073, 0.032, 'x', (x, 0, 0), 'gunmetal', verts=48, bevel=0.002))
P.append(cyl(0.070, 0.072, 'x', (1.014, 0, 0), 'gunmetal_dark', verts=48, bevel=0.002, r2=0.088))
P.append(cyl(0.080, 0.004, 'x', (1.051, 0, 0), 'polymer', verts=48, bevel=0))
P.append(cyl(0.072, 0.08, 'x', (0.04, 0, 0), 'gunmetal_dark', verts=48, bevel=0.003))
P.append(cyl(0.056, 0.004, 'x', (0.001, 0, 0), 'polymer', verts=32, bevel=0))
# The rail under the tube; the shoulder rest behind the grip, the pistol grip, the foregrip.
P.append(box(0.46, 0.034, 0.018, (0.46, 0, -0.072), 'gunmetal_dark'))
P.append(box(0.07, 0.032, 0.028, (0.25, 0, -0.084), 'gunmetal_dark'))
P.append(box(0.12, 0.070, 0.026, (0.25, 0, -0.108), 'rubber', bevel=0.005))
P += grip(0.41, -0.074, depth=0.092)
P.append(box(0.044, 0.034, 0.016, (0.63, 0, -0.084), 'gunmetal_dark', bevel=0.002))
P.append(profile([(0.612, -0.090), (0.648, -0.090), (0.640, -0.170), (0.604, -0.170)], 0.032, 'xz', material='polymer'))
# The display box on the left, its face and a dim strip; an LED.
P.append(box(0.11, 0.034, 0.064, (0.46, 0.084, 0.012), 'gunmetal_dark'))
P.append(panel(0.080, 0.044, (0.46, 0.1015, 0.014), material='display', both=False, bevel=0))
P.append(panel(0.060, 0.004, (0.46, 0.1015, -0.014), material='display_dim', both=False, bevel=0))
P += led((0.53, 0.1015, 0.030))
# The carry handle: a ring half sunk into the tube's top, a sight post ahead of it.
P.append(torus(0.052, 0.009, 'y', (0.50, 0, 0.066), 'polymer', segments=32, ring=10))
P.append(box(0.06, 0.030, 0.022, (0.50, 0, 0.072), 'gunmetal_dark'))
P.append(box(0.014, 0.012, 0.024, (0.82, 0, 0.078), 'gunmetal_dark', bevel=0.001, seg=1))

finish('rocket-launcher', P, grip=0.398, long=True, note='one fat tube, shouldered; the display box on the left')

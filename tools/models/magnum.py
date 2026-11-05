"""The magnum: a big semi-auto pistol. A tall serrated slide over a thick
frame, a polymer grip holding the mag, a short rail under the barrel, a
compensator at the muzzle, the ammo panel on the slide's rear. 0.30 m.
Held by the grip."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *
from forge import *

scene()
P = []

# The slide, chamfered at the front, and the frame under it.
P.append(profile([(0.0, -0.018), (0.0, 0.026), (0.235, 0.026), (0.262, 0.008), (0.262, -0.018)], 0.040, 'xz', material='gunmetal', bevel=0.003))
P.append(profile([(0.004, -0.040), (0.010, -0.016), (0.262, -0.016), (0.262, -0.050), (0.036, -0.050)], 0.036, 'xz', material='gunmetal_dark', bevel=0.002))
P.append(panel(0.064, 0.054, (0.075, 0.019, -0.096), material='rubber', bevel=0.001))
# Serrations, rear and front, both sides; the ejection port on the right; an LED on the left.
for x in (0.024, 0.032, 0.040, 0.048, 0.056):
    P.append(panel(0.0035, 0.032, (x, 0.020, 0.004), material='polymer', bevel=0))
for x in (0.196, 0.206, 0.216):
    P.append(panel(0.0035, 0.028, (x, 0.020, 0.002), material='polymer', bevel=0))
P.append(panel(0.034, 0.016, (0.105, 0.020, 0.012), side=-1, material='polymer', both=False, bevel=0))
P += led((0.13, 0.018, -0.030))
# The ammo panel: a small wedge on the slide's rear top, and the front sight.
P += display_wedge(0.004, 0.056, 0.026, 0.020, 0.034, margin=0.0015, brow=False, nose=0.4)
P.append(box(0.008, 0.007, 0.010, (0.248, 0, 0.030), 'gunmetal_dark', bevel=0.001, seg=1))
# The grip holding the mag, the guard, the mag's base plate below it.
P += grip(0.085, -0.040, depth=0.105, rake=0.030, w=0.038, t=0.050, forward=0.046)
P.append(box(0.054, 0.038, 0.010, (0.056, 0, -0.148), 'polymer', bevel=0.002))
# Finger grooves on the grip's front, the slide stop and the mag release on the left.
for i in range(3):
    P.append(box(0.010, 0.034, 0.006, (0.110 - 0.030 * (0.015 + i * 0.022) / 0.105, 0, -0.056 - i * 0.022), 'rubber', bevel=0.002, seg=1))
P.append(box(0.030, 0.004, 0.008, (0.09, 0.020, -0.022), 'gunmetal_dark', bevel=0.001, seg=1))
P.append(cyl(0.005, 0.004, 'y', (0.112, 0.021, -0.042), 'gunmetal_dark', verts=12, bevel=0.0005))
# The short rail under the dust cover, hung upside down.
for o in rail(0.17, 0.25, 0.047, w=0.022, notches=3):
    P.append(rotate(o, rx=180))
# The barrel and the compensator: a steel block with two ports on top.
P.append(cyl(0.0095, 0.06, 'x', (0.275, 0, 0.002), 'steel', verts=24))
P.append(box(0.040, 0.038, 0.038, (0.281, 0, 0.002), 'steel', bevel=0.003))
for x in (0.270, 0.286):
    P.append(box(0.009, 0.020, 0.003, (x, 0, 0.0205), 'polymer', bevel=0, seg=1))
P.append(panel(0.20, 0.003, (0.13, 0.020, 0.016), bevel=0))

finish('magnum', P, grip=0.072, long=False, note='a pistol: held by the grip, the barrel forward of it')

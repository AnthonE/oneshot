"""The sniper rifle: a long heavy barrel ending in a big brake, the bipod
folded along it, a large scope in two rings, a skeletal stock with a cheek
rest, a box mag in front of the grip. 1.25 m."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *
from forge import *

scene()
P = []

# The skeletal stock: a butt plate, two bars, a strut, a cheek rest.
P += butt_pad(0.0, 0.048, 0.120, z=-0.004)
P.append(box(0.022, 0.044, 0.116, (0.027, 0, -0.004), 'gunmetal_dark'))
P.append(box(0.29, 0.036, 0.018, (0.18, 0, 0.040), 'gunmetal'))
P.append(box(0.27, 0.036, 0.016, (0.17, 0, -0.050), 'gunmetal'))
P.append(box(0.018, 0.030, 0.080, (0.17, 0, -0.005), 'gunmetal'))
P.append(box(0.11, 0.048, 0.022, (0.13, 0, 0.060), 'polymer'))
# The receiver, its panel lines, the bolt handle (right), the count panel (left).
P.append(profile([(0.30, -0.042), (0.30, 0.050), (0.58, 0.050), (0.62, 0.028), (0.62, -0.030), (0.58, -0.042)], 0.066, 'xz', material='gunmetal'))
P.append(panel(0.26, 0.004, (0.45, 0.034, -0.014)))
P.append(panel(0.004, 0.084, (0.43, 0.034, 0.004)))
P.append(panel(0.044, 0.020, (0.345, 0.034, 0.018), material='display', bevel=0, both=False))
P += led((0.39, 0.034, 0.030))
P.append(cyl(0.006, 0.040, 'y', (0.47, -0.050, 0.012), 'steel', verts=12))
P.append(sphere(0.010, (0.47, -0.069, 0.012), 'steel', segments=12))
# The rail, the two rings and the scope: eyepiece, tube, turrets, objective bell, glass both ends.
P += rail(0.30, 0.60, 0.050, notches=6)
for x in (0.375, 0.515):
    P.append(box(0.022, 0.040, 0.024, (x, 0, 0.072), 'gunmetal_dark', bevel=0.002))
    P.append(torus(0.022, 0.005, 'x', (x, 0, 0.096), 'gunmetal_dark', segments=24, ring=8))
P.append(cyl(0.019, 0.24, 'x', (0.445, 0, 0.096), 'gunmetal', verts=24))
P.append(cyl(0.019, 0.045, 'x', (0.5875, 0, 0.096), 'gunmetal', verts=24, r2=0.031))
P.append(cyl(0.028, 0.003, 'x', (0.6085, 0, 0.096), 'glass', verts=24, bevel=0))
P.append(cyl(0.026, 0.035, 'x', (0.3075, 0, 0.096), 'gunmetal', verts=24, r2=0.019))
P.append(cyl(0.023, 0.003, 'x', (0.2915, 0, 0.096), 'glass', verts=24, bevel=0))
P.append(cyl(0.011, 0.018, 'z', (0.46, 0, 0.122), 'gunmetal_dark', verts=16))
P.append(cyl(0.011, 0.018, 'y', (0.46, 0.026, 0.096), 'gunmetal_dark', verts=16))
# The grip and its guard, the box mag in front.
P += grip(0.365, -0.042, w=0.036, t=0.044)
P += mag(0.49, -0.044, 0.085, dx=0.072, w=0.034, curve=0)
# The shroud, the barrel, the big brake, the bipod folded under the barrel, the flashlight under the shroud.
P += shroud(0.62, 0.80, 0.056, 0.056)
P.append(cyl(0.012, 0.64, 'x', (0.93, 0, 0), 'steel', verts=24))
P += brake(1.25, 0.012, length=0.075, r_out=0.024, slots=4)
P.append(box(0.034, 0.052, 0.032, (0.815, 0, -0.022), 'gunmetal_dark'))
for y in (0.018, -0.018):
    P.append(cyl(0.005, 0.26, 'x', (0.955, y, -0.026), 'gunmetal_dark', verts=12, bevel=0.001))
    P.append(box(0.016, 0.012, 0.012, (1.085, y, -0.026), 'rubber', bevel=0.002, seg=1))
P += flashlight(0.66, 0.78, 0.014, -0.048, mount_z=-0.025)

finish('sniper', P, grip=0.352, long=True, note='skeletal stock, big scope in rings, bipod folded under the barrel')

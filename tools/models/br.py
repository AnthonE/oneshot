"""The battle rifle: longer than the AR, the mag in front of the grip, a
boxed scope on the rail instead of the display (a dim strip on the side
carries the count), a long barrel ending in a muzzle brake. 0.88 m."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *
from forge import *

scene()
P = []

# The stock: a slab with a rising toe, then the receiver, taller and wider.
P.append(profile([(0.016, -0.052), (0.016, 0.040), (0.036, 0.056), (0.25, 0.056), (0.25, -0.026), (0.19, -0.052)], 0.062, 'xz', material='gunmetal'))
P.append(profile([(0.235, -0.040), (0.235, 0.060), (0.515, 0.060), (0.545, 0.034), (0.545, -0.034), (0.515, -0.040)], 0.072, 'xz', material='gunmetal'))
P += butt_pad(0.0, 0.060, 0.098, z=-0.006)
P.append(box(0.10, 0.056, 0.012, (0.12, 0, 0.058), 'polymer', bevel=0.003))
P.append(panel(0.14, 0.046, (0.125, 0.032, -0.006), material='olive', bevel=0.001))
# Panel lines, the ejection port (right), the count strip and an LED (left).
P.append(panel(0.26, 0.004, (0.39, 0.037, -0.014)))
P.append(panel(0.004, 0.092, (0.262, 0.037, 0.008)))
P.append(panel(0.050, 0.030, (0.455, 0.037, 0.016), side=-1, material='polymer', both=False))
P.append(panel(0.044, 0.006, (0.300, 0.037, 0.034), material='display_dim', bevel=0, both=False))
P += led((0.345, 0.037, 0.034))
# The rail and the boxed scope on it: two rings, a housing, the tube, a
# bell forward, the eyepiece, glass both ends, a turret on top.
P += rail(0.24, 0.515, 0.060, w=0.026, notches=6)
for x in (0.325, 0.445):
    P.append(box(0.018, 0.034, 0.018, (x, 0, 0.080), 'gunmetal_dark', bevel=0.002))
P.append(box(0.11, 0.046, 0.044, (0.385, 0, 0.098), 'gunmetal_dark'))
P.append(cyl(0.018, 0.19, 'x', (0.385, 0, 0.098), 'gunmetal', verts=24))
P.append(cyl(0.019, 0.034, 'x', (0.497, 0, 0.098), 'gunmetal', verts=24, r2=0.026))
P.append(cyl(0.023, 0.003, 'x', (0.5145, 0, 0.098), 'glass', verts=24, bevel=0))
P.append(cyl(0.023, 0.026, 'x', (0.277, 0, 0.098), 'gunmetal', verts=24, r2=0.018))
P.append(cyl(0.020, 0.003, 'x', (0.2635, 0, 0.098), 'glass', verts=24, bevel=0))
P.append(box(0.014, 0.014, 0.010, (0.40, 0, 0.124), 'polymer', bevel=0.001, seg=1))
# The grip and its guard, the mag in front of it, the handguard under the shroud.
P += grip(0.32, -0.040, w=0.036, t=0.044)
P += mag(0.428, -0.044, 0.112, dx=0.064, w=0.032, curve=5)
# The shroud, a gas block, the barrel and its brake, the flashlight under the shroud.
P += shroud(0.545, 0.715, 0.056, 0.056)
P.append(box(0.024, 0.028, 0.030, (0.735, 0, 0.006), 'gunmetal_dark', bevel=0.002))
P.append(cyl(0.011, 0.36, 'x', (0.70, 0, 0), 'steel', verts=24))
P += brake(0.88, 0.011, length=0.05, r_out=0.019)
P += flashlight(0.59, 0.71, 0.016, -0.050, mount_z=-0.028)

finish('br', P, grip=0.307, long=True, note='a boxed scope on the rail, the mag in front of the grip, a brake')

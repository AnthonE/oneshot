"""The shotgun: a pump. The tube magazine under the barrel with a ribbed
fore-end on it, a slotted heat shield over the barrel, a solid stock with
a rubber pad, four brass shells in a side-saddle on the receiver, a small
display. 0.95 m."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *
from forge import *

scene()
P = []

# The solid stock, its pad and cheek piece; the receiver.
P.append(profile([(0.016, -0.066), (0.016, 0.028), (0.040, 0.040), (0.30, 0.040), (0.30, -0.032), (0.21, -0.052)], 0.044, 'xz', material='polymer'))
P += butt_pad(0.0, 0.044, 0.096, z=-0.019)
P.append(box(0.12, 0.048, 0.012, (0.14, 0, 0.038), 'rubber', bevel=0.003))
P.append(profile([(0.29, -0.040), (0.29, 0.050), (0.46, 0.050), (0.49, 0.030), (0.49, -0.040)], 0.056, 'xz', material='gunmetal'))
P.append(panel(0.17, 0.004, (0.39, 0.029, -0.016)))
P.append(panel(0.048, 0.030, (0.40, 0.029, 0.010), side=-1, material='polymer', both=False))
# The side-saddle on the left: a plate and four brass shells lying along the barrel.
P.append(panel(0.084, 0.082, (0.385, 0.029, 0.004), material='polymer', both=False, bevel=0.001, thick=0.004))
for z in (-0.024, -0.005, 0.014, 0.033):
    P.append(cyl(0.009, 0.062, 'x', (0.385, 0.040, z), 'brass', verts=12, bevel=0.001))
    P.append(cyl(0.0095, 0.006, 'x', (0.358, 0.040, z), 'brass', verts=12, bevel=0))
# The display on the receiver's rear top, the rail in front of it, an LED.
P += display_wedge(0.295, 0.360, 0.050, 0.026, 0.038, margin=0.002, nose=0.4)
P += rail(0.37, 0.46, 0.050, notches=3)
P += led((0.33, 0.029, 0.030))
# The grip and its guard.
P += grip(0.335, -0.034)
# The barrel, the slotted heat shield over it, the muzzle; the tube mag under it with its cap and band.
P.append(cyl(0.011, 0.46, 'x', (0.72, 0, 0), 'steel', verts=24))
P += shroud(0.52, 0.85, 0.040, 0.030, z=0.012, slots=4)
P.append(tube(0.014, 0.0105, 0.030, 'x', (0.935, 0, 0), 'steel', verts=24))
P.append(cyl(0.011, 0.40, 'x', (0.70, 0, -0.027), 'steel', verts=20))
P.append(cyl(0.013, 0.022, 'x', (0.905, 0, -0.027), 'gunmetal_dark', verts=20))
P.append(box(0.022, 0.036, 0.062, (0.866, 0, -0.012), 'gunmetal_dark'))
# The ribbed fore-end on the tube and its action bars back into the receiver.
P.append(cyl(0.020, 0.17, 'x', (0.635, 0, -0.027), 'polymer', verts=16, bevel=0.003))
for x in (0.575, 0.605, 0.635, 0.665, 0.695):
    P.append(cyl(0.0225, 0.010, 'x', (x, 0, -0.027), 'polymer', verts=16, bevel=0.001))
for y in (0.015, -0.015):
    P.append(cyl(0.004, 0.13, 'x', (0.53, y, -0.036), 'steel', verts=8, bevel=0))
# The flashlight under the tube mag at the front.
P += flashlight(0.77, 0.89, 0.013, -0.056, mount_z=-0.030)

finish('shotgun', P, grip=0.322, long=True, note='pump: tube mag, ribbed fore-end, side-saddle of four shells')

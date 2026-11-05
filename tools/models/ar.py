"""The assault rifle: a bullpup. One tall, wide chamfered block is receiver
and stock at once, the mag behind the grip, a big display housing over
the grip, a stubby shrouded barrel and the flashlight under it. 0.78 m."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *
from forge import *

scene()
P = []

W = 0.084  # the body's width
# The body: receiver and stock in one slab, the stock taller, chamfered at
# the rear top, stepping down to the receiver, the nose sloping to the barrel.
P.append(profile([(0.016, -0.072), (0.016, 0.058), (0.046, 0.086), (0.215, 0.086), (0.245, 0.072), (0.50, 0.072),
                  (0.56, 0.042), (0.56, -0.024), (0.52, -0.044), (0.27, -0.044), (0.245, -0.072)], W, 'xz', material='gunmetal'))
P += butt_pad(0.0, W - 0.004, 0.126, z=-0.006)
# The stock's toe, a polymer cheek piece on its top, and the olive side
# panels over the stock.
P.append(box(0.12, W - 0.008, 0.018, (0.09, 0, -0.064), 'polymer'))
P.append(box(0.13, W - 0.014, 0.012, (0.11, 0, 0.088), 'polymer', bevel=0.003))
P.append(panel(0.17, 0.09, (0.125, W / 2, 0.006), material='olive', bevel=0.001))
# Panel lines, the olive receiver panel, side vents up front and the
# ejection port (right side only, above the mag).
P.append(panel(0.26, 0.004, (0.385, W / 2, -0.012)))
P.append(panel(0.004, 0.11, (0.245, W / 2, 0.006)))
P.append(panel(0.15, 0.034, (0.39, W / 2, 0.028), material='olive', bevel=0.001))
for i in range(3):
    P.append(panel(0.006, 0.028, (0.455 + i * 0.022, W / 2, -0.026), material='polymer'))
P.append(panel(0.056, 0.030, (0.19, W / 2, 0.026), side=-1, material='polymer', both=False))
P += led((0.30, W / 2, 0.046))
# The rail, the big display housing on it, and the dim strip behind it.
P += rail(0.245, 0.52, 0.072, w=0.026, notches=6)
P += display_wedge(0.255, 0.43, 0.083, 0.066, 0.062, nose=0.45)
P.append(panel(0.06, 0.004, (0.335, 0.031, 0.095), material='display_dim', bevel=0))
# The mag behind the grip, the grip with its guard, the handguard under the
# front with its finger grooves.
P += mag(0.175, -0.072, 0.13, dx=0.072, w=0.034)
P += grip(0.315, -0.044, w=0.036, t=0.044)
P.append(box(0.14, W - 0.012, 0.028, (0.465, 0, -0.054), 'polymer'))
for i in range(4):
    P.append(box(0.004, W - 0.008, 0.004, (0.415 + i * 0.03, 0, -0.068), 'gunmetal_dark', bevel=0, seg=1))
# The barrel, its shroud, the muzzle ring, the front post and the flashlight.
P.append(cyl(0.011, 0.26, 'x', (0.66, 0, 0.004), 'steel', verts=24))
P += shroud(0.555, 0.715, 0.062, 0.062, z=0.004, slots=3, slot_len=0.034)
P.append(tube(0.016, 0.010, 0.034, 'x', (0.77, 0, 0.004), 'steel', verts=24))
P.append(box(0.014, 0.012, 0.018, (0.70, 0, 0.044), 'gunmetal_dark', bevel=0.001, seg=1))
P += flashlight(0.60, 0.73, 0.018, -0.052, mount_z=-0.027)

finish('ar', P, grip=0.302, long=True, note='bullpup: the mag behind the grip, the display over it')

"""The SMG: compact. A box receiver on a side-folding stock (a bar and a
plate), a stick mag in front of the grip, a stubby shrouded barrel with
the flashlight under it, a low display wedge doubling as the rear sight.
0.55 m."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *
from forge import *

scene()
P = []

# The folding stock: the pad, its plate, the bar, the hinge on the receiver's rear.
P += butt_pad(0.0, 0.030, 0.090, z=0.0, thick=0.014)
P.append(box(0.014, 0.028, 0.086, (0.021, 0, 0), 'gunmetal_dark', bevel=0.002))
P.append(box(0.16, 0.014, 0.022, (0.10, 0, 0.018), 'gunmetal_dark', bevel=0.002))
P.append(box(0.026, 0.056, 0.038, (0.183, 0, 0.016), 'gunmetal_dark'))
# The receiver, its lines, the port (right), the charging handle (left), the count strip (left).
P.append(profile([(0.17, -0.036), (0.17, 0.040), (0.42, 0.040), (0.44, 0.024), (0.44, -0.026), (0.42, -0.036)], 0.050, 'xz', material='gunmetal'))
P.append(panel(0.22, 0.004, (0.30, 0.026, -0.012)))
P.append(panel(0.040, 0.024, (0.30, 0.026, 0.010), side=-1, material='polymer', both=False))
P.append(box(0.022, 0.014, 0.010, (0.24, 0.031, 0.026), 'gunmetal_dark', bevel=0.001, seg=1))
P.append(panel(0.044, 0.010, (0.265, 0.026, 0.012), material='display_dim', both=False, bevel=0))
P += led((0.33, 0.026, 0.024))
# The rail, the low display wedge at its rear as the sight, a front post.
P += rail(0.20, 0.42, 0.040, notches=5)
P += display_wedge(0.20, 0.265, 0.051, 0.020, 0.030, margin=0.002, nose=0.45)
P.append(box(0.010, 0.008, 0.016, (0.405, 0, 0.058), 'gunmetal_dark', bevel=0.001, seg=1))
# The grip and its guard, the stick mag in front, the handguard.
P += grip(0.325, -0.032, depth=0.080, w=0.030, t=0.038)
P += mag(0.428, -0.040, 0.135, dx=0.034, w=0.024, curve=0)
P.append(box(0.07, 0.044, 0.014, (0.48, 0, -0.025), 'polymer'))
# The stubby barrel, its shroud, the muzzle ring and the flashlight under it.
P.append(cyl(0.008, 0.11, 'x', (0.495, 0, 0), 'steel', verts=20))
P += shroud(0.44, 0.52, 0.040, 0.040, slots=2, slot_len=0.02)
P.append(tube(0.011, 0.0075, 0.024, 'x', (0.538, 0, 0), 'steel', verts=20))
P += flashlight(0.45, 0.54, 0.012, -0.046, mount_z=-0.028)

finish('smg', P, grip=0.312, long=True, note='compact: folding stock bar, stick mag, low display sight')

"""Veil carbine: a long slim clamshell that tapers to a slanted muzzle, a
bone hoop sight sunk in its back, three cyan slits a side, a loop grip.
From Meshy (tools/models/meshy_gen.py carbine)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single alien energy carbine, long and slim. A deep violet clamshell hull, deepest at the rear '
         'and tapering along its whole length to a slanted open muzzle lined with dark iron; a row of dark '
         'iron cooling fins under the barrel; a hoop-shaped sight of bone-white ceramic sunk into its back '
         'above the grip; three short slits glowing bright cyan in a row along its side; under its middle, a '
         'closed loop handle of the same violet shell, to hold it by. It is alien-made, grown more than '
         'built: no screws, no rails and no straight edges. Its hull is deep violet metal with an iridescent '
         "sheen that turns blue at the edges, like a beetle's wing case, in a few overlapping plates with "
         'fine dark seams between them; dark iron ribbing shows in the gaps. 95 centimetres long, 22 '
         'centimetres tall and 7 centimetres wide. Battle-used: fine scratches, heat stains, scuffs and heat '
         'stains on the plates.')
TEXTURE = ('Deep violet iridescent alien metal plates turning blue at the edges, fine dark seams, dark iron '
           'ribbing in the gaps, a bone-white ceramic sight hoop, short slits glowing bright cyan, fine '
           'scratches, heat stains')
ASPECT = '16:9'

scene()
gun, rise = generated('carbine', 0.954, hold=0.34, glow=['#4ffff0'])
finish('carbine', [gun], grip=0.29, rise=rise, long=True, note='Veil carbine: slanted muzzle, hoop sight, loop grip')

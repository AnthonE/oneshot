"""Veil launcher: a fat barrel out of a rounded breech bulb, a bone talon
curving down under the muzzle, a fin sight, slits on the bulb, a loop grip
(the drum the picture was asked for, it left out).
From Meshy (tools/models/meshy_gen.py launcher)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single alien grenade launcher. A fat, short barrel of dark iron growing out of a rounded, deep '
         'violet breech bulb; a drum magazine on the side of the breech; a bone-white talon curving down '
         'under the muzzle; a small fin-shaped sight on top; short slits glowing bright green on the bulb; '
         'under its middle, a closed loop handle of the same violet shell, to hold it by. It is alien-made, '
         'grown more than built: no screws, no rails and no straight edges. Its hull is deep violet metal '
         "with an iridescent sheen that turns blue at the edges, like a beetle's wing case, in a few "
         'overlapping plates with fine dark seams between them; dark iron ribbing shows in the gaps. 90 '
         'centimetres long, 30 centimetres tall and 14 centimetres wide. Battle-used: fine scratches, heat '
         'stains, scuffs and heat stains on the plates.')
TEXTURE = ('Deep violet iridescent alien metal plates turning blue at the edges, fine dark seams, dark iron '
           'ribbing in the gaps, a dark iron barrel, a bone-white talon, short slits glowing bright green, '
           'fine scratches, heat stains')
ASPECT = '16:9'

scene()
gun, rise = generated('launcher', 0.905, hold=0.23, glow=['#30ff70'])
finish('launcher', [gun], grip=0.317, rise=rise, fore=0.56, long=True, note='Veil launcher: breech bulb, fat barrel, bone talon')

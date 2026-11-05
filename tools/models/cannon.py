"""Veil cannon: a thick tube out of a bulb, a ring of core light near the
muzzle and a slit of it down the tube's back, dark bands, a shoulder rest
curling under the rear, a front loop and the main loop.
From Meshy (tools/models/meshy_gen.py cannon)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single alien heavy cannon, carried on the shoulder. A thick tube growing out of a large rounded '
         'bulb at its rear, the bulb clad in overlapping plates; a ring glowing bright green around the tube '
         "near the muzzle and one long slit glowing green down the tube's back; dark iron bands around the "
         'tube; a shoulder rest curling down under the rear; two closed loop handles under it, one at the '
         'middle to hold it by and one further forward. It is alien-made, grown more than built: no screws, '
         'no rails and no straight edges. Its hull is deep violet metal with an iridescent sheen that turns '
         "blue at the edges, like a beetle's wing case, in a few overlapping plates with fine dark seams "
         'between them; dark iron ribbing shows in the gaps. 125 centimetres long, 34 centimetres tall and '
         '16 centimetres wide. Battle-used: fine scratches, heat stains, scuffs and heat stains on the '
         'plates.')
TEXTURE = ('Deep violet iridescent alien metal plates turning blue at the edges, fine dark seams, dark iron '
           'ribbing in the gaps, dark iron bands, a ring and a long slit glowing bright green, fine '
           'scratches, heat stains')
ASPECT = '16:9'

scene()
gun, rise = generated('cannon', 1.25, hold=0.20, glow=['#30ff70'])
finish('cannon', [gun], grip=0.55, rise=rise, fore=0.84, under=0.03, long=True, note='Veil cannon: bulb and tube, a ring and a slit of core light')

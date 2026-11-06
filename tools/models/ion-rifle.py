"""Veil ion rifle: a fat clamshell with two prongs at its mouth (the upper
longer), five dim vents a side, one bright slit down the spine, a loop grip.
From Meshy (tools/models/meshy_gen.py ion-rifle)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single alien ion rifle, short and fat. A deep violet clamshell hull, deep and rounded; at its '
         'mouth two blunt prongs, the upper longer than the lower, with a dark gap between them and an '
         'energy coil glowing cyan at the back of the gap; five small vents glowing dim cyan in a row along '
         'its side; one bright slit glowing cyan down its spine; under its middle, a closed loop handle of '
         'the same violet shell, to hold it by. It is alien-made, grown more than built: no screws, no rails '
         'and no straight edges. Its hull is deep violet metal with an iridescent sheen that turns blue at '
         "the edges, like a beetle's wing case, in a few overlapping plates with fine dark seams between "
         'them; dark iron ribbing shows in the gaps. 62 centimetres long, 26 centimetres tall and 8 '
         'centimetres wide. Battle-used: fine scratches, heat stains, scuffs and heat stains on the plates.')
TEXTURE = ('Deep violet iridescent alien metal plates turning blue at the edges, fine dark seams, dark iron '
           'ribbing in the gaps, a row of small vents glowing dim cyan, one bright slit glowing cyan, fine '
           'scratches, heat stains')
ASPECT = '16:9'

scene()
gun, rise = generated('ion-rifle', 0.616, hold=0.17, glow=['#4ff2ff'], glow_strength=0.6)
finish('ion-rifle', [gun], grip=0.228, rise=rise, long=True, note='Veil ion rifle: fat clamshell, two prongs, five vents a side')

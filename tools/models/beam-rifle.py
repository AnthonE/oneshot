"""Veil beam rifle: a long slim clamshell with a crescent tail curling under
its rear, one long slit of light down the top seam, a forked muzzle, a scope
bump, a loop grip.
From Meshy (tools/models/meshy_gen.py beam-rifle)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single alien long-range beam rifle, very long and slender. A deep violet clamshell hull; at its '
         'rear a crescent-shaped tail curling down and forward under the stock; one long thin slit glowing '
         'bright cyan running along the seam on its top; a low rounded scope bump on its back; a forked '
         'muzzle of two slim dark iron prongs with a gap between them; under its middle, a closed loop '
         'handle of the same violet shell, to hold it by. It is alien-made, grown more than built: no '
         'screws, no rails and no straight edges. Its hull is deep violet metal with an iridescent sheen '
         "that turns blue at the edges, like a beetle's wing case, in a few overlapping plates with fine "
         'dark seams between them; dark iron ribbing shows in the gaps. 130 centimetres long, 24 centimetres '
         'tall and 7 centimetres wide. Battle-used: fine scratches, heat stains, scuffs and heat stains on '
         'the plates.')
TEXTURE = ('Deep violet iridescent alien metal plates turning blue at the edges, fine dark seams, dark iron '
           'ribbing in the gaps, dark iron muzzle prongs, one long thin slit glowing bright cyan, fine '
           'scratches, heat stains')
ASPECT = '16:9'

scene()
gun, rise = generated('beam-rifle', 1.30, hold=0.22, glow=['#4f9cff'], glow_floor=0.65)
finish('beam-rifle', [gun], grip=0.52, rise=rise, long=True, note='Veil beam rifle: crescent tail, forked muzzle, a slit down the spine')

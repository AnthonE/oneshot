"""Veil shards: a bulbous clamshell on a bone keel, a crest of magenta
crystals leaning back along its spine (the tallest at the front), two slits a
side, a loop grip, a short dark muzzle glowing magenta.
From Meshy (tools/models/meshy_gen.py shards)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single alien crystal-shard launcher. A bulbous, deep violet clamshell body riding on a '
         'bone-white keel along its underside; a crest of long glowing magenta crystal spikes standing out '
         'of its back, leaning rearward, the tallest at the front; two short slits glowing magenta on its '
         'side; a short dark iron muzzle glowing magenta inside; under its middle, a closed loop handle of '
         'the same violet shell, to hold it by. It is alien-made, grown more than built: no screws, no rails '
         'and no straight edges. Its hull is deep violet metal with an iridescent sheen that turns blue at '
         "the edges, like a beetle's wing case, in a few overlapping plates with fine dark seams between "
         'them; dark iron ribbing shows in the gaps. 58 centimetres long, 30 centimetres tall and 9 '
         'centimetres wide. Battle-used: fine scratches, heat stains, scuffs and heat stains on the plates.')
TEXTURE = ('Deep violet iridescent alien metal plates turning blue at the edges, fine dark seams, dark iron '
           'ribbing in the gaps, a bone-white keel, glowing magenta crystal spikes, short slits glowing '
           'magenta, a dark iron muzzle, fine scratches, heat stains')
ASPECT = '16:9'

scene()
gun, rise = generated('shards', 0.577, hold=0.14, glow=['#e04fff'], glow_floor=0.6)
finish('shards', [gun], grip=0.335, rise=rise, long=False, note='Veil shards: bone keel, a crest of magenta crystals')

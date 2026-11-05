"""The shotgun: a pump. The tube magazine under the barrel with a ribbed
fore-end on it, a slotted heat shield over the barrel, a solid stock with
a rubber pad, four shells in a side-saddle on the receiver, a small
display. 0.95 m.
From Meshy (tools/models/meshy_gen.py shotgun)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single futuristic military pump-action shotgun. A thick barrel under a slotted steel heat '
         'shield; a tube magazine under the barrel carrying a ribbed black polymer pump fore-end; a boxy '
         'receiver of dark gunmetal with an olive drab panel, a side-saddle on it holding four red shotgun '
         'shells with brass bases, and a small glowing blue display; a solid olive drab stock with a rubber '
         'butt pad; a black polymer pistol grip with a trigger guard. 95 centimetres long, 22 centimetres '
         'tall and 7 centimetres wide. Service-worn: the paint chipped to bare steel on the edges, scuffed, '
         'dusty.')
TEXTURE = ('Dark gunmetal and olive drab painted steel chipped to bare metal on the edges, matte black '
           'polymer pump and grip, a slotted steel heat shield, red shotgun shells with brass bases, a '
           'rubber butt pad, a small glowing blue display, scuffs, dust, grime')
ASPECT = '16:9'

scene()
gun, rise = generated('shotgun', 0.95, hold=0.27, flip=True, glow=['#49b8ff'])
finish('shotgun', [gun], grip=0.28, rise=rise, long=True, note='pump: tube mag, ribbed fore-end, side-saddle of four shells')

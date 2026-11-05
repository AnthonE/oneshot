"""The SMG: compact. A box receiver on a side-folding stock (a bar and a
plate), a stick mag in front of the grip, a stubby shrouded barrel with
the flashlight under it, a low display wedge doubling as the rear sight.
0.55 m.
From Meshy (tools/models/meshy_gen.py smg)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single compact futuristic military submachine gun. A short box receiver of dark gunmetal with an '
         'olive drab panel; a side-folding skeleton stock, unfolded, made of one steel bar and a butt plate; '
         'a long straight stick magazine in front of the pistol grip; a stubby barrel inside a vented shroud '
         'with a small flashlight under it; a low wedge-shaped rear sight housing with a small glowing blue '
         'display; a black polymer pistol grip with a trigger guard. 55 centimetres long, 24 centimetres '
         'tall and 6 centimetres wide. Service-worn: the paint chipped to bare steel on the edges, scuffed, '
         'dusty.')
TEXTURE = ('Dark gunmetal and olive drab painted steel chipped to bare metal on the edges, matte black '
           'polymer grip, a bare steel stock bar, a small glowing blue display, scuffs, dust, grime')
ASPECT = '16:9'

scene()
gun, rise = generated('smg', 0.55, hold=0.43, flip=True, glow=['#49b8ff'])
finish('smg', [gun], grip=0.23, rise=rise, long=True, note='compact: folding stock bar, stick mag, low display sight')

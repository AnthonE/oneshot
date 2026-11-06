"""The battle rifle: longer than the AR, the mag in front of the grip, a
boxed scope on the rail instead of the display (a dim strip on the side
carries the count), a long barrel ending in a muzzle brake. 0.88 m.
From Meshy (tools/models/meshy_gen.py br)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single futuristic military battle rifle of conventional layout. A long slab-sided receiver of '
         'dark gunmetal with olive drab side panels and fine panel lines; a straight box magazine in front '
         'of the pistol grip; a fixed stock with a cheek riser and a rubber butt pad; a squared, boxed '
         'optical sight with a blue-tinted lens, low on a top rail; a long barrel in a slim vented '
         'handguard, ending in a slotted muzzle brake; a small dim blue ammunition counter strip on the side '
         'of the receiver; a black polymer pistol grip with a trigger guard. 88 centimetres long, 24 '
         'centimetres tall and 7 centimetres wide. Service-worn: the paint chipped to bare steel on the '
         'edges, scuffed, dusty.')
TEXTURE = ('Dark gunmetal and olive drab painted steel chipped to bare metal on the edges, matte black '
           'polymer grip, a rubber butt pad, a blue-tinted sight lens, a small dim blue counter strip, '
           'scuffs, dust, grime')
ASPECT = '16:9'

scene()
gun, rise = generated('br', 0.88, hold=0.33, glow=['#49b8ff'])
temper(gun)
finish('br', [gun], grip=0.255, rise=rise, long=True, note='a boxed scope on the rail, the mag in front of the grip, a brake')

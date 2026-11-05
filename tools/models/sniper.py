"""The sniper rifle: a long heavy barrel ending in a big brake, the bipod
folded along it, a large scope in two rings, a skeletal stock with a cheek
rest, a box mag in front of the grip. 1.25 m.
From Meshy (tools/models/meshy_gen.py sniper)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single futuristic military anti-materiel sniper rifle. A very long, heavy, fluted barrel ending '
         'in a large slab-sided muzzle brake; a bipod folded flat along the underside of the barrel; a large '
         'telescopic sight with a blue-tinted objective lens, in two rings on a top rail; a skeletal stock '
         'with an adjustable cheek rest and a rubber butt pad; a box magazine in front of the pistol grip; a '
         'receiver of dark gunmetal with olive drab panels; a black polymer pistol grip with a trigger '
         'guard. 125 centimetres long, 26 centimetres tall and 8 centimetres wide. Service-worn: the paint '
         'chipped to bare steel on the edges, scuffed, dusty.')
TEXTURE = ('Dark gunmetal and olive drab painted steel chipped to bare metal on the edges, matte black '
           'polymer grip and cheek rest, a matte black telescopic sight with a blue-tinted lens, a rubber '
           'butt pad, scuffs, dust, grime')
ASPECT = '16:9'

scene()
gun, rise = generated('sniper', 1.25, hold=0.22)
finish('sniper', [gun], grip=0.3, rise=rise, long=True, note='skeletal stock, big scope in rings, bipod folded under the barrel')

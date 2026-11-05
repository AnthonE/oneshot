"""The assault rifle: a bullpup. One tall, wide chamfered block is receiver
and stock at once, the mag behind the grip, a big display housing over
the grip, a stubby shrouded barrel and the flashlight under it. 0.78 m.
From Meshy (tools/models/meshy_gen.py ar)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single futuristic military bullpup assault rifle. One tall, slab-sided block of dark gunmetal is receiver '
         'and stock at once, chamfered, with olive drab side panels and fine panel lines; the box magazine sits behind '
         'the pistol grip; a raised angular sight housing on top carries a small glowing blue digital display at its '
         'rear; a short thick barrel inside a vented shroud; a small flashlight under the barrel; a black polymer '
         'pistol grip with a trigger guard; a rubber butt pad. 78 centimetres long, 26 centimetres tall and 8 '
         'centimetres wide. Service-worn: the paint chipped to bare steel on the edges, scuffed, dusty.')
TEXTURE = ('Dark gunmetal and olive drab painted steel chipped to bare metal on the edges, matte black polymer grip, '
           'a rubber butt pad, a small glowing blue display, scuffs, dust, grime')
ASPECT = '16:9'

scene()
gun, rise = generated('ar', 0.78, hold=0.34, glow=['#49b8ff'])
finish('ar', [gun], grip=0.35, rise=rise, fore=0.51, long=True, note='bullpup: the mag behind the grip, the display over it')

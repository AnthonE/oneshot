"""The magnum: a big semi-auto pistol. A tall serrated slide over a thick
frame, a polymer grip holding the mag, a short rail under the barrel, a
compensator at the muzzle, the ammo panel on the slide's rear. 0.30 m.
Held by the grip.
From Meshy (tools/models/meshy_gen.py magnum)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single large futuristic military semi-automatic pistol. A tall, heavy slide of dark gunmetal '
         'with rear serrations over a thick frame; a black polymer grip with a textured panel and a magazine '
         'base plate; a short accessory rail under the barrel; a ported compensator at the muzzle; a small '
         'glowing blue ammunition display on the rear of the slide; a trigger guard squared at its front. 30 '
         'centimetres long, 19 centimetres tall and 4 centimetres wide. Service-worn: the paint chipped to '
         'bare steel on the edges, scuffed, dusty.')
TEXTURE = ('Dark gunmetal slide worn to bare steel on the edges, matte black textured polymer grip, a small '
           'glowing blue display, scuffs, grime')
ASPECT = '4:3'

scene()
gun, rise = generated('magnum', 0.305, hold=0.39, glow=['#49b8ff'])
finish('magnum', [gun], grip=0.052, rise=rise, long=False, note='a pistol: held by the grip, the barrel forward of it')

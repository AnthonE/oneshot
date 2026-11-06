"""The rocket launcher: one fat olive tube with gunmetal bands, a bell at
the muzzle, a breech at the rear over a shoulder rest, a rail under it
carrying the pistol grip and a foregrip, the display box on its side, a
carry handle on top. 1.05 m, shouldered.
From Meshy (tools/models/meshy_gen.py rocket-launcher)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single futuristic military shoulder-fired rocket launcher. One fat olive drab launch tube 14 '
         'centimetres across, with dark gunmetal reinforcing bands, a flared bell at the muzzle and a vented '
         'breech at the rear above a padded shoulder rest; a rail under the tube carrying a black pistol '
         'grip with a trigger guard and, ahead of it, a vertical foregrip; a boxy sight unit on its side '
         'with a small glowing blue display; a carry handle on top. 105 centimetres long, 32 centimetres '
         'tall and 16 centimetres wide. Service-worn: the paint chipped to bare steel on the edges, scuffed, '
         'dusty. Scorched black at the breech.')
TEXTURE = ('Chipped olive drab paint over steel, dark gunmetal bands, scorched black soot at the breech, '
           'matte black polymer grips and shoulder pad, a small glowing blue display, scuffs, dust, grime')
ASPECT = '16:9'

scene()
gun, rise = generated('rocket-launcher', 1.054, hold=0.22, glow=['#49b8ff'])
temper(gun)
finish('rocket-launcher', [gun], grip=0.455, rise=rise, fore=0.727, under=0.03, long=True, note='one fat tube, shouldered; the display box on its side')

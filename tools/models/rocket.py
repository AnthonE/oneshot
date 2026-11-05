"""The rocket: a cylinder with a conical nose, a flared nozzle and four
swept fins at the tail, an amber band under the nose. 0.60 m. Not held:
the grip at its middle.
From Meshy (tools/models/meshy_gen.py rocket)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single rocket for a shoulder-fired launcher. A cylindrical body of olive drab painted steel, 10 '
         'centimetres across, with a dark conical nose, an amber band just behind the nose, a flared black '
         'nozzle at the tail and four swept steel fins around it. 60 centimetres long. Scuffed, the paint '
         'chipped.')
TEXTURE = ('Chipped olive drab paint over steel, a dark steel nose cone, an amber band, a black nozzle, bare '
           'steel fins, scuffs')
ASPECT = '16:9'
FRONT = 'the nose'

scene()
gun, rise = generated('rocket', 0.6, tris=4000)
finish('rocket', [gun], grip=0.3, rise=rise, long=True, note='the projectile, not held')

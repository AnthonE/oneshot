"""The ten-gallon: a weathered cowboy hat. Headwear (`proto::HEADWEAR`).
From Meshy (tools/models/meshy_gen.py ten-gallon --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = 'A single weathered cowboy hat: dark tan felt, a tall pinched crown with a crease down the top, a wide brim curled up at both sides, a plain dark leather band round the crown. Sweat-stained and dusty.'
TEXTURE = 'Dark tan felt, sweat-stained and dusty, a plain dark leather band'
VIEW = (' Alone on a plain white background, seen from the front and a little to one side, three-quarter on, at eye level. '
        'A real object, photographed in soft even daylight. Exactly one object. No head, no mannequin, no person, no stand, '
        'no ground, no shadow, no text, no labels, no logos.')
WIDTH = 0.62

head_piece('ten-gallon', WIDTH, deep=0.8, tall=0.9, bottom=1.72, middle=-0.047)

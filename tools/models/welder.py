"""The welder: a welding hood with a dark window. Headwear (`proto::HEADWEAR`).
From Meshy (tools/models/meshy_gen.py welder --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = 'A single welding helmet, seen without anyone in it: a smooth faceted hard shell of matte charcoal grey fibreglass with a sharp ridge down the middle, a flat rectangular dark green glass window at eye height in a steel frame, burn marks and scorched paint round the window. Cut off flat at the top of the forehead, only the front face shield, no head harness.'
TEXTURE = 'Matte charcoal fibreglass scorched and scratched, dark green glass, a dull steel frame'
VIEW = (' Alone on a plain white background, seen from the front and a little to one side, three-quarter on, at eye level. '
        'A real object, photographed in soft even daylight. Exactly one object. No head, no mannequin, no person, no stand, '
        'no ground, no shadow, no text, no labels, no logos.')
WIDTH = 0.385

head_piece('welder', WIDTH, top=1.655, front=0.335)

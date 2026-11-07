"""The beanie: a short knit watch cap. Headwear (`proto::HEADWEAR`).
From Meshy (tools/models/meshy_gen.py beanie --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = "A single short knit watch cap, a fisherman's beanie: chunky ribbed wool in charcoal grey with a thick folded-up cuff, shallow and round, standing on its own as if on a head. Plain, nothing on it."
TEXTURE = 'Chunky ribbed charcoal grey wool, a thick folded cuff, plain'
VIEW = (' Alone on a plain white background, seen from the front and a little to one side, three-quarter on, at eye level. '
        'A real object, photographed in soft even daylight. Exactly one object. No head, no mannequin, no person, no stand, '
        'no ground, no shadow, no text, no labels, no logos.')
WIDTH = 0.385

# The label Meshy lettered under the cuff.
head_piece('beanie', WIDTH, tall=0.55, bottom=1.752, middle=-0.047,
           paint=lambda o: blotted(o, (-0.2, -0.25, 1.7), (0.2, 0.3, 1.8), 0.02, facing=(0, 0, -1)))

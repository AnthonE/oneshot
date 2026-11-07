"""The spurs: tooled cowboy boots with steel spurs. Footwear (`proto::FOOTWEAR`).
From Meshy (tools/models/meshy_gen.py spurs --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = 'A single tall cowboy boot: tan leather with dark stitched scrollwork up a high shaft cut in a scallop, a rounded toe, an angled stacked wooden heel, a steel spur strapped round the heel with a star rowel. Broad in the foot. Dusty and worn.'
TEXTURE = 'Tan leather with dark stitched scrollwork, a stacked wooden heel, a dull steel spur, dust'
VIEW = (' Alone on a plain white background, seen from the outer side, a little from the front and a little above, the toe pointing '
        'to the lower right. A real object, photographed in soft even daylight. Exactly one boot, a right boot, not a pair. No leg, no '
        'foot, no person, no stand, no ground, no shadow, no text, no labels, no logos.')
LENGTH = 0.44
WIDE = 0.22

boots_piece('spurs', LENGTH, yaw=90, wide=WIDE, high=0.42)

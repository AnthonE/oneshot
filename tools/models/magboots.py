"""The mag boots: armoured magnetic boots with a lit seam. Footwear (`proto::FOOTWEAR`).
From Meshy (tools/models/meshy_gen.py magboots --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = 'A single futuristic armoured magnetic spacewalk boot: overlapping plates of worn gunmetal grey over dark rubber joints, a thick flat segmented sole with a heavy magnetic plate, a thin glowing cyan light seam along the side of the sole, ankle plates with a clasp. Broad and blocky. Scuffed and chipped.'
TEXTURE = 'Worn gunmetal plates chipped to steel, dark rubber joints, a thin glowing cyan seam'
VIEW = (' Alone on a plain white background, seen from the outer side, a little from the front and a little above, the toe pointing '
        'to the lower right. A real object, photographed in soft even daylight. Exactly one boot, a right boot, not a pair. No leg, no '
        'foot, no person, no stand, no ground, no shadow, no text, no labels, no logos.')
LENGTH = 0.44
WIDE = 0.22

boots_piece('magboots', LENGTH, yaw=90, wide=WIDE)

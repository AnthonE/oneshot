"""The wellies: yellow rubber rain boots. Footwear (`proto::FOOTWEAR`).
From Meshy (tools/models/meshy_gen.py wellies --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = 'A single tall rubber rain boot: glossy bright yellow rubber, a wide round toe, a tall plain shaft with a thick rolled rim, a black ridged sole and a low heel. Broad and simple. Mud splashed up the lower half.'
TEXTURE = 'Glossy yellow rubber, mud splashed up the lower half, a black ridged sole'
VIEW = (' Alone on a plain white background, seen from the outer side, a little from the front and a little above, the toe pointing '
        'to the lower right. A real object, photographed in soft even daylight. Exactly one boot, a right boot, not a pair. No leg, no '
        'foot, no person, no stand, no ground, no shadow, no text, no labels, no logos.')
LENGTH = 0.44
WIDE = 0.22

boots_piece('wellies', LENGTH, yaw=90, wide=WIDE, high=0.42)

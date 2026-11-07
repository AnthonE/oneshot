"""The loggers: oiled-brown logger boots with a heavy lugged sole. Footwear (`proto::FOOTWEAR`).
From Meshy (tools/models/meshy_gen.py loggers --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = "A single tall logger's work boot: oiled dark-brown full-grain leather, ankle-high lacing with brass hooks and tan rawhide laces, a stitched round steel-capped toe, a heavy black lugged rubber sole with a raised heel. Broad and chunky. Worn, scuffed at the toe, sawdust in the welt."
TEXTURE = 'Oiled dark-brown leather scuffed at the toe, brass hooks, tan rawhide laces, black lugged rubber sole'
VIEW = (' Alone on a plain white background, seen from the outer side, a little from the front and a little above, the toe pointing '
        'to the lower right. A real object, photographed in soft even daylight. Exactly one boot, a right boot, not a pair. No leg, no '
        'foot, no person, no stand, no ground, no shadow, no text, no labels, no logos.')
LENGTH = 0.44
WIDE = 0.22

boots_piece('loggers', LENGTH, yaw=90, wide=WIDE)

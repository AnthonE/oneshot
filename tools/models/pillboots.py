"""The pill boots: huge goofy cartoon boots in a capsule's two colours, mint green
at the toe and white at the heel. Footwear (`proto::FOOTWEAR`).
From Meshy (tools/models/meshy_gen.py pillboots --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = ('A single huge goofy cartoon boot, comically oversized and bulbous, like a boot drawn in an animated cartoon: a giant '
         'smooth rounded toe much wider than the ankle, a thick smooth seamless rubber body, a short wide shaft that ends below the '
         'calf, no laces, no stitching, no buckles. It is coloured like a medicine capsule: the whole front half, from the tip of '
         'the toe back to the middle of the foot, is glossy mint green; the back half and the shaft are glossy off-white; one thin '
         'black band runs round the boot where the two colours meet. A thick flat black sole. Shiny and smooth like a pill.')
TEXTURE = 'Glossy mint green on the toe and the front half, glossy off-white on the heel, the back half and the shaft, one thin black band where they meet, a thick black sole'
VIEW = (' Alone on a plain white background, seen from the outer side, a little from the front and a little above, the toe pointing '
        'to the lower right. A real object, photographed in soft even daylight. Exactly one boot, a right boot, not a pair. No leg, no '
        'foot, no person, no stand, no ground, no shadow, no text, no labels, no logos.')
# Bigger than the other boots on purpose: the joke is the size. Still drawing
# only, no hitbox (DESIGN.md §15).
LENGTH = 0.56
WIDE = 0.30

boots_piece('pillboots', LENGTH, yaw=90, wide=WIDE, high=0.34)

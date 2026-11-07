"""The high-tops: chunky white and teal basketball sneakers. Footwear (`proto::FOOTWEAR`).
From Meshy (tools/models/meshy_gen.py hightops --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = 'A single chunky retro high-top basketball sneaker: off-white leather panels with teal suede overlays, a padded collar, thick white flat laces, a wide rubber cupsole in cream. Broad and rounded at the toe. Plain panels, no stripes, no marks of any maker. A little worn.'
TEXTURE = 'Off-white leather on the toe and the side panels, solid teal suede only on the heel counter and the collar, a cream rubber cupsole, white laces, a little worn; the side panels are completely plain, with no stripe, no curve, no shape on them'
VIEW = (' Alone on a plain white background, seen from the outer side, a little from the front and a little above, the toe pointing '
        'to the lower right. A real object, photographed in soft even daylight. Exactly one boot, a right boot, not a pair. No leg, no '
        'foot, no person, no stand, no ground, no shadow, no text, no labels, no logos.')
LENGTH = 0.44
WIDE = 0.22

# The label Meshy lettered on the tongue, and the flecks of its old paint down it.
def tongue(o):
    blotted(o, (-0.06, -0.075, 0.185), (0.06, 0.06, 0.27), 0.02)
    blotted(o, (-0.06, -0.22, 0.04), (0.06, -0.02, 0.2), 0.3)


boots_piece('hightops', LENGTH, yaw=90, wide=WIDE, coat='plain', paint=tongue)

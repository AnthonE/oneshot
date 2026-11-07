"""The skull: a ram's skull worn as a mask. Headwear (`proto::HEADWEAR`).
From Meshy (tools/models/meshy_gen.py skull --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = "A single old bleached ram's skull worn as a mask, no lower jaw: dry ivory bone, dark hollow eye sockets, a long narrow snout, and two thick ridged horns curling back and down tightly beside the skull. Cracked and weathered, dirt in the sutures."
TEXTURE = 'Dry ivory bone cracked and weathered, brown dirt in the sutures, dark ridged horn'
VIEW = (' Alone on a plain white background, seen from the front and a little to one side, three-quarter on, at eye level. '
        'A real object, photographed in soft even daylight. Exactly one object. No head, no mannequin, no person, no stand, '
        'no ground, no shadow, no text, no labels, no logos.')
WIDTH = 0.82

head_piece('skull', WIDTH, deep=0.72, top=1.668, front=0.385, shape=lambda o: drawn_in(o, 0.15, 0.55))

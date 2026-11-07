"""The respirator: a half-face rubber gas mask with two filters. Headwear (`proto::HEADWEAR`).
From Meshy (tools/models/meshy_gen.py respirator --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = "A single half-face respirator mask, the kind worn over the nose and mouth: moulded matte black rubber, two round olive-green filter canisters at the cheeks angled forward, a small round exhale valve at the chin, two short pieces of black webbing strap at the sides. 22 centimetres across. Worn, dusty, the filters' paint scuffed to metal."
TEXTURE = 'Matte black moulded rubber, olive-green painted filter canisters scuffed to steel, dust in the creases'
VIEW = (' Alone on a plain white background, seen from the front and a little to one side, three-quarter on, at eye level. '
        'A real object, photographed in soft even daylight. Exactly one object. No head, no mannequin, no person, no stand, '
        'no ground, no shadow, no text, no labels, no logos.')
WIDTH = 0.37

head_piece('respirator', WIDTH, top=1.515, front=0.31)

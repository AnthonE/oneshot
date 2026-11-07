"""The demon: a lacquered demon mask with two short horns. Headwear (`proto::HEADWEAR`).
From Meshy (tools/models/meshy_gen.py demon --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = 'A single carved wooden full-face demon mask: deep red lacquer over a fierce furrowed brow, wide snarling mouth with bared white fangs, a broad flat nose, gilded eyes, two short curved ivory horns at the temples. Hollow, made to be worn. The lacquer chipped on the edges down to dark wood.'
TEXTURE = 'Deep red lacquer chipped to dark wood on the edges, gilded eyes, ivory horns and fangs, black in the creases'
VIEW = (' Alone on a plain white background, seen from the front and a little to one side, three-quarter on, at eye level. '
        'A real object, photographed in soft even daylight. Exactly one object. No head, no mannequin, no person, no stand, '
        'no ground, no shadow, no text, no labels, no logos.')
WIDTH = 0.31

head_piece('demon', WIDTH, top=1.72, front=0.285, paint=lambda o: hue_turned(o, 0.0, 150.0, sat=0.85, val=0.95))

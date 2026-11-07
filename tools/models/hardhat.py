"""The hard hat: a scuffed yellow hard hat, on a log. Headwear (`proto::HEADWEAR`).
From Meshy (tools/models/meshy_gen.py hardhat --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = 'A single construction hard hat: a rounded dome of glossy safety-yellow plastic with a raised ridge from front to back and a short peak over the brow. Scuffed and scratched, flecks of paint and dirt. Plain, nothing printed on it.'
TEXTURE = 'Safety-yellow plastic, scuffed and scratched, flecks of dirt, nothing printed on it'
VIEW = (' Alone on a plain white background, seen from the front and a little to one side, three-quarter on, at eye level. '
        'A real object, photographed in soft even daylight. Exactly one object. No head, no mannequin, no person, no stand, '
        'no ground, no shadow, no text, no labels, no logos.')
WIDTH = 0.4

head_piece('hardhat', WIDTH, tall=0.8, bottom=1.73, back=-0.26)

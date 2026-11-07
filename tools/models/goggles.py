"""The goggles: brass flying goggles on a leather strap. Headwear (`proto::HEADWEAR`).
From Meshy (tools/models/meshy_gen.py goggles --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = "A single pair of old aviator's flying goggles: two round brass eyecups with dark smoked glass lenses, joined by a short brass bridge over the nose, worn brown leather padding round each eyecup, a wide brown leather head strap with a buckle. Scratched, the brass tarnished."
TEXTURE = 'Tarnished scratched brass eyecups, dark smoked glass, worn brown leather'
VIEW = (' Alone on a plain white background, seen from the front and a little to one side, three-quarter on, at eye level. '
        'A real object, photographed in soft even daylight. Exactly one object. No head, no mannequin, no person, no stand, '
        'no ground, no shadow, no text, no labels, no logos.')
WIDTH = 0.4

head_piece('goggles', WIDTH, bottom=1.474, front=0.215, inside=False, wrap=lambda o: strapped(o, 0.1))

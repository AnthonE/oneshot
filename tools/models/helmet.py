"""The helmet: a combat helmet with its night vision folded up. Headwear (`proto::HEADWEAR`).
From Meshy (tools/models/meshy_gen.py helmet --tris 8000)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gear import *

IMAGE = 'A single modern military ballistic helmet: a smooth low dome of matte olive drab with a cut high over the ears, short side rails, a steel mount on the front brow with a compact twin-tube night vision device folded up against the front of the dome. No visor, no ear cups, no straps hanging. Scuffed, the paint worn on the rim.'
TEXTURE = 'Matte olive drab paint worn on the rim, black anodised rails and night vision device, dark glass'
VIEW = (' Alone on a plain white background, seen from the front and a little to one side, three-quarter on, at eye level. '
        'A real object, photographed in soft even daylight. Exactly one object. No head, no mannequin, no person, no stand, '
        'no ground, no shadow, no text, no labels, no logos.')
WIDTH = 0.4

head_piece('helmet', WIDTH, yaw=180, bottom=1.752, back=-0.27)

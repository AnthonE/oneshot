"""Veil sword: an iron hilt in shell plates with a knuckle bow, and from its
front two curved blades of cyan light diverging up and down like a tuning
fork. +X is the blade's direction.
From Meshy (tools/models/meshy_gen.py sword)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single alien energy sword. A short hilt of dark iron clad in glossy pearlescent violet shell '
         'plates, with a curved knuckle guard under it; from the front of the hilt two long, flat, curved '
         'blades of solid glowing pale cyan energy sweep forward, one arcing up and one arcing down, with a '
         'gap between them, like the two tines of a tuning fork, each ending in a point. It is alien-made: '
         'smooth and organic, with no screws, no rails and no straight edges. 100 centimetres long, 34 '
         'centimetres tall and 4 centimetres wide.')
TEXTURE = ('A dark iron hilt clad in glossy pearlescent violet shell plates, two blades of glowing pale cyan '
           'energy, bright at their edges')
ASPECT = '16:9'
FRONT = 'the points of the blades'

scene()
gun, rise = generated('sword', 1.01, hold=0.50, glow=['#4ff2ff'], glow_strength=2.0, glow_floor=0.6, pale=True)
finish('sword', [gun], grip=0.25, rise=rise, long=False, note='Veil sword: iron hilt in shell plates, two blades of light')

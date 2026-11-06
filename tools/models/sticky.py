"""The sticky: an ion core held in a dark cage of three rings, with a grip pad at
each crossing for what it lands on. 0.13 m. Not held: the grip at its middle.
Ours, made on lib.py's kit: no mesh commissioned."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

MID = 0.065
CORE = 0.038
CAGE = 0.056
RING = 0.006
PAD = 0.011
AT = (MID, 0, 0)

scene()
parts = [sphere(CORE, at=AT, material='ion')]
parts += [torus(CAGE, RING, axis=a, at=AT, material='iron') for a in 'xyz']
parts += [
    sphere(PAD, at=(MID + dx, dy, dz), material='gunmetal_dark')
    for dx, dy, dz in ((CAGE, 0, 0), (-CAGE, 0, 0), (0, CAGE, 0), (0, -CAGE, 0), (0, 0, CAGE), (0, 0, -CAGE))
]
finish('sticky', parts, grip=MID, long=False, note='an ion core in a cage, not held')

"""Veil ion pistol: a rounded body with its glowing core showing through the
top seam, a three-fingered bone claw at the muzzle, a swept integral grip.
From Meshy (tools/models/meshy_gen.py ion-pistol)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single alien ion pistol. A rounded, bulbous, deep violet clamshell body with a seam along its '
         'top through which a glowing green energy core shows; three curved bone-white claws around its '
         'muzzle, like fingers closing; a swept-back grip growing out of the body in one piece with it. It '
         'is alien-made, grown more than built: no screws, no rails and no straight edges. Its hull is deep '
         "violet metal with an iridescent sheen that turns blue at the edges, like a beetle's wing case, in "
         'a few overlapping plates with fine dark seams between them; dark iron ribbing shows in the gaps. '
         '37 centimetres long, 20 centimetres tall and 7 centimetres wide. Battle-used: fine scratches, heat '
         'stains, scuffs and heat stains on the plates.')
TEXTURE = ('Deep violet iridescent alien metal plates turning blue at the edges, fine dark seams, dark iron '
           'ribbing in the gaps, bone-white ceramic claws, a seam glowing bright green, fine scratches, heat '
           'stains')
ASPECT = '4:3'

scene()
gun, rise = generated('ion-pistol', 0.366, hold=0.30, flip=True, glow=['#30ff70'])
finish('ion-pistol', [gun], grip=0.045, rise=rise, long=False, note='Veil ion pistol: core in the seam, bone claw, swept grip')

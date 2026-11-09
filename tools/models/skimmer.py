"""The skimmer (look/craft.rs, `sim::vehicle::SKIMMER`): a one-seat hover
bike of the Veil's, our own design: a catamaran, two blade hulls with a gun
in each prow and a thruster in each stern, a saddle on the bridge between
them, open ahead of it like a tuning fork. From Meshy
(tools/models/meshy_gen.py skimmer), cut by craft.py's kit.

    tools/craft.sh skimmer        the loop: this, then the game's views of it
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from craft import *

IMAGE = ('A single alien one-seat hover bike, a machine that floats low over the ground, as long as a small car. '
         'It is a catamaran: two long slender blade-like pontoon hulls side by side run its whole length, each ending '
         'forward in a sharp downturned prow with one short gun barrel set in its tip, and aft in a round thruster '
         'nozzle glowing cyan. Behind the middle a low bridge joins the two hulls and carries a dark padded saddle, '
         "like a motorcycle's, with two handgrips on short stalks ahead of it and a low swept windbreak cowl ahead of "
         'the grips; ahead of the cowl the gap between the two hulls is open, so that from above it is a tuning fork. '
         'A small upright fin at the tail. A long strip of cyan light runs under each hull. '
         "Its hull is deep violet metal with an iridescent sheen that turns blue at the edges, like a beetle's wing "
         'case, in a few large overlapping plates with fine dark seams between them; dark iron ribbing and machinery '
         'show in the gaps. Alien-made, grown more than built: no screws, no rivets, no straight box sections. '
         'Battle-used: fine scratches, scuffs and heat stains on the plates. '
         '3 metres long, 1.7 metres wide and 1.1 metres tall.')
VIEW = (' Alone on a plain white background, seen from the front left and from well above, three-quarter on, its nose '
        'pointing to the lower left, the whole craft in the frame with room around it. A real machine, photographed in '
        'soft even daylight. Exactly one craft. No pilot, no rider, no person, no wheels, no landing gear, no stand, no '
        'ground, no shadow, no exhaust flame, no text, no labels, no logos, no scale bar.')
TEXTURE = ('Deep violet iridescent alien metal plates turning blue at the edges, fine dark seams, dark iron ribbing and '
           'machinery in the gaps, strips of cyan light, round thruster nozzles ringed in cyan light, a dark padded '
           'saddle, dark steel gun barrels, fine scratches, scuffs, heat stains')
ASPECT = '16:9'

# Nose to tail, metres: the sim's hull (2 × SKIM_HALF_L).
LENGTH = 2 * sim('SKIM_HALF_L')
# The mesh as it came: degrees about up that bring its nose to +X, and the
# half kept and mirrored over the other (None: as it came).
TURN = 180
MIRROR = None
# Its paint: the least roughness it keeps, its metal's scale (a craft is
# met from outside, under the sky, where a gun's matte reads as plastic),
# and how hard its lights burn.
ROUGH = 0.3
METAL = 0.7
GLOW = 1.8
# How far its keels ride over the ground, metres: a hover's gap.
BELLY = 0.22
# Where the sim seats its rider (`Vehicle::mount`: metres ahead of the
# hull's middle, and the feet over the ground) and where its bolts leave
# (`Vehicle::gun_at`: ahead, to each side, over the ground).
SEAT = (-0.35, 0.5)
GUNS = (sim('SKIM_HALF_L') - 0.1, 0.42, 0.55)
# `slide`: from the fin's root, the saddle's middle as it came, to the
# prows' necks, and how far forward.
SADDLE = (-1.08, -0.75, 1.2, 0.4)

scene()
o = hull('skimmer', LENGTH, turn=TURN, width=2 * sim('SKIM_HALF_W'), belly=BELLY, mirror=MIRROR, pale=True,
         rough=ROUGH, metal=METAL, glow_strength=GLOW)
# The saddle came 0.4 m behind where the sim seats its rider: it and the
# cowl go forward to it, the thrusters and the prows staying where they are.
slide(o, *SADDLE)
hl, hw, r = sim('SKIM_HALF_L'), sim('SKIM_HALF_W'), sim('BODY_R')
eye = Vector((SEAT[0], 0, SEAT[1] + sim('CROUCH_EYE')))
deliver(
    'skimmer', o,
    steel=[((-hl, -hw, 0), (hl, hw, sim('SKIM_SILL')))],
    rider=[((SEAT[0] - r, -r, SEAT[1]), (SEAT[0] + r, r, SEAT[1] + sim('CROUCH_H')))],
    marks=[(GUNS[0], side * GUNS[1], GUNS[2]) for side in (1, -1)],
    chase=(eye + Vector((-cam('SKIM_CAM_BACK_M'), 0, cam('SKIM_CAM_UP_M'))), eye + Vector((cam('CRAFT_CAM_MEET_M'), 0, 0))),
    behind=-0.5 * hl,
    note='Veil skimmer: a catamaran of a hover bike, a gun in each prow, a thruster in each stern')

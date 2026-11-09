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
# Aft of this is its sterns (the thrusters' rings); a hull's spine, this far
# out from the centre line; the seam of light along it, half its width,
# from this far aft; ahead of here it runs on down the prow's face.
STERN = -0.5 * sim('SKIM_HALF_L')
SPINE = 0.52
# The bars as they came: the box their loose pieces lie in (hull space),
# and how far they are moved to come to the rider's hands; the stalk that
# carries them there, and how far the crossbar reaches each way.
# `craft.rs`'s grips are the grips' middles after it.
BARS = ((-0.03, -0.4, 0.78), (0.17, 0.4, 0.89))
BARS_BY = (-0.235, 0.0, 0.4)
# Its foot, the point it bows toward, and its top; how thick at each end.
COLUMN = ((0.17, 0.0, 0.8), (0.05, 0.0, 1.12), (-0.15, 0.0, 1.22))
COLUMN_R = (0.05, 0.022)
CROSS = 0.13
# The brake levers Meshy gave it, a bicycle's: the box they lie in.
LEVERS = ((0.07, -0.37, 0.822), (0.12, 0.37, 0.848))
SEAM = 0.011
SPINE_FROM = -1.0
PROW = 1.0

scene()
o = hull('skimmer', LENGTH, turn=TURN, width=2 * sim('SKIM_HALF_W'), belly=BELLY, mirror=MIRROR, pale=True,
         rough=ROUGH, metal=METAL, glow_strength=GLOW)
# The saddle came 0.4 m behind where the sim seats its rider: it and the
# cowl go forward to it, the thrusters and the prows staying where they are.
slide(o, *SADDLE)

# The bars: Meshy's sit on the cowl, a forearm lower and further off than
# the rider's hands can come (he sits upright in the sim's seat, and his
# arms are short). They go up and back to his hands, as one piece.
drop(o, pieces(o, *LEVERS))
bars = pieces(o, *BARS)
shift(o, bars, BARS_BY)

# Its lights and its iron, by where each texel lies (hull space, metres).
tx = texels(o)
pos, nor, on = tx
x, across, up = pos[..., 0], np.abs(pos[..., 1]), nor[..., 2]
# Its three thrusters as Meshy painted them are steel cans, cold inside.
thrusters(o, tx, rings(o, STERN), can=1.5, long=0.2)
# A seam of light down each hull's spine and over its prow to the gun: what
# the chase camera sees of it, and what tells its nose from its tail.
light(o, on * band(across, SPINE - SEAM, SPINE + SEAM, SEAM * 0.5) * band(x, SPINE_FROM, 9.0, 0.05)
      * (up > np.where(x > PROW, -0.5, 0.4)))
# The bars stand on a stalk grown from the cowl, bowed back to the rider and
# thinning as it goes, a crossbar between them.
name = o.name
o = join([o] + stalk(*COLUMN, *COLUMN_R)
         + [rod((COLUMN[2][0], -CROSS, COLUMN[2][2]), (COLUMN[2][0], CROSS, COLUMN[2][2]), COLUMN_R[1] * 0.8)], name)

hl, hw, r = sim('SKIM_HALF_L'), sim('SKIM_HALF_W'), sim('BODY_R')
eye = Vector((SEAT[0], 0, SEAT[1] + sim('CROUCH_EYE')))
deliver(
    'skimmer', o,
    steel=[((-hl, -hw, 0), (hl, hw, sim('SKIM_SILL')))],
    rider=[((SEAT[0] - r, -r, SEAT[1]), (SEAT[0] + r, r, SEAT[1] + sim('CROUCH_H')))],
    marks=[(GUNS[0], side * GUNS[1], GUNS[2]) for side in (1, -1)],
    chase=(eye + Vector((-cam('SKIM_CAM_BACK_M'), 0, cam('SKIM_CAM_UP_M'))), eye + Vector((cam('CRAFT_CAM_MEET_M'), 0, 0))),
    behind=STERN,
    note='Veil skimmer: a catamaran of a hover bike, a gun in each prow, a thruster in each stern')

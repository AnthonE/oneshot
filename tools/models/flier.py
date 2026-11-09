"""The flier (look/craft.rs, `sim::vehicle::FLIER`): a one-seat attack
craft of the Veil's, our own design: a wedge of a lifting body, flat under
and rounded over, its canopy flush in its nose, short broad wings swept
back to upturned tips, a gun pod under each, two canted fins and two
engines. From Meshy (tools/models/meshy_gen.py flier), cut by craft.py's
kit.

    tools/craft.sh flier          the loop: this, then the game's views of it
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from craft import *
from craft import _image

IMAGE = ('A single alien one-seat attack aircraft, small, about the size of a car. A wedge-shaped lifting body, flat '
         'underneath and rounded on top, with a sharp chisel nose and a low flush canopy of dark smoked glass over its '
         'front half, blending smoothly into two short broad swept-back wings whose tips turn up into small winglets. '
         'Under the middle of each wing a slim gun pod with one barrel pointing forward, reaching just ahead of the '
         'wing. Two tall fins canted outward over the tail. Two round engine nozzles side by side at the tail glowing '
         'cyan, and a thin strip of cyan light along the trailing edge of each wing. '
         "Its hull is deep violet metal with an iridescent sheen that turns blue at the edges, like a beetle's wing "
         'case, in a few large overlapping plates with fine dark seams between them; dark iron ribbing and machinery '
         'show in the gaps. Alien-made, grown more than built: no screws, no rivets, no straight box sections. '
         'Battle-used: fine scratches, scuffs and heat stains on the plates. '
         '4.6 metres long, 5.2 metres across the wings and 1.5 metres tall.')
VIEW = (' Alone on a plain white background, seen from the front left and from well above, three-quarter on, its nose '
        'pointing to the lower left, the whole craft in the frame with room around it. A real machine, photographed in '
        'soft even daylight. Exactly one craft. No pilot, no rider, no person, no wheels, no landing gear, no stand, no '
        'ground, no shadow, no exhaust flame, no text, no labels, no logos, no scale bar.')
TEXTURE = ('Deep violet iridescent alien metal plates turning blue at the edges, fine dark seams, dark iron ribbing and '
           'machinery in the gaps, thin strips of cyan light, round engine nozzles glowing cyan inside, a canopy of '
           'dark smoked glass, dark steel gun barrels, fine scratches, scuffs, heat stains')
ASPECT = '16:9'

# Nose to tail, metres: the sim's hull (2 × FLY_HALF_L).
LENGTH = 2 * sim('FLY_HALF_L')
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
# How far its belly hangs over the ground at rest, metres (its wings then
# lie in the sim's slab of them), and how tall it stands from there: the
# picture's was a tenth flatter than the sim's fuselage.
BELLY = 0.2
HEIGHT = sim('FLY_H') - BELLY
# Where the sim seats its pilot (`Vehicle::mount`: metres ahead of the
# hull's middle, and the feet over the ground) and where its bolts leave
# (`Vehicle::gun_at`: ahead, to each side, over the wing's underside).
SEAT = (0.4, 0.15)
GUNS = (1.1, 1.9, sim('FLY_WING_Y') + 0.15)
# Aft of this is its tail (the engines' rings); the wings begin this far out
# from the centre line; the belly's paint; the keel's line of light, half
# its width, and from where to where; and the size of the cells the top's
# lights are found under by.
STERN = -0.5 * sim('FLY_HALF_L')
WING_FROM = 0.95
BELLY_PAINT = '#4b2a8f'
KEEL = 0.02
KEEL_FROM = -1.5
KEEL_TO = 0.9
CELL = 0.03
hull_l, hull_w = sim('FLY_HALF_L'), sim('FLY_HALF_W')

scene()
o = hull('flier', LENGTH, turn=TURN, width=2 * sim('FLY_HALF_W'), height=HEIGHT, belly=BELLY, mirror=MIRROR, pale=True,
         rough=ROUGH, metal=METAL, glow_strength=GLOW)

# Its lights and its iron, by where each texel lies (hull space, metres).
tx = texels(o)
pos, nor, on = tx
x, y, across, up = pos[..., 0], pos[..., 1], np.abs(pos[..., 1]), nor[..., 2]
under = on * np.clip((-up - 0.2) / 0.3, 0.0, 1.0)
# The picture never saw its belly, and it came back a muddy plum: the
# top's violet, so it is one craft from the ground.
paint(o, under, BELLY_PAINT, mix=0.6)
# Its three engines as Meshy painted them are steel cans, cold inside.
thrusters(o, tx, rings(o, STERN))
# The light along each wing's trailing edge is on its top alone; the same
# line under it, where it is seen from the ground, and a keel line aft.
glow = _image(o, 'Emission Color')
lit = np.empty(glow.size[0] * glow.size[1] * 4, dtype=np.float32)
glow.pixels.foreach_get(lit)
lit = lit.reshape(glow.size[1], glow.size[0], 4)[..., :3].max(axis=-1)
top = on & (lit > 0.3) & (up > 0.2) & (across > WING_FROM)
cells = np.zeros((int(2 * hull_l / CELL) + 2, int(2 * hull_w / CELL) + 2), dtype=bool)
at = lambda a, half: np.clip(((a + half) / CELL).astype(int), 0, None)
cells[at(x[top], hull_l), at(y[top], hull_w)] = True
# A cell either way: the under side's edge is not quite over the top's.
cells |= np.roll(cells, 1, 0) | np.roll(cells, -1, 0) | np.roll(cells, 1, 1) | np.roll(cells, -1, 1)
light(o, under * (across > WING_FROM) * cells[np.minimum(at(x, hull_l), cells.shape[0] - 1), np.minimum(at(y, hull_w), cells.shape[1] - 1)])
light(o, under * band(across, 0.0, KEEL, KEEL * 0.5) * band(x, KEEL_FROM, KEEL_TO, 0.05))

hl, hw, bw, r = sim('FLY_HALF_L'), sim('FLY_HALF_W'), sim('FLY_BODY_HALF_W'), sim('BODY_R')
wy, wt = sim('FLY_WING_Y'), sim('FLY_WING_T')
eye = Vector((SEAT[0], 0, SEAT[1] + sim('CROUCH_EYE')))
deliver(
    'flier', o,
    # `Vehicle::hull`: its fuselage, and its wings' slab.
    steel=[((-hl, -bw, 0), (hl, bw, sim('FLY_H'))), ((-hl * 0.6, -hw, wy), (hl * 0.3, hw, wy + wt))],
    rider=[((SEAT[0] - r, -r, SEAT[1]), (SEAT[0] + r, r, SEAT[1] + sim('CROUCH_H')))],
    marks=[(GUNS[0], side * GUNS[1], GUNS[2]) for side in (1, -1)],
    chase=(eye + Vector((-cam('FLY_CAM_BACK_M'), 0, cam('FLY_CAM_UP_M'))), eye + Vector((cam('CRAFT_CAM_MEET_M'), 0, 0))),
    behind=STERN,
    note='Veil flier: a wedge of a lifting body, short swept wings, a gun pod under each, twin fins and engines')

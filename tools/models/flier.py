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
# its width, and from where to where.
STERN = -0.5 * sim('FLY_HALF_L')
WING_FROM = 0.95
BELLY_PAINT = '#4b2a8f'
KEEL = 0.02
KEEL_FROM = -1.5
KEEL_TO = 0.9
# The line of light under each wing, half its width.
EDGE = 0.022
# The canopy: from where to where along the hull and how far out from the
# centre line; its paint as it came is this grey (how little its channels
# differ) and this dark.
GLASS = (0.45, 1.75, 0.45)
GLASS_GREY = 0.12
GLASS_DARK = 0.5

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
paint(o, under, BELLY_PAINT, mix=0.75, rough=0.5, metal=0.3)
# Its canopy came a matte grey lid: dark glass.
base = _image(o, 'Base Color')
rgb = np.empty(base.size[0] * base.size[1] * 4, dtype=np.float32)
base.pixels.foreach_get(rgb)
rgb = rgb.reshape(base.size[1], base.size[0], 4)[::2, ::2, :3]
grey = (rgb.max(axis=-1) - rgb.min(axis=-1) < GLASS_GREY) & (rgb.max(axis=-1) < GLASS_DARK)
paint(o, on & grey & (up > 0.1) & (across < GLASS[2]) & (x > GLASS[0]) & (x < GLASS[1]), '#0b0a16', mix=0.85, rough=0.14,
      metal=0.5)
# Its three engines as Meshy painted them are steel cans, cold inside.
thrusters(o, tx, rings(o, STERN), throat=0.95, deep=0.5, least=0.13)
# The light along each wing's trailing edge is on its top alone; the same
# line under it, where it is seen from the ground, and a keel line aft.
glow = _image(o, 'Emission Color')
lit = np.empty(glow.size[0] * glow.size[1] * 4, dtype=np.float32)
glow.pixels.foreach_get(lit)
lit = lit.reshape(glow.size[1], glow.size[0], 4)[..., :3].max(axis=-1)
top = on & (lit > 0.3) & (up > 0.2) & (across > WING_FROM)
# The line the top's light runs along, as how far back it is at each span.
edge = np.polyfit(across[top], x[top], 2)
span = (float(across[top].min()), float(across[top].max()))
light(o, under * band(x, np.polyval(edge, across) - EDGE, np.polyval(edge, across) + EDGE, EDGE * 0.5)
      * band(across, span[0], span[1], 0.03))
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

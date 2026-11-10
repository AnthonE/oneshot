"""The skimmer (look/craft.rs, `sim::vehicle::SKIMMER`): a one-seat hover
bike of the Veil's, our own design, grown here by rule (veil.py) on
craft.py's kit: a sled, a stag beetle of a bike. Two hulls, each an engine's
pod aft with a lit mouth in its stern and gills of fire down its flank,
necking at an intake to a blade of a runner that bows out and hooks back in
like a mandible, curling up to a prow with a gun under its tip; a deck
between the pods with a saddle on it, open ahead like a tuning fork; a
keeled cowl before the saddle with a visor of dark glass and two horns that
sweep back to its rider's hands; a wing case over the deck behind him with
a seam of light down it and a fin canted out from each of its shoulders.
Its plates lap like a wing case, its seams are cut by rule with dark iron in
them, and its shade is traced into its colours: it carries no map.

    tools/craft.sh skimmer        the loop: this, then the game's views of it
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from craft import *
from veil import *

# Nose to tail, metres: the sim's hull (2 × SKIM_HALF_L).
LENGTH = 2 * sim('SKIM_HALF_L')
HL, HW, SILL = sim('SKIM_HALF_L'), sim('SKIM_HALF_W'), sim('SKIM_SILL')
# How far its keels ride over the ground, metres: a hover's gap.
BELLY = 0.22
# Where the sim seats its rider (`Vehicle::mount`: metres ahead of the
# hull's middle, and the feet over the ground) and where its bolts leave
# (`Vehicle::gun_at`: ahead, to each side, over the ground).
SEAT = (-0.35, 0.5)
GUNS = (HL - 0.1, 0.42, 0.55)
# Where its rider's fists close (ahead, to each side, over the ground): the
# horns end there. `craft.rs`'s `SKIM_GRIPS` is where his wrists are put,
# and his fists are a hand's length up and on from them.
GRIP = (-0.19, 0.285, 1.325)

# A hull, stern to prow: its top line, its keel, its half-width, and how
# far out from the centre line its middle runs. A pod to `NECK`, where it
# necks to a runner's blade that bows out and hooks in to the gun.
STERN = -HL
NECK = (-0.75, -0.4)
H_TOP = curve((STERN, 0.70), (-1.15, 0.76), (NECK[0], 0.72), (NECK[1], 0.54), (0.0, 0.50), (0.7, 0.47), (1.1, 0.54),
              (1.35, 0.63), (HL, 0.66))
H_KEEL = curve((STERN, 0.38), (-1.2, 0.26), (-0.6, BELLY), (0.6, BELLY), (1.0, 0.28), (1.3, 0.44), (HL, 0.6))
H_WIDE = curve((STERN, 0.185), (-1.15, 0.205), (NECK[0], 0.19), (NECK[1], 0.09), (0.0, 0.07), (0.8, 0.06), (1.3, 0.045),
               (HL, 0.02))
H_OUT = curve((STERN, 0.6), (NECK[1], 0.62), (0.5, 0.70), (1.1, 0.60), (HL, 0.435))
H_AT = (STERN, -1.3, -1.05, NECK[0], -0.57, NECK[1], -0.1, 0.3, 0.7, 1.05, 1.3, 1.43, 1.49)
H_N = 16
# How round a hull is (2 an ellipse): the pod's, and the blade's (a ridge).
H_ROUND = curve((NECK[0], 2.1), (NECK[1], 1.45))
MOUTH = 0.82
# The deck between the pods: from where to where, and its top.
DECK = (-1.1, 0.36, 0.46)
# The saddle on it, and the cowl before it: each its half-width and its
# top, stern to prow. Its rider sits this high (the foot of Triple T's trunk
# at the sim's seat, measured in the game's side view).
SEAT_TOP = 0.835
S_WIDE = curve((-1.05, 0.05), (-0.8, 0.13), (-0.35, 0.15), (0.0, 0.2), (0.3, 0.28), (0.6, 0.24), (0.85, 0.14), (1.0, 0.03))
S_TOP = curve((-1.05, 0.52), (-0.82, 0.76), (-0.6, SEAT_TOP), (-0.12, SEAT_TOP), (0.08, 0.8), (0.3, SILL + 0.01), (0.55, 0.76),
              (0.8, 0.6), (1.0, 0.46))
S_AT = (-1.03, -0.82, -0.6, -0.36, -0.12, 0.08, 0.3, 0.5, 0.7, 0.88, 0.98)
S_ROUND = 1.75
PAD = (-0.8, -0.1)
# The ribs that tie the saddle to the hulls: where along the craft.
RIBS = (-0.9, -0.62, 0.16)
# The wing case over the deck behind the rider: its half-width and its top,
# stern to saddle.
K_WIDE = curve((-1.44, 0.14), (-1.15, 0.33), (-0.85, 0.36), (-0.6, 0.2))
K_TOP = curve((-1.44, 0.5), (-1.15, 0.66), (-0.85, 0.74), (-0.6, 0.7))
K_AT = (-1.43, -1.3, -1.12, -0.95, -0.8, -0.68, -0.6)
# A horn: where it leaves the cowl, what it bows toward, and where it turns
# in to the grip; how thick at its foot and at its end (a blade, half as
# thick across as along).
HORN = ((0.42, 0.2, 0.72), (0.44, 0.5, 1.32), (GRIP[0], 0.385, GRIP[2]))
HORN_R = (0.085, 0.03)
GRIP_R = 0.021
# A fin: its foot's leading and trailing edges, its tip's, from how high to
# how high, and how far out its foot and its tip stand (canted out).
FIN = ((-0.86, -1.3), (-1.3, -1.46), 0.5, 1.02, 0.24, 0.44)
# The cowl's eyes, a spindle of light each side of its nose: from where to
# where, and how far up its flank.
EYE = (0.5, 0.94, 0.5)

scene()
# A seam between two plates, with dark iron in it; a hairline; a seam with
# light in it.
GAP = dict(width=0.045, depth=0.02, flat=0.5)
SEAM = dict(width=0.028, depth=0.012)
GLOW = dict(floor='light', wall='iron', flat=0.6)
X = lambda q: q[:, 0]
Y = lambda q: np.abs(q[:, 1])
Z = lambda q: q[:, 2]
both = lambda *r: np.minimum.reduce(r)
run = lambda a, b: (lambda q: np.minimum(X(q) - a, b - X(q)))
parts = []

MZ = (H_TOP(STERN) + H_KEEL(STERN)) / 2
MW, MH = H_WIDE(STERN) * MOUTH, (H_TOP(STERN) - MZ) * MOUTH
neck = next(i for i, x in enumerate(H_AT) if x >= NECK[1]) + 2


def hull(side):
    """One of its two hulls: a pod with an engine's mouth for a stern, and
    the blade it necks to, ridged along its back and its outer flank."""
    at = (STERN, side * H_OUT(STERN), MZ)
    rings = [oval(x, side * H_OUT(x), (H_TOP(x) + H_KEEL(x)) / 2 + 0.03, H_WIDE(x), (H_TOP(x) - H_KEEL(x)) / 2 - 0.03,
                  (H_TOP(x) - H_KEEL(x)) / 2 + 0.03, n=H_N, p=H_ROUND(x), pb=2.5) for x in H_AT]
    o = solid(loft(lip(at, MW, MH, n=H_N, p=2.3) + rings, 'shell', ends=('open', 'tip'), name='hull',
                   sharp=((H_N // 4, neck, 99), (0 if side > 0 else H_N // 2, neck, 99))), 'hull')
    return o, mouth(at, MW, MH, deep=0.2, core=(0.13, 0.1, 0.78, 0.74), vanes=2, n=H_N, p=2.3), at


burns = []
hulls = []
for side in (1, -1):
    o, engine, at = hull(side)
    hulls.append(o)
    parts += engine
    burns.append(at)
hulls = solid(hulls, 'hulls')
out = lambda q: Y(q) - H_OUT(X(q))
mid = lambda q: (H_TOP(X(q)) + H_KEEL(X(q))) / 2
# Rules for where a seam runs, each metres inside its edge: over a hull's
# middle, and on its outer flank.
top = lambda q: Z(q) - mid(q) - 0.05
flank = lambda q: out(q) - 0.03
# A seam of light down each pod's back; a line of light along each outer
# flank, a third of the way up it, from the pod to the prow.
seam(hulls, out, 0.07, 0.016, **GLOW, within=lambda q: both(top(q), run(STERN + 0.14, NECK[1] - 0.06)(q)))
seam(hulls, lambda q: Z(q) - (H_KEEL(X(q)) + 0.36 * (H_TOP(X(q)) - H_KEEL(X(q)))), 0.036, 0.012, **GLOW,
     within=lambda q: both(flank(q), run(-1.12, 1.3)(q)))
# The intake: a slot of dark iron round each pod's neck.
seam(hulls, plane((NECK[0] + 0.14, 0, 0.6), (1, 0, 0.5)), 0.1, 0.03, flat=0.6, where=lambda q: Z(q) > mid(q) - 0.12)
# Its petals: a seam each side of a pod's back; and one across each blade.
for side in (1, -1):
    for lean in (-0.9, 0.9):
        seam(hulls, plane((0, side * H_OUT(-1.1), MZ + 0.02), (0, side * 1.0, lean)), **SEAM,
             within=lambda q: both(run(STERN + 0.12, NECK[0] + 0.02)(q), side * q[:, 1], Z(q) - MZ))
seam(hulls, plane((0.72, 0, 0), (1, 0, 0.45)), **GAP)
# Gills down each pod's outer flank, the engine's fire in them.
for x in (-0.92, -1.04, -1.16):
    seam(hulls, plane((x, 0, 0.56), (1, 0, 0.55)), 0.05, 0.02, floor='throat', wall='iron', flat=0.5,
         within=lambda q: both(out(q) - 0.05, Z(q) - 0.49, 0.64 - Z(q)))
parts += [
    # Each pod's back: a paler plate over it, proud of the neck's; each
    # blade's back a plate of its own; the darker hull under them. A rim of
    # iron round each mouth.
    plate(hulls, lambda q: both(top(q) + 0.02, X(q) - STERN - 0.1, NECK[0] - 0.03 - X(q)), 0.03, 'shell_pale', name='backs'),
    plate(hulls, lambda q: both(top(q) + 0.02, X(q) - NECK[0] + 0.07, NECK[1] + 0.04 - X(q)), 0.018, 'shell', name='necks',
          clear=None),
    plate(hulls, lambda q: both(top(q) - 0.03, X(q) - NECK[1] - 0.1, 0.66 - X(q)), 0.012, 'shell', name='blades', clear=None),
    plate(hulls, lambda q: STERN + 0.085 - X(q), 0.012, 'iron', name='rims'),
]
repaint(hulls, 'shell_deep', where=lambda q: Z(q) < mid(q) + 0.03, of='shell')
parts.append(hulls)

# The deck between the pods, the rider's feet on a board each side of the
# saddle.
deck = loft([oval(x, 0, DECK[2] - 0.06, H_OUT(x) - H_WIDE(x) * 0.6, 0.06, 0.07, n=12, p=5.0)
             for x in np.linspace(DECK[0], DECK[1], 5)], 'iron', ends=('flat', 'flat'), levels=1, name='deck')
parts.append(deck)
for side in (1, -1):
    for dx, dy, dz, at in ((0.42, 0.2, 0.03, (-0.26, side * 0.3, SEAT[1] - 0.02)), (0.035, 0.2, 0.06, (-0.05, side * 0.3, SEAT[1]))):
        board = box(dx, dy, dz, at=at, material=None, bevel=0.006)
        board.data.materials.append(skin('shell_deep'))
        parts.append(board)
# Ribs from the saddle down to each hull, over the deck.
for x in RIBS:
    for side in (1, -1):
        reach = H_OUT(x) - H_WIDE(x) * 0.4
        parts.append(stem(bow((x, side * 0.1, 0.62), (x - 0.03, side * reach * 0.55, 0.6), (x - 0.06, side * reach, 0.5), steps=5),
                          [0.034, 0.032, 0.03, 0.03, 0.032, 0.036], n=8, material='iron', name='rib'))

# The saddle and the cowl before it: one hump down the deck's middle, the
# cowl keeled down its nose.
base = DECK[2] - 0.06
rings = [oval(x, 0, base, S_WIDE(x), S_TOP(x) - base, 0.05, n=14, p=S_ROUND) for x in S_AT]
keel = next(i for i, x in enumerate(S_AT) if x >= 0.3)
saddle = solid(loft(rings, 'shell', name='saddle', sharp=((14 // 4 + 0, keel, 99),)), 'saddle')
# A cushion on its spine, for the rider.
pad = lambda q: both(X(q) - PAD[0], PAD[1] - X(q), Z(q) - (SEAT_TOP - 0.07))
seam(saddle, pad, 0.02, 0.008)
repaint(saddle, 'pad', where=lambda q: pad(q) > 0.01)
# The cowl: a visor of dark glass before the rider, and a seam across its
# brow.
visor = lambda q: both(X(q) - 0.1, plane((0.5, 0, 0.78), (-0.5, 0, 1))(q), 0.23 - Y(q))
seam(saddle, visor, 0.022, 0.009)
repaint(saddle, 'glass', where=lambda q: visor(q) > 0.011)
seam(saddle, plane((0.66, 0, 0), (1, 0, 0.6)), **GAP, where=lambda q: X(q) > 0.3)
parts.append(saddle)
# Its eyes: a spindle of light along each flank of the cowl's nose.
for side in (1, -1):
    path = []
    for x in np.linspace(EYE[0], EYE[1], 7):
        up = S_TOP(x) - base
        path.append((x, side * (S_WIDE(x) * max(1 - EYE[2] ** S_ROUND, 0.0) ** (1 / S_ROUND) - 0.012), base + EYE[2] * up))
    parts.append(stem(path, [0.005, 0.017, 0.026, 0.03, 0.026, 0.016, 0.005], n=8, material='light', name='eye', flat=0.7))

# The wing case behind the rider: a low dome over the deck between the
# pods, a seam of light down its middle, its end alight.
rings = [oval(x, 0, DECK[2] - 0.02, K_WIDE(x), K_TOP(x) - DECK[2] + 0.02, 0.04, n=14, p=2.2) for x in K_AT]
case = solid(loft(rings, 'shell', name='case'), 'case')
seam(case, plane((0, 0, 0), (0, 1, 0)), 0.075, 0.016, **GLOW, within=lambda q: both(Z(q) - DECK[2] - 0.08, run(-1.36, -0.7)(q)))
for side in (1, -1):
    seam(case, plane((0, side * 0.19, 0), (-0.25, side, 0)), **SEAM, within=lambda q: both(Z(q) - DECK[2] - 0.06, run(-1.3, -0.72)(q)))
repaint(case, 'light', where=lambda q: X(q) < K_AT[0] + 0.07)
parts.append(case)

# The horns: blades from the cowl's shoulders up, out and back to the
# rider's hands, each out of a socket of iron, a grip turned in at its end.
for side in (1, -1):
    path = bow(*[(x, side * y, z) for x, y, z in HORN], steps=7)
    radii = [HORN_R[0] + (HORN_R[1] - HORN_R[0]) * (i / 7) ** 0.7 for i in range(8)]
    parts.append(stem(path, radii, n=10, material='shell_deep', name='horn', ends=('tip', 'tip'), flat=0.5, thin=(0, 1, 0)))
    socket = sphere(1.0, at=(HORN[0][0], side * HORN[0][1], HORN[0][2] + 0.03), material=None, scale=(0.13, 0.085, 0.07),
                    segments=16)
    socket.data.materials.append(skin('iron'))
    grip = cyl(GRIP_R, 0.2, axis='y', at=(GRIP[0], side * (GRIP[1] + 0.005), GRIP[2]), material=None, verts=14)
    grip.data.materials.append(skin('pad'))
    knob = sphere(HORN_R[1] * 1.1, at=(GRIP[0], side * 0.385, GRIP[2]), material=None, segments=16)
    knob.data.materials.append(skin('shell_deep'))
    cap = sphere(GRIP_R * 1.3, at=(GRIP[0], side * (GRIP[1] - 0.1), GRIP[2]), material=None, segments=12)
    cap.data.materials.append(skin('light_dim'))
    parts += [socket, grip, knob, cap]

# The fins: one canted out from each shoulder of the wing case, a light down
# its trailing edge.
(f0, f1), (t0, t1), z0, z1, y0, y1 = FIN
for side in (1, -1):
    rings = []
    for k in (0.0, 0.3, 0.6, 0.85, 1.0):
        rings.append(foil(side * (y0 + (y1 - y0) * k), f0 + (t0 - f0) * k, f1 + (t1 - f1) * k, z0 + (z1 - z0) * k,
                          0.085 * (1 - 0.75 * k), n=12, under=1.0, upright=True))
    fin = solid(loft(rings, 'shell', ends=('flat', 'tip'), name='fin'), 'fin')
    rise = lambda q: np.clip((Z(q) - z0) / (z1 - z0), 0.0, 1.0)
    chord = lambda q: (X(q) - (f0 + (t0 - f0) * rise(q))) / ((f1 + (t1 - f1) * rise(q)) - (f0 + (t0 - f0) * rise(q)))
    repaint(fin, 'light', where=lambda q: (chord(q) > 0.8) & (Z(q) > 0.62))
    parts.append(fin)

# A gun under each prow's tip: its barrel in a collar of light.
for side in (1, -1):
    y, z = side * GUNS[1], GUNS[2]
    for at, long, wide, paint in ((GUNS[0] - 0.1, 0.24, 0.03, 'iron'), (GUNS[0] - 0.1, 0.06, 0.04, 'light'),
                                  (GUNS[0] - 0.012, 0.04, 0.036, 'iron')):
        part = cyl(wide, long, at=(at, y, z), material=None, verts=14, bevel=0.003)
        part.data.materials.append(skin(paint))
        parts.append(part)

o = solid(parts, 'skimmer')
# Soot round the engines' mouths and aft of the gills.
shade(o, tone=0.14, edge=0.75,
      stain=lambda q: 1.0 - 0.4 * np.clip((-1.12 - q[:, 0]) / 0.36, 0.0, 1.0) * (np.abs(q[:, 1]) > 0.38))
gloss(o)

r = sim('BODY_R')
eye = Vector((SEAT[0], 0, SEAT[1] + sim('CROUCH_EYE')))
deliver(
    'skimmer', o,
    steel=[((-HL, -HW, 0), (HL, HW, SILL))],
    rider=[((SEAT[0] - r, -r, SEAT[1]), (SEAT[0] + r, r, SEAT[1] + sim('CROUCH_H')))],
    marks=[(GUNS[0], side * GUNS[1], GUNS[2]) for side in (1, -1)],
    chase=(eye + Vector((-cam('SKIM_CAM_BACK_M'), 0, cam('SKIM_CAM_UP_M'))), eye + Vector((cam('CRAFT_CAM_MEET_M'), 0, 0))),
    burns=burns,
    note="Veil skimmer: a sled of a hover bike, two engine pods necking to runners' blades that hook in like mandibles, "
         "a gun under each prow, a wing case and two fins behind its rider, horns back to his hands")

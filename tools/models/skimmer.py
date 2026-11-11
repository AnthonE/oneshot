"""The skimmer (look/craft.rs, `sim::vehicle::SKIMMER`): a one-seat hover
bike of the Veil's, our own design, grown here by rule (veil.py) on
craft.py's kit: a sled, a stag beetle of a bike. Two hulls, each an engine's
pod aft with a turbine's stator down the throat of its stern and louvres
down its flank that show its fire when it boosts, necking at an intake to a
blade of a runner that bows out and hooks back in like a mandible, curling
up to a prow with a gun under its tip; a deck between the pods with a
saddle on it and the lift's two lenses under it, open ahead like a tuning
fork; a keeled cowl before the saddle with a visor of dark glass, louvres
down its flanks and two braced control arms that rake back to its rider's
hands; a wing case over the deck behind him with a hairline of light down
it and a fin canted out from each of its shoulders. It is a machine and is
lit like one: its engines, its lift, a lamp at each fin's tip and each gun's
collar, and nothing drawn on it in light. Its plates lap like a wing case
and are fastened down, its seams are cut by rule with dark iron in them,
and its shade is traced into its colours: it carries no map.

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
H_TOP = curve((STERN, 0.70), (-1.15, 0.76), (NECK[0], 0.72), (NECK[1], 0.54), (0.0, 0.50), (0.7, 0.47), (1.1, 0.55),
              (1.35, 0.7), (HL, 0.83))
H_KEEL = curve((STERN, 0.38), (-1.2, 0.26), (-0.6, BELLY), (0.6, BELLY), (1.0, 0.28), (1.3, 0.5), (HL, 0.77))
H_WIDE = curve((STERN, 0.185), (-1.15, 0.205), (NECK[0], 0.19), (NECK[1], 0.09), (0.0, 0.07), (0.8, 0.06), (1.3, 0.045),
               (HL, 0.02))
H_OUT = curve((STERN, 0.6), (NECK[1], 0.62), (0.5, 0.70), (1.1, 0.60), (HL, 0.435))
H_AT = (STERN, -1.3, -1.05, NECK[0], -0.57, NECK[1], -0.1, 0.3, 0.7, 1.05, 1.3, 1.43, 1.49)
H_N = 16
# How round a hull is (2 an ellipse): the pod's, and the blade's (a ridge).
H_ROUND = curve((NECK[0], 2.1), (NECK[1], 1.45))
MOUTH = 0.9
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
# The arms that tie the saddle to the hulls: where along the craft.
RIBS = (-0.66, 0.16)
# The wing case over the deck behind the rider: its half-width and its top,
# stern to saddle.
K_WIDE = curve((-1.44, 0.14), (-1.15, 0.33), (-0.85, 0.36), (-0.6, 0.2))
K_TOP = curve((-1.44, 0.5), (-1.15, 0.66), (-0.85, 0.74), (-0.6, 0.7))
K_AT = (-1.43, -1.3, -1.12, -0.95, -0.8, -0.68, -0.6)
# A horn: where it leaves the cowl, what it bows toward, and where it turns
# in to the grip; how thick at its foot and at its end (a blade, half as
# thick across as along).
HORN = ((0.4, 0.2, 0.73), (0.3, 0.49, 1.3), (GRIP[0], 0.385, GRIP[2]))
HORN_R = (0.058, 0.024)
# The brace under each: from the cowl's flank up to the arm's first knuckle.
BRACE = (0.62, 0.17, 0.6)
# Its knuckles: at which of its bow's eight points.
KNUCKLES = (2, 5)
GRIP_R = 0.021
# A fin: its foot's leading and trailing edges, its tip's, from how high to
# how high, and how far out its foot and its tip stand (canted out).
FIN = ((-0.86, -1.3), (-1.3, -1.46), 0.5, 1.02, 0.24, 0.44)
# The cowl's louvres, down each flank of its nose: where along the craft.
LOUVRES = (0.5, 0.58, 0.66)
# The lift's lenses under the deck, one each side of the keel: where along
# the craft, how far out, how long and wide, how far they stand out of it.
LIFT = (-0.3, 0.25, 0.3, 0.085, 0.035)

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
    # (Its core lies at the throat's floor, small: from the saddle the lip
    # hides its top and the wall under it goes from black to lit. Up at the
    # lip and two thirds across it was a lamp plugged in the end of a tube.)
    # A stator before it: the fire is seen between a turbine's vanes.
    return o, (mouth(at, MW, MH, deep=0.3, core=(0.22, 0.07, 0.56, 0.56), vanes=0, n=H_N, p=2.3, wall='iron')
               + stator(at, MW, MH, 0.3, blades=11)), at


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
# A channel of dark iron along each blade's outer flank, a third of the way
# up it, thinning from the neck to the gun. (It was a line of light: a
# stripe of light down a flank is drawn on, and lights nothing.)
taper = curve((NECK[1], 1.5), (1.3, 0.4))
seam(hulls, lambda q: (Z(q) - (H_KEEL(X(q)) + 0.36 * (H_TOP(X(q)) - H_KEEL(X(q))))) / taper(X(q)), 0.03, 0.014, flat=0.5,
     within=lambda q: both(flank(q), run(NECK[1], 1.3)(q)))
# The intake: a notch of dark iron across each pod's neck.
seam(hulls, plane((NECK[0] + 0.14, 0, 0.6), (1, 0, 0.5)), 0.1, 0.03, flat=0.6, where=lambda q: Z(q) > mid(q) + 0.06)
# Its petals: a seam each side of a pod's back; and a hairline over each blade's.
for side in (1, -1):
    for lean in (-0.9, 0.9):
        seam(hulls, plane((0, side * H_OUT(-1.1), MZ + 0.02), (0, side * 1.0, lean)), **SEAM,
             within=lambda q: both(run(STERN + 0.12, NECK[0] + 0.02)(q), side * q[:, 1], Z(q) - MZ))
# A hairline round each pod, where its cowl comes off.
seam(hulls, plane((-1.27, 0, 0), (1, 0, 0.12)), 0.014, 0.006, where=lambda q: X(q) < NECK[0])
cross = plane((0.72, 0, 0), (1, 0, 0.45))
seam(hulls, cross, **SEAM, where=lambda q: Z(q) > mid(q))
# A hairline along each blade's flank, under the lip of its back's plate,
# from the neck to that seam.
seam(hulls, lambda q: top(q) - 0.03, **SEAM, within=lambda q: both(X(q) - NECK[1] - 0.12, -cross(q)))
# Louvres down each pod's outer flank, under its back's plate: dark, and
# the engine's fire in them when it boosts (a vein: `craft.rs`'s `BURNS`).
for x in (-0.9, -1.02, -1.14):
    seam(hulls, plane((x, 0, 0.45), (1, 0, 0.55)), 0.05, 0.02, floor='vein', wall='iron', flat=0.5,
         within=lambda q: both(out(q) - 0.05, Z(q) - 0.375, 0.5 - Z(q)))
parts += [
    # Each pod's back: a paler plate over it, proud of the neck's; each
    # blade's back a pale strake of its own; the darker hull under them. A
    # rim of iron round each mouth.
    plate(hulls, lambda q: both(top(q) + 0.02, X(q) - STERN - 0.1, NECK[0] - 0.03 - X(q)), 0.03, 'shell_pale', name='backs'),
    plate(hulls, lambda q: both(top(q) + 0.02, X(q) - NECK[0] + 0.07, NECK[1] + 0.04 - X(q)), 0.018, 'shell', name='necks',
          clear=None),
    # (The strake's lip stands clear of the hairline under it, and it ends
    # short of the seam across the blade: a plate cut along a seam's own
    # line carries half its groove up on its edge.)
    plate(hulls, lambda q: both(top(q) - 0.052, X(q) - NECK[1] - 0.1, -cross(q) - 0.022), 0.025, 'shell_pale', name='blades',
          clear=None),
    plate(hulls, lambda q: STERN + 0.05 - X(q), 0.012, 'iron', name='rims'),
]
# The blades dark, the pods violet over a darker hull.
repaint(hulls, 'shell_deep', where=lambda q: (Z(q) < mid(q) + 0.03) | (X(q) > NECK[1] + 0.04), of='shell')
parts.append(hulls)
# A vein of the engine's fire down each pod's back: a thread laid along its
# ridge, asleep until the engine wakes it (`craft.rs`'s `BURNS`).
for side in (1, -1):
    path = [(x, side * H_OUT(x), H_TOP(x) + 0.022) for x in np.linspace(STERN + 0.2, NECK[0] - 0.06, 8)]
    parts.append(stem(path, [0.004, 0.012, 0.014, 0.014, 0.014, 0.014, 0.012, 0.004], n=8, material='vein', name='vein', flat=0.7))

# The deck between the pods, drawn in to a bow ahead, the rider's feet on a
# board each side of the saddle.
deck = loft([oval(x, 0, DECK[2] - 0.06, (H_OUT(x) - H_WIDE(x) * 0.6) * k, 0.06, 0.07, n=12, p=3.0)
             for x, k in zip(np.linspace(DECK[0], DECK[1] + 0.12, 6), (1.0, 1.0, 1.0, 1.0, 0.86, 0.5))], 'shell_deep',
            ends=('flat', 'tip'), levels=1, name='deck')
parts.append(deck)
for side in (1, -1):
    parts += lens((LIFT[0], side * LIFT[1], DECK[2] - 0.125), LIFT[2], LIFT[3], LIFT[4], vanes=4)
for side in (1, -1):
    for dx, dy, dz, at in ((0.42, 0.2, 0.03, (-0.26, side * 0.3, SEAT[1] - 0.02)), (0.035, 0.2, 0.06, (-0.05, side * 0.3, SEAT[1]))):
        board = box(dx, dy, dz, at=at, material=None, bevel=0.006)
        board.data.materials.append(skin('shell_deep'))
        parts.append(board)
# Arms from the saddle down to each hull, over the deck: blades of shell.
for x in RIBS:
    for side in (1, -1):
        reach = H_OUT(x) - H_WIDE(x) * 0.4
        parts.append(stem(bow((x, side * 0.1, 0.62), (x - 0.03, side * reach * 0.55, 0.62), (x - 0.06, side * reach, 0.5), steps=5),
                          [0.075, 0.07, 0.062, 0.06, 0.064, 0.072], n=8, material='shell_deep', name='arm', flat=0.4,
                          thin=(0, 0, 1)))
        # A boss of iron where each meets the saddle and the hull.
        for at in ((x, side * 0.14, 0.62), (x - 0.055, side * (reach - 0.03), 0.52)):
            boss = sphere(1.0, at=at, material=None, scale=(0.085, 0.05, 0.04), segments=12)
            boss.data.materials.append(skin('iron'))
            parts.append(boss)

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
seam(saddle, plane((0.74, 0, 0), (1, 0, 0.6)), **GAP, where=lambda q: X(q) > 0.3)
# Louvres down each flank of its nose, raked as its brow's seam is: where
# its lift breathes. (It had an eye of light there each side: a face.)
for x in LOUVRES:
    seam(saddle, plane((x, 0, base + 0.12), (1, 0, 0.6)), 0.03, 0.014, flat=0.5,
         within=lambda q: both(Y(q) - 0.09, Z(q) - base - 0.05, base + 0.62 * (S_TOP(X(q)) - base) - Z(q)))
parts.append(saddle)

# The wing case behind the rider: a low dome over the deck between the
# pods, a seam of light down its middle.
rings = [oval(x, 0, DECK[2] - 0.02, K_WIDE(x), K_TOP(x) - DECK[2] + 0.02, 0.04, n=14, p=2.2) for x in K_AT]
case = solid(loft(rings, 'shell', name='case'), 'case')
seam(case, plane((0, 0, 0), (0, 1, 0)), 0.02, 0.012, **GLOW, within=lambda q: both(Z(q) - DECK[2] - 0.08, run(-1.36, -0.7)(q)))
for side in (1, -1):
    seam(case, plane((0, side * 0.19, 0), (-0.25, side, 0)), **SEAM, within=lambda q: both(Z(q) - DECK[2] - 0.06, run(-1.3, -0.72)(q)))
parts.append(case)

# The control arms: blades from the cowl's shoulders raked up, out and back
# to the rider's hands, each out of a socket of iron with a brace of iron
# under it, a grip turned in at its end.
for side in (1, -1):
    path = bow(*[(x, side * y, z) for x, y, z in HORN], steps=7)
    radii = [HORN_R[0] + (HORN_R[1] - HORN_R[0]) * (i / 7) ** 0.7 for i in range(8)]
    horn = solid(stem(path, radii, n=10, material='shell_deep', name='horn', ends=('tip', 'tip'), flat=0.5, thin=(0, 1, 0)), 'horn')
    # Its knuckles: a joint of dark iron across it at each, the blade
    # between them a paler plate, so it is of the hulls' make and not a hose.
    joints = []
    for i in KNUCKLES:
        at = np.array(path[i][:])
        joints.append(plane(at, (path[i + 1] - path[i - 1])[:]))
        seam(horn, joints[-1], 0.03, 0.01, flat=0.5, where=lambda q, at=at: np.linalg.norm(q - at, axis=1) < 0.16)
    repaint(horn, 'shell', where=lambda q: (joints[0](q) > 0.016) & (joints[1](q) < -0.016), of='shell_deep')
    parts.append(horn)
    socket = sphere(1.0, at=(HORN[0][0], side * HORN[0][1], HORN[0][2] + 0.035), material=None, scale=(0.12, 0.08, 0.035),
                    segments=16)
    socket.data.materials.append(skin('iron'))
    grip = cyl(GRIP_R, 0.2, axis='y', at=(GRIP[0], side * (GRIP[1] + 0.005), GRIP[2]), material=None, verts=14)
    grip.data.materials.append(skin('pad'))
    knob = sphere(HORN_R[1] * 1.1, at=(GRIP[0], side * 0.385, GRIP[2]), material=None, segments=16)
    knob.data.materials.append(skin('shell_deep'))
    cap = sphere(GRIP_R * 1.25, at=(GRIP[0], side * (GRIP[1] - 0.1), GRIP[2]), material=None, segments=12)
    cap.data.materials.append(skin('iron'))
    brace = stem([(BRACE[0], side * BRACE[1], BRACE[2]), tuple((Vector((BRACE[0], side * BRACE[1], BRACE[2])) + path[KNUCKLES[0]]) / 2
                                                             + Vector((0.03, side * 0.02, 0.0))), tuple(path[KNUCKLES[0]])],
                 [0.02, 0.013, 0.013], n=8, material='iron', name='brace')
    parts += [socket, grip, knob, cap, brace]

# The fins: one canted out from each shoulder of the wing case, a lamp at
# the tip of its trailing edge (its trail leaves from there).
(f0, f1), (t0, t1), z0, z1, y0, y1 = FIN
for side in (1, -1):
    rings = []
    for k in (0.0, 0.3, 0.6, 0.85, 1.0):
        rings.append(foil(side * (y0 + (y1 - y0) * k), f0 + (t0 - f0) * k, f1 + (t1 - f1) * k, z0 + (z1 - z0) * k,
                          0.06 * (1 - 0.75 * k), n=12, under=1.0, upright=True))
    fin = solid(loft(rings, 'shell', ends=('flat', 'tip'), name='fin'), 'fin')
    rise = lambda q: np.clip((Z(q) - z0) / (z1 - z0), 0.0, 1.0)
    chord = lambda q: (X(q) - (f0 + (t0 - f0) * rise(q))) / ((f1 + (t1 - f1) * rise(q)) - (f0 + (t0 - f0) * rise(q)))
    repaint(fin, 'light', where=lambda q: (chord(q) > 0.86) & (Z(q) > 0.92))
    parts.append(fin)

# A gun slung under each prow's hook: its housing beside the blade on a
# pylon of iron, a collar of light round its barrel.
for side in (1, -1):
    y, z = side * GUNS[1], GUNS[2]
    parts.append(stem([(GUNS[0] - 0.4, y, z - 0.03), (GUNS[0] - 0.3, y, z - 0.012), (GUNS[0] - 0.16, y, z), (GUNS[0] - 0.05, y, z)],
                      [0.015, 0.044, 0.047, 0.04], n=10, material='shell_deep', name='gun', ends=('tip', 'flat')))
    for at, long, wide, paint in ((GUNS[0] - 0.2, 0.1, 0.052, 'iron'), (GUNS[0] - 0.115, 0.022, 0.05, 'light_dim'),
                                  (GUNS[0] - 0.045, 0.12, 0.028, 'iron')):
        part = cyl(wide, long, at=(at, y, z), material=None, verts=14, bevel=0.003)
        part.data.materials.append(skin(paint))
        parts.append(part)
    x = GUNS[0] - 0.24
    pylon = box(0.13, abs(H_OUT(x) - GUNS[1]) + 0.04, 0.045, at=(x, side * (H_OUT(x) + GUNS[1]) / 2, z), material=None, bevel=0.006)
    pylon.data.materials.append(skin('iron'))
    parts.append(pylon)

o = solid(parts, 'skimmer')
# Its fasteners: a ring round each pod's stern, a row down each side of a
# pod's back plate, round the wing case's foot and along each fin's.
pins = []
for side in (1, -1):
    for a in np.linspace(0.0, TAU, 12, endpoint=False):
        pins.append((STERN + 0.095, side * H_OUT(STERN) + 1.2 * H_WIDE(STERN) * math.cos(a), MZ + 1.2 * (H_TOP(STERN) - MZ) * math.sin(a)))
    for x in np.linspace(STERN + 0.2, NECK[0] - 0.1, 6):
        for lean in (1, -1):
            pins.append((x, side * H_OUT(x) + lean * 1.1 * H_WIDE(x), (H_TOP(x) + H_KEEL(x)) / 2 + 0.085))
    for x in np.linspace(-1.3, -0.76, 5):
        pins.append((x, side * (K_WIDE(x) + 0.02), DECK[2] + 0.1))
    for k in (0.12, 0.5, 0.88):
        pins.append((f0 + (f1 - f0) * k, side * (y0 + 0.05), z0 + 0.07))
o = solid([o, studs(o, pins)], 'skimmer')


def soot(q):
    """What is left of the paint: soot round the engines' mouths and aft of
    the louvres, and each mouth's lip blued by its heat."""
    aft = np.clip((-1.12 - q[:, 0]) / 0.36, 0.0, 1.0) * (np.abs(q[:, 1]) > 0.38)
    left = (1.0 - 0.45 * aft)[:, None] * np.ones((1, 3))
    lip = (np.clip((STERN + 0.1 - q[:, 0]) / 0.1, 0.0, 1.0) * (np.abs(q[:, 1]) > 0.38))[:, None]
    return left * (1 - lip) + lip * np.array([1.25, 1.0, 1.45])


shade(o, tone=0.14, stain=soot)
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

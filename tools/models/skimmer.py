"""The skimmer (look/craft.rs, `sim::vehicle::SKIMMER`): a one-seat hover
bike of the Veil's, our own design, grown here by rule (veil.py) on
craft.py's kit: a catamaran, a stag beetle of a bike. Two blade hulls like
mandibles, toed in to a gun in each prow, an engine's mouth in each stern;
a deck between them with a saddle on it, open ahead like a tuning fork; a
cowl before the saddle with two horns that sweep back to its rider's hands;
a fin behind with a light down its edge. Its seams are cut by rule, its
lights are the gaps between its plates, and its shade is traced into its
colours: it carries no map.

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
# Where its rider's hands are (`craft.rs`'s `SKIM_GRIPS`: ahead, to each
# side, over the ground): the horns end there.
GRIP = (-0.18, 0.285, 1.24)

# A hull, stern to prow: its top line, its keel, its half-width, and how
# far out from the centre line its middle runs (toed in to the gun).
STERN = -HL
H_TOP = curve((STERN, 0.70), (-1.15, 0.76), (-0.7, 0.70), (0.0, 0.58), (0.7, 0.50), (1.1, 0.56), (1.35, 0.63), (HL, 0.66))
H_KEEL = curve((STERN, 0.38), (-1.2, 0.26), (-0.6, BELLY), (0.6, BELLY), (1.0, 0.28), (1.3, 0.44), (HL, 0.6))
H_WIDE = curve((STERN, 0.19), (-1.15, 0.22), (-0.6, 0.17), (0.0, 0.12), (0.8, 0.085), (1.3, 0.05), (HL, 0.012))
H_OUT = curve((STERN, 0.62), (0.4, 0.62), (1.0, 0.56), (HL, 0.43))
H_AT = (STERN, -1.3, -1.05, -0.7, -0.3, 0.2, 0.7, 1.05, 1.3, 1.43, 1.49)
H_N = 16
MOUTH = 0.82
# The deck between the hulls: from where to where, and its top.
DECK = (-1.1, 0.36, 0.46)
# The saddle on it, and the cowl before it: each its half-width and its
# top, stern to prow.
S_WIDE = curve((-1.05, 0.05), (-0.8, 0.15), (-0.35, 0.17), (0.0, 0.21), (0.3, 0.28), (0.6, 0.24), (0.85, 0.14), (1.0, 0.03))
S_TOP = curve((-1.05, 0.52), (-0.8, 0.7), (-0.5, 0.74), (-0.2, 0.74), (0.05, 0.8), (0.3, SILL + 0.01), (0.55, 0.76),
              (0.8, 0.6), (1.0, 0.46))
S_AT = (-1.03, -0.8, -0.55, -0.3, -0.05, 0.15, 0.32, 0.5, 0.7, 0.88, 0.98)
PAD = (-0.78, -0.08)
# The ribs that tie the saddle to the hulls: where along the craft.
RIBS = (-0.9, -0.62, 0.16)
# A horn: where it leaves the cowl, what it bows toward, and where it turns
# in to the grip; how thick at its foot and at its end.
HORN = ((0.42, 0.2, 0.72), (0.42, 0.5, 1.24), (GRIP[0], 0.385, GRIP[2]))
HORN_R = (0.06, 0.022)
GRIP_R = 0.021
# The fin behind the saddle: its foot's leading and trailing edges, its
# tip's, and how tall it stands.
FIN = ((-0.78, -1.3), (-1.3, -1.46), 0.44, 1.02)

scene()
SEAM = dict(width=0.028, depth=0.012)
GLOW = dict(floor='light', wall='iron', flat=0.45)
X = lambda q: q[:, 0]
Y = lambda q: np.abs(q[:, 1])
Z = lambda q: q[:, 2]
both = lambda *r: np.minimum.reduce(r)
parts = []


def hull(side):
    """One of its two hulls: a blade, its stern an engine's mouth."""
    mz = (H_TOP(STERN) + H_KEEL(STERN)) / 2
    mw, mh = H_WIDE(STERN) * MOUTH, (H_TOP(STERN) - mz) * MOUTH
    at = (STERN, side * H_OUT(STERN), mz)
    rings = [oval(x, side * H_OUT(x), (H_TOP(x) + H_KEEL(x)) / 2 + 0.03, H_WIDE(x), (H_TOP(x) - H_KEEL(x)) / 2 - 0.03,
                  (H_TOP(x) - H_KEEL(x)) / 2 + 0.03, n=H_N, p=1.75, pb=2.5) for x in H_AT]
    # Its back is a ridge, a blade's.
    o = solid(loft(lip(at, mw, mh, n=H_N, p=2.3) + rings, 'shell', ends=('open', 'tip'), name='hull', sharp=(H_N // 4,)),
              'hull')
    return o, mouth(at, mw, mh, deep=0.2, core=(0.13, 0.1, 0.78, 0.74), vanes=2, n=H_N, p=2.3), at


burns = []
hulls = []
for side in (1, -1):
    o, engine, at = hull(side)
    hulls.append(o)
    parts += engine
    burns.append(at)
hulls = solid(hulls, 'hulls')
out = lambda q: Y(q) - H_OUT(X(q))
top = lambda q: Z(q) > (H_TOP(X(q)) + H_KEEL(X(q))) / 2 + 0.06
under = lambda q: Z(q) < (H_TOP(X(q)) + H_KEEL(X(q))) / 2 - 0.05
# A seam of light down each hull's spine and over its prow to the gun; the
# lift's light along each keel.
seam(hulls, out, 0.04, 0.012, **GLOW, where=lambda q: top(q) & (X(q) > STERN + 0.32) & (X(q) < 1.36))
seam(hulls, out, 0.07, 0.014, floor='light_dim', wall='iron', flat=0.6,
     where=lambda q: under(q) & (X(q) > -1.15) & (X(q) < 1.0))
# The prow's plate, the stern's cowl, two seams across.
seam(hulls, plane((0.95, 0, 0), (1, 0, 0.45)), **SEAM)
seam(hulls, plane((0.35, 0, 0), (1, 0, -0.35)), **SEAM)
# Gills on each hull's outer flank.
for x in (-0.86, -0.96, -1.06):
    seam(hulls, plane((x, 0, 0.5), (1, 0, 0.55)), 0.024, 0.012,
         where=lambda q: (out(q) > 0.08) & (Z(q) > 0.36) & (Z(q) < 0.6))
parts += [hulls,
          # Each hull's back: a plate over its top, the darker hull under it.
          plate(hulls, lambda q: both(Z(q) - (H_TOP(X(q)) + H_KEEL(X(q))) / 2 - 0.04, X(q) + 1.08, 0.1 - X(q)), 0.014,
                'shell', name='backs'),
          plate(hulls, lambda q: STERN + 0.26 - X(q), 0.012, 'iron', name='cowls')]
repaint(hulls, 'shell_deep', where=lambda q: Z(q) < (H_TOP(X(q)) + H_KEEL(X(q))) / 2 + 0.03, of='shell')

# The deck between them.
deck = loft([oval(x, 0, DECK[2] - 0.06, H_OUT(x) - H_WIDE(x) * 0.5, 0.06, 0.07, n=12, p=5.0)
             for x in np.linspace(DECK[0], DECK[1], 5)], 'iron', ends=('flat', 'flat'), levels=1, name='deck')
parts.append(deck)
# Ribs from the saddle down to each hull, over the deck.
for x in RIBS:
    for side in (1, -1):
        parts.append(stem(bow((x, side * 0.1, 0.62), (x - 0.03, side * 0.3, 0.6), (x - 0.06, side * 0.52, 0.5), steps=5),
                          [0.034, 0.032, 0.03, 0.03, 0.032, 0.036], n=8, material='iron', name='rib'))

# The saddle and the cowl before it: one hump down the deck's middle.
rings = [oval(x, 0, DECK[2] - 0.06, S_WIDE(x), S_TOP(x) - DECK[2] + 0.06, 0.05, n=14, p=2.3) for x in S_AT]
saddle = solid(loft(rings, 'shell', name='saddle'), 'saddle')
pad = lambda q: both(X(q) - PAD[0], PAD[1] - X(q), Z(q) - 0.56)
seam(saddle, pad, 0.024, 0.01)
repaint(saddle, 'pad', where=lambda q: pad(q) > 0.012)
# The cowl: a visor of dark glass before the rider, a seam across its
# brow, and a slit of light each side of its nose.
visor = lambda q: both(X(q) - 0.04, plane((0.5, 0, 0.78), (-0.5, 0, 1))(q), 0.23 - Y(q))
seam(saddle, visor, 0.022, 0.009)
repaint(saddle, 'glass', where=lambda q: visor(q) > 0.011)
seam(saddle, plane((0.66, 0, 0), (1, 0, 0.6)), **SEAM, where=lambda q: X(q) > 0.3)
for side in (1, -1):
    seam(saddle, plane((0.5, 0, 0.63), (-0.42, 0, 1)), 0.035, 0.011, **GLOW,
         where=lambda q: (X(q) > 0.5) & (X(q) < 0.93) & (side * q[:, 1] > 0.035))
parts.append(saddle)

# The horns: from the cowl's shoulders up, out and back to the rider's
# hands, a grip turned in at the end of each.
for side in (1, -1):
    path = bow(*[(x, side * y, z) for x, y, z in HORN], steps=7)
    radii = [HORN_R[0] + (HORN_R[1] - HORN_R[0]) * (i / 7) ** 0.7 for i in range(8)]
    parts.append(stem(path, radii, n=10, material='shell_deep', name='horn', ends=('tip', 'tip')))
    grip = cyl(GRIP_R, 0.2, axis='y', at=(GRIP[0], side * (GRIP[1] + 0.005), GRIP[2]), material=None, verts=14)
    grip.data.materials.append(skin('pad'))
    knob = sphere(HORN_R[1] * 1.25, at=(GRIP[0], side * 0.385, GRIP[2]), material=None, segments=16)
    knob.data.materials.append(skin('shell_deep'))
    cap = sphere(GRIP_R * 1.3, at=(GRIP[0], side * (GRIP[1] - 0.1), GRIP[2]), material=None, segments=12)
    cap.data.materials.append(skin('light'))
    parts += [grip, knob, cap]

# The fin, a light down its trailing edge.
(f0, f1), (t0, t1), z0, z1 = FIN
rings = []
for k in (0.0, 0.3, 0.6, 0.85, 1.0):
    rings.append(foil(0, f0 + (t0 - f0) * k, f1 + (t1 - f1) * k, z0 + (z1 - z0) * k, 0.09 * (1 - 0.75 * k), n=12, under=1.0,
                      upright=True))
fin = solid(loft(rings, 'shell', ends=('flat', 'tip'), name='fin'), 'fin')
aft = lambda q: X(q) - (f1 + (t1 - f1) * (Z(q) - z0) / (z1 - z0))
repaint(fin, 'light', where=lambda q: (aft(q) < 0.035) & (Z(q) > 0.56))
parts.append(fin)

# A gun in each prow, a ring of light behind its muzzle.
for side in (1, -1):
    y, z = side * GUNS[1], GUNS[2]
    barrel = cyl(0.026, 0.24, at=(GUNS[0] - 0.1, y, z), material=None, verts=14)
    barrel.data.materials.append(skin('iron'))
    ring = cyl(0.038, 0.04, at=(GUNS[0] - 0.06, y, z), material=None, verts=14)
    ring.data.materials.append(skin('light'))
    parts += [barrel, ring]

o = solid(parts, 'skimmer')
smooth(o, 38)
shade(o)

r = sim('BODY_R')
eye = Vector((SEAT[0], 0, SEAT[1] + sim('CROUCH_EYE')))
deliver(
    'skimmer', o,
    steel=[((-HL, -HW, 0), (HL, HW, SILL))],
    rider=[((SEAT[0] - r, -r, SEAT[1]), (SEAT[0] + r, r, SEAT[1] + sim('CROUCH_H')))],
    marks=[(GUNS[0], side * GUNS[1], GUNS[2]) for side in (1, -1)],
    chase=(eye + Vector((-cam('SKIM_CAM_BACK_M'), 0, cam('SKIM_CAM_UP_M'))), eye + Vector((cam('CRAFT_CAM_MEET_M'), 0, 0))),
    burns=burns,
    note='Veil skimmer: a catamaran of a hover bike, two blade hulls toed in like mandibles, a gun in each prow, an '
         "engine's mouth in each stern, horns back to its rider's hands")

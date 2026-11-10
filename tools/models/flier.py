"""The flier (look/craft.rs, `sim::vehicle::FLIER`): a one-seat attack craft
of the Veil's, our own design, grown here by rule (veil.py) on craft.py's
kit: a swift. A low broad body with a hawk's beak of a nose, a long dark
glass blister over its pilot running back into a spine, scythe wings swept
back to points with a feathered trailing edge, a lance through each wing's
leading edge, a forked tail with its engine's mouth between the prongs.
Its seams are cut by rule, its lights are the gaps between its plates, and
its shade is traced into its colours: it carries no map.

    tools/craft.sh flier          the loop: this, then the game's views of it
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from craft import *
from veil import *

# Nose to tail, metres: the sim's hull (2 × FLY_HALF_L).
LENGTH = 2 * sim('FLY_HALF_L')
HL, HW, BW = sim('FLY_HALF_L'), sim('FLY_HALF_W'), sim('FLY_BODY_HALF_W')
WY, WT, TOP = sim('FLY_WING_Y'), sim('FLY_WING_T'), sim('FLY_H')
# How far its belly hangs over the ground at rest, metres.
BELLY = 0.2
# Where the sim seats its pilot (`Vehicle::mount`: metres ahead of the
# hull's middle, and the feet over the ground) and where its bolts leave
# (`Vehicle::gun_at`: ahead, to each side, over the wing's underside).
SEAT = (0.4, 0.15)
GUNS = (1.1, 1.9, WY + 0.15)

# The body, tail to nose: its top line, its keel, its half-width and the
# height its sides are widest at (its chine, a crease the wings leave it by).
TAIL = -1.72
B_TOP = curve((TAIL, 0.86), (-1.3, 0.94), (-0.8, 1.0), (-0.2, 1.02), (0.5, 0.98), (1.0, 0.9), (1.5, 0.77), (1.95, 0.62),
              (2.3, 0.49))
B_KEEL = curve((TAIL, 0.52), (-1.3, 0.36), (-0.8, 0.25), (-0.2, BELLY), (0.6, BELLY), (1.2, 0.27), (1.7, 0.36), (2.1, 0.43),
               (2.3, 0.46))
B_WIDE = curve((TAIL, 0.5), (-1.3, 0.72), (-0.8, 0.86), (-0.2, BW), (0.4, 0.86), (0.9, 0.7), (1.4, 0.46), (1.9, 0.2),
               (2.3, 0.02))
B_CHINE = curve((TAIL, 0.7), (0.4, 0.7), (1.5, 0.6), (2.3, 0.475))
B_AT = (TAIL, -1.5, -1.2, -0.8, -0.3, 0.2, 0.6, 1.0, 1.35, 1.65, 1.95, 2.15, 2.28)
B_N = 20
# Its engine's mouth, in its tail's end: how much of the tail's section it
# is, how deep, and the vanes across it; the cowl of iron round it runs
# this far forward.
MOUTH = 0.84
MOUTH_DEEP = 0.26
VANES = 2
# Its core: how far in from the tail's end its middle is, how long it is,
# and what share of the mouth's width and height.
CORE = (0.17, 0.14, 0.8, 0.72)
COWL = -1.46
# The blister over the pilot and the spine it runs back into: its
# half-width and its crest, tail to nose; glass ahead of `GLASS`.
C_WIDE = curve((-1.47, 0.03), (-1.2, 0.11), (-0.6, 0.26), (0.0, 0.4), (0.5, 0.43), (1.0, 0.36), (1.4, 0.22), (1.72, 0.03))
C_TOP = curve((-1.47, 0.97), (-1.2, 1.08), (-0.6, 1.3), (0.0, 1.46), (0.35, TOP), (0.8, 1.4), (1.2, 1.18), (1.5, 0.95),
              (1.72, 0.74))
C_AT = (-1.45, -1.2, -0.9, -0.5, -0.1, 0.3, 0.7, 1.05, 1.35, 1.58, 1.7)
GLASS = -0.2
# A wing, root to tip: its leading and trailing edges (x at each span), the
# height of its middle and its thickness. A scythe: broad where it leaves
# the body, swept back to a point, its trailing edge in two lobes and the
# tip, as a wing's feathers end.
W_LE = curve((0.5, 0.98), (0.95, 0.70), (1.4, 0.40), (1.85, 0.0), (2.25, -0.52), (2.5, -0.97), (2.6, -1.2))
W_TE = curve((0.5, -1.45), (0.95, -1.37), (1.2, -1.2), (1.45, -1.36), (1.7, -1.08), (2.0, -1.3), (2.25, -1.12),
             (2.45, -1.26), (2.6, -1.32))
W_Z = curve((0.5, 0.72), (1.4, 0.72), (2.2, 0.69), (2.6, 0.64))
W_T = curve((0.5, 0.15), (0.95, 0.12), (1.6, 0.075), (2.3, 0.04), (2.6, 0.012))
W_AT = (0.5, 0.95, 1.2, 1.45, 1.7, 2.0, 2.25, 2.45, 2.59)
# The sheath along a wing's leading edge, this deep; the line of light
# before its trailing edge, this far ahead of it.
SHEATH = 0.2
EDGE = 0.16
# The feathers' shafts: seams from each lobe's end toward the wing's root
# (a point each lies through, and the way it runs).
SHAFTS = (((-1.36, 1.45), (1.96, -0.55)), ((-1.3, 2.0), (1.6, -0.55)))
# The tail's prongs: from where to where, and how far out.
PRONG = (-0.9, -2.28, 0.6)

scene()
SEAM = dict(width=0.03, depth=0.013)
GLOW = dict(floor='light', wall='iron', flat=0.45)
X = lambda q: q[:, 0]
Y = lambda q: np.abs(q[:, 1])
Z = lambda q: q[:, 2]
both = lambda *r: np.minimum.reduce(r)


# The body: a skin over its sections, open at the tail, its rim turned in
# to be its engine's mouth.
MZ = (B_TOP(TAIL) + B_KEEL(TAIL)) / 2
MW, MH = B_WIDE(TAIL) * MOUTH, (B_TOP(TAIL) - MZ) * MOUTH
rings = [oval(x, 0, B_CHINE(x), B_WIDE(x), B_TOP(x) - B_CHINE(x), B_CHINE(x) - B_KEEL(x), n=B_N, p=2.0, pb=2.6) for x in B_AT]
body = solid(loft(lip((TAIL, 0, MZ), MW, MH, n=B_N) + rings, 'shell', ends=('open', 'tip'), name='body',
                  sharp=(0, B_N // 2)), 'body')
top = lambda q: Z(q) > B_CHINE(X(q)) + 0.04
under = lambda q: Z(q) < B_CHINE(X(q)) - 0.12
# The beak's plate, the collar behind the blister, the shoulders.
seam(body, plane((1.7, 0, 0), (1, 0, 0.3)), **SEAM)
seam(body, plane((-0.95, 0, 0), (1, 0, -0.3)), **SEAM, where=lambda q: X(q) > COWL + 0.1)
for side in (1, -1):
    seam(body, plane((0, side * 0.6, 0), (0.2, side, 0)), **SEAM,
         where=lambda q: top(q) & (X(q) < 1.25) & (X(q) > COWL) & (side * q[:, 1] > 0.3))
    # Its eyes: a slit of light each side of the beak.
    seam(body, plane((1.7, 0, 0.665), (0.3, 0, 1)), 0.04, 0.012, **GLOW,
         where=lambda q: (X(q) > 1.38) & (X(q) < 1.98) & (side * q[:, 1] > 0.05))
# Gills: three slits a shoulder.
for x in (-1.08, -1.2, -1.32):
    seam(body, plane((x, 0, 0.8), (1, 0, 0.5)), 0.028, 0.014,
         where=lambda q: top(q) & (Y(q) > 0.42) & (Y(q) < B_WIDE(X(q)) - 0.1))
# Under it: the keel's line of light, and a plate of light under each wing's
# root, where it is lifted.
seam(body, plane((0, 0, 0), (0, 1, 0)), 0.04, 0.012, **GLOW, where=lambda q: under(q) & (X(q) > -1.3) & (X(q) < 1.15))
seam(body, plane((0.9, 0, 0), (1, 0, 0)), **SEAM, where=under)
for side in (1, -1):
    lift = ellipse(-0.3, side * 0.47, 0.62, 0.17)
    seam(body, lift, 0.03, 0.012, where=under)
    repaint(body, 'light_dim', where=lambda q: under(q) & (lift(q) > 0.016))
# The blister's seat, a darker plate round its foot, and the engine's cowl.
seat = lambda q: both(C_WIDE(X(q)) + 0.085 - Y(q), 1.8 - X(q), X(q) - COWL - 0.02, Z(q) - B_CHINE(X(q)) - 0.05)
parts = [body, plate(body, seat, 0.014, 'shell_deep', name='seat'),
         plate(body, lambda q: COWL - X(q), 0.012, 'iron', name='cowl')]

parts += mouth((TAIL, 0, MZ), MW, MH, MOUTH_DEEP, CORE, VANES, n=B_N)

# The blister and its spine, sunk a little into the body's top: glass
# ahead, shell behind, a line of light down its crest.
rings = []
for x in C_AT:
    base = B_TOP(x) - 0.1
    rings.append(oval(x, 0, base, C_WIDE(x), max(C_TOP(x) - base, 0.03), 0.06, n=16, p=2.2))
canopy = solid(loft(rings, 'shell', name='canopy'), 'canopy')
rake = plane((GLASS, 0, 1.2), (1, 0, -0.5))
repaint(canopy, 'glass', where=lambda q: rake(q) > 0)
seam(canopy, rake, 0.04, 0.012)
seam(canopy, plane((0, 0, 0), (0, 1, 0)), 0.05, 0.014, **GLOW,
     where=lambda q: (Z(q) > C_TOP(X(q)) - 0.06) & (X(q) > -1.36) & (rake(q) < -0.12))
parts.append(canopy)

# The wings.
wings = solid([loft([foil(side * y, W_LE(y), W_TE(y), W_Z(y), W_T(y), n=14) for y in W_AT], 'shell', name='wing')
               for side in (1, -1)], 'wings')
span = lambda q: (Y(q) > 1.0) & (Y(q) < 2.5)
fore = beside(W_LE, -SHEATH)
seam(wings, beside(W_TE, EDGE), 0.05, 0.012, **GLOW, where=span)
for (px, py), (dx, dy) in SHAFTS:
    for side in (1, -1):
        seam(wings, plane((px, side * py, 0), (dy, -side * dx, 0)), 0.026, 0.01,
             where=lambda q: (side * q[:, 1] > 0.98) & (fore(q) < -0.03) & (X(q) > W_TE(Y(q)) + EDGE + 0.05))
seam(wings, fore, 0.026, 0.01, where=span)
repaint(wings, 'shell_deep', where=lambda q: span(q) & (fore(q) > 0.014))
repaint(wings, 'light', where=lambda q: Y(q) > 2.535)
parts.append(wings)


def lance(side):
    """A gun's fairing through the wing's leading edge, its barrel, and the
    ring of light behind its muzzle."""
    y, z = side * GUNS[1], GUNS[2]
    r = curve((-0.75, 0.02), (-0.4, 0.075), (0.0, 0.1), (0.4, 0.09), (0.66, 0.062), (0.74, 0.05))
    pod = loft([oval(x, y, z, r(x), r(x), n=12, p=2.0) for x in (-0.75, -0.4, 0.0, 0.4, 0.66, 0.74)], 'shell_deep',
               ends=('tip', 'flat'), name='lance')
    barrel = cyl(0.03, GUNS[0] - 0.7, at=((GUNS[0] + 0.7) / 2, y, z), material=None, verts=16)
    barrel.data.materials.append(skin('iron'))
    ring = cyl(0.045, 0.05, at=(GUNS[0] - 0.12, y, z), material=None, verts=16)
    ring.data.materials.append(skin('light'))
    return [pod, barrel, ring]


def prong(side):
    """A blade of the forked tail, lifting and spreading as it goes aft."""
    x0, x1, y = PRONG
    rings = []
    for x in np.linspace(x0, x1, 7):
        k = (x - x0) / (x1 - x0)
        rings.append(oval(x, side * (y + 0.14 * k), 0.72 + 0.16 * k ** 1.5, 0.18 * (1 - k) ** 0.7 + 0.015,
                          0.075 * (1 - k) ** 0.6 + 0.012, n=12, p=2.2))
    o = solid(loft(rings, 'shell', name='prong', sharp=(0, 6)), 'prong')
    return repaint(o, 'light', where=lambda q: X(q) < x1 + 0.09)


for side in (1, -1):
    parts += [prong(side)] + lance(side)

o = solid(parts, 'flier')
smooth(o, 38)
shade(o)

r = sim('BODY_R')
eye = Vector((SEAT[0], 0, SEAT[1] + sim('CROUCH_EYE')))
deliver(
    'flier', o,
    # `Vehicle::hull`: its fuselage, and its wings' slab.
    steel=[((-HL, -BW, 0), (HL, BW, TOP)), ((-HL * 0.6, -HW, WY), (HL * 0.3, HW, WY + WT))],
    rider=[((SEAT[0] - r, -r, SEAT[1]), (SEAT[0] + r, r, SEAT[1] + sim('CROUCH_H')))],
    marks=[(GUNS[0], side * GUNS[1], GUNS[2]) for side in (1, -1)],
    chase=(eye + Vector((-cam('FLY_CAM_BACK_M'), 0, cam('FLY_CAM_UP_M'))), eye + Vector((cam('CRAFT_CAM_MEET_M'), 0, 0))),
    # Its mouth's two halves and its middle, for the boost's flames.
    burns=[(TAIL, MW * 0.5, MZ), (TAIL, -MW * 0.5, MZ), (TAIL, 0.0, MZ)],
    note='Veil flier: a swift, scythe wings and a forked tail, a lance through each wing, its engine a mouth between the prongs')

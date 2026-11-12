"""The flier (look/craft.rs, `sim::vehicle::FLIER`): a one-seat attack craft
of the Veil's, our own design, grown here by rule (veil.py) on craft.py's
kit: a swift in its stoop. A low broad body with a hawk's bill of a nose,
keeled and hooked, louvres down each flank of it; two panes of dark glass
over its pilot either side of a crest that carries one hairline of light
from bill to tail; two long blades of wings held down and back, each one
plane from a broad root to a point, plated along its spar, hung from a
hinge's bare barrel in the body's flank and braced by two jacks under it,
a pod with a lamp at its tip; a lance out of each wing's leading edge; and
one tall fin on its back, a shark's, swept from behind its glass to a lamp
at its tip over the engine's mouth, vaned, in its tail: from astern three
blades, the fin up and the wings down and out. It is a machine and is lit
like one: its engine, its lift's lenses, a lamp at each wing's tip and at
its fin's, and one hairline down its crest; nothing drawn on it in light. Its
plates lap like a wing case and are fastened down, its seams are cut by rule
with dark iron in them, and its shade is traced into its colours: it carries
no map.

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
# Its bill is deep and keeled, and hooks down past its tip.
TAIL = -1.72
B_TOP = curve((TAIL, 0.86), (-1.3, 0.94), (-0.8, 1.0), (-0.2, 1.02), (0.5, 0.98), (1.0, 0.92), (1.5, 0.84), (1.95, 0.74),
              (2.2, 0.60), (2.3, 0.42))
B_KEEL = curve((TAIL, 0.52), (-1.3, 0.36), (-0.8, 0.25), (-0.2, BELLY), (0.6, BELLY), (1.2, 0.26), (1.7, 0.32), (2.1, 0.35),
               (2.3, 0.38))
B_WIDE = curve((TAIL, 0.5), (-1.3, 0.72), (-0.8, 0.86), (-0.2, BW), (0.4, 0.86), (0.9, 0.72), (1.4, 0.56), (1.9, 0.34),
               (2.15, 0.15), (2.3, 0.02))
B_CHINE = curve((TAIL, 0.7), (0.4, 0.7), (1.5, 0.6), (2.3, 0.39))
B_AT = (TAIL, -1.5, -1.2, -0.8, -0.3, 0.2, 0.6, 1.0, 1.35, 1.65, 1.95, 2.15, 2.27)
B_N = 20
# How round its back is (2 an ellipse), and how keeled its bill, from
# `BILL` forward (under 2 a ridge): the bill is its own plates from there,
# a paler upper mandible and a deeper lower.
B_ROUND = curve((1.0, 2.0), (1.6, 1.5))
BILL = 1.28
# Its engine's mouth, in its tail's end: how much of the tail's section it
# is, how deep, and the vanes across it; the cowl of iron round it runs
# this far forward.
MOUTH = 0.84
MOUTH_DEEP = 0.26
# (No upright ribs: a row of them with the fire behind was a grin, and two
# a window's bars.)
VANES = 3
RIBS = 0
# Its core: how far in from the tail's end its middle is, how long it is,
# and what share of the mouth's width and height.
CORE = (0.2, 0.1, 0.6, 0.5)
COWL = -1.46
# The blister over the pilot and the spine it runs back into: its
# half-width and its crest, tail to nose; glass ahead of `GLASS`, and a
# crest of shell this wide over the glass to `CREST`.
C_WIDE = curve((-1.47, 0.03), (-1.2, 0.11), (-0.6, 0.24), (0.0, 0.31), (0.5, 0.33), (1.0, 0.28), (1.4, 0.19), (1.72, 0.03))
C_TOP = curve((-1.47, 0.97), (-1.2, 1.08), (-0.6, 1.3), (0.0, 1.46), (0.35, TOP), (0.8, 1.4), (1.2, 1.18), (1.5, 0.98),
              (1.72, 0.8))
C_AT = (-1.45, -1.2, -0.9, -0.5, -0.1, 0.3, 0.7, 1.05, 1.35, 1.58, 1.7)
GLASS = -0.2
CREST = (0.055, 1.52)
# The bill's louvres, down each flank of its upper mandible: where along
# the craft.
LOUVRES = (1.42, 1.53, 1.64)
# A wing, root to tip: its leading and trailing edges (x at each span), the
# height of its middle and its thickness. One long blade, a narrow triangle
# held down and back, twice as long as it is wide at its root: both its edges
# swept, a ridge down its back ahead of its spar (broad and flat it was a
# delta: a hairline edge on and one pale sheet face on), drooping from the body's shoulder to a point a hand over where its belly
# rides. (It was a sickle with seven feathers stepped down its trailing
# edge: a bird's wing, and from astern fourteen paddles.) It is in the sim's
# wing slab to the lance, and hangs under it from there out.
W_LE = curve((0.5, 0.6), (0.9, 0.52), (1.9, -0.22), (2.54, -0.86))
W_TE = curve((0.5, -0.42), (0.9, -0.48), (1.9, -0.92), (2.54, -1.08))
W_Z = curve((0.5, 0.86), (0.9, 0.82), (1.9, 0.55), (2.54, 0.27))
W_T = curve((0.5, 0.2), (0.9, 0.19), (1.9, 0.13), (2.54, 0.065))
W_AT = (0.5, 0.9, 1.3, 1.7, 2.1, 2.36, 2.54)
# Its spar and its flap's hinge: the shares of its chord they lie at (0 its
# trailing edge), and the spans its two cross seams lie at.
SPAR, FLAP = 0.5, 0.22
RIBS_AT = (1.42, 1.95)
# The hinge it hangs from: a bare barrel along the body's flank (where along
# the craft its middle is, how long, how far out, how high, how thick), and
# the jacks under the wing: where each leaves the body (along the craft) and
# the span it meets the wing at.
HINGE = (0.02, 0.96, 0.88, 0.83, 0.1)
JACKS = ((0.22, 1.3), (-0.22, 1.2))
# The pod at its tip: how far it runs ahead of the tip's leading edge and
# behind its trailing edge, and how thick.
POD = (0.2, 0.12, 0.052)
# The fin on its back, foot to tip: its leading and trailing edges (x at
# each height) and its thickness. A shark's: its leading edge swept from just
# behind the glass, its trailing edge hollow, its tip over the tail's end and
# as far over the blister as the blister is over the wings. Its foot is sunk
# in the blister's spine. (It had a forked tail of two small blades there:
# two stubs beside the mouth from the saddle, and nothing at all at range.)
F_LE = curve((1.0, -0.08), (1.5, -0.6), (2.0, -1.26), (2.4, -1.84))
F_TE = curve((1.0, -1.5), (1.5, -1.58), (2.0, -1.78), (2.4, -2.02))
F_T = curve((1.0, 0.11), (1.6, 0.08), (2.4, 0.03))
F_AT = (1.0, 1.25, 1.5, 1.75, 2.0, 2.22, 2.4)
# Its spar and its rudder's hinge (shares of its chord, 0 its trailing
# edge), the heights its two cross seams lie at, and from how high it is lit.
F_SPAR, F_RUDDER = 0.55, 0.28
F_RIBS = (1.72, 2.08)
F_LIT = 2.3
# The hoop of its frame over the glass: where along the craft.
HOOP = 0.98
# The lift's lenses under the wings' roots: where along the craft, how far
# out, how long and wide, and how far their middles are sunk into the belly;
# the port under the chin its bomb leaves by the same (on the centre line).
LIFT = (-0.3, 0.47, 0.6, 0.11, 0.03)
PORT = (1.45, 0.34, 0.11, 0.028)

scene()
# A seam between two plates, with dark iron in it; a hairline; a seam with
# light in it.
GAP = dict(width=0.055, depth=0.028, flat=0.5)
SEAM = dict(width=0.03, depth=0.013)
GLOW = dict(floor='light', wall='iron', flat=0.45)
X = lambda q: q[:, 0]
Y = lambda q: np.abs(q[:, 1])
Z = lambda q: q[:, 2]
both = lambda *r: np.minimum.reduce(r)
run = lambda a, b: (lambda q: np.minimum(X(q) - a, b - X(q)))

# The body: a skin over its sections, open at the tail, its rim turned in
# to be its engine's mouth; its chine a crease, and its bill's ridge.
MZ = (B_TOP(TAIL) + B_KEEL(TAIL)) / 2
MW, MH = B_WIDE(TAIL) * MOUTH, (B_TOP(TAIL) - MZ) * MOUTH
rings = [oval(x, 0, B_CHINE(x), B_WIDE(x), B_TOP(x) - B_CHINE(x), B_CHINE(x) - B_KEEL(x), n=B_N, p=B_ROUND(x), pb=2.6)
         for x in B_AT]
bill = 2 + next(i for i, x in enumerate(B_AT) if x >= BILL)
body = solid(loft(lip((TAIL, 0, MZ), MW, MH, n=B_N) + rings, 'shell', ends=('open', 'tip'), name='body',
                  sharp=(0, B_N // 2, (B_N // 4, bill, 99))), 'body')
# Rules for where a seam runs, each metres inside its edge: over the
# chine, under it.
top = lambda q: Z(q) - B_CHINE(X(q)) - 0.04
under = lambda q: B_CHINE(X(q)) - 0.12 - Z(q)
# The bill's plate, the collar behind the blister, the shoulders: gaps with
# iron in them.
beak = plane((BILL, 0, 0), (1, 0, 0.3))
gape = lambda q: Z(q) - B_CHINE(X(q)) + 0.035
collar = plane((-0.95, 0, 0), (1, 0, -0.3))
shoulder = {side: plane((0, side * 0.6, 0), (0.2, side, 0)) for side in (1, -1)}
seam(body, beak, **GAP)
# The bill's gape: a gap of iron along its chine, the upper mandible over it
# and the lower under.
seam(body, gape, 0.05, 0.026, flat=0.5, within=run(BILL + 0.08, 2.2))
seam(body, collar, **GAP, where=lambda q: X(q) > COWL + 0.1)
for side in (1, -1):
    seam(body, shoulder[side], **GAP, within=lambda q: both(top(q), run(COWL, 1.25)(q), side * q[:, 1] - 0.3))
# Gills: three louvres a side across its back, from the blister's foot out
# to the shoulder, the engine's fire in them, walls and all (the chase
# camera sees their forward walls, not their floors).
for x in (-1.06, -1.2, -1.34):
    seam(body, plane((x, 0, 0.8), (1, 0, 0.5)), 0.075, 0.02, floor='vein', wall='iron', flat=0.45,
         within=lambda q: both(top(q), Y(q) - C_WIDE(X(q)) - 0.13, B_WIDE(X(q)) - 0.12 - Y(q)))
# Louvres down each flank of the bill's upper mandible, raked as its plate's
# seam is. (It had an eye of light there each side: a face.)
for x in LOUVRES:
    seam(body, plane((x, 0, 0.6), (1, 0, 0.3)), 0.034, 0.014, flat=0.5,
         within=lambda q: both(gape(q) - 0.07, Y(q) - 0.1, B_TOP(X(q)) - 0.09 - Z(q)))
# Hairlines across its back, where its panels come off.
for x in (-0.5, 0.32):
    seam(body, plane((x, 0, 0), (1, 0, 0)), **SEAM, where=lambda q: (top(q) > 0) & (Y(q) > C_WIDE(X(q)) + 0.1))
# And one across the bill: its upper mandible is two plates.
seam(body, plane((1.8, 0, 0.6), (1, 0, 0.3)), **SEAM, where=lambda q: gape(q) > 0.04)
# Under it: the keel's hairline of light and a seam across.
seam(body, plane((0, 0, 0), (0, 1, 0)), 0.022, 0.012, **GLOW, within=lambda q: both(under(q), run(-1.3, 1.15)(q)))
seam(body, plane((0.9, 0, 0), (1, 0, 0)), **SEAM, where=lambda q: under(q) > 0)
# A channel of dark iron along each flank under the wing's root.
seam(body, lambda q: Z(q) - (B_CHINE(X(q)) - 0.1), 0.075, 0.022, flat=0.6,
     within=lambda q: both(run(-1.25, 0.75)(q), Y(q) - 0.45))
# Its plates, lapping like a wing case: the blister's seat, a darker plate
# round its foot; a darker shoulder each side; the mantle over its back,
# proud of them and of the engine's cowl it runs back over; the cowl, iron.
foot = lambda q: C_WIDE(X(q)) + 0.085 - Y(q)
seat = lambda q: both(foot(q), 0.2 - foot(q), 1.8 - X(q), X(q) - COWL - 0.02, top(q) - 0.01)
parts = [plate(body, seat, 0.014, 'shell_deep', name='seat', clear=None)]
for side in (1, -1):
    parts.append(plate(body, lambda q: both(top(q), shoulder[side](q) - 0.03, run(COWL + 0.04, 1.2)(q)), 0.02,
                       'shell_deep', name='shoulder'))
parts.append(plate(body, lambda q: both(top(q), -shoulder[1](q) - 0.03, -shoulder[-1](q) - 0.03, -collar(q) - 0.03,
                                        X(q) - COWL + 0.1, 0.06 - foot(q)), 0.036, 'shell', name='mantle'))
parts.append(plate(body, lambda q: COWL - X(q), 0.012, 'iron', name='cowl'))
# What of the body lies under the blister is never seen: taken off.
hidden = lambda q: both(foot(q) - 0.2, run(-1.3, 1.5)(q), top(q))
bm = bmesh.new()
bm.from_mesh(body.data)
bmesh.ops.delete(bm, geom=[f for f in bm.faces if hidden(np.array([f.calc_center_median()[:]]))[0] > 0], context='FACES')
bm.to_mesh(body.data)
bm.free()
repaint(body, 'shell_pale', where=lambda q: (beak(q) > 0.03) & (gape(q) > 0), of='shell')
repaint(body, 'shell_deep', where=lambda q: (beak(q) > 0.03) & (gape(q) < 0), of='shell')
parts.append(body)

parts += mouth((TAIL, 0, MZ), MW, MH, MOUTH_DEEP, CORE, VANES, n=B_N, ribs=RIBS, wall='iron')
# The lenses under it: the lift's, a grille of light under each wing's
# root, and the port under the chin its bomb leaves by.
for side in (1, -1):
    parts += lens((LIFT[0], side * LIFT[1], B_KEEL(LIFT[0]) + LIFT[4]), LIFT[2], LIFT[3], 0.05, vanes=6)
parts += lens((PORT[0], 0, B_KEEL(PORT[0]) + PORT[3]), PORT[1], PORT[2], 0.04, paint='ember')

# The blister and its spine, sunk a little into the body's top: glass
# ahead, shell behind; one line of light down its crest from bill to tail,
# over the glass in a crest of shell.
rings = []
for x in C_AT:
    base = B_TOP(x) - 0.1
    rings.append(oval(x, 0, base, C_WIDE(x), max(C_TOP(x) - base, 0.03), 0.06, n=16, p=1.8))
canopy = solid(loft(rings, 'shell', name='canopy'), 'canopy')
rake = plane((GLASS, 0, 1.2), (1, 0, -0.5))
repaint(canopy, 'glass', where=lambda q: rake(q) > 0)
seam(canopy, rake, 0.04, 0.012)
seam(canopy, plane((0, 0, 0), (0, 1, 0)), 0.024, 0.012, **GLOW,
     within=lambda q: both(Z(q) - C_TOP(X(q)) + 0.07, X(q) + 1.36, CREST[1] - 0.04 - X(q)))
parts += [canopy,
          plate(canopy, lambda q: both(CREST[0] - Y(q), rake(q) + 0.06, CREST[1] - X(q), Z(q) - C_TOP(X(q)) + 0.12), 0.012,
                'shell_deep', name='crest', clear=None),
          plate(canopy, lambda q: both(0.03 - np.abs(X(q) - HOOP), Z(q) - B_TOP(X(q)) - 0.02), 0.009, 'shell_deep', name='hoop',
                clear=None)]

# The wings: each one long blade, creased down its two edges, its plates
# cut by rule: a gap along its spar, a hairline at its flap's hinge and two
# across it; a paler strake proud of it ahead of the spar, the flap a tone
# of its own.
chord = lambda q: W_LE(Y(q)) - W_TE(Y(q))
share = lambda q: (X(q) - W_TE(Y(q))) / chord(q)
spar = lambda q: (share(q) - SPAR) * chord(q)
flap = lambda q: (share(q) - FLAP) * chord(q)


def wing(side):
    n = 12
    o = solid(loft([foil(side * y, W_LE(y), W_TE(y), W_Z(y), W_T(y), n=n, under=0.8) for y in W_AT], 'shell_deep',
                   ends=('flat', 'tip'), name='wing', sharp=(0, 2, n // 2)), 'wing')
    seam(o, spar, 0.04, 0.014, flat=0.5, within=lambda q: both(Y(q) - 0.98, 2.05 - Y(q)))
    seam(o, flap, **SEAM, within=lambda q: both(Y(q) - 1.0, RIBS_AT[1] - Y(q)))
    for y in RIBS_AT:
        seam(o, lambda q, y=y: Y(q) - y, **SEAM)
    strake = plate(o, lambda q: both(spar(q) - 0.04, Y(q) - 1.0, 2.2 - Y(q)), 0.016, 'shell_pale', name='strake', clear=None)
    repaint(o, 'shell', where=lambda q: (flap(q) < -0.02) & (Y(q) > 1.02) & (Y(q) < RIBS_AT[1] - 0.02), of='shell_deep')
    out = [o, strake]
    # The pod at its tip, and its lamp, looking aft.
    y, z = side * W_AT[-1], W_Z(W_AT[-1])
    x0, x1 = W_LE(W_AT[-1]) + POD[0], W_TE(W_AT[-1]) - POD[1]
    out.append(stem([(x0, y, z), (x0 - 0.12, y, z), (x1 + 0.1, y, z), (x1, y, z)], [0.01, POD[2] * 0.85, POD[2], POD[2] * 0.7], n=8,
                    material='iron', name='pod', ends=('tip', 'flat')))
    lamp = cyl(POD[2] * 0.62, 0.04, at=(x1 - 0.012, y, z), material=None, verts=10, bevel=0.003)
    lamp.data.materials.append(skin('light'))
    out.append(lamp)
    # The hinge it hangs from: a barrel of bare alloy in the body's flank,
    # two collars of iron round it.
    hx, long, hy, hz, r = HINGE
    barrel = cyl(r, long, at=(hx, side * hy, hz), material=None, verts=14, bevel=0.004)
    barrel.data.materials.append(skin('alloy'))
    out.append(barrel)
    for k in (-0.36, 0.36):
        collar = cyl(r * 1.12, 0.06, at=(hx + long * k, side * hy, hz), material=None, verts=14, bevel=0.004)
        collar.data.materials.append(skin('iron'))
        out.append(collar)
    # The jacks under it: a barrel of iron out of the body's flank under the
    # chine, and the bare ram out of it up to the wing's underside.
    for x, span in JACKS:
        foot = Vector((x, side * (B_WIDE(x) - 0.05), B_CHINE(x) - 0.2))
        head = Vector((x - 0.2, side * span, W_Z(span) - 0.3 * W_T(span)))
        out.append(stem([tuple(foot), tuple(foot.lerp(head, 0.55))], 0.036, n=8, material='iron', name='jack', levels=0))
        out.append(stem([tuple(foot.lerp(head, 0.5)), tuple(head)], 0.018, n=8, material='alloy', name='ram', levels=0))
    return out


for side in (1, -1):
    parts += wing(side)


# A lance: the gun's fairing, a prong grown out of the wing's leading edge in
# the wing's own plane and paint. Along the craft at each station: half its
# width, half its depth, and how much of the wing's droop it lies at (1 at
# its root, in the wing; 0 at its muzzle, level); its middle's height runs
# from the wing's own at the gun's span up to the muzzle the sim fires from.
L_AT = (-0.86, -0.62, -0.34, -0.08, 0.18, 0.44, 0.64, 0.72)
L_WIDE = (0.02, 0.15, 0.19, 0.17, 0.125, 0.085, 0.064, 0.058)
L_DEEP = (0.02, 0.07, 0.086, 0.086, 0.076, 0.064, 0.056, 0.052)
L_ROLL = (1.0, 1.0, 1.0, 0.9, 0.6, 0.25, 0.0, 0.0)


def lance(side):
    """A gun, as part of its wing: a flat prong out of the blade's leading
    edge, creased down its two edges as the blade is and lying at the blade's
    own droop, drawn forward and up to a short barrel in an iron collar and
    the ring of light behind its muzzle. (It was a pod on a pylon over the
    wing: a thing bolted on.)"""
    y, z = side * GUNS[1], GUNS[2]
    n = 12
    droop = math.atan((W_Z(GUNS[1] + 0.05) - W_Z(GUNS[1] - 0.05)) / 0.1) * side
    mid = curve((L_AT[0], W_Z(GUNS[1]) + 0.005), (-0.3, W_Z(GUNS[1]) + 0.01), (0.2, z - 0.07), (0.5, z - 0.008), (L_AT[-1], z))
    rings = []
    for x, w, h, k in zip(L_AT, L_WIDE, L_DEEP, L_ROLL):
        turn = Matrix.Rotation(droop * k, 3, 'X')
        rings.append([turn @ Vector((0.0, q.y, q.z)) + Vector((x, y, mid(x))) for q in oval(0.0, 0.0, 0.0, w, h, n=n, p=2.2)])
    prong = solid(loft(rings, 'shell_deep', ends=('tip', 'flat'), name='lance', levels=1, sharp=(0, n // 2)), 'lance')
    # A gap round it where the barrel's housing starts, and that housing the
    # paler violet, as the wing's strake is.
    seam(prong, plane((0.36, 0, 0), (1, 0, 0)), 0.03, 0.012, flat=0.5)
    repaint(prong, 'shell_pale', where=lambda q: X(q) > 0.38, of='shell_deep')
    out = [prong]
    for at, long, wide, paint in ((0.735, 0.07, 0.066, 'iron'), (0.9, 0.3, 0.04, 'iron'), (GUNS[0] - 0.1, 0.05, 0.058, 'light_dim'),
                                  (GUNS[0] - 0.035, 0.07, 0.047, 'iron')):
        part = cyl(wide, long, at=(at, y, z), material=None, verts=16, bevel=0.004)
        part.data.materials.append(skin(paint))
        out.append(part)
    return out


def fin():
    """The fin on its back: one blade creased down its two edges, cut into
    plates as a wing is (a gap along its spar, a hairline at its rudder's
    hinge, two across it), a paler strake proud of it ahead of the spar, the
    rudder a tone of its own, and its tip's trailing corner a lamp: the
    highest thing on the craft, and where its trail leaves."""
    n = 12
    o = solid(loft([foil(0.0, F_LE(z), F_TE(z), z, F_T(z), n=n, under=1.0, upright=True) for z in F_AT], 'shell_deep',
                   ends=('flat', 'tip'), name='fin', sharp=(0, n // 2)), 'fin')
    span = lambda q: F_LE(Z(q)) - F_TE(Z(q))
    part = lambda q: (X(q) - F_TE(Z(q))) / span(q)
    stay = lambda q: (part(q) - F_SPAR) * span(q)
    hinge = lambda q: (part(q) - F_RUDDER) * span(q)
    seam(o, stay, 0.036, 0.012, flat=0.5, within=lambda q: both(Z(q) - 1.42, F_RIBS[1] + 0.1 - Z(q)))
    seam(o, hinge, **SEAM, within=lambda q: both(Z(q) - 1.42, F_RIBS[1] - Z(q)))
    for z in F_RIBS:
        seam(o, lambda q, z=z: Z(q) - z, **SEAM)
    strake = plate(o, lambda q: both(stay(q) - 0.036, Z(q) - 1.46, F_RIBS[1] + 0.06 - Z(q)), 0.014, 'shell_pale', name='strake',
                   clear=None)
    repaint(o, 'shell', where=lambda q: (hinge(q) < -0.02) & (Z(q) > 1.44) & (Z(q) < F_RIBS[1] - 0.02), of='shell_deep')
    repaint(o, 'light', where=lambda q: (Z(q) > F_LIT) & (part(q) < 0.5))
    return [o, strake]


parts += fin()
for side in (1, -1):
    parts += lance(side)

o = solid(parts, 'flier')
# Its fasteners: a row each side of each shoulder's seam, an arc over its
# back before the engine's cowl, a row down each wing ahead of its spar and
# up each face of its fin.
pins = []
for side in (1, -1):
    for x in np.linspace(-1.3, 1.0, 7):
        for lean in (-0.08, 0.085):
            pins.append((x, side * (0.6 - 0.2 * x + lean), B_TOP(x)))
    for y in np.linspace(1.08, 1.86, 5):
        pins.append((W_TE(y) + (SPAR + 0.09) * (W_LE(y) - W_TE(y)), side * y, W_Z(y) + W_T(y)))
    for z in np.linspace(1.56, 2.0, 4):
        pins.append((F_TE(z) + (F_SPAR + 0.1) * (F_LE(z) - F_TE(z)), side * 0.08, z))
for a in np.linspace(0.12, math.pi - 0.12, 8):
    x = COWL + 0.075
    pins.append((x, 1.1 * B_WIDE(x) * math.cos(a), B_CHINE(x) + 1.1 * (B_TOP(x) - B_CHINE(x)) * math.sin(a)))
o = solid([o, studs(o, pins)], 'flier')


def soot(q):
    """What is left of the paint: soot aft of the gills and round the mouth,
    and the mouth's lip blued by its heat."""
    low = q[:, 2] < 1.1
    aft = np.clip((-1.05 - q[:, 0]) / 0.7, 0.0, 1.0) * (np.abs(q[:, 1]) < 0.5) * low
    left = (1.0 - 0.45 * aft)[:, None] * np.ones((1, 3))
    lip = (np.clip((TAIL + 0.12 - q[:, 0]) / 0.12, 0.0, 1.0) * (np.abs(q[:, 1]) < 0.5) * low)[:, None]
    # And down each wing aft of its lance, where the gun's smoke lies.
    y = np.abs(q[:, 1])
    smoke = np.clip(1.0 - np.abs(y - GUNS[1]) / 0.22, 0.0, 1.0) * np.clip((W_LE(y) + 0.1 - q[:, 0]) / 0.5, 0.0, 1.0) * (y > 1.0)
    left = left * (1.0 - 0.4 * smoke)[:, None]
    return left * (1 - lip) + lip * np.array([1.25, 1.0, 1.45])


shade(o, tone=0.16, stain=soot)
gloss(o)

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
    note="Veil flier: a swift in its stoop, two long blades of wings held down and back and a shark's fin on its back, a lance "
         f"out of each wing, its engine a mouth in its tail; its fin's tip at x {F_TE(F_AT[-1]) + 0.06:.2f}, y 0, z {F_AT[-1]:.2f}")

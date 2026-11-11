"""The flier (look/craft.rs, `sim::vehicle::FLIER`): a one-seat attack craft
of the Veil's, our own design, grown here by rule (veil.py) on craft.py's
kit: a swift. A low broad body with a hawk's bill of a nose, keeled and
hooked, louvres down each flank of it; two panes of dark glass over its
pilot either side of a crest that carries one hairline of light from bill to
tail; sickle wings that leave it narrow, hold forward to a wrist and sweep
back to a point, their trailing edge stepped feather by feather, each
feather worked from an iron jack at its root; a lance through each wing's
leading edge; two blades of a forked tail, curved up and out, with its
engine's mouth, vaned, between them. It is a machine and is lit like one:
its engine, its lift's lenses, a lamp at each wing's tip and each tail
blade's, and one hairline down its crest; nothing drawn on it in light. Its
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
# A wing, root to tip: its leading edge (x at each span), the height of its
# middle and its thickness. A sickle: it leaves the body, holds forward to a
# wrist and sweeps back to a point.
W_LE = curve((0.5, 0.98), (0.95, 0.74), (1.4, 0.66), (1.8, 0.52), (2.15, 0.10), (2.4, -0.55), (2.6, -1.2))
W_Z = curve((0.5, 0.72), (1.4, 0.72), (2.2, 0.69), (2.6, 0.64))
W_T = curve((0.5, 0.16), (0.95, 0.14), (1.6, 0.12), (2.3, 0.07), (2.6, 0.03))
# Its arm: the round dark spar along that edge, this deep at each span.
ARM = curve((0.5, 0.5), (0.95, 0.42), (1.8, 0.36), (2.3, 0.26), (2.6, 0.08))
A_AT = (0.5, 0.95, 1.4, 1.8, 2.15, 2.4, 2.59)
# Its feathers: the span each leaves the arm at, and where each ends (x,
# span): the trailing edge steps back feather by feather to the tip. How
# wide a feather is at its widest (half of it), its width along its length
# as shares of that, how thin it is for its width, how much lower each lies
# than the one inboard of it, and how much of its end is alight, metres.
F_ROOT = (0.98, 1.22, 1.46, 1.70, 1.92, 2.12, 2.28)
F_END = ((-0.80, 1.02), (-0.94, 1.30), (-1.05, 1.60), (-1.14, 1.90), (-1.22, 2.16), (-1.29, 2.37), (-1.33, 2.5))
F_WIDE = 0.175
F_SHAPE = (0.6, 0.86, 1.0, 1.0, 0.98, 0.9, 0.72, 0.42, 0.06)
F_THIN = 0.11
F_STEP = 0.011
F_LIT = 0.055
# The jack at each feather's root: how long, wide and tall, metres.
JACK = (0.15, 0.075, 0.042)
# The blades of its tail: from where to where, how far out they start and
# how far they spread, how high they start and how far they rise; half
# their width at its widest and their width along their length as shares of
# it, how thin they are for it, and how far they are canted, degrees (their
# outer edges high: from its own saddle a pilot sees their faces).
BLADE = (-0.95, -2.28, 0.58, 0.19, 0.76, 0.3)
BLADE_WIDE = 0.17
BLADE_SHAPE = (0.45, 0.7, 0.88, 1.0, 1.0, 1.0, 0.86, 0.5, 0.1)
BLADE_THIN = 0.2
BLADE_CANT = 40
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

# The wings, as a swift's: an arm along the leading edge, round and dark,
# and the feathers that fan back from under it, each lapping the next one
# out, by turns the shell's violet and its deeper one, each worked by a jack
# of iron at its root; the last one's end a lamp, the wing's tip.
def arm(side):
    return loft([foil(side * y, W_LE(y), W_LE(y) - ARM(y), W_Z(y), W_T(y), n=10) for y in A_AT], 'shell_deep', name='arm')


def feather(side, i):
    """One feather: a blade from under the arm at span `F_ROOT[i]` back to
    its end at `F_END[i]`, a little lower than the one inboard of it."""
    y0, (x1, y1) = F_ROOT[i], F_END[i]
    sink = 0.02 + F_STEP * i
    root = Vector((W_LE(y0) - ARM(y0) + 0.12, side * y0, W_Z(y0) - sink))
    end = Vector((x1, side * y1, W_Z(y1) - sink - 0.01))
    # It bows a little outward on its way.
    bend = (root + end) / 2 + Vector((0.0, side * 0.035, 0.0))
    o = solid(stem(bow(root, bend, end, steps=8), [F_WIDE * k for k in F_SHAPE], n=8, material='shell' if i % 2 else 'shell_deep',
                   name='feather', flat=F_THIN, thin=(0, 0, 1), sharp=(0, 2, 4)), 'feather')
    # (Creased down its two edges and its quill: a blade with a spine, where
    # it was a paddle.)
    # Its jack: a block of iron on it where it leaves the arm, and the pin
    # it turns on.
    at = (W_LE(y0) - ARM(y0) - 0.035, side * y0, W_Z(y0) - sink + 0.022)
    jack = box(*JACK, at=at, material=None, bevel=0.006)
    jack.data.materials.append(skin('iron'))
    pin = cyl(0.017, JACK[1] + 0.03, axis='y', at=(at[0] + 0.02, at[1], at[2] + 0.006), material=None, verts=8, bevel=0.0)
    pin.data.materials.append(skin('iron'))
    if i == len(F_ROOT) - 1:
        repaint(o, 'light', where=lambda q: np.linalg.norm(q - np.array(end), axis=1) < F_LIT)
    return [o, jack, pin]


wings = [arm(side) for side in (1, -1)] + [part for side in (1, -1) for i in range(len(F_ROOT)) for part in feather(side, i)]
parts += wings


def lance(side):
    """A gun's fairing through the wing's leading edge, its barrel in an
    iron collar, and the ring of light behind its muzzle."""
    y, z = side * GUNS[1], GUNS[2]
    r = curve((-0.8, 0.02), (-0.4, 0.1), (0.0, 0.13), (0.4, 0.115), (0.62, 0.09), (0.7, 0.08))
    pod = loft([oval(x, y, z, r(x), r(x), n=10, p=2.0) for x in (-0.8, -0.6, -0.4, -0.2, 0.0, 0.2, 0.4, 0.62, 0.7)],
               'shell_deep', ends=('tip', 'flat'), name='lance', levels=1)
    out = []
    for at, long, wide, paint in ((0.74, 0.1, 0.075, 'iron'), (0.92, 0.3, 0.05, 'iron'), (GUNS[0] - 0.1, 0.06, 0.075, 'light_dim'),
                                  (GUNS[0] - 0.035, 0.07, 0.056, 'iron')):
        part = cyl(wide, long, at=(at, y, z), material=None, verts=18, bevel=0.004)
        part.data.materials.append(skin(paint))
        out.append(part)
    return [pod] + out


def blade(side):
    """A blade of the forked tail: a feather on a cant, its outer edge high,
    lifting and spreading as it goes aft, broad to its last third and then a
    point, its end alight; pale, as the bill's upper mandible is, so the
    craft's two ends are its two pale things and the tail is not taken for
    one more feather of a wing."""
    x0, x1, y0, spread, z0, rise = BLADE
    path = [(x0 + (x1 - x0) * k, side * (y0 + spread * k), z0 + rise * k ** 1.4) for k in np.linspace(0.0, 1.0, 9)]
    cant = math.radians(BLADE_CANT)
    o = solid(stem(path, [BLADE_WIDE * k for k in BLADE_SHAPE], n=8, material='shell', name='blade', flat=BLADE_THIN,
                   thin=(0, -side * math.sin(cant), math.cos(cant))), 'blade')
    return repaint(o, 'light', where=lambda q: X(q) < x1 + 0.07)


for side in (1, -1):
    parts += [blade(side)] + lance(side)

o = solid(parts, 'flier')
# Its fasteners: a row each side of each shoulder's seam, an arc over its
# back before the engine's cowl, and a row down each wing's arm.
pins = []
for side in (1, -1):
    for x in np.linspace(-1.3, 1.0, 7):
        for lean in (-0.08, 0.085):
            pins.append((x, side * (0.6 - 0.2 * x + lean), B_TOP(x)))
    for y in np.linspace(0.75, 2.35, 7):
        pins.append((W_LE(y) - 0.3 * ARM(y), side * y, W_Z(y) + W_T(y)))
for a in np.linspace(0.12, math.pi - 0.12, 8):
    x = COWL + 0.075
    pins.append((x, 1.1 * B_WIDE(x) * math.cos(a), B_CHINE(x) + 1.1 * (B_TOP(x) - B_CHINE(x)) * math.sin(a)))
o = solid([o, studs(o, pins)], 'flier')


def soot(q):
    """What is left of the paint: soot aft of the gills and round the mouth,
    and the mouth's lip blued by its heat."""
    aft = np.clip((-1.05 - q[:, 0]) / 0.7, 0.0, 1.0) * (np.abs(q[:, 1]) < 0.5)
    left = (1.0 - 0.45 * aft)[:, None] * np.ones((1, 3))
    lip = (np.clip((TAIL + 0.12 - q[:, 0]) / 0.12, 0.0, 1.0) * (np.abs(q[:, 1]) < 0.5))[:, None]
    # And at each feather's root, aft of its jack: what a pilot sees of his
    # own wings.
    y = np.abs(q[:, 1])
    root = np.clip(1.0 - (W_LE(y) - ARM(y) - q[:, 0]) / 0.4, 0.0, 1.0) * (q[:, 0] < W_LE(y) - ARM(y)) * (y > 0.95)
    left = left * (1.0 - 0.5 * root)[:, None]
    return left * (1 - lip) + lip * np.array([1.25, 1.0, 1.45])


shade(o, tone=0.16, stain=soot)
gloss(o)

r = sim('BODY_R')
eye = Vector((SEAT[0], 0, SEAT[1] + sim('CROUCH_EYE')))
x0, x1, y0, spread, z0, rise = BLADE
deliver(
    'flier', o,
    # `Vehicle::hull`: its fuselage, and its wings' slab.
    steel=[((-HL, -BW, 0), (HL, BW, TOP)), ((-HL * 0.6, -HW, WY), (HL * 0.3, HW, WY + WT))],
    rider=[((SEAT[0] - r, -r, SEAT[1]), (SEAT[0] + r, r, SEAT[1] + sim('CROUCH_H')))],
    marks=[(GUNS[0], side * GUNS[1], GUNS[2]) for side in (1, -1)],
    chase=(eye + Vector((-cam('FLY_CAM_BACK_M'), 0, cam('FLY_CAM_UP_M'))), eye + Vector((cam('CRAFT_CAM_MEET_M'), 0, 0))),
    # Its mouth's two halves and its middle, for the boost's flames.
    burns=[(TAIL, MW * 0.5, MZ), (TAIL, -MW * 0.5, MZ), (TAIL, 0.0, MZ)],
    note='Veil flier: a swift, sickle wings with stepped feathers and a forked tail of two blades, a lance through each '
         f"wing, its engine a mouth between the blades; its tail's tips at x {x1}, y ±{y0 + spread:.2f}, z {z0 + rise:.2f}")

"""Overpass' broken flyover: everything built, in the map's own coordinates (GLBs
at the origin, `look/built.rs` draws them), over a drowned city at dusk: the
flyover and its slip roads, the parking blocks, walls, signs, lamps, puddles on
the road, and out of bounds the fallen deck, towers, and their lights in the flood.

    blender -b --python tools/models/overpass.py

What collides is `maps/overpass/map.json`'s hidden solids; these numbers follow
it. Red's half is written and mirrored; the wrecks are `look/boulders.rs`'.
"""
import math
import os
import random
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import arch  # noqa: E402
from arch import BACK, cbox, extrude, export, make_objects, mirror_parts, paint, panel, part, plank, quad, rod, spin, strip_light, surface, tbox, to_blender, tri  # noqa: E402,F401

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, '..', '..'))
OUT = os.path.join(ROOT, 'crates', 'client', 'assets', 'models', 'overpass')

arch.TILES.update({'road': 5.0, 'blast': 3.6, 'blast_dark': 3.6, 'rust': 2.4})
arch.UNTRACED.update({'city', 'city_lights', 'flood', 'shaft', 'puddle', 'wet_road'})

DECK = 5.0
ROOF = 3.5
FLOOD = -2.2
# What the wet road's reflection keeps of the near things, and how much longer it
# draws them.
WET_DIM = {'blast': 0.2, 'blast_dark': 0.08, 'road': 0.14, 'rust': 0.16, 'light_sodium': 0.7, 'light_pink': 0.65, 'light_cyan': 0.65,
           'light_red': 0.7, 'light_blue': 0.7, 'light_warm': 0.5, 'light_cold': 0.5}
WET_STRETCH_LIGHT = 1.8
# What a reflection of the city keeps: it's rain-beaten and far.
DIM = {'city': 0.6, 'city_lights': 0.5, 'light_warm': 0.55, 'light_cold': 0.55, 'light_pink': 0.8, 'light_cyan': 0.8, 'light_sodium': 0.7}
# Jersey barrier's section, s across its width, inside its box.
def jersey_section(half):
    return [(-half, 0.0), (half, 0.0), (half, 0.08), (half * 0.45, 0.3), (half * 0.38, 0.95), (-half * 0.38, 0.95), (-half * 0.45, 0.3), (-half, 0.08)]


def jersey(a, b, half=0.3, y=0.0):
    extrude((a[0], a[1]), (b[0], b[1]), [(s, h + y) for s, h in jersey_section(half)], 'blast', caps=True, cell=1.5)


def turned_jersey(x, z, length, angle_deg, s=1):
    a = math.radians(angle_deg)
    # A turned box's size is [along its own x, along its own z].
    ux, uz = math.cos(a) * s, math.sin(a)
    if length is None:
        return
    jersey((x - ux * 1.8, z - uz * 1.8), (x + ux * 1.8, z + uz * 1.8))


# ----------------------------------------------------------------- the flyover
def flyover():
    # Its road, its body narrowing under, out past the bounds to where it fell.
    far = 24.5
    for z0, z1 in ((-far, 0.0), (0.0, far)):
        cbox(-4.5, 4.86, z0, 4.5, DECK, z1, 'road', c=0.0, cell=1.6)
    girder = [(-3.2, 4.1), (3.2, 4.1), (4.5, 4.62), (4.5, 4.86), (-4.5, 4.86), (-4.5, 4.62)]
    extrude((0, -far), (0, far), girder, 'blast', caps=False, cell=1.6)
    # Lines: its edges and a broken middle, and the joints across it.
    for x in (-3.9, 3.9):
        cbox(x - 0.07, DECK, -far, x + 0.07, DECK + 0.006, far, 'paint', c=0.0, cell=6.0)
    z = -far
    while z < far:
        cbox(-0.08, DECK, z, 0.08, DECK + 0.006, z + 2.6, 'paint_y', c=0.0, cell=6.0)
        z += 6.0
    for z in (-15.0, -3.0, 9.0, 21.0):
        cbox(-4.48, DECK, z - 0.1, 4.48, DECK + 0.008, z + 0.1, 'steel', c=0.0, cell=6.0)
    # What's left where it broke: the end jagged, rebar out of it.
    rng = random.Random(9)
    for s in (-1, 1):
        for i in range(10):
            x = -4.2 + i * 0.93
            rod((x, 4.7 + rng.random() * 0.2, s * far), (x + rng.uniform(-0.3, 0.3), 4.7 + rng.uniform(-1.4, -0.2), s * (far + rng.uniform(0.8, 2.2))), 0.03, 'rust', sides=5)
        for i in range(7):
            x = -4.2 + i * 1.4
            w = rng.uniform(0.5, 1.1)
            cbox(x - w / 2, 4.1, s * far - 0.4 if s > 0 else s * far - 0.4 - rng.random(), x + w / 2, DECK + rng.uniform(-0.4, 0.0), s * far + 0.4 + rng.random() if s > 0 else s * far + 0.4, 'blast', c=0.1, cell=2.0)
        # The barricade closing it where the fight stops: stacked blocks.
        for x in (-3.2, -1.1, 1.0, 3.1):
            cbox(x - 0.95, DECK, s * 21.7 - 0.3, x + 0.95, DECK + 0.72, s * 21.7 + 0.3, 'blast', c=0.05)
            cbox(x - 0.8, DECK + 0.72, s * 21.7 - 0.3, x + 0.8, DECK + 1.44, s * 21.7 + 0.3, 'blast_dark', c=0.05)
    # Past the gap, the deck's next span fallen at one end, tilted up at us, a
    # lamp bent over on it.
    for sgn in (-1, 1):
        a0, a1 = (0.0, sgn * 26.4), (0.0, sgn * 38.0)
        extrude(a0, a1, [(-4.4, -0.9), (4.4, -0.9), (4.4, 0.0), (-4.4, 0.0)], 'blast', caps=True, cell=1.6, ya=DECK + 1.8, yb=DECK - 6.0)
        extrude(a0, a1, [(-4.4, 0.0), (4.4, 0.0), (4.4, 0.04), (-4.4, 0.04)], 'road', caps=True, cell=1.6, ya=DECK + 1.8, yb=DECK - 6.0)
        rod((3.9, DECK + 1.6, sgn * 29.0), (3.9, DECK + 4.6, sgn * 30.4), 0.11, 'steel', sides=8)
        rod((3.9, DECK + 4.6, sgn * 30.4), (2.2, DECK + 3.4, sgn * 33.2), 0.09, 'steel', sides=6)
        rng2 = random.Random(31 if sgn > 0 else 32)
        for i in range(9):
            x = -4.0 + i * 1.0
            rod((x, DECK + 1.2, sgn * 26.4), (x + rng2.uniform(-0.4, 0.4), DECK + rng2.uniform(0.2, 1.6), sgn * (25.0 + rng2.uniform(0.0, 0.9))), 0.03, 'rust', sides=5)
    # The lorry the containers came off: its cab, burnt, on the flyover.
    cbox(1.5, DECK, 3.4, 3.9, DECK + 0.5, 5.6, 'rust', c=0.08)
    cbox(1.62, DECK + 0.5, 3.5, 3.78, DECK + 2.2, 5.5, 'rust', c=0.14)
    cbox(1.7, DECK + 1.2, 3.46, 3.7, DECK + 2.0, 3.5, 'blast_dark', c=0.0)
    for x in (1.7, 3.7):
        spin(x, 4.0, [(0.46, DECK + 0.02), (0.46, DECK + 0.46), (0.02, DECK + 0.46)], 'rust', sides=12, smooth=False)
    container(-3.5, DECK, 6.6, -1.1, 12.6, 0.0)
    container_turned(1.9, -9.8, 2.4, 6.0, 14.0)


def container(x0, y0, z0, x1, z1, _):
    """A shipping box along z, its long sides ribbed, inside its box."""
    cbox(x0 + 0.06, y0, z0 + 0.06, x1 - 0.06, y0 + 2.5, z1 - 0.06, 'rust', c=0.02, cell=1.2)
    for x, z in ((x0, z0), (x1 - 0.12, z0), (x0, z1 - 0.12), (x1 - 0.12, z1 - 0.12)):
        cbox(x, y0, z, x + 0.12, y0 + 2.6, z + 0.12, 'blast_dark', c=0.01, cell=3.0)
    # Its doors at the north end: four locking bars.
    for i in range(4):
        x = x0 + 0.35 + i * (x1 - x0 - 0.7) / 3
        rod((x, y0 + 0.25, z1 - 0.03), (x, y0 + 2.35, z1 - 0.03), 0.03, 'steel', sides=6)
    # Its owner's marks, in no language anyone reads.
    rng = random.Random(int(x0 * 13 + z0 * 7))
    for k in range(5):
        h = rng.uniform(0.3, 0.6)
        w = rng.uniform(0.08, 0.32)
        zz = z0 + 0.8 + k * 0.62
        quad((x0 + 0.055, y0 + 1.5, zz), (x0 + 0.055, y0 + 1.5, zz + w), (x0 + 0.055, y0 + 1.5 + h, zz + w), (x0 + 0.055, y0 + 1.5 + h, zz), 'paint', (-1, 0, 0), cell=9.0)
    cbox(x0, y0, z0, x1, y0 + 0.14, z1, 'blast_dark', c=0.02, cell=3.0)
    cbox(x0, y0 + 2.46, z0, x1, y0 + 2.6, z1, 'blast_dark', c=0.02, cell=3.0)
    z = z0 + 0.3
    while z < z1 - 0.2:
        for x, w in ((x0, 0.06), (x1 - 0.06, 0.06)):
            cbox(x, y0 + 0.14, z, x + w, y0 + 2.46, z + 0.13, 'rust', c=0.0, cell=3.0)
        z += 0.34


def container_turned(cx, cz, w, ln, deg):
    a = math.radians(deg)
    # A turned box's angle turns +x toward +z: its own z-long axis.
    ux, uz = math.cos(a), math.sin(a)
    tbox(cx, cz, ux, uz, w / 2 - 0.06, ln / 2 - 0.06, DECK, DECK + 2.5, 'rust', cell=1.2)
    tbox(cx, cz, ux, uz, w / 2, ln / 2, DECK, DECK + 0.14, 'blast_dark', cell=3.0)
    tbox(cx, cz, ux, uz, w / 2, ln / 2, DECK + 2.46, DECK + 2.6, 'blast_dark', cell=3.0)
    vx, vz = -uz, ux
    t = -ln / 2 + 0.3
    while t < ln / 2 - 0.2:
        for s in (-1, 1):
            tbox(cx + ux * s * (w / 2 - 0.03) + vx * t, cz + uz * s * (w / 2 - 0.03) + vz * t, ux, uz, 0.03, 0.065, DECK + 0.14, DECK + 2.46, 'rust', cell=3.0)
        t += 0.34


def red_half(sx):
    """Everything mirrored, for sx = 1 red's (-x), -1 blue's."""
    X = lambda x: x * sx
    light = 'light_red' if sx > 0 else 'light_blue'
    def bx(x0, y0, z0, x1, y1, z1, mat='blast', c=0.04, **kw):
        a, b = sorted((X(x0), X(x1)))
        cbox(a, y0, z0, b, y1, z1, mat, c=c, **kw)

    # The flyover's barriers, either side of each slip road's mouth.
    for z0, z1 in ((-22.0, -17.0), (-11.0, 11.0), (17.0, 22.0)):
        jersey((X(-4.325), z0), (X(-4.325), z1), half=0.175, y=DECK)
    # Lamps: a pole, its arm over the road, the sodium head.
    for z in (-20.0, -6.0, 6.0, 20.0):
        px = X(-4.32)
        rod((px, DECK, z), (px, DECK + 6.2, z), 0.12, 'steel', sides=8, taper=0.6)
        rod((px, DECK + 6.2, z), (X(-3.3), DECK + 6.42, z), 0.05, 'steel', sides=6)
        hx0, hx1 = sorted((X(-3.62), X(-3.0)))
        cbox(hx0, DECK + 6.34, z - 0.12, hx1, DECK + 6.46, z + 0.12, 'steel', c=0.02, cell=3.0)
        cbox(hx0 + 0.02, DECK + 6.32, z - 0.09, hx1 - 0.02, DECK + 6.34, z + 0.09, 'light_sodium', c=0.0, cell=3.0)
    # Its piers, each flared into a cross-head under the deck.
    for z in (-17.0, -6.0, 6.0, 17.0):
        bx(-3.6, 0.0, z - 0.7, -2.2, 3.5, z + 0.7, 'blast', c=0.1)
        bx(-3.45, 3.5, z - 0.7, -2.35, 4.1, z + 0.7, 'blast', c=0.06)
        bx(-3.6, 0.0, z - 0.7, -2.2, 0.26, z + 0.7, 'blast_dark', c=0.03)
    # The slip roads: a bank then a span on legs, the road on top.
    for sz, (z0, z1) in ((1, (11.0, 17.0)), (-1, (-17.0, -11.0))):
        xa, xb = X(-19.0), X(-11.0)
        def t(x, y, z):
            return (x, y, z)
        # The bank's road and walls.
        quad((xa, 0.01, z0), (xa, 0.01, z1), (xb, 2.76, z1), (xb, 2.76, z0), 'road', (0, 1, 0), cell=1.6)
        for z, o in ((z0, -1), (z1, 1)):
            quad((xa, 0.0, z), (xb, 0.0, z), (xb, 2.76, z), (xa, 0.01, z), 'blast', (0, 0, o), cell=1.4)
        quad((xb, 0.0, z0), (xb, 0.0, z1), (xb, 2.26, z1), (xb, 2.26, z0), 'blast', (X(1), 0, 0), cell=1.4)
        # The span from there to the flyover.
        xc = X(-4.5)
        for off0, off1, mat, drop, thick in ((-3.0, 3.0, 'road', 0.0, 0.06), (-3.0, 3.0, 'blast', 0.06, 0.44)):
            zm = (z0 + z1) / 2
            quad((xb, 2.76 - drop, z0), (xb, 2.76 - drop, z1), (xc, DECK - drop, z1), (xc, DECK - drop, z0), mat, (0, 1, 0) if drop == 0.0 else (0, -1, 0), cell=1.6)
        for z, o in ((z0, -1), (z1, 1)):
            quad((xb, 2.26, z), (xc, 4.5, z), (xc, DECK, z), (xb, 2.76, z), 'blast', (0, 0, o), cell=1.4)
        quad((xb, 2.26, z0), (xb, 2.26, z1), (xc, 4.5, z1), (xc, 4.5, z0), 'blast_dark', (0, -1, 0), cell=1.6)
        # Its end, the lip that hangs under the flyover's edge.
        quad((xc, 4.5, z0), (xc, 4.5, z1), (xc, 4.62, z1), (xc, 4.62, z0), 'blast_dark', (X(1), 0, 0), cell=1.6)
        # Its lines, a low kerb drawn on its top edges.
        for z in (z0 + 0.25, z1 - 0.25):
            quad((xa, 0.02, z - 0.06), (xa, 0.02, z + 0.06), (xb, 2.77, z + 0.06), (xb, 2.77, z - 0.06), 'paint', (0, 1, 0), cell=8.0)
            quad((xb, 2.77, z - 0.06), (xb, 2.77, z + 0.06), (xc, DECK + 0.006, z + 0.06), (xc, DECK + 0.006, z - 0.06), 'paint', (0, 1, 0), cell=8.0)
        # Legs under the span, a head across them.
        for z in ((11.3, 15.7) if sz > 0 else (-12.3, -16.7)):
            bx(-9.2, 0.0, z, -8.2, 3.1, z + 1.0, 'blast', c=0.08)
            bx(-9.2, 0.0, z, -8.2, 0.24, z + 1.0, 'blast_dark', c=0.03)
    # The barriers on the road.
    def road_jersey(cx, cz, deg, along_z):
        # The map's turned box: size [0.6, 4] with angle, or [3.6, 0.6].
        a = math.radians(deg)
        ux, uz = math.cos(a), math.sin(a)
        if along_z:
            ux, uz = -uz, ux
            half = 2.0
        else:
            half = 1.8
        x0, z0 = X(cx - ux * half), cz - uz * half
        x1, z1 = X(cx + ux * half), cz + uz * half
        jersey((x0, z0), (x1, z1))
    road_jersey(-13.2, 0.0, 8, True)
    road_jersey(-7.2, -3.2, 35, False)
    road_jersey(-7.4, 3.6, -30, False)
    road_jersey(-19.8, -8.5, 70, False)
    # Two the map calls 3.2 m long.
    for cx, cz, deg in ((-21.0, 19.2, 10), (-14.0, -19.6, -14)):
        a = math.radians(deg)
        ux, uz = math.cos(a), math.sin(a)
        jersey((X(cx - ux * 1.6), cz - uz * 1.6), (X(cx + ux * 1.6), cz + uz * 1.6))
    block(sx, X, bx, light)


def block(sx, X, bx, light):
    """A parking block: two floors of cast concrete, its columns and beams set
    back from the planes that collide, stairs inside, a ramp down its front."""
    # The floor: oil-dark, its bays painted.
    bx(-31.6, -0.1, -9.6, -23.0, 0.012, 9.6, 'blast_dark', c=0.0, cell=1.6)
    for z in (-7.8, -5.6, -3.4, 3.4, 5.6, 7.8):
        bx(-29.0, 0.012, z - 0.05, -24.2, 0.018, z + 0.05, 'paint', c=0.0, cell=6.0)
    # The back and the sides, panelled.
    a, b = sorted((X(-32.0), X(-31.6)))
    panel(a, 0.0, -10.0, b, ROOF, 10.0, '+x' if sx > 0 else '-x', mat='blast', plinth='blast_dark')
    a, b = sorted((X(-32.0), X(-23.0)))
    panel(a, 0.0, 9.6, b, ROOF, 10.0, '-z', mat='blast', plinth='blast_dark')
    panel(a, 0.0, -10.0, b, ROOF, -9.6, '+z', mat='blast', plinth='blast_dark')
    # The back wall to the top of the bounds, behind everything.
    bx(-33.0, 0.0, -23.0, -32.0, 8.0, 23.0, 'blast', c=0.0, cell=1.6)
    # Columns on the front, a beam over them.
    for z0, z1 in ((-5.6, -4.9), (4.9, 5.6), (-9.6, -8.9), (8.9, 9.6)):
        bx(-23.6, 0.0, z0, -22.9, ROOF - 0.4, z1, 'blast', c=0.06)
        bx(-23.6, 0.0, z0, -22.9, 0.26, z1, 'blast_dark', c=0.03)
    # The roof, its holes for the stairs; ribbed under.
    bx(-32.0, ROOF - 0.4, -3.0, -23.0, ROOF, 3.0, 'road', c=0.02, cell=1.6)
    bx(-29.0, ROOF - 0.4, -10.0, -23.0, ROOF, -3.0, 'road', c=0.02, cell=1.6)
    bx(-29.0, ROOF - 0.4, 3.0, -23.0, ROOF, 10.0, 'road', c=0.02, cell=1.6)
    bx(-32.0, ROOF - 0.4, -10.0, -29.0, ROOF, -9.0, 'road', c=0.02)
    bx(-32.0, ROOF - 0.4, 9.0, -29.0, ROOF, 10.0, 'road', c=0.02)
    for x in (-30.5, -28.0, -25.5):
        bx(x - 0.16, ROOF - 0.62, -9.6, x + 0.16, ROOF - 0.4, 9.6, 'blast_dark', c=0.03, cell=2.0)
    # Parapets, inside theirs, its side's light low on the inner face.
    for z0, z1 in ((-10.0, -3.0), (3.0, 10.0)):
        bx(-23.3, ROOF, z0, -23.0, ROOF + 0.98, z1, 'blast', c=0.03)
        bx(-23.3, ROOF + 0.98, z0, -23.0, ROOF + 1.1, z1, 'blast_dark', c=0.03)
    bx(-32.0, ROOF, -10.0, -31.6, ROOF + 1.1, 10.0, 'blast', c=0.03)
    bx(-31.6, ROOF, 9.7, -23.3, ROOF + 1.1, 10.0, 'blast', c=0.03)
    bx(-31.6, ROOF, -10.0, -23.3, ROOF + 1.1, -9.7, 'blast', c=0.03)
    bx(-31.58, ROOF + 0.12, -9.6, -31.56, ROOF + 0.17, 9.6, light, c=0.0, cell=30.0)
    # The lights on its floor's ceiling, in its side's colour.
    for z in (-6.8, 6.8):
        bx(-29.0, ROOF - 0.44, z - 0.06, -24.0, ROOF - 0.4, z + 0.06, light, c=0.0, cell=30.0)
    # The stairs inside: steps with their noses on the sim's ramp.
    for sz, z_lo, z_hi in ((1, -9.0, -3.0), (-1, 9.0, 3.0)):
        n = 10
        run = (z_hi - z_lo) / n
        for i in range(n):
            za, zb = sorted((z_lo + run * i, z_lo + run * (i + 1)))
            top = ROOF * i / n
            bx(-31.0, 0.0, za, -29.0, max(top, 0.01), zb, 'blast', c=0.02)
    # Its ramp down the front, the road on it, low walls drawn along it.
    xa, xb = X(-15.0), X(-23.0)
    quad((xa, 0.01, -3.0), (xa, 0.01, 3.0), (xb, ROOF, 3.0), (xb, ROOF, -3.0), 'road', (0, 1, 0), cell=1.6)
    for z, o in ((-3.0, -1), (3.0, 1)):
        quad((xa, 0.0, z), (xb, 0.0, z), (xb, ROOF, z), (xa, 0.01, z), 'blast', (0, 0, o), cell=1.4)
    # Its back, seen from the floor inside, up to the roof's underside; the
    # roof's cut edge filled where the ramp leaves it.
    quad((xb, 0.0, -3.0), (xb, 0.0, 3.0), (xb, ROOF - 0.4, 3.0), (xb, ROOF - 0.4, -3.0), 'blast', (-X(1), 0, 0), cell=1.4)
    quad((X(-23.02), ROOF, -3.0), (X(-23.02), ROOF, 3.0), (xb, ROOF, 3.0), (xb, ROOF, -3.0), 'road', (0, 1, 0), cell=1.6)
    for z in (-2.75, 2.75):
        quad((xa, 0.02, z - 0.06), (xa, 0.02, z + 0.06), (xb, ROOF + 0.005, z + 0.06), (xb, ROOF + 0.005, z - 0.06), 'paint', (0, 1, 0), cell=8.0)
    # The block's upper face to the road: a great sign in its side's colour.
    a, b = sorted((X(-23.02), X(-23.0)))


def walls():
    """The retaining walls north and south, panelled, a rail on top; the road's
    lanes, crossings and lamp-pools; puddles."""
    for s in (-1, 1):
        z0, z1 = (22.0, 23.0) if s > 0 else (-23.0, -22.0)
        face = '-z' if s > 0 else '+z'
        x = -32.0
        while x < 32.0:
            panel(x, 0.0, z0, x + 8.0, 5.6, z1, face, pier=0.36, band=0.3, mat='blast', plinth='blast_dark')
            x += 8.0
        # A coping and a rail over it.
        cbox(-33.0, 5.6, z0 - 0.02, 33.0, 5.74, z1 + 0.02, 'blast_dark', c=0.03, cell=2.4)


def neon(x, y, z, h, s, mat, rng):
    """A sign in no language anyone reads: strokes of light on a dark board."""
    bw = h * 0.46
    n = rng.randint(3, 5)
    board = 'rust'
    cbox(x - bw * n / 2 - 0.1, y - 0.12, min(z, z + s * 0.1), x + bw * n / 2 + 0.1, y + h + 0.12, max(z, z + s * 0.1), board, c=0.01, cell=1.4)
    face = z - s * 0.012 if s > 0 else z + s * 0.012
    f = lambda a, b, c, d: quad((a, b, face), (c, b, face), (c, d, face), (a, d, face), mat, (0, 0, -s), cell=30.0)
    for k in range(n):
        gx = x - bw * n / 2 + (k + 0.5) * bw
        strokes = rng.randint(3, 5)
        w = 0.05 * h + 0.02
        f(gx - bw * 0.34, y, gx - bw * 0.34 + w, y + h)
        for _ in range(strokes):
            yy = y + rng.random() * (h - w)
            xx = gx - bw * 0.34 + rng.random() * bw * 0.3
            f(xx, yy, xx + rng.uniform(0.25, 0.68) * bw, yy + w)
        if rng.random() < 0.5:
            f(gx + bw * 0.3, y + h * rng.random() * 0.4, gx + bw * 0.3 + w, y + h)


def wet_road():
    """The ground: a wet road over its own reflection (`look/pool.rs`), a little
    past the walls."""
    n = 20
    rows = [[(-34.0 + 68.0 * i / n, 0.0, -24.0 + 48.0 * j / n) for i in range(n + 1)] for j in range(n + 1)]
    surface(rows, 'wet_road', (0, 1, 0), lambda p: (p[0], p[2]))


def road_lines():
    for z in (-7.2, -3.6, 3.6, 7.2):
        x = -22.0
        while x < 22.0:
            if abs(x) > 5.2:
                cbox(x, 0.0, z - 0.06, x + 2.2, 0.006, z + 0.06, 'paint', c=0.0, cell=6.0)
            x += 5.4
    for z in (-10.6, 10.6):
        cbox(-22.0, 0.0, z - 0.09, 22.0, 0.006, z + 0.09, 'paint_y', c=0.0, cell=6.0)
    # A crossing either side of the flyover.
    for sx in (-1, 1):
        for i in range(-8, 9):
            x0 = sx * 7.2 - 0.9
            cbox(x0, 0.0, i * 1.1 - 0.28, x0 + 1.8, 0.006, i * 1.1 + 0.28, 'paint', c=0.0, cell=6.0)
    # Manholes.
    for x, z in ((-9.0, 1.0), (12.0, -6.0), (-16.0, 9.0), (4.0, -9.0), (18.0, 4.0), (-5.6, -15.0)):
        spin(x, z, [(0.42, 0.006), (0.001, 0.006)], 'steel', sides=16, smooth=False)


def puddles():
    """Standing rain: flat dark mirrors a hair over the road, roughly round, more
    along the kerbs and the low middle."""
    rng = random.Random(4)
    pa = part('puddle')
    spots = []
    for _ in range(46):
        x = rng.uniform(-22, 22)
        z = rng.choice((rng.uniform(-10, 10), rng.uniform(-21, -18), rng.uniform(18, 21)))
        if 11 < abs(z) < 17 and 11 < abs(x) < 19:
            continue
        spots.append((x, z, rng.uniform(0.6, 2.2), rng.uniform(0.4, 1.2), rng.uniform(0, math.pi)))
    for _ in range(18):
        spots.append((rng.uniform(-3.6, 3.6), rng.uniform(-21, 21), rng.uniform(0.5, 1.4), rng.uniform(0.3, 0.8), rng.uniform(0, math.pi), DECK))
    for x, z, a, b, turn, *lift in spots:
        y0 = (lift[0] if lift else 0.0) + 0.008
        n = 14
        c, s = math.cos(turn), math.sin(turn)
        ring = []
        for i in range(n):
            t = math.tau * i / n
            r = 1.0 + 0.22 * math.sin(t * 3 + a * 4) + 0.12 * math.sin(t * 5 + b * 7)
            px, pz = math.cos(t) * a * r, math.sin(t) * b * r
            ring.append((x + px * c - pz * s, y0, z + px * s + pz * c))
        inner = [(x + (q[0] - x) * 0.55, y0, z + (q[2] - z) * 0.55) for q in ring]
        base = pa.add([(x, y0, z)] + inner + ring)
        pa.colors[base] = (1, 1, 1, 1)
        for i in range(n):
            pa.colors[base + 1 + i] = (1, 1, 1, 0.9)
            pa.colors[base + 1 + n + i] = (1, 1, 1, 0.0)
        for i in range(n):
            i1 = (i + 1) % n
            pa.faces.append(((base, base + 1 + i1, base + 1 + i), [(0, 0)] * 3, True))
            pa.faces.append(((base + 1 + i, base + 1 + i1, base + 1 + n + i1, base + 1 + n + i), [(0, 0)] * 4, True))


def pools():
    """Where the lamps fall on the wet: soft added light, stretched."""
    pa = part('shaft')
    def pool(x, y, z, a, b, col):
        n, rows = 12, 3
        base = pa.add([(x, y, z)] + [(x + math.cos(math.tau * i / n) * a * (r + 1) / rows, y, z + math.sin(math.tau * i / n) * b * (r + 1) / rows) for r in range(rows) for i in range(n)])
        pa.colors[base] = col
        for r in range(rows):
            k = (1.0 - (r + 1) / rows) ** 2
            for i in range(n):
                pa.colors[base + 1 + r * n + i] = (col[0] * k, col[1] * k, col[2] * k)
        for i in range(n):
            pa.faces.append(((base, base + 1 + (i + 1) % n, base + 1 + i), [(0, 0)] * 3, True))
        for r in range(rows - 1):
            for i in range(n):
                i1 = (i + 1) % n
                a0, a1 = base + 1 + r * n + i, base + 1 + r * n + i1
                b0, b1 = base + 1 + (r + 1) * n + i, base + 1 + (r + 1) * n + i1
                pa.faces.append(((a0, a1, b1, b0), [(0, 0)] * 4, True))
    sodium = (0.8, 0.46, 0.17)
    for sx in (-1, 1):
        for z in (-20.0, -6.0, 6.0, 20.0):
            pool(sx * 3.0, DECK + 0.012, z, 2.4, 6.5, sodium)
        # Neon's spill on the deck's ends.
        pool(sx * 2.0, DECK + 0.012, 15.0, 2.0, 4.0, (0.3, 0.04, 0.16))
        pool(sx * 2.0, DECK + 0.012, -15.0, 2.0, 4.0, (0.03, 0.18, 0.26))
    pool(0.0, 0.012, 0.0, 6.0, 8.0, (0.2, 0.13, 0.07))
    for sx in (-1, 1):
        pool(sx * 16.0, 0.012, 19.4, 8.5, 3.2, (0.3, 0.05, 0.18))
        pool(sx * 16.0, 0.012, -19.4, 8.5, 3.2, (0.04, 0.2, 0.28))


def signs():
    rng = random.Random(21)
    for sx in (-1, 1):
        neon(sx * 16.0, 2.6, 22.0, 1.5, 1, 'light_pink', rng)
        neon(sx * 16.0, 2.6, -22.0, 1.5, -1, 'light_cyan', rng)
        neon(sx * 6.0, 3.3, 22.0, 0.9, 1, 'light_cyan', rng)
        neon(sx * 6.0, 3.3, -22.0, 0.9, -1, 'light_pink', rng)
        neon(sx * 26.0, 3.2, 22.0, 1.0, 1, 'light_warm', rng)
        neon(sx * 26.0, 3.2, -22.0, 1.0, -1, 'light_warm', rng)


# --------------------------------------------------------------------- the city
# A floor and a bay of a tower, metres; a floor's glass over its floor.
WIN_FLOOR = 3.3
WIN_PITCH = 1.9
BAND = (1.0, 2.75)
# A bay lit with none lit beside it, and with one; a tower's share of both.
WIN_LIT = 0.1
WIN_RUN = 0.6
# A lit window nearest the map, of the frame's white, and what the farthest
# keeps of it: unlit, so they are what the screen shows.
WIN_GLOW = 0.62
WIN_FAR = 0.5
# What stands off a tower's wall, a step each: its glass, its piers, its lights.
SKIN_M = 0.15
# The dusk is down the map's +z (`look/mood.rs` RAIN_DUSK's sun): what a face
# turned to it takes on, over its own.
DUSK = (1.1, 0.55, 0.24)
SIGN_RGB = ((1.0, 0.2, 0.52), (0.22, 0.85, 1.0), (1.0, 0.52, 0.14))
# The big signs and the old road's other decks out in the flood: no tower
# stands in them.
SIGNS = ((-58.0, 62.0, 34.0, 'light_pink'), (64.0, 58.0, 28.0, 'light_cyan'), (-66.0, -58.0, 30.0, 'light_cyan'), (56.0, -64.0, 36.0, 'light_pink'))
DECKS = ((-140.0, 30.0, -44.0, 44.0, 9.0), (46.0, -40.0, 160.0, -60.0, 12.0), (44.0, 52.0, 140.0, 78.0, 7.0))
# The city's vertices: what each one's paint is multiplied by (`paint_city`).
CITY_TONE = {}


def toned(n0, k):
    """Everything put in the city since it had `n0` vertices is `k` of its paint."""
    k = k if isinstance(k, tuple) else (k, k, k)
    for i in range(n0, len(part('city').verts)):
        CITY_TONE[i] = k


def faces_of(x, z, w, d):
    """A tower's two faces toward the map: (middle, along, out, half its length)."""
    fx, fz = (-1.0 if x > 0 else 1.0), (-1.0 if z > 0 else 1.0)
    return (((x, z + fz * d), (1.0, 0.0), (0.0, fz), w), ((x + fx * w, z), (0.0, 1.0), (fx, 0.0), d))


def on_face(f, u0, u1, y0, y1, off, mat, k=None):
    """A sheet on a tower's face, `off` out of it: its tone in the city's paint,
    or its colour as a light."""
    (ox, oz), (ax, az), (nx, nz), _ = f
    at = lambda u, y: (ox + ax * u + nx * off, y, oz + az * u + nz * off)
    pa = part(mat)
    n0 = len(pa.verts)
    quad(at(u0, y0), at(u1, y0), at(u1, y1), at(u0, y1), mat, (nx, 0.0, nz), cell=400.0)
    if mat == 'city':
        toned(n0, k)
    else:
        for i in range(n0, len(pa.verts)):
            pa.colors[i] = k


def glyphs(rng, f, u, y, h, rgb):
    """A sign down a tower's face in no language anyone reads, on its dark board."""
    w = h / 7.0
    on_face(f, u - w * 0.75, u + w * 0.75, y - 0.6, y + h + 0.6, SKIN_M * 3, 'city', 0.45)
    n = int(h / (w * 1.25))
    for k in range(n):
        gy = y + h - (k + 1) * h / n
        cell = h / n * 0.8
        t = w * 0.11
        on_face(f, u - w * 0.5, u - w * 0.5 + t, gy, gy + cell, SKIN_M * 4, 'city_lights', rgb)
        for _ in range(rng.randint(2, 4)):
            yy = gy + rng.random() * (cell - t)
            a = u - w * 0.5 + rng.random() * w * 0.3
            on_face(f, a, min(u + w * 0.5, a + rng.uniform(0.3, 0.9) * w), yy, yy + t, SKIN_M * 4, 'city_lights', rgb)


def tower(rng, x, z, w, d, h):
    """A dark tower out in the flood: a shaft that steps back as it climbs, its
    floors and piers on the faces toward the map, a few of its windows lit, its
    plant on its roof."""
    foot = FLOOD - 3.0
    own = rng.uniform(0.8, 1.2)
    r = math.hypot(x, z)
    far = min(1.0, max(0.0, (r - 90.0) / 240.0))
    # Its shaft's tiers: (x, z, half wide, half deep, from, to).
    kind = rng.random()
    tiers = []
    if kind < 0.28:
        tiers.append((x, z, w, d, foot, h))
    elif kind < 0.78:
        steps = 2 if rng.random() < 0.6 else 3
        cuts = [rng.uniform(0.5, 0.78)] if steps == 2 else [rng.uniform(0.36, 0.52), rng.uniform(0.68, 0.86)]
        tx, tz, tw, td, y0 = x, z, w, d, foot
        for k in range(steps):
            y1 = h if k == steps - 1 else FLOOD + (h - FLOOD) * cuts[k]
            tiers.append((tx, tz, tw, td, y0, y1))
            nw, nd = tw * rng.uniform(0.6, 0.86), td * rng.uniform(0.6, 0.86)
            # A step back from one side, or from both.
            tx += rng.choice((-1.0, 0.0, 1.0)) * (tw - nw)
            tz += rng.choice((-1.0, 0.0, 1.0)) * (td - nd)
            tw, td, y0 = nw, nd, y1
    else:
        # A podium and two shafts on it, one the taller.
        y1 = FLOOD + (h - FLOOD) * rng.uniform(0.22, 0.38)
        tiers.append((x, z, w, d, foot, y1))
        low = h - (h - y1) * rng.uniform(0.15, 0.4)
        tall = rng.choice((-1.0, 1.0))
        if w >= d:
            tiers.append((x + tall * w * 0.56, z, w * 0.4, d * 0.8, y1, h))
            tiers.append((x - tall * w * 0.56, z, w * 0.4, d * 0.8, y1, low))
        else:
            tiers.append((x, z + tall * d * 0.56, w * 0.8, d * 0.4, y1, h))
            tiers.append((x, z - tall * d * 0.56, w * 0.8, d * 0.4, y1, low))
    for (tx, tz, tw, td, y0, y1) in tiers:
        n0 = len(part('city').verts)
        cbox(tx - tw, y0, tz - td, tx + tw, y1, tz + td, 'city', c=0.0, cell=400.0)
        toned(n0, own)
    # Its face: ribbons of glass a floor, upright fins, both, or bare wall.
    face = rng.choice(('bands', 'bands', 'fins', 'grid', 'grid', 'bare'))
    pitch = rng.uniform(2.8, 4.6)
    lit = rng.choice((0.0, 0.4, 0.7, 1.0, 1.0, 1.5, 2.2))
    warm = rng.random() < 0.7
    glow = WIN_GLOW * (1.0 - (1.0 - WIN_FAR) * far)
    floors = int((h - FLOOD) / WIN_FLOOR)
    whole = set(rng.sample(range(1, max(2, floors)), min(max(1, floors - 1), rng.choice((0, 0, 1, 2, 3))))) if lit > 0.0 else set()
    stair = rng.random() < 0.3
    for (tx, tz, tw, td, y0, y1) in tiers:
        for f in faces_of(tx, tz, tw, td):
            half = f[3]
            edge = min(1.2, half * 0.2)
            cols = max(1, int((half - edge) * 2 / WIN_PITCH))
            bay = (half - edge) * 2 / cols
            core = rng.randrange(cols) if stair else -1
            fl0 = max(1, int(math.ceil((y0 - FLOOD) / WIN_FLOOR)))
            fl1 = int((y1 - 0.8 - BAND[1] - FLOOD) / WIN_FLOOR)
            if face in ('fins', 'grid') and fl1 >= fl0:
                n = max(2, int(half * 2 / pitch))
                for k in range(n + 1):
                    u = -half + 0.4 + (half * 2 - 0.8) * k / n
                    on_face(f, u - 0.32, u + 0.32, max(y0, FLOOD), y1 - 0.6, SKIN_M * 2, 'city', own * (1.45 if face == 'fins' else 0.62))
            for fl in range(fl0, fl1 + 1):
                y = FLOOD + fl * WIN_FLOOR
                if face in ('bands', 'grid'):
                    g = own * (1.12 if rng.random() < 0.15 else rng.uniform(1.4, 1.85))
                    on_face(f, -half + edge, half - edge, y + BAND[0], y + BAND[1], SKIN_M, 'city', (g, g * 1.06, g * 1.16))
                if lit <= 0.0 or (fl not in whole and rng.random() < 0.3):
                    continue
                on = False
                for c in range(cols):
                    p = 0.78 if fl in whole else (WIN_RUN if on else WIN_LIT * lit)
                    on = rng.random() < p
                    if not on and c != core:
                        continue
                    u = -half + edge + (c + 0.5) * bay
                    b = glow * rng.uniform(0.5, 1.1) * (0.75 if fl in whole else 1.0)
                    col = (1.0, 0.6, 0.28) if (rng.random() < 0.8) == warm else (0.55, 0.74, 1.0)
                    if not on:
                        # The stair's slit, lit on every floor, low.
                        b, col, wide = glow * 0.4, (0.6, 0.8, 1.0), 0.14
                    else:
                        wide = 0.36
                    on_face(f, u - bay * wide, u + bay * wide, y + BAND[0] + 0.12, y + BAND[1] - 0.12, SKIN_M * 3, 'city_lights', (col[0] * b, col[1] * b, col[2] * b))
    # Its roof: plant, a tank, a mast and its lamp.
    tx, tz, tw, td, _, top = max(tiers, key=lambda t: t[5])
    for _ in range(rng.randint(0, 3)):
        bw, bd = rng.uniform(1.2, 0.45 * tw + 1.2), rng.uniform(1.2, 0.45 * td + 1.2)
        bx, bz = tx + rng.uniform(-1, 1) * max(0.0, tw - bw), tz + rng.uniform(-1, 1) * max(0.0, td - bd)
        n0 = len(part('city').verts)
        cbox(bx - bw, top, bz - bd, bx + bw, top + rng.uniform(2.0, 6.5), bz + bd, 'city', c=0.0, cell=400.0)
        toned(n0, own * rng.uniform(0.8, 1.1))
    if rng.random() < 0.3:
        tr = rng.uniform(1.4, 2.6)
        cx, cz = tx + rng.uniform(-1, 1) * max(0.0, tw - tr), tz + rng.uniform(-1, 1) * max(0.0, td - tr)
        n0 = len(part('city').verts)
        spin(cx, cz, [(tr, top + 1.2), (tr, top + 1.2 + tr * 1.6), (0.001, top + 1.8 + tr * 1.6)], 'city', sides=8, smooth=False)
        for a in range(4):
            lx, lz = cx + math.cos(a * math.tau / 4 + 0.6) * tr * 0.7, cz + math.sin(a * math.tau / 4 + 0.6) * tr * 0.7
            rod((lx, top, lz), (lx, top + 1.2, lz), 0.14, 'city', sides=4)
        toned(n0, own * 0.9)
    if rng.random() < 0.34:
        tip = top + rng.uniform(12, 30)
        n0 = len(part('city').verts)
        rod((tx, top, tz), (tx, tip, tz), 0.5, 'city', sides=5, taper=0.2)
        toned(n0, own)
        spin(tx, tz, [(0.7, tip), (0.7, tip + 0.6), (0.001, tip + 0.6)], 'light_red', sides=6, smooth=False)
    # A line of light under its roof, or a sign down its face: the near ones.
    fs = faces_of(tx, tz, tw, td)
    if far < 0.6 and rng.random() < 0.22:
        rgb = rng.choice(SIGN_RGB)
        b = glow * 1.3
        for f in fs:
            on_face(f, -f[3], f[3], top - 0.75, top - 0.3, SKIN_M * 3, 'city_lights', (rgb[0] * b, rgb[1] * b, rgb[2] * b))
    if far < 0.45 and rng.random() < 0.3:
        f = rng.choice(fs)
        rgb = rng.choice(SIGN_RGB[:2])
        b = glow * 1.5
        sh = rng.uniform(10.0, 18.0)
        if top - 4.0 - sh > FLOOD + 8.0 and f[3] > 3.0:
            glyphs(rng, f, rng.uniform(-1, 1) * (f[3] - 2.4), rng.uniform(max(FLOOD + 8.0, top - sh - 22.0), top - 4.0 - sh), sh, (rgb[0] * b, rgb[1] * b, rgb[2] * b))


def stands_clear(x, z, w, d):
    """Whether a tower here is out of the signs and the old road's decks."""
    for sx, sz, _, _ in SIGNS:
        if abs(x - sx) < w + 13.0 and abs(z - sz) < d + 8.0:
            return False
    for ax, az, bx_, bz, _ in DECKS:
        for k in range(17):
            px, pz = ax + (bx_ - ax) * k / 16, az + (bz - az) * k / 16
            if abs(x - px) < w + 8.0 and abs(z - pz) < d + 8.0:
                return False
    return True


def city():
    for ring, count, rmin, rmax, hmin, hmax in ((0, 14, 82, 118, 26, 66), (1, 20, 135, 195, 44, 118), (2, 26, 220, 330, 80, 190)):
        for i in range(count):
            # Each tower by its own roll: one changed leaves the others where they stood.
            rng = random.Random(7700 + ring * 100 + i)
            for _ in range(8):
                a = math.tau * (i + rng.random() * 0.7) / count
                r = rng.uniform(rmin, rmax)
                x, z = math.cos(a) * r * 1.2, math.sin(a) * r
                w, d = rng.uniform(8, 16), rng.uniform(8, 16)
                if stands_clear(x, z, w, d):
                    tower(rng, x, z, w, d, rng.uniform(hmin, hmax))
                    break
    rng = random.Random(77)
    # Big signs high on the nearest, for colour across the flood.
    for x, z, h, mat in SIGNS:
        s = -1 if z > 0 else 1
        n0 = len(part('city').verts)
        cbox(x - 7.5, h - 0.4, z - 0.4, x + 7.5, h + 8.4, z + 0.4, 'city', c=0.0, cell=400.0)
        for px in (x - 5.0, x + 5.0):
            cbox(px - 0.5, FLOOD - 2.0, z - 0.5, px + 0.5, h - 0.4, z + 0.5, 'city', c=0.0, cell=400.0)
            # Its frame: a brace between its legs.
        for y in (h * 0.35, h * 0.7):
            cbox(x - 5.0, y - 0.3, z - 0.3, x + 5.0, y + 0.3, z + 0.3, 'city', c=0.0, cell=400.0)
        toned(n0, 0.8)
        neon_far(x, h, z + s * 0.5, s, mat, rng)
    # Another deck of the old road, standing out in the flood, on its piers.
    for (ax, az, bx_, bz, y) in DECKS:
        extrude((ax, az), (bx_, bz), [(-4.5, y - 1.0), (4.5, y - 1.0), (4.5, y), (-4.5, y)], 'city', caps=True, cell=400.0)
        # Its parapets.
        for off in (-4.5, 4.2):
            extrude((ax, az), (bx_, bz), [(off, y), (off + 0.3, y), (off + 0.3, y + 0.9), (off, y + 0.9)], 'city', caps=True, cell=400.0)
        n = 7
        for k in range(1, n):
            px, pz = ax + (bx_ - ax) * k / n, az + (bz - az) * k / n
            cbox(px - 1.0, FLOOD - 2.0, pz - 1.0, px + 1.0, y - 1.0, pz + 1.0, 'city', c=0.0, cell=400.0)
            cbox(px - 3.4, y - 1.9, pz - 1.3, px + 3.4, y - 1.0, pz + 1.3, 'city', c=0.0, cell=400.0)
    # And the flyover's own far piers, north and south, past where it fell.
    for s in (-1, 1):
        for sx in (-1, 1):
            cbox(sx * 3.6 - 1.4 if sx > 0 else sx * 3.6, FLOOD - 2.0, s * 32.0 - 0.7, sx * 3.6 if sx > 0 else sx * 3.6 + 1.4, 4.1, s * 32.0 + 0.7, 'blast', c=0.06)


def neon_far(x, y, z, s, mat, rng):
    f = lambda a, b, c, d: quad((a, b, z), (c, b, z), (c, d, z), (a, d, z), mat, (0, 0, s), cell=30.0)
    for k in range(5):
        gx = x - 6.0 + k * 2.8
        w = 0.32
        f(gx, y, gx + w, y + 7.6)
        for _ in range(4):
            yy = y + rng.random() * 7.2
            xx = gx + rng.random() * 1.0
            f(xx, yy, xx + rng.uniform(0.8, 1.9), yy + w)


def paint_city():
    """The towers' colour, unlit: each ring a step paler and bluer into the rain,
    misted at the water; each tower its own value, its glass over its wall, a
    face to the dusk warmed by it, one turned from it darker."""
    pa = part('city')
    turned = {}
    for idx, _, _ in pa.faces:
        a, b, c = (Vector(pa.verts[i]) for i in idx[:3])
        n = (b - a).cross(c - a)
        if n.length > 1e-9:
            n.normalize()
            for i in idx:
                turned.setdefault(i, n)
    for i, (x, y, z) in enumerate(pa.verts):
        r = math.hypot(x, z)
        t = min(1.0, max(0.0, (r - 70.0) / 280.0))
        near, far = (0.014, 0.017, 0.022), (0.042, 0.052, 0.062)
        c = [near[k] + (far[k] - near[k]) * t for k in range(3)]
        # Its own tone, less of it the farther into the rain.
        own = CITY_TONE.get(i, (1.0, 1.0, 1.0))
        c = [c[k] * (1.0 + (own[k] - 1.0) * (1.0 - 0.55 * t)) for k in range(3)]
        # Paler up where the sky is on it.
        c = [v * (0.9 + 0.3 * min(1.0, max(0.0, (y - FLOOD) / 120.0))) for v in c]
        n = turned.get(i)
        if n is not None:
            lit = max(0.0, n.z)
            c = [c[k] * (1.0 - 0.14 * max(0.0, -n.z)) + near[1] * DUSK[k] * lit * (1.0 + 2.0 * t) for k in range(3)]
        mist = max(0.0, 1.0 - (y - FLOOD) / 22.0)
        c = [c[k] + (far[k] * 1.1 - c[k]) * mist * 0.45 for k in range(3)]
        pa.colors[i] = tuple(c)


def flood():
    s = 460.0
    rows = [[(-s + 2 * s * i / 8, FLOOD, -s + 2 * s * j / 8) for i in range(9)] for j in range(9)]
    surface(rows, 'flood', (0, 1, 0), lambda p: (p[0], p[2]))


def tone(p, n, name):
    """Over the occlusion: rain's streaks down every face, dark under drips and
    at a wall's foot; the concrete mottled."""
    x, y, z = p
    if name == 'rust':
        # The containers' and the cab's paint, under the rust in the photograph.
        if -3.6 <= x <= -1.0 and 6.5 <= z <= 12.7:
            return (0.34, 0.62, 0.58)
        if -1.0 <= x <= 5.0 and -14.0 <= z <= -5.5:
            return (1.0, 0.46, 0.22)
        if 1.4 <= x <= 4.0 and 3.3 <= z <= 5.7:
            return (0.7, 0.2, 0.15)
    mottle = 0.86 + 0.14 * math.sin(x * 0.53 + z * 0.31 + y * 0.17) * math.sin(z * 0.47 - x * 0.23 + y * 0.61)
    k = mottle
    if abs(n.y) < 0.5:
        streak = 0.5 + 0.5 * math.sin((x + z) * 3.1 + math.sin((x - z) * 1.3) * 1.7)
        k *= 1.0 - 0.18 * streak
        # Dirt splashed up the foot of a wall.
        foot = max(0.0, 1.0 - max(0.0, y) / 0.9) if y < 0.9 else 0.0
        k *= 1.0 - 0.3 * foot
    return (k, k, k)


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    flyover()
    for sx in (1, -1):
        red_half(sx)
    walls()
    wet_road()
    road_lines()
    puddles()
    pools()
    signs()
    city()
    paint_city()
    flood()
    objs = make_objects()
    def ground(verts, polys):
        base = len(verts)
        s = 34.0
        verts.extend([to_blender((-s, 0.0, -s)), to_blender((s, 0.0, -s)), to_blender((s, 0.0, s)), to_blender((-s, 0.0, s))])
        polys.append((base, base + 1, base + 2, base + 3))
    paint(objs, ground, tone)
    refl = mirror_parts(objs, {
        'reflect_light': ['light_warm', 'light_cold', 'light_pink', 'light_cyan', 'light_sodium', 'city_lights'],
        'reflect_city': ['city'],
    }, FLOOD, keep=lambda p: max(abs(p[0]), abs(p[2])) > 22.0, dim=DIM)
    # The road's reflection: the near things upside down under it, its lights
    # longer than they stand.
    inside = lambda p: abs(p[0]) <= 33.5 and abs(p[2]) <= 23.5 and p[1] < 13.0 and not (abs(p[0]) < 4.6 and p[1] > 4.5)
    wet = mirror_parts(objs, {'wet_blast': ['blast', 'blast_dark', 'road'], 'wet_rust': ['rust']}, 0.0, keep=inside, dim=WET_DIM)
    wet.update(mirror_parts(objs, {'wet_light': ['light_sodium', 'light_pink', 'light_cyan', 'light_red', 'light_blue', 'light_warm', 'light_cold']}, 0.0, keep=inside, dim=WET_DIM, stretch=WET_STRETCH_LIGHT))
    objs.update(refl)
    objs.update(wet)
    export(objs, OUT, 'overpass', bare=set(refl) | set(wet) | {'shaft', 'city', 'city_lights'})


main()

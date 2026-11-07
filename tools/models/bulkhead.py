"""Bulkhead's dock: everything built, in the map's own coordinates (GLBs at the
origin, `look/built.rs` draws them). A steel dock in orbit: a pit, lanes, a bridge
over it, a base at each end; frames and pipes and gantries; along the north wall a
viewport on a ringed planet, a freighter moored outside it.

    blender -b --python tools/models/bulkhead.py

What collides is `maps/bulkhead/map.json`'s hidden boxes; these numbers follow
them. The planet's picture is painted here (`textures/planet.jpg`), lit from one
side, as nothing out there moves.
"""
import math
import os
import random
import sys

import bpy
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import arch  # noqa: E402
from arch import PARTS as PARTS_  # noqa: E402
from arch import BACK, cbox, digits, extrude, export, make_objects, paint, panel, part, quad, rod, spin, strip_light, surface, to_blender, torus, tri  # noqa: E402,F401

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, '..', '..'))
OUT = os.path.join(ROOT, 'crates', 'client', 'assets', 'models', 'bulkhead')
TEX = os.path.join(ROOT, 'crates', 'client', 'assets', 'textures', 'planet.jpg')

arch.TILES.update({'hull': 2.3, 'plate': 2.2, 'dark': 2.2, 'tread': 2.0, 'red': 2.3, 'blue': 2.3, 'rust': 2.4})
arch.UNTRACED.update({'space', 'stars', 'planet', 'ring', 'shaft', 'ship', 'ship_dark'})

LANE = 1.5
DECK = 3.0
LID = 11.0
WIN_LO = 3.4
WIN_HI = 9.2
PLANET = (-75.0, 80.0, 326.0, 107.0)  # x, y, z, r: all of it inside the camera's far plane
SUN_DIR = np.array([-0.72, 0.25, -0.65])


def hazard(x0, y, z0, x1, z1, along_x=True, width=0.42):
    """Black and yellow chevrons laid flat, a hair over a floor."""
    length = (x1 - x0) if along_x else (z1 - z0)
    n = max(1, int(length / width))
    w = length / n
    for i in range(n):
        mat = 'paint_y' if i % 2 == 0 else 'paint_k'
        if along_x:
            a, b = x0 + i * w, x0 + (i + 1) * w
            quad((a, y, z0), (b, y, z0), (b + (z1 - z0) * 0.5, y, z1), (a + (z1 - z0) * 0.5, y, z1), mat, (0, 1, 0), cell=9.0)
        else:
            a, b = z0 + i * w, z0 + (i + 1) * w
            quad((x0, y, a), (x0, y, b), (x1, y, b + (x1 - x0) * 0.5), (x1, y, a + (x1 - x0) * 0.5), mat, (0, 1, 0), cell=9.0)


def crate(x0, y0, z0, x1, y1, z1, mat='plate'):
    """Cargo inside its box: a body set back, corner posts and banding to the plane."""
    t = 0.08
    cbox(x0 + t, y0, z0 + t, x1 - t, y1 - 0.06, z1 - t, mat, c=0.02)
    for x, z in ((x0, z0), (x1 - 0.14, z0), (x0, z1 - 0.14), (x1 - 0.14, z1 - 0.14)):
        cbox(x, y0, z, x + 0.14, y1, z + 0.14, 'dark', c=0.015, cell=3.0)
    for y in (y0 + 0.02, y1 - 0.12):
        cbox(x0, y, z0, x1, y + 0.1, z1, 'dark', c=0.015, cell=3.0)


# ---------------------------------------------------------------------- the hull
def hull():
    # North wall: dado to the viewport, its frames, what's over it.
    for x in range(-24, 24, 6):
        panel(x, LANE, 16.0, x + 6, WIN_LO, 17.0, '-z', pier=0.36, band=0.24, mat='hull', plinth='dark')
    cbox(-24.0, WIN_HI, 16.0, 24.0, LID, 17.0, 'hull', c=0.02, cell=1.6)
    cbox(-24.0, WIN_LO, 16.0, -20.5, WIN_HI, 17.0, 'hull', c=0.02, cell=1.6)
    cbox(20.5, WIN_LO, 16.0, 24.0, WIN_HI, 17.0, 'hull', c=0.02, cell=1.6)
    for x in range(-20, 21, 5):
        # A mullion: a deep steel frame, a line of light up its side.
        cbox(x - 0.28, WIN_LO, 16.0, x + 0.28, WIN_HI, 17.2, 'dark', c=0.03)
        strip_light(x - 0.3, WIN_LO + 0.2, 16.05, x - 0.28, WIN_HI - 0.2, 16.1, 'light_white')
    cbox(-20.5, WIN_LO - 0.12, 16.0, 20.5, WIN_LO, 17.2, 'dark', c=0.03, cell=2.0)
    cbox(-20.5, WIN_HI, 16.0, 20.5, WIN_HI + 0.12, 17.2, 'dark', c=0.03, cell=2.0)
    # South wall: panels, big pipes along them, valves, a recessed tank each side.
    for x in range(-24, 24, 6):
        panel(x, LANE, -17.0, x + 6, 6.3, -16.0, '+z', pier=0.36, band=0.24, mat='hull', plinth='dark')
        panel(x, 6.3, -17.0, x + 6, LID, -16.0, '+z', pier=0.36, band=0.24, mat='hull', plinth='dark')
    for i, x in enumerate(range(-24, 24, 6)):
        digits(f'{i + 1:02d}', (x + 1.4, 6.9, -15.86), (1, 0), (0, 1), 1.5, 'paint_y_box' if False else 'paint_w')
        cbox(x + 0.6, 6.55, -15.87, x + 5.4, 6.68, -15.85, 'paint_y', c=0.0, cell=9.0)
    for y, r in ((4.7, 0.22), (5.3, 0.14), (9.0, 0.3)):

        rod((-24.0, y, -15.72), (24.0, y, -15.72), r, 'pipe', sides=10)
    for x in (-18.0, -6.0, 6.0, 18.0):
        for y, r in ((4.7, 0.3), (9.0, 0.38)):
            rod((x - 0.14, y, -15.72), (x + 0.14, y, -15.72), r, 'dark', sides=10)
        rod((x, 2.0, -15.72), (x, 5.2, -15.72), 0.1, 'pipe', sides=8)
    # The bases' ends: a great blast door shut, a side's stripes on it.
    for s, paint_mat, light in ((-1, 'red', 'light_red'), (1, 'blue', 'light_blue')):
        x0, x1 = sorted((s * 25.0, s * 24.0))
        cbox(x0, 0.0, -16.0, x1, LID, 16.0, 'hull', c=0.0, cell=1.6)
        def wall_box(a, y0, z0, b, y1, z1, mat='dark', c=0.03):
            lo, hi = sorted((s * a, s * b))
            cbox(lo, y0, z0, hi, y1, z1, mat, c=c)
        wall_box(24.0, 3.0, -6.2, 23.86, 9.8, -5.6)
        wall_box(24.0, 3.0, 5.6, 23.86, 9.8, 6.2)
        wall_box(24.0, 9.2, -6.2, 23.86, 9.8, 6.2)
        for z0, z1 in ((-5.6, 0.0), (0.0, 5.6)):
            wall_box(24.0, 3.0, z0 + 0.02, 23.92, 9.2, z1 - 0.02, paint_mat, c=0.02)
        lo, hi = sorted((s * 23.92, s * 23.9))
        strip_light(lo, 3.2, -0.05, hi, 9.0, 0.05, 'light_white')
        strip_light(lo, 4.4, -5.4, hi, 4.6, 5.4, light)
        for z in (-3.6, 3.6):
            wall_box(23.92, 5.2, z - 1.2, 23.88, 7.6, z + 1.2, 'dark', c=0.0)
    # Corners out of sight of the fight but not of the viewport.
    for s in (-1, 1):
        cbox(-25.0, 0.0, s * 16.0 if s > 0 else -17.0, 25.0, LANE, s * 17.0 if s > 0 else -16.0, 'dark', c=0.0, cell=3.0)


def roof():
    # The lid and its frames, north to south; a truss east to west on them.
    cbox(-25.0, LID - 0.06, -17.0, 25.0, LID, 17.0, 'dark', c=0.0, cell=2.4)
    for x in range(-24, 25, 4):
        beam = x
        cbox(beam - 0.2, LID - 0.7, -16.0, beam + 0.2, LID - 0.06, 16.0, 'hull', c=0.03, cell=2.0)
        cbox(beam - 0.34, LID - 0.8, -16.0, beam + 0.34, LID - 0.7, 16.0, 'dark', c=0.02, cell=3.0)
    # Panels of light between the frames, over the bridge.
    for x in (-18.0, -10.0, -2.0, 2.0, 10.0, 18.0):
        cbox(x - 1.4, LID - 0.5, -2.2, x + 1.4, LID - 0.44, 2.2, 'dark', c=0.0, cell=3.0)
        strip_light(x - 1.26, LID - 0.52, -2.04, x + 1.26, LID - 0.5, 2.04, 'light_white')
    # The crane over top mid, on rails along the walls.
    for z in (-15.2, 15.2):
        cbox(-24.0, 9.3, z - 0.3, 24.0, 9.7, z + 0.3, 'dark', c=0.03, cell=3.0)
    for x in (-1.5, 1.5):
        cbox(x - 0.3, 8.9, -15.5, x + 0.3, 9.3, 15.5, 'crane', c=0.04, cell=2.4)
    for z in range(-14, 15, 4):
        cbox(-1.5, 8.94, z - 0.1, 1.5, 9.2, z + 0.1, 'crane', c=0.02, cell=3.0)
    cbox(-1.9, 7.9, -1.4, 1.9, 8.9, 1.4, 'dark', c=0.08)
    cbox(-1.6, 7.7, -1.1, 1.6, 7.9, 1.1, 'hull', c=0.05)
    for x, z in ((-1.1, -0.8), (1.1, -0.8), (-1.1, 0.8), (1.1, 0.8)):
        rod((x, 7.7, z), (x * 0.6, 6.3, z * 0.6), 0.03, 'pipe', sides=6)
    spin(0.0, 0.0, [(0.001, 6.0), (0.8, 6.0), (0.8, 6.24), (0.5, 6.42), (0.001, 6.42)], 'dark', sides=20, smooth=False)


# ------------------------------------------------------------------ what's fought on
def lanes():
    """A lane 1.5 m up along each side, its face to the pit in panels and a strip."""
    for s in (-1, 1):
        z0, z1 = sorted((s * 12.0, s * 16.0))
        for x in range(-24, 24, 6):
            cbox(x, LANE - 0.1, z0, x + 6, LANE, z1, 'tread', c=0.0, cell=1.6)
        face = '-z' if s > 0 else '+z'
        zf0, zf1 = sorted((s * 12.0, s * 12.4))
        edges = [-24, -20, -10, -7, 7, 10, 20, 24]
        for a, b in zip(edges, edges[1:]):
            panel(a, 0.0, zf0, b, LANE - 0.1, zf1, face, pier=0.26, band=0.2, mat='hull', plinth='dark')
        # A strip of amber along it.
        for a, b in ((-20.0, -10.0), (-7.0, 7.0), (10.0, 20.0)):
            zl0, zl1 = sorted((s * 11.99, s * 11.97))
            strip_light(a + 0.3, 1.2, zl0, b - 0.3, 1.26, zl1, 'light_amber')
        # The lane's rail side: a yellow line at its edge.
        zy0, zy1 = sorted((s * 12.04, s * 12.14))
        cbox(-24.0, LANE, zy0, 24.0, LANE + 0.006, zy1, 'paint_y', c=0.0, cell=6.0)
        # Steps into the pit, each side of it: the map's boxes, 3 m wide.
        for sx in (-1, 1):
            xa, xb = sorted((sx * 10.0, sx * 7.0))
            for za, zb, h in ((11.0, 12.0, 1.0), (10.0, 11.0, 0.5)):
                lo, hi = sorted((s * za, s * zb))
                cbox(xa, 0.0, lo, xb, h, hi, 'tread', c=0.03)
        # Steps up into the base, inside the map's three.
        for sx in (-1, 1):
            xa, xb = sorted((sx * 24.0, sx * 20.0))
            for za, zb, h in ((10.5, 12.0, 2.0), (9.0, 10.5, 2.5), (7.5, 9.0, 3.0)):
                lo, hi = sorted((s * za, s * zb))
                cbox(xa, 0.0, lo, xb, h, hi, 'tread', c=0.03)
    for sx in (-1, 1):
        for sz in (-1, 1):
            # The lane's cover: cargo. Map: x 13..14, 13..14.5 ; x 3..4, 14.6..16.
            a, b = sorted((sx * 14.0, sx * 13.0))
            c, d = sorted((sz * 13.0, sz * 14.5))
            crate(a, LANE, c, b, 2.7, d, 'rust')
            a, b = sorted((sx * 4.0, sx * 3.0))
            c, d = sorted((sz * 14.6, sz * 16.0))
            crate(a, LANE, c, b, 2.6, d, 'dark')


def bridge():
    """The bridge deck to deck: a girder each side, cross-braced, tread on top."""
    for x0, x1 in ((-18.0, -3.0), (3.0, 18.0)):
        cbox(x0, DECK - 0.12, -1.5, x1, DECK, 1.5, 'tread', c=0.0, cell=1.6)
        for z in (-1.5, 1.34):
            cbox(x0, 2.2, z, x1, DECK - 0.12, z + 0.16, 'hull', c=0.02, cell=1.6)
        # Cross-frames under it.
        x = x0 + 1.5
        while x < x1:
            cbox(x - 0.1, 2.2, -1.34, x + 0.1, DECK - 0.12, 1.34, 'dark', c=0.02, cell=2.0)
            x += 3.0
        # A strip along each side, under the edge.
        for z0, z1 in ((-1.53, -1.5), (1.5, 1.53)):
            strip_light(x0 + 0.2, 2.72, z0, x1 - 0.2, 2.78, z1, 'light_cyan')
    # Top mid.
    cbox(-3.0, DECK - 0.12, -3.5, 3.0, DECK, 3.5, 'tread', c=0.0, cell=1.6)
    cbox(-3.0, 2.2, -3.5, 3.0, DECK - 0.12, 3.5, 'hull', c=0.02, cell=1.6)
    spin(0.0, 0.0, [(1.22, DECK + 0.004), (1.3, DECK + 0.004)][::-1], 'paint_y', sides=40, smooth=False)
    for s in (-1, 1):
        lo, hi = sorted((s * 3.53, s * 3.5))
        strip_light(-2.8, 2.72, lo, 2.8, 2.78, hi, 'light_cyan')
    for sx in (-1, 1):
        for sz in (-1, 1):
            a, b = sorted((sx * 3.0, sx * 2.4))
            c, d = sorted((sz * 2.0, sz * 3.5))
            crate(a, DECK, c, b, 3.9, d, 'dark')
    # Its piers: columns in the pit carrying the bridge.
    for sx in (-1, 1):
        for x in (8.0, 14.0):
            a, b = sorted((sx * (x - 0.4), sx * (x + 0.4)))
            cbox(a, 0.0, -0.5, b, 2.2, 0.5, 'hull', c=0.08)
            cbox(a, 0.0, -0.5, b, 0.24, 0.5, 'dark', c=0.03)
            lo, hi = sorted((sx * (x - 0.42), sx * (x - 0.4)))
            strip_light(lo, 0.5, -0.04, hi, 2.0, 0.04, 'light_cyan')


def bases():
    for s, paint_mat, light in ((-1, 'red', 'light_red'), (1, 'blue', 'light_blue')):
        def bx(a, y0, z0, b, y1, z1, mat='hull', c=0.03, **kw):
            lo, hi = sorted((s * a, s * b))
            cbox(lo, y0, z0, hi, y1, z1, mat, c=c, **kw)
        # The deck over the hangar: tread on a painted edge.
        bx(24.0, DECK - 0.12, -7.5, 18.0, DECK, 7.5, 'tread', c=0.0, cell=1.6)
        bx(24.0, 2.2, -7.5, 18.0, DECK - 0.12, 7.5, paint_mat, c=0.02, cell=1.6)
        lo, hi = sorted((s * 18.03, s * 18.0))
        strip_light(lo, 2.72, -7.3, hi, 2.78, -1.6, light)
        strip_light(lo, 2.72, 1.6, hi, 2.78, 7.3, light)
        # Its front's columns and walls, in the map's boxes.
        for z0, z1 in ((-7.5, -6.9), (6.9, 7.5)):
            bx(18.6, 0.0, z0, 18.0, 2.2, z1, 'hull', c=0.05)
        for z0, z1 in ((-7.5, -3.0), (3.0, 7.5)):
            bx(18.4, DECK, z0, 18.0, 5.18, z1, paint_mat, c=0.03)
            bx(18.4, 5.18, z0, 18.0, 5.3, z1, 'dark', c=0.03)
            lo2, hi2 = sorted((s * 18.0, s * 17.98))
            strip_light(lo2, 4.6, z0 + 0.2, hi2, 4.66, z1 - 0.2, light)
        # The shutters either side of the hangar's door, ribbed.
        for z0, z1 in ((-6.9, -3.0), (3.0, 6.9)):
            bx(18.6, 0.0, z0, 18.0, 2.2, z1, 'hull', c=0.03)
            for y in (0.5, 0.9, 1.3, 1.7):
                lo2, hi2 = sorted((s * 18.0, s * 17.99))
                cbox(lo2, y, z0 + 0.08, hi2, y + 0.1, z1 - 0.08, 'dark', c=0.0, cell=3.0)
        for z0, z1 in ((-3.3, -3.0), (3.0, 3.3)):
            bx(18.7, 0.0, z0, 18.0, 2.2, z1, 'dark', c=0.04)
        # The hangar under: its low wall, the door's control, cargo.
        bx(24.0, 0.0, -2.0, 23.2, 1.0, 2.0, 'dark', c=0.05)
        for z0, z1 in ((-6.0, -3.5), (3.5, 6.0)):
            a, b = sorted((s * 16.6, s * 16.0))
            crate(a, 0.0, z0, b, 1.1, z1, paint_mat)
        # Ribs and a strip under the deck, its side's colour.
        for z in (-6.0, -3.0, 0.0, 3.0, 6.0):
            bx(24.0, 1.98, z - 0.12, 18.6, 2.2, z + 0.12, 'dark', c=0.02, cell=2.0)
        lo, hi = sorted((s * 23.2, s * 19.0))
        strip_light(lo, 1.94, -0.05, hi, 1.98, 0.05, light)
        # Floor: a parking box in hazard lines.
        lo, hi = sorted((s * 23.0, s * 19.0))
        hazard(lo, 0.012, -5.2, hi, -4.9)
        hazard(lo, 0.012, 4.9, hi, 5.2)


def bulkheads():
    """A bulkhead across each lane each side of the middle: a steel frame, its door
    in the middle of the lane, hazard round the door, a lit head over it."""
    for sx in (-1, 1):
        for sz in (-1, 1):
            a, b = sorted((sx * 6.4, sx * 5.6))
            def bx(y0, z0, y1, z1, mat='hull', c=0.03):
                lo, hi = sorted((sz * z0, sz * z1))
                cbox(a, y0, lo, b, y1, hi, mat, c=c)
            bx(LANE, 12.0, 6.4, 13.1)
            bx(LANE, 14.9, 6.4, 16.0)
            bx(4.1, 13.1, 6.4, 14.9)
            # Its jambs and head in dark steel, hazard on the jambs' faces.
            bx(LANE, 12.94, 4.1, 13.1, 'dark', c=0.02)
            bx(LANE, 14.9, 4.1, 15.06, 'dark', c=0.02)
            bx(3.94, 13.1, 4.1, 14.9, 'dark', c=0.02)
            lo, hi = sorted((sz * 13.14, sz * 14.86))
            la, lb = sorted((sx * 6.42, sx * 6.4))
            strip_light(la, 4.16, lo, lb, 4.22, hi, 'light_amber')
            la, lb = sorted((sx * 5.6, sx * 5.58))
            strip_light(la, 4.16, lo, lb, 4.22, hi, 'light_amber')
            for x0, x1 in (sorted((sx * 6.42, sx * 6.4)), sorted((sx * 5.6, sx * 5.58))):
                for z0, z1 in ((12.5, 12.9), (15.1, 15.5)):
                    n = 6
                    for i in range(n):
                        y0 = LANE + 0.1 + i * 0.4
                        lo, hi = sorted((sz * z0, sz * z1))
                        cbox(x0, y0, lo, x1, y0 + 0.4, hi, 'paint_y' if i % 2 == 0 else 'paint_k', c=0.0, cell=3.0)


def pit():
    # Its cover, in the map's boxes.
    for sx in (-1, 1):
        for x0, x1, z0, z1, y1, mat in ((13.0, 12.4, -7.0, -4.0, 1.2, 'plate'), (13.0, 12.4, 4.0, 7.0, 1.2, 'plate'),
                                        (5.0, 3.6, -7.0, -5.6, 2.2, 'dark'), (5.0, 3.6, 5.6, 7.0, 2.2, 'dark')):
            a, b = sorted((sx * x0, sx * x1))
            crate(a, 0.0, z0, b, y1, z1, 'rust' if mat == 'plate' else 'plate')
        # The cover on the bridge, in its boxes.
        a, b = sorted((sx * 10.5, sx * 9.5))
        crate(a, DECK, 0.4, b, 4.0, 1.5, 'plate')
        a, b = sorted((sx * 14.5, sx * 13.5))
        crate(a, DECK, -1.5, b, 4.0, -0.4, 'dark')
    # The dais's ring, the lifts' pads' hazard, lines down the pit.
    spin(0.0, 0.0, [(1.4, 0.008), (1.56, 0.008)][::-1], 'paint_y', sides=40, smooth=False)
    for z in (-6.5, 6.5):
        hazard(-1.45, 0.01, z - 1.45, 1.45, z - 1.25)
        hazard(-1.45, 0.01, z + 1.25, 1.45, z + 1.45)
    for x in (-16.0, 16.0):
        cbox(x - 0.06, 0.0, -11.9, x + 0.06, 0.006, 11.9, 'paint_y', c=0.0, cell=6.0)


# ------------------------------------------------------------------- what's outside
def space():
    """Black, stars, a planet with its rings, a freighter moored outside, all far
    off and never fogged."""
    # The black behind it all.
    s = 590.0
    for a, b, c, d, o in (
        ((-s, -s, s), (s, -s, s), (s, s, s), (-s, s, s), (0, 0, -1)),
        ((-s, s, -s), (s, s, -s), (s, s, s), (-s, s, s), (0, -1, 0)),
        ((-s, -s, -s), (s, -s, -s), (s, -s, s), (-s, -s, s), (0, 1, 0)),
        ((-s, -s, -s), (-s, s, -s), (-s, s, s), (-s, -s, s), (1, 0, 0)),
        ((s, -s, -s), (s, s, -s), (s, s, s), (s, -s, s), (-1, 0, 0)),
    ):
        quad(a, b, c, d, 'space', o, cell=4000.0)
    pa = part('space')
    for i in range(len(pa.verts)):
        pa.colors[i] = (0.0, 0.0, 0.0)
    # Stars: small quads turned to the dock, brighter ones few.
    rng = random.Random(3)
    st = part('stars')
    for _ in range(2600):
        z = rng.random() * 1.0 + 0.02
        a = rng.uniform(0.0, math.tau)
        d = Vectorish(rng.gauss(0, 1), rng.gauss(0, 0.7), abs(rng.gauss(0, 1)) + 0.25)
        x, y, zz = d
        r = 560.0
        px, py, pz = x * r, y * r, zz * r
        size = rng.choice((0.42, 0.42, 0.42, 0.54, 0.7, 1.0))
        tx, tz = (zz, -x)
        l = math.hypot(tx, tz) or 1.0
        tx, tz = tx / l * size, tz / l * size
        base = st.add([(px - tx, py - size, pz - tz), (px + tx, py - size, pz + tz), (px + tx, py + size, pz + tz), (px - tx, py + size, pz - tz)])
        v = rng.uniform(0.25, 1.0) ** 2 * (2.6 if size > 0.6 else 1.2)
        tint = rng.choice(((1.0, 1.0, 1.0), (0.82, 0.9, 1.0), (1.0, 0.9, 0.78)))
        for k in range(4):
            st.colors[base + k] = (v * tint[0], v * tint[1], v * tint[2])
        st.faces.append(((base, base + 3, base + 2, base + 1), [(0, 0)] * 4, True))
    # The planet: a sphere facing us, its map painted below.
    px, py, pz, pr = PLANET
    pl = part('planet')
    rows, cols = 48, 96
    base = pl.add([(px + pr * math.sin(math.pi * j / rows) * math.cos(math.tau * i / cols), py + pr * math.cos(math.pi * j / rows), pz + pr * math.sin(math.pi * j / rows) * math.sin(math.tau * i / cols))
                   for j in range(rows + 1) for i in range(cols + 1)])
    for j in range(rows):
        for i in range(cols):
            q = base + j * (cols + 1) + i
            # Wound to face out of the sphere: seen from the dock, its near side.
            uv = [(i / cols, 1 - j / rows), ((i + 1) / cols, 1 - j / rows), ((i + 1) / cols, 1 - (j + 1) / rows), (i / cols, 1 - (j + 1) / rows)]
            pl.faces.append(((q, q + 1, q + cols + 2, q + cols + 1), uv, True))
    # Its air: a soft bright rim round the disc, added.
    sh = part('shaft')
    cx, cy, cz = px, py, pz - pr * 0.1
    n = 96
    rim = [(pr * 0.97, 0.0), (pr * 1.0, 0.0), (pr * 1.012, 0.55), (pr * 1.05, 0.0)]
    fx, fz = 1.0, 0.0
    base = sh.add([(cx + r * math.cos(math.tau * i / n), cy + r * math.sin(math.tau * i / n), cz) for r, _ in rim for i in range(n)])
    lit = np.array([-0.72, 0.25])
    lit = lit / np.linalg.norm(lit)
    for ring, (r, k) in enumerate(rim):
        for i in range(n):
            a = math.tau * i / n
            side = max(0.0, math.cos(a) * lit[0] + math.sin(a) * lit[1]) ** 1.2
            v = k * side
            sh.colors[base + ring * n + i] = (v * 0.42, v * 0.7, v * 1.0)
    for ring in range(len(rim) - 1):
        for i in range(n):
            i1 = (i + 1) % n
            q = (base + ring * n + i, base + ring * n + i1, base + (ring + 1) * n + i1, base + (ring + 1) * n + i)
            sh.faces.append((q, [(0, 0)] * 4, True))
    # Its rings, tilted, the near arc over its face.
    rp = part('ring')
    tilt = math.radians(22.0)
    roll = math.radians(-17.0)
    def ring_at(a, r):
        x, z = math.cos(a) * r, math.sin(a) * r
        y = z * math.sin(tilt)
        z2 = z * math.cos(tilt)
        return (px + x * math.cos(roll) - y * math.sin(roll), py + x * math.sin(roll) + y * math.cos(roll), pz + z2)
    bands = [(1.28, 0.02), (1.36, 0.14), (1.5, 0.22), (1.53, 0.03), (1.58, 0.16), (1.84, 0.1), (1.92, 0.0)]
    n = 128
    base = rp.add([ring_at(math.tau * i / n, pr * r) for r, _ in bands for i in range(n)])
    for b, (r, k) in enumerate(bands):
        for i in range(n):
            a = math.tau * i / n
            # The planet's shadow across the far side of the rings.
            shade = 0.12 if (0.7 < a < 1.75) else 1.0
            v = k * shade
            rp.colors[base + b * n + i] = (v * 0.8, v * 0.72, v * 0.6)
    for b in range(len(bands) - 1):
        for i in range(n):
            i1 = (i + 1) % n
            rp.faces.append(((base + b * n + i, base + b * n + i1, base + (b + 1) * n + i1, base + (b + 1) * n + i), [(0, 0)] * 4, True))
    # The dock's own arm reaching out, and a freighter moored at it.
    for x0, x1, y0, y1, z0, z1 in ((-120.0, 120.0, -14.0, -2.0, 17.0, 62.0),):
        cbox(x0, y0, z0, x1, y1, z1, 'hull', c=0.0, cell=30.0)
    ship(48.0, 9.0, 150.0)
    light_ship()


def light_ship():
    """The freighter has no light of its own here: the planet's day on its side,
    painted, from up and to its left."""
    for name in ('ship', 'ship_dark'):
        pa = PARTS_[name] if name in PARTS_ else None
        if pa is None:
            continue
        for i, (x, y, z) in enumerate(pa.verts):
            t = 0.5 + 0.5 * max(-1.0, min(1.0, (y - 9.0) / 12.0))
            k = (0.05 + 0.2 * t) * (1.0 if name == 'ship' else 0.5)
            pa.colors[i] = (k * 0.85, k * 0.92, k * 1.05)


def ship(cx, cy, cz):
    """A blunt freighter in profile: a long hull, a bridge block, tanks, its running
    lights, far enough that its shape is what reads."""
    cbox(cx - 62, cy - 7, cz - 11, cx + 62, cy + 7, cz + 11, 'ship', c=2.0, cell=8.0)
    cbox(cx - 74, cy - 4, cz - 8, cx - 62, cy + 4, cz + 8, 'ship_dark', c=2.0, cell=8.0)
    cbox(cx + 44, cy + 7, cz - 7, cx + 60, cy + 18, cz + 7, 'ship', c=1.0, cell=8.0)
    cbox(cx + 48, cy + 18, cz - 3, cx + 57, cy + 22, cz + 3, 'ship_dark', c=0.5, cell=8.0)
    for x in range(-52, 40, 16):
        cbox(cx + x, cy + 7, cz - 8, cx + x + 13, cy + 12, cz + 8, 'ship_dark', c=0.6, cell=8.0)
    for x in (cx - 40, cx - 10, cx + 20):
        cbox(x - 0.7, cy - 7.6, cz - 11.4, x + 0.7, cy + 6.6, cz - 11.0, 'ship_dark', c=0.0, cell=8.0)
    rng = random.Random(11)
    for _ in range(40):
        x = cx + rng.uniform(-60, 60)
        y = cy + rng.uniform(-5, 5)
        strip_light(x - 0.8, y - 0.2, cz - 11.06, x + 0.8, y + 0.2, cz - 11.0, 'light_cold')
    for x, m in ((cx - 74.2, 'light_red'), (cx + 62.2, 'light_blue')):
        strip_light(x - 0.4, cy - 0.4, cz - 8.4, x + 0.4, cy + 0.4, cz - 7.6, m)


def Vectorish(x, y, z):
    l = math.sqrt(x * x + y * y + z * z) or 1.0
    return (x / l, y / l, z / l)


def paint_planet():
    """The planet's map: a lit gas giant in bands and storms, its dark side black."""
    h, w = 512, 1024
    rng = np.random.default_rng(5)
    v, u = np.meshgrid(np.linspace(0, 1, h), np.linspace(0, 1, w, endpoint=False), indexing='ij')
    def noise(fx, fy, seed):
        g = np.random.default_rng(seed).standard_normal((fy, fx))
        from numpy.fft import ifft2, fft2
        # Smooth by low-passing: a wrapped band-limited field.
        ky = np.fft.fftfreq(h)[:, None] * h
        kx = np.fft.fftfreq(w)[None, :] * w
        spec = np.random.default_rng(seed).standard_normal((h, w)) + 1j * np.random.default_rng(seed + 1).standard_normal((h, w))
        amp = np.exp(-(kx / fx) ** 2 - (ky / fy) ** 2)
        f = np.real(np.fft.ifft2(spec * amp))
        return f / np.abs(f).max()
    warp = noise(14, 40, 1) * 0.05
    bands = np.sin((v + warp) * math.pi * 19) * 0.34 + np.sin((v + warp * 1.6) * math.pi * 47 + 1.0) * 0.16 + noise(90, 180, 2) * 0.24
    t = np.clip(bands * 0.5 + 0.5, 0, 1)[..., None]
    a, b, c = np.array([0.46, 0.34, 0.26]), np.array([0.86, 0.71, 0.52]), np.array([0.62, 0.64, 0.68])
    col = a * (1 - t) ** 2 + b * 2 * t * (1 - t) + c * t ** 2
    # A storm.
    du, dv = (u - 0.68) * 2.0, (v - 0.56) * 3.6
    storm = np.exp(-(du * du + dv * dv) * 240.0)[..., None]
    col = col * (1 - storm) + np.array([0.8, 0.42, 0.24]) * storm
    # Lit from the sun's side: the sphere's normals, the terminator soft.
    theta, phi = v * math.pi, u * math.tau
    nx, ny, nz = np.sin(theta) * np.cos(phi), np.cos(theta), np.sin(theta) * np.sin(phi)
    light = SUN_DIR / np.linalg.norm(SUN_DIR)
    d = nx * light[0] + ny * light[1] + nz * light[2]
    lit = np.clip(d * 1.5 + 0.06, 0.0, 1.0)[..., None] ** 0.8
    col = col * lit
    col = np.clip(col, 0, 1) ** (1 / 2.2)
    print('PLANET mean', float(col.mean()), 'max', float(col.max()), 'nan', int(np.isnan(col).sum()))
    # Written raw and cut by the convert tools/ground.sh uses: Blender's own JPEG
    # save reads its settings from a scene a script doesn't have.
    raw = TEX + '.ppm'
    with open(raw, 'wb') as f:
        f.write(f'P6 {w} {h} 255\n'.encode())
        f.write((np.nan_to_num(col) * 255.0).astype(np.uint8).tobytes())
    os.system(f"convert '{raw}' -strip -quality 86 '{TEX}' && rm '{raw}'")


def tone(p, n, name):
    """Over the occlusion: scuffed low, a mottle of old paint, oil down from the
    frames on a wall."""
    x, y, z = p
    k = 0.88 + 0.12 * math.sin(x * 0.71 + z * 0.43) * math.sin(z * 0.59 - x * 0.31 + y * 0.77)
    if abs(n.y) < 0.5:
        k *= 1.0 - 0.28 * max(0.0, 1.0 - y / 1.1) if y >= 0 else 0.72
        streak = 0.5 + 0.5 * math.sin((x + z) * 2.7 + math.sin((x - z) * 1.1) * 2.2)
        k *= 1.0 - 0.12 * streak
    return (k, k, k)


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    paint_planet()
    hull()
    roof()
    lanes()
    bridge()
    bases()
    bulkheads()
    pit()
    space()
    objs = make_objects()
    def ground(verts, polys):
        base = len(verts)
        s = 30.0
        verts.extend([to_blender((-s, 0.0, -s)), to_blender((s, 0.0, -s)), to_blender((s, 0.0, s)), to_blender((-s, 0.0, s))])
        polys.append((base, base + 1, base + 2, base + 3))
    paint(objs, ground, tone)
    export(objs, OUT, 'dock', bare={'space', 'stars', 'planet', 'ring', 'shaft', 'ship', 'ship_dark'})


main()

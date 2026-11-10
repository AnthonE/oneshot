"""Pithead's mine: everything built, in the map's own coordinates (GLBs at the
origin, `look/built.rs` draws them). A coal mine's pithead high in the mountains
in a blizzard: at each end a shaft house clad in its side's painted corrugated
steel, a steel headframe on its roof turning its sheave wheels, cables off to a
winding house on a crag below the lip; between them the open shaft, and hung in it a truss
gantry from crown to crown through the winding cab, the sheave deck on the
cab's roof, the cage across under it and the collar under that, all on legs
going down into the cloud; plate-girder decks along the long sides from crag
to crag; rime on everything the wind meets, icicles under every edge it drips
from. Out in the snow, crags standing out of the cloud, a cableway's trestles
walking off toward the peaks.

    blender -b --python tools/models/pithead.py

What collides is `maps/pithead/map.json`'s hidden boxes; these numbers follow
them, red's side as the JSON writes it, blue's turned about the middle. The
wind is the mood's (`look/mood.rs`, BLIZZARD): rime builds on what faces it.
"""
import math
import os
import random
import sys

import bpy
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import arch  # noqa: E402
from arch import CELL, cbox, export, make_objects, paint, part, plank, quad, rod, spin, surface, to_blender, torus, tri  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, '..', '..'))
OUT = os.path.join(ROOT, 'crates', 'client', 'assets', 'models', 'pithead')

arch.TILES.update({'blast': 3.6, 'blast_dark': 3.6, 'floor': 3.6, 'snow': 2.6, 'rust': 2.4, 'tread': 2.0})
arch.UNTRACED.update({'drift', 'peaks', 'window', 'pool', 'track'})

# Toward where the wind blows (x, z): the mood's `wind_azim` 1.25. Rime faces
# the other way.
WIND = (math.sin(1.25), math.cos(1.25))
UPWIND = (-WIND[0], 0.0, -WIND[1])
# The corrugation: crest to crest, and how deep, m.
PITCH = 0.2
CORR = 0.04
# The paint, linear, before the occlusion: each side's cladding and the rest.
RED = (0.19, 0.032, 0.02)
BLUE = (0.018, 0.065, 0.25)
GREYGREEN = (0.1, 0.12, 0.115)
STEEL = (0.04, 0.042, 0.046)
RUSTY = (0.13, 0.06, 0.035)
# The shaft's old steel: rust-brown gone dull, never a side's red.
HUB_STEEL = (0.085, 0.055, 0.045)
GALV = (0.16, 0.17, 0.18)
RIME = (0.74, 0.79, 0.86)
# Rime feathered on an upright face: a grey crust over the paint, never the
# white of what lies on a top.
RIME_SIDE = (0.3, 0.33, 0.38)
SNOWY = (0.8, 0.85, 0.92)
ROCK = (0.045, 0.05, 0.062)
# The lamps (map.json's, x z y as it writes them, both sides): warm light
# painted on what they see.
LAMPS = [(-17.0, 5.8, 3.0), (-17.0, 2.7, 10.0), (0.0, 9.6, 1.8), (-7.5, 6.2, 11.0), (0.0, 2.8, 2.6)]
# The lamps over the doors to the bridge and the spur, painted only: each
# shines down its wall and onto what is before it (`pool`, at its floor's
# height), no light of its own.
DOOR_LAMPS = [(-12.62, 3.3, 2.5, '+x', 0.1), (-16.75, 3.3, -2.78, '-z', 0.0)]
LAMPS = LAMPS + [(x, y, z) for x, y, z, _, _ in DOOR_LAMPS]
LAMPS = LAMPS + [(-x, y, -z) for x, y, z in LAMPS]
LAMP_RGB = (1.0, 0.66, 0.36)
# The sheave wheels: where they turn about (x, y), their z, their radius
# (the client's `built::PITHEAD` spins them about this line).
SHEAVE = (-15.6, 18.6)
SHEAVE_Z = (6.0, 9.8)
SHEAVE_R = 2.1


def _lattice(i, j, seed):
    h = (i * 374761393 + j * 668265263 + seed * 2147483647) & 0xffffffff
    h = ((h ^ (h >> 13)) * 1274126177) & 0xffffffff
    return ((h ^ (h >> 16)) & 0xffff) / 65535.0


def vnoise(x, z, seed=0, octaves=3):
    """Value noise over the plan, 0..1, no period anyone will see."""
    total, amp, norm, f = 0.0, 1.0, 0.0, 1.0
    for o in range(octaves):
        px, pz = x * f + o * 17.3, z * f - o * 9.1
        i, j = math.floor(px), math.floor(pz)
        fx, fz = px - i, pz - j
        ux, uz = fx * fx * (3 - 2 * fx), fz * fz * (3 - 2 * fz)
        a, b = _lattice(i, j, seed + o), _lattice(i + 1, j, seed + o)
        c, d = _lattice(i, j + 1, seed + o), _lattice(i + 1, j + 1, seed + o)
        total += amp * ((a * (1 - ux) + b * ux) * (1 - uz) + (c * (1 - ux) + d * ux) * uz)
        norm += amp
        amp *= 0.5
        f *= 2.03
    return total / norm


def face(pa, idx, out, smooth=False):
    """A face on a part from its vertex indices, wound to look along `out` (the
    kit's rule: (b - a) x (last - a) is where a face looks)."""
    a, b, d = (pa.verts[idx[0]], pa.verts[idx[1]], pa.verts[idx[-1]])
    u = (b[0] - a[0], b[1] - a[1], b[2] - a[2])
    v = (d[0] - a[0], d[1] - a[1], d[2] - a[2])
    n = (u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0])
    if n[0] * out[0] + n[1] * out[1] + n[2] * out[2] < 0:
        idx = tuple(reversed(idx))
    pa.faces.append((tuple(idx), [(0, 0)] * len(idx), smooth))


class Side:
    """Red's side as the JSON writes it (s = 1), or blue's, turned half about the
    middle (s = -1): every point and direction through `p` and `o`."""

    def __init__(self, s):
        self.s = s
        self.team = 'red' if s > 0 else 'blue'

    def p(self, x, y, z):
        return (self.s * x, y, self.s * z)

    def o(self, x, y, z):
        return (self.s * x, y, self.s * z)

    def box(self, x0, y0, z0, x1, y1, z1, mat, **kw):
        a, b = self.p(x0, y0, z0), self.p(x1, y1, z1)
        cbox(min(a[0], b[0]), y0, min(a[2], b[2]), max(a[0], b[0]), y1, max(a[2], b[2]), mat, **kw)

    def quad(self, a, b, c, d, mat, out, cell=CELL):
        quad(self.p(*a), self.p(*b), self.p(*c), self.p(*d), mat, self.o(*out), cell)

    def tri(self, a, b, c, mat, out):
        tri(self.p(*a), self.p(*b), self.p(*c), mat, self.o(*out))

    def rod(self, a, b, r, mat, sides=8, taper=1.0):
        rod(self.p(*a), self.p(*b), r, mat, sides, taper)


SIDES = (Side(1), Side(-1))


def face_axes(face):
    """A wall face's outward normal and which axis runs along it."""
    sign = 1.0 if face[0] == '+' else -1.0
    if face[1] == 'x':
        return (sign, 0.0, 0.0), 'z'
    return (0.0, 0.0, sign), 'x'


def corrugated(S, plane, a0, a1, y0, y1, face, mat='clad', back=0.06, fine=6):
    """Corrugated sheet on a wall's face: its crests `back` behind the plane the
    sim collides (`plane`, the x or z the face stands on), its troughs CORR
    further, from a0 to a1 along the face and y0 to y1 up it."""
    out, along = face_axes(face)
    n = out[0] + out[2]
    cols = max(2, int(round((a1 - a0) / (PITCH / fine))))
    rows = max(1, int(math.ceil((y1 - y0) / 0.4)))
    grid = []
    for j in range(rows + 1):
        y = y0 + (y1 - y0) * j / rows
        row = []
        for i in range(cols + 1):
            a = a0 + (a1 - a0) * i / cols
            d = back + CORR * (0.5 - 0.5 * math.cos(math.tau * a / PITCH))
            q = plane - n * d
            row.append(S.p(q, y, a) if along == 'z' else S.p(a, y, q))
        grid.append(row)
    surface(grid, mat, S.o(*out), lambda p: (p[0] + p[2], p[1]))


def flat(S, plane, a0, a1, y0, y1, face, mat, back=0.0, cell=CELL):
    """A flat face standing `back` behind its plane."""
    out, along = face_axes(face)
    q = plane - (out[0] + out[2]) * back
    if along == 'z':
        S.quad((q, y0, a0), (q, y0, a1), (q, y1, a1), (q, y1, a0), mat, out, cell)
    else:
        S.quad((a0, y0, q), (a1, y0, q), (a1, y1, q), (a0, y1, q), mat, out, cell)


def clad_wall(S, x0, y0, z0, x1, y1, z1, face, plinth=1.0, inner_from=0.0, clad=True, ends=True):
    """One of a shaft house's wall boxes: concrete to `plinth` and corrugated steel
    over it on its face, a lined inside with its columns out to the inner plane, a
    steel coping on its top, the box's ends and soffit closed."""
    out, along = face_axes(face)
    if along == 'z':
        (po, pi) = (x0, x1) if out[0] < 0 else (x1, x0)
        a0, a1 = z0, z1
    else:
        (po, pi) = (z0, z1) if out[2] < 0 else (z1, z0)
        a0, a1 = x0, x1
    n = out[0] + out[2]
    thick = abs(pi - po)
    top = y1 - 0.14 if y1 > 7.5 else y1
    # The plinth: concrete on the face, the cladding set back over it.
    pl = min(max(plinth, y0), top)
    if pl > y0 + 0.01:
        lo, hi = sorted((po, po - n * 0.24))
        if along == 'z':
            S.box(lo, y0, a0, hi, pl, a1, 'blast', c=0.03)
        else:
            S.box(a0, y0, lo, a1, pl, hi, 'blast', c=0.03)
    if clad and top > pl + 0.05:
        corrugated(S, po, a0, a1, pl, top, face, 'clad')
    elif top > pl + 0.05:
        flat(S, po, a0, a1, pl, top, face, 'blast', back=0.0)
    # The inside: a pale lining behind its columns and girts.
    face_in = ('+' if face[0] == '-' else '-') + face[1]
    yi = max(y0, inner_from)
    if top > yi + 0.05:
        flat(S, pi, a0, a1, yi, top, face_in, 'lining', back=0.12, cell=0.35)
        span = a1 - a0
        k = max(1, int(round(span / 3.0)))
        for i in range(k + 1):
            a = a0 + span * i / k
            a = min(max(a, a0 + 0.12), a1 - 0.12)
            lo, hi = sorted((pi, pi + n * 0.2))
            if along == 'z':
                S.box(lo, yi, a - 0.12, hi, top, a + 0.12, 'steel', c=0.01)
            else:
                S.box(a - 0.12, yi, lo, a + 0.12, top, hi, 'steel', c=0.01)
        for gy in (2.2, 4.9):
            if yi + 0.1 < gy < top - 0.2:
                lo, hi = sorted((pi, pi + n * 0.1))
                if along == 'z':
                    S.box(lo, gy, a0, hi, gy + 0.16, a1, 'steel', c=0.01, cell=3.0)
                else:
                    S.box(a0, gy, lo, a1, gy + 0.16, hi, 'steel', c=0.01, cell=3.0)
    # The coping, and the box closed at its ends and under.
    if top < y1:
        lo, hi = sorted((po, pi))
        if along == 'z':
            S.box(lo, top, a0, hi, y1, a1, 'steel', c=0.02, cell=2.0)
        else:
            S.box(a0, top, lo, a1, y1, hi, 'steel', c=0.02, cell=2.0)
        snow_cap(S, along, lo, hi, a0, a1, y1)
    lo, hi = sorted((po, pi))
    if ends:
        for a, sgn in ((a0, -1.0), (a1, 1.0)):
            o = (0.0, 0.0, sgn) if along == 'z' else (sgn, 0.0, 0.0)
            if along == 'z':
                S.quad((lo, y0, a), (hi, y0, a), (hi, top, a), (lo, top, a), 'steel', o)
            else:
                S.quad((a, y0, lo), (a, y0, hi), (a, top, hi), (a, top, lo), 'steel', o)
    if y0 > 0.05:
        if along == 'z':
            S.quad((lo, y0, a0), (hi, y0, a0), (hi, y0, a1), (lo, y0, a1), 'steel', (0, -1, 0))
        else:
            S.quad((a0, y0, lo), (a1, y0, lo), (a1, y0, hi), (a0, y0, hi), 'steel', (0, -1, 0))
    if top >= y1 and y1 < 7.5:
        if along == 'z':
            S.quad((lo, y1, a0), (hi, y1, a0), (hi, y1, a1), (lo, y1, a1), 'steel', (0, 1, 0))
        else:
            S.quad((a0, y1, lo), (a1, y1, lo), (a1, y1, hi), (a0, y1, hi), 'steel', (0, 1, 0))


def snow_cap(S, along, lo, hi, a0, a1, y, h=0.05):
    """Snow on a narrow top: a low cushion inside its plan, ragged along it."""
    n = max(2, int((a1 - a0) / 0.35))
    rows = []
    for j in range(3):
        v = j / 2
        row = []
        for i in range(n + 1):
            u = i / n
            a = a0 + (a1 - a0) * u
            e = min(u * (a1 - a0), (1 - u) * (a1 - a0), 0.3) / 0.3
            k = h * (1 - (2 * v - 1) ** 2) * e * (0.6 + 0.4 * math.sin(a * 2.3 + lo * 1.7) * math.sin(a * 0.9))
            c = lo + 0.02 + (hi - lo - 0.04) * v
            row.append(S.p(c, y + 0.004 + max(0.0, k), a) if along == 'z' else S.p(a, y + 0.004 + max(0.0, k), c))
        rows.append(row)
    surface(rows, 'snow', (0, 1, 0), lambda p: (p[0], p[2]))


def snow_top(S, x0, y, z0, x1, z1, h=0.06):
    """Snow lying on a flat top over its whole plan."""
    n, m = max(2, int((x1 - x0) / 0.4)), max(2, int((z1 - z0) / 0.4))
    rows = []
    for j in range(m + 1):
        row = []
        for i in range(n + 1):
            u, v = i / n, j / m
            e = min(u, 1 - u, v, 1 - v) * 2
            k = h * (1 - (1 - min(1.0, e * 2.5)) ** 2) * (0.7 + 0.3 * math.sin(u * 7.1 + v * 5.3 + x0))
            row.append(S.p(x0 + (x1 - x0) * u, y + 0.004 + k, z0 + (z1 - z0) * v))
        rows.append(row)
    surface(rows, 'snow', (0, 1, 0), lambda p: (p[0], p[2]))


def icicles(S, a, b, y, rng, spacing=0.12, longest=0.45, fill=0.8):
    """Icicles hung under an edge from a to b (x, z), y its underside."""
    ln = math.hypot(b[0] - a[0], b[1] - a[1])
    n = int(ln / spacing)
    for i in range(n):
        if rng.random() > fill:
            continue
        t = (i + rng.random()) / n
        x, z = a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t
        ln_ = longest * (0.15 + 0.85 * rng.random() ** 2.2)
        r = 0.012 + 0.03 * ln_ / longest
        S.rod((x, y + 0.01, z), (x, y - ln_, z), r, 'ice', sides=5, taper=0.06)


def drift(S, x0, z0, x1, z1, y, walls, reach=0.9, seed=0, holes=()):
    """Snow blown onto a floor, a hair over it: thick against `walls` ('-x',
    '+x', '-z', '+z': which of its edges stand walled), thin out over it. None
    over `holes`, the floor's openings (x0, z0, x1, z1 each): a bank laid across
    a stairwell hangs in the air over the stairs, a pale sheet whoever climbs
    them looks down through."""
    rng = random.Random(seed)
    pa = part('drift')
    nx, nz = max(2, int((x1 - x0) / 0.25)), max(2, int((z1 - z0) / 0.25))
    base = len(pa.verts)
    ph = rng.random() * 10
    for j in range(nz + 1):
        for i in range(nx + 1):
            x, z = x0 + (x1 - x0) * i / nx, z0 + (z1 - z0) * j / nz
            d = 9.0
            for w in walls:
                d = min(d, {'-x': x - x0, '+x': x1 - x, '-z': z - z0, '+z': z1 - z}[w])
            gx, gz = S.p(x, 0.0, z)[0], S.p(x, 0.0, z)[2]
            wob = vnoise(gx * 0.9, gz * 0.9, seed)
            k = max(0.0, 1.0 - d / (reach * (0.6 + 0.8 * wob)))
            # Out over the floor, snow lies in drifts the wind left, not spots.
            speck = 0.5 * smooth(0.5, 0.78, vnoise(gx * 0.45, gz * 0.45, seed + 7))
            grain = 0.12 * vnoise(gx * 4.0, gz * 4.0, seed + 3)
            alpha = min(0.95, k ** 1.4 + speck + grain * speck * 2.0)
            q = S.p(x, y + 0.012, z)
            pa.verts.append(q)
            pa.colors[len(pa.verts) - 1] = (1.0, 1.0, 1.0, alpha)
    for j in range(nz):
        for i in range(nx):
            cx0, cx1 = x0 + (x1 - x0) * i / nx, x0 + (x1 - x0) * (i + 1) / nx
            cz0, cz1 = z0 + (z1 - z0) * j / nz, z0 + (z1 - z0) * (j + 1) / nz
            if any(cx1 > hx0 + 1e-4 and cx0 < hx1 - 1e-4 and cz1 > hz0 + 1e-4 and cz0 < hz1 - 1e-4
                   for hx0, hz0, hx1, hz1 in holes):
                continue
            k = base + j * (nx + 1) + i
            face(pa, (k, k + 1, k + nx + 2, k + nx + 1), (0, 1, 0), True)


def crate(S, x0, y0, z0, x1, y1, z1, mat='steel_box'):
    """A steel chest inside its box: a body set back, corner posts and bands out to
    the plane, snow on its lid."""
    t = 0.07
    S.box(x0 + t, y0, z0 + t, x1 - t, y1 - 0.05, z1 - t, mat, c=0.02)
    for x, z in ((x0, z0), (x1 - 0.12, z0), (x0, z1 - 0.12), (x1 - 0.12, z1 - 0.12)):
        S.box(x, y0, z, x + 0.12, y1, z + 0.12, 'steel', c=0.015, cell=3.0)
    for y in (y0 + 0.02, y1 - 0.1):
        S.box(x0, y, z0, x1, y + 0.08, z1, 'steel', c=0.015, cell=3.0)
    snow_top(S, x0 + 0.03, y1, z0 + 0.03, x1 - 0.03, z1 - 0.03, h=0.05)


def lamp(S, x, y, z, ceiling):
    """A pendant lamp under a ceiling: a stem, a shade, the lens under it."""
    S.rod((x, ceiling, z), (x, y + 0.2, z), 0.025, 'steel', sides=6)
    px, pz = _xz(S, x, z)
    spin(px, pz, [(0.001, y + 0.12), (0.22, y + 0.06), (0.26, y + 0.04), (0.06, y + 0.22), (0.001, y + 0.22)], 'steel', sides=12)
    spin(px, pz, [(0.001, y + 0.05), (0.2, y + 0.06), (0.001, y + 0.07)], 'light_sodium', sides=12, smooth=False)


# ----------------------------------------------------------------- the shaft house
def shaft_house(S):
    rng = random.Random(11 if S.s > 0 else 12)
    # The walls on the drop: concrete up from under the lip, clad over it.
    clad_wall(S, -21.4, -3.0, -2.4, -21.0, 8.1, 14.4, '-x', plinth=0.9)
    clad_wall(S, -21.0, -3.0, 14.0, -13.0, 8.1, 14.4, '+z', plinth=0.9)
    # The spur's side, its door in the middle.
    clad_wall(S, -21.0, 0.0, -2.4, -18.0, 8.1, -2.0, '-z', plinth=0.9)
    clad_wall(S, -15.5, 0.0, -2.4, -13.0, 8.1, -2.0, '-z', plinth=0.9)
    clad_wall(S, -18.0, 2.6, -2.4, -15.5, 8.1, -2.0, '-z', plinth=0.9)
    # The shaft's side: concrete on the ground floor, its door to the bridge.
    for z0, z1 in ((-2.0, 1.0), (4.0, 14.0)):
        clad_wall(S, -13.4, 0.0, z0, -13.0, 3.0, z1, '+x', plinth=3.0)
    clad_wall(S, -13.4, 2.6, 1.0, -13.0, 3.0, 4.0, '+x', plinth=3.0)
    # Over them: cladding to the crown, the mezzanine's rail, the jamb.
    clad_wall(S, -13.4, 3.0, -2.0, -13.0, 6.5, 5.0, '+x', plinth=3.0)
    clad_wall(S, -13.4, 3.5, 8.0, -13.0, 6.5, 10.0, '+x', plinth=3.5)
    # The rail where the mezzanine looks over the shaft: a steel parapet in the
    # side's paint, a lamp strip under its top.
    S.box(-13.36, 3.0, 5.0, -13.04, 4.38, 8.0, 'team_' + S.team, c=0.02)
    S.box(-13.4, 4.38, 5.0, -13.0, 4.5, 8.0, 'galv', c=0.02)
    S.box(-13.0, 4.2, 5.1, -12.985, 4.28, 7.9, 'light_' + S.team, c=0.0, cell=20.0)
    snow_cap(S, 'z', -13.4, -13.0, 5.0, 8.0, 4.5, h=0.03)
    # A strip of the side's light over the doors to the bridge and the spur.
    S.box(-13.0, 2.68, 1.15, -12.98, 2.78, 3.85, 'light_' + S.team, c=0.0, cell=20.0)
    S.box(-17.85, 2.68, -2.42, -15.65, 2.78, -2.4, 'light_' + S.team, c=0.0, cell=20.0)
    # Door frames: steel round each opening, inside its jambs.
    for z in (1.0, 3.9):
        S.box(-13.4, 0.0, z, -13.0, 2.6, z + 0.1, 'steel', c=0.01)
    S.box(-13.4, 2.5, 1.0, -13.0, 2.6, 4.0, 'steel', c=0.01)
    for x in (-18.0, -15.6):
        S.box(x, 0.0, -2.4, x + 0.1, 2.6, -2.0, 'steel', c=0.01)
    # The mezzanine: tread plate on joists, a channel along its open edge.
    S.box(-21.0, 3.36, 5.0, -13.0, 3.5, 14.0, 'tread', c=0.0, cell=2.0, under=False)
    S.box(-21.0, 3.0, 5.0, -13.0, 3.36, 5.18, 'steel', c=0.01, cell=3.0)
    for x in np.arange(-20.6, -13.2, 1.5):
        S.box(x, 3.04, 5.18, x + 0.14, 3.36, 14.0, 'steel', c=0.01, cell=3.0)
    S.quad((-21.0, 3.36, 5.18), (-13.0, 3.36, 5.18), (-13.0, 3.36, 14.0), (-21.0, 3.36, 14.0), 'steel', (0, -1, 0), cell=3.0)
    drift(S, -21.0, 5.0, -13.0, 14.0, 3.5, ['+z', '-x'], reach=0.7, seed=3 + S.s)
    # The stairs up the west wall: steps under the slope, closed sides.
    stairs(S, -21.0, -1.0, -19.0, 5.0, 0.0, 3.5)
    stairs(S, -21.0, 6.0, -19.0, 12.0, 3.5, 7.0)
    # The crown's floor: the shaft house's roof, its ceiling under it.
    for x0, z0, x1, z1 in ((-19.0, -2.0, -13.0, 14.0), (-21.0, -2.0, -19.0, 5.6), (-21.0, 12.0, -19.0, 14.0)):
        S.box(x0, 6.86, z0, x1, 7.0, z1, 'tread', c=0.0, cell=2.0, under=False)
        S.quad((x0, 6.86, z0), (x1, 6.86, z0), (x1, 6.86, z1), (x0, 6.86, z1), 'steel', (0, -1, 0), cell=3.0)
    for x in np.arange(-20.7, -13.1, 1.6):
        S.box(x, 6.5, -2.0, x + 0.18, 6.86, 14.0, 'steel', c=0.01, cell=3.0)
    S.box(-19.0, 6.5, 5.6, -18.82, 6.86, 12.0, 'steel', c=0.01, cell=3.0)
    # Its edge to the shaft, a deep channel, icicles off it.
    S.box(-13.2, 6.5, -2.0, -13.0, 7.0, 14.0, 'steel', c=0.01, cell=3.0)
    # Short: they hang in the mezzanine's window and the way out to the deck.
    icicles(S, (-13.02, -1.8), (-13.02, 13.8), 6.5, rng, longest=0.3, fill=0.6)
    drift(S, -21.0, -2.0, -13.0, 14.0, 7.0, ['-x', '+z', '-z'], reach=1.1, seed=7 + S.s,
          holes=[(-21.0, 5.6, -19.0, 12.0)])
    # Its crate.
    crate(S, -16.6, 7.0, 8.4, -15.4, 8.2, 9.6)
    # Its floor: concrete over the rock, the snow blown in at its doors.
    S.quad((-21.0, 0.02, -2.0), (-13.4, 0.02, -2.0), (-13.4, 0.02, 14.0), (-21.0, 0.02, 14.0), 'floor', (0, 1, 0), cell=1.0)
    drift(S, -15.6, 0.3, -13.4, 4.7, 0.02, ['+x'], reach=1.3, seed=91 + S.s)
    drift(S, -18.6, -2.0, -14.9, 0.4, 0.02, ['-z'], reach=1.2, seed=93 + S.s)
    # The lamps.
    lamp(S, -17.0, 5.8, 3.0, 6.5)
    lamp(S, -17.0, 2.7, 10.0, 3.0)


# A flight's stringer: how wide it stands in from its plane, its flange's
# depth, how far its web is set back behind the plane, m.
STRINGER = (0.1, 0.12, 0.04)


def stringer(S, axis, a0, a1, y0, y1, plane, inw, n):
    """A plate stringer down one side of a flight of n treads that runs a0 to a1
    along `axis` ('x' or 'z') and rises y0 to y1, inside the sim's ramp (which is
    solid, so the side is closed): a flange along the slope from where its foot
    clears the floor, under it a web down to the flight's floor, set back from
    `plane` (the side the sim collides; `inw` the way in from it across the
    flight), a stiffener every other tread out to the plane, and its other face
    at the treads' ends, so the flight has a side from on it too. Cut into
    bays, so each takes its own shade and frost. Returns where the flange
    starts and its top along the flight."""
    wide, deep, back = STRINGER
    rate = (y1 - y0) / (a1 - a0)
    run = (a1 - a0) / n

    def top(a):
        return y0 + rate * (a - a0) - 0.03

    def at(a, y, c):
        return (a, y, c) if axis == 'x' else (c, y, a)

    s = a0 + (deep + 0.03) / rate
    mid = plane + inw * wide / 2
    (ax, _, az), (bx, _, bz) = S.p(*at(s, 0.0, mid)), S.p(*at(a1, 0.0, mid))
    plank((ax, az), (bx, bz), top(s), top(a1), -wide / 2, wide / 2, deep, 'steel')
    bays = [s] + [a0 + run * k for k in range(2, n, 2)] + [a1]
    for c, o in ((plane + inw * back, -inw), (plane + inw * wide, inw)):
        out = at(0.0, 0.0, o)
        for a, b in zip(bays, bays[1:]):
            if a == s:
                S.tri(at(a, y0, c), at(b, y0, c), at(b, top(b) - deep, c), 'steel', out)
            else:
                S.quad(at(a, y0, c), at(b, y0, c), at(b, top(b) - deep, c), at(a, top(a) - deep, c), 'steel', out)
    c0, c1 = sorted((plane, plane + inw * back))
    for a in bays[1:-1]:
        S.box(*at(a - 0.03, y0, c0), *at(a + 0.03, top(a - 0.03) - deep, c1), 'steel', c=0.005, under=False)
    return s, top


def stairs(S, x0, z0, x1, z1, y0, y1):
    """A flight up +z inside the sim's ramp: each tread's nose on the slope, a
    plate stringer down each side, its head closed."""
    n = int(round((y1 - y0) / 0.25))
    run = (z1 - z0) / n
    wide = STRINGER[0]
    for k in range(1, n):
        y = y0 + (y1 - y0) * k / n
        za = z0 + run * k
        S.box(x0 + wide, y - 0.06, za, x1 - wide, y, za + run, 'tread', c=0.005, cell=3.0)
        S.box(x0 + wide, y0, za, x1 - wide, y - 0.06, za + 0.03, 'steel', c=0.0, cell=3.0, under=False)
    for plane, inw in ((x0, 1.0), (x1, -1.0)):
        stringer(S, 'z', z0, z1, y0, y1, plane, inw, n)
    S.quad((x0, y0, z1), (x1, y0, z1), (x1, y1 - 0.03, z1), (x0, y1 - 0.03, z1), 'steel', (0, 0, 1))
    # A handrail on the wall's side.
    S.rod((x0 + 0.08, y0 + 0.95, z0), (x0 + 0.08, y1 + 0.95, z1), 0.025, 'galv', sides=6)
    for t in (0.1, 0.5, 0.9):
        z = z0 + (z1 - z0) * t
        y = y0 + (y1 - y0) * t
        S.rod((x0, y + 0.95, z), (x0 + 0.08, y + 0.95, z), 0.02, 'galv', sides=6)


# ------------------------------------------------------------------- the headframe
def headframe(S):
    rng = random.Random(21 if S.s > 0 else 22)
    legs = [(-14.2, 2.2), (-14.2, 12.6), (-21.0, 2.2), (-21.0, 12.6)]
    for x, z in legs:
        # A built-up column inside its box: a body, its angles at the corners.
        S.box(x + 0.06, 7.0, z + 0.06, x + 0.74, 12.4, z + 0.74, 'rusty', c=0.02, cell=1.2)
        for cx, cz in ((x, z), (x + 0.62, z), (x, z + 0.62), (x + 0.62, z + 0.62)):
            S.box(cx, 7.0, cz, cx + 0.18, 12.4, cz + 0.18, 'steel', c=0.01, cell=1.6)
        for y in np.arange(7.6, 12.2, 1.2):
            S.box(x, y, z, x + 0.8, y + 0.14, z + 0.8, 'steel', c=0.01)
        S.box(x - 0.0, 7.0, z, x + 0.8, 7.12, z + 0.8, 'steel', c=0.01)
    # The beams over the crown (dress, out of reach) and the tower on them,
    # narrowing to the sheave deck.
    S.box(-21.0, 12.4, 2.2, -13.4, 13.0, 3.0, 'steel', c=0.02, cell=1.6)
    S.box(-21.0, 12.4, 12.6, -13.4, 13.0, 13.4, 'steel', c=0.02, cell=1.6)
    S.box(-17.6, 12.4, 3.0, -16.8, 13.0, 12.6, 'steel', c=0.02, cell=1.6)
    top = 16.4
    tl = [(-14.6, 3.0), (-14.6, 12.6), (-20.0, 3.0), (-20.0, 12.6)]
    for (x, z), (tx, tz) in zip(legs, tl):
        S.rod((x + 0.4, 13.0, z + 0.4), (tx + 0.2, top, tz + 0.2 if z < 7 else tz - 0.2), 0.2, 'rusty', sides=6)
    # X bracing on its four faces.
    ys = (13.0, top)
    for (a, b) in ((0, 1), (2, 3), (0, 2), (1, 3)):
        (ax, az), (bx, bz) = legs[a], legs[b]
        (tax, taz), (tbx, tbz) = tl[a], tl[b]
        pa0 = (ax + 0.4, ys[0], az + 0.4)
        pb0 = (bx + 0.4, ys[0], bz + 0.4)
        pa1 = (tax + 0.2, ys[1], taz)
        pb1 = (tbx + 0.2, ys[1], tbz)
        S.rod(pa0, pb1, 0.06, 'steel', sides=5)
        S.rod(pb0, pa1, 0.06, 'steel', sides=5)
        mid = tuple((p + q) / 2 for p, q in zip(pa0, pa1)), tuple((p + q) / 2 for p, q in zip(pb0, pb1))
        S.rod(mid[0], mid[1], 0.07, 'steel', sides=5)
    # The sheave deck, its rails.
    S.box(-20.2, top, 2.6, -13.6, top + 0.3, 13.0, 'steel', c=0.02, cell=1.6)
    snow_top(S, -20.15, top + 0.3, 2.65, -13.65, 12.95, h=0.08)
    for x0, z0, x1, z1 in ((-20.2, 2.6, -13.6, 2.6), (-20.2, 13.0, -13.6, 13.0), (-20.2, 2.6, -20.2, 13.0), (-13.6, 2.6, -13.6, 13.0)):
        S.rod((x0, top + 1.3, z0), (x1, top + 1.3, z1), 0.035, 'galv', sides=6)
        for t in np.linspace(0.0, 1.0, 6):
            x, z = x0 + (x1 - x0) * t, z0 + (z1 - z0) * t
            S.rod((x, top + 0.3, z), (x, top + 1.3, z), 0.03, 'galv', sides=6)
    # The wheels' A-frames and bearings.
    cx, cy = SHEAVE
    for z in SHEAVE_Z:
        for dz in (-0.45, 0.45):
            S.rod((cx - 1.4, top + 0.3, z + dz), (cx, cy, z + dz * 0.6), 0.09, 'steel', sides=6)
            S.rod((cx + 1.4, top + 0.3, z + dz), (cx, cy, z + dz * 0.6), 0.09, 'steel', sides=6)
        S.box(cx - 0.22, cy - 0.22, z - 0.5, cx + 0.22, cy + 0.22, z + 0.5, 'steel', c=0.03)
        sheave(S, cx, cy, z)
    # The cables: off the wheels' tops west to the winding house, and down
    # their east sides to the head's clamps.
    for z in SHEAVE_Z:
        S.rod((cx - SHEAVE_R * 0.98, cy + 0.35, z), (-29.0, -1.2, z), 0.04, 'cable', sides=5)
        S.rod((cx + SHEAVE_R, cy, z), (cx + SHEAVE_R, 13.0, z), 0.04, 'cable', sides=5)
        S.box(cx + SHEAVE_R - 0.25, 13.0, z - 0.25, cx + SHEAVE_R + 0.25, 13.5, z + 0.25, 'steel', c=0.03)
    # The back legs: down from the head to the winding house's crag.
    for z in (3.4, 12.2):
        a = (-19.8, top, z)
        b = (-28.0, -3.0, z)
        S.rod(a, b, 0.3, 'rusty', sides=6)
        n = 7
        for k in range(1, n):
            t = k / n
            p = tuple(a[i] + (b[i] - a[i]) * t for i in range(3))
            q = (p[0], p[1], 7.8)
            S.rod(p, q, 0.06, 'steel', sides=5)
    # A beacon on the head, in the side's colour, and icicles off the deck.
    spin(*_xz(S, -17.0, 7.8), [(0.001, top + 1.3), (0.18, top + 1.32), (0.2, top + 1.5), (0.001, top + 1.62)], 'light_' + S.team, sides=10, smooth=False)
    S.rod((-17.0, top + 0.3, 7.8), (-17.0, top + 1.3, 7.8), 0.05, 'steel', sides=6)
    icicles(S, (-20.2, 2.62), (-13.6, 2.62), top, rng, longest=0.7, spacing=0.18)
    icicles(S, (-20.2, 12.98), (-13.6, 12.98), top, rng, longest=0.7, spacing=0.18)


def _xz(S, x, z):
    p = S.p(x, 0.0, z)
    return p[0], p[2]


def sheave(S, cx, cy, z):
    """A sheave wheel standing in the x-y plane at depth z: rim, hub, spokes. Its
    own part (`wheel_<side>`), which the client turns."""
    mat = 'wheel_' + S.team
    c = S.p(cx, cy, z)
    torus(c, SHEAVE_R, 0.12, (0, 0, 1), mat, segs=48, sides=8)
    torus(c, SHEAVE_R - 0.16, 0.06, (0, 0, 1), mat, segs=48, sides=6)
    for k in range(8):
        a = math.tau * k / 8
        p = (cx + math.cos(a) * (SHEAVE_R - 0.15), cy + math.sin(a) * (SHEAVE_R - 0.15), z)
        S.rod((cx, cy, z), p, 0.06, mat, sides=5)
    S.rod((cx, cy, z - 0.3), (cx, cy, z + 0.3), 0.28, mat, sides=10)


# ------------------------------------------------------------ the gantry, the hub
def gantry():
    rng = random.Random(31)
    # The deck: tread plate on cross beams between two plate girders.
    for z0, z1 in ((-1.5, -1.36), (1.36, 1.5)):
        for x0, x1 in ((-13.0, -3.0), (3.0, 13.0)):
            cbox(x0, 6.5, z0, x1, 7.0, z1, 'steel', c=0.01, cell=1.6)
    # The deck either side of the landing, the landing between, the soffit
    # under them: no two faces on one plane.
    for x0, x1 in ((-13.0, -3.0), (3.0, 13.0)):
        cbox(x0, 6.86, -1.36, x1, 7.0, 1.36, 'tread', c=0.0, cell=2.0, under=False)
    cbox(-3.0, 6.86, -3.0, 3.0, 7.0, 3.0, 'tread', c=0.0, cell=2.0, under=False)
    quad((-13.0, 6.86, -1.36), (13.0, 6.86, -1.36), (13.0, 6.86, 1.36), (-13.0, 6.86, 1.36), 'steel', (0, -1, 0), cell=3.0)
    for z0, z1 in ((-3.0, -1.36), (1.36, 3.0)):
        quad((-3.0, 6.86, z0), (3.0, 6.86, z0), (3.0, 6.86, z1), (-3.0, 6.86, z1), 'steel', (0, -1, 0), cell=3.0)
    for x in np.arange(-12.6, 12.7, 1.4):
        cbox(x, 6.56, -1.36, x + 0.14, 6.86, 1.36, 'steel', c=0.01, cell=3.0)
    # The landing's edges.
    cbox(-3.0, 6.5, -3.0, 3.0, 7.0, -2.86, 'steel', c=0.01)
    cbox(-3.0, 6.5, 2.86, 3.0, 7.0, 3.0, 'steel', c=0.01)
    for s in (-1, 1):
        x0, x1 = sorted((s * 3.0, s * 2.86))
        cbox(x0, 6.5, -3.0, x1, 7.0, -1.5, 'steel', c=0.01)
        cbox(x0, 6.5, 1.5, x1, 7.0, 3.0, 'steel', c=0.01)
    # The truss under it, out of reach: chords, posts, diagonals.
    yb = 5.7
    for z in (-1.38, 1.38):
        for x0, x1 in ((-12.6, -3.0), (3.0, 12.6)):
            rod((x0, yb, z), (x1, yb, z), 0.07, 'steel', sides=6)
            xs = np.linspace(x0, x1, 7)
            for i, x in enumerate(xs):
                rod((x, yb, z), (x, 6.5, z), 0.05, 'steel', sides=5)
                if i + 1 < len(xs):
                    a, b = (x, yb, z), (xs[i + 1], 6.5, z)
                    if (i % 2) == (0 if x0 < 0 else 1):
                        a, b = (x, 6.5, z), (xs[i + 1], yb, z)
                    rod(a, b, 0.045, 'steel', sides=5)
        rod((-12.6, yb, z), (-13.0, 6.5, z), 0.07, 'steel', sides=6)
        rod((12.6, yb, z), (13.0, 6.5, z), 0.07, 'steel', sides=6)
    for x in np.linspace(-12.6, -3.0, 7).tolist() + np.linspace(3.0, 12.6, 7).tolist():
        rod((x, yb, -1.38), (x, yb, 1.38), 0.04, 'steel', sides=5)
    icicles(S1, (-13.0, -1.52), (13.0, -1.52), 6.5, rng, longest=0.4, spacing=0.16, fill=0.6)
    icicles(S1, (-13.0, 1.52), (13.0, 1.52), 6.5, rng, longest=0.4, spacing=0.16, fill=0.6)
    drift(S1, -13.0, -1.36, 13.0, 1.36, 7.0, ['-z', '+z'], reach=0.35, seed=41)
    drift(S1, -3.0, -3.0, 3.0, 3.0, 7.0, ['-z', '+z', '-x', '+x'], reach=0.5, seed=43)
    cab()
    for S in SIDES:
        sheave_stairs(S)
        crate(S, -4.4, 7.0, 0.3, -3.2, 8.2, 1.5)
        # Snow banked along the stairs' foot and against the crate.
        drift(S, -9.5, -1.36, -2.9, 1.36, 7.0, ['+z'], reach=0.8, seed=44 + S.s)


S1 = SIDES[0]


def cab():
    """The winding cab on the gantry's landing: grey-green sheds' walls all round,
    a door at each end off to one side of the gantry, the winding drum in the
    middle, lit windows; its roof the sheave deck, posts at its corners."""
    rng = random.Random(53)
    for S in SIDES:
        shed_wall(S, -2.9, 7.0, 2.6, 2.9, 10.0, 3.0, '+z', windows=(-2.4, 2.4, 8.4, 9.3))
        shed_wall(S, -2.9, 7.0, 0.3, -2.5, 10.0, 2.6, '-x')
        shed_wall(S, -2.9, 7.0, -2.6, -2.5, 10.0, -1.5, '-x')
        # The door's frame and the light over it, in neither side's colour.
        for z in (-1.5, 0.2):
            S.box(-2.95, 7.0, z, -2.45, 9.0, z + 0.1, 'steel', c=0.01)
        S.box(-2.95, 8.9, -1.5, -2.45, 9.0, 0.3, 'steel', c=0.01)
        S.box(-2.96, 9.1, -1.3, -2.92, 9.18, 0.1, 'light_sodium', c=0.0, cell=20.0)
        # The posts on the deck over it: built-up columns, a lamp on each.
        for x0 in (-2.9, 2.5):
            S.box(x0, 10.5, 2.8, x0 + 0.4, 12.2, 3.2, 'rusty', c=0.02)
            for cx in (x0, x0 + 0.3):
                S.box(cx, 10.5, 2.8, cx + 0.1, 12.2, 2.9, 'steel', c=0.005)
            px, pz = _xz(S, x0 + 0.2, 3.0)
            spin(px, pz, [(0.001, 12.2), (0.14, 12.2), (0.14, 12.3), (0.001, 12.3)], 'steel', sides=10, smooth=False)
            spin(px, pz, [(0.001, 12.3), (0.09, 12.32), (0.1, 12.45), (0.001, 12.5)], 'light_sodium', sides=10)
        icicles(S, (-2.9, 3.22), (2.9, 3.22), 10.0, rng, longest=0.6, spacing=0.14)
    # The drum on its bed, wound with rope, its bearings at its ends.
    cbox(-0.9, 7.0, -0.6, 0.9, 7.3, 0.6, 'steel', c=0.02)
    rod((0.0, 8.25, -0.6), (0.0, 8.25, 0.6), 0.9, 'rusty', sides=28)
    for z in np.linspace(-0.5, 0.5, 9):
        torus((0.0, 8.25, z), 0.88, 0.045, (0, 0, 1), 'cable', segs=40, sides=5)
    for z0 in (-0.6, 0.48):
        cbox(-0.35, 7.3, z0, 0.35, 8.45, z0 + 0.12, 'steel', c=0.01)
    cbox(-0.9, 9.15, -0.6, 0.9, 9.4, 0.6, 'steel', c=0.02)
    # The roof: tread plate on joists, the cab's ceiling under them, snow on it.
    cbox(-2.9, 10.36, -3.2, 2.9, 10.5, 3.2, 'tread', c=0.0, cell=2.0)
    cbox(-2.9, 10.0, -3.2, 2.9, 10.36, -3.0, 'steel', c=0.01)
    cbox(-2.9, 10.0, 3.0, 2.9, 10.36, 3.2, 'steel', c=0.01)
    for x in np.arange(-2.7, 2.8, 1.1):
        cbox(x, 10.04, -3.0, x + 0.14, 10.36, 3.0, 'steel', c=0.01, cell=3.0)
    # Just under the joists, not on their plane.
    quad((-2.9, 10.02, -3.0), (2.9, 10.02, -3.0), (2.9, 10.02, 3.0), (-2.9, 10.02, 3.0), 'lining', (0, -1, 0), cell=0.35)
    drift(S1, -2.9, -3.2, 2.9, 3.2, 10.5, ['+z', '-z'], reach=0.5, seed=55)


def sheave_stairs(S):
    """A flight up beside the gantry to the sheave deck, rising +x: its treads on
    the slope between two plate stringers down to its floor at the gantry's
    height, a string of lights along its outer side so the way up reads. Under
    its floor, out of reach, what carries it: outriggers off the gantry's girder
    to a channel along its outer edge, each braced from the truss's foot."""
    x0, z0, x1, z1, y0, y1 = -9.0, 1.5, -2.9, 3.2, 7.0, 10.5
    n = int(round((y1 - y0) / 0.25))
    run = (x1 - x0) / n
    wide, deep, back = STRINGER
    # The channel's width under the outer stringer.
    chan = 0.14
    # The foot's plate, level with the gantry's deck, out to the channel.
    S.box(x0, y0 - 0.06, z0, x0 + run, y0, z1 - chan, 'tread', c=0.005, cell=3.0)
    for k in range(1, n):
        y = y0 + (y1 - y0) * k / n
        xa = x0 + run * k
        S.box(xa, y - 0.06, z0 + wide, xa + run, y, z1 - wide, 'tread', c=0.005, cell=3.0)
        S.box(xa, y0, z0 + wide, xa + 0.03, y - 0.06, z1 - wide, 'steel', c=0.0, cell=3.0, under=False)
    for zp, inw in ((z0, 1.0), (z1, -1.0)):
        xs, top = stringer(S, 'x', x0, x1, y0, y1, zp, inw, n)
    # The inner web's foot, past the gantry's girder and the foot's plate. A
    # riser and a stiffener stand open under on the floor and its soffit: no
    # two faces on one plane.
    S.quad((x0 + run, y0, z0), (x1, y0, z0), (x1, y0, z0 + back), (x0 + run, y0, z0 + back), 'steel', (0, 1, 0))
    S.quad((x1, y0, z0), (x1, y0, z1), (x1, y1 - 0.03, z1), (x1, y1 - 0.03, z0), 'steel', (1, 0, 0))
    S.quad((x0 + run, y0, z0), (x1, y0, z0), (x1, y0, z1 - chan), (x0 + run, y0, z1 - chan), 'steel', (0, -1, 0))
    S.rod((xs + 0.1, top(xs + 0.1) - deep / 2, z1 + 0.01), (x1, top(x1) - deep / 2, z1 + 0.01), 0.025, 'light_sodium', sides=4)
    # What carries it: the channel along its outer edge and across its foot,
    # as deep as the gantry's girder, an outrigger at the foot and at each of
    # the truss's posts, a brace to each from the truss's lower chord.
    S.box(x0, y0 - 0.5, z1 - chan, x1, y0, z1, 'steel', c=0.01, cell=1.6)
    S.box(x0, y0 - 0.5, z0, x0 + chan, y0 - 0.06, z1 - chan, 'steel', c=0.01, cell=1.6)
    for x in (x0 + chan / 2, -7.8, -6.2, -4.6):
        if x > x0 + chan:
            S.box(x - 0.07, y0 - 0.3, z0, x + 0.07, y0, z1 - chan, 'steel', c=0.01, cell=3.0)
        S.rod((x, 5.7, 1.38), (x, y0 - 0.45, z1 - chan / 2), 0.045, 'steel', sides=5)
    icicles(S, (x0, z1 + 0.01), (x1, z1 + 0.01), y0 - 0.5, random.Random(61 if S.s > 0 else 62), longest=0.4, spacing=0.16, fill=0.6)


def cage():
    """The cage: the catwalk across the shaft from deck to deck under the
    gantry, tread plate on joists between two plate girders, a lamp hung under
    it either side of the plinth, each throwing its pool on the collar."""
    rng = random.Random(56)
    cbox(-1.5, 3.36, -8.0, 1.5, 3.5, 8.0, 'tread', c=0.0, cell=2.0, under=False)
    for x0, x1 in ((-1.5, -1.2), (1.2, 1.5)):
        cbox(x0, 3.0, -8.0, x1, 3.36, 8.0, 'steel', c=0.01, cell=3.0)
        cbox(x0, 1.4, -8.0, x1, 1.52, 8.0, 'steel', c=0.01, cell=3.0)
        cbox(x0 + 0.1, 1.52, -8.0, x1 - 0.1, 3.0, 8.0, 'steel', c=0.01, cell=1.6)
        for z in np.arange(-7.4, 7.5, 1.5):
            cbox(x0 + 0.02, 1.52, z, x1 - 0.02, 3.0, z + 0.08, 'steel', c=0.005)
        icicles(S1, (x0 - 0.01, -8.0), (x0 - 0.01, 8.0), 1.4, rng, longest=0.45, fill=0.6)
        icicles(S1, (x1 + 0.01, -8.0), (x1 + 0.01, 8.0), 1.4, rng, longest=0.45, fill=0.6)
    for z in np.arange(-7.6, 7.7, 1.4):
        cbox(-1.2, 3.04, z, 1.2, 3.36, z + 0.14, 'steel', c=0.01, cell=3.0)
    quad((-1.2, 3.36, -8.0), (1.2, 3.36, -8.0), (1.2, 3.36, 8.0), (-1.2, 3.36, 8.0), 'steel', (0, -1, 0), cell=3.0)
    drift(S1, -1.5, -8.0, 1.5, 8.0, 3.5, ['-x', '+x'], reach=0.4, seed=54)
    for S in SIDES:
        lamp(S, 0.0, 2.8, 2.6, 3.0)
        pool(S, 0.0, 2.6, 3.0, -1.0)


def collar():
    """The shaft's collar a metre down: a steel landing over the drop on legs
    into the cloud, the sword's plinth in its middle with its hazard band, the
    skips' pits off two corners, a crate; and each side's bridge from its
    hall's door."""
    rng = random.Random(57)
    cbox(-4.5, -1.14, -4.0, 4.5, -1.0, 4.0, 'tread', c=0.0, cell=2.0)
    for z0 in (-4.0, 3.82):
        cbox(-4.5, -1.5, z0, 4.5, -1.0, z0 + 0.18, 'steel', c=0.01, cell=3.0)
    for x in np.arange(-4.3, 4.4, 1.2):
        cbox(x, -1.46, -3.82, x + 0.14, -1.14, 3.82, 'steel', c=0.01, cell=3.0)
    quad((-4.5, -1.46, -4.0), (4.5, -1.46, -4.0), (4.5, -1.46, 4.0), (-4.5, -1.46, 4.0), 'steel', (0, -1, 0), cell=3.0)
    # The plinth: a steel cap, its band yellow and black, a hatch's ribs.
    spin(0.0, 0.0, [(0.001, -1.0), (1.6, -1.0), (1.6, -0.63), (1.57, -0.6), (0.001, -0.6)], 'steel', sides=40, smooth=False)
    for k in range(24):
        a0, a1 = math.tau * k / 24, math.tau * (k + 1) / 24
        spin(0.0, 0.0, [(1.605, -0.95), (1.605, -0.7)], 'paint_y' if k % 2 else 'paint_k', sides=4, smooth=False, arc=(a0, a1))
    for k in range(8):
        a = math.tau * k / 8 + 0.2
        ca, sa = math.cos(a), math.sin(a)
        rod((ca * 0.3, -0.6, sa * 0.3), (ca * 1.45, -0.6, sa * 1.45), 0.03, 'steel', sides=4)
    for S in SIDES:
        # The skip's pit: its plate, a hazard edge round it.
        S.box(4.5, -1.14, 2.0, 6.6, -1.0, 4.0, 'tread', c=0.0, cell=2.0)
        S.box(4.5, -1.5, 2.0, 6.6, -1.14, 4.0, 'steel', c=0.01)
        for k, x in enumerate(np.arange(4.6, 6.5, 0.3)):
            S.box(x, -1.0, 3.88, x + 0.3, -0.98, 4.0, 'paint_y' if k % 2 else 'paint_k', c=0.0)
            S.box(x, -1.0, 2.0, x + 0.3, -0.98, 2.12, 'paint_y' if k % 2 else 'paint_k', c=0.0)
        icicles(S, (4.5, 4.02), (6.6, 4.02), -1.5, rng, longest=0.4)
        crate(S, -4.2, -1.0, 1.8, -3.0, 0.2, 3.0)
        bridge(S, rng)
    icicles(S1, (-4.5, -4.02), (4.5, -4.02), -1.5, rng, longest=0.5)
    icicles(S1, (-4.5, 4.02), (4.5, 4.02), -1.5, rng, longest=0.5)
    drift(S1, -4.5, -4.0, 4.5, 4.0, -1.0, ['-z', '+z'], reach=0.6, seed=59)


def bridge(S, rng):
    """The side's way down from its hall's door to the collar: a plate over the
    crag's lip, a ramp, a run, tread on a steel frame."""
    S.box(-13.0, -0.04, 1.0, -11.0, 0.1, 4.0, 'tread', c=0.0, cell=2.0)
    S.box(-13.0, -0.5, 1.0, -11.0, -0.04, 4.0, 'steel', c=0.01)
    S.quad((-11.0, 0.1, 1.0), (-8.0, -1.0, 1.0), (-8.0, -1.0, 4.0), (-11.0, 0.1, 4.0), 'tread', (0.34, 0.94, 0.0), cell=2.0)
    for z, o in ((1.0, -1.0), (4.0, 1.0)):
        S.quad((-11.0, -1.5, z), (-8.0, -1.5, z), (-8.0, -1.0, z), (-11.0, 0.1, z), 'steel', (0, 0, o))
    S.quad((-11.0, -1.5, 1.0), (-8.0, -1.5, 1.0), (-8.0, -1.5, 4.0), (-11.0, -1.5, 4.0), 'steel', (0, -1, 0))
    S.box(-8.0, -1.14, 1.0, -4.5, -1.0, 4.0, 'tread', c=0.0, cell=2.0)
    for z0 in (1.0, 3.82):
        S.box(-8.0, -1.5, z0, -4.5, -1.0, z0 + 0.18, 'steel', c=0.01, cell=3.0)
    S.quad((-8.0, -1.46, 1.18), (-4.5, -1.46, 1.18), (-4.5, -1.46, 3.82), (-8.0, -1.46, 3.82), 'steel', (0, -1, 0), cell=3.0)
    for z in (0.98, 4.02):
        icicles(S, (-8.0, z), (-4.5, z), -1.5, rng, longest=0.35)
    drift(S, -13.0, 1.0, -11.0, 4.0, 0.1, ['-z', '+z'], reach=0.5, seed=58 + S.s)


def legs():
    """What holds the hub up and what the decks' middles stand on: steel legs
    from under their floors down into the cloud, braced, under the floors'
    own plans so nobody falls through one."""
    def leg(x, z, top, sides=8):
        rod((x, top, z), (x, -72.0, z), 0.22, 'rusty', sides=sides)
    feet = [(-4.2, -3.7), (4.2, -3.7), (-4.2, 3.7), (4.2, 3.7)]
    for x, z in feet:
        leg(x, z, -1.5)
    for y in (-6.0, -16.0, -30.0, -48.0):
        for (ax, az), (bx, bz) in ((feet[0], feet[1]), (feet[2], feet[3]), (feet[0], feet[2]), (feet[1], feet[3])):
            rod((ax, y, az), (bx, y, bz), 0.1, 'steel', sides=6)
            rod((ax, y, az), (bx, y - 8.0, bz), 0.06, 'steel', sides=5)
    for S in SIDES:
        for x, z in ((-7.7, 1.3), (-7.7, 3.7), (-10.6, 1.3), (-10.6, 3.7)):
            a = S.p(x, 0.0, z)
            leg(a[0], a[2], -1.5)
        # The deck's mid bent: a leg under each girder, braced across.
        for z in (8.2, 13.8):
            a = S.p(-2.5, 0.0, z)
            leg(a[0], a[2], 1.2, sides=10)
        for y in (-4.0, -14.0, -28.0):
            S.rod((-2.5, y, 8.2), (-2.5, y, 13.8), 0.1, 'steel', sides=6)
            S.rod((-2.5, y, 8.2), (-2.5, y - 9.0, 13.8), 0.06, 'steel', sides=5)


# ------------------------------------------------------------------- the decks
def girder(S, x0, x1, z0, z1, y0, y1, rng):
    """A plate girder along x inside its box: a web between flanges at its top
    and foot, a stiffener every metre and a half, icicles off its foot."""
    S.box(x0, y0, z0 + 0.14, x1, y1, z1 - 0.14, 'steel', c=0.01, cell=1.6)
    S.box(x0, y0, z0, x1, y0 + 0.12, z1, 'steel', c=0.01, cell=3.0)
    S.box(x0, y1 - 0.1, z0, x1, y1, z1, 'steel', c=0.01, cell=3.0)
    for x in np.arange(x0 + 0.6, x1 - 0.3, 1.5):
        S.box(x, y0 + 0.12, z0 + 0.02, x + 0.08, y1 - 0.1, z1 - 0.02, 'steel', c=0.005)
    icicles(S, (x0, z0 - 0.01), (x1, z0 - 0.01), y0, rng, longest=0.5, fill=0.7)
    icicles(S, (x0, z1 + 0.01), (x1, z1 + 0.01), y0, rng, longest=0.5, fill=0.7)


def shed_wall(S, x0, y0, z0, x1, y1, z1, face, windows=None):
    """A wall of the mine's sheds (the gallery on the deck, the winding cab):
    grey-green sheet outside, lining inside, a steel coping, its ends closed;
    `windows` (a0, a1, y0, y1) lit panes painted on its sheet."""
    out, along = face_axes(face)
    if along == 'z':
        (po, pi) = (x0, x1) if out[0] < 0 else (x1, x0)
        a0, a1 = z0, z1
    else:
        (po, pi) = (z0, z1) if out[2] < 0 else (z1, z0)
        a0, a1 = x0, x1
    n = out[0] + out[2]
    top = y1 - 0.12
    corrugated(S, po, a0, a1, y0, top, face, 'clad_grey', back=0.0, fine=4)
    face_in = ('+' if face[0] == '-' else '-') + face[1]
    flat(S, pi, a0, a1, y0, top, face_in, 'lining', back=0.0, cell=0.35)
    lo, hi = sorted((po, pi))
    if along == 'z':
        S.box(lo, top, a0, hi, y1, a1, 'steel', c=0.02, cell=2.0)
    else:
        S.box(a0, top, lo, a1, y1, hi, 'steel', c=0.02, cell=2.0)
    snow_cap(S, along, lo, hi, a0, a1, y1, h=0.04)
    for a, sgn in ((a0, -1.0), (a1, 1.0)):
        o = (0.0, 0.0, sgn) if along == 'z' else (sgn, 0.0, 0.0)
        if along == 'z':
            S.quad((lo, y0, a), (hi, y0, a), (hi, top, a), (lo, top, a), 'steel', o)
        else:
            S.quad((a, y0, lo), (a, y0, hi), (a, top, hi), (a, top, lo), 'steel', o)
    if windows:
        wa0, wa1, wy0, wy1 = windows
        q = po + n * 0.012
        for a in np.arange(wa0, wa1 - 0.5, 1.1):
            if along == 'z':
                S.box(min(q, po), wy0, a, max(q, po), wy1, a + 0.8, 'window', c=0.0, cell=20.0)
            else:
                S.box(a, wy0, min(q, po), a + 0.8, wy1, max(q, po), 'window', c=0.0, cell=20.0)


def deck(S):
    """The deck from the side's mezzanine across the shaft to the far crag:
    tread plate on joists between two plate girders, its abutment on the
    side's crag, a ramp down at the far end; the gallery over its first stretch."""
    rng = random.Random(61 if S.s > 0 else 62)
    S.box(-13.0, 3.36, 8.0, 8.0, 3.5, 14.0, 'tread', c=0.0, cell=2.0, under=False)
    for x in np.arange(-11.7, 7.9, 1.5):
        S.box(x, 3.04, 8.4, x + 0.14, 3.36, 13.6, 'steel', c=0.01, cell=3.0)
    S.quad((-12.0, 3.36, 8.4), (8.0, 3.36, 8.4), (8.0, 3.36, 13.6), (-12.0, 3.36, 13.6), 'steel', (0, -1, 0), cell=3.0)
    for z0, z1 in ((8.0, 8.4), (13.6, 14.0)):
        girder(S, -12.0, 8.0, z0, z1, 1.2, 3.0, rng)
        S.box(-13.0, 3.0, z0, 8.0, 3.36, z1, 'steel', c=0.01, cell=3.0)
    # The abutment: board-marked concrete off the crag, under the deck's end.
    S.box(-13.0, -3.0, 8.0, -12.0, 3.0, 14.0, 'blast_dark', c=0.03, cell=1.2)
    # Its rail on the drop by the ramp.
    x0, x1 = 5.5, 7.8
    S.box(x0, 3.5, 13.72, x1, 4.3, 13.98, 'steel', c=0.01)
    S.box(x0, 4.3, 13.7, x1, 4.4, 14.0, 'galv', c=0.01)
    for x in np.arange(x0, x1 - 0.05, 1.0):
        S.box(x, 3.5, 13.7, x + 0.08, 4.3, 14.0, 'steel', c=0.005)
    snow_cap(S, 'x', 13.7, 14.0, x0, x1, 4.4, h=0.03)
    drift(S, -5.0, 8.0, 8.0, 14.0, 3.5, ['+z'], reach=1.3, seed=63 + S.s)
    drift(S, -13.0, 8.0, -10.0, 14.0, 3.5, ['+z'], reach=0.9, seed=64 + S.s)
    crate(S, -12.4, 3.5, 9.0, -11.2, 4.7, 10.2)
    gallery(S)
    # The ramp down to the far spur: tread on a concrete wedge.
    for z, o in ((11.0, -1.0), (14.0, 1.0)):
        S.tri((13.0, 0.0, z), (8.0, 0.0, z), (8.0, 3.5, z), 'blast', (0, 0, o))
    S.quad((13.0, 0.0, 11.0), (8.0, 3.5, 11.0), (8.0, 3.5, 14.0), (13.0, 0.0, 14.0), 'tread', (0.57, 0.82, 0.0), cell=2.0)
    S.quad((8.0, 0.0, 11.0), (8.0, 0.0, 14.0), (8.0, 3.0, 14.0), (8.0, 3.0, 11.0), 'blast', (-1, 0, 0))
    for t in np.arange(0.05, 1.0, 0.1):
        x = 13.0 - 5.0 * t
        y = 3.5 * t
        S.rod((x, y + 0.012, 11.05), (x, y + 0.012, 13.95), 0.012, 'galv', sides=4)


def gallery(S):
    """The roofed stretch of the deck past the mezzanine: a shed over the way,
    open at its ends, its windows lit, snow on its roof, a lamp inside."""
    shed_wall(S, -10.0, 3.5, 8.0, -5.0, 6.5, 8.4, '-z', windows=(-9.6, -5.4, 4.7, 5.5))
    shed_wall(S, -10.0, 3.5, 13.6, -5.0, 6.5, 14.0, '+z', windows=(-9.6, -5.4, 4.7, 5.5))
    S.box(-10.0, 6.86, 8.0, -5.0, 7.0, 14.0, 'steel', c=0.03, cell=2.0, under=False)
    S.quad((-10.0, 6.86, 8.4), (-5.0, 6.86, 8.4), (-5.0, 6.86, 13.6), (-10.0, 6.86, 13.6), 'steel', (0, -1, 0), cell=3.0)
    for x in np.arange(-9.9, -5.0, 1.2):
        S.box(x, 6.5, 8.4, x + 0.16, 6.86, 13.6, 'steel', c=0.01, cell=3.0)
    S.box(-10.0, 6.5, 8.0, -5.0, 6.86, 8.4, 'steel', c=0.01, cell=3.0)
    S.box(-10.0, 6.5, 13.6, -5.0, 6.86, 14.0, 'steel', c=0.01, cell=3.0)
    # Inside: a column at each end and the middle, a girt under the windows.
    for z0, z1 in ((8.4, 8.6), (13.4, 13.6)):
        for x in (-10.0, -7.6, -5.2):
            S.box(x, 3.5, z0, x + 0.2, 6.5, z1, 'steel', c=0.01)
        S.box(-10.0, 4.5, z0, -5.0, 4.66, z1, 'steel', c=0.01, cell=3.0)
    snow_top(S, -9.95, 7.0, 8.05, -5.05, 13.95, h=0.12)
    rng = random.Random(65 if S.s > 0 else 66)
    for z in (7.98, 14.02):
        icicles(S, (-10.0, z), (-5.0, z), 6.5, rng, longest=0.55, spacing=0.14)
    lamp(S, -7.5, 6.3, 11.0, 6.5)


def spur(S):
    """The spur's cover: crates and a low concrete wall."""
    crate(S, -19.5, 0.0, -9.0, -18.3, 1.2, -7.8)
    S.box(-17.0, 0.0, -12.5, -14.0, 1.1, -12.0, 'blast', c=0.06)
    snow_top(S, -16.97, 1.1, -12.47, -14.03, -12.03, h=0.05)
    crate(S, -15.2, 0.0, -6.2, -14.0, 1.2, -5.0)


def door_lamps(S):
    """A lamp over each door out of the shaft house: a bracket off the wall, a
    shade, its lens under it; and the warm pool it throws on the snow."""
    for x, y, z, face, floor in DOOR_LAMPS:
        out = (1.0, 0.0) if face == '+x' else (0.0, -1.0)
        if face == '+x':
            S.box(x - 0.38, y + 0.16, z - 0.04, x, y + 0.22, z + 0.04, 'steel', c=0.0)
        else:
            S.box(x - 0.04, y + 0.16, z, x + 0.04, y + 0.22, z + 0.38, 'steel', c=0.0)
        px, pz = _xz(S, x, z)
        spin(px, pz, [(0.001, y + 0.08), (0.24, y + 0.02), (0.27, y), (0.08, y + 0.2), (0.001, y + 0.2)], 'steel', sides=12)
        # A bulb under the shade that shows from the side.
        spin(px, pz, [(0.001, y - 0.16), (0.08, y - 0.13), (0.12, y - 0.06), (0.13, y + 0.02), (0.001, y + 0.02)], 'light_sodium', sides=12)
        pool(S, x + out[0] * 1.2, z + out[1] * 1.2, 3.2, floor)


def pool(S, x, z, r, y):
    """Lamplight lying on the snow: a disc added over the frame, brightest under
    the lamp, gone at `r`."""
    pa = part('pool')
    n, m = 28, 7
    cx, cz = _xz(S, x, z)
    base = pa.add([(cx, y + 0.02, cz)])
    pa.colors[base] = (0.16, 0.16, 0.16)
    for j in range(1, m + 1):
        rr = r * j / m
        k = 0.16 * (1.0 - j / m) ** 2
        for i in range(n):
            a = math.tau * i / n
            pa.verts.append((cx + math.cos(a) * rr, y + 0.02, cz + math.sin(a) * rr))
            pa.colors[len(pa.verts) - 1] = (k, k, k)
    for i in range(n):
        face(pa, (base, base + 1 + i, base + 1 + (i + 1) % n), (0, 1, 0))
    for j in range(m - 1):
        for i in range(n):
            a, b = base + 1 + j * n + i, base + 1 + j * n + (i + 1) % n
            face(pa, (a, b, b + n, a + n), (0, 1, 0))


def lane(S, pts, half, seed):
    """A way trodden through the snow: grey slush along a path, worst along its
    middle, broken at its edges."""
    rng = random.Random(seed)
    pa = part('track')
    ph = rng.random() * 10
    for (ax, az), (bx, bz) in zip(pts, pts[1:]):
        ln = math.hypot(bx - ax, bz - az)
        ux, uz = (bx - ax) / ln, (bz - az) / ln
        nx, nz = -uz, ux
        rows, cols = max(2, int(ln / 0.3)), 6
        base = len(pa.verts)
        for j in range(rows + 1):
            t = j / rows
            for i in range(cols + 1):
                s_ = -1.0 + 2.0 * i / cols
                x = ax + (bx - ax) * t + nx * s_ * half
                z = az + (bz - az) * t + nz * s_ * half
                wob = 0.5 + 0.5 * math.sin(x * 2.1 + ph) * math.sin(z * 1.7 - ph)
                end = min(1.0, t * ln / 0.8, (1 - t) * ln / 0.8) if (j == 0 or j == rows) else 1.0
                alpha = min(0.85, (1.0 - abs(s_) ** 2) * (0.45 + 0.4 * wob) * end)
                pa.verts.append(S.p(x, 0.014, z))
                pa.colors[len(pa.verts) - 1] = (1.0, 1.0, 1.0, alpha)
        for j in range(rows):
            for i in range(cols):
                k = base + j * (cols + 1) + i
                face(pa, (k, k + 1, k + cols + 2, k + cols + 1), (0, 1, 0), True)


def lanes(S):
    lane(S, [(-16.75, -2.8), (-16.5, -5.0), (-15.0, -9.0), (-12.8, -12.5)], 1.0, 83)
    lane(S, [(-16.75, -2.8), (-18.6, -6.2)], 0.8, 84)


# ---------------------------------------------------------- out in the blizzard
def crag(x, z, top, r, seed, lean=(0.0, 0.0)):
    """A rock needle standing out of the cloud: rings of noise up to its top, in
    beds that step in, so each bed's top is a ledge the snow lies on, and split
    by a few deep clefts down its height."""
    rng = np.random.default_rng(seed)
    pa = part('crag')
    n, m = 30, 46
    ys = np.linspace(-90.0, top, m)
    ph = rng.random(8) * 10
    clefts = rng.random(3) * math.tau
    bed = 4.0 + rng.random() * 3.0
    base = len(pa.verts)
    for j, y in enumerate(ys):
        t = j / (m - 1)
        rr = r * (1.0 + 1.1 * (1 - t) ** 2) * (1.0 - 0.5 * t ** 2.5)
        # The beds: within each, the face leans out toward its foot, so its top
        # steps in over the one under it.
        f = ((y - top) / bed) % 1.0
        rr *= 0.9 + 0.12 * f
        for i in range(n):
            a = math.tau * i / n
            k = 1.0 + 0.2 * math.sin(a * 3 + ph[0] + y * 0.05) + 0.1 * math.sin(a * 5 + ph[1] - y * 0.11) + 0.07 * math.sin(a * 11 + ph[2] + y * 0.3)
            for c in clefts:
                k -= 0.22 * max(0.0, 1.0 - abs((a - c + math.pi) % math.tau - math.pi) / 0.18)
            ox, oz = lean[0] * t * 30, lean[1] * t * 30
            pa.verts.append((x + ox + math.cos(a) * rr * k, y, z + oz + math.sin(a) * rr * k))
    for j in range(m - 1):
        for i in range(n):
            i1 = (i + 1) % n
            a, b = base + j * n + i, base + j * n + i1
            c, d = base + (j + 1) * n + i1, base + (j + 1) * n + i
            mx = (pa.verts[a][0] + pa.verts[c][0]) / 2 - x
            mz = (pa.verts[a][2] + pa.verts[c][2]) / 2 - z
            face(pa, (a, b, c, d), (mx, 0.0, mz), True)
    # A broken top: a few points round its middle, not one.
    cx, cz = x + lean[0] * 30, z + lean[1] * 30
    cap = pa.add([(cx + math.cos(a) * r * 0.25, top + r * (0.15 + 0.35 * rng.random()), cz + math.sin(a) * r * 0.25) for a in (0.0, 2.1, 4.2)])
    for i in range(n):
        i1 = (i + 1) % n
        k = cap + int(3 * i / n)
        face(pa, (base + (m - 1) * n + i, base + (m - 1) * n + i1, k), (0, 1, 0))
    face(pa, (cap, cap + 1, cap + 2), (0, 1, 0))


def winding_house(S):
    """Each side's winding house on its crag below the lip: clad in grey-green,
    its windows lit, a snow roof, a stack."""
    x0, x1, z0, z1, y0, y1 = -36.0, -28.0, 1.0, 14.6, -4.5, 1.2
    crag(*_xz(S, -32.0, 7.8), y0 - 0.2, 6.0, 101 if S.s > 0 else 102)
    for plane, a0, a1, face in ((x1, z0, z1, '+x'), (x0, z0, z1, '-x')):
        corrugated(S, plane, a0, a1, y0, y1, face, 'clad_grey', back=0.0, fine=3)
    for plane, a0, a1, face in ((z1, x0, x1, '+z'), (z0, x0, x1, '-z')):
        corrugated(S, plane, a0, a1, y0, y1, face, 'clad_grey', back=0.0, fine=3)
    # A pitched roof under snow.
    ridge = y1 + 2.4
    for za, zb, o in ((z0 - 0.3, z1 + 0.3, 1),):
        S.quad((x0 - 0.4, y1, za), (x0 - 0.4, y1, zb), ((x0 + x1) / 2, ridge, zb), ((x0 + x1) / 2, ridge, za), 'snow', (-0.5, 1, 0))
        S.quad((x1 + 0.4, y1, za), (x1 + 0.4, y1, zb), ((x0 + x1) / 2, ridge, zb), ((x0 + x1) / 2, ridge, za), 'snow', (0.5, 1, 0))
        for z in (za, zb):
            S.tri((x0, y1, z), (x1, y1, z), ((x0 + x1) / 2, ridge - 0.1, z), 'clad_grey', (0, 0, -1 if z == za else 1))
    # Its windows toward the mine, lit.
    for z in np.arange(z0 + 1.2, z1 - 1.0, 2.2):
        S.box(x1 + 0.005, -2.6, z, x1 + 0.03, -1.2, z + 1.2, 'window', c=0.0, cell=20.0)
    # The stack, smoking (the client's vents, `ambient.rs`'s STACKS).
    S.rod((-34.0, ridge - 0.6, 11.5), (-34.0, ridge + 4.5, 11.5), 0.4, 'steel', sides=10)
    icicles(S, (x1 + 0.45, z0), (x1 + 0.45, z1), y1, random.Random(5 + S.s), longest=0.9, spacing=0.2)


def cableway(S):
    """A cableway's trestles walking off from the winding house over the cloud,
    each on its own crag, two cables sagging between them, a tub or two on them."""
    pts = [(-33.0, 16.0, 3.0), (-46.0, 40.0, -6.0), (-62.0, 72.0, -14.0), (-84.0, 112.0, -22.0), (-112.0, 160.0, -32.0)]
    heads = []
    for i, (x, z, top) in enumerate(pts):
        h = 13.0 if i else 4.0
        if i:
            crag(*_xz(S, x, z), top, 4.0 + i * 1.2, 200 + i + (0 if S.s > 0 else 50))
        # An A-frame across the line, lattice legs to a crossarm.
        d = (pts[min(i + 1, len(pts) - 1)][0] - pts[max(i - 1, 0)][0], pts[min(i + 1, len(pts) - 1)][1] - pts[max(i - 1, 0)][1])
        ln = math.hypot(*d)
        u = (-d[1] / ln, d[0] / ln)
        for s in (-1, 1):
            foot = (x + u[0] * s * 3.2, top, z + u[1] * s * 3.2)
            head = (x + u[0] * s * 1.6, top + h, z + u[1] * s * 1.6)
            S.rod(foot, head, 0.2, 'rusty', sides=6)
        S.rod((x - u[0] * 2.6, top + h, z - u[1] * 2.6), (x + u[0] * 2.6, top + h, z + u[1] * 2.6), 0.18, 'steel', sides=6)
        for k in range(1, 4):
            y = top + h * k / 4
            w = 3.2 - 1.6 * k / 4
            S.rod((x - u[0] * w, y, z - u[1] * w), (x + u[0] * w, y, z + u[1] * w), 0.07, 'steel', sides=5)
        heads.append([(x + u[0] * s * 2.2, top + h + 0.1, z + u[1] * s * 2.2) for s in (-1, 1)])
    for i in range(len(heads) - 1):
        for c in range(2):
            a, b = heads[i][c], heads[i + 1][c]
            ln = math.dist(a, b)
            prev = a
            for k in range(1, 13):
                t = k / 12
                p = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t - ln * 0.05 * 4 * t * (1 - t), a[2] + (b[2] - a[2]) * t)
                S.rod(prev, p, 0.05, 'cable', sides=4)
                prev = p
        # A tub hung on the loaded cable.
        t = 0.4 + 0.2 * (i % 2)
        a, b = heads[i][0], heads[i + 1][0]
        ln = math.dist(a, b)
        p = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t - ln * 0.05 * 4 * t * (1 - t), a[2] + (b[2] - a[2]) * t)
        S.rod(p, (p[0], p[1] - 2.0, p[2]), 0.05, 'steel', sides=4)
        q = S.p(*p)
        cbox(q[0] - 0.7, p[1] - 3.0, q[2] - 0.5, q[0] + 0.7, p[1] - 2.0, q[2] + 0.5, 'rusty', c=0.05)


def peaks():
    """The range round the mine out of the cloud: one sheet painted by slope and
    height, snow where it can lie, dark rock where it can't, paler with distance."""
    pa = part('peaks')
    rings, segs = 70, 420
    r = 150.0 * (570.0 / 150.0) ** (np.arange(rings + 1) / rings)
    a = np.arange(segs) / segs * math.tau
    rr, aa = np.meshgrid(r, a, indexing='ij')
    x, z = rr * np.cos(aa), rr * np.sin(aa)
    # Massifs, not a fence of teeth: they rise from the cloud well out, broad,
    # each a few summits.
    t = np.clip((rr - 185.0) / 200.0, 0.0, 1.0)
    rise = t * t * (3.0 - 2.0 * t)
    y = -95.0 + rise * (55.0 + 150.0 * ridged(x * 0.42, z * 0.42, 17) ** 1.3)
    da = np.abs(np.roll(y, -1, axis=1) - np.roll(y, 1, axis=1)) / np.maximum(2.0 * rr * math.tau / segs, 1e-3)
    dr = np.abs(np.gradient(y, axis=0)) / np.maximum(np.gradient(rr, axis=0), 1e-3)
    slope = np.maximum(da, dr)
    # Dark rock in ribs and gullies down whatever is steep, more of it low.
    rock = np.clip((slope - 0.32) / 0.3, 0.0, 1.0)
    streak = ridged(x * 4.0, z * 4.0, 9)
    rock = np.clip(rock * (0.3 + 2.0 * streak ** 1.5), 0.0, 1.0)
    snow = np.array(SNOWY)
    stone = np.array([0.03, 0.035, 0.045])
    col = snow[None, None, :] * (1 - rock[..., None]) + stone[None, None, :] * rock[..., None]
    air = np.array([0.5, 0.56, 0.66])
    far = np.clip((rr - 260.0) / 260.0, 0.0, 1.0)[..., None] * 0.1
    col = col * (1 - far) + air[None, None, :] * far
    base = pa.add([(float(x[j, i]), float(y[j, i]), float(z[j, i])) for j in range(rings + 1) for i in range(segs)])
    for j in range(rings + 1):
        for i in range(segs):
            c = col[j, i]
            pa.colors[base + j * segs + i] = (float(c[0]), float(c[1]), float(c[2]))
    for j in range(rings):
        for i in range(segs):
            i1 = (i + 1) % segs
            p, q = base + j * segs + i, base + j * segs + i1
            u, v = base + (j + 1) * segs + i1, base + (j + 1) * segs + i
            face(pa, (p, v, u, q), (0, 1, 0), True)


def ridged(x, z, seed):
    """Ridged noise over (x, z) arrays: sharp crests, smooth valleys, in 0..1."""
    rng = np.random.default_rng(seed)
    n = 256
    table = rng.random((n, n))

    def value(px, pz):
        ix, iz = np.floor(px).astype(int), np.floor(pz).astype(int)
        fx, fz = px - ix, pz - iz
        ux, uz = fx * fx * (3 - 2 * fx), fz * fz * (3 - 2 * fz)
        i0, i1, j0, j1 = ix % n, (ix + 1) % n, iz % n, (iz + 1) % n
        return (table[i0, j0] * (1 - ux) + table[i1, j0] * ux) * (1 - uz) + (table[i0, j1] * (1 - ux) + table[i1, j1] * ux) * uz
    total, amp, f, weight = 0.0, 0.5, 1.0 / 90.0, np.ones_like(x)
    for _ in range(7):
        v = 1.0 - np.abs(value(x * f, z * f) * 2.0 - 1.0)
        v = v * v * weight
        weight = np.clip(v * 2.0, 0.0, 1.0)
        total = total + v * amp
        amp *= 0.52
        f *= 2.07
    return total


# ----------------------------------------------------------------------- the paint
def wobble(x, y, z):
    return 0.5 + 0.5 * math.sin(x * 0.53 + z * 0.37 + y * 0.21) * math.sin(z * 0.47 - x * 0.27 + y * 0.67)


def lamp_light(p, n):
    """The lamps' warm light on a point facing n, 0 to ~1."""
    k = 0.0
    for lx, ly, lz in LAMPS:
        d = (lx - p[0], ly - p[1], lz - p[2])
        dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
        if dd > 64.0:
            continue
        ln = math.sqrt(dd) or 1.0
        facing = (d[0] * n[0] + d[1] * n[1] + d[2] * n[2]) / ln
        if facing <= 0.0:
            continue
        # How much of the lamp it sees, over a few points of its shade, so the
        # light fades across an edge instead of stepping at a vertex.
        q = (p[0] + n[0] * 0.05, p[1] + n[1] * 0.05, p[2] + n[2] * 0.05)
        seen = sum(arch.seen(q, (lx + dx, ly - 0.1, lz + dz)) for dx, dz in ((0, 0), (0.35, 0), (-0.35, 0), (0, 0.35), (0, -0.35))) / 5.0
        k += seen * facing * 2.2 / (1.0 + dd)
    return min(k, 1.0)


def mix(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def smooth(e0, e1, x):
    t = max(0.0, min(1.0, (x - e0) / (e1 - e0)))
    return t * t * (3 - 2 * t)


def indoors(x, y, z, gn=(0.0, 0.0, 0.0)):
    """Whether a face looks into a shaft house, under its roof, where no wind or
    frost reaches: a step off it along its normal is inside the walls."""
    x, y, z = x + gn[0] * 0.3, y + gn[1] * 0.3, z + gn[2] * 0.3
    if x > 0:
        x, z = -x, -z
    return -21.25 <= x <= -13.15 and -2.25 <= z <= 14.25 and y < 6.84


def tone(p, n, name):
    x, y, z = p
    gn = (n.x, n.z, -n.y)
    inside = indoors(x, y, z, gn)
    # The frost is turned with the map: each side's own windward faces, so
    # neither side looks across at a frostier backdrop than the other.
    s = 1.0 if x < 0 else -1.0
    windward = 0.0 if inside else max(0.0, s * (gn[0] * UPWIND[0] + gn[2] * UPWIND[2]))
    up = gn[1]
    w = wobble(x, y, z)
    streak = 0.5 + 0.5 * math.sin((x + z) * 3.1 + math.sin((x - z) * 1.3) * 2.0)
    warm = lamp_light(p, gn)
    out = None
    if name in ('clad', 'clad_grey'):
        if name == 'clad_grey':
            paint_ = GREYGREEN
        else:
            paint_ = RED if x < 0 else BLUE
        paint_ = mix(paint_, (paint_[0] * 0.7, paint_[1] * 0.7, paint_[2] * 0.75), streak * 0.5)
        along = z if abs(gn[0]) > abs(gn[2]) else x
        trough = 0.5 - 0.5 * math.cos(math.tau * along / PITCH)
        # Rime only where the wind meets the sheet square, packed in its
        # troughs, in drifts down it; a line of it under the coping and over
        # the plinth, where the snow catches.
        rime = smooth(0.6, 0.95, windward) * trough ** 2 * smooth(0.45, 0.75, vnoise(x * 0.5 + z * 0.5, y * 0.7, 9)) * 0.85
        rime = max(rime, 0.6 * smooth(0.25, 0.0, abs(y - 7.75)) * trough)
        rime = max(rime, 0.5 * smooth(0.25, 0.0, y - 0.9) * trough if y > 0.9 else 0.0)
        out = mix(paint_, RIME, min(1.0, rime))
    elif name in ('steel', 'steel_paint', 'steel_box', 'rusty') or name.startswith('wheel_') or name.startswith('team_'):
        base = {'steel': STEEL, 'steel_paint': (0.06, 0.07, 0.075), 'steel_box': (0.12, 0.13, 0.12), 'rusty': RUSTY}.get(name, STEEL)
        if name.startswith('team_'):
            base = RED if name.endswith('red') else BLUE
        # What hangs in the shaft (the gantry, the cab's posts and stairs, the
        # cage, the collar, the decks' girders, the legs) is old steel gone
        # rust-brown, the same on both halves and in neither side's colour.
        hub = abs(x) < 12.5 and not inside and name in ('steel', 'rusty')
        if hub:
            base = HUB_STEEL if name == 'steel' else mix(RUSTY, HUB_STEEL, 0.5)
        base = mix(base, (base[0] * 0.6, base[1] * 0.6, base[2] * 0.62), streak * 0.4)
        # Rime feathers on the face the wind meets, in patches, never all of
        # it; snow on what looks up.
        # In patches: at most a third of a windward face, the steel dark under it.
        patch = smooth(0.55, 0.75, vnoise(x * 0.8 + z * 0.3, y * 1.3, 5))
        rime = smooth(0.5, 0.9, windward) * patch * 0.9
        # Out in the shaft's wind every upright face takes a thin frost.
        if hub and abs(up) < 0.6:
            rime = max(rime, 0.22 + 0.1 * w)
        out = mix(base, RIME_SIDE, rime)
        # Snow lies on what looks up, but not down the shaft under the collar.
        if up > 0.6 and not inside and y > -1.6:
            out = mix(out, RIME, 0.8 * (0.6 + 0.4 * w))
        # The legs go into the cloud: grey-blue with depth, never black.
        if y < -2.0:
            out = mix(out, (0.3, 0.33, 0.4), 0.7 * smooth(-2.0, -30.0, y))
    elif name == 'galv':
        patch = smooth(0.55, 0.75, vnoise(x * 0.8 + z * 0.3, y * 1.3, 5))
        rime = smooth(0.5, 0.9, windward) * patch * 0.8
        out = mix(GALV, RIME_SIDE, rime)
        if up > 0.6 and not inside:
            out = mix(out, RIME, 0.6)
    elif name == 'cable':
        out = (0.03, 0.03, 0.035)
    elif name == 'crag':
        snow = max(0.0, min(1.0, (up - 0.35) / 0.3)) * (0.5 + 0.5 * w)
        out = mix(ROCK, SNOWY, snow)
        # The cloud the needle stands in: lost into it from its foot up.
        fog = max(0.0, min(1.0, (-40.0 - y) / 40.0))
        out = mix(out, (0.55, 0.6, 0.68), fog)
    elif name in ('blast', 'blast_dark', 'floor'):
        k = 0.86 + 0.14 * w
        k *= 1.0 - 0.14 * streak if abs(up) < 0.5 else 1.0
        # Wet and dark at a wall's foot where the snow lies against it.
        k *= 1.0 - 0.25 * max(0.0, 1.0 - y / 0.6) if 0.0 <= y < 0.6 and abs(up) < 0.5 else 1.0
        out = (k * 0.76, k * 0.88, k * 1.1)
    elif name == 'tread':
        k = 0.84 + 0.16 * w
        out = (k * 0.86, k * 0.92, k * 1.0)
    elif name == 'rust':
        # Old steel's rust, dark and red as it is in the cold, not ochre.
        k = 0.8 + 0.2 * w
        out = (k * 0.36, k * 0.24, k * 0.21)
    elif name in ('blast', 'blast_dark', 'floor'):
        k = 0.86 + 0.14 * w
        k *= 1.0 - 0.14 * streak if abs(up) < 0.5 else 1.0
        # Wet and dark at a wall's foot where the snow lies against it.
        k *= 1.0 - 0.25 * max(0.0, 1.0 - y / 0.6) if 0.0 <= y < 0.6 and abs(up) < 0.5 else 1.0
        out = (k * 0.76, k * 0.88, k * 1.1)
    elif name == 'tread':
        k = 0.84 + 0.16 * w
        out = (k * 0.86, k * 0.92, k * 1.0)
    elif name == 'rust':
        # Old steel's rust, dark and red as it is in the cold, not ochre.
        k = 0.8 + 0.2 * w
        out = (k * 0.36, k * 0.24, k * 0.21)
    elif name == 'pitwall':
        k = 0.75 + 0.25 * w
        out = (0.11 * k, 0.12 * k, 0.135 * k)
        # Seeps: darker streaks down the pours from the ring beams.
        out = mix(out, (0.05, 0.05, 0.055), 0.5 * smooth(0.6, 0.85, vnoise(x * 0.9 + z * 0.9, y * 0.08, 17)))
        if y > -6.0 and up < 0.5:
            out = mix(out, RIME_SIDE, 0.5 * smooth(-6.0, 0.4, y) * smooth(0.45, 0.7, vnoise(x * 0.6 + z * 0.6, y * 0.9, 3)))
        out = mix(out, (0.004, 0.005, 0.007), 0.92 * smooth(-3.0, -40.0, y))
    elif name == 'lining':
        k = 0.8 + 0.2 * w
        out = (0.09 * k, 0.1 * k, 0.11 * k)
    else:
        out = (1.0, 1.0, 1.0)
    if warm > 0.0:
        # The lamps light the room themselves; this is their warmth on what
        # the light misses (no shadows under the decks), a touch.
        out = (out[0] + warm * LAMP_RGB[0] * 0.3, out[1] + warm * LAMP_RGB[1] * 0.3, out[2] + warm * LAMP_RGB[2] * 0.3)
    return out


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    for S in SIDES:
        shaft_house(S)
        headframe(S)
        deck(S)
        spur(S)
        winding_house(S)
        cableway(S)
        door_lamps(S)
        lanes(S)
    gantry()
    cage()
    collar()
    legs()
    # Needles out of the cloud round the rock, far enough to stand apart.
    for i, (x, z, top, r) in enumerate([(30.0, 40.0, -14.0, 5.0), (-58.0, -30.0, -8.0, 7.0), (70.0, -55.0, 4.0, 9.0),
                                         (-20.0, -62.0, -20.0, 6.0), (95.0, 30.0, 12.0, 11.0), (-90.0, 60.0, 8.0, 10.0),
                                         (10.0, 85.0, -2.0, 8.0), (-40.0, 110.0, 14.0, 12.0)]):
        crag(x, z, top, r, 300 + i)
    peaks()
    objs = make_objects()

    def ground(verts, polys):
        # The crags' tops and their faces into the shaft, for the occlusion.
        def quad_(pts):
            b = len(verts)
            verts.extend([to_blender(p) for p in pts])
            polys.append((b, b + 1, b + 2, b + 3))
        for s_ in (1, -1):
            quad_([(s_ * -21.0, 0.0, -14.0 * s_), (s_ * -12.0, 0.0, -14.0 * s_), (s_ * -12.0, 0.0, 14.0 * s_), (s_ * -21.0, 0.0, 14.0 * s_)])
            quad_([(s_ * -12.0, 0.0, -14.0 * s_), (s_ * -12.0, 0.0, 14.0 * s_), (s_ * -12.0, -40.0, 14.0 * s_), (s_ * -12.0, -40.0, -14.0 * s_)])
    paint(objs, ground, tone)
    export(objs, OUT, 'pithead')


main()

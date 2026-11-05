"""Forge, the human line's shared parts, on top of lib's primitives: the
display wedge, the rail, the vented shroud, the flashlight pod, the grip
with its guard, the mag, the muzzle, panel lines. Every helper returns a
list of parts for `finish`, in gun space (+X forward, +Z up, +Y left).

Not a gun: build.sh will run it and it does nothing.
"""

import math

from lib import *


def turned(o, ry=0.0, at=(0, 0, 0)):
    """A part built about the origin, turned `ry` degrees about Y (positive
    tips its top forward) and its origin put at `at`."""
    rotate(o, ry=ry)
    move(o, *at)
    return o


def panel(dx, dz, at, side=1, material='gunmetal_dark', both=True, bevel=0.0005, thick=0.002):
    """A thin strip 1 mm proud of a side face at |y| = at[1]: a panel line, a
    port, a vent. `both` mirrors it to the other side."""
    x, y, z = at
    o = box(dx, thick, dz, (x, y * side, z), material, bevel=bevel, seg=1)
    return mirror_y(o) if both else o


def display_wedge(x0, x1, zb, h, w, angle=20, material='gunmetal_dark', brow=True, nose=0.5, margin=0.004):
    """The ammo display's housing: a wedge on the rail whose rear face leans
    back `angle` degrees toward the shooter, the glowing panel and its bezel
    sitting on that face; its front slopes down to the rail over `nose` of
    its height. `brow` adds a dark lip over the panel."""
    a = math.radians(angle)
    t = math.tan(a)
    pts = [(x0, zb), (x1, zb), (x1, zb + h * (1 - nose)), (x1 - h * nose * 1.2, zb + h), (x0 + h * t, zb + h)]
    parts = [profile(pts, w, 'xz', material=material, bevel=0.003)]
    n = Vector((-math.cos(a), 0, math.sin(a)))
    mid = Vector((x0 + h * t / 2, 0, zb + h / 2))
    bezel = box(0.002, w - 2 * margin, h - 2 * margin, material='polymer', bevel=0.001, seg=1)
    turned(bezel, angle, mid + n * 0.0009)
    face = box(0.002, w - 4 * margin, h - 4 * margin, material='display', bevel=0.0005, seg=1)
    turned(face, angle, mid + n * 0.0019)
    parts += [bezel, face]
    if brow:
        parts.append(box(0.016, w + 0.004, 0.006, (x0 + h * t - 0.004, 0, zb + h + 0.002), material, bevel=0.0015, seg=1))
    return parts


def rail(x0, x1, z, w=0.021, notches=0):
    """A top rail along X with its base at `z`: a dovetail profile, and
    `notches` dark cross strips."""
    hw = w / 2
    pts = [(-hw + 0.003, z), (hw - 0.003, z), (hw - 0.003, z + 0.005), (hw, z + 0.005), (hw, z + 0.011), (-hw, z + 0.011), (-hw, z + 0.005), (-hw + 0.003, z + 0.005)]
    parts = [profile(pts, x1 - x0, 'yz', at=(x0 + x1) / 2, material='gunmetal_dark', bevel=0.001, seg=1)]
    for i in range(notches):
        x = x0 + (i + 0.5) * (x1 - x0) / notches
        parts.append(box(0.004, w + 0.002, 0.003, (x, 0, z + 0.0105), 'polymer', bevel=0))
    return parts


def shroud(x0, x1, w, h, z=0.0, material='gunmetal', slots=3, slot_len=0.03, slot_mat='polymer'):
    """A chamfered barrel shroud from x0 to x1, `slots` vent strips a side."""
    parts = [box(x1 - x0, w, h, ((x0 + x1) / 2, 0, z), material)]
    for i in range(slots):
        x = x0 + (x1 - x0) * (i + 1) / (slots + 1)
        parts.append(panel(slot_len, h * 0.28, (x, w / 2, z), material=slot_mat))
    return parts


def flashlight(x0, x1, r, z, mount_z=None, verts=24):
    """A flashlight pod along X, lens at x1, under the barrel at `z`; a
    mount block up to `mount_z`."""
    L = x1 - x0
    parts = [
        cyl(r, L - 0.012, 'x', ((x0 + x1) / 2 - 0.004, 0, z), 'gunmetal_dark', verts=verts, bevel=0.002),
        tube(r + 0.001, r - 0.004, 0.012, 'x', (x1 - 0.006, 0, z), 'steel', verts=verts),
        cyl(r - 0.004, 0.004, 'x', (x1 - 0.002, 0, z), 'glass', verts=verts, bevel=0),
        cyl(r * 0.75, 0.012, 'x', (x0 + 0.004, 0, z), 'rubber', verts=verts, bevel=0.002),
    ]
    if mount_z is not None:
        parts.append(box(L * 0.4, r * 1.3, abs(mount_z - z) + 0.004, ((x0 + x1) / 2, 0, (mount_z + z) / 2), 'gunmetal_dark', bevel=0.002))
    return parts


def grip(x, z_top, depth=0.085, rake=0.026, w=0.032, t=0.040, guard=True, forward=0.052, trigger=True):
    """A polymer pistol grip hanging from `z_top`, its top centred on `x`,
    raked back; a trigger guard `forward` of it. Returns the parts; the hand
    is at about x - rake / 2."""
    pts = [(x - t / 2, z_top), (x + t / 2, z_top), (x + t / 2 - rake, z_top - depth), (x - t / 2 - rake, z_top - depth)]
    parts = [profile(pts, w, 'xz', material='polymer', bevel=0.004)]
    # a cap on the bottom
    parts.append(box(t + 0.004, w + 0.004, 0.006, (x - rake, 0, z_top - depth + 0.003), 'gunmetal_dark', bevel=0.0015, seg=1))
    if guard:
        gh = 0.048
        xf = x + t / 2 + forward
        xr = x + t / 2 - rake * gh / depth - 0.006
        parts.append(box(0.008, 0.016, gh, (xf, 0, z_top - gh / 2), 'polymer', bevel=0.0015, seg=1))
        parts.append(box(xf - xr + 0.004, 0.016, 0.009, ((xf + xr) / 2, 0, z_top - gh + 0.0045), 'polymer', bevel=0.0015, seg=1))
    if trigger:
        parts.append(box(0.006, 0.010, 0.026, (x + t / 2 + 0.017, 0, z_top - 0.018), 'gunmetal_dark', bevel=0.001, seg=1))
    return parts


def mag(x, z_top, depth, dx=0.064, w=0.028, curve=8.0, material='polymer', well=True):
    """A mag hanging from `z_top` at centre `x`: two boxes at `curve`
    degrees, the lower swung forward, a base plate; `well` a lip around
    its mouth."""
    parts = []
    if well:
        parts.append(box(dx + 0.014, w + 0.010, 0.026, (x, 0, z_top - 0.010), 'gunmetal_dark', bevel=0.003))
    if curve == 0:
        parts.append(box(dx, w, depth, (x, 0, z_top - depth / 2), material))
        parts.append(box(dx + 0.005, w + 0.004, 0.008, (x, 0, z_top - depth + 0.004), 'gunmetal_dark', bevel=0.0015, seg=1))
        return parts
    h1 = depth * 0.5
    h2 = depth * 0.55
    parts.append(box(dx, w, h1, (x, 0, z_top - h1 / 2), material))
    b = math.radians(-curve)
    c = Vector((x - (h2 / 2) * math.sin(b), 0, z_top - h1 - (h2 / 2) * math.cos(b)))
    lower = box(dx - 0.004, w, h2, material=material)
    turned(lower, -curve, c)
    base = box(dx + 0.005, w + 0.004, 0.008, (0, 0, -h2 / 2 + 0.002), 'gunmetal_dark', bevel=0.0015, seg=1)
    turned(base, -curve, c)
    rib = box(0.003, w + 0.002, h1 * 0.7, (x + dx * 0.25, 0, z_top - h1 / 2 - 0.004), 'gunmetal_dark', bevel=0, seg=1)
    parts += [lower, base, rib]
    return parts


def brake(x_end, r, length=0.04, r_out=None, slots=3, verts=24):
    """A muzzle brake at the barrel's end: a wider steel tube, slots cut as
    dark strips on top and both sides."""
    ro = r_out or r + 0.006
    xc = x_end - length / 2
    parts = [tube(ro, r - 0.001, length, 'x', (xc, 0, 0), 'steel', verts=verts)]
    for i in range(slots):
        x = x_end - length * (i + 1) / (slots + 1)
        parts.append(box(0.004, ro * 0.9, 0.003, (x, 0, ro), 'polymer', bevel=0, seg=1))
        parts.append(panel(0.004, ro * 0.9, (x, ro, 0), material='polymer', bevel=0))
    return parts


def butt_pad(x0, w, h, z=0.0, thick=0.016):
    """A rubber pad on the stock's rear, its back at x0."""
    return [box(thick, w, h, (x0 + thick / 2, 0, z), 'rubber', bevel=0.003)]


def led(at, material='amber', dx=0.006, dz=0.004):
    """A tiny indicator on a side face at |y| = at[1]."""
    x, y, z = at
    return [box(dx, 0.002, dz, (x, y, z), material, bevel=0, seg=1)]

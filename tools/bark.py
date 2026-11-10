#!/usr/bin/env python3
"""The bark of Triple T's own hands in view (`tools/ground.sh bark` calls this):
a piece of Poly Haven's photograph of furrowed brown bark, cut to lie on a hand.

    tools/bark.py SRC_DIR ID OUT_DIR      # → OUT_DIR/bark_{albedo,normal}.jpg

Triple T's map is one atlas for all of him, a millimetre a texel: bark across
a room, a smear on the hands a player holds up to the eye. `look/bark.rs` lays
this over them by where a pixel lies on the hand, so it has to be small (a
furrow a few millimetres across, as his own are), fine (a seventh of a
millimetre a texel), his colour, and lay edge to edge:

- **A piece, not the tile.** The photograph is a metre of trunk. Half of it,
  clear of its knots (a knot every hand's width is a pattern), is laid
  `BARK_TILE_M` across (`look/bark.rs`), which is its furrows at the size of
  his.
- **Edge to edge.** The piece is no tile: its last `BLEND` texels are faded
  into its first, each way.
- **His colour.** Its mean is brought to the atlas' own on a hand, so the
  hands in view are the arms everyone else sees; and the trunk's own broad
  light and dark (a pale side, a dark one) are taken out, or they come round
  as a stripe every tile. The atlas lays his own over it.
- **Two maps.** The colour; and the normal (OpenGL's green) with the
  photograph's occlusion where a normal map keeps its z, which the shader
  works out again from the other two.

numpy and Pillow, as the models' cuts.
"""

import sys

import numpy as np
from PIL import Image

# The cut's side, px, and how many of the piece's texels fade into its other
# edge.
PX = 1024
BLEND = 96
# Where the piece is taken, as shares of the photograph from its top left:
# under its knots.
AT = (0.2, 0.45)
# The atlas' mean on a hand, linear (`tools/models/hand.py` bakes the body's
# lift in): what the piece's mean is brought to.
MEAN = (0.228, 0.143, 0.069)
# How much of the piece's broad light and dark is taken out, and how broad
# that is, as a share of its side.
FLATTEN = 0.9
FLATTEN_SIZE = 0.12
# How much of the photograph's own swing of colour round its mean is kept:
# over 1 draws its furrows darker and its ridges paler.
CONTRAST = 1.0
# How much of its occlusion is laid into its colour: a furrow is dark in any
# light, and the light a match's lamps throw has no occlusion.
SHADE = 0.25
# How steep its normals are against the photograph's, at the hand's size.
STEEP = 1.3


def to_linear(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def to_srgb(c):
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * np.clip(c, 0.0, None) ** (1 / 2.4) - 0.055)


def piece(a):
    """The piece of a map, its far edges faded into its near ones: PX square
    and periodic."""
    h, w = a.shape[:2]
    x0, y0 = int(AT[0] * w), int(AT[1] * h)
    n = PX + BLEND
    assert x0 + n <= w and y0 + n <= h, 'the piece runs off the photograph'
    c = a[y0:y0 + n, x0:x0 + n].astype(np.float32)
    t = np.arange(BLEND, dtype=np.float32) / BLEND
    t = t * t * (3.0 - 2.0 * t)
    for axis in (0, 1):
        ramp = t.reshape([-1 if i == axis else 1 for i in range(c.ndim)])
        near = np.take(c, range(BLEND), axis=axis)
        far = np.take(c, range(PX, PX + BLEND), axis=axis)
        body = np.take(c, range(BLEND, PX), axis=axis)
        c = np.concatenate([far + (near - far) * ramp, body], axis=axis)
    return c


def blur(a, size):
    """`a` blurred over `size` of its side, round its edges (it is periodic)."""
    f = np.fft.fftfreq(a.shape[0])
    k = np.exp(-2.0 * (np.pi * size * a.shape[0]) ** 2 * (f[:, None] ** 2 + f[None, :] ** 2))
    return np.real(np.fft.ifft2(np.fft.fft2(a) * k)).astype(np.float32)


def load(src, ident, what, mode):
    return np.asarray(Image.open(f'{src}/{ident}_{what}_2k.jpg').convert(mode), dtype=np.float32) / 255.0


def main():
    src, ident, out = sys.argv[1:4]
    ao = piece(load(src, ident, 'ao', 'L'))
    colour = to_linear(piece(load(src, ident, 'diff', 'RGB')))
    # Each of red, green and blue by its own: a pale side is a greyer one.
    for ch in range(3):
        broad = blur(colour[..., ch], FLATTEN_SIZE)
        colour[..., ch] *= (float(colour[..., ch].mean()) / np.maximum(broad, 1e-3)) ** FLATTEN
    colour *= (1.0 - SHADE * (1.0 - ao))[..., None]
    mean = colour.reshape(-1, 3).mean(0)
    colour = np.array(MEAN, np.float32) * (colour / mean) ** CONTRAST
    colour *= np.array(MEAN, np.float32) / colour.reshape(-1, 3).mean(0)
    Image.fromarray((np.clip(to_srgb(colour), 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8)).save(
        f'{out}/bark_albedo.jpg', quality=88, subsampling=0)

    n = piece(load(src, ident, 'nor_gl', 'RGB'))[..., :2] * 2.0 - 1.0
    n *= STEEP
    n /= np.maximum(np.linalg.norm(n, axis=-1, keepdims=True), 1.0)
    packed = np.concatenate([n * 0.5 + 0.5, ao[..., None]], axis=-1)
    Image.fromarray((np.clip(packed, 0.0, 1.0) * 255.0 + 0.5).astype(np.uint8)).save(
        f'{out}/bark_normal.jpg', quality=90, subsampling=0)
    got = colour.reshape(-1, 3).mean(0)
    print('bark    %-16s linear mean %.3f %.3f %.3f   occlusion %.2f   (a piece %d px of the photograph, laid edge to edge)'
          % (ident, got[0], got[1], got[2], float(ao.mean()), PX))


if __name__ == '__main__':
    main()

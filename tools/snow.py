#!/usr/bin/env python3
"""The snow's set (`tools/ground.sh snow` calls this): Poly Haven's photograph
of crusted snow laid over a relief the wind left.

    tools/snow.py SRC_DIR ID OUT_DIR      # → OUT_DIR/snow_{albedo,normal,rough,ao}.jpg

As it came, the photograph is grey crust full of black pits, and lifted to
snow's white its pits were pepper: a floor of static, one value from wall to
wall. Snow's own colour is nearly one white; what a snowfield shows is its
shape. So the pits are closed and the photograph flattened toward white, and
the shape is made here: swells a few metres across with the wind's ripples
over them in patches, as a height the cut's maps are taken from. Its slopes
go into the normal map under the photograph's own crust (a low sun and a lamp
rake them), and how deep a place lies below the snow round it into the
occlusion map, which is what shades it under the sky alone.

Everything is periodic over the tile, so it lays edge to edge. numpy and
ImageMagick's `convert`, as the other cuts.
"""

import subprocess
import sys

import numpy as np

# The cut's size, px, and the size it is laid at, m (`look/built.rs`'s
# `SNOW_TILE_M`, `SNOW_LAID` times over). The photograph was shot 2.6 m
# across: it is laid twice across the tile, so its crust keeps its own size.
PX = 1024
TILE_M = 5.2
PHOTO = 2

# The photograph: its pits closed over this radius, px of the photograph laid
# (a pit is a centimetre), what is left of its contrast, and the white it is
# lifted to (linear: old snow in flat light).
PIT_PX = 2
CONTRAST = 0.45
WHITE = 0.6
# How much of the photograph's own crust the normal map keeps.
CRUST = 1.0
# Flecks of ice in its crust, which is its grain at the feet: how much
# brighter one is than the snow (of its stored value), and the share of the
# snow that is one.
# Nothing in it is dark: dark grain in snow is the pepper again.
FLECK = 0.08
FLECK_SHARE = 0.1

# The swells: their length, m, and their height (a standard deviation), m.
SWELL_M = 2.6
SWELL_H = 0.022
# The wind's ripples: their length crest to crest, m, how far that wanders
# (a share), how long a crest runs against its width, their height, m, and
# the bearing the wind blew on over the tile, degrees from its u.
RIPPLE_M = 0.55
RIPPLE_SPREAD = 0.3
RIPPLE_LONG = 3.0
RIPPLE_H = 0.006
WIND_DEG = 35.0
# Ripples lie in patches, the snow between them smooth (ripple from wall to
# wall, in blue, is water): the patches' size, m, how far the field that
# places them is pushed toward smooth (0: half the snow rippled), and how
# much ripple the smooth snow keeps.
PATCH_M = 2.2
PATCH_BIAS = 0.45
PATCH_FLOOR = 0.05
# A crest is sharper than its trough: this much of the ripple's square added.
CREST = 0.35

# The shade of a hollow under the sky, which is all that shows the snow's
# shape where no sun reaches: a ripple's trough (how far round a place is
# looked, m, and how dark the deepest are), and a swell's basin.
HOLLOW_M = 0.14
HOLLOW = 0.3
BASIN_M = 0.7
BASIN = 0.2
SEED = 7


def read(path, px, grey):
    fmt = 'pgm' if grey else 'ppm'
    raw = subprocess.run(['convert', path, '-filter', 'Lanczos', '-resize', f'{px}x{px}!', '-depth', '8', f'{fmt}:-'],
                         check=True, capture_output=True).stdout
    _, size, _, body = raw.split(b'\n', 3)
    w, h = map(int, size.split())
    a = np.frombuffer(body, np.uint8)[:w * h * (1 if grey else 3)]
    return (a.reshape(h, w) if grey else a.reshape(h, w, 3)).astype(np.float64) / 255.0


def write(a, path, quality):
    a = np.clip(np.rint(a * 255.0), 0, 255).astype(np.uint8)
    grey = a.ndim == 2
    head = b'%s\n%d %d\n255\n' % (b'P5' if grey else b'P6', a.shape[1], a.shape[0])
    kind = ['-colorspace', 'Gray'] if grey else ['-type', 'TrueColor']
    subprocess.run(['convert', ('pgm:-' if grey else 'ppm:-'), '-strip', '-quality', str(quality), *kind, path],
                   input=head + a.tobytes(), check=True)


def linear(v):
    return np.where(v <= 0.04045, v / 12.92, ((v + 0.055) / 1.055) ** 2.4)


def srgb(v):
    return np.where(v <= 0.0031308, v * 12.92, 1.055 * np.maximum(v, 0.0) ** (1 / 2.4) - 0.055)


def disk(r):
    return [(dy, dx) for dy in range(-r, r + 1) for dx in range(-r, r + 1) if dx * dx + dy * dy <= r * r + 1]


def close(a, r):
    """Pits no wider than `r` filled (a grey closing), round the tile's edges."""
    grown = np.max([np.roll(a, s, (0, 1)) for s in disk(r)], axis=0)
    return np.min([np.roll(grown, s, (0, 1)) for s in disk(r)], axis=0)


def laid(a):
    return np.tile(a, (PHOTO, PHOTO) + (1,) * (a.ndim - 2))


# Cycles a tile, along the image's columns (u) and its rows.
KX = np.fft.fftfreq(PX, d=1.0 / PX)[None, :]
KY = np.fft.fftfreq(PX, d=1.0 / PX)[:, None]


def field(rng, weight):
    """Noise with this spectrum, periodic over the tile, its deviation 1."""
    f = np.real(np.fft.ifft2(np.fft.fft2(rng.standard_normal((PX, PX))) * weight))
    return (f - f.mean()) / f.std()


def ring(length_m, spread):
    """Features `length_m` across, any way round."""
    k0 = TILE_M / length_m
    return np.exp(-((np.hypot(KX, KY) - k0) ** 2) / (2 * (k0 * spread) ** 2))


def blur(a, sigma_m):
    s = sigma_m / TILE_M
    return np.real(np.fft.ifft2(np.fft.fft2(a) * np.exp(-2 * (np.pi * s) ** 2 * (KX ** 2 + KY ** 2))))


def relief():
    """The snow's height over the tile, m."""
    rng = np.random.default_rng(SEED)
    swell = field(rng, ring(SWELL_M, 0.45)) * SWELL_H
    # The ripples: waves across the wind, their crests `RIPPLE_LONG` times
    # longer than the wave.
    wind = np.radians(WIND_DEG)
    along = KX * np.cos(wind) + KY * np.sin(wind)
    across = -KX * np.sin(wind) + KY * np.cos(wind)
    k0 = TILE_M / RIPPLE_M
    spread = k0 * RIPPLE_SPREAD
    ripple = field(rng, np.exp(-((np.abs(along) - k0) ** 2) / (2 * spread ** 2)
                               - across ** 2 / (2 * (k0 / RIPPLE_LONG) ** 2)))
    ripple = ripple + CREST * (ripple * ripple - 1.0)
    patch = np.clip(0.5 + 0.9 * (field(rng, ring(PATCH_M, 0.5)) - PATCH_BIAS), 0.0, 1.0)
    patch = patch * patch * (3.0 - 2.0 * patch)
    return swell + ripple * (PATCH_FLOOR + (1.0 - PATCH_FLOOR) * patch) * RIPPLE_H


def main():
    src, name, out = sys.argv[1:4]
    part = lambda kind: f'{src}/{name}_{kind}_2k.jpg'
    px = PX // PHOTO
    h = relief()

    # Its colour: the photograph's grey with its pits closed, most of its
    # contrast taken, lifted to snow's white.
    grey = close(read(part('diff'), px, False) @ np.array([0.2126, 0.7152, 0.0722]), PIT_PX)
    grey = grey.mean() + (grey - grey.mean()) * CONTRAST
    grey = laid(srgb(linear(grey) * WHITE / linear(grey).mean()))
    # Its flecks: the brightest texels of a noise of its own, a texel each.
    fleck = np.random.default_rng(SEED + 1).random((PX, PX))
    grey = grey * (1.0 + FLECK * (fleck > 1.0 - FLECK_SHARE))
    write(np.repeat(grey[:, :, None], 3, axis=2), f'{out}/snow_albedo.jpg', 88)

    # Its slopes: the relief's, with the photograph's crust on them. A
    # normal map's green is up the image (GL's), and a row runs down it.
    crust = laid(read(part('nor_gl'), px, False)) * 2.0 - 1.0
    nz = np.maximum(crust[:, :, 2], 0.2)
    spec = np.fft.fft2(h)
    du = np.real(np.fft.ifft2(spec * 2j * np.pi * KX)) / TILE_M
    dv = -np.real(np.fft.ifft2(spec * 2j * np.pi * KY)) / TILE_M
    n = np.stack([crust[:, :, 0] / nz * CRUST - du, crust[:, :, 1] / nz * CRUST - dv, np.ones_like(du)], axis=2)
    n /= np.linalg.norm(n, axis=2, keepdims=True)
    write(n * 0.5 + 0.5, f'{out}/snow_normal.jpg', 90)

    # Its shade under the sky: a place lying below the snow round it is a
    # hollow, and the deepest are `HOLLOW` darker, at the ripples' size and
    # at the swells'; the photograph's own under that.
    def hollow(round_m):
        deep = blur(h, round_m) - h
        return np.clip(0.5 + deep / (4.0 * deep.std()), 0.0, 1.0)
    write(laid(read(part('ao'), px, True)) * (1.0 - HOLLOW * hollow(HOLLOW_M)) * (1.0 - BASIN * hollow(BASIN_M)),
          f'{out}/snow_ao.jpg', 84)
    write(laid(read(part('rough'), px, True)), f'{out}/snow_rough.jpg', 82)

    slope = np.degrees(np.arctan(np.hypot(du, dv)))
    print(f'snow    relief ±{h.std() * 100:.1f} cm, slopes {slope.mean():.1f}° mean {np.percentile(slope, 99):.1f}° at most, '
          f'white {linear(grey).mean():.3f} linear')


if __name__ == '__main__':
    main()

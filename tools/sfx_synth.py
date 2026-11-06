#!/usr/bin/env python3
# python3 tools/sfx_synth.py [name…]: the truck's sounds, made here from
# noise, tones and our CC0 metal impacts (Kenney's, cut by Gates) rather than
# by a model: a landing's slam, a crash, a tyre's skid and a fire (the last
# two loops the game holds and pitches). Each is mono 44.1 kHz Vorbis in
# crates/client/assets/sound, written by ffmpeg. Seeded: the same every run.
# Needs numpy and ffmpeg with libvorbis.

import subprocess
import sys
import wave
from pathlib import Path

import numpy as np

SR = 44_100
OUT = Path('crates/client/assets/sound')
rng = np.random.default_rng(0x7_2C_4)


def clip(name):
    """One of our clips, as floats."""
    raw = subprocess.run(
        ['ffmpeg', '-v', 'error', '-i', str(OUT / f'{name}.ogg'), '-f', 'f32le', '-ac', '1', '-ar', str(SR), '-'],
        check=True, capture_output=True).stdout
    return np.frombuffer(raw, dtype=np.float32).astype(np.float64)


def pitch(x, k):
    """Played `k` times as fast (lower and longer under 1)."""
    t = np.arange(0, len(x) - 1, k)
    return np.interp(t, np.arange(len(x)), x)


def lowpass(x, hz):
    a = np.exp(-2 * np.pi * hz / SR)
    y = np.empty_like(x)
    acc = 0.0
    for i, v in enumerate(x):
        acc = (1 - a) * v + a * acc
        y[i] = acc
    return y


def highpass(x, hz):
    return x - lowpass(x, hz)


def band(x, hz, q):
    """A biquad band-pass (RBJ's, constant peak)."""
    w = 2 * np.pi * hz / SR
    al = np.sin(w) / (2 * q)
    b0, b2 = al, -al
    a0, a1, a2 = 1 + al, -2 * np.cos(w), 1 - al
    y = np.zeros_like(x)
    x1 = x2 = y1 = y2 = 0.0
    for i, v in enumerate(x):
        o = (b0 * v + b2 * x2 - a1 * y1 - a2 * y2) / a0
        x2, x1, y2, y1 = x1, v, y1, o
        y[i] = o
    return y


def env(n, attack, decay):
    t = np.arange(n) / SR
    return np.minimum(t / max(attack, 1e-4), 1.0) * np.exp(-t / decay)


def at(dst, src, secs, gain=1.0):
    i = int(secs * SR)
    n = min(len(src), len(dst) - i)
    dst[i:i + n] += src[:n] * gain


def loop(x, fade):
    """Seamless: its tail faded over its head."""
    n = int(fade * SR)
    head, body, tail = x[:n], x[n:-n] if n else x, x[-n:]
    ramp = np.linspace(0, 1, n)
    return np.concatenate([tail * (1 - ramp) + head * ramp, body])


def write(name, x, peak_db):
    x = x - np.mean(x)
    x = x * (10 ** (peak_db / 20) / max(np.max(np.abs(x)), 1e-9))
    wav = OUT / f'{name}.wav'
    with wave.open(str(wav), 'wb') as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes((np.clip(x, -1, 1) * 32767).astype('<i2').tobytes())
    subprocess.run(['ffmpeg', '-v', 'error', '-y', '-i', str(wav), '-c:a', 'libvorbis', '-q:a', '5',
                    str(OUT / f'{name}.ogg')], check=True)
    wav.unlink()
    print(f'{name}.ogg {len(x) / SR:.2f} s')


def thud(secs, hz0, hz1, decay):
    n = int(secs * SR)
    t = np.arange(n) / SR
    f = hz1 + (hz0 - hz1) * np.exp(-t / 0.06)
    return np.sin(2 * np.pi * np.cumsum(f) / SR) * env(n, 0.002, decay)


def truck_land(k):
    """Down on its wheels: the tyres' and the springs' thump, the body's
    steel slammed on its stops, a rattle of what is loose in the bed."""
    n = int(1.1 * SR)
    x = np.zeros(n)
    at(x, thud(0.8, 90, 38, 0.22), 0.0, 1.0)
    at(x, lowpass(rng.standard_normal(int(0.3 * SR)), 260) * env(int(0.3 * SR), 0.002, 0.07), 0.0, 2.4)
    at(x, pitch(clip(f'impact_metal_{1 + k}'), 0.55), 0.012, 0.9)
    at(x, pitch(clip(f'impact_metal_{3 - k}'), 0.72), 0.05, 0.45)
    for j in range(5):
        tick = pitch(clip(f'impactMetal_light_00{j % 3}'), rng.uniform(0.8, 1.15))
        at(x, tick, 0.12 + j * rng.uniform(0.04, 0.09), 0.18 * (0.8 ** j))
    # The springs' creak back up.
    at(x, band(rng.standard_normal(int(0.4 * SR)), 520, 9.0) * env(int(0.4 * SR), 0.08, 0.12), 0.1, 0.5)
    write(f'truck_land_{k}', x, -1.0)


def crash():
    """Its nose into rock at speed: a crunch of sheet steel, a deep hit, grit."""
    n = int(1.2 * SR)
    x = np.zeros(n)
    at(x, thud(0.7, 120, 45, 0.16), 0.0, 1.0)
    at(x, pitch(clip('impact_metal_0'), 0.62), 0.0, 1.0)
    at(x, pitch(clip('impact_metal_4'), 0.8), 0.03, 0.7)
    crunch = band(rng.standard_normal(int(0.45 * SR)), 1800, 1.2) * env(int(0.45 * SR), 0.004, 0.09)
    crunch *= 0.6 + 0.4 * (rng.random(len(crunch)) < 0.08)
    at(x, crunch, 0.0, 1.2)
    at(x, highpass(rng.standard_normal(int(0.6 * SR)), 2500) * env(int(0.6 * SR), 0.02, 0.15), 0.05, 0.25)
    write('crash_0', x, -1.0)


def skid():
    """Tyres sliding on dirt and rock: a squeal that wavers over a hiss of
    grit. A loop."""
    secs = 2.6
    n = int(secs * SR)
    t = np.arange(n) / SR
    wobble = lowpass(rng.standard_normal(n), 3.0)
    wobble /= np.max(np.abs(wobble))
    squeal = np.zeros(n)
    for hz, g in ((760, 1.0), (1140, 0.55), (1520, 0.35), (2280, 0.15)):
        f = hz * (1 + 0.04 * wobble + 0.01 * np.sin(2 * np.pi * 7.3 * t))
        squeal += g * np.sin(2 * np.pi * np.cumsum(f) / SR)
    flutter = 0.55 + 0.45 * np.abs(lowpass(rng.standard_normal(n), 14.0)) / 0.15
    squeal *= np.clip(flutter, 0.2, 1.2)
    grit = band(rng.standard_normal(n), 2600, 0.7) * (0.7 + 0.3 * rng.random(n))
    rumble = lowpass(rng.standard_normal(n), 180) * 4
    write('skid_0', loop(squeal * 0.6 + grit * 0.3 + rumble * 0.25, 0.3), -4.0)


def fire():
    """A truck burning: the roar of the fire, its crackle and pops. A loop."""
    secs = 3.4
    n = int(secs * SR)
    roar = lowpass(lowpass(rng.standard_normal(n), 320), 320) * 9
    swell = 0.75 + 0.25 * lowpass(rng.standard_normal(n), 1.5) / 0.03
    roar *= np.clip(swell, 0.4, 1.3)
    hiss = band(rng.standard_normal(n), 3200, 0.8) * 0.18
    crackle = np.zeros(n)
    for _ in range(int(secs * 38)):
        m = int(rng.uniform(0.002, 0.02) * SR)
        pop = rng.standard_normal(m) * env(m, 0.0003, rng.uniform(0.002, 0.008))
        at(crackle, pop, rng.uniform(0, secs - 0.03), rng.uniform(0.2, 1.0) ** 2)
    crackle = highpass(crackle, 700)
    write('fire_0', loop(roar + hiss + crackle * 0.9, 0.4), -3.0)


MADE = {
    'truck_land_0': lambda: truck_land(0),
    'truck_land_1': lambda: truck_land(1),
    'crash_0': crash,
    'skid_0': skid,
    'fire_0': fire,
}

if __name__ == '__main__':
    for name in sys.argv[1:] or MADE:
        MADE[name]()

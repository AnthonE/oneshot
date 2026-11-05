#!/usr/bin/env python3
"""Our own sounds, synthesized: what Gates has no recording of (the shield's
alarm, recharge, hits and pop; the alien guns; the vent and the charge; the
engine and a rocket in flight; a grenade's bounce; the spawn; the
countdown). Each is mono 44.1 kHz Vorbis in crates/client/assets/sound,
written by ffmpeg. Seeded: the same files every run.

  python3 tools/sfx.py            # every sound
  python3 tools/sfx.py reel       # and traces/sfx_reel.ogg, all of them in a row

Needs numpy and ffmpeg (with libvorbis).
"""

import os
import subprocess
import sys
import tempfile
import wave

import numpy as np

SR = 44_100
OUT = os.path.join(os.path.dirname(__file__), '..', 'crates', 'client', 'assets', 'sound')
rng = np.random.default_rng(0x5EED)


def t_of(sec):
    return np.arange(int(sec * SR)) / SR


def noise(sec):
    return rng.standard_normal(int(sec * SR))


def band(x, lo, hi, soft=0.15):
    """Zero-phase band-pass by FFT, edges eased over `soft` of an octave-ish."""
    n = len(x)
    f = np.fft.rfftfreq(n, 1 / SR)
    X = np.fft.rfft(x)
    m = np.ones_like(f)
    if lo > 0:
        m *= 1 / (1 + (lo / np.maximum(f, 1e-3)) ** (2 / soft))
    if hi < SR / 2:
        m *= 1 / (1 + (np.maximum(f, 1e-3) / hi) ** (2 / soft))
    return np.fft.irfft(X * m, n)


def lp1(x, hz):
    a = 1 - np.exp(-2 * np.pi * hz / SR)
    y = np.empty_like(x)
    s = 0.0
    for i, v in enumerate(x):
        s += a * (v - s)
        y[i] = s
    return y


def sweep(t, f0, f1, curve='exp'):
    """A tone gliding f0 → f1 over t (phase-accumulated)."""
    u = t / max(t[-1], 1e-9)
    f = f0 * (f1 / f0) ** u if curve == 'exp' else f0 + (f1 - f0) * u
    return np.sin(2 * np.pi * np.cumsum(f) / SR)


def env(t, attack, decay):
    """Linear attack, exponential decay (to 1/e every `decay` s)."""
    a = np.clip(t / max(attack, 1e-6), 0, 1)
    return a * np.exp(-np.maximum(t - attack, 0) / decay)


def fade(x, tin=0.002, tout=0.01):
    n = len(x)
    i, o = int(tin * SR), int(tout * SR)
    x = x.copy()
    if i:
        x[:i] *= np.linspace(0, 1, i)
    if o:
        x[n - o:] *= np.linspace(1, 0, o)
    return x


def seam(x, sec):
    """A loop: the last `sec` crossfaded (equal power) into the head, then
    cut, so the wrap is the loop point."""
    k = int(sec * SR)
    body = x[:-k].copy()
    tail = x[-k:]
    u = np.linspace(0, np.pi / 2, k)
    body[:k] = body[:k] * np.sin(u) + tail * np.cos(u)
    return body


def echo(x, delay, gain, n=3):
    d = int(delay * SR)
    y = np.concatenate([x, np.zeros(d * n)])
    for k in range(1, n + 1):
        y[k * d:k * d + len(x)] += x * gain ** k
    return y


def level(x, peak_db=-1.0):
    p = np.max(np.abs(x))
    return x * (10 ** (peak_db / 20) / p) if p > 0 else x


def write(name, x, peak_db=-1.0, loop=False):
    x = level(x if loop else fade(x), peak_db)
    pcm = (np.clip(x, -1, 1) * 32767).astype('<i2')
    with tempfile.NamedTemporaryFile(suffix='.wav', delete=False) as f:
        path = f.name
    with wave.open(path, 'wb') as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())
    dst = os.path.join(OUT, f'{name}.ogg')
    subprocess.run(['ffmpeg', '-loglevel', 'error', '-y', '-i', path, '-c:a', 'libvorbis', '-q:a', '4', dst],
                   check=True)
    os.unlink(path)
    MADE.append((name, x))


MADE = []


def far(x):
    """A shot from a long way off: its highs gone, the land's echo."""
    return echo(band(x, 60, 1400), 0.17, 0.4, 3)


# ── The shield ──────────────────────────────────────────────────────────

def shield_beep():
    # Halo 2's alarm: a hard little square-ish bip, repeated by the game.
    t = t_of(0.075)
    f = 1975.0
    x = sum(np.sin(2 * np.pi * f * k * t) / k for k in (1, 3, 5, 7))
    return x * np.clip(t / 0.003, 0, 1) * np.clip((t[-1] - t) / 0.012, 0, 1)


def shield_charge():
    t = t_of(1.6)
    rise = sweep(t, 180, 900) + 0.5 * sweep(t, 270, 1350) + 0.25 * sweep(t, 360, 1800)
    trem = 0.75 + 0.25 * np.sin(2 * np.pi * 18 * t)
    fizz = band(noise(1.6), 2500, 7000) * 0.25
    e = np.clip(t / 0.12, 0, 1) * np.clip((1.6 - t) / 0.5, 0, 1)
    return (rise * trem + fizz * (0.4 + t / 1.6)) * e


def shield_hit(k):
    t = t_of(0.2)
    crackle = band(noise(0.2) * (rng.random(len(t)) < 0.08), 1800 + 300 * k, 6500) * 6
    hiss = band(noise(0.2), 2200, 8000) * 0.6
    zap = sweep(t, 2400 - 200 * k, 700) * 0.7
    return (crackle + hiss + zap) * env(t, 0.002, 0.05)


def shield_pop():
    t = t_of(0.6)
    burst = band(noise(0.6), 800, 9000) * env(t, 0.001, 0.03)
    drop = sweep(t, 1400, 110) * env(t, 0.003, 0.12)
    thump = np.sin(2 * np.pi * 62 * t) * env(t, 0.002, 0.09) * 1.2
    fizz = band(noise(0.6) * (rng.random(len(t)) < 0.05), 3000, 8000) * env(t, 0.05, 0.15) * 4
    return burst + drop + thump + fizz


# ── The alien guns (ion, shards, carbine, beam, cannon, launcher, sword) ──

def ion_rifle(k):
    t = t_of(0.24)
    wob = 1 + 0.08 * np.sin(2 * np.pi * 70 * t)
    tone = sweep(t, (1500 + 90 * k) * wob, 280)
    return (tone + band(noise(0.24), 600, 5000) * 0.45 * env(t, 0.001, 0.02)) * env(t, 0.002, 0.06)


def ion_pistol(k):
    t = t_of(0.28)
    tone = sweep(t, 2100 + 120 * k, 420) + 0.4 * sweep(t, 1050 + 60 * k, 210)
    return (tone + band(noise(0.28), 900, 6000) * 0.35 * env(t, 0.001, 0.02)) * env(t, 0.002, 0.07)


def ion_charged():
    t = t_of(0.9)
    whoomp = sweep(t, 700, 70) * env(t, 0.004, 0.25)
    sub = np.sin(2 * np.pi * 48 * t) * env(t, 0.01, 0.2) * 0.9
    rush = band(noise(0.9), 200, 3000) * env(t, 0.005, 0.18) * 0.6
    return whoomp + sub + rush


def charge_hold():
    # A loop the game pitches up as the charge fills.
    t = t_of(1.05)
    x = sum(np.sin(2 * np.pi * 220 * k * t) * a for k, a in ((1, 1), (2, 0.5), (3, 0.3), (5, 0.15)))
    x *= 0.8 + 0.2 * np.sin(2 * np.pi * 8 * t)
    x += band(noise(1.05), 1500, 6000) * 0.12
    return seam(x, 0.05)


def shards(k):
    t = t_of(0.22)
    glass = sum(np.sin(2 * np.pi * f * (1 + 0.03 * k) * t) * np.exp(-t / d)
                for f, d in ((2850, 0.05), (4130, 0.035), (5370, 0.025), (7020, 0.018)))
    thwip = band(noise(0.22), 1500, 9000) * env(t, 0.001, 0.015) * 0.8
    return glass + thwip


def carbine(k):
    t = t_of(0.34)
    crack = band(noise(0.34), 1200, 9000) * env(t, 0.0005, 0.012) * 1.4
    zap = sweep(t, 1100 + 80 * k, 360) * env(t, 0.002, 0.05)
    body = np.sin(2 * np.pi * 130 * t) * env(t, 0.002, 0.04) * 0.6
    return crack + zap + body


def beam(k):
    t = t_of(1.1)
    crack = band(noise(1.1), 1500, 12000) * env(t, 0.0003, 0.01) * 1.6
    whine = (sweep(t, 4200 - 150 * k, 2600) + 0.5 * sweep(t, 6100, 3900)) * env(t, 0.002, 0.25) * 0.5
    sizzle = band(noise(1.1) * (rng.random(len(t)) < 0.04), 3000, 8000) * env(t, 0.03, 0.3) * 3
    boom = np.sin(2 * np.pi * 75 * t) * env(t, 0.002, 0.07) * 0.8
    return crack + whine + sizzle + boom


def cannon(k):
    t = t_of(0.8)
    thoomp = sweep(t, 160 - 10 * k, 40) * env(t, 0.004, 0.18) * 1.2
    bubble = band(noise(0.8), 150, 1800) * (0.6 + 0.4 * np.sin(2 * np.pi * 28 * t)) * env(t, 0.01, 0.2)
    glug = sweep(t, 900, 250) * env(t, 0.002, 0.06) * 0.5
    return thoomp + bubble + glug


def launcher(k):
    t = t_of(0.45)
    thunk = np.sin(2 * np.pi * (95 + 5 * k) * t) * env(t, 0.002, 0.06) * 1.3
    clank = sum(np.sin(2 * np.pi * f * t) * np.exp(-t / 0.04) for f in (620, 1530, 2410)) * 0.35
    puff = band(noise(0.45), 300, 4000) * env(t, 0.002, 0.04) * 0.7
    return thunk + clank + puff


def sword_swing(k):
    t = t_of(0.5)
    u = t / 0.5
    whoosh = band(noise(0.5), 300, 2500) * np.sin(np.pi * u) ** 2 * (1.0 + 0.15 * k)
    hum = sum(np.sin(2 * np.pi * 110 * (1 + 0.3 * np.sin(np.pi * u)) * h * t) / h for h in (1, 2, 3)) * 0.5
    return (whoosh + hum * np.sin(np.pi * u)) * env(t, 0.02, 0.3)


def sword_on():
    t = t_of(0.6)
    rise = sweep(t, 90, 330) + 0.5 * sweep(t, 180, 660)
    snap = band(noise(0.6), 2000, 9000) * env(t, 0.001, 0.03)
    return rise * env(t, 0.05, 0.25) + snap


def vent():
    t = t_of(1.1)
    hiss = band(noise(1.1), 2500, 10000) * env(t, 0.01, 0.35)
    whistle = sweep(t, 3200, 2400) * env(t, 0.02, 0.25) * 0.25
    clank = sum(np.sin(2 * np.pi * f * t) * np.exp(-t / 0.03) for f in (840, 1910)) * 0.6
    return hiss + whistle + clank


# ── Things moving: the engine, a rocket ─────────────────────────────────

def engine():
    # 4 s at 45 firings a second: whole cycles, so the wrap is seamless
    # before the seam's crossfade even starts. The game pitches it.
    sec = 4.0
    n = int(sec * SR)
    t = np.arange(n) / SR
    fire = 45.0
    pulses = np.zeros(n)
    beat = (np.arange(int(sec * fire)) * SR / fire).astype(int)
    pulses[beat] = 0.75 + 0.5 * rng.random(len(beat))
    # The block's ring: a decaying resonance each firing excites.
    k = np.arange(int(0.02 * SR)) / SR
    ring = np.sin(2 * np.pi * 140 * k) * np.exp(-k / 0.006) + 0.6 * np.sin(2 * np.pi * 330 * k) * np.exp(-k / 0.003)
    x = np.convolve(pulses, ring)[:n]
    x += np.sin(2 * np.pi * fire * t) * 0.35 + np.sin(2 * np.pi * fire * 0.5 * t) * 0.25
    x += band(noise(sec), 80, 900) * 0.18
    return seam(band(x, 25, 2500), 0.08)


def rocket_fly():
    sec = 1.5
    t = t_of(sec)
    roar = band(noise(sec), 300, 2600) * (0.85 + 0.15 * np.sin(2 * np.pi * 14 * t))
    hiss = band(noise(sec), 3000, 8000) * 0.35
    return seam(roar + hiss, 0.1)


def wind():
    # A soft outdoor bed for the big maps.
    sec = 12.0
    t = t_of(sec)
    x = band(noise(sec), 70, 700) * (0.7 + 0.3 * np.sin(2 * np.pi * t / sec * 3))
    x += band(noise(sec), 900, 2500) * 0.15 * (0.5 + 0.5 * np.sin(2 * np.pi * t / sec * 5 + 1))
    return seam(x, 1.0)


# ── Small things ────────────────────────────────────────────────────────

def tink(k):
    t = t_of(0.18)
    fs = np.array((2330, 3710, 5870)) * (1 + 0.06 * k)
    x = sum(np.sin(2 * np.pi * f * t) * np.exp(-t / d) for f, d in zip(fs, (0.05, 0.03, 0.02)))
    return x + band(noise(0.18), 2000, 9000) * env(t, 0.0005, 0.004)


def spawn():
    t = t_of(1.0)
    x = np.zeros_like(t)
    for i, f in enumerate((523.25, 659.25, 783.99, 1046.5)):
        d = t - i * 0.07
        x += np.where(d > 0, np.sin(2 * np.pi * f * d) * np.exp(-np.maximum(d, 0) / 0.25), 0)
    x += band(noise(1.0), 3000, 9000) * env(t, 0.1, 0.3) * 0.3
    return x * 0.7


def tick():
    t = t_of(0.09)
    return np.sin(2 * np.pi * 880 * t) * env(t, 0.002, 0.03)


def go():
    t = t_of(0.5)
    return (np.sin(2 * np.pi * 1320 * t) + 0.4 * np.sin(2 * np.pi * 1980 * t)) * env(t, 0.003, 0.18)


def main():
    write('shield_beep_0', shield_beep(), -4)
    write('shield_charge_0', shield_charge(), -3)
    for k in range(3):
        write(f'shield_hit_{k}', shield_hit(k), -3)
    write('shield_pop_0', shield_pop(), -1)
    guns = {
        'shot_ion': ion_rifle, 'shot_ionp': ion_pistol, 'shot_shards': shards, 'shot_carbine': carbine,
        'shot_beam': beam, 'shot_cannon': cannon, 'shot_launcher': launcher,
    }
    for name, f in guns.items():
        takes = [f(k) for k in range(3)]
        for k, x in enumerate(takes):
            write(f'{name}_{k}', x, -1)
        write(f'{name}_far_0', far(takes[0]), -6)
    write('shot_ion_charged_0', ion_charged(), -1)
    write('charge_hold_0', charge_hold(), -6, loop=True)
    for k in range(2):
        write(f'sword_swing_{k}', sword_swing(k), -2)
    write('sword_on_0', sword_on(), -3)
    write('vent_0', vent(), -3)
    write('engine_0', engine(), -2, loop=True)
    write('rocket_fly_0', rocket_fly(), -3, loop=True)
    write('wind_0', wind(), -3, loop=True)
    for k in range(3):
        write(f'tink_{k}', tink(k), -3)
    write('spawn_0', spawn(), -4)
    write('tick_0', tick(), -6)
    write('go_0', go(), -4)
    print(f'{len(MADE)} sounds into {os.path.normpath(OUT)}')
    if 'reel' in sys.argv[1:]:
        gap = np.zeros(int(0.35 * SR))
        reel = np.concatenate([np.concatenate([level(x, -3), gap]) for _, x in MADE])
        os.makedirs('traces', exist_ok=True)
        write_reel = os.path.join('traces', 'sfx_reel.ogg')
        pcm = (np.clip(reel, -1, 1) * 32767).astype('<i2')
        with tempfile.NamedTemporaryFile(suffix='.wav', delete=False) as f:
            path = f.name
        with wave.open(path, 'wb') as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(SR)
            w.writeframes(pcm.tobytes())
        subprocess.run(['ffmpeg', '-loglevel', 'error', '-y', '-i', path, '-c:a', 'libvorbis', '-q:a', '5', write_reel],
                       check=True)
        os.unlink(path)
        print('reel:', write_reel, ', '.join(n for n, _ in MADE))


if __name__ == '__main__':
    main()

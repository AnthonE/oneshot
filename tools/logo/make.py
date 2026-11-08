#!/usr/bin/env python3
"""Draft marks for oneshot and its coin: SVG, the display face's glyphs as paths.

    pip install fonttools
    python3 tools/logo/make.py [--hero <a 1500x500-ish screenshot>]

writes tools/logo/drafts/*.svg and their PNGs (Chromium draws them, through
render.mjs). The game's marks (game-*, icon-*) and the coin's (coin-*) are
drafts for the operator to pick from; the coin's picture and banner are welded
at its create on pump.fun (Elo Pros' docs/launch/ONESHOT-COIN.md).

Unitblock is the title's face (crates/client/assets/fonts, CC0); the colours
are the title's own (crates/client/src/ui.rs); the corner badge is Elo Pros'
mark, as on JUNK's coin.
"""
import argparse
import shutil
import subprocess
import sys
from pathlib import Path
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.boundsPen import BoundsPen

HERE = Path(__file__).resolve().parent
FONTS = HERE.parent.parent / "crates" / "client" / "assets" / "fonts"
OUT = HERE / "drafts"
FONT = TTFont(FONTS / "Unitblock.ttf")
GS = FONT.getGlyphSet()
CMAP = FONT.getBestCmap()
UPM = FONT["head"].unitsPerEm

# the game's own colours (oneshot crates/client/src/ui.rs)
INK = "#1b1916"      # ui::BG
INK_DEEP = "#0e0c0a"
GOLD = "#e8b931"     # ui::MARK
GOLD_HI = "#f5cc53"  # ui::GO_HOVER
BLUE = "#3982ba"     # ui::ACCENT
BONE = "#eeebe5"     # ui::TITLE
LIME = "#b8ff38"     # Elo Pros' mark
ELO_INK = "#0a0c0a"


def text(s, size, x, y, track=0.0, anchor="start"):
    """Path data for `s` in Unitblock at `size` px, baseline at y. Returns (d, width)."""
    k = size / UPM
    adv = []
    for ch in s:
        g = CMAP[ord(ch)]
        adv.append((g, GS[g].width * k))
    width = sum(a for _, a in adv) + track * size * (len(s) - 1)
    if anchor == "middle":
        x -= width / 2
    pen = SVGPathPen(GS)
    cx = x
    for g, a in adv:
        GS[g].draw(TransformPen(pen, (k, 0, 0, -k, cx, y)))
        cx += a + track * size
    return pen.getCommands(), width


def ink_bounds(s, size):
    """Visual (ink) box of `s` at origin: (xmin, ymin_up, xmax, ymax_up) in px."""
    k = size / UPM
    bp = BoundsPen(GS)
    cx = 0
    for ch in s:
        g = CMAP[ord(ch)]
        GS[g].draw(TransformPen(bp, (k, 0, 0, k, cx, 0)))
        cx += GS[g].width * k
    return bp.bounds


def elo_mark(x, y, s, outline=None):
    """The Elo Pros tile at (x, y), side s (watchtower/art/make_logo.py's geometry)."""
    k = s / 64
    o = ""
    if outline:
        w = outline
        o = f'<rect x="{x - w + k}" y="{y - w + k}" width="{62 * k + 2 * w}" height="{62 * k + 2 * w}" rx="{14 * k + w}" fill="{ELO_INK}"/>'
    d = "M14 26 L32 11 L50 26 L50 35 L32 20 L14 35 Z M14 44 L32 29 L50 44 L50 53 L32 38 L14 53 Z"
    return (o + f'<g transform="translate({x} {y}) scale({k})">'
            f'<rect x="1" y="1" width="62" height="62" rx="14" fill="{LIME}"/>'
            f'<path fill="{ELO_INK}" d="{d}"/></g>')


def svg(w, h, body, defs=""):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">'
            f'<defs>{defs}</defs>{body}</svg>\n')


def reticle(cx, cy, r, sw, color, tick=0.42, gap=0.0, dot=None):
    """A ring with four ticks pointing in from N/E/S/W, and an optional centre dot."""
    t = r * tick
    parts = [f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{color}" stroke-width="{sw}"/>']
    for dx, dy in ((0, -1), (1, 0), (0, 1), (-1, 0)):
        x1, y1 = cx + dx * (r + sw * 0.5 + gap), cy + dy * (r + sw * 0.5 + gap)
        x2, y2 = cx + dx * (r - t), cy + dy * (r - t)
        parts.append(f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{color}" stroke-width="{sw}" stroke-linecap="butt"/>')
    if dot:
        parts.append(f'<circle cx="{cx}" cy="{cy}" r="{dot}" fill="{color}"/>')
    return "".join(parts)


def coin_ground(face0, face1, rim, rim_w=26, rim_gap=20, edge=INK_DEEP, gid="f"):
    W = 1024
    defs = (f'<radialGradient id="{gid}" cx="50%" cy="40%" r="62%">'
            f'<stop offset="0" stop-color="{face0}"/><stop offset="1" stop-color="{face1}"/></radialGradient>')
    body = (f'<rect width="{W}" height="{W}" fill="{edge}"/>'
            f'<circle cx="512" cy="512" r="{500}" fill="{rim}"/>'
            f'<circle cx="512" cy="512" r="{500 - rim_w}" fill="{edge}"/>'
            f'<circle cx="512" cy="512" r="{500 - rim_w - rim_gap}" fill="url(#{gid})"/>')
    return defs, body


# ---------- the coin ----------

def coin_reticle():
    defs, body = coin_ground("#3a3222", "#120f0b", GOLD)
    d, _ = text("1", 440, 512, 512 + 440 * 0.36, anchor="middle")
    body += reticle(512, 512, 300, 40, GOLD, tick=0.0)
    for dx, dy in ((0, -1), (1, 0), (0, 1), (-1, 0)):
        body += (f'<line x1="{512 + dx * 400}" y1="{512 + dy * 400}" x2="{512 + dx * 268}" y2="{512 + dy * 268}" '
                 f'stroke="{GOLD}" stroke-width="40"/>')
    body += f'<path d="{d}" fill="{GOLD}"/>'
    body += elo_mark(700, 150, 190, outline=16)
    return svg(1024, 1024, body, defs)


def cartridge(cx, base, h, w):
    """A rifle round standing on its base at (cx, base): brass case, copper bullet."""
    case_h = h * 0.64
    neck_h = h * 0.07
    rim = base - h * 0.035
    sh_top = base - case_h
    neck_top = sh_top - neck_h
    nw = w * 0.62
    bullet_h = h - case_h - neck_h
    tip = neck_top - bullet_h
    x0, x1 = cx - w / 2, cx + w / 2
    n0, n1 = cx - nw / 2, cx + nw / 2
    groove = rim - h * 0.03
    case = (f"M{x0 + w * 0.02} {base} L{x1 - w * 0.02} {base} L{x1 - w * 0.02} {rim} "
            f"L{x1 - w * 0.08} {groove} L{x1} {groove - h * 0.012} "
            f"L{x1} {sh_top + w * 0.25} L{n1} {sh_top - h * 0.005} L{n1} {neck_top} "
            f"L{n0} {neck_top} L{n0} {sh_top - h * 0.005} L{x0} {sh_top + w * 0.25} "
            f"L{x0} {groove - h * 0.012} L{x0 + w * 0.08} {groove} L{x0 + w * 0.02} {rim} Z")
    bullet = (f"M{n0} {neck_top + 2} L{n0} {neck_top - bullet_h * 0.30} "
              f"C{n0} {tip + bullet_h * 0.30} {cx - nw * 0.12} {tip + bullet_h * 0.05} {cx} {tip} "
              f"C{cx + nw * 0.12} {tip + bullet_h * 0.05} {n1} {tip + bullet_h * 0.30} {n1} {neck_top - bullet_h * 0.30} "
              f"L{n1} {neck_top + 2} Z")
    defs = (
        '<linearGradient id="brass" x1="0" x2="1" y1="0" y2="0">'
        '<stop offset="0" stop-color="#7a5414"/><stop offset=".18" stop-color="#c99a2e"/>'
        '<stop offset=".34" stop-color="#ffe9a3"/><stop offset=".5" stop-color="#e8b931"/>'
        '<stop offset=".82" stop-color="#9c6e17"/><stop offset="1" stop-color="#5e3f0c"/></linearGradient>'
        '<linearGradient id="copper" x1="0" x2="1" y1="0" y2="0">'
        '<stop offset="0" stop-color="#6b2d14"/><stop offset=".2" stop-color="#b8592c"/>'
        '<stop offset=".36" stop-color="#ffc19a"/><stop offset=".55" stop-color="#d0703c"/>'
        '<stop offset="1" stop-color="#4f200d"/></linearGradient>')
    body = (f'<ellipse cx="{cx}" cy="{base + 4}" rx="{w * 0.85}" ry="{w * 0.12}" fill="#000" opacity=".45"/>'
            f'<path d="{bullet}" fill="url(#copper)"/><path d="{case}" fill="url(#brass)"/>'
            f'<rect x="{x0 + w * 0.02}" y="{rim}" width="{w * 0.96}" height="{base - rim}" fill="#000" opacity=".18"/>')
    return defs, body


def coin_round():
    defs, body = coin_ground("#30597a", "#0b1520", GOLD)
    d2, b2 = cartridge(512, 880, 740, 190)
    defs += d2
    body += f'<g transform="rotate(-10 512 520)">{b2}</g>'
    body += elo_mark(700, 150, 190, outline=16)
    return svg(1024, 1024, body, defs)


def coin_impact():
    """One hole: a gold plate, shot once, dead centre."""
    import math, random
    rnd = random.Random(7)
    defs = ('<radialGradient id="plate" cx="40%" cy="32%" r="75%">'
            '<stop offset="0" stop-color="#ffe08a"/><stop offset=".45" stop-color="#e8b931"/>'
            '<stop offset="1" stop-color="#8a6312"/></radialGradient>'
            '<radialGradient id="hole" cx="50%" cy="50%" r="50%">'
            '<stop offset="0" stop-color="#000"/><stop offset=".62" stop-color="#0b0906"/>'
            '<stop offset=".78" stop-color="#4a3510"/><stop offset=".9" stop-color="#b98a22"/>'
            '<stop offset="1" stop-color="#fff0b8" stop-opacity="0"/></radialGradient>')
    body = (f'<rect width="1024" height="1024" fill="{INK_DEEP}"/>'
            f'<circle cx="512" cy="512" r="500" fill="{INK}"/>'
            f'<circle cx="512" cy="512" r="470" fill="url(#plate)"/>'
            f'<circle cx="512" cy="512" r="470" fill="none" stroke="#fff3c4" stroke-opacity=".35" stroke-width="6"/>'
            f'<circle cx="512" cy="512" r="430" fill="none" stroke="{INK}" stroke-opacity=".25" stroke-width="4"/>')
    cracks = []
    n = 9
    for i in range(n):
        a = 2 * math.pi * i / n + rnd.uniform(-.25, .25)
        ln = rnd.uniform(150, 330)
        pts = [(512 + 92 * math.cos(a), 512 + 92 * math.sin(a))]
        r, aa = 92, a
        while r < 92 + ln:
            r += rnd.uniform(40, 80)
            aa += rnd.uniform(-.18, .18)
            pts.append((512 + r * math.cos(aa), 512 + r * math.sin(aa)))
        w = 16
        pl = " ".join(f"{x:.1f},{y:.1f}" for x, y in pts)
        cracks.append(f'<polyline points="{pl}" fill="none" stroke="#3a2a08" stroke-width="{w}" stroke-linecap="round" stroke-linejoin="round"/>')
        cracks.append(f'<polyline points="{pl}" fill="none" stroke="#fff3c4" stroke-opacity=".5" stroke-width="4" transform="translate(5 5)" stroke-linecap="round" stroke-linejoin="round"/>')
    body += "".join(cracks)
    body += f'<circle cx="512" cy="512" r="150" fill="url(#hole)"/>'
    body += elo_mark(700, 150, 190, outline=16)
    return svg(1024, 1024, body, defs)


# ---------- the game ----------

def word_reticle(color=GOLD, ring=None, w_total=None, size=200, bg=None):
    """ONESHOT, the first O a reticle. Returns (svg body, width, height)."""
    ring = ring or color
    pad = size * 0.25
    b = ink_bounds("O", size)
    o_w = b[2] - b[0]
    cap = b[3] - b[1]
    gap = size * 0.07
    r = cap * 0.5
    d, wrest = text("NESHOT", size, pad + 2 * r + gap + size * 0.04, pad + cap, track=0.04)
    sw = size * 0.095
    cx, cy = pad + r, pad + cap / 2
    body = reticle(cx, cy, r - sw / 2, sw, ring, tick=0.0, dot=sw * 0.55)
    # ticks outside the ring, N and S only, to keep the word's height honest
    for dy in (-1, 1):
        y1 = cy + dy * (r - sw * 0.2)
        y2 = cy + dy * (r - sw * 1.55)
        body += f'<line x1="{cx}" y1="{y1}" x2="{cx}" y2="{y2}" stroke="{ring}" stroke-width="{sw * 0.7}"/>'
    for dx in (-1, 1):
        x1 = cx + dx * (r - sw * 0.2)
        x2 = cx + dx * (r - sw * 1.55)
        body += f'<line x1="{x1}" y1="{cy}" x2="{x2}" y2="{cy}" stroke="{ring}" stroke-width="{sw * 0.8}"/>'
    body += f'<path d="{d}" fill="{color}"/>'
    W = pad + 2 * r + gap + size * 0.04 + wrest + pad
    H = cap + 2 * pad
    if bg:
        body = f'<rect width="{W}" height="{H}" fill="{bg}"/>' + body
    return body, W, H


def game_reticle():
    body, W, H = word_reticle(GOLD, bg=INK)
    return svg(round(W), round(H), body)


def game_tracer():
    size = 200
    pad = size * 0.25
    b = ink_bounds("ONESHOT", size)
    d, w = text("ONESHOT", size, pad, pad + (b[3] - b[1]), track=0.04)
    cap = b[3] - b[1]
    W, H = w + 2 * pad, cap + 2 * pad
    # the tracer: a thin slash through the word, rising left to right
    x0, y0, x1, y1 = pad * 0.1, pad + cap * 0.70, W - pad * 0.1, pad + cap * 0.36
    defs = (f'<mask id="cut"><rect width="{W}" height="{H}" fill="#fff"/>'
            f'<line x1="{x0}" y1="{y0}" x2="{x1}" y2="{y1}" stroke="#000" stroke-width="{size * 0.05}"/></mask>'
            f'<linearGradient id="tr" x1="0" x2="1"><stop offset="0" stop-color="{GOLD}" stop-opacity="0"/>'
            f'<stop offset=".55" stop-color="{GOLD}"/><stop offset="1" stop-color="#fff6d6"/></linearGradient>')
    body = (f'<rect width="{W}" height="{H}" fill="{INK}"/>'
            f'<path d="{d}" fill="{BONE}" mask="url(#cut)"/>'
            f'<line x1="{x0}" y1="{y0}" x2="{x1}" y2="{y1}" stroke="url(#tr)" stroke-width="{size * 0.022}" stroke-linecap="round"/>'
            f'<circle cx="{x1}" cy="{y1}" r="{size * 0.035}" fill="#fff6d6"/>')
    return svg(round(W), round(H), body, defs)


def game_split():
    """ONE in gold, SHOT in bone, a chamfered '1' badge before it."""
    size = 200
    pad = size * 0.25
    b = ink_bounds("O", size)
    cap = b[3] - b[1]
    bw = cap * 1.0
    c = bw * 0.22
    x, y = pad, pad
    badge = (f'<path d="M{x + c} {y} L{x + bw - c} {y} L{x + bw} {y + c} L{x + bw} {y + cap - c} '
             f'L{x + bw - c} {y + cap} L{x + c} {y + cap} L{x} {y + cap - c} L{x} {y + c} Z" fill="{GOLD}"/>')
    one, _ = text("1", cap * 1.05, x + bw / 2, y + cap / 2 + cap * 1.05 * 0.36 * 0.98, anchor="middle")
    badge += f'<path d="{one}" fill="{INK}"/>'
    tx = x + bw + size * 0.16
    d1, w1 = text("ONE", size, tx, pad + cap, track=0.04)
    d2, w2 = text("SHOT", size, tx + w1 + size * 0.04, pad + cap, track=0.04)
    W = tx + w1 + size * 0.04 + w2 + pad
    H = cap + 2 * pad
    body = (f'<rect width="{W}" height="{H}" fill="{INK}"/>' + badge +
            f'<path d="{d1}" fill="{GOLD}"/><path d="{d2}" fill="{BONE}"/>')
    return svg(round(W), round(H), body)


def icon_reticle():
    body = f'<rect width="512" height="512" rx="96" fill="{INK}"/>'
    body += reticle(256, 256, 160, 34, GOLD, tick=0.0, dot=22)
    for dx, dy in ((0, -1), (1, 0), (0, 1), (-1, 0)):
        body += (f'<line x1="{256 + dx * 150}" y1="{256 + dy * 150}" x2="{256 + dx * 92}" y2="{256 + dy * 92}" '
                 f'stroke="{GOLD}" stroke-width="28"/>')
    return svg(512, 512, body)


def icon_one():
    body = f'<rect width="512" height="512" rx="96" fill="{INK}"/>'
    d, _ = text("1", 400, 256, 256 + 400 * 0.36, anchor="middle")
    body += reticle(256, 256, 190, 26, BLUE, tick=0.0)
    body += f'<path d="{d}" fill="{GOLD}"/>'
    return svg(512, 512, body)


ALL = {
    "coin-a-reticle": coin_reticle,
    "coin-b-round": coin_round,
    "coin-c-impact": coin_impact,
    "game-a-reticle": game_reticle,
    "game-b-tracer": game_tracer,
    "game-c-split": game_split,
    "icon-a-reticle": icon_reticle,
    "icon-b-one": icon_one,
}

def banner_html(hero, pos="center 60%/cover"):
    """pump.fun's 1500x500 banner: the reticle wordmark over a match."""
    body, W, H = word_reticle(GOLD)
    word = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" '
            f'width="{W * 0.62:.0f}" height="{H * 0.62:.0f}">{body}</svg>')
    sub, sw = text("FREE ARENA SHOOTER  \u00b7  1V1 AND 2V2 RANKED  \u00b7  IN YOUR BROWSER", 34, 0, 34, track=0.06)
    line = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -4 {sw} 44" width="{sw}" height="44">'
            f'<path d="{sub}" fill="{BONE}"/></svg>')
    mark = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="44" height="44">{elo_mark(0, 0, 64)}</svg>'
    shot = f'<div style="position:absolute;inset:0;background:url(hero.jpg) {pos} no-repeat #000"></div>' if hero else ""
    return f'''<!doctype html><html><head><meta charset="utf-8"><style>
@font-face{{font-family:A;src:url(../../../crates/client/assets/fonts/Aileron-Bold.otf)}}
body{{margin:0;width:1500px;height:500px;overflow:hidden;background:{INK}}}</style></head><body>{shot}
<div style="position:absolute;inset:0;background:linear-gradient(90deg,rgba(27,25,22,.96) 0%,rgba(27,25,22,.85) 38%,rgba(27,25,22,.15) 75%,rgba(27,25,22,0) 100%)"></div>
<div style="position:absolute;left:70px;top:130px">{word}<div style="margin:22px 0 0 34px">{line}</div></div>
<div style="position:absolute;right:40px;bottom:34px;display:flex;align-items:center;gap:12px;color:#fff;font:30px A">{mark}<span>Elo Pros</span></div>
</body></html>'''


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--hero", help="a screenshot of a match for the banner's ground")
    ap.add_argument("--hero-pos", default="center 60%/cover",
                    help="CSS background position/size for it, e.g. 'right 50%/auto 125%' for the trailer's scope plate")
    a = ap.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    for name, f in ALL.items():
        (OUT / f"{name}.svg").write_text(f())
    if a.hero:
        shutil.copy(a.hero, OUT / "hero.jpg")
    (OUT / "banner.html").write_text(banner_html(a.hero, a.hero_pos))
    subprocess.run(["node", str(HERE / "render.mjs"), str(OUT)], check=True)
    (OUT / "banner.html").unlink()
    (OUT / "hero.jpg").unlink(missing_ok=True)
    print("\n".join(sorted(p.name for p in OUT.glob("*.png"))))

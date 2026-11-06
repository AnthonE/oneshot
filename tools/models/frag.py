"""The frag: a canister grenade. A dark cylinder with a polymer band, a
bottom cap, a top cap narrowing to the fuze head with its lever and ring,
an amber light ring at the shoulder. 0.16 m. Not held: the grip at its
middle.
From Meshy (tools/models/meshy_gen.py frag)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single futuristic military fragmentation grenade, a canister. A short cylinder of dark gunmetal, '
         '9 centimetres across, with a black polymer band around its middle, a flat bottom cap, and a top '
         'cap narrowing to a fuze head with a safety lever and a pull ring; a ring of small glowing amber '
         'indicator lights around its shoulder. 16 centimetres long. Scuffed, the paint chipped to bare '
         'steel on the edges.')
TEXTURE = ('Dark gunmetal chipped to bare steel on the edges, a matte black polymer band, a steel fuze head '
           'with a lever and a pull ring, small glowing amber lights, scuffs')
ASPECT = '4:3'
FRONT = 'the fuze'

scene()
gun, rise = generated('frag', 0.16, glow=['#ff8000'], tris=4000, tex_px=SMALL_TEX_PX, map_px=SMALL_MAP_PX)
finish('frag', [gun], grip=0.08, rise=rise, long=False, note='a canister, not held')

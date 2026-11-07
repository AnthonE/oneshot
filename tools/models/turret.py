"""The truck's minigun (look/turret.rs): a six-barrel rotary gun in a
cradle, cut in two so its barrels spin: `turret.glb` (the cradle, which
pitches) and `turret-barrels.glb` (the cluster, about its own axis).
From Meshy (tools/models/meshy_gen.py turret).

Turret space as the client wants it: +X along the bore, +Y up, +Z the
gun's right; the pivot (the trunnion the cradle pitches on) at the origin,
metres. The barrels' piece is about its own spin axis, its rear at x = 0,
and `TURRET_BARRELS_AT` (printed) is where that sits in the cradle's frame.
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

IMAGE = ('A single heavy six-barrel rotary machine gun, a vehicle-mounted weapon, without its mount. A long '
         'cluster of six thick steel barrels held together by two round clamps, ending in a ribbed flash '
         'cowl at the muzzle; behind them a chunky boxy receiver with a drive motor housing on its top, a '
         'short angled armour cowl over the top of the receiver, a large ammunition box hung on its side '
         'feeding a curved belt chute into the receiver, and two upright spade handles with a thumb trigger '
         'at the very rear. Olive-drab painted armour with chipped edges and stencilled panel lines, '
         'gunmetal receiver, dark blued steel barrels turning straw and blue from heat near the muzzle, '
         'brass rounds in the belt. 140 centimetres long, 40 centimetres tall and 30 centimetres wide. '
         'Battle-used: scuffs, oil streaks, carbon soot around the muzzle.')
TEXTURE = ('Olive-drab painted armour with chipped edges showing bare steel, gunmetal receiver, dark blued steel '
           'barrels heat-tinted straw and blue near the muzzle, brass ammunition belt, oil streaks, carbon soot')
ASPECT = '16:9'

# The cradle's numbers, metres: its length from the grips' rear to the
# muzzle (the sim's TURRET_REACH from the pivot, and the grips behind it).
LENGTH = 1.40
# The pivot, this far ahead of the grips' rear.
PIVOT = 0.35
# Where the barrels are cut off the receiver, a share of the length from
# the rear (read off the sheet), and the hold (the bore's height share).
CUT = 0.50

scene()
gun, rise = generated('turret', LENGTH, hold=None, rough=0.55, metal=0.6, tris=24000)
lo, hi = bounds(gun)

# The barrels: everything ahead of the cut, about the middle of their own
# cross-section (the spin axis), a piece of their own.
cut_x = lo.x + CUT * (hi.x - lo.x)
co = [v.co for v in gun.data.vertices if v.co.x > cut_x + 0.05]
ay = (min(c.y for c in co) + max(c.y for c in co)) / 2
az = (min(c.z for c in co) + max(c.z for c in co)) / 2

bpy.ops.object.select_all(action='DESELECT')
gun.select_set(True)
bpy.context.view_layer.objects.active = gun
bpy.ops.object.mode_set(mode='EDIT')
bpy.ops.mesh.select_all(action='DESELECT')
bpy.ops.object.mode_set(mode='OBJECT')
for p in gun.data.polygons:
    # A face that reaches past the cut spins: a barrel's long sides run
    # back into the receiver, and left behind they are spikes that don't.
    p.select = max(gun.data.vertices[i].co.x for i in p.vertices) > cut_x
bpy.ops.object.mode_set(mode='EDIT')
bpy.ops.mesh.separate(type='SELECTED')
bpy.ops.object.mode_set(mode='OBJECT')
barrels = next(o for o in bpy.context.scene.objects if o.type == 'MESH' and o is not gun)
barrels.name = 'turret-barrels'

# Both into turret space: the pivot at the origin, on the barrels' axis.
pivot = Vector((lo.x + PIVOT, ay, az))
for o in (gun, barrels):
    o.data.transform(Matrix.Translation(-pivot))
    o.data.update()
blo, bhi = bounds(barrels)
at = blo.x

# The sheets: the whole gun as it is put together, then each piece alone.
os.makedirs(MODELS, exist_ok=True)
os.makedirs(TRACES, exist_ok=True)
copies = []
for o in (gun, barrels):
    c = o.copy()
    c.data = o.data.copy()
    bpy.context.scene.collection.objects.link(c)
    copies.append(c)
whole = join(copies, name='whole')
gun.hide_render = barrels.hide_render = True
sheet(whole, os.path.join(TRACES, 'turret.png'), title='turret')
whole.hide_render = True
gun.hide_render = False
sheet(gun, os.path.join(TRACES, 'turret-cradle.png'), title='turret (cradle)')
gun.hide_render, barrels.hide_render = True, False
sheet(barrels, os.path.join(TRACES, 'turret-barrels.png'), title='turret (barrels)')
barrels.data.transform(Matrix.Translation(Vector((-at, 0, 0))))
barrels.data.update()

# The barrels' heat: a map of where they glow, baked into their own copy
# of the paint as its emission, dark at the receiver and full at the
# muzzle (HEAT_FROM to the end, squared), so the client lights it by one
# colour times how hot it runs (look/turret.rs). Over the paint's
# brightness: the bare steel glows, the soot and the clamps' paint less.
HEAT_FROM = 0.35
HEAT_PX = 512
blo, bhi = bounds(barrels)
heat = bpy.data.images.new('turret-heat', HEAT_PX, HEAT_PX)
for i, m in enumerate(barrels.data.materials):
    m = m.copy()
    barrels.data.materials[i] = m
    nt = m.node_tree
    bsdf = next(n for n in nt.nodes if n.type == 'BSDF_PRINCIPLED')
    tex = nt.nodes.new('ShaderNodeTexImage')
    tex.image = heat
    nt.nodes.active = tex
    # A bake-only emission: the ramp along x, then put back as the map.
    co = nt.nodes.new('ShaderNodeTexCoord')
    xyz = nt.nodes.new('ShaderNodeSeparateXYZ')
    ramp = nt.nodes.new('ShaderNodeMapRange')
    ramp.inputs['From Min'].default_value = bhi.x * HEAT_FROM
    ramp.inputs['From Max'].default_value = bhi.x
    sq = nt.nodes.new('ShaderNodeMath')
    sq.operation = 'POWER'
    sq.inputs[1].default_value = 2.0
    emit = nt.nodes.new('ShaderNodeEmission')
    out = next(n for n in nt.nodes if n.type == 'OUTPUT_MATERIAL')
    old = out.inputs['Surface'].links[0].from_socket
    nt.links.new(co.outputs['Object'], xyz.inputs[0])
    nt.links.new(xyz.outputs['X'], ramp.inputs['Value'])
    nt.links.new(ramp.outputs['Result'], sq.inputs[0])
    nt.links.new(sq.outputs['Value'], emit.inputs['Color'])
    nt.links.new(emit.outputs['Emission'], out.inputs['Surface'])
    m['bake'] = (old.node.name, old.name)
bpy.context.scene.render.engine = 'CYCLES'
bpy.context.scene.cycles.samples = 1
bpy.context.scene.cycles.device = 'CPU'
bpy.ops.object.select_all(action='DESELECT')
barrels.hide_render = False
barrels.select_set(True)
bpy.context.view_layer.objects.active = barrels
bpy.ops.object.bake(type='EMIT', margin=4)
heat.pack()
for m in barrels.data.materials:
    nt = m.node_tree
    out = next(n for n in nt.nodes if n.type == 'OUTPUT_MATERIAL')
    node, sock = m['bake']
    nt.links.new(nt.nodes[node].outputs[sock], out.inputs['Surface'])
    bsdf = next(n for n in nt.nodes if n.type == 'BSDF_PRINCIPLED')
    tex = next(n for n in nt.nodes if n.type == 'TEX_IMAGE' and n.image == heat)
    nt.links.new(tex.outputs['Color'], bsdf.inputs['Emission Color'])
    # Off till the client lights it; the exporter keeps the map at any
    # strength over 0.
    bsdf.inputs['Emission Strength'].default_value = 1.0
    m.name = 'barrels'

# Gun space (+X fore, +Y left, +Z up) to the client's turret frame (+X fore,
# +Y up, +Z right): glTF's exporter turns +Z up into +Y up itself, and
# +Y left into -Z, which is the right's opposite: as wanted.
info = {'file': 'turret', 'barrels_at': round(at, 4), 'reach': round(at + bhi.x, 4),
        'rear': round(lo.x - pivot.x, 4), 'tris': 0}
clo, chi = bounds(gun)
info['cradle'] = [[round(v, 3) for v in clo], [round(v, 3) for v in chi]]
for o, name in ((gun, 'turret'), (barrels, 'turret-barrels')):
    bpy.ops.object.select_all(action='DESELECT')
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    bpy.ops.export_scene.gltf(filepath=os.path.join(MODELS, name + '.glb'), export_format='GLB', export_apply=True,
                              export_yup=True, use_selection=True, export_image_format='JPEG',
                              export_jpeg_quality=88)
    info['tris'] += sum(len(p.vertices) - 2 for p in o.data.polygons)
with open(os.path.join(TRACES, 'turret.json'), 'w') as f:
    json.dump(info, f, indent=1)
print('MODEL', json.dumps(info))

"""The hands in view: one armoured hand and forearm, closed on a grip, made for
first person and held rigid to the gun (look/hands.rs). The fist's hole is at
the origin; the right is the mesh, the left its mirror. From Meshy
(tools/models/meshy_gen.py hand)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import json

import bmesh
from lib import *
from mathutils import Matrix, Vector

IMAGE = ('A single detached right hand and forearm of a futuristic armoured soldier, a game asset. A charcoal black '
         'tactical combat glove with reinforced knuckle plates and armoured finger segments, closed firmly around an '
         'empty upright cylindrical grip 4 centimetres across: the four fingers wrapped and the thumb over them, a '
         'clear round hole through the fist where the grip would be. The wrist runs into a long matte gunmetal and '
         'olive armoured gauntlet up to the elbow, ending in a clean flat cut. The forearm lies horizontally, the '
         'fist to the right, seen exactly from the side with the back of the hand toward the viewer. Plain white '
         'background, soft daylight, a real object. Exactly one hand and one forearm. No gun, nothing held, no body, '
         'no shoulder, no ground, no shadow, no text, no logos.')
TEXTURE = ('Charcoal black tactical glove, scuffed reinforced plates, matte gunmetal and olive drab armoured gauntlet '
           'with fine panel lines, chipped edges, dust and grime')
ASPECT = '16:9'
# Both of what it builds; build.sh finds a model's script by this.
HANDS = ('hand', 'hand-left')
# Its length, rear of the gauntlet to knuckles, and where the fist's hole is in
# the raw mesh: a share of that from the rear, and of its height from the bottom.
LENGTH = 0.31
PAINT = 0.42
HOLE = (0.835, 0.43)


def export(o, name):
    bpy.ops.object.select_all(action='DESELECT')
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    path = os.path.join(MODELS, name + '.glb')
    bpy.ops.export_scene.gltf(filepath=path, export_format='GLB', export_apply=True, export_yup=True, use_selection=True,
                              export_image_format='JPEG', export_jpeg_quality=88)
    print('MODEL', json.dumps({'file': name, 'bytes': os.path.getsize(path), 'size_m': LENGTH}))


if __name__ == '__main__':
    scene()
    o, _ = generated('hand', LENGTH, rough=0.72, metal=0.25)
    # Under a noon sun, its paint as it came reads as a bandage.
    darken(o, PAINT)
    lo, hi = bounds(o)
    hole = Vector((lo.x + HOLE[0] * (hi.x - lo.x), (lo.y + hi.y) / 2, lo.z + HOLE[1] * (hi.z - lo.z)))
    o.data.transform(Matrix.Translation(-hole))
    o.data.update()
    os.makedirs(TRACES, exist_ok=True)
    sheet(o, os.path.join(TRACES, 'hand.png'), title='hand', fist=(0.0, 0.0))

    # The left: a true mirror, its faces turned back out.
    left = o.copy()
    left.data = o.data.copy()
    bpy.context.collection.objects.link(left)
    left.data.transform(Matrix.Scale(-1, 4, Vector((0, 1, 0))))
    bm = bmesh.new()
    bm.from_mesh(left.data)
    bmesh.ops.reverse_faces(bm, faces=bm.faces)
    bm.to_mesh(left.data)
    bm.free()
    left.data.update()

    for mesh, name in ((o, 'hand'), (left, 'hand-left')):
        mesh.data.transform(TO_EXPORT)
        mesh.data.update()
        export(mesh, name)

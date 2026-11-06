"""The hands in view: Triple T's own wooden arms, cut from its model's `char1_arms`
and held rigid to the gun (look/hands.rs). Only the fingers are posed, as its hold
clip has them: the right closed on a grip, the left cupped for a fore-end. The arm
stays as it was skinned, so nothing is dragged or twisted. The hole each hand
holds by is at the origin.

`hand-sword` is the right again for a sword's hilt: its wrist bent, as a real one
is to hold a blade forward of the forearm, and fingers and thumb closed on the
hilt, its arm cut longer so more of it shows. `-- <name>…` builds only those."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import json
import math

import bmesh
from lib import *
from mathutils import Matrix, Quaternion

# What it builds, each from that side's arm; build.sh finds a model's script by this.
HANDS = {'hand': 'Right', 'hand-left': 'Left', 'hand-sword': 'Right'}
CLIP = 'Rifle_Hold_Loop'
# Triple T's hands are a big man's: how big it is drawn against the guns.
SCALE = 0.85
# The body's material lifts its dark albedo 2.6 times (look/bodies.rs' LIFT);
# the map's own values are sRGB, so here that is 2.6 ** (1 / 2.2).
LIFT = 1.54
# Where each arm is cut, as a share of the way from the wrist to the elbow: the
# right runs straight out of the frame, the left across it, past the elbow.
CUT = {'hand': 1.12, 'hand-left': 1.9, 'hand-sword': 1.7}
# The hole against the middle finger's joints: along the hand from the knuckle,
# and from the back of the hand toward the palm, metres at its own size.
HOLE = {'Right': (-0.018, 0.036), 'Left': (-0.018, 0.036)}
# The sword's: which way the forearm comes to the hand, in the hand's frame (along
# the knuckles, toward the thumb's side, toward the back of the hand); how much
# further each finger joint closes and each thumb joint comes in over the hilt, degrees.
SWORD_FOREARM = (0.38, 0.8, -0.46)
SWORD_CURL = 14.0
SWORD_THUMB = (24.0, 22.0)


def bone(rig, name):
    return rig.matrix_world @ rig.pose.bones[name].head


def fingers_closed(rig, side):
    """The rig at rest, but for this hand's fingers as the clip has them."""
    action = bpy.data.actions[CLIP + '_' + rig.name]
    start = action.frame_range[0]
    rig.animation_data_clear()
    for b in rig.pose.bones:
        b.rotation_mode = 'QUATERNION'
        b.rotation_quaternion = (1, 0, 0, 0)
        b.location = (0, 0, 0)
        b.scale = (1, 1, 1)
    for curve in action.fcurves:
        name = curve.data_path.split('"')[1]
        if curve.data_path.endswith('rotation_quaternion') and name.startswith(side + 'Hand') and name != side + 'Hand':
            rig.pose.bones[name].rotation_quaternion[curve.array_index] = curve.evaluate(start)
    bpy.context.view_layer.update()


def axes(rig, side):
    """The hand's frame in the rig's own space: along the knuckles, toward the
    thumb, out of its back."""
    wrist = rig.pose.bones[side + 'Hand'].head
    along = (rig.pose.bones[side + 'HandMiddle1'].head - wrist).normalized()
    thumb = rig.pose.bones[side + 'HandIndex1'].head - rig.pose.bones[side + 'HandPinky1'].head
    thumb = (thumb - along * thumb.dot(along)).normalized()
    return along, thumb, along.cross(thumb)


def turned(rig, name, axis, degrees):
    """Bone `name` and all it carries turned about `axis` through its head."""
    b = rig.pose.bones[name]
    turn = Quaternion(axis, math.radians(degrees)).to_matrix().to_4x4()
    b.matrix = Matrix.Translation(b.head) @ turn @ Matrix.Translation(-b.head) @ b.matrix
    bpy.context.view_layer.update()


def sword_grip(rig, side):
    """The wrist bent and the fingers closed on a hilt."""
    along, thumb, back = axes(rig, side)
    forearm = (rig.pose.bones[side + 'Hand'].head - rig.pose.bones[side + 'ForeArm'].head).normalized()
    want = (along * SWORD_FOREARM[0] + thumb * SWORD_FOREARM[1] + back * SWORD_FOREARM[2]).normalized()
    b = rig.pose.bones[side + 'Hand']
    turn = want.rotation_difference(forearm).to_matrix().to_4x4()
    b.matrix = Matrix.Translation(b.head) @ turn @ Matrix.Translation(-b.head) @ b.matrix
    bpy.context.view_layer.update()
    along, thumb, back = axes(rig, side)
    for finger in ('Index', 'Middle', 'Ring', 'Pinky'):
        for joint in '123':
            turned(rig, f'{side}Hand{finger}{joint}', thumb, SWORD_CURL)
    turned(rig, side + 'HandThumb1', along, -SWORD_THUMB[0])
    turned(rig, side + 'HandThumb1', back, -SWORD_THUMB[1])
    for joint in '23':
        turned(rig, f'{side}HandThumb{joint}', along, -SWORD_THUMB[0] * 0.7)


def hand_frame(rig, side):
    """World to gun space with the hand laid in it: its hole at the origin, the
    knuckles toward +X, the back of the hand +Z; the thumb's side +Y on the right
    hand and -Y on the left, as one's mirror lies against the other."""
    wrist = bone(rig, side + 'Hand')
    knuckle = bone(rig, side + 'HandMiddle1')
    along = (knuckle - wrist).normalized()
    thumb = bone(rig, side + 'HandIndex1') - bone(rig, side + 'HandPinky1')
    thumb = (thumb - along * thumb.dot(along)).normalized()
    if side == 'Left':
        thumb = -thumb
    back = along.cross(thumb)
    at = HOLE[side]
    hole = knuckle + along * at[0] - back * at[1]
    turn = Matrix((along, thumb, back)).to_4x4()
    return Matrix.Scale(SCALE, 4) @ turn @ Matrix.Translation(-hole)


def cut_arm(me, rig, side, name):
    """Only this arm, wrist to just past the elbow, its end capped."""
    wrist = bone(rig, side + 'Hand')
    elbow = bone(rig, side + 'ForeArm')
    at = wrist + (elbow - wrist) * CUT[name]
    bm = bmesh.new()
    bm.from_mesh(me)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-4)
    cut = bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], plane_co=at,
                                 plane_no=wrist - elbow, clear_inner=True)
    edges = [e for e in cut['geom_cut'] if isinstance(e, bmesh.types.BMEdge)]
    bmesh.ops.holes_fill(bm, edges=edges, sides=0)
    # What the cut's plane leaves of the rest of the body: only the arm's own
    # piece, the one with the wrist in it.
    seen = set()
    start = min(bm.verts, key=lambda v: (v.co - wrist).length)
    walk = [start]
    while walk:
        v = walk.pop()
        if v in seen:
            continue
        seen.add(v)
        walk.extend(e.other_vert(v) for e in v.link_edges)
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if v not in seen], context='VERTS')
    bm.to_mesh(me)
    bm.free()


def baked(side, name):
    """This arm as one still mesh in gun space."""
    bpy.ops.import_scene.gltf(filepath=os.path.join(MODELS, 'triple-t.glb'))
    arms = bpy.data.objects['char1_arms']
    rig = arms.parent
    fingers_closed(rig, side)
    if name == 'hand-sword':
        sword_grip(rig, side)
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(arms.evaluated_get(dg), depsgraph=dg)
    me.transform(arms.matrix_world)
    cut_arm(me, rig, side, name)
    me.transform(hand_frame(rig, side))
    # Its seams were split for the file: smooth across them again.
    me.polygons.foreach_set('use_smooth', [True] * len(me.polygons))
    me.use_auto_smooth = True
    me.auto_smooth_angle = math.radians(50)
    me.update()
    o = bpy.data.objects.new('hand', me)
    bpy.context.scene.collection.objects.link(o)
    for other in [x for x in bpy.data.objects if x is not o]:
        bpy.data.objects.remove(other, do_unlink=True)
    return o


def export(o, name):
    bpy.ops.object.select_all(action='DESELECT')
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    path = os.path.join(MODELS, name + '.glb')
    bpy.ops.export_scene.gltf(filepath=path, export_format='GLB', export_apply=True, export_yup=True, use_selection=True,
                              export_image_format='JPEG', export_jpeg_quality=88)
    lo, hi = bounds(o)
    print('MODEL', json.dumps({'file': name, 'bytes': os.path.getsize(path), 'size_m': [round(v, 3) for v in hi - lo],
                               'tris': sum(len(p.vertices) - 2 for p in o.data.polygons)}))


if __name__ == '__main__':
    only = sys.argv[sys.argv.index('--') + 1:] if '--' in sys.argv else list(HANDS)
    for name in only:
        side = HANDS[name]
        scene()
        o = baked(side, name)
        darken(o, LIFT)
        os.makedirs(TRACES, exist_ok=True)
        sheet(o, os.path.join(TRACES, name + '.png'), title=name, fist=(0.0, 0.0))
        o.data.transform(TO_EXPORT)
        o.data.update()
        export(o, name)

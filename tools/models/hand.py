"""The hands in view: Triple T's own wooden arm, cut from its model's `char1_arms`
and held rigid to the gun (look/hands.rs). Only the fingers are posed, closed as
its hold clip closes them; the arm stays as it was skinned, so nothing is dragged
or twisted. The fist's hole is at the origin; the right is the mesh, the left its
mirror."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import json
import math

import bmesh
from lib import *
from mathutils import Matrix, Vector

# Both of what it builds; build.sh finds a model's script by this.
HANDS = ('hand', 'hand-left')
CLIP = 'Rifle_Hold_Loop'
# Triple T's hands are a big man's: how big it is drawn against the guns.
SCALE = 0.85
# The body's material lifts its dark albedo 2.6 times (look/bodies.rs' LIFT);
# the map's own values are sRGB, so here that is 2.6 ** (1 / 2.2).
LIFT = 1.54
# Where the arm is cut, as a share of the way from the wrist past the elbow.
CUT = 1.12
# The fist's hole against the middle finger's joints: along the hand from the
# knuckle, and from the back of the hand toward the palm, metres at its own size.
HOLE = (-0.018, 0.036)


def bone(rig, name):
    return rig.matrix_world @ rig.pose.bones[name].head


def fingers_closed(rig):
    """The rig at rest, but for the right hand's fingers as the clip has them."""
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
        if curve.data_path.endswith('rotation_quaternion') and name.startswith('RightHand') and name != 'RightHand':
            rig.pose.bones[name].rotation_quaternion[curve.array_index] = curve.evaluate(start)
    bpy.context.view_layer.update()


def hand_frame(rig):
    """World to gun space with the hand laid in it: the fist's hole at the
    origin, the knuckles toward +X, the thumb's side +Y, the back of the hand +Z."""
    wrist = bone(rig, 'RightHand')
    knuckle = bone(rig, 'RightHandMiddle1')
    along = (knuckle - wrist).normalized()
    thumb = bone(rig, 'RightHandIndex1') - bone(rig, 'RightHandPinky1')
    thumb = (thumb - along * thumb.dot(along)).normalized()
    back = along.cross(thumb)
    hole = knuckle + along * HOLE[0] - back * HOLE[1]
    turn = Matrix((along, thumb, back)).to_4x4()
    return Matrix.Scale(SCALE, 4) @ turn @ Matrix.Translation(-hole)


def cut_arm(me, rig):
    """Only the right arm, wrist to just past the elbow, its end capped."""
    wrist = bone(rig, 'RightHand')
    elbow = bone(rig, 'RightForeArm')
    at = wrist + (elbow - wrist) * CUT
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


def baked():
    """The arm as one still mesh in gun space."""
    bpy.ops.import_scene.gltf(filepath=os.path.join(MODELS, 'triple-t.glb'))
    arms = bpy.data.objects['char1_arms']
    rig = arms.parent
    fingers_closed(rig)
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(arms.evaluated_get(dg), depsgraph=dg)
    me.transform(arms.matrix_world)
    cut_arm(me, rig)
    me.transform(hand_frame(rig))
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
    scene()
    o = baked()
    darken(o, LIFT)
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

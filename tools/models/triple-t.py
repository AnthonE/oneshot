"""Triple T's clips put right, in place in models/triple-t.glb (Gates' file, from
tools/assets.mjs: run this after it; run twice, it changes nothing).

A body plays a clip on its legs and hips and the hold over its spine, head and
arms (look/anim.rs), so:

- **The lean.** Sprint_Loop tips the hips 37 degrees and the crouches 34, and each
  brings its own head back up; a hold keys the spine and head as a standing body
  holds them, so they took the whole lean, nose to the floor. The hips come up to
  CLIPS' lean, the thighs turned back and the hips moved so the legs don't move.
- **The left hand.** Rifle_Hold_Loop's sits beside the gun. Its arm is solved so the
  palm is under the gun at every key of the clip, held still against the gun.

Only keyframe values are written: the file's layout stays byte for byte. Blender's
own exporter would bake all 55 clips again and move the bones' frames, which
look/paint.rs and bodies::HELD_* are measured in."""
import json
import math
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from lib import MODELS
from mathutils import Matrix, Quaternion, Vector

# Each clip's forward lean off Idle_Loop's hips, degrees.
CLIPS = {'Sprint_Loop': 12.0, 'Crouch_Idle_Loop': 15.0, 'Crouch_Fwd_Loop': 15.0}
THIGHS = ('LeftUpLeg', 'RightUpLeg')
HOLD = 'Rifle_Hold_Loop'
# bodies::HELD_AT, HELD_TURN (x, y, z, w): the gun's holder in the right hand's
# bone, its axes barrel, up, right, its unit a metre where the bone's is a centimetre.
HELD_AT = Vector((4.447, 13.763, -1.861))
HELD_TURN = Quaternion((-0.18712, 0.65416, 0.57784, -0.45073)).normalized()
# The left hand against the gun, each (right, along the barrel, up) from the point
# on its axis over the right fist, metres: where the wrist goes, which way the
# knuckles lie from it, and which way the thumb's side of them does.
WRIST = (-0.045, 0.07, -0.09)
KNUCKLES = (0.5, 0.86, 0.05)
THUMB = (-0.8, 0.45, 0.35)


class Glb:
    def __init__(self, path):
        self.path = path
        raw = open(path, 'rb').read()
        size = struct.unpack_from('<I', raw, 12)[0]
        self.top = raw[:20 + size + 8]
        self.json = json.loads(raw[20:20 + size])
        self.bin = bytearray(raw[20 + size + 8:])
        self.nodes = {n.get('name'): n for n in self.json['nodes']}
        self.names = [n.get('name') for n in self.json['nodes']]
        self.parent = {c: i for i, n in enumerate(self.json['nodes']) for c in n.get('children', [])}

    def track(self, clip, bone, path):
        """The accessor of a clip's channel, or None."""
        anim = next(a for a in self.json['animations'] if a['name'] == clip)
        for ch in anim['channels']:
            t = ch['target']
            if self.names[t['node']] == bone and t['path'] == path:
                return anim['samplers'][ch['sampler']]['output']
        return None

    def _at(self, accessor):
        a = self.json['accessors'][accessor]
        view = self.json['bufferViews'][a['bufferView']]
        assert a['componentType'] == 5126 and 'byteStride' not in view
        return a['count'], view.get('byteOffset', 0) + a.get('byteOffset', 0), {'VEC3': 3, 'VEC4': 4}[a['type']]

    def read(self, accessor):
        n, at, c = self._at(accessor)
        v = struct.unpack_from(f'<{n * c}f', self.bin, at)
        return [v[i * c:(i + 1) * c] for i in range(n)]

    def write(self, accessor, rows):
        n, at, c = self._at(accessor)
        assert len(rows) == n
        struct.pack_into(f'<{n * c}f', self.bin, at, *[x for r in rows for x in r])

    def turns(self, clip, bone):
        """A channel's rotations, as quaternions."""
        return [Quaternion((q[3], q[0], q[1], q[2])) for q in self.read(self.track(clip, bone, 'rotation'))]

    def set_turns(self, clip, bone, turns):
        """Each kept on its last one's side, for what interpolates between them."""
        rows, last = [], None
        for q in turns:
            q = q.normalized()
            if last is not None and q.dot(last) < 0:
                q = -q
            rows.append((q.x, q.y, q.z, q.w))
            last = q
        self.write(self.track(clip, bone, 'rotation'), rows)

    def place(self, bone):
        return Vector(self.nodes[bone]['translation'])

    def save(self):
        with open(self.path, 'wb') as f:
            f.write(self.top + self.bin)


def lean_of(hips, rest):
    """How far `hips` lean forward of `rest`, in the model's frame (+Z forward), radians."""
    up = (hips @ rest.inverted()) @ Vector((0, 1, 0))
    return math.atan2(up.z, up.y)


def stand_up(glb, clip, lean):
    """The hips to `lean` radians on average, the legs where they were."""
    up = glb.nodes[glb.names[glb.parent[glb.names.index('Hips')]]]
    frame = Quaternion((up['rotation'][3], up['rotation'][0], up['rotation'][1], up['rotation'][2]))
    rest = frame @ glb.turns('Idle_Loop', 'Hips')[0]
    hips = glb.turns(clip, 'Hips')
    places = glb.read(glb.track(clip, 'Hips', 'translation'))
    thighs = [glb.turns(clip, t) for t in THIGHS]
    assert all(len(t) == len(hips) for t in thighs) and len(places) == len(hips)
    was = sum(lean_of(frame @ q, rest) for q in hips) / len(hips)
    # About +X, forward is toward +Z; carried into the hips' parent's frame.
    turn = frame.inverted() @ Quaternion(Vector((1, 0, 0)), lean - was) @ frame
    joints = (glb.place(THIGHS[0]) + glb.place(THIGHS[1])) / 2
    out_hips, out_places, out_thighs = [], [], [[] for _ in THIGHS]
    for k, q in enumerate(hips):
        new = turn @ q
        out_hips.append(new)
        # The thighs' joints stay where they were, and the thighs as they were.
        moved = Vector(places[k]) + (q @ joints) - (new @ joints)
        out_places.append(tuple(moved))
        for i, t in enumerate(thighs):
            out_thighs[i].append(new.inverted() @ q @ t[k])
    glb.set_turns(clip, 'Hips', out_hips)
    glb.write(glb.track(clip, 'Hips', 'translation'), out_places)
    for name, t in zip(THIGHS, out_thighs):
        glb.set_turns(clip, name, t)
    print('MODEL', json.dumps({'clip': clip, 'lean_was': round(math.degrees(was), 1), 'lean': round(math.degrees(lean), 1)}))


def frame_to(d, n, d2, n2):
    """The rotation taking a direction and a normal to it onto another pair."""
    a = Matrix((d, n, d.cross(n))).transposed()
    b = Matrix((d2, n2, d2.cross(n2))).transposed()
    return (b @ a.transposed()).to_quaternion()


def chain(glb, bones, turns, k):
    """Down `bones` from the chest's frame: each one's place and turn at key `k`."""
    at, rot, out = Vector((0, 0, 0)), Quaternion(), []
    for b in bones:
        at = at + rot @ glb.place(b)
        rot = rot @ turns[b][k]
        out.append((at.copy(), rot.copy()))
    return out


def under_the_gun(glb):
    """The left arm solved at every key of the hold, in the chest's frame, which is
    the clip's whatever the hips do."""
    right = ['RightShoulder', 'RightArm', 'RightForeArm', 'RightHand']
    left = ['LeftShoulder', 'LeftArm', 'LeftForeArm', 'LeftHand']
    turns = {b: glb.turns(HOLD, b) for b in right + left}
    for b in right + left:
        assert glb.track(HOLD, b, 'translation') is None
    knuckle = glb.place('LeftHandMiddle1').normalized()
    side = glb.place('LeftHandIndex1') - glb.place('LeftHandPinky1')
    side = (side - knuckle * side.dot(knuckle)).normalized()
    holder = lambda v: Vector((v[1], v[2], v[0]))
    want_k = holder(KNUCKLES).normalized()
    want_t = holder(THUMB)
    want_t = (want_t - want_k * want_t.dot(want_k)).normalized()
    out = {b: [] for b in left[1:]}
    for k in range(len(turns['LeftArm'])):
        hand_at, hand_rot = chain(glb, right, turns, k)[-1]
        gun_at = hand_at + hand_rot @ HELD_AT
        gun_rot = hand_rot @ HELD_TURN
        (_, up), (s, ra), (e0, rf), (w0, _) = chain(glb, left, turns, k)
        w = gun_at + gun_rot @ (holder(WRIST) * 100.0)
        a, b = (e0 - s).length, (w0 - e0).length
        to = w - s
        d = min(max(to.length, abs(a - b) + 1e-3), a + b - 1e-3)
        along = to.normalized()
        pole = e0 - s
        out_to = (pole - along * pole.dot(along)).normalized()
        cos = (a * a + d * d - b * b) / (2 * a * d)
        e = s + (along * cos + out_to * math.sqrt(max(1 - cos * cos, 0))) * a
        w = s + along * d
        # The bones' twist and the bend's plane stay the clip's.
        du, df = (e0 - s).normalized(), (w0 - e0).normalized()
        n = du.cross(df).normalized()
        du2, df2 = (e - s).normalized(), (w - e).normalized()
        n2 = du2.cross(df2).normalized()
        arm = frame_to(du, n, du2, n2) @ ra
        fore = frame_to(df, n, df2, n2) @ rf
        hand = frame_to(knuckle, side, gun_rot @ want_k, gun_rot @ want_t)
        out['LeftArm'].append(up.inverted() @ arm)
        out['LeftForeArm'].append(arm.inverted() @ fore)
        out['LeftHand'].append(fore.inverted() @ hand)
    for b, q in out.items():
        glb.set_turns(HOLD, b, q)
    print('MODEL', json.dumps({'clip': HOLD, 'left_arm_keys': len(out['LeftArm'])}))


if __name__ == '__main__':
    glb = Glb(os.path.join(MODELS, 'triple-t.glb'))
    for clip, lean in CLIPS.items():
        stand_up(glb, clip, math.radians(lean))
    under_the_gun(glb)
    glb.save()

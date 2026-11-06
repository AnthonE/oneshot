#!/usr/bin/env python3
"""A model from Meshy, in two stages, after Gates' ci/meshy_gen.py:

    nano-banana-pro text-to-image  ->  image-to-3d (smart topology, meshy-t2.5)

    tools/models/meshy_gen.py ar                  a gun: its script's IMAGE and TEXTURE
    tools/models/meshy_gen.py ar --image-only     just the picture, to look at first;
                                                  then without the flag: the mesh of it
    tools/models/meshy_gen.py crate --image-prompt "…" [--mesh-prompt "…"]   anything else
    tools/models/meshy_gen.py ar --paint gilded --style "…"   a new paint on the mesh it has

A paint keeps the gun's triangles and UVs and comes back as `<slug>~<tag>_raw.glb`;
`MODEL_PAINT=<tag> tools/models/build.sh <slug>` cuts it (10 credits).

It writes `<slug>.png` (the picture), `<slug>_raw.glb` (the mesh as it came)
and `<slug>.json` (the prompts, both task ids, the credits, the date) into
sources/meshy (MESHY_SRC names another place; git-ignored: the raw is a
download, and what ships is the cut the gun's script makes of it:
`tools/models/build.sh ar`). A slug that already has a raw is refused
without --roll, which sets the old roll aside in `rolls/`.

A gun's picture is its side, the muzzle to the right, on white; the script's
IMAGE says what it is and SIDE (below) is added to it, with the script's
FRONT for "the muzzle" where it has none (a rocket's nose). No proper noun
in a prompt, ever: these are ours by name and look.

Costs credits: about 9 for the picture and 15 for the mesh (the balance is
printed before and after; `credits` in the sidecar is the difference, so
it is only true of a run that had the account to itself).

The key is MESHY_API_KEY, from the environment or .env.local, and is never
written anywhere.

Traps Gates paid for:
  - One view of an irregular thing (a rock) comes back a slab. --multiview
    asks the picture stage for several views and builds from all of them,
    which costs the meshy-7.1 path (smart topology takes one picture only),
    and that path ignores --tris unless it remeshes, so it is told to.
  - Say EXACTLY ONE and name what not to draw: "held in one hand" modelled
    a hand, a spear came back as two.
  - The size a mesh comes at is a guess; the gun's script sets the length.
  - The glow map it ships is wrong more often than right; the gun's script
    drops it and lights what the picture painted bright instead.
"""
import argparse
import ast
import base64
import datetime
import json
import os
import shutil
import sys
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, '..', '..'))
OUT = os.environ.get('MESHY_SRC', os.path.join(ROOT, 'sources', 'meshy'))
API = 'https://api.meshy.ai'
POLL_S = 10
# Meshy's own estimate is two or three minutes; past twenty it has hung.
TIMEOUT_S = 1200

# The mesh stage's model, and how big it paints.
MESH = 'meshy-t2.5'
PAINT_PX = '4k'

# Added to a gun's IMAGE: the view the mesh stage reads best, and the things
# a picture stage draws unasked.
SIDE = (' Lying horizontally on a plain white background, seen exactly from the side in flat profile, {front} pointing '
        'right. A real object, photographed in soft daylight. Exactly one object. '
        'No ground, no shadow, no people, no hands, no text, no labels, no logos, no scale bar.')
# Added to a gun's TEXTURE.
PAINT = ', no text, no logos'
# The paint stage's model: the one that takes the light back out of a paint.
PAINT_MODEL = 'meshy-6'


def key():
    k = os.environ.get('MESHY_API_KEY', '').strip()
    env = os.path.join(ROOT, '.env.local')
    if not k and os.path.exists(env):
        for line in open(env):
            if line.startswith('MESHY_API_KEY='):
                k = line.split('=', 1)[1].strip().strip('"\'')
    return k or sys.exit('MESHY_API_KEY is not set (the environment, or .env.local)')


def call(method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(API + path, data=data, method=method)
    req.add_header('Authorization', 'Bearer ' + key())
    if data:
        req.add_header('Content-Type', 'application/json')
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as e:
        sys.exit(f'{method} {path} -> {e.code}: {e.read().decode()[:400]}')


def wait(path, task, label):
    """Polls one task until it has SUCCEEDED, or exits saying which and why."""
    t0 = time.time()
    last = None
    while True:
        t = call('GET', f'{path}/{task}')
        now = (t.get('status'), t.get('progress', 0))
        if now != last:
            print(f'  {label} {task[:8]} {now[0]} {now[1]}%', flush=True)
            last = now
        if now[0] == 'SUCCEEDED':
            return t
        if now[0] in ('FAILED', 'CANCELED'):
            sys.exit(f'{label} {task}: {now[0]}: {t.get("task_error")}')
        if time.time() - t0 > TIMEOUT_S:
            sys.exit(f'{label} {task}: still {now[0]} after {TIMEOUT_S} s; resume it by its id')
        time.sleep(POLL_S)


def fetch(url, dest):
    with urllib.request.urlopen(urllib.request.Request(url), timeout=300) as r:
        open(dest, 'wb').write(r.read())
    return os.path.getsize(dest)


def data_uri(path):
    ext = os.path.splitext(path)[1].lower().lstrip('.')
    mime = {'jpg': 'jpeg', 'jpeg': 'jpeg', 'png': 'png', 'webp': 'webp'}[ext]
    return f'data:image/{mime};base64,' + base64.b64encode(open(path, 'rb').read()).decode()


def script_prompts(slug):
    """A gun's script's IMAGE, TEXTURE, ASPECT and FRONT, read without running
    it (it is Blender's)."""
    path = os.path.join(HERE, slug + '.py')
    if not os.path.exists(path):
        return {}
    found = {}
    for node in ast.parse(open(path).read()).body:
        if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
            if node.targets[0].id in ('IMAGE', 'TEXTURE', 'ASPECT', 'FRONT'):
                found[node.targets[0].id] = ast.literal_eval(node.value)
    return found


def set_aside(out, slug):
    """The roll before this one, kept under rolls/ by its mesh task."""
    side = os.path.join(out, slug + '.json')
    done = json.load(open(side)) if os.path.exists(side) else {}
    tag = (done.get('paint_task') or done.get('mesh_task', 'old'))[:8]
    dest = os.path.join(out, 'rolls', f'{slug}-{tag}')
    os.makedirs(dest, exist_ok=True)
    for f in os.listdir(out):
        stem = os.path.splitext(f)[0]
        if stem in (slug, slug + '_raw') or stem.startswith(slug + '_') and stem[len(slug) + 1:].isdigit():
            shutil.move(os.path.join(out, f), os.path.join(dest, f))
    print(f'  the last roll is in {dest}')


def paint(a, own):
    """A new paint on the mesh `a.slug` already has, from a style prompt or a
    picture. The mesh task has to be one of this account's, and Meshy's tasks
    expire: past that, it needs its raw uploading as `model_url`."""
    made = json.load(open(os.path.join(a.out, a.slug + '.json')))
    tag = f'{a.slug}~{a.paint}'
    glb = os.path.join(a.out, tag + '_raw.glb')
    if os.path.exists(glb):
        if not a.roll:
            sys.exit(f'{glb} is there; --roll makes another (and spends the credits)')
        set_aside(a.out, tag)
    style = a.style or own.get('TEXTURE')
    if not (style or a.style_image):
        sys.exit(f'{a.slug}: no --style or --style-image, and no TEXTURE in its script')
    ep = '/openapi/v1/retexture'
    bal0 = call('GET', '/openapi/v1/balance')['balance']
    print(f'balance {bal0}')
    task = call('POST', ep, {
        'input_task_id': made['mesh_task'],
        **({'image_style_url': data_uri(a.style_image)} if a.style_image else {'text_style_prompt': style.strip() + PAINT}),
        'ai_model': a.paint_model,
        'enable_original_uv': True,
        'enable_pbr': True,
        'texture_resolution': PAINT_PX,
        'target_formats': ['glb'],
    })['result']
    done = wait(ep, task, 'paint')
    size = fetch(done['model_urls']['glb'], glb)
    bal1 = call('GET', '/openapi/v1/balance')['balance']
    json.dump({
        'slug': tag,
        'date': datetime.date.today().isoformat(),
        'vendor': 'Meshy',
        'style': None if a.style_image else style,
        'style_image': os.path.basename(a.style_image) if a.style_image else None,
        'mesh_task': made['mesh_task'],
        'paint_task': task,
        'paint_model': a.paint_model,
        'paint_px': PAINT_PX,
        'credits': bal0 - bal1,
        'glb_bytes': size,
    }, open(os.path.join(a.out, tag + '.json'), 'w'), indent=1)
    print(f'\n{glb}  {size / 1e6:.1f} MB   {bal0 - bal1} credits (paint {task})')


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('slug')
    ap.add_argument('--image-prompt', help="the picture; default: the gun's script's IMAGE, with SIDE")
    ap.add_argument('--mesh-prompt', help="the paint; default: the script's TEXTURE; with neither, painted from the picture")
    ap.add_argument('--out', default=OUT)
    ap.add_argument('--aspect', help='the picture, 1:1 unless the script says (ASPECT)')
    ap.add_argument('--tris', type=int, help='target_polycount for the mesh (meshy-t2.5 makes 15,000 unasked, up to 30,000)')
    ap.add_argument('--mesh-model', default=MESH, help=f'{MESH} (smart topology), or a standard one: meshy-7.1')
    ap.add_argument('--multiview', action='store_true', help='several views, and a mesh from all of them')
    ap.add_argument('--ref', action='append', default=[], help='a picture it must look like (up to 5): image-to-image')
    ap.add_argument('--no-texture', action='store_true', help='the shape only')
    ap.add_argument('--image-only', action='store_true', help='stop after the picture')
    ap.add_argument('--resume-image', help='this picture task, not a new one')
    ap.add_argument('--resume-mesh', help='this mesh task, not a new one')
    ap.add_argument('--roll', action='store_true', help='again, though a raw is there')
    ap.add_argument('--paint', metavar='TAG', help='a new paint on the mesh it has, not a new gun')
    ap.add_argument('--style', help="what the paint is; default: the script's TEXTURE")
    ap.add_argument('--style-image', help='or a picture it should look like')
    ap.add_argument('--paint-model', default=PAINT_MODEL, help=f'{PAINT_MODEL}, or latest')
    a = ap.parse_args()

    own = script_prompts(a.slug)
    if a.paint:
        return paint(a, own)
    side = SIDE.format(front=own.get('FRONT', 'the muzzle'))
    image_prompt = a.image_prompt or (own['IMAGE'].strip() + side if 'IMAGE' in own else None)
    mesh_prompt = a.mesh_prompt or (own['TEXTURE'].strip() + PAINT if 'TEXTURE' in own else None)
    aspect = a.aspect or own.get('ASPECT', '1:1')
    if not image_prompt:
        sys.exit(f'{a.slug}: no --image-prompt, and no IMAGE in tools/models/{a.slug}.py')
    os.makedirs(a.out, exist_ok=True)
    glb = os.path.join(a.out, a.slug + '_raw.glb')
    if os.path.exists(glb) and not a.image_only:
        if not (a.roll or a.resume_mesh):
            sys.exit(f'{glb} is there; --roll makes another (and spends the credits)')
        set_aside(a.out, a.slug)

    # A picture made with --image-only and not yet meshed is the one to mesh.
    side_path = os.path.join(a.out, a.slug + '.json')
    if not (a.resume_image or a.image_only or a.roll) and os.path.exists(side_path):
        side = json.load(open(side_path))
        if 'mesh_task' not in side and side.get('image_prompt') == image_prompt:
            a.resume_image = side['image_task']
            print(f'the picture already made: {a.resume_image}')

    bal0 = call('GET', '/openapi/v1/balance')['balance']
    print(f'balance {bal0}')

    # The picture. nano-banana-pro: the mesh is only as good as it.
    img_ep = '/openapi/v1/image-to-image' if a.ref else '/openapi/v1/text-to-image'
    img_id = a.resume_image or call('POST', img_ep, {
        'ai_model': 'nano-banana-pro',
        'prompt': image_prompt,
        **({'reference_image_urls': [data_uri(f) for f in a.ref[:5]]} if a.ref else {}),
        # One or the other: a sheet of views has a shape of its own (400).
        **({'generate_multi_view': True} if a.multiview else {'aspect_ratio': aspect}),
    })['result']
    img = wait(img_ep, img_id, 'image')
    urls = [u for u in (img.get('image_urls') or [img.get('image_url')]) if u]
    if not urls:
        sys.exit(f'image {img_id} succeeded with no picture: {img}')
    for i, u in enumerate(urls):
        fetch(u, os.path.join(a.out, a.slug + (f'_{i}' if i else '') + '.png'))
    print(f'  {os.path.join(a.out, a.slug)}.png  ({len(urls)} view(s), image task {img_id})')
    if a.image_only:
        json.dump({'slug': a.slug, 'image_prompt': image_prompt, 'image_task': img_id}, open(side_path, 'w'), indent=1)
        print(f'{bal0 - call("GET", "/openapi/v1/balance")["balance"]} credits; the mesh of it: without --image-only')
        return

    # The mesh. Smart topology beat the standard model on everything Gates
    # measured: more shape, more contrast, a third of the file, half the
    # credits, and it keeps its size and origin. It refuses remove_lighting,
    # which measured as nothing on the other path. A standard model
    # (--mesh-model meshy-7.1) is a dense scan, so it is remeshed.
    multi = a.multiview and len(urls) > 1
    mesh_ep = '/openapi/v1/multi-image-to-3d' if multi else '/openapi/v1/image-to-3d'
    if a.resume_mesh:
        mesh_id = a.resume_mesh
    else:
        common = {
            **({'should_texture': False} if a.no_texture else {
                'should_texture': True,
                'enable_pbr': True,
                'texture_resolution': PAINT_PX,
                **({'texture_prompt': mesh_prompt} if mesh_prompt else {}),
            }),
            'auto_size': True,
            'origin_at': 'bottom',
            **({'target_polycount': a.tris} if a.tris else {}),
        }
        if multi:
            # meshy-7.1, not 7 (deprecated 2026-09-18). It keeps to --tris
            # only when it remeshes: three stones asked for 3,000 triangles
            # came back at 580,271.
            body = {'image_urls': urls[:4], 'ai_model': 'meshy-7.1', **common,
                    **({'should_remesh': True, 'topology': 'triangle'} if a.tris else {})}
        elif a.mesh_model.startswith('meshy-t'):
            body = {'image_url': urls[0], 'model_type': 'smart-topology', 'ai_model': a.mesh_model, **common}
        else:
            body = {'image_url': urls[0], 'model_type': 'standard', 'ai_model': a.mesh_model, **common,
                    'should_remesh': True, 'topology': 'triangle'}
        mesh_id = call('POST', mesh_ep, body)['result']
    mesh = wait(mesh_ep, mesh_id, 'mesh ')
    size = fetch(mesh['model_urls']['glb'], glb)
    bal1 = call('GET', '/openapi/v1/balance')['balance']

    json.dump({
        'slug': a.slug,
        'date': datetime.date.today().isoformat(),
        'vendor': 'Meshy',
        'image_prompt': image_prompt,
        'mesh_prompt': mesh_prompt,
        'image_task': img_id,
        'mesh_task': mesh_id,
        'image_model': 'nano-banana-pro',
        'image_stage': img_ep.rsplit('/', 1)[-1],
        'references': [os.path.basename(f) for f in a.ref],
        'mesh_model': 'meshy-7.1 multi-image-to-3d' if multi else a.mesh_model,
        'paint_px': PAINT_PX,
        'views': len(urls),
        'target_polycount': a.tris,
        'credits': bal0 - bal1,
        'glb_bytes': size,
    }, open(side_path, 'w'), indent=1)
    print(f'\n{glb}  {size / 1e6:.1f} MB   {bal0 - bal1} credits (image {img_id}, mesh {mesh_id})')


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Put one platform's depot live in the Elo Pros launcher. After Gates' ci/publish_depot.py.

    ci/publish_depot.py --platform linux-x86_64              build, upload, publish
    ci/publish_depot.py --platform win-x86_64 --no-build     package what is compiled
    ci/publish_depot.py --platform linux-x86_64 --dry-run    every check, no writes
    ci/publish_depot.py --platform linux-x86_64 --notarize   ...and seal its digest on chain

An OPERATOR ACT, run from a box that can `ssh morr` (as ci/publish_web.sh).
The order is the point: refuse a dirty tree and a build the live server
would refuse (its server.json `proto`), package (ci/depot.py), rsync to
<depots>/oneshot/<build>/<platform>/, re-hash every file where it landed,
and only then point published.json's key for this platform at it (the
previous file kept as .bak). Then read the live manifest back.

The launcher only fills a native row its manifest already has: the Elo Pros
repo's data/launcher/oneshot.manifest.json needs a slot for the platform.

`--notarize` seals the depot digest the origin serves on chain 4663 (the
notary contract, label `oneshot-<platform>`) with `cast send` run ON the
origin, where the key lives; nothing in the install path reads the chain,
so it can come later or never.
"""
from __future__ import annotations

import argparse
import json
import re
import shlex
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import depot  # noqa: E402  the packager, one implementation

SLUG = depot.SLUG
DEFAULT_HOST = "morr"
DEFAULT_DEPOTS = "/data/apps/scry-data/depots"
DEFAULT_API = "https://elopros.com/api"
SERVER_JSON = "https://game.elopros.com/oneshot/server.json"
NOTARY = "0x0C15fA7829458118e3d26229F58FE0443f8b792c"  # chain 4663


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    p = subprocess.run(cmd, text=True, capture_output=True, **kw)
    if p.returncode != 0:
        sys.exit(f"publish: `{' '.join(cmd)}` failed:\n{p.stdout}\n{p.stderr}".rstrip())
    return p


def ssh(host: str | None, script: str) -> str:
    """One command on the origin; `None` is this box (`--local`)."""
    if host is None:
        return run(["bash", "-c", script]).stdout.strip()
    return run(["ssh", "-o", "BatchMode=yes", host, script]).stdout.strip()


def refuse_wire_mismatch(url: str, strand: bool) -> None:
    src = (depot.ROOT / "crates/proto/src/lib.rs").read_text()
    want = int(re.search(r"pub const PROTO_VER: u16 = (\d+)", src).group(1))
    try:
        live = json.loads(run(["curl", "-s", "--max-time", "15", url]).stdout).get("proto")
    except (json.JSONDecodeError, AttributeError):
        live = None
    if live == want:
        print(f"   wire ok: the live server and this build speak proto {want}")
        return
    why = f"the live server ({url}) speaks proto {live} and this build {want}"
    if strand:
        print(f"   !! {why}: publishing anyway (--strand)")
        return
    sys.exit(f"publish: {why}; every launch would be refused. Deploy the server first "
             "(ci/deploy_server.sh), or pass --strand.")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--platform", default="linux-x86_64", choices=sorted(depot.TARGETS))
    ap.add_argument("--no-build", action="store_true")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--notarize", action="store_true")
    ap.add_argument("--host", default=DEFAULT_HOST)
    ap.add_argument("--local", action="store_true", help="the origin is this box")
    ap.add_argument("--depots", default=DEFAULT_DEPOTS)
    ap.add_argument("--api", default=DEFAULT_API)
    ap.add_argument("--server-json", default=SERVER_JSON)
    ap.add_argument("--strand", action="store_true")
    a = ap.parse_args()
    host = None if a.local else a.host

    if run(["git", "status", "--porcelain"], cwd=depot.ROOT).stdout.strip():
        sys.exit("publish: the tree has uncommitted changes; a published build names a commit")
    build = depot.build_id()
    print(f"== {SLUG} {a.platform} · build {build}" + ("  [DRY RUN]" if a.dry_run else ""))
    refuse_wire_mismatch(a.server_json, a.strand)

    cmd = [sys.executable, str(depot.ROOT / "ci" / "depot.py"), "--platform", a.platform]
    if a.no_build:
        cmd.append("--no-build")
    if subprocess.run(cmd, cwd=depot.ROOT).returncode != 0:
        sys.exit("publish: packaging failed: nothing uploaded")
    stage = depot.depot_dir(a.platform, build)
    doc = json.loads((stage / "depot.json").read_text())
    if doc.get("build") != build or doc.get("platform") != a.platform:
        sys.exit(f"publish: {stage}/depot.json says {doc.get('build')} {doc.get('platform')}")

    remote = f"{a.depots}/{SLUG}/{build}/{a.platform}"
    dest = f"{remote}/" if host is None else f"{host}:{remote}/"
    print(f"== upload · {stage} → {dest}")
    if a.dry_run:
        print("\nDRY RUN: nothing sent, published.json untouched")
        return 0
    ssh(host, f"mkdir -p {shlex.quote(remote)}")
    run(["rsync", "-a", "--info=stats1", f"{stage}/", dest])

    print("== verify · re-hashing every file at the origin")
    checker = (
        "import json,hashlib,pathlib\n"
        "d=json.load(open('depot.json'));bad=[]\n"
        "for f in d['files']:\n"
        "    p=pathlib.Path(f['path'])\n"
        "    if not p.is_file(): bad.append((f['path'],'missing'))\n"
        "    elif hashlib.sha256(p.read_bytes()).hexdigest()!=f['sha256']: bad.append((f['path'],'sha256'))\n"
        "    elif f['executable'] and not p.stat().st_mode & 0o111: bad.append((f['path'],'mode'))\n"
        "print(json.dumps({'n':len(d['files']),'bad':bad}))\n"
    )
    r = json.loads(ssh(host, f"cd {shlex.quote(remote)} && python3 -c {shlex.quote(checker)}").splitlines()[-1])
    if r["bad"]:
        sys.exit(f"publish: the origin doesn't hold what was packaged: {r['bad'][:10]}")
    print(f"   {r['n']} files verified")

    pointer = f"{a.depots}/{SLUG}/published.json"
    print(f"== publish · {pointer}: {a.platform} → {build}")
    writer = (
        "import json,pathlib,shutil\n"
        f"p=pathlib.Path({pointer!r})\n"
        "d=json.loads(p.read_text()) if p.exists() else {}\n"
        "before=dict(d)\n"
        "if p.exists(): shutil.copy2(p,str(p)+'.bak')\n"
        f"d[{a.platform!r}]={build!r}\n"
        "p.write_text(json.dumps(d))\n"
        "print(json.dumps({'before':before,'after':d}))\n"
    )
    r = json.loads(ssh(host, f"python3 -c {shlex.quote(writer)}").splitlines()[-1])
    print(f"   before {r['before']}\n   after  {r['after']}")

    print("== confirm · the live manifest")
    rows = json.loads(run(["curl", "-sf", "--max-time", "30", f"{a.api}/launcher/manifest/{SLUG}"]).stdout).get("builds", [])
    row = next((b for b in rows if b.get("platform") == a.platform), None)
    if row is None:
        sys.exit(f"publish: the manifest has no {a.platform} slot: add one to the Elo Pros repo's "
                 f"data/launcher/{SLUG}.manifest.json (the origin never makes a row)")
    if row.get("version") != build or row.get("depot_state") != "published":
        sys.exit(f"publish: the origin serves {row.get('version')} ({row.get('depot_state')}), not {build}")
    digest = row.get("depot_digest")
    # The one string from the network that reaches a shell line below: a
    # bytes32, or nothing.
    if digest is not None and not re.fullmatch(r"0x[0-9a-fA-F]{64}", str(digest)):
        sys.exit(f"publish: the manifest's depot_digest isn't a bytes32: {digest!r}")
    print(f"   ok: {a.platform} → {build}, digest {digest}")

    if a.notarize:
        if not digest:
            sys.exit("publish: the manifest gave no digest to notarize")
        label, memo = f"{SLUG}-{a.platform}", f"{SLUG} {build} {a.platform} depot"
        print(f"== notarize · {digest} as {label}")
        send = (
            "set -a; . /data/apps/secrets/keys.env; set +a; "
            'POOL="${ELO_RH_RPC_POOL:-$SCRY_RH_RPC_POOL}"; RPC=""; '
            'for u in $(printf %s "$POOL" | tr "," " "); do '
            '  [ "$(~/.foundry/bin/cast chain-id --rpc-url "$u" 2>/dev/null)" = 4663 ] && { RPC=$u; break; }; '
            "done; "
            '[ -n "$RPC" ] || { echo "no reachable 4663 endpoint in ELO_RH_RPC_POOL" >&2; exit 1; }; '
            f"~/.foundry/bin/cast send {NOTARY} 'notarize(bytes32,string,string)' {shlex.quote(digest)} "
            f"{shlex.quote(label)} {shlex.quote(memo)} --rpc-url \"$RPC\" --private-key \"$PRIVATE_KEY\" --json"
        )
        out = ssh(host, send)
        try:
            t = json.loads(out.splitlines()[-1])
            print(f"   tx {t.get('transactionHash')} status {t.get('status')}")
        except (json.JSONDecodeError, IndexError):
            print("   " + out)
    print("\nDONE")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Package the desktop client as an Elo Pros launcher depot. After Gates' ci/depot.py.

    ci/depot.py                          the Linux depot: build, stage, depot.json
    ci/depot.py --platform win-x86_64    the Windows one (x86_64-pc-windows-gnu, mingw)
    ci/depot.py --no-build               package what is already compiled
    ci/depot.py --self-test              the document's rules, no compiler

A depot is one build of one game: every file with its sha256, and the one
command that starts it. The launcher fetches every file, hashes it, and only
then moves the build into place (the Elo Pros repo's docs/client/LAUNCHER.md
§3, `elo-depot`). It lands in target/depot/<platform>/<build>/.

The build id is `<version>-g<sha9>`; a dirty tree adds the binary's own hash
(`-dirty.<8 hex>`), so two dirty builds never share a directory on a
player's disk. The digest that gets notarized is the launcher's to compute
(`elo digest`), never this script's. Publishing is ci/publish_depot.py.

No system library is bundled on Linux (the distro's libasound, libudev,
libwayland); what the binary links against is measured into
`requires.libs`. Windows bundles the mingw runtime DLLs the exe imports,
since no Windows machine has them, with their notice (ci/licenses/).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CLIENT = ROOT / "crates" / "client"
SLUG = "oneshot"
DEPOT_VERSION = 1

# The platform tags are the launcher's (`elo-depot`'s PLATFORMS): Windows is
# `win-x86_64`, never `windows-x86_64`, or the row never renders.
TARGETS = {
    "linux-x86_64": {
        "triple": None,
        "exec": "oneshot",
        "features": "sound,wayland,pad",
        "strip": "strip",
        "method": "elf-dt-needed",
        "dlopen": "libxkbcommon-x11, libvulkan, libGL",
    },
    "win-x86_64": {
        "triple": "x86_64-pc-windows-gnu",
        "exec": "oneshot.exe",
        "features": "sound,pad",
        "strip": "x86_64-w64-mingw32-strip",
        "method": "pe-import-table",
        "dlopen": "vulkan-1.dll, d3d12.dll",
        # Whether the toolchain has its own copy of a DLL: the mingw runtime
        # resolves to a path, Windows' own DLLs to nothing.
        "dll_probe": "x86_64-w64-mingw32-gcc",
        "pe_format": "pei-x86-64",
    },
}

# Baked into the document, and so into its digest: never rewritten later.
DEFAULT_ROOT = "https://elopros.com/api/launcher/depot/{slug}/{build}/{platform}/files"

# What the launcher fills. oneshot finds its server from server.json
# (`--server` is a page's URL, not the game's host:port the launcher's Join
# would pass), and the player is proved through the launcher's socket
# (src/elo.rs), so it is started with nothing.
LAUNCH_ARGS: list[str] = []


def sh(*cmd: str, check: bool = True) -> str:
    p = subprocess.run(cmd, cwd=ROOT, text=True, capture_output=True)
    if check and p.returncode != 0:
        sys.exit(f"depot: `{' '.join(cmd)}` failed:\n{p.stderr.strip()}")
    return p.stdout.strip()


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for block in iter(lambda: fh.read(1 << 18), b""):
            h.update(block)
    return h.hexdigest()


def crate_version() -> str:
    m = re.search(r'^version\s*=\s*"([^"]+)"', (CLIENT / "Cargo.toml").read_text(), re.M)
    if not m:
        sys.exit("depot: no version in crates/client/Cargo.toml")
    return m.group(1)


def build_id(binary: Path | None = None) -> str:
    ver = crate_version()
    sha = sh("git", "rev-parse", "--short=9", "HEAD")
    if not sh("git", "status", "--porcelain"):
        return f"{ver}-g{sha}"
    mark = sha256_file(binary)[:8] if binary and binary.is_file() else "unknown"
    return f"{ver}-g{sha}-dirty.{mark}"


def needed_libs(binary: Path, platform: str) -> tuple[list[str], str]:
    """What the binary links against (link time only), or why unmeasured."""
    if TARGETS[platform]["method"] == "pe-import-table":
        probes = [(["objdump", "-p", str(binary)], r"^\s*DLL Name:\s*(\S+)")]
    else:
        probes = [(["objdump", "-p", str(binary)], r"^\s*NEEDED\s+(\S+)"),
                  (["readelf", "-d", str(binary)], r"\(NEEDED\).*\[([^\]]+)\]")]
    for cmd, pat in probes:
        if shutil.which(cmd[0]) is None:
            continue
        p = subprocess.run(cmd, text=True, capture_output=True)
        if p.returncode == 0:
            return sorted(set(re.findall(pat, p.stdout, re.M))), ""
    return [], "no objdump or readelf on the build machine: the libraries were NOT measured"


def toolchain_dll(name: str, probe: str) -> Path | None:
    p = subprocess.run([probe, f"-print-file-name={name}"], text=True, capture_output=True)
    got = Path(p.stdout.strip())
    return got if p.returncode == 0 and got.is_absolute() and got.is_file() else None


def runtime_dlls(binary: Path, platform: str) -> list[Path]:
    """Every DLL the build needs that Windows doesn't provide, transitively
    (libstdc++ imports libgcc_s and libwinpthread, which the exe never names).
    Refuses rather than stage none when it can't look."""
    spec = TARGETS[platform]
    probe = spec.get("dll_probe")
    if not probe:
        return []
    if shutil.which(probe) is None:
        sys.exit(f"depot: {probe} is not here, so the build's runtime DLLs can't be found")
    found: dict[str, Path] = {}
    queue, seen = [binary], set()
    while queue:
        imports, why = needed_libs(queue.pop(), platform)
        if why:
            sys.exit(f"depot: {why}")
        for name in imports:
            if name.lower() in seen:
                continue
            seen.add(name.lower())
            src = toolchain_dll(name, probe)
            if src is None:
                continue
            fmt = re.search(r"file format (\S+)", sh("objdump", "-f", str(src)))
            if not fmt or fmt.group(1) != spec["pe_format"]:
                sys.exit(f"depot: {src} is not {spec['pe_format']} (0xc000007b on a player's machine)")
            found[name.lower()] = src
            queue.append(src)
    return [found[k] for k in sorted(found)]


def safe_relpath(raw: str) -> str:
    """The launcher's path rule, at the writing end."""
    if not raw or "\x00" in raw or "\\" in raw or len(raw) > 1024:
        sys.exit(f"depot: bad path {raw!r}")
    if raw.startswith("/") or raw.endswith("/") or (len(raw) > 1 and raw[1] == ":"):
        sys.exit(f"depot: paths must be relative files: {raw!r}")
    if any(p in ("", ".", "..") for p in raw.split("/")):
        sys.exit(f"depot: path escapes the build directory: {raw!r}")
    return raw


def build_depot_doc(stage: Path, build: str, root: str, *, platform: str,
                    requires: dict | None = None) -> dict:
    exec_name = TARGETS[platform]["exec"]
    files, seen = [], set()
    for p in sorted(stage.rglob("*")):
        if not p.is_file() or p.is_symlink() or p.name == "depot.json":
            continue
        rel = safe_relpath(p.relative_to(stage).as_posix())
        if rel.lower() in seen:
            sys.exit(f"depot: two files differ only by case: {rel!r}")
        seen.add(rel.lower())
        files.append({"path": rel, "sha256": sha256_file(p), "bytes": p.stat().st_size,
                      "executable": rel == exec_name})
    if exec_name not in {f["path"] for f in files}:
        sys.exit(f"depot: launch.exec {exec_name!r} is not one of the staged files")
    doc = {
        "depot_version": DEPOT_VERSION,
        "slug": SLUG,
        "build": build,
        "platform": platform,
        "root": root.format(slug=SLUG, build=build, platform=platform),
        "files": files,
        "launch": {"exec": exec_name, "args": LAUNCH_ARGS, "cwd": ".", "env": {}},
    }
    if requires:
        doc["requires"] = requires
    return doc


def target_dir(platform: str) -> Path:
    """A cross build has a build folder of its own, so it holds no lock
    the native one waits on (ci/release.sh builds them side by side)."""
    spec = TARGETS[platform]
    return CLIENT / "target" / "cross" if spec["triple"] else CLIENT / "target"


def built_binary(platform: str) -> Path:
    spec = TARGETS[platform]
    sub = f"{spec['triple']}/desktop" if spec["triple"] else "desktop"
    return target_dir(platform) / sub / spec["exec"]


def stage_build(out: Path, platform: str, do_build: bool) -> tuple[Path, dict]:
    spec = TARGETS[platform]
    if do_build:
        cmd = ["cargo", "build", "--profile", "desktop", "--locked", "--bin", "oneshot",
               "--features", spec["features"]]
        if spec["triple"]:
            cmd += ["--target", spec["triple"], "--target-dir", str(target_dir(platform))]
        print("== " + " ".join(cmd))
        if subprocess.run(cmd, cwd=CLIENT).returncode != 0:
            sys.exit("depot: the build failed: nothing staged")
    binary = built_binary(platform)
    if not binary.is_file():
        sys.exit(f"depot: {binary} is not there. Drop --no-build, or build it first.")
    libs, lib_why = needed_libs(binary, platform)

    stage = out / ".staging"
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir(parents=True)
    dest = stage / spec["exec"]
    shutil.copy2(binary, dest)

    runtime = runtime_dlls(binary, platform)
    for src in runtime:
        shutil.copy2(src, stage / src.name)
    if runtime:
        shutil.copy2(ROOT / "ci" / "licenses" / "MINGW-RUNTIME.txt", stage / "LICENSE-MINGW-RUNTIME.txt")
        print(f"   staged {len(runtime)} runtime DLL(s): {', '.join(p.name for p in runtime)}")

    # Bevy reads assets/ beside the executable. Only what git tracks: a
    # stray render or download in the folder never ships.
    rels = [r for r in sh("git", "ls-files", "-z", "--", "crates/client/assets").split("\0") if r]
    if not rels:
        sys.exit("depot: git tracks no crates/client/assets: the client would draw nothing")
    for rel in rels:
        src = ROOT / rel
        if not src.is_file():
            sys.exit(f"depot: {rel} is tracked but not on disk")
        dst = stage / "assets" / Path(rel).relative_to("crates/client/assets")
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
    print(f"   staged {len(rels)} asset file(s)")
    shutil.copy2(ROOT / "NOTICE", stage / "NOTICE")
    shutil.copy2(CLIENT / "DESKTOP.md", stage / "README.md")

    if shutil.which(spec["strip"]):
        before = dest.stat().st_size
        subprocess.run([spec["strip"], str(dest)], check=False)
        print(f"   stripped {before:,} -> {dest.stat().st_size:,} bytes")
    else:
        print(f"   NOT stripped: {spec['strip']} is not here")
    dest.chmod(0o755)

    bundled = sorted(p.name for p in runtime)
    shipped = {b.lower() for b in bundled}
    requires = {
        "libs": [lib for lib in libs if lib.lower() not in shipped],
        "method": spec["method"],
        "complete": False,
        "measured": not lib_why,
        "why": lib_why or ("loaded from the player's machine at start; link-time entries only "
                           f"(opened at runtime and not listed: {spec['dlopen']})"),
    }
    if bundled:
        requires["bundled"] = bundled
    return stage, requires


def self_test() -> int:
    tmp = Path(tempfile.mkdtemp(prefix="oneshot-depot-"))
    stage = tmp / "s"
    (stage / "assets" / "fonts").mkdir(parents=True)
    (stage / "oneshot").write_bytes(b"\x7fELF\n")
    (stage / "assets" / "fonts" / "a.otf").write_bytes(b"font")
    doc = build_depot_doc(stage, "0.1.0-gdeadbeef", DEFAULT_ROOT, platform="linux-x86_64")
    checks = [
        (doc["slug"] == SLUG and doc["platform"] == "linux-x86_64", "slug and platform"),
        ([f["path"] for f in doc["files"]] == ["assets/fonts/a.otf", "oneshot"], "relative posix paths, sorted"),
        ([f["executable"] for f in doc["files"]] == [False, True], "only the launch binary is executable"),
        (doc["files"][1]["sha256"] == hashlib.sha256(b"\x7fELF\n").hexdigest(), "the real bytes' sha256"),
        (doc["root"] == f"https://elopros.com/api/launcher/depot/{SLUG}/0.1.0-gdeadbeef/linux-x86_64/files",
         "the root names this build and platform"),
        (set(re.findall(r"\{([a-z_]+)\}", " ".join(LAUNCH_ARGS))) <= {"server", "wallet", "build_dir", "host", "servers"},
         "every placeholder is one the launcher fills"),
        (set(TARGETS) <= {"any", "linux-x86_64", "linux-aarch64", "win-x86_64", "mac-arm64", "mac-x86_64"},
         "platform tags in the launcher's vocabulary"),
        (TARGETS["win-x86_64"]["exec"].endswith(".exe"), "Windows launches a .exe"),
        (re.match(r"^\d+\.\d+\.\d+-g[0-9a-f]+(-dirty\.([0-9a-f]{8}|unknown))?$", build_id()) is not None, "build id shape"),
        (json.loads(json.dumps(doc)) == doc, "round-trips through JSON"),
        ((ROOT / "NOTICE").is_file() and (CLIENT / "DESKTOP.md").is_file()
         and (ROOT / "ci" / "licenses" / "MINGW-RUNTIME.txt").is_file(), "the notices to stage are there"),
    ]
    for bad in ("../x", "/abs", "a//b", "./a", "a\\b", ""):
        try:
            safe_relpath(bad)
            checks.append((False, f"refuses {bad!r}"))
        except SystemExit:
            checks.append((True, f"refuses {bad!r}"))
    shutil.rmtree(tmp, ignore_errors=True)
    for ok, what in checks:
        print(("  ok  " if ok else "  FAIL ") + what)
    bad = [w for ok, w in checks if not ok]
    print(f"{'FAIL' if bad else 'PASS'}: {len(checks) - len(bad)}/{len(checks)}")
    return 1 if bad else 0


def depot_dir(platform: str, build: str, out: Path | None = None) -> Path:
    return (out or ROOT / "target" / "depot") / platform / build


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--platform", default="linux-x86_64", choices=sorted(TARGETS))
    ap.add_argument("--root", default=DEFAULT_ROOT, help="where the launcher fetches from (in the digest)")
    ap.add_argument("--out", default=str(ROOT / "target" / "depot"))
    ap.add_argument("--no-build", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    out = Path(a.out) / a.platform
    print(f"== depot {SLUG} ({a.platform})")
    stage, requires = stage_build(out, a.platform, not a.no_build)
    build = build_id(stage / TARGETS[a.platform]["exec"])
    final = out / build
    if final.exists():
        shutil.rmtree(final)
    stage.rename(final)
    doc = build_depot_doc(final, build, a.root, platform=a.platform, requires=requires)
    (final / "depot.json").write_text(json.dumps(doc, indent=2) + "\n")
    total = sum(f["bytes"] for f in doc["files"])
    print(f"   build {build}: {len(doc['files'])} files, {total:,} bytes")
    print(f"   {final / 'depot.json'}")
    print(f"   needs from the machine: {', '.join(requires['libs']) or 'nothing measured'}")
    if shutil.which("elo"):
        print(f"   digest {sh('elo', 'digest', str(final / 'depot.json'), check=False)}")
    print(f"   publish: ci/publish_depot.py --platform {a.platform} --no-build")
    return 0


if __name__ == "__main__":
    sys.exit(main())

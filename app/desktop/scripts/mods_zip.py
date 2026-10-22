"""Packs the mods zips release.sh signs (signature.rs explains the two layouts):

  v1  shroudtopia-mods.zip     shroudtopia.dll + winmm.dll + mods/<name>.dll: what apps up to 0.1.5 accept, nothing more
  v2  shroudtopia-mods-v2.zip  v1 + EML's dbghelp.dll (eml-mods/eml.lock) + mods/minimap_mod/ + mods/max-stack/ + mods/twitch-drops/

    python scripts/mods_zip.py --dlls <folder> [--v1 <zip>] [--v2 <zip>] [--install <game folder>]

<folder> is MSBuild's output (build.bat: build/x64): the loader, the mods' DLLs at its top or under mods/, and
minimap_mod.dll at its top, under mods/ or under mods/minimap_mod/. The minimap's data comes from the submodule
(third-party/Enshrouded-minimap/assets), Max Stack and Twitch Drops from eml-mods/. EML is downloaded once into
~/.shroudtopia/cache and used only if its size and sha256 are the ones eml.lock names.

--install unpacks the v2 zip into a game folder, for trying a build in the game before any release: the files it
replaces are copied to <game folder>/shroudtopia-backup-<time>/ first, and shroudtopia.json is never touched.
"""

import argparse
import hashlib
import os
import shutil
import sys
import time
import urllib.request
import zipfile

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
LOADER = ["shroudtopia.dll", "winmm.dll"]
MODS = ["basics_mod.dll", "flight_mod.dll", "first_person_view.dll"]
MINIMAP_ASSETS = os.path.join(REPO, "third-party", "Enshrouded-minimap", "assets")
EML_MODS = os.path.join(REPO, "eml-mods")
EML_LOCK = os.path.join(EML_MODS, "eml.lock")
# the Lua mods under eml-mods/ that ship
EML_MOD_FOLDERS = ["max-stack", "twitch-drops"]


def stop(msg):
    sys.exit("!! " + msg)


def find(src, name, *subdirs):
    for d in ("",) + subdirs:
        p = os.path.join(src, d, name)
        if os.path.isfile(p):
            return p
    stop(f"{name} not found in {src}")


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def eml_dll():
    lock = dict(line.split(" ", 1) for line in open(EML_LOCK).read().split("\n") if line.strip())
    url, size, sha = lock["url"].strip(), int(lock["size"]), lock["sha256"].strip()
    cache = os.path.join(os.path.expanduser("~"), ".shroudtopia", "cache", "eml-" + sha[:16], "dbghelp.dll")
    if not (os.path.isfile(cache) and os.path.getsize(cache) == size and sha256(cache) == sha):
        os.makedirs(os.path.dirname(cache), exist_ok=True)
        print("    downloading", url)
        part = cache + ".part"
        urllib.request.urlretrieve(url, part)
        if os.path.getsize(part) != size or sha256(part) != sha:
            os.remove(part)
            stop(f"{url} is not the file eml.lock names (size/sha256 differ)")
        os.replace(part, cache)
    return cache


def tree(folder, prefix):
    """(source, path in the zip) of every file under folder; build leftovers are left out."""
    out = []
    for root, dirs, files in os.walk(folder):
        dirs[:] = sorted(d for d in dirs if not d.startswith("."))
        for f in sorted(files):
            if f.startswith(".") or f.lower().endswith((".pdb", ".exp", ".lib", ".ilk")):
                continue
            src = os.path.join(root, f)
            out.append((src, prefix + "/" + os.path.relpath(src, folder).replace(os.sep, "/")))
    return out


def v1_files(dlls):
    return [(find(dlls, n), n) for n in LOADER] + [(find(dlls, n, "mods"), "mods/" + n) for n in MODS]


def v2_files(dlls):
    files = v1_files(dlls) + [(eml_dll(), "dbghelp.dll")]
    if not os.path.isfile(os.path.join(MINIMAP_ASSETS, "mod.json")):
        stop("third-party/Enshrouded-minimap is empty: git submodule update --init third-party/Enshrouded-minimap")
    files.append((find(dlls, "minimap_mod.dll", "mods", os.path.join("mods", "minimap_mod")), "mods/minimap_mod/minimap_mod.dll"))
    files += [f for f in tree(MINIMAP_ASSETS, "mods/minimap_mod") if f[1] != "mods/minimap_mod/minimap_mod.dll"]
    for m in EML_MOD_FOLDERS:
        files += tree(os.path.join(EML_MODS, m), "mods/" + m)
    return files


def write(path, files):
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for src, name in files:
            z.write(src, name)
    print(f"    {os.path.basename(path)}: {len(files)} files, {os.path.getsize(path) // 1024} KiB")


def install(zip_path, game):
    if not os.path.isfile(os.path.join(game, "enshrouded.exe")):
        stop(f"{game} has no enshrouded.exe")
    backup = os.path.join(game, "shroudtopia-backup-" + time.strftime("%Y%m%d-%H%M%S"))
    with zipfile.ZipFile(zip_path) as z:
        for name in z.namelist():
            dest = os.path.join(game, *name.split("/"))
            if os.path.isfile(dest):
                keep = os.path.join(backup, *name.split("/"))
                os.makedirs(os.path.dirname(keep), exist_ok=True)
                shutil.copy2(dest, keep)
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            with z.open(name) as src, open(dest, "wb") as out:
                shutil.copyfileobj(src, out)
    print(f"    installed into {game}" + (f" (replaced files kept in {backup})" if os.path.isdir(backup) else ""))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dlls", required=True)
    ap.add_argument("--v1")
    ap.add_argument("--v2")
    ap.add_argument("--install")
    a = ap.parse_args()
    if a.v1:
        write(a.v1, v1_files(a.dlls))
    v2 = a.v2 or (os.path.join(os.path.expanduser("~"), ".shroudtopia", "cache", "local-mods-v2.zip") if a.install else None)
    if v2:
        write(v2, v2_files(a.dlls))
    if a.install:
        install(v2, a.install)


main()

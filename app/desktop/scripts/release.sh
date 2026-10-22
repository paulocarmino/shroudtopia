#!/usr/bin/env bash
# Packs and signs a Shroudtopia release (the files the app's updater takes from GitHub Releases). It does NOT publish:
# at the end it prints the `gh release create` command for Paulo to run himself.
#
#   bash app/desktop/scripts/release.sh --new-key
#       makes the Ed25519 key pair once: the private key at $SHROUDTOPIA_RELEASE_KEY
#       (default %USERPROFILE%\.shroudtopia\release-signing.key, outside every repository; never printed, never sent)
#       and its public half in app/desktop/release-signing.pub, which is compiled into the app (signature.rs).
#       A key that exists is never replaced: a new key is a new app for everyone.
#
#   bash app/desktop/scripts/release.sh <version> --dlls <folder> [--exe <Shroudtopia.exe>] [--notes <file>]
#       <folder> has the loader and the mods as MSBuild left them (build.bat: build/x64): shroudtopia.dll, winmm.dll,
#       the mods' DLLs (basics_mod.dll, flight_mod.dll, first_person_view.dll) and minimap_mod.dll, at its top or
#       under mods/ (scripts/mods_zip.py adds the minimap's data, EML, Max Stack and Twitch Drops).
#       --exe defaults to app/desktop/dist/Shroudtopia.exe (scripts/build.sh). Writes app/desktop/release/<version>/:
#         shroudtopia-mods.zip         v1: shroudtopia.dll + winmm.dll + mods/*.dll, for apps up to 0.1.5
#         shroudtopia-release.json     v1 manifest { payload, signature }, signed with the private key
#         shroudtopia-mods-v2.zip      v2: v1 + dbghelp.dll (EML) + mods/minimap_mod/ + mods/max-stack/ + mods/twitch-drops/
#         shroudtopia-release-v2.json  v2 manifest (signature.rs), what this app reads
#         Shroudtopia.exe
#         Shroudtopia.zip              the exe zipped, for the landing page's download button
#       and checks both signatures with the public key before it says it is done.
#
# Needs bash, openssl 3 (Ed25519, `pkeyutl -rawin`) and python (to write the zip and the JSON).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DESKTOP="$(cd "$HERE/.." && pwd)"
KEY="${SHROUDTOPIA_RELEASE_KEY:-$HOME/.shroudtopia/release-signing.key}"
PUB="$DESKTOP/release-signing.pub"
# the first python that runs (on Windows `python3` may be the Microsoft Store stub)
PY=""
for candidate in python3 python py; do
    if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c "import zipfile" >/dev/null 2>&1; then PY="$candidate"; break; fi
done

stop() { echo "!! $*" >&2; exit 1; }
pub_of() { openssl pkey -in "$1" -pubout -outform DER | tail -c 32 | base64 | tr -d '\n'; }

if [ "${1:-}" = "--new-key" ]; then
    [ -f "$KEY" ] && stop "There is a key already at $KEY. It is not replaced: a new key is a new app for everyone."
    mkdir -p "$(dirname "$KEY")"
    (umask 077 && openssl genpkey -algorithm ed25519 -out "$KEY")
    if command -v icacls >/dev/null 2>&1; then icacls "$(cygpath -w "$KEY" 2>/dev/null || echo "$KEY")" /inheritance:r /grant:r "${USERNAME:-$USER}:F" >/dev/null 2>&1 || true; fi
    pub_of "$KEY" > "$PUB"
    echo "The private key is at $KEY. Keep a copy somewhere safe: without it no release can be signed."
    echo "The public key is in app/desktop/release-signing.pub: $(cat "$PUB")"
    exit 0
fi

VERSION="${1:-}"
shift || true
echo "$VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || stop "Usage: release.sh <version x.y.z> --dlls <folder> [--exe file] [--notes file]  (or --new-key)"
DLLS=""
EXE="$DESKTOP/dist/Shroudtopia.exe"
NOTES=""
while [ $# -gt 0 ]; do
    case "$1" in
        --dlls) shift; DLLS="${1:-}" ;;
        --exe) shift; EXE="${1:-}" ;;
        --notes) shift; NOTES="${1:-}" ;;
        *) stop "unknown option $1" ;;
    esac
    shift
done
[ -n "$PY" ] || stop "python is needed to write the zip"
[ -f "$KEY" ] || stop "No private key at $KEY (SHROUDTOPIA_RELEASE_KEY names another). Signing is done on the PC that has it."
[ -f "$PUB" ] || stop "No app/desktop/release-signing.pub"
[ "$(pub_of "$KEY")" = "$(tr -d '\n\r' < "$PUB")" ] || stop "The key at $KEY is not the one release-signing.pub names (the one the apps have). Nothing was signed."
[ -d "$DLLS" ] || stop "--dlls <folder with shroudtopia.dll, winmm.dll and the mods>"
[ -f "$EXE" ] || stop "No $EXE (run scripts/build.sh first)"
APP_VERSION="$(grep -m1 '^version' "$DESKTOP/src-tauri/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')"

OUT="$DESKTOP/release/$VERSION"
rm -rf "$OUT"
mkdir -p "$OUT"
"$PY" "$HERE/mods_zip.py" --dlls "$DLLS" --v1 "$OUT/shroudtopia-mods.zip" --v2 "$OUT/shroudtopia-mods-v2.zip"
cp "$EXE" "$OUT/Shroudtopia.exe"
# what the landing page hands out: the exe inside a zip (browsers and Windows treat a bare .exe download harshly).
# The updater keeps using the signed Shroudtopia.exe; this zip is not part of the signed manifest.
"$PY" - "$OUT" <<'PYEOF'
import os, sys, zipfile
out = sys.argv[1]
with zipfile.ZipFile(os.path.join(out, "Shroudtopia.zip"), "w", zipfile.ZIP_DEFLATED) as z:
    z.write(os.path.join(out, "Shroudtopia.exe"), "Shroudtopia.exe")
PYEOF

line() { printf 'file %s %s %s\n' "$1" "$(wc -c < "$OUT/$1" | tr -d ' ')" "$(openssl dgst -sha256 -r "$OUT/$1" | cut -d' ' -f1)"; }
base64 -d < "$PUB" > "$OUT/pub.raw"
"$PY" - "$OUT/pub.raw" "$OUT/pub.der" <<'PYEOF'
import sys
raw = open(sys.argv[1], "rb").read()
open(sys.argv[2], "wb").write(bytes.fromhex("302a300506032b6570032100") + raw)
PYEOF
# sign <header> <mods zip> <manifest>: the manifest signature.rs reads, checked with the public half as the app will
sign() {
    {
        printf '%s\nversion %s\napp %s\n' "$1" "$VERSION" "$APP_VERSION"
        line Shroudtopia.exe
        line "$2"
    } > "$OUT/payload.txt"
    openssl pkeyutl -sign -rawin -inkey "$KEY" -in "$OUT/payload.txt" -out "$OUT/signature.bin"
    "$PY" - "$OUT" "$3" <<'PYEOF'
import base64, json, os, sys
out, name = sys.argv[1], sys.argv[2]
payload = open(os.path.join(out, "payload.txt"), "rb").read().decode("utf-8")
sig = base64.b64encode(open(os.path.join(out, "signature.bin"), "rb").read()).decode()
json.dump({"payload": payload, "signature": sig}, open(os.path.join(out, name), "w"), indent=2)
PYEOF
    openssl pkeyutl -verify -rawin -pubin -keyform DER -inkey "$OUT/pub.der" -in "$OUT/payload.txt" -sigfile "$OUT/signature.bin" >/dev/null || stop "The $3 just signed does not verify."
}
sign "shroudtopia release v1" shroudtopia-mods.zip shroudtopia-release.json
sign "shroudtopia release v2" shroudtopia-mods-v2.zip shroudtopia-release-v2.json
rm -f "$OUT/payload.txt" "$OUT/signature.bin" "$OUT/pub.raw" "$OUT/pub.der"

echo "==> Signed release $VERSION (app $APP_VERSION) in app/desktop/release/$VERSION:"
ls -l "$OUT"
echo
echo "Not published. To publish it (Paulo, by hand):"
echo "  gh release create v$VERSION --repo paulocarmino/shroudtopia ${NOTES:+--notes-file \"$NOTES\"} \\"
echo "     \"$OUT/shroudtopia-mods.zip\" \"$OUT/shroudtopia-release.json\" \"$OUT/shroudtopia-mods-v2.zip\" \"$OUT/shroudtopia-release-v2.json\" \\"
echo "     \"$OUT/Shroudtopia.exe\" \"$OUT/Shroudtopia.zip\""

#!/usr/bin/env bash
# Builds Shroudtopia.exe for Windows, portable and unsigned, into app/desktop/dist/Shroudtopia.exe.
#
#   1) the Svelte frontend          -> build/          (Node, inside the container)
#   2) cargo test of the core       (Linux, inside the container; --no-test skips it)
#   3) the Rust side (core + Tauri) -> .exe            (cross-compiled with cargo-xwin, as OrangeMacros)
#
# Usage:   bash app/desktop/scripts/build.sh [--no-test] [--test-only] [--icons]
#   --test-only   only step 2 (cargo test of the core and the Windows glue's portable parts)
#   --icons       regenerate src-tauri/icons from the repository's logo_square.png first
#
# It runs from Git Bash (it re-runs itself inside WSL, where Docker is) or from WSL's bash. Nothing is installed on
# Windows. Building on /mnt/c is slow, so the sources are copied (rsync) to a WSL-native folder first
# ($SHROUDTOPIA_BUILD_DIR, default ~/.cache/shroudtopia-build); cargo's target folder, the registry, node_modules and
# the Windows SDK live in Docker volumes / that folder and are reused by every later run. The lock files the build
# makes are copied back next to the sources.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DESKTOP="$(cd "$HERE/.." && pwd)"
APP="$(cd "$DESKTOP/.." && pwd)"
REPO="$(cd "$APP/.." && pwd)"

case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
        echo "==> Git Bash: continuing inside WSL..."
        WSL_SCRIPT="$(MSYS_NO_PATHCONV=1 wsl.exe -- wslpath -a "$(cygpath -m "$HERE/build.sh")" | tr -d '\r')"
        MSYS_NO_PATHCONV=1 exec wsl.exe -- bash -l "$WSL_SCRIPT" "$@"
        ;;
esac

TEST=1
EXE=1
ICONS=0
while [ $# -gt 0 ]; do
    case "$1" in
        --no-test) TEST=0 ;;
        --test-only) EXE=0 ;;
        --icons) ICONS=1 ;;
        *) echo "unknown option: $1 (known: --no-test, --test-only, --icons)" >&2; exit 2 ;;
    esac
    shift
done

IMAGE="shroudtopia-tauri-xwin"
TARGET="x86_64-pc-windows-msvc"
STAGE="${SHROUDTOPIA_BUILD_DIR:-$HOME/.cache/shroudtopia-build}"

command -v docker >/dev/null 2>&1 || { echo "!! Docker was not found (it lives in WSL on Paulo's PC)." >&2; exit 1; }

echo "==> [0/3] Copying the sources to $STAGE ..."
mkdir -p "$STAGE/app/desktop" "$STAGE/out"
# node_modules, .svelte-kit and build stay in the stage between runs (excluded, so --delete leaves them alone)
rsync -a --delete \
    --exclude node_modules --exclude .svelte-kit --exclude /build --exclude /dist --exclude target --exclude /src-tauri/gen \
    "$DESKTOP/" "$STAGE/app/desktop/"
# the contract the core's tests read (crates/shroudtopia-core reaches ../../../contract.example.json)
cp "$APP/contract.example.json" "$APP/CONTRACT.md" "$STAGE/app/"
if [ "$ICONS" = 1 ]; then cp "$REPO/logo_square.png" "$STAGE/logo_square.png"; fi

echo "==> [1/3] Building the toolchain image (cached after the first time)..."
docker build -f "$DESKTOP/docker/tauri-xwin.Dockerfile" -t "$IMAGE" "$DESKTOP/docker"

echo "==> [2/3] Frontend, tests and cross-compile in the container..."
rm -f "$STAGE/out/Shroudtopia.exe"
docker run --rm \
    -v "$STAGE":/work -w /work/app/desktop \
    -v shroudtopia-cargo-registry:/usr/local/cargo/registry \
    -v orange-xwin-cache:/xwin \
    -v shroudtopia-target:/target \
    -e XWIN_CACHE_DIR=/xwin \
    -e CARGO_TARGET_DIR=/target \
    -e TEST="$TEST" -e EXE="$EXE" -e ICONS="$ICONS" -e TARGET="$TARGET" -e OWNER="$(id -u):$(id -g)" \
    "$IMAGE" \
    bash -c 'set -euo pipefail
        # what the container writes in the stage goes back to the WSL user, so the next rsync can update it
        trap "chown -R \"\$OWNER\" /work" EXIT
        if [ "$ICONS" = 1 ]; then
            echo "==> icons from logo_square.png"
            (cd src-tauri && cargo tauri icon /work/logo_square.png -o icons >/dev/null)
            rm -rf src-tauri/icons/android src-tauri/icons/ios
        fi
        if [ "$TEST" = 1 ]; then
            echo "==> cargo test (core + win glue, Linux)"
            cargo test --workspace
        fi
        if [ "$EXE" = 1 ]; then
            echo "==> frontend (vite build)"
            if [ -f package-lock.json ]; then npm ci --no-audit --no-fund; else npm install --no-audit --no-fund; fi
            npm run build
            echo "==> cargo tauri build ($TARGET)"
            (cd src-tauri && cargo tauri build --runner cargo-xwin --target "$TARGET" --no-bundle)
            cp "/target/$TARGET/release/Shroudtopia.exe" /work/out/Shroudtopia.exe
        fi'

# the lock files made in the stage go back next to the sources (they are committed)
for f in Cargo.lock src-tauri/Cargo.lock package-lock.json; do
    if [ -f "$STAGE/app/desktop/$f" ]; then cp "$STAGE/app/desktop/$f" "$DESKTOP/$f"; fi
done
if [ "$ICONS" = 1 ]; then
    rm -rf "$DESKTOP/src-tauri/icons"
    cp -r "$STAGE/app/desktop/src-tauri/icons" "$DESKTOP/src-tauri/icons"
fi

if [ "$EXE" = 1 ]; then
    echo "==> [3/3] Collecting the artifact..."
    mkdir -p "$DESKTOP/dist"
    if cp "$STAGE/out/Shroudtopia.exe" "$DESKTOP/dist/Shroudtopia.exe"; then
        echo "    OK -> app/desktop/dist/Shroudtopia.exe ($(du -h "$DESKTOP/dist/Shroudtopia.exe" | cut -f1))"
    else
        cp "$STAGE/out/Shroudtopia.exe" "$DESKTOP/dist/Shroudtopia.new.exe"
        echo "!! dist/Shroudtopia.exe is in use (the app is running): the new build is dist/Shroudtopia.new.exe" >&2
        exit 3
    fi
fi

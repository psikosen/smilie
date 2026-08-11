#!/usr/bin/env bash
# Download the Pyodide WASM bundle into src-tauri/assets/pyodide/.
#
# Source: https://github.com/pyodide/pyodide/releases (MPL-2.0)
# Pyodide is a CPython runtime compiled to WebAssembly, used by the
# Experiment sandbox iframe / worker (src-tauri/assets/experiment-*).
#
# Run once on a fresh checkout. The bundle is gitignored — CI does not
# need to run this (no Pyodide-backed tests in CI).
#
#   ./scripts/install_pyodide.sh                    # default version
#   PYODIDE_VERSION=0.27.2 ./scripts/install_pyodide.sh
#
# Pin the version so behavior is reproducible across machines. Bump
# in lockstep with the worker.js compatibility comment at the top of
# src-tauri/assets/experiment-worker.js.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$REPO_ROOT/src-tauri/assets/pyodide"
mkdir -p "$DEST"

# Pinned. 0.27.x is the current stable line and ships numpy in -core.
VERSION="${PYODIDE_VERSION:-0.27.2}"

# The "core" distribution is ~30MB compressed and still includes numpy +
# micropip, which is everything the experiment runner pre-loads. The
# "full" distribution adds scipy/pandas/etc. and is ~200MB — too heavy
# for vendoring. Override with PYODIDE_DIST=full if you need the kitchen
# sink locally.
DIST="${PYODIDE_DIST:-core}"

case "$DIST" in
  core) asset="pyodide-core-${VERSION}.tar.bz2" ;;
  full) asset="pyodide-${VERSION}.tar.bz2" ;;
  *)
    echo "Unknown PYODIDE_DIST=$DIST (expected 'core' or 'full')" >&2
    exit 1
    ;;
esac

url="https://github.com/pyodide/pyodide/releases/download/${VERSION}/${asset}"
echo "↓ $url"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

curl -fL --progress-bar -o "$tmp/archive.tar.bz2" "$url"
tar -xjf "$tmp/archive.tar.bz2" -C "$tmp"

# Archive layout: extracts to a top-level "pyodide/" directory that
# contains pyodide.js, pyodide.asm.wasm, pyodide-lock.json, and the
# packages/ tree. We flatten that one level into our DEST.
src="$tmp/pyodide"
if [[ ! -d "$src" ]]; then
  echo "Archive did not contain a top-level pyodide/ dir — check release layout" >&2
  exit 1
fi

# Wipe the old install so a version downgrade can't leave stale .whl files.
rm -rf "$DEST"
mkdir -p "$DEST"
cp -R "$src/." "$DEST/"

# Sanity check — these are the entry-point files the worker imports.
for f in pyodide.js pyodide.asm.wasm pyodide-lock.json; do
  if [[ ! -f "$DEST/$f" ]]; then
    echo "Missing $DEST/$f after extract — install likely broken" >&2
    exit 1
  fi
done

echo "✓ installed Pyodide $VERSION ($DIST) into $DEST"
echo ""
echo "  Files: $(find "$DEST" -type f | wc -l | tr -d ' ')"
echo "  Size:  $(du -sh "$DEST" | cut -f1)"

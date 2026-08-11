#!/usr/bin/env bash
# Download libpdfium for the host platform into assets/pdfium/.
#
# Source: https://github.com/bblanchon/pdfium-binaries (Apache-2.0)
# Invoked on first run or whenever you need to refresh the binary.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$REPO_ROOT/assets/pdfium"
mkdir -p "$DEST"

# Version tag — pin so CI builds are reproducible. Update as needed.
VERSION="${PDFIUM_VERSION:-chromium/6996}"

os="$(uname -s)"
arch="$(uname -m)"

case "$os/$arch" in
  Darwin/arm64)  asset="pdfium-mac-arm64.tgz";     lib="lib/libpdfium.dylib";  out="libpdfium.dylib" ;;
  Darwin/x86_64) asset="pdfium-mac-x64.tgz";       lib="lib/libpdfium.dylib";  out="libpdfium.dylib" ;;
  Linux/x86_64)  asset="pdfium-linux-x64.tgz";     lib="lib/libpdfium.so";     out="libpdfium.so" ;;
  Linux/aarch64) asset="pdfium-linux-arm64.tgz";   lib="lib/libpdfium.so";     out="libpdfium.so" ;;
  *)
    echo "Unsupported host: $os/$arch" >&2
    echo "Open pdfium-binaries releases and place the dylib at $DEST/ manually." >&2
    exit 1
    ;;
esac

url="https://github.com/bblanchon/pdfium-binaries/releases/download/${VERSION}/${asset}"
echo "↓ $url"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

curl -fL --progress-bar -o "$tmp/archive.tgz" "$url"
tar -xzf "$tmp/archive.tgz" -C "$tmp"

if [[ ! -f "$tmp/$lib" ]]; then
  echo "Archive did not contain $lib — check release layout" >&2
  exit 1
fi

cp -f "$tmp/$lib" "$DEST/$out"
echo "✓ installed $DEST/$out"
echo ""
echo "To use a custom path instead, export:"
echo "  SMILIE_PDFIUM_LIB=$DEST/$out"

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

require_command npm
require_command cargo

if [[ ! -d node_modules ]]; then
  echo "Installing Node dependencies..."
  npm ci
fi

if [[ ! -f assets/pdfium/libpdfium.dylib \
   && ! -f assets/pdfium/libpdfium.so \
   && ! -f assets/pdfium/pdfium.dll ]]; then
  echo "Installing PDFium..."
  ./scripts/install_pdfium.sh
fi

if [[ ! -f src-tauri/assets/pyodide/pyodide.js ]]; then
  echo "Installing Pyodide..."
  ./scripts/install_pyodide.sh
fi

if [[ "$(uname -s)" == "Darwin" ]]; then
  if [[ ! -x .venv/bin/python ]]; then
    python_bin=""
    for candidate in python3.11 python3.12 python3; do
      if command -v "$candidate" >/dev/null 2>&1; then
        python_bin="$candidate"
        break
      fi
    done
    if [[ -z "$python_bin" ]]; then
      echo "Python 3 is required for the macOS MLX sidecar." >&2
      exit 1
    fi
    echo "Creating the Python environment with $python_bin..."
    "$python_bin" -m venv .venv
  fi

  if ! .venv/bin/python -c "import mlx_vlm" >/dev/null 2>&1; then
    echo "Installing MLX sidecar dependencies..."
    .venv/bin/python -m pip install -r src-tauri/python/requirements.txt
  fi
fi

echo "Starting Smilie..."
exec npm run tauri:dev

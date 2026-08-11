# mlx-vlm sidecar

Python sidecar wrapping [Blaizzy/mlx-vlm](https://github.com/Blaizzy/mlx-vlm)
for Apple Silicon macOS. Rust spawns it on demand and speaks
newline-delimited JSON-RPC over stdio. See
`src-tauri/src/runtime/mlx_vlm.rs` for the protocol.

## Install

```bash
python3.11 -m venv .venv
.venv/bin/python -m pip install -r src-tauri/python/requirements.txt
```

Run these commands from the repository root. Keeping `.venv` outside
`src-tauri/` prevents Tauri's development watcher from restarting the app
when Python creates or updates files in the environment.

Then tell the Rust runtime which python to use (optional):

```bash
export SMILIE_PYTHON="$PWD/.venv/bin/python3"
```

For sidecar development and parser tests:

```bash
pip install -r requirements-dev.txt
python -m pytest -q
```

## Protocol

Request:  `{"id": "...", "method": "ocr|reason|generate", "params": {...}}`
Response: `{"id": "...", "ok": true, "result": {...}}`
          or `{"id": "...", "ok": false, "error": "..."}`

Missing weights are reported with an error prefixed by
`missing_weights:<absolute path>` so the Rust runtime can surface the
typed variant to the UI.

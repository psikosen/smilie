# Smilie model assets

Drop weight files into the subdirectories below. The app reads
`manifest.json` at startup and lazily loads each model per role +
platform. Missing weights surface a `MissingWeights` error in the UI
(Settings → Runtime) without crashing the pipeline.

## Layout

```
assets/models/
├── manifest.json                  # edit to point runtimes at your weights
├── ocr/
│   ├── glm-ocr-0.9b/              # mlx-vlm (macOS, BF16 original)
│   ├── glm-ocr-0.9b.gguf          # llama.cpp (Windows/Linux)
│   ├── glm-ocr-0.9b-mobile.gguf   # llama.cpp (iOS/Android)
│   └── mmproj.gguf                # VLM multimodal projector
├── reasoning/
│   ├── reasoning-mlx/             # mlx-vlm (macOS)
│   ├── qwen3.6-35b.gguf           # llama.cpp PC (NVFP4 alt)
│   └── gemma-4-e4b.gguf           # llama.cpp mobile
├── generator/
│   ├── generator-mlx/
│   └── generator.gguf
```

## Runtime options

- `mlx-vlm` — macOS only, Apple Silicon, BF16 via
  [Blaizzy/mlx-vlm](https://github.com/Blaizzy/mlx-vlm). Uses the python
  sidecar at `src-tauri/python/mlx_sidecar/`.
- `llama.cpp` — cross-platform, GGUF, via `llama-cpp-2` Rust bindings.
  Vision models need a paired `mmproj.gguf`.
- `llama-server` — optional desktop fallback running a standalone
  `llama-server` binary over HTTP.

## Editing `manifest.json`

Paths may be relative to the app root (e.g. `assets/models/...`) or
absolute. `mmproj` is only needed for multimodal (OCR) models.

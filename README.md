# Smilie

![CI](https://github.com/psikosen/smilie/actions/workflows/ci.yml/badge.svg)

Scientific paper → interactive learning pages.

Drop a PDF. Smilie's vision model reads it, the reasoning model plans
your lessons, and the generator model builds persistent interactive
pages you can revisit forever.

**Tauri v2 + SvelteKit + TypeScript · Rust core · SQLite · mlx-vlm + llama.cpp**

## Highlights

- Native drag-drop on desktop
- Share-sheet ingest on mobile
- Structured-block page rendering (no model HTML drift)
- Idle-shutdown sidecar reclaims model RAM after 5 min
- FTS5 page search
- Quiz spaced repetition

## Screenshots

![Library](docs/screenshots/library.png)

![Paper detail](docs/screenshots/paper-detail.png)

## Quick start

```bash
# Full local setup and desktop launch
./run-local.sh

# frontend-only (UI preview without ingest)
npm install
npm run dev
```

`run-local.sh` installs missing Node dependencies, PDFium, Pyodide, and
the macOS MLX Python environment before starting Tauri. Model weights remain
local and must be placed as described under [Model weights](#model-weights).

## Docs

- [`docs/plans/2026-04-24-smilie-design.md`](docs/plans/2026-04-24-smilie-design.md) — **design contract, source of truth**
- [`scientific_paper_learning_app_srs.md`](scientific_paper_learning_app_srs.md) — original product spec
- [`assets/models/README.md`](assets/models/README.md) — where to drop model weights
- [`src-tauri/python/README.md`](src-tauri/python/README.md) — mlx-vlm python sidecar

## Documentation

- [`docs/architecture.md`](docs/architecture.md) — system + pipeline diagrams (NEW)
- [`docs/plans/2026-04-24-smilie-design.md`](docs/plans/2026-04-24-smilie-design.md) — design contract
- [`docs/mobile.md`](docs/mobile.md) — iOS + Android setup
- [`docs/security.md`](docs/security.md) — CSP rationale
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — dev setup + conventions (NEW)

## PDF rendering

`pdfium-render` wraps Google's pdfium C++ library, which must be
present as a dynamic library at runtime. `scripts/install_pdfium.sh`
downloads the right binary for your host from
[bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)
into `assets/pdfium/`. Override with `SMILIE_PDFIUM_LIB=/path/to/libpdfium.dylib`.

## Model weights

The runtime layer is fully wired. Drop model files into
`assets/models/{ocr,reasoning,generator}/` per
[`assets/models/manifest.json`](assets/models/manifest.json) and restart.
Missing weights surface as `MissingWeights` errors in Settings → Runtime
so the pipeline halts cleanly with a user hint instead of crashing.

On macOS the app spawns a python sidecar that talks to
[Blaizzy/mlx-vlm](https://github.com/Blaizzy/mlx-vlm) over
newline-delimited JSON-RPC. On Windows/Linux/mobile it uses
`llama-cpp-2` Rust bindings against GGUF files.

## Project layout

```
Smilie/
├── src/                          # SvelteKit frontend
│   ├── lib/
│   │   ├── theme/                # Night-Tokyo tokens + dynamic lighting
│   │   ├── components/           # Library, Dropzone, Sidebar, PageViewer, ProgressPanel, Settings
│   │   ├── stores/               # papers, jobs, paper_detail
│   │   └── ipc/                  # typed wrappers over Tauri invoke/events
│   └── routes/                   # /, /papers/[id], /settings
├── src-tauri/
│   ├── migrations/001_init.sql
│   ├── python/mlx_sidecar/       # mlx-vlm python sidecar (macOS)
│   ├── assets/page-sdk.js        # Smilie Page SDK (injected into every generated page)
│   └── src/
│       ├── db/                   # sqlx queries
│       ├── ingest/               # PDF import + page rendering
│       ├── pipeline/             # 10-stage orchestrator
│       ├── runtime/              # ModelRuntime trait + mlx/llama impls
│       ├── validation.rs
│       └── commands/             # Tauri IPC surface
└── assets/models/                # drop your weights here
```

## Commands

```bash
npm run dev          # vite dev server (browser preview, UI only)
npm run tauri:dev    # full Tauri app (macOS native)
npm run build        # frontend production build
npm run tauri:build  # full desktop bundle
npm run check        # svelte-check
npm run tsc          # tsc --noEmit
(cd src-tauri && cargo check)
(cd src-tauri && cargo test)
```

## Known limitations

Post-MVP work, tracked but not yet shipped:

- Light theme
- Multi-paper compare
- Voice tutor
- Mobile native PDF rendering

## License

TBD.

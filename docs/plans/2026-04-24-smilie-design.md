# Smilie — Design Document

> Scientific Paper → Interactive Learning Pages App
> Date: 2026-04-24 · Status: validated · Owner: Raymond Gonzalez

This is the **single source of truth** for the Smilie build. Every implementation decision below is locked. If something is unclear, re-read this doc before guessing. Update this doc if scope changes.

---

## 1. Product summary

User drops a scientific PDF → vision model (GLM-OCR 0.9B via mlx-vlm on macOS) extracts structure → reasoning model creates a learning plan → generator model produces persistent interactive HTML/CSS/JS learning pages → pages are saved locally and browsable offline forever.

Full spec of behavior lives in `scientific_paper_learning_app_srs.md`. This doc specifies **how we build it**.

---

## 2. Locked technical decisions

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | **Tauri v2** cross-platform (macOS / iOS / Android / Windows / Linux) | One codebase, native shell, SvelteKit frontend, Rust core, Python sidecar where needed |
| 2 | **SvelteKit + TypeScript** frontend | Small bundle, fast compile, strong motion/animation ergonomics for the night-Tokyo UI |
| 3 | **SQLite** via `sqlx` (Rust) | Matches SRS §13; FTS5 for search |
| 4 | **Real AI code day 1, no stubs, no mock JSON** | User preference. Missing weights → pipeline halts with `MissingWeights` error + UI hint |
| 5 | **Hybrid runtime**: `mlx-vlm` python sidecar on macOS · `llama.cpp` (GGUF) via Rust bindings on Windows/Linux/mobile | Matches SRS §3. mlx-vlm for Apple Silicon native; GGUF elsewhere |
| 6 | **Manifest-driven models** (`assets/models/manifest.json`) | SRS §10.1 profiles; user edits JSON to point runtime at weights |
| 7 | **Native drag-drop only** (Tauri `onDragDropEvent`); mobile uses native picker + share-sheet | User explicitly requested native, not HTML5 drag |
| 8 | **No HTML5 drag-drop on desktop** | All file ingress goes through Tauri OS-level event |
| 9 | **Page generation produces real HTML/CSS/JS** via generator model, sandboxed in iframe with `sandbox="allow-scripts"` (no network by default) | SRS §14, §15 |
| 10 | **Pipeline is resumable** across app restarts via `jobs` table | SRS §16.2 |

---

## 3. Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Tauri v2 App                                               │
│                                                             │
│  ┌─────────── Frontend (SvelteKit + TS) ─────────────────┐  │
│  │  Library  ·  Sidebar  ·  Page Viewer  ·  Settings    │  │
│  │  Theme tokens + dynamic lighting                     │  │
│  └──────────────────────▲────────────────────────────────┘  │
│                         │ Tauri invoke / events             │
│  ┌──────────────────────▼────────────────────────────────┐  │
│  │  Rust core                                           │  │
│  │  ingest · pipeline · job_queue · db · runtime trait  │  │
│  └──────────────────────▼────────────────────────────────┘  │
│     ┌─────────────────┬─────────────────┬───────────────┐  │
│     │ MlxVlmSidecar   │ LlamaCppLocal   │ LlamaCppServer│  │
│     │ (macOS, python) │ (mobile + all)  │ (Win/Linux)   │  │
│     └─────────────────┴─────────────────┴───────────────┘  │
│                         │                                  │
│                         ▼                                  │
│              assets/models/manifest.json                  │
└───────────────────────────────────────────────────────────┘
```

**Runtime trait** (Rust, `src-tauri/src/runtime/mod.rs`):

```rust
#[async_trait]
pub trait ModelRuntime: Send + Sync {
    async fn ocr(&self, page_image: PageImage) -> Result<OcrBlocks, RuntimeError>;
    async fn reason(&self, paper: PaperJson) -> Result<LearningPlan, RuntimeError>;
    async fn generate(&self, spec: PageSpec) -> Result<GeneratedPage, RuntimeError>;
    fn status(&self) -> RuntimeStatus; // Ready | MissingWeights { role, expected_path } | Error(String)
}
```

**Selector** — at app init, read `manifest.json`, detect platform, wire the correct impl per role. All three roles can be the same runtime (e.g., mlx-vlm does all three on macOS) or different.

---

## 4. Pipeline stages (SRS §12.1)

| # | Stage | Input | Output | Persistence |
|---|-------|-------|--------|-------------|
| 1 | `import_pdf` | uploaded file path | canonical copy under `app_data/papers/<id>.pdf`, `papers` row | SQLite |
| 2 | `render_pages` | pdf | page PNGs in `app_data/page_images/<id>/page_<n>.png` | filesystem |
| 3 | `ocr_extract` | page images | `ocr_blocks`, `figures`, `tables`, `equations` rows | SQLite |
| 4 | `structure_paper` | ocr blocks | normalized `paper.json` (SRS §4.1 shape) | JSON column + file |
| 5 | `build_learning_plan` | paper.json | `learning_plan.json` (page specs) | JSON column |
| 6 | `generate_pages` | per page spec | HTML/CSS/JS per page | filesystem `app_data/pages/<page_id>/` |
| 7 | `validate_pages` | page files | status `valid`/`failed` | SQLite `pages.validation_status` |
| 8 | `save_pages` | validated pages | finalized, sidebar entry ready | SQLite `pages.status = ready` |
| 9 | `index_pages` | pages | FTS5 row | SQLite `pages_fts` |
| 10 | `complete` | — | job marked done, event emitted | SQLite |

**Events emitted to frontend** (Tauri `emit`):
`pipeline:stage_started`, `:stage_progress`, `:stage_completed`, `:stage_failed`, `:page_ready`.

**Resumability** — on startup, for any job where `status ∈ {running, pending, paused}`, resume from the stage after the last `completed` stage.

---

## 5. Data model

### 5.1 SQLite tables

See `src-tauri/migrations/001_init.sql`. Summary:

- `papers` — id, title, authors (json), file_path, original_filename, page_count, processing_status, timestamps
- `pages` — id, paper_id, title, type, source_sections (json), source_pages (json), html_path, source_json, model_id, prompt_hash, validation_status, status, order_index, timestamps
- `ocr_blocks` — id, paper_id, page_number, block_type, text, bbox (json), confidence, source_model
- `jobs` — id, paper_id, stage, status, progress, current_model, error_message, retry_count, timestamps
- `assets` — id, paper_id, kind, path, meta
- `figures` / `tables_` / `equations` — extracted artifacts
- `settings` — key/value
- `generation_logs` — job_id, ts, level, message
- `pages_fts` — FTS5 virtual table over `pages(title, body)`

Indexes: `pages(paper_id, order_index)`, `jobs(status)`, `ocr_blocks(paper_id, page_number)`.

### 5.2 Filesystem layout

```
<app_data>/
  smilie.db
  papers/<paper_id>.pdf
  page_images/<paper_id>/page_<n>.png
  pages/<page_id>/
    index.html
    style.css
    script.js
    meta.json

<app_dir>/assets/models/
  manifest.json
  ocr/<weights>
  reasoning/<weights>
  generator/<weights>
```

### 5.3 Model manifest schema

```json
{
  "version": 1,
  "platform_profiles": {
    "macos":   { "ocr": "ocr_mlx", "reasoning": "reasoning_mlx", "generator": "reasoning_mlx" },
    "windows": { "ocr": "ocr_llama", "reasoning": "reasoning_llama", "generator": "generator_llama" },
    "linux":   { "ocr": "ocr_llama", "reasoning": "reasoning_llama", "generator": "generator_llama" },
    "ios":     { "ocr": "ocr_llama_mobile", "reasoning": "reasoning_llama_mobile", "generator": "reasoning_llama_mobile" },
    "android": { "ocr": "ocr_llama_mobile", "reasoning": "reasoning_llama_mobile", "generator": "reasoning_llama_mobile" }
  },
  "models": {
    "ocr_mlx":              { "runtime": "mlx-vlm", "path": "assets/models/ocr/glm-ocr-0.9b",      "precision": "bf16" },
    "reasoning_mlx":        { "runtime": "mlx-vlm", "path": "assets/models/reasoning/<TBD>",       "precision": "bf16" },
    "ocr_llama":            { "runtime": "llama.cpp", "path": "assets/models/ocr/glm-ocr-0.9b.gguf", "mmproj": "assets/models/ocr/mmproj.gguf" },
    "reasoning_llama":      { "runtime": "llama.cpp", "path": "assets/models/reasoning/qwen3.6-35b.gguf" },
    "generator_llama":      { "runtime": "llama.cpp", "path": "assets/models/generator/<TBD>.gguf" },
    "ocr_llama_mobile":     { "runtime": "llama.cpp", "path": "assets/models/ocr/glm-ocr-0.9b-mobile.gguf", "mmproj": "assets/models/ocr/mmproj.gguf" },
    "reasoning_llama_mobile": { "runtime": "llama.cpp", "path": "assets/models/reasoning/gemma-4-e4b.gguf" }
  }
}
```

---

## 6. Runtime implementations

### 6.1 `MlxVlmSidecar` (macOS)

- Ships Python sidecar in `src-tauri/python/mlx_sidecar/`
- Entry: `python -m mlx_sidecar.server --manifest <path>` — reads stdin JSON-RPC, writes stdout JSON
- Uses [Blaizzy/mlx-vlm](https://github.com/Blaizzy/mlx-vlm)
- Rust spawns sidecar via `tauri_plugin_shell` or `std::process::Command`, owns its lifecycle, restarts on crash
- Methods: `ocr`, `reason`, `generate` — one request at a time per paper (queue)
- Health check on init: sidecar reports which roles have weights available

### 6.2 `LlamaCppLocal` (all platforms, required for mobile)

- Uses `llama-cpp-2` Rust crate (Apache-2.0), compiled into the app
- Loads GGUF model files directly, no subprocess
- Vision via `mmproj` companion file
- Single `LlamaContext` per model, pooled

### 6.3 `LlamaCppServer` (Windows/Linux desktop, optional)

- Ships `llama-server` prebuilt binary per platform
- HTTP localhost interface, Rust spawns + talks JSON over HTTP
- Fallback when `LlamaCppLocal` has build issues

**v1 scope:** ship all three impls, but selection defaults to `MlxVlmSidecar` on macOS, `LlamaCppLocal` elsewhere.

---

## 7. Page generation

### 7.1 Page specs produced by learning plan

```ts
type PageType =
  | "informational" | "concept" | "formula" | "figure"
  | "table" | "quiz" | "simulation";

interface PageSpec {
  id: string;
  paper_id: string;
  title: string;
  type: PageType;
  source_sections: string[];
  source_pages: number[];
  source_content: string;   // relevant extracted text + equations
  assets?: { figures?: string[]; tables?: string[]; equations?: string[] };
}
```

### 7.2 Generator prompt contract

- System prompt: writes self-contained HTML5 using the Smilie Page SDK (see §7.3). No remote scripts. No network fetches. Must embed `<!-- smilie:source {section_ids} -->` comment.
- User prompt: page spec + constrained output format (`<html>…</html>` only, no markdown fence).
- Output validated (§7.4). On failure, one retry with error feedback appended, then mark page `failed` and move on.

### 7.3 Smilie Page SDK

Small JS module injected into every generated page's iframe (`/page-sdk.js`):

- `Smilie.theme` — injects night-Tokyo CSS vars so pages inherit design
- `Smilie.plot(canvas, {x,y})` — lightweight charting (no external libs)
- `Smilie.katex(el, latex)` — local KaTeX build for formula rendering
- `Smilie.quiz(el, questions)` — quiz widget primitives
- `Smilie.slider(el, {min,max,step,onChange})` — interactive control primitive
- `Smilie.source(id)` — link back to source section in original PDF

Model is told to use these helpers instead of re-implementing. Shrinks prompt and enforces consistency.

### 7.4 Validation (SRS §15)

- HTML parses (use `html5ever` on Rust side)
- No `<script src="http">`, no `fetch(`, no `XMLHttpRequest`, no `import(`  remote URLs — regex + AST scan
- Page has `<title>`, exactly one Smilie page-root element
- References at least one `source_sections` id
- Smoke test: load in headless WebView, check `smilie:ready` event fires within 3s, no thrown errors

---

## 8. Design system — Night-Tokyo

### 8.1 Tokens (`src/lib/theme/tokens.css`)

```css
:root {
  --bg-void:     #07060b;
  --bg-panel:    #0e0b18;
  --bg-elevated: #15101f;
  --bg-glass:    rgba(25,18,42,0.55);

  --violet-100:  #c8a8ff;
  --violet-300:  #9a6bff;
  --violet-500:  #6a2dff;   /* primary */
  --violet-700:  #3d1199;
  --violet-glow: #b580ff;

  --cyan-accent: #4ff0ff;
  --magenta:     #ff3ea5;

  --text-hi:     #f3ecff;
  --text-mid:    #a498c4;
  --text-dim:    #6b5f87;

  --border:      rgba(154,107,255,0.18);
  --shadow-deep: 0 24px 80px -20px rgba(106,45,255,0.45);

  --radius-sm: 6px; --radius-md: 10px; --radius-lg: 16px;
  --dur-fast: 120ms; --dur: 180ms; --dur-slow: 320ms;
  --ease: cubic-bezier(.2,.9,.3,1);
}
```

### 8.2 Effects

- **Metallic panels** — layered gradients (linear + radial highlight)
- **Cursor-follow glow** — `--mx`/`--my` CSS vars, JS listener on app root
- **Shimmer sweep** — animated conic gradient on dropzone + active sidebar item (6s loop)
- **Edge-lit focus** — 1px violet border + outer `box-shadow` glow
- **Scanline grain** — SVG noise overlay, 3% opacity
- **Reduced-motion** — disables shimmer, cursor-follow, parallax; keeps fades

### 8.3 Typography

`Geist Sans` UI, `Geist Mono` code/equations. Bundled locally (`src/lib/fonts/`).

---

## 9. UI screens

1. **Library / Empty state** — center dropzone ("Drop a paper"), below it paper-card grid (thumbnail, title, page count, status dot)
2. **Paper detail** — retractable 280px sidebar (collapses to 48px icon rail), resizer, right full-bleed iframe viewer
3. **Generation progress** — bottom-right slide-in panel: stage list w/ current glowing, per-stage progress, pause/cancel/retry
4. **Settings** — right slide-in: Models / Runtime / Generation / Storage / About
5. **Page viewer chrome** — toolbar: Regenerate · Export · Delete · Source→page N
6. **Mobile** — sidebar becomes bottom-sheet; otherwise same tokens

Sidebar page-type icons: 📄 informational · 💡 concept · Σ formula · 📊 figure · 🔢 table · ? quiz · ▶ simulation.

---

## 10. Tauri integration points

### 10.1 Commands (Rust → exposed to frontend)

- `ingest_pdf(file_path: String) -> PaperId`
- `list_papers() -> Vec<PaperSummary>`
- `get_paper(id: PaperId) -> PaperDetail` (includes pages)
- `get_page(id: PageId) -> PageRecord`
- `regenerate_page(id: PageId) -> ()`
- `delete_paper(id: PaperId) -> ()`
- `pause_job(id: JobId)` / `resume_job(id: JobId)` / `cancel_job(id: JobId)`
- `get_settings() -> Settings`
- `update_settings(patch: SettingsPatch) -> ()`
- `list_runtime_status() -> RuntimeStatusReport` (per role)
- `search_pages(query: String) -> Vec<PageHit>`

### 10.2 Events (Rust → Frontend)

`pipeline:stage_started`, `:stage_progress`, `:stage_completed`, `:stage_failed`, `:page_ready`, `paper:updated`, `runtime:status_changed`.

### 10.3 Native drag-drop

```ts
// frontend
import { getCurrentWebview } from "@tauri-apps/api/webview";
getCurrentWebview().onDragDropEvent((e) => {
  if (e.payload.type === "drop") for (const p of e.payload.paths) invoke("ingest_pdf", { filePath: p });
});
```

Browser HTML5 drag events are **not used**.

---

## 11. Security (SRS §14)

- Generated pages rendered in `<iframe sandbox="allow-scripts">` (no `allow-same-origin`, no `allow-top-navigation`)
- CSP on iframe: `default-src 'self'; script-src 'self' 'unsafe-inline'; connect-src 'none'; img-src 'self' data:`
- Tauri CSP locked down: no remote scripts in main window either
- File ingestion copies PDF into `app_data`; original left untouched
- Model paths whitelist — only paths under `assets/models/` or user-approved dirs

---

## 12. Project layout

```
Smilie/
├── README.md
├── scientific_paper_learning_app_srs.md
├── docs/plans/2026-04-24-smilie-design.md
├── assets/models/
│   └── manifest.json
├── package.json                         # frontend (SvelteKit)
├── svelte.config.js · vite.config.ts · tsconfig.json
├── src/
│   ├── app.html · app.d.ts
│   ├── lib/
│   │   ├── theme/tokens.css · effects.ts
│   │   ├── components/          # Dropzone, Sidebar, PageViewer, ProgressPanel, Settings
│   │   ├── stores/              # papers, jobs, settings (svelte stores)
│   │   ├── ipc/                 # typed wrappers over `invoke` / events
│   │   ├── page-sdk/            # JS SDK injected into iframes
│   │   └── fonts/
│   ├── routes/
│   │   ├── +layout.svelte · +page.svelte   # library
│   │   ├── papers/[id]/+page.svelte        # paper detail
│   │   └── settings/+page.svelte
│   └── styles/global.css
└── src-tauri/
    ├── Cargo.toml · tauri.conf.json · build.rs
    ├── migrations/001_init.sql
    ├── python/mlx_sidecar/           # mlx-vlm python sidecar (macOS)
    └── src/
        ├── main.rs · lib.rs
        ├── commands/                  # tauri command handlers
        ├── db/                        # sqlx models + queries
        ├── ingest/                    # pdf import, page rendering
        ├── pipeline/                  # stage impls + orchestrator
        ├── runtime/                   # ModelRuntime trait + impls
        │   ├── mod.rs
        │   ├── mlx_vlm.rs
        │   ├── llama_cpp_local.rs
        │   └── llama_cpp_server.rs
        ├── validation/                # HTML/CSS/JS validators
        └── events.rs
```

### File size ceilings (per CLAUDE.md)

- Rust services / hooks: **≤ 300 lines**
- Svelte UI components: **≤ 500 lines**
- Split when exceeded.

---

## 13. Build order (tracked as tasks)

1. Scaffold Tauri v2 + SvelteKit + TS project
2. Theme tokens + effects layer
3. SQLite + migrations + `db` module
4. Runtime trait + impls skeleton (no weights yet)
5. Manifest loader + runtime selector
6. PDF ingest + page rendering
7. Pipeline orchestrator + job queue
8. OCR stage (mlx-vlm sidecar, macOS)
9. `LlamaCppLocal` wire-up
10. Learning plan stage
11. Page generation stage + Smilie Page SDK
12. Validation stage + sandboxed viewer
13. Frontend: library + dropzone (native drag)
14. Frontend: sidebar + page viewer + progress panel
15. Frontend: settings screen + manifest editor
16. Mobile adapters (share-sheet intake, bottom-sheet sidebar)
17. Full-text search (FTS5)
18. Export + regenerate + delete flows
19. QA pass + tests for §13.* on CLAUDE.md priorities

---

## 14. Testing policy

Per CLAUDE.md:
- ≥ 60% coverage
- Test priority: pipeline/orchestration → validation → commands → ingest
- `cargo test` for Rust, `vitest` + `@testing-library/svelte` for frontend
- `npx tsc -b --noEmit` before every commit (TS check)

---

## 15. Open items (revisit when relevant)

- Which reasoning/generator model IDs ship by default on macOS? (SRS mentions Qwen3.6-35B for PC; macOS TBD — user to pick)
- GGUF source for GLM-OCR 0.9B (HF repo)
- Mobile AI may need quantization smaller than e4b — benchmark later
- Supplementary PDF import (SRS §11) deferred post-MVP

---

## 16. Non-goals (v1)

- Cloud sync, collaboration, Anki export, citation graph, voice tutor — all SRS §18 post-MVP
- Multi-paper comparison
- Paper recommendation engine

---

*This document is the contract. Implementation deviations must be reflected here first.*

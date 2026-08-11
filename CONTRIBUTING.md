# Contributing to Smilie

Thanks for working on Smilie. This file is the short version of the
"how we work" rules. The long version lives in
[`docs/plans/2026-04-24-smilie-design.md`](docs/plans/2026-04-24-smilie-design.md)
(design contract) and [`docs/architecture.md`](docs/architecture.md)
(system map).

## Dev setup

```bash
# 1. JS deps
npm install

# 2. native PDF rendering library (per host OS)
./scripts/install_pdfium.sh

# 3. run the full Tauri app
npm run tauri:dev
```

`npm run dev` starts the Vite dev server alone for UI-only iteration —
ingest and runtimes are stubbed because there's no Rust backend
attached.

For mobile setup see [`docs/mobile.md`](docs/mobile.md). For the
sidecar's Python deps see
[`src-tauri/python/README.md`](src-tauri/python/README.md).

## File-size caps

From the project [`CLAUDE.md`](CLAUDE.md):

| Kind                              | Hard cap |
| --------------------------------- | -------- |
| Rust hooks / service modules      | **300 lines** |
| Svelte UI components              | **500 lines** |

If you need to exceed the cap, split first: extract pure helpers,
factor by responsibility, then add a thin composition layer wiring the
pieces. One file = one responsibility.

## Adding a new block / page type

The generator never emits raw HTML. It emits structured blocks, and
the Rust renderer owns all styling. A new block type touches **three
files in lockstep**:

1. `src-tauri/src/runtime/types.rs` — add a new `Block::*` variant
   with `#[serde(rename_all = "snake_case")]` field naming.
2. `src-tauri/src/pipeline/render.rs` — add a render arm. Unknown
   variants fall through to a paragraph render so the UI never crashes
   on a malformed block.
3. `src-tauri/python/mlx_sidecar/server.py::_normalize_block` — add
   normalization so sidecar output matches the Rust enum exactly.

Anything skipping any of these three will silently degrade.

## Editing the model manifest

Models are wired by [`assets/models/manifest.json`](assets/models/manifest.json).

1. Drop the weight file(s) into `assets/models/<role>/`.
2. Add or edit the entry in `manifest.json`. `path` may be relative to
   the repo root or absolute. mlx-vlm models point at a directory;
   llama.cpp models point at a `.gguf` file (with optional `mmproj` for
   vision).
3. Restart the app, or use **Settings → Runtime → Reload manifest** to
   re-bind without rebuilding.

Missing weights surface as `RuntimeStatus::MissingWeights` with the
expected path — the pipeline halts cleanly instead of crashing.

## Tests

Target **>= 60% line coverage** across the workspace, never regressing
in CI. Priority order when adding tests:

1. Pipeline orchestrator + per-stage logic
2. Validators (`validation.rs`) and structure heuristics
3. Tauri commands (the IPC surface)
4. Frontend stores + IPC wrappers

Run frontend + backend tests after any feature work larger than a
one-liner.

## Commits & PRs

- Short, imperative subject lines (e.g. `pipeline: persist stage on
  cancel`).
- One concern per PR. Refactor PRs separate from feature PRs.
- Reference the design plan section when changing pipeline behavior.
- CI is in [`.github/workflows/ci.yml`](.github/workflows/ci.yml). It
  must pass before merge.

## Local quality gates

Run before pushing:

```bash
npm run lint && npm run check && npm test \
  && cd src-tauri \
  && cargo fmt \
  && cargo clippy --all-targets -- -D warnings \
  && cargo test --lib
```

The Rust step matches what CI runs. `npm run check` is svelte-check;
`npm run lint` runs ESLint over the TS/JS surface.

## Workarounds

If you must add temporary code (compat shim, upstream-bug workaround,
etc.) include in the same diff:

- why it exists,
- what blocks removal,
- the exact deletion condition (version, upstream fix, milestone).

No two competing sources of truth. Legacy code needs a removal plan.

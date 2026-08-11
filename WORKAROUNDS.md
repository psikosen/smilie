# Workarounds

Per `CLAUDE.md` §5, every workaround / compatibility shim must record:

- **Why it exists**
- **What blocks removal**
- **Exact deletion condition** (version, upstream fix, milestone)

Update this file whenever you add or remove a workaround. Resist the urge
to leave "temporary" code without an entry here.

---

## `Shell::open` deprecation in tauri-plugin-shell 2.3.x

- **File:** `src-tauri/src/commands/runtime_info.rs` (around line 55, in
  `reveal_models_folder`).
- **Why:** `tauri_plugin_shell::ShellExt::shell().open(...)` is marked
  deprecated upstream in tauri-plugin-shell 2.3.x. The replacement is the
  separate `tauri-plugin-opener` crate (`OpenerExt::open_path`). Adding a
  new prod dependency was out of scope for the change set that introduced
  this comment.
- **Blocks removal:** Need to add `tauri-plugin-opener` as a Cargo
  dependency and register it in `lib.rs::run` alongside the other
  plugins.
- **Sunset:** When the next dependency bump touches `tauri-plugin-shell`,
  add `tauri-plugin-opener`, swap the `Shell::open` call site to
  `OpenerExt::open_path`, and drop the `#[allow(deprecated)]` attribute.

---

## `assets/pyodide/placeholder/PLACEHOLDER.txt`

- **File:** `src-tauri/assets/pyodide/placeholder/PLACEHOLDER.txt`.
- **Why:** `tauri.conf.json` declares the bundle resource glob
  `assets/pyodide/**/*`. On a fresh checkout `scripts/install_pyodide.sh`
  has not run yet, so the directory is empty and Tauri's bundler errors
  out with "no files matched". The placeholder keeps the glob non-empty
  on developer clones and on CI runners that don't fetch Pyodide.
- **Blocks removal:** Either vendor Pyodide directly into the repo
  (large, license-clean), or move Pyodide assets out of `bundle.resources`
  and serve them at runtime from a non-resource path (download-on-first-use
  with a checksum check).
- **Sunset:** Delete the placeholder when EITHER the install_pyodide.sh
  step is folded into a make-style bootstrap that always runs before
  build, OR Pyodide is moved off `bundle.resources` to a runtime-fetched
  cache path.

---

## Pyodide hardcodes `cdn.jsdelivr.net` for `setCdnUrl` / `micropip.install`

- **Files:** `src-tauri/assets/experiment-worker.js` (Q-owned),
  `src-tauri/assets/pyodide/pyodide.js`.
- **Why:** Pyodide's package loader resolves non-core wheels (anything
  not pinned in `pyodide-lock.json`) by GETting `cdn.jsdelivr.net`.
  The current Tauri CSP `connect-src 'self' ipc: http://ipc.localhost`
  blocks that, so any model-emitted `micropip.install("scipy")` (or
  similar) will fail in the iframe. Core packages (numpy, micropip)
  bundled in the vendored tarball still work because they load from the
  asset protocol, not the CDN.
- **Blocks removal:** Need either a vendored package mirror under
  `assets/pyodide/packages/` that Pyodide is pointed at via
  `pyodide.setCdnUrl(asset_url_to_local_mirror)`, OR a CSP relaxation
  that adds `https://cdn.jsdelivr.net` to `connect-src` (and the
  associated network access acceptance).
- **Sunset:** When we ship a packaged mirror OR explicitly relax CSP
  for Pyodide-only network access (note: not done by default — opens a
  network egress path the rest of the app does not use).

---

## Experiment + comparison pipelines do not flow through `JobQueue`

- **Files:** `src-tauri/src/commands/experiments.rs::spawn_pipeline`,
  `src-tauri/src/experiment/pipeline.rs`,
  `src-tauri/src/commands/comparisons.rs::spawn_pipeline`,
  `src-tauri/src/comparison/pipeline.rs`.
- **Why:** Design doc §2 specifies routing the experiment pipeline (and
  the new two-paper comparison pipeline) through the existing
  `JobQueue` machinery using a synthetic
  `paper_id = format!("experiment:{experiment_id}")` /
  `format!("comparison:{comparison_id}")` so cancel + resume +
  crash-recovery come for free. The `jobs` table in
  `migrations/001_init.sql` has a hard
  `paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE`,
  which rejects synthetic non-paper keys. We use a bare `tokio::spawn`
  for both today; an interrupted experiment is left orphaned in
  `analyzing` / `generating` / `testing`, and an interrupted comparison
  is left in `analyzing`, until the next manual user action.
- **Blocks removal:** Need a migration that lifts the FK constraint on
  `jobs.paper_id` (most likely by replacing it with a typed
  `entity_kind` + `entity_id` pair, or by relaxing the FK to a soft
  reference) so the JobQueue can hold rows whose paper_id is a
  synthetic experiment / comparison key.
- **Sunset:** When a future migration lifts the `jobs.paper_id` FK
  constraint, swap both `spawn_pipeline` call sites to
  `state.jobs.start("experiment:<id>")` /
  `state.jobs.start("comparison:<id>")` and confirm
  `JobQueue::resume_on_startup` resumes them from their last persisted
  FSM state.

---

## `GeneratedPage.html` + `write_page_files` legacy fallback

- **Files:** `src-tauri/src/runtime/types.rs` (`GeneratedPage::html`
  field) and `src-tauri/src/pipeline/stages/generate.rs::write_page_files`
  (the `None => page.html.clone()` branch).
- **Why:** Older runtime impls used to emit free-form HTML directly. The
  pipeline now prefers the structured `blocks` field and renders HTML
  deterministically on the Rust side. The `html` field is still kept on
  the wire so a runtime that only knows the old contract doesn't crash
  the deserializer. Today, every shipping runtime impl
  (`mlx_vlm.rs`, `llama_cpp_local/`, `llama_cpp_server.rs`) sets `html`
  to `String::new()` and always populates `blocks`, so the fallback
  branch is dead in practice.
- **Blocks removal:** Need to confirm there is no in-tree or external
  runtime impl producing non-block output. Once the legacy free-form
  codepath is fully retired (target: when the llama-cpp-2 vision OCR
  work ships), the `html` field and the `None`-branch fallback in
  `write_page_files` can both be deleted.
- **Sunset:** When llama-cpp-2 vision OCR ships and the legacy
  free-form-output path is removed everywhere, drop
  `GeneratedPage.html` plus the `match &page.blocks` `None` arm.

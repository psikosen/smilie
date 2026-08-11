# Smilie — Experiments Design

> Date: 2026-04-26 · Status: validated · Owner: Raymond Gonzalez

Per-paper experimental reproductions: a button on the paper-detail page
hands the paper to the model, which returns a plausibility verdict + a
runnable Python experiment (TDD-loop verified). The user can adjust
input variables, rerun, and refine via prompt — every refine forks a
new versioned copy.

## 1. Stack decisions

- **Sandbox**: Pyodide (Python in WASM) inside a sandboxed iframe, with
  a Web Worker doing the actual interpreter work so the UI thread stays
  responsive. Bundled with the app — no CDN at runtime.
- **TDD loop**: model emits `{tests, code}`, runner executes inside
  Pyodide, on failure feed `{tests, code, errors}` back to the model
  with "fix the code, do not modify the tests" up to **3 retries**.
  Final state stored regardless of pass/fail.
- **Forking**: `parent_experiment_id` + `version` chain; refine prompts
  always create a new row.
- **Performance**:
  - Lazy-load: Pyodide WASM is not fetched until the first experiment
    open in this session.
  - Persistent worker per paper-detail page: load once, reuse for every
    Run + every Refine.
  - Pre-pinned package set: `numpy`, `micropip`, plus on-demand
    `micropip.install()` for anything else the model asks for.
  - Cache the WASM via the iframe's HTTP cache (asset protocol serves
    with a long cache-control once we set it).

## 2. Pipeline (per experiment, separate from per-paper pipeline)

```
analyze_feasibility →   { plausible, score, reasoning, approach }
   │
   ├─ implausible (score < 50) → done. show verdict + reason.
   └─ plausible →
        generate_artifacts → { tests, code, inputs_schema, outputs_schema }
        │
        run_tests_and_refine    [up to 3 attempts]
        │       │
        │       └─ fail → feed errors back, model patches code, run again
        │
        save_experiment → row insert. status = ready (or failed).
```

Stages are tracked in the existing `jobs` table with `paper_id` set to
the synthetic key `experiment:<experiment_id>` so the existing job
queue + cancel/resume machinery works for free.

## 3. Data model — `004_experiments.sql`

```sql
CREATE TABLE experiments (
  id                    TEXT PRIMARY KEY,
  paper_id              TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  parent_experiment_id  TEXT REFERENCES experiments(id) ON DELETE SET NULL,
  version               INTEGER NOT NULL DEFAULT 1,
  title                 TEXT NOT NULL,
  status                TEXT NOT NULL,
  plausibility_score    INTEGER,
  plausibility_reason   TEXT,
  approach_summary      TEXT,
  inputs_schema         TEXT NOT NULL DEFAULT '[]',
  outputs_schema        TEXT NOT NULL DEFAULT '[]',
  python_code           TEXT,
  python_tests          TEXT,
  test_attempts         INTEGER NOT NULL DEFAULT 0,
  test_last_output      TEXT,
  refine_prompt         TEXT,
  model_id              TEXT,
  created_at            INTEGER NOT NULL,
  updated_at            INTEGER NOT NULL
);

CREATE INDEX idx_experiments_paper  ON experiments(paper_id, created_at DESC);
CREATE INDEX idx_experiments_parent ON experiments(parent_experiment_id);

CREATE TABLE experiment_runs (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  experiment_id   TEXT NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
  ts              INTEGER NOT NULL,
  inputs_json     TEXT NOT NULL,
  outputs_json    TEXT,
  error_message   TEXT,
  duration_ms     INTEGER
);

CREATE INDEX idx_experiment_runs_exp_ts ON experiment_runs(experiment_id, ts DESC);
```

Status FSM: `analyzing → implausible | (generating → testing → ready | failed)`.

## 4. Schema for inputs / outputs (model-emitted JSON)

```ts
type InputSpec =
  | { name: string; kind: "number"; default: number; min?: number; max?: number; step?: number; label: string; help?: string }
  | { name: string; kind: "int";    default: number; min?: number; max?: number; step?: number; label: string; help?: string }
  | { name: string; kind: "choice"; default: string; options: string[]; label: string; help?: string }
  | { name: string; kind: "text";   default: string; label: string; help?: string }
  | { name: string; kind: "vector"; default: number[]; length: number; label: string; help?: string };

type OutputSpec =
  | { name: string; kind: "number"; label: string; precision?: number }
  | { name: string; kind: "text";   label: string }
  | { name: string; kind: "table";  label: string; columns: string[] }
  | { name: string; kind: "series"; label: string; x_label?: string; y_label?: string };  // {x:[], y:[]}
```

Variable controls auto-render from `inputs_schema`. Outputs render in a
typed panel: number → big chip, text → preformatted, table → styled
HTML table, series → tiny SVG line chart (we already ship `Smilie.plot`
in the page-SDK; reuse).

## 5. Pyodide runner protocol (host ↔ iframe ↔ worker)

Iframe entry: `src-tauri/assets/experiment-runner.html`. Host parent
(Svelte component) communicates via `postMessage` with the
following messages:

```ts
// Host → iframe
type HostMsg =
  | { type: "init" }
  | { type: "run";       runId: string; code: string; inputs: Record<string, unknown> }
  | { type: "run_tests"; runId: string; code: string; tests: string }
  | { type: "install";   runId: string; package: string };

// Iframe → host
type RunnerMsg =
  | { type: "ready" }                                  // pyodide loaded
  | { type: "loading"; progress: number }              // 0..1 bytes loaded
  | { type: "log";   level: "info"|"warn"|"err"; line: string }
  | { type: "result"; runId: string; outputs: Record<string, unknown> }
  | { type: "tests_result"; runId: string; passed: boolean; report: string }
  | { type: "error"; runId: string; message: string };
```

The worker entry-point pre-loads `numpy` + `micropip` on `init`. `run`
sets `inputs` as a python dict named `INPUTS`, executes `code`, and
collects a python dict named `OUTPUTS` (`outputs[name] = value`).
`run_tests` exec's `code` first, then `tests`, capturing assertion
results. Output values are JSON-serializable Python (numbers, strings,
lists, `np.ndarray` → list).

## 6. IPC surface (Rust → Svelte)

```ts
// Already pre-wired in src/lib/ipc/index.ts before agent dispatch.
ipc.analyzeExperiment(paperId): Promise<ExperimentSummary>           // creates a new "analyzing" row, kicks off pipeline
ipc.listExperimentsForPaper(paperId): Promise<ExperimentSummary[]>
ipc.getExperiment(id): Promise<ExperimentDetail>
ipc.recordExperimentRun(experimentId, inputs, outputs, errorMessage, durationMs): Promise<void>
ipc.listExperimentRuns(experimentId): Promise<ExperimentRun[]>
ipc.refineExperiment(parentId, prompt): Promise<ExperimentSummary>   // creates a new fork, kicks off pipeline
ipc.deleteExperiment(id): Promise<void>
```

Plus pipeline events broadcast on a new `experiment` channel:
`{type: "experiment_status", experiment_id, status, score?, error?}`.

## 7. UI layout

Paper-detail page gains a tab strip above the layout: `Pages` (current
view) | `Experiments`. Switching to Experiments shows:

- **Left rail**: list of experiments for this paper (each row: title +
  version + score chip + status indicator). Tap a row to switch.
- **Right pane**: `ExperimentRunner` for the selected experiment.
  Top: title + plausibility chip + parent-version breadcrumb if forked.
  Body:
  - Sandbox iframe (auto-grows up to 60% height of pane)
  - Variable controls (sliders/inputs auto from `inputs_schema`)
  - Outputs panel (typed renderers)
  - Last run summary + duration
  Bottom toolbar: `▶ Run` · `↻ Last result` · `🗑 Delete` ·
  `✎ Refine with prompt` (opens dialog).

The refine dialog has a textarea, a primary "Fork" button, and an
"Inherit inputs from current" toggle (default on).

## 8. File map

New:
- `src-tauri/migrations/004_experiments.sql`
- `src-tauri/src/experiment/mod.rs` — pipeline stages
- `src-tauri/src/experiment/pipeline.rs` — analyze → generate → tests → save loop
- `src-tauri/src/commands/experiments.rs`
- `src-tauri/python/mlx_sidecar/server.py` — three new methods (`analyze_experiment`, `generate_experiment`, `refine_experiment`)
- `src-tauri/assets/experiment-runner.html`
- `src-tauri/assets/experiment-worker.js`
- `src-tauri/assets/pyodide/` (vendored bundle)
- `src/lib/components/ExperimentTab.svelte`
- `src/lib/components/ExperimentRunner.svelte`
- `src/lib/components/ExperimentVariableControls.svelte`
- `src/lib/components/ExperimentVersionList.svelte`
- `src/lib/components/RefineExperimentDialog.svelte`
- `src/lib/stores/experiments.ts`

Edited:
- `src-tauri/src/db/mod.rs` — register migration 004
- `src-tauri/src/db/queries.rs` — experiment CRUD
- `src-tauri/src/runtime/mod.rs` — extend `ModelRuntime` trait with three new methods
- `src-tauri/src/runtime/mlx_vlm.rs` — RPC wiring
- `src-tauri/src/runtime/llama_cpp_local/mod.rs` — text-only parity
- `src-tauri/src/lib.rs` — register experiment commands + module
- `src/routes/papers/[id]/+page.svelte` — tab strip
- `src/lib/ipc/index.ts` — typed wrappers (pre-wired before dispatch)

## 9. Out of scope (v2+)

- Tree visualization of forks (v1 ships flat list)
- Compare outputs across versions
- Pin a "winning" version
- Export experiment as standalone .ipynb / .py
- Bring-your-own-package allowlist UI (v1: model emits micropip calls in code)
- Server-side / paired-desktop experiment runs for mobile

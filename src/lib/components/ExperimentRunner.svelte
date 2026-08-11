<script lang="ts">
  /**
   * ExperimentRunner — user-facing shell for a single experiment.
   * Owns header, version rail, refine dialog, variable controls,
   * outputs panel, run history, and the run-timeout watchdog. The
   * iframe pane + postMessage broker live in <ExperimentIframeHost>.
   * We hand the host a `pendingRun`; it buffers until ready, dispatches,
   * then reports back via `onResult` / `onError`.
   * Tauri-only paths gated behind `isTauri()`.
   */
  import { onMount, onDestroy } from "svelte";
  import {
    ipc,
    type ExperimentDetail,
    type ExperimentRun,
    type InputSpec,
    type ExperimentSummary,
  } from "$lib/ipc";
  import { isTauri } from "$lib/ipc/tauri-guard";
  import { toast } from "$lib/stores/toast";
  import { experiments } from "$lib/stores/experiments";
  import { resolveRunnerUrl } from "$lib/experiment/runner-url";
  import { startRunTimeout, type RunTimeoutHandle } from "$lib/experiment/run-timeout";
  import ExperimentVariableControls from "./ExperimentVariableControls.svelte";
  import ExperimentOutputs from "./ExperimentOutputs.svelte";
  import ExperimentVersionList from "./ExperimentVersionList.svelte";
  import ExperimentStatusBanner from "./ExperimentStatusBanner.svelte";
  import ExperimentRunHistory from "./ExperimentRunHistory.svelte";
  import ExperimentIframeHost, {
    type PendingRun,
    type PendingTests,
  } from "./ExperimentIframeHost.svelte";
  import RefineExperimentDialog from "./RefineExperimentDialog.svelte";

  interface Props {
    detail: ExperimentDetail;
    /** All summaries for the open paper; used by the version chain rail. */
    summaries: ExperimentSummary[];
    onSelect: (id: string) => void;
  }

  let { detail, summaries, onSelect }: Props = $props();

  // ── Iframe URL resolution (helper, see runner-url.ts). ───────────
  let runnerUrl = $state<string | null>(null);
  let runnerUrlError = $state<string | null>(null);

  // ── Inputs state (auto-seeded from defaults) ─────────────────────
  let inputValues = $state<Record<string, unknown>>({});

  function defaultsFromSchema(schema: InputSpec[]): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    for (const spec of schema) {
      if (spec.kind === "vector") out[spec.name] = spec.default.slice();
      else out[spec.name] = spec.default;
    }
    return out;
  }

  // Re-seed inputs whenever the selected experiment changes. Forks
  // start from model-emitted defaults (no inherit-inputs handoff in v1).
  let lastDetailId = $state<string | null>(null);
  $effect(() => {
    if (detail.id === lastDetailId) return;
    lastDetailId = detail.id;
    inputValues = defaultsFromSchema(detail.inputs_schema);
    lastOutputs = null;
    lastError = null;
    runHistory = [];
    void refreshRunHistory();
  });

  // ── Run history ──────────────────────────────────────────────────
  let runHistory = $state<ExperimentRun[]>([]);

  async function refreshRunHistory() {
    if (!isTauri()) return;
    try {
      const runs = await ipc.listExperimentRuns(detail.id);
      runHistory = runs.slice(0, 5);
    } catch (err) {
      console.error("listExperimentRuns failed", err);
    }
  }

  // ── Run / outputs / dialog state ─────────────────────────────────
  let lastOutputs = $state<Record<string, unknown> | null>(null);
  let lastError = $state<string | null>(null);
  let running = $state(false);
  let approachOpen = $state(false);
  let dialogOpen = $state(false);
  let refineBusy = $state(false);
  let forceTryBusy = $state(false);

  // Setting `pendingRun` fires a dispatch (host buffers until worker
  // `ready`); clearing is a no-op.
  let pendingRun = $state<PendingRun | null>(null);
  let pendingTests = $state<PendingTests | null>(null);
  let pendingRunId = $state<string | null>(null);
  let pendingRunStart = $state(0);
  let iframeReady = $state(false);

  // Watchdog: Pyodide can wedge on pathological code. We post a soft
  // `cancel`, then hard-recreate the iframe (kills the worker) after grace.
  const EXPERIMENT_RUN_TIMEOUT_MS = 60_000;
  const EXPERIMENT_CANCEL_GRACE_MS = 2_000;
  let timeoutHandle: RunTimeoutHandle | null = null;
  let host: ExperimentIframeHost | null = $state(null);

  function clearRunTimers() {
    timeoutHandle?.clear();
    timeoutHandle = null;
  }

  function recordRun(outputs: Record<string, unknown> | null, errorMessage: string | null, durationMs: number) {
    if (!isTauri()) return;
    void ipc
      .recordExperimentRun(detail.id, { ...inputValues }, outputs, errorMessage, durationMs)
      .then(() => refreshRunHistory())
      .catch((err) => console.error("recordExperimentRun failed", err));
  }

  /** Hard-recreate the iframe after a stop/timeout grace window. */
  function hardStop(forRunId: string, message: string) {
    if (pendingRunId !== forRunId) return;
    lastError = message;
    recordRun(null, message, Date.now() - pendingRunStart);
    host?.recreate();
    iframeReady = false;
    pendingRunId = null;
    pendingRun = null;
    running = false;
    clearRunTimers();
    toast.error(message);
  }

  /** Stop the in-flight run; cooperative cancel then hard-recreate after grace. */
  function stopRun(reason: "timeout" | "manual") {
    if (!running || !pendingRunId) return;
    const cancelledRunId = pendingRunId;
    host?.cancel(cancelledRunId);
    clearRunTimers();
    const message =
      reason === "timeout"
        ? `Run timed out after ${EXPERIMENT_RUN_TIMEOUT_MS / 1000}s — restarted runner`
        : "Run cancelled — restarted runner";
    timeoutHandle = startRunTimeout({
      timeoutMs: 0,
      graceMs: EXPERIMENT_CANCEL_GRACE_MS,
      onSoftCancel: () => {},
      onHardRecreate: () => hardStop(cancelledRunId, message),
    });
    toast[reason === "timeout" ? "error" : "info"](
      reason === "timeout" ? "Run timed out — attempting to stop" : "Stopping run…",
    );
  }

  function startRun() {
    if (running) return;
    if (!detail.python_code) {
      toast.error("No code generated yet for this experiment.");
      return;
    }
    if (!isTauri()) {
      toast.error("Run only available in the desktop app.");
      return;
    }
    if (!iframeReady) { toast.info("Sandbox is still loading…"); return; }
    const runId = `run_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;
    pendingRunId = runId;
    pendingRunStart = Date.now();
    running = true;
    lastError = null;
    clearRunTimers();
    timeoutHandle = startRunTimeout({
      timeoutMs: EXPERIMENT_RUN_TIMEOUT_MS,
      graceMs: EXPERIMENT_CANCEL_GRACE_MS,
      // Cooperative stop; iframe is hard-killed by onHardRecreate if wedged.
      onSoftCancel: () => {
        if (!running || pendingRunId !== runId) return;
        host?.cancel(runId);
        toast.error("Run timed out — attempting to stop");
      },
      onHardRecreate: () =>
        hardStop(runId, `Run timed out after ${EXPERIMENT_RUN_TIMEOUT_MS / 1000}s — restarted runner`),
    });
    pendingRun = { runId, code: detail.python_code, inputs: { ...inputValues } };
  }

  // ── Iframe host event handlers ───────────────────────────────────
  function onHostReady() { iframeReady = true; }

  function onHostResult(e: { runId: string; outputs: Record<string, unknown>; durationMs: number }) {
    if (e.runId !== pendingRunId) return;
    clearRunTimers();
    lastOutputs = e.outputs;
    lastError = null;
    running = false;
    pendingRunId = null;
    pendingRun = null;
    recordRun(e.outputs, null, e.durationMs);
  }

  function onHostError(e: { runId: string; message: string; durationMs: number }) {
    // `__worker_crash__` is the iframe's sentinel for fatal crashes.
    if (e.runId !== pendingRunId && e.runId !== "__worker_crash__") return;
    clearRunTimers();
    lastError = e.message;
    running = false;
    pendingRunId = null;
    pendingRun = null;
    toast.error(`Run failed: ${e.message}`);
    recordRun(null, e.message, e.durationMs);
  }

  async function deleteCurrent() {
    if (!isTauri()) return;
    const ok = confirm(`Delete experiment “${detail.title}” v${detail.version}? Run history will be lost.`);
    if (!ok) return;
    try {
      await ipc.deleteExperiment(detail.id);
      experiments.removeFromList(detail.id);
      toast.success("Experiment deleted");
    } catch (err) { toast.error(`Delete failed: ${String(err)}`); }
  }

  async function submitRefine(prompt: string) {
    if (!isTauri()) return;
    refineBusy = true;
    try {
      const summary = await ipc.refineExperiment(detail.id, prompt);
      experiments.upsertSummary(summary);
      onSelect(summary.id);
      toast.success("Refining…");
      dialogOpen = false;
    } catch (err) {
      toast.error(`Refine failed: ${String(err)}`);
    } finally {
      refineBusy = false;
    }
  }

  /** "Try anyway" override — only reachable when status==="implausible"
   *  AND plausibility_score >= 50 (the banner gates the button). The
   *  Rust command repeats the score check defensively. */
  async function forceTry() {
    if (!isTauri() || forceTryBusy) return;
    forceTryBusy = true;
    try {
      const summary = await ipc.forceGenerateExperiment(detail.id);
      experiments.upsertSummary(summary);
      // Refresh the open detail so the banner immediately flips to the
      // generating-stage progress UI.
      void experiments.loadDetail(detail.id);
      toast.success("Generating anyway…");
    } catch (err) {
      toast.error(`Override failed: ${String(err)}`);
    } finally {
      forceTryBusy = false;
    }
  }

  function applyHistoryInputs(run: ExperimentRun) {
    // Defensively merge — refined schemas may have dropped keys.
    const next: Record<string, unknown> = {};
    for (const spec of detail.inputs_schema) {
      next[spec.name] = spec.name in run.inputs ? run.inputs[spec.name] : inputValues[spec.name];
    }
    inputValues = next;
  }

  const parentSummary = $derived(
    detail.parent_experiment_id ? summaries.find((s) => s.id === detail.parent_experiment_id) ?? null : null,
  );
  const canRun = $derived(detail.status === "ready" && Boolean(detail.python_code) && isTauri());

  onMount(() => {
    void resolveRunnerUrl().then((res) => {
      runnerUrl = res.url;
      runnerUrlError = res.error;
    });
  });

  onDestroy(() => {
    clearRunTimers();
  });
</script>

<section class="runner">
  <header class="head">
    <div class="title-row">
      <h2 title={detail.title}>{detail.title}</h2>
      {#if detail.plausibility_score !== null}
        <span class="chip score" title={detail.plausibility_reason ?? ""}>
          Plausibility {detail.plausibility_score}/100
        </span>
      {/if}
      {#if parentSummary}
        <button
          type="button"
          class="chip ghost"
          onclick={() => onSelect(parentSummary.id)}
        >
          ← Forked from v{parentSummary.version}
        </button>
      {/if}
    </div>

    <ExperimentVersionList
      list={summaries}
      selectedId={detail.id}
      onSelect={(id) => onSelect(id)}
    />

    {#if detail.approach_summary}
      <div class="approach">
        <button
          type="button"
          class="approach-toggle"
          onclick={() => (approachOpen = !approachOpen)}
          aria-expanded={approachOpen}
        >
          <span aria-hidden="true">{approachOpen ? "▾" : "▸"}</span>
          How this works
        </button>
        {#if approachOpen}
          <p class="approach-body">{detail.approach_summary}</p>
        {/if}
      </div>
    {/if}
  </header>

  <ExperimentStatusBanner
    {detail}
    onRefine={() => (dialogOpen = true)}
    onForceTry={forceTry}
    {forceTryBusy}
  />

  <ExperimentIframeHost
    bind:this={host}
    {runnerUrl}
    {runnerUrlError}
    {pendingRun}
    {pendingTests}
    visible={canRun}
    onReady={onHostReady}
    onResult={onHostResult}
    onError={onHostError}
  />

  {#if detail.status === "ready"}
    <div class="panel">
      <div class="panel-head">Variables</div>
      <ExperimentVariableControls
        schema={detail.inputs_schema}
        bind:values={inputValues}
      />
    </div>

    <div class="toolbar">
      <button
        type="button"
        class="btn primary"
        disabled={!canRun || running || !iframeReady}
        onclick={startRun}
      >
        {running ? "Running…" : "▶ Run"}
      </button>
      <button
        type="button"
        class="btn ghost"
        disabled={!running}
        onclick={() => stopRun("manual")}
      >
        ■ Stop
      </button>
      <button
        type="button"
        class="btn ghost"
        onclick={() => (dialogOpen = true)}
      >
        ✎ Refine with prompt
      </button>
      <button
        type="button"
        class="btn danger"
        onclick={deleteCurrent}
        aria-label="Delete experiment"
      >
        🗑 Delete
      </button>
    </div>

    <div class="panel">
      <div class="panel-head">Outputs</div>
      <ExperimentOutputs schema={detail.outputs_schema} values={lastOutputs} />
    </div>

    {#if runHistory.length > 0}
      <div class="panel">
        <div class="panel-head">Recent runs</div>
        <ExperimentRunHistory runs={runHistory} onApply={applyHistoryInputs} />
      </div>
    {/if}
  {/if}
</section>

<RefineExperimentDialog
  open={dialogOpen}
  parentTitle={detail.title}
  parentVersion={detail.version}
  busy={refineBusy}
  onSubmit={submitRefine}
  onCancel={() => (dialogOpen = false)}
/>

<style>
  .runner { display: flex; flex-direction: column; gap: 14px; padding: 16px 18px 24px; overflow-y: auto; height: 100%; box-sizing: border-box; }
  .head { display: flex; flex-direction: column; gap: 10px; }
  .title-row { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
  h2 {
    margin: 0; font-size: var(--text-lg); font-weight: 600; letter-spacing: -0.01em;
    color: var(--text-hi); flex: 1; min-width: 0;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .chip {
    padding: 4px 10px; border-radius: var(--radius-pill);
    font-size: var(--text-xs); font-family: var(--font-mono);
    border: 1px solid var(--border); background: var(--bg-elevated); color: var(--text-mid);
  }
  .chip.score { color: var(--violet-100); border-color: var(--border-strong); }
  .chip.ghost { background: transparent; cursor: pointer; }
  .chip.ghost:hover { color: var(--text-hi); border-color: var(--violet-400); }
  .approach { display: flex; flex-direction: column; gap: 6px; }
  .approach-toggle {
    align-self: flex-start; background: transparent; border: 0; padding: 0;
    color: var(--violet-100); font-size: var(--text-xs);
    text-transform: uppercase; letter-spacing: 0.12em; cursor: pointer;
  }
  .approach-body {
    margin: 0; padding: 10px 12px; border-radius: var(--radius-sm);
    background: rgba(154, 107, 255, 0.06); border: 1px solid var(--border-subtle);
    color: var(--text-mid); font-size: var(--text-sm); line-height: 1.6;
  }
  .panel {
    display: flex; flex-direction: column; gap: 10px; padding: 14px;
    border-radius: var(--radius-md); background: var(--bg-panel);
    border: 1px solid var(--border-subtle);
  }
  .panel-head {
    font-size: var(--text-xs); text-transform: uppercase;
    letter-spacing: 0.14em; color: var(--violet-200);
  }
  .toolbar { display: flex; flex-wrap: wrap; gap: 8px; }
  .btn {
    padding: 8px 14px; border-radius: var(--radius-pill);
    border: 1px solid transparent; color: white;
    font-size: var(--text-sm); cursor: pointer;
    transition: transform var(--dur) var(--ease), box-shadow var(--dur) var(--ease);
  }
  .btn.primary { background: var(--violet-500); }
  .btn.primary:hover:not(:disabled) {
    transform: translateY(-1px); box-shadow: 0 8px 20px -8px rgba(106, 45, 255, 0.7);
  }
  .btn.ghost { background: transparent; color: var(--text-mid); border-color: var(--border); }
  .btn.ghost:hover { color: var(--text-hi); border-color: var(--border-strong); }
  .btn.danger {
    background: transparent; color: var(--status-err);
    border-color: rgba(255, 107, 123, 0.3);
  }
  .btn.danger:hover {
    background: rgba(255, 107, 123, 0.08); border-color: var(--status-err);
  }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>

<script lang="ts">
  /**
   * ExperimentIframeHost — owns the Pyodide sandbox iframe pane.
   *
   * Responsibilities (extracted from ExperimentRunner so each file
   * stays under the size cap, per CLAUDE.md):
   *   1. Mount the `<iframe>` and force-recreate it via `{#key
   *      iframeIteration}` when a wedged worker needs killing.
   *   2. Render the boot progress bar + iframe-loading banner.
   *   3. postMessage broker: subscribe to `window.message`, gate every
   *      message on `ev.source === iframeEl.contentWindow`, and
   *      translate worker frames into typed events for the parent.
   *   4. Track "have we sent init?" + "is the worker ready?" + buffer
   *      pending sends until ready (so the parent can fire-and-forget
   *      `pendingRun` / `pendingTests` regardless of boot state).
   *   5. Watchdog hard-recreate: when the parent's run timeout grace
   *      window expires, the parent calls `recreate()` to nuke the
   *      iframe and its worker.
   *
   * The host is intentionally dumb about run history, inputs schema,
   * and persistence — those belong to ExperimentRunner.
   */
  import { onMount, onDestroy } from "svelte";

  // ── Public types ─────────────────────────────────────────────────
  export interface PendingRun {
    runId: string;
    code: string;
    inputs: Record<string, unknown>;
  }
  export interface PendingTests {
    runId: string;
    code: string;
    tests: string;
  }

  interface Props {
    runnerUrl: string | null;
    runnerUrlError?: string | null;
    /** When non-null, the host posts a `run` to the worker (buffering until ready). */
    pendingRun: PendingRun | null;
    /** When non-null, the host posts a `run_tests` to the worker (buffering until ready). */
    pendingTests?: PendingTests | null;
    /** Show the iframe pane at all. Parent gates on `canRun`. */
    visible: boolean;

    onReady?: () => void;
    onLoading?: (e: { progress: number }) => void;
    onResult?: (e: { runId: string; outputs: Record<string, unknown>; durationMs: number }) => void;
    onError?: (e: { runId: string; message: string; durationMs: number }) => void;
    onTestsResult?: (e: { runId: string; passed: boolean; report: unknown; durationMs: number }) => void;
  }

  let {
    runnerUrl,
    runnerUrlError = null,
    pendingRun,
    pendingTests = null,
    visible,
    onReady,
    onLoading,
    onResult,
    onError,
    onTestsResult,
  }: Props = $props();

  // ── Iframe + worker handshake state ──────────────────────────────
  let iframeEl: HTMLIFrameElement | null = $state(null);
  let iframeReady = $state(false);
  let iframeBootProgress = $state(0);
  /** Bumping this forces Svelte to tear down + remount the iframe. */
  let iframeIteration = $state(0);
  /** Last run id we dispatched to the worker — used to gate replies. */
  let activeRunId = $state<string | null>(null);
  let activeRunStart = $state(0);
  /** Last tests run we dispatched. Distinct so a long-running test doesn't shadow a run. */
  let activeTestsId = $state<string | null>(null);
  let activeTestsStart = $state(0);
  /** Track which pending* identities we've already dispatched, so the parent
   * can re-emit the same object without us double-firing. */
  let dispatchedRunId = $state<string | null>(null);
  let dispatchedTestsId = $state<string | null>(null);

  // ── postMessage helpers ──────────────────────────────────────────
  function postToIframe(msg: unknown) {
    if (!iframeEl?.contentWindow) return;
    iframeEl.contentWindow.postMessage(msg, "*");
  }

  function onIframeLoad() {
    // Some webviews don't deliver iframe_alive reliably; kick init manually.
    postToIframe({ type: "init" });
  }

  function onWindowMessage(ev: MessageEvent) {
    if (!iframeEl || ev.source !== iframeEl.contentWindow) return;
    const data = ev.data as { type?: unknown };
    if (!data || typeof data !== "object" || typeof data.type !== "string") return;

    switch (data.type) {
      case "iframe_alive":
        postToIframe({ type: "init" });
        break;
      case "loading": {
        const p = (data as { progress?: number }).progress;
        if (typeof p === "number") {
          iframeBootProgress = Math.max(0, Math.min(1, p));
          onLoading?.({ progress: iframeBootProgress });
        }
        break;
      }
      case "ready":
        iframeReady = true;
        iframeBootProgress = 1;
        onReady?.();
        // Now that the worker is up, flush any buffered pendings.
        flushPendings();
        break;
      case "result": {
        const payload = data as { runId?: string; outputs?: Record<string, unknown> };
        const runId = payload.runId ?? "";
        if (runId !== activeRunId) break;
        const durationMs = Date.now() - activeRunStart;
        const outputs = payload.outputs ?? {};
        activeRunId = null;
        onResult?.({ runId, outputs, durationMs });
        break;
      }
      case "error": {
        const payload = data as { runId?: string; message?: string };
        const runId = payload.runId ?? "";
        // `__worker_crash__` is the iframe's sentinel for fatal crashes.
        if (runId !== activeRunId && runId !== "__worker_crash__") break;
        const targetRunId = activeRunId ?? runId;
        const durationMs = activeRunStart > 0 ? Date.now() - activeRunStart : 0;
        activeRunId = null;
        onError?.({
          runId: targetRunId,
          message: payload.message || "Unknown runtime error",
          durationMs,
        });
        break;
      }
      case "tests_result": {
        const payload = data as {
          runId?: string;
          passed?: boolean;
          report?: unknown;
        };
        const runId = payload.runId ?? "";
        if (runId !== activeTestsId) break;
        const durationMs = Date.now() - activeTestsStart;
        activeTestsId = null;
        onTestsResult?.({
          runId,
          passed: Boolean(payload.passed),
          report: payload.report,
          durationMs,
        });
        break;
      }
    }
  }

  // ── Pending dispatch (buffer until ready) ────────────────────────
  function flushPendings() {
    if (!iframeReady) return;
    if (pendingRun && pendingRun.runId !== dispatchedRunId) {
      dispatchedRunId = pendingRun.runId;
      activeRunId = pendingRun.runId;
      activeRunStart = Date.now();
      postToIframe({
        type: "run",
        runId: pendingRun.runId,
        code: pendingRun.code,
        inputs: pendingRun.inputs,
      });
    }
    if (pendingTests && pendingTests.runId !== dispatchedTestsId) {
      dispatchedTestsId = pendingTests.runId;
      activeTestsId = pendingTests.runId;
      activeTestsStart = Date.now();
      postToIframe({
        type: "run_tests",
        runId: pendingTests.runId,
        code: pendingTests.code,
        tests: pendingTests.tests,
      });
    }
  }

  // React to new pendings — flush when ready, otherwise the `ready` handler will.
  $effect(() => {
    void pendingRun;
    void pendingTests;
    flushPendings();
  });

  // ── Public API: cancel + recreate (parent calls these) ───────────
  /** Cooperative cancel — post `cancel` for a runId. No-op if unknown. */
  export function cancel(runId: string) {
    postToIframe({ type: "cancel", runId });
  }

  /**
   * Hard-recreate the iframe. Used by the parent's run-timeout watchdog
   * when the worker doesn't reply to `cancel` within the grace window.
   */
  export function recreate() {
    iframeIteration += 1;
    iframeReady = false;
    iframeBootProgress = 0;
    activeRunId = null;
    activeTestsId = null;
    dispatchedRunId = null;
    dispatchedTestsId = null;
  }

  /** Read-only ready flag for the parent's enable-Run gate. */
  export function isReady(): boolean {
    return iframeReady;
  }

  // ── Lifecycle ────────────────────────────────────────────────────
  onMount(() => {
    if (typeof window !== "undefined") {
      window.addEventListener("message", onWindowMessage);
    }
  });

  onDestroy(() => {
    if (typeof window !== "undefined") {
      window.removeEventListener("message", onWindowMessage);
    }
  });
</script>

<div class="iframe-wrap" class:hidden={!visible}>
  {#if runnerUrl}
    {#if !iframeReady && iframeBootProgress < 1}
      <div class="boot" aria-live="polite">
        <div class="boot-label">Loading Python sandbox…</div>
        <div class="boot-bar">
          <div class="boot-fill" style="width: {(iframeBootProgress * 100).toFixed(0)}%"></div>
        </div>
      </div>
    {/if}
    {#key iframeIteration}
      <iframe
        bind:this={iframeEl}
        title="Experiment sandbox"
        src={runnerUrl}
        sandbox="allow-scripts"
        loading="lazy"
        referrerpolicy="no-referrer"
        onload={onIframeLoad}
      ></iframe>
    {/key}
  {:else if runnerUrlError}
    <div class="iframe-empty">
      Could not resolve runner URL: {runnerUrlError}
    </div>
  {:else}
    <div class="iframe-empty">Resolving runner location…</div>
  {/if}
</div>

<style>
  .iframe-wrap {
    position: relative; width: 100%; min-height: 220px; max-height: 60%;
    border-radius: var(--radius-md); border: 1px solid var(--border);
    background: var(--bg-void); overflow: hidden;
  }
  .iframe-wrap.hidden { display: none; }
  .iframe-wrap iframe {
    width: 100%; height: 100%; min-height: 240px;
    border: 0; background: var(--bg-void); display: block;
  }
  .iframe-empty {
    display: grid; place-items: center; height: 100%; min-height: 220px;
    padding: 20px; text-align: center; color: var(--text-mid); font-size: var(--text-sm);
  }
  .boot {
    position: absolute; inset: 0; display: flex; flex-direction: column;
    align-items: center; justify-content: center; gap: 10px;
    background: var(--bg-void); z-index: 1;
  }
  .boot-label {
    font-size: var(--text-xs); text-transform: uppercase;
    letter-spacing: 0.12em; color: var(--text-mid);
  }
  .boot-bar {
    width: min(280px, 70%); height: 4px;
    background: var(--bg-elevated); border-radius: var(--radius-pill);
    border: 1px solid var(--border); overflow: hidden;
  }
  .boot-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--violet-300), var(--violet-glow));
    box-shadow: 0 0 10px -2px var(--violet-glow);
    transition: width var(--dur) var(--ease);
  }
</style>

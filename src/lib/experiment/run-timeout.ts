/**
 * Run timeout watchdog for the experiment iframe runner.
 *
 * The Pyodide worker is owned by an iframe; killing the iframe kills
 * the worker (and any wedged Python loop). This module wraps the
 * two-stage stop flow so ExperimentRunner.svelte stays under the size
 * cap.
 *
 * Stage 1 — soft cancel: post a `cancel` message to the iframe. Today
 * the worker is a no-op for that message, but sending it costs nothing
 * and lets us add cooperative cancel later without changing callers.
 * Stage 2 — hard recreate: after `graceMs`, if no result/error has
 * arrived, the host calls `onHardRecreate()` (the component recreates
 * the iframe via `{#key iteration}`).
 *
 * Pure functions only — no DOM, no Svelte. The component owns the
 * iframe ref; this module only schedules timers and calls back.
 */

export interface RunTimeoutHandle {
  /** Cancel any pending soft + hard timers. Idempotent. */
  clear: () => void;
}

export interface RunTimeoutDeps {
  timeoutMs: number;
  graceMs: number;
  /**
   * Called when the soft timeout fires. The host should post a
   * `cancel` message to the iframe; we don't do it here because the
   * runner module owns the postMessage path.
   */
  onSoftCancel: () => void;
  /**
   * Called when the grace window elapses without a result/error. The
   * host should recreate the iframe to kill the worker.
   */
  onHardRecreate: () => void;
}

/**
 * Start a watchdog. Returns a handle the caller MUST call `.clear()` on
 * once the run completes (result or error received) — otherwise the
 * hard-recreate timer will fire after the grace window.
 *
 * Edge case: if `clear()` is called between the soft fire and the hard
 * fire, the hard recreate is also cancelled. This is intentional —
 * the run completed via the soft cancel path.
 */
export function startRunTimeout(deps: RunTimeoutDeps): RunTimeoutHandle {
  let softTimer: ReturnType<typeof setTimeout> | null = null;
  let hardTimer: ReturnType<typeof setTimeout> | null = null;

  softTimer = setTimeout(() => {
    softTimer = null;
    deps.onSoftCancel();
    hardTimer = setTimeout(() => {
      hardTimer = null;
      deps.onHardRecreate();
    }, deps.graceMs);
  }, deps.timeoutMs);

  return {
    clear() {
      if (softTimer != null) {
        clearTimeout(softTimer);
        softTimer = null;
      }
      if (hardTimer != null) {
        clearTimeout(hardTimer);
        hardTimer = null;
      }
    },
  };
}

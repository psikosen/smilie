<script lang="ts">
  /**
   * Tiny strip of recent runs. Each chip shows a stable hash of the
   * inputs + duration + status dot; clicking re-applies those inputs
   * to the parent's variable controls.
   */
  import type { ExperimentRun } from "$lib/ipc";
  import { shortInputsHash } from "$lib/experiment/runner-url";

  interface Props {
    runs: ExperimentRun[];
    onApply: (run: ExperimentRun) => void;
  }

  let { runs, onApply }: Props = $props();
</script>

{#if runs.length > 0}
  <div class="history">
    {#each runs as run (run.id)}
      <button
        type="button"
        class="run-chip"
        class:err={!!run.error_message}
        onclick={() => onApply(run)}
        title={run.error_message ?? "Click to re-apply these inputs"}
      >
        <span class="hash">#{shortInputsHash(run.inputs)}</span>
        {#if run.duration_ms !== null}
          <span class="dur">{run.duration_ms}ms</span>
        {/if}
        <span class="dot" aria-hidden="true">{run.error_message ? "●" : "○"}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .history {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .run-chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border);
    background: var(--bg-elevated);
    color: var(--text-mid);
    font-size: var(--text-xs);
    font-family: var(--font-mono);
    cursor: pointer;
    transition: color var(--dur) var(--ease), border-color var(--dur) var(--ease);
  }
  .run-chip:hover {
    border-color: var(--violet-400);
    color: var(--text-hi);
  }
  .run-chip.err {
    border-color: rgba(255, 107, 123, 0.3);
    color: var(--status-err);
  }
  .hash { color: var(--violet-100); }
  .dur { color: var(--text-dim); }
  .dot { font-size: 10px; }
  .run-chip.err .dot { color: var(--status-err); }
</style>

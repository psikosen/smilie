<script lang="ts">
  /**
   * Flat list of versions for the selected experiment, walking the
   * `parent_experiment_id` chain back to the root. Each row: version
   * + plausibility score + relative timestamp. Tap to switch.
   *
   * v2 will tree-visualize forks; for v1 we keep it flat — simpler to
   * read for the model's typical 2-3 fork depth.
   */
  import type { ExperimentSummary, ExperimentId } from "$lib/ipc";

  interface Props {
    list: ExperimentSummary[];
    selectedId: ExperimentId | null;
    onSelect: (id: ExperimentId) => void;
  }

  let { list, selectedId, onSelect }: Props = $props();

  const chain = $derived.by(() => {
    if (!selectedId) return [] as ExperimentSummary[];
    const byId = new Map(list.map((e) => [e.id, e] as const));
    const seen = new Set<ExperimentId>();
    const out: ExperimentSummary[] = [];
    let cursor: ExperimentSummary | null = byId.get(selectedId) ?? null;
    while (cursor && !seen.has(cursor.id)) {
      seen.add(cursor.id);
      out.push(cursor);
      cursor = cursor.parent_experiment_id
        ? byId.get(cursor.parent_experiment_id) ?? null
        : null;
    }
    return out;
  });

  function relTime(ts: number): string {
    const now = Date.now() / 1000;
    const delta = Math.max(0, now - ts);
    if (delta < 60) return "just now";
    if (delta < 3600) return `${Math.round(delta / 60)}m ago`;
    if (delta < 86400) return `${Math.round(delta / 3600)}h ago`;
    return `${Math.round(delta / 86400)}d ago`;
  }
</script>

{#if chain.length > 1}
  <section class="versions" aria-label="Experiment versions">
    <div class="head">
      <span class="eyebrow">Version chain</span>
    </div>
    <ol class="list">
      {#each chain as v (v.id)}
        <li>
          <button
            type="button"
            class="row"
            class:active={v.id === selectedId}
            onclick={() => onSelect(v.id)}
          >
            <span class="ver">v{v.version}</span>
            {#if v.plausibility_score !== null}
              <span class="score">{v.plausibility_score}/100</span>
            {/if}
            <span class="ts">{relTime(v.created_at)}</span>
          </button>
        </li>
      {/each}
    </ol>
  </section>
{/if}

<style>
  .versions {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid var(--border-subtle);
  }

  .head {
    display: flex;
    align-items: baseline;
  }

  .eyebrow {
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--violet-200);
    opacity: 0.8;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .row {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border);
    background: var(--bg-elevated);
    color: var(--text-mid);
    font-size: var(--text-xs);
    transition:
      color var(--dur) var(--ease),
      border-color var(--dur) var(--ease),
      background var(--dur) var(--ease);
  }

  .row:hover:not(.active) {
    color: var(--text-hi);
    border-color: var(--border-strong);
  }

  .row.active {
    color: var(--text-hi);
    border-color: var(--violet-400);
    background: rgba(154, 107, 255, 0.14);
    box-shadow: 0 0 0 1px var(--violet-400) inset;
  }

  .ver {
    font-family: var(--font-mono);
    color: var(--violet-100);
    font-weight: 600;
  }

  .score {
    font-family: var(--font-mono);
    color: var(--violet-200);
  }

  .ts {
    color: var(--text-dim);
  }
</style>

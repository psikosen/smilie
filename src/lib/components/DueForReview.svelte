<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { ipc, events, type DuePage } from "$lib/ipc";
  import { isTauri } from "$lib/ipc/tauri-guard";

  let due = $state<DuePage[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let retrying = $state(false);
  let unlisten: (() => void) | null = null;
  let destroyed = false;
  let refreshVersion = 0;

  async function refresh() {
    if (!isTauri()) {
      loading = false;
      return;
    }

    const version = ++refreshVersion;
    try {
      const next = await ipc.listDuePages();
      if (destroyed || version !== refreshVersion) return;
      due = next;
      error = null;
    } catch (cause) {
      if (destroyed || version !== refreshVersion) return;
      const message = cause instanceof Error ? cause.message : String(cause);
      error = message.replace(/^Error:\s*/i, "") || "Unknown review error";
    } finally {
      if (!destroyed && version === refreshVersion) loading = false;
    }
  }

  async function retry() {
    if (retrying) return;
    retrying = true;
    await refresh();
    retrying = false;
  }

  onMount(() => {
    destroyed = false;
    void (async () => {
      await refresh();
      if (!isTauri() || destroyed) return;
      try {
        const stop = await events.onPaperUpdated(() => {
          void refresh();
        });
        if (destroyed) stop();
        else unlisten = stop;
      } catch (cause) {
        if (!destroyed) {
          const message = cause instanceof Error ? cause.message : String(cause);
          error = `Review updates are unavailable: ${message.replace(/^Error:\s*/i, "")}`;
        }
      }
    })();

    return () => {
      destroyed = true;
      refreshVersion += 1;
      unlisten?.();
      unlisten = null;
    };
  });

  function open(item: DuePage) {
    void goto(`/papers/${item.paper_id}/?page=${item.page_id}`);
  }
</script>

{#if !loading && (due.length > 0 || error)}
  <section class="due-row" aria-labelledby="review-heading">
    <header class="due-head">
      <div class="due-eyebrow">Spaced repetition</div>
      <h2 id="review-heading">{error && due.length === 0 ? "Review queue unavailable" : "Due for review"}</h2>
      {#if due.length > 0}
        <span class="count" aria-label={`${due.length} ${due.length === 1 ? "page" : "pages"} due`}>
          {due.length} due
        </span>
      {/if}
    </header>
    {#if due.length > 0}
      <ul class="chip-row">
        {#each due as item (item.page_id)}
          <li>
            <button
              type="button"
              class="due-chip"
              onclick={() => open(item)}
              aria-label={`Review ${item.title}`}
              title={item.title}
            >
              <span class="due-glyph" aria-hidden="true">↻</span>
              <span class="due-title">{item.title}</span>
              <span class="due-arrow" aria-hidden="true">→</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if error}
      <div class="review-error" role="alert">
        <span>Could not refresh reviews: {error}</span>
        <button type="button" disabled={retrying} onclick={retry}>
          {retrying ? "Retrying…" : "Retry"}
        </button>
      </div>
    {/if}
  </section>
{/if}

<style>
  .due-row {
    max-width: 1340px;
    margin: 0 auto 24px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px 16px;
    border-radius: var(--radius-lg);
    background: linear-gradient(160deg, rgba(154, 107, 255, 0.06), rgba(79, 240, 255, 0.04));
    border: 1px solid var(--border);
  }

  .due-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 0 4px;
  }

  .due-eyebrow {
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.16em;
    color: var(--violet-200);
    opacity: 0.75;
  }

  .due-head h2 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
    letter-spacing: 0;
  }

  .count {
    margin-left: auto;
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--violet-200);
    padding: 2px 8px;
    border-radius: var(--radius-pill);
    background: rgba(154, 107, 255, 0.12);
    border: 1px solid var(--border);
  }

  .chip-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .due-chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    padding: 8px 12px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border);
    background: var(--bg-elevated, rgba(255, 255, 255, 0.02));
    color: var(--text-hi);
    font-size: var(--text-sm);
    cursor: pointer;
    max-width: 320px;
    transition:
      transform var(--dur) var(--ease),
      border-color var(--dur) var(--ease),
      box-shadow var(--dur) var(--ease);
  }

  .due-chip:hover {
    transform: translateY(-1px);
    border-color: var(--border-strong);
    box-shadow: 0 6px 18px -10px rgba(106, 45, 255, 0.6);
  }

  .due-chip:focus-visible,
  .review-error button:focus-visible {
    outline: 2px solid var(--cyan-accent);
    outline-offset: 2px;
  }

  .due-glyph {
    color: var(--violet-200);
    font-size: 13px;
  }

  .due-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .due-arrow {
    color: var(--cyan-accent);
    opacity: 0.7;
  }

  .review-error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid rgba(255, 107, 123, 0.3);
    border-radius: var(--radius-md);
    color: var(--status-err);
    background: rgba(255, 107, 123, 0.06);
    font-size: var(--text-sm);
  }

  .review-error button {
    min-width: 72px;
    min-height: 36px;
    padding: 0 12px;
    border: 1px solid rgba(255, 107, 123, 0.4);
    border-radius: var(--radius-md);
    color: var(--text-hi);
    background: rgba(255, 107, 123, 0.1);
  }

  .review-error button:disabled {
    opacity: 0.65;
    cursor: progress;
  }

  @media (max-width: 720px) {
    .due-row {
      padding: 14px;
    }

    .due-head {
      align-items: flex-start;
      flex-wrap: wrap;
    }

    .due-eyebrow {
      flex-basis: 100%;
    }

    .chip-row {
      flex-wrap: nowrap;
      margin-inline: -14px;
      padding: 0 14px 4px;
      overflow-x: auto;
      scroll-snap-type: x proximity;
    }

    .chip-row li {
      flex: 0 0 auto;
      max-width: min(82vw, 320px);
      scroll-snap-align: start;
    }

    .due-chip {
      width: 100%;
      min-height: 44px;
    }

    .review-error {
      align-items: stretch;
      flex-direction: column;
    }

    .review-error button {
      min-height: 44px;
    }
  }

  @media (hover: none), (pointer: coarse), (prefers-reduced-motion: reduce) {
    .due-chip:hover {
      transform: none;
    }
  }
</style>

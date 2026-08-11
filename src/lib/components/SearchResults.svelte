<script module lang="ts">
  export function createSearchRequestSequence() {
    let current = 0;
    return {
      next: () => ++current,
      isCurrent: (request: number) => request === current,
    };
  }

  export function searchResultHref(paperId: string, pageId: string): string {
    return `/papers/${encodeURIComponent(paperId)}/?page=${encodeURIComponent(pageId)}`;
  }
</script>

<script lang="ts">
  import { ipc, type PageRecord } from "$lib/ipc";
  import { isTauri } from "$lib/ipc/tauri-guard";
  import { goto } from "$app/navigation";

  interface Props {
    query: string;
  }
  let { query }: Props = $props();

  let results = $state<PageRecord[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  const searchRequests = createSearchRequestSequence();

  function iconForType(t: string): string {
    switch (t) {
      case "informational": return "I";
      case "concept": return "C";
      case "formula": return "Σ";
      case "figure": return "F";
      case "table": return "T";
      case "quiz": return "?";
      case "simulation": return "▶";
      default: return "P";
    }
  }

  function typeLabel(t: string): string {
    switch (t) {
      case "informational": return "Informational";
      case "concept": return "Concept explainer";
      case "formula": return "Formula explorer";
      case "figure": return "Figure explorer";
      case "table": return "Table explorer";
      case "quiz": return "Quiz";
      case "simulation": return "Simulation";
      default: return "Learning page";
    }
  }

  function sourceLabel(result: PageRecord): string {
    if (result.source_pages.length) {
      return `Paper p. ${result.source_pages.join(", ")}`;
    }
    if (result.source_sections.length) {
      return result.source_sections.slice(0, 2).join(" · ");
    }
    return "Source reference unavailable";
  }

  function statusLabel(status: PageRecord["status"]): string {
    switch (status) {
      case "ready": return "Saved offline";
      case "queued": return "Queued";
      case "generating": return "Generating";
      case "failed": return "Failed";
    }
  }

  $effect(() => {
    const q = query.trim();
    const request = searchRequests.next();
    loading = false;
    results = [];
    error = null;
    if (!isTauri() || q.length < 2) {
      return;
    }
    loading = true;
    ipc
      .searchPages(q)
      .then((r) => {
        if (!searchRequests.isCurrent(request)) return;
        results = r;
        error = null;
      })
      .catch((e) => {
        if (!searchRequests.isCurrent(request)) return;
        error = String(e);
        results = [];
      })
      .finally(() => {
        if (searchRequests.isCurrent(request)) loading = false;
      });
  });

  function openResult(r: PageRecord) {
    void goto(searchResultHref(r.paper_id, r.id));
  }
</script>

<section
  class="results-wrap fade-rise"
  aria-labelledby="search-results-title"
  aria-busy={loading}
>
  <div class="head">
    <h2 id="search-results-title">Results for <span class="q">"{query}"</span></h2>
    <span class="result-state" role="status" aria-live="polite">
      {#if loading}
        Searching saved pages…
      {:else if query.trim().length >= 2 && !error}
        {results.length} {results.length === 1 ? "result" : "results"}
      {/if}
    </span>
  </div>

  {#if error}
    <div class="err" role="alert">
      Search could not be completed. {error}
    </div>
  {:else if !isTauri()}
    <div class="dim" role="status">Search runs in the desktop app.</div>
  {:else if query.trim().length < 2}
    <div class="dim" role="status">Type at least 2 characters.</div>
  {:else if results.length === 0 && !loading}
    <div class="dim" role="status">No saved learning pages match this search.</div>
  {:else}
    <ul class="list" aria-label="Saved learning page search results">
      {#each results as r (r.id)}
        <li>
          <button
            type="button"
            class="row"
            onclick={() => openResult(r)}
            aria-label={`${r.title}. ${typeLabel(r.type)}. ${sourceLabel(r)}. ${statusLabel(r.status)}.`}
          >
            <span class="glyph" aria-hidden="true">{iconForType(r.type)}</span>
            <span class="copy">
              <span class="title">{r.title}</span>
              <span class="meta">{typeLabel(r.type)} · {sourceLabel(r)}</span>
            </span>
            <span
              class="status"
              class:ready={r.status === "ready"}
              class:failed={r.status === "failed"}
            >
              {statusLabel(r.status)}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .results-wrap {
    max-width: 920px;
    margin: 32px auto 0;
    padding: 0 24px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .head {
    display: flex;
    align-items: baseline;
    gap: 12px;
    flex-wrap: wrap;
  }

  .head h2 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--text-hi);
  }

  .q { color: var(--violet-100); font-family: var(--font-mono); font-size: var(--text-md); }

  .dim,
  .result-state { color: var(--text-dim); font-size: var(--text-sm); }
  .err { color: var(--status-err); font-size: var(--text-sm); }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .row {
    display: grid;
    grid-template-columns: 28px 1fr auto;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    text-align: left;
    color: var(--text-hi);
    background: var(--bg-panel);
    border: 1px solid var(--border);
    transition:
      transform var(--dur) var(--ease),
      border-color var(--dur) var(--ease),
      box-shadow var(--dur) var(--ease);
  }

  .row:hover,
  .row:focus-visible {
    transform: translateY(-1px);
    border-color: var(--border-strong);
    box-shadow: 0 10px 24px -12px rgba(106, 45, 255, 0.5);
  }

  .row:focus-visible {
    outline: 2px solid var(--cyan-accent);
    outline-offset: 2px;
  }

  .glyph {
    font-family: var(--font-mono);
    font-size: 14px;
    color: var(--violet-200);
    justify-self: center;
  }

  .copy {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .meta {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-dim);
    font-family: var(--font-mono);
    font-size: 10px;
  }

  .status {
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding: 2px 8px;
    border-radius: var(--radius-pill);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-dim);
    border: 1px solid var(--border-subtle);
  }

  .status.ready {
    color: var(--status-ok);
    border-color: rgba(93, 255, 160, 0.22);
    background: rgba(93, 255, 160, 0.055);
  }

  .status.failed {
    color: var(--status-err);
    border-color: rgba(255, 107, 123, 0.28);
    background: rgba(255, 107, 123, 0.07);
  }

  @media (max-width: 620px) {
    .results-wrap {
      margin-top: 20px;
      padding: 0 14px;
    }

    .row {
      grid-template-columns: 28px minmax(0, 1fr);
      min-height: 64px;
      padding: 11px 12px;
    }

    .status {
      grid-column: 2;
      justify-self: start;
    }
  }
</style>

<script lang="ts">
  /**
   * Compare tab — left-rail list of comparisons for the open paper plus
   * an inline "+ New comparison" picker that lets the user select another
   * paper from the library to diff against. Right pane is CompareViewer.
   *
   * Comparison lifecycle is server-driven; we only orchestrate the
   * picker → IPC kick → store upsert + selection. The store subscribes
   * to `comparison` events for status flips.
   */
  import { onDestroy } from "svelte";
  import {
    ipc,
    type ComparisonDetail,
    type ComparisonId,
    type ComparisonStatus,
    type PaperId,
    type PaperSummary,
  } from "$lib/ipc";
  import { isTauri } from "$lib/ipc/tauri-guard";
  import { papers } from "$lib/stores/papers";
  import { comparisons } from "$lib/stores/comparisons";
  import { toast } from "$lib/stores/toast";
  import CompareViewer from "./CompareViewer.svelte";

  interface Props {
    paperId: PaperId;
  }
  let { paperId }: Props = $props();

  let selectedId = $state<ComparisonId | null>(null);
  let selectedDetail = $state<ComparisonDetail | null>(null);
  let detailLoading = $state(false);
  let detailError = $state<string | null>(null);
  let pickerOpen = $state(false);
  let creatingFor = $state<PaperId | null>(null);
  let refineBusy = $state(false);

  // Track previous status so we can fire toasts on transitions.
  let prevStatus = $state<Record<ComparisonId, ComparisonStatus>>({});

  $effect(() => {
    if (paperId) {
      void comparisons.open(paperId);
      void papers.init();
    }
  });

  onDestroy(() => {
    comparisons.close();
  });

  // Auto-select first comparison once the list loads.
  $effect(() => {
    const list = $comparisons.list;
    if (!selectedId && list.length > 0) {
      selectedId = list[0]!.id;
    }
    if (selectedId && !list.some((c) => c.id === selectedId)) {
      selectedId = list[0]?.id ?? null;
    }
  });

  // Pull detail whenever selection changes / store list refreshes.
  $effect(() => {
    const id = selectedId;
    if (!id) {
      selectedDetail = null;
      return;
    }
    if (!isTauri()) {
      selectedDetail = null;
      return;
    }
    detailLoading = true;
    detailError = null;
    void comparisons
      .loadDetail(id)
      .then((detail) => {
        if (selectedId === id) {
          selectedDetail = detail;
          detailLoading = false;
        }
      })
      .catch((err) => {
        if (selectedId === id) {
          detailError = String(err);
          detailLoading = false;
        }
      });
  });

  // Toast on terminal-status transitions for any tracked comparison.
  $effect(() => {
    const next: Record<ComparisonId, ComparisonStatus> = {};
    for (const c of $comparisons.list) {
      next[c.id] = c.status;
      const prior = prevStatus[c.id];
      if (prior && prior !== c.status) {
        if (c.status === "ready") {
          toast.success(`Comparison ready: ${c.paper_b_title}`);
        } else if (c.status === "failed") {
          toast.error(`Comparison failed: ${c.paper_b_title}`);
        }
      }
    }
    prevStatus = next;
  });

  // Other papers — exclude this paper from the picker.
  const otherPapers = $derived<PaperSummary[]>(
    $papers.items.filter((p) => p.id !== paperId),
  );

  async function startComparison(other: PaperSummary) {
    if (!isTauri()) {
      toast.error("New comparison only available in the desktop app.");
      return;
    }
    creatingFor = other.id;
    pickerOpen = false;
    try {
      const summary = await ipc.compareWithPaper(paperId, other.id);
      comparisons.upsertSummary(summary);
      selectedId = summary.id;
      // Idempotent on the (a,b) pair — friendly distinction in the toast.
      toast.success(
        summary.status === "ready"
          ? "Comparison loaded from cache"
          : "Analyzing comparison…",
      );
    } catch (err) {
      toast.error(`Could not start comparison: ${String(err)}`);
    } finally {
      creatingFor = null;
    }
  }

  async function refineSelected() {
    if (!selectedDetail || !isTauri()) return;
    refineBusy = true;
    try {
      // Backend is idempotent on the (a, b) pair — calling
      // compareWithPaper again kicks a fresh analyze pass.
      const summary = await ipc.compareWithPaper(
        selectedDetail.paper_a_id,
        selectedDetail.paper_b_id,
      );
      comparisons.upsertSummary(summary);
      selectedId = summary.id;
      toast.success("Re-analyzing comparison…");
    } catch (err) {
      toast.error(`Could not refine: ${String(err)}`);
    } finally {
      refineBusy = false;
    }
  }

  function statusDot(status: ComparisonStatus): string {
    return status === "ready" ? "●" : status === "failed" ? "●" : "…";
  }
  function statusClass(status: ComparisonStatus): string {
    return status === "ready" ? "ok" : status === "failed" ? "err" : "busy";
  }
</script>

<div class="tab">
  <aside class="rail">
    <div class="rail-head">
      <button
        type="button"
        class="new-btn"
        onclick={() => (pickerOpen = !pickerOpen)}
        disabled={!!creatingFor}
        aria-expanded={pickerOpen}
      >
        {creatingFor ? "Starting…" : "+ New comparison"}
      </button>
      {#if pickerOpen}
        <div class="picker" role="listbox" aria-label="Pick a paper to compare">
          {#if otherPapers.length === 0}
            <div class="picker-empty">
              {#if !isTauri()}
                Library only loads in the desktop app.
              {:else if $papers.loading}
                Loading…
              {:else}
                Add another paper to your library to compare.
              {/if}
            </div>
          {:else}
            {#each otherPapers as other (other.id)}
              <button
                type="button"
                class="picker-item"
                role="option"
                aria-selected="false"
                onclick={() => startComparison(other)}
                disabled={creatingFor === other.id}
                title={other.title}
              >
                <span class="picker-title">{other.title || "Untitled"}</span>
                {#if other.authors.length > 0}
                  <span class="picker-meta">
                    {other.authors.slice(0, 2).join(", ")}
                  </span>
                {/if}
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>

    <nav class="rail-list" aria-label="Comparisons">
      {#if $comparisons.loading && $comparisons.list.length === 0}
        <div class="empty">Loading…</div>
      {:else if $comparisons.error}
        <div class="empty err">{$comparisons.error}</div>
      {:else if $comparisons.list.length === 0}
        <div class="empty">
          No comparisons yet. Click <strong>+ New comparison</strong> to diff
          this paper against another in your library.
        </div>
      {:else}
        {#each $comparisons.list as c (c.id)}
          <button
            type="button"
            class="item"
            class:active={c.id === selectedId}
            onclick={() => (selectedId = c.id)}
            title={c.paper_b_title}
          >
            <span class="item-title">{c.paper_b_title}</span>
            <span
              class="dot {statusClass(c.status)}"
              aria-label={c.status}
              title={c.status}
            >
              {statusDot(c.status)}
            </span>
          </button>
        {/each}
      {/if}
    </nav>
  </aside>

  <section class="pane">
    {#if !isTauri()}
      <div class="empty-pane">
        <h2>Compare</h2>
        <p>Open the desktop app to compare papers in your library.</p>
      </div>
    {:else if detailError}
      <div class="empty-pane">
        <h2>Could not load comparison</h2>
        <p>{detailError}</p>
      </div>
    {:else if detailLoading && !selectedDetail}
      <div class="empty-pane"><p>Loading comparison…</p></div>
    {:else}
      {#key selectedDetail?.id ?? "none"}
        <CompareViewer
          comparison={selectedDetail}
          onRefine={refineSelected}
          {refineBusy}
        />
      {/key}
    {/if}
  </section>
</div>

<style>
  .tab {
    position: relative;
    display: grid;
    grid-template-columns: 260px 1fr;
    height: 100%;
    overflow: hidden;
  }

  .rail {
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--border);
    background: var(--bg-panel);
    overflow: hidden;
  }
  .rail-head {
    position: relative;
    padding: 12px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .new-btn {
    width: 100%;
    padding: 8px 12px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border-strong);
    background: var(--violet-500);
    color: white;
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    transition: transform var(--dur) var(--ease), box-shadow var(--dur) var(--ease);
  }
  .new-btn:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: 0 8px 20px -8px rgba(106, 45, 255, 0.7);
  }
  .new-btn:disabled { opacity: 0.55; cursor: progress; }

  .picker {
    position: absolute;
    left: 12px;
    right: 12px;
    top: calc(100% - 6px);
    z-index: var(--z-overlay);
    max-height: min(320px, 60dvh);
    overflow-y: auto;
    border-radius: var(--radius-md);
    border: 1px solid var(--border-strong);
    background: var(--bg-glass);
    backdrop-filter: blur(20px) saturate(140%);
    -webkit-backdrop-filter: blur(20px) saturate(140%);
    box-shadow: 0 18px 40px -16px rgba(0, 0, 0, 0.6);
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .picker-empty {
    padding: 14px 10px;
    color: var(--text-dim);
    font-size: var(--text-xs);
    line-height: 1.5;
  }

  .picker-item {
    display: grid;
    grid-template-columns: 1fr;
    text-align: left;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    border: 0;
    background: transparent;
    color: var(--text-mid);
    cursor: pointer;
    transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
  }
  .picker-item:hover:not(:disabled) {
    background: rgba(154, 107, 255, 0.10);
    color: var(--text-hi);
  }
  .picker-item:disabled { opacity: 0.6; cursor: progress; }
  .picker-title {
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--text-hi);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .picker-meta {
    font-size: 11px;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .rail-list {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .empty {
    padding: 14px 8px;
    color: var(--text-dim);
    font-size: var(--text-xs);
    line-height: 1.55;
  }
  .empty.err { color: var(--status-err); }
  .empty strong { color: var(--text-mid); }

  .item {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    text-align: left;
    color: var(--text-mid);
    font-size: var(--text-sm);
    border: 0;
    background: transparent;
    cursor: pointer;
    transition:
      background var(--dur) var(--ease),
      color var(--dur) var(--ease);
  }
  .item:hover {
    color: var(--text-hi);
    background: rgba(154, 107, 255, 0.06);
  }
  .item.active {
    color: var(--text-hi);
    background: linear-gradient(
      90deg,
      rgba(154, 107, 255, 0.18),
      rgba(154, 107, 255, 0.04) 70%
    );
    box-shadow: inset 2px 0 0 var(--violet-400);
  }
  .item-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .dot {
    width: 10px;
    height: 10px;
    display: inline-grid;
    place-items: center;
    line-height: 1;
    font-family: var(--font-mono);
    font-size: 10px;
  }
  .dot.ok  { color: var(--status-ok); }
  .dot.err { color: var(--status-err); }
  .dot.busy { color: var(--violet-200); }

  .pane {
    min-width: 0;
    height: 100%;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .pane > :global(section.viewer) {
    flex: 1;
    min-height: 0;
  }
  .empty-pane {
    margin: auto;
    max-width: 440px;
    padding: 32px;
    text-align: center;
    color: var(--text-mid);
  }
  .empty-pane h2 {
    margin: 0 0 6px;
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--text-hi);
  }
  .empty-pane p {
    margin: 0;
    font-size: var(--text-sm);
    line-height: 1.55;
  }

  @media (max-width: 720px) {
    .tab {
      grid-template-columns: 1fr;
    }
    .rail {
      max-height: 45vh;
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }
  }
</style>

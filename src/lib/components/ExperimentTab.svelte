<script lang="ts">
  /**
   * Top-level Experiments tab content. Layout:
   *   Left rail (240px desktop, full-width bottom-sheet on mobile):
   *     list of experiments + "+ New experiment" button.
   *   Right pane: <ExperimentRunner detail={selected} /> or empty state.
   *
   * Owns the selection + the inherited-inputs handoff for refines.
   */
  import { onDestroy } from "svelte";
  import {
    ipc,
    type ExperimentDetail,
    type ExperimentSummary,
    type ExperimentId,
    type PaperId,
  } from "$lib/ipc";
  import { isTauri } from "$lib/ipc/tauri-guard";
  import { experiments } from "$lib/stores/experiments";
  import { viewport } from "$lib/stores/viewport";
  import { closeRequest } from "$lib/stores/drawer";
  import { toast } from "$lib/stores/toast";
  import ExperimentRunner from "./ExperimentRunner.svelte";

  interface Props {
    paperId: PaperId;
  }

  let { paperId }: Props = $props();

  let selectedId = $state<ExperimentId | null>(null);
  let selectedDetail = $state<ExperimentDetail | null>(null);
  let detailLoading = $state(false);
  let detailError = $state<string | null>(null);
  let creating = $state(false);
  let mobileListOpen = $state(false);

  // Open the store for this paper. Store handles its own teardown of the
  // event subscription on `close()`.
  $effect(() => {
    if (paperId) void experiments.open(paperId);
  });

  onDestroy(() => {
    experiments.close();
  });

  // Auto-select first ready (or first) experiment when the list arrives.
  $effect(() => {
    const list = $experiments.list;
    if (!selectedId && list.length > 0) {
      const ready = list.find((e) => e.status === "ready") ?? list[0];
      selectedId = ready?.id ?? null;
    }
    // If the selected experiment was deleted (no longer in the list),
    // pick the next available. Avoids dangling state.
    if (selectedId && !list.some((e) => e.id === selectedId)) {
      selectedId = list[0]?.id ?? null;
    }
  });

  // Whenever the selection changes (or the store list refreshes due to
  // an event), pull a fresh detail. We always fetch — the list event
  // is the trigger so the runner reflects updated status / score.
  $effect(() => {
    const id = selectedId;
    if (!id) {
      selectedDetail = null;
      return;
    }
    if (!isTauri()) {
      // In browser dev mode we have no backend — skip detail fetch.
      selectedDetail = null;
      return;
    }
    detailLoading = true;
    detailError = null;
    void experiments
      .loadDetail(id)
      .then((detail) => {
        // Guard against rapid selection changes — only commit if still current.
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

  // Close mobile sheet on Esc / global close request.
  $effect(() => {
    $closeRequest;
    if (mobileListOpen) mobileListOpen = false;
  });

  $effect(() => {
    if (!$viewport.mobile && mobileListOpen) mobileListOpen = false;
  });

  async function newExperiment() {
    if (!isTauri()) {
      toast.error("New experiment only available in the desktop app.");
      return;
    }
    creating = true;
    try {
      const summary = await ipc.analyzeExperiment(paperId);
      experiments.upsertSummary(summary);
      selectedId = summary.id;
      toast.success("Analyzing feasibility…");
    } catch (err) {
      toast.error(`Could not start experiment: ${String(err)}`);
    } finally {
      creating = false;
    }
  }

  function selectFromList(id: ExperimentId) {
    selectedId = id;
    if ($viewport.mobile) mobileListOpen = false;
  }

  function statusGlyph(status: ExperimentSummary["status"]): string {
    switch (status) {
      case "analyzing":
      case "generating":
      case "testing":
        return "…";
      case "ready":
        return "●";
      case "failed":
        return "●";
      case "implausible":
        return "○";
    }
  }

  function statusClass(status: ExperimentSummary["status"]): string {
    switch (status) {
      case "ready":
        return "ok";
      case "failed":
        return "err";
      case "implausible":
        return "soft";
      default:
        return "busy";
    }
  }

  // Bridge: when the runner triggers a fork via Refine, ExperimentRunner
  // calls `experiments.upsertSummary` and `onSelect(newId)` itself. We
  // just switch to the new id — the new runner re-seeds from the
  // model-emitted defaults (no inherit-inputs handoff in v1).
  function onRunnerSelect(id: ExperimentId) {
    selectedId = id;
  }
</script>

<div class="tab" class:mobile={$viewport.mobile}>
  {#if $viewport.mobile && mobileListOpen}
    <button
      class="backdrop"
      type="button"
      aria-label="Close experiments list"
      onclick={() => (mobileListOpen = false)}
    ></button>
  {/if}

  <aside
    class="rail"
    class:mobile={$viewport.mobile}
    class:open={mobileListOpen}
    aria-hidden={$viewport.mobile && !mobileListOpen ? "true" : "false"}
  >
    <div class="rail-head">
      <button
        type="button"
        class="new-btn"
        onclick={newExperiment}
        disabled={creating}
      >
        {creating ? "Starting…" : "+ New experiment"}
      </button>
    </div>
    <nav class="rail-list" aria-label="Experiments">
      {#if $experiments.loading && $experiments.list.length === 0}
        <div class="empty">Loading…</div>
      {:else if $experiments.error}
        <div class="empty err">{$experiments.error}</div>
      {:else if $experiments.list.length === 0}
        <div class="empty">
          No experiments yet. Click <strong>+ New experiment</strong> to ask the
          model whether this paper has a runnable reproduction.
        </div>
      {:else}
        {#each $experiments.list as exp (exp.id)}
          <button
            type="button"
            class="item"
            class:active={exp.id === selectedId}
            onclick={() => selectFromList(exp.id)}
            title={exp.title}
          >
            <span class="item-title">{exp.title}</span>
            <span class="meta">
              <span class="ver">v{exp.version}</span>
              {#if exp.plausibility_score !== null}
                <span class="plaus">{exp.plausibility_score}</span>
              {/if}
              <span class="dot {statusClass(exp.status)}" aria-label={exp.status}>
                {statusGlyph(exp.status)}
              </span>
            </span>
          </button>
        {/each}
      {/if}
    </nav>
  </aside>

  <section class="pane">
    {#if !isTauri()}
      <div class="empty-pane">
        <h2>Experiments</h2>
        <p>Open the desktop app to analyze and run experiments per paper.</p>
      </div>
    {:else if detailError}
      <div class="empty-pane">
        <h2>Could not load experiment</h2>
        <p>{detailError}</p>
      </div>
    {:else if !selectedId}
      <div class="empty-pane">
        <h2>Pick an experiment</h2>
        <p>Pick one from the list, or click <strong>+ New experiment</strong> to start one.</p>
      </div>
    {:else if detailLoading && !selectedDetail}
      <div class="empty-pane"><p>Loading experiment…</p></div>
    {:else if selectedDetail}
      {#key selectedDetail.id}
        <ExperimentRunner
          detail={selectedDetail}
          summaries={$experiments.list}
          onSelect={onRunnerSelect}
        />
      {/key}
    {/if}
  </section>

  {#if $viewport.mobile}
    <button
      type="button"
      class="list-fab"
      onclick={() => (mobileListOpen = !mobileListOpen)}
      aria-label="Show experiments list"
    >
      <span aria-hidden="true">≡</span>
      <span>Experiments</span>
      <span class="fab-meta">{$experiments.list.length}</span>
    </button>
  {/if}
</div>

<style>
  .tab {
    position: relative; display: grid; grid-template-columns: 240px 1fr;
    height: 100%; overflow: hidden;
  }
  .tab.mobile { grid-template-columns: 1fr; }
  .rail {
    display: flex; flex-direction: column;
    border-right: 1px solid var(--border); background: var(--bg-panel); overflow: hidden;
  }
  .rail.mobile {
    position: fixed; inset: auto 0 0 0; width: 100%;
    height: min(75dvh, 600px); max-height: 75dvh;
    border-right: 0; border-top: 1px solid var(--border-strong);
    border-radius: var(--radius-xl) var(--radius-xl) 0 0;
    box-shadow: 0 -28px 60px -20px rgba(0, 0, 0, 0.7);
    transform: translateY(100%);
    transition: transform var(--dur-slow) var(--ease);
    z-index: var(--z-drawer);
    padding-bottom: max(24px, env(safe-area-inset-bottom));
  }
  .rail.mobile.open { transform: translateY(0); }
  .backdrop {
    position: fixed; inset: 0;
    background: var(--bg-overlay);
    backdrop-filter: blur(8px); -webkit-backdrop-filter: blur(8px);
    border: 0; cursor: pointer; z-index: calc(var(--z-drawer) - 1);
  }
  .rail-head { padding: 12px; border-bottom: 1px solid var(--border-subtle); }
  .new-btn {
    width: 100%; padding: 8px 12px; border-radius: var(--radius-pill);
    border: 1px solid var(--border-strong);
    background: var(--violet-500); color: white;
    font-size: var(--text-sm); font-weight: 500; cursor: pointer;
    transition: transform var(--dur) var(--ease), box-shadow var(--dur) var(--ease);
  }
  .new-btn:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: 0 8px 20px -8px rgba(106, 45, 255, 0.7);
  }
  .new-btn:disabled { opacity: 0.55; cursor: progress; }
  .rail-list {
    flex: 1; overflow-y: auto; padding: 8px;
    display: flex; flex-direction: column; gap: 2px;
  }
  .empty {
    padding: 14px 8px; color: var(--text-dim);
    font-size: var(--text-xs); line-height: 1.55;
  }
  .empty.err { color: var(--status-err); }
  .empty strong { color: var(--text-mid); }
  .item {
    display: grid; grid-template-columns: 1fr auto; gap: 8px;
    padding: 8px 10px; border-radius: var(--radius-sm);
    text-align: left; color: var(--text-mid); font-size: var(--text-sm);
    border: 0; background: transparent; cursor: pointer;
    transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
  }
  .item:hover { color: var(--text-hi); background: rgba(154, 107, 255, 0.06); }
  .item.active {
    color: var(--text-hi);
    background: linear-gradient(90deg, rgba(154, 107, 255, 0.18), rgba(154, 107, 255, 0.04) 70%);
    box-shadow: inset 2px 0 0 var(--violet-400);
  }
  .item-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .meta {
    display: inline-flex; align-items: center; gap: 6px;
    font-family: var(--font-mono); font-size: 10px; color: var(--text-dim);
  }
  .ver { color: var(--violet-100); }
  .plaus { color: var(--violet-200); }
  .dot { width: 10px; height: 10px; display: inline-grid; place-items: center; line-height: 1; }
  .dot.ok { color: var(--status-ok); }
  .dot.err { color: var(--status-err); }
  .dot.busy { color: var(--violet-200); }
  .dot.soft { color: var(--text-dim); }
  .pane { min-width: 0; height: 100%; overflow: hidden; display: flex; flex-direction: column; }
  .pane > :global(section.runner) { flex: 1; min-height: 0; }
  .empty-pane {
    margin: auto; max-width: 440px; padding: 32px;
    text-align: center; color: var(--text-mid);
  }
  .empty-pane h2 {
    margin: 0 0 6px; font-size: var(--text-lg);
    font-weight: 600; color: var(--text-hi);
  }
  .empty-pane p { margin: 0; font-size: var(--text-sm); line-height: 1.55; }
  .empty-pane strong { color: var(--violet-100); }
  .list-fab {
    position: fixed; left: 14px; right: 14px;
    bottom: max(14px, env(safe-area-inset-bottom));
    z-index: calc(var(--z-drawer) + 1);
    display: grid; grid-template-columns: auto 1fr auto;
    align-items: center; gap: 10px; padding: 12px 16px;
    border-radius: var(--radius-pill); border: 1px solid var(--border-strong);
    background: var(--bg-glass);
    backdrop-filter: blur(20px) saturate(140%);
    -webkit-backdrop-filter: blur(20px) saturate(140%);
    color: var(--text-hi);
    box-shadow: 0 18px 40px -16px rgba(106, 45, 255, 0.5);
    font-size: var(--text-sm);
  }
  .fab-meta {
    font-family: var(--font-mono); font-size: 11px; color: var(--violet-200);
    padding: 2px 8px; border-radius: var(--radius-pill);
    background: rgba(154, 107, 255, 0.12); border: 1px solid var(--border);
  }
</style>

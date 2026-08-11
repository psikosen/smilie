<script lang="ts">
  import type { PaperSummary } from "$lib/ipc";
  import { cursorGlow } from "$lib/theme/effects";
  import { ipc } from "$lib/ipc";
  import { papers } from "$lib/stores/papers";
  import { toast } from "$lib/stores/toast";
  import ConfirmDialog from "./ConfirmDialog.svelte";

  interface Props {
    paper: PaperSummary;
    onOpen?: (id: string) => void;
    layout?: "grid" | "list";
  }
  let { paper, onOpen, layout = "grid" }: Props = $props();
  let deleting = $state(false);
  let askingDelete = $state(false);
  let resuming = $state(false);

  const displayTitle = $derived(
    paper.title || paper.authors[0] || "Untitled",
  );
  const titleId = $derived(`paper-${paper.id}-title`);
  const statusId = $derived(`paper-${paper.id}-status`);
  const pageCountId = $derived(`paper-${paper.id}-pages`);
  const pageCountLabel = $derived(
    `${paper.page_count} ${paper.page_count === 1 ? "page" : "pages"}`,
  );

  function openDelete(e: Event) {
    e.stopPropagation();
    e.preventDefault();
    askingDelete = true;
  }

  async function doDelete() {
    if (deleting) return;
    deleting = true;
    try {
      await ipc.deletePaper(paper.id);
      await papers.refresh();
      askingDelete = false;
      toast.success(`Deleted "${displayTitle}"`);
    } catch (err) {
      toast.error(`Delete failed: ${err}`);
    } finally {
      deleting = false;
    }
  }

  async function resume(e: Event) {
    e.stopPropagation();
    e.preventDefault();
    if (resuming) return;
    resuming = true;
    try {
      // The library doesn't carry the job id, so fetch the paper
      // detail to find its active (failed) job, then resume it.
      const detail = await ipc.getPaper(paper.id);
      const job = detail.active_job;
      if (!job) {
        toast.error("No active job to resume.");
        return;
      }
      await ipc.resumeJob(job.id);
      await papers.refresh();
      toast.info(`Resumed generation for "${displayTitle}"`);
    } catch (err) {
      toast.error(`Resume failed: ${err}`);
    } finally {
      resuming = false;
    }
  }

  const statusColor = $derived.by(() => {
    switch (paper.processing_status) {
      case "complete":
        return "var(--status-ok)";
      case "running":
      case "queued":
        return "var(--violet-300)";
      case "failed":
        return "var(--status-err)";
    }
  });

  const statusLabel = $derived.by(() => {
    switch (paper.processing_status) {
      case "complete":
        return "Ready";
      case "running":
        return "Generating";
      case "queued":
        return "Queued";
      case "failed":
        return "Failed";
    }
  });
</script>

<article
  class="card"
  class:list={layout === "list"}
  use:cursorGlow={{ strength: 0.9 }}
  aria-labelledby={titleId}
>
  <button
    class="open-target"
    type="button"
    aria-labelledby={titleId}
    aria-describedby={`${statusId} ${pageCountId}`}
    title={`Open ${displayTitle}`}
    onclick={() => onOpen?.(paper.id)}
  ></button>

  <div class="thumb" aria-hidden="true">
    {#if paper.thumbnail_path}
      <img src={`file://${paper.thumbnail_path}`} alt="" />
    {:else}
      <div class="thumb-placeholder">
        <svg viewBox="0 0 48 48" width="28" height="28" fill="none">
          <rect x="10" y="6" width="28" height="36" rx="3" stroke="currentColor" stroke-width="1.6" />
          <path d="M16 16h16M16 22h16M16 28h12" stroke="currentColor" stroke-width="1.4" />
        </svg>
      </div>
    {/if}
  </div>

  <div class="body">
    <h3 class="title" id={titleId}>{displayTitle}</h3>
    {#if paper.authors.length}
      <div class="authors">{paper.authors.slice(0, 2).join(", ")}{paper.authors.length > 2 ? "…" : ""}</div>
    {/if}
    <div class="meta">
      <span
        class="dot"
        style={`background: ${statusColor}; color: ${statusColor}`}
        aria-hidden="true"
      ></span>
      <span
        class="status"
        id={statusId}
        aria-label={`Processing status: ${statusLabel}`}
      >{statusLabel}</span>
      <span class="sep" aria-hidden="true">·</span>
      <span id={pageCountId}>{pageCountLabel}</span>
      <span class="meta-spacer"></span>
      {#if paper.processing_status === "failed"}
        <button
          class="resume-btn"
          type="button"
          aria-label={`Resume processing for ${displayTitle}`}
          title={`Resume processing for ${displayTitle}`}
          disabled={resuming}
          onclick={resume}
        >
          {#if resuming}
            <span class="spinner sm" aria-hidden="true"></span>
            Resuming
          {:else}
            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" aria-hidden="true">
              <path d="M8 5v14l11-7L8 5z" fill="currentColor"/>
            </svg>
            Resume
          {/if}
        </button>
      {/if}
      <button
        class="delete-btn"
        type="button"
        aria-label={`Delete ${displayTitle}`}
        title={`Delete ${displayTitle}`}
        disabled={deleting}
        onclick={openDelete}
      >
        {#if deleting}
          <span class="spinner" aria-hidden="true"></span>
        {:else}
          <svg viewBox="0 0 24 24" width="13" height="13" fill="none">
            <path d="M5 7h14M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        {/if}
      </button>
    </div>
  </div>
</article>

<ConfirmDialog
  open={askingDelete}
  title="Delete this paper?"
  message={`"${displayTitle}" and all generated pages will be removed. This cannot be undone.`}
  confirmLabel="Delete"
  cancelLabel="Keep"
  danger
  busy={deleting}
  onConfirm={doDelete}
  onCancel={() => { if (!deleting) askingDelete = false; }}
/>

<style>
  .card {
    --mx: 50;
    --my: 50;
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    padding: 14px;
    border-radius: var(--radius-lg);
    text-align: left;
    color: var(--text-hi);
    background:
      radial-gradient(
        600px circle at calc(var(--mx) * 1%) calc(var(--my) * 1%),
        rgba(154, 107, 255, 0.16),
        transparent 55%
      ),
      linear-gradient(115deg, rgba(255, 255, 255, 0.06), transparent 24%),
      linear-gradient(160deg, #070610, var(--bg-elevated));
    border: 1px solid var(--border);
    box-shadow:
      var(--shadow-md),
      inset 0 1px 0 rgba(255, 255, 255, 0.055);
    transition:
      transform var(--dur) var(--ease),
      border-color var(--dur) var(--ease),
      box-shadow var(--dur) var(--ease);
  }

  .open-target {
    position: absolute;
    inset: 0;
    z-index: 1;
    border-radius: inherit;
    cursor: pointer;
  }

  .open-target:focus-visible,
  .resume-btn:focus-visible,
  .delete-btn:focus-visible {
    outline: 2px solid var(--cyan-accent);
    outline-offset: 3px;
  }

  .card:hover {
    transform: translateY(-2px);
    border-color: var(--border-strong);
    box-shadow:
      var(--shadow-lg),
      0 0 24px -8px var(--violet-glow);
  }

  /* ── List layout ──────────────────────────────────────────
     Horizontal row: small thumb left, title/meta right.
     The delete button stays in `.meta` and floats right via
     `.meta-spacer`, same as grid. */
  .card.list {
    flex-direction: row;
    align-items: center;
    padding: 10px 14px;
    gap: 14px;
  }

  .card.list:hover {
    transform: translateY(-1px);
  }

  .card.list .thumb {
    aspect-ratio: 1 / 1;
    width: 44px;
    height: 44px;
    flex: 0 0 44px;
    border-radius: var(--radius-sm);
  }

  .card.list .body {
    flex: 1;
    min-width: 0;
    flex-direction: row;
    align-items: center;
    gap: 14px;
  }

  .card.list .title {
    -webkit-line-clamp: 1;
    line-clamp: 1;
    flex: 1;
    min-width: 0;
  }

  .card.list .authors {
    flex: 0 0 auto;
    max-width: 220px;
  }

  .card.list .meta {
    flex: 0 0 auto;
    width: auto;
    margin-top: 0;
  }

  .thumb {
    position: relative;
    z-index: 2;
    pointer-events: none;
    aspect-ratio: 4 / 5;
    border-radius: var(--radius-md);
    overflow: hidden;
    background:
      linear-gradient(135deg, rgba(154, 107, 255, 0.12), transparent 48%),
      #05040a;
    border: 1px solid var(--border-subtle);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.025);
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .thumb-placeholder {
    width: 100%;
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--violet-200);
    background:
      repeating-linear-gradient(
        0deg,
        rgba(79, 240, 255, 0.035) 0 1px,
        transparent 1px 9px
      );
  }

  .body {
    position: relative;
    z-index: 2;
    pointer-events: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .title {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
    line-height: 1.3;
    letter-spacing: 0;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
  }

  .authors {
    font-size: var(--text-xs);
    color: var(--text-mid);
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 1;
    line-clamp: 1;
    overflow: hidden;
  }

  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    font-size: var(--text-xs);
    color: var(--text-dim);
    margin-top: 2px;
    width: 100%;
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    box-shadow: 0 0 8px currentColor;
  }

  .status {
    color: var(--text-mid);
    font-weight: 600;
  }

  .sep {
    opacity: 0.5;
  }

  /* Delete button lives in the meta row — same height as the status text,
     pushed to the right by `.meta-spacer`, hidden until card hover. */
  .meta-spacer {
    flex: 1;
  }

  .delete-btn {
    position: relative;
    z-index: 3;
    pointer-events: auto;
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-dim);
    border-radius: var(--radius-sm);
    opacity: 0;
    transition:
      opacity var(--dur) var(--ease),
      color var(--dur) var(--ease),
      border-color var(--dur) var(--ease),
      background var(--dur) var(--ease);
    cursor: pointer;
  }

  .card:hover .delete-btn,
  .card:focus-within .delete-btn,
  .delete-btn:focus-visible {
    opacity: 0.85;
  }

  .delete-btn:hover {
    opacity: 1;
    color: #ff8a8a;
    border-color: rgba(255, 138, 138, 0.45);
    background: rgba(60, 15, 20, 0.5);
  }

  .delete-btn:disabled {
    opacity: 0.6;
    cursor: progress;
  }

  /* Resume sits next to delete in the meta row. Always visible for
     failed papers (unlike delete which hides until hover) — the
     entire reason a card is on screen with status=failed is so the
     user can act on it. */
  .resume-btn {
    position: relative;
    z-index: 3;
    pointer-events: auto;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-height: 36px;
    padding: 0 11px;
    font-size: 11px;
    font-weight: 500;
    color: white;
    background: var(--violet-500);
    border: 1px solid transparent;
    border-radius: var(--radius-pill);
    cursor: pointer;
    transition:
      transform var(--dur) var(--ease),
      box-shadow var(--dur) var(--ease),
      background var(--dur) var(--ease);
  }

  .resume-btn:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: 0 6px 14px -8px rgba(106, 45, 255, 0.7);
  }

  .resume-btn:disabled {
    opacity: 0.6;
    cursor: progress;
  }

  .spinner.sm {
    width: 9px;
    height: 9px;
    border-width: 1.6px;
    border-color: rgba(255, 255, 255, 0.35);
    border-top-color: white;
  }

  .spinner {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid rgba(255, 138, 138, 0.2);
    border-top-color: #ff8a8a;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  @media (max-width: 720px) {
    .card.list {
      align-items: flex-start;
      padding: 12px;
    }

    .card.list .thumb {
      width: 56px;
      height: 70px;
      flex-basis: 56px;
      aspect-ratio: 4 / 5;
    }

    .card.list .body {
      flex-direction: column;
      align-items: stretch;
      gap: 4px;
    }

    .card.list .authors {
      max-width: 100%;
    }

    .card.list .meta {
      width: 100%;
    }

    .delete-btn,
    .resume-btn {
      min-height: 44px;
    }

    .delete-btn {
      width: 44px;
      opacity: 0.85;
    }
  }

  @media (hover: none), (pointer: coarse) {
    .delete-btn {
      opacity: 0.85;
    }

    .card:hover,
    .card.list:hover,
    .resume-btn:hover:not(:disabled) {
      transform: none;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .card,
    .resume-btn {
      transition: none;
    }

    .card:hover,
    .card.list:hover,
    .resume-btn:hover:not(:disabled) {
      transform: none;
    }
  }
</style>

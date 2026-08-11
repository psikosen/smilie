<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import type { PageRecord, ReadingLevel } from "$lib/ipc";
  import { ipc, events } from "$lib/ipc";
  import { isTauri } from "$lib/ipc/tauri-guard";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { toast } from "$lib/stores/toast";
  import { levelLabel, pendingLabel } from "./level-toggle";
  import ReadingLevelToggle from "./ReadingLevelToggle.svelte";

  interface Props {
    page: PageRecord | null;
    onRegenerate?: (id: string) => void;
    onDeletePaper?: () => void;
  }
  let { page, onRegenerate, onDeletePaper }: Props = $props();

  let exportBusy = $state(false);
  let iframeEl = $state<HTMLIFrameElement | null>(null);

  function typeLabel(type: string): string {
    switch (type) {
      case "informational": return "Informational";
      case "concept": return "Concept explainer";
      case "formula": return "Formula explorer";
      case "figure": return "Figure explorer";
      case "table": return "Table explorer";
      case "quiz": return "Quiz";
      case "simulation": return "Simulation";
      default: return type;
    }
  }

  function validationLabel(status: PageRecord["validation_status"]): string {
    switch (status) {
      case "valid": return "Validated";
      case "failed": return "Validation failed";
      case "pending": return "Validation pending";
      default: return "Not validated";
    }
  }

  function persistenceLabel(record: PageRecord): string {
    switch (record.status) {
      case "ready": return "Saved offline";
      case "queued": return "Queued";
      case "generating": return "Generating";
      case "failed": return "Generation failed";
    }
  }

  // ── Reading-level slider ────────────────────────────────────────
  /** When the user clicks a level that isn't cached, we optimistically
   *  set this to the requested level and show the "Generating <level>…"
   *  pill. Once `pipeline:page_ready` fires for the page (or the page
   *  prop reflects the updated current_level), we clear it. */
  let pendingLevelByPage = $state<Record<string, ReadingLevel>>({});

  const pendingLevel = $derived<ReadingLevel | null>(
    page ? (pendingLevelByPage[page.id] ?? null) : null,
  );

  /** A regen is in flight for the current page when we have a pending
   *  level AND the server hasn't yet flipped current_level to it. */
  const regenInFlight = $derived(
    !!page && pendingLevel !== null && page.current_level !== pendingLevel,
  );

  // Clear stale pending entries whenever the page record updates with
  // a current_level that matches the pending value (regen landed) or
  // when the cached_levels grew to include it (race-friendly fallback).
  $effect(() => {
    if (!page) return;
    const pid = page.id;
    const pending = pendingLevelByPage[pid];
    if (!pending) return;
    if (page.current_level === pending || page.cached_levels.includes(pending)) {
      const next = { ...pendingLevelByPage };
      delete next[pid];
      pendingLevelByPage = next;
    }
  });

  // Subscribe to pipeline events so we toast when a level lands. The
  // paper_detail store will refresh `page` independently — this purely
  // surfaces a friendly success message.
  let unlistenPipeline: (() => void) | null = null;
  let componentAlive = false;
  onMount(() => {
    componentAlive = true;
    if (!isTauri()) return;
    void events
      .onPipeline((ev) => {
        const p = ev.payload;
        if (p.type !== "page_ready") return;
        const pendingForReady = pendingLevelByPage[p.page_id];
        if (!pendingForReady) return;
        toast.success(`${levelLabel(pendingForReady)} ready`);
      })
      .then((un) => {
        if (!componentAlive) {
          un();
          return;
        }
        unlistenPipeline = un;
      })
      .catch((error) => {
        console.error("pipeline listener failed", error);
      });
  });

  onDestroy(() => {
    componentAlive = false;
    unlistenPipeline?.();
    unlistenPipeline = null;
  });

  async function pickLevel(level: ReadingLevel) {
    if (!page) return;
    const targetPage = page;
    if (regenInFlight) return; // disable while one is in flight
    if (targetPage.current_level === level) return;
    if (!isTauri()) {
      toast.error("Reading-level switch only available in the desktop app.");
      return;
    }
    // Optimistic pending if we know the level isn't cached. Cached
    // levels swap instantly server-side and the paper_detail store
    // refreshes via paper:updated within a tick.
    if (!targetPage.cached_levels.includes(level)) {
      pendingLevelByPage = { ...pendingLevelByPage, [targetPage.id]: level };
    }
    try {
      await ipc.setPageReadingLevel(targetPage.id, level);
    } catch (err) {
      toast.error(`Could not switch level: ${String(err)}`);
      // Roll back the optimistic pending marker on error.
      if (pendingLevelByPage[targetPage.id]) {
        const next = { ...pendingLevelByPage };
        delete next[targetPage.id];
        pendingLevelByPage = next;
      }
    }
  }

  // Listen for quiz-attempt postMessages emitted by the Smilie Page SDK
  // running inside the iframe. The SDK only knows how to talk to its
  // parent — this is the parent — so we forward attempts to the Tauri
  // command that records them and updates the SRS schedule.
  //
  // We accept any origin because the iframe sandbox runs the page from
  // tauri's `asset://` scheme and the origin is otherwise opaque. The
  // payload is structurally validated below before we trust any field.
  onMount(() => {
    function onMessage(ev: MessageEvent) {
      const data = ev.data as unknown;
      if (
        !data ||
        typeof data !== "object" ||
        (data as { type?: unknown }).type !== "smilie:quiz_attempt"
      ) {
        return;
      }
      const payload = data as {
        type: string;
        page_id?: unknown;
        correct?: unknown;
        total?: unknown;
      };
      const pageId = typeof payload.page_id === "string" ? payload.page_id : null;
      const correct =
        typeof payload.correct === "number" ? payload.correct : null;
      const total =
        typeof payload.total === "number" ? payload.total : null;
      if (ev.source !== iframeEl?.contentWindow) return;
      if (!page || pageId !== page.id) return;
      if (
        correct === null ||
        total === null ||
        !Number.isInteger(correct) ||
        !Number.isInteger(total) ||
        total <= 0 ||
        correct < 0 ||
        correct > total
      ) {
        return;
      }
      if (!isTauri()) return;
      void ipc.recordQuizAttempt(pageId, correct, total).catch((err) => {
        console.error("record_quiz_attempt failed", err);
      });
    }
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  });

  async function exportPage() {
    if (!page) return;
    if (!isTauri()) return;
    exportBusy = true;
    try {
      const mod = await import("@tauri-apps/plugin-dialog");
      const dest = await mod.save({
        defaultPath: `${page.title.replace(/[^a-z0-9]+/gi, "_")}.html`,
        filters: [{ name: "HTML", extensions: ["html"] }],
      });
      if (!dest) return;
      await ipc.exportPage(page.id, dest);
      // Surface the resolved filename, not the full path — that's what
      // the user will recognise in their save dialog.
      const basename = dest.split(/[\\/]/).pop() ?? dest;
      toast.success(`Exported to ${basename}`);
    } catch (err) {
      toast.error("Export failed");
      console.error("export failed", err);
    } finally {
      exportBusy = false;
    }
  }

  const iframeSrc = $derived.by(() => {
    if (!page || !page.html_path || page.status !== "ready") return null;
    // Tauri's asset protocol serves files inside the configured scope.
    // `convertFileSrc(path, 'asset')` returns the right URL form per
    // platform — on macOS it's `http://asset.localhost/<encoded>`.
    return isTauri() ? convertFileSrc(page.html_path, "asset") : null;
  });

  const statusMessage = $derived.by(() => {
    if (!page) return "Pick a page from the sidebar to open it here.";
    switch (page.status) {
      case "queued": return "Queued for generation.";
      case "generating": return "The generator is writing this page now.";
      case "failed": return "Generation failed. Regenerate to retry.";
      case "ready":
        if (iframeSrc) return "";
        if (!isTauri()) return "Preview only available in the desktop app.";
        return "Page file missing — try Regenerate.";
    }
    return "";
  });

  const sourceLabel = $derived.by(() => {
    if (!page) return "";
    if (page.source_pages.length > 0) return `Paper p. ${page.source_pages.join(", ")}`;
    return "Source pages unavailable";
  });

  const sectionLabel = $derived.by(() => {
    if (!page) return "";
    if (page.source_sections.length > 0) {
      return page.source_sections.slice(0, 2).join(" · ");
    }
    return "Section unavailable";
  });
</script>

<section
  class="viewer"
  aria-label={page ? `Learning page: ${page.title}` : "Learning page viewer"}
  aria-busy={page?.status === "queued" || page?.status === "generating"}
>
  <p class="viewer-status" role="status" aria-live="polite" aria-atomic="true">
    {page ? `Opened ${page.title}. ${persistenceLabel(page)}.` : "No learning page selected."}
  </p>
  {#if page}
    <header class="chrome">
      <div class="title-wrap">
        <h1 class="title" title={page.title}>{page.title}</h1>
        <div class="page-meta">
          <span>{typeLabel(page.type)}</span>
          <span title="Local persistence status">{persistenceLabel(page)}</span>
          <span title="Source section">{sectionLabel}</span>
          <span title="Source pages">{sourceLabel}</span>
          <span
            class:valid={page.validation_status === "valid"}
            class:invalid={page.validation_status === "failed"}
          >
            {validationLabel(page.validation_status)}
          </span>
        </div>
      </div>
      <div class="actions" role="toolbar" aria-label="Learning page actions">
        <!-- Reading-level segmented toggle. Lives in the chrome's
             right-side cluster so it's discoverable but doesn't crowd
             the title. Three segments: ELI5 / Default / Expert. -->
        <ReadingLevelToggle
          currentLevel={page.current_level}
          cachedLevels={page.cached_levels}
          {pendingLevel}
          disabled={regenInFlight}
          onPick={pickLevel}
        />

        {#if regenInFlight && pendingLevel}
          <span class="pending-pill" aria-live="polite">
            <span class="pill-bar" aria-hidden="true">
              <span class="pill-bar-sweep"></span>
            </span>
            {pendingLabel(pendingLevel)}
          </span>
        {/if}

        <button
          class="chip"
          type="button"
          onclick={() => page && onRegenerate?.(page.id)}
          disabled={!isTauri() || page.status === "queued" || page.status === "generating"}
          aria-label="Regenerate this page"
        >
          ↻ Regenerate
        </button>
        <button
          class="chip"
          type="button"
          onclick={exportPage}
          disabled={!isTauri() || page.status !== "ready" || exportBusy}
          aria-label="Export this page"
        >
          {exportBusy ? "Exporting…" : "↓ Export"}
        </button>
        <button
          class="chip danger"
          type="button"
          onclick={() => onDeletePaper?.()}
          aria-label="Delete this paper and all saved learning pages"
        >
          Delete paper
        </button>
      </div>
    </header>
  {/if}

  <div class="stage">
    {#if iframeSrc}
      <!-- Keying on `iframeSrc` forces Svelte to recreate the iframe
           when the page changes, which makes the webview process drop
           the previous document instead of bouncing between two cached
           pages in the same iframe. Big help for memory on long browse
           sessions. The `loading="lazy"` is a no-op here (single
           iframe, always in viewport) but signals intent. -->
      {#key iframeSrc}
        <iframe
          title={`Interactive learning page: ${page?.title ?? "Page"}`}
          src={iframeSrc}
          sandbox="allow-scripts"
          loading="lazy"
          referrerpolicy="no-referrer"
          bind:this={iframeEl}
        ></iframe>
      {/key}
    {:else}
      <div class="empty" role="status" aria-live="polite">
        {#if page}
          <div class="empty-title">{page.title}</div>
        {/if}
        <p>{statusMessage}</p>
        {#if page && (page.status === "failed" || (page.status === "ready" && !iframeSrc && isTauri()))}
          <button
            class="chip empty-action"
            type="button"
            onclick={() => onRegenerate?.(page.id)}
          >
            Regenerate page
          </button>
        {/if}
        <div class="empty-flow" aria-hidden="true">
          <span>Structure</span>
          <i></i>
          <span>Plan</span>
          <i></i>
          <span>Page</span>
          <i></i>
          <span>Saved</span>
        </div>
      </div>
    {/if}
  </div>
</section>

<style>
  .viewer {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    background:
      radial-gradient(900px 560px at 82% 8%, rgba(154, 107, 255, 0.12), transparent 62%),
      var(--bg-void);
  }

  .viewer-status {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  .chrome {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    background:
      linear-gradient(115deg, rgba(255, 255, 255, 0.04), transparent 28%),
      rgba(9, 8, 18, 0.96);
    box-shadow: inset 0 -1px 0 rgba(255, 255, 255, 0.025);
  }

  .title-wrap {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .title {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text-hi);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .page-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    color: var(--text-dim);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.035em;
    text-transform: uppercase;
  }

  .page-meta span {
    min-height: 18px;
    padding: 2px 6px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-subtle);
    background: rgba(255, 255, 255, 0.025);
  }

  .page-meta .valid {
    color: var(--status-ok);
    border-color: rgba(93, 255, 160, 0.22);
  }

  .page-meta .invalid {
    color: var(--status-err);
    border-color: rgba(255, 107, 123, 0.28);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }

  .chip {
    min-height: 36px;
    padding: 6px 10px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border);
    background: var(--bg-elevated);
    color: var(--text-mid);
    font-size: var(--text-xs);
    transition:
      color var(--dur) var(--ease),
      background var(--dur) var(--ease),
      border-color var(--dur) var(--ease);
  }

  .chip:hover:not(:disabled) {
    color: var(--violet-100);
    border-color: var(--border-strong);
  }

  .chip:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .chip.danger:hover {
    color: var(--status-err);
    border-color: rgba(255, 107, 123, 0.4);
  }

  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .stage iframe {
    flex: 1;
    width: 100%;
    border: 0;
    background: var(--bg-void);
  }

  .empty {
    margin: auto;
    max-width: 440px;
    padding: 32px;
    text-align: center;
    color: var(--text-mid);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background:
      linear-gradient(125deg, rgba(255, 255, 255, 0.045), transparent 32%),
      rgba(255, 255, 255, 0.025);
    box-shadow: var(--shadow-md);
  }

  .empty-title {
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--text-hi);
    margin-bottom: 8px;
  }

  .empty p {
    margin: 0;
    font-size: var(--text-sm);
    line-height: 1.55;
  }

  .empty-action {
    margin-top: 16px;
    color: var(--violet-100);
    border-color: var(--border-strong);
  }

  .empty-flow {
    display: flex;
    justify-content: center;
    align-items: center;
    flex-wrap: wrap;
    gap: 7px;
    margin-top: 18px;
    color: var(--text-dim);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .empty-flow span {
    padding: 3px 7px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-subtle);
    background: rgba(154, 107, 255, 0.055);
  }

  .empty-flow i {
    width: 18px;
    height: 1px;
    background: linear-gradient(90deg, var(--violet-400), var(--cyan-accent));
  }

  /* "Generating <level>…" pill — mirrors the experiment-status sweep. */
  .pending-pill {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--border-strong);
    background: rgba(154, 107, 255, 0.10);
    color: var(--violet-100);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }
  .pill-bar {
    position: relative;
    width: 36px;
    height: 3px;
    border-radius: var(--radius-pill);
    overflow: hidden;
    background: rgba(154, 107, 255, 0.10);
  }
  .pill-bar-sweep {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 40%;
    background: linear-gradient(
      90deg,
      transparent 0%,
      var(--violet-400) 50%,
      transparent 100%
    );
    animation: pill-sweep 1.4s cubic-bezier(0.45, 0.05, 0.55, 0.95) infinite;
  }
  @keyframes pill-sweep {
    0%   { transform: translateX(-120%); }
    100% { transform: translateX(420%); }
  }
  @media (prefers-reduced-motion: reduce) {
    .pill-bar-sweep {
      animation: none;
      width: 100%;
      transform: translateX(0);
      opacity: 0.6;
    }
  }

  @media (max-width: 900px) {
    .chrome {
      align-items: stretch;
      flex-direction: column;
    }

    .actions {
      flex-wrap: wrap;
    }
  }

  @media (max-width: 560px) {
    .chrome {
      padding: 10px;
    }

    .actions {
      flex-wrap: nowrap;
      overflow-x: auto;
      padding: 2px 2px 6px;
      scrollbar-width: none;
    }

    .actions::-webkit-scrollbar {
      display: none;
    }

    .chip {
      min-height: 44px;
      flex: none;
    }

    .empty {
      width: calc(100% - 24px);
      padding: 24px 18px;
    }
  }

  .chip:focus-visible {
    outline: 2px solid var(--cyan-accent);
    outline-offset: 2px;
  }
</style>

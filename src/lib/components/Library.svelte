<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import TopBar from "./TopBar.svelte";
  import Dropzone from "./Dropzone.svelte";
  import DragOverlay from "./DragOverlay.svelte";
  import PaperCard from "./PaperCard.svelte";
  import SearchResults from "./SearchResults.svelte";
  import DueForReview from "./DueForReview.svelte";
  import { papers } from "$lib/stores/papers";
  import { jobs } from "$lib/stores/jobs";
  import { searchQuery } from "$lib/stores/search";
  import { drag } from "$lib/stores/drag";
  import { isTauri } from "$lib/ipc/tauri-guard";

  type View = "grid" | "list";
  const VIEW_KEY = "smilie:libraryView";

  const flow = [
    "Paper",
    "Structured data",
    "AI understanding",
    "Interactive pages",
    "Saved learning system",
  ] as const;

  const pipeline = [
    {
      code: "01",
      title: "PDF intake",
      detail: "Local file capture and durable paper record",
    },
    {
      code: "02",
      title: "Vision structure",
      detail: "GLM-OCR extracts text, layout, tables, figures, formulas",
    },
    {
      code: "03",
      title: "Paper understanding",
      detail: "Reasoning model builds concepts, prerequisites, and dependencies",
    },
    {
      code: "04",
      title: "Learning plan",
      detail: "Pages are chosen by section, source page, and interaction value",
    },
    {
      code: "05",
      title: "Page build",
      detail: "Explanations, quizzes, explorers, diagrams, and simulations",
    },
    {
      code: "06",
      title: "Permanent save",
      detail: "Validated pages stay available offline until deleted",
    },
  ] as const;

  const systemBadges = ["GLM-OCR 0.9B", "Resumable queue", "SQLite pages"] as const;

  let view = $state<View>("grid");
  let setupError = $state<string | null>(null);
  let retrying = $state(false);

  onMount(() => {
    void papers.init().catch((error) => {
      setupError = `Could not initialize the paper library: ${error}`;
    });
    void jobs.init().catch((error) => {
      console.error("Could not initialize pipeline updates", error);
    });
    void drag.init();
    try {
      const v = localStorage.getItem(VIEW_KEY);
      if (v === "grid" || v === "list") view = v;
    } catch {
      // Private mode / quota error - default to grid.
    }
  });

  function setView(v: View) {
    view = v;
    try {
      localStorage.setItem(VIEW_KEY, v);
    } catch {}
  }

  function openPaper(id: string) {
    void goto(`/papers/${id}/`);
  }

  function openSettings() {
    void goto("/settings/");
  }

  async function retryLibrary() {
    if (retrying) return;
    retrying = true;
    setupError = null;
    try {
      await papers.refresh();
    } finally {
      retrying = false;
    }
  }

  const hasPapers = $derived($papers.items.length > 0);
  const libraryError = $derived($papers.error || setupError);
  const empty = $derived(!hasPapers && !$papers.loading && !libraryError);
  const searching = $derived($searchQuery.trim().length >= 2);
</script>

<TopBar onSettings={openSettings} />

<main
  class="library noise-bg tokyo-grid"
  class:is-empty={empty}
  class:has-papers={hasPapers}
  aria-busy={$papers.loading}
>
  {#if searching}
    <SearchResults query={$searchQuery} />
  {:else}
    <section class="hero" class:compact={hasPapers} aria-labelledby="library-hero-title">
      <div class="intake surface-metallic fade-rise">
        <div class="hero-copy">
          <div class="eyebrow">
            {hasPapers ? "Add to your learning library" : "Scientific paper to saved learning system"}
          </div>
          <h1 id="library-hero-title">
            {hasPapers ? "Build from another paper." : "Paper in. Interactive textbook out."}
          </h1>
          <p>
            {#if hasPapers}
              Import another PDF while your existing pages stay ready to study.
            {:else}
              Upload a PDF and Smilie turns dense research into structured data,
              a learning plan, and durable interactive pages.
            {/if}
          </p>

          {#if !hasPapers}
            <ol class="flow-ribbon" aria-label="Transformation flow">
              {#each flow as item, index (item)}
                <li class="flow-item">
                  <span class="flow-node">{item}</span>
                  {#if index < flow.length - 1}
                    <span class="flow-arrow" aria-hidden="true">→</span>
                  {/if}
                </li>
              {/each}
            </ol>

            <ul class="system-badges" aria-label="System capabilities">
              {#each systemBadges as badge (badge)}
                <li>{badge}</li>
              {/each}
            </ul>
          {/if}
        </div>

        <div class="drop-wrap">
          <Dropzone compact={hasPapers} />
        </div>
      </div>

      {#if !hasPapers}
        <section class="pipeline-map fade-rise" aria-labelledby="pipeline-title">
          <header class="map-head">
            <div>
              <div class="map-kicker">Generation path</div>
              <h2 id="pipeline-title">From paper structure to permanent pages</h2>
            </div>
            <div class="save-chip">offline first</div>
          </header>

          <ol class="map-rail">
            {#each pipeline as step (step.code)}
              <li class="step">
                <div class="step-code" aria-hidden="true">{step.code}</div>
                <div class="step-body">
                  <h3>{step.title}</h3>
                  <p>{step.detail}</p>
                </div>
              </li>
            {/each}
          </ol>
        </section>
      {/if}
    </section>

    {#if !isTauri()}
      <div class="browser-hint" role="note">
        Browser preview is UI-only. Use <code>npm run tauri:dev</code> for ingest and generation.
      </div>
    {/if}
  {/if}

  {#if !searching && !hasPapers && $papers.loading}
    <section class="library-state" role="status" aria-live="polite">
      <span class="state-spinner" aria-hidden="true"></span>
      <div>
        <h2>Loading saved papers</h2>
        <p>Restoring your local learning systems.</p>
      </div>
    </section>
  {:else if !searching && !hasPapers && libraryError}
    <section class="library-state error-state" role="alert">
      <div>
        <h2>Could not load your library</h2>
        <p>{libraryError}</p>
      </div>
      <button type="button" class="retry-btn" disabled={retrying} onclick={retryLibrary}>
        {retrying ? "Retrying…" : "Retry"}
      </button>
    </section>
  {/if}

  {#if !searching}
    <DueForReview />
  {/if}

  {#if !searching && hasPapers}
    <section class="grid-section" aria-labelledby="library-heading">
      <header class="grid-header">
        <div>
          <div class="section-kicker">Permanent learning systems</div>
          <h2 id="library-heading">Your library</h2>
        </div>
        <div class="header-right">
          <div class="count" aria-live="polite">
            {$papers.items.length} paper{$papers.items.length === 1 ? "" : "s"}
          </div>
          <div class="view-toggle" role="group" aria-label="Library view">
            <button
              type="button"
              class="seg"
              class:active={view === "grid"}
              aria-pressed={view === "grid"}
              aria-label="Grid view"
              title="Grid view"
              onclick={() => setView("grid")}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" aria-hidden="true">
                <rect x="3" y="3" width="7" height="7" rx="1" stroke="currentColor" stroke-width="1.6"/>
                <rect x="14" y="3" width="7" height="7" rx="1" stroke="currentColor" stroke-width="1.6"/>
                <rect x="3" y="14" width="7" height="7" rx="1" stroke="currentColor" stroke-width="1.6"/>
                <rect x="14" y="14" width="7" height="7" rx="1" stroke="currentColor" stroke-width="1.6"/>
              </svg>
              <span class="sr-only">Grid</span>
            </button>
            <button
              type="button"
              class="seg"
              class:active={view === "list"}
              aria-pressed={view === "list"}
              aria-label="List view"
              title="List view"
              onclick={() => setView("list")}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" aria-hidden="true">
                <path d="M4 6h16M4 12h16M4 18h16" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>
              </svg>
              <span class="sr-only">List</span>
            </button>
          </div>
        </div>
      </header>
      {#if view === "grid"}
        <ul class="grid">
          {#each $papers.items as p (p.id)}
            <li><PaperCard paper={p} onOpen={openPaper} /></li>
          {/each}
        </ul>
      {:else}
        <ul class="list">
          {#each $papers.items as p (p.id)}
            <li><PaperCard paper={p} onOpen={openPaper} layout="list" /></li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</main>

<DragOverlay />

<style>
  .library {
    position: relative;
    height: calc(100vh - 56px);
    height: calc(100dvh - 56px);
    overflow-y: auto;
    padding: 34px 44px 64px;
  }

  .hero {
    max-width: 1340px;
    margin: 0 auto 34px;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(360px, 480px);
    gap: 18px;
    align-items: stretch;
  }

  .hero.compact {
    grid-template-columns: 1fr;
    margin-bottom: 22px;
  }

  .intake {
    position: relative;
    display: grid;
    grid-template-columns: minmax(280px, 1fr) minmax(340px, 0.82fr);
    gap: 28px;
    min-height: 438px;
    padding: 34px;
    overflow: hidden;
    box-shadow:
      var(--shadow-lg),
      0 0 70px -38px rgba(79, 240, 255, 0.75);
  }

  .hero.compact .intake {
    grid-template-columns: minmax(280px, 1fr) minmax(320px, 440px);
    min-height: 232px;
    padding: 24px 28px;
  }

  .intake::before {
    content: "";
    position: absolute;
    inset: 0;
    pointer-events: none;
    background:
      linear-gradient(110deg, transparent 0%, rgba(255, 255, 255, 0.09) 16%, transparent 31%),
      radial-gradient(620px 260px at 70% 10%, rgba(79, 240, 255, 0.13), transparent 64%),
      radial-gradient(420px 300px at 18% 95%, rgba(255, 62, 165, 0.12), transparent 68%);
    opacity: 0.9;
  }

  .hero-copy,
  .drop-wrap {
    position: relative;
    z-index: 1;
  }

  .hero-copy {
    display: flex;
    flex-direction: column;
    justify-content: center;
    min-width: 0;
  }

  .eyebrow,
  .section-kicker,
  .map-kicker {
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.16em;
    color: var(--cyan-accent);
  }

  h1 {
    max-width: 660px;
    margin: 12px 0 0;
    font-size: 64px;
    font-weight: 650;
    letter-spacing: 0;
    line-height: 0.95;
    background: linear-gradient(
      180deg,
      #ffffff 0%,
      #f0e8ff 34%,
      #b895ff 67%,
      #6f38ff 100%
    );
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }

  .hero.compact h1 {
    max-width: 740px;
    font-size: 40px;
    line-height: 1.05;
  }

  .hero-copy p {
    max-width: 560px;
    margin: 18px 0 0;
    font-size: var(--text-md);
    line-height: 1.6;
    color: var(--text-mid);
  }

  .flow-ribbon {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 24px;
    align-items: center;
    margin-top: 26px;
    padding: 0;
    list-style: none;
    color: var(--text-hi);
  }

  .flow-node {
    display: inline-flex;
    align-items: center;
    min-height: 28px;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background:
      linear-gradient(135deg, rgba(255, 255, 255, 0.055), rgba(154, 107, 255, 0.06)),
      rgba(6, 5, 12, 0.78);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08);
    font-size: var(--text-xs);
    white-space: nowrap;
  }

  .flow-item {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .flow-arrow {
    color: var(--violet-200);
    opacity: 0.82;
  }

  .system-badges {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 18px;
    padding: 0;
    list-style: none;
  }

  .system-badges li,
  .save-chip {
    padding: 5px 9px;
    border-radius: var(--radius-pill);
    border: 1px solid rgba(79, 240, 255, 0.2);
    background: rgba(79, 240, 255, 0.055);
    color: var(--cyan-glow);
    font-size: var(--text-xs);
    font-family: var(--font-mono);
  }

  .drop-wrap {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 0;
  }

  .pipeline-map {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 438px;
    padding: 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background:
      radial-gradient(420px 260px at 88% 8%, rgba(154, 107, 255, 0.16), transparent 62%),
      linear-gradient(150deg, rgba(9, 8, 18, 0.94), rgba(14, 11, 24, 0.9));
    box-shadow: var(--shadow-md);
    overflow: hidden;
  }

  .pipeline-map::before {
    content: "";
    position: absolute;
    inset: 0;
    pointer-events: none;
    background: linear-gradient(110deg, transparent 0%, rgba(255, 255, 255, 0.08) 12%, transparent 24%);
    opacity: 0.55;
  }

  .map-head,
  .map-rail {
    position: relative;
    z-index: 1;
  }

  .map-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 2px 2px 14px;
    border-bottom: 1px solid var(--divider);
  }

  .map-head h2 {
    margin: 6px 0 0;
    font-size: var(--text-lg);
    line-height: 1.2;
    letter-spacing: 0;
  }

  .map-rail {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 14px;
    padding-left: 0;
    margin: 0;
    list-style: none;
  }

  .step {
    display: grid;
    grid-template-columns: 42px 1fr;
    gap: 10px;
    padding: 10px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: rgba(255, 255, 255, 0.025);
  }

  .step-code {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--violet-100);
    background: linear-gradient(145deg, rgba(154, 107, 255, 0.18), rgba(6, 5, 12, 0.78));
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .step h3 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 650;
    color: var(--text-hi);
  }

  .step p {
    margin: 3px 0 0;
    color: var(--text-mid);
    font-size: var(--text-xs);
    line-height: 1.45;
  }

  .browser-hint {
    max-width: 1340px;
    margin: -12px auto 32px;
    padding: 10px 14px;
    border-radius: var(--radius-md);
    font-size: var(--text-xs);
    color: var(--text-mid);
    background: rgba(154, 107, 255, 0.06);
    border: 1px dashed var(--border);
    text-align: center;
  }

  .browser-hint code {
    font-family: var(--font-mono);
    color: var(--violet-100);
  }

  .library-state {
    max-width: 1340px;
    margin: 0 auto 24px;
    min-height: 78px;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background:
      linear-gradient(110deg, rgba(255, 255, 255, 0.045), transparent 28%),
      rgba(8, 7, 16, 0.86);
  }

  .library-state h2,
  .library-state p {
    margin: 0;
  }

  .library-state h2 {
    font-size: var(--text-md);
  }

  .library-state p {
    margin-top: 4px;
    color: var(--text-mid);
    font-size: var(--text-sm);
  }

  .state-spinner {
    width: 24px;
    height: 24px;
    flex: 0 0 24px;
    border: 2px solid rgba(178, 139, 255, 0.2);
    border-top-color: var(--cyan-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .error-state {
    justify-content: space-between;
    border-color: rgba(255, 107, 123, 0.3);
  }

  .error-state h2 {
    color: var(--status-err);
  }

  .retry-btn {
    min-width: 84px;
    min-height: 40px;
    padding: 0 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    color: var(--text-hi);
    background: linear-gradient(150deg, var(--violet-500), var(--violet-700));
  }

  .retry-btn:focus-visible,
  .seg:focus-visible {
    outline: 2px solid var(--cyan-accent);
    outline-offset: 2px;
  }

  .retry-btn:disabled {
    opacity: 0.65;
    cursor: progress;
  }

  .grid-section {
    max-width: 1340px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .grid-header {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    padding: 0 4px;
  }

  .grid-header h2 {
    margin: 5px 0 0;
    font-size: var(--text-lg);
    font-weight: 650;
    letter-spacing: 0;
  }

  .count {
    font-size: var(--text-xs);
    color: var(--text-dim);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: 14px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .grid > li,
  .list > li {
    min-width: 0;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .view-toggle {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    overflow: hidden;
    background: rgba(255, 255, 255, 0.02);
  }

  .seg {
    width: 40px;
    height: 36px;
    display: grid;
    place-items: center;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    transition: color var(--dur) var(--ease), background var(--dur) var(--ease);
  }

  .seg:hover {
    color: var(--text-mid);
  }

  .seg.active {
    color: var(--violet-100);
    background: rgba(154, 107, 255, 0.16);
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    border: 0;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  @media (max-width: 1280px) {
    .hero {
      grid-template-columns: 1fr;
    }

    .pipeline-map {
      min-height: 0;
    }
  }

  @media (max-width: 860px) {
    .intake,
    .hero.compact .intake {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 720px) {
    .library {
      padding: 18px 14px 48px;
    }

    .hero {
      gap: 14px;
      margin-bottom: 26px;
    }

    .intake,
    .pipeline-map {
      padding: 18px;
    }

    h1 {
      font-size: 42px;
      line-height: 1;
    }

    .hero.compact h1 {
      font-size: 34px;
    }

    .flow-ribbon {
      gap: 6px;
    }

    .flow-arrow {
      display: none;
    }

    .grid-header {
      align-items: flex-start;
      gap: 12px;
    }

    .header-right {
      flex-direction: column;
      align-items: flex-end;
      gap: 8px;
    }

    .library-state {
      align-items: flex-start;
    }

    .error-state {
      flex-direction: column;
    }

    .retry-btn {
      width: 100%;
      min-height: 44px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .state-spinner {
      animation-duration: 1.6s;
    }
  }
</style>
